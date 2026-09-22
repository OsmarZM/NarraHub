//! A fronteira única entre mutação de domínio e Sync V2 (NH-079, etapas B1 e B2).
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
//!     m.excluir(tipo, id)     ANTES do DELETE: impactos, estado causal, preflight, eventos preparados
//!   fim:
//!     reescritos com linha própria: relê o estado canônico → upsert, ANTES das exclusões
//!     excluídos:  confere que sumiram → delete (descendentes antes do pai)
//!     reescritos de existência derivada (ordens): upsert depois das exclusões
//!     agregado cujo estado canônico já é o da revisão corrente não gera evento
//!   COMMIT
//! ```
//!
//! ## Excluir não é só apagar
//!
//! ```text
//! m.excluir(pai)
//!   preflight (antes do SQL):
//!     Excluido(filho A), Excluido(filho B)   → precisam sumir     → eventos delete
//!     Reescrito(sobrevivente C)              → precisa continuar  → relido depois → evento upsert
//!     Bloqueado(motivo)                      → efeito sobre tipo ainda não coberto → recusa tudo
//!   cada afetado: divergência aberta ou evento pendente → recusa (nada de alteração silenciosa)
//! serviço executa o DELETE
//! fim: reescrita de quem tem linha própria, exclusões, reescrita das ordens — tudo numa transação
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
//! ## Fronteira com o blob store
//!
//! O arquivo de um anexo é escrito **dentro** da closure e **fora** da transação: o sistema de
//! arquivos não participa do `ROLLBACK`. A ordem é sempre arquivo primeiro, linha depois:
//!
//! ```text
//! blob publicado → linha + evento → COMMIT     ok
//! blob publicado → falha → ROLLBACK            blob órfão: inofensivo, endereçado por conteúdo,
//!                                              reaproveitado se a ação for repetida
//! ```
//!
//! O contrário (linha commitada apontando para arquivo que não existe) é o estado proibido. Blob
//! publicado sem referência não é apagado automaticamente (ADR 0010 §11): pode ser o arquivo de uma
//! repetição, de um backup ou de um evento que outro aparelho ainda vai pedir. A única limpeza
//! automática é a de `.part` abandonado em staging, uma vez por arranque
//! (`BlobStore::limpar_staging_abandonado`).
//!
//! ## Resolução de conflito (etapa F)
//!
//! Uma resolução também é uma `Mutacao` — não existe caminho paralelo. Ela declara o conflito
//! ([`Mutacao::declarar_resolucao`]) e os efeitos com a base escolhida ([`Mutacao::efeito`],
//! [`Mutacao::excluir_como_efeito`]); o fim monta o **certificado** a partir da lista exata do que
//! vai ser emitido e o emite como membro 0 de um grupo `resolution`:
//!
//! ```text
//! membro 0     conflict_resolution/<conflictKey>   upsert da raiz, payload = certificado
//! membros 1..N os efeitos, na ordem declarada; efeito com base escolhida SEMPRE vira evento
//! mesma transação: sync_divergences daquele conflito fecha (índice local)
//! ```
//!
//! ## O que ela não é
//!
//! Não é savepoint: `executar` dentro de `executar` é erro. Não guarda transação entre chamadas.
//! Não conhece Tauri. Não decide nada de domínio — isso continua no serviço.

use std::cell::Cell;
use std::collections::HashSet;

use rusqlite::{Transaction, TransactionBehavior};

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::sync::{AggregateRef, GrupoDeMutacao, Operation};
use crate::infrastructure::sqlite::sync_codec::{self, EstadoConcorrente, Impacto};
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
    /// Declarado pelo serviço: criado ou alterado.
    Gravou(AggregateRef),
    /// Sobrevivente de uma exclusão: mudou por FK/gatilho/derivação e continua existindo.
    ///
    /// **`Excluiu` domina `Reescreveu` do mesmo agregado**: se o agregado já vai desaparecer nesta
    /// operação, a reescrita que a cascata causaria nele é absorvida e não gera revisão intermediária
    /// (ver `coletar` e `finalizar`). É o caso do card que possui um campo exclusivo: apagar o card
    /// apaga o campo, e o efeito do campo sobre o card não vira evento.
    Reescreveu(AggregateRef),
    Excluiu {
        agregado: AggregateRef,
        universe_id: String,
    },
    /// Efeito de uma resolução: parte de uma base ESCOLHIDA (um dos lados do conflito), e não da
    /// revisão corrente. Sempre vira evento, mesmo quando o estado não mudou: a revisão nova é o
    /// ponto em que os dois lados passam a ter uma cabeça só.
    Efeito {
        agregado: AggregateRef,
        operacao: Operation,
        base_rev: String,
        other_rev: String,
        universe_id: String,
    },
}

/// O conflito que uma resolução fecha. Ver [`Mutacao::declarar_resolucao`].
#[derive(Debug, Clone)]
pub struct DeclaracaoDeResolucao {
    pub conflict_key: String,
    pub kind: String,
    pub participante_um: crate::domain::conflito::ConflictParticipant,
    pub participante_outro: crate::domain::conflito::ConflictParticipant,
    /// A escolha, em termos portáteis (`a`, `b`, `auto`, `rename`, `merge`, …).
    pub choice: String,
    pub universe_id: String,
    /// Como o índice LOCAL registra o fechamento: `local`, `remote` ou `manual`.
    pub resolucao_local: String,
}

