use crate::database::error::DatabaseCommandResult;
use crate::domain::workspace::{HistoryEntry, RelationCard, TimelineEvent};
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
