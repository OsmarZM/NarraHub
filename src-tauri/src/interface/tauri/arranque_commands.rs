//! O comando que prepara o acervo no arranque (NH-079 etapa C, fatia 2).
//!
//! ```text
//! migrations (database::upgrade)  →  storage_prepare_archive  →  aplicação e sync liberados
//! ```
//!
//! Uma chamada só, porque a ordem é obrigatória e não pode depender de quem chama:
//! conversão de mídia, adoção do acervo, conferência de que não sobrou órfão, e só então `Ready`.
//! A sequência inteira está em `application::arranque`.

use tauri::{AppHandle, State};

use crate::application::arranque::{preparar_acervo, ResumoDoAcervo};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::database::estado::EstadoDoBanco;
use crate::infrastructure::sqlite::SqliteDatabase;

/// **Prepara o acervo e libera o aplicativo.**
///
/// Este é o **único** caminho que abre o banco sem passar por `interface::tauri::database`, e o
/// motivo é estrutural: aquela função exige `Ready`, e é este comando que produz o `Ready`. Passar
/// por ela aqui seria pedir a chave que este comando existe para entregar.
///
/// A guarda não some — ela muda de lugar: nada de domínio roda antes de a fase virar `Ready`, e a
/// falha deixa o banco em `RecoveryRequired`, preservado, com a causa legível.
#[tauri::command]
pub async fn storage_prepare_archive(
    app: AppHandle,
    estado: State<'_, EstadoDoBanco>,
) -> DatabaseCommandResult<ResumoDoAcervo> {
    let app_data = crate::database::app_data_path(&app).map_err(DatabaseCommandError::storage)?;
    let caminho =
        crate::database::app_database_path(&app).map_err(DatabaseCommandError::storage)?;
    let database = SqliteDatabase::new(caminho);
    let store = crate::infrastructure::blob_store::BlobStore::new(&app_data);
    // A identidade precisa existir antes da gênese: ela assina os eventos dela.
    let identidade = crate::application::sync_bootstrap::prepare(&app_data, &database)?;
    preparar_acervo(&database, &store, &identidade, &estado)
}
