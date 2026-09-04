use super::error::{DatabaseCommandError, DatabaseCommandResult};
use super::health::{inspect_database, DatabaseHealthReport};
use chrono::Utc;
use rusqlite::{Connection, DatabaseName, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, State};
use uuid::Uuid;

const BACKUP_FORMAT_VERSION: u32 = 1;
const DATABASE_FILE_NAME: &str = "narrahub.db";
const MANIFEST_FILE_NAME: &str = "manifest.json";
const STAGING_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const DEFAULT_AUTOMATIC_RETENTION: usize = 5;

#[derive(Debug, Default)]
pub struct BackupRuntimeState {
    pub(crate) running: AtomicBool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupReason {
    #[default]
    Manual,
    PreMigration,
    PreUpdate,
    PreRestore,
    Periodic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDatabaseManifest {
    pub file: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupAssetEntry {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupAssetsManifest {
    pub count: usize,
    pub total_bytes: u64,
    pub manifest_sha256: String,
    pub files: Vec<BackupAssetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format_version: u32,
    pub backup_id: String,
    pub schema_version: i64,
    pub app_version: String,
    pub created_at: String,
    pub reason: BackupReason,
    pub database: BackupDatabaseManifest,
    pub assets: BackupAssetsManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub manifest: Option<BackupManifest>,
    pub database_health: Option<DatabaseHealthReport>,
}

#[tauri::command]
pub async fn backup_create(
    app: AppHandle,
    state: State<'_, BackupRuntimeState>,
    reason: Option<BackupReason>,
) -> DatabaseCommandResult<BackupManifest> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    if state.running.swap(true, Ordering::AcqRel) {
        return Err(DatabaseCommandError::conflict(
            "Já existe um backup em andamento.",
        ));
    }

    let database_path = app_data.join(DATABASE_FILE_NAME);
    let backups_root = app_data.join("backups");
    let assets_path = app_data.join("assets");
    let task = tauri::async_runtime::spawn_blocking(move || {
        create_backup_at(
            &database_path,
            assets_path.is_dir().then_some(assets_path.as_path()),
            &backups_root,
            env!("CARGO_PKG_VERSION"),
            reason.unwrap_or_default(),
        )
    })
    .await;
    state.running.store(false, Ordering::Release);

    task.map_err(|error| {
        DatabaseCommandError::unavailable(format!("O processo de backup falhou: {error}"))
    })?
    .map_err(DatabaseCommandError::storage)
}

#[tauri::command]
pub fn backup_list(app: AppHandle) -> DatabaseCommandResult<Vec<BackupManifest>> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    list_backups_at(&app_data.join("backups")).map_err(DatabaseCommandError::storage)
}

#[tauri::command]
pub fn backup_validate(
    app: AppHandle,
    backup_id: String,
) -> DatabaseCommandResult<BackupValidation> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    validate_backup_at(&app_data.join("backups"), &backup_id).map_err(|message| {
        if message.contains("inválido") {
            DatabaseCommandError::validation(message)
        } else {
            DatabaseCommandError::storage(message)
        }
    })
}

pub fn create_backup_at(
    database_path: &Path,
    assets_path: Option<&Path>,
    backups_root: &Path,
    app_version: &str,
    reason: BackupReason,
) -> Result<BackupManifest, String> {
    if !database_path.is_file() {
        return Err(format!(
            "Banco local não encontrado em {}.",
            database_path.display()
        ));
    }
    fs::create_dir_all(backups_root).map_err(|error| error.to_string())?;
    cleanup_staging_directories(backups_root)?;

    let backup_id = format!(
        "{}_{}",
        Utc::now().format("%Y-%m-%d_%H%M%S"),
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let staging = backups_root.join(format!(".tmp-{backup_id}"));
    let destination = backups_root.join(&backup_id);
    fs::create_dir(&staging).map_err(|error| error.to_string())?;

    let result = create_backup_in_staging(
        database_path,
        assets_path,
        &staging,
        &backup_id,
        app_version,
        reason,
    );
    let manifest = match result {
        Ok(manifest) => manifest,
        Err(error) => {
            fs::remove_dir_all(&staging).ok();
            return Err(error);
        }
    };

    fs::rename(&staging, &destination)
        .map_err(|error| format!("Não foi possível publicar o backup concluído: {error}"))?;
    let validation = validate_backup_at(backups_root, &backup_id)?;
    if !validation.valid {
        return Err(format!(
            "O backup foi preservado, mas falhou na validação após a publicação: {}",
            validation.errors.join(" ")
        ));
    }
    if let Err(error) =
        apply_automatic_retention_at(backups_root, DEFAULT_AUTOMATIC_RETENTION, Some(&backup_id))
    {
        eprintln!(
            "[NarraHub][backup] Retention failed after publishing backup {backup_id}: {error}"
        );
    }
    Ok(manifest)
}

fn create_backup_in_staging(
    database_path: &Path,
    assets_path: Option<&Path>,
    staging: &Path,
    backup_id: &str,
    app_version: &str,
    reason: BackupReason,
) -> Result<BackupManifest, String> {
    let destination_database = staging.join(DATABASE_FILE_NAME);
    let source = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Não foi possível abrir o banco para backup: {error}"))?;
    source
        .backup(DatabaseName::Main, &destination_database, None)
        .map_err(|error| format!("A cópia consistente do SQLite falhou: {error}"))?;

    let health = inspect_database(&destination_database)?;
    if !health.integrity_result.eq_ignore_ascii_case("ok") {
        return Err("O snapshot criado não passou no integrity_check do SQLite.".into());
    }

    let database = BackupDatabaseManifest {
        file: DATABASE_FILE_NAME.into(),
        sha256: hash_file(&destination_database)?,
        size_bytes: fs::metadata(&destination_database)
            .map_err(|error| error.to_string())?
            .len(),
    };
    let assets = copy_assets(assets_path, &staging.join("assets"))?;
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        backup_id: backup_id.into(),
        schema_version: health.schema_version,
        app_version: app_version.into(),
        created_at: Utc::now().to_rfc3339(),
        reason,
        database,
        assets,
    };
    write_json(&staging.join(MANIFEST_FILE_NAME), &manifest)?;
    Ok(manifest)
}

pub fn list_backups_at(backups_root: &Path) -> Result<Vec<BackupManifest>, String> {
    if !backups_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut manifests: Vec<BackupManifest> = Vec::new();
    for entry in fs::read_dir(backups_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
            || file_name.starts_with(".tmp-")
        {
            continue;
        }
        let manifest_path = entry.path().join(MANIFEST_FILE_NAME);
        if manifest_path.is_file() {
            manifests.push(read_json(&manifest_path)?);
        }
    }
    manifests.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(manifests)
}

pub fn apply_automatic_retention_at(
    backups_root: &Path,
    keep_latest: usize,
    protected_backup_id: Option<&str>,
) -> Result<Vec<String>, String> {
    if !backups_root.is_dir() {
        return Ok(Vec::new());
    }
    let canonical_root = fs::canonicalize(backups_root).map_err(|error| error.to_string())?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(backups_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if name.starts_with(".tmp-") || !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        if validate_backup_id(&name).is_err() {
            continue;
        }
        let manifest_path = entry.path().join(MANIFEST_FILE_NAME);
        let Ok(manifest) = read_json::<BackupManifest>(&manifest_path) else {
            continue;
        };
        if manifest.backup_id != name
            || !matches!(
                manifest.reason,
                BackupReason::PreMigration | BackupReason::PreUpdate | BackupReason::Periodic
            )
        {
            continue;
        }
        candidates.push((manifest, entry.path()));
    }
    candidates.sort_by(|(left, _), (right, _)| {
        let left_protected = protected_backup_id.is_some_and(|id| id == left.backup_id);
        let right_protected = protected_backup_id.is_some_and(|id| id == right.backup_id);
        right_protected
            .cmp(&left_protected)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| right.backup_id.cmp(&left.backup_id))
    });

