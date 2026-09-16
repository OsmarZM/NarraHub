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
//! **A consequência, dita às claras:** sem `sortOrder` no payload, cada aparelho calcula a posição
//! do anexo na hora em que ele chega, e duas galerias podem ficar em ordens diferentes depois de
//! convergir. É a mesma pergunta de `planning_field_order`, e está registrada do mesmo jeito na
//! seção 6.1 da matriz: ou nasce `attachment_order(owner)` no padrão de `chapter_order`, ou fica
//! escrito que a ordem da galeria é estado local. Decidir antes da gênese.
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
    let tabela = match anexo.owner_type.as_str() {
        "entity" => "entities",
        "chapter" => "chapters",
        // O universo é o próprio dono, e já foi conferido acima.
        _ => return Ok(None),
    };
    if !existe(connection, tabela, &anexo.owner_id)? {
        return Ok(Some(format!("{} {}", anexo.owner_type, anexo.owner_id)));
    }
    Ok(None)
}

pub(super) fn pai_ausente(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    validar(connection, &envelope.payload)
}

/// ## O cursor não espera o arquivo
///
/// O payload carrega `blobHash`, e o blob pode não estar aqui ainda — a transferência é separada
/// do log causal. O evento é aplicado, a linha materializa com a referência, e o cursor avança.
/// Travar o cursor por arquivo faltando pararia a replicação inteira por causa de uma imagem.
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
