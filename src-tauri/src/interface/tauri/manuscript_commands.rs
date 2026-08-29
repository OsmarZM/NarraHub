use crate::application::manuscript_service;
use crate::database::error::DatabaseCommandResult;
use crate::domain::manuscript::{
    Book, BookOption, BookUpdate, Chapter, ChapterOption, ChapterUpdate, Story, StoryUpdate,
};
use tauri::AppHandle;

#[tauri::command]
pub fn story_list(app: AppHandle, universe_id: String) -> DatabaseCommandResult<Vec<Story>> {
    manuscript_service::list_stories(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn story_create(
    app: AppHandle,
    universe_id: String,
    name: String,
) -> DatabaseCommandResult<Story> {
    manuscript_service::create_story(&super::database(&app)?, &universe_id, &name)
}

#[tauri::command]
pub fn story_update(app: AppHandle, id: String, patch: StoryUpdate) -> DatabaseCommandResult<()> {
    manuscript_service::update_story(&super::database(&app)?, &id, patch)
}

#[tauri::command]
pub fn story_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    manuscript_service::delete_story(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn book_list_by_story(app: AppHandle, story_id: String) -> DatabaseCommandResult<Vec<Book>> {
    manuscript_service::list_books_by_story(&super::database(&app)?, &story_id)
}

#[tauri::command]
pub fn book_list_by_universe(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<BookOption>> {
    manuscript_service::list_books_by_universe(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn book_create(app: AppHandle, story_id: String, name: String) -> DatabaseCommandResult<Book> {
    manuscript_service::create_book(&super::database(&app)?, &story_id, &name)
}

#[tauri::command]
pub fn book_update(app: AppHandle, id: String, patch: BookUpdate) -> DatabaseCommandResult<()> {
    manuscript_service::update_book(&super::database(&app)?, &id, patch)
}

#[tauri::command]
pub fn book_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    manuscript_service::delete_book(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn chapter_list_by_book(
    app: AppHandle,
    book_id: String,
) -> DatabaseCommandResult<Vec<Chapter>> {
    manuscript_service::list_chapters_by_book(&super::database(&app)?, &book_id)
}

#[tauri::command]
pub fn chapter_list_by_universe(
    app: AppHandle,
    universe_id: String,
) -> DatabaseCommandResult<Vec<ChapterOption>> {
    manuscript_service::list_chapters_by_universe(&super::database(&app)?, &universe_id)
}

#[tauri::command]
pub fn chapter_get(app: AppHandle, id: String) -> DatabaseCommandResult<Option<Chapter>> {
    manuscript_service::get_chapter(&super::database(&app)?, &id)
}

#[tauri::command]
pub fn chapter_create(
    app: AppHandle,
    book_id: String,
    title: String,
) -> DatabaseCommandResult<Chapter> {
    manuscript_service::create_chapter(&super::database(&app)?, &book_id, &title)
}

#[tauri::command]
pub fn chapter_update(
    app: AppHandle,
    id: String,
    patch: ChapterUpdate,
) -> DatabaseCommandResult<()> {
    manuscript_service::update_chapter(&super::database(&app)?, &id, patch)
}

#[tauri::command]
pub fn chapter_reorder(
    app: AppHandle,
    book_id: String,
    chapter_ids: Vec<String>,
) -> DatabaseCommandResult<()> {
    manuscript_service::reorder_chapters(&super::database(&app)?, &book_id, &chapter_ids)
}

#[tauri::command]
pub fn chapter_delete(app: AppHandle, id: String) -> DatabaseCommandResult<()> {
    manuscript_service::delete_chapter(&super::database(&app)?, &id)
}
