//! Sync V2 — aplicação de evento recebido (ADR 0009 §12, etapa 4).
//!
//! Os quatro efeitos da seção 12, numa transação só:
//!
//! ```text
//! BEGIN
//!   INSERT INTO sync_events          o envelope inteiro, com a assinatura da origem
//!   UPDATE/DELETE no agregado        aplica
//!   INSERT INTO sync_applied_events  idempotência
//!   UPDATE sync_cursors              avança — etapa 5
//! COMMIT
//! ```
//!
//! | Se faltasse | O que quebra |
//! | --- | --- |
//! | o envelope | o relay não tem o que retransmitir; propagação transitiva morre |
//! | a aplicação | o cursor diz "vi", e o dado não está lá |
//! | a idempotência | reaplica na reconexão |
//! | o cursor | retransmite para sempre |
//!
//! O quarto efeito é da etapa 5, e por um motivo: decidir se o cursor pode
//! avançar depende de saber se a sequência ficou contígua, o que é decisão da
//! **sessão** e não de um evento isolado. Esta camada guarda e aplica; quem
//! ordena e move o cursor é a próxima etapa, na mesma transação.
//!
//! ## O que esta camada NÃO faz
//!
//! Não verifica assinatura nem consulta roster — isso é a etapa 7. Aqui a
//! pergunta é "este evento cabe na minha história?", não "confio em quem o
//! mandou?". Separar as duas é o que permite testar causalidade sem
//! criptografia e criptografia sem causalidade.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::conflito::ConflictParticipant;
use crate::domain::sync::{
    classify, compute_revision, AggregateRef, Causality, EventEnvelope, Operation,
};
use crate::infrastructure::sqlite::sync_codec::{self, Impacto};
use crate::infrastructure::sqlite::sync_repository::{self, aggregate_history};
use rusqlite::{OptionalExtension, Transaction};

/// O que aconteceu com o evento que chegou.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    /// Exclusão e edição partiram da mesma base. **Nenhuma das duas venceu.**
    ///
    /// "Delete vence" apaga uma edição feita de propósito; "edit ressuscita"
    /// desfaz uma exclusão feita de propósito. As duas são perda silenciosa,
    /// em direções opostas.
    DivergenteComExclusao { id_divergencia: String },
    /// Aplicado: o agregado mudou e o evento entrou no log.
    Aplicado,
    /// Já conhecíamos este evento. Repetição é normal em rede, e não é erro.
    JaAplicado,
    /// Duas edições a partir da mesma base. **Nada foi sobrescrito** — as
    /// duas revisões ficam preservadas e a divergência é registrada.
    Divergente { id_divergencia: String },
    /// Chegou a exclusão de um pai, e a cascata apagaria um descendente que a origem da exclusão
    /// não apagou antes. O `DELETE` físico **não** rodou: o pai e o filho continuam, e a exclusão
    /// fica como divergência `parent_deletion_blocked` para o escritor decidir.
    ExclusaoDoPaiBloqueada { id_divergencia: String },
    /// Chegou uma tag com o nome de uma tag daqui, e elas são agregados diferentes (B5).
    ///
    /// `UNIQUE(universe_id, name COLLATE NOCASE)` não deixa as duas coexistirem, e nenhuma das
    /// duas está errada: os dois escritores criaram "Mar" de boa-fé. **Nada é aplicado e nada é
    /// alterado.** A decisão — são a mesma tag, ou uma vai ser renomeada? — é do escritor.
    ConflitoDeNomeDeTag { id_divergencia: String },
    /// `base_rev` que não conhecemos. Não é conflito: falta história
    /// intermediária, e o agregado precisa de reconciliação.
    PrecisaReconciliar,
}

