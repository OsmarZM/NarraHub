//! Codecs do canvas (B5): `canvas_node`, `canvas_node_position` e `canvas_edge`.
//!
//! ## Conteúdo e posição são agregados diferentes
//!
//! ```text
//! canvas_node            identity = node.id   tipo, texto, imagem, cor
//! canvas_node_position   identity = node.id   onde ele está
//! ```
//!
//! `save_node_position` já era uma operação separada na interface: arrastar um elemento e editar
//! o texto dele são ações diferentes, e juntá-las num agregado só faria "A moveu" contra "B
//! escreveu" virar conflito sem que nada de verdade tivesse colidido. É a mesma separação da B3
//! entre `entity` e `canvas_entity_position`, com **uma diferença que importa**: a posição da
//! entidade tem tabela própria, e a do elemento livre mora em colunas do próprio `canvas_nodes`.
//! `canvas_node_position` é, por isso, um agregado de **existência derivada** — como as ordens,
//! ele não tem linha para chamar de sua e só existe enquanto o nó existe.
//!
//! ## A aresta e as pontas polimórficas
//!
//! ```text
//! source/target: { kind: 'entity' | 'canvas', id }
//! ```
//!
//! Não há FK possível para uma ponta que pode ser de duas tabelas, então a integridade é feita
//! aqui, com a **mesma função** na emissão local e na aplicação remota:
//!
//! ```text
//! ponta que não existe (ainda)   → dependência: o evento espera
//! ponta em outro universo        → inconsistência permanente: erro
//! ```
//!
//! E, desde a migration 22, as arestas morrem com a ponta por **gatilho de schema**, não por
//! limpeza manual de um serviço. Antes disso, apagar uma entidade deixava a aresta no arquivo
//! para sempre: ela sumia da tela pelo filtro da leitura e ninguém a via de novo. Invisível não é
//! ausente — e agora que a aresta é causal, um dado que existe no banco sem existir causalmente
//! seria divergência esperando para acontecer.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{de_json, erro, para_json, EstadoDoAgregado, Impacto};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::is_known_endpoint_kind;
use crate::domain::ids::now_timestamp;
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoCanonico {
    pub id: String,
    pub universe_id: String,
    pub kind: String,
    pub text: String,
    pub image_blob_hash: String,
    pub image_mime_type: String,
    pub color: String,
}

