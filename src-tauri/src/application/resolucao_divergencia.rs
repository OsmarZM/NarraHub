//! Resolução de divergências do Sync V2.
//!
//! ## Nesta etapa: só `parent_deletion_blocked`
//!
//! O resolvedor lê o **tipo** da divergência antes de fazer qualquer coisa. Cada tipo tem um
//! contrato próprio, e um tipo sem contrato é recusado — em especial, um resolvedor genérico de
//! divergência concorrente **nunca** executa uma exclusão só porque um dos lados era `delete`.
//!
//! ```text
//! kind                      escolha          efeito
//! parent_deletion_blocked   ManterLocal      pai e descendentes ficam; nasce um upsert do pai com
//!                                            base = revisão da exclusão remota (restauração que viu
//!                                            o delete: sequencial nos outros aparelhos)
//! parent_deletion_blocked   AceitarRemoto    refaz o preflight de descendentes AGORA; sobrou filho
//!                                            vivo → recusa, nada muda, divergência continua aberta;
//!                                            não sobrou → DELETE + tombstone com a revisão remota
//! concurrent                qualquer         recusado: a caixa de conciliação é a etapa F
//! ```
//!
//! ## Por que aceitar a exclusão refaz o preflight
//!
//! O bloqueio aconteceu porque havia um descendente que a origem da exclusão não apagou. Entre o
//! bloqueio e a decisão, o escritor pode ter criado mais filhos, ou resolvido os que existiam.
//! Aceitar com base no que era verdade no bloqueio deixaria a cascata apagar trabalho concorrente —
//! exatamente a perda silenciosa que o bloqueio existe para impedir. O descendente se resolve
//! primeiro, por uma mutação normal (excluir o anexo, por exemplo); depois aceitar conclui.
//!
//! ## Tudo numa transação
//!
//! Roda dentro de [`Mutacao::executar`]: a decisão, a escrita no domínio, o evento (quando há) e a
//! marcação da divergência como resolvida são commitados juntos, ou nada é.

use rusqlite::OptionalExtension;

