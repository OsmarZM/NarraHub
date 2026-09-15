//! Migration de schema segura: backup validado antes, banco original de volta se falhar.
//!
//! ## O problema que existia
//!
//! O `tauri-plugin-sql` aplicava as migrations no `setup` do plugin, por `preload` no
//! `tauri.conf.json` — **antes** do portão de compatibilidade do frontend e sem backup nenhum.
//! Cada migration do `sqlx` roda na sua transação, mas uma subida de várias versões para no meio
//! se uma delas falhar: o banco fica numa versão intermediária, que nenhum executável publicado
//! conhece. E um banco mais novo que o executável fazia o `sqlx` recusar com `VersionMissing`
//! dentro do `setup`, derrubando o app antes de a tela existir.
//!
//! ## O fluxo agora
//!
//! ```text
//! frontend                         Rust (este módulo)
//! ────────                         ──────────────────
//! database_compatibility           banco mais novo? para aqui, tela de recuperação
//! database_migration_prepare  ──▶  marcador de migration interrompida? devolve o original primeiro
//!                                  schema atrás? backup consistente + validado + marcador
//!                                  backup falhou? ERRO: o pool não abre, nada migra
//! Database.load (plugin-sql)       aplica as migrations
//!   ok  ─▶ database_migration_finish     confere a versão e o integrity_check, apaga o marcador
//!   erro ─▶ database_migration_rollback  devolve o banco do backup, apaga o marcador
//! ```
//!
//! O **marcador** (`migration-em-andamento.json`) é o que cobre a queda: se o processo morre no
//! meio da migration, o próximo arranque encontra o marcador e devolve o banco original antes de
//! tentar de novo. Sem marcador, uma queda deixaria a versão intermediária para sempre.
//!
//! O backup fica em `<dados do app>/backups/<id>/narrahub.db`, com `manifest.json`, e aparece na
//! lista de backups de Configurações com o motivo "antes da migração" — ver
//! `docs/BACKUP_E_MIGRATIONS.md`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use super::backup::{create_backup_at, validate_backup_at, BackupManifest, BackupReason};
use super::duravel;
use super::error::{DatabaseCommandError, DatabaseCommandResult};
use super::estado::{EstadoDoBanco, FaseDoBanco};
use super::health::{inspect_compatibility, inspect_database};
use super::migrations::LATEST_SCHEMA_VERSION;

const DATABASE_FILE_NAME: &str = "narrahub.db";
const SIDECARS: [&str; 2] = ["narrahub.db-wal", "narrahub.db-shm"];
pub const MARKER_FILE_NAME: &str = "migration-em-andamento.json";

/// O que o marcador guarda. Suficiente para devolver o banco sem consultar mais nada.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationMarker {
    pub backup_id: String,
    pub from_version: i64,
    pub to_version: i64,
}

/// O que a preparação decidiu, para a tela e para o próximo passo do arranque.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPreparation {
    /// Há migration a aplicar nesta abertura.
    pub needed: bool,
    pub from_version: i64,
    pub to_version: i64,
    /// O backup feito antes, quando houve migration a aplicar.
    pub backup: Option<BackupManifest>,
    /// Uma migration anterior tinha sido interrompida e o banco original foi devolvido agora.
    pub recovered_interrupted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRollback {
    /// O banco original voltou ao lugar.
    pub restored: bool,
    pub backup_id: Option<String>,
    pub schema_version: i64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Comandos
// ═══════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub async fn database_migration_prepare(
    app: AppHandle,
    estado: State<'_, EstadoDoBanco>,
) -> DatabaseCommandResult<MigrationPreparation> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    estado.definir(FaseDoBanco::Unprepared);
    let resultado = tauri::async_runtime::spawn_blocking(move || {
        prepare_at(&app_data, env!("CARGO_PKG_VERSION"))
    })
    .await
    .map_err(|error| DatabaseCommandError::unavailable(error.to_string()))?;
    estado.definir(match &resultado {
        Ok(preparo) if preparo.needed => FaseDoBanco::Migrating,
        Ok(_) => FaseDoBanco::Ready,
        Err(_) => FaseDoBanco::RecoveryRequired,
    });
    resultado
}

