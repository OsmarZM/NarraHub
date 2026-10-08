//! Pacote portátil: lista explícita de arquivos, limites e validação antes de publicar.
use super::backup::{hash_file, validate_backup_at, BackupManifest};
use super::migrations::LATEST_SCHEMA_VERSION;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

pub const MAX_PACKAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 50_000;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const PACKAGE_FORMAT: &str = "narrahub-backup-v1";

fn invalid(message: impl std::fmt::Display) -> String {
    format!("Backup externo inválido: {message}")
}

pub fn safe_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 1024
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
    {
        return Err(invalid("caminho não permitido"));
    }
    for component in name.split('/') {
        let base = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with(['.', ' '])
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&base.as_str())
        {
            return Err(invalid("caminho não permitido"));
        }
    }
    Ok(())
}

fn expected_files(manifest: &BackupManifest) -> Result<HashMap<String, u64>, String> {
    if manifest.format_version != 1
        || manifest.database.file != "narrahub.db"
        || manifest.schema_version > LATEST_SCHEMA_VERSION
    {
        return Err(invalid(
            "formato ou schema não suportado; atualize o NarraHub",
        ));
    }
    let mut expected = HashMap::from([("narrahub.db".into(), manifest.database.size_bytes)]);
    let mut folded = HashSet::from(["narrahub.db".to_string(), "manifest.json".to_string()]);
    let mut total = manifest.database.size_bytes;
    if manifest.assets.files.len() + 2 > MAX_ENTRIES {
        return Err(invalid("arquivos demais"));
    }
    for asset in &manifest.assets.files {
        safe_name(&asset.path)?;
        let name = format!("assets/{}", asset.path);
        if !folded.insert(name.to_lowercase()) {
            return Err(invalid("arquivo duplicado"));
        }
        total = total
            .checked_add(asset.size_bytes)
            .ok_or_else(|| invalid("tamanho excessivo"))?;
        expected.insert(name, asset.size_bytes);
    }
    if total > MAX_PACKAGE_BYTES {
        return Err(invalid("limite de 8 GB excedido"));
    }
    Ok(expected)
}

/// A sanitização acontece exclusivamente no snapshot de exportação, nunca na base ativa.
pub fn sanitize_snapshot(directory: &Path, manifest: &mut BackupManifest) -> Result<(), String> {
    let path = directory.join("narrahub.db");
    {
        let db = Connection::open(&path).map_err(invalid)?;
        let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='collaboration_sessions')", [], |row| row.get(0)).map_err(invalid)?;
        if exists {
            db.execute_batch("PRAGMA secure_delete=ON; UPDATE collaboration_sessions SET encryption_key='', revoke_token='', status='ended', ended_at=COALESCE(ended_at, datetime('now')); PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE; VACUUM;").map_err(invalid)?;
        }
    }
    manifest.database.sha256 = hash_file(&path)?;
    manifest.database.size_bytes = fs::metadata(&path).map_err(invalid)?.len();
    super::duravel::gravar_atomico(
        &directory.join("manifest.json"),
        &serde_json::to_vec_pretty(manifest).map_err(invalid)?,
    )
    .map_err(invalid)
}

pub fn write_package(
    directory: &Path,
    destination: &Path,
    manifest: &BackupManifest,
) -> Result<(), String> {
    let expected = expected_files(manifest)?;
    let file = File::create(destination).map_err(invalid)?;
    let mut archive = ZipWriter::new(file);
    archive.set_comment(PACKAGE_FORMAT);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let mut names: Vec<_> = expected.keys().cloned().collect();
    names.push("manifest.json".into());
    names.sort();
    for name in names {
        let source = directory.join(&name);
        if fs::symlink_metadata(&source)
            .map_err(invalid)?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("link simbólico"));
        }
        archive.start_file(&name, options).map_err(invalid)?;
        io::copy(&mut File::open(source).map_err(invalid)?, &mut archive).map_err(invalid)?;
    }
    archive
        .finish()
        .map_err(invalid)?
        .sync_all()
        .map_err(invalid)
}

