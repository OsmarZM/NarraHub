use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::universe::{Universe, UniverseStats, UniverseUpdate, UniverseWithStats};
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

/// Cria o universo numa gravação só.
///
/// O caminho antigo inseria e depois fazia um `UPDATE` separado quando havia
/// capa — duas idas ao banco, e uma janela em que o universo existia sem capa.
pub fn create(
    database: &SqliteDatabase,
    name: &str,
    description: &str,
    cover_image: &str,
) -> DatabaseCommandResult<Universe> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O universo precisa de um nome.",
        ));
    }
    let timestamp = now_timestamp();
    let universe = Universe {
        id: new_id(),
        name: name.to_string(),
        description: description.to_string(),
        cover_image: cover_image.to_string(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    let connection = database.write()?;
    universe_repository::insert(&connection, &universe)?;
    Ok(universe)
}

pub fn update(
    database: &SqliteDatabase,
    id: &str,
    patch: UniverseUpdate,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    if patch.name.as_deref().is_some_and(|name| name.trim().is_empty()) {
        return Err(DatabaseCommandError::validation(
            "O universo precisa de um nome.",
        ));
    }
    let connection = database.write()?;
    if !universe_repository::update(&connection, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Universo não encontrado."));
    }
    Ok(())
}

/// Exclui o universo e, por tabela em cascata, tudo que pendura nele.
///
/// A cascata só acontece porque esta conexão liga `foreign_keys` — o
/// `tauri-plugin-sql` não liga, então o caminho antigo deixava histórias,
/// entidades e capítulos órfãos no arquivo depois de excluir o universo.
pub fn delete(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !universe_repository::delete(&connection, id)? {
        return Err(DatabaseCommandError::not_found("Universo não encontrado."));
    }
    Ok(())
}
