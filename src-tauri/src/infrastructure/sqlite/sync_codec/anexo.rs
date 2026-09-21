//! Codec de `attachment` — **payload definitivo** (B5; contrato de blob do ADR 0010, fatia 7).
//!
//! ```text
//! { id, universeId, ownerType, ownerId, blobHash, mimeType, caption }
//! ```
//!
//! A B1 emitia a struct `Attachment` inteira, com `createdAt` e `sortOrder` dentro. Os dois saíram:
//!
//! ```text
//! createdAt   relógio local. Dois aparelhos que criam o mesmo anexo não têm o mesmo relógio, e
//!             a identidade causal do conteúdo não pode depender do horário de quem digitou.
//! sortOrder   número FÍSICO local, calculado por `COALESCE(MAX(sort_order), -1) + 1` no INSERT.
//!             Não existe reordenação de anexo no app: nenhum comando, nenhum gateway. Mandar o
//!             número junto seria carregar posição de banco como se fosse conteúdo autoral.
//! ```
//!
//! **A posição não está aqui porque ela é outro agregado.** Desde a B2.2 a ordem da galeria
//! converge por `attachment_position` (um por anexo, com `sortOrder`): a posição não é conteúdo
//! autoral do anexo, e mover um anexo não pode revisar o conteúdo dele. A pergunta aberta na
//! seção 6.1 da matriz — lista inteira ou estado local — foi respondida por uma terceira opção,
//! posição por item.
//!
//! O que **nunca** entra é byte: o que viaja é `blobHash`, e o arquivo vai pelo caminho de fora.
//! O log é assinado e append-only — o que entra nele não sai mais.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{de_json, erro, existe, para_json, EstadoDoAgregado};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::is_known_attachment_owner;
use crate::domain::ids::now_timestamp;
use crate::domain::sync::{EventEnvelope, Operation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnexoCanonico {
    pub id: String,
    pub universe_id: String,
    pub owner_type: String,
    pub owner_id: String,
    /// `SHA-256` dos bytes reais. O arquivo viaja fora do log.
    pub blob_hash: String,
    pub mime_type: String,
    pub caption: String,
}

pub(super) fn ler(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, owner_type, owner_id, legado, blob_hash, mime_type, caption)) =
        connection
            .query_row(
                "SELECT universe_id, owner_type, owner_id, data_url, blob_hash, mime_type, caption
                   FROM attachments WHERE id = ?1",
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
    // A coluna legada é transporte de leitura para a tela. Se ela ainda tiver bytes, o anexo não
    // passou pela migração do ADR 0010 e não pode virar evento.
    super::manuscrito::exigir_imagem_migrada("attachment", id, &legado, &blob_hash)?;
    let payload = para_json(&AnexoCanonico {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        owner_type,
        owner_id,
        blob_hash,
        mime_type,
        caption,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

pub(super) fn validar(
    connection: &Connection,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    let anexo: AnexoCanonico = serde_json::from_str(payload)
        .map_err(|error| DatabaseCommandError::storage(format!("Anexo ilegível: {error}")))?;
    if !is_known_attachment_owner(&anexo.owner_type) {
        return Err(DatabaseCommandError::storage(format!(
            "Tipo de dono desconhecido para anexo: '{}'. Estado incompatível.",
            anexo.owner_type
        )));
    }
    if !anexo.blob_hash.is_empty()
        && !crate::infrastructure::blob_store::e_hash_canonico(&anexo.blob_hash)
    {
        return Err(DatabaseCommandError::storage(format!(
            "O anexo {} traz uma referência que não é um SHA-256 canônico.",
            anexo.id
        )));
    }
    if !existe(connection, "universes", &anexo.universe_id)? {
        return Ok(Some(format!("universe {}", anexo.universe_id)));
    }

    // **Identidade parental imutável.** Um anexo não muda de dono nem de universo: não existe
    // operação de mover anexo no app, e um evento que dissesse isso moveria a imagem de uma ficha
    // para outra em silêncio. Se um dia existir "mover anexo", o contrato muda de propósito, aqui.
    if let Some((universo, tipo, dono)) = connection
        .query_row(
            "SELECT universe_id, owner_type, owner_id FROM attachments WHERE id = ?1",
            [&anexo.id],
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
    {
        if (universo.as_str(), tipo.as_str(), dono.as_str())
            != (
                anexo.universe_id.as_str(),
                anexo.owner_type.as_str(),
                anexo.owner_id.as_str(),
            )
        {
            return Err(DatabaseCommandError::storage(format!(
                "O anexo {} é de {tipo} {dono} no universo {universo}, e o evento diz {} {} no \
                 universo {}. O dono do anexo é imutável: estado incompatível.",
                anexo.id, anexo.owner_type, anexo.owner_id, anexo.universe_id
            )));
        }
    }

    // **O dono precisa ser DESTE universo.** Conferir só a existência deixaria um anexo atravessar
    // a fronteira entre dois acervos: a imagem de um romance apareceria na ficha de outro.
    let universo_do_dono: Option<String> = match anexo.owner_type.as_str() {
        "universe" => {
            if anexo.owner_id != anexo.universe_id {
                return Err(DatabaseCommandError::storage(format!(
                    "O anexo {} diz ser do universo {}, e o dono declarado é o universo {}. \
                     Estado incompatível.",
                    anexo.id, anexo.universe_id, anexo.owner_id
                )));
            }
            return Ok(None);
        }
        "entity" => connection
            .query_row(
                "SELECT universe_id FROM entities WHERE id = ?1",
                [&anexo.owner_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(erro)?,
        "chapter" => connection
            .query_row(
                "SELECT s.universe_id FROM chapters c JOIN books b ON b.id = c.book_id
                   JOIN stories s ON s.id = b.story_id WHERE c.id = ?1",
                [&anexo.owner_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(erro)?,
        outro => {
            return Err(DatabaseCommandError::storage(format!(
                "Tipo de dono desconhecido para anexo: '{outro}'. Estado incompatível."
            )))
        }
    };
    match universo_do_dono {
        None => Ok(Some(format!("{} {}", anexo.owner_type, anexo.owner_id))),
        Some(outro) if outro != anexo.universe_id => Err(DatabaseCommandError::storage(format!(
            "O anexo {} é do universo {}, e {} {} é do universo {outro}. Um anexo não pertence a \
             conteúdo de outro universo: estado incompatível.",
            anexo.id, anexo.universe_id, anexo.owner_type, anexo.owner_id
        ))),
        Some(_) => Ok(None),
    }
}

pub(super) fn pai_ausente(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    validar(connection, &envelope.payload)
}

/// ## Quem espera o arquivo é a drenagem, não o codec
///
/// O payload carrega `blobHash`, e o arquivo viaja fora do log. Este `aplicar` não olha o disco: a
/// pergunta "o blob está aqui?" é da drenagem (`sync_session`), que só chama a aplicação depois de
/// conferir as referências extraídas por `sync_codec::midia` (etapa D, item 12). Blob ausente é
/// dependência de materialização — o evento espera, como espera um pai que não chegou.
pub(super) fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    if envelope.operation == Operation::Delete {
        tx.execute(
            "DELETE FROM attachments WHERE id = ?1",
            [&envelope.aggregate_id],
        )
        .map_err(erro)?;
        return Ok(());
    }
    let anexo: AnexoCanonico = de_json(envelope)?;
    if anexo.id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(
            "O payload descreve um anexo diferente do agregado do envelope.",
        ));
    }
    // `sort_order` e `created_at` são locais: quem chega por último entra no fim desta galeria.
    tx.execute(
        "INSERT INTO attachments
           (id, universe_id, owner_type, owner_id, data_url, blob_hash, mime_type, caption,
            sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7,
                 (SELECT COALESCE(MAX(sort_order), -1) + 1
                    FROM attachments
                   WHERE universe_id = ?2 AND owner_type = ?3 AND owner_id = ?4),
                 ?8)
         ON CONFLICT(id) DO UPDATE SET
            owner_type = excluded.owner_type,
            owner_id = excluded.owner_id,
            blob_hash = excluded.blob_hash,
            mime_type = excluded.mime_type,
            caption = excluded.caption",
        rusqlite::params![
            &anexo.id,
            &anexo.universe_id,
            &anexo.owner_type,
            &anexo.owner_id,
            &anexo.blob_hash,
            &anexo.mime_type,
            &anexo.caption,
            now_timestamp()
        ],
    )
    .map_err(erro)?;
    Ok(())
}
