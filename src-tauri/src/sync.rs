use rusqlite::{params, params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const TABLES: &[&str] = &[
    "universes",
    "stories",
    "books",
    "chapters",
    "entities",
    "entity_attributes",
    "entity_templates",
    "relations",
    "mentions",
    "timeline_events",
    "planning_items",
];

#[derive(Default)]
pub struct SyncState {
    running: bool,
    address: Option<String>,
    pairing_code: Option<String>,
    device_name: String,
    shutdown: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncServerStatus {
    running: bool,
    address: Option<String>,
    pairing_code: Option<String>,
    device_name: String,
}

#[derive(Debug, Serialize)]
pub struct SyncResult {
    received: usize,
    sent: usize,
    conflicts: usize,
    peer_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TableSnapshot {
    name: String,
    rows: Vec<BTreeMap<String, JsonValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    tables: Vec<TableSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SyncRequest {
    code: String,
    device_name: String,
    snapshot: Snapshot,
}

#[derive(Debug, Serialize, Deserialize)]
struct SyncResponse {
    ok: bool,
    peer_name: String,
    snapshot: Option<Snapshot>,
    received: usize,
    conflicts: usize,
    error: Option<String>,
}

#[tauri::command]
pub fn sync_status(state: State<'_, Mutex<SyncState>>) -> Result<SyncServerStatus, String> {
    let state = state
        .lock()
        .map_err(|_| "Estado de sincronização indisponível".to_string())?;
    Ok(status_from(&state))
}

#[tauri::command]
pub fn sync_start(
    app: AppHandle,
    state: State<'_, Mutex<SyncState>>,
    device_name: String,
) -> Result<SyncServerStatus, String> {
    let mut state = state
        .lock()
        .map_err(|_| "Estado de sincronização indisponível".to_string())?;
    if state.running {
        return Ok(status_from(&state));
    }

    let listener = TcpListener::bind("0.0.0.0:0")
        .map_err(|error| format!("Não foi possível abrir a porta local: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let local_ip = local_ip_address();
    let code = format!("{:06}", Uuid::new_v4().as_u128() % 1_000_000);
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = shutdown.clone();
    let thread_code = code.clone();
    let thread_name = sanitize_device_name(&device_name);
    let db_path = database_path(&app)?;

    std::thread::spawn(move || {
        while !thread_shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = handle_connection(stream, &db_path, &thread_code, &thread_name);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(_) => break,
            }
        }
    });

    state.running = true;
    state.address = Some(format!("{local_ip}:{port}"));
    state.pairing_code = Some(code);
    state.device_name = sanitize_device_name(&device_name);
    state.shutdown = Some(shutdown);
    Ok(status_from(&state))
}

#[tauri::command]
pub fn sync_stop(state: State<'_, Mutex<SyncState>>) -> Result<SyncServerStatus, String> {
    let mut state = state
        .lock()
        .map_err(|_| "Estado de sincronização indisponível".to_string())?;
    if let Some(shutdown) = state.shutdown.take() {
        shutdown.store(true, Ordering::Relaxed);
    }
    state.running = false;
    state.address = None;
    state.pairing_code = None;
    Ok(status_from(&state))
}

#[tauri::command]
pub async fn sync_connect(
    app: AppHandle,
    address: String,
    code: String,
    device_name: String,
) -> Result<SyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = database_path(&app)?;
        let local_snapshot = export_snapshot(&db_path)?;
        let sent = row_count(&local_snapshot);
        let socket = resolve_address(&address)?;
        let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(8))
            .map_err(|error| format!("Não foi possível conectar a {address}: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| error.to_string())?;

        let request = SyncRequest {
            code: code.trim().to_string(),
            device_name: sanitize_device_name(&device_name),
            snapshot: local_snapshot,
        };
        let payload = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        stream
            .write_all(&payload)
            .map_err(|error| error.to_string())?;
        stream.write_all(b"\n").map_err(|error| error.to_string())?;
        stream.flush().map_err(|error| error.to_string())?;

        let mut response_line = String::new();
        BufReader::new(stream)
            .read_line(&mut response_line)
            .map_err(|error| error.to_string())?;
        let response: SyncResponse = serde_json::from_str(&response_line)
            .map_err(|error| format!("Resposta de sincronização inválida: {error}"))?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| "Pareamento recusado".to_string()));
        }
        let remote_snapshot = response
            .snapshot
            .ok_or_else(|| "O dispositivo não enviou dados".to_string())?;
        let received = row_count(&remote_snapshot);
        let (_, local_conflicts) = merge_snapshot(&db_path, &remote_snapshot)?;
        Ok(SyncResult {
            received,
            sent,
            conflicts: response.conflicts + local_conflicts,
            peer_name: response.peer_name,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

fn handle_connection(
    mut stream: TcpStream,
    db_path: &Path,
    code: &str,
    device_name: &str,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let mut line = String::new();
    BufReader::new(stream.try_clone().map_err(|error| error.to_string())?)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let request: SyncRequest = serde_json::from_str(&line).map_err(|error| error.to_string())?;

    let response = if request.code != code {
        SyncResponse {
            ok: false,
            peer_name: device_name.to_string(),
            snapshot: None,
            received: 0,
            conflicts: 0,
            error: Some("Código de pareamento inválido".to_string()),
        }
    } else {
        let (received, conflicts) = merge_snapshot(db_path, &request.snapshot)?;
        let snapshot = export_snapshot(db_path)?;
        SyncResponse {
            ok: true,
            peer_name: device_name.to_string(),
            snapshot: Some(snapshot),
            received,
            conflicts,
            error: None,
        }
    };

    let payload = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
    stream
        .write_all(&payload)
        .map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok(())
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("narrahub.db"))
}

fn export_snapshot(path: &Path) -> Result<Snapshot, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    let mut tables = Vec::new();
    for &table in TABLES {
        let mut statement = connection
            .prepare(&format!("SELECT * FROM {table}"))
            .map_err(|error| error.to_string())?;
        let columns: Vec<String> = statement
            .column_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        let rows = statement
            .query_map([], |row| {
                let mut values = BTreeMap::new();
                for (index, column) in columns.iter().enumerate() {
                    let value = match row.get_ref(index)? {
                        rusqlite::types::ValueRef::Null => JsonValue::Null,
                        rusqlite::types::ValueRef::Integer(value) => JsonValue::from(value),
                        rusqlite::types::ValueRef::Real(value) => JsonValue::from(value),
                        rusqlite::types::ValueRef::Text(value) => {
                            JsonValue::String(String::from_utf8_lossy(value).to_string())
                        }
                        rusqlite::types::ValueRef::Blob(value) => {
                            JsonValue::String(hex_encode(value))
                        }
                    };
                    values.insert(column.clone(), value);
                }
                Ok(values)
            })
            .map_err(|error| error.to_string())?;
        let rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        tables.push(TableSnapshot {
            name: table.to_string(),
            rows,
        });
    }
    Ok(Snapshot { tables })
}

fn merge_snapshot(path: &Path, snapshot: &Snapshot) -> Result<(usize, usize), String> {
    let mut connection = Connection::open(path).map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let mut applied = 0;
    let mut conflicts = 0;

    for table in &snapshot.tables {
        if !TABLES.contains(&table.name.as_str()) {
            continue;
        }
        for row in &table.rows {
            if row.is_empty() || !row.contains_key("id") {
                continue;
            }
            if table.name == "chapters" && chapter_conflicts(&transaction, row)? {
                record_chapter_conflict(&transaction, row)?;
                conflicts += 1;
                continue;
            }
            let columns: Vec<&String> = row.keys().collect();
            let placeholders = (1..=columns.len())
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>()
                .join(",");
            let updates = columns
                .iter()
                .filter(|column| column.as_str() != "id")
                .map(|column| format!("{column}=excluded.{column}"))
                .collect::<Vec<_>>()
                .join(",");
            let has_updated_at = row.contains_key("updated_at");
            let condition = if has_updated_at {
                format!(" WHERE excluded.updated_at > {}.updated_at", table.name)
            } else {
                String::new()
            };
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {}{}",
                table.name,
                columns
                    .iter()
                    .map(|column| column.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                placeholders,
                updates,
                condition,
            );
            let values = columns
                .iter()
                .map(|column| json_to_sql(&row[*column]))
                .collect::<Vec<_>>();
            applied += transaction
                .execute(&sql, params_from_iter(values))
                .map_err(|error| format!("Falha em {}: {error}", table.name))?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok((applied, conflicts))
}

fn chapter_conflicts(
    connection: &Connection,
    remote: &BTreeMap<String, JsonValue>,
) -> Result<bool, String> {
    let id = remote
        .get("id")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let updated_at = remote
        .get("updated_at")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let content = remote
        .get("content")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let local = connection.query_row(
        "SELECT updated_at, content FROM chapters WHERE id = ?1",
        [id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );
    match local {
        Ok((local_updated, local_content)) => {
            Ok(local_updated == updated_at && local_content != content)
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn record_chapter_conflict(
    connection: &Connection,
    remote: &BTreeMap<String, JsonValue>,
) -> Result<(), String> {
    let id = remote
        .get("id")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let remote_content = remote
        .get("content")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let local_content: String = connection
        .query_row("SELECT content FROM chapters WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    connection.execute(
        "INSERT INTO sync_conflicts (id, aggregate_type, aggregate_id, field, local_value, remote_value, created_at)
         VALUES (?1, 'chapter', ?2, 'content', ?3, ?4, datetime('now'))",
        params![Uuid::new_v4().to_string(), id, local_content, remote_content],
    ).map_err(|error| error.to_string())?;
    Ok(())
}

fn json_to_sql(value: &JsonValue) -> rusqlite::types::Value {
    match value {
        JsonValue::Null => rusqlite::types::Value::Null,
        JsonValue::Bool(value) => rusqlite::types::Value::Integer(i64::from(*value)),
        JsonValue::Number(value) if value.is_i64() => {
            rusqlite::types::Value::Integer(value.as_i64().unwrap_or_default())
        }
        JsonValue::Number(value) => {
            rusqlite::types::Value::Real(value.as_f64().unwrap_or_default())
        }
        JsonValue::String(value) => rusqlite::types::Value::Text(value.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

fn row_count(snapshot: &Snapshot) -> usize {
    snapshot.tables.iter().map(|table| table.rows.len()).sum()
}

fn resolve_address(address: &str) -> Result<SocketAddr, String> {
    address
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| "Endereço inválido".to_string())
}

fn local_ip_address() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            socket.local_addr()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn status_from(state: &SyncState) -> SyncServerStatus {
    SyncServerStatus {
        running: state.running,
        address: state.address.clone(),
        pairing_code: state.pairing_code.clone(),
        device_name: state.device_name.clone(),
    }
}

fn sanitize_device_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "NarraHub".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::{MIGRATION_V1, MIGRATION_V2};

    fn temporary_database(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("narrahub-{label}-{}.db", Uuid::new_v4()));
        let connection = Connection::open(&path).expect("create temp db");
        connection
            .execute_batch(MIGRATION_V1)
            .expect("migration v1");
        connection
            .execute_batch(MIGRATION_V2)
            .expect("migration v2");
        path
    }

    fn seed_universe(path: &Path, id: &str, name: &str, updated_at: &str) {
        let connection = Connection::open(path).expect("open temp db");
        connection.execute(
            "INSERT INTO universes (id, name, description, cover_image, created_at, updated_at) VALUES (?1, ?2, '', '', ?3, ?3)",
            params![id, name, updated_at],
        ).expect("seed universe");
    }

    #[test]
    fn snapshot_round_trip_copies_real_rows() {
        let source = temporary_database("source");
        let target = temporary_database("target");
        seed_universe(
            &source,
            "universe-1",
            "Cidade sem Sol",
            "2026-08-20 12:00:00",
        );

        let snapshot = export_snapshot(&source).expect("export snapshot");
        let (applied, conflicts) = merge_snapshot(&target, &snapshot).expect("merge snapshot");
        let target_connection = Connection::open(&target).expect("open target");
        let name: String = target_connection
            .query_row(
                "SELECT name FROM universes WHERE id = 'universe-1'",
                [],
                |row| row.get(0),
            )
            .expect("read copied universe");

        assert!(applied >= 1);
        assert_eq!(conflicts, 0);
        assert_eq!(name, "Cidade sem Sol");
        std::fs::remove_file(source).ok();
        std::fs::remove_file(target).ok();
    }

    #[test]
    fn older_snapshot_does_not_replace_newer_content() {
        let source = temporary_database("older");
        let target = temporary_database("newer");
        seed_universe(&source, "universe-1", "Nome antigo", "2026-08-20 10:00:00");
        seed_universe(&target, "universe-1", "Nome novo", "2026-08-20 12:00:00");

        merge_snapshot(&target, &export_snapshot(&source).expect("snapshot")).expect("merge");
        let connection = Connection::open(&target).expect("open target");
        let name: String = connection
            .query_row(
                "SELECT name FROM universes WHERE id = 'universe-1'",
                [],
                |row| row.get(0),
            )
            .expect("read universe");

        assert_eq!(name, "Nome novo");
        std::fs::remove_file(source).ok();
        std::fs::remove_file(target).ok();
    }
}
