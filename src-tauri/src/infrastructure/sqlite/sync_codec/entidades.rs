//! Codecs das entidades (B3): `entity`, `relation`, `timeline_event` e `canvas_entity_position`.
//!
//! ## Payload canônico
//!
//! ```text
//! entity                    ficha inteira: entities + entity_attributes + custom fields do dono
//! relation                  aresta com identidade própria; as duas pontas são imutáveis
//! timeline_event            evento do universo; a entidade é opcional e some para nulo (SET NULL)
//! canvas_entity_position    posição persistida da entidade no grafo (autoral)
//! ```
//!
//! Fora do payload: `created_at`, `updated_at`, ids das linhas internas (atributo é identificado
//! pela chave), `image` legada (o que viaja é a referência de blob) e o `sort_order` das linhas
//! internas — a posição de um atributo é a da lista.
//!
//! **Estado de viewport não entra em nada disto.** Zoom, pan, seleção e hover vivem na memória do
//! componente; o que sincroniza é a posição que o escritor arrastou, que está no banco.
//!
//! ## Números
//!
//! `sortKey` da timeline e `positionX/Y` são `REAL`. Dois aparelhos só produzem o mesmo payload se
//! tiverem o mesmo `f64`, e a serialização do `serde_json` é a representação mínima que faz
//! round-trip — determinística para os mesmos bits. Valor não finito (NaN, infinito) é recusado na
//! leitura: ele não tem representação canônica e nem chegaria a um JSON válido.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::manuscrito::{campos, exigir_imagem_migrada, gravar_campos, CampoPersonalizado};
use super::{de_json, erro, existe, para_json, EstadoDoAgregado, Impacto};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtributoCanonico {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntidadeCanonica {
    pub id: String,
    pub universe_id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub name: String,
    pub description: String,
    pub summary: String,
    pub canon_status: String,
    pub image_blob_hash: String,
    pub image_mime_type: String,
    /// Estado interno da ficha: não tem evento próprio.
    pub attributes: Vec<AtributoCanonico>,
    pub custom_fields: Vec<CampoPersonalizado>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelacaoCanonica {
    pub id: String,
    pub universe_id: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(rename = "type")]
    pub relation_type: String,
    pub label: String,
    pub bidirectional: bool,
    pub importance: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventoCanonico {
    pub id: String,
    pub universe_id: String,
    pub title: String,
    pub description: String,
    pub event_type: String,
    pub start_date: String,
    pub end_date: String,
    /// `None` depois de a entidade ser excluída (`SET NULL`) — reescrita legítima, não corrupção.
    pub entity_id: Option<String>,
    pub display_date: String,
    pub sort_key: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoCanonica {
    pub entity_id: String,
    pub universe_id: String,
    pub position_x: f64,
    pub position_y: f64,
}

fn exigir_finito(valor: f64, campo: &str, id: &str) -> DatabaseCommandResult<f64> {
    if !valor.is_finite() {
        return Err(DatabaseCommandError::storage(format!(
            "{campo} de {id} não é um número finito; não há payload canônico para isso."
        )));
    }
    Ok(valor)
}

// ── Leitura canônica ─────────────────────────────────────────────────────

fn atributos(
    connection: &Connection,
    entity_id: &str,
) -> DatabaseCommandResult<Vec<AtributoCanonico>> {
    let mut consulta = connection
        .prepare(
            "SELECT key, value FROM entity_attributes
              WHERE entity_id = ?1 ORDER BY sort_order, key, id",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([entity_id], |row| {
            Ok(AtributoCanonico {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

pub fn ler_entidade(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((
        universe_id,
        entity_type,
        name,
        description,
        summary,
        canon_status,
        legado,
        hash,
        mime,
    )) = connection
        .query_row(
            "SELECT universe_id, type, name, description, summary, canon_status,
                        image, image_blob_hash, image_mime_type
                   FROM entities WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    exigir_imagem_migrada("entity", id, &legado, &hash)?;
    let payload = para_json(&EntidadeCanonica {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        entity_type,
        name,
        description,
        summary,
        canon_status,
        image_blob_hash: hash,
        image_mime_type: mime,
        attributes: atributos(connection, id)?,
        custom_fields: campos(connection, "entity", id)?,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

pub fn ler_relacao(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(relacao) = connection
        .query_row(
            "SELECT universe_id, source_id, target_id, type, label, bidirectional, importance
               FROM relations WHERE id = ?1",
            [id],
            |row| {
                Ok(RelacaoCanonica {
                    id: id.to_string(),
                    universe_id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    relation_type: row.get(3)?,
                    label: row.get(4)?,
                    bidirectional: row.get::<_, i64>(5)? != 0,
                    importance: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    Ok(Some(EstadoDoAgregado {
        universe_id: relacao.universe_id.clone(),
        payload: para_json(&relacao)?,
    }))
}

pub fn ler_evento(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(evento) = connection
        .query_row(
            "SELECT universe_id, title, description, event_type, start_date, end_date,
                    entity_id, display_date, sort_key
               FROM timeline_events WHERE id = ?1",
            [id],
            |row| {
                Ok(EventoCanonico {
                    id: id.to_string(),
                    universe_id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    event_type: row.get(3)?,
                    start_date: row.get(4)?,
                    end_date: row.get(5)?,
                    entity_id: row.get(6)?,
                    display_date: row.get(7)?,
                    sort_key: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    exigir_finito(evento.sort_key, "sortKey", id)?;
    Ok(Some(EstadoDoAgregado {
        universe_id: evento.universe_id.clone(),
        payload: para_json(&evento)?,
    }))
}

pub fn ler_posicao(
    connection: &Connection,
    entity_id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    // 0 → não existe · 1 → estado canônico · mais de uma → inconsistência, nunca "escolhe uma".
    let Some(universe_id) = universo_da_posicao(connection, entity_id)? else {
        return Ok(None);
    };
    let (position_x, position_y): (f64, f64) = connection
        .query_row(
            "SELECT position_x, position_y FROM canvas_entity_positions
              WHERE entity_id = ?1 AND universe_id = ?2",
            [entity_id, universe_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(erro)?;
    exigir_finito(position_x, "positionX", entity_id)?;
    exigir_finito(position_y, "positionY", entity_id)?;
    let payload = para_json(&PosicaoCanonica {
        entity_id: entity_id.to_string(),
        universe_id: universe_id.clone(),
        position_x,
        position_y,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

// ── Impactos de exclusão ─────────────────────────────────────────────────

pub fn impactos_da_entidade(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    // `planning_field_links.entity_id ON DELETE CASCADE`: o card sobrevive sem a ligação (B4).
    let mut impactos = super::planejamento::cards_que_perdem_ligacao(connection, "entity_id", id)?;

    let ids = |sql: &str| -> DatabaseCommandResult<Vec<String>> {
        let mut consulta = connection.prepare(sql).map_err(erro)?;
        let linhas = consulta.query_map([id], |row| row.get(0)).map_err(erro)?;
        linhas.collect::<Result<Vec<String>, _>>().map_err(erro)
    };

    // FK CASCADE das duas pontas: a relação some com qualquer uma das entidades.
    for relacao in
        ids("SELECT id FROM relations WHERE source_id = ?1 OR target_id = ?1 ORDER BY id")?
    {
        impactos.push(Impacto::Excluido(AggregateRef::new("relation", relacao)));
    }
    // FK CASCADE: a posição persistida no grafo.
    if existe_posicao(connection, id)? {
        impactos.push(Impacto::Excluido(AggregateRef::new(
            "canvas_entity_position",
            id,
        )));
    }
    // Gatilho `trg_entity_attachments_delete`.
    for anexo in ids(
        "SELECT id FROM attachments WHERE owner_type = 'entity' AND owner_id = ?1
          ORDER BY sort_order, id",
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new("attachment", anexo)));
    }
    // Gatilho `trg_entity_metadata_delete` (as marcações; os campos personalizados são internos).
    impactos.extend(super::manuscrito::atribuicoes_do_dono(
        connection, "entity", id,
    )?);
    // FK SET NULL: o evento da linha do tempo **sobrevive** sem a entidade.
    for evento in ids("SELECT id FROM timeline_events WHERE entity_id = ?1 ORDER BY id")? {
        impactos.push(Impacto::Reescrito(AggregateRef::new(
            "timeline_event",
            evento,
        )));
    }
    Ok(impactos)
}

fn existe_posicao(connection: &Connection, entity_id: &str) -> DatabaseCommandResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM canvas_entity_positions WHERE entity_id = ?1)",
            [entity_id],
            |row| row.get(0),
        )
        .map_err(erro)
}

pub fn impactos_do_evento(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    // Gatilho `trg_timeline_metadata_delete`.
    super::manuscrito::atribuicoes_do_dono(connection, "timeline", id)
}

// ── Dependências ─────────────────────────────────────────────────────────

fn universo_da_entidade(
    connection: &Connection,
    entity_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT universe_id FROM entities WHERE id = ?1",
            [entity_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

/// A entidade existe e está no universo esperado?
///
/// ```text
/// não existe                        → falta (pode chegar)
/// existe em outro universo          → erro: estado incompatível, nunca fica válido
/// ```
fn ponta(
    connection: &Connection,
    entity_id: &str,
    universo: &str,
    papel: &str,
) -> DatabaseCommandResult<Option<String>> {
    match universo_da_entidade(connection, entity_id)? {
        None => Ok(Some(format!("entity {entity_id}"))),
        Some(outro) if outro != universo => Err(DatabaseCommandError::storage(format!(
            "A {papel} {entity_id} está no universo {outro}, e o payload diz {universo}. Estado \
             incompatível."
        ))),
        Some(_) => Ok(None),
    }
}

/// **As regras estruturais dos quatro tipos da B3, num lugar só.**
///
/// A mesma função responde as duas perguntas:
///
/// ```text
/// apply remoto     este evento pode ser aplicado aqui?        (falta = espera; erro = recusa)
/// Mutacao local    este estado pode virar evento?             (qualquer não-Ok = rollback)
/// ```
///
/// Duas implementações separadas divergiriam, e o lado local produziria evento que o lado remoto
/// recusa — a pior forma de divergir, porque o autor só descobre no outro aparelho.
pub(super) fn validar(
    connection: &Connection,
    tipo: &str,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    match tipo {
        "entity" => {
            let entidade: EntidadeCanonica = serde_json::from_str(payload).map_err(de_erro)?;
            Ok((!existe(connection, "universes", &entidade.universe_id)?)
                .then(|| format!("universe {}", entidade.universe_id)))
        }
        "relation" => {
            let relacao: RelacaoCanonica = serde_json::from_str(payload).map_err(de_erro)?;
            if !existe(connection, "universes", &relacao.universe_id)? {
                return Ok(Some(format!("universe {}", relacao.universe_id)));
            }
            if let Some(falta) = ponta(
                connection,
                &relacao.source_id,
                &relacao.universe_id,
                "ponta de origem",
            )? {
                return Ok(Some(falta));
            }
            ponta(
                connection,
                &relacao.target_id,
                &relacao.universe_id,
                "ponta de destino",
            )
        }
        "timeline_event" => {
            let evento: EventoCanonico = serde_json::from_str(payload).map_err(de_erro)?;
            exigir_finito(evento.sort_key, "sortKey", &evento.id)?;
            if !existe(connection, "universes", &evento.universe_id)? {
                return Ok(Some(format!("universe {}", evento.universe_id)));
            }
            match evento.entity_id.as_deref() {
                Some(entidade) => ponta(
                    connection,
                    entidade,
                    &evento.universe_id,
                    "entidade do evento",
                ),
                None => Ok(None),
            }
        }
        "canvas_entity_position" => {
            let posicao: PosicaoCanonica = serde_json::from_str(payload).map_err(de_erro)?;
            exigir_finito(posicao.position_x, "positionX", &posicao.entity_id)?;
            exigir_finito(posicao.position_y, "positionY", &posicao.entity_id)?;
            // Uma entidade tem UMA posição. O schema deixaria duas (a PK é (universe_id, entity_id)),
            // e o agregado é identificado só pela entidade: linha em outro universo é incompatível,
            // e criar a segunda seria inventar um segundo estado para o mesmo agregado.
            if let Some(outro) = universo_da_posicao(connection, &posicao.entity_id)? {
                if outro != posicao.universe_id {
                    return Err(DatabaseCommandError::storage(format!(
                        "A entidade {} já tem posição no universo {outro}, e o payload diz {}. Estado \
                         incompatível: uma entidade tem uma posição.",
                        posicao.entity_id, posicao.universe_id
                    )));
                }
            }
            ponta(
                connection,
                &posicao.entity_id,
                &posicao.universe_id,
                "entidade da posição",
            )
        }
        _ => Ok(None),
    }
}

fn de_erro(error: serde_json::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(format!("Payload canônico ilegível: {error}"))
}

/// O universo da (única) posição daquela entidade. Mais de uma linha é inconsistência.
fn universo_da_posicao(
    connection: &Connection,
    entity_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    let mut consulta = connection
        .prepare(
            "SELECT universe_id FROM canvas_entity_positions WHERE entity_id = ?1
              ORDER BY universe_id",
        )
        .map_err(erro)?;
    let universos: Vec<String> = consulta
        .query_map([entity_id], |row| row.get(0))
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;
    match universos.len() {
        0 => Ok(None),
        1 => Ok(Some(universos[0].clone())),
        _ => Err(DatabaseCommandError::storage(format!(
            "A entidade {entity_id} tem {} posições no canvas ({}). O agregado é identificado pela \
             entidade: não há estado canônico com duas.",
            universos.len(),
            universos.join(", ")
        ))),
    }
}

fn imutavel(
    connection: &Connection,
    tabela: &str,
    coluna: &str,
    id: &str,
    esperado: &str,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    let atual: Option<String> = connection
        .query_row(
            &format!("SELECT {coluna} FROM {tabela} WHERE id = ?1"),
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?;
    match atual {
        Some(atual) if atual != esperado => Err(DatabaseCommandError::storage(format!(
            "{} {} está em {coluna} {atual} aqui, e o evento diz {esperado}. O pai é imutável: o \
             evento não é aplicado.",
            envelope.aggregate_type, envelope.aggregate_id
        ))),
        _ => Ok(()),
    }
}

pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    // Imutabilidade é pergunta do lado remoto: um evento que diga outro pai descreve outra árvore.
    match envelope.aggregate_type.as_str() {
        "entity" => {
            let entidade: EntidadeCanonica = de_json(envelope)?;
            imutavel(
                connection,
                "entities",
                "universe_id",
                &envelope.aggregate_id,
                &entidade.universe_id,
                envelope,
            )?;
        }
        "relation" => {
            let relacao: RelacaoCanonica = de_json(envelope)?;
            for (coluna, valor) in [
                ("universe_id", &relacao.universe_id),
                ("source_id", &relacao.source_id),
                ("target_id", &relacao.target_id),
            ] {
                imutavel(
                    connection,
                    "relations",
                    coluna,
                    &envelope.aggregate_id,
                    valor,
                    envelope,
                )?;
            }
        }
        "timeline_event" => {
            let evento: EventoCanonico = de_json(envelope)?;
            imutavel(
                connection,
                "timeline_events",
                "universe_id",
                &envelope.aggregate_id,
                &evento.universe_id,
                envelope,
            )?;
            conferir_destacamento(
                connection,
                &envelope.aggregate_id,
                evento.entity_id.as_deref(),
            )?;
        }
        "canvas_entity_position" => {
            let posicao: PosicaoCanonica = de_json(envelope)?;
            if posicao.entity_id != envelope.aggregate_id {
                return Err(DatabaseCommandError::storage(format!(
                    "A posição descreve a entidade {}, e o envelope é de {}.",
                    posicao.entity_id, envelope.aggregate_id
                )));
            }
        }
        _ => {}
    }
    // E depois a MESMA validação estrutural que a `Mutacao` local usa.
    validar(connection, &envelope.aggregate_type, &envelope.payload)
}

/// **A entidade de um evento se destaca, e não se reancora.**
///
/// ```text
/// linha existente:  Some(E) → Some(E)   ok
///                   Some(E) → None      ok      é o SET NULL da exclusão da entidade
///                   Some(E1) → Some(E2) recusa  trocar a entidade não é operação do app
///                   None → Some(E)      recusa  reancorar não é operação do app
///                   None → None         ok
/// linha nova:       nasce com Some(E) ou None, à vontade
/// ```
///
/// Reancorar chegando como **sequencial** significaria que a origem partiu da revisão que já tem a
/// entidade em nulo e mesmo assim a trouxe de volta: isso o app não faz.
fn conferir_destacamento(
    connection: &Connection,
    id: &str,
    no_payload: Option<&str>,
) -> DatabaseCommandResult<()> {
    let atual: Option<Option<String>> = connection
        .query_row(
            "SELECT entity_id FROM timeline_events WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?;
    let Some(aqui) = atual else {
        return Ok(());
    };
    match (aqui.as_deref(), no_payload) {
        (Some(aqui), Some(payload)) if aqui != payload => {
            Err(DatabaseCommandError::storage(format!(
                "O evento {id} está ligado à entidade {aqui} aqui, e o payload diz {payload}. \
                 Trocar a entidade de um evento não é uma operação do app; o evento não é aplicado."
            )))
        }
        (None, Some(payload)) => Err(DatabaseCommandError::storage(format!(
            "O evento {id} está sem entidade aqui (ela foi excluída), e o payload diz {payload}. \
             Reancorar a entidade de um evento não é uma operação do app; o evento não é aplicado."
        ))),
        _ => Ok(()),
    }
}

// ── Aplicação ────────────────────────────────────────────────────────────

fn conferir_id(envelope: &EventEnvelope, id: &str) -> DatabaseCommandResult<()> {
    if id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(format!(
            "O payload descreve {} {id}, e o envelope é de {}.",
            envelope.aggregate_type, envelope.aggregate_id
        )));
    }
    Ok(())
}

pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    let id = envelope.aggregate_id.as_str();
    if envelope.operation == Operation::Delete {
        let sql = match envelope.aggregate_type.as_str() {
            "entity" => "DELETE FROM entities WHERE id = ?1",
            "relation" => "DELETE FROM relations WHERE id = ?1",
            "timeline_event" => "DELETE FROM timeline_events WHERE id = ?1",
            "canvas_entity_position" => "DELETE FROM canvas_entity_positions WHERE entity_id = ?1",
            outro => return Err(super::nao_coberto(outro)),
        };
        tx.execute(sql, [id]).map_err(erro)?;
        return Ok(());
    }

    let agora = now_timestamp();
    match envelope.aggregate_type.as_str() {
        "entity" => {
            let entidade: EntidadeCanonica = de_json(envelope)?;
            conferir_id(envelope, &entidade.id)?;
            super::manuscrito::conferir_hash(&entidade.image_blob_hash)?;
            tx.execute(
                "INSERT INTO entities
                    (id, universe_id, type, name, description, summary, image, canon_status,
                     image_blob_hash, image_mime_type, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, ?8, ?9, ?10, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                    type = excluded.type, name = excluded.name,
                    description = excluded.description, summary = excluded.summary,
                    canon_status = excluded.canon_status, image = '',
                    image_blob_hash = excluded.image_blob_hash,
                    image_mime_type = excluded.image_mime_type, updated_at = excluded.updated_at",
                rusqlite::params![
                    &entidade.id,
                    &entidade.universe_id,
                    &entidade.entity_type,
                    &entidade.name,
                    &entidade.description,
                    &entidade.summary,
                    &entidade.canon_status,
                    &entidade.image_blob_hash,
                    &entidade.image_mime_type,
                    &agora
                ],
            )
            .map_err(erro)?;
            // Os atributos são estado interno: a lista do payload substitui a daqui, inteira.
            tx.execute("DELETE FROM entity_attributes WHERE entity_id = ?1", [id])
                .map_err(erro)?;
            for (posicao, atributo) in entidade.attributes.iter().enumerate() {
                tx.execute(
                    "INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![new_id(), id, &atributo.key, &atributo.value, posicao as i64],
                )
                .map_err(erro)?;
            }
            gravar_campos(
                tx,
                &entidade.universe_id,
                "entity",
                id,
                &entidade.custom_fields,
            )
        }
        "relation" => {
            let relacao: RelacaoCanonica = de_json(envelope)?;
            conferir_id(envelope, &relacao.id)?;
            tx.execute(
                "INSERT INTO relations
                    (id, universe_id, source_id, target_id, type, label, bidirectional,
                     importance, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    type = excluded.type, label = excluded.label,
                    bidirectional = excluded.bidirectional, importance = excluded.importance",
                rusqlite::params![
                    &relacao.id,
                    &relacao.universe_id,
                    &relacao.source_id,
                    &relacao.target_id,
                    &relacao.relation_type,
                    &relacao.label,
                    i64::from(relacao.bidirectional),
                    &relacao.importance,
                    &agora
                ],
            )
            .map_err(erro)?;
            Ok(())
        }
        "timeline_event" => {
            let evento: EventoCanonico = de_json(envelope)?;
            conferir_id(envelope, &evento.id)?;
            tx.execute(
                "INSERT INTO timeline_events
                    (id, universe_id, title, description, event_type, start_date, end_date,
                     entity_id, display_date, sort_key, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title, description = excluded.description,
                    event_type = excluded.event_type, start_date = excluded.start_date,
                    end_date = excluded.end_date, entity_id = excluded.entity_id,
                    display_date = excluded.display_date, sort_key = excluded.sort_key,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    &evento.id,
                    &evento.universe_id,
                    &evento.title,
                    &evento.description,
                    &evento.event_type,
                    &evento.start_date,
                    &evento.end_date,
                    &evento.entity_id,
                    &evento.display_date,
                    evento.sort_key,
                    &agora
                ],
            )
            .map_err(erro)?;
            Ok(())
        }
        "canvas_entity_position" => {
            let posicao: PosicaoCanonica = de_json(envelope)?;
            tx.execute(
                "INSERT INTO canvas_entity_positions
                    (universe_id, entity_id, position_x, position_y, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(universe_id, entity_id) DO UPDATE SET
                    position_x = excluded.position_x, position_y = excluded.position_y,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    &posicao.universe_id,
                    &posicao.entity_id,
                    posicao.position_x,
                    posicao.position_y,
                    &agora
                ],
            )
            .map_err(erro)?;
            Ok(())
        }
        outro => Err(super::nao_coberto(outro)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Vetores de canonicalização da B3.** Mudar qualquer um destes exige `canonicalFormatVersion`
    /// novo: a gênese (etapa C) usa exatamente estes codecs.
    #[test]
    fn formato_canonico_fixado_por_vetor() {
        let casos: Vec<(String, &str)> = vec![
            (
                para_json(&EntidadeCanonica {
                    id: "e1".into(),
                    universe_id: "u1".into(),
                    entity_type: "Personagem".into(),
                    name: "Frodo".into(),
                    description: "d".into(),
                    summary: "".into(),
                    canon_status: "CANON".into(),
                    image_blob_hash: "".into(),
                    image_mime_type: "".into(),
                    attributes: vec![AtributoCanonico {
                        key: "Idade".into(),
                        value: "50".into(),
                    }],
                    custom_fields: vec![CampoPersonalizado {
                        key: "Origem".into(),
                        value: "Condado".into(),
                    }],
                })
                .expect("json"),
                r#"{"id":"e1","universeId":"u1","type":"Personagem","name":"Frodo","description":"d","summary":"","canonStatus":"CANON","imageBlobHash":"","imageMimeType":"","attributes":[{"key":"Idade","value":"50"}],"customFields":[{"key":"Origem","value":"Condado"}]}"#,
            ),
            (
                para_json(&RelacaoCanonica {
                    id: "r1".into(),
                    universe_id: "u1".into(),
                    source_id: "e1".into(),
                    target_id: "e2".into(),
                    relation_type: "custom".into(),
                    label: "amigo".into(),
                    bidirectional: true,
                    importance: "normal".into(),
                })
                .expect("json"),
                r#"{"id":"r1","universeId":"u1","sourceId":"e1","targetId":"e2","type":"custom","label":"amigo","bidirectional":true,"importance":"normal"}"#,
            ),
            (
                para_json(&EventoCanonico {
                    id: "t1".into(),
                    universe_id: "u1".into(),
                    title: "Batalha".into(),
                    description: "".into(),
                    event_type: "MARCO".into(),
                    start_date: "1400-01-01".into(),
                    end_date: "".into(),
                    entity_id: Some("e1".into()),
                    display_date: "Ano 1400".into(),
                    sort_key: 2.5,
                })
                .expect("json"),
                r#"{"id":"t1","universeId":"u1","title":"Batalha","description":"","eventType":"MARCO","startDate":"1400-01-01","endDate":"","entityId":"e1","displayDate":"Ano 1400","sortKey":2.5}"#,
            ),
            (
                para_json(&EventoCanonico {
                    id: "t2".into(),
                    universe_id: "u1".into(),
                    title: "Sem entidade".into(),
                    description: "".into(),
                    event_type: "MARCO".into(),
                    start_date: "1400-01-02".into(),
                    end_date: "".into(),
                    entity_id: None,
                    display_date: "".into(),
                    sort_key: 0.0,
                })
                .expect("json"),
                r#"{"id":"t2","universeId":"u1","title":"Sem entidade","description":"","eventType":"MARCO","startDate":"1400-01-02","endDate":"","entityId":null,"displayDate":"","sortKey":0.0}"#,
            ),
            (
                para_json(&PosicaoCanonica {
                    entity_id: "e1".into(),
                    universe_id: "u1".into(),
                    position_x: -12.5,
                    position_y: 340.0,
                })
                .expect("json"),
                r#"{"entityId":"e1","universeId":"u1","positionX":-12.5,"positionY":340.0}"#,
            ),
        ];
        for (obtido, esperado) in casos {
            assert_eq!(obtido, esperado);
        }
    }

    #[test]
    fn numero_nao_finito_nao_tem_payload() {
        assert!(exigir_finito(f64::NAN, "sortKey", "t1").is_err());
        assert!(exigir_finito(f64::INFINITY, "positionX", "e1").is_err());
        assert!(exigir_finito(0.0, "sortKey", "t1").is_ok());
    }
}
