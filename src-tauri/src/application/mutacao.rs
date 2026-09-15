//! A fronteira única entre mutação de domínio e Sync V2 (NH-079, etapa B1).
//!
//! ## Invariante
//!
//! ```text
//! mutação sincronizável confirmada
//! ⇔
//! domínio + estado causal (aggregate_state, revision_history, tombstone) + evento
//! commitados na MESMA transação IMMEDIATE
//!
//! após qualquer queda: tudo existe, ou nada existe
//! ```
//!
//! ## Contrato
//!
//! ```text
//! Mutacao::executar(database, identidade, |m| { ... })
//!   BEGIN IMMEDIATE
//!   closure:
//!     m.tx()                  a transação; repositórios recebem ESTA, e só ela
//!     m.gravou(tipo, id)      create/update — o estado é lido no fim, depois da escrita
//!     m.excluir(tipo, id)     ANTES do DELETE: afetados, estado causal, preflight, eventos preparados
//!   fim:
//!     exclusões: confere que cada afetado sumiu
//!     emite eventos na ordem declarada (descendentes antes do pai)
//!   COMMIT
//! ```
//!
//! **Uma ação, um conjunto coerente de revisões.** O serviço declara agregados, não SQLs: cinco
//! `UPDATE` num mesmo agregado viram uma revisão, lida do estado final.
//!
//! ## Por que a exclusão é outra sequência
//!
//! Depois de `DELETE FROM chapters`, o gatilho `trg_chapter_attachments_delete` já apagou os anexos,
//! e ninguém consegue mais saber quais eram. Por isso [`Mutacao::excluir`] roda **antes** do SQL
//! destrutivo: descobre os descendentes, recusa se algum tiver estado concorrente, e deixa os eventos
//! de exclusão prontos. Descobrir os filhos depois da cascata é proibido por construção.
//!
//! ## O que ela não é
//!
//! Não é savepoint: `executar` dentro de `executar` é erro. Não guarda transação entre chamadas.
//! Não conhece Tauri. Não decide nada de domínio — isso continua no serviço.

use std::cell::Cell;

use rusqlite::{Transaction, TransactionBehavior};

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::sync::{AggregateRef, Operation};
use crate::infrastructure::sqlite::sync_codec::{self, EstadoConcorrente};
use crate::infrastructure::sqlite::sync_repository::{append_event_in_transaction, LocalChange};
use crate::infrastructure::sqlite::SqliteDatabase;

thread_local! {
    static EM_MUTACAO: Cell<bool> = const { Cell::new(false) };
}

/// Solta a marca de "dentro de uma mutação" em qualquer saída, inclusive pânico.
struct Marca;

impl Marca {
    fn entrar() -> DatabaseCommandResult<Self> {
        if EM_MUTACAO.with(Cell::get) {
            return Err(DatabaseCommandError::storage(
                "Mutacao::executar foi chamada dentro de outra mutação. Uma ação de domínio é uma \
                 transação só; aninhar esconderia um commit parcial.",
            ));
        }
        EM_MUTACAO.with(|marca| marca.set(true));
        Ok(Marca)
    }
}

impl Drop for Marca {
    fn drop(&mut self) {
        EM_MUTACAO.with(|marca| marca.set(false));
    }
}

#[derive(Debug, Clone)]
enum Operacao {
    Gravou(AggregateRef),
    Excluiu {
        agregado: AggregateRef,
        universe_id: String,
    },
}

/// A mutação em andamento. Só existe dentro de [`Mutacao::executar`].
pub struct Mutacao<'t, 'c> {
    tx: &'t Transaction<'c>,
    operacoes: Vec<Operacao>,
}

impl<'t, 'c> Mutacao<'t, 'c> {
    /// Abre a transação, roda a ação, emite as revisões e confirma — ou não confirma nada.
    pub fn executar<T>(
        database: &SqliteDatabase,
        identidade: &DeviceIdentity,
        acao: impl FnOnce(&mut Mutacao<'_, '_>) -> DatabaseCommandResult<T>,
    ) -> DatabaseCommandResult<T> {
        let _marca = Marca::entrar()?;
        let mut connection = database.write()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

        let valor = {
            let mut mutacao = Mutacao {
                tx: &tx,
                operacoes: Vec::new(),
            };
            let valor = acao(&mut mutacao)?;
            mutacao.finalizar(identidade)?;
            valor
        };

        falha::verificar(falha::Ponto::AntesDoCommit)?;
        tx.commit()
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        Ok(valor)
    }