#[tauri::command]
pub fn database_migration_finish(
    app: AppHandle,
    estado: State<'_, EstadoDoBanco>,
) -> DatabaseCommandResult<i64> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    let resultado = finish_at(&app_data);
    // Só a confirmação libera. Uma falha aqui deixa em `Migrating`: o arranque chama o rollback.
    if resultado.is_ok() {
        estado.definir(FaseDoBanco::Ready);
    }
    resultado
}

#[tauri::command]
pub async fn database_migration_rollback(
    app: AppHandle,
    estado: State<'_, EstadoDoBanco>,
) -> DatabaseCommandResult<MigrationRollback> {
    let app_data = super::app_data_path(&app).map_err(DatabaseCommandError::unavailable)?;
    let resultado = tauri::async_runtime::spawn_blocking(move || rollback_at(&app_data))
        .await
        .map_err(|error| DatabaseCommandError::unavailable(error.to_string()))?;
    estado.definir(match &resultado {
        // O original voltou, mas ainda está no schema antigo: o próximo arranque prepara de novo.
        Ok(_) => FaseDoBanco::Unprepared,
        Err(_) => FaseDoBanco::RecoveryRequired,
    });
    resultado
}

// ═══════════════════════════════════════════════════════════════════════════
// Regras, testáveis sem Tauri
// ═══════════════════════════════════════════════════════════════════════════

fn database_path(app_data: &Path) -> PathBuf {
    app_data.join(DATABASE_FILE_NAME)
}

fn backups_root(app_data: &Path) -> PathBuf {
    app_data.join("backups")
}

fn marker_path(app_data: &Path) -> PathBuf {
    app_data.join(MARKER_FILE_NAME)
}

pub fn read_marker(app_data: &Path) -> DatabaseCommandResult<Option<MigrationMarker>> {
    let caminho = marker_path(app_data);
    if !caminho.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&caminho).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O registro da migration anterior não pôde ser lido: {error}"
        ))
    })?;
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O registro da migration anterior está corrompido ({error}). O banco não foi aberto; \
             o backup mais recente \"antes da migração\" em Configurações → Backup pode ser restaurado."
        ))
    })
}

/// Escreve o marcador de forma atômica **e durável**: temporário sincronizado no disco, troca de
/// nome, diretório sincronizado. Um marcador que some numa queda de energia deixaria uma migration
/// interrompida sem ninguém para desfazê-la.
fn write_marker(app_data: &Path, marker: &MigrationMarker) -> DatabaseCommandResult<()> {
    let bytes = serde_json::to_vec_pretty(marker)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    duravel::gravar_atomico(&marker_path(app_data), &bytes).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O registro da migration não pôde ser gravado ({error}). Nada foi migrado."
        ))
    })
}

fn remove_marker(app_data: &Path) -> DatabaseCommandResult<()> {
    duravel::remover_duravel(&marker_path(app_data)).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O registro da migration não pôde ser removido: {error}"
        ))
    })
}

/// Antes de o pool abrir: devolve uma migration interrompida, e faz o backup da que vai rodar.
pub fn prepare_at(
    app_data: &Path,
    app_version: &str,
) -> DatabaseCommandResult<MigrationPreparation> {
    let mut recovered_interrupted = false;
    if read_marker(app_data)?.is_some() {
        rollback_at(app_data)?;
        recovered_interrupted = true;
    }

    let banco = database_path(app_data);
    let compatibilidade = inspect_compatibility(&banco).map_err(DatabaseCommandError::storage)?;
    let from_version = compatibilidade.schema_version;

    // Proteção própria, sem depender de o frontend ter consultado a compatibilidade antes: um banco
    // mais novo que esta build não é "já na versão". Abrir o pool nele escreveria em colunas que
    // este executável não conhece.
    if from_version > LATEST_SCHEMA_VERSION {
        return Err(DatabaseCommandError::conflict(format!(
            "O banco está no schema {from_version}, mais novo que o {LATEST_SCHEMA_VERSION} que esta \
             versão do NarraHub conhece. Ele não foi aberto nem alterado."
        )));
    }

    // Instalação nova, ou banco já na versão: não há o que proteger.
    if !compatibilidade.database_exists || from_version >= LATEST_SCHEMA_VERSION {
        return Ok(MigrationPreparation {
            needed: false,
            from_version,
            to_version: LATEST_SCHEMA_VERSION,
            backup: None,
            recovered_interrupted,
        });
    }

    // Só o banco: migration de schema não toca nos arquivos de mídia, e copiar o acervo de
    // imagens a cada atualização custaria minutos e espaço num celular.
    let manifest = create_backup_at(
        &banco,
        None,
        &backups_root(app_data),
        app_version,
        BackupReason::PreMigration,
    )
    .map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O backup antes da atualização do banco falhou ({error}). Por segurança o banco não \
             foi atualizado nem aberto."
        ))
    })?;
    let validacao = validate_backup_at(&backups_root(app_data), &manifest.backup_id)
        .map_err(DatabaseCommandError::storage)?;
    if !validacao.valid {
        return Err(DatabaseCommandError::storage(format!(
            "O backup antes da atualização do banco não passou na validação ({}). Por segurança o \
             banco não foi atualizado nem aberto.",
            validacao.errors.join("; ")
        )));
    }

    write_marker(
        app_data,
        &MigrationMarker {
            backup_id: manifest.backup_id.clone(),
            from_version,
            to_version: LATEST_SCHEMA_VERSION,
        },
    )?;

    Ok(MigrationPreparation {
        needed: true,
        from_version,
        to_version: LATEST_SCHEMA_VERSION,
        backup: Some(manifest),
        recovered_interrupted,
    })
}

