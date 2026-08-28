use crate::database::error::DatabaseCommandResult;
use crate::domain::collaboration::{
    CollaborationContribution, CollaborationSession, NewCollaborationSession,
};
use rusqlite::{Connection, Row, Transaction};

use super::connection::map_sqlite_error;

const CONTRIBUTION_COLUMNS: &str = "id, session_id, sequence, contributor, kind, universe_id, \
     target_type, target_id, target_label, field, original_value, proposed_value, message, \
     status, created_at, reviewed_at";

fn contribution_from_row(row: &Row<'_>) -> rusqlite::Result<CollaborationContribution> {
    Ok(CollaborationContribution {
        id: row.get("id")?,
        session_id: row.get("session_id")?,
        sequence: row.get("sequence")?,
        contributor: row.get("contributor")?,
        kind: row.get("kind")?,
        universe_id: row.get("universe_id")?,
        target_type: row.get("target_type")?,
        target_id: row.get("target_id")?,
        target_label: row.get("target_label")?,
        field: row.get("field")?,
        original_value: row.get("original_value")?,
        proposed_value: row.get("proposed_value")?,
        message: row.get("message")?,
        status: row.get("status")?,
        created_at: row.get("created_at")?,
        reviewed_at: row.get("reviewed_at")?,
    })
}

