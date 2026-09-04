//! Onde a chave privada mora: em arquivo, **fora do banco**.
//!
//! ADR 0009 §5. O banco vai para backup e backup vai para pendrive; dois
//! aparelhos com a mesma identidade produziriam sequências conflitantes sob o
//! mesmo `device_id`, que é corrupção silenciosa do cursor de todos os
//! outros.
//!
//! ## O caso que essa separação cria: backup restaurado em outra máquina
//!
//! A assimetria é consequência direta da decisão acima:
//!
//! ```text
//! chave privada   fora do backup
//! sync_devices    dentro do backup   →  o banco restaurado carrega
//!                                       old-desktop com is_self = 1
//! ```
//!
//! O banco afirma ser um dispositivo cuja chave não existe mais ali. Deixar
//! assim seria pior que um erro de arranque: o aparelho novo assinaria com
//! uma chave enquanto se apresenta com o `device_id` de outro, e os peers
//! recusariam tudo — ou pior, aceitariam e embaralhariam duas sequências sob
//! a mesma origem.
//!
//! A reconciliação rebaixa o `self` antigo e registra a identidade nova. O
//! `seq` dela começa em 1 porque é derivado de `MAX(seq)` **da própria
//! origem**, e a origem é nova — que é o argumento que fecha a decisão de não
//! guardar contador.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::identity::{base32, decode_base32};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const IDENTITY_FILE_NAME: &str = "sync-identity.json";

/// O arquivo é versionado desde a primeira gravação.
///
/// A chave X25519 do Noise (etapa 8) vai morar aqui também, e um formato sem
/// versão obrigaria a adivinhar o que fazer com o arquivo antigo.
const FORMATO: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ArquivoDeIdentidade {
    format_version: u32,
    device_id: String,
    ed25519_secret: String,
    ed25519_public: String,
}

pub fn identity_path(app_data: &Path) -> PathBuf {
    app_data.join(IDENTITY_FILE_NAME)
}

/// Carrega a identidade do disco, gerando na primeira execução.
pub fn load_or_create(app_data: &Path) -> DatabaseCommandResult<DeviceIdentity> {
    let caminho = identity_path(app_data);
    if caminho.is_file() {
        return carregar(&caminho);
    }
    let identidade = DeviceIdentity::generate();
    gravar(&caminho, &identidade)?;
    Ok(identidade)
}

fn carregar(caminho: &Path) -> DatabaseCommandResult<DeviceIdentity> {
    let bruto = std::fs::read_to_string(caminho).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "Não foi possível ler a identidade de sincronização: {error}"
        ))
    })?;
    let arquivo: ArquivoDeIdentidade = serde_json::from_str(&bruto).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "A identidade de sincronização está ilegível: {error}"
        ))
    })?;

    if arquivo.format_version != FORMATO {
        return Err(DatabaseCommandError::storage(format!(
            "A identidade de sincronização está no formato {} e este aplicativo entende o {FORMATO}.",
            arquivo.format_version
        )));
    }

    let segredo = decode_base32(&arquivo.ed25519_secret)
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
        .ok_or_else(|| {
            DatabaseCommandError::storage(
                "A chave privada de sincronização está corrompida. Apagar o arquivo gera uma \
                 identidade nova, e o aparelho precisará ser pareado de novo.",
            )
        })?;

    let identidade = DeviceIdentity::from_secret_bytes(segredo);

    // O arquivo guarda `device_id` e a pública por conveniência de leitura,
    // mas quem manda é a privada. Divergência aqui é adulteração ou edição à
    // mão, e seguir em frente gravaria eventos com uma origem que não bate
    // com a assinatura.
    if identidade.device_id() != arquivo.device_id {
        return Err(DatabaseCommandError::storage(
            "A identidade de sincronização é inconsistente: o device_id gravado não deriva da \
             chave privada do arquivo.",
        ));
    }

    Ok(identidade)
}

fn gravar(caminho: &Path, identidade: &DeviceIdentity) -> DatabaseCommandResult<()> {
    let arquivo = ArquivoDeIdentidade {
        format_version: FORMATO,
        device_id: identidade.device_id().to_string(),
        ed25519_secret: base32(&identidade.secret_bytes()),
        ed25519_public: identidade.public_base32(),
    };
    let conteudo = serde_json::to_string_pretty(&arquivo)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai)
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    }
    std::fs::write(caminho, conteudo).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "Não foi possível gravar a identidade de sincronização: {error}"
        ))
    })?;
    restringir_permissoes(caminho);
    Ok(())
}