    /// A transação da mutação. É a única que os repositórios usados aqui podem receber.
    pub fn tx(&self) -> &Transaction<'c> {
        self.tx
    }

    /// Declara que o agregado foi criado ou alterado. O estado é lido no fim, na mesma transação.
    pub fn gravou(&mut self, tipo: &str, id: &str) -> DatabaseCommandResult<()> {
        if !sync_codec::coberto(tipo) {
            return Err(sync_codec::nao_coberto(tipo));
        }
        self.operacoes
            .push(Operacao::Gravou(AggregateRef::new(tipo, id)));
        Ok(())
    }

    /// Prepara a exclusão **antes** do SQL destrutivo.
    ///
    /// Descobre o agregado e tudo o que a exclusão apagaria por chave estrangeira ou gatilho,
    /// recusa se algum deles tiver estado concorrente, e registra as exclusões na ordem em que os
    /// eventos precisam sair: do descendente mais profundo até o próprio agregado.
    ///
    /// Depois desta chamada, o serviço executa o `DELETE`.
    pub fn excluir(&mut self, tipo: &str, id: &str) -> DatabaseCommandResult<()> {
        let raiz = AggregateRef::new(tipo, id);
        if sync_codec::ler(self.tx, &raiz)?.is_none() {
            return Err(DatabaseCommandError::not_found(format!(
                "Não há {tipo} {id} para excluir."
            )));
        }

        let mut ordem = Vec::new();
        self.coletar(&raiz, &mut ordem)?;

        for agregado in &ordem {
            if let Some(motivo) = sync_codec::estado_concorrente(self.tx, agregado)? {
                let por_que = match motivo {
                    EstadoConcorrente::DivergenciaAberta => {
                        "tem duas versões esperando a sua decisão"
                    }
                    EstadoConcorrente::EventoPendente => {
                        "tem alterações de outro aparelho que ainda não puderam ser aplicadas"
                    }
                };
                return Err(DatabaseCommandError::conflict(format!(
                    "Não dá para excluir agora: {} {} {por_que}. Resolva isso antes; nada foi apagado.",
                    agregado.aggregate_type, agregado.aggregate_id
                )));
            }
        }

        for agregado in ordem {
            let ja = self.operacoes.iter().any(|operacao| {
                matches!(operacao, Operacao::Excluiu { agregado: existente, .. } if existente == &agregado)
            });
            if ja {
                continue;
            }
            let universe_id = sync_codec::ler(self.tx, &agregado)?
                .map(|estado| estado.universe_id)
                .unwrap_or_default();
            self.operacoes.push(Operacao::Excluiu {
                agregado,
                universe_id,
            });
        }
        Ok(())
    }

    /// Descendentes primeiro, depois o próprio agregado.
    fn coletar(
        &self,
        agregado: &AggregateRef,
        ordem: &mut Vec<AggregateRef>,
    ) -> DatabaseCommandResult<()> {
        for filho in sync_codec::descendentes(self.tx, agregado)? {
            self.coletar(&filho, ordem)?;
        }
        if !ordem.contains(agregado) {
            ordem.push(agregado.clone());
        }
        Ok(())
    }

    fn finalizar(self, identidade: &DeviceIdentity) -> DatabaseCommandResult<()> {
        falha::verificar(falha::Ponto::AposAMutacao)?;

        let excluidos: Vec<&AggregateRef> = self
            .operacoes
            .iter()
            .filter_map(|operacao| match operacao {
                Operacao::Excluiu { agregado, .. } => Some(agregado),
                Operacao::Gravou(_) => None,
            })
            .collect();

        let mut emitidos: Vec<AggregateRef> = Vec::new();
        for operacao in &self.operacoes {
            match operacao {
                Operacao::Gravou(agregado) => {
                    // Gravado e depois excluído na mesma ação: só a exclusão existe para os outros.
                    if excluidos.contains(&agregado) || emitidos.contains(agregado) {
                        continue;
                    }
                    let estado = sync_codec::ler(self.tx, agregado)?.ok_or_else(|| {
                        DatabaseCommandError::storage(format!(
                            "A mutação declarou {} {} como gravado, e ele não existe no fim da \
                             transação. Nada foi confirmado.",
                            agregado.aggregate_type, agregado.aggregate_id
                        ))
                    })?;
                    append_event_in_transaction(
                        self.tx,
                        identidade,
                        &LocalChange {
                            universe_id: &estado.universe_id,
                            aggregate: agregado.clone(),
                            operation: Operation::Upsert,
                            payload: &estado.payload,
                        },
                    )?;
                    emitidos.push(agregado.clone());
                }
                Operacao::Excluiu {
                    agregado,
                    universe_id,
                } => {
                    if sync_codec::ler(self.tx, agregado)?.is_some() {
                        return Err(DatabaseCommandError::storage(format!(
                            "A mutação preparou a exclusão de {} {}, e ele continua existindo no fim \
                             da transação. Nada foi confirmado.",
                            agregado.aggregate_type, agregado.aggregate_id
                        )));
                    }
                    append_event_in_transaction(
                        self.tx,
                        identidade,
                        &LocalChange {
                            universe_id,
                            aggregate: agregado.clone(),
                            operation: Operation::Delete,
                            payload: "",
                        },
                    )?;
                    emitidos.push(agregado.clone());
                }
            }
            if emitidos.len() == 1 {
                falha::verificar(falha::Ponto::DuranteOsEventos)?;
            }
        }
        Ok(())
    }
}