/// Depois de o plugin migrar: só apaga o marcador se o banco chegou inteiro na versão.
pub fn finish_at(app_data: &Path) -> DatabaseCommandResult<i64> {
    let banco = database_path(app_data);
    let saude = inspect_database(&banco).map_err(DatabaseCommandError::storage)?;
    if saude.schema_version != LATEST_SCHEMA_VERSION {
        return Err(DatabaseCommandError::storage(format!(
            "O banco ficou na versão {} depois da atualização, e esta versão do NarraHub espera a \
             {LATEST_SCHEMA_VERSION}.",
            saude.schema_version
        )));
    }
    if !saude.integrity_result.eq_ignore_ascii_case("ok") {
        return Err(DatabaseCommandError::storage(format!(
            "O banco atualizado não passou na verificação de integridade: {}.",
            saude.integrity_result
        )));
    }
    remove_marker(app_data)?;
    Ok(saude.schema_version)
}

/// Devolve o banco que existia antes da migration.
///
/// Sem marcador não há o que devolver, e nada é tocado: sem saber qual backup é o da migration,
/// restaurar "o mais recente" poderia trazer de volta um backup manual antigo.
pub fn rollback_at(app_data: &Path) -> DatabaseCommandResult<MigrationRollback> {
    let Some(marker) = read_marker(app_data)? else {
        let banco = database_path(app_data);
        let versao = inspect_compatibility(&banco)
            .map(|c| c.schema_version)
            .unwrap_or(0);
        return Ok(MigrationRollback {
            restored: false,
            backup_id: None,
            schema_version: versao,
        });
    };

    let validacao = validate_backup_at(&backups_root(app_data), &marker.backup_id)
        .map_err(DatabaseCommandError::storage)?;
    if !validacao.valid {
        return Err(DatabaseCommandError::storage(format!(
            "O backup {} feito antes da atualização não passou na validação ({}). O banco atual \
             foi mantido como está; nada foi apagado.",
            marker.backup_id,
            validacao.errors.join("; ")
        )));
    }

    let origem = backups_root(app_data)
        .join(&marker.backup_id)
        .join(DATABASE_FILE_NAME);
    let banco = database_path(app_data);

    // 1. Cópia do backup para o lado, sincronizada. Falhar aqui não tocou em nada.
    let temporario = app_data.join("narrahub.db.restaurando");
    fs::copy(&origem, &temporario)
        .and_then(|_| duravel::sincronizar_arquivo(&temporario))
        .map_err(|error| {
            let _ = fs::remove_file(&temporario);
            DatabaseCommandError::storage(format!(
                "O banco original não pôde ser copiado do backup ({error}). O banco atual foi mantido."
            ))
        })?;

    // 2. Os -wal/-shm são do banco migrado: ao lado do original, o SQLite os leria como parte dele.
    //    Saem de lugar ANTES da troca, e voltam se ela falhar.
    let mut afastados: Vec<(PathBuf, PathBuf)> = Vec::new();
    for sidecar in SIDECARS {
        let caminho = app_data.join(sidecar);
        if !caminho.exists() {
            continue;
        }
        let destino_afastado = app_data.join(format!("{sidecar}.migrado"));
        if let Err(error) = fs::rename(&caminho, &destino_afastado) {
            devolver(&afastados);
            let _ = fs::remove_file(&temporario);
            return Err(DatabaseCommandError::storage(format!(
                "O arquivo {sidecar} do banco atualizado está em uso ({error}). Feche o NarraHub e abra \
                 de novo; o banco atual foi mantido e o backup continua em {}.",
                origem.display()
            )));
        }
        afastados.push((caminho, destino_afastado));
    }

    // 3. A troca, portável e sem depender de o rename aceitar destino existente (duravel.rs). Com o
    //    banco ainda aberto por alguém, falha aqui sem mudar nada.
    if let Err(error) = duravel::substituir_arquivo(&temporario, &banco) {
        devolver(&afastados);
        let _ = fs::remove_file(&temporario);
        return Err(DatabaseCommandError::storage(format!(
            "O banco original não pôde voltar ao lugar ({error}). O banco atual foi mantido; o \
             backup continua em {}.",
            origem.display()
        )));
    }
    for (_, afastado) in &afastados {
        let _ = fs::remove_file(afastado);
    }

    let versao = inspect_compatibility(&banco)
        .map_err(DatabaseCommandError::storage)?
        .schema_version;
    remove_marker(app_data)?;
    Ok(MigrationRollback {
        restored: true,
        backup_id: Some(marker.backup_id),
        schema_version: versao,
    })
}

