use crate::application::workspace_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::workspace::{HistoryEntry, RelationCard, TimelineEvent};
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
