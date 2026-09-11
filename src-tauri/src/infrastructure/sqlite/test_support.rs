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
                .execute_batch(
                    // `synchronous = OFF` **só em teste**, e a diferença vale a
                    // suíte inteira.
                    //
                    // O banco entra em WAL na migration 1, e cada uma das 20
                    // migrations e um `execute_batch` -- uma transação, um
                    // `fsync`. Num SSD externo por USB o `fsync` custa centenas
                    // de milissegundos, e o resultado medido era 17,5 s para
                    // criar um banco de teste que não faz nada: `user 0.03s`,
                    // `sys 0.05s`, e dezessete segundos esperando disco.
                    //
                    // O que `synchronous = OFF` desliga é a garantia contra
                    // **queda de energia**, e um banco de teste que existe por
                    // milissegundos e é apagado no `Drop` não tem o que perder
                    // numa queda. Travamento, WAL, FK, gatilho e transação
                    // continuam idênticos -- nada da semântica que os gates
                    // provam depende disso.
                    "PRAGMA synchronous = OFF; PRAGMA foreign_keys = ON;",
                )
                .expect("preparar o banco de teste");
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

/// Uma origem remota **de verdade**: identidade própria, chave própria, e a
/// linha do roster com a pública que realmente deriva o `device_id`.
///
/// Existe porque a etapa 7 passou a cobrar a cadeia inteira. Um teste com
/// chave inventada não exercitaria o caminho que a produção percorre — ele
/// seria recusado, e com razão.
pub fn origem_remota_confiavel(
    connection: &Connection,
    quem_introduz: &crate::domain::identity::DeviceIdentity,
) -> crate::domain::identity::DeviceIdentity {
    let identidade = crate::domain::identity::DeviceIdentity::generate();
    let sessao =
        crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(quem_introduz);
    crate::infrastructure::sqlite::sync_trust::introduzir_dispositivo(
        connection,
        &sessao,
        identidade.device_id(),
        &identidade.public_base32(),
    )
    .expect("introduzir origem remota");
    identidade
}

/// O `self` de um banco de teste, sem passar pelo arquivo de identidade.
pub fn self_de_teste(connection: &Connection) -> crate::domain::identity::DeviceIdentity {
    let identidade = crate::domain::identity::DeviceIdentity::generate();
    connection
        .execute(
            "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
             VALUES (?1, 'Este aparelho', ?2, 1)",
            rusqlite::params![identidade.device_id(), identidade.public_base32()],
        )
        .expect("registrar self de teste");
    identidade
}
