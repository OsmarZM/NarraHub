//! Codecs de agregado do Sync V2 (NH-079).
//!
//! Um codec responde, para **um** tipo de agregado, as perguntas de que a fronteira `Mutacao`, a
//! aplicação remota e — na etapa C — a gênese precisam. É a mesma função nos três lugares:
//!
//! ```text
//! ler_canonico           estado atual no formato do payload do evento        (None = não existe)
//! impactos_da_exclusao   o que a exclusão deste agregado faz com OUTROS agregados, antes do DELETE
//! pai_ausente            o upsert remoto depende de algo que ainda não chegou?
//! aplicar                escreve no domínio o que o evento diz (o estado causal é de sync_apply)
//! ```
//!
//! **Mesmo estado = mesmo payload = mesma revisão.** O payload canônico não carrega relógio local
//! (`created_at`, `updated_at`), cache, contagem derivável nem posição que pertence a outro agregado.
//! Os formatos estão em `docs/sync/MATRIZ_COBERTURA_NH079.md`, seção 8.
//!
//! ## Impacto de exclusão: excluir não é o único efeito
//!
//! ```text
//! Excluido(agregado)    some junto (FK CASCADE ou gatilho)  → evento delete, filhos antes do pai
//! Reescrito(agregado)   sobrevive, mas muda (SET NULL, lista que perde um item)
//!                                                          → estado relido depois do SQL → upsert
//! Bloqueado(motivo)     o efeito atinge agregado ainda não coberto → a exclusão é recusada
//! ```
//!
//! O catálogo de todos os efeitos do schema — FK e gatilho — está em [`catalogo`], com gate que
//! reprova qualquer FK ou gatilho novo sem classificação.

use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

mod anexo;
pub mod catalogo;
pub mod manuscrito;
pub mod palavras;

/// O estado de um agregado como o evento o carrega.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDoAgregado {
    pub universe_id: String,
    pub payload: String,
}

/// O que a exclusão de um agregado faz com outro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impacto {
    /// Some junto com o agregado excluído.
    Excluido(AggregateRef),
    /// Continua existindo, com estado diferente.
    Reescrito(AggregateRef),
    /// O efeito atinge algo que o Sync V2 ainda não sabe representar. Fail closed.
    Bloqueado(String),
}

/// Os tipos que a fronteira sabe representar.
pub const TIPOS_COBERTOS: &[&str] = &[
    "universe",
    "story",
    "book",
    "chapter",
    "chapter_order",
    "attachment",
    "tag_assignment",
];

pub fn coberto(tipo: &str) -> bool {
    TIPOS_COBERTOS.contains(&tipo)
}

pub fn nao_coberto(tipo: &str) -> DatabaseCommandError {
    DatabaseCommandError::storage(format!(
        "O agregado '{tipo}' ainda não é coberto pelo Sync V2 (NH-079). Uma mutação dele não pode \
         ser declarada: o evento não teria como ser lido nem verificado."
    ))
}

pub(crate) fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// Agregado cuja existência é derivada de outro: não tem linha própria.
///
/// `chapter_order(livro)` existe enquanto o livro existe. Na exclusão remota, ele só conta como
/// "descendente vivo" se ainda tiver revisão corrente — a linha que o sustenta é o livro.
pub fn existencia_derivada(tipo: &str) -> bool {
    tipo == "chapter_order"
}

/// O estado canônico atual, lido na conexão/transação recebida.
pub fn ler_canonico(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let id = agregado.aggregate_id.as_str();
    match agregado.aggregate_type.as_str() {
        "universe" => manuscrito::ler_universo(connection, id),
        "story" => manuscrito::ler_historia(connection, id),
        "book" => manuscrito::ler_livro(connection, id),
        "chapter" => manuscrito::ler_capitulo(connection, id),
        "chapter_order" => manuscrito::ler_ordem(connection, id),
        "tag_assignment" => manuscrito::ler_atribuicao(connection, id),
        "attachment" => anexo::ler(connection, id),
        outro => Err(nao_coberto(outro)),
    }
}

