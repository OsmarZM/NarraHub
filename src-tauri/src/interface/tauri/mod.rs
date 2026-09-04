//! Comandos Tauri do core.
//!
//! Um comando aqui faz três coisas e nada além disso: descobre o banco,
//! chama o caso de uso e devolve o erro no contrato que o frontend já
//! entende. Regra que aparecer neste arquivo está no lugar errado.

pub mod canvas_commands;
pub mod collaboration_commands;
pub mod entity_commands;
pub mod knowledge_commands;
pub mod manuscript_commands;
pub mod planning_commands;
pub mod universe_commands;
pub mod workspace_commands;

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::infrastructure::sqlite::SqliteDatabase;
use ::tauri::AppHandle;

/// Resolve o banco do app. Não guarda estado: o caminho depende do
/// `AppHandle`, e a restauração de backup troca o arquivo debaixo do app —
/// um handle memorizado apontaria para o banco antigo depois disso.
pub fn database(app: &AppHandle) -> DatabaseCommandResult<SqliteDatabase> {
    let path = crate::database::app_database_path(app).map_err(DatabaseCommandError::storage)?;
    Ok(SqliteDatabase::new(path))
}

/// A identidade de sincronização deste aparelho, pronta para assinar.
///
/// Não guarda estado, pelo mesmo motivo de `database`: a restauração de
/// backup troca o banco debaixo do app, e uma identidade memorizada deixaria
/// o `self` do roster apontando para o aparelho de onde o backup veio. Rodar
/// o arranque a cada comando de escrita é barato — `reconcile_self` é uma
/// consulta indexada quando não há nada a mudar — e é o que dispensa
/// invalidação de cache num caminho onde errar significa assinar eventos com
/// a origem de outro aparelho.
pub fn sync_identity(app: &AppHandle) -> DatabaseCommandResult<DeviceIdentity> {
    let app_data = crate::database::app_data_path(app).map_err(DatabaseCommandError::storage)?;
    crate::application::sync_bootstrap::prepare(&app_data, &database(app)?)
}
