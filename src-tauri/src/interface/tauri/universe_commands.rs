use crate::application::universe_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::universe::{Universe, UniverseStats, UniverseUpdate, UniverseWithStats};
use tauri::AppHandle;

#[tauri::command]
pub fn universe_list(app: AppHandle) -> DatabaseCommandResult<Vec<UniverseWithStats>> {
    universe_service::list_with_stats(&super::database(&app)?)
}

#[tauri::command]
pub fn universe_get(app: AppHandle, id: String) -> DatabaseCommandResult<Option<Universe>> {
    universe_service::get(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn universe_stats(app: AppHandle, universe_id: String) -> DatabaseCommandResult<UniverseStats> {
    universe_service::stats(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn universe_create(
    app: AppHandle,
    name: String,
    description: String,
    cover_image: String,
) -> DatabaseCommandResult<Universe> {
    universe_service::create(&super::database(&app)?, &name, &description, &cover_image)
}

#[tauri::command]
pub fn universe_update(
    app: AppHandle,
    id: String,
    patch: UniverseUpdate,
) -> DatabaseCommandResult<()> {
    universe_service::update(&super::database(&app)?, &id, patch)
}

#[tauri::command]
pub fn universe_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    universe_service::delete(&super::database(&app)?, &id)
}
