//! Codecs de agregado do Sync V2 (NH-079, etapa B1).
//!
//! Um codec responde, para **um** tipo de agregado, as três perguntas de que a fronteira `Mutacao`
//! e a aplicação remota precisam — e responde igual nos dois lugares, porque é a mesma função:
//!
//! ```text
//! ler            o estado atual do agregado, no formato do payload do evento     (None = não existe)
//! descendentes   os agregados que a exclusão deste apagaria por FK ou gatilho   (lidos ANTES do DELETE)
//! ```
//!
//! **Cobertura desta etapa: `chapter` e `attachment`.** Tipo sem codec é erro, e é de propósito:
//! declarar mutação de um agregado que ninguém sabe ler geraria evento sem conteúdo verificável.
//! A matriz em `docs/sync/MATRIZ_COBERTURA_NH079.md` diz o que entra em cada etapa.
//!
//! O formato do payload é o que já circula (etapas 5 e 13): a struct `Chapter` e a `Attachment`
//! sem `data_url`. A canonicalização definitiva — campos, ordem, exclusão de relógio local —
//! chega com a gênese (etapa C) e a versão de formato no hello (etapa E).

use rusqlite::{Connection, OptionalExtension};

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::Attachment;
use crate::domain::sync::AggregateRef;
use crate::infrastructure::sqlite::{canvas_repository, manuscript_repository};

/// O estado de um agregado como o evento o carrega.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDoAgregado {
    pub universe_id: String,
    pub payload: String,
}

/// Os tipos que a fronteira sabe representar nesta etapa.
pub const TIPOS_COBERTOS: &[&str] = &["chapter", "attachment"];

pub fn coberto(tipo: &str) -> bool {
    TIPOS_COBERTOS.contains(&tipo)
}

pub fn nao_coberto(tipo: &str) -> DatabaseCommandError {
    DatabaseCommandError::storage(format!(
        "O agregado '{tipo}' ainda não é coberto pelo Sync V2 (NH-079). Uma mutação dele não pode \
         ser declarada: o evento não teria como ser lido nem verificado."
    ))
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// O estado atual, lido na conexão/transação recebida.
pub fn ler(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    match agregado.aggregate_type.as_str() {
        "chapter" => ler_capitulo(connection, &agregado.aggregate_id),
        "attachment" => ler_anexo(connection, &agregado.aggregate_id),
        outro => Err(nao_coberto(outro)),
    }
}

/// Os descendentes que a exclusão apagaria, **filhos antes do pai** e do mais profundo para cima.
///
/// Inclui o que um gatilho apaga, não só a chave estrangeira: `trg_chapter_attachments_delete`
/// remove os anexos do capítulo sem que nenhum `FOREIGN KEY` diga isso.
pub fn descendentes(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Vec<AggregateRef>> {
    match agregado.aggregate_type.as_str() {
        "chapter" => {
            let mut consulta = connection
                .prepare(
                    "SELECT id FROM attachments
                      WHERE owner_type = 'chapter' AND owner_id = ?1
                      ORDER BY sort_order, id",
                )
                .map_err(erro)?;
            let ids = consulta
                .query_map([&agregado.aggregate_id], |row| row.get::<_, String>(0))
                .map_err(erro)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(erro)?;
            Ok(ids
                .into_iter()
                .map(|id| AggregateRef::new("attachment", &id))
                .collect())
        }
        "attachment" => Ok(Vec::new()),
        outro => Err(nao_coberto(outro)),
    }
}

fn ler_capitulo(
    connection: &Connection,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(capitulo) = manuscript_repository::get_chapter(connection, id)? else {
        return Ok(None);
    };
    let universe_id: String = connection
        .query_row(
            "SELECT s.universe_id
               FROM chapters c
               JOIN books b ON b.id = c.book_id
               JOIN stories s ON s.id = b.story_id
              WHERE c.id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?
        .ok_or_else(|| {
            DatabaseCommandError::storage(format!(
                "O capítulo {id} não está ligado a um universo por livro e história."
            ))
        })?;
    let payload = serde_json::to_string(&capitulo)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

fn ler_anexo(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
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

/// Por que um agregado não pode ser apagado agora (preflight da exclusão).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoConcorrente {
    /// Há divergência aberta: duas versões esperando o escritor decidir.
    DivergenciaAberta,
    /// Há evento recebido e ainda não aplicado (história que falta chegar).
    EventoPendente,
}

/// O agregado tem estado concorrente que uma exclusão destruiria?
pub fn estado_concorrente(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<EstadoConcorrente>> {
    let divergencia: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_divergences
                            WHERE aggregate_type = ?1 AND aggregate_id = ?2 AND resolved_at = '')",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    if divergencia {
        return Ok(Some(EstadoConcorrente::DivergenciaAberta));
    }
    let pendente: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events e
                            WHERE e.aggregate_type = ?1 AND e.aggregate_id = ?2
                              AND NOT EXISTS (SELECT 1 FROM sync_applied_events a
                                               WHERE a.event_id = e.event_id))",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    Ok(pendente.then_some(EstadoConcorrente::EventoPendente))
}
