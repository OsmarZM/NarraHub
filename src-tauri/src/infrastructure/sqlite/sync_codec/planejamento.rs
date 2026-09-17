//! Codecs do planejamento (B4): `planning_item` e `planning_field_definition`.
//!
//! ## O que é card e o que é lugar
//!
//! ```text
//! planning_item                 o card inteiro: texto, imagem, capítulo ligado, valores e relações
//! planning_item_position(item)  a etapa (coluna) e a posição do card — ver `super::posicao`
//! planning_field_definition     a propriedade do universo (ou exclusiva de um card)
//! ```
//!
//! Mover um card é **revisão da posição dele**, não do conteúdo: arrastar no quadro e editar o texto
//! são ações diferentes e não podem conflitar entre si. Até a B2.2 o lugar de todos os cards era uma
//! lista só por universo (`planning_order`), e dois cards criados ao mesmo tempo viravam divergência.
//!
//! ## Valores do card: duas tabelas, um conceito, sem duplicata
//!
//! ```text
//! custom_field_values (JSON)   valores ESCALARES (texto, número, checkbox, select…)
//! planning_field_links         valores de RELAÇÃO (story, character/entity, tags)
//! ```
//!
//! A migration 13 moveu as relações que a build de desenvolvimento havia escrito no JSON para a
//! tabela normalizada e limpou o JSON. Desde então não há dois donos do mesmo dado: o JSON não é
//! legado a migrar, é a representação corrente dos escalares. As duas coisas são **estado interno do
//! card** — mudar um campo é uma revisão do `planning_item`, nunca um evento por linha ou por link.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{de_json, erro, existe, para_json, EstadoDoAgregado, Impacto};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::planning::{is_known_field_scope, SCOPE_CARD};
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

/// Um valor escalar do card, identificado pela definição de campo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValorDeCampo {
    pub field_id: String,
    pub value: Value,
}

/// Um valor de relação do card. `kind` é a ponta preenchida na linha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LigacaoDeCampo {
    pub field_id: String,
    pub kind: String,
    pub target_id: String,
}

pub const TIPOS_DE_LIGACAO: &[&str] = &["story", "entity", "tag"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardCanonico {
    pub id: String,
    pub universe_id: String,
    pub chapter_id: Option<String>,
    pub title: String,
    pub description: String,
    pub target_words: i64,
    pub image_blob_hash: String,
    pub image_mime_type: String,
    /// Estado interno: escalares, em ordem de `fieldId`.
    pub values: Vec<ValorDeCampo>,
    /// Estado interno: relações, em ordem de (`fieldId`, `kind`, `targetId`).
    pub links: Vec<LigacaoDeCampo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CampoCanonico {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub field_type: String,
    /// As opções de `select`/`multi_select`, como o array que o banco guarda em `options_json`.
    pub options: Value,
    pub scope: String,
    /// Só no escopo `card`: o card dono. **Dependência causal explícita** — o campo exclusivo de um
    /// card não existe sem ele, e some com ele (FK `owner_item_id ON DELETE CASCADE`).
    pub owner_item_id: Option<String>,
}

// ── Leitura canônica ─────────────────────────────────────────────────────

fn valores_do_card(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<ValorDeCampo>> {
    let json: String = connection
        .query_row(
            "SELECT custom_field_values FROM planning_items WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    let objeto: serde_json::Map<String, Value> = serde_json::from_str(&json).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "Os valores do card {id} não são um objeto: {error}"
        ))
    })?;
    // `BTreeMap` pela ordem: o JSON do banco não tem ordem garantida, o payload canônico tem.
    let mut valores: Vec<ValorDeCampo> = objeto
        .into_iter()
        .map(|(field_id, value)| ValorDeCampo { field_id, value })
        .collect();
    valores.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    Ok(valores)
}

