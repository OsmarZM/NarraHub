//! Comandos Tauri do core.
//!
//! Um comando aqui faz três coisas e nada além disso: descobre o banco,
//! chama o caso de uso e devolve o erro no contrato que o frontend já
//! entende. Regra que aparecer neste arquivo está no lugar errado.

pub mod collaboration_commands;
pub mod entity_commands;
pub mod knowledge_commands;
pub mod manuscript_commands;
pub mod planning_commands;
pub mod universe_commands;
pub mod workspace_commands;

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::infrastructure::sqlite::SqliteDatabase;
use ::tauri::AppHandle;

/// Resolve o banco do app. Não guarda estado: o caminho depende do
/// `AppHandle`, e a restauração de backup troca o arquivo debaixo do app —
/// um handle memorizado apontaria para o banco antigo depois disso.
pub fn database(app: &AppHandle) -> DatabaseCommandResult<SqliteDatabase> {
    let path = crate::database::app_database_path(app).map_err(DatabaseCommandError::storage)?;
    Ok(SqliteDatabase::new(path))
}
