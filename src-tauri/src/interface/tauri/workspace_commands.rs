use crate::application::workspace_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::workspace::{HistoryEntry, NewTimelineEvent, RelationCard, TimelineEvent};
use tauri::AppHandle;

#[tauri::command]
pub fn timeline_list(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<TimelineEvent>> {
    workspace_service::list_timeline(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn relations_list(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<RelationCard>> {
    workspace_service::list_relations(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn history_list(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<HistoryEntry>> {
    workspace_service::list_history(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn timeline_create(
    app: AppHandle,
    universe_id: String,
    event: NewTimelineEvent,
) -> DatabaseCommandResult<String> {
    workspace_service::create_timeline_event(&super::database(&app)?, &universe_id, event)
}

#[tauri::command]
pub fn timeline_rename(app: AppHandle, id: String, title: String) -> DatabaseCommandResult<()> {
    workspace_service::rename_timeline_event(&super::database(&app)?, &id, &title)
}

#[tauri::command]
pub fn timeline_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    workspace_service::delete_timeline_event(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn relation_create(
    app: AppHandle,
    universe_id: String,
    source_id: String,
    target_id: String,
    label: String,
) -> DatabaseCommandResult<String> {
    workspace_service::create_relation(
        &super::database(&app)?,
        &universe_id,
        &source_id,
        &target_id,
        &label,
    )
}

#[tauri::command]
pub fn relation_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    workspace_service::delete_relation(&super::database(&app)?, &id)
}
