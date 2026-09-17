//! Codecs do manuscrito (B2): `universe`, `story`, `book`, `chapter` e `tag_assignment`.
//!
//! A posição de história, livro e capítulo é agregado próprio, um por item — ver [`super::posicao`]
//! (B2.2). Até a B2.2 ela era uma lista inteira por pai (`chapter_order(livro)`), e duas criações
//! concorrentes perfeitamente compatíveis viravam divergência.
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
//! sort_order                    pertence ao agregado de posição do item (B2.2)
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

pub(super) fn campos(
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

/// Imagem ainda em base64 na coluna legada não entra em evento: o backfill (ADR 0010) roda antes.
pub(super) fn exigir_imagem_migrada(
    tipo: &str,
    id: &str,
    legado: &str,
    hash: &str,
) -> DatabaseCommandResult<()> {
    if hash.is_empty() && !legado.trim().is_empty() {
        return Err(DatabaseCommandError::conflict(format!(
            "A imagem de {tipo} {id} ainda não foi convertida para o armazenamento de imagens. Ela não \
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
    exigir_imagem_migrada("universe", id, &legado, &hash)?;
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
    exigir_imagem_migrada("book", id, &legado, &hash)?;
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

/// As regras de toda ordem recebida (story_order, book_order, chapter_order):
///
/// ```text
/// pai ausente                          → falta (PrecisaReconciliar)
/// id repetido                          → erro
/// filho de outro pai                   → erro (pai imutável)
/// filho citado que não existe aqui     → falta
/// filho daqui que a lista não cita     → falta; nunca "vai para o fim"
/// ```
///
/// Passando tudo, a ordem materializada é exatamente a lista.
/// O universo de um livro, ou erro: livro sem história é inconsistência, não "universo vazio".
fn universo_obrigatorio(connection: &Connection, book_id: &str) -> DatabaseCommandResult<String> {
    universo_do_livro(connection, book_id)?.ok_or_else(|| {
        DatabaseCommandError::storage(format!(
            "O livro {book_id} não está ligado a uma história. Nada foi aplicado."
        ))
    })
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

/// Existe enquanto o pai existe; lista vazia é uma ordem válida. Ids na ordem `sort_order, id`.
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
pub(super) fn atribuicoes_do_dono(
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
    // `planning_field_links.story_id ON DELETE CASCADE`: o card sobrevive sem a ligação (B4).
    let mut impactos = super::planejamento::cards_que_perdem_ligacao(connection, "story_id", id)?;
    for livro in ids(
        connection,
        "SELECT id FROM books WHERE story_id = ?1 ORDER BY sort_order, id",
        id,
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new("book", livro)));
    }
    impactos.extend(atribuicoes_do_dono(connection, "story", id)?);
    // Nenhuma história irmã muda: a posição é por item, e nada é compactado (B2.2).
    Ok(impactos)
}

pub fn impactos_do_livro(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<Impacto>> {
    let mut impactos = Vec::new();
    for capitulo in ids(
        connection,
        "SELECT id FROM chapters WHERE book_id = ?1 ORDER BY sort_order, id",
        id,
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new("chapter", capitulo)));
    }
    impactos.extend(atribuicoes_do_dono(connection, "book", id)?);
    // Nenhum livro irmão muda: a posição é por item, e nada é compactado (B2.2).
    Ok(impactos)
}

pub fn impactos_do_capitulo(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Vec<Impacto>> {
    // `planning_items.chapter_id ON DELETE SET NULL`: o card sobrevive com o capítulo em nulo (B4).
    let mut impactos: Vec<Impacto> = ids(
        connection,
        "SELECT id FROM planning_items WHERE chapter_id = ?1 ORDER BY id",
        id,
    )?
    .into_iter()
    .map(|card| Impacto::Reescrito(AggregateRef::new("planning_item", card)))
    .collect();
    for anexo in ids(
        connection,
        "SELECT id FROM attachments WHERE owner_type = 'chapter' AND owner_id = ?1
          ORDER BY sort_order, id",
        id,
    )? {
        impactos.push(Impacto::Excluido(AggregateRef::new("attachment", anexo)));
    }
    impactos.extend(atribuicoes_do_dono(connection, "chapter", id)?);
    // Nenhum capítulo irmão muda: a posição é por item, e nada é compactado (B2.2).
    Ok(impactos)
}

// ── Dependência de criação ───────────────────────────────────────────────

/// Confere, ANTES de aplicar um upsert remoto, tudo de que o estado final depende.
///
/// ```text
/// Ok(None)          pode aplicar; o estado materializado vai ser exatamente o payload
/// Ok(Some(falta))   história incompleta: algo que ainda pode chegar (pai, capítulo citado)
///                   → não aplica, não marca, o cursor espera
/// Err(..)           inconsistência que nunca fica válida → a sessão para
/// ```
///
/// **O pai é imutável.** Nenhuma escrita do app move história, livro ou capítulo de pai. Um evento
/// que diga outro pai para um agregado que já existe aqui não é "mover": é um payload que descreve
/// outra árvore, e registrar a revisão dele deixaria domínio e revisão dizendo coisas diferentes.
/// O universo do dono de uma marcação de tag. `None` quando o dono ainda não chegou.
///
/// Sobe a árvore quando precisa: o livro não tem `universe_id`, ele tem história.
pub(super) fn universo_do_dono(
    connection: &Connection,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    let sql = match owner_type {
        // O universo é dono de si mesmo.
        "universe" => "SELECT id FROM universes WHERE id = ?1",
        "story" => "SELECT universe_id FROM stories WHERE id = ?1",
        "book" => {
            "SELECT s.universe_id FROM books b JOIN stories s ON s.id = b.story_id \
                   WHERE b.id = ?1"
        }
        "chapter" => {
            "SELECT s.universe_id FROM chapters c JOIN books b ON b.id = c.book_id \
                      JOIN stories s ON s.id = b.story_id WHERE c.id = ?1"
        }
        "entity" => "SELECT universe_id FROM entities WHERE id = ?1",
        "timeline" => "SELECT universe_id FROM timeline_events WHERE id = ?1",
        "planning" => "SELECT universe_id FROM planning_items WHERE id = ?1",
        outro => {
            return Err(DatabaseCommandError::storage(format!(
                "Tipo de dono de tag desconhecido: '{outro}'. Estado incompatível."
            )))
        }
    };
    connection
        .query_row(sql, [owner_id], |row| row.get(0))
        .optional()
        .map_err(erro)
}

/// **A regra da marcação de tag, uma só, cobrada no local e no remoto.**
///
/// Existência é dependência (esperar resolve). **Universo diferente é inconsistência permanente**:
/// uma tag de um universo marcando conteúdo de outro nunca vira válida, e deixá-la entrar
/// atravessaria a fronteira entre dois acervos — o escritor veria a tag de um romance aparecer
/// dentro de outro, sem ter feito nada.
pub(super) fn validar_atribuicao(
    connection: &Connection,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    let atribuicao: AtribuicaoDeTag = serde_json::from_str(payload).map_err(|error| {
        DatabaseCommandError::storage(format!("Marcação de tag ilegível: {error}"))
    })?;
    let Some(universo_da_tag) = connection
        .query_row(
            "SELECT universe_id FROM content_tags WHERE id = ?1",
            [&atribuicao.tag_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(Some(format!("content_tag {}", atribuicao.tag_id)));
    };
    match universo_do_dono(connection, &atribuicao.owner_type, &atribuicao.owner_id)? {
        None => Ok(Some(format!(
            "{} {}",
            atribuicao.owner_type, atribuicao.owner_id
        ))),
        Some(universo_do_dono) if universo_do_dono != universo_da_tag => {
            Err(DatabaseCommandError::storage(format!(
                "A tag {} é do universo {universo_da_tag}, e {} {} é do universo \
                 {universo_do_dono}. Uma tag não marca conteúdo de outro universo: estado \
                 incompatível.",
                atribuicao.tag_id, atribuicao.owner_type, atribuicao.owner_id
            )))
        }
        Some(_) => Ok(None),
    }
}

pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    let falta = |tabela: &str, tipo: &str, id: &str| -> DatabaseCommandResult<Option<String>> {
        Ok((!existe(connection, tabela, id)?).then(|| format!("{tipo} {id}")))
    };
    let pai_imutavel = |tabela: &str, coluna: &str, esperado: &str| -> DatabaseCommandResult<()> {
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
        "story" => {
            let historia: HistoriaCanonica = de_json(envelope)?;
            pai_imutavel("stories", "universe_id", &historia.universe_id)?;
            falta("universes", "universe", &historia.universe_id)
        }
        "book" => {
            let livro: LivroCanonico = de_json(envelope)?;
            pai_imutavel("books", "story_id", &livro.story_id)?;
            falta("stories", "story", &livro.story_id)
        }
        "chapter" => {
            let capitulo: CapituloCanonico = de_json(envelope)?;
            pai_imutavel("chapters", "book_id", &capitulo.book_id)?;
            falta("books", "book", &capitulo.book_id)
        }
        // A MESMA função que `validar_para_emissao` chama: nenhum caminho local escapa de uma
        // regra que o outro lado cobra.
        "tag_assignment" => validar_atribuicao(connection, &envelope.payload),
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

pub(super) fn conferir_hash(hash: &str) -> DatabaseCommandResult<()> {
    if !hash.is_empty() && !crate::infrastructure::blob_store::e_hash_canonico(hash) {
        return Err(DatabaseCommandError::storage(
            "O evento traz uma referência de imagem que não é um SHA-256 canônico.",
        ));
    }
    Ok(())
}

pub(super) fn gravar_campos(
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
            let universo = universo_obrigatorio(tx, id)?;
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
            let universo = universo_obrigatorio(tx, &capitulo.book_id)?;
            gravar_campos(tx, &universo, "chapter", id, &capitulo.custom_fields)
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
