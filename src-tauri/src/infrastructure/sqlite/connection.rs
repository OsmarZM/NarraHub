use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// O plano da Fase 4 exige `foreign_keys` e `busy_timeout` em toda conexão.
/// Estão aqui, num lugar só, porque pragma esquecido não falha na hora: falha
/// depois, como FK não aplicada ou `database is locked` sob autosave.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(8);

/// Ponto único de abertura de conexão do core.
///
/// Não há pool: SQLite abre conexão em microssegundos e o plano pede
/// "transações curtas". Um pool próprio conviveria mal com o pool que o
/// `tauri-plugin-sql` ainda mantém aberto durante a migração gradual — o
/// banco está em WAL desde a migration 1, que é o que torna essa convivência
/// possível.
#[derive(Debug, Clone)]
pub struct SqliteDatabase {
    path: PathBuf,
    /// A fase do banco autoriza escrever? Quando não, `write()` recusa — e com isso recusa toda a
    /// `Mutacao`, que é quem pede a conexão de escrita (etapa C2).
    somente_leitura: bool,
}

impl SqliteDatabase {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            somente_leitura: false,
        }
    }

    /// O banco conforme a fase: `Ready` escreve, o degradado `ReadyReadOnly` só lê.
    ///
    /// **Um lugar só.** Espalhar essa decisão pelos comandos deixaria um comando novo escrevendo
    /// num acervo que a sincronização ainda não sabe descrever — e nenhum teste de comportamento
    /// perceberia sem o Tauri rodando.
    pub fn conforme_a_fase(
        fase: crate::database::estado::FaseDoBanco,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            path: path.into(),
            somente_leitura: fase != crate::database::estado::FaseDoBanco::Ready,
        }
    }

    pub fn e_somente_leitura(&self) -> bool {
        self.somente_leitura
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Conexão de leitura. Abre somente-leitura de propósito: um comando de
    /// consulta que grave por engano falha aqui, e não silenciosamente.
    pub fn read(&self) -> DatabaseCommandResult<Connection> {
        self.open(OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    }

    pub fn write(&self) -> DatabaseCommandResult<Connection> {
        if self.somente_leitura {
            return Err(DatabaseCommandError::unavailable(
                "Este acervo tem imagens antigas que ainda não puderam ser convertidas. Até isso \
                 ser resolvido o texto pode ser lido, mas não alterado — uma edição agora ficaria \
                 registrada de um jeito que a sincronização não saberia descrever.",
            ));
        }
        self.open(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    }

    fn open(&self, flags: OpenFlags) -> DatabaseCommandResult<Connection> {
        if !self.path.is_file() {
            return Err(DatabaseCommandError::unavailable(
                "O banco local do NarraHub ainda não foi criado.",
            ));
        }
        let connection = Connection::open_with_flags(&self.path, flags).map_err(|error| {
            DatabaseCommandError::storage(format!("Não foi possível abrir o banco local: {error}"))
        })?;
        apply_pragmas(&connection)?;
        Ok(connection)
    }
}

pub fn apply_pragmas(connection: &Connection) -> DatabaseCommandResult<()> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    // Os gates G10/G11 da etapa G vigiam, por esta porta, toda conexão do runtime.
    #[cfg(test)]
    crate::database::legado_v1::vigia::instalar(connection);
    Ok(())
}

/// Traduz o erro do rusqlite para o contrato que o frontend já entende.
/// Violação de FK/unique vira `conflict`, e não `storage`: a diferença é o que
/// decide se a interface mostra "já existe" ou "falhou ao gravar".
pub fn map_sqlite_error(error: rusqlite::Error) -> DatabaseCommandError {
    use rusqlite::ffi::ErrorCode;
    if let rusqlite::Error::SqliteFailure(failure, ref message) = error {
        let detail = message.clone().unwrap_or_else(|| error.to_string());
        return match failure.code {
            ErrorCode::ConstraintViolation => DatabaseCommandError::conflict(detail),
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                DatabaseCommandError::unavailable(detail)
            }
            _ => DatabaseCommandError::storage(detail),
        };
    }
    if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
        return DatabaseCommandError::not_found("Registro não encontrado.");
    }
    DatabaseCommandError::storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;

    #[test]
    fn abrir_banco_inexistente_e_indisponivel_nao_erro_de_storage() {
        // A interface trata `unavailable` como "abra ou crie um universo",
        // e `storage` como falha real de disco. Confundir os dois manda o
        // usuário para a mensagem errada.
        let database = SqliteDatabase::new("caminho/que/nao/existe.db");
        let error = database.read().expect_err("deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::Unavailable);
    }

    #[test]
    fn conexao_aplica_foreign_keys_e_busy_timeout() {
        let connection = Connection::open_in_memory().expect("abrir memória");
        apply_pragmas(&connection).expect("aplicar pragmas");

        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("ler pragma");
        assert_eq!(foreign_keys, 1, "FK precisa estar ligada em toda conexão");

        let busy: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("ler pragma");
        assert_eq!(busy, BUSY_TIMEOUT.as_millis() as i64);
    }

    #[test]
    fn violacao_de_constraint_vira_conflict() {
        let connection = Connection::open_in_memory().expect("abrir memória");
        connection
            .execute_batch("CREATE TABLE t (id TEXT PRIMARY KEY);")
            .expect("criar tabela");
        connection
            .execute("INSERT INTO t VALUES ('a')", [])
            .expect("inserir");
        let error = connection
            .execute("INSERT INTO t VALUES ('a')", [])
            .expect_err("duplicata deveria falhar");
        assert_eq!(map_sqlite_error(error).kind, DatabaseErrorKind::Conflict);
    }
}
