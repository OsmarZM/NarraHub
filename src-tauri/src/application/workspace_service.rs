use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::workspace::{HistoryEntry, NewTimelineEvent, RelationCard, TimelineEvent};
use crate::infrastructure::sqlite::{workspace_repository, SqliteDatabase};

pub fn list_timeline(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<TimelineEvent>> {
    let connection = database.read()?;
    workspace_repository::list_timeline(&connection, universe_id)
}

pub fn list_relations(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<RelationCard>> {
    let connection = database.read()?;
    workspace_repository::list_relations(&connection, universe_id)
}

pub fn list_history(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<HistoryEntry>> {
    let connection = database.read()?;
    workspace_repository::list_history(&connection, universe_id)
}

pub fn create_timeline_event(
    database: &SqliteDatabase,
    universe_id: &str,
    event: NewTimelineEvent,
) -> DatabaseCommandResult<String> {
    if event.title.trim().is_empty() {
        return Err(DatabaseCommandError::validation("O evento precisa de um título."));
    }
    let id = new_id();
    let connection = database.write()?;
    workspace_repository::insert_timeline_event(
        &connection,
        &id,
        universe_id,
        &event,
        &now_timestamp(),
    )?;
    Ok(id)
}

pub fn rename_timeline_event(
    database: &SqliteDatabase,
    id: &str,
    title: &str,
) -> DatabaseCommandResult<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(DatabaseCommandError::validation("O evento precisa de um título."));
    }
    let connection = database.write()?;
    if !workspace_repository::rename_timeline_event(&connection, id, title, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Evento não encontrado."));
    }
    Ok(())
}

pub fn delete_timeline_event(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !workspace_repository::delete_timeline_event(&connection, id)? {
        return Err(DatabaseCommandError::not_found("Evento não encontrado."));
    }
    Ok(())
}
