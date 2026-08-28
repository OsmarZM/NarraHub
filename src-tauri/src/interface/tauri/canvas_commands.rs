use crate::application::canvas_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::canvas::{
    Attachment, CanvasEdge, CanvasEndpoint, CanvasEntityPosition, CanvasNode, CanvasNodePatch,
};
use tauri::AppHandle;

#[tauri::command]
pub fn canvas_nodes(app: AppHandle, universe_id: String) -> DatabaseCommandResult<Vec<CanvasNode>> {
    canvas_service::list_nodes(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn canvas_node_create(
    app: AppHandle,
    universe_id: String,
    kind: String,
    text: String,
    image: String,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<CanvasNode> {
    canvas_service::create_node(
        &super::database(&app)?,
        &universe_id,
        &kind,
        &text,
        &image,
        x,
        y,
    )
}

#[tauri::command]
pub fn canvas_node_update(
    app: AppHandle,
    id: String,
    patch: CanvasNodePatch,
) -> DatabaseCommandResult<()> {
    canvas_service::update_node(&super::database(&app)?, &id, patch)
}

#[tauri::command]
pub fn canvas_node_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    canvas_service::delete_node(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn canvas_node_position(
    app: AppHandle,
    id: String,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<()> {
    canvas_service::save_node_position(&super::database(&app)?, &id, x, y)
}

#[tauri::command]
pub fn canvas_entity_positions(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<CanvasEntityPosition>> {
    canvas_service::list_entity_positions(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn canvas_entity_position_save(
    app: AppHandle,
    universe_id: String,
    entity_id: String,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<()> {
    canvas_service::save_entity_position(&super::database(&app)?, &universe_id, &entity_id, x, y)
}

#[tauri::command]
pub fn canvas_layout_clear(app: AppHandle, universe_id: String) -> DatabaseCommandResult<()> {
    canvas_service::clear_layout(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn canvas_edges(app: AppHandle, universe_id: String) -> DatabaseCommandResult<Vec<CanvasEdge>> {
    canvas_service::list_edges(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn canvas_edge_create(
    app: AppHandle,
    universe_id: String,
    source: CanvasEndpoint,
    target: CanvasEndpoint,
    label: String,
) -> DatabaseCommandResult<CanvasEdge> {
    canvas_service::create_edge(
        &super::database(&app)?,
        &universe_id,
        &source,
        &target,
        &label,
    )
}

#[tauri::command]
pub fn canvas_edge_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    canvas_service::delete_edge(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn attachments_list(
    app: AppHandle,
    universe_id: String,
    owner_type: String,
    owner_id: String,
) -> DatabaseCommandResult<Vec<Attachment>> {
    canvas_service::list_attachments(
        &super::database(&app)?,
        &universe_id,
        &owner_type,
        &owner_id,
    )
}

#[tauri::command]
pub fn attachment_create(
    app: AppHandle,
    universe_id: String,
    owner_type: String,
    owner_id: String,
    data_url: String,
    caption: String,
) -> DatabaseCommandResult<Attachment> {
    canvas_service::create_attachment(
        &super::database(&app)?,
        &universe_id,
        &owner_type,
        &owner_id,
        &data_url,
        &caption,
    )
}

#[tauri::command]
pub fn attachment_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    canvas_service::delete_attachment(&super::database(&app)?, &id)
}