/// Onde o elemento livre está. Separado do conteúdo de propósito (ver o doc do módulo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoCanonica {
    pub node_id: String,
    pub position_x: f64,
    pub position_y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PontaCanonica {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArestaCanonica {
    pub id: String,
    pub universe_id: String,
    pub source: PontaCanonica,
    pub target: PontaCanonica,
    pub label: String,
}

// ── Leitura canônica ─────────────────────────────────────────────────────

pub fn ler_no(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, kind, text, legado, hash, mime, color)) = connection
        .query_row(
            "SELECT universe_id, kind, text, image, image_blob_hash, image_mime_type, color
               FROM canvas_nodes WHERE id = ?1",
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
                ))
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    super::manuscrito::exigir_imagem_migrada("canvas_node", id, &legado, &hash)?;
    let payload = para_json(&NoCanonico {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        kind,
        text,
        image_blob_hash: hash,
        image_mime_type: mime,
        color,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

/// A posição existe enquanto o nó existir: ela não tem linha própria.
pub fn ler_posicao(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, x, y)) = connection
        .query_row(
            "SELECT universe_id, position_x, position_y FROM canvas_nodes WHERE id = ?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let payload = para_json(&PosicaoCanonica {
        node_id: id.to_string(),
        position_x: x,
        position_y: y,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

pub fn ler_aresta(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(aresta) = connection
        .query_row(
            "SELECT universe_id, source_kind, source_id, target_kind, target_id, label
               FROM canvas_edges WHERE id = ?1",
            [id],
            |row| {
                Ok(ArestaCanonica {
                    id: id.to_string(),
                    universe_id: row.get(0)?,
                    source: PontaCanonica {
                        kind: row.get(1)?,
                        id: row.get(2)?,
                    },
                    target: PontaCanonica {
                        kind: row.get(3)?,
                        id: row.get(4)?,
                    },
                    label: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let universe_id = aresta.universe_id.clone();
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload: para_json(&aresta)?,
    }))
}

// ── Impactos de exclusão ─────────────────────────────────────────────────

/// As arestas com uma ponta neste alvo. É o efeito do gatilho da migration 22, lido antes do SQL.
pub fn arestas_da_ponta(
    connection: &Connection,
    kind: &str,
    id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut consulta = connection
        .prepare(
            "SELECT id FROM canvas_edges
              WHERE (source_kind = ?1 AND source_id = ?2)
                 OR (target_kind = ?1 AND target_id = ?2)
              ORDER BY id",
        )
        .map_err(erro)?;
    let arestas: Vec<String> = consulta
        .query_map([kind, id], |row| row.get(0))
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;
    Ok(arestas
        .into_iter()
        .map(|aresta| Impacto::Excluido(AggregateRef::new("canvas_edge", aresta)))
        .collect())
}

pub fn impactos_do_no(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut impactos = arestas_da_ponta(connection, "canvas", id)?;
    // A posição mora nas colunas do nó: ela some com ele, como agregado próprio.
    impactos.push(Impacto::Excluido(AggregateRef::new(
        "canvas_node_position",
        id,
    )));
    Ok(impactos)
}

// ── Validação (a mesma no local e no remoto) ─────────────────────────────

fn universo_da_ponta(
    connection: &Connection,
    kind: &str,
    id: &str,
) -> DatabaseCommandResult<Option<String>> {
    let tabela = match kind {
        "entity" => "entities",
        "canvas" => "canvas_nodes",
        outro => {
            return Err(DatabaseCommandError::storage(format!(
                "Ponta de ligação desconhecida: '{outro}'. Estado incompatível."
            )))
        }
    };
    connection
        .query_row(
            &format!("SELECT universe_id FROM {tabela} WHERE id = ?1"),
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

pub(super) fn validar(
    connection: &Connection,
    tipo: &str,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    let ilegivel = |error: serde_json::Error| {
        DatabaseCommandError::storage(format!("Canvas ilegível: {error}"))
    };
    match tipo {
        "canvas_node" => {
            let no: NoCanonico = serde_json::from_str(payload).map_err(ilegivel)?;
            if !crate::domain::canvas::is_known_node_kind(&no.kind) {
                return Err(DatabaseCommandError::storage(format!(
                    "Tipo de elemento desconhecido no canvas: '{}'. Estado incompatível.",
                    no.kind
                )));
            }
            if !super::existe(connection, "universes", &no.universe_id)? {
                return Ok(Some(format!("universe {}", no.universe_id)));
            }
            Ok(None)
        }
        "canvas_node_position" => {
            let posicao: PosicaoCanonica = serde_json::from_str(payload).map_err(ilegivel)?;
            if !super::existe(connection, "canvas_nodes", &posicao.node_id)? {
                return Ok(Some(format!("canvas_node {}", posicao.node_id)));
            }
            Ok(None)
        }
        "canvas_edge" => {
            let aresta: ArestaCanonica = serde_json::from_str(payload).map_err(ilegivel)?;
            for ponta in [&aresta.source, &aresta.target] {
                if !is_known_endpoint_kind(&ponta.kind) {
                    return Err(DatabaseCommandError::storage(format!(
                        "Ponta de ligação desconhecida: '{}'. Estado incompatível.",
                        ponta.kind
                    )));
                }
            }
            if aresta.source == aresta.target {
                return Err(DatabaseCommandError::storage(format!(
                    "A ligação {} tem as duas pontas no mesmo lugar. Estado incompatível.",
                    aresta.id
                )));
            }
            for ponta in [&aresta.source, &aresta.target] {
                match universo_da_ponta(connection, &ponta.kind, &ponta.id)? {
                    // Ainda não chegou: esperar resolve.
                    None => return Ok(Some(format!("{} {}", ponta.kind, ponta.id))),
                    Some(outro) if outro != aresta.universe_id => {
                        return Err(DatabaseCommandError::storage(format!(
                            "A ponta {} {} está no universo {outro}, e a ligação diz {}. \
                             Estado incompatível.",
                            ponta.kind, ponta.id, aresta.universe_id
                        )))
                    }
                    Some(_) => {}
                }
            }
            Ok(None)
        }
        outro => Err(super::nao_coberto(outro)),
    }
}

pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    validar(connection, &envelope.aggregate_type, &envelope.payload)
}

// ── Aplicação ────────────────────────────────────────────────────────────

pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    let id = envelope.aggregate_id.as_str();
    if envelope.operation == Operation::Delete {
        match envelope.aggregate_type.as_str() {
            "canvas_node" => {
                tx.execute("DELETE FROM canvas_nodes WHERE id = ?1", [id])
                    .map_err(erro)?;
            }
            "canvas_edge" => {
                tx.execute("DELETE FROM canvas_edges WHERE id = ?1", [id])
                    .map_err(erro)?;
            }
            // A posição não tem linha própria: ela some quando o nó sai.
            "canvas_node_position" => {}
            outro => return Err(super::nao_coberto(outro)),
        }
        return Ok(());
    }

    match envelope.aggregate_type.as_str() {
        "canvas_node" => {
            let no: NoCanonico = de_json(envelope)?;
            conferir_id(&no.id, envelope)?;
            // A posição é de outro agregado: um upsert de conteúdo não pode mover o elemento.
            tx.execute(
                "INSERT INTO canvas_nodes
                   (id, universe_id, kind, text, image, image_blob_hash, image_mime_type, color,
                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    kind = excluded.kind,
                    text = excluded.text,
                    image_blob_hash = excluded.image_blob_hash,
                    image_mime_type = excluded.image_mime_type,
                    color = excluded.color,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    &no.id,
                    &no.universe_id,
                    &no.kind,
                    &no.text,
                    &no.image_blob_hash,
                    &no.image_mime_type,
                    &no.color,
                    now_timestamp()
                ],
            )
            .map_err(erro)?;
            Ok(())
        }
        "canvas_node_position" => {
            let posicao: PosicaoCanonica = de_json(envelope)?;
            conferir_id(&posicao.node_id, envelope)?;
            let mexeu = tx
                .execute(
                    "UPDATE canvas_nodes SET position_x = ?1, position_y = ?2, updated_at = ?3
                      WHERE id = ?4",
                    rusqlite::params![
                        posicao.position_x,
                        posicao.position_y,
                        now_timestamp(),
                        &posicao.node_id
                    ],
                )
                .map_err(erro)?;
            if mexeu == 0 {
                return Err(DatabaseCommandError::storage(format!(
                    "A posição chegou para o elemento {} que não existe aqui.",
                    posicao.node_id
                )));
            }
            Ok(())
        }
        "canvas_edge" => {
            let aresta: ArestaCanonica = de_json(envelope)?;
            conferir_id(&aresta.id, envelope)?;
            tx.execute(
                "INSERT INTO canvas_edges
                   (id, universe_id, source_kind, source_id, target_kind, target_id, label,
                    created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    source_kind = excluded.source_kind,
                    source_id = excluded.source_id,
                    target_kind = excluded.target_kind,
                    target_id = excluded.target_id,
                    label = excluded.label",
                rusqlite::params![
                    &aresta.id,
                    &aresta.universe_id,
                    &aresta.source.kind,
                    &aresta.source.id,
                    &aresta.target.kind,
                    &aresta.target.id,
                    &aresta.label,
                    now_timestamp()
                ],
            )
            .map_err(erro)?;
            Ok(())
        }
        outro => Err(super::nao_coberto(outro)),
    }
}

fn conferir_id(id: &str, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    if id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(format!(
            "O payload descreve {} {id}, e o envelope é de {}.",
            envelope.aggregate_type, envelope.aggregate_id
        )));
    }
    Ok(())
}
