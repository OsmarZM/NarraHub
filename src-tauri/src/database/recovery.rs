use super::backup::{
    copy_assets, create_backup_at, hash_file, validate_backup_at, BackupReason, BackupRuntimeState,
};
use super::error::{DatabaseCommandError, DatabaseCommandResult};
use super::health::inspect_database;
use super::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha384};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, State};
use uuid::Uuid;

const DATABASE_FILE_NAME: &str = "narrahub.db";
const RESTORE_EXPIRATION: Duration = Duration::from_secs(10 * 60);
const RESTORE_STAGING_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Default)]
pub struct RestoreRuntimeState {
    pending: Mutex<Option<PendingRestore>>,
}

#[derive(Debug, Clone)]
struct PendingRestore {
    token: String,
    backup_id: String,
    safety_backup_id: String,
    staging: PathBuf,
    expected_database_sha256: String,
    schema_version: i64,
    prepared_at: SystemTime,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreparation {
    pub token: String,
    pub backup_id: String,
    pub safety_backup_id: String,
    pub schema_version: i64,
    pub created_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCommitResult {
    pub restored_backup_id: String,
    pub safety_backup_id: String,
    pub rollback_id: String,
    pub schema_version: i64,
    pub requires_restart: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackManifest<'a> {
    format_version: u32,
    created_at: String,
    source_backup_id: &'a str,
    safety_backup_id: &'a str,
}

/// Onde a troca de arquivos deve falhar de propósito, para exercitar o rollback.
///
/// Era um `bool` que só falhava depois de tudo instalado — o caminho em que o
/// `SwapProgress` está completo e o rollback tem tudo para desfazer. Os estados
/// parciais, em que o banco ativo já saiu mas o restaurado ainda não entrou, nunca
/// eram exercitados, e é neles que um rollback incompleto deixaria o usuário sem
/// banco nenhum.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SwapFailurePoint {
    #[default]
    None,
    /// Depois de retirar o banco ativo, os sidecars e os assets, antes de instalar
    /// o restaurado. O rollback precisa devolver tudo o que saiu.
    AfterActiveMoved,
    /// Depois de instalar a base restaurada, com a troca completa.
    AfterInstall,
    /// Como `AfterInstall`, mas o rollback também falha. É o pior cenário do produto:
    /// o usuário fica sem a base nova e sem a antiga, e a única coisa entre ele e a
    /// perda do livro é a mensagem apontar para os arquivos preservados.
    AfterInstallWithBrokenRollback,
}

#[derive(Default)]
struct SwapProgress {
    original_database_moved: bool,
    original_assets_moved: bool,
    installed_database: bool,
    installed_assets: bool,
    moved_sidecars: Vec<String>,
}

#[tauri::command]
pub async fn backup_restore_prepare(
    app: AppHandle,
    backup_state: State<'_, BackupRuntimeState>,
    restore_state: State<'_, RestoreRuntimeState>,
    backup_id: String,
) -> DatabaseCommandResult<RestorePreparation> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    if backup_state.running.swap(true, Ordering::AcqRel) {
        return Err(DatabaseCommandError::conflict(
            "Já existe uma operação de backup ou recuperação em andamento.",
        ));
    }

    let task = tauri::async_runtime::spawn_blocking(move || {
        prepare_restore_at(&app_data, &backup_id, env!("CARGO_PKG_VERSION"))
    })
    .await;
    backup_state.running.store(false, Ordering::Release);

    let (preparation, pending) = task
        .map_err(|error| {
            DatabaseCommandError::unavailable(format!(
                "A preparação da restauração falhou: {error}"
            ))
        })?
        .map_err(classify_restore_error)?;
    let previous = restore_state
        .pending
        .lock()
        .map_err(|_| {
            DatabaseCommandError::unavailable("O estado de restauração ficou indisponível.")
        })?
        .replace(pending);
    if let Some(previous) = previous {
        remove_restore_staging(&previous.staging);
    }
    Ok(preparation)
}

