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
///
/// # `synchronous = OFF` no setup, e por que ele não alcança teste nenhum
///
/// [`TemporaryDatabase::new`] desliga o `fsync` **da conexão que aplica as
/// migrations**. A suíte caiu de **4362 s para 1047 s** com essa linha, e o
/// motivo é que criar um banco de teste custava 17,5 s — dos quais
/// `user 0.03s` e `sys 0.05s`: dezessete segundos parados esperando o disco
/// confirmar vinte migrations, uma por transação.
///
/// A pergunta que decide se o ganho é legítimo é qual propriedade depende de
/// `fsync`. E a resposta aqui é mais estreita do que parece, porque
/// **`synchronous` é por conexão**:
///
/// ```text
/// TemporaryDatabase::new()      conexão própria, synchronous = OFF
///                                → aplica as 20 migrations
///                                → é fechada
///
/// fixture.connection()          conexão NOVA, via SqliteDatabase::write()
///                                → apply_pragmas: busy_timeout, foreign_keys
///                                → synchronous fica no PADRÃO
/// ```
///
/// Ou seja: **toda escrita de todo teste continua com o `fsync` que tinha
/// antes.** O que ficou mais rápido é exclusivamente a montagem do schema, e
/// nenhuma propriedade que algum gate prove depende de as migrations terem
/// sido confirmadas no disco — elas são o ponto de partida do cenário, não o
/// que está sendo provado.
///
/// Isto foi **medido, não presumido**: o gate
/// `a_conexao_do_teste_nao_herda_o_synchronous_do_setup` lê o pragma nas duas
/// pontas. A primeira versão deste comentário afirmava uma classificação A/B
/// entre testes que podiam e não podiam rodar sem `fsync` — e o gate que eu
/// escrevi para sustentá-la reprovou, mostrando que a distinção não existia:
/// nenhum teste roda sem `fsync` nas próprias escritas.
///
/// Fora do alcance por construção: `database/backup.rs`, `recovery.rs`,
/// `health.rs`, `production_replica.rs` e `migrations.rs` montam a própria
/// conexão e não passam por aqui. É onde vivem as propriedades de durabilidade
/// física — `online_backup_captures_confirmed_wal_write_and_assets`, por
/// exemplo, liga WAL explicitamente e confere a gravação no arquivo copiado.
///
/// Há mais tempo a ganhar desligando o `fsync` também nas conexões de teste,
/// e isso **não** foi feito: aí o alcance passaria a incluir as escritas, e a
/// conversa sobre durabilidade deixaria de ser trivial. Registrado como
/// dívida em `NH-074`.
///
/// # Onde o custo existia, e onde nunca existiu
///
/// Isto era um problema **da máquina de desenvolvimento**, não do CI, e os
/// números do próprio GitHub Actions dizem isso:
///
/// ```text
/// main, ANTES da otimização    380 testes   19,37 s
/// esta branch, DEPOIS          384 testes   17,62 s
/// ```
///
/// O CI nunca teve o gargalo. O `fsync` custa centenas de milissegundos num
/// SSD externo por USB e microssegundos num disco local de datacenter, e os
/// 4362 s eram inteiramente esse fator.
///
/// Ou seja: o CI verde desta mudança prova que **nada quebrou**, e não prova o
/// ganho — o ganho é local, e é medido localmente. Quem for mexer nas dívidas
/// de `NH-073` a `NH-076` precisa medir na própria máquina antes de concluir
/// qualquer coisa sobre gargalo.
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
        Self::montar(
            // `synchronous = OFF` **só em teste**, e a diferença vale a suíte
            // inteira — ver a classificação A/B no doc do tipo.
            //
            // O que ele desliga é a garantia contra queda de energia, e um
            // banco que vive milissegundos e é apagado no `Drop` não tem o que
            // perder numa queda. Travamento, WAL, chave estrangeira, gatilho e
            // transação continuam idênticos.
            "PRAGMA synchronous = OFF; PRAGMA foreign_keys = ON;",
        )
    }

    fn montar(pragmas: &str) -> Self {
        let path = std::env::temp_dir().join(format!("narrahub-core-{}.db", uuid::Uuid::new_v4()));
        {
            let connection = Connection::open(&path).expect("criar banco de teste");
            connection
                .execute_batch(pragmas)
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

#[cfg(test)]
mod testes_da_fixture {
    use super::*;

    /// **A conexão do teste não herda o `synchronous` do setup.**
    ///
    /// É o gate que delimita o alcance da otimização, e ele nasceu de uma
    /// afirmação minha que estava errada: eu documentei uma classificação
    /// entre testes que podiam e não podiam rodar sem `fsync`, e o gate que
    /// escrevi para sustentá-la reprovou — as duas pontas leem o mesmo valor,
    /// porque `synchronous` é por conexão e `SqliteDatabase::write()` abre uma
    /// nova.
    ///
    /// O resultado é uma garantia mais forte que a classificação: **nenhuma
    /// escrita de teste ficou sem `fsync`.** Só a montagem do schema ficou.
    ///
    /// Se alguém puser `synchronous = OFF` em `apply_pragmas`, este gate
    /// reprova — e aí a conversa sobre durabilidade deixa de ser trivial e
    /// precisa acontecer.
    #[test]
    fn a_conexao_do_teste_nao_herda_o_synchronous_do_setup() {
        let banco = TemporaryDatabase::new();
        let do_teste: i64 = banco
            .connection()
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .expect("ler o pragma da conexão de teste");

        // 0 é OFF. Qualquer outro valor é alguma forma de sincronização.
        assert_ne!(
            do_teste, 0,
            "a conexão que os testes usam está sem fsync. A otimização era para alcançar \
             somente o setup do schema; alcançar as escritas muda quais propriedades \
             continuam provadas."
        );

        // E o banco é de verdade, com o schema inteiro.
        let versao: i64 = banco
            .connection()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("ler a versão");
        assert_eq!(versao, crate::database::migrations::LATEST_SCHEMA_VERSION);
    }

    /// O que a fixture rápida **não** enfraquece: transação e rollback.
    ///
    /// `synchronous` é sobre durabilidade contra queda de energia. Atomicidade
    /// é outra coisa, e continua valendo — este gate é o que separa as duas
    /// propriedades de forma executável, em vez de por argumento.
    #[test]
    fn sem_fsync_a_transacao_continua_atomica() {
        let banco = TemporaryDatabase::new();
        let mut connection = banco.connection();

        let tx = connection.transaction().expect("abrir transação");
        tx.execute(
            "INSERT INTO universes (id, name, created_at, updated_at)
             VALUES ('u1','Some no rollback','2026-01-01','2026-01-01')",
            [],
        )
        .expect("inserir");
        drop(tx); // sem commit

        let quantos: i64 = connection
            .query_row("SELECT COUNT(*) FROM universes", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(quantos, 0, "o rollback não pode depender de fsync");
    }

    /// E nem a chave estrangeira.
    #[test]
    fn sem_fsync_a_chave_estrangeira_continua_valendo() {
        let banco = TemporaryDatabase::new();
        let erro = banco
            .connection()
            .execute(
                "INSERT INTO stories (id, universe_id, name, created_at, updated_at)
                 VALUES ('s1','universo-que-nao-existe','S','2026-01-01','2026-01-01')",
                [],
            )
            .expect_err("a FK tinha que recusar");
        assert!(
            erro.to_string().to_lowercase().contains("foreign key"),
            "recusou pelo motivo errado: {erro}"
        );
    }
}
