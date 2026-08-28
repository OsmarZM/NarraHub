use crate::database::error::DatabaseCommandResult;
use crate::domain::universe::{Universe, UniverseStats, UniverseWithStats};
use crate::infrastructure::sqlite::{universe_repository, SqliteDatabase};

pub fn list_with_stats(database: &SqliteDatabase) -> DatabaseCommandResult<Vec<UniverseWithStats>> {
    let connection = database.read()?;
    universe_repository::list_with_stats(&connection)
}

pub fn get(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<Option<Universe>> {
    let connection = database.read()?;
    universe_repository::get(&connection, id)
}

pub fn stats(database: &SqliteDatabase, universe_id: &str) -> DatabaseCommandResult<UniverseStats> {
    let connection = database.read()?;
    universe_repository::stats(&connection, universe_id)
}
