use crate::application::collaboration_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::collaboration::{
    CollaborationContribution, CollaborationSession, IncomingContribution, NewCollaborationSession,
};
use tauri::AppHandle;

#[tauri::command]
pub fn collaboration_sessions(app: AppHandle) -> DatabaseCommandResult<Vec<CollaborationSession>> {
    collaboration_service::list_sessions(&super::database(&app)?)
}

#[tauri::command]
pub fn collaboration_contributions(
    app: AppHandle,
    session_id: Option<String>,
) -> DatabaseCommandResult<Vec<CollaborationContribution>> {
    collaboration_service::list_contributions(&super::database(&app)?, session_id.as_deref())
}

#[tauri::command]
pub fn collaboration_save_session(
    app: AppHandle,
    session: NewCollaborationSession,
) -> DatabaseCommandResult<()> {
    collaboration_service::save_session(&super::database(&app)?, session)
}

#[tauri::command]
pub fn collaboration_store_contribution(
    app: AppHandle,
    session_id: String,
    sequence: i64,
    contribution: IncomingContribution,
) -> DatabaseCommandResult<bool> {
    collaboration_service::store_contribution(
        &super::database(&app)?,
        &session_id,
        sequence,
        contribution,
    )
}

#[tauri::command]
pub fn collaboration_end_all(app: AppHandle, status: String) -> DatabaseCommandResult<()> {
    collaboration_service::end_all_active(&super::database(&app)?, &status)
}

#[tauri::command]
pub fn collaboration_end_session(
    app: AppHandle,
    id: String,
    status: String,
) -> DatabaseCommandResult<()> {
    collaboration_service::end_session(&super::database(&app)?, &id, &status)
}

#[tauri::command]
pub fn collaboration_review(
    app: AppHandle,
    id: String,
    decision: String,
) -> DatabaseCommandResult<()> {
    collaboration_service::review(&super::database(&app)?, &id, &decision)
}

#[tauri::command]
pub fn collaboration_approve_all(app: AppHandle, session_id: String) -> DatabaseCommandResult<i64> {
    collaboration_service::approve_all(&super::database(&app)?, &session_id)
}