#[cfg(unix)]
fn restringir_permissoes(caminho: &Path) {
    use std::os::unix::fs::PermissionsExt;
    // 0600: só o dono lê. Falhar aqui não impede o app de funcionar, e
    // derrubar o arranque por causa de permissão seria pior que o risco.
    let _ = std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restringir_permissoes(_caminho: &Path) {
    // No Windows o diretório de dados do app já é do usuário, e no Android o
    // armazenamento privado do aplicativo idem. Um ACL explícito aqui não
    // acrescentaria garantia e acrescentaria caminho de erro.
}

/// O que a reconciliação entre o arquivo e o banco encontrou.
#[derive(Debug, PartialEq, Eq)]
pub enum Reconciliacao {
    /// O banco já reconhecia esta identidade.
    JaEra,
    /// Primeiro arranque: nenhum `self` no banco ainda.
    Registrado,
    /// Banco restaurado de outro aparelho. O `self` antigo foi rebaixado.
    SelfRebindado { anterior: String },
}

/// Garante que o `self` de `sync_devices` seja a identidade do arquivo.
pub fn reconcile_self(
    connection: &mut Connection,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<Reconciliacao> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let anterior: Option<String> = tx
        .query_row(
            "SELECT device_id FROM sync_devices WHERE is_self = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let resultado = match anterior.as_deref() {
        Some(atual) if atual == identidade.device_id() => Reconciliacao::JaEra,
        Some(antigo) => {
            // Rebaixa, não apaga: os eventos daquela origem continuam no log,
            // e a FK de `sync_events` exige que a linha permaneça. Apagar
            // reescreveria a história de um aparelho que existiu de verdade.
            tx.execute(
                "UPDATE sync_devices
                    SET is_self = 0, state = 'retired', state_changed_at = datetime('now')
                  WHERE device_id = ?1",
                [antigo],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            Reconciliacao::SelfRebindado {
                anterior: antigo.to_string(),
            }
        }
        None => Reconciliacao::Registrado,
    };

    if resultado != Reconciliacao::JaEra {
        tx.execute(
            "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
             VALUES (?1, '', ?2, 1)
             ON CONFLICT(device_id) DO UPDATE SET
                 is_self = 1,
                 state = 'active',
                 ed25519_public = excluded.ed25519_public",
            rusqlite::params![identidade.device_id(), identidade.public_base32()],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    }

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(resultado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;

    struct DiretorioTemporario(PathBuf);

    impl DiretorioTemporario {
        fn novo() -> Self {
            let caminho =
                std::env::temp_dir().join(format!("narrahub-identidade-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&caminho).expect("criar diretório de teste");
            Self(caminho)
        }
    }

    impl Drop for DiretorioTemporario {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_identidade_nasce_uma_vez_e_persiste() {
        let dir = DiretorioTemporario::novo();
        let primeira = load_or_create(&dir.0).expect("gerar identidade");
        let segunda = load_or_create(&dir.0).expect("reler identidade");
        assert_eq!(
            primeira.device_id(),
            segunda.device_id(),
            "reabrir o app não pode trocar a identidade do aparelho"
        );
        assert_eq!(primeira.secret_bytes(), segunda.secret_bytes());
    }

    /// GATE DO ADR 0009 §5: a privada não mora no banco.
    #[test]
    fn a_chave_privada_fica_fora_do_banco() {
        let dir = DiretorioTemporario::novo();
        let identidade = load_or_create(&dir.0).expect("gerar identidade");
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");
        reconcile_self(&mut connection, &identidade).expect("registrar self");

        let segredo = base32(&identidade.secret_bytes());
        let mut nomes = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("listar tabelas");
        let tabelas: Vec<String> = nomes
            .query_map([], |row| row.get(0))
            .expect("consultar")
            .collect::<Result<_, _>>()
            .expect("ler");
        drop(nomes);

        for tabela in tabelas {
            let mut colunas = connection
                .prepare(&format!("SELECT * FROM {tabela} LIMIT 200"))
                .expect("preparar varredura");
            let encontrou = colunas
                .query_map([], |row| {
                    let mut achou = false;
                    let mut indice = 0;
                    while let Ok(valor) = row.get::<_, rusqlite::types::Value>(indice) {
                        if let rusqlite::types::Value::Text(texto) = valor {
                            achou |= texto.contains(&segredo);
                        }
                        indice += 1;
                    }
                    Ok(achou)
                })
                .expect("varrer")
                .any(|linha| linha.unwrap_or(false));
            assert!(
                !encontrou,
                "a chave privada apareceu na tabela {tabela} — o banco vai para backup"
            );
        }
    }

    #[test]
    fn o_primeiro_arranque_registra_o_self() {
        let dir = DiretorioTemporario::novo();
        let identidade = load_or_create(&dir.0).expect("gerar identidade");
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");

        assert_eq!(
            reconcile_self(&mut connection, &identidade).expect("registrar"),
            Reconciliacao::Registrado
        );
        assert_eq!(
            reconcile_self(&mut connection, &identidade).expect("repetir"),
            Reconciliacao::JaEra,
            "reconciliar duas vezes não pode mexer em nada"
        );

        let (id, publica): (String, String) = connection
            .query_row(
                "SELECT device_id, ed25519_public FROM sync_devices WHERE is_self = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler self");
        assert_eq!(id, identidade.device_id());
        assert_eq!(publica, identidade.public_base32());
    }

    /// GATE DO RESTORE: backup de outro aparelho não continua sendo o `self`.
    ///
    /// Sem isto, o aparelho novo assinaria com a própria chave enquanto se
    /// apresenta com o `device_id` de outro — e o `seq` continuaria de onde o
    /// aparelho antigo parou, embaralhando duas sequências sob a mesma origem.
    #[test]
    fn banco_restaurado_de_outro_aparelho_rebinda_o_self() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");

        // O banco chega do backup já afirmando ser outro aparelho.
        connection
            .execute(
                "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                 VALUES ('old-desktop', 'Desktop antigo', 'CHAVEANTIGA', 1)",
                [],
            )
            .expect("semear self antigo");

        let dir = DiretorioTemporario::novo();
        let nova = load_or_create(&dir.0).expect("identidade da máquina nova");

        assert_eq!(
            reconcile_self(&mut connection, &nova).expect("reconciliar"),
            Reconciliacao::SelfRebindado {
                anterior: "old-desktop".into()
            }
        );

        let selfs: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_devices WHERE is_self = 1",
                [],
                |row| row.get(0),
            )
            .expect("contar selfs");
        assert_eq!(selfs, 1, "só pode existir um self");

        let atual: String = connection
            .query_row(
                "SELECT device_id FROM sync_devices WHERE is_self = 1",
                [],
                |row| row.get(0),
            )
            .expect("ler self");
        assert_eq!(atual, nova.device_id());

        // O aparelho antigo continua no roster, aposentado. Apagar
        // reescreveria a história de um aparelho que existiu de verdade — e a
        // FK de `sync_events` depende dessa linha.
        let estado: String = connection
            .query_row(
                "SELECT state FROM sync_devices WHERE device_id = 'old-desktop'",
                [],
                |row| row.get(0),
            )
            .expect("ler estado do antigo");
        assert_eq!(estado, "retired");
    }

    #[test]
    fn arquivo_adulterado_e_recusado_em_vez_de_aceito() {
        let dir = DiretorioTemporario::novo();
        load_or_create(&dir.0).expect("gerar identidade");
        let caminho = identity_path(&dir.0);

        let bruto = std::fs::read_to_string(&caminho).expect("ler");
        let trocado = bruto.replace(
            &bruto[bruto.find("\"device_id\": \"").unwrap() + 14
                ..bruto.find("\"device_id\": \"").unwrap() + 20],
            "AAAAAA",
        );
        std::fs::write(&caminho, trocado).expect("gravar adulterado");

        let erro =
            load_or_create(&dir.0).expect_err("device_id que não deriva da chave é recusado");
        assert!(
            erro.to_string().contains("inconsistente"),
            "recusou pelo motivo errado: {erro}"
        );
    }
}
