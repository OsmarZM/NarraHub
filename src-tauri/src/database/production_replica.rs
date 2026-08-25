use super::backup::{
    create_backup_at, list_backups_at, validate_backup_at, BackupManifest, BackupReason,
    BackupRuntimeState,
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

const DEVELOPMENT_IDENTIFIER: &str = "com.narrahub.app.dev";
const PRODUCTION_IDENTIFIER: &str = "com.narrahub.app";
const DATABASE_FILE_NAME: &str = "narrahub.db";
const SNAPSHOT_RETENTION: usize = 10;
const MAX_VISIBLE_CHANGES: usize = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionReplicaStatus {
    pub enabled: bool,
    pub source_exists: bool,
    pub source_modified_at: Option<String>,
    pub snapshot_id: Option<String>,
    pub captured_at: Option<String>,
    pub previous_snapshot_id: Option<String>,
    pub schema_version: Option<i64>,
    pub counts: ReplicaCounts,
    pub changes: ReplicaChanges,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaCounts {
    pub universes: usize,
    pub stories: usize,
    pub books: usize,
    pub chapters: usize,
    pub entities: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaChanges {
    pub added_count: usize,
    pub removed_count: usize,
    pub added: Vec<ReplicaChange>,
    pub removed: Vec<ReplicaChange>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaChange {
    pub kind: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionReplicaCatalog {
    pub snapshot_id: String,
    pub captured_at: String,
    pub universes: Vec<ReplicaCatalogItem>,
    pub stories: Vec<ReplicaCatalogItem>,
    pub books: Vec<ReplicaCatalogItem>,
    pub chapters: Vec<ReplicaCatalogItem>,
    pub entities: Vec<ReplicaCatalogItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaCatalogItem {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub detail: String,
    pub word_count: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionReplicaChapter {
    pub id: String,
    pub title: String,
    pub content: String,
    pub word_count: u64,
    pub updated_at: String,
    pub book_name: String,
    pub story_name: String,
    pub universe_name: String,
    pub snapshot_id: String,
    pub captured_at: String,
}

struct ReplicaPaths {
    source_database: PathBuf,
    snapshots_root: PathBuf,
}

#[tauri::command]
pub fn production_replica_status(app: AppHandle) -> Result<ProductionReplicaStatus, String> {
    let Some(paths) = replica_paths(&app)? else {
        return Ok(disabled_status());
    };
    status_at(&paths.source_database, &paths.snapshots_root, true)
}

#[tauri::command]
pub async fn production_replica_refresh(
    app: AppHandle,
    state: State<'_, BackupRuntimeState>,
) -> Result<ProductionReplicaStatus, String> {
    let paths = replica_paths(&app)?.ok_or_else(|| {
        "A réplica de produção só existe no perfil de desenvolvimento.".to_string()
    })?;
    if state.running.swap(true, Ordering::AcqRel) {
        return Err("Já existe uma operação de backup ou recuperação em andamento.".into());
    }
    let task = tauri::async_runtime::spawn_blocking(move || {
        refresh_at(
            &paths.source_database,
            &paths.snapshots_root,
            env!("CARGO_PKG_VERSION"),
        )
    })
    .await;
    state.running.store(false, Ordering::Release);
    task.map_err(|error| format!("A atualização da réplica falhou: {error}"))?
}

#[tauri::command]
pub fn production_replica_catalog(app: AppHandle) -> Result<ProductionReplicaCatalog, String> {
    let paths = replica_paths(&app)?.ok_or_else(|| {
        "A réplica de produção só existe no perfil de desenvolvimento.".to_string()
    })?;
    load_latest_catalog(&paths.snapshots_root)
}

#[tauri::command]
pub fn production_replica_chapter(
    app: AppHandle,
    chapter_id: String,
) -> Result<ProductionReplicaChapter, String> {
    if chapter_id.is_empty() || chapter_id.len() > 100 {
        return Err("Identificador de capítulo inválido.".into());
    }
    let paths = replica_paths(&app)?.ok_or_else(|| {
        "A réplica de produção só existe no perfil de desenvolvimento.".to_string()
    })?;
    load_latest_chapter(&paths.snapshots_root, &chapter_id)
}

fn replica_paths(app: &AppHandle) -> Result<Option<ReplicaPaths>, String> {
    if app.config().identifier != DEVELOPMENT_IDENTIFIER {
        return Ok(None);
    }
    let development_data = super::app_data_path(app)?;
    let parent = development_data
        .parent()
        .ok_or_else(|| "Não foi possível localizar a raiz dos perfis NarraHub.".to_string())?;
    Ok(Some(ReplicaPaths {
        source_database: parent.join(PRODUCTION_IDENTIFIER).join(DATABASE_FILE_NAME),
        snapshots_root: development_data.join("production-replicas"),
    }))
}

fn refresh_at(
    source_database: &Path,
    snapshots_root: &Path,
    app_version: &str,
) -> Result<ProductionReplicaStatus, String> {
    validate_source_path(source_database)?;
    fs::create_dir_all(snapshots_root).map_err(|error| error.to_string())?;
    let previous = list_backups_at(snapshots_root)?;
    let created = create_backup_at(
        source_database,
        None,
        snapshots_root,
        app_version,
        BackupReason::Manual,
    )?;

    if let Some(existing) = previous
        .iter()
        .find(|manifest| manifest.database.sha256 == created.database.sha256)
    {
        remove_generated_snapshot(snapshots_root, &created.backup_id)?;
        retain_generated_snapshots(snapshots_root)?;
        return status_at(source_database, snapshots_root, true).map(|mut status| {
            status.snapshot_id = Some(existing.backup_id.clone());
            status
        });
    }

    retain_generated_snapshots(snapshots_root)?;
    status_at(source_database, snapshots_root, true)
}

fn validate_source_path(source_database: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source_database).map_err(|_| {
        "O banco instalado de produção não foi encontrado neste computador.".to_string()
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("A origem da réplica não é um banco local regular permitido.".into());
    }
    if source_database.file_name().and_then(|name| name.to_str()) != Some(DATABASE_FILE_NAME)
        || source_database
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some(PRODUCTION_IDENTIFIER)
    {
        return Err("A origem não corresponde ao perfil local de produção do NarraHub.".into());
    }
    Ok(())
}

fn status_at(
    source_database: &Path,
    snapshots_root: &Path,
    enabled: bool,
) -> Result<ProductionReplicaStatus, String> {
    let source_exists = source_database.is_file();
    let source_modified_at = source_exists
        .then(|| fs::metadata(source_database).and_then(|metadata| metadata.modified()))
        .transpose()
        .map_err(|error| error.to_string())?
        .map(system_time_to_rfc3339);
    let manifests = list_backups_at(snapshots_root)?;
    let Some(latest) = manifests.first() else {
        return Ok(ProductionReplicaStatus {
            enabled,
            source_exists,
            source_modified_at,
            snapshot_id: None,
            captured_at: None,
            previous_snapshot_id: None,
            schema_version: None,
            counts: ReplicaCounts::default(),
            changes: ReplicaChanges::default(),
        });
    };
    ensure_snapshot_valid(snapshots_root, latest)?;
    let current_catalog = load_catalog(&snapshot_database(snapshots_root, latest))?;
    let previous = manifests.get(1);
    let changes = if let Some(previous) = previous {
        ensure_snapshot_valid(snapshots_root, previous)?;
        let previous_catalog = load_catalog(&snapshot_database(snapshots_root, previous))?;
        compare_catalogs(&current_catalog, &previous_catalog)
    } else {
        ReplicaChanges::default()
    };

    Ok(ProductionReplicaStatus {
        enabled,
        source_exists,
        source_modified_at,
        snapshot_id: Some(latest.backup_id.clone()),
        captured_at: Some(latest.created_at.clone()),
        previous_snapshot_id: previous.map(|manifest| manifest.backup_id.clone()),
        schema_version: Some(latest.schema_version),
        counts: catalog_counts(&current_catalog),
        changes,
    })
}

fn disabled_status() -> ProductionReplicaStatus {
    ProductionReplicaStatus {
        enabled: false,
        source_exists: false,
        source_modified_at: None,
        snapshot_id: None,
        captured_at: None,
        previous_snapshot_id: None,
        schema_version: None,
        counts: ReplicaCounts::default(),
        changes: ReplicaChanges::default(),
    }
}

fn load_latest_catalog(snapshots_root: &Path) -> Result<ProductionReplicaCatalog, String> {
    let manifests = list_backups_at(snapshots_root)?;
    let latest = manifests
        .first()
        .ok_or_else(|| "Nenhuma réplica de produção foi criada ainda.".to_string())?;
    ensure_snapshot_valid(snapshots_root, latest)?;
    let mut catalog = load_catalog(&snapshot_database(snapshots_root, latest))?;
    catalog.snapshot_id = latest.backup_id.clone();
    catalog.captured_at = latest.created_at.clone();
    Ok(catalog)
}

fn load_latest_chapter(
    snapshots_root: &Path,
    chapter_id: &str,
) -> Result<ProductionReplicaChapter, String> {
    let manifests = list_backups_at(snapshots_root)?;
    let latest = manifests
        .first()
        .ok_or_else(|| "Nenhuma réplica de produção foi criada ainda.".to_string())?;
    ensure_snapshot_valid(snapshots_root, latest)?;
    let connection = open_read_only(&snapshot_database(snapshots_root, latest))?;
    connection
        .query_row(
            "SELECT c.id, c.title, c.content, c.word_count, c.updated_at,
                    b.name, s.name, u.name
             FROM chapters c
             JOIN books b ON b.id = c.book_id
             JOIN stories s ON s.id = b.story_id
             JOIN universes u ON u.id = s.universe_id
             WHERE c.id = ?1",
            [chapter_id],
            |row| {
                Ok(ProductionReplicaChapter {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    word_count: row.get::<_, i64>(3)?.max(0) as u64,
                    updated_at: row.get(4)?,
                    book_name: row.get(5)?,
                    story_name: row.get(6)?,
                    universe_name: row.get(7)?,
                    snapshot_id: latest.backup_id.clone(),
                    captured_at: latest.created_at.clone(),
                })
            },
        )
        .map_err(|error| format!("Capítulo não encontrado na réplica: {error}"))
}

fn load_catalog(database_path: &Path) -> Result<ProductionReplicaCatalog, String> {
    let connection = open_read_only(database_path)?;
    Ok(ProductionReplicaCatalog {
        snapshot_id: String::new(),
        captured_at: String::new(),
        universes: load_catalog_items(
            &connection,
            "SELECT id, NULL, name, description, 0, updated_at FROM universes ORDER BY name",
        )?,
        stories: load_catalog_items(
            &connection,
            "SELECT id, universe_id, name, description, 0, updated_at FROM stories ORDER BY sort_order, name",
        )?,
        books: load_catalog_items(
            &connection,
            "SELECT id, story_id, name, description, 0, updated_at FROM books ORDER BY sort_order, name",
        )?,
        chapters: load_catalog_items(
            &connection,
            "SELECT id, book_id, title, status, word_count, updated_at FROM chapters ORDER BY sort_order, title",
        )?,
        entities: load_catalog_items(
            &connection,
            "SELECT id, universe_id, name, type, 0, updated_at FROM entities ORDER BY type, name",
        )?,
    })
}

fn load_catalog_items(
    connection: &Connection,
    sql: &str,
) -> Result<Vec<ReplicaCatalogItem>, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let items = statement
        .query_map([], |row| {
            Ok(ReplicaCatalogItem {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                detail: row.get(3)?,
                word_count: row.get::<_, i64>(4)?.max(0) as u64,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(items)
}

fn compare_catalogs(
    current: &ProductionReplicaCatalog,
    previous: &ProductionReplicaCatalog,
) -> ReplicaChanges {
    let current_items = catalog_identities(current);
    let previous_items = catalog_identities(previous);
    let added_all: Vec<_> = current_items.difference(&previous_items).cloned().collect();
    let removed_all: Vec<_> = previous_items.difference(&current_items).cloned().collect();
    ReplicaChanges {
        added_count: added_all.len(),
        removed_count: removed_all.len(),
        added: added_all.into_iter().take(MAX_VISIBLE_CHANGES).collect(),
        removed: removed_all.into_iter().take(MAX_VISIBLE_CHANGES).collect(),
    }
}

fn catalog_identities(catalog: &ProductionReplicaCatalog) -> BTreeSet<ReplicaChange> {
    let mut identities = BTreeSet::new();
    for (kind, items) in [
        ("universe", &catalog.universes),
        ("story", &catalog.stories),
        ("book", &catalog.books),
        ("chapter", &catalog.chapters),
        ("entity", &catalog.entities),
    ] {
        identities.extend(items.iter().map(|item| ReplicaChange {
            kind: kind.into(),
            id: item.id.clone(),
            name: item.name.clone(),
        }));
    }
    identities
}

fn catalog_counts(catalog: &ProductionReplicaCatalog) -> ReplicaCounts {
    ReplicaCounts {
        universes: catalog.universes.len(),
        stories: catalog.stories.len(),
        books: catalog.books.len(),
        chapters: catalog.chapters.len(),
        entities: catalog.entities.len(),
    }
}

fn ensure_snapshot_valid(root: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let validation = validate_backup_at(root, &manifest.backup_id)?;
    if validation.valid {
        Ok(())
    } else {
        Err(format!(
            "A réplica {} falhou na validação: {}",
            manifest.backup_id,
            validation.errors.join(" ")
        ))
    }
}

fn retain_generated_snapshots(root: &Path) -> Result<(), String> {
    let manifests = list_backups_at(root)?;
    for manifest in manifests.into_iter().skip(SNAPSHOT_RETENTION) {
        ensure_snapshot_valid(root, &manifest)?;
        remove_generated_snapshot(root, &manifest.backup_id)?;
    }
    Ok(())
}

fn remove_generated_snapshot(root: &Path, backup_id: &str) -> Result<(), String> {
    if backup_id.is_empty()
        || !backup_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Identificador de réplica inválido.".into());
    }
    let canonical_root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let directory = root.join(backup_id);
    let metadata = fs::symlink_metadata(&directory).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("A retenção recusou uma réplica que não é um diretório regular.".into());
    }
    let canonical_directory = fs::canonicalize(&directory).map_err(|error| error.to_string())?;
    if canonical_directory.parent() != Some(canonical_root.as_path()) {
        return Err("A retenção recusou uma réplica fora da raiz permitida.".into());
    }
    fs::remove_dir_all(canonical_directory).map_err(|error| error.to_string())
}

fn snapshot_database(root: &Path, manifest: &BackupManifest) -> PathBuf {
    root.join(&manifest.backup_id).join(DATABASE_FILE_NAME)
}

fn open_read_only(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Não foi possível abrir a réplica somente para leitura: {error}"))
}

fn system_time_to_rfc3339(value: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::MIGRATION_V1;
    use rusqlite::params;
    use sha2::{Digest, Sha256};
    use uuid::Uuid;

    struct ReplicaTestPaths {
        root: PathBuf,
        source: PathBuf,
        snapshots: PathBuf,
    }

    impl ReplicaTestPaths {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("narrahub-production-replica-{}", Uuid::new_v4()));
            let production = root.join(PRODUCTION_IDENTIFIER);
            fs::create_dir_all(&production).expect("create production profile");
            let source = production.join(DATABASE_FILE_NAME);
            let connection = Connection::open(&source).expect("create production db");
            connection
                .execute_batch(MIGRATION_V1)
                .expect("apply schema");
            connection
                .pragma_update(None, "user_version", 10)
                .expect("set schema");
            connection
                .execute_batch(
                    "PRAGMA foreign_keys = ON;
                 INSERT INTO universes (id, name) VALUES ('u1', 'Produção');
                 INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Saga segura');
                 INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro real');
                 INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('c1', 'b1', 'Capítulo preservado', '<p>Texto canônico</p>', 2);",
                )
                .expect("seed production");
            Self {
                snapshots: root.join("dev/production-replicas"),
                root,
                source,
            }
        }
    }

    impl Drop for ReplicaTestPaths {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    #[test]
    fn refresh_never_changes_source_and_unchanged_content_does_not_duplicate_snapshot() {
        let paths = ReplicaTestPaths::new();
        let before = file_hash(&paths.source);
        let first = refresh_at(&paths.source, &paths.snapshots, "test").expect("first refresh");
        let second = refresh_at(&paths.source, &paths.snapshots, "test").expect("second refresh");

        assert_eq!(before, file_hash(&paths.source));
        assert_eq!(first.snapshot_id, second.snapshot_id);
        assert_eq!(list_backups_at(&paths.snapshots).unwrap().len(), 1);
        assert_eq!(first.counts.stories, 1);
        let chapter = load_latest_chapter(&paths.snapshots, "c1").expect("read chapter");
        assert_eq!(chapter.content, "<p>Texto canônico</p>");
    }

    #[test]
    fn next_snapshot_reports_deleted_story_book_and_chapter() {
        let paths = ReplicaTestPaths::new();
        refresh_at(&paths.source, &paths.snapshots, "test").expect("baseline");
        let connection = Connection::open(&paths.source).expect("open source");
        connection
            .execute("DELETE FROM stories WHERE id = ?1", params!["s1"])
            .expect("simulate production deletion");
        drop(connection);

        let status = refresh_at(&paths.source, &paths.snapshots, "test").expect("changed snapshot");
        assert_eq!(status.counts.stories, 0);
        assert_eq!(status.counts.books, 0);
        assert_eq!(status.counts.chapters, 0);
        assert_eq!(status.changes.removed_count, 3);
        assert!(status
            .changes
            .removed
            .iter()
            .any(|change| change.kind == "story" && change.name == "Saga segura"));
    }

    fn file_hash(path: &Path) -> Vec<u8> {
        Sha256::digest(fs::read(path).expect("read file")).to_vec()
    }
}