/// A mutação em andamento. Só existe dentro de [`Mutacao::executar`].
pub struct Mutacao<'t, 'c> {
    tx: &'t Transaction<'c>,
    operacoes: Vec<Operacao>,
    /// A raiz da primeira exclusão declarada. Faz do grupo um `delete_tree`, e é o que a decisão
    /// apresenta ao escritor se o grupo for bloqueado em outro aparelho.
    raiz_da_exclusao: Option<AggregateRef>,
    /// Presente quando esta mutação é a resolução de um conflito.
    resolucao: Option<DeclaracaoDeResolucao>,
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
                raiz_da_exclusao: None,
                resolucao: None,
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
    /// Percorre os impactos: o que some junto (`Excluido`, recursivo), o que sobrevive mudado
    /// (`Reescrito`) e o que ainda não pode ser tocado (`Bloqueado`, que recusa tudo). Todo afetado
    /// passa pelo preflight de estado concorrente. Depois desta chamada, o serviço executa o
    /// `DELETE`.
    pub fn excluir(&mut self, tipo: &str, id: &str) -> DatabaseCommandResult<()> {
        if !sync_codec::coberto(tipo) {
            return Err(sync_codec::nao_coberto(tipo));
        }
        let raiz = AggregateRef::new(tipo, id);
        if sync_codec::ler_canonico(self.tx, &raiz)?.is_none() {
            return Err(DatabaseCommandError::not_found(format!(
                "Não há {tipo} {id} para excluir."
            )));
        }
        if self.raiz_da_exclusao.is_none() {
            self.raiz_da_exclusao = Some(raiz.clone());
        }

        let coleta = coletar(&raiz, |agregado| {
            sync_codec::impactos_da_exclusao(self.tx, agregado)
        })?;

        // O escopo de cada afetado é lido AGORA, com todos ainda vivos. Afetado sem universo não
        // vira evento com `universe_id` vazio: é inconsistência, e a transação inteira falha.
        let mut preparados = Vec::with_capacity(coleta.excluidos.len());
        for agregado in coleta.excluidos {
            let estado = sync_codec::ler_canonico(self.tx, &agregado)?;
            let universe_id = universo_do_afetado(&agregado, estado)?;
            preparados.push((agregado, universe_id));
        }
        for agregado in &coleta.reescritos {
            universo_do_afetado(agregado, sync_codec::ler_canonico(self.tx, agregado)?)?;
        }

        let afetados = coleta
            .reescritos
            .iter()
            .map(|agregado| (agregado, "alterado"))
            .chain(preparados.iter().map(|(agregado, _)| (agregado, "apagado")));
        for (agregado, efeito) in afetados {
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
                    "Não dá para excluir agora: {} {} seria {efeito} e {por_que}. Resolva isso \
                     antes; nada foi apagado.",
                    agregado.aggregate_type, agregado.aggregate_id
                )));
            }
        }

        // **A ordem de emissão depende do tipo de sobrevivente.**
        //
        // ```text
        // 1  reescrita de agregado com linha própria (card)  ANTES das exclusões
        // 2  exclusões (descendentes → raiz)
        // 3  reescrita de existência derivada (ordens)       DEPOIS das exclusões
        // ```
        //
        // O card sobrevive com linha própria: o estado dele sem o campo é materializável antes de o
        // campo ser apagado, e vir primeiro é o que faz o receptor conhecer a concorrência ANTES do
        // SQL destrutivo — uma edição concorrente abre divergência e o delete seguinte é bloqueado.
        //
        // A ordem de um livro é o contrário: a lista sem o capítulo só materializa depois de o
        // capítulo sair. Como o cursor de uma origem é contíguo, pôr a ordem primeiro travaria a
        // origem inteira — foi o travamento que a B2.1 encontrou.
        let (derivados, com_linha): (Vec<_>, Vec<_>) = coleta
            .reescritos
            .into_iter()
            .partition(|agregado| sync_codec::existencia_derivada(&agregado.aggregate_type));
        for agregado in com_linha {
            self.declarar_reescrita(agregado);
        }
        for (agregado, universe_id) in preparados {
            let ja = self.operacoes.iter().any(|operacao| {
                matches!(operacao, Operacao::Excluiu { agregado: existente, .. } if existente == &agregado)
                    || matches!(operacao, Operacao::Efeito { agregado: existente, .. } if existente == &agregado)
            });
            if ja {
                continue;
            }
            self.operacoes.push(Operacao::Excluiu {
                agregado,
                universe_id,
            });
        }
        for agregado in derivados {
            self.declarar_reescrita(agregado);
        }
        Ok(())
    }

    /// Declara a exclusão de um agregado que **já não existe** aqui — para torná-la descendente de
    /// uma revisão que chegou de fora. Só a resolução de uma decisão usa isto: "manter o local"
    /// quando o local é a ausência e o outro aparelho reescreveu o agregado.
    ///
    /// Sem preflight nem cascata: nada é apagado agora. O evento parte da revisão corrente, que o
    /// chamador ajustou antes.
    pub fn reafirmou_exclusao(&mut self, agregado: AggregateRef, universe_id: &str) {
        self.operacoes.push(Operacao::Excluiu {
            agregado,
            universe_id: universe_id.to_string(),
        });
    }

    /// **Esta mutação é a resolução de um conflito.** Uma vez por mutação.
    pub fn declarar_resolucao(
        &mut self,
        declaracao: DeclaracaoDeResolucao,
    ) -> DatabaseCommandResult<()> {
        if self.resolucao.is_some() {
            return Err(DatabaseCommandError::storage(
                "Uma mutação resolve um conflito só. Nada foi confirmado.",
            ));
        }
        let chave = crate::domain::conflito::chave_do_conflito(
            &declaracao.kind,
            &declaracao.participante_um,
            &declaracao.participante_outro,
        );
        if chave != declaracao.conflict_key {
            return Err(DatabaseCommandError::storage(
                "A resolução declara participantes que não produzem a chave do conflito. Nada foi confirmado.",
            ));
        }
        self.resolucao = Some(declaracao);
        Ok(())
    }

    /// Efeito de uma resolução com a base escolhida. O estado final (existe ou não) já precisa
    /// estar no domínio quando a mutação terminar.
    pub fn efeito(
        &mut self,
        agregado: AggregateRef,
        operacao: Operation,
        base_rev: &str,
        other_rev: &str,
        universe_id: &str,
    ) -> DatabaseCommandResult<()> {
        if self.resolucao.is_none() {
            return Err(DatabaseCommandError::storage(
                "Efeito com base escolhida só existe dentro de uma resolução. Nada foi confirmado.",
            ));
        }
        if !sync_codec::coberto(&agregado.aggregate_type) {
            return Err(sync_codec::nao_coberto(&agregado.aggregate_type));
        }
        self.operacoes.retain(|operacao| {
            !matches!(operacao, Operacao::Gravou(a) | Operacao::Reescreveu(a) if a == &agregado)
        });
        self.operacoes.push(Operacao::Efeito {
            agregado,
            operacao,
            base_rev: base_rev.to_string(),
            other_rev: other_rev.to_string(),
            universe_id: universe_id.to_string(),
        });
        Ok(())
    }

    /// A exclusão como efeito de uma resolução: o MESMO preflight de [`Mutacao::excluir`] (roda de
    /// novo, agora — a decisão antiga não autoriza a cascata), e a raiz sai com a base escolhida.
    pub fn excluir_como_efeito(
        &mut self,
        tipo: &str,
        id: &str,
        base_rev: &str,
        other_rev: &str,
    ) -> DatabaseCommandResult<()> {
        self.excluir(tipo, id)?;
        let raiz = AggregateRef::new(tipo, id);
        let posicao = self.operacoes.iter().position(
            |operacao| matches!(operacao, Operacao::Excluiu { agregado, .. } if agregado == &raiz),
        );
        let Some(posicao) = posicao else {
            return Err(DatabaseCommandError::storage(
                "A exclusão preparada da resolução sumiu. Nada foi confirmado.",
            ));
        };
        let Operacao::Excluiu { universe_id, .. } = self.operacoes.remove(posicao) else {
            unreachable!("a posição veio de um Excluiu");
        };
        self.efeito(raiz, Operation::Delete, base_rev, other_rev, &universe_id)
    }

    fn declarar_reescrita(&mut self, agregado: AggregateRef) {
        let ja = self.operacoes.iter().any(
            |operacao| matches!(operacao, Operacao::Reescreveu(existente) if existente == &agregado),
        );
        if !ja {
            self.operacoes.push(Operacao::Reescreveu(agregado));
        }
    }

    fn finalizar(self, identidade: &DeviceIdentity) -> DatabaseCommandResult<()> {
        falha::verificar(falha::Ponto::AposAMutacao)?;

        let excluidos: Vec<&AggregateRef> = self
            .operacoes
            .iter()
            .filter_map(|operacao| match operacao {
                Operacao::Excluiu { agregado, .. } => Some(agregado),
                _ => None,
            })
            .collect();

        // ── 1. O que vai virar evento, na ordem de emissão ──
        //
        // Tudo é decidido ANTES de gravar o primeiro evento: o grupo precisa saber quantos membros
        // tem, e o total entra na assinatura de cada um (B2.2).
        struct Pendente {
            universe_id: String,
            agregado: AggregateRef,
            operacao: Operation,
            payload: String,
            /// A base escolhida de um efeito de resolução; `None` = a revisão corrente.
            base_forcada: Option<String>,
            outra: String,
        }
        let mut pendentes: Vec<Pendente> = Vec::new();
        let mut emitidos: Vec<AggregateRef> = Vec::new();
        for operacao in &self.operacoes {
            match operacao {
                Operacao::Gravou(agregado) | Operacao::Reescreveu(agregado) => {
                    // Gravado e depois excluído na mesma ação: só a exclusão existe para os outros.
                    if excluidos.contains(&agregado) || emitidos.contains(agregado) {
                        continue;
                    }
                    let estado = sync_codec::ler_canonico(self.tx, agregado)?.ok_or_else(|| {
                        let como = match operacao {
                            Operacao::Reescreveu(_) => "como sobrevivente de uma exclusão",
                            _ => "como gravado",
                        };
                        DatabaseCommandError::storage(format!(
                            "A mutação declarou {} {} {como}, e ele não existe no fim da \
                             transação. Nada foi confirmado.",
                            agregado.aggregate_type, agregado.aggregate_id
                        ))
                    })?;
                    let universe_id = universo_do_afetado(agregado, Some(estado.clone()))?;
                    // O evento que sai daqui tem de ser aceitável pelo apply remoto. Mesma função
                    // dos dois lados: falhar aqui desfaz domínio e evento juntos.
                    sync_codec::validar_para_emissao(self.tx, agregado, &estado.payload)?;
                    emitidos.push(agregado.clone());
                    // Mesmo estado, mesmo payload: nada a revisar.
                    if sync_codec::payload_da_revisao_corrente(self.tx, agregado)?.as_deref()
                        == Some(estado.payload.as_str())
                    {
                        continue;
                    }
                    pendentes.push(Pendente {
                        universe_id,
                        agregado: agregado.clone(),
                        operacao: Operation::Upsert,
                        payload: estado.payload,
                        base_forcada: None,
                        outra: String::new(),
                    });
                }
                Operacao::Excluiu {
                    agregado,
                    universe_id,
                } => {
                    if emitidos.contains(agregado) {
                        continue;
                    }
                    if sync_codec::ler_canonico(self.tx, agregado)?.is_some() {
                        return Err(DatabaseCommandError::storage(format!(
                            "A mutação preparou a exclusão de {} {}, e ele continua existindo no fim \
                             da transação. Nada foi confirmado.",
                            agregado.aggregate_type, agregado.aggregate_id
                        )));
                    }
                    emitidos.push(agregado.clone());
                    pendentes.push(Pendente {
                        universe_id: universe_id.clone(),
                        agregado: agregado.clone(),
                        operacao: Operation::Delete,
                        payload: String::new(),
                        base_forcada: None,
                        outra: String::new(),
                    });
                }
                Operacao::Efeito {
                    agregado,
                    operacao,
                    base_rev,
                    other_rev,
                    universe_id,
                } => {
                    if emitidos.contains(agregado) {
                        return Err(DatabaseCommandError::storage(format!(
                            "A resolução declarou {} {} duas vezes. Nada foi confirmado.",
                            agregado.aggregate_type, agregado.aggregate_id
                        )));
                    }
                    let estado = sync_codec::ler_canonico(self.tx, agregado)?;
                    let payload = match (operacao, estado) {
                        (Operation::Upsert, Some(estado)) => {
                            sync_codec::validar_para_emissao(self.tx, agregado, &estado.payload)?;
                            estado.payload
                        }
                        (Operation::Delete, None) => String::new(),
                        (Operation::Upsert, None) => {
                            return Err(DatabaseCommandError::storage(format!(
                                "A resolução mantém {} {}, e ele não existe no fim da transação. Nada foi confirmado.",
                                agregado.aggregate_type, agregado.aggregate_id
                            )))
                        }
                        (Operation::Delete, Some(_)) => {
                            return Err(DatabaseCommandError::storage(format!(
                                "A resolução exclui {} {}, e ele continua existindo no fim da transação. Nada foi confirmado.",
                                agregado.aggregate_type, agregado.aggregate_id
                            )))
                        }
                    };
                    emitidos.push(agregado.clone());
                    pendentes.push(Pendente {
                        universe_id: universe_id.clone(),
                        agregado: agregado.clone(),
                        operacao: *operacao,
                        payload,
                        base_forcada: Some(base_rev.clone()),
                        outra: other_rev.clone(),
                    });
                }
            }
        }

        // ── 1b. A resolução: o certificado sai da lista EXATA do que vai ser emitido ──
        if self.resolucao.is_some() && pendentes.is_empty() {
            return Err(DatabaseCommandError::storage(
                "A resolução não produziu nenhum efeito: não há o que certificar. Nada foi confirmado.",
            ));
        }
        let mut revisoes_esperadas: Vec<String> = Vec::with_capacity(pendentes.len() + 1);
        let mut chave_da_resolucao: Option<(String, String)> = None;
        if let Some(declaracao) = &self.resolucao {
            use crate::infrastructure::sqlite::sync_codec::resolucao::{
                Certificado, EfeitoCanonico, ParticipanteCanonico, TIPO,
            };
            let mut results = Vec::with_capacity(pendentes.len());
            for pendente in &pendentes {
                let base = match &pendente.base_forcada {
                    Some(base) => base.clone(),
                    None => sync_codec::revisao_corrente(self.tx, &pendente.agregado)?
                        .unwrap_or_default(),
                };
                let resultado = crate::domain::sync::compute_revision(
                    &base,
                    &pendente.agregado,
                    pendente.operacao,
                    &pendente.payload,
                );
                revisoes_esperadas.push(resultado.clone());
                results.push(EfeitoCanonico {
                    aggregate_type: pendente.agregado.aggregate_type.clone(),
                    aggregate_id: pendente.agregado.aggregate_id.clone(),
                    operation: pendente.operacao.as_str().to_string(),
                    base_rev: base,
                    other_rev: pendente.outra.clone(),
                    result_rev: resultado,
                });
            }
            let (um, outro) = (
                declaracao.participante_um.clone(),
                declaracao.participante_outro.clone(),
            );
            let (a, b) = if um.canonico().as_bytes() <= outro.canonico().as_bytes() {
                (um, outro)
            } else {
                (outro, um)
            };
            let certificado = Certificado {
                conflict_key: declaracao.conflict_key.clone(),
                kind: declaracao.kind.clone(),
                participant_a: ParticipanteCanonico::do_conflito(&a),
                participant_b: ParticipanteCanonico::do_conflito(&b),
                choice: declaracao.choice.clone(),
                results,
            };
            let payload = certificado.canonico()?;
            let agregado = AggregateRef::new(TIPO, &declaracao.conflict_key);
            if sync_codec::revisao_corrente(self.tx, &agregado)?.is_some()
                || sync_codec::ler_canonico(self.tx, &agregado)?.is_some()
            {
                return Err(DatabaseCommandError::conflict(
                    "Este conflito já tem uma decisão registrada aqui. Nada foi alterado.",
                ));
            }
            self.tx
                .execute(
                    "INSERT INTO conflict_resolutions (conflict_key, universe_id, kind, certificate)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        &declaracao.conflict_key,
                        &declaracao.universe_id,
                        &declaracao.kind,
                        &payload
                    ],
                )
                .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            sync_codec::validar_para_emissao(self.tx, &agregado, &payload)?;
            let resultado =
                crate::domain::sync::compute_revision("", &agregado, Operation::Upsert, &payload);
            revisoes_esperadas.insert(0, resultado.clone());
            pendentes.insert(
                0,
                Pendente {
                    universe_id: declaracao.universe_id.clone(),
                    agregado,
                    operacao: Operation::Upsert,
                    payload,
                    base_forcada: None,
                    outra: String::new(),
                },
            );
            chave_da_resolucao = Some((declaracao.conflict_key.clone(), resultado));
        }

        // ── 2. A ação inteira como um grupo ──
        //
        // Um id, índices contíguos, o total. É o que permite ao receptor não aplicar metade de uma
        // ação: nenhum membro altera o domínio de lá antes de o grupo inteiro chegar e poder entrar.
        let mutation_id = crate::domain::ids::new_id();
        let total = i64::try_from(pendentes.len())
            .ok()
            .filter(|total| *total <= crate::domain::sync::MAXIMO_DE_MEMBROS_DO_GRUPO)
            .ok_or_else(|| {
                DatabaseCommandError::conflict(format!(
                    "Esta ação altera {} itens de uma vez, acima do máximo de {} que a \
                     sincronização transporta como uma ação só. Nada foi alterado.",
                    pendentes.len(),
                    crate::domain::sync::MAXIMO_DE_MEMBROS_DO_GRUPO
                ))
            })?;
        let (kind, root_type, root_id) = match (&self.resolucao, &self.raiz_da_exclusao) {
            (Some(declaracao), _) => (
                crate::infrastructure::sqlite::sync_codec::resolucao::KIND_DO_GRUPO.to_string(),
                crate::infrastructure::sqlite::sync_codec::resolucao::TIPO.to_string(),
                declaracao.conflict_key.clone(),
            ),
            (None, Some(raiz)) => (
                "delete_tree".to_string(),
                raiz.aggregate_type.clone(),
                raiz.aggregate_id.clone(),
            ),
            (None, None) => (String::new(), String::new(), String::new()),
        };
        for (indice, pendente) in pendentes.iter().enumerate() {
            if let Some(base) = &pendente.base_forcada {
                // A base escolhida vira, por um instante, a revisão corrente: é dela que o evento
                // parte. O tombstone sai — o que for emitido agora é a cabeça nova.
                self.tx
                    .execute(
                        "INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(aggregate_type, aggregate_id)
                         DO UPDATE SET current_rev = excluded.current_rev",
                        rusqlite::params![
                            &pendente.agregado.aggregate_type,
                            &pendente.agregado.aggregate_id,
                            base
                        ],
                    )
                    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
                self.tx
                    .execute(
                        "DELETE FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                        [
                            &pendente.agregado.aggregate_type,
                            &pendente.agregado.aggregate_id,
                        ],
                    )
                    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            }
            let envelope = append_event_in_transaction(
                self.tx,
                identidade,
                &LocalChange {
                    universe_id: &pendente.universe_id,
                    aggregate: pendente.agregado.clone(),
                    operation: pendente.operacao,
                    payload: &pendente.payload,
                    grupo: GrupoDeMutacao {
                        mutation_id: mutation_id.clone(),
                        index: indice as i64,
                        count: total,
                        kind: kind.clone(),
                        root_type: root_type.clone(),
                        root_id: root_id.clone(),
                    },
                },
            )?;
            if let Some(esperada) = revisoes_esperadas.get(indice) {
                if envelope.new_rev != *esperada {
                    return Err(DatabaseCommandError::storage(format!(
                        "{} {} saiu com uma revisão diferente da que o certificado declara. Nada foi confirmado.",
                        pendente.agregado.aggregate_type, pendente.agregado.aggregate_id
                    )));
                }
            }
            if indice == 0 {
                falha::verificar(falha::Ponto::DuranteOsEventos)?;
            }
        }

        // ── 3. O índice local do conflito fecha na mesma transação ──
        if let (Some(declaracao), Some((chave, revisao))) = (&self.resolucao, chave_da_resolucao) {
            self.tx
                .execute(
                    "UPDATE sync_divergences
                        SET resolved_at = CASE WHEN resolved_at = '' THEN ?1 ELSE resolved_at END,
                            resolution = CASE WHEN resolution = '' THEN ?2 ELSE resolution END,
                            resolution_rev = ?3
                      WHERE conflict_key = ?4 AND resolution_rev = ''",
                    rusqlite::params![
                        crate::domain::ids::now_timestamp(),
                        &declaracao.resolucao_local,
                        revisao,
                        chave
                    ],
                )
                .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        }
        Ok(())
    }
}

