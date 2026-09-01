use crate::application::planning_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::planning::{PlanningCardPlacement, PlanningFieldDefinition, PlanningItem};
use std::collections::BTreeMap;
use tauri::AppHandle;

#[tauri::command]
pub fn planning_list(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<PlanningItem>> {
    planning_service::list(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn planning_create(
    app: AppHandle,
    universe_id: String,
    title: String,
    description: String,
    chapter_id: Option<String>,
    image: String,
) -> DatabaseCommandResult<String> {
    planning_service::create(
        &super::database(&app)?,
        &universe_id,
        &title,
        &description,
        chapter_id.as_deref(),
        &image,
    )
}

#[tauri::command]
pub fn planning_delete(
    app: AppHandle,
    id: String,
    universe_id: String,
) -> DatabaseCommandResult<()> {
    planning_service::delete(&super::database(&app)?, &id, &universe_id)
}

#[tauri::command]
pub fn planning_save_order(
    app: AppHandle,
    universe_id: String,
    placements: Vec<PlanningCardPlacement>,
) -> DatabaseCommandResult<()> {
    planning_service::save_order(&super::database(&app)?, &universe_id, &placements)
}

#[tauri::command]
pub fn planning_field_links(
    app: AppHandle,
    card_id: String,
) -> DatabaseCommandResult<BTreeMap<String, Vec<String>>> {
    planning_service::list_field_links(&super::database(&app)?, &card_id)
}

/// `card_id` ausente devolve o catálogo do universo; presente, devolve só o
/// que aquela ficha pode mostrar (universais + os do próprio card).
#[tauri::command]
pub fn planning_field_definitions(
    app: AppHandle,
    universe_id: String,
    card_id: Option<String>,
) -> DatabaseCommandResult<Vec<PlanningFieldDefinition>> {
    planning_service::list_field_definitions(
        &super::database(&app)?,
        &universe_id,
        card_id.as_deref(),
    )
}

#[tauri::command]
pub fn planning_field_definition_create(
    app: AppHandle,
    universe_id: String,
    name: String,
    field_type: String,
    options: Vec<String>,
    scope: String,
    card_id: Option<String>,
) -> DatabaseCommandResult<PlanningFieldDefinition> {
    planning_service::create_field_definition(
        &super::database(&app)?,
        &universe_id,
        &name,
        &field_type,
        &options,
        &scope,
        card_id.as_deref(),
    )
}

#[tauri::command]
pub fn planning_field_definition_set_scope(
    app: AppHandle,
    id: String,
    universe_id: String,
    scope: String,
    card_id: Option<String>,
) -> DatabaseCommandResult<()> {
    planning_service::set_field_definition_scope(
        &super::database(&app)?,
        &id,
        &universe_id,
        &scope,
        card_id.as_deref(),
    )
}

#[tauri::command]
pub fn planning_field_definition_rename(
    app: AppHandle,
    id: String,
    universe_id: String,
    name: String,
) -> DatabaseCommandResult<()> {
    planning_service::rename_field_definition(&super::database(&app)?, &id, &universe_id, &name)
}

#[tauri::command]
pub fn planning_field_definition_delete(
    app: AppHandle,
    id: String,
    universe_id: String,
) -> DatabaseCommandResult<()> {
    planning_service::delete_field_definition(&super::database(&app)?, &id, &universe_id)
}

/// Grava a ficha inteira do card.
///
/// Antes vivia em `database/planning.rs`, com validação, transação e SQL no mesmo arquivo.
/// Era o último comando de domínio fora de `interface/tauri` — ver a Fase 3 do roadmap.
#[tauri::command]
pub fn planning_save_card(
    app: AppHandle,
    request: planning_service::PlanningCardSaveRequest,
) -> DatabaseCommandResult<()> {
    planning_service::save_card(&super::database(&app)?, request)
}