/// Aplica um evento recebido, dentro de uma transação que já existe.
///
/// Quem chama abre o `BEGIN IMMEDIATE` — pelo mesmo motivo do
/// `sync_repository`: uma transação `DEFERRED` leria o estado antes de o lock
/// existir.
pub fn apply_remote_event(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Applied> {
    // A idempotência vem primeiro, e é por `event_id`: dois aparelhos podem
    // mandar o mesmo evento na mesma sessão, e reaplicar um upsert seria
    // inofensivo enquanto reaplicar um delete depois de uma recriação
    // apagaria conteúdo novo.
    let ja_aplicado: bool = tx
        .query_row(
            "SELECT 1 FROM sync_applied_events WHERE event_id = ?1",
            [&envelope.event_id],
            |_| Ok(true),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .unwrap_or(false);
    if ja_aplicado {
        return Ok(Applied::JaAplicado);
    }

    // O envelope entra no log ANTES de qualquer decisão sobre aplicar. Mesmo
    // um evento que vira divergência precisa ficar guardado: é ele que o
    // relay retransmite, e é dele que a resolução do conflito vai ler a outra
    // versão. Guardar só o que foi aplicado perderia metade da história.
    guardar_envelope(tx, envelope)?;

    let aggregate = AggregateRef::new(&envelope.aggregate_type, &envelope.aggregate_id);
    let historia = aggregate_history(tx, &aggregate)?;

    match classify(&historia, &envelope.base_rev, &envelope.new_rev) {
        Causality::AlreadyPresent => {
            // A revisão já é conhecida por outro caminho — evento diferente,
            // mesmo resultado. Marca como aplicado para não voltar.
            marcar_aplicado(tx, &envelope.event_id)?;
            Ok(Applied::JaAplicado)
        }
        Causality::Sequential => {
            match envelope.operation {
                // Preflight causal da exclusão remota, ANTES de qualquer SQL destrutivo.
                //
                // A origem que apagou este agregado emitiu, antes, a exclusão de cada descendente
                // que ela conhecia. Se ainda existe descendente aqui, ou se um sobrevivente tem
                // decisão ou alteração pendente, é trabalho concorrente — e a FK ou o gatilho o
                // destruiriam em silêncio. Depois da cascata não há como corrigir.
                Operation::Delete => {
                    if let Some(motivo) = motivo_para_bloquear_exclusao_remota(tx, envelope)? {
                        let id = bloquear_exclusao(tx, envelope, &historia, &motivo)?;
                        return Ok(Applied::ExclusaoDoPaiBloqueada { id_divergencia: id });
                    }
                }
                // Dependências: pai ou capítulo citado que ainda não chegou é história incompleta.
                // Não aplica, não marca; o cursor espera. Pai divergente é erro (pai imutável).
                Operation::Upsert => {
                    if sync_codec::dependencias(tx, envelope)?.is_some() {
                        return Ok(Applied::PrecisaReconciliar);
                    }
                    // Esperar não resolve nome de tag ocupado, e aplicar é impossível: o
                    // `UNIQUE` do schema recusaria o `INSERT`. Vira decisão.
                    if envelope.aggregate_type == "content_tag" {
                        if let Some(homonima) =
                            sync_codec::conhecimento::tag_homonima(tx, envelope)?
                        {
                            let id = conflito_de_nome_de_tag(tx, envelope, &historia, &homonima)?;
                            return Ok(Applied::ConflitoDeNomeDeTag { id_divergencia: id });
                        }
                    }
                }
            }
            aplicar_com_estado_causal(tx, envelope)?;
            conferir_materializacao(tx, envelope)?;
            registrar_revisao(tx, envelope)?;
            marcar_aplicado(tx, &envelope.event_id)?;
            Ok(Applied::Aplicado)
        }
        Causality::Concurrent { base_rev } => {
            // Nada é sobrescrito. As duas revisões existem, e quem decide é o
            // humano (ADR 0009 §16). O evento fica no log e a revisão remota
            // entra na história — sem virar a corrente.
            registrar_revisao(tx, envelope)?;
            marcar_aplicado(tx, &envelope.event_id)?;
            let id = registrar_divergencia(tx, envelope, &base_rev, &historia)?;
            Ok(Applied::Divergente { id_divergencia: id })
        }
        Causality::ConcurrentComExclusao { base_rev } => {
            // Registra as duas versões e para. O agregado **continua
            // excluído** até o humano decidir: manter a exclusão ou restaurar
            // a versão editada. Restaurar sozinho seria ressurreição.
            registrar_revisao(tx, envelope)?;
            marcar_aplicado(tx, &envelope.event_id)?;
            let id = registrar_divergencia(tx, envelope, &base_rev, &historia)?;
            Ok(Applied::DivergenteComExclusao { id_divergencia: id })
        }
        Causality::Unknown { .. } => {
            // NÃO marca como aplicado: quando a história intermediária
            // chegar, este evento precisa ser reavaliado. O envelope fica
            // guardado, então nada se perde.
            Ok(Applied::PrecisaReconciliar)
        }
    }
}

/// Por que a exclusão remota não pode rodar agora, ou `None`.
///
/// ```text
/// Bloqueado(motivo)        efeito sobre agregado ainda não coberto        → bloqueia
/// Excluido(filho) vivo     a origem não apagou este filho                  → bloqueia
/// Reescrito(sobrevivente)  com divergência aberta ou evento pendente aqui → bloqueia; sem isso,
///                          a reescrita dele chega da origem como o evento seguinte
/// ```
///
/// Um filho **sem linha própria** (a posição do item, B2.2) só conta como vivo se ainda tiver revisão
/// corrente: a linha que o sustenta é a do item, e a origem já emitiu a exclusão dele antes. Se a
/// posição tiver sido movida aqui, a revisão corrente é a daqui — e a exclusão do item é bloqueada,
/// porque o movimento concorrente é trabalho autoral.
pub fn motivo_para_bloquear_exclusao_remota(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    let agregado = AggregateRef::new(&envelope.aggregate_type, &envelope.aggregate_id);
    if !sync_codec::coberto(&agregado.aggregate_type) {
        // A aplicação falha fechada logo em seguida; não há o que simular.
        return Ok(None);
    }
    for impacto in sync_codec::impactos_da_exclusao(tx, &agregado)? {
        match impacto {
            Impacto::Bloqueado(motivo) => return Ok(Some(motivo)),
            Impacto::Excluido(filho) => {
                let vivo = !sync_codec::existencia_derivada(&filho.aggregate_type)
                    || sync_codec::revisao_corrente(tx, &filho)?.is_some();
                if vivo {
                    return Ok(Some(format!(
                        "{} {} ainda existe aqui e não foi apagado pelo outro aparelho",
                        filho.aggregate_type, filho.aggregate_id
                    )));
                }
            }
            // O sobrevivente muda com a exclusão; a reescrita dele vem da origem logo depois, como
            // evento próprio. O que não pode é ele ter decisão ou história pendente aqui: aí a
            // mudança atropelaria trabalho concorrente.
            Impacto::Reescrito(sobrevivente) => {
                if sync_codec::estado_concorrente_para_evento(tx, &sobrevivente, envelope)?
                    .is_some()
                {
                    return Ok(Some(format!(
                        "{} {} tem decisão ou alteração pendente aqui e seria alterado pela exclusão",
                        sobrevivente.aggregate_type, sobrevivente.aggregate_id
                    )));
                }
            }
        }
    }
    Ok(None)
}

/// **Invariante estrutural no fim da sessão:** para um agregado coberto, sem decisão aberta nem
/// evento pendente, revisão corrente presente ⇒ o domínio é o payload dela.
pub fn conferir_revisao_corrente(
    tx: &Transaction<'_>,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<()> {
    if !sync_codec::coberto(&agregado.aggregate_type)
        || sync_codec::estado_concorrente(tx, agregado)?.is_some()
    {
        return Ok(());
    }
    let Some(registrado) = sync_codec::payload_da_revisao_corrente(tx, agregado)? else {
        return Ok(());
    };
    let materializado = sync_codec::ler_canonico(tx, agregado)?.map(|estado| estado.payload);
    if materializado.as_deref() != Some(registrado.as_str()) {
        return Err(DatabaseCommandError::storage(format!(
            "{} {}: a revisão corrente não é o estado do banco. A sessão não é confirmada.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }
    Ok(())
}

/// **Invariante do apply:** depois de aplicar, o estado materializado É o evento.
///
/// ```text
/// upsert   ler_canonico(agregado).payload == envelope.payload
/// delete   ler_canonico(agregado) == None      (exceto sem linha própria: some com o item)
/// ```
///
/// Falhar aqui desfaz tudo (a sessão para): registrar a revisão com outro estado no domínio seria o
/// estado "revisão diz X, banco tem Y" que nenhuma sincronização seguinte consegue detectar.
fn conferir_materializacao(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    let agregado = AggregateRef::new(&envelope.aggregate_type, &envelope.aggregate_id);
    let materializado = sync_codec::ler_canonico(tx, &agregado)?.map(|estado| estado.payload);
    let coerente = match envelope.operation {
        Operation::Upsert => materializado.as_deref() == Some(envelope.payload.as_str()),
        Operation::Delete => {
            materializado.is_none() || sync_codec::existencia_derivada(&agregado.aggregate_type)
        }
    };
    if !coerente {
        return Err(DatabaseCommandError::storage(format!(
            "Depois de aplicar {} {}, o estado no banco não é o do evento. Nada foi confirmado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }
    Ok(())
}

/// Registra a exclusão bloqueada: a revisão entra na história (um evento posterior que parta dela
/// precisa ser reconhecido), o evento fica aplicado como decisão pendente, e o agregado continua.
fn bloquear_exclusao(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
    historia: &crate::domain::sync::AggregateHistory,
    _motivo: &str,
) -> DatabaseCommandResult<String> {
    registrar_revisao(tx, envelope)?;
    marcar_aplicado(tx, &envelope.event_id)?;
    let daqui = participante_local(envelope, historia);
    let de_la = ConflictParticipant::new(
        &envelope.aggregate_type,
        &envelope.aggregate_id,
        &envelope.new_rev,
        envelope.operation.as_str(),
    );
    let id = registrar_divergencia_entre(
        tx,
        envelope,
        &envelope.base_rev,
        historia,
        "parent_deletion_blocked",
        daqui,
        de_la,
    )?;
    tx.execute(
        "UPDATE sync_divergences SET kind = 'parent_deletion_blocked' WHERE id = ?1",
        [&id],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(id)
}

/// Registra o conflito de nome de tag, do mesmo jeito que a exclusão bloqueada: a revisão entra
/// na história (um evento posterior que parta dela precisa ser reconhecido), o evento fica
/// aplicado como decisão pendente, e **o domínio não é tocado**.
/// **A identidade que colidiu é guardada.** `aggregate_id` é a tag que chegou (T2), e
/// `related_aggregate_id` é a tag daqui que ocupava o nome (T1). Guardar só o nome não serviria:
/// renomear T1 depois apagaria o rastro de com quem T2 colidiu, e a etapa F precisa das duas
/// identidades para poder oferecer "são a mesma tag" — decisão que tem de juntar as marcações das
/// duas.
fn conflito_de_nome_de_tag(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
    historia: &crate::domain::sync::AggregateHistory,
    homonima: &str,
) -> DatabaseCommandResult<String> {
    registrar_revisao(tx, envelope)?;
    marcar_aplicado(tx, &envelope.event_id)?;

    // **Aqui os dois participantes são agregados DIFERENTES**, e é por isso que a chave não pode
    // sair de `(aggregateType, aggregateId, revisões)`: no PC a tag que chegou é T2 e a daqui é
    // T1; no Android é o contrário. Descrever os dois lados com a mesma estrutura, e ordenar, é o
    // que faz os dois chegarem à mesma chave.
    let daqui = ConflictParticipant::new(
        "content_tag",
        homonima,
        sync_codec::revisao_corrente(tx, &AggregateRef::new("content_tag", homonima))?
            .unwrap_or_default(),
        "upsert",
    );
    let de_la = ConflictParticipant::new(
        "content_tag",
        &envelope.aggregate_id,
        &envelope.new_rev,
        envelope.operation.as_str(),
    );
    let id = registrar_divergencia_entre(
        tx,
        envelope,
        &envelope.base_rev,
        historia,
        "tag_name_conflict",
        daqui,
        de_la,
    )?;
    tx.execute(
        "UPDATE sync_divergences
            SET kind = 'tag_name_conflict', related_aggregate_id = ?2
          WHERE id = ?1",
        [&id, &homonima.to_string()],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(id)
}

/// **A decisão de um grupo que não pôde entrar inteiro** (B2.2).
///
/// O receptor tentou o grupo num `SAVEPOINT`, um membro divergiu ou foi bloqueado, e tudo foi
/// desfeito. Aqui o grupo vira decisão:
///
/// ```text
/// cada membro   revisão entra na história e evento fica aplicado    o cursor anda; um evento futuro
///                                                                    que parta de um membro é reconhecido
/// o domínio     NÃO muda                                            nenhum membro entra sozinho
/// a decisão     UMA, ancorada no membro que não entrou, com o mutation_id
/// ```
///
/// Uma decisão por ação, e não uma por efeito: o escritor decide "o livro foi excluído no outro
/// aparelho", não cinco conflitos em posições e capítulos. A resolução aplica ou restaura o grupo
/// inteiro.
pub fn registrar_decisao_do_grupo(
    tx: &Transaction<'_>,
    membros: &[EventEnvelope],
    ancora: usize,
    resultado: &Applied,
) -> DatabaseCommandResult<String> {
    let envelope = &membros[ancora];
    // A história da âncora é lida ANTES de registrar as revisões: é contra ela que a divergência
    // descreve os dois lados.
    let historia = aggregate_history(
        tx,
        &AggregateRef::new(&envelope.aggregate_type, &envelope.aggregate_id),
    )?;
    for membro in membros {
        registrar_revisao(tx, membro)?;
        marcar_aplicado(tx, &membro.event_id)?;
    }
    let id = match resultado {
        Applied::ExclusaoDoPaiBloqueada { .. } => bloquear_exclusao(tx, envelope, &historia, "")?,
        Applied::ConflitoDeNomeDeTag { .. } => {
            let homonima =
                sync_codec::conhecimento::tag_homonima(tx, envelope)?.unwrap_or_default();
            conflito_de_nome_de_tag(tx, envelope, &historia, &homonima)?
        }
        Applied::Divergente { .. } | Applied::DivergenteComExclusao { .. } => {
            let base = match classify(&historia, &envelope.base_rev, &envelope.new_rev) {
                Causality::Concurrent { base_rev }
                | Causality::ConcurrentComExclusao { base_rev } => base_rev,
                _ => envelope.base_rev.clone(),
            };
            registrar_divergencia(tx, envelope, &base, &historia)?
        }
        outro => {
            return Err(DatabaseCommandError::storage(format!(
                "Grupo de mutação sem decisão a registrar ({outro:?}). Nada foi confirmado."
            )))
        }
    };
    tx.execute(
        "UPDATE sync_divergences SET mutation_id = ?1 WHERE id = ?2",
        [&envelope.grupo.mutation_id, &id],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(id)
}

/// Aplica um membro de um grupo cuja decisão foi "aceitar" — refazendo, AGORA, tudo o que o apply
/// normal cobraria: a revisão daqui ainda é a base do membro, a dependência existe, e a exclusão não
/// apagaria descendente vivo. Qualquer recusa derruba a resolução inteira.
pub fn aplicar_membro_decidido(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    let agregado = AggregateRef::new(&envelope.aggregate_type, &envelope.aggregate_id);
    let atual = sync_codec::revisao_corrente(tx, &agregado)?.unwrap_or_default();
    if atual != envelope.base_rev {
        return Err(DatabaseCommandError::conflict(format!(
            "{} {} foi alterado depois que a ação do outro aparelho chegou. Aceitar agora apagaria \
             essa alteração. Nada foi aplicado.",
            envelope.aggregate_type, envelope.aggregate_id
        )));
    }
    match envelope.operation {
        Operation::Delete => {
            if let Some(motivo) = motivo_para_bloquear_exclusao_remota(tx, envelope)? {
                return Err(DatabaseCommandError::conflict(format!(
                    "Não dá para aceitar a exclusão de {} {}: {motivo}. Resolva isso antes. Nada foi \
                     apagado.",
                    envelope.aggregate_type, envelope.aggregate_id
                )));
            }
        }
        Operation::Upsert => {
            if let Some(falta) = sync_codec::dependencias(tx, envelope)? {
                return Err(DatabaseCommandError::conflict(format!(
                    "Não dá para aceitar {} {} ainda: falta {falta}. Nada foi aplicado.",
                    envelope.aggregate_type, envelope.aggregate_id
                )));
            }
        }
    }
    aplicar_com_estado_causal(tx, envelope)?;
    conferir_materializacao(tx, envelope)
}

/// Domínio (pelo codec) e estado causal do agregado, juntos.
///
/// ```text
/// upsert   revisão corrente = new_rev; tombstone sai (restauração que viu a exclusão)
/// delete   tombstone com new_rev; revisão corrente sai
/// ```
fn aplicar_com_estado_causal(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    sync_codec::aplicar(tx, envelope)?;
    let tipo = &envelope.aggregate_type;
    let id = &envelope.aggregate_id;
    let resultado = match envelope.operation {
        Operation::Upsert => tx
            .execute(
                "INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(aggregate_type, aggregate_id)
                 DO UPDATE SET current_rev = excluded.current_rev",
                rusqlite::params![tipo, id, &envelope.new_rev],
            )
            .and_then(|_| {
                tx.execute(
                    "DELETE FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                    [tipo, id],
                )
            }),
        Operation::Delete => tx
            .execute(
                "INSERT INTO sync_tombstones
                    (aggregate_type, aggregate_id, deleted_rev, origin_device_id, origin_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(aggregate_type, aggregate_id)
                 DO UPDATE SET deleted_rev = excluded.deleted_rev,
                               origin_device_id = excluded.origin_device_id,
                               origin_seq = excluded.origin_seq",
                rusqlite::params![tipo, id, &envelope.new_rev, &envelope.device_id, envelope.seq],
            )
            .and_then(|_| {
                tx.execute(
                    "DELETE FROM sync_aggregate_state WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                    [tipo, id],
                )
            }),
    };
    resultado.map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Aplica no domínio a exclusão remota que ficou bloqueada, quando o escritor a aceita.
///
/// O evento já está no log, marcado como aplicado, e a revisão dele já está na história — o que
/// faltava era o `DELETE` físico e o tombstone. Nenhum evento novo nasce: a revisão da exclusão já
/// é a mesma em todos os aparelhos. O preflight da exclusão roda de novo AGORA: o que era verdade
/// no bloqueio não autoriza a cascata.
pub fn aplicar_exclusao_bloqueada(
    tx: &Transaction<'_>,
    event_id: &str,
) -> DatabaseCommandResult<()> {
    let envelope = tx
        .query_row(
            &format!(
                "SELECT {} FROM sync_events WHERE event_id = ?1",
                sync_repository::colunas_do_envelope("")
            ),
            [event_id],
            sync_repository::envelope_da_linha,
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let Some(envelope) = envelope else {
        return Err(DatabaseCommandError::storage(format!(
            "O evento {event_id} da exclusão bloqueada não está no log."
        )));
    };
    if envelope.operation != Operation::Delete {
        return Err(DatabaseCommandError::storage(format!(
            "O evento {event_id} não é uma exclusão; aceitar não pode apagar nada."
        )));
    }
    if let Some(motivo) = motivo_para_bloquear_exclusao_remota(tx, &envelope)? {
        return Err(DatabaseCommandError::conflict(format!(
            "Não dá para aceitar a exclusão de {} {}: {motivo}. Resolva isso antes. Nada foi apagado.",
            envelope.aggregate_type, envelope.aggregate_id
        )));
    }
    aplicar_com_estado_causal(tx, &envelope)?;
    conferir_materializacao(tx, &envelope)
}

fn guardar_envelope(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    sync_repository::gravar_envelope(tx, envelope, true)
}

fn marcar_aplicado(tx: &Transaction<'_>, event_id: &str) -> DatabaseCommandResult<()> {
    tx.execute(
        "INSERT OR IGNORE INTO sync_applied_events (event_id) VALUES (?1)",
        [event_id],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

fn registrar_revisao(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    tx.execute(
        "INSERT OR IGNORE INTO sync_revision_history
            (aggregate_type, aggregate_id, rev, base_rev, event_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            &envelope.aggregate_type,
            &envelope.aggregate_id,
            &envelope.new_rev,
            &envelope.base_rev,
            &envelope.event_id,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Guarda as duas versões e a base comum, mais **o que cada lado fez**.
///
/// A operação de cada lado não é enfeite. Sem ela, um lado local excluído chega
/// à tela como `local_rev = ""`, e "aqui foi apagado" fica indistinguível de
/// "não sabemos o que tem aqui" — que são escolhas opostas para o escritor.
/// Com as duas operações registradas, a caixa de conciliação sabe qual par de
/// botões oferecer:
///
/// ```text
/// delete + upsert  ->  [manter a exclusão]  ou  [restaurar a edição]
/// upsert + upsert  ->  [ficar com esta]     ou  [ficar com aquela]
/// upsert + delete  ->  [manter o conteúdo]  ou  [aceitar a exclusão]
/// ```
///
/// Quando o lado local é uma exclusão, `local_rev` passa a ser a revisão **da
/// própria exclusão** ([`AggregateHistory::deleted_rev`]), e não uma string
/// vazia: a exclusão é uma revisão causal, não a ausência de uma.
fn registrar_divergencia(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
    base_rev: &str,
    historia: &crate::domain::sync::AggregateHistory,
) -> DatabaseCommandResult<String> {
    // O outro participante é o próprio agregado, do lado de cá. Conflito entre agregados
    // DIFERENTES (tag homônima) informa o participante explicitamente — ver `com_participantes`.
    let daqui = participante_local(envelope, historia);
    let de_la = ConflictParticipant::new(
        &envelope.aggregate_type,
        &envelope.aggregate_id,
        &envelope.new_rev,
        envelope.operation.as_str(),
    );
    registrar_divergencia_entre(tx, envelope, base_rev, historia, "concurrent", daqui, de_la)
}

/// O lado de cá, descrito sem perspectiva: qual revisão está materializada aqui e o que foi feito.
fn participante_local(
    envelope: &EventEnvelope,
    historia: &crate::domain::sync::AggregateHistory,
) -> ConflictParticipant {
    let (rev, operacao) = match (&historia.current_rev, &historia.deleted_rev) {
        (Some(rev), _) => (rev.as_str(), "upsert"),
        (None, Some(rev)) => (rev.as_str(), "delete"),
        (None, None) => ("", ""),
    };
    ConflictParticipant::new(
        &envelope.aggregate_type,
        &envelope.aggregate_id,
        rev,
        operacao,
    )
}

/// Grava a divergência com as **duas** identidades: a portátil e a perspectiva local.
///
/// ```text
/// conflict_key, participant_a, participant_b   iguais nos dois aparelhos
/// local_rev, remote_rev                        perspectiva de quem detectou
/// ```
///
/// A perspectiva continua porque o resolvedor precisa dela operacionalmente: o de
/// `parent_deletion_blocked` confere se a revisão materializada aqui ainda é a que estava quando o
/// conflito foi detectado, antes de deixar a cascata rodar. O que a etapa F para de fazer é
/// **mostrar** "local/remote" ao escritor — não é deixar de saber quem é quem aqui dentro.
fn registrar_divergencia_entre(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
    base_rev: &str,
    historia: &crate::domain::sync::AggregateHistory,
    tipo_de_conflito: &str,
    daqui: ConflictParticipant,
    de_la: ConflictParticipant,
) -> DatabaseCommandResult<String> {
    let id = crate::domain::ids::new_id();
    let (local_rev, local_operation) = match (&historia.current_rev, &historia.deleted_rev) {
        (Some(rev), _) => (rev.as_str(), "upsert"),
        (None, Some(rev)) => (rev.as_str(), "delete"),
        (None, None) => ("", ""),
    };
    let (primeiro, segundo) = crate::domain::conflito::ordenar(&daqui, &de_la);
    let chave = crate::domain::conflito::chave_do_conflito(tipo_de_conflito, &daqui, &de_la);
    tx.execute(
        "INSERT INTO sync_divergences
            (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
             local_operation, remote_operation, remote_event_id,
             conflict_key, participant_a, participant_b)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            &id,
            &envelope.aggregate_type,
            &envelope.aggregate_id,
            base_rev,
            local_rev,
            &envelope.new_rev,
            local_operation,
            envelope.operation.as_str(),
            &envelope.event_id,
            &chave,
            &primeiro,
            &segundo,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(id)
}

/// Monta o envelope que a origem produziria — usado para construir eventos de
/// teste sem duplicar a regra de cálculo da revisão.
pub fn envelope_de_origem(
    device_id: &str,
    seq: i64,
    universe_id: &str,
    aggregate: &AggregateRef,
    operation: Operation,
    payload: &str,
    base_rev: &str,
) -> EventEnvelope {
    EventEnvelope {
        event_id: crate::domain::ids::new_id(),
        device_id: device_id.to_string(),
        seq,
        universe_id: universe_id.to_string(),
        aggregate_type: aggregate.aggregate_type.clone(),
        aggregate_id: aggregate.aggregate_id.clone(),
        operation,
        payload: payload.to_string(),
        base_rev: base_rev.to_string(),
        new_rev: compute_revision(base_rev, aggregate, operation, payload),
        signature: String::new(),
        grupo: crate::domain::sync::GrupoDeMutacao::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseCommandResult;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};
    use rusqlite::{Connection, TransactionBehavior};

    const ORIGEM: &str = "dev-remoto";

    fn preparar(fixture: &TemporaryDatabase) -> Connection {
        let connection = fixture.database.write().expect("abrir escrita");
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                 INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');
                 INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                   VALUES ('dev-remoto', 'Remoto', 'CHAVE', 0);",
            )
            .expect("semear");
        connection
    }

    fn capitulo(titulo: &str, conteudo: &str) -> String {
        format!(
            r#"{{"id":"cap-1","bookId":"b1","title":"{titulo}","content":"{conteudo}",
                 "summary":"","sceneOrigin":"","sceneDestination":"",
                 "status":"rascunho","canonStatus":"canon","customFields":[]}}"#
        )
        .replace('\n', "")
        .replace("                 ", "")
    }

    fn agregado() -> AggregateRef {
        AggregateRef::new("chapter", "cap-1")
    }

    fn aplicar(connection: &mut Connection, envelope: &EventEnvelope) -> Applied {
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("abrir transação");
        let resultado = apply_remote_event(&tx, envelope).expect("aplicar");
        tx.commit().expect("commit");
        resultado
    }

    /// GATE DA ETAPA 4: os efeitos da seção 12 acontecem juntos.
    #[test]
    fn evento_sequencial_aplica_e_deixa_os_tres_efeitos() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let payload = capitulo("Capitulo Um", "Ela entrou na cidade.");
        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &payload,
            "",
        );

        assert_eq!(aplicar(&mut connection, &envelope), Applied::Aplicado);

        // 1 — o envelope ficou guardado, com a assinatura da origem intacta.
        let (guardado, assinatura): (i64, String) = connection
            .query_row(
                "SELECT COUNT(*), COALESCE(MAX(signature), '') FROM sync_events WHERE event_id = ?1",
                [&envelope.event_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("consultar log");
        assert_eq!(
            guardado, 1,
            "sem o envelope o relay não tem o que retransmitir"
        );
        assert_eq!(assinatura, envelope.signature);

        // 2 — o agregado mudou.
        let titulo: String = connection
            .query_row("SELECT title FROM chapters WHERE id = 'cap-1'", [], |row| {
                row.get(0)
            })
            .expect("ler capítulo");
        assert_eq!(titulo, "Capitulo Um");

        // 3 — a idempotência foi registrada.
        let aplicados: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_applied_events WHERE event_id = ?1",
                [&envelope.event_id],
                |row| row.get(0),
            )
            .expect("consultar aplicados");
        assert_eq!(aplicados, 1, "sem isto o evento reaplica na reconexão");

        // E a revisão corrente do agregado passou a ser a do evento.
        let corrente: String = connection
            .query_row(
                "SELECT current_rev FROM sync_aggregate_state
                  WHERE aggregate_type = 'chapter' AND aggregate_id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .expect("ler revisão corrente");
        assert_eq!(corrente, envelope.new_rev);
    }

    /// Repetição é normal em rede, e não pode ser erro nem reaplicação.
    #[test]
    fn evento_repetido_nao_reaplica() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Primeiro", "texto"),
            "",
        );
        assert_eq!(aplicar(&mut connection, &envelope), Applied::Aplicado);

        // Alguém edita localmente depois de aplicar.
        connection
            .execute(
                "UPDATE chapters SET title = 'Editado depois' WHERE id = 'cap-1'",
                [],
            )
            .expect("editar");

        assert_eq!(aplicar(&mut connection, &envelope), Applied::JaAplicado);

        let titulo: String = connection
            .query_row("SELECT title FROM chapters WHERE id = 'cap-1'", [], |row| {
                row.get(0)
            })
            .expect("ler");
        assert_eq!(
            titulo, "Editado depois",
            "reaplicar o mesmo evento sobrescreveu uma edição posterior"
        );
    }

    /// O caso que o `updated_at` não distingue: duas edições da mesma base.
    ///
    /// **Nada é sobrescrito.** As duas revisões ficam registradas e a
    /// divergência é anotada para o humano decidir (ADR 0009 §16).
    #[test]
    fn evento_concorrente_registra_divergencia_sem_sobrescrever() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        // Base comum.
        let base = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Base", "Ela entrou na cidade."),
            "",
        );
        assert_eq!(aplicar(&mut connection, &base), Applied::Aplicado);

        // O aparelho local escreve por cima da base.
        let local = envelope_de_origem(
            ORIGEM,
            2,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Versao local", "Ela entrou silenciosamente."),
            &base.new_rev,
        );
        assert_eq!(aplicar(&mut connection, &local), Applied::Aplicado);

        // E chega um evento que partiu da MESMA base, de outro aparelho.
        let concorrente = envelope_de_origem(
            "dev-terceiro",
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Versao remota", "Ela entrou pela ponte sul."),
            &base.new_rev,
        );
        connection
            .execute(
                "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                 VALUES ('dev-terceiro', 'Terceiro', 'CHAVE', 0)",
                [],
            )
            .expect("registrar terceiro");

        let resultado = aplicar(&mut connection, &concorrente);
        assert!(
            matches!(resultado, Applied::Divergente { .. }),
            "esperava divergência, veio {resultado:?}"
        );

        // O conteúdo local NÃO foi sobrescrito.
        let titulo: String = connection
            .query_row("SELECT title FROM chapters WHERE id = 'cap-1'", [], |row| {
                row.get(0)
            })
            .expect("ler");
        assert_eq!(
            titulo, "Versao local",
            "a versão remota sobrescreveu a local"
        );

        // As duas revisões estão preservadas, com a base comum registrada.
        let (base_reg, local_reg, remota_reg): (String, String, String) = connection
            .query_row(
                "SELECT base_rev, local_rev, remote_rev FROM sync_divergences",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("ler divergência");
        assert_eq!(base_reg, base.new_rev);
        assert_eq!(local_reg, local.new_rev);
        assert_eq!(remota_reg, concorrente.new_rev);

        // E o envelope da versão remota ficou guardado: é dele que a
        // resolução vai ler o outro texto.
        let payload_remoto: String = connection
            .query_row(
                "SELECT payload FROM sync_events WHERE event_id = ?1",
                [&concorrente.event_id],
                |row| row.get(0),
            )
            .expect("ler payload remoto");
        assert!(payload_remoto.contains("ponte sul"));
    }

    /// Base desconhecida **não** é conflito, e não pode ser marcada como
    /// aplicada: quando a história intermediária chegar, o evento precisa ser
    /// reavaliado.
    #[test]
    fn base_desconhecida_fica_pendente_de_reconciliacao() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let orfao = envelope_de_origem(
            ORIGEM,
            5,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Orfao", "texto"),
            "revisao-que-nunca-vimos",
        );
        assert_eq!(
            aplicar(&mut connection, &orfao),
            Applied::PrecisaReconciliar
        );

        let existe: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM chapters WHERE id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .expect("contar capítulos");
        assert_eq!(existe, 0, "aplicou um evento cuja história não conhecemos");

        let aplicados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_applied_events", [], |row| {
                row.get(0)
            })
            .expect("contar aplicados");
        assert_eq!(
            aplicados, 0,
            "marcar como aplicado impediria a reavaliação quando a história chegar"
        );

        // Mas o envelope ficou guardado: nada se perde.
        let guardados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar log");
        assert_eq!(guardados, 1);
    }

    /// A idempotência por `event_id` não é redundante — ela é a que sobrevive
    /// à poda da história de revisões.
    ///
    /// A mutação revelou isto: removendo a checagem de `event_id`, todos os
    /// testes continuavam verdes, porque `classify` devolvia `AlreadyPresent`
    /// ao reconhecer a `new_rev` em `sync_revision_history`. Só que o ADR 0009
    /// §11 diz que essa tabela guarda **as últimas** revisões por agregado —
    /// ela pode ser podada. Depois da poda, quem impede a reaplicação é o
    /// registro de `event_id`, e mais nada.
    ///
    /// Sem ele, um evento antigo reaparecendo numa reconexão sobrescreveria
    /// edições posteriores.
    #[test]
    fn a_idempotencia_sobrevive_a_poda_da_historia_de_revisoes() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Original", "texto"),
            "",
        );
        assert_eq!(aplicar(&mut connection, &envelope), Applied::Aplicado);

        // O escritor edita depois, e a história antiga é podada.
        connection
            .execute(
                "UPDATE chapters SET title = 'Editado depois' WHERE id = 'cap-1'",
                [],
            )
            .expect("editar");
        connection
            .execute("DELETE FROM sync_revision_history", [])
            .expect("podar a história");
        connection
            .execute("DELETE FROM sync_aggregate_state", [])
            .expect("podar o estado");

        // O mesmo evento reaparece numa reconexão.
        assert_eq!(aplicar(&mut connection, &envelope), Applied::JaAplicado);

        let titulo: String = connection
            .query_row("SELECT title FROM chapters WHERE id = 'cap-1'", [], |row| {
                row.get(0)
            })
            .expect("ler");
        assert_eq!(
            titulo, "Editado depois",
            "com a história podada, só o registro de event_id impede a reaplicação"
        );
    }

    /// Dois aparelhos que fazem a MESMA edição a partir da mesma base
    /// convergem sem conflito.
    ///
    /// A revisão é `H(base ‖ agregado ‖ operação ‖ payload)`, então conteúdo
    /// igual produz `new_rev` igual mesmo vindo de origens diferentes. Tratar
    /// isso como divergência incomodaria o escritor com uma escolha entre dois
    /// textos idênticos.
    #[test]
    fn a_mesma_edicao_feita_em_dois_aparelhos_converge_sem_divergencia() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute(
                "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                 VALUES ('dev-terceiro', 'Terceiro', 'CHAVE', 0)",
                [],
            )
            .expect("registrar terceiro");

        let payload = capitulo("Mesmo titulo", "Mesmo texto.");
        let de_um = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &payload,
            "",
        );
        let do_outro = envelope_de_origem(
            "dev-terceiro",
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &payload,
            "",
        );
        assert_eq!(
            de_um.new_rev, do_outro.new_rev,
            "conteúdo igual a partir da mesma base precisa produzir a mesma revisão"
        );

        assert_eq!(aplicar(&mut connection, &de_um), Applied::Aplicado);
        assert_eq!(aplicar(&mut connection, &do_outro), Applied::JaAplicado);

        let divergencias: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_divergences", [], |row| {
                row.get(0)
            })
            .expect("contar divergências");
        assert_eq!(
            divergencias, 0,
            "dois textos idênticos não podem virar uma pergunta para o escritor"
        );
    }

    #[test]
    fn delete_remoto_apaga_e_deixa_tombstone() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let criacao = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &capitulo("Vai sumir", "texto"),
            "",
        );
        aplicar(&mut connection, &criacao);
        let exclusao = envelope_de_origem(
            ORIGEM,
            2,
            "u1",
            &agregado(),
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        assert_eq!(aplicar(&mut connection, &exclusao), Applied::Aplicado);

        let existe: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM chapters WHERE id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(existe, 0);

        let tombstone: String = connection
            .query_row(
                "SELECT deleted_rev FROM sync_tombstones
                  WHERE aggregate_type = 'chapter' AND aggregate_id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .expect("ler tombstone");
        assert_eq!(tombstone, exclusao.new_rev);
    }

    /// Agregado sem aplicação implementada **para a sessão**.
    ///
    /// Ignorar seria o pior estado possível: o evento constaria como
    /// aplicado, o cursor avançaria, e o dado nunca chegaria — sem nada
    /// registrando a falta.
    #[test]
    fn agregado_desconhecido_para_a_sessao_em_vez_de_sumir() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            // Tipo inventado de propósito: o teste é sobre parar diante do desconhecido, não sobre
            // uma etapa pendente. Apontar para um tipo real o faz quebrar a cada etapa nova.
            &AggregateRef::new("agregado_que_nunca_vai_existir", "x1"),
            Operation::Upsert,
            r#"{"id":"x1"}"#,
            "",
        );

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transação");
        let erro = apply_remote_event(&tx, &envelope)
            .expect_err("agregado sem aplicação precisa parar a sessão");
        assert!(
            erro.to_string().contains("ainda não tem aplicação"),
            "parou pelo motivo errado: {erro}"
        );
        drop(tx);

        let aplicados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_applied_events", [], |row| {
                row.get(0)
            })
            .expect("contar");
        assert_eq!(aplicados, 0);
    }

    /// Payload que descreve outro agregado é recusado.
    ///
    /// O envelope diz `cap-1` e o JSON diz `cap-9`: aplicar escreveria num
    /// capítulo que ninguém pediu, e a revisão ficaria registrada no agregado
    /// errado.
    #[test]
    fn payload_de_outro_agregado_e_recusado() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let payload = capitulo("Impostor", "texto").replace("cap-1", "cap-9");
        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado(),
            Operation::Upsert,
            &payload,
            "",
        );

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transação");
        let erro = apply_remote_event(&tx, &envelope).expect_err("payload de outro agregado");
        assert!(
            erro.to_string().contains("e o envelope é de cap-1"),
            "recusou pelo motivo errado: {erro}"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Anexo recebido (ADR 0010, fatia 7)
    // ═══════════════════════════════════════════════════════════════════════

    /// O payload canônico definitivo da B5: camelCase, sem relógio, sem posição física, e **sem
    /// campo nenhum onde caibam bytes**.
    fn anexo(hash: &str) -> String {
        format!(
            r#"{{"id":"anexo-1","universeId":"u1","ownerType":"chapter",
                 "ownerId":"cap-1","blobHash":"{hash}","mimeType":"image/png","caption":""}}"#
        )
        .replace('\n', "")
        .replace("                 ", "")
    }

    /// O payload da B1, que um peer de versão antiga ainda produziria.
    fn anexo_da_b1(hash: &str, data_url: &str) -> String {
        format!(
            r#"{{"id":"anexo-1","universe_id":"u1","owner_type":"chapter",
                 "owner_id":"cap-1","data_url":"{data_url}","blob_hash":"{hash}",
                 "mime_type":"image/png","caption":"","sort_order":0,
                 "created_at":"2026-01-01 00:00:00"}}"#
        )
        .replace('\n', "")
        .replace("                 ", "")
    }

    /// **Evento de anexo com bytes dentro é recusado.**
    ///
    /// O que isto fecha não é acidente: é um peer com versão antiga, um banco
    /// importado, ou uma linha adulterada. Aplicar gravaria a base64 de volta
    /// no acervo — e o evento é assinado e append-only, então ele continuaria
    /// chegando em cada sessão.
    ///
    /// A sessão para aqui de propósito. Marcar como aplicado sem gravar
    /// esconderia o problema; gravar traria os bytes de volta.
    ///
    /// **A B5 endureceu isto de um jeito que vale dizer:** o payload canônico não tem mais campo
    /// `dataUrl`. Não existe mais "anexo com bytes onde os bytes são ignorados" — o formato é
    /// recusado na desserialização, antes de qualquer decisão sobre o conteúdo.
    #[test]
    fn evento_de_anexo_com_bytes_e_recusado() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute_batch(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('cap-1','b1','Cap','', 0);",
            )
            .expect("semear capítulo");

        let hash = crate::infrastructure::blob_store::hash_dos_bytes(b"imagem");
        let agregado = AggregateRef::new("attachment", "anexo-1");
        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado,
            Operation::Upsert,
            &anexo_da_b1(&hash, "data:image/png;base64,aW1hZ2Vt"),
            "",
        );

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transação");
        let erro = apply_remote_event(&tx, &envelope).expect_err("o evento traz bytes");
        assert!(
            erro.message.contains("data_url") || erro.message.contains("Anexo ilegível"),
            "recusou pelo motivo errado: {}",
            erro.message
        );
        drop(tx);

        let quantos: i64 = connection
            .query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(quantos, 0, "nada podia ter sido gravado");
    }

    /// Referência que não é hash canônico também é recusada.
    #[test]
    fn evento_de_anexo_com_referencia_torta_e_recusado() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute_batch(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('cap-1','b1','Cap','', 0);",
            )
            .expect("semear capítulo");

        let agregado = AggregateRef::new("attachment", "anexo-1");
        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado,
            Operation::Upsert,
            &anexo("../../../etc/passwd"),
            "",
        );

        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transação");
        let erro = apply_remote_event(&tx, &envelope).expect_err("referência torta");
        assert!(erro.message.contains("canônico"), "{}", erro.message);
    }

    /// **E o anexo bem formado aplica.**
    ///
    /// Senão os dois gates acima passariam por vácuo: uma aplicação que recusa
    /// tudo também recusa bytes.
    ///
    /// `apply_remote_event` não consulta o disco — a presença do blob é
    /// conferida antes, na drenagem (`sync_session`, etapa D item 12), que é o
    /// único caminho que chama esta função fora de teste. Aqui o que se prova é
    /// só a forma: referência canônica entra, coluna legada fica vazia.
    #[test]
    fn anexo_bem_formado_aplica() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute_batch(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('cap-1','b1','Cap','', 0);",
            )
            .expect("semear capítulo");

        let hash = crate::infrastructure::blob_store::hash_dos_bytes(b"a-que-vem-depois");
        let agregado = AggregateRef::new("attachment", "anexo-1");
        let envelope = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado,
            Operation::Upsert,
            &anexo(&hash),
            "",
        );

        let resultado = aplicar(&mut connection, &envelope);
        assert_eq!(resultado, Applied::Aplicado, "o evento tinha que aplicar");

        let (gravado, inline): (String, String) = connection
            .query_row(
                "SELECT blob_hash, data_url FROM attachments WHERE id = 'anexo-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler o anexo");
        assert_eq!(gravado, hash, "a referência tinha que ficar");
        assert_eq!(inline, "", "e a coluna legada, vazia");

        // E o evento foi marcado como aplicado.
        let aplicados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_applied_events", [], |row| {
                row.get(0)
            })
            .expect("contar");
        assert_eq!(aplicados, 1);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // B2: dependências, pai imutável — e posição por item (B2.2)
    // ═══════════════════════════════════════════════════════════════════════

    /// A posição de um capítulo, no formato canônico da B2.2.
    fn posicao_env(seq: i64, capitulo: &str, livro: &str, sort_order: i64) -> EventEnvelope {
        envelope_de_origem(
            ORIGEM,
            seq,
            "u1",
            &AggregateRef::new("chapter_position", capitulo),
            Operation::Upsert,
            &format!(r#"{{"chapterId":"{capitulo}","bookId":"{livro}","sortOrder":{sort_order}}}"#),
            "",
        )
    }

    fn sort_order(connection: &Connection, capitulo: &str) -> i64 {
        connection
            .query_row(
                "SELECT sort_order FROM chapters WHERE id = ?1",
                [capitulo],
                |row| row.get(0),
            )
            .expect("sort_order")
    }

    fn semear_capitulos(connection: &Connection, livro: &str, ids: &[&str]) {
        for (i, id) in ids.iter().enumerate() {
            connection
                .execute(
                    "INSERT INTO chapters (id, book_id, title, sort_order) VALUES (?1, ?2, ?1, ?3)",
                    rusqlite::params![id, livro, i as i64],
                )
                .expect("capítulo");
        }
    }

    fn marcado(connection: &Connection, envelope: &EventEnvelope) -> bool {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_applied_events WHERE event_id = ?1)",
                [&envelope.event_id],
                |row| row.get(0),
            )
            .expect("marcado")
    }

    fn aplicar_tentando(
        connection: &mut Connection,
        envelope: &EventEnvelope,
    ) -> DatabaseCommandResult<Applied> {
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transação");
        let resultado = apply_remote_event(&tx, envelope)?;
        tx.commit().expect("commit");
        Ok(resultado)
    }

    fn materializado(connection: &Connection, tipo: &str, id: &str) -> Option<String> {
        sync_codec::ler_canonico(connection, &AggregateRef::new(tipo, id))
            .expect("ler")
            .map(|estado| estado.payload)
    }

    /// A posição aplicada é exatamente o payload — e só o item citado muda.
    #[test]
    fn posicao_aplicada_e_exatamente_o_payload_e_nao_mexe_nos_irmaos() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        semear_capitulos(&connection, "b1", &["c1", "c2", "c3"]);

        let envelope = posicao_env(1, "c3", "b1", -1);
        assert_eq!(aplicar(&mut connection, &envelope), Applied::Aplicado);
        assert_eq!(
            materializado(&connection, "chapter_position", "c3").as_deref(),
            Some(envelope.payload.as_str())
        );
        assert_eq!(
            (sort_order(&connection, "c1"), sort_order(&connection, "c2")),
            (0, 1),
            "a posição de um capítulo mexeu nos irmãos"
        );
    }

    /// Empate de posição é estado válido: dois itens com o mesmo número.
    #[test]
    fn posicao_empatada_com_um_irmao_e_aplicada() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        semear_capitulos(&connection, "b1", &["c1", "c2"]);

        let envelope = posicao_env(1, "c2", "b1", 0);
        assert_eq!(aplicar(&mut connection, &envelope), Applied::Aplicado);
        assert_eq!(sort_order(&connection, "c1"), sort_order(&connection, "c2"));
    }

    #[test]
    fn posicao_de_capitulo_que_nao_chegou_espera_sem_marcar() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        semear_capitulos(&connection, "b1", &["c1"]);

        let envelope = posicao_env(1, "c-ainda-nao-chegou", "b1", 0);
        assert_eq!(
            aplicar(&mut connection, &envelope),
            Applied::PrecisaReconciliar
        );
        assert!(
            !marcado(&connection, &envelope),
            "posição de item ausente marcada como aplicada"
        );
        assert_eq!(sort_order(&connection, "c1"), 0);
    }

    /// A posição não muda o item de pai: pai diferente é erro, nada marcado.
    #[test]
    fn posicao_com_outro_pai_e_recusada_e_nao_marca() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute(
                "INSERT INTO books (id, story_id, name) VALUES ('b2', 's1', 'Outro')",
                [],
            )
            .expect("livro 2");
        semear_capitulos(&connection, "b1", &["c1"]);

        let envelope = posicao_env(1, "c1", "b2", 0);
        let erro = aplicar_tentando(&mut connection, &envelope).expect_err("outro pai");
        assert!(erro.message.contains("imutável"), "{}", erro.message);
        assert!(!marcado(&connection, &envelope));
    }

    /// Pai imutável: capítulo, livro e história que já existem aqui com outro pai.
    #[test]
    fn evento_que_troca_o_pai_e_recusado_e_nao_marca() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name) VALUES ('s2', 'u1', 'Outra');
                 INSERT INTO books (id, story_id, name) VALUES ('b2', 's1', 'Livro 2');",
            )
            .expect("semear");
        crate::infrastructure::sqlite::test_support::seed_universe(&connection, "u2");
        semear_capitulos(&connection, "b1", &["cap-1"]);

        let casos = [
            (
                AggregateRef::new("chapter", "cap-1"),
                capitulo("Movido", "texto").replace("\"bookId\":\"b1\"", "\"bookId\":\"b2\""),
                "book_id",
            ),
            (
                AggregateRef::new("book", "b1"),
                r#"{"id":"b1","storyId":"s2","name":"Livro","description":"","coverBlobHash":"","coverMimeType":"","customFields":[]}"#.to_string(),
                "story_id",
            ),
            (
                AggregateRef::new("story", "s1"),
                r#"{"id":"s1","universeId":"u2","name":"Historia","description":"","customFields":[]}"#.to_string(),
                "universe_id",
            ),
        ];
        for (agregado, payload, coluna) in casos {
            let envelope =
                envelope_de_origem(ORIGEM, 1, "u1", &agregado, Operation::Upsert, &payload, "");
            let erro = aplicar_tentando(&mut connection, &envelope).expect_err("pai trocado");
            assert!(
                erro.message.contains("imutável") && erro.message.contains(coluna),
                "{}",
                erro.message
            );
            assert!(!marcado(&connection, &envelope));
        }
    }

    /// Após todo `Aplicado`, o estado materializado do agregado é o payload do evento — para cada
    /// tipo coberto pela B2.
    #[test]
    fn todo_aplicado_da_b2_materializa_exatamente_o_payload() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        let eventos = [
            ("universe", "u9", r#"{"id":"u9","name":"Novo","description":"d","coverBlobHash":"","coverMimeType":"","customFields":[{"key":"Tom","value":"frio"}]}"#.to_string()),
            ("story", "s9", r#"{"id":"s9","universeId":"u9","name":"S","description":"","customFields":[]}"#.to_string()),
            ("book", "b9", r#"{"id":"b9","storyId":"s9","name":"L","description":"","coverBlobHash":"","coverMimeType":"","customFields":[{"key":"Série","value":"I"}]}"#.to_string()),
            ("chapter", "c9", r#"{"id":"c9","bookId":"b9","title":"T","content":"<p>um dois</p>","summary":"","sceneOrigin":"","sceneDestination":"","status":"IDEIA","canonStatus":"CANON","customFields":[]}"#.to_string()),
            ("chapter_position", "c9", r#"{"chapterId":"c9","bookId":"b9","sortOrder":4}"#.to_string()),
        ];
        for (seq, (tipo, id, payload)) in eventos.iter().enumerate() {
            let envelope = envelope_de_origem(
                ORIGEM,
                seq as i64 + 1,
                "u9",
                &AggregateRef::new(*tipo, *id),
                Operation::Upsert,
                payload,
                "",
            );
            assert_eq!(
                aplicar(&mut connection, &envelope),
                Applied::Aplicado,
                "{tipo}"
            );
            assert_eq!(
                materializado(&connection, tipo, id).as_deref(),
                Some(payload.as_str()),
                "{tipo} {id}: materializado difere do payload"
            );
        }
    }

    /// História e livro seguem o mesmo contrato de posição do capítulo (B2.2).
    #[test]
    fn posicao_de_historia_e_de_livro_tem_o_mesmo_contrato() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        crate::infrastructure::sqlite::test_support::seed_universe(&connection, "u2");
        let evento = |seq: i64, tipo: &str, id: &str, payload: &str, base: &str| {
            envelope_de_origem(
                ORIGEM,
                seq,
                "u1",
                &AggregateRef::new(tipo, id),
                Operation::Upsert,
                payload,
                base,
            )
        };

        // Exato.
        let historia = evento(
            1,
            "story_position",
            "s1",
            r#"{"storyId":"s1","universeId":"u1","sortOrder":9}"#,
            "",
        );
        assert_eq!(aplicar(&mut connection, &historia), Applied::Aplicado);
        assert_eq!(
            materializado(&connection, "story_position", "s1").as_deref(),
            Some(historia.payload.as_str())
        );
        let livro = evento(
            2,
            "book_position",
            "b1",
            r#"{"bookId":"b1","storyId":"s1","sortOrder":5}"#,
            "",
        );
        assert_eq!(aplicar(&mut connection, &livro), Applied::Aplicado);
        assert_eq!(
            materializado(&connection, "book_position", "b1").as_deref(),
            Some(livro.payload.as_str())
        );

        // Outro pai: erro, nada marcado. A partir da revisão corrente, para o evento ser
        // SEQUENCIAL e chegar à validação — com base vazia ele seria concorrente, viraria
        // divergência, e a regra do pai nem seria consultada.
        for (tipo, id, payload) in [
            (
                "story_position",
                "s1",
                r#"{"storyId":"s1","universeId":"u2","sortOrder":0}"#,
            ),
            (
                "book_position",
                "b1",
                r#"{"bookId":"b1","storyId":"s-outra","sortOrder":0}"#,
            ),
        ] {
            let base = if tipo == "story_position" {
                &historia.new_rev
            } else {
                &livro.new_rev
            };
            let envelope = evento(3, tipo, id, payload, base);
            let erro = aplicar_tentando(&mut connection, &envelope).expect_err(tipo);
            assert!(erro.message.contains("imutável"), "{}", erro.message);
            assert!(!marcado(&connection, &envelope));
        }

        // Item que não chegou: espera, nada marcado.
        for (tipo, id, payload) in [
            (
                "story_position",
                "s-nao-chegou",
                r#"{"storyId":"s-nao-chegou","universeId":"u1","sortOrder":0}"#,
            ),
            (
                "book_position",
                "b-nao-chegou",
                r#"{"bookId":"b-nao-chegou","storyId":"s1","sortOrder":0}"#,
            ),
        ] {
            let envelope = evento(3, tipo, id, payload, "");
            assert_eq!(
                aplicar(&mut connection, &envelope),
                Applied::PrecisaReconciliar,
                "{payload}"
            );
            assert!(!marcado(&connection, &envelope));
        }
    }
}