    let mut removed = Vec::new();
    for (manifest, directory) in candidates.into_iter().skip(keep_latest.max(1)) {
        if protected_backup_id.is_some_and(|protected| protected == manifest.backup_id) {
            continue;
        }
        let canonical_directory =
            fs::canonicalize(&directory).map_err(|error| error.to_string())?;
        if canonical_directory.parent() != Some(canonical_root.as_path()) {
            return Err(format!(
                "A retenção recusou um diretório fora da raiz de backups: {}",
                directory.display()
            ));
        }
        fs::remove_dir_all(&canonical_directory).map_err(|error| {
            format!(
                "Não foi possível remover o backup automático {}: {error}",
                manifest.backup_id
            )
        })?;
        removed.push(manifest.backup_id);
    }
    Ok(removed)
}

pub fn validate_backup_at(
    backups_root: &Path,
    backup_id: &str,
) -> Result<BackupValidation, String> {
    validate_backup_id(backup_id)?;
    let backup_directory = backups_root.join(backup_id);
    if !backup_directory.is_dir() {
        return Err("Backup não encontrado.".into());
    }

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let manifest_path = backup_directory.join(MANIFEST_FILE_NAME);
    let manifest: BackupManifest = match read_json(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Ok(BackupValidation {
                valid: false,
                errors: vec![error],
                warnings,
                manifest: None,
                database_health: None,
            });
        }
    };
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        errors.push(format!(
            "Formato de backup {} não suportado por esta versão.",
            manifest.format_version
        ));
    }
    if manifest.backup_id != backup_id {
        errors.push("O identificador do manifesto não corresponde ao diretório.".into());
    }

    if manifest.database.file != DATABASE_FILE_NAME {
        errors.push("O manifesto aponta para um arquivo de banco não permitido.".into());
    }
    let database_path = backup_directory.join(DATABASE_FILE_NAME);
    match hash_file(&database_path) {
        Ok(hash) if hash == manifest.database.sha256 => {}
        Ok(_) => errors.push("O hash do banco não corresponde ao manifesto.".into()),
        Err(error) => errors.push(error),
    }
    if database_path.is_file() {
        match fs::metadata(&database_path) {
            Ok(metadata) if metadata.len() == manifest.database.size_bytes => {}
            Ok(_) => errors.push("O tamanho do banco não corresponde ao manifesto.".into()),
            Err(error) => errors.push(error.to_string()),
        }
    }

    let database_health = if database_path.is_file() {
        match inspect_database(&database_path) {
            Ok(health) => {
                if !health.integrity_result.eq_ignore_ascii_case("ok") {
                    errors.push("O banco do backup falhou no integrity_check.".into());
                }
                if health.schema_version != manifest.schema_version {
                    errors.push("A versão do schema não corresponde ao manifesto.".into());
                }
                if health.foreign_key_violations > 0 {
                    warnings.push(format!(
                        "O snapshot preserva {} violação(ões) de foreign key da base de origem.",
                        health.foreign_key_violations
                    ));
                }
                for issue in &health.issues {
                    if issue.code != "sqlite_integrity" && issue.code != "foreign_key_violation" {
                        warnings.push(format!("{}: {}", issue.code, issue.message));
                    }
                }
                Some(health)
            }
            Err(error) => {
                errors.push(error);
                None
            }
        }
    } else {
        errors.push("O arquivo de banco do backup não existe.".into());
        None
    };

    validate_assets(
        &backup_directory.join("assets"),
        &manifest.assets,
        &mut errors,
    )?;
    Ok(BackupValidation {
        valid: errors.is_empty(),
        errors,
        warnings,
        manifest: Some(manifest),
        database_health,
    })
}

