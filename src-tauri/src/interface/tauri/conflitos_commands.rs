//! **A porta dos conflitos do Sync V2 para a tela** (etapa F).
//!
//! Três comandos e um aviso. A tela recebe DTOs prontos (`application::conflitos`): nunca uma linha
//! de `sync_divergences`, nunca um envelope. A resolução volta como uma `Acao` portátil e passa
//! pela `Mutacao`, como qualquer escrita de domínio.

use tauri::AppHandle;

use crate::application::conflitos::{
    self, AvisoDeEpoca, DetalheDoConflito, FiltroDeConflitos, ResumoDoConflito,
};
use crate::application::resolucao_divergencia::{self, Acao, ResultadoDaResolucao};
use crate::database::error::DatabaseCommandResult;

/// Os conflitos deste aparelho, com filtro por universo, tipo de item, tipo de conflito e estado.
#[tauri::command]
pub fn sync_conflitos_listar(
    app: AppHandle,
    filtro: Option<FiltroDeConflitos>,
) -> DatabaseCommandResult<Vec<ResumoDoConflito>> {
    let database = super::database(&app)?;
    let connection = database.read()?;
    conflitos::listar(&connection, &filtro.unwrap_or_default())
}

/// As duas versões de um conflito, o que mudou e o que dá para fazer.
#[tauri::command]
pub fn sync_conflito_inspecionar(
    app: AppHandle,
    conflict_key: String,
) -> DatabaseCommandResult<DetalheDoConflito> {
    let database = super::database(&app)?;
    let connection = database.read()?;
    conflitos::inspecionar(&connection, &conflict_key)
}

/// Resolve um conflito. A decisão vira um fato causal e viaja na próxima sincronização.
#[tauri::command]
pub fn sync_conflito_resolver(
    app: AppHandle,
    conflict_key: String,
    acao: Acao,
) -> DatabaseCommandResult<ResultadoDaResolucao> {
    let database = super::database(&app)?;
    let identidade = super::sync_identity(&app)?;
    resolucao_divergencia::resolver_conflito(&database, &identidade, &conflict_key, &acao)
}

/// O aviso da atualização que girou a época causal (E0-beta).
#[tauri::command]
pub fn sync_v2_aviso_de_epoca(app: AppHandle) -> DatabaseCommandResult<AvisoDeEpoca> {
    let database = super::database(&app)?;
    let connection = database.read()?;
    conflitos::aviso_de_epoca(&connection)
}