/// Os efeitos **diretos** da exclusão sobre outros agregados, lidos ANTES do SQL destrutivo.
///
/// Só um nível: quem precisa da árvore desce pelos `Excluido`. Estado interno do próprio agregado
/// (campos personalizados, por exemplo) não é impacto — some com ele e não tem identidade própria.
pub fn impactos_da_exclusao(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let id = agregado.aggregate_id.as_str();
    match agregado.aggregate_type.as_str() {
        "universe" => Ok(manuscrito::impactos_do_universo()),
        "story" => manuscrito::impactos_da_historia(connection, id),
        "book" => manuscrito::impactos_do_livro(connection, id),
        "chapter" => manuscrito::impactos_do_capitulo(connection, id),
        "chapter_order" | "attachment" | "tag_assignment" => Ok(Vec::new()),
        outro => Err(nao_coberto(outro)),
    }
}

/// Para upsert remoto: o agregado de que este depende ainda não existe aqui?
///
/// Devolve a descrição do que falta. Quem chama trata como história incompleta — não aplica, não
/// marca como aplicado, e o cursor espera o pai chegar.
pub fn pai_ausente(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    if envelope.operation != Operation::Upsert {
        return Ok(None);
    }
    match envelope.aggregate_type.as_str() {
        "universe" => Ok(None),
        "story" | "book" | "chapter" | "chapter_order" | "tag_assignment" => {
            manuscrito::pai_ausente(connection, envelope)
        }
        "attachment" => anexo::pai_ausente(connection, envelope),
        _ => Ok(None),
    }
}

/// Escreve no domínio o que o evento diz. Tombstone, revisão corrente e cursor são de `sync_apply`.
///
/// **Falha fechada em tipo desconhecido.** Ignorar um agregado que ainda não sabemos aplicar
/// produziria o pior estado possível: o evento constaria como aplicado e o dado nunca chegaria.
pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    match envelope.aggregate_type.as_str() {
        "universe" | "story" | "book" | "chapter" | "chapter_order" | "tag_assignment" => {
            manuscrito::aplicar(tx, envelope)
        }
        "attachment" => anexo::aplicar(tx, envelope),
        outro => Err(DatabaseCommandError::storage(format!(
            "Agregado '{outro}' ainda não tem aplicação de evento implementada. A sessão para \
             aqui de propósito: avançar marcaria o evento como aplicado sem que o dado tivesse \
             chegado, e ninguém saberia que faltou."
        ))),
    }
}

/// O payload de um evento, no tipo canônico. Campo desconhecido é recusado: outro formato de
/// canonicalização não pode ser aplicado como se fosse este.
pub(crate) fn de_json<T: serde::de::DeserializeOwned>(
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<T> {
    serde_json::from_str(&envelope.payload).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O payload de {} {} não está no formato canônico: {error}",
            envelope.aggregate_type, envelope.aggregate_id
        ))
    })
}

pub fn para_json<T: serde::Serialize>(valor: &T) -> DatabaseCommandResult<String> {
    serde_json::to_string(valor).map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

pub(crate) fn existe(
    connection: &Connection,
    tabela: &str,
    id: &str,
) -> DatabaseCommandResult<bool> {
    connection
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {tabela} WHERE id = ?1)"),
            [id],
            |row| row.get(0),
        )
        .map_err(erro)
}

/// Por que um agregado não pode ser alterado silenciosamente agora (preflight).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoConcorrente {
    /// Há divergência aberta: duas versões esperando o escritor decidir.
    DivergenciaAberta,
    /// Há evento recebido e ainda não aplicado (história que falta chegar).
    EventoPendente,
}

/// O agregado tem estado concorrente que uma exclusão ou reescrita destruiria?
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

/// A revisão corrente do agregado, se houver.
pub fn revisao_corrente(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT current_rev FROM sync_aggregate_state
              WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

/// O payload da revisão corrente, quando ela veio de um evento que está no log.
pub fn payload_da_revisao_corrente(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT e.payload
               FROM sync_aggregate_state s
               JOIN sync_revision_history h
                 ON h.aggregate_type = s.aggregate_type AND h.aggregate_id = s.aggregate_id
                AND h.rev = s.current_rev
               JOIN sync_events e ON e.event_id = h.event_id
              WHERE s.aggregate_type = ?1 AND s.aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}