/// O universo de um agregado afetado, ou erro. Nunca `""`.
///
/// Um evento sem escopo não é roteável nem verificável no receptor, e um `unwrap_or_default` aqui
/// transformaria uma inconsistência do banco num evento assinado e retransmitido para sempre.
fn universo_do_afetado(
    agregado: &AggregateRef,
    estado: Option<sync_codec::EstadoDoAgregado>,
) -> DatabaseCommandResult<String> {
    match estado {
        Some(estado) if !estado.universe_id.trim().is_empty() => Ok(estado.universe_id),
        Some(_) => Err(DatabaseCommandError::storage(format!(
            "{} {} não tem universo. O banco está inconsistente e nada foi confirmado.",
            agregado.aggregate_type, agregado.aggregate_id
        ))),
        None => Err(DatabaseCommandError::storage(format!(
            "{} {} foi listado como afetado e não existe. O banco está inconsistente e nada foi \
             confirmado.",
            agregado.aggregate_type, agregado.aggregate_id
        ))),
    }
}

/// O resultado da coleta de impactos de uma exclusão.
#[derive(Debug, Default, PartialEq, Eq)]
struct Coleta {
    /// Descendentes primeiro, a raiz por último.
    excluidos: Vec<AggregateRef>,
    /// Sobreviventes, sem os que também são excluídos (excluir vence reescrever).
    reescritos: Vec<AggregateRef>,
}