/// Extrai somente a lista declarada num diretório recém-criado, sem tocar no acervo.
pub fn unpack<R: Read + Seek>(reader: R, destination: &Path) -> Result<BackupManifest, String> {
    let mut archive = ZipArchive::new(reader).map_err(invalid)?;
    if archive.comment() != PACKAGE_FORMAT.as_bytes() || archive.len() > MAX_ENTRIES {
        return Err(invalid("formato não reconhecido"));
    }
    let manifest: BackupManifest = {
        let entry = archive.by_name("manifest.json").map_err(invalid)?;
        if entry.size() > MAX_MANIFEST_BYTES {
            return Err(invalid("manifesto excessivo"));
        }
        serde_json::from_reader(entry.take(MAX_MANIFEST_BYTES + 1)).map_err(invalid)?
    };
    let mut expected = expected_files(&manifest)?;
    expected.insert("manifest.json".into(), MAX_MANIFEST_BYTES);
    if archive.len() != expected.len() {
        return Err(invalid("lista de arquivos diverge do manifesto"));
    }
    let mut seen = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(invalid)?;
        safe_name(entry.name())?;
        if entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || !seen.insert(entry.name().to_lowercase())
        {
            return Err(invalid("link ou arquivo duplicado"));
        }
        let size = expected
            .get(entry.name())
            .ok_or_else(|| invalid("arquivo não permitido"))?;
        if (entry.name() != "manifest.json" && entry.size() != *size) || entry.size() > *size {
            return Err(invalid("tamanho diverge do manifesto"));
        }
    }
    fs::create_dir(destination).map_err(invalid)?;
    let result = (|| {
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(invalid)?;
            let path = destination.join(entry.name());
            fs::create_dir_all(
                path.parent()
                    .ok_or_else(|| invalid("caminho sem diretório"))?,
            )
            .map_err(invalid)?;
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(invalid)?;
            let size = entry.size();
            let copied = io::copy(&mut (&mut entry).take(size + 1), &mut file).map_err(invalid)?;
            if copied != size {
                return Err(invalid("arquivo truncado ou expandido"));
            }
            file.flush().map_err(invalid)?;
            file.sync_all().map_err(invalid)?;
        }
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

pub fn import_package(reader: impl Read + Seek, backups: &Path) -> Result<BackupManifest, String> {
    fs::create_dir_all(backups).map_err(invalid)?;
    let stage = backups.join(format!(".tmp-import-{}", uuid::Uuid::new_v4()));
    let mut manifest = unpack(reader, &stage)?;
    let result = (|| {
        manifest.backup_id = format!("import-{}", uuid::Uuid::new_v4().simple());
        // Não tornar uma importação manual elegível à retenção dos snapshots automáticos.
        manifest.reason = super::backup::BackupReason::Manual;
        super::duravel::gravar_atomico(
            &stage.join("manifest.json"),
            &serde_json::to_vec_pretty(&manifest).map_err(invalid)?,
        )
        .map_err(invalid)?;
        let validation_root = backups.join(format!(".tmp-check-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&validation_root).map_err(invalid)?;
        let inside = validation_root.join(&manifest.backup_id);
        fs::rename(&stage, &inside).map_err(invalid)?;
        let validated = validate_backup_at(&validation_root, &manifest.backup_id);
        let publish = (|| {
            let validation = validated?;
            if validation
                .database_health
                .as_ref()
                .is_some_and(|health| health.foreign_key_violations != 0)
            {
                return Err(invalid("relações quebradas no banco (foreign keys)"));
            }
            if !validation.valid || validation.database_health.is_none() {
                return Err(invalid(validation.errors.join(" ")));
            }
            sanitize_snapshot(&inside, &mut manifest)?;
            let sanitized = validate_backup_at(&validation_root, &manifest.backup_id)?;
            if !sanitized.valid {
                return Err(invalid(sanitized.errors.join(" ")));
            }
            fs::rename(&inside, backups.join(&manifest.backup_id)).map_err(invalid)?;
            super::duravel::sincronizar_diretorio(backups).map_err(invalid)?;
            Ok(manifest.clone())
        })();
        let _ = fs::remove_dir_all(&validation_root);
        publish
    })();
    let _ = fs::remove_dir_all(&stage);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::backup::{create_backup_at, BackupReason};
    use crate::database::migrations::sql_for_version;
    use std::io::Cursor;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("nh-portable-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let db = Connection::open(root.join("narrahub.db")).unwrap();
            for version in 1..=LATEST_SCHEMA_VERSION {
                db.execute_batch(sql_for_version(version).unwrap()).unwrap();
            }
            db.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
                .unwrap();
            Self(root)
        }
        fn snapshot(&self) -> BackupManifest {
            create_backup_at(
                &self.0.join("narrahub.db"),
                Some(&self.0.join("assets")),
                &self.0.join("backups"),
                "test",
                BackupReason::Manual,
            )
            .unwrap()
        }
        fn zip(&self, manifest: &BackupManifest) -> PathBuf {
            let path = self.0.join("portable.narrahub-backup");
            write_package(
                &self.0.join("backups").join(&manifest.backup_id),
                &path,
                manifest,
            )
            .unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn wal_roundtrip_includes_saved_text_and_assets_but_excludes_private_identity() {
        let f = Fixture::new();
        let db = Connection::open(f.0.join("narrahub.db")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; INSERT INTO universes(id,name) VALUES('u','Texto salvo no WAL');").unwrap();
        fs::create_dir_all(f.0.join("assets/blobs")).unwrap();
        fs::write(f.0.join("assets/blobs/image.webp"), b"imagem-de-teste").unwrap();
        fs::write(
            f.0.join("sync-identity.json"),
            b"PRIVATE-IDENTITY-MUST-NOT-EXPORT",
        )
        .unwrap();
        let m = f.snapshot();
        let path = f.zip(&m);
        let mut zip = ZipArchive::new(File::open(&path).unwrap()).unwrap();
        assert!(zip.by_name("sync-identity.json").is_err());
        let imported =
            import_package(File::open(path).unwrap(), &f.0.join("fresh/backups")).unwrap();
        let restored = f.0.join("fresh/backups").join(&imported.backup_id);
        let value: String = Connection::open(restored.join("narrahub.db"))
            .unwrap()
            .query_row("SELECT name FROM universes WHERE id='u'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "Texto salvo no WAL");
        assert_eq!(
            fs::read(restored.join("assets/blobs/image.webp")).unwrap(),
            b"imagem-de-teste"
        );
        assert!(
            validate_backup_at(&f.0.join("fresh/backups"), &imported.backup_id)
                .unwrap()
                .valid
        );
    }

    #[test]
    fn portable_copy_removes_sharing_credentials_without_touching_active_database() {
        let f = Fixture::new();
        let db = Connection::open(f.0.join("narrahub.db")).unwrap();
        db.execute_batch("INSERT INTO collaboration_sessions(id,title,permission,universe_ids,encryption_key,revoke_token,status,created_at,expires_at) VALUES('s','Histórico','view','[]','SECRET-ENCRYPTION-MARKER','SECRET-REVOKE-MARKER','active','2026-01-01','2099-01-01');").unwrap();
        let mut m = f.snapshot();
        let snapshot = f.0.join("backups").join(&m.backup_id);
        sanitize_snapshot(&snapshot, &mut m).unwrap();
        let bytes = fs::read(snapshot.join("narrahub.db")).unwrap();
        for secret in [
            b"SECRET-ENCRYPTION-MARKER".as_slice(),
            b"SECRET-REVOKE-MARKER".as_slice(),
        ] {
            assert!(!bytes.windows(secret.len()).any(|window| window == secret));
        }
        let source: String = db
            .query_row(
                "SELECT encryption_key FROM collaboration_sessions WHERE id='s'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(source, "SECRET-ENCRYPTION-MARKER");
        let state: String = Connection::open(snapshot.join("narrahub.db"))
            .unwrap()
            .query_row(
                "SELECT status FROM collaboration_sessions WHERE id='s'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "ended");
        assert!(
            validate_backup_at(&f.0.join("backups"), &m.backup_id)
                .unwrap()
                .valid
        );
    }

    #[test]
    fn unsafe_names_and_windows_aliases_are_rejected() {
        for path in [
            "../escape",
            "/absolute",
            "C:/file",
            "a\\b",
            "a/./b",
            "CON.txt",
            "a/nul",
            "name.",
            "a/ ",
            "a//b",
            "a/COM1.bin",
            "a\0b",
        ] {
            assert!(safe_name(path).is_err(), "{path}");
        }
        assert!(safe_name("blobs/capa-ação.webp").is_ok());
    }

    fn hostile_zip(manifest: &BackupManifest, extra: &str, symlink: bool) -> Cursor<Vec<u8>> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.set_comment(PACKAGE_FORMAT);
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(manifest).unwrap())
            .unwrap();
        if symlink {
            zip.add_symlink(extra, "../../outside", SimpleFileOptions::default())
                .unwrap();
        } else {
            zip.start_file(extra, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap()
    }

    #[test]
    fn rejects_traversal_symlinks_extra_entries_and_future_schema_before_extraction() {
        let f = Fixture::new();
        let m = f.snapshot();
        for (name, link) in [
            ("../outside", false),
            ("narrahub.db", true),
            ("sync-identity.json", false),
        ] {
            let destination = f.0.join("unpack");
            assert!(unpack(hostile_zip(&m, name, link), &destination).is_err());
            assert!(!destination.exists());
        }
        let mut future = m.clone();
        future.schema_version = LATEST_SCHEMA_VERSION + 1;
        assert!(expected_files(&future).is_err());
        future = m;
        future.database.size_bytes = MAX_PACKAGE_BYTES + 1;
        assert!(expected_files(&future).is_err());
    }

    #[test]
    fn corruption_never_publishes_an_imported_backup() {
        let f = Fixture::new();
        let m = f.snapshot();
        fs::write(
            f.0.join("backups").join(&m.backup_id).join("narrahub.db"),
            vec![0_u8; m.database.size_bytes as usize],
        )
        .unwrap();
        let path = f.zip(&m);
        let imported = f.0.join("imports");
        assert!(import_package(File::open(path).unwrap(), &imported).is_err());
        assert_eq!(fs::read_dir(&imported).unwrap().count(), 0);
    }

    #[test]
    fn foreign_key_violations_are_refused_even_when_internal_snapshot_preserves_them() {
        let f = Fixture::new();
        let db = Connection::open(f.0.join("narrahub.db")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO stories(id,universe_id,name) VALUES('s','missing','Órfã');").unwrap();
        let m = f.snapshot();
        assert!(
            validate_backup_at(&f.0.join("backups"), &m.backup_id)
                .unwrap()
                .valid
        );
        let archive = f.zip(&m);
        let imports = f.0.join("imports");
        assert!(import_package(File::open(archive).unwrap(), &imports)
            .unwrap_err()
            .contains("foreign keys"));
        assert_eq!(fs::read_dir(imports).unwrap().count(), 0);
    }

    #[test]
    fn missing_asset_duplicate_case_and_truncated_zip_are_rejected() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("assets")).unwrap();
        fs::write(f.0.join("assets/image"), b"image").unwrap();
        let mut m = f.snapshot();
        assert!(unpack(hostile_zip(&m, "narrahub.db", false), &f.0.join("missing")).is_err());
        let mut duplicate = m.assets.files[0].clone();
        duplicate.path = "IMAGE".into();
        m.assets.files.push(duplicate);
        assert!(expected_files(&m).is_err());
        assert!(unpack(Cursor::new(b"PK truncated"), &f.0.join("truncated")).is_err());
    }
}
