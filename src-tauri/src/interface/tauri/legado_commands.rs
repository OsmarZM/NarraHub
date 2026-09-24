//! **A porta da caixa de recuperação do legado** (etapa H, H-R3).
//!
//! A tela nunca vê `sync_conflicts` — nem o nome dela. O que atravessa aqui são itens já prontos
//! da `legacy_recovery_items`, que o importador do arranque preencheu antes de o banco ficar
//! `Ready`. Preservar cria um capítulo novo por `Mutacao` normal; descartar mexe só na caixa.

use tauri::AppHandle;

use crate::application::legado_recuperacao::{
    self, DestinoPossivel, ItemDeRecuperacao, PedidoDePreservacao,
};
use crate::database::error::DatabaseCommandResult;

/// Quantas versões antigas ainda esperam decisão. É o número do aviso das Configurações.
#[tauri::command]
pub fn legado_pendentes(app: AppHandle) -> DatabaseCommandResult<i64> {
    let database = super::database(&app)?;
    let connection = database.read()?;
    legado_recuperacao::contar_pendentes(&connection)
}

/// A caixa inteira: o que espera decisão e o que já foi decidido.
#[tauri::command]
pub fn legado_listar(app: AppHandle) -> DatabaseCommandResult<Vec<ItemDeRecuperacao>> {
    let database = super::database(&app)?;
    let connection = database.read()?;
    legado_recuperacao::listar(&connection)
}

/// Os livros que podem receber um capítulo recuperado. O do capítulo original vem marcado.
#[tauri::command]
pub fn legado_destinos(
    app: AppHandle,
    item: String,
) -> DatabaseCommandResult<Vec<DestinoPossivel>> {
    let database = super::database(&app)?;
    let connection = database.read()?;
    legado_recuperacao::destinos(&connection, &item)
}

/// Preserva a versão antiga como capítulo novo. Devolve o id do capítulo criado.
#[tauri::command]
pub fn legado_preservar(
    app: AppHandle,
    pedido: PedidoDePreservacao,
) -> DatabaseCommandResult<String> {
    let database = super::database(&app)?;
    let identidade = super::sync_identity(&app)?;
    legado_recuperacao::preservar(&database, &identidade, &pedido)
}

/// Descarta a pendência. O registro histórico continua no banco.
#[tauri::command]
pub fn legado_descartar(app: AppHandle, item: String) -> DatabaseCommandResult<()> {
    let database = super::database(&app)?;
    legado_recuperacao::descartar(&database, &item)
}
