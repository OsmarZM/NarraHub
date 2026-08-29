use crate::application::knowledge_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::knowledge::{ContentTag, ContentTagAssignment, MentionOccurrence};
use tauri::AppHandle;

#[tauri::command]
pub fn tags_list(app: AppHandle, universe_id: String) -> DatabaseCommandResult<Vec<ContentTag>> {
    knowledge_service::list_tags(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn tags_for_owner(
    app: AppHandle,
    owner_type: String,
    owner_id: String,
) -> DatabaseCommandResult<Vec<ContentTag>> {
    knowledge_service::list_owner_tags(&super::database(&app)?, &owner_type, &owner_id)
}

#[tauri::command]
pub fn tag_assignments(
    app: AppHandle,
    universe_ids: Vec<String>,
    owner_types: Vec<String>,
) -> DatabaseCommandResult<Vec<ContentTagAssignment>> {
    knowledge_service::list_assignments(&super::database(&app)?, &universe_ids, &owner_types)
}

#[tauri::command]
pub fn tag_create(
    app: AppHandle,
    universe_id: String,
    name: String,
    color: String,
) -> DatabaseCommandResult<ContentTag> {
    knowledge_service::create_tag(&super::database(&app)?, &universe_id, &name, &color)
}

#[tauri::command]
pub fn tag_set(
    app: AppHandle,
    owner_type: String,
    owner_id: String,
    tag_id: String,
    assigned: bool,
) -> DatabaseCommandResult<()> {
    knowledge_service::set_tag(
        &super::database(&app)?,
        &owner_type,
        &owner_id,
        &tag_id,
        assigned,
    )
}

#[tauri::command]
pub fn tag_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    knowledge_service::delete_tag(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn mentions_list(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<MentionOccurrence>> {
    knowledge_service::list_mentions_by_universe(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn mentions_sync(
    app: AppHandle,
    chapter_id: String,
    entity_ids: Vec<String>,
) -> DatabaseCommandResult<()> {
    knowledge_service::sync_chapter_mentions(&super::database(&app)?, &chapter_id, &entity_ids)
}
