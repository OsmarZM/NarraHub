//! O arranque que faz a identidade existir antes da primeira escrita.
//!
//! ADR 0009, etapa 3. Sem isto, `load_or_create` e `reconcile_self` são
//! infraestrutura sem chamador — funcionam em teste e nunca rodam no
//! aplicativo. A revisão da etapa 2.5 apontou exatamente essa lacuna.
//!
//! ```text
//! ARRANQUE DO APP
//!    │
//!    ▼
//! load_or_create(identidade)      arquivo, fora do banco
//!    │
//!    ▼
//! reconcile_self(banco)           o `self` do roster passa a ser esta chave
//!    │
//!    ▼
//! DeviceIdentity disponível
//!    │
//!    ▼
//! serviço de aplicação
//!    ├── alteração de domínio
//!    └── evento assinado          NA MESMA TRANSAÇÃO
//! ```
//!
//! ## O que acontece se alguém pular a ordem
//!
//! Nada silencioso, e isso é de propósito. `append_event_in_transaction`
//! exige que a identidade seja o `self` registrado; sem a reconciliação, a
//! escrita **falha** em vez de gravar um evento órfão ou sem assinatura. A
//! sequência acima é o que faz o caminho feliz existir, não o que impede o
//! caminho errado — quem impede é o banco.

use crate::database::error::DatabaseCommandResult;
use crate::domain::identity::DeviceIdentity;
use crate::infrastructure::identity_store;
use crate::infrastructure::sqlite::SqliteDatabase;
use std::path::Path;

