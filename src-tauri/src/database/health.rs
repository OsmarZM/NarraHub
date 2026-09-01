use super::error::{DatabaseCommandError, DatabaseCommandResult};
use super::migrations::LATEST_SCHEMA_VERSION;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;

const COUNTED_TABLES: &[&str] = &[
    "universes",
    "stories",
    "books",
    "chapters",
    "entities",
    "relations",
    "mentions",
    "attachments",
    "collaboration_contributions",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseHealthIssue {
    pub code: String,
    pub message: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseHealthReport {
    pub healthy: bool,
    pub checked_at: String,
    pub schema_version: i64,
    pub integrity_result: String,
    pub foreign_key_violations: u64,
    pub issues: Vec<DatabaseHealthIssue>,
    pub table_counts: BTreeMap<String, u64>,
}

/// Resposta do portão de compatibilidade que roda antes de abrir o pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseCompatibility {
    pub database_exists: bool,
    pub schema_version: i64,
    pub supported_schema_version: i64,
    pub compatible: bool,
}

/// Diz se este executável entende o banco que está no disco — **sem abri-lo para escrita**.
///
/// Existe separado de `database_health` por dois motivos que importam num portão de boot:
/// ele não roda `integrity_check`, `foreign_key_check` nem as consultas de invariante, que
/// são caras num banco de vários MB; e ele **não falha quando o banco ainda não existe**,
/// que é o primeiro boot de qualquer instalação nova.
#[tauri::command]
pub fn database_compatibility(app: AppHandle) -> DatabaseCommandResult<DatabaseCompatibility> {
    let path = super::app_database_path(&app).map_err(DatabaseCommandError::unavailable)?;
    inspect_compatibility(&path).map_err(DatabaseCommandError::storage)
}

pub fn inspect_compatibility(path: &Path) -> Result<DatabaseCompatibility, String> {
    if !path.is_file() {
        // Instalação nova: não há o que ser incompatível, e o plugin vai criar o banco.
        return Ok(DatabaseCompatibility {
            database_exists: false,
            schema_version: 0,
            supported_schema_version: LATEST_SCHEMA_VERSION,
            compatible: true,
        });
    }

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Não foi possível ler a versão do banco local: {error}"))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    let schema_version = detect_schema_version(&connection)?;

    Ok(DatabaseCompatibility {
        database_exists: true,
        schema_version,
        supported_schema_version: LATEST_SCHEMA_VERSION,
        // Banco mais antigo é compatível: as migrations sobem. Mais novo, não: este
        // executável não conhece as colunas que ele tem, e escrever ali estragaria dado.
        compatible: schema_version <= LATEST_SCHEMA_VERSION,
    })
}

#[tauri::command]
pub fn database_health(app: AppHandle) -> DatabaseCommandResult<DatabaseHealthReport> {
    let path = super::app_database_path(&app).map_err(DatabaseCommandError::unavailable)?;
    inspect_database(&path).map_err(DatabaseCommandError::storage)
}

pub fn inspect_database(path: &Path) -> Result<DatabaseHealthReport, String> {
    if !path.is_file() {
        return Err(format!("Banco local não encontrado em {}.", path.display()));
    }

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Não foi possível abrir o banco para diagnóstico: {error}"))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;

    let integrity_rows = collect_first_column(&connection, "PRAGMA integrity_check")?;
    let integrity_result = integrity_rows.join("; ");
    let integrity_ok = integrity_rows.len() == 1 && integrity_rows[0].eq_ignore_ascii_case("ok");
    let foreign_key_violations = count_query_rows(&connection, "PRAGMA foreign_key_check")?;
    let schema_version = detect_schema_version(&connection)?;
    let mut issues = inspect_domain_invariants(&connection)?;

    if !integrity_ok {
        issues.push(DatabaseHealthIssue {
            code: "sqlite_integrity".into(),
            message: "O SQLite encontrou falhas de integridade física ou estrutural.".into(),
            count: integrity_rows.len() as u64,
        });
    }
    if foreign_key_violations > 0 {
        issues.push(DatabaseHealthIssue {
            code: "foreign_key_violation".into(),
            message: "Existem referências que violam foreign keys.".into(),
            count: foreign_key_violations,
        });
    }

    let table_counts = collect_table_counts(&connection)?;
    Ok(DatabaseHealthReport {
        healthy: integrity_ok && foreign_key_violations == 0 && issues.is_empty(),
        checked_at: chrono::Utc::now().to_rfc3339(),
        schema_version,
        integrity_result,
        foreign_key_violations,
        issues,
        table_counts,
    })
}

fn inspect_domain_invariants(connection: &Connection) -> Result<Vec<DatabaseHealthIssue>, String> {
    let checks = [
        (
            "chapter_without_book",
            "Há capítulos sem um livro existente.",
            "SELECT COUNT(*) FROM chapters c LEFT JOIN books b ON b.id = c.book_id WHERE b.id IS NULL",
        ),
        (
            "book_without_story",
            "Há livros sem uma história existente.",
            "SELECT COUNT(*) FROM books b LEFT JOIN stories s ON s.id = b.story_id WHERE s.id IS NULL",
        ),
        (
            "story_without_universe",
            "Há histórias sem um universo existente.",
            "SELECT COUNT(*) FROM stories s LEFT JOIN universes u ON u.id = s.universe_id WHERE u.id IS NULL",
        ),
        (
            "entity_without_universe",
            "Há entidades sem um universo existente.",
            "SELECT COUNT(*) FROM entities e LEFT JOIN universes u ON u.id = e.universe_id WHERE u.id IS NULL",
        ),
        (
            "relation_without_endpoint",
            "Há relações apontando para entidades inexistentes.",
            "SELECT COUNT(*) FROM relations r LEFT JOIN entities source ON source.id = r.source_id LEFT JOIN entities target ON target.id = r.target_id WHERE source.id IS NULL OR target.id IS NULL",
        ),
        (
            "relation_crosses_universe",
            "Há relações cujas entidades não pertencem ao universo da relação.",
            "SELECT COUNT(*) FROM relations r JOIN entities source ON source.id = r.source_id JOIN entities target ON target.id = r.target_id WHERE source.universe_id <> r.universe_id OR target.universe_id <> r.universe_id",
        ),
        (
            "mention_without_target",
            "Há menções sem capítulo ou entidade existente.",
            "SELECT COUNT(*) FROM mentions m LEFT JOIN chapters c ON c.id = m.chapter_id LEFT JOIN entities e ON e.id = m.entity_id WHERE c.id IS NULL OR e.id IS NULL",
        ),
    ];

    let mut issues = Vec::new();
    for (code, message, sql) in checks {
        let count = query_count(connection, sql)?;
        if count > 0 {
            issues.push(DatabaseHealthIssue {
                code: code.into(),
                message: message.into(),
                count,
            });
        }
    }
    Ok(issues)
}

fn detect_schema_version(connection: &Connection) -> Result<i64, String> {
    if table_exists(connection, "_sqlx_migrations")? {
        return connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string());
    }
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn collect_table_counts(connection: &Connection) -> Result<BTreeMap<String, u64>, String> {
    let mut counts = BTreeMap::new();
    for table in COUNTED_TABLES {
        if table_exists(connection, table)? {
            counts.insert(
                (*table).into(),
                query_count(connection, &format!("SELECT COUNT(*) FROM {table}"))?,
            );
        }
    }
    Ok(counts)
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn query_count(connection: &Connection, sql: &str) -> Result<u64, String> {
    connection
        .query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|value| value.max(0) as u64)
        .map_err(|error| error.to_string())
}

fn count_query_rows(connection: &Connection, sql: &str) -> Result<u64, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    let mut count = 0_u64;
    while rows.next().map_err(|error| error.to_string())?.is_some() {
        count += 1;
    }
    Ok(count)
}

