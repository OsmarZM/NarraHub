//! Codec de `attachment` (B1; ADR 0010, fatia 7).
//!
//! O payload é a struct `Attachment` sem `data_url`: o que viaja é a referência de blob. O formato
//! é o da B1 e está marcado para revisão na B5 (seção 8 da matriz): ainda carrega `created_at`.

use rusqlite::{Connection, Transaction};

use super::{erro, existe, EstadoDoAgregado};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::Attachment;
use crate::domain::sync::{EventEnvelope, Operation};
use crate::infrastructure::sqlite::canvas_repository;

pub(super) fn ler(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(anexo) = canvas_repository::get_attachment(connection, id)? else {
        return Ok(None);
    };
    // `data_url` é transporte de leitura para a tela; no evento ele não pode aparecer (ADR 0010).
    let payload = serde_json::to_string(&Attachment {
        data_url: String::new(),
        ..anexo.clone()
    })
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(Some(EstadoDoAgregado {
        universe_id: anexo.universe_id,
        payload,
    }))
}

fn do_evento(envelope: &EventEnvelope) -> DatabaseCommandResult<Attachment> {
    let anexo: Attachment = serde_json::from_str(&envelope.payload).map_err(|error| {
        DatabaseCommandError::storage(format!("O payload do anexo não descreve um anexo: {error}"))
    })?;
    if anexo.id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(
            "O payload descreve um anexo diferente do agregado do envelope.",
        ));
    }
    Ok(anexo)
}

pub(super) fn pai_ausente(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    let anexo = do_evento(envelope)?;
    if !existe(connection, "universes", &anexo.universe_id)? {
        return Ok(Some(format!("universe {}", anexo.universe_id)));
    }
    if anexo.owner_type == "chapter" && !existe(connection, "chapters", &anexo.owner_id)? {
        return Ok(Some(format!("chapter {}", anexo.owner_id)));
    }
    Ok(None)
}

/// ## O cursor não espera o arquivo
///
/// O payload carrega `blob_hash`, e o blob pode não estar aqui ainda — a transferência é separada
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
    let anexo = do_evento(envelope)?;
    // Bytes num evento de anexo é o defeito que a etapa 13 existe para impedir, e o log é assinado
    // e append-only: o que entra aqui não sai mais. Recusar é a última chance.
    if !anexo.data_url.is_empty() {
        return Err(DatabaseCommandError::storage(
            "O evento de anexo traz conteúdo embutido. Ele não vai ser aplicado: o contrato do \
             ADR 0010 é referência por hash, e aplicar isto gravaria os bytes de volta.",
        ));
    }
    if !anexo.blob_hash.is_empty()
        && !crate::infrastructure::blob_store::e_hash_canonico(&anexo.blob_hash)
    {
        return Err(DatabaseCommandError::storage(
            "O evento de anexo traz uma referência que não é um SHA-256 canônico.",
        ));
    }
    canvas_repository::upsert_attachment_from_event(tx, &anexo)
}
