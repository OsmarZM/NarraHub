//! O estado do Sync V2, para quem está olhando a tela (etapa 14, fatia 2).
//!
//! ## A lacuna que esta fatia fecha
//!
//! Treze etapas construíram o motor e **nenhuma abriu uma porta**. O
//! `invoke_handler` do `lib.rs` registrava 108 comandos e nenhum era do V2:
//! sem estado, sem roster, sem pendência. O que o usuário alcançava ao
//! sincronizar era o V1 (`src-tauri/src/sync.rs`), que copia tabelas inteiras
//! sem passar pelo log de eventos.
//!
//! Este módulo é a primeira leitura do V2 que chega a uma tela.
//!
//! ## Só leitura, e de propósito
//!
//! Nada aqui escreve. Consulta o que o motor já mantém — roster, log,
//! cursores, conflitos, pendência de mídia — e devolve números. É o que
//! permite a fatia 2 entrar sem que exista transporte de produto ainda: a
//! escuta nasce na fatia 3, junto com o pareamento por PIN, que é o que dá a
//! ela algo para dizer. Uma escuta que aceita conexão e fecha seria porta
//! pintada na parede.
//!
//! ## `device_id` nunca é parâmetro
//!
//! Quem responde "que aparelho é este" é a identidade Ed25519 em arquivo, via
//! [`crate::application::sync_bootstrap::prepare`] — nunca um argumento de
//! quem chama. É a invariante das etapas 2.5 e 8 levada à fronteira: um
//! `device_id` que atravessasse o IPC seria um `&str` escolhido pelo chamador,
//! e o roster autoriza por identidade, não por afirmação.

use crate::database::error::DatabaseCommandResult;
use crate::domain::identity::DeviceIdentity;
use crate::infrastructure::sqlite::SqliteDatabase;
use rusqlite::Connection;
use serde::Serialize;

/// Um aparelho do roster, como a tela precisa ver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AparelhoConhecido {
    pub device_id: String,
    pub nome: String,
    /// `active`, `retired` ou `revoked` — o `CHECK` da tabela é a fonte.
    pub estado: String,
    /// Este aparelho, entre os do roster.
    pub e_este_aparelho: bool,
    /// Quem introduziu. Vazio quando o pareamento foi direto.
    pub introduzido_por: String,
    /// Até onde o conteúdo desta origem chegou, e por onde.
    pub baseline: i64,
    pub ultimo_aplicado: i64,
}

/// O que ainda não foi aplicado, por origem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendenciaDeOrigem {
    pub device_id: String,
    pub nome: String,
    /// Eventos daquela origem no log que ainda não foram aplicados.
    pub eventos: i64,
    /// A menor sequência que falta. É a lacuna que impede o cursor de andar.
    pub primeira_sequencia: i64,
}

/// O estado do Sync V2 neste aparelho.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Panorama {
    /// Derivado da chave pública, nunca recebido de fora.
    pub device_id: String,
    pub chave_publica: String,
    /// O roster inteiro, este aparelho incluído.
    pub aparelhos: Vec<AparelhoConhecido>,
    /// Eventos que nasceram aqui. É o outbox — que é consulta, não tabela.
    pub eventos_deste_aparelho: i64,
    /// Eventos de outras origens ainda não aplicados, somados.
    pub eventos_pendentes: i64,
    pub pendentes_por_origem: Vec<PendenciaDeOrigem>,
    /// Conflitos de capítulo esperando decisão humana.
    pub conflitos_abertos: i64,
    /// Pendências de mídia da etapa 13 que continuam abertas.
    ///
    /// Entra aqui porque afeta o que consegue **sair** deste aparelho: um
    /// documento com mídia inline não passa em `update_chapter`, e o escritor
    /// precisa de um lugar onde ver que isso existe.
    pub pendencias_de_midia: i64,
}

/// Lê o estado do V2 sem escrever nada.
///
/// A identidade vem de fora — de [`crate::application::sync_bootstrap::prepare`],
/// que o `AppHandle` já resolve a cada comando. Recebê-la em vez de derivá-la
/// aqui é o que mantém este módulo sem acesso a disco e testável com um banco
/// temporário.
pub fn panorama(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<Panorama> {
    let connection = database.read()?;
    let eu = identidade.device_id().to_string();

    Ok(Panorama {
        device_id: eu.clone(),
        chave_publica: identidade.public_base32(),
        aparelhos: aparelhos(&connection, &eu)?,
        eventos_deste_aparelho: contar(
            &connection,
            "SELECT COUNT(*) FROM sync_events WHERE device_id = ?1",
            [&eu],
        )?,
        eventos_pendentes: contar(
            &connection,
            "SELECT COUNT(*)
               FROM sync_events e
              WHERE e.device_id <> ?1
                AND NOT EXISTS (
                      SELECT 1 FROM sync_applied_events a WHERE a.event_id = e.event_id
                    )",
            [&eu],
        )?,
        pendentes_por_origem: pendentes_por_origem(&connection, &eu)?,
        conflitos_abertos: contar(
            &connection,
            "SELECT COUNT(*) FROM sync_conflicts WHERE resolved_at = ''",
            [],
        )?,
        pendencias_de_midia: contar(
            &connection,
            "SELECT COUNT(*) FROM blob_migration_issues WHERE resolved_at = ''",
            [],
        )?,
    })
}

fn contar<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    parametros: P,
) -> DatabaseCommandResult<i64> {
    connection
        .query_row(sql, parametros, |row| row.get(0))
        .map_err(|erro| crate::database::error::DatabaseCommandError::storage(erro.to_string()))
}