/// Falha injetada nos pontos críticos da fronteira. Só existe em build de teste; em produção é
/// uma função vazia que o compilador remove.
pub(crate) mod falha {
    use crate::database::error::DatabaseCommandResult;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Ponto {
        /// A ação terminou (domínio escrito), nenhum evento emitido.
        AposAMutacao,
        /// O primeiro evento foi gravado, os seguintes não.
        DuranteOsEventos,
        /// Domínio e eventos gravados, `COMMIT` ainda não.
        AntesDoCommit,
    }

    #[cfg(test)]
    thread_local! {
        static ARMADA: std::cell::Cell<Option<Ponto>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub fn armar(ponto: Option<Ponto>) {
        ARMADA.with(|armada| armada.set(ponto));
    }

    #[cfg(test)]
    pub fn verificar(ponto: Ponto) -> DatabaseCommandResult<()> {
        if ARMADA.with(|armada| armada.get()) == Some(ponto) {
            armar(None);
            return Err(crate::database::error::DatabaseCommandError::storage(
                format!("falha injetada em {ponto:?}"),
            ));
        }
        Ok(())
    }

    #[cfg(not(test))]
    #[inline(always)]
    pub fn verificar(_ponto: Ponto) -> DatabaseCommandResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::falha::{armar, Ponto};
    use super::*;
    use crate::application::{canvas_service, manuscript_service};
    use crate::domain::canvas::Attachment;
    use crate::domain::manuscript::ChapterUpdate;
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::sync_session::receber_eventos;
    use crate::infrastructure::sqlite::test_support::{
        origem_remota_confiavel, seed_universe, self_de_teste, TemporaryDatabase,
    };
    use crate::infrastructure::sqlite::{canvas_repository, sync_codec};
    use rusqlite::{Connection, OptionalExtension};

    /// Um aparelho de teste: banco com universo → história → livro → capítulo legado `c1`.
    struct Aparelho {
        banco: TemporaryDatabase,
        eu: DeviceIdentity,
    }

    impl Aparelho {
        fn novo() -> Self {
            let banco = TemporaryDatabase::new();
            let eu = {
                let connection = banco.database.write().expect("escrita");
                seed_universe(&connection, "u1");
                connection
                    .execute_batch(
                        "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                         INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');
                         INSERT INTO chapters (id, book_id, title, content, word_count)
                         VALUES ('c1', 'b1', 'Capítulo 1', 'original', 1);",
                    )
                    .expect("semear");
                self_de_teste(&connection)
            };
            Self { banco, eu }
        }

        fn conexao(&self) -> Connection {
            self.banco.connection()
        }

        fn editar(&self, texto: &str) -> DatabaseCommandResult<()> {
            manuscript_service::update_chapter(
                &self.banco.database,
                &self.eu,
                "c1",
                ChapterUpdate {
                    content: Some(texto.to_string()),
                    word_count: Some(1),
                    ..ChapterUpdate::default()
                },
            )
        }

        /// Anexo no capítulo, pela fronteira (sem blob: o que se prova aqui é a causalidade).
        fn anexar(&self, id: &str) {
            Mutacao::executar(&self.banco.database, &self.eu, |m| {
                canvas_repository::insert_attachment(
                    m.tx(),
                    &Attachment {
                        id: id.to_string(),
                        universe_id: "u1".into(),
                        owner_type: "chapter".into(),
                        owner_id: "c1".into(),
                        data_url: String::new(),
                        blob_hash: String::new(),
                        mime_type: String::new(),
                        caption: String::new(),
                        sort_order: 0,
                        created_at: "2026-09-15 10:00:00".into(),
                    },
                )?;
                m.gravou("attachment", id)
            })
            .expect("anexar");
        }

        fn eventos(&self) -> Vec<(String, String, String)> {
            let connection = self.conexao();
            let mut consulta = connection
                .prepare(
                    "SELECT aggregate_type, aggregate_id, operation FROM sync_events ORDER BY seq",
                )
                .expect("consulta");
            consulta
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("eventos")
        }

        fn conteudo(&self) -> Option<String> {
            self.conexao()
                .query_row("SELECT content FROM chapters WHERE id = 'c1'", [], |row| {
                    row.get(0)
                })
                .optional()
                .expect("conteúdo")
        }

        fn existe(&self, tabela: &str, id: &str) -> bool {
            self.conexao()
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {tabela} WHERE id = ?1)"),
                    [id],
                    |row| row.get(0),
                )
                .expect("existe")
        }

