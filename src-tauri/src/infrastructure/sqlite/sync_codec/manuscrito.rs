//! Codecs do manuscrito (B2): `universe`, `story`, `book`, `chapter`, `chapter_order` e
//! `tag_assignment` (só o que os gatilhos do manuscrito apagam).
//!
//! ## Payload canônico
//!
//! Uma struct por tipo, com os campos **na ordem declarada** (é a ordem do JSON), nomes em
//! camelCase e campo desconhecido recusado na leitura. Fica de fora tudo o que não é identidade
//! autoral do agregado:
//!
//! ```text
//! created_at, updated_at        relógio local
//! word_count                    derivável do content (palavras.rs)
//! chapters.sort_order           pertence a chapter_order(livro)
//! stories/books.sort_order      posição local de apresentação; não há reordenação pelo usuário
//! cover_image (legado)          bytes: o payload leva a referência de blob
//! ids das linhas de campo personalizado   internos; o campo é identificado pela chave
//! ```
//!
//! Campos personalizados entram como lista `[{key, value}]` na ordem `sort_order, key` — a posição
//! de cada um é a da lista, não o número guardado.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{de_json, erro, existe, palavras, para_json, EstadoDoAgregado, Impacto};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CampoPersonalizado {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UniversoCanonico {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cover_blob_hash: String,
    pub cover_mime_type: String,
    pub custom_fields: Vec<CampoPersonalizado>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoriaCanonica {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub description: String,
    pub custom_fields: Vec<CampoPersonalizado>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivroCanonico {
    pub id: String,
    pub story_id: String,
    pub name: String,
    pub description: String,
    pub cover_blob_hash: String,
    pub cover_mime_type: String,
    pub custom_fields: Vec<CampoPersonalizado>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapituloCanonico {
    pub id: String,
    pub book_id: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub scene_origin: String,
    pub scene_destination: String,
    pub status: String,
    pub canon_status: String,
    pub custom_fields: Vec<CampoPersonalizado>,
}

/// A ordem dos capítulos de um livro. Identidade do agregado: o id do livro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrdemDosCapitulos {
    pub book_id: String,
    pub chapter_ids: Vec<String>,
}

/// Marcação de tag. Identidade: `tagId:ownerType:ownerId` — duas marcações da mesma tag no mesmo
/// dono são a mesma, em qualquer aparelho (o `id` da linha é local).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtribuicaoDeTag {
    pub tag_id: String,
    pub owner_type: String,
    pub owner_id: String,
}

pub fn id_da_atribuicao(tag_id: &str, owner_type: &str, owner_id: &str) -> String {
    format!("{tag_id}:{owner_type}:{owner_id}")
}

fn partes_da_atribuicao(id: &str) -> DatabaseCommandResult<(String, String, String)> {
    let partes: Vec<&str> = id.splitn(3, ':').collect();
    match partes.as_slice() {
        [tag, tipo, dono] if !tag.is_empty() && !tipo.is_empty() && !dono.is_empty() => {
            Ok((tag.to_string(), tipo.to_string(), dono.to_string()))
        }
        _ => Err(DatabaseCommandError::storage(format!(
            "'{id}' não é uma identidade de marcação de tag (tagId:ownerType:ownerId)."
        ))),
    }
}

// ── Leitura canônica ─────────────────────────────────────────────────────