#[tauri::command]
pub async fn backup_restore_commit(
    app: AppHandle,
    backup_state: State<'_, BackupRuntimeState>,
    restore_state: State<'_, RestoreRuntimeState>,
    token: String,
) -> DatabaseCommandResult<RestoreCommitResult> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    if backup_state.running.swap(true, Ordering::AcqRel) {
        return Err(DatabaseCommandError::conflict(
            "Já existe uma operação de backup ou recuperação em andamento.",
        ));
    }

    let pending = {
        let mut guard = match restore_state.pending.lock() {
            Ok(guard) => guard,
            Err(_) => {
                backup_state.running.store(false, Ordering::Release);
                return Err(DatabaseCommandError::unavailable(
                    "O estado de restauração ficou indisponível.",
                ));
            }
        };
        match guard.as_ref() {
            Some(pending) if pending.token == token => guard.take().expect("pending checked"),
            _ => {
                backup_state.running.store(false, Ordering::Release);
                return Err(DatabaseCommandError::not_found(
                    "A preparação de restauração não existe ou já expirou.",
                ));
            }
        }
    };

    let task =
        tauri::async_runtime::spawn_blocking(move || commit_restore_at(&app_data, pending)).await;
    backup_state.running.store(false, Ordering::Release);
    task.map_err(|error| {
        DatabaseCommandError::unavailable(format!("A troca recuperável do banco falhou: {error}"))
    })?
    .map_err(DatabaseCommandError::storage)
}

fn classify_restore_error(message: String) -> DatabaseCommandError {
    if message.contains("não é restaurável")
        || message.contains("checksum")
        || message.contains("schema")
        || message.contains("Atualize o NarraHub")
    {
        DatabaseCommandError::validation(message)
    } else if message.contains("não encontrado") {
        DatabaseCommandError::not_found(message)
    } else {
        DatabaseCommandError::storage(message)
    }
}