pub(crate) fn copy_assets(
    source: Option<&Path>,
    destination: &Path,
) -> Result<BackupAssetsManifest, String> {
    let mut entries = Vec::new();
    if let Some(source) = source.filter(|path| path.is_dir()) {
        copy_asset_directory(source, source, destination, &mut entries)?;
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    build_assets_manifest(entries)
}

fn copy_asset_directory(
    root: &Path,
    current: &Path,
    destination: &Path,
    entries: &mut Vec<BackupAssetEntry>,
) -> Result<(), String> {
    let mut children = fs::read_dir(current)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let file_type = child.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err(format!(
                "Asset simbólico não é permitido: {}",
                child.path().display()
            ));
        }
        if file_type.is_dir() {
            copy_asset_directory(root, &child.path(), destination, entries)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = child
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_path_buf();
        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(child.path(), &target).map_err(|error| error.to_string())?;
        entries.push(BackupAssetEntry {
            path: relative_path(&relative),
            sha256: hash_file(&target)?,
            size_bytes: fs::metadata(&target)
                .map_err(|error| error.to_string())?
                .len(),
        });
    }
    Ok(())
}

fn validate_assets(
    assets_root: &Path,
    expected: &BackupAssetsManifest,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let mut actual = Vec::new();
    if assets_root.is_dir() {
        scan_asset_directory(assets_root, assets_root, &mut actual)?;
    }
    actual.sort_by(|left, right| left.path.cmp(&right.path));
    let actual_manifest = build_assets_manifest(actual)?;
    if actual_manifest.files != expected.files
        || actual_manifest.count != expected.count
        || actual_manifest.total_bytes != expected.total_bytes
        || actual_manifest.manifest_sha256 != expected.manifest_sha256
    {
        errors.push("Os assets não correspondem ao manifesto do backup.".into());
    }
    Ok(())
}