fn ligacoes_do_card(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Vec<LigacaoDeCampo>> {
    let mut consulta = connection
        .prepare(
            "SELECT field_definition_id,
                    CASE WHEN story_id IS NOT NULL THEN 'story'
                         WHEN entity_id IS NOT NULL THEN 'entity'
                         ELSE 'tag' END AS kind,
                    COALESCE(story_id, entity_id, tag_id) AS target_id
               FROM planning_field_links
              WHERE planning_item_id = ?1
              ORDER BY field_definition_id, kind, target_id",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([id], |row| {
            Ok(LigacaoDeCampo {
                field_id: row.get(0)?,
                kind: row.get(1)?,
                target_id: row.get(2)?,
            })
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

pub fn ler_card(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, chapter_id, title, description, target_words, legado, hash, mime)) =
        connection
            .query_row(
                "SELECT universe_id, chapter_id, title, description, target_words,
                        image, image_blob_hash, image_mime_type
                   FROM planning_items WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(erro)?
    else {
        return Ok(None);
    };
    super::manuscrito::exigir_imagem_migrada("planning_item", id, &legado, &hash)?;
    let payload = para_json(&CardCanonico {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        chapter_id,
        title,
        description,
        target_words,
        image_blob_hash: hash,
        image_mime_type: mime,
        values: valores_do_card(connection, id)?,
        links: ligacoes_do_card(connection, id)?,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

pub fn ler_campo(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, name, field_type, options_json, scope, owner_item_id)) = connection
        .query_row(
            "SELECT universe_id, name, field_type, options_json, scope, owner_item_id
               FROM planning_field_definitions WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let options: Value = serde_json::from_str(&options_json).map_err(|error| {
        DatabaseCommandError::storage(format!("As opções do campo {id} não são válidas: {error}"))
    })?;
    let payload = para_json(&CampoCanonico {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        name,
        field_type,
        options,
        scope,
        owner_item_id,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

// ── Impactos de exclusão ─────────────────────────────────────────────────

fn ids(connection: &Connection, sql: &str, parametro: &str) -> DatabaseCommandResult<Vec<String>> {
    let mut consulta = connection.prepare(sql).map_err(erro)?;
    let linhas = consulta
        .query_map([parametro], |row| row.get(0))
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

pub fn impactos_do_card(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut impactos = Vec::new();
    // FK `planning_field_definitions.owner_item_id ON DELETE CASCADE`: o campo exclusivo do card
    // some com ele.
    for campo in ids(
        connection,
        "SELECT id FROM planning_field_definitions WHERE owner_item_id = ?1 ORDER BY id",
        id,
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new(
            "planning_field_definition",
            campo,
        )));
    }
    // Gatilho `trg_planning_metadata_delete`.
    impactos.extend(super::manuscrito::atribuicoes_do_dono(
        connection, "planning", id,
    )?);
    // Nenhum card irmão muda: a posição é por item, e nada é compactado (B2.2).
    Ok(impactos)
}

/// **O gatilho que reescreve vários cards.**
///
/// ```text
/// trg_planning_field_definition_delete   tira a chave do campo de custom_field_values de CADA card
/// FK planning_field_links.field_definition_id CASCADE   apaga as relações daquele campo
/// ```
///
/// Os dois efeitos atingem o mesmo agregado: o card. Sem declarar, uma exclusão de campo reescreveria
/// quarenta cards com um evento só — exatamente a escrita invisível que abriu a NH-079.
pub fn impactos_do_campo(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut cards: Vec<String> = ids(
        connection,
        "SELECT DISTINCT planning_item_id FROM planning_field_links WHERE field_definition_id = ?1",
        id,
    )?;
    // O gatilho alcança todo card do MESMO universo que tenha a chave no JSON.
    let mut consulta = connection
        .prepare(
            "SELECT p.id FROM planning_items p
               JOIN planning_field_definitions f ON f.id = ?1
              WHERE p.universe_id = f.universe_id
                AND json_type(p.custom_field_values, '$.\"' || f.id || '\"') IS NOT NULL",
        )
        .map_err(erro)?;
    let com_valor: Vec<String> = consulta
        .query_map([id], |row| row.get(0))
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;
    cards.extend(com_valor);
    cards.sort();
    cards.dedup();
    Ok(cards
        .into_iter()
        .map(|card| Impacto::Reescrito(AggregateRef::new("planning_item", card)))
        .collect())
}

/// Os cards que perdem uma ligação quando `alvo` (história, entidade ou tag) é excluído.
pub fn cards_que_perdem_ligacao(
    connection: &Connection,
    coluna: &str,
    alvo: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let cards = ids(
        connection,
        &format!(
            "SELECT DISTINCT planning_item_id FROM planning_field_links WHERE {coluna} = ?1
              ORDER BY planning_item_id"
        ),
        alvo,
    )?;
    Ok(cards
        .into_iter()
        .map(|card| Impacto::Reescrito(AggregateRef::new("planning_item", card)))
        .collect())
}

// ── Validação (a mesma no local e no remoto) ─────────────────────────────

fn de_erro(error: serde_json::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(format!("Payload canônico ilegível: {error}"))
}

fn universo_de(
    connection: &Connection,
    tabela: &str,
    id: &str,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            &format!("SELECT universe_id FROM {tabela} WHERE id = ?1"),
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

/// O alvo existe e está no mesmo universo?
fn alvo_no_universo(
    connection: &Connection,
    tabela: &str,
    tipo: &str,
    id: &str,
    universo: &str,
) -> DatabaseCommandResult<Option<String>> {
    match universo_de(connection, tabela, id)? {
        None => Ok(Some(format!("{tipo} {id}"))),
        Some(outro) if outro != universo => Err(DatabaseCommandError::storage(format!(
            "{tipo} {id} está no universo {outro}, e o payload diz {universo}. Estado incompatível."
        ))),
        Some(_) => Ok(None),
    }
}

pub(super) fn validar(
    connection: &Connection,
    tipo: &str,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    match tipo {
        "planning_item" => {
            let card: CardCanonico = serde_json::from_str(payload).map_err(de_erro)?;
            if !existe(connection, "universes", &card.universe_id)? {
                return Ok(Some(format!("universe {}", card.universe_id)));
            }
            // O capítulo do card precisa ser deste universo (o mesmo que `save_card` cobra).
            if let Some(capitulo) = card.chapter_id.as_deref() {
                match universo_do_capitulo(connection, capitulo)? {
                    None => return Ok(Some(format!("chapter {capitulo}"))),
                    Some(outro) if outro != card.universe_id => {
                        return Err(DatabaseCommandError::storage(format!(
                            "O capítulo {capitulo} está no universo {outro}, e o card diz {}. \
                             Estado incompatível.",
                            card.universe_id
                        )))
                    }
                    Some(_) => {}
                }
            }
            for valor in &card.values {
                if let Some(falta) =
                    campo_utilizavel(connection, &valor.field_id, &card.universe_id, &card.id)?
                {
                    return Ok(Some(falta));
                }
            }
            for ligacao in &card.links {
                if let Some(falta) =
                    campo_utilizavel(connection, &ligacao.field_id, &card.universe_id, &card.id)?
                {
                    return Ok(Some(falta));
                }
                let (tabela, tipo_alvo) = match ligacao.kind.as_str() {
                    "story" => ("stories", "story"),
                    "entity" => ("entities", "entity"),
                    "tag" => ("content_tags", "content_tag"),
                    outro => {
                        return Err(DatabaseCommandError::storage(format!(
                            "Tipo de ligação desconhecido no card {}: '{outro}'.",
                            card.id
                        )))
                    }
                };
                if let Some(falta) = alvo_no_universo(
                    connection,
                    tabela,
                    tipo_alvo,
                    &ligacao.target_id,
                    &card.universe_id,
                )? {
                    return Ok(Some(falta));
                }
            }
            Ok(None)
        }
        "planning_field_definition" => {
            let campo: CampoCanonico = serde_json::from_str(payload).map_err(de_erro)?;
            if !is_known_field_scope(&campo.scope) {
                return Err(DatabaseCommandError::storage(format!(
                    "Alcance de campo desconhecido: '{}'.",
                    campo.scope
                )));
            }
            if !campo.options.is_array() {
                return Err(DatabaseCommandError::storage(format!(
                    "As opções do campo {} precisam ser uma lista.",
                    campo.id
                )));
            }
            match (campo.scope.as_str(), campo.owner_item_id.as_deref()) {
                (SCOPE_CARD, None) => {
                    return Err(DatabaseCommandError::storage(format!(
                        "O campo {} é exclusivo de um card e não diz qual.",
                        campo.id
                    )))
                }
                (scope, Some(dono)) if scope != SCOPE_CARD => {
                    return Err(DatabaseCommandError::storage(format!(
                        "O campo {} é universal e não pode ter o card {dono} como dono.",
                        campo.id
                    )))
                }
                _ => {}
            }
            if !existe(connection, "universes", &campo.universe_id)? {
                return Ok(Some(format!("universe {}", campo.universe_id)));
            }
            match campo.owner_item_id.as_deref() {
                Some(dono) => alvo_no_universo(
                    connection,
                    "planning_items",
                    "planning_item",
                    dono,
                    &campo.universe_id,
                ),
                None => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

fn universo_do_capitulo(
    connection: &Connection,
    chapter_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT s.universe_id
               FROM chapters c JOIN books b ON b.id = c.book_id
               JOIN stories s ON s.id = b.story_id
              WHERE c.id = ?1",
            [chapter_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

/// A definição existe, é deste universo e pode ser usada por este card?
fn campo_utilizavel(
    connection: &Connection,
    field_id: &str,
    universo: &str,
    card_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    let encontrado: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT universe_id, owner_item_id FROM planning_field_definitions WHERE id = ?1",
            [field_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(erro)?;
    let Some((do_campo, dono)) = encontrado else {
        return Ok(Some(format!("planning_field_definition {field_id}")));
    };
    if do_campo != universo {
        return Err(DatabaseCommandError::storage(format!(
            "O campo {field_id} é do universo {do_campo}, e o card {card_id} é de {universo}. \
             Estado incompatível."
        )));
    }
    match dono {
        Some(dono) if dono != card_id => Err(DatabaseCommandError::storage(format!(
            "O campo {field_id} é exclusivo do card {dono} e o card {card_id} tem valor nele. \
             Estado incompatível."
        ))),
        _ => Ok(None),
    }
}

// ── Dependências (lado remoto) ───────────────────────────────────────────

pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    let imutavel = |tabela: &str, coluna: &str, esperado: &str| -> DatabaseCommandResult<()> {
        let atual: Option<String> = connection
            .query_row(
                &format!("SELECT {coluna} FROM {tabela} WHERE id = ?1"),
                [&envelope.aggregate_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(erro)?;
        match atual {
            Some(atual) if atual != esperado => Err(DatabaseCommandError::storage(format!(
                "{} {} está em {coluna} {atual} aqui, e o evento diz {esperado}. O pai é imutável: \
                 o evento não é aplicado.",
                envelope.aggregate_type, envelope.aggregate_id
            ))),
            _ => Ok(()),
        }
    };
    match envelope.aggregate_type.as_str() {
        "planning_item" => {
            let card: CardCanonico = de_json(envelope)?;
            imutavel("planning_items", "universe_id", &card.universe_id)?;
        }
        "planning_field_definition" => {
            let campo: CampoCanonico = de_json(envelope)?;
            imutavel(
                "planning_field_definitions",
                "universe_id",
                &campo.universe_id,
            )?;
        }
        _ => {}
    }
    validar(connection, &envelope.aggregate_type, &envelope.payload)
}

// ── Aplicação ────────────────────────────────────────────────────────────

pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    let id = envelope.aggregate_id.as_str();
    if envelope.operation == Operation::Delete {
        let sql = match envelope.aggregate_type.as_str() {
            "planning_item" => "DELETE FROM planning_items WHERE id = ?1",
            "planning_field_definition" => "DELETE FROM planning_field_definitions WHERE id = ?1",
            outro => return Err(super::nao_coberto(outro)),
        };
        tx.execute(sql, [id]).map_err(erro)?;
        return Ok(());
    }

    let agora = now_timestamp();
    match envelope.aggregate_type.as_str() {
        "planning_item" => {
            let card: CardCanonico = de_json(envelope)?;
            if card.id != envelope.aggregate_id {
                return Err(DatabaseCommandError::storage(format!(
                    "O payload descreve o card {}, e o envelope é de {id}.",
                    card.id
                )));
            }
            super::manuscrito::conferir_hash(&card.image_blob_hash)?;
            let valores = serde_json::to_string(&serde_json::Map::from_iter(
                card.values
                    .iter()
                    .map(|valor| (valor.field_id.clone(), valor.value.clone())),
            ))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            tx.execute(
                "INSERT INTO planning_items
                    (id, universe_id, chapter_id, title, description, image, custom_field_values,
                     status, target_words, sort_order, image_blob_hash, image_mime_type,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, 'IDEIAS', ?7,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM planning_items
                           WHERE universe_id = ?2 AND status = 'IDEIAS'),
                         ?8, ?9, ?10, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                    chapter_id = excluded.chapter_id, title = excluded.title,
                    description = excluded.description, image = '',
                    custom_field_values = excluded.custom_field_values,
                    target_words = excluded.target_words,
                    image_blob_hash = excluded.image_blob_hash,
                    image_mime_type = excluded.image_mime_type,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    &card.id,
                    &card.universe_id,
                    &card.chapter_id,
                    &card.title,
                    &card.description,
                    &valores,
                    card.target_words,
                    &card.image_blob_hash,
                    &card.image_mime_type,
                    &agora
                ],
            )
            .map_err(erro)?;
            // As relações do card são reescritas por inteiro, como o próprio `save_card` faz.
            tx.execute(
                "DELETE FROM planning_field_links WHERE planning_item_id = ?1",
                [id],
            )
            .map_err(erro)?;
            for ligacao in &card.links {
                let (story, entidade, tag) = match ligacao.kind.as_str() {
                    "story" => (Some(&ligacao.target_id), None, None),
                    "entity" => (None, Some(&ligacao.target_id), None),
                    _ => (None, None, Some(&ligacao.target_id)),
                };
                tx.execute(
                    "INSERT INTO planning_field_links
                        (id, planning_item_id, field_definition_id, story_id, entity_id, tag_id,
                         created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        new_id(),
                        id,
                        &ligacao.field_id,
                        story,
                        entidade,
                        tag,
                        &agora
                    ],
                )
                .map_err(erro)?;
            }
            Ok(())
        }
        "planning_field_definition" => {
            let campo: CampoCanonico = de_json(envelope)?;
            if campo.id != envelope.aggregate_id {
                return Err(DatabaseCommandError::storage(format!(
                    "O payload descreve o campo {}, e o envelope é de {id}.",
                    campo.id
                )));
            }
            let opcoes = serde_json::to_string(&campo.options)
                .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            tx.execute(
                "INSERT INTO planning_field_definitions
                    (id, universe_id, name, field_type, options_json, sort_order, scope,
                     owner_item_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM planning_field_definitions
                           WHERE universe_id = ?2),
                         ?6, ?7, ?8, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name, field_type = excluded.field_type,
                    options_json = excluded.options_json, scope = excluded.scope,
                    owner_item_id = excluded.owner_item_id, updated_at = excluded.updated_at",
                rusqlite::params![
                    &campo.id,
                    &campo.universe_id,
                    &campo.name,
                    &campo.field_type,
                    &opcoes,
                    &campo.scope,
                    &campo.owner_item_id,
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

    /// **Vetores de canonicalização da B4.** A gênese (etapa C) usa estes mesmos codecs.
    #[test]
    fn formato_canonico_fixado_por_vetor() {
        let casos: Vec<(String, &str)> = vec![
            (
                para_json(&CardCanonico {
                    id: "p1".into(),
                    universe_id: "u1".into(),
                    chapter_id: Some("c1".into()),
                    title: "Cena do porto".into(),
                    description: "d".into(),
                    target_words: 1200,
                    image_blob_hash: "".into(),
                    image_mime_type: "".into(),
                    values: vec![ValorDeCampo {
                        field_id: "f1".into(),
                        value: Value::String("tenso".into()),
                    }],
                    links: vec![LigacaoDeCampo {
                        field_id: "f2".into(),
                        kind: "entity".into(),
                        target_id: "e1".into(),
                    }],
                })
                .expect("json"),
                r#"{"id":"p1","universeId":"u1","chapterId":"c1","title":"Cena do porto","description":"d","targetWords":1200,"imageBlobHash":"","imageMimeType":"","values":[{"fieldId":"f1","value":"tenso"}],"links":[{"fieldId":"f2","kind":"entity","targetId":"e1"}]}"#,
            ),
            (
                para_json(&CampoCanonico {
                    id: "f1".into(),
                    universe_id: "u1".into(),
                    name: "Tom".into(),
                    field_type: "select".into(),
                    options: serde_json::json!(["tenso", "calmo"]),
                    scope: "universal".into(),
                    owner_item_id: None,
                })
                .expect("json"),
                r#"{"id":"f1","universeId":"u1","name":"Tom","fieldType":"select","options":["tenso","calmo"],"scope":"universal","ownerItemId":null}"#,
            ),
        ];
        for (obtido, esperado) in casos {
            assert_eq!(obtido, esperado);
        }
    }
}