/// Todos os impactos da exclusão de `raiz`.
///
/// Resiste a ciclo e a filho compartilhado. Cada agregado é marcado como visitado **antes** de
/// descer nele, então nada é visitado duas vezes. Um filho que já está no caminho atual é um ciclo
/// de posse — dado corrompido —, e a exclusão é recusada em vez de escolher uma ordem arbitrária.
/// Um `Bloqueado` em qualquer nível recusa a exclusão inteira.
fn coletar(
    raiz: &AggregateRef,
    mut impactos: impl FnMut(&AggregateRef) -> DatabaseCommandResult<Vec<Impacto>>,
) -> DatabaseCommandResult<Coleta> {
    type Impactos<'f> = dyn FnMut(&AggregateRef) -> DatabaseCommandResult<Vec<Impacto>> + 'f;

    fn visitar(
        agregado: &AggregateRef,
        impactos: &mut Impactos<'_>,
        visitados: &mut HashSet<(String, String)>,
        caminho: &mut Vec<AggregateRef>,
        coleta: &mut Coleta,
    ) -> DatabaseCommandResult<()> {
        let chave = (
            agregado.aggregate_type.clone(),
            agregado.aggregate_id.clone(),
        );
        if !visitados.insert(chave) {
            if caminho.contains(agregado) {
                return Err(DatabaseCommandError::storage(format!(
                    "Ciclo de posse em {} {}: um agregado aparece como descendente de si mesmo. \
                     Nada foi apagado.",
                    agregado.aggregate_type, agregado.aggregate_id
                )));
            }
            return Ok(());
        }
        caminho.push(agregado.clone());
        for impacto in impactos(agregado)? {
            match impacto {
                Impacto::Excluido(filho) => visitar(&filho, impactos, visitados, caminho, coleta)?,
                Impacto::Reescrito(sobrevivente) => {
                    if !coleta.reescritos.contains(&sobrevivente) {
                        coleta.reescritos.push(sobrevivente);
                    }
                }
                Impacto::Bloqueado(motivo) => {
                    return Err(DatabaseCommandError::conflict(format!(
                        "Não dá para excluir agora: {motivo}. Nada foi apagado."
                    )))
                }
            }
        }
        caminho.pop();
        coleta.excluidos.push(agregado.clone());
        Ok(())
    }

    let mut visitados = HashSet::new();
    let mut caminho = Vec::new();
    let mut coleta = Coleta::default();
    visitar(
        raiz,
        &mut impactos,
        &mut visitados,
        &mut caminho,
        &mut coleta,
    )?;
    let excluidos = coleta.excluidos.clone();
    coleta
        .reescritos
        .retain(|sobrevivente| !excluidos.contains(sobrevivente));
    Ok(coleta)
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
pub(crate) mod tests {
    use super::falha::{armar, Ponto};
    use super::*;
    use crate::application::{canvas_service, manuscript_service};
    use crate::domain::canvas::Attachment;
    use crate::domain::manuscript::ChapterUpdate;
    use crate::infrastructure::sqlite::sync_session::receber_eventos_sem_conferir_blobs as receber_eventos;
    use crate::infrastructure::sqlite::test_support::{
        origem_remota_confiavel, seed_universe, self_de_teste, TemporaryDatabase,
    };
    use crate::infrastructure::sqlite::{canvas_repository, sync_codec};
    use rusqlite::{Connection, OptionalExtension};

    /// Um aparelho de teste: banco com universo → história → livro → capítulo legado `c1`.
    pub(crate) struct Aparelho {
        pub(crate) banco: TemporaryDatabase,
        pub(crate) eu: DeviceIdentity,
    }

    impl Aparelho {
        pub(crate) fn novo() -> Self {
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

        pub(crate) fn conexao(&self) -> Connection {
            self.banco.connection()
        }

        pub(crate) fn editar(&self, texto: &str) -> DatabaseCommandResult<()> {
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
        pub(crate) fn anexar(&self, id: &str) {
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

        pub(crate) fn eventos(&self) -> Vec<(String, String, String)> {
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

        pub(crate) fn existe(&self, tabela: &str, id: &str) -> bool {
            self.conexao()
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {tabela} WHERE id = ?1)"),
                    [id],
                    |row| row.get(0),
                )
                .expect("existe")
        }

        pub(crate) fn estado_causal(&self, tipo: &str, id: &str) -> Option<String> {
            self.conexao()
                .query_row(
                    "SELECT current_rev FROM sync_aggregate_state WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                    [tipo, id],
                    |row| row.get(0),
                )
                .optional()
                .expect("estado")
        }

        pub(crate) fn tombstone(&self, tipo: &str, id: &str) -> bool {
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
        pub(crate) fn coerente(&self, tipo: &str, id: &str) {
            let connection = self.conexao();
            let agregado = AggregateRef::new(tipo, id);
            match sync_codec::ler_canonico(&connection, &agregado).expect("ler") {
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
    fn coletar_resiste_a_ciclo_e_a_filho_compartilhado() {
        let r = |id: &str| AggregateRef::new("chapter", id);
        let x = |id: &str| Impacto::Excluido(AggregateRef::new("chapter", id));
        // a → b, a → c, b → d, c → d (diamante)
        let diamante = |agregado: &AggregateRef| -> DatabaseCommandResult<Vec<Impacto>> {
            Ok(match agregado.aggregate_id.as_str() {
                "a" => vec![x("b"), x("c")],
                "b" | "c" => vec![x("d")],
                _ => vec![],
            })
        };
        let coleta = coletar(&r("a"), diamante).expect("diamante");
        let ids: Vec<&str> = coleta
            .excluidos
            .iter()
            .map(|a| a.aggregate_id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["d", "b", "c", "a"],
            "cada um uma vez, filhos antes do pai"
        );

        // a → b → a: sem marcar visitado antes da descida, isto nunca terminaria.
        let mut chamadas = 0;
        let ciclo = |agregado: &AggregateRef| -> DatabaseCommandResult<Vec<Impacto>> {
            chamadas += 1;
            assert!(chamadas < 10, "a coleta entrou em laço");
            Ok(match agregado.aggregate_id.as_str() {
                "a" => vec![x("b")],
                _ => vec![x("a")],
            })
        };
        let erro = coletar(&r("a"), ciclo).expect_err("ciclo é inconsistência");
        assert!(erro.message.contains("Ciclo de posse"), "{}", erro.message);

        // Auto-referência.
        let proprio =
            |_: &AggregateRef| -> DatabaseCommandResult<Vec<Impacto>> { Ok(vec![x("a")]) };
        assert!(coletar(&r("a"), proprio).is_err());
    }

    /// Reescrito sai da lista quando o mesmo agregado também é excluído; Bloqueado recusa tudo.
    #[test]
    fn coletar_separa_reescrito_de_excluido_e_bloqueio_recusa() {
        // Um sobrevivente qualquer: aqui só importa a regra de coleta, não o tipo.
        let ordem = AggregateRef::new("planning_item", "p1");
        let livro = |agregado: &AggregateRef| -> DatabaseCommandResult<Vec<Impacto>> {
            Ok(match agregado.aggregate_type.as_str() {
                "book" => vec![
                    Impacto::Excluido(ordem.clone()),
                    Impacto::Excluido(AggregateRef::new("chapter", "c1")),
                ],
                "chapter" => vec![Impacto::Reescrito(ordem.clone())],
                _ => vec![],
            })
        };
        let coleta = coletar(&AggregateRef::new("book", "b1"), livro).expect("livro");
        assert!(coleta.reescritos.is_empty(), "excluir vence reescrever");
        assert_eq!(coleta.excluidos.len(), 3);

        let so_capitulo = coletar(&AggregateRef::new("chapter", "c1"), livro).expect("capítulo");
        assert_eq!(so_capitulo.reescritos, vec![ordem.clone()]);

        let bloqueado = |_: &AggregateRef| -> DatabaseCommandResult<Vec<Impacto>> {
            Ok(vec![Impacto::Bloqueado("card ligado".into())])
        };
        let erro = coletar(&AggregateRef::new("chapter", "c1"), bloqueado).expect_err("bloqueio");
        assert!(erro.message.contains("card ligado"), "{}", erro.message);
    }

    /// Afetado sem universo derruba a exclusão inteira; nunca sai evento com `universe_id` vazio.
    #[test]
    fn exclusao_com_afetado_sem_universo_falha_inteira() {
        let aparelho = Aparelho::novo();
        {
            let connection = aparelho.banco.database.write().expect("escrita");
            seed_universe(&connection, "");
            connection
                .execute(
                    "INSERT INTO attachments (id, universe_id, owner_type, owner_id, data_url, created_at)
                     VALUES ('a-sem-escopo', '', 'chapter', 'c1', '', '2026-09-15 10:00:00')",
                    [],
                )
                .expect("anexo inconsistente");
        }

        let erro = manuscript_service::delete_chapter(&aparelho.banco.database, &aparelho.eu, "c1")
            .expect_err("tinha que recusar");
        assert!(
            erro.message.contains("não tem universo"),
            "{}",
            erro.message
        );
        assert!(aparelho.existe("chapters", "c1"));
        assert!(aparelho.existe("attachments", "a-sem-escopo"));
        assert!(aparelho.eventos().is_empty());

        let erro = universo_do_afetado(&AggregateRef::new("attachment", "x"), None)
            .expect_err("afetado sumido");
        assert!(erro.message.contains("não existe"), "{}", erro.message);
    }

    /// **A exclusão de `c1` como uma origem real a emite desde a B2.2:** um grupo de duas partes —
    /// a posição, depois o capítulo — com o mesmo `mutation_id`, assinado junto. Montar os dois como
    /// eventos soltos testaria um emissor que não existe mais.
    pub(crate) fn exclusao_remota_do_capitulo(
        origem: &DeviceIdentity,
        rev_do_capitulo: &str,
    ) -> [crate::domain::sync::EventEnvelope; 2] {
        exclusao_remota_do_capitulo_no_grupo(origem, rev_do_capitulo, &crate::domain::ids::new_id())
    }

    /// A mesma ação, com o `mutation_id` escolhido: para provar que o grupo é da origem.
    pub(crate) fn exclusao_remota_do_capitulo_no_grupo(
        origem: &DeviceIdentity,
        rev_do_capitulo: &str,
        mutation_id: &str,
    ) -> [crate::domain::sync::EventEnvelope; 2] {
        // Montada pelo helper canônico (B6, item 8): seq contíguo, grupo coerente e assinatura de
        // cada membro saem dele, não de repetição à mão em cada teste.
        let eventos = crate::infrastructure::sqlite::test_support::AcaoRemota::da(origem)
            .com_mutation_id(mutation_id)
            .delete(AggregateRef::new("chapter_position", "c1"), "")
            .delete(AggregateRef::new("chapter", "c1"), rev_do_capitulo)
            .exclusao_de(AggregateRef::new("chapter", "c1"))
            .assinada();
        let [posicao, capitulo]: [crate::domain::sync::EventEnvelope; 2] = eventos
            .try_into()
            .expect("a ação da exclusão do capítulo tem dois membros");
        [posicao, capitulo]
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
            &eventos[eventos.len() - 6..],
            &[
                ("chapter_position".into(), "c1".into(), "delete".into()),
                ("attachment_position".into(), "a1".into(), "delete".into()),
                ("attachment".into(), "a1".into(), "delete".into()),
                ("attachment_position".into(), "a2".into(), "delete".into()),
                ("attachment".into(), "a2".into(), "delete".into()),
                ("chapter".into(), "c1".into(), "delete".into()),
            ],
            "cada filho antes do seu pai, a posição antes do item; nenhuma lista reescrita (B2.2)"
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
        // **O tipo aqui é inventado de propósito.** Este teste é sobre falhar fechado diante de um
        // agregado desconhecido, não sobre uma etapa pendente. Apontá-lo para um tipo real ainda
        // não coberto (`canvas_node` na B5, `entity_template_set` na B6) o faz quebrar toda vez que
        // a cobertura avança — e, pior, o faz parar de testar o que promete no dia em que alguém
        // reapontar para um tipo que já ganhou codec.
        let erro = Mutacao::executar(&aparelho.banco.database, &aparelho.eu, |m| {
            m.gravou("agregado_que_nunca_vai_existir", "x1")
        })
        .expect_err("tipo desconhecido tem de falhar fechado");
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
            // B2.2: criar declara o anexo e a posição dele; excluir tira a posição e depois o anexo.
            vec!["upsert", "upsert", "delete", "delete"]
        );
        let _ = std::fs::remove_dir_all(raiz);
    }

    /// **SQLite × blob store:** rollback depois de o arquivo ser publicado deixa o blob órfão — e só
    /// ele. Repetir a ação reaproveita o mesmo arquivo; nada duplica, nada fica em staging.
    #[test]
    fn rollback_depois_do_blob_deixa_so_o_arquivo_orfao_e_repetir_o_reaproveita() {
        let aparelho = Aparelho::novo();
        let raiz = std::env::temp_dir().join(format!("narrahub-b1-orfao-{}", uuid::Uuid::new_v4()));
        let store = crate::infrastructure::blob_store::BlobStore::new(raiz.clone());
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let arquivos = |pasta: &std::path::Path| -> Vec<std::path::PathBuf> {
            let mut pilha = vec![pasta.to_path_buf()];
            let mut achados = Vec::new();
            while let Some(atual) = pilha.pop() {
                let Ok(filhos) = std::fs::read_dir(&atual) else {
                    continue;
                };
                for filho in filhos.flatten() {
                    let caminho = filho.path();
                    if caminho.is_dir() {
                        pilha.push(caminho);
                    } else {
                        achados.push(caminho);
                    }
                }
            }
            achados
        };
        let criar = || {
            canvas_service::create_attachment(
                &aparelho.banco.database,
                &store,
                &aparelho.eu,
                "u1",
                "chapter",
                "c1",
                png,
                "",
            )
        };

        armar(Some(Ponto::AntesDoCommit));
        assert!(criar().is_err());

        let linhas: i64 = aparelho
            .conexao()
            .query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(linhas, 0, "linha sem commit sobreviveu");
        assert!(aparelho.eventos().is_empty());
        let orfaos = arquivos(&store.raiz());
        assert_eq!(
            orfaos.len(),
            1,
            "o blob publicado antes do rollback fica, e só ele"
        );
        assert!(
            arquivos(&raiz.join(crate::infrastructure::blob_store::DIRETORIO_DE_STAGING))
                .is_empty(),
            "sobrou temporário em staging"
        );

        let anexo = criar().expect("repetir");
        assert_eq!(
            arquivos(&store.raiz()),
            orfaos,
            "a repetição duplicou o arquivo"
        );
        assert_eq!(
            orfaos[0].file_name().and_then(|n| n.to_str()),
            Some(anexo.blob_hash.as_str()),
            "a linha nova aponta para o arquivo que tinha ficado órfão"
        );
        aparelho.coerente("attachment", &anexo.id);
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
        let [posicao, exclusao] = exclusao_remota_do_capitulo(&outra, &rev_conhecida);

        let mut connection = aparelho.banco.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[posicao, exclusao]).expect("receber");
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
        let [posicao, exclusao] = exclusao_remota_do_capitulo(&outra, &rev_conhecida);

        let mut connection = aparelho.banco.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[posicao, exclusao]).expect("receber");
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
                "sync_codec/mod",
                include_str!("../infrastructure/sqlite/sync_codec/mod.rs"),
            ),
            (
                "sync_codec/manuscrito",
                include_str!("../infrastructure/sqlite/sync_codec/manuscrito.rs"),
            ),
            (
                "sync_codec/anexo",
                include_str!("../infrastructure/sqlite/sync_codec/anexo.rs"),
            ),
            (
                "universe_repository",
                include_str!("../infrastructure/sqlite/universe_repository.rs"),
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

    /// **Nenhuma escrita sincronizável coberta fica fora da `Mutacao` (gates da B2 e B3).**
    ///
    /// Varre TODA função pública dos serviços já cobertos. Leitura (`list_*`, `get*`, `stats`) é
    /// dispensada pelo nome; qualquer outra precisa passar pela fronteira, ou estar em
    /// `FORA_DE_PROPOSITO` **com o motivo**. Uma função nova que escreva por conta própria reprova
    /// aqui sem ninguém precisar lembrar de listá-la.
    #[test]
    fn escritas_do_manuscrito_passam_todas_pela_mutacao() {
        const FORA_DE_PROPOSITO: &[(&str, &str, &str)] = &[
            (
                "universe_service",
                "delete",
                "recusa sempre (a árvore do universo ainda não é coberta) e não escreve",
            ),
            (
                "knowledge_service",
                "sync_chapter_mentions",
                "menção é DERIVADA do texto do capítulo: cada aparelho recalcula ao aplicar o \
                 capítulo. Sincronizar mandaria o mesmo dado duas vezes, a segunda com chance \
                 de discordar da primeira",
            ),
        ];
        let leitura = |nome: &str| {
            nome == "list"
                || nome == "stats"
                || nome.starts_with("list_")
                || nome.starts_with("get")
        };
        let mut conferidas = 0;
        for (servico, fonte) in [
            ("manuscript_service", include_str!("manuscript_service.rs")),
            ("universe_service", include_str!("universe_service.rs")),
            ("entity_service", include_str!("entity_service.rs")),
            ("workspace_service", include_str!("workspace_service.rs")),
            ("canvas_service", include_str!("canvas_service.rs")),
            ("planning_service", include_str!("planning_service.rs")),
            ("knowledge_service", include_str!("knowledge_service.rs")),
        ] {
            let codigo = sem_testes(fonte);
            let mut resto = codigo;
            while let Some(inicio) = resto.find("\npub fn ") {
                let assinatura = &resto[inicio + "\npub fn ".len()..];
                let nome = &assinatura[..assinatura.find('(').expect("assinatura")];
                resto = &resto[inicio + 1..];
                if leitura(nome) {
                    continue;
                }
                let corpo = corpo(codigo, nome);
                if let Some((_, _, motivo)) = FORA_DE_PROPOSITO
                    .iter()
                    .find(|(fora_servico, fora, _)| *fora_servico == servico && *fora == nome)
                {
                    assert!(
                        !corpo.contains("Mutacao::executar("),
                        "{servico}::{nome} está em FORA_DE_PROPOSITO ({motivo}) e passou pela \
                         fronteira: tire da lista"
                    );
                    continue;
                }
                assert!(
                    corpo.contains("Mutacao::executar("),
                    "{servico}::{nome} escreve fora da Mutacao"
                );
                for proibido in [
                    "database.write()",
                    ".transaction(",
                    "transaction_with_behavior",
                    "append_event_in_transaction",
                ] {
                    assert!(
                        !corpo.contains(proibido),
                        "{servico}::{nome} usa `{proibido}` por fora da fronteira"
                    );
                }
                conferidas += 1;
            }
        }
        // 10 do manuscrito, 2 do universo (B2); 5 de entidade, 5 de workspace (B3); 8 do
        // planejamento (B4); 10 de canvas e 4 de conhecimento (B1/B3/B5).
        //
        // `knowledge_service` **não estava na lista de fontes** até a B5 — o gate varria seis
        // serviços e esse não era um deles. As escritas de tag nunca reprovaram porque nunca
        // foram olhadas: estavam invisíveis, não dispensadas.
        assert_eq!(conferidas, 44, "o gate conferiu {conferidas} escritas");
    }
}
