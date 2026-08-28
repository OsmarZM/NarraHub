//! Banco de teste com o schema real, não com um schema desenhado à mão.
//!
//! Reproduzir as tabelas no teste esconde exatamente o defeito que este core
//! precisa pegar: query que casa com o schema imaginado e não com o que está
//! no disco do usuário.

use crate::database::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
use crate::infrastructure::sqlite::SqliteDatabase;
use rusqlite::Connection;
use std::path::PathBuf;

pub fn migrated_memory_database() -> Connection {
    let connection = Connection::open_in_memory().expect("abrir banco em memória");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("ligar foreign keys");
    for version in 1..=LATEST_SCHEMA_VERSION {
        connection
            .execute_batch(sql_for_version(version).expect("migration conhecida"))
            .unwrap_or_else(|error| panic!("aplicar migration v{version}: {error}"));
    }
    connection
        .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
        .expect("gravar versão do schema");
    connection
}

pub fn seed_universe(connection: &Connection, universe_id: &str) {
    connection
        .execute(
            "INSERT INTO universes (id, name, description, created_at, updated_at)
             VALUES (?1, ?1, '', '2026-01-01 00:00:00', '2026-01-01 00:00:00')",
            [universe_id],
        )
        .expect("semear universo");
}

/// Banco em arquivo, para testar a camada de aplicação — que abre a própria
/// conexão e portanto não aceita um `:memory:` de fora.
///
/// O arquivo se apaga sozinho no `Drop`. Sem isso, cada execução de teste
/// deixaria lixo no temp do desenvolvedor.
pub struct TemporaryDatabase {
    pub database: SqliteDatabase,
    path: PathBuf,
}

impl Default for TemporaryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl TemporaryDatabase {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("narrahub-core-{}.db", uuid::Uuid::new_v4()));
        {
            let connection = Connection::open(&path).expect("criar banco de teste");
            connection
                .execute_batch("PRAGMA foreign_keys = ON;")
                .expect("ligar foreign keys");
            for version in 1..=LATEST_SCHEMA_VERSION {
                connection
                    .execute_batch(sql_for_version(version).expect("migration conhecida"))
                    .unwrap_or_else(|error| panic!("aplicar migration v{version}: {error}"));
            }
            connection
                .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
                .expect("gravar versão do schema");
        }
        let database = SqliteDatabase::new(path.clone());
        Self { database, path }
    }

    pub fn connection(&self) -> Connection {
        self.database.write().expect("abrir conexão de teste")
    }
}

impl Drop for TemporaryDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
    }
}