fn campos(
    connection: &Connection,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<CampoPersonalizado>> {
    let mut consulta = connection
        .prepare(
            "SELECT key, value FROM content_custom_fields
              WHERE owner_type = ?1 AND owner_id = ?2
              ORDER BY sort_order, key, id",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([owner_type, owner_id], |row| {
            Ok(CampoPersonalizado {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

/// Capa ainda em base64 na coluna legada não entra em evento: o backfill (ADR 0010) roda antes.
fn exigir_capa_migrada(
    tipo: &str,
    id: &str,
    legado: &str,
    hash: &str,
) -> DatabaseCommandResult<()> {
    if hash.is_empty() && !legado.trim().is_empty() {
        return Err(DatabaseCommandError::conflict(format!(
            "A capa de {tipo} {id} ainda não foi convertida para o armazenamento de imagens. Ela não \
             pode entrar na sincronização assim; abra o app de novo para concluir a conversão. \
             Nada foi alterado."
        )));
    }
    Ok(())
}

pub fn ler_universo(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((name, description, legado, hash, mime)) = connection
        .query_row(
            "SELECT name, description, cover_image, cover_blob_hash, cover_mime_type
               FROM universes WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    exigir_capa_migrada("universe", id, &legado, &hash)?;
    let payload = para_json(&UniversoCanonico {
        id: id.to_string(),
        name,
        description,
        cover_blob_hash: hash,
        cover_mime_type: mime,
        custom_fields: campos(connection, "universe", id)?,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id: id.to_string(),
        payload,
    }))
}

pub fn ler_historia(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, name, description)) = connection
        .query_row(
            "SELECT universe_id, name, description FROM stories WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let payload = para_json(&HistoriaCanonica {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        name,
        description,
        custom_fields: campos(connection, "story", id)?,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

fn universo_do_livro(
    connection: &Connection,
    book_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT s.universe_id FROM books b JOIN stories s ON s.id = b.story_id WHERE b.id = ?1",
            [book_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

pub fn ler_livro(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((story_id, name, description, legado, hash, mime)) = connection
        .query_row(
            "SELECT story_id, name, description, cover_image, cover_blob_hash, cover_mime_type
               FROM books WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    exigir_capa_migrada("book", id, &legado, &hash)?;
    let universe_id = universo_do_livro(connection, id)?.ok_or_else(|| {
        DatabaseCommandError::storage(format!("O livro {id} não está ligado a uma história."))
    })?;
    let payload = para_json(&LivroCanonico {
        id: id.to_string(),
        story_id,
        name,
        description,
        cover_blob_hash: hash,
        cover_mime_type: mime,
        custom_fields: campos(connection, "book", id)?,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

pub fn ler_capitulo(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(mut capitulo) = connection
        .query_row(
            "SELECT book_id, title, content, summary, scene_origin, scene_destination, status,
                    canon_status
               FROM chapters WHERE id = ?1",
            [id],
            |row| {
                Ok(CapituloCanonico {
                    id: id.to_string(),
                    book_id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    summary: row.get(3)?,
                    scene_origin: row.get(4)?,
                    scene_destination: row.get(5)?,
                    status: row.get(6)?,
                    canon_status: row.get(7)?,
                    custom_fields: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let universe_id = universo_do_livro(connection, &capitulo.book_id)?.ok_or_else(|| {
        DatabaseCommandError::storage(format!(
            "O capítulo {id} não está ligado a um universo por livro e história."
        ))
    })?;
    capitulo.custom_fields = campos(connection, "chapter", id)?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload: para_json(&capitulo)?,
    }))
}

fn ids_dos_capitulos(connection: &Connection, book_id: &str) -> DatabaseCommandResult<Vec<String>> {
    let mut consulta = connection
        .prepare("SELECT id FROM chapters WHERE book_id = ?1 ORDER BY sort_order, id")
        .map_err(erro)?;
    let linhas = consulta
        .query_map([book_id], |row| row.get(0))
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

/// Existe enquanto o livro existe; lista vazia é uma ordem válida.
pub fn ler_ordem(
    connection: &Connection,
    book_id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(universe_id) = universo_do_livro(connection, book_id)? else {
        return Ok(None);
    };
    let payload = para_json(&OrdemDosCapitulos {
        book_id: book_id.to_string(),
        chapter_ids: ids_dos_capitulos(connection, book_id)?,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

pub fn ler_atribuicao(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let (tag_id, owner_type, owner_id) = partes_da_atribuicao(id)?;
    let Some(universe_id) = connection
        .query_row(
            "SELECT t.universe_id
               FROM content_tag_assignments a JOIN content_tags t ON t.id = a.tag_id
              WHERE a.tag_id = ?1 AND a.owner_type = ?2 AND a.owner_id = ?3",
            [&tag_id, &owner_type, &owner_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let payload = para_json(&AtribuicaoDeTag {
        tag_id,
        owner_type,
        owner_id,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

// ── Impactos de exclusão ─────────────────────────────────────────────────

const UNIVERSO_BLOQUEADO: &str = "excluir um universo ainda depende de tipos que estão sendo \
     migrados para o Sync V2 (entidades, relações, linha do tempo, planejamento, tags, canvas)";

pub fn impactos_do_universo() -> Vec<Impacto> {
    vec![Impacto::Bloqueado(UNIVERSO_BLOQUEADO.to_string())]
}

fn ids(connection: &Connection, sql: &str, parametro: &str) -> DatabaseCommandResult<Vec<String>> {
    let mut consulta = connection.prepare(sql).map_err(erro)?;
    let linhas = consulta
        .query_map([parametro], |row| row.get(0))
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

/// `trg_*_metadata_delete`: as marcações do dono somem com ele.
fn atribuicoes_do_dono(
    connection: &Connection,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut consulta = connection
        .prepare(
            "SELECT tag_id FROM content_tag_assignments
              WHERE owner_type = ?1 AND owner_id = ?2 ORDER BY tag_id",
        )
        .map_err(erro)?;
    let tags: Vec<String> = consulta
        .query_map([owner_type, owner_id], |row| row.get(0))
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;
    Ok(tags
        .into_iter()
        .map(|tag| {
            Impacto::Excluido(AggregateRef::new(
                "tag_assignment",
                id_da_atribuicao(&tag, owner_type, owner_id),
            ))
        })
        .collect())
}

pub fn impactos_da_historia(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut impactos = Vec::new();
    // `planning_field_links.story_id ON DELETE CASCADE` reescreve o card (B4).
    let cards: i64 = connection
        .query_row(
            "SELECT COUNT(DISTINCT planning_item_id) FROM planning_field_links WHERE story_id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    if cards > 0 {
        impactos.push(Impacto::Bloqueado(format!(
            "a história está ligada a {cards} card(s) do planejamento, que ainda está sendo migrado \
             para o Sync V2. Remova a história dos campos desses cards antes"
        )));
    }
    for livro in ids(
        connection,
        "SELECT id FROM books WHERE story_id = ?1 ORDER BY sort_order, id",
        id,
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new("book", livro)));
    }
    impactos.extend(atribuicoes_do_dono(connection, "story", id)?);
    Ok(impactos)
}

pub fn impactos_do_livro(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<Impacto>> {
    // A ordem sai PRIMEIRO: nos outros aparelhos, quando a exclusão de cada capítulo chegar, a
    // ordem já está excluída causalmente e não conta como sobrevivente que mudaria sem revisão.
    let mut impactos = vec![Impacto::Excluido(AggregateRef::new("chapter_order", id))];
    for capitulo in ids_dos_capitulos(connection, id)? {
        impactos.push(Impacto::Excluido(AggregateRef::new("chapter", capitulo)));
    }
    impactos.extend(atribuicoes_do_dono(connection, "book", id)?);
    Ok(impactos)
}

pub fn impactos_do_capitulo(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut impactos = Vec::new();
    // `planning_items.chapter_id ON DELETE SET NULL` reescreve o card (B4).
    let cards: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM planning_items WHERE chapter_id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    if cards > 0 {
        impactos.push(Impacto::Bloqueado(format!(
            "o capítulo está ligado a {cards} card(s) do planejamento, que ainda está sendo migrado \
             para o Sync V2. Desvincule o capítulo desses cards antes"
        )));
    }
    for anexo in ids(
        connection,
        "SELECT id FROM attachments WHERE owner_type = 'chapter' AND owner_id = ?1
          ORDER BY sort_order, id",
        id,
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new("attachment", anexo)));
    }
    impactos.extend(atribuicoes_do_dono(connection, "chapter", id)?);
    let livro: Option<String> = connection
        .query_row("SELECT book_id FROM chapters WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(erro)?;
    if let Some(livro) = livro {
        impactos.push(Impacto::Reescrito(AggregateRef::new(
            "chapter_order",
            livro,
        )));
    }
    Ok(impactos)
}

// ── Dependência de criação ───────────────────────────────────────────────

pub fn pai_ausente(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    let falta = |tabela: &str, tipo: &str, id: &str| -> DatabaseCommandResult<Option<String>> {
        Ok((!existe(connection, tabela, id)?).then(|| format!("{tipo} {id}")))
    };
    match envelope.aggregate_type.as_str() {
        "story" => {
            let historia: HistoriaCanonica = de_json(envelope)?;
            falta("universes", "universe", &historia.universe_id)
        }
        "book" => {
            let livro: LivroCanonico = de_json(envelope)?;
            falta("stories", "story", &livro.story_id)
        }
        "chapter" => {
            let capitulo: CapituloCanonico = de_json(envelope)?;
            falta("books", "book", &capitulo.book_id)
        }
        "chapter_order" => {
            let ordem: OrdemDosCapitulos = de_json(envelope)?;
            falta("books", "book", &ordem.book_id)
        }
        "tag_assignment" => {
            let atribuicao: AtribuicaoDeTag = de_json(envelope)?;
            if let Some(falta) = falta("content_tags", "content_tag", &atribuicao.tag_id)? {
                return Ok(Some(falta));
            }
            let tabela = match atribuicao.owner_type.as_str() {
                "universe" => "universes",
                "story" => "stories",
                "book" => "books",
                "chapter" => "chapters",
                "entity" => "entities",
                "timeline" => "timeline_events",
                "planning" => "planning_items",
                outro => {
                    return Err(DatabaseCommandError::storage(format!(
                        "Tipo de dono de tag desconhecido: '{outro}'."
                    )))
                }
            };
            falta(tabela, &atribuicao.owner_type, &atribuicao.owner_id)
        }
        _ => Ok(None),
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

fn conferir_hash(hash: &str) -> DatabaseCommandResult<()> {
    if !hash.is_empty() && !crate::infrastructure::blob_store::e_hash_canonico(hash) {
        return Err(DatabaseCommandError::storage(
            "O evento traz uma referência de imagem que não é um SHA-256 canônico.",
        ));
    }
    Ok(())
}

fn gravar_campos(
    tx: &Transaction<'_>,
    universe_id: &str,
    owner_type: &str,
    owner_id: &str,
    campos: &[CampoPersonalizado],
) -> DatabaseCommandResult<()> {
    tx.execute(
        "DELETE FROM content_custom_fields WHERE owner_type = ?1 AND owner_id = ?2",
        [owner_type, owner_id],
    )
    .map_err(erro)?;
    let agora = now_timestamp();
    for (posicao, campo) in campos.iter().enumerate() {
        tx.execute(
            "INSERT INTO content_custom_fields
                (id, universe_id, owner_type, owner_id, key, value, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                new_id(),
                universe_id,
                owner_type,
                owner_id,
                &campo.key,
                &campo.value,
                posicao as i64,
                &agora
            ],
        )
        .map_err(erro)?;
    }
    Ok(())
}

pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    let id = envelope.aggregate_id.as_str();
    if envelope.operation == Operation::Delete {
        let sql = match envelope.aggregate_type.as_str() {
            "universe" => {
                return Err(DatabaseCommandError::storage(format!(
                    "Exclusão de universo recebida, e ela não é aplicada: {UNIVERSO_BLOQUEADO}."
                )))
            }
            "story" => "DELETE FROM stories WHERE id = ?1",
            "book" => "DELETE FROM books WHERE id = ?1",
            "chapter" => "DELETE FROM chapters WHERE id = ?1",
            // A ordem não tem linha própria: ela some com o livro.
            "chapter_order" => return Ok(()),
            "tag_assignment" => {
                let (tag, tipo, dono) = partes_da_atribuicao(id)?;
                tx.execute(
                    "DELETE FROM content_tag_assignments
                      WHERE tag_id = ?1 AND owner_type = ?2 AND owner_id = ?3",
                    [&tag, &tipo, &dono],
                )
                .map_err(erro)?;
                return Ok(());
            }
            outro => return Err(super::nao_coberto(outro)),
        };
        tx.execute(sql, [id]).map_err(erro)?;
        return Ok(());
    }

    let agora = now_timestamp();
    match envelope.aggregate_type.as_str() {
        "universe" => {
            let universo: UniversoCanonico = de_json(envelope)?;
            conferir_id(envelope, &universo.id)?;
            conferir_hash(&universo.cover_blob_hash)?;
            tx.execute(
                "INSERT INTO universes
                    (id, name, description, cover_image, cover_blob_hash, cover_mime_type,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name, description = excluded.description,
                    cover_image = '', cover_blob_hash = excluded.cover_blob_hash,
                    cover_mime_type = excluded.cover_mime_type, updated_at = excluded.updated_at",
                rusqlite::params![
                    &universo.id,
                    &universo.name,
                    &universo.description,
                    &universo.cover_blob_hash,
                    &universo.cover_mime_type,
                    &agora
                ],
            )
            .map_err(erro)?;
            gravar_campos(tx, id, "universe", id, &universo.custom_fields)
        }
        "story" => {
            let historia: HistoriaCanonica = de_json(envelope)?;
            conferir_id(envelope, &historia.id)?;
            tx.execute(
                "INSERT INTO stories (id, universe_id, name, description, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM stories WHERE universe_id = ?2),
                         ?5, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name, description = excluded.description,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    &historia.id,
                    &historia.universe_id,
                    &historia.name,
                    &historia.description,
                    &agora
                ],
            )
            .map_err(erro)?;
            gravar_campos(
                tx,
                &historia.universe_id,
                "story",
                id,
                &historia.custom_fields,
            )
        }
        "book" => {
            let livro: LivroCanonico = de_json(envelope)?;
            conferir_id(envelope, &livro.id)?;
            conferir_hash(&livro.cover_blob_hash)?;
            tx.execute(
                "INSERT INTO books
                    (id, story_id, name, description, cover_image, cover_blob_hash, cover_mime_type,
                     sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM books WHERE story_id = ?2),
                         ?7, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name, description = excluded.description,
                    cover_image = '', cover_blob_hash = excluded.cover_blob_hash,
                    cover_mime_type = excluded.cover_mime_type, updated_at = excluded.updated_at",
                rusqlite::params![
                    &livro.id,
                    &livro.story_id,
                    &livro.name,
                    &livro.description,
                    &livro.cover_blob_hash,
                    &livro.cover_mime_type,
                    &agora
                ],
            )
            .map_err(erro)?;
            let universo = universo_do_livro(tx, id)?.unwrap_or_default();
            gravar_campos(tx, &universo, "book", id, &livro.custom_fields)
        }
        "chapter" => {
            let capitulo: CapituloCanonico = de_json(envelope)?;
            conferir_id(envelope, &capitulo.id)?;
            if let Err(motivo) =
                crate::infrastructure::blob_document::exigir_blob_safe(&capitulo.content)
            {
                return Err(DatabaseCommandError::storage(format!(
                    "O evento de capítulo traz conteúdo que não pode ser gravado: {motivo}"
                )));
            }
            tx.execute(
                "INSERT INTO chapters
                    (id, book_id, title, content, summary, scene_origin, scene_destination,
                     word_count, status, canon_status, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM chapters WHERE book_id = ?2),
                         ?11, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title, content = excluded.content,
                    summary = excluded.summary, scene_origin = excluded.scene_origin,
                    scene_destination = excluded.scene_destination,
                    word_count = excluded.word_count, status = excluded.status,
                    canon_status = excluded.canon_status, updated_at = excluded.updated_at",
                rusqlite::params![
                    &capitulo.id,
                    &capitulo.book_id,
                    &capitulo.title,
                    &capitulo.content,
                    &capitulo.summary,
                    &capitulo.scene_origin,
                    &capitulo.scene_destination,
                    palavras::contar(&capitulo.content),
                    &capitulo.status,
                    &capitulo.canon_status,
                    &agora
                ],
            )
            .map_err(erro)?;
            let universo = universo_do_livro(tx, &capitulo.book_id)?.unwrap_or_default();
            gravar_campos(tx, &universo, "chapter", id, &capitulo.custom_fields)
        }
        "chapter_order" => {
            let ordem: OrdemDosCapitulos = de_json(envelope)?;
            conferir_id(envelope, &ordem.book_id)?;
            // Os listados, na posição da lista; um id que não existe aqui (ainda) é ignorado.
            // Os que existem aqui e a lista não cita vão depois, na ordem que já tinham.
            let existentes = ids_dos_capitulos(tx, id)?;
            let mut posicao = 0i64;
            let mut atualizar = tx
                .prepare("UPDATE chapters SET sort_order = ?1 WHERE id = ?2 AND book_id = ?3")
                .map_err(erro)?;
            for capitulo in ordem.chapter_ids.iter().filter(|c| existentes.contains(c)) {
                atualizar
                    .execute(rusqlite::params![posicao, capitulo, id])
                    .map_err(erro)?;
                posicao += 1;
            }
            for capitulo in existentes.iter().filter(|c| !ordem.chapter_ids.contains(c)) {
                atualizar
                    .execute(rusqlite::params![posicao, capitulo, id])
                    .map_err(erro)?;
                posicao += 1;
            }
            Ok(())
        }
        "tag_assignment" => {
            let atribuicao: AtribuicaoDeTag = de_json(envelope)?;
            conferir_id(
                envelope,
                &id_da_atribuicao(
                    &atribuicao.tag_id,
                    &atribuicao.owner_type,
                    &atribuicao.owner_id,
                ),
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    new_id(),
                    &atribuicao.tag_id,
                    &atribuicao.owner_type,
                    &atribuicao.owner_id,
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

    /// **Vetores de canonicalização.** Mudar um destes é mudar o formato canônico: toda revisão
    /// calculada antes deixa de bater. Precisa de `canonicalFormatVersion` novo (etapa E).
    #[test]
    fn formato_canonico_fixado_por_vetor() {
        let campos = vec![CampoPersonalizado {
            key: "Tom".into(),
            value: "sombrio".into(),
        }];
        let casos: Vec<(String, &str)> = vec![
            (
                para_json(&UniversoCanonico {
                    id: "u1".into(),
                    name: "Terra".into(),
                    description: "".into(),
                    cover_blob_hash: "".into(),
                    cover_mime_type: "".into(),
                    custom_fields: campos.clone(),
                })
                .expect("json"),
                r#"{"id":"u1","name":"Terra","description":"","coverBlobHash":"","coverMimeType":"","customFields":[{"key":"Tom","value":"sombrio"}]}"#,
            ),
            (
                para_json(&HistoriaCanonica {
                    id: "s1".into(),
                    universe_id: "u1".into(),
                    name: "Saga".into(),
                    description: "d".into(),
                    custom_fields: vec![],
                })
                .expect("json"),
                r#"{"id":"s1","universeId":"u1","name":"Saga","description":"d","customFields":[]}"#,
            ),
            (
                para_json(&LivroCanonico {
                    id: "b1".into(),
                    story_id: "s1".into(),
                    name: "Livro".into(),
                    description: "".into(),
                    cover_blob_hash: "".into(),
                    cover_mime_type: "".into(),
                    custom_fields: vec![],
                })
                .expect("json"),
                r#"{"id":"b1","storyId":"s1","name":"Livro","description":"","coverBlobHash":"","coverMimeType":"","customFields":[]}"#,
            ),
            (
                para_json(&CapituloCanonico {
                    id: "c1".into(),
                    book_id: "b1".into(),
                    title: "Um".into(),
                    content: "<p>texto</p>".into(),
                    summary: "".into(),
                    scene_origin: "".into(),
                    scene_destination: "".into(),
                    status: "IDEIA".into(),
                    canon_status: "CANON".into(),
                    custom_fields: vec![],
                })
                .expect("json"),
                r#"{"id":"c1","bookId":"b1","title":"Um","content":"<p>texto</p>","summary":"","sceneOrigin":"","sceneDestination":"","status":"IDEIA","canonStatus":"CANON","customFields":[]}"#,
            ),
            (
                para_json(&OrdemDosCapitulos {
                    book_id: "b1".into(),
                    chapter_ids: vec!["c3".into(), "c1".into(), "c2".into()],
                })
                .expect("json"),
                r#"{"bookId":"b1","chapterIds":["c3","c1","c2"]}"#,
            ),
            (
                para_json(&AtribuicaoDeTag {
                    tag_id: "t1".into(),
                    owner_type: "chapter".into(),
                    owner_id: "c1".into(),
                })
                .expect("json"),
                r#"{"tagId":"t1","ownerType":"chapter","ownerId":"c1"}"#,
            ),
        ];
        for (obtido, esperado) in casos {
            assert_eq!(obtido, esperado);
        }
    }

    #[test]
    fn payload_com_campo_a_mais_nao_e_aceito() {
        let resultado: Result<HistoriaCanonica, _> = serde_json::from_str(
            r#"{"id":"s1","universeId":"u1","name":"Saga","description":"","customFields":[],"updatedAt":"x"}"#,
        );
        assert!(resultado.is_err());
    }
}