fn prepare_restore_at(
    app_data: &Path,
    backup_id: &str,
    app_version: &str,
) -> Result<(RestorePreparation, PendingRestore), String> {
    cleanup_restore_staging(app_data)?;
    let backups_root = app_data.join("backups");
    let validation = validate_backup_at(&backups_root, backup_id)?;
    if !validation.valid {
        return Err(format!(
            "O backup selecionado não é restaurável: {}",
            validation.errors.join(" ")
        ));
    }
    let manifest = validation
        .manifest
        .ok_or_else(|| "O backup validado não possui manifesto.".to_string())?;
    if manifest.schema_version > LATEST_SCHEMA_VERSION {
        return Err(format!(
            "O backup usa o schema {}, mas esta versão suporta até o schema {}. Atualize o NarraHub antes de restaurar.",
            manifest.schema_version, LATEST_SCHEMA_VERSION
        ));
    }
    validate_migration_compatibility(&backups_root.join(backup_id).join(DATABASE_FILE_NAME))?;

    let active_database = app_data.join(DATABASE_FILE_NAME);
    let active_assets = app_data.join("assets");
    let safety_backup = create_backup_at(
        &active_database,
        active_assets.is_dir().then_some(active_assets.as_path()),
        &backups_root,
        app_version,
        BackupReason::PreRestore,
    )?;
    let safety_validation = validate_backup_at(&backups_root, &safety_backup.backup_id)?;
    if !safety_validation.valid {
        return Err(format!(
            "O backup de segurança anterior à restauração não foi validado: {}",
            safety_validation.errors.join(" ")
        ));
    }

    let token = Uuid::new_v4().simple().to_string();
    let staging = app_data.join(format!(".restore-{token}"));
    fs::create_dir(&staging)
        .map_err(|error| format!("Não foi possível criar a área temporária: {error}"))?;

    let source_directory = backups_root.join(backup_id);
    let staged_database = staging.join(DATABASE_FILE_NAME);
    let prepare_result = (|| {
        fs::copy(source_directory.join(DATABASE_FILE_NAME), &staged_database)
            .map_err(|error| format!("Não foi possível preparar o banco restaurável: {error}"))?;
        if hash_file(&staged_database)? != manifest.database.sha256 {
            return Err("A cópia temporária do banco divergiu do manifesto.".into());
        }
        let source_assets = source_directory.join("assets");
        let staged_assets = copy_assets(
            source_assets.is_dir().then_some(source_assets.as_path()),
            &staging.join("assets"),
        )?;
        if staged_assets != manifest.assets {
            return Err("A cópia temporária dos assets divergiu do manifesto.".into());
        }
        let health = inspect_database(&staged_database)?;
        if !health.integrity_result.eq_ignore_ascii_case("ok")
            || health.schema_version != manifest.schema_version
        {
            return Err(
                "A cópia temporária não passou na validação final de schema e integridade.".into(),
            );
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = prepare_result {
        remove_restore_staging(&staging);
        return Err(error);
    }

    let pending = PendingRestore {
        token: token.clone(),
        backup_id: backup_id.into(),
        safety_backup_id: safety_backup.backup_id.clone(),
        staging,
        expected_database_sha256: manifest.database.sha256,
        schema_version: manifest.schema_version,
        prepared_at: SystemTime::now(),
    };
    let preparation = RestorePreparation {
        token,
        backup_id: backup_id.into(),
        safety_backup_id: safety_backup.backup_id,
        schema_version: manifest.schema_version,
        created_at: manifest.created_at,
        warnings: validation.warnings,
    };
    Ok((preparation, pending))
}

fn commit_restore_at(
    app_data: &Path,
    pending: PendingRestore,
) -> Result<RestoreCommitResult, String> {
    commit_restore_at_internal(app_data, pending, SwapFailurePoint::None)
}

fn commit_restore_at_internal(
    app_data: &Path,
    pending: PendingRestore,
    failure_point: SwapFailurePoint,
) -> Result<RestoreCommitResult, String> {
    if SystemTime::now()
        .duration_since(pending.prepared_at)
        .is_ok_and(|age| age > RESTORE_EXPIRATION)
    {
        remove_restore_staging(&pending.staging);
        return Err("A preparação expirou. Valide o backup novamente.".into());
    }

    let staged_database = pending.staging.join(DATABASE_FILE_NAME);
    let staged_hash = match hash_file(&staged_database) {
        Ok(hash) => hash,
        Err(error) => {
            remove_restore_staging(&pending.staging);
            return Err(error);
        }
    };
    if staged_hash != pending.expected_database_sha256 {
        remove_restore_staging(&pending.staging);
        return Err("O banco preparado foi alterado antes da restauração.".into());
    }
    let staged_health = match inspect_database(&staged_database) {
        Ok(health) => health,
        Err(error) => {
            remove_restore_staging(&pending.staging);
            return Err(error);
        }
    };
    if !staged_health.integrity_result.eq_ignore_ascii_case("ok")
        || staged_health.schema_version != pending.schema_version
    {
        remove_restore_staging(&pending.staging);
        return Err("O banco preparado deixou de ser íntegro ou compatível.".into());
    }

    let recovery_root = app_data.join("recovery");
    fs::create_dir_all(&recovery_root).map_err(|error| error.to_string())?;
    let rollback_id = format!(
        "restore-{}_{}",
        Utc::now().format("%Y-%m-%d_%H%M%S"),
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let rollback_directory = recovery_root.join(&rollback_id);
    fs::create_dir(&rollback_directory)
        .map_err(|error| format!("Não foi possível criar o ponto de rollback: {error}"))?;
    let rollback_manifest = RollbackManifest {
        format_version: 1,
        created_at: Utc::now().to_rfc3339(),
        source_backup_id: &pending.backup_id,
        safety_backup_id: &pending.safety_backup_id,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&rollback_manifest)
        .map_err(|error| format!("Não foi possível registrar o rollback: {error}"))?;
    fs::write(
        rollback_directory.join("restore-rollback.json"),
        manifest_bytes,
    )
    .map_err(|error| format!("Não foi possível registrar o rollback: {error}"))?;

    let active_database = app_data.join(DATABASE_FILE_NAME);
    let active_assets = app_data.join("assets");
    let staged_assets = pending.staging.join("assets");
    let mut progress = SwapProgress::default();
    let swap_result = (|| {
        fs::rename(&active_database, rollback_directory.join(DATABASE_FILE_NAME)).map_err(
            |error| {
                format!(
                    "Não foi possível retirar o banco ativo. Confirme que todas as conexões foram encerradas: {error}"
                )
            },
        )?;
        progress.original_database_moved = true;

        for sidecar in ["narrahub.db-wal", "narrahub.db-shm"] {
            let source = app_data.join(sidecar);
            if source.exists() {
                fs::rename(&source, rollback_directory.join(sidecar)).map_err(|error| {
                    format!("Não foi possível preservar o arquivo SQLite {sidecar}: {error}")
                })?;
                progress.moved_sidecars.push(sidecar.into());
            }
        }
        if active_assets.is_dir() {
            fs::rename(&active_assets, rollback_directory.join("assets"))
                .map_err(|error| format!("Não foi possível preservar os assets atuais: {error}"))?;
            progress.original_assets_moved = true;
        }

        if failure_point == SwapFailurePoint::AfterActiveMoved {
            return Err("Falha de teste após retirar a base ativa.".into());
        }

        fs::rename(&staged_database, &active_database)
            .map_err(|error| format!("Não foi possível instalar o banco restaurado: {error}"))?;
        progress.installed_database = true;
        if staged_assets.is_dir() {
            fs::rename(&staged_assets, &active_assets).map_err(|error| {
                format!("Não foi possível instalar os assets restaurados: {error}")
            })?;
            progress.installed_assets = true;
        }

        if matches!(
            failure_point,
            SwapFailurePoint::AfterInstall | SwapFailurePoint::AfterInstallWithBrokenRollback
        ) {
            return Err("Falha de teste após instalar a base restaurada.".into());
        }

        let installed_health = inspect_database(&active_database)?;
        if !installed_health.integrity_result.eq_ignore_ascii_case("ok")
            || installed_health.schema_version != pending.schema_version
            || hash_file(&active_database)? != pending.expected_database_sha256
        {
            return Err(
                "A base instalada falhou na verificação final; o rollback será aplicado.".into(),
            );
        }
        Ok::<(), String>(())
    })();

    if let Err(error) = swap_result {
        let rollback_error = rollback_swap(
            app_data,
            &rollback_directory,
            &active_database,
            &active_assets,
            &progress,
            failure_point,
        )
        .err();
        remove_restore_staging(&pending.staging);
        return match rollback_error {
            Some(rollback_error) => Err(format!(
                "{error} O rollback automático também falhou: {rollback_error}. Os arquivos preservados estão em {}.",
                rollback_directory.display()
            )),
            None => Err(format!("{error} A base anterior foi restaurada automaticamente.")),
        };
    }

    remove_restore_staging(&pending.staging);

    Ok(RestoreCommitResult {
        restored_backup_id: pending.backup_id,
        safety_backup_id: pending.safety_backup_id,
        rollback_id,
        schema_version: pending.schema_version,
        requires_restart: true,
    })
}

fn rollback_swap(
    app_data: &Path,
    rollback_directory: &Path,
    active_database: &Path,
    active_assets: &Path,
    progress: &SwapProgress,
    failure_point: SwapFailurePoint,
) -> Result<(), String> {
    // Falhar aqui, antes de mexer em qualquer arquivo, simula o caso real em que o
    // rollback não consegue devolver nada — disco cheio, arquivo travado por outro
    // processo, permissão negada. O que importa provar é que, mesmo assim, nada do
    // que foi preservado é destruído.
    if failure_point == SwapFailurePoint::AfterInstallWithBrokenRollback {
        return Err("Falha de teste ao desfazer a troca.".into());
    }
    if progress.installed_assets && active_assets.exists() {
        fs::remove_dir_all(active_assets).map_err(|error| error.to_string())?;
    }
    if progress.installed_database && active_database.exists() {
        fs::remove_file(active_database).map_err(|error| error.to_string())?;
    }
    if progress.original_database_moved {
        fs::rename(rollback_directory.join(DATABASE_FILE_NAME), active_database)
            .map_err(|error| error.to_string())?;
    }
    for sidecar in &progress.moved_sidecars {
        fs::rename(rollback_directory.join(sidecar), app_data.join(sidecar))
            .map_err(|error| error.to_string())?;
    }
    if progress.original_assets_moved {
        fs::rename(rollback_directory.join("assets"), active_assets)
            .map_err(|error| error.to_string())?;
    }
    fs::remove_file(rollback_directory.join("restore-rollback.json")).ok();
    fs::remove_dir(rollback_directory).ok();
    Ok(())
}

fn cleanup_restore_staging(app_data: &Path) -> Result<(), String> {
    for entry in fs::read_dir(app_data).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(".restore-")
            || !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
        {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= RESTORE_STAGING_RETENTION);
        if stale {
            fs::remove_dir_all(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn validate_migration_compatibility(database_path: &Path) -> Result<(), String> {
    let connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Não foi possível verificar as migrations do backup: {error}"))?;
    let has_migrations: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !has_migrations {
        return Ok(());
    }

    let mut statement = connection
        .prepare("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
        .map_err(|error| error.to_string())?;
    let applied = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in applied {
        let (version, checksum) = row.map_err(|error| error.to_string())?;
        let sql = sql_for_version(version).ok_or_else(|| {
            format!(
                "O backup registra a migration {version}, que não existe nesta versão do NarraHub."
            )
        })?;
        let expected = Sha384::digest(sql.as_bytes());
        if checksum.as_slice() != expected.as_slice() {
            return Err(format!(
                "A migration {version} do backup tem checksum incompatível. A restauração foi bloqueada antes de alterar a base ativa."
            ));
        }
    }
    Ok(())
}

fn remove_restore_staging(path: &Path) {
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(".restore-"))
    {
        fs::remove_dir_all(path).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::backup::{create_backup_at, BackupReason};
    use crate::database::migrations::MIGRATION_V1;
    use rusqlite::{params, Connection};
    use std::io::Write;

    struct TestAppData {
        root: PathBuf,
    }

    impl TestAppData {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("narrahub-restore-{}", Uuid::new_v4()));
            fs::create_dir_all(&root).expect("create app data");
            let connection = Connection::open(root.join(DATABASE_FILE_NAME)).expect("create db");
            connection
                .execute_batch(MIGRATION_V1)
                .expect("apply schema");
            connection
                .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
                .expect("set schema");
            Self { root }
        }

        fn database(&self) -> PathBuf {
            self.root.join(DATABASE_FILE_NAME)
        }

        fn insert_universe(&self, id: &str, name: &str) {
            Connection::open(self.database())
                .expect("open db")
                .execute(
                    "INSERT INTO universes (id, name) VALUES (?1, ?2)",
                    params![id, name],
                )
                .expect("insert universe");
        }

        /// Cria os arquivos que o rollback precisa devolver além do banco.
        ///
        /// Um `-wal` avulso ao lado de um banco que não está em modo WAL é removido
        /// pelo próprio SQLite na primeira abertura. Por isso os sidecars são escritos
        /// depois da preparação, e por isso as asserções sobre eles vêm **antes** de
        /// qualquer leitura do banco: `universe_names()` abriria a conexão e apagaria
        /// o arquivo que o teste quer conferir.
        fn with_sidecars_and_asset(&self) -> &Self {
            for sidecar in ["narrahub.db-wal", "narrahub.db-shm"] {
                fs::write(self.root.join(sidecar), b"sidecar original").expect("write sidecar");
            }
            let assets = self.root.join("assets");
            fs::create_dir_all(&assets).expect("create assets");
            fs::write(assets.join("capa.webp"), b"asset original").expect("write asset");
            self
        }

        /// `integrity_check` e `foreign_key_check` no banco ativo. Conferir só os
        /// nomes dos universos prova que o arquivo voltou, não que ele voltou são.
        fn assert_healthy(&self) {
            let connection = Connection::open(self.database()).expect("open db");
            let integrity: String = connection
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .expect("integrity_check");
            assert_eq!(integrity, "ok", "banco ativo corrompido após o rollback");
            let violations: i64 = connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .expect("foreign_key_check");
            assert_eq!(violations, 0, "foreign keys quebradas após o rollback");
        }

        /// Nada de staging nem de ponto de rollback pode sobrar: o que sobra hoje
        /// vira lixo que a próxima restauração encontra pela frente.
        fn assert_no_leftovers(&self) {
            for entry in fs::read_dir(&self.root).expect("read app data") {
                let name = entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string();
                assert!(
                    !name.starts_with(".restore-"),
                    "staging de restauração ficou para trás: {name}"
                );
            }
            let recovery = self.root.join("recovery");
            if recovery.is_dir() {
                let pontos = fs::read_dir(&recovery).expect("read recovery").count();
                assert_eq!(pontos, 0, "o ponto de rollback não foi limpo após desfazer");
            }
        }

        /// Confere que o rollback devolveu tudo o que retirou junto do banco.
        /// Precisa rodar antes de qualquer abertura do SQLite.
        fn assert_sidecars_and_asset_restored(&self) {
            for relative in ["narrahub.db-wal", "narrahub.db-shm", "assets/capa.webp"] {
                let conteudo = fs::read(self.root.join(relative))
                    .unwrap_or_else(|error| panic!("o rollback não devolveu {relative}: {error}"));
                let esperado: &[u8] = if relative.starts_with("assets/") {
                    b"asset original"
                } else {
                    b"sidecar original"
                };
                assert_eq!(conteudo, esperado, "{relative} voltou com conteúdo errado");
            }
        }

        fn universe_names(&self) -> Vec<String> {
            let connection = Connection::open(self.database()).expect("open db");
            let mut statement = connection
                .prepare("SELECT name FROM universes ORDER BY name")
                .expect("prepare");
            statement
                .query_map([], |row| row.get(0))
                .expect("query")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect")
        }
    }

    impl Drop for TestAppData {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    #[test]
    fn restore_round_trip_preserves_current_state_and_installs_selected_backup() {
        let app = TestAppData::new();
        app.insert_universe("u1", "Estado do backup");
        fs::create_dir_all(app.root.join("assets")).expect("create assets");
        fs::write(app.root.join("assets/cover.bin"), b"old-cover").expect("write asset");
        let selected = create_backup_at(
            &app.database(),
            Some(&app.root.join("assets")),
            &app.root.join("backups"),
            "test",
            BackupReason::Manual,
        )
        .expect("create selected backup");

        app.insert_universe("u2", "Estado atual");
        fs::write(app.root.join("assets/cover.bin"), b"current-cover").expect("update asset");
        let (prepared, pending) =
            prepare_restore_at(&app.root, &selected.backup_id, "test").expect("prepare restore");
        let result = commit_restore_at(&app.root, pending).expect("commit restore");

        assert_eq!(app.universe_names(), vec!["Estado do backup"]);
        assert_eq!(
            fs::read(app.root.join("assets/cover.bin")).unwrap(),
            b"old-cover"
        );
        assert_eq!(result.safety_backup_id, prepared.safety_backup_id);
        let rollback = app.root.join("recovery").join(result.rollback_id);
        let rollback_connection =
            Connection::open(rollback.join(DATABASE_FILE_NAME)).expect("open rollback");
        let current_count: i64 = rollback_connection
            .query_row("SELECT COUNT(*) FROM universes", [], |row| row.get(0))
            .expect("count rollback rows");
        assert_eq!(current_count, 2);
        assert_eq!(
            fs::read(rollback.join("assets/cover.bin")).unwrap(),
            b"current-cover"
        );
    }

    #[test]
    fn invalid_backup_never_creates_a_restore_staging_or_changes_active_database() {
        let app = TestAppData::new();
        app.insert_universe("u1", "Canônico");
        let selected = create_backup_at(
            &app.database(),
            None,
            &app.root.join("backups"),
            "test",
            BackupReason::Manual,
        )
        .expect("create backup");
        fs::OpenOptions::new()
            .append(true)
            .open(
                app.root
                    .join("backups")
                    .join(&selected.backup_id)
                    .join(DATABASE_FILE_NAME),
            )
            .expect("open backup")
            .write_all(b"tamper")
            .expect("tamper backup");

        let error = prepare_restore_at(&app.root, &selected.backup_id, "test")
            .expect_err("reject invalid backup");
        assert!(error.contains("não é restaurável"));
        assert_eq!(app.universe_names(), vec!["Canônico"]);
        assert!(fs::read_dir(&app.root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".restore-")));
    }

    /// Prepara uma restauração real e injeta a falha no ponto pedido, devolvendo a
    /// mensagem de erro. O cenário é sempre o mesmo — um backup antigo e uma base
    /// atual com conteúdo a mais — para que o rollback tenha algo concreto a perder.
    fn restore_falhando_em(ponto: SwapFailurePoint) -> (TestAppData, String) {
        let app = TestAppData::new();
        app.insert_universe("u1", "Backup");
        let selected = create_backup_at(
            &app.database(),
            None,
            &app.root.join("backups"),
            "test",
            BackupReason::Manual,
        )
        .expect("create backup");
        app.insert_universe("u2", "Atual");
        let (_, pending) =
            prepare_restore_at(&app.root, &selected.backup_id, "test").expect("prepare restore");
        // Depois da preparação: é a última coisa que abre o SQLite antes da troca.
        app.with_sidecars_and_asset();

        let error = commit_restore_at_internal(&app.root, pending, ponto)
            .expect_err("a falha injetada precisa abortar a restauração");
        (app, error)
    }

    #[test]
    fn failed_swap_restores_the_active_database() {
        let (app, error) = restore_falhando_em(SwapFailurePoint::AfterInstall);

        assert!(error.contains("base anterior foi restaurada"));
        // Antes de abrir o banco: ver o comentário em `with_sidecars_and_asset`.
        app.assert_sidecars_and_asset_restored();
        app.assert_no_leftovers();
        assert_eq!(app.universe_names(), vec!["Atual", "Backup"]);
        app.assert_healthy();
    }

    /// O caso que o teste anterior não alcançava: a falha acontece com o banco ativo
    /// já retirado e o restaurado ainda não instalado. É o único momento em que o
    /// usuário fica sem banco nenhum no disco, e o rollback precisa devolver tudo o
    /// que saiu — arquivo principal, WAL, SHM e assets.
    #[test]
    fn falha_antes_de_instalar_devolve_banco_sidecars_e_assets() {
        let (app, error) = restore_falhando_em(SwapFailurePoint::AfterActiveMoved);

        assert!(
            error.contains("base anterior foi restaurada"),
            "o rollback precisa ter sido aplicado, e não apenas relatado: {error}"
        );
        app.assert_sidecars_and_asset_restored();
        app.assert_no_leftovers();
        assert_eq!(app.universe_names(), vec!["Atual", "Backup"]);
        app.assert_healthy();
    }

    /// O pior cenário do produto: a restauração falha **e** o rollback falha junto.
    ///
    /// O usuário fica sem a base nova e sem a antiga no lugar de sempre. A única coisa
    /// entre ele e a perda do livro é a mensagem de erro apontar para onde os arquivos
    /// foram preservados — então é isso que este teste cobra, junto da garantia de que
    /// a tentativa de limpeza não apaga justamente o que sobrou.
    #[test]
    fn rollback_falho_preserva_os_arquivos_e_diz_onde_eles_estao() {
        let (app, error) = restore_falhando_em(SwapFailurePoint::AfterInstallWithBrokenRollback);

        assert!(
            error.contains("rollback automático também falhou"),
            "o usuário precisa saber que o rollback não deu conta: {error}"
        );

        // A mensagem tem que nomear o diretório, e o diretório tem que existir de fato.
        let recovery_root = app.root.join("recovery");
        let preservado = fs::read_dir(&recovery_root)
            .expect("o diretório de recuperação precisa existir")
            .map(|entry| entry.expect("entry").path())
            .next()
            .expect("o ponto de rollback não pode ter sido apagado");
        let nome = preservado
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            error.contains(&nome),
            "a mensagem precisa nomear o diretório preservado ({nome}): {error}"
        );

        // E o que está lá dentro precisa ser a base anterior, íntegra — não um resto.
        let banco_preservado = preservado.join(DATABASE_FILE_NAME);
        assert!(
            banco_preservado.exists(),
            "a base anterior tem que continuar no ponto de rollback"
        );
        let connection = Connection::open(&banco_preservado).expect("abrir base preservada");
        let integridade: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity_check");
        assert_eq!(
            integridade, "ok",
            "a base preservada não pode estar corrompida"
        );
        let nomes: Vec<String> = connection
            .prepare("SELECT name FROM universes ORDER BY name")
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("collect");
        assert_eq!(
            nomes,
            vec!["Atual".to_string(), "Backup".to_string()],
            "o conteúdo anterior tem que estar inteiro no ponto de rollback"
        );

        // O manifesto é o que permite entender depois o que aconteceu ali.
        assert!(
            preservado.join("restore-rollback.json").exists(),
            "o manifesto do rollback não pode ser apagado quando o rollback falha"
        );
    }

    /// O backup de segurança é criado na preparação, antes de qualquer troca. Se ele
    /// sumisse quando a restauração falha, o usuário perderia a única cópia do estado
    /// que tinha antes de tentar restaurar.
    #[test]
    fn falha_na_restauracao_preserva_o_backup_de_seguranca() {
        let (app, _) = restore_falhando_em(SwapFailurePoint::AfterActiveMoved);

        assert_eq!(
            list_pre_restore_backups(&app.root.join("backups")),
            1,
            "o backup pré-restauração precisa sobreviver à falha"
        );
    }

    #[test]
    fn backup_from_a_newer_schema_is_rejected_before_safety_backup() {
        let app = TestAppData::new();
        Connection::open(app.database())
            .expect("open db")
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION + 1)
            .expect("set future schema");
        let selected = create_backup_at(
            &app.database(),
            None,
            &app.root.join("backups"),
            "future-test",
            BackupReason::Manual,
        )
        .expect("create future backup");
        Connection::open(app.database())
            .expect("open db")
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
            .expect("restore supported schema");

        let error = prepare_restore_at(&app.root, &selected.backup_id, "test")
            .expect_err("reject future schema");
        assert!(error.contains("Atualize o NarraHub"));
        assert_eq!(
            list_pre_restore_backups(&app.root.join("backups")),
            0,
            "schema rejection must happen before creating a safety backup"
        );
    }

    #[test]
    fn modified_applied_migration_is_rejected_before_restore() {
        let app = TestAppData::new();
        let connection = Connection::open(app.database()).expect("open db");
        connection
            .execute_batch(
                "CREATE TABLE _sqlx_migrations (
                    version BIGINT PRIMARY KEY,
                    description TEXT NOT NULL,
                    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    success BOOLEAN NOT NULL,
                    checksum BLOB NOT NULL,
                    execution_time BIGINT NOT NULL
                );",
            )
            .expect("create migrations table");
        connection
            .execute(
                "INSERT INTO _sqlx_migrations
                 (version, description, success, checksum, execution_time)
                 VALUES (1, 'modified migration', 1, ?1, 0)",
                [b"wrong-checksum".as_slice()],
            )
            .expect("insert incompatible checksum");
        drop(connection);
        let selected = create_backup_at(
            &app.database(),
            None,
            &app.root.join("backups"),
            "test",
            BackupReason::Manual,
        )
        .expect("create incompatible backup");

        let error = prepare_restore_at(&app.root, &selected.backup_id, "test")
            .expect_err("reject modified migration");
        assert!(error.contains("checksum incompatível"));
        assert_eq!(list_pre_restore_backups(&app.root.join("backups")), 0);
    }

    fn list_pre_restore_backups(backups_root: &Path) -> usize {
        crate::database::backup::list_backups_at(backups_root)
            .expect("list backups")
            .into_iter()
            .filter(|backup| backup.reason == BackupReason::PreRestore)
            .count()
    }
}