pub fn list_sessions(connection: &Connection) -> DatabaseCommandResult<Vec<CollaborationSession>> {
    let mut statement = connection
        .prepare(
            "SELECT s.id, s.title, s.permission, s.universe_ids, s.encryption_key, s.revoke_token,
                    s.status, s.created_at, s.expires_at, s.ended_at,
                    COALESCE(SUM(CASE WHEN c.kind = 'edit' AND c.status = 'pending' THEN 1 ELSE 0 END), 0) AS pending_count,
                    COALESCE(SUM(CASE WHEN c.kind = 'note' THEN 1 ELSE 0 END), 0) AS note_count
               FROM collaboration_sessions s
               LEFT JOIN collaboration_contributions c ON c.session_id = s.id
              GROUP BY s.id
              ORDER BY s.created_at DESC",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(CollaborationSession {
                id: row.get("id")?,
                title: row.get("title")?,
                permission: row.get("permission")?,
                universe_ids: row.get("universe_ids")?,
                encryption_key: row.get("encryption_key")?,
                revoke_token: row.get("revoke_token")?,
                status: row.get("status")?,
                created_at: row.get("created_at")?,
                expires_at: row.get("expires_at")?,
                ended_at: row.get("ended_at")?,
                pending_count: row.get("pending_count")?,
                note_count: row.get("note_count")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// Contribuições ordenadas pela urgência da revisão: pendentes primeiro,
/// depois recados, e por último o que já foi decidido.
pub fn list_contributions(
    connection: &Connection,
    session_id: Option<&str>,
) -> DatabaseCommandResult<Vec<CollaborationContribution>> {
    let order =
        "ORDER BY CASE status WHEN 'pending' THEN 0 WHEN 'noted' THEN 1 ELSE 2 END, sequence DESC";
    let sql = match session_id {
        Some(_) => format!(
            "SELECT {CONTRIBUTION_COLUMNS} FROM collaboration_contributions WHERE session_id = ?1 {order}"
        ),
        None => format!("SELECT {CONTRIBUTION_COLUMNS} FROM collaboration_contributions {order}"),
    };
    let parameters: Vec<&dyn rusqlite::ToSql> = match session_id.as_ref() {
        Some(id) => vec![id],
        None => Vec::new(),
    };
    let mut statement = connection.prepare(&sql).map_err(map_sqlite_error)?;
    let rows = statement
        .query_map(parameters.as_slice(), contribution_from_row)
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// Grava a sessão, reativando-a se o mesmo id voltar.
///
/// O `ON CONFLICT` limpa `ended_at` de propósito: reabrir um link encerrado é
/// operação normal, e deixar a data antiga faria a tela mostrar uma sessão
/// ativa marcada como terminada.
pub fn upsert_session(
    connection: &Connection,
    session: &NewCollaborationSession,
    universe_ids_json: &str,
    created_at: &str,
) -> DatabaseCommandResult<()> {
    let (id, title, permission) = (&session.id, &session.title, &session.permission);
    let (encryption_key, revoke_token) = (&session.encryption_key, &session.revoke_token);
    let expires_at = &session.expires_at;
    connection
        .execute(
            "INSERT INTO collaboration_sessions
               (id, title, permission, universe_ids, encryption_key, revoke_token,
                status, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               permission = excluded.permission,
               universe_ids = excluded.universe_ids,
               encryption_key = excluded.encryption_key,
               revoke_token = excluded.revoke_token,
               status = 'active',
               expires_at = excluded.expires_at,
               ended_at = NULL",
            rusqlite::params![
                id,
                title,
                permission,
                universe_ids_json,
                encryption_key,
                revoke_token,
                created_at,
                expires_at
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

/// `OR IGNORE` porque a mesma contribuição pode chegar duas vezes pela rede:
/// o `UNIQUE(session_id, sequence)` do schema torna o reenvio inofensivo.
/// Devolve se algo realmente entrou, que é o que diz ao chamador se houve
/// novidade para avisar na tela.
pub fn insert_contribution(
    connection: &Connection,
    contribution: &CollaborationContribution,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "INSERT OR IGNORE INTO collaboration_contributions
               (id, session_id, sequence, contributor, kind, universe_id, target_type, target_id,
                target_label, field, original_value, proposed_value, message, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                contribution.id,
                contribution.session_id,
                contribution.sequence,
                contribution.contributor,
                contribution.kind,
                contribution.universe_id,
                contribution.target_type,
                contribution.target_id,
                contribution.target_label,
                contribution.field,
                contribution.original_value,
                contribution.proposed_value,
                contribution.message,
                contribution.status,
                contribution.created_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn end_all_active(
    connection: &Connection,
    status: &str,
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "UPDATE collaboration_sessions SET status = ?1, ended_at = ?2 WHERE status = 'active'",
            [status, timestamp],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn end_session(
    connection: &Connection,
    id: &str,
    status: &str,
    timestamp: &str,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "UPDATE collaboration_sessions SET status = ?1, ended_at = ?2 WHERE id = ?3",
            [status, timestamp, id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

/// Uma proposta de edição ainda pendente, dentro da transação da revisão.
///
/// A leitura acontece **dentro** da transação de propósito: ela é o que impede
/// a mesma proposta de ser aplicada duas vezes se dois cliques chegarem juntos.
pub fn pending_edit(
    transaction: &Transaction<'_>,
    id: &str,
) -> DatabaseCommandResult<Option<CollaborationContribution>> {
    let mut statement = transaction
        .prepare(&format!(
            "SELECT {CONTRIBUTION_COLUMNS} FROM collaboration_contributions
              WHERE id = ?1 AND kind = 'edit' AND status = 'pending'"
        ))
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query_map([id], contribution_from_row)
        .map_err(map_sqlite_error)?;
    match rows.next() {
        Some(row) => Ok(Some(row.map_err(map_sqlite_error)?)),
        None => Ok(None),
    }
}

pub fn pending_edits_of_session(
    connection: &Connection,
    session_id: &str,
) -> DatabaseCommandResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM collaboration_contributions
              WHERE session_id = ?1 AND kind = 'edit' AND status = 'pending'
              ORDER BY sequence",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([session_id], |row| row.get::<_, String>("id"))
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn mark_reviewed(
    transaction: &Transaction<'_>,
    id: &str,
    decision: &str,
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    transaction
        .execute(
            "UPDATE collaboration_contributions SET status = ?1, reviewed_at = ?2 WHERE id = ?3",
            [decision, timestamp, id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

/// Escreve o valor proposto numa coluna que **veio da lista de permitidos**,
/// nunca do texto recebido. Ver `domain::collaboration::writable_column`.
pub fn apply_column_change(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
    value: &str,
    target_id: &str,
    timestamp: &str,
) -> DatabaseCommandResult<bool> {
    let affected = transaction
        .execute(
            &format!("UPDATE {table} SET {column} = ?1, updated_at = ?2 WHERE id = ?3"),
            [value, timestamp, target_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn apply_attribute_change(
    transaction: &Transaction<'_>,
    new_id: &str,
    entity_id: &str,
    key: &str,
    value: &str,
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    // `COLLATE NOCASE` para "Idade" e "idade" serem o mesmo atributo — é o que
    // o caminho antigo fazia, e mudar isso criaria linha duplicada na ficha.
    let affected = transaction
        .execute(
            "UPDATE entity_attributes SET value = ?1
              WHERE entity_id = ?2 AND key = ?3 COLLATE NOCASE",
            [value, entity_id, key],
        )
        .map_err(map_sqlite_error)?;
    if affected == 0 {
        transaction
            .execute(
                "INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
                 VALUES (?1, ?2, ?3, ?4,
                         (SELECT COALESCE(MAX(sort_order), -1) + 1
                            FROM entity_attributes WHERE entity_id = ?2))",
                [new_id, entity_id, key, value],
            )
            .map_err(map_sqlite_error)?;
    }
    transaction
        .execute(
            "UPDATE entities SET updated_at = ?1 WHERE id = ?2",
            [timestamp, entity_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn log_applied_change(
    transaction: &Transaction<'_>,
    id: &str,
    contribution: &CollaborationContribution,
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    transaction
        .execute(
            "INSERT INTO change_log
               (id, universe_id, entity_type, entity_id, action, field, old_value, new_value, created_at)
             VALUES (?1, ?2, ?3, ?4, 'update', ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                contribution.universe_id,
                contribution.target_type,
                contribution.target_id,
                contribution.field,
                contribution.original_value,
                contribution.proposed_value,
                timestamp,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}