fn scan_asset_directory(
    root: &Path,
    current: &Path,
    entries: &mut Vec<BackupAssetEntry>,
) -> Result<(), String> {
    for child in fs::read_dir(current).map_err(|error| error.to_string())? {
        let child = child.map_err(|error| error.to_string())?;
        let file_type = child.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err(format!(
                "Asset simbólico não é permitido: {}",
                child.path().display()
            ));
        }
        if file_type.is_dir() {
            scan_asset_directory(root, &child.path(), entries)?;
        } else if file_type.is_file() {
            let relative = child
                .path()
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_path_buf();
            entries.push(BackupAssetEntry {
                path: relative_path(&relative),
                sha256: hash_file(&child.path())?,
                size_bytes: child.metadata().map_err(|error| error.to_string())?.len(),
            });
        }
    }
    Ok(())
}

fn build_assets_manifest(entries: Vec<BackupAssetEntry>) -> Result<BackupAssetsManifest, String> {
    let serialized = serde_json::to_vec(&entries).map_err(|error| error.to_string())?;
    Ok(BackupAssetsManifest {
        count: entries.len(),
        total_bytes: entries.iter().map(|entry| entry.size_bytes).sum(),
        manifest_sha256: hash_bytes(&serialized),
        files: entries,
    })
}