fn collect_first_column(connection: &Connection, sql: &str) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let values = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::MIGRATION_V1;
    use rusqlite::params;
    use uuid::Uuid;

    fn test_database() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("narrahub-health-{}.db", Uuid::new_v4()));
        let connection = Connection::open(&path).expect("create test database");
        connection
            .execute_batch(MIGRATION_V1)
            .expect("apply schema");
        connection
            .pragma_update(None, "user_version", 10)
            .expect("set schema version");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        path
    }

    #[test]
    fn healthy_database_satisfies_core_invariants() {
        let path = test_database();
        let connection = Connection::open(&path).expect("open database");
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO universes (id, name) VALUES ('u1', 'Mundo');
             INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Saga');
             INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');
             INSERT INTO chapters (id, book_id, title) VALUES ('c1', 'b1', 'Capítulo');
             INSERT INTO entities (id, universe_id, type, name) VALUES ('e1', 'u1', 'Personagem', 'Lia');
             INSERT INTO entities (id, universe_id, type, name) VALUES ('e2', 'u1', 'Lugar', 'Torre');
             INSERT INTO relations (id, universe_id, source_id, target_id, label) VALUES ('r1', 'u1', 'e1', 'e2', 'visita');",
        ).expect("seed valid domain");
        drop(connection);

        let report = inspect_database(&path).expect("inspect database");
        assert!(report.healthy, "unexpected issues: {:?}", report.issues);
        assert_eq!(report.schema_version, 10);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn relation_cannot_cross_universe_without_health_failure() {
        let path = test_database();
        let connection = Connection::open(&path).expect("open database");
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO universes (id, name) VALUES ('u1', 'Primeiro'), ('u2', 'Segundo');
             INSERT INTO entities (id, universe_id, type, name) VALUES ('e1', 'u1', 'Personagem', 'Lia');
             INSERT INTO entities (id, universe_id, type, name) VALUES ('e2', 'u2', 'Lugar', 'Torre');
             INSERT INTO relations (id, universe_id, source_id, target_id, label) VALUES ('r1', 'u1', 'e1', 'e2', 'inválida');",
        ).expect("seed cross-universe relation");
        drop(connection);

        let report = inspect_database(&path).expect("inspect database");
        assert!(!report.healthy);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "relation_crosses_universe"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn deleting_entity_preserves_referential_integrity() {
        let path = test_database();
        let connection = Connection::open(&path).expect("open database");
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO universes (id, name) VALUES ('u1', 'Mundo');
             INSERT INTO entities (id, universe_id, type, name) VALUES ('e1', 'u1', 'Personagem', 'Lia');
             INSERT INTO entities (id, universe_id, type, name) VALUES ('e2', 'u1', 'Lugar', 'Torre');
             INSERT INTO relations (id, universe_id, source_id, target_id, label) VALUES ('r1', 'u1', 'e1', 'e2', 'visita');",
        ).expect("seed relation");
        connection
            .execute("DELETE FROM entities WHERE id = ?1", params!["e1"])
            .expect("delete entity");
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM relations", [], |row| row.get(0))
            .expect("count relations");
        assert_eq!(remaining, 0);
        drop(connection);

        let report = inspect_database(&path).expect("inspect database");
        assert!(report.healthy, "unexpected issues: {:?}", report.issues);
        std::fs::remove_file(path).ok();
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use crate::database::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
    use uuid::Uuid;

    struct TempDb(std::path::PathBuf);
    impl Drop for TempDb {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).ok();
        }
    }

    fn banco_no_schema(versao: i64) -> TempDb {
        let path = std::env::temp_dir().join(format!("narrahub-compat-{}.db", Uuid::new_v4()));
        let connection = Connection::open(&path).expect("criar banco");
        for v in 1..=LATEST_SCHEMA_VERSION {
            connection
                .execute_batch(sql_for_version(v).expect("migration conhecida"))
                .expect("aplicar migration");
        }
        // O runtime registra a versão em _sqlx_migrations; os testes reproduzem isso para
        // exercitar o mesmo caminho de detecção que o app usa de verdade.
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS _sqlx_migrations (version BIGINT PRIMARY KEY);",
            )
            .expect("criar tabela de migrations");
        for v in 1..=versao {
            connection
                .execute("INSERT INTO _sqlx_migrations (version) VALUES (?1)", [v])
                .expect("registrar migration");
        }
        drop(connection);
        TempDb(path)
    }

    #[test]
    fn banco_ausente_nao_bloqueia_o_primeiro_boot() {
        let inexistente = std::env::temp_dir().join(format!("nao-existe-{}.db", Uuid::new_v4()));
        let resultado = inspect_compatibility(&inexistente).expect("não pode falhar");
        assert!(!resultado.database_exists);
        assert!(
            resultado.compatible,
            "instalação nova não tem banco, e isso não é incompatibilidade"
        );
    }

    #[test]
    fn banco_no_schema_atual_e_compativel() {
        let db = banco_no_schema(LATEST_SCHEMA_VERSION);
        let resultado = inspect_compatibility(&db.0).expect("inspecionar");
        assert_eq!(resultado.schema_version, LATEST_SCHEMA_VERSION);
        assert!(resultado.compatible);
    }

    #[test]
    fn banco_mais_antigo_e_compativel_porque_as_migrations_sobem() {
        let db = banco_no_schema(LATEST_SCHEMA_VERSION - 1);
        let resultado = inspect_compatibility(&db.0).expect("inspecionar");
        assert!(
            resultado.compatible,
            "banco antigo é o caso normal de atualização, não de bloqueio"
        );
    }

    /// O incidente de 2026-09-01: instalar uma versão antiga sobre um banco novo.
    #[test]
    fn banco_mais_novo_que_o_app_e_incompativel() {
        let db = banco_no_schema(LATEST_SCHEMA_VERSION + 1);
        let resultado = inspect_compatibility(&db.0).expect("inspecionar sem falhar");
        assert_eq!(resultado.schema_version, LATEST_SCHEMA_VERSION + 1);
        assert_eq!(resultado.supported_schema_version, LATEST_SCHEMA_VERSION);
        assert!(
            !resultado.compatible,
            "um banco mais novo que o executável precisa ser recusado antes de abrir o pool"
        );
    }

    /// O portão precisa **informar**, não estourar: se ele falhar, o app volta a morrer
    /// sem dizer nada, que é exatamente o defeito que ele existe para corrigir.
    #[test]
    fn incompatibilidade_e_resposta_e_nao_erro() {
        let db = banco_no_schema(LATEST_SCHEMA_VERSION + 1);
        assert!(
            inspect_compatibility(&db.0).is_ok(),
            "incompatibilidade tem que voltar como dado, nunca como Err"
        );
    }
}
