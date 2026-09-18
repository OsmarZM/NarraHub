use crate::application::mutacao::Mutacao;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
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
    identidade: &DeviceIdentity,
    universe_id: &str,
    event: NewTimelineEvent,
) -> DatabaseCommandResult<String> {
    if event.title.trim().is_empty() {
        return Err(DatabaseCommandError::validation(
            "O evento precisa de um título.",
        ));
    }
    let id = new_id();
    Mutacao::executar(database, identidade, |m| {
        workspace_repository::insert_timeline_event(
            m.tx(),
            &id,
            universe_id,
            &event,
            &now_timestamp(),
        )?;
        m.gravou("timeline_event", &id)
    })?;
    Ok(id)
}

pub fn rename_timeline_event(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
    title: &str,
) -> DatabaseCommandResult<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O evento precisa de um título.",
        ));
    }
    Mutacao::executar(database, identidade, |m| {
        if !workspace_repository::rename_timeline_event(m.tx(), id, title, &now_timestamp())? {
            return Err(DatabaseCommandError::not_found("Evento não encontrado."));
        }
        m.gravou("timeline_event", id)
    })
}

pub fn delete_timeline_event(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        m.excluir("timeline_event", id).map_err(|erro| {
            if erro.kind == crate::database::error::DatabaseErrorKind::NotFound {
                DatabaseCommandError::not_found("Evento não encontrado.")
            } else {
                erro
            }
        })?;
        if !workspace_repository::delete_timeline_event(m.tx(), id)? {
            return Err(DatabaseCommandError::not_found("Evento não encontrado."));
        }
        Ok(())
    })
}

pub fn create_relation(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    universe_id: &str,
    source_id: &str,
    target_id: &str,
    label: &str,
) -> DatabaseCommandResult<String> {
    if source_id == target_id {
        return Err(DatabaseCommandError::validation(
            "Uma entidade não se relaciona com ela mesma.",
        ));
    }
    let id = new_id();
    Mutacao::executar(database, identidade, |m| {
        workspace_repository::insert_relation(
            m.tx(),
            &id,
            universe_id,
            source_id,
            target_id,
            label.trim(),
            &now_timestamp(),
        )?;
        m.gravou("relation", &id)
    })?;
    Ok(id)
}

pub fn delete_relation(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        m.excluir("relation", id).map_err(|erro| {
            if erro.kind == crate::database::error::DatabaseErrorKind::NotFound {
                DatabaseCommandError::not_found("Relação não encontrada.")
            } else {
                erro
            }
        })?;
        if !workspace_repository::delete_relation(m.tx(), id)? {
            return Err(DatabaseCommandError::not_found("Relação não encontrada."));
        }
        Ok(())
    })
}