fn cleanup_staging_directories(backups_root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(backups_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let is_staging = entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
            && entry.file_name().to_string_lossy().starts_with(".tmp-");
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        if is_staging && modified.is_some_and(staging_is_stale) {
            fs::remove_dir_all(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn staging_is_stale(modified: SystemTime) -> bool {
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age >= STAGING_RETENTION)
}

fn validate_backup_id(backup_id: &str) -> Result<(), String> {
    if backup_id.is_empty()
        || backup_id.len() > 80
        || !backup_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Identificador de backup inválido.".into());
    }
    Ok(())
}

fn relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Não foi possível abrir {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = File::create(path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let file = File::open(path)
        .map_err(|error| format!("Não foi possível abrir {}: {error}", path.display()))?;
    serde_json::from_reader(file).map_err(|error| format!("Manifesto inválido: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::{LATEST_SCHEMA_VERSION, MIGRATION_V1};
    use rusqlite::params;

    struct TestPaths {
        root: std::path::PathBuf,
        database: std::path::PathBuf,
        assets: std::path::PathBuf,
        backups: std::path::PathBuf,
    }

    impl TestPaths {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("narrahub-backup-{}", Uuid::new_v4()));
            let assets = root.join("assets");
            let backups = root.join("backups");
            fs::create_dir_all(&assets).expect("create assets directory");
            Self {
                database: root.join(DATABASE_FILE_NAME),
                root,
                assets,
                backups,
            }
        }
    }

    impl Drop for TestPaths {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    fn open_database(paths: &TestPaths) -> Connection {
        let connection = Connection::open(&paths.database).expect("create database");
        connection
            .execute_batch(MIGRATION_V1)
            .expect("apply schema");
        connection
            .pragma_update(None, "user_version", 10)
            .expect("set schema version");
        connection
    }

    /// GATE DO ADR 0009 §5: a identidade de sincronização não entra no backup.
    ///
    /// A decisão é explícita e tem consequência prática forte — restaurar um
    /// backup em outro aparelho cria um dispositivo **novo**, que precisa
    /// parear de novo. Se a chave viajasse junto:
    ///
    /// ```text
    /// dois aparelhos com o mesmo device_id
    ///   →  duas sequências diferentes sob a mesma origem
    ///   →  o cursor de TODOS os outros peers passa a significar duas coisas
    ///   →  perda silenciosa, do tipo que ninguém descobre no dia
    /// ```
    ///
    /// O backup empacota `narrahub.db` e `assets/` por lista explícita, então
    /// hoje o arquivo já fica de fora. Este gate existe para o dia em que
    /// alguém trocar a lista por "copie o diretório de dados inteiro", que é
    /// a mudança mais natural do mundo e a mais cara aqui.
    #[test]
    fn o_backup_nao_leva_a_identidade_de_sincronizacao() {
        let paths = TestPaths::new();
        let connection = open_database(&paths);
        drop(connection);

        // O arquivo de identidade mora ao lado do banco, no diretório de
        // dados do app — exatamente onde um backup descuidado o pegaria.
        let identidade = paths
            .root
            .join(crate::infrastructure::identity_store::IDENTITY_FILE_NAME);
        fs::write(
            &identidade,
            r#"{"ed25519_secret":"SEGREDOQUENAOPODEVIAJAR"}"#,
        )
        .expect("semear identidade");

        let manifest = create_backup_at(
            &paths.database,
            Some(paths.assets.as_path()),
            &paths.backups,
            "0.0.0-test",
            BackupReason::Manual,
        )
        .expect("criar backup");

        let destino = paths.backups.join(&manifest.backup_id);

        // Varre a árvore inteira do backup: nome de arquivo e conteúdo. Não
        // basta conferir a lista de arquivos que o código diz empacotar — é
        // exatamente essa lista que a mudança perigosa trocaria.
        let mut visitados = Vec::new();
        let mut pilha = vec![destino.clone()];
        while let Some(atual) = pilha.pop() {
            for entrada in fs::read_dir(&atual).expect("ler diretório do backup") {
                let entrada = entrada.expect("entrada do diretório");
                let caminho = entrada.path();
                if caminho.is_dir() {
                    pilha.push(caminho);
                    continue;
                }
                let nome = caminho.to_string_lossy().to_string();
                assert!(
                    !nome.contains("sync-identity"),
                    "o backup levou a identidade de sincronização: {nome}"
                );
                let conteudo = fs::read(&caminho).expect("ler arquivo do backup");
                assert!(
                    !String::from_utf8_lossy(&conteudo).contains("SEGREDOQUENAOPODEVIAJAR"),
                    "a chave privada apareceu dentro de {nome}"
                );
                visitados.push(nome);
            }
        }

        assert!(
            !visitados.is_empty(),
            "a varredura não encontrou arquivo nenhum; ela quebrou e passaria vazia"
        );
    }

    #[test]
    fn online_backup_captures_confirmed_wal_write_and_assets() {
        let paths = TestPaths::new();
        let connection = open_database(&paths);
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .expect("enable wal");
        connection
            .execute(
                "INSERT INTO universes (id, name) VALUES (?1, ?2)",
                params!["u1", "Cidade sem Sol"],
            )
            .expect("insert universe");
        fs::write(paths.assets.join("cover.bin"), b"asset-real").expect("write asset");

        let manifest = create_backup_at(
            &paths.database,
            Some(&paths.assets),
            &paths.backups,
            "0.7.3-test",
            BackupReason::PreUpdate,
        )
        .expect("create backup");
        let backup_database = paths
            .backups
            .join(&manifest.backup_id)
            .join(DATABASE_FILE_NAME);
        let backup_connection = Connection::open(backup_database).expect("open backup database");
        let name: String = backup_connection
            .query_row("SELECT name FROM universes WHERE id = 'u1'", [], |row| {
                row.get(0)
            })
            .expect("read confirmed WAL row");

        assert_eq!(name, "Cidade sem Sol");
        assert_eq!(manifest.assets.count, 1);
        assert_eq!(manifest.schema_version, 10);
        let validation =
            validate_backup_at(&paths.backups, &manifest.backup_id).expect("validate backup");
        assert!(
            validation.valid,
            "validation errors: {:?}",
            validation.errors
        );
    }

    #[test]
    fn changed_database_is_rejected_by_manifest_hash() {
        let paths = TestPaths::new();
        let _connection = open_database(&paths);
        let manifest = create_backup_at(
            &paths.database,
            None,
            &paths.backups,
            "0.7.3-test",
            BackupReason::Manual,
        )
        .expect("create backup");
        let backup_database = paths
            .backups
            .join(&manifest.backup_id)
            .join(DATABASE_FILE_NAME);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(backup_database)
            .expect("open backup");
        file.write_all(b"corruption")
            .expect("change database bytes");

        let validation = validate_backup_at(&paths.backups, &manifest.backup_id)
            .expect("validate changed backup");
        assert!(!validation.valid);
        assert!(validation
            .errors
            .iter()
            .any(|error| error.contains("hash do banco")));
    }

    #[test]
    fn manifest_cannot_redirect_database_outside_backup_directory() {
        let paths = TestPaths::new();
        let _connection = open_database(&paths);
        let manifest = create_backup_at(
            &paths.database,
            None,
            &paths.backups,
            "0.7.3-test",
            BackupReason::Manual,
        )
        .expect("create backup");
        let manifest_path = paths
            .backups
            .join(&manifest.backup_id)
            .join(MANIFEST_FILE_NAME);
        let mut changed: BackupManifest = read_json(&manifest_path).expect("read manifest");
        changed.database.file = "../narrahub.db".into();
        write_json(&manifest_path, &changed).expect("write changed manifest");

        let validation = validate_backup_at(&paths.backups, &manifest.backup_id)
            .expect("validate changed manifest");
        assert!(!validation.valid);
        assert!(validation
            .errors
            .iter()
            .any(|error| error.contains("arquivo de banco não permitido")));
    }

    #[test]
    fn interrupted_staging_directory_is_never_listed_or_mistaken_for_a_backup() {
        let paths = TestPaths::new();
        let _connection = open_database(&paths);
        let interrupted = paths.backups.join(".tmp-interrupted");
        fs::create_dir_all(&interrupted).expect("create interrupted directory");
        fs::write(interrupted.join(DATABASE_FILE_NAME), b"partial").expect("write partial backup");

        assert!(list_backups_at(&paths.backups)
            .expect("list backups")
            .is_empty());
        create_backup_at(
            &paths.database,
            None,
            &paths.backups,
            "0.7.3-test",
            BackupReason::Manual,
        )
        .expect("create replacement backup");
        assert!(
            interrupted.exists(),
            "fresh staging may belong to another process"
        );
        assert_eq!(
            list_backups_at(&paths.backups).expect("list backups").len(),
            1
        );
    }

    #[test]
    fn staging_is_only_stale_after_retention_window() {
        assert!(!staging_is_stale(SystemTime::now()));
        assert!(staging_is_stale(
            SystemTime::now() - STAGING_RETENTION - Duration::from_secs(1)
        ));
    }

    #[test]
    fn unsafe_backup_identifier_is_rejected() {
        let paths = TestPaths::new();
        let error =
            validate_backup_at(&paths.backups, "../narrahub.db").expect_err("reject traversal");
        assert!(error.contains("inválido"));
    }

    #[test]
    fn automatic_retention_preserves_manual_and_pre_restore_backups() {
        let paths = TestPaths::new();
        let _connection = open_database(&paths);
        let manual = create_backup_at(
            &paths.database,
            None,
            &paths.backups,
            "0.7.3-test",
            BackupReason::Manual,
        )
        .expect("create manual backup");
        let pre_restore = create_backup_at(
            &paths.database,
            None,
            &paths.backups,
            "0.7.3-test",
            BackupReason::PreRestore,
        )
        .expect("create pre-restore backup");
        for _ in 0..7 {
            create_backup_at(
                &paths.database,
                None,
                &paths.backups,
                "0.7.3-test",
                BackupReason::Periodic,
            )
            .expect("create automatic backup");
        }

        let backups = list_backups_at(&paths.backups).expect("list retained backups");
        assert_eq!(
            backups
                .iter()
                .filter(|backup| backup.reason == BackupReason::Periodic)
                .count(),
            DEFAULT_AUTOMATIC_RETENTION
        );
        assert!(backups
            .iter()
            .any(|backup| backup.backup_id == manual.backup_id));
        assert!(backups
            .iter()
            .any(|backup| backup.backup_id == pre_restore.backup_id));
    }

    #[test]
    #[ignore = "executado explicitamente contra uma base desktop indicada por NARRAHUB_REAL_DB"]
    fn real_desktop_database_creates_a_valid_temporary_backup() {
        let database = std::env::var_os("NARRAHUB_REAL_DB")
            .map(std::path::PathBuf::from)
            .expect("set NARRAHUB_REAL_DB to an existing desktop database");
        assert!(database.is_file(), "real database does not exist");
        let root = std::env::temp_dir().join(format!("narrahub-real-backup-{}", Uuid::new_v4()));
        let backups = root.join("backups");
        let assets = database.parent().map(|parent| parent.join("assets"));

        let manifest = create_backup_at(
            &database,
            assets.as_deref().filter(|path| path.is_dir()),
            &backups,
            "real-runtime-validation",
            BackupReason::PreUpdate,
        )
        .expect("create backup from real desktop database");
        let validation =
            validate_backup_at(&backups, &manifest.backup_id).expect("validate real backup");

        let expected_schema = std::env::var("NARRAHUB_EXPECTED_SCHEMA")
            .ok()
            .and_then(|value| value.parse::<i64>().ok());
        if let Some(expected_schema) = expected_schema {
            assert_eq!(manifest.schema_version, expected_schema);
        }
        assert!(
            manifest.schema_version <= LATEST_SCHEMA_VERSION,
            "desktop schema {} is newer than this build {}",
            manifest.schema_version,
            LATEST_SCHEMA_VERSION
        );
        assert!(
            validation.valid,
            "validation errors: {:?}",
            validation.errors
        );
        assert!(validation.database_health.is_some());
        crate::database::recovery::validate_migration_compatibility(&database)
            .expect("real desktop migrations must match this build");

        if let Some(reference_database) =
            std::env::var_os("NARRAHUB_REFERENCE_DB").map(std::path::PathBuf::from)
        {
            let reference = Connection::open_with_flags(
                &reference_database,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("open reference database read-only");
            let migrated =
                Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .expect("open migrated database read-only");
            for table in [
                "universes",
                "stories",
                "books",
                "chapters",
                "entities",
                "relations",
                "mentions",
                "timeline_events",
                "planning_items",
                "content_tags",
                "content_tag_assignments",
            ] {
                let sql = format!("SELECT COUNT(*) FROM {table}");
                let reference_count: i64 = reference
                    .query_row(&sql, [], |row| row.get(0))
                    .unwrap_or_else(|error| panic!("count {table} in reference database: {error}"));
                let migrated_count: i64 = migrated
                    .query_row(&sql, [], |row| row.get(0))
                    .unwrap_or_else(|error| panic!("count {table} in migrated database: {error}"));
                assert_eq!(
                    migrated_count, reference_count,
                    "migration changed the number of rows in {table}"
                );
            }
        }
        fs::remove_dir_all(root).ok();
    }
}