use crate::application::mutacao::Mutacao;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::now_timestamp;
use crate::domain::sync::AggregateRef;
use crate::infrastructure::sqlite::{sync_apply, sync_codec, SqliteDatabase};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escolha {
    ManterLocal,
    AceitarRemoto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolucao {
    /// O pai e os descendentes ficaram; a restauração foi emitida.
    MantidoLocal,
    /// A exclusão remota foi aplicada.
    ExclusaoConcluida,
}

struct Divergencia {
    agregado: AggregateRef,
    local_rev: String,
    remote_rev: String,
    remote_event_id: String,
    kind: String,
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

pub fn resolver(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id_divergencia: &str,
    escolha: Escolha,
) -> DatabaseCommandResult<Resolucao> {
    Mutacao::executar(database, identidade, |m| {
        let divergencia = ler_aberta(m, id_divergencia)?;
        match divergencia.kind.as_str() {
            "parent_deletion_blocked" => match escolha {
                Escolha::ManterLocal => manter_local(m, id_divergencia, &divergencia),
                Escolha::AceitarRemoto => aceitar_exclusao(m, id_divergencia, &divergencia),
            },
            "concurrent" => Err(DatabaseCommandError::conflict(
                "Esta divergência é entre duas versões editadas, e a escolha entre elas ainda não \
                 existe nesta versão. Nada foi alterado.",
            )),
            outro => Err(DatabaseCommandError::storage(format!(
                "Tipo de divergência desconhecido: '{outro}'. Nada foi alterado."
            ))),
        }
    })
}

fn ler_aberta(m: &Mutacao<'_, '_>, id: &str) -> DatabaseCommandResult<Divergencia> {
    m.tx()
        .query_row(
            "SELECT aggregate_type, aggregate_id, local_rev, remote_rev, remote_event_id, kind
               FROM sync_divergences WHERE id = ?1 AND resolved_at = ''",
            [id],
            |row| {
                Ok(Divergencia {
                    agregado: AggregateRef::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                    local_rev: row.get(2)?,
                    remote_rev: row.get(3)?,
                    remote_event_id: row.get(4)?,
                    kind: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(erro)?
        .ok_or_else(|| {
            DatabaseCommandError::not_found(format!(
                "Não há divergência aberta com id {id}. Ela pode já ter sido resolvida."
            ))
        })
}

fn marcar_resolvida(m: &Mutacao<'_, '_>, id: &str, resolucao: &str) -> DatabaseCommandResult<()> {
    let linhas = m
        .tx()
        .execute(
            "UPDATE sync_divergences SET resolved_at = ?1, resolution = ?2
              WHERE id = ?3 AND resolved_at = ''",
            rusqlite::params![now_timestamp(), resolucao, id],
        )
        .map_err(erro)?;
    if linhas != 1 {
        return Err(DatabaseCommandError::storage(
            "A divergência deixou de estar aberta no meio da resolução. Nada foi confirmado.",
        ));
    }
    Ok(())
}

fn manter_local(
    m: &mut Mutacao<'_, '_>,
    id: &str,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<Resolucao> {
    let agregado = &divergencia.agregado;
    if sync_codec::ler_canonico(m.tx(), agregado)?.is_none() {
        return Err(DatabaseCommandError::storage(format!(
            "{} {} não existe mais aqui; não há o que manter. Nada foi alterado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }
    // A restauração parte da revisão da exclusão: quem a recebe sabe que ela viu o delete.
    let linhas = m
        .tx()
        .execute(
            "UPDATE sync_aggregate_state SET current_rev = ?1
              WHERE aggregate_type = ?2 AND aggregate_id = ?3",
            rusqlite::params![
                &divergencia.remote_rev,
                &agregado.aggregate_type,
                &agregado.aggregate_id
            ],
        )
        .map_err(erro)?;
    if linhas != 1 {
        return Err(DatabaseCommandError::storage(format!(
            "{} {} existe sem estado causal. Nada foi confirmado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }
    m.gravou(&agregado.aggregate_type, &agregado.aggregate_id)?;
    // Os sobreviventes que a exclusão reescreveria (a ordem do livro, para capítulo) também são
    // declarados: o outro aparelho já os reescreveu sem este agregado, e a versão daqui precisa
    // existir como revisão — senão a restauração chega num livro cuja ordem não o cita.
    for impacto in sync_codec::impactos_da_exclusao(m.tx(), agregado)? {
        if let sync_codec::Impacto::Reescrito(sobrevivente) = impacto {
            if sync_codec::ler_canonico(m.tx(), &sobrevivente)?.is_some() {
                m.gravou(&sobrevivente.aggregate_type, &sobrevivente.aggregate_id)?;
            }
        }
    }
    marcar_resolvida(m, id, "local")?;
    Ok(Resolucao::MantidoLocal)
}

fn aceitar_exclusao(
    m: &mut Mutacao<'_, '_>,
    id: &str,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<Resolucao> {
    let agregado = &divergencia.agregado;

    let atual: Option<String> = m
        .tx()
        .query_row(
            "SELECT current_rev FROM sync_aggregate_state
              WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?;
    if atual.as_deref() != Some(divergencia.local_rev.as_str()) {
        return Err(DatabaseCommandError::conflict(format!(
            "{} {} foi alterado depois que a exclusão chegou. Aceitar agora apagaria essa alteração. \
             Nada foi apagado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }

    let outra_pendencia: bool = m
        .tx()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_divergences
                            WHERE aggregate_type = ?1 AND aggregate_id = ?2
                              AND resolved_at = '' AND id <> ?3)
                 OR EXISTS(SELECT 1 FROM sync_events e
                            WHERE e.aggregate_type = ?1 AND e.aggregate_id = ?2
                              AND NOT EXISTS (SELECT 1 FROM sync_applied_events a
                                               WHERE a.event_id = e.event_id))",
            rusqlite::params![&agregado.aggregate_type, &agregado.aggregate_id, id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    if outra_pendencia {
        return Err(DatabaseCommandError::conflict(format!(
            "{} {} tem outra decisão ou alteração pendente. Resolva isso antes. Nada foi apagado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }

    // O preflight da exclusão (filho vivo, sobrevivente que mudaria, efeito bloqueado) roda de novo
    // lá dentro, AGORA: o que era verdade no bloqueio não autoriza a cascata.
    sync_apply::aplicar_exclusao_bloqueada(m.tx(), &divergencia.remote_event_id)?;
    marcar_resolvida(m, id, "remote")?;
    Ok(Resolucao::ExclusaoConcluida)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::canvas_service;
    use crate::application::mutacao::tests::{ordem_remota, Aparelho};
    use crate::domain::identity::DeviceIdentity;
    use crate::domain::sync::{EventEnvelope, Operation};
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::sync_session::{receber_eventos, Relatorio};
    use crate::infrastructure::sqlite::test_support::origem_remota_confiavel;

    struct Cenario {
        aparelho: Aparelho,
        outra: DeviceIdentity,
        exclusao: EventEnvelope,
        rev_conhecida: String,
    }

    impl Cenario {
        /// O capítulo que os dois conhecem, um anexo criado só AQUI, e a exclusão do capítulo
        /// vinda da outra origem.
        fn bloqueado() -> Self {
            let aparelho = Aparelho::novo();
            aparelho
                .editar("versão que os dois conhecem")
                .expect("editar");
            let rev_conhecida = aparelho.estado_causal("chapter", "c1").expect("revisão");
            aparelho.anexar("a-concorrente");
            let outra = {
                let connection = aparelho.banco.database.write().expect("escrita");
                origem_remota_confiavel(&connection, &aparelho.eu)
            };
            // A origem exclui o capítulo e DEPOIS reescreve a ordem sem ele.
            let ordem = ordem_remota(&outra, 2, "", &[]);
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
            let cenario = Self {
                aparelho,
                outra,
                exclusao,
                rev_conhecida,
            };
            let relatorio = cenario.entregar(&[ordem, cenario.exclusao.clone()]);
            assert_eq!(relatorio.divergencias, 1, "a exclusão tinha que bloquear");
            cenario
        }

        fn entregar(&self, eventos: &[EventEnvelope]) -> Relatorio {
            let mut connection = self.aparelho.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, eventos).expect("receber")
        }

        fn divergencias_abertas(&self) -> Vec<String> {
            let connection = self.aparelho.conexao();
            let mut consulta = connection
                .prepare("SELECT id FROM sync_divergences WHERE resolved_at = '' ORDER BY id")
                .expect("consulta");
            consulta
                .query_map([], |row| row.get(0))
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("ids")
        }

        fn id_divergencia(&self) -> String {
            let abertas = self.divergencias_abertas();
            assert_eq!(abertas.len(), 1, "{abertas:?}");
            abertas[0].clone()
        }

        fn cursor(&self) -> i64 {
            self.aparelho
                .conexao()
                .query_row(
                    "SELECT last_seq_applied FROM sync_cursors WHERE origin_device_id = ?1",
                    [self.outra.device_id()],
                    |row| row.get(0),
                )
                .expect("cursor")
        }

        fn resolver(&self, escolha: Escolha) -> DatabaseCommandResult<Resolucao> {
            resolver(
                &self.aparelho.banco.database,
                &self.aparelho.eu,
                &self.id_divergencia(),
                escolha,
            )
        }

        fn resolucao(&self, id: &str) -> String {
            self.aparelho
                .conexao()
                .query_row(
                    "SELECT resolution FROM sync_divergences WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .expect("resolução")
        }
    }

    #[test]
    fn exclusao_bloqueada_avanca_o_cursor_e_nao_se_repete_na_sessao_seguinte() {
        let cenario = Cenario::bloqueado();
        assert_eq!(
            cenario.cursor(),
            1,
            "o cursor não pode travar na exclusão bloqueada"
        );
        assert_eq!(cenario.divergencias_abertas().len(), 1);

        // A mesma exclusão chega de novo (retransmissão, outra sessão, relay).
        let relatorio = cenario.entregar(std::slice::from_ref(&cenario.exclusao));
        assert_eq!(relatorio.divergencias, 0, "reaplicou a exclusão bloqueada");
        assert_eq!(
            cenario.divergencias_abertas().len(),
            1,
            "divergência duplicada"
        );
        // A ordem sem c1 espera: c1 continua aqui, e a ordem não pode ser materializada.
        assert_eq!(cenario.cursor(), 1);
        assert!(cenario.aparelho.existe("chapters", "c1"));
        assert!(cenario.aparelho.existe("attachments", "a-concorrente"));
    }

    #[test]
    fn manter_local_preserva_pai_e_filho_e_emite_restauracao_que_viu_a_exclusao() {
        let cenario = Cenario::bloqueado();
        let id = cenario.id_divergencia();
        let eventos_antes = cenario.aparelho.eventos().len();

        assert_eq!(
            cenario.resolver(Escolha::ManterLocal).expect("manter"),
            Resolucao::MantidoLocal
        );

        assert!(cenario.aparelho.existe("chapters", "c1"));
        assert!(cenario.aparelho.existe("attachments", "a-concorrente"));
        assert!(cenario.divergencias_abertas().is_empty());
        assert_eq!(cenario.resolucao(&id), "local");
        cenario.aparelho.coerente("chapter", "c1");
        cenario.aparelho.coerente("attachment", "a-concorrente");

        let eventos = cenario.aparelho.eventos();
        assert_eq!(
            eventos.len(),
            eventos_antes + 2,
            "restauração do capítulo e a ordem daqui, que o cita"
        );
        let base: String = cenario
            .aparelho
            .conexao()
            .query_row(
                "SELECT base_rev FROM sync_events
                  WHERE device_id = ?1 AND aggregate_type = 'chapter' ORDER BY seq DESC LIMIT 1",
                [cenario.aparelho.eu.device_id()],
                |row| row.get(0),
            )
            .expect("restauração");
        assert_eq!(
            base, cenario.exclusao.new_rev,
            "a restauração parte da exclusão"
        );

        // A exclusão chegando de novo não reabre nada.
        let relatorio = cenario.entregar(std::slice::from_ref(&cenario.exclusao));
        // A ordem que o outro aparelho mandou sem c1 deixa de esperar e vira decisão: aqui a ordem
        // mantida cita c1. Nenhuma divergência nova no capítulo.
        assert_eq!(relatorio.divergencias, 1);
        let em_capitulo: i64 = cenario
            .aparelho
            .conexao()
            .query_row(
                "SELECT COUNT(*) FROM sync_divergences WHERE aggregate_type = 'chapter' AND resolved_at = ''",
                [],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(em_capitulo, 0);
        assert!(cenario.aparelho.existe("chapters", "c1"));
    }

    #[test]
    fn aceitar_a_exclusao_nao_apaga_descendente_ainda_vivo_e_conclui_depois_de_resolve_lo() {
        let cenario = Cenario::bloqueado();
        let id = cenario.id_divergencia();
        let eventos_antes = cenario.aparelho.eventos();

        let erro = cenario
            .resolver(Escolha::AceitarRemoto)
            .expect_err("o filho concorrente ainda existe");
        assert!(erro.message.contains("a-concorrente"), "{}", erro.message);
        assert!(cenario.aparelho.existe("chapters", "c1"));
        assert!(cenario.aparelho.existe("attachments", "a-concorrente"));
        assert_eq!(cenario.divergencias_abertas(), vec![id.clone()]);
        assert_eq!(cenario.aparelho.eventos(), eventos_antes);

        // O escritor resolve o descendente: exclui o anexo, por mutação normal.
        canvas_service::delete_attachment(
            &cenario.aparelho.banco.database,
            &cenario.aparelho.eu,
            "a-concorrente",
        )
        .expect("excluir o anexo");
        cenario.aparelho.coerente("attachment", "a-concorrente");

        assert_eq!(
            cenario.resolver(Escolha::AceitarRemoto).expect("aceitar"),
            Resolucao::ExclusaoConcluida
        );
        assert!(!cenario.aparelho.existe("chapters", "c1"));
        assert_eq!(cenario.resolucao(&id), "remote");
        cenario.aparelho.coerente("chapter", "c1");
        let tombstone: String = cenario
            .aparelho
            .conexao()
            .query_row(
                "SELECT deleted_rev FROM sync_tombstones WHERE aggregate_type = 'chapter' AND aggregate_id = 'c1'",
                [],
                |row| row.get(0),
            )
            .expect("tombstone");
        assert_eq!(
            tombstone, cenario.exclusao.new_rev,
            "a revisão da exclusão é a remota"
        );

        // A ordem que esperava c1 sair agora pode ser materializada, exatamente como veio.
        cenario.entregar(&[]);
        assert_eq!(cenario.cursor(), 2);
        cenario.aparelho.coerente("chapter_order", "b1");
    }

    #[test]
    fn aceitar_a_exclusao_recusa_pai_editado_depois_do_bloqueio() {
        let cenario = Cenario::bloqueado();
        canvas_service::delete_attachment(
            &cenario.aparelho.banco.database,
            &cenario.aparelho.eu,
            "a-concorrente",
        )
        .expect("excluir o anexo");
        cenario.aparelho.editar("escrito depois").expect("editar");
        assert_ne!(
            cenario.aparelho.estado_causal("chapter", "c1"),
            Some(cenario.rev_conhecida.clone())
        );

        let erro = cenario
            .resolver(Escolha::AceitarRemoto)
            .expect_err("apagaria a edição");
        assert!(erro.message.contains("alterado depois"), "{}", erro.message);
        assert!(cenario.aparelho.existe("chapters", "c1"));
        assert_eq!(cenario.divergencias_abertas().len(), 1);
    }

    /// O resolvedor genérico não apaga: divergência `concurrent` é recusada, mesmo com `delete`.
    #[test]
    fn divergencia_concorrente_nao_e_resolvida_por_exclusao() {
        let aparelho = Aparelho::novo();
        aparelho.editar("texto").expect("editar");
        aparelho
            .conexao()
            .execute(
                "INSERT INTO sync_divergences
                   (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                    local_operation, remote_operation, remote_event_id)
                 VALUES ('d-conc', 'chapter', 'c1', '', 'x', 'y', 'upsert', 'delete', 'e')",
                [],
            )
            .expect("divergência");
        for escolha in [Escolha::AceitarRemoto, Escolha::ManterLocal] {
            assert!(resolver(&aparelho.banco.database, &aparelho.eu, "d-conc", escolha).is_err());
        }
        assert!(aparelho.existe("chapters", "c1"));
        let aberta: bool = aparelho
            .conexao()
            .query_row(
                "SELECT resolved_at = '' FROM sync_divergences WHERE id = 'd-conc'",
                [],
                |row| row.get(0),
            )
            .expect("aberta");
        assert!(aberta);
    }

    /// O outro lado: a restauração emitida por "manter o meu" chega a um aparelho onde a exclusão
    /// foi aplicada, e é sequencial — o capítulo volta, sem abrir divergência.
    #[test]
    fn restauracao_que_viu_a_exclusao_e_aplicada_no_aparelho_que_excluiu() {
        let aparelho = Aparelho::novo();
        aparelho.editar("conhecida").expect("editar");
        let rev = aparelho.estado_causal("chapter", "c1").expect("revisão");
        let payload =
            sync_codec::ler_canonico(&aparelho.conexao(), &AggregateRef::new("chapter", "c1"))
                .expect("ler")
                .expect("existe")
                .payload;
        let outra = {
            let connection = aparelho.banco.database.write().expect("escrita");
            origem_remota_confiavel(&connection, &aparelho.eu)
        };
        let agregado = AggregateRef::new("chapter", "c1");
        let ordem = ordem_remota(&outra, 2, "", &[]);
        let mut exclusao = envelope_de_origem(
            outra.device_id(),
            1,
            "u1",
            &agregado,
            Operation::Delete,
            "",
            &rev,
        );
        exclusao.signature = outra.sign(&exclusao);
        let mut restauracao = envelope_de_origem(
            outra.device_id(),
            3,
            "u1",
            &agregado,
            Operation::Upsert,
            &payload,
            &exclusao.new_rev,
        );
        restauracao.signature = outra.sign(&restauracao);

        let mut connection = aparelho.banco.database.write().expect("escrita");
        receber_eventos(&mut connection, &[ordem, exclusao]).expect("exclusão");
        drop(connection);
        assert!(!aparelho.existe("chapters", "c1"));

        let mut connection = aparelho.banco.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[restauracao]).expect("restauração");
        drop(connection);
        assert_eq!(relatorio.divergencias, 0);
        assert_eq!(relatorio.aplicados, 1);
        assert!(aparelho.existe("chapters", "c1"));
        assert!(!aparelho.tombstone("chapter", "c1"));
        aparelho.coerente("chapter", "c1");
    }
}