fn aparelhos(connection: &Connection, eu: &str) -> DatabaseCommandResult<Vec<AparelhoConhecido>> {
    let mut consulta = connection
        .prepare(
            "SELECT d.device_id, d.name, d.state, d.introduced_by,
                    COALESCE(c.baseline_seq, 0), COALESCE(c.last_seq_applied, 0)
               FROM sync_devices d
               LEFT JOIN sync_cursors c ON c.origin_device_id = d.device_id
              ORDER BY d.is_self DESC, d.added_at, d.device_id",
        )
        .map_err(|erro| crate::database::error::DatabaseCommandError::storage(erro.to_string()))?;

    let linhas = consulta
        .query_map([], |row| {
            let device_id: String = row.get(0)?;
            Ok(AparelhoConhecido {
                e_este_aparelho: device_id == eu,
                device_id,
                nome: row.get(1)?,
                estado: row.get(2)?,
                introduzido_por: row.get(3)?,
                baseline: row.get(4)?,
                ultimo_aplicado: row.get(5)?,
            })
        })
        .and_then(|linhas| linhas.collect::<Result<Vec<_>, _>>())
        .map_err(|erro| crate::database::error::DatabaseCommandError::storage(erro.to_string()))?;

    Ok(linhas)
}

fn pendentes_por_origem(
    connection: &Connection,
    eu: &str,
) -> DatabaseCommandResult<Vec<PendenciaDeOrigem>> {
    let mut consulta = connection
        .prepare(
            "SELECT e.device_id, COALESCE(d.name, ''), COUNT(*), MIN(e.seq)
               FROM sync_events e
               LEFT JOIN sync_devices d ON d.device_id = e.device_id
              WHERE e.device_id <> ?1
                AND NOT EXISTS (
                      SELECT 1 FROM sync_applied_events a WHERE a.event_id = e.event_id
                    )
              GROUP BY e.device_id
              ORDER BY e.device_id",
        )
        .map_err(|erro| crate::database::error::DatabaseCommandError::storage(erro.to_string()))?;

    let linhas = consulta
        .query_map([eu], |row| {
            Ok(PendenciaDeOrigem {
                device_id: row.get(0)?,
                nome: row.get(1)?,
                eventos: row.get(2)?,
                primeira_sequencia: row.get(3)?,
            })
        })
        .and_then(|linhas| linhas.collect::<Result<Vec<_>, _>>())
        .map_err(|erro| crate::database::error::DatabaseCommandError::storage(erro.to_string()))?;

    Ok(linhas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync_bootstrap;
    use crate::domain::sync::{AggregateRef, Operation};
    use crate::infrastructure::sqlite::sync_repository::{append_local_event, LocalChange};
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;

    struct Aparelho {
        banco: TemporaryDatabase,
        dados: std::path::PathBuf,
        identidade: DeviceIdentity,
    }

    impl Aparelho {
        fn novo() -> Self {
            let dados =
                std::env::temp_dir().join(format!("narrahub-panorama-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dados).expect("criar diretório");
            let banco = TemporaryDatabase::new();
            let identidade = sync_bootstrap::prepare(&dados, &banco.database).expect("arranque");
            Self {
                banco,
                dados,
                identidade,
            }
        }

        fn panorama(&self) -> Panorama {
            panorama(&self.banco.database, &self.identidade).expect("panorama")
        }

        fn escrever_evento(&self, id: &str) {
            let mut connection = self.banco.database.write().expect("conexão");
            append_local_event(
                &mut connection,
                &self.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", id),
                    operation: Operation::Upsert,
                    payload: r#"{"t":"a"}"#,
                },
            )
            .expect("evento local");
        }
    }

    impl Drop for Aparelho {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dados);
        }
    }

    /// **O `device_id` do panorama vem da identidade, não de parâmetro.**
    ///
    /// É a invariante das etapas 2.5 e 8 na fronteira: a função não tem por
    /// onde receber um `device_id`, e o que ela devolve é derivado da chave
    /// pública. Um panorama que aceitasse o id de quem pergunta seria um
    /// `&str` escolhido pelo chamador num lugar em que o roster autoriza por
    /// identidade.
    #[test]
    fn o_device_id_do_panorama_e_derivado_da_chave_e_nao_recebido() {
        let aparelho = Aparelho::novo();
        let visto = aparelho.panorama();

        assert_eq!(visto.device_id, aparelho.identidade.device_id());
        assert_eq!(visto.chave_publica, aparelho.identidade.public_base32());

        // Dois aparelhos distintos não colidem — o id acompanha a chave.
        let outro = Aparelho::novo();
        assert_ne!(visto.device_id, outro.panorama().device_id);
    }

    /// Aparelho novo: o roster tem só ele, e nada está pendente.
    ///
    /// O gate existe porque "zero" e "não olhei" se parecem na tela, e a
    /// primeira impressão do usuário sobre a sincronização é justamente esta.
    #[test]
    fn aparelho_recem_nascido_tem_so_a_si_mesmo_no_roster() {
        let aparelho = Aparelho::novo();
        let visto = aparelho.panorama();

        assert_eq!(visto.aparelhos.len(), 1, "o `self` tinha que estar lá");
        let eu = &visto.aparelhos[0];
        assert!(eu.e_este_aparelho);
        assert_eq!(eu.device_id, visto.device_id);
        assert_eq!(eu.estado, "active");
        assert_eq!(eu.introduzido_por, "", "pareamento direto não tem padrinho");

        assert_eq!(visto.eventos_deste_aparelho, 0);
        assert_eq!(visto.eventos_pendentes, 0);
        assert!(visto.pendentes_por_origem.is_empty());
        assert_eq!(visto.conflitos_abertos, 0);
        assert_eq!(visto.pendencias_de_midia, 0);
    }

    /// **O outbox é consulta, não tabela — e o panorama conta certo.**
    ///
    /// Evento que nasce aqui entra em `eventos_deste_aparelho`, e **não** em
    /// pendentes: o próprio aparelho não espera por si mesmo. Foi o que a
    /// etapa 3 decidiu quando dispensou a tabela de outbox, e o panorama tem
    /// que dizer a mesma coisa.
    #[test]
    fn evento_local_conta_como_meu_e_nunca_como_pendente() {
        let aparelho = Aparelho::novo();
        aparelho.escrever_evento("u1");
        aparelho.escrever_evento("u2");

        let visto = aparelho.panorama();
        assert_eq!(visto.eventos_deste_aparelho, 2);
        assert_eq!(
            visto.eventos_pendentes, 0,
            "evento próprio não é pendência: o aparelho não espera por si"
        );
        assert!(visto.pendentes_por_origem.is_empty());
    }

    /// Pendência de mídia da etapa 13 aparece no panorama.
    ///
    /// Não é curiosidade: documento com mídia inline não passa em
    /// `update_chapter`, então isto explica por que uma gravação foi recusada.
    #[test]
    fn pendencia_de_midia_aberta_aparece_e_a_resolvida_nao() {
        let aparelho = Aparelho::novo();
        let connection = aparelho.banco.connection();
        connection
            .execute_batch(
                "INSERT INTO blob_migration_issues
                        (id, surface, table_name, row_id, side, reason, detail)
                    VALUES ('i1', 7, 'chapters', 'cap1', 'content', 'legacy_unrecognized', 'nao abre');
                 INSERT INTO blob_migration_issues
                        (id, surface, table_name, row_id, side, reason, detail, resolved_at)
                    VALUES ('i2', 7, 'chapters', 'cap2', 'content', 'legacy_unrecognized', 'ja resolvida',
                            '2026-01-01');",
            )
            .expect("semear pendências");

        assert_eq!(
            aparelho.panorama().pendencias_de_midia,
            1,
            "só a aberta conta"
        );
    }

    /// Conflito aberto aparece; conflito resolvido sai.
    #[test]
    fn conflito_aberto_aparece_e_o_resolvido_sai() {
        let aparelho = Aparelho::novo();
        let visto_antes = aparelho.panorama();
        assert_eq!(visto_antes.conflitos_abertos, 0);

        let connection = aparelho.banco.connection();
        connection
            .execute_batch(
                "INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value,
                         resolved_at)
                    VALUES ('c1','chapter','cap1','content','a','b','');
                 INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value,
                         resolved_at)
                    VALUES ('c2','chapter','cap2','content','a','b','2026-01-02');",
            )
            .expect("semear conflitos");

        assert_eq!(aparelho.panorama().conflitos_abertos, 1);
    }
}