        fn estado_causal(&self, tipo: &str, id: &str) -> Option<String> {
            self.conexao()
                .query_row(
                    "SELECT current_rev FROM sync_aggregate_state WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                    [tipo, id],
                    |row| row.get(0),
                )
                .optional()
                .expect("estado")
        }

        fn tombstone(&self, tipo: &str, id: &str) -> bool {
            self.conexao()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2)",
                    [tipo, id],
                    |row| row.get(0),
                )
                .expect("tombstone")
        }

        /// **Gate comportamental:** o domínio e a representação causal dizem a mesma coisa.
        ///
        /// Existe no domínio ⇒ tem revisão corrente, e o payload do evento dessa revisão é o estado
        /// lido agora. Não existe ⇒ não tem revisão corrente e tem tombstone.
        fn coerente(&self, tipo: &str, id: &str) {
            let connection = self.conexao();
            let agregado = AggregateRef::new(tipo, id);
            match sync_codec::ler(&connection, &agregado).expect("ler") {
                Some(estado) => {
                    let rev = self.estado_causal(tipo, id).unwrap_or_else(|| {
                        panic!("{tipo} {id} existe no domínio sem revisão corrente")
                    });
                    let payload: String = connection
                        .query_row(
                            "SELECT payload FROM sync_events WHERE new_rev = ?1",
                            [&rev],
                            |row| row.get(0),
                        )
                        .unwrap_or_else(|_| panic!("revisão {rev} de {tipo} {id} sem evento"));
                    assert_eq!(
                        payload, estado.payload,
                        "{tipo} {id}: evento e domínio divergem"
                    );
                }
                None => {
                    assert!(
                        self.estado_causal(tipo, id).is_none(),
                        "{tipo} {id} some do domínio e segue corrente"
                    );
                    assert!(
                        self.tombstone(tipo, id),
                        "{tipo} {id} some do domínio sem tombstone"
                    );
                }
            }
        }
    }

    #[test]
    fn sucesso_grava_dominio_estado_causal_e_evento_juntos() {
        let aparelho = Aparelho::novo();
        aparelho.editar("primeira edição").expect("editar");

        assert_eq!(aparelho.conteudo().as_deref(), Some("primeira edição"));
        assert_eq!(
            aparelho.eventos(),
            vec![("chapter".into(), "c1".into(), "upsert".into())]
        );
        aparelho.coerente("chapter", "c1");
    }

    #[test]
    fn falha_depois_da_mutacao_e_antes_do_evento_nao_deixa_nada() {
        let aparelho = Aparelho::novo();
        armar(Some(Ponto::AposAMutacao));
        assert!(aparelho.editar("não pode ficar").is_err());

        assert_eq!(
            aparelho.conteudo().as_deref(),
            Some("original"),
            "domínio sem evento"
        );
        assert!(aparelho.eventos().is_empty());
        assert!(aparelho.estado_causal("chapter", "c1").is_none());
    }

    #[test]
    fn falha_com_eventos_gravados_e_antes_do_commit_nao_deixa_nada() {
        let aparelho = Aparelho::novo();
        armar(Some(Ponto::AntesDoCommit));
        assert!(aparelho.editar("não pode ficar").is_err());

        assert_eq!(aparelho.conteudo().as_deref(), Some("original"));
        assert!(aparelho.eventos().is_empty(), "evento sem domínio");
        assert!(aparelho.estado_causal("chapter", "c1").is_none());
    }

    #[test]
    fn falha_no_meio_da_geracao_dos_eventos_desfaz_a_exclusao_inteira() {
        let aparelho = Aparelho::novo();
        aparelho.anexar("a1");
        aparelho.anexar("a2");
        let antes = aparelho.eventos();

        // A exclusão emite três eventos (a1, a2, c1); a falha vem depois do primeiro.
        armar(Some(Ponto::DuranteOsEventos));
        assert!(
            manuscript_service::delete_chapter(&aparelho.banco.database, &aparelho.eu, "c1")
                .is_err()
        );

        assert!(aparelho.existe("chapters", "c1"));
        assert!(aparelho.existe("attachments", "a1") && aparelho.existe("attachments", "a2"));
        assert_eq!(
            aparelho.eventos(),
            antes,
            "nenhum evento da exclusão sobrevive"
        );
        assert!(!aparelho.tombstone("attachment", "a1"));
        aparelho.coerente("attachment", "a1");
    }

    #[test]
    fn repetir_depois_da_falha_produz_uma_revisao_so() {
        let aparelho = Aparelho::novo();
        armar(Some(Ponto::AntesDoCommit));
        assert!(aparelho.editar("texto").is_err());
        aparelho.editar("texto").expect("repetir");

        assert_eq!(aparelho.eventos().len(), 1);
        aparelho.coerente("chapter", "c1");
    }

    #[test]
    fn exclusao_emite_os_filhos_antes_do_pai_e_deixa_tombstone_de_todos() {
        let aparelho = Aparelho::novo();
        aparelho.editar("com história").expect("editar");
        aparelho.anexar("a1");
        aparelho.anexar("a2");

        manuscript_service::delete_chapter(&aparelho.banco.database, &aparelho.eu, "c1")
            .expect("excluir");

        let eventos = aparelho.eventos();
        assert_eq!(
            &eventos[eventos.len() - 3..],
            &[
                ("attachment".into(), "a1".into(), "delete".into()),
                ("attachment".into(), "a2".into(), "delete".into()),
                ("chapter".into(), "c1".into(), "delete".into()),
            ],
            "filhos antes do pai"
        );
        for (tipo, id) in [
            ("chapter", "c1"),
            ("attachment", "a1"),
            ("attachment", "a2"),
        ] {
            aparelho.coerente(tipo, id);
        }
    }

    #[test]
    fn cascata_so_acontece_depois_do_preflight_e_filho_em_conflito_impede_tudo() {
        let aparelho = Aparelho::novo();
        aparelho.anexar("a1");
        aparelho
            .conexao()
            .execute(
                "INSERT INTO sync_divergences
                   (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id)
                 VALUES ('d1', 'attachment', 'a1', '', 'x', 'y', 'e')",
                [],
            )
            .expect("divergência aberta");
        let antes = aparelho.eventos();

        let erro = manuscript_service::delete_chapter(&aparelho.banco.database, &aparelho.eu, "c1")
            .expect_err("tinha que recusar");
        assert!(
            erro.message.contains("nada foi apagado"),
            "{}",
            erro.message
        );
        assert!(aparelho.existe("chapters", "c1") && aparelho.existe("attachments", "a1"));
        assert_eq!(aparelho.eventos(), antes);
    }

    #[test]
    fn exclusao_preparada_que_nao_aconteceu_nao_e_confirmada() {
        let aparelho = Aparelho::novo();
        let resultado = Mutacao::executar(&aparelho.banco.database, &aparelho.eu, |m| {
            m.excluir("chapter", "c1")
            // …e o serviço "esquece" o DELETE.
        });
        assert!(resultado.is_err());
        assert!(aparelho.existe("chapters", "c1"));
        assert!(aparelho.eventos().is_empty());
    }

    #[test]
    fn mutacao_dentro_de_mutacao_e_recusada() {
        let aparelho = Aparelho::novo();
        let resultado = Mutacao::executar(&aparelho.banco.database, &aparelho.eu, |_| {
            Mutacao::executar(&aparelho.banco.database, &aparelho.eu, |_| Ok(()))
        });
        let erro = resultado.expect_err("aninhar tinha que falhar");
        assert!(
            erro.message.contains("dentro de outra mutação"),
            "{}",
            erro.message
        );
        // E a marca foi solta: a próxima mutação funciona.
        aparelho.editar("depois").expect("mutação seguinte");
    }

    #[test]
    fn agregado_nao_coberto_nao_pode_ser_declarado() {
        let aparelho = Aparelho::novo();
        let erro = Mutacao::executar(&aparelho.banco.database, &aparelho.eu, |m| {
            m.gravou("entity", "e1")
        })
        .expect_err("entidade ainda não é coberta");
        assert!(erro.message.contains("NH-079"), "{}", erro.message);
    }

    #[test]
    fn o_mesmo_agregado_declarado_duas_vezes_vira_uma_revisao() {
        let aparelho = Aparelho::novo();
        Mutacao::executar(&aparelho.banco.database, &aparelho.eu, |m| {
            m.tx()
                .execute("UPDATE chapters SET title = 'um' WHERE id = 'c1'", [])
                .map_err(|e| DatabaseCommandError::storage(e.to_string()))?;
            m.gravou("chapter", "c1")?;
            m.tx()
                .execute("UPDATE chapters SET title = 'dois' WHERE id = 'c1'", [])
                .map_err(|e| DatabaseCommandError::storage(e.to_string()))?;
            m.gravou("chapter", "c1")
        })
        .expect("mutação");
        assert_eq!(aparelho.eventos().len(), 1);
        aparelho.coerente("chapter", "c1");
    }

    #[test]
    fn anexo_criado_e_excluido_pelo_servico_passa_pela_fronteira() {
        let aparelho = Aparelho::novo();
        let raiz = std::env::temp_dir().join(format!("narrahub-b1-blobs-{}", uuid::Uuid::new_v4()));
        let store = crate::infrastructure::blob_store::BlobStore::new(raiz.clone());
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let anexo = canvas_service::create_attachment(
            &aparelho.banco.database,
            &store,
            &aparelho.eu,
            "u1",
            "chapter",
            "c1",
            png,
            "legenda",
        )
        .expect("criar anexo");
        aparelho.coerente("attachment", &anexo.id);

        canvas_service::delete_attachment(&aparelho.banco.database, &aparelho.eu, &anexo.id)
            .expect("excluir");
        aparelho.coerente("attachment", &anexo.id);
        assert_eq!(
            aparelho
                .eventos()
                .iter()
                .map(|(_, _, op)| op.as_str())
                .collect::<Vec<_>>(),
            vec!["upsert", "delete"]
        );
        let _ = std::fs::remove_dir_all(raiz);
    }

    /// Exclusão remota do pai com um filho vivo que a origem não apagou: o DELETE não roda.
    #[test]
    fn exclusao_remota_do_pai_e_bloqueada_por_filho_concorrente() {
        let aparelho = Aparelho::novo();
        aparelho
            .editar("versão que os dois conhecem")
            .expect("editar");
        let rev_conhecida = aparelho.estado_causal("chapter", "c1").expect("revisão");
        // Um anexo criado AQUI, que a outra origem nunca viu.
        aparelho.anexar("a-concorrente");

        let outra = {
            let connection = aparelho.banco.database.write().expect("escrita");
            origem_remota_confiavel(&connection, &aparelho.eu)
        };
        let mut exclusao = envelope_de_origem(
            outra.device_id(),
            1,
            "u1",
            &AggregateRef::new("chapter", "c1"),
            Operation::Delete,
            "",
            &rev_conhecida,
        );
        exclusao.signature = outra.sign(&exclusao);

        let mut connection = aparelho.banco.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[exclusao]).expect("receber");
        drop(connection);

        assert_eq!(relatorio.divergencias, 1);
        assert!(
            aparelho.existe("chapters", "c1"),
            "a FK apagaria o capítulo e o filho"
        );
        assert!(
            aparelho.existe("attachments", "a-concorrente"),
            "o filho concorrente sumiu"
        );
        assert_eq!(
            aparelho.estado_causal("chapter", "c1").as_deref(),
            Some(rev_conhecida.as_str())
        );
        let tipo: String = aparelho
            .conexao()
            .query_row(
                "SELECT kind FROM sync_divergences WHERE aggregate_type = 'chapter' AND aggregate_id = 'c1' AND resolved_at = ''",
                [],
                |row| row.get(0),
            )
            .expect("divergência registrada");
        assert_eq!(tipo, "parent_deletion_blocked");
    }

    /// Contraprova: sem descendente vivo, a mesma exclusão remota apaga, com tombstone.
    #[test]
    fn exclusao_remota_do_pai_sem_descendente_vivo_segue_normalmente() {
        let aparelho = Aparelho::novo();
        aparelho
            .editar("versão que os dois conhecem")
            .expect("editar");
        let rev_conhecida = aparelho.estado_causal("chapter", "c1").expect("revisão");

        let outra = {
            let connection = aparelho.banco.database.write().expect("escrita");
            origem_remota_confiavel(&connection, &aparelho.eu)
        };
        let mut exclusao = envelope_de_origem(
            outra.device_id(),
            1,
            "u1",
            &AggregateRef::new("chapter", "c1"),
            Operation::Delete,
            "",
            &rev_conhecida,
        );
        exclusao.signature = outra.sign(&exclusao);

        let mut connection = aparelho.banco.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[exclusao]).expect("receber");
        drop(connection);

        assert_eq!(relatorio.divergencias, 0);
        assert!(!aparelho.existe("chapters", "c1"));
        aparelho.coerente("chapter", "c1");
    }
}