/// Deixa o aparelho pronto para produzir eventos assinados.
///
/// Idempotente: rodar de novo no mesmo aparelho não muda nada. Precisa rodar
/// **também depois de uma restauração de backup**, porque o banco restaurado
/// chega afirmando ser outro aparelho — a identidade em arquivo não muda, o
/// banco muda debaixo dela.
pub fn prepare(
    app_data: &Path,
    database: &SqliteDatabase,
) -> DatabaseCommandResult<DeviceIdentity> {
    let identidade = identity_store::load_or_create(app_data)?;
    let mut connection = database.write()?;
    identity_store::reconcile_self(&mut connection, &identidade)?;
    Ok(identidade)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sync::{AggregateRef, Operation};
    use crate::infrastructure::sqlite::sync_repository::{append_local_event, LocalChange};
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;

    struct DiretorioTemporario(std::path::PathBuf);

    impl DiretorioTemporario {
        fn novo() -> Self {
            let caminho =
                std::env::temp_dir().join(format!("narrahub-boot-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&caminho).expect("criar diretório");
            Self(caminho)
        }
    }

    impl Drop for DiretorioTemporario {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn mudanca<'a>(id: &'a str) -> LocalChange<'a> {
        LocalChange {
            universe_id: "u1",
            aggregate: AggregateRef::new("chapter", id),
            operation: Operation::Upsert,
            payload: r#"{"t":"a"}"#,
        }
    }

    /// GATE DA ETAPA 3 — a sequência de arranque, cobrada passo a passo.
    ///
    /// Antes da primeira escrita sincronizável:
    ///
    /// 1. a identidade existe no disco;
    /// 2. a identidade foi carregada;
    /// 3. `reconcile_self` rodou;
    /// 4. `sync_devices` tem exatamente um `self`;
    /// 5. o `self` corresponde à chave;
    /// 6. e só então a transação de domínio produz evento.
    #[test]
    fn a_sequencia_de_arranque_deixa_o_aparelho_pronto_para_escrever() {
        let dir = DiretorioTemporario::novo();
        let temporario = TemporaryDatabase::new();

        // 1 e 2 — antes do arranque não há identidade no disco.
        assert!(
            !identity_store::identity_path(&dir.0).is_file(),
            "o teste começou com identidade pronta e não provaria nada"
        );

        let identidade = prepare(&dir.0, &temporario.database).expect("arranque");

        assert!(
            identity_store::identity_path(&dir.0).is_file(),
            "a identidade precisa ficar no disco: ela é o que o aparelho É"
        );

        let connection = temporario.database.read().expect("abrir leitura");

        // 4 — exatamente um self.
        let selfs: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_devices WHERE is_self = 1",
                [],
                |row| row.get(0),
            )
            .expect("contar selfs");
        assert_eq!(selfs, 1);

        // 5 — e ele corresponde à chave carregada.
        let (id, publica): (String, String) = connection
            .query_row(
                "SELECT device_id, ed25519_public FROM sync_devices WHERE is_self = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler self");
        assert_eq!(id, identidade.device_id());
        assert_eq!(publica, identidade.public_base32());
        drop(connection);

        // 6 — a escrita passa a produzir evento assinado.
        let mut connection = temporario.database.write().expect("abrir escrita");
        let envelope = append_local_event(&mut connection, &identidade, &mudanca("cap-1"))
            .expect("depois do arranque a escrita sincronizável funciona");
        assert!(crate::domain::identity::verify(
            &envelope,
            &identidade.public_base32()
        ));
    }

    /// E sem o arranque, a escrita **falha** em vez de gravar sem evento.
    ///
    /// É o outro lado do gate: a ordem não é uma convenção que alguém precisa
    /// lembrar, é uma pré-condição que o banco cobra.
    #[test]
    fn sem_o_arranque_a_escrita_sincronizavel_nao_acontece() {
        let dir = DiretorioTemporario::novo();
        let temporario = TemporaryDatabase::new();

        // A identidade existe em disco, mas `reconcile_self` não rodou.
        let identidade = identity_store::load_or_create(&dir.0).expect("gerar identidade");

        let mut connection = temporario.database.write().expect("abrir escrita");
        append_local_event(&mut connection, &identidade, &mudanca("cap-1"))
            .expect_err("sem reconciliação não há `self`, e sem `self` não há origem");

        let eventos: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar eventos");
        assert_eq!(eventos, 0);
    }

    #[test]
    fn o_arranque_e_idempotente() {
        let dir = DiretorioTemporario::novo();
        let temporario = TemporaryDatabase::new();

        let primeira = prepare(&dir.0, &temporario.database).expect("primeiro arranque");
        let segunda = prepare(&dir.0, &temporario.database).expect("segundo arranque");
        assert_eq!(primeira.device_id(), segunda.device_id());

        let connection = temporario.database.read().expect("abrir leitura");
        let dispositivos: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_devices", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(
            dispositivos, 1,
            "reabrir o app não pode criar dispositivo novo"
        );
    }

    /// Arranque depois de restaurar backup de outro aparelho.
    ///
    /// A identidade em arquivo não muda; o banco muda debaixo dela. Por isso
    /// `prepare` precisa rodar de novo depois de uma restauração, e não só no
    /// primeiro boot do processo.
    #[test]
    fn o_arranque_depois_de_restaurar_backup_reassume_a_identidade_local() {
        let dir = DiretorioTemporario::novo();
        let temporario = TemporaryDatabase::new();
        let identidade = prepare(&dir.0, &temporario.database).expect("arranque");

        // Chega um banco restaurado, afirmando ser outro aparelho.
        {
            let connection = temporario.database.write().expect("abrir escrita");
            connection
                .execute(
                    "UPDATE sync_devices SET is_self = 0 WHERE device_id = ?1",
                    [identidade.device_id()],
                )
                .expect("rebaixar");
            connection
                .execute(
                    "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                     VALUES ('old-desktop', 'Antigo', 'CHAVEANTIGA', 1)",
                    [],
                )
                .expect("semear self alheio");
        }

        let depois = prepare(&dir.0, &temporario.database).expect("arranque pós-restauração");
        assert_eq!(
            depois.device_id(),
            identidade.device_id(),
            "a identidade é do aparelho, não do acervo: restaurar não a troca"
        );

        let connection = temporario.database.read().expect("abrir leitura");
        let atual: String = connection
            .query_row(
                "SELECT device_id FROM sync_devices WHERE is_self = 1",
                [],
                |row| row.get(0),
            )
            .expect("ler self");
        assert_eq!(atual, identidade.device_id());
    }
}