/// Devolve os arquivos que tinham saído de lugar, quando a restauração desiste.
fn devolver(afastados: &[(PathBuf, PathBuf)]) {
    for (original, afastado) in afastados {
        let _ = fs::rename(afastado, original);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::sql_for_version;
    use rusqlite::Connection;
    use sha2::{Digest, Sha384};

    struct Dados(PathBuf);

    impl Dados {
        fn novo() -> Self {
            let raiz =
                std::env::temp_dir().join(format!("narrahub-upgrade-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&raiz).expect("diretório");
            Self(raiz)
        }

        /// Um banco como o `sqlx` deixa: migrations aplicadas e registradas com checksum.
        fn banco_na_versao(&self, versao: i64) {
            let connection = Connection::open(database_path(&self.0)).expect("abrir");
            connection
                .execute_batch(
                    "CREATE TABLE _sqlx_migrations (
                        version BIGINT PRIMARY KEY, description TEXT NOT NULL,
                        installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        success BOOLEAN NOT NULL, checksum BLOB NOT NULL,
                        execution_time BIGINT NOT NULL)",
                )
                .expect("tabela do sqlx");
            for v in 1..=versao {
                let sql = sql_for_version(v).expect("migration conhecida");
                connection.execute_batch(sql).expect("migration");
                connection
                    .execute(
                        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                         VALUES (?1, 'teste', 1, ?2, 0)",
                        rusqlite::params![v, Sha384::digest(sql.as_bytes()).to_vec()],
                    )
                    .expect("registro");
            }
        }

        fn universo(&self, nome: &str) {
            Connection::open(database_path(&self.0))
                .expect("abrir")
                .execute("INSERT INTO universes (id, name) VALUES (?1, ?1)", [nome])
                .expect("universo");
        }

        fn universos(&self) -> Vec<String> {
            let connection = Connection::open(database_path(&self.0)).expect("abrir");
            let mut consulta = connection
                .prepare("SELECT name FROM universes ORDER BY name")
                .expect("consulta");
            consulta
                .query_map([], |row| row.get(0))
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("nomes")
        }

        /// O que o plugin faria: aplica o resto das migrations.
        fn migrar_como_o_plugin(&self, de: i64, ate: i64) {
            let connection = Connection::open(database_path(&self.0)).expect("abrir");
            for v in (de + 1)..=ate {
                let sql = sql_for_version(v).expect("migration conhecida");
                connection.execute_batch(sql).expect("migration");
                connection
                    .execute(
                        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                         VALUES (?1, 'teste', 1, ?2, 0)",
                        rusqlite::params![v, Sha384::digest(sql.as_bytes()).to_vec()],
                    )
                    .expect("registro");
            }
        }
    }

    impl Drop for Dados {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn instalacao_nova_nao_faz_backup_nem_marcador() {
        let dados = Dados::novo();
        let preparo = prepare_at(&dados.0, "teste").expect("preparar");
        assert!(!preparo.needed);
        assert!(preparo.backup.is_none());
        assert!(read_marker(&dados.0).expect("marcador").is_none());
    }

    #[test]
    fn banco_na_versao_atual_nao_faz_backup() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION);
        let preparo = prepare_at(&dados.0, "teste").expect("preparar");
        assert!(!preparo.needed);
        assert!(!backups_root(&dados.0).exists(), "backup desnecessário");
    }

    #[test]
    fn banco_antigo_ganha_backup_validado_e_marcador_antes_de_migrar() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 3);
        dados.universo("Reino de Aether");

        let preparo = prepare_at(&dados.0, "teste").expect("preparar");
        assert!(preparo.needed);
        assert_eq!(preparo.from_version, LATEST_SCHEMA_VERSION - 3);
        let backup = preparo.backup.expect("backup");
        assert_eq!(backup.schema_version, LATEST_SCHEMA_VERSION - 3);
        assert!(matches!(backup.reason, BackupReason::PreMigration));
        assert!(
            validate_backup_at(&backups_root(&dados.0), &backup.backup_id)
                .expect("validar")
                .valid
        );
        assert_eq!(
            read_marker(&dados.0).expect("marcador"),
            Some(MigrationMarker {
                backup_id: backup.backup_id,
                from_version: LATEST_SCHEMA_VERSION - 3,
                to_version: LATEST_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn migration_bem_sucedida_apaga_o_marcador() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 2);
        prepare_at(&dados.0, "teste").expect("preparar");
        dados.migrar_como_o_plugin(LATEST_SCHEMA_VERSION - 2, LATEST_SCHEMA_VERSION);

        assert_eq!(
            finish_at(&dados.0).expect("concluir"),
            LATEST_SCHEMA_VERSION
        );
        assert!(read_marker(&dados.0).expect("marcador").is_none());
    }

    #[test]
    fn migration_que_parou_no_meio_nao_e_dada_como_concluida() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 3);
        prepare_at(&dados.0, "teste").expect("preparar");
        // Subiu duas das três e parou.
        dados.migrar_como_o_plugin(LATEST_SCHEMA_VERSION - 3, LATEST_SCHEMA_VERSION - 1);

        assert!(
            finish_at(&dados.0).is_err(),
            "versão intermediária não pode passar"
        );
        assert!(
            read_marker(&dados.0).expect("marcador").is_some(),
            "o marcador fica para devolver o original"
        );
    }

    #[test]
    fn falha_na_migration_devolve_o_banco_original_inteiro() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 3);
        dados.universo("Reino de Aether");
        prepare_at(&dados.0, "teste").expect("preparar");

        // O plugin sobe parte e escreve algo que não pode sobreviver ao rollback.
        dados.migrar_como_o_plugin(LATEST_SCHEMA_VERSION - 3, LATEST_SCHEMA_VERSION - 1);
        dados.universo("escrito no meio da migration");
        fs::write(dados.0.join("narrahub.db-wal"), b"wal do banco migrado").expect("wal");

        let volta = rollback_at(&dados.0).expect("rollback");
        assert!(volta.restored);
        assert_eq!(volta.schema_version, LATEST_SCHEMA_VERSION - 3);
        assert_eq!(dados.universos(), vec!["Reino de Aether".to_string()]);
        assert!(!dados.0.join("narrahub.db-wal").exists());
        assert!(read_marker(&dados.0).expect("marcador").is_none());
    }

    #[test]
    fn queda_durante_a_migration_e_desfeita_no_proximo_arranque() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 3);
        dados.universo("Marte 2189");
        prepare_at(&dados.0, "teste").expect("preparar");
        // O processo morreu com o banco na versão intermediária e o marcador no disco.
        dados.migrar_como_o_plugin(LATEST_SCHEMA_VERSION - 3, LATEST_SCHEMA_VERSION - 2);

        let preparo = prepare_at(&dados.0, "teste").expect("próximo arranque");
        assert!(preparo.recovered_interrupted);
        assert!(
            preparo.needed,
            "o banco original volta e a migration é tentada de novo"
        );
        assert_eq!(preparo.from_version, LATEST_SCHEMA_VERSION - 3);
        assert_eq!(dados.universos(), vec!["Marte 2189".to_string()]);
        assert!(
            read_marker(&dados.0).expect("marcador").is_some(),
            "novo marcador para a nova tentativa"
        );
    }

    #[test]
    fn rollback_sem_marcador_nao_toca_em_nada() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION);
        dados.universo("Colônia Zero");
        let volta = rollback_at(&dados.0).expect("rollback");
        assert!(!volta.restored);
        assert_eq!(dados.universos(), vec!["Colônia Zero".to_string()]);
    }

    #[test]
    fn backup_que_nao_pode_ser_feito_impede_a_migration() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 1);
        // A raiz de backups ocupada por um arquivo: não há onde gravar.
        fs::write(backups_root(&dados.0), b"arquivo no lugar da pasta").expect("bloquear");

        let erro = prepare_at(&dados.0, "teste").expect_err("tinha que recusar");
        assert!(
            erro.message.contains("não foi atualizado nem aberto"),
            "{}",
            erro.message
        );
        assert!(
            read_marker(&dados.0).expect("marcador").is_none(),
            "sem backup, sem marcador"
        );
    }

    #[test]
    fn banco_mais_novo_que_a_build_e_recusado_mesmo_sem_o_portao_do_frontend() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION);
        Connection::open(database_path(&dados.0))
            .expect("abrir")
            .execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (?1, 'do futuro', 1, x'00', 0)",
                [LATEST_SCHEMA_VERSION + 1],
            )
            .expect("simular versão futura");
        dados.universo("não pode ser tocado");

        let erro = prepare_at(&dados.0, "teste").expect_err("tinha que recusar");
        assert!(erro.message.contains("mais novo"), "{}", erro.message);
        assert!(
            !backups_root(&dados.0).exists(),
            "nada de backup de banco que não será migrado"
        );
        assert!(read_marker(&dados.0).expect("marcador").is_none());
        assert_eq!(dados.universos(), vec!["não pode ser tocado".to_string()]);
    }

    /// Rollback com o banco migrado ainda aberto por uma conexão SQLite — o caso real do Windows,
    /// onde o arquivo não pode ser renomeado. Nada pode ser perdido, o marcador fica para tentar de
    /// novo, e depois de a conexão fechar o rollback conclui.
    #[test]
    fn rollback_com_o_banco_aberto_nao_perde_nada_e_conclui_depois_de_fechar() {
        let dados = Dados::novo();
        dados.banco_na_versao(LATEST_SCHEMA_VERSION - 2);
        dados.universo("Reino de Aether");
        prepare_at(&dados.0, "teste").expect("preparar");
        dados.migrar_como_o_plugin(LATEST_SCHEMA_VERSION - 2, LATEST_SCHEMA_VERSION - 1);
        dados.universo("escrito na migração");

        let aberta = Connection::open(database_path(&dados.0)).expect("segurar aberto");
        aberta
            .query_row("SELECT COUNT(*) FROM universes", [], |r| r.get::<_, i64>(0))
            .expect("usar");
        let tentativa = rollback_at(&dados.0);

        if cfg!(windows) {
            let erro = tentativa.expect_err("no Windows o banco aberto impede a troca");
            assert!(erro.message.contains("foi mantido"), "{}", erro.message);
            assert!(
                read_marker(&dados.0).expect("marcador").is_some(),
                "marcador fica para a próxima tentativa"
            );
            assert!(
                !dados.0.join("narrahub.db.restaurando").exists(),
                "sobrou cópia temporária"
            );
            drop(aberta);
            let volta = rollback_at(&dados.0).expect("depois de fechar, conclui");
            assert!(volta.restored);
        } else {
            assert!(tentativa.expect("em Unix a troca é permitida").restored);
            drop(aberta);
        }
        assert_eq!(dados.universos(), vec!["Reino de Aether".to_string()]);
        assert!(read_marker(&dados.0).expect("marcador").is_none());
    }

    #[test]
    fn marcador_gravado_e_removido_sem_deixar_temporario() {
        let dados = Dados::novo();
        let marker = MigrationMarker {
            backup_id: "x".into(),
            from_version: 1,
            to_version: 2,
        };
        write_marker(&dados.0, &marker).expect("gravar");
        write_marker(&dados.0, &marker).expect("regravar");
        assert_eq!(read_marker(&dados.0).expect("ler"), Some(marker));
        remove_marker(&dados.0).expect("remover");
        let restos: Vec<_> = fs::read_dir(&dados.0).expect("ler").collect();
        assert!(restos.is_empty(), "sobrou arquivo do marcador");
    }
}
