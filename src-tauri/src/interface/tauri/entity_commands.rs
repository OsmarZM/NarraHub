use crate::application::entity_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::entity::{Entity, EntityAttribute, EntityUpdate, EntityWithDetails, NewEntity};
use tauri::AppHandle;

#[tauri::command]
pub fn entity_list(app: AppHandle, universe_id: String) -> DatabaseCommandResult<Vec<Entity>> {
    entity_service::list(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn entity_details(
    app: AppHandle,
    id: String,
) -> DatabaseCommandResult<Option<EntityWithDetails>> {
    entity_service::get_with_details(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn entity_create(app: AppHandle, input: NewEntity) -> DatabaseCommandResult<Entity> {
    entity_service::create(&super::database(&app)?, input)
}

#[tauri::command]
pub fn entity_update(app: AppHandle, id: String, patch: EntityUpdate) -> DatabaseCommandResult<()> {
    entity_service::update(&super::database(&app)?, &id, patch)
}

#[tauri::command]
pub fn entity_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    entity_service::delete(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn entity_attribute_save(
    app: AppHandle,
    attribute: EntityAttribute,
) -> DatabaseCommandResult<()> {
    entity_service::save_attribute(&super::database(&app)?, attribute)
}

#[tauri::command]
pub fn entity_attribute_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    entity_service::remove_attribute(&super::database(&app)?, &id)
}
