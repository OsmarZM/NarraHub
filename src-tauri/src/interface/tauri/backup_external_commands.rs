//! Seletores e cópias externas; acesso a arquivos fica nesta porta de plataforma.
use crate::database::backup::{
    create_backup_at, hash_file, BackupManifest, BackupReason, BackupRuntimeState,
};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::database::portable::{
    import_package, sanitize_snapshot, write_package, MAX_PACKAGE_BYTES,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::FsExt;

const CONFIG: &str = "external-backup.json";
const INTERVAL_SECONDS: i64 = 30 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReceipt {
    pub created_at: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExternalBackupStatus {
    pub destination: Option<String>,
    pub enabled: bool,
    pub last_success: Option<ExportReceipt>,
    pub last_error: String,
    last_attempt: Option<String>,
    fingerprint: String,
    owner: String,
}

impl Default for ExternalBackupStatus {
    fn default() -> Self {
        Self {
            destination: None,
            enabled: false,
            last_success: None,
            last_error: String::new(),
            last_attempt: None,
            fingerprint: String::new(),
            owner: uuid::Uuid::new_v4().simple().to_string(),
        }
    }
}

struct Busy<'a>(&'a AtomicBool);
impl<'a> Busy<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        if flag.swap(true, Ordering::AcqRel) {
            Err("Já existe uma operação de backup ou recuperação em andamento.".into())
        } else {
            Ok(Self(flag))
        }
    }
}
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct WorkingDirectory(PathBuf);
impl WorkingDirectory {
    fn new(root: &Path) -> Result<Self, String> {
        let path = root.join(format!(".tmp-portable-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for WorkingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn load(root: &Path) -> Result<ExternalBackupStatus, String> {
    match fs::read(root.join(CONFIG)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Configuração de backup externo inválida: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ExternalBackupStatus::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn save(root: &Path, status: &ExternalBackupStatus) -> Result<(), String> {
    crate::database::duravel::gravar_atomico(
        &root.join(CONFIG),
        &serde_json::to_vec_pretty(status).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn source_fingerprint(root: &Path) -> Result<String, String> {
    let mut value = String::new();
    for name in ["narrahub.db", "narrahub.db-wal"] {
        match fs::metadata(root.join(name)) {
            Ok(meta) => value.push_str(&format!(
                "{name}:{}:{:?};",
                meta.len(),
                meta.modified().map_err(|e| e.to_string())?
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && name.ends_with("-wal") => {
                value.push_str("no-wal;")
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(value)
}

pub(crate) fn validate_destination(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let parent = fs::canonicalize(
        path.parent()
            .ok_or("Escolha uma pasta de destino válida.")?,
    )
    .map_err(|e| e.to_string())?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let temp = fs::canonicalize(std::env::temp_dir()).map_err(|e| e.to_string())?;
    if parent.starts_with(&root) || parent.starts_with(&temp) {
        return Err(
            "Escolha um destino fora da pasta de dados do NarraHub e das pastas temporárias."
                .into(),
        );
    }
    let target = parent.join(path.file_name().ok_or("Nome do arquivo ausente.")?);
    if fs::symlink_metadata(&target)
        .is_ok_and(|meta| meta.file_type().is_symlink() || !meta.is_file())
    {
        return Err("O destino precisa ser um arquivo regular.".into());
    }
    Ok(target)
}

fn package(root: &Path, stage: &Path, reason: BackupReason) -> Result<PathBuf, String> {
    let mut manifest = create_backup_at(
        &root.join("narrahub.db"),
        Some(&root.join("assets")),
        &root.join("backups"),
        env!("CARGO_PKG_VERSION"),
        reason,
    )?;
    let snapshot = stage.join(&manifest.backup_id);
    fs::create_dir(&snapshot).map_err(|e| e.to_string())?;
    let original = root.join("backups").join(&manifest.backup_id);
    fs::copy(original.join("narrahub.db"), snapshot.join("narrahub.db"))
        .map_err(|e| e.to_string())?;
    crate::database::backup::copy_assets(Some(&original.join("assets")), &snapshot.join("assets"))?;
    sanitize_snapshot(&snapshot, &mut manifest)?;
    let validation = crate::database::backup::validate_backup_at(stage, &manifest.backup_id)?;
    if !validation.valid
        || validation
            .database_health
            .as_ref()
            .is_none_or(|health| health.foreign_key_violations != 0)
    {
        return Err(format!(
            "O snapshot não passou na validação para exportação: {}",
            validation.errors.join(" ")
        ));
    }
    let path = stage.join("archive.narrahub-backup");
    write_package(&snapshot, &path, &manifest)?;
    Ok(path)
}

fn digest_reader(mut reader: impl Read) -> Result<(String, u64), String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut total = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_PACKAGE_BYTES {
            return Err("O arquivo ultrapassa o limite de 8 GB.".into());
        }
        hasher.update(&buffer[..n]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}
fn verify_target(
    app: &AppHandle,
    target: FilePath,
    package: &Path,
) -> Result<(String, u64), String> {
    let options =
        serde_json::from_value(serde_json::json!({"read":true})).map_err(|e| e.to_string())?;
    let actual = digest_reader(app.fs().open(target, options).map_err(|e| e.to_string())?)?;
    if actual.0 != hash_file(package)?
        || actual.1 != fs::metadata(package).map_err(|e| e.to_string())?.len()
    {
        return Err("A cópia externa divergiu do pacote. Nenhum sucesso foi registrado.".into());
    }
    Ok(actual)
}
fn publish(
    app: &AppHandle,
    target: FilePath,
    package: &Path,
    root: &Path,
) -> Result<(String, u64), String> {
    match &target {
        FilePath::Path(path) => {
            let path = validate_destination(path, root)?;
            let partial =
                path.with_file_name(format!(".narrahub-export-{}.partial", uuid::Uuid::new_v4()));
            let result = (|| {
                fs::copy(package, &partial).map_err(|e| e.to_string())?;
                verify_target(app, FilePath::Path(partial.clone()), package)?;
                crate::database::duravel::substituir_arquivo(&partial, &path)
                    .map_err(|e| e.to_string())?;
                verify_target(app, FilePath::Path(path), package)
            })();
            let _ = fs::remove_file(partial);
            result
        }
        FilePath::Url(_) => {
            let options = serde_json::from_value(
                serde_json::json!({"read":false,"write":true,"truncate":true}),
            )
            .map_err(|e| e.to_string())?;
            let mut output = app
                .fs()
                .open(target.clone(), options)
                .map_err(|e| e.to_string())?;
            std::io::copy(
                &mut File::open(package).map_err(|e| e.to_string())?,
                &mut output,
            )
            .map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
            drop(output);
            verify_target(app, target, package)
        }
    }
}

async fn worker<T: Send + 'static>(
    app: AppHandle,
    task: impl FnOnce(AppHandle, PathBuf) -> Result<T, String> + Send + 'static,
) -> DatabaseCommandResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<BackupRuntimeState>();
        let _busy = Busy::acquire(&state.running)?;
        let root = crate::database::app_data_path(&app)?;
        task(app.clone(), root)
    })
    .await
    .map_err(|e| DatabaseCommandError::storage(e.to_string()))?
    .map_err(DatabaseCommandError::storage)
}

#[tauri::command]
pub async fn backup_external_status(app: AppHandle) -> DatabaseCommandResult<ExternalBackupStatus> {
    let root = crate::database::app_data_path(&app).map_err(DatabaseCommandError::storage)?;
    load(&root).map_err(DatabaseCommandError::storage)
}

#[tauri::command]
pub async fn backup_export_external(
    app: AppHandle,
) -> DatabaseCommandResult<Option<ExportReceipt>> {
    worker(app, |app, root| {
        let file_name = format!(
            "NarraHub-{}.narrahub-backup",
            Utc::now().format("%Y%m%d-%H%M%S")
        );
        let Some(target) = app
            .dialog()
            .file()
            .set_title("Salvar backup externo")
            .set_file_name(&file_name)
            .add_filter("Backup NarraHub", &["narrahub-backup"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let stage = WorkingDirectory::new(&root)?;
        let fingerprint = source_fingerprint(&root)?;
        let package = package(&root, &stage.0, BackupReason::Manual)?;
        let (sha256, size_bytes) = publish(&app, target, &package, &root)?;
        let receipt = ExportReceipt {
            created_at: Utc::now().to_rfc3339(),
            file_name,
            size_bytes,
            sha256,
        };
        let mut status = load(&root)?;
        status.last_success = Some(receipt.clone());
        // Uma exportação manual não comprova que o destino periódico esteja atualizado.
        if !status.enabled {
            status.fingerprint = fingerprint;
        }
        status.last_error.clear();
        save(&root, &status)?;
        Ok(Some(receipt))
    })
    .await
}

#[tauri::command]
pub async fn backup_import_external(
    app: AppHandle,
) -> DatabaseCommandResult<Option<BackupManifest>> {
    worker(app, |app, root| {
        let Some(source) = app
            .dialog()
            .file()
            .set_title("Abrir backup NarraHub")
            .add_filter("Backup NarraHub", &["narrahub-backup"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let stage = WorkingDirectory::new(&root)?;
        let path = stage.0.join("incoming.narrahub-backup");
        let options =
            serde_json::from_value(serde_json::json!({"read":true})).map_err(|e| e.to_string())?;
        let mut input = app.fs().open(source, options).map_err(|e| e.to_string())?;
        let mut output = File::create(&path).map_err(|e| e.to_string())?;
        let bytes = std::io::copy(&mut (&mut input).take(MAX_PACKAGE_BYTES + 1), &mut output)
            .map_err(|e| e.to_string())?;
        if bytes > MAX_PACKAGE_BYTES {
            return Err("O arquivo ultrapassa o limite de 8 GB.".into());
        }
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        import_package(
            File::open(path).map_err(|e| e.to_string())?,
            &root.join("backups"),
        )
        .map(Some)
    })
    .await
}

#[tauri::command]
pub async fn backup_external_configure(
    app: AppHandle,
    enabled: bool,
) -> DatabaseCommandResult<ExternalBackupStatus> {
    worker(app, move |app, root| {
        let mut status = load(&root)?;
        if enabled {
            let Some(destination) = pick_directory(&app)? else {
                return Ok(status);
            };
            #[cfg(not(target_os = "android"))]
            validate_destination(
                &Path::new(&destination).join("probe.narrahub-backup"),
                &root,
            )?;
            status.destination = Some(destination);
            status.fingerprint.clear();
            status.last_attempt = None;
        }
        status.enabled = enabled;
        status.last_error.clear();
        save(&root, &status)?;
        Ok(status)
    })
    .await
}

#[tauri::command]
pub async fn backup_external_tick(
    app: AppHandle,
    force: bool,
) -> DatabaseCommandResult<ExternalBackupStatus> {
    worker(app, move |app, root| {
        let mut status = load(&root)?;
        if !status.enabled {
            return Ok(status);
        }
        let fingerprint = source_fingerprint(&root)?;
        let elapsed = status
            .last_attempt
            .as_ref()
            .and_then(|date| DateTime::parse_from_rfc3339(date).ok())
            .map(|date| (Utc::now() - date.with_timezone(&Utc)).num_seconds())
            .unwrap_or(INTERVAL_SECONDS);
        if !force && (elapsed < INTERVAL_SECONDS || fingerprint == status.fingerprint) {
            return Ok(status);
        }
        status.last_attempt = Some(Utc::now().to_rfc3339());
        // Registrar antes do trabalho permite backoff também após erro/interrupção.
        save(&root, &status)?;
        let result = (|| {
            let stage = WorkingDirectory::new(&root)?;
            let package = package(&root, &stage.0, BackupReason::Periodic)?;
            let file_name = format!(
                "NarraHub-auto-{}-{}-{}.narrahub-backup",
                status.owner,
                Utc::now().format("%Y%m%d%H%M%S"),
                uuid::Uuid::new_v4().simple()
            );
            let target = publish_directory(
                &app,
                status
                    .destination
                    .as_deref()
                    .ok_or("Escolha um destino externo.")?,
                &file_name,
                &package,
                &root,
            )?;
            let (sha256, size_bytes) = verify_target(&app, target, &package)?;
            let receipt = ExportReceipt {
                created_at: Utc::now().to_rfc3339(),
                file_name,
                size_bytes,
                sha256,
            };
            Ok::<_, String>(receipt)
        })();
        match result {
            Ok(receipt) => {
                status.last_success = Some(receipt);
                status.fingerprint = fingerprint;
                status.last_error = prune_directory(&app, &status).err().unwrap_or_default();
            }
            Err(error) => status.last_error = error,
        }
        save(&root, &status)?;
        Ok(status)
    })
    .await
}

#[cfg(not(target_os = "android"))]
fn pick_directory(app: &AppHandle) -> Result<Option<String>, String> {
    app.dialog()
        .file()
        .set_title("Pasta para cópias externas automáticas")
        .blocking_pick_folder()
        .map(|path| match path {
            FilePath::Path(path) => Ok(path.to_string_lossy().into_owned()),
            _ => Err("Escolha uma pasta local.".into()),
        })
        .transpose()
}
#[cfg(not(target_os = "android"))]
fn publish_directory(
    app: &AppHandle,
    destination: &str,
    name: &str,
    package: &Path,
    root: &Path,
) -> Result<FilePath, String> {
    let target = FilePath::Path(Path::new(destination).join(name));
    publish(app, target.clone(), package, root)?;
    Ok(target)
}
#[cfg(not(target_os = "android"))]
fn prune_directory(_app: &AppHandle, status: &ExternalBackupStatus) -> Result<(), String> {
    let destination = Path::new(
        status
            .destination
            .as_deref()
            .ok_or("Destino externo ausente.")?,
    );
    let prefix = format!("NarraHub-auto-{}-", status.owner);
    prune_path(destination, &prefix)
}

#[cfg(not(target_os = "android"))]
fn prune_path(destination: &Path, prefix: &str) -> Result<(), String> {
    let mut files = fs::read_dir(destination)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            managed_name(&name, prefix)
                && entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_file() && !kind.is_symlink())
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| entry.file_name());
    let delete_count = files.len().saturating_sub(5);
    for entry in files.into_iter().take(delete_count) {
        fs::remove_file(entry.path())
            .map_err(|e| format!("A cópia foi salva; retenção pendente: {e}"))?;
    }
    Ok(())
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    fn retention_only_removes_old_regular_copies_owned_by_this_profile() {
        let dir = std::env::temp_dir().join(format!("nh-retention-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let prefix = format!("NarraHub-auto-{}-", "a".repeat(32));
        for index in 0..8 {
            fs::write(
                dir.join(format!(
                    "{prefix}202610060000{index:02}-{}.narrahub-backup",
                    "b".repeat(32)
                )),
                b"valid-copy",
            )
            .unwrap();
        }
        for name in [
            "manual.narrahub-backup",
            "user.txt",
            "NarraHub-auto-other-profile.narrahub-backup",
        ] {
            fs::write(dir.join(name), b"preserve").unwrap();
        }
        let folder = format!("{prefix}20261006000099-{}.narrahub-backup", "c".repeat(32));
        fs::create_dir(dir.join(&folder)).unwrap();
        prune_path(&dir, &prefix).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 9);
        assert!(dir.join("manual.narrahub-backup").is_file());
        assert!(dir.join("user.txt").is_file());
        assert!(dir.join(folder).is_dir());
        assert!(!dir
            .join(format!(
                "{prefix}20261006000000-{}.narrahub-backup",
                "b".repeat(32)
            ))
            .exists());
        assert!(dir
            .join(format!(
                "{prefix}20261006000007-{}.narrahub-backup",
                "b".repeat(32)
            ))
            .is_file());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hostile_unicode_names_are_not_managed_and_do_not_panic() {
        assert!(!managed_name(
            &format!("owned-{}.narrahub-backup", "é".repeat(23) + "x"),
            "owned-"
        ));
        assert!(!managed_name("owned-anything.narrahub-backup", "owned-"));
    }

    #[test]
    fn destination_inside_app_or_temporary_storage_is_refused() {
        let dir = std::env::temp_dir().join(format!("nh-destination-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let error = validate_destination(&dir.join("backup.narrahub-backup"), &dir).unwrap_err();
        assert!(error.contains("fora da pasta"));
        assert!(
            validate_destination(&std::env::temp_dir().join("backup.narrahub-backup"), &dir)
                .is_err()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backup_lock_rejects_overlap_and_releases_even_after_error() {
        let flag = AtomicBool::new(false);
        {
            let _guard = Busy::acquire(&flag).unwrap();
            assert!(Busy::acquire(&flag).is_err());
        }
        assert!(Busy::acquire(&flag).is_ok());
    }
}
#[cfg(not(target_os = "android"))]
fn managed_name(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|tail| tail.strip_suffix(".narrahub-backup"))
        .is_some_and(|tail| {
            tail.is_ascii()
                && tail.len() == 47
                && tail.as_bytes()[14] == b'-'
                && tail[..14].bytes().all(|byte| byte.is_ascii_digit())
                && tail[15..].bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

#[cfg(target_os = "android")]
struct AndroidBackup(tauri::plugin::PluginHandle<tauri::Wry>);
pub fn plugin_backup() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("narrahub-backup")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _app.manage(AndroidBackup(
                _api.register_android_plugin("com.narrahub.app", "BackupPlugin")?,
            ));
            Ok(())
        })
        .build()
}
#[cfg(target_os = "android")]
fn pick_directory(app: &AppHandle) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct Picked {
        destination: Option<String>,
    }
    app.state::<AndroidBackup>()
        .0
        .run_mobile_plugin::<Picked>("pickDirectory", ())
        .map(|result| result.destination)
        .map_err(|e| e.to_string())
}
#[cfg(target_os = "android")]
fn publish_directory(
    app: &AppHandle,
    destination: &str,
    name: &str,
    package: &Path,
    _root: &Path,
) -> Result<FilePath, String> {
    #[derive(Deserialize)]
    struct Published {
        uri: String,
    }
    let result = app.state::<AndroidBackup>().0.run_mobile_plugin::<Published>("writeDirectory", serde_json::json!({"destination":destination,"source":package.to_string_lossy(),"name":name,"sha256":hash_file(package)?})).map_err(|e| e.to_string())?;
    serde_json::from_value(serde_json::Value::String(result.uri)).map_err(|e| e.to_string())
}
#[cfg(target_os = "android")]
fn prune_directory(app: &AppHandle, status: &ExternalBackupStatus) -> Result<(), String> {
    app.state::<AndroidBackup>().0.run_mobile_plugin::<serde_json::Value>("pruneDirectory", serde_json::json!({"destination":status.destination,"prefix":format!("NarraHub-auto-{}-",status.owner)})).map(|_| ()).map_err(|e| e.to_string())
}
