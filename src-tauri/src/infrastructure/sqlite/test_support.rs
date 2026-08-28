//! Banco de teste com o schema real, não com um schema desenhado à mão.
//!
//! Reproduzir as tabelas no teste esconde exatamente o defeito que este core
//! precisa pegar: query que casa com o schema imaginado e não com o que está
//! no disco do usuário.

use crate::database::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
use rusqlite::Connection;

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