#[cfg(test)]
mod gate_estrutural {
    /// Corpo de uma função pública pelo nome, até a próxima função de topo.
    fn corpo<'a>(fonte: &'a str, nome: &str) -> &'a str {
        let inicio = fonte
            .find(&format!("pub fn {nome}("))
            .unwrap_or_else(|| panic!("a função {nome} sumiu; o gate perdeu o alvo"));
        let resto = &fonte[inicio..];
        let fim = resto[1..]
            .find("\npub fn ")
            .or_else(|| resto[1..].find("\nfn "))
            .or_else(|| resto[1..].find("\n#[cfg(test)]"))
            .map(|i| i + 1)
            .unwrap_or(resto.len());
        &resto[..fim]
    }

    fn sem_testes(fonte: &str) -> &str {
        fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .unwrap_or(fonte)
    }

    /// **As escritas cobertas só existem pela fronteira.**
    ///
    /// Uma escrita sincronizável que abre a própria conexão ou transação consegue gravar o domínio
    /// sem passar pela emissão do evento — que é exatamente o estado "domínio sem evento" que a
    /// `Mutacao` existe para tornar impossível.
    #[test]
    fn escritas_cobertas_passam_pela_mutacao_e_por_mais_nada() {
        let manuscrito = include_str!("manuscript_service.rs");
        let canvas = include_str!("canvas_service.rs");
        for (fonte, funcao) in [
            (manuscrito, "update_chapter"),
            (manuscrito, "delete_chapter"),
            (canvas, "create_attachment"),
            (canvas, "delete_attachment"),
        ] {
            let corpo = corpo(fonte, funcao);
            assert!(
                corpo.contains("Mutacao::executar("),
                "{funcao} não passa pela Mutacao"
            );
            for proibido in [
                "database.write()",
                ".transaction(",
                "transaction_with_behavior",
                "append_event_in_transaction",
            ] {
                assert!(
                    !corpo.contains(proibido),
                    "{funcao} usa `{proibido}` por fora da fronteira"
                );
            }
        }
    }

    /// **Os repositórios usados pela fronteira não abrem transação nem conexão escondida.**
    #[test]
    fn repositorios_da_mutacao_usam_a_transacao_recebida() {
        for (nome, fonte) in [
            (
                "manuscript_repository",
                include_str!("../infrastructure/sqlite/manuscript_repository.rs"),
            ),
            (
                "canvas_repository",
                include_str!("../infrastructure/sqlite/canvas_repository.rs"),
            ),
            (
                "sync_codec",
                include_str!("../infrastructure/sqlite/sync_codec.rs"),
            ),
            ("blob_fields", include_str!("blob_fields.rs")),
        ] {
            let codigo = sem_testes(fonte);
            for proibido in [
                ".transaction(",
                "transaction_with_behavior",
                ".write()",
                "Connection::open",
                "BEGIN",
            ] {
                assert!(
                    !codigo.contains(proibido),
                    "{nome} contém `{proibido}` fora dos testes"
                );
            }
        }
    }
}
