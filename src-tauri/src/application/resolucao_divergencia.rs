//! **Resolução de divergências do Sync V2** (etapa F).
//!
//! Uma resolução é uma `Mutacao` normal — não existe caminho paralelo. Ela declara o conflito,
//! escreve no domínio o estado escolhido e declara os efeitos com a base escolhida; a `Mutacao`
//! monta o certificado (`conflict_resolution/<conflictKey>`) a partir da lista exata do que emitiu,
//! e o grupo inteiro viaja como uma ação atômica. Ver `sync_codec::resolucao` para o contrato do
//! certificado e `sync_apply::apply_efeito_de_resolucao` para como os outros aparelhos o recebem.
//!
//! ```text
//! kind / operações             ações                       efeito
//! concurrent upsert×upsert     FicarComA | FicarComB       upsert do escolhido, base = ele, outra = o outro
//! concurrent upsert×delete     Restaurar | ManterExclusao  upsert ou delete (preflight de novo, AGORA)
//! concurrent delete×delete     automática                  delete canônico, base = participant_a
//! concurrent payloads iguais   automática                  upsert canônico, base = participant_a
//! concurrent de um grupo       FicarComA | FicarComB       a AÇÃO inteira, membro a membro
//! concurrent sobre uma decisão FicarComA | FicarComB       a decisão escolhida e os efeitos dela
//! parent_deletion_blocked      ManterLocal | AceitarExclusao  o contrato de sempre, agora certificado
//! tag_name_conflict            Renomear | Mesclar          semântica própria (ver abaixo)
//! ```
//!
//! `A` e `B` são os participantes na ordem canônica da chave — nunca "meu" e "dele": dois aparelhos
//! que escolhem a mesma versão escolhem a mesma letra, e produzem a mesma decisão.
//!
//! ## Tag homônima: renomear ou mesclar
//!
//! As duas tags são agregados diferentes; em cada aparelho uma está materializada e a outra não
//! (o `UNIQUE` do nome não deixa). Renomear libera o nome e materializa a outra. Mesclar mantém a
//! tag de `participant_a` (ordem canônica: os dois aparelhos escolhem a mesma sobrevivente), move
//! para ela todas as marcações da outra — inclusive as que ainda esperavam a tag existir — e exclui
//! a outra. Tudo numa `Mutacao`.
//!
//! ## Por que aceitar a exclusão refaz o preflight
//!
//! O bloqueio aconteceu porque havia um descendente que a origem da exclusão não apagou. Entre o
//! bloqueio e a decisão, o escritor pode ter criado mais filhos, ou resolvido os que existiam.
//! Aceitar com base no que era verdade no bloqueio deixaria a cascata apagar trabalho concorrente.
//! O mesmo vale para "manter a exclusão" num conflito de edição contra exclusão.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::application::mutacao::{DeclaracaoDeResolucao, Mutacao};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::conflito::{partes_da_identidade, ConflictParticipant};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::now_timestamp;
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};
use crate::infrastructure::sqlite::sync_codec::resolucao::{Certificado, TIPO as TIPO_RESOLUCAO};
use crate::infrastructure::sqlite::{sync_apply, sync_codec, sync_repository, SqliteDatabase};

// ═══════════════════════════════════════════════════════════════════════════
// API
// ═══════════════════════════════════════════════════════════════════════════

/// O que o escritor decide. Portátil: nada aqui depende de qual aparelho está olhando.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "camelCase")]
pub enum Acao {
    /// Ficar com o participante A (ordem canônica).
    FicarComA,
    /// Ficar com o participante B (ordem canônica).
    FicarComB,
    /// Edição contra exclusão: fica o conteúdo.
    Restaurar,
    /// Edição contra exclusão: fica a exclusão.
    ManterExclusao,
    /// Exclusão bloqueada: pai e descendentes ficam.
    ManterLocal,
    /// Exclusão bloqueada: a exclusão do outro aparelho é aceita.
    AceitarExclusao,
    /// Tag homônima: uma das duas ganha outro nome.
    #[serde(rename_all = "camelCase")]
    Renomear { tag_id: String, nome: String },
    /// Tag homônima: as duas são a mesma tag.
    Mesclar,
}

/// O que a resolução confirmou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultadoDaResolucao {
    pub conflict_key: String,
    /// A revisão de `conflict_resolution/<conflictKey>`: o fato causal que fechou o conflito.
    pub resolution_rev: String,
}

/// Compatibilidade da API anterior à etapa F (exclusão bloqueada, por id local).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escolha {
    ManterLocal,
    AceitarRemoto,
}

/// O resultado na API anterior à etapa F.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolucao {
    /// O pai e os descendentes ficaram; a restauração foi emitida.
    MantidoLocal,
    /// A exclusão remota foi aplicada.
    ExclusaoConcluida,
}

/// **A API anterior à etapa F**, mantida para a exclusão bloqueada: o mesmo caminho certificado.
pub fn resolver(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id_divergencia: &str,
    escolha: Escolha,
) -> DatabaseCommandResult<Resolucao> {
    let (chave, kind): (String, String) = {
        let connection = database.read()?;
        connection
            .query_row(
                "SELECT conflict_key, kind FROM sync_divergences WHERE id = ?1 AND resolved_at = ''",
                [id_divergencia],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(erro)?
            .ok_or_else(|| {
                DatabaseCommandError::not_found(format!(
                    "Não há divergência aberta com id {id_divergencia}. Ela pode já ter sido resolvida."
                ))
            })?
    };
    match kind.as_str() {
        "parent_deletion_blocked" => {}
        "concurrent" => {
            return Err(DatabaseCommandError::conflict(
                "Esta divergência é entre duas versões editadas: escolha uma das versões pela \
                 resolução de conflitos. Nada foi alterado.",
            ))
        }
        outro => {
            return Err(DatabaseCommandError::storage(format!(
                "Tipo de divergência sem esta escolha: '{outro}'. Nada foi alterado."
            )))
        }
    }
    let acao = match escolha {
        Escolha::ManterLocal => Acao::ManterLocal,
        Escolha::AceitarRemoto => Acao::AceitarExclusao,
    };
    resolver_conflito(database, identidade, &chave, &acao)?;
    Ok(match escolha {
        Escolha::ManterLocal => Resolucao::MantidoLocal,
        Escolha::AceitarRemoto => Resolucao::ExclusaoConcluida,
    })
}

/// **Resolve o conflito aberto aqui com a chave dada.** Tudo numa `Mutacao`.
pub fn resolver_conflito(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    conflict_key: &str,
    acao: &Acao,
) -> DatabaseCommandResult<ResultadoDaResolucao> {
    Mutacao::executar(database, identidade, |m| {
        let divergencia = ler_aberta(m.tx(), conflict_key)?;
        let permitidas = acoes_permitidas(m.tx(), &divergencia)?;
        let nome_da_acao = nome(acao);
        if !permitidas.contains(&nome_da_acao) {
            return Err(DatabaseCommandError::validation(format!(
                "A ação '{nome_da_acao}' não se aplica a este conflito. Nada foi alterado."
            )));
        }
        match (divergencia.kind.as_str(), acao) {
            ("parent_deletion_blocked", Acao::ManterLocal) => {
                exclusao_bloqueada(m, &divergencia, true)
            }
            ("parent_deletion_blocked", Acao::AceitarExclusao) => {
                exclusao_bloqueada(m, &divergencia, false)
            }
            ("tag_name_conflict", Acao::Renomear { tag_id, nome }) => {
                renomear_tag(m, &divergencia, tag_id, nome)
            }
            ("tag_name_conflict", Acao::Mesclar) => mesclar_tags(m, &divergencia),
            ("concurrent", _) if !divergencia.mutation_id.is_empty() => {
                concorrente_do_grupo(m, &divergencia, acao)
            }
            ("concurrent", _) if divergencia.agregado.aggregate_type == TIPO_RESOLUCAO => {
                decisao_concorrente(m, &divergencia, acao)
            }
            ("concurrent", _) => concorrente(m, &divergencia, acao),
            _ => Err(DatabaseCommandError::validation(
                "Esta ação não se aplica a este conflito. Nada foi alterado.",
            )),
        }?;
        Ok(ResultadoDaResolucao {
            conflict_key: conflict_key.to_string(),
            resolution_rev: String::new(),
        })
    })
    .and_then(|mut resultado| {
        let connection = database.read()?;
        resultado.resolution_rev = sync_codec::revisao_corrente(
            &connection,
            &AggregateRef::new(TIPO_RESOLUCAO, conflict_key),
        )?
        .unwrap_or_default();
        Ok(resultado)
    })
}

/// **As resoluções automáticas**: exclusão contra exclusão, e duas edições que chegaram ao mesmo
/// conteúdo. Nenhuma pergunta ao escritor — mas cada uma é uma resolução causal de verdade, que
/// converge os dois lados para uma revisão só. Devolve quantas foram resolvidas.
pub fn resolver_automaticas(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<usize> {
    superar_perguntas_obsoletas(database)?;
    let chaves: Vec<String> = {
        let connection = database.read()?;
        let mut consulta = connection
            .prepare(
                "SELECT DISTINCT conflict_key FROM sync_divergences
                  WHERE resolved_at = '' AND kind = 'concurrent' AND conflict_key <> ''
                  ORDER BY conflict_key",
            )
            .map_err(erro)?;
        let linhas = consulta.query_map([], |row| row.get(0)).map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)?
    };
    let mut resolvidas = 0usize;
    for chave in chaves {
        let elegivel = {
            let connection = database.read()?;
            let divergencia = ler_aberta(&connection, &chave)?;
            plano_automatico(&connection, &divergencia)?.is_some()
        };
        if !elegivel {
            continue;
        }
        Mutacao::executar(database, identidade, |m| {
            let divergencia = ler_aberta(m.tx(), &chave)?;
            let Some(plano) = plano_automatico(m.tx(), &divergencia)? else {
                return Ok(());
            };
            declarar(m, &divergencia, "auto", "manual")?;
            for efeito in plano {
                m.efeito(
                    efeito.agregado,
                    efeito.operacao,
                    &efeito.base,
                    &efeito.outra,
                    &efeito.universo,
                )?;
            }
            Ok(())
        })?;
        resolvidas += 1;
    }
    Ok(resolvidas)
}

/// **Pergunta obsoleta não chega ao escritor.**
///
/// Uma divergência de edição cujo lado de lá já tem sucessor conhecido aqui foi superada: o outro
/// aparelho seguiu editando (ou excluiu) a partir dela, e o sucessor já foi classificado — aplicado,
/// ou virou a divergência nova, que é a pergunta de verdade. A antiga fecha como `manual`, sem fato
/// causal: não houve decisão, houve uma pergunta que deixou de existir. Só o índice local muda.
fn superar_perguntas_obsoletas(database: &SqliteDatabase) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    connection
        .execute(
            "UPDATE sync_divergences SET resolved_at = ?1, resolution = 'manual'
              WHERE resolved_at = '' AND kind = 'concurrent' AND mutation_id = ''
                AND aggregate_type <> ?2
                AND EXISTS (SELECT 1 FROM sync_revision_history h
                             WHERE h.aggregate_type = sync_divergences.aggregate_type
                               AND h.aggregate_id = sync_divergences.aggregate_id
                               AND h.base_rev = sync_divergences.remote_rev)",
            rusqlite::params![now_timestamp(), TIPO_RESOLUCAO],
        )
        .map_err(erro)?;
    Ok(())
}

/// As ações que o escritor pode tomar neste conflito. Vazio = resolução automática.
pub fn acoes_permitidas(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<Vec<&'static str>> {
    Ok(match divergencia.kind.as_str() {
        "parent_deletion_blocked" => vec!["manterLocal", "aceitarExclusao"],
        "tag_name_conflict" => vec!["renomear", "mesclar"],
        "concurrent" => {
            if plano_automatico(connection, divergencia)?.is_some() {
                Vec::new()
            } else if divergencia.agregado.aggregate_type == TIPO_RESOLUCAO {
                vec!["ficarComA", "ficarComB"]
            } else {
                match (
                    divergencia.local_operation.as_str(),
                    divergencia.remote_operation.as_str(),
                ) {
                    ("upsert", "upsert") => vec!["ficarComA", "ficarComB"],
                    ("upsert", "delete") | ("delete", "upsert") => {
                        vec!["restaurar", "manterExclusao"]
                    }
                    // Uma ação inteira cujos membros não são todos resolvíveis sozinhos.
                    _ if !divergencia.mutation_id.is_empty() => vec!["ficarComA", "ficarComB"],
                    _ => Vec::new(),
                }
            }
        }
        _ => Vec::new(),
    })
}

fn nome(acao: &Acao) -> &'static str {
    match acao {
        Acao::FicarComA => "ficarComA",
        Acao::FicarComB => "ficarComB",
        Acao::Restaurar => "restaurar",
        Acao::ManterExclusao => "manterExclusao",
        Acao::ManterLocal => "manterLocal",
        Acao::AceitarExclusao => "aceitarExclusao",
        Acao::Renomear { .. } => "renomear",
        Acao::Mesclar => "mesclar",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// A divergência como o resolvedor a lê
// ═══════════════════════════════════════════════════════════════════════════

/// Uma divergência aberta, com os dois participantes já decodificados.
#[derive(Debug, Clone)]
pub struct Divergencia {
    pub id: String,
    pub agregado: AggregateRef,
    pub base_rev: String,
    pub local_rev: String,
    pub remote_rev: String,
    pub local_operation: String,
    pub remote_operation: String,
    pub remote_event_id: String,
    pub kind: String,
    pub mutation_id: String,
    pub related_aggregate_id: String,
    pub conflict_key: String,
    /// Participante A (ordem canônica).
    pub a: ConflictParticipant,
    /// Participante B (ordem canônica).
    pub b: ConflictParticipant,
    pub detected_at: String,
}

impl Divergencia {
    /// O participante que é o lado DAQUI.
    pub fn local(&self) -> &ConflictParticipant {
        if self.e_local(&self.a) {
            &self.a
        } else {
            &self.b
        }
    }

    /// O participante que é o lado de LÁ.
    pub fn remoto(&self) -> &ConflictParticipant {
        if self.e_local(&self.a) {
            &self.b
        } else {
            &self.a
        }
    }

    fn e_local(&self, participante: &ConflictParticipant) -> bool {
        match self.kind.as_str() {
            "tag_name_conflict" => participante.aggregate_id == self.related_aggregate_id,
            _ => {
                participante.revision == self.local_rev
                    && participante.operation == self.local_operation
            }
        }
    }
}

fn participante(canonico: &str) -> DatabaseCommandResult<ConflictParticipant> {
    let partes = partes_da_identidade(canonico)
        .filter(|p| p.len() == 4)
        .ok_or_else(|| {
            DatabaseCommandError::storage(format!(
                "Participante de conflito ilegível: '{canonico}'."
            ))
        })?;
    Ok(ConflictParticipant::new(
        &partes[0], &partes[1], &partes[2], &partes[3],
    ))
}

pub fn ler_aberta(
    connection: &rusqlite::Connection,
    chave: &str,
) -> DatabaseCommandResult<Divergencia> {
    ler_por_chave(connection, chave, true)?.ok_or_else(|| {
        DatabaseCommandError::not_found(
            "Não há conflito aberto com esta chave. Ele pode já ter sido resolvido.".to_string(),
        )
    })
}

pub fn ler_por_chave(
    connection: &rusqlite::Connection,
    chave: &str,
    so_aberta: bool,
) -> DatabaseCommandResult<Option<Divergencia>> {
    let linha = connection
        .query_row(
            &format!(
                "SELECT id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                        local_operation, remote_operation, remote_event_id, kind, mutation_id,
                        related_aggregate_id, conflict_key, participant_a, participant_b, detected_at
                   FROM sync_divergences
                  WHERE conflict_key = ?1 {}
                  ORDER BY resolved_at = '' DESC, detected_at DESC LIMIT 1",
                if so_aberta { "AND resolved_at = ''" } else { "" }
            ),
            [chave],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                ))
            },
        )
        .optional()
        .map_err(erro)?;
    let Some(l) = linha else {
        return Ok(None);
    };
    Ok(Some(Divergencia {
        id: l.0,
        agregado: AggregateRef::new(&l.1, &l.2),
        base_rev: l.3,
        local_rev: l.4,
        remote_rev: l.5,
        local_operation: l.6,
        remote_operation: l.7,
        remote_event_id: l.8,
        kind: l.9,
        mutation_id: l.10,
        related_aggregate_id: l.11,
        conflict_key: l.12,
        a: participante(&l.13)?,
        b: participante(&l.14)?,
        detected_at: l.15,
    }))
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// O evento que produziu esta revisão deste agregado, se ele está no log daqui.
pub fn evento_da_revisao(
    connection: &rusqlite::Connection,
    agregado: &AggregateRef,
    revisao: &str,
) -> DatabaseCommandResult<Option<EventEnvelope>> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM sync_events
                  WHERE aggregate_type = ?1 AND aggregate_id = ?2 AND new_rev = ?3
                  ORDER BY rowid LIMIT 1",
                sync_repository::colunas_do_envelope("")
            ),
            rusqlite::params![&agregado.aggregate_type, &agregado.aggregate_id, revisao],
            sync_repository::envelope_da_linha,
        )
        .optional()
        .map_err(erro)
}

/// O payload de uma revisão: o do evento no log, ou o do domínio quando a revisão é a corrente (um
/// aparelho semeado por bootstrap não tem o log que produziu as revisões dele).
pub fn payload_da_revisao(
    connection: &rusqlite::Connection,
    agregado: &AggregateRef,
    revisao: &str,
) -> DatabaseCommandResult<Option<String>> {
    if let Some(evento) = evento_da_revisao(connection, agregado, revisao)? {
        return Ok(Some(evento.payload));
    }
    if sync_codec::revisao_corrente(connection, agregado)?.as_deref() == Some(revisao) {
        return Ok(sync_codec::ler_canonico(connection, agregado)?.map(|e| e.payload));
    }
    Ok(None)
}

/// A cabeça daqui de um agregado: a revisão corrente, ou a da exclusão, ou nenhuma.
fn cabeca(
    connection: &rusqlite::Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<(String, Operation)>> {
    let historia = sync_repository::aggregate_history(connection, agregado)?;
    Ok(match (historia.current_rev, historia.deleted_rev) {
        (Some(rev), _) => Some((rev, Operation::Upsert)),
        (None, Some(rev)) => Some((rev, Operation::Delete)),
        (None, None) => None,
    })
}

/// O universo do conflito: o do evento que o trouxe, ou o do estado daqui.
fn universo_do_conflito(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<String> {
    let do_evento: Option<String> = connection
        .query_row(
            "SELECT universe_id FROM sync_events WHERE event_id = ?1",
            [&divergencia.remote_event_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?;
    if let Some(universo) = do_evento.filter(|u| !u.is_empty()) {
        return Ok(universo);
    }
    if let Some(estado) = sync_codec::ler_canonico(connection, &divergencia.agregado)? {
        return Ok(estado.universe_id);
    }
    Err(DatabaseCommandError::storage(
        "O conflito não tem universo conhecido aqui. Nada foi alterado.",
    ))
}

fn declarar(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    choice: &str,
    resolucao_local: &str,
) -> DatabaseCommandResult<()> {
    let universe_id = universo_do_conflito(m.tx(), divergencia)?;
    // O índice local fecha ANTES dos efeitos: aberto, ele mesmo faria o preflight de uma exclusão
    // (ou a aplicação de um membro decidido) recusar o que a decisão manda fazer. A `Mutacao`
    // completa o `resolution_rev` no fim, na mesma transação — ou nada disto fica.
    m.tx()
        .execute(
            "UPDATE sync_divergences SET resolved_at = ?1, resolution = ?2
              WHERE conflict_key = ?3 AND resolved_at = ''",
            rusqlite::params![now_timestamp(), resolucao_local, &divergencia.conflict_key],
        )
        .map_err(erro)?;
    m.declarar_resolucao(DeclaracaoDeResolucao {
        conflict_key: divergencia.conflict_key.clone(),
        kind: divergencia.kind.clone(),
        participante_um: divergencia.a.clone(),
        participante_outro: divergencia.b.clone(),
        choice: choice.to_string(),
        universe_id,
        resolucao_local: resolucao_local.to_string(),
    })
}

/// Escreve no domínio o estado de uma revisão que está no log (upsert).
fn materializar(
    m: &Mutacao<'_, '_>,
    agregado: &AggregateRef,
    revisao: &str,
) -> DatabaseCommandResult<EventEnvelope> {
    let evento = evento_da_revisao(m.tx(), agregado, revisao)?.ok_or_else(|| {
        DatabaseCommandError::storage(format!(
            "A versão escolhida de {} {} não está neste aparelho. Nada foi alterado.",
            agregado.aggregate_type, agregado.aggregate_id
        ))
    })?;
    if evento.operation == Operation::Upsert {
        if let Some(falta) = sync_codec::dependencias(m.tx(), &evento)? {
            return Err(DatabaseCommandError::conflict(format!(
                "Não dá para ficar com esta versão ainda: falta {falta}. Nada foi alterado."
            )));
        }
        sync_codec::aplicar(m.tx(), &evento)?;
    }
    Ok(evento)
}

/// Remove do domínio (o preflight de exclusão da `Mutacao` já rodou).
fn apagar_do_dominio(m: &Mutacao<'_, '_>, agregado: &AggregateRef) -> DatabaseCommandResult<()> {
    let envelope = EventEnvelope {
        event_id: String::new(),
        device_id: String::new(),
        seq: 0,
        universe_id: String::new(),
        aggregate_type: agregado.aggregate_type.clone(),
        aggregate_id: agregado.aggregate_id.clone(),
        operation: Operation::Delete,
        payload: String::new(),
        base_rev: String::new(),
        new_rev: String::new(),
        signature: String::new(),
        grupo: Default::default(),
    };
    sync_codec::aplicar(m.tx(), &envelope)
}

/// A cabeça daqui ainda é o lado daqui do conflito? Senão a decisão seria sobre algo que já mudou.
fn exigir_cabeca_do_conflito(
    m: &Mutacao<'_, '_>,
    agregado: &AggregateRef,
    esperada: &str,
) -> DatabaseCommandResult<()> {
    let atual = cabeca(m.tx(), agregado)?.map(|(rev, _)| rev);
    if atual.as_deref() != Some(esperada) {
        return Err(DatabaseCommandError::conflict(format!(
            "{} {} mudou depois que o conflito foi detectado. Sincronize e decida de novo; nada foi \
             alterado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// concurrent — um agregado
// ═══════════════════════════════════════════════════════════════════════════

fn concorrente(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    acao: &Acao,
) -> DatabaseCommandResult<()> {
    let escolhido = participante_escolhido(divergencia, acao);
    let outro = if escolhido == &divergencia.a {
        &divergencia.b
    } else {
        &divergencia.a
    };
    let local = divergencia.local().clone();
    exigir_cabeca_do_conflito(m, &divergencia.agregado, &local.revision)?;
    let escolha = if escolhido == &divergencia.a {
        "a"
    } else {
        "b"
    };
    let lado = if escolhido.revision == local.revision {
        "local"
    } else {
        "remote"
    };
    declarar(m, divergencia, escolha, lado)?;
    efeito_do_escolhido(m, &divergencia.agregado, escolhido, outro, &local)
}

/// Qual participante a ação escolhe. "Restaurar" é o lado que existe; "manter a exclusão", o que
/// excluiu — a mesma resposta em qualquer aparelho, porque os participantes são os mesmos.
fn participante_escolhido<'d>(
    divergencia: &'d Divergencia,
    acao: &Acao,
) -> &'d ConflictParticipant {
    match acao {
        Acao::FicarComA => &divergencia.a,
        Acao::FicarComB => &divergencia.b,
        Acao::Restaurar if divergencia.a.operation == "upsert" => &divergencia.a,
        Acao::ManterExclusao if divergencia.a.operation == "delete" => &divergencia.a,
        _ => &divergencia.b,
    }
}

/// Deixa o agregado no estado do participante escolhido e declara o efeito com a base dele.
fn efeito_do_escolhido(
    m: &mut Mutacao<'_, '_>,
    agregado: &AggregateRef,
    escolhido: &ConflictParticipant,
    outro: &ConflictParticipant,
    local: &ConflictParticipant,
) -> DatabaseCommandResult<()> {
    let universo = universo_de(m, agregado, &escolhido.revision)?;
    match (escolhido.operation.as_str(), local.operation.as_str()) {
        ("upsert", "delete") => {
            // Restaurar aqui, onde a exclusão aconteceu: o item volta, e a posição dele — que saiu
            // junto, na mesma ação — volta também, descendendo das duas cabeças dela.
            materializar(m, agregado, &escolhido.revision)?;
            m.efeito(
                agregado.clone(),
                Operation::Upsert,
                &escolhido.revision,
                &outro.revision,
                &universo,
            )?;
            efeito_da_posicao(m, agregado, Operation::Upsert, &universo)
        }
        ("upsert", _) => {
            if escolhido.revision != local.revision {
                materializar(m, agregado, &escolhido.revision)?;
            }
            m.efeito(
                agregado.clone(),
                Operation::Upsert,
                &escolhido.revision,
                &outro.revision,
                &universo,
            )
        }
        ("delete", "upsert") => {
            m.excluir_como_efeito(
                &agregado.aggregate_type,
                &agregado.aggregate_id,
                &escolhido.revision,
                &outro.revision,
            )?;
            apagar_do_dominio(m, agregado)
        }
        _ => {
            // Manter a exclusão onde ela aconteceu: a posição sai antes do item, como na ação
            // original, descendendo das duas cabeças dela.
            efeito_da_posicao(m, agregado, Operation::Delete, &universo)?;
            m.efeito(
                agregado.clone(),
                Operation::Delete,
                &escolhido.revision,
                &outro.revision,
                &universo,
            )
        }
    }
}

/// A posição de um item (existência derivada), quando a exclusão do item a levou junto AQUI.
///
/// A cabeça daqui é o tombstone da posição; a de lá é a revisão de onde a exclusão partiu — o
/// outro aparelho não mexeu nela, só no item. O efeito descende das duas.
fn efeito_da_posicao(
    m: &mut Mutacao<'_, '_>,
    item: &AggregateRef,
    operacao: Operation,
    universo: &str,
) -> DatabaseCommandResult<()> {
    let Some(tipo) = sync_codec::posicao::posicao_do_item(&item.aggregate_type) else {
        return Ok(());
    };
    let posicao = AggregateRef::new(tipo, &item.aggregate_id);
    let Some((tombstone, Operation::Delete)) = cabeca(m.tx(), &posicao)? else {
        return Ok(());
    };
    let de_la = evento_da_revisao(m.tx(), &posicao, &tombstone)?
        .map(|evento| evento.base_rev)
        .unwrap_or_default();
    if operacao == Operation::Upsert && sync_codec::ler_canonico(m.tx(), &posicao)?.is_none() {
        return Ok(());
    }
    m.efeito(posicao, operacao, &tombstone, &de_la, universo)
}

/// O universo de um agregado para o efeito: o do estado atual, ou o do evento da revisão.
fn universo_de(
    m: &Mutacao<'_, '_>,
    agregado: &AggregateRef,
    revisao: &str,
) -> DatabaseCommandResult<String> {
    if let Some(estado) = sync_codec::ler_canonico(m.tx(), agregado)? {
        return Ok(estado.universe_id);
    }
    if let Some(evento) = evento_da_revisao(m.tx(), agregado, revisao)? {
        if !evento.universe_id.is_empty() {
            return Ok(evento.universe_id);
        }
    }
    let ultimo: Option<String> = m
        .tx()
        .query_row(
            "SELECT universe_id FROM sync_events
              WHERE aggregate_type = ?1 AND aggregate_id = ?2 AND universe_id <> ''
              ORDER BY rowid DESC LIMIT 1",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?;
    ultimo.ok_or_else(|| {
        DatabaseCommandError::storage(format!(
            "{} {} não tem universo conhecido aqui. Nada foi alterado.",
            agregado.aggregate_type, agregado.aggregate_id
        ))
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// automáticas
// ═══════════════════════════════════════════════════════════════════════════

struct EfeitoPlanejado {
    agregado: AggregateRef,
    operacao: Operation,
    base: String,
    outra: String,
    universo: String,
}

/// O plano de uma resolução automática, ou `None` quando o conflito precisa do escritor.
///
/// Por agregado, o par é (cabeça daqui, revisão de lá). Elegível só quando **todo** par é exclusão
/// contra exclusão ou dois conteúdos canônicos idênticos; a base é sempre o primeiro do par na ordem
/// canônica — os dois aparelhos escolhem a mesma, e chegam à mesma revisão final.
fn plano_automatico(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<Option<Vec<EfeitoPlanejado>>> {
    if divergencia.kind != "concurrent" || divergencia.agregado.aggregate_type == TIPO_RESOLUCAO {
        return Ok(None);
    }
    let pares: Vec<(AggregateRef, String, String, String)> = if divergencia.mutation_id.is_empty() {
        vec![(
            divergencia.agregado.clone(),
            divergencia.remote_rev.clone(),
            divergencia.remote_operation.clone(),
            String::new(),
        )]
    } else {
        membros_do_grupo(connection, divergencia)?
            .into_iter()
            .map(|membro| {
                (
                    AggregateRef::new(&membro.aggregate_type, &membro.aggregate_id),
                    membro.new_rev,
                    membro.operation.as_str().to_string(),
                    membro.universe_id,
                )
            })
            .collect()
    };
    let mut plano = Vec::new();
    for (agregado, remota, operacao_remota, universo_do_membro) in pares {
        let Some((local, operacao_local)) = cabeca(connection, &agregado)? else {
            return Ok(None);
        };
        if local == remota {
            continue;
        }
        let eligivel = match (operacao_local, operacao_remota.as_str()) {
            (Operation::Delete, "delete") => true,
            (Operation::Upsert, "upsert") => {
                let daqui = payload_da_revisao(connection, &agregado, &local)?;
                let de_la = payload_da_revisao(connection, &agregado, &remota)?;
                daqui.is_some() && daqui == de_la
            }
            _ => false,
        };
        if !eligivel {
            return Ok(None);
        }
        let um = ConflictParticipant::new(
            &agregado.aggregate_type,
            &agregado.aggregate_id,
            &local,
            operacao_local.as_str(),
        );
        let outro = ConflictParticipant::new(
            &agregado.aggregate_type,
            &agregado.aggregate_id,
            &remota,
            &operacao_remota,
        );
        let (base, outra) = if um.canonico().as_bytes() <= outro.canonico().as_bytes() {
            (local, remota)
        } else {
            (remota, local)
        };
        let universo = match sync_codec::ler_canonico(connection, &agregado)? {
            Some(estado) => estado.universe_id,
            None if !universo_do_membro.is_empty() => universo_do_membro,
            None => universo_do_conflito(connection, divergencia)?,
        };
        plano.push(EfeitoPlanejado {
            agregado,
            operacao: operacao_local,
            base,
            outra,
            universo,
        });
    }
    if plano.is_empty() {
        return Ok(None);
    }
    Ok(Some(plano))
}

// ═══════════════════════════════════════════════════════════════════════════
// concurrent — a ação inteira de um grupo
// ═══════════════════════════════════════════════════════════════════════════

/// Os membros do grupo da decisão, na ordem do grupo. O grupo é `(origem, mutation_id)`: a origem
/// é a do evento em que a decisão foi ancorada.
fn membros_do_grupo(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<Vec<EventEnvelope>> {
    if divergencia.mutation_id.is_empty() {
        return Ok(Vec::new());
    }
    let mut consulta = connection
        .prepare(&format!(
            "SELECT {} FROM sync_events
              WHERE mutation_id = ?1
                AND device_id = (SELECT device_id FROM sync_events WHERE event_id = ?2)
              ORDER BY mutation_index",
            sync_repository::colunas_do_envelope("")
        ))
        .map_err(erro)?;
    let membros = consulta
        .query_map(
            [&divergencia.mutation_id, &divergencia.remote_event_id],
            sync_repository::envelope_da_linha,
        )
        .map_err(erro)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(erro)?;
    Ok(membros)
}

/// "Ficar com a ação de lá" ou "ficar com a daqui", membro a membro, numa mutação.
fn concorrente_do_grupo(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    acao: &Acao,
) -> DatabaseCommandResult<()> {
    let escolhido = participante_escolhido(divergencia, acao);
    let fica_o_remoto = escolhido.revision != divergencia.local().revision;
    let escolha = if escolhido == &divergencia.a {
        "a"
    } else {
        "b"
    };
    declarar(
        m,
        divergencia,
        escolha,
        if fica_o_remoto { "remote" } else { "local" },
    )?;
    let membros = membros_do_grupo(m.tx(), divergencia)?;
    if fica_o_remoto {
        // Desfaz-se na ordem inversa o que só existe aqui? Não: fica a ação de lá, membro a membro,
        // na ordem do grupo — que é a ordem em que ela é materializável.
        for membro in &membros {
            let agregado = AggregateRef::new(&membro.aggregate_type, &membro.aggregate_id);
            let cabeca_daqui = cabeca(m.tx(), &agregado)?;
            if cabeca_daqui.as_ref().map(|(rev, _)| rev.as_str()) == Some(membro.new_rev.as_str()) {
                continue;
            }
            let outra = cabeca_daqui.map(|(rev, _)| rev).unwrap_or_default();
            match membro.operation {
                Operation::Upsert => {
                    materializar(m, &agregado, &membro.new_rev)?;
                    m.efeito(
                        agregado,
                        Operation::Upsert,
                        &membro.new_rev,
                        &outra,
                        &membro.universe_id,
                    )?;
                }
                Operation::Delete => {
                    if sync_codec::ler_canonico(m.tx(), &agregado)?.is_some() {
                        m.excluir_como_efeito(
                            &agregado.aggregate_type,
                            &agregado.aggregate_id,
                            &membro.new_rev,
                            &outra,
                        )?;
                        apagar_do_dominio(m, &agregado)?;
                    } else {
                        m.efeito(
                            agregado,
                            Operation::Delete,
                            &membro.new_rev,
                            &outra,
                            &membro.universe_id,
                        )?;
                    }
                }
            }
        }
    } else {
        // Fica a daqui: a ação de lá é rejeitada inteira. Cada agregado dela passa a ter a cabeça
        // daqui como resultado — e o que só existia lá sai, porque só existia pela ação rejeitada.
        for membro in membros.iter().rev() {
            let agregado = AggregateRef::new(&membro.aggregate_type, &membro.aggregate_id);
            match cabeca(m.tx(), &agregado)? {
                Some((rev, _)) if rev == membro.new_rev => {}
                Some((rev, Operation::Upsert)) => {
                    let universo = universo_de(m, &agregado, &rev)?;
                    m.efeito(
                        agregado,
                        Operation::Upsert,
                        &rev,
                        &membro.new_rev,
                        &universo,
                    )?;
                }
                Some((rev, Operation::Delete)) => {
                    m.efeito(
                        agregado,
                        Operation::Delete,
                        &rev,
                        &membro.new_rev,
                        &membro.universe_id,
                    )?;
                }
                None if membro.operation == Operation::Upsert => {
                    m.efeito(
                        agregado,
                        Operation::Delete,
                        &membro.new_rev,
                        "",
                        &membro.universe_id,
                    )?;
                }
                None => {}
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// concurrent sobre conflict_resolution — duas decisões diferentes
// ═══════════════════════════════════════════════════════════════════════════

/// Escolhe uma das duas decisões: ela passa a ser a de todos, com os efeitos dela.
fn decisao_concorrente(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    acao: &Acao,
) -> DatabaseCommandResult<()> {
    let (escolhido, outro) = match acao {
        Acao::FicarComA => (&divergencia.a, &divergencia.b),
        Acao::FicarComB => (&divergencia.b, &divergencia.a),
        _ => {
            return Err(DatabaseCommandError::validation(
                "Duas decisões diferentes se resolvem escolhendo uma delas.",
            ))
        }
    };
    exigir_cabeca_do_conflito(m, &divergencia.agregado, &divergencia.local().revision)?;
    let payload_escolhido = payload_da_revisao(m.tx(), &divergencia.agregado, &escolhido.revision)?
        .ok_or_else(|| {
            DatabaseCommandError::storage("A decisão escolhida não está neste aparelho.")
        })?;
    let payload_outro =
        payload_da_revisao(m.tx(), &divergencia.agregado, &outro.revision)?.unwrap_or_default();
    let certificado =
        Certificado::ler(&payload_escolhido).map_err(DatabaseCommandError::storage)?;
    let outro_certificado = Certificado::ler(&payload_outro).ok();
    let escolha = if escolhido == &divergencia.a {
        "a"
    } else {
        "b"
    };
    let lado = if escolhido.revision == divergencia.local().revision {
        "local"
    } else {
        "remote"
    };
    declarar(m, divergencia, escolha, lado)?;

    // Os efeitos da decisão escolhida, sobre o que cada agregado é aqui.
    for efeito in &certificado.results {
        let agregado = AggregateRef::new(&efeito.aggregate_type, &efeito.aggregate_id);
        let alternativa = outro_certificado
            .as_ref()
            .and_then(|c| {
                c.results.iter().find(|r| {
                    r.aggregate_type == efeito.aggregate_type
                        && r.aggregate_id == efeito.aggregate_id
                })
            })
            .map(|r| r.result_rev.clone())
            .unwrap_or_default();
        let cabeca_daqui = cabeca(m.tx(), &agregado)?.map(|(rev, _)| rev);
        let outra = match cabeca_daqui {
            Some(rev) if rev != efeito.result_rev => rev,
            _ => alternativa,
        };
        let universo = universo_de(m, &agregado, &efeito.result_rev)?;
        if efeito.operation == "upsert" {
            if cabeca(m.tx(), &agregado)?.map(|(rev, _)| rev).as_deref()
                != Some(efeito.result_rev.as_str())
            {
                materializar(m, &agregado, &efeito.result_rev)?;
            }
            m.efeito(
                agregado,
                Operation::Upsert,
                &efeito.result_rev,
                &outra,
                &universo,
            )?;
        } else if sync_codec::ler_canonico(m.tx(), &agregado)?.is_some() {
            m.excluir_como_efeito(
                &agregado.aggregate_type,
                &agregado.aggregate_id,
                &efeito.result_rev,
                &outra,
            )?;
            apagar_do_dominio(m, &agregado)?;
        } else {
            m.efeito(
                agregado,
                Operation::Delete,
                &efeito.result_rev,
                &outra,
                &universo,
            )?;
        }
    }

    // E a própria decisão sobre o conflito original passa a ser a escolhida.
    let universo = universo_do_conflito(m.tx(), divergencia)?;
    m.tx()
        .execute(
            "INSERT INTO conflict_resolutions (conflict_key, universe_id, kind, certificate)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(conflict_key) DO UPDATE SET
                universe_id = excluded.universe_id, kind = excluded.kind,
                certificate = excluded.certificate",
            rusqlite::params![
                &certificado.conflict_key,
                &universo,
                &certificado.kind,
                &payload_escolhido
            ],
        )
        .map_err(erro)?;
    m.efeito(
        divergencia.agregado.clone(),
        Operation::Upsert,
        &escolhido.revision,
        &outro.revision,
        &universo,
    )?;
    // O conflito original, se ainda estiver aberto aqui, é fechado pela decisão escolhida.
    m.tx()
        .execute(
            "UPDATE sync_divergences SET resolved_at = ?1, resolution = 'manual'
              WHERE conflict_key = ?2 AND resolved_at = ''",
            rusqlite::params![now_timestamp(), &certificado.conflict_key],
        )
        .map_err(erro)?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// parent_deletion_blocked
// ═══════════════════════════════════════════════════════════════════════════

/// A exclusão bloqueada, com o contrato de sempre — agora como resolução certificada.
///
/// ```text
/// manter    o pai (e a ação inteira, num grupo) fica; nasce a restauração a partir da exclusão
/// aceitar   refaz o preflight AGORA e aplica a exclusão; a revisão final parte da dela
/// ```
fn exclusao_bloqueada(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    manter: bool,
) -> DatabaseCommandResult<()> {
    let local = divergencia.local().clone();
    let remoto = divergencia.remoto().clone();
    let escolha = if (manter && local == divergencia.a) || (!manter && remoto == divergencia.a) {
        "a"
    } else {
        "b"
    };
    declarar(
        m,
        divergencia,
        escolha,
        if manter { "local" } else { "remote" },
    )?;
    let membros = membros_do_grupo(m.tx(), divergencia)?;
    if membros.len() > 1 {
        if manter {
            manter_local_o_grupo(m, divergencia, &membros)
        } else {
            aceitar_o_grupo(m, divergencia, &membros)
        }
    } else if manter {
        manter_local(m, divergencia)
    } else {
        aceitar_exclusao(m, divergencia)
    }
}

/// "Manter o local" vale para a ação inteira, e fecha a causalidade de **todo** membro.
///
/// ```text
/// membro   aqui                       efeito
/// upsert   existe, payload igual      adota new_rev da origem; sem evento
/// upsert   existe, payload diferente  o estado daqui vira revisão nova sobre new_rev da origem
/// upsert   não existe (excluído aqui) exclusão nova sobre new_rev da origem
/// delete   não existe                 adota new_rev da origem como tombstone; sem evento
/// delete   existe                     restauração sobre o tombstone da origem
/// ```
///
/// **Ordem: exclusões reafirmadas da raiz para as folhas, depois os sobreviventes.** A âncora da
/// decisão sai sempre como efeito, com a base da origem e a cabeça daqui como a outra.
fn manter_local_o_grupo(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    membros: &[EventEnvelope],
) -> DatabaseCommandResult<()> {
    let exclusoes = membros
        .iter()
        .rev()
        .filter(|membro| membro.operation == Operation::Delete);
    let reescritas = membros
        .iter()
        .filter(|membro| membro.operation == Operation::Upsert);
    for membro in exclusoes.chain(reescritas) {
        let agregado = AggregateRef::new(&membro.aggregate_type, &membro.aggregate_id);
        let e_ancora = agregado == divergencia.agregado;
        let estado = sync_codec::ler_canonico(m.tx(), &agregado)?;
        match (membro.operation, estado) {
            (_, Some(estado)) if e_ancora => {
                adotar_revisao(m, &agregado, &membro.new_rev)?;
                m.efeito(
                    agregado,
                    Operation::Upsert,
                    &membro.new_rev,
                    &divergencia.local_rev,
                    &estado.universe_id,
                )?;
            }
            (Operation::Upsert, Some(estado)) if estado.payload == membro.payload => {
                adotar_revisao(m, &agregado, &membro.new_rev)?;
            }
            (Operation::Upsert, Some(_)) | (Operation::Delete, Some(_)) => {
                adotar_revisao(m, &agregado, &membro.new_rev)?;
                m.gravou(&agregado.aggregate_type, &agregado.aggregate_id)?;
            }
            (Operation::Upsert, None) => {
                adotar_revisao(m, &agregado, &membro.new_rev)?;
                m.reafirmou_exclusao(agregado, &membro.universe_id);
            }
            (Operation::Delete, None) => {
                adotar_tombstone(m, &agregado, membro)?;
            }
        }
    }
    Ok(())
}

fn adotar_tombstone(
    m: &Mutacao<'_, '_>,
    agregado: &AggregateRef,
    membro: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    m.tx()
        .execute(
            "INSERT INTO sync_tombstones
                (aggregate_type, aggregate_id, deleted_rev, origin_device_id, origin_seq)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(aggregate_type, aggregate_id)
             DO UPDATE SET deleted_rev = excluded.deleted_rev,
                           origin_device_id = excluded.origin_device_id,
                           origin_seq = excluded.origin_seq",
            rusqlite::params![
                &agregado.aggregate_type,
                &agregado.aggregate_id,
                &membro.new_rev,
                &membro.device_id,
                membro.seq
            ],
        )
        .map_err(erro)?;
    m.tx()
        .execute(
            "DELETE FROM sync_aggregate_state WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
        )
        .map_err(erro)?;
    Ok(())
}

/// A revisão da origem passa a ser a corrente daqui, e o tombstone sai.
fn adotar_revisao(
    m: &Mutacao<'_, '_>,
    agregado: &AggregateRef,
    rev: &str,
) -> DatabaseCommandResult<()> {
    m.tx()
        .execute(
            "INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(aggregate_type, aggregate_id) DO UPDATE SET current_rev = excluded.current_rev",
            rusqlite::params![&agregado.aggregate_type, &agregado.aggregate_id, rev],
        )
        .map_err(erro)?;
    m.tx()
        .execute(
            "DELETE FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
        )
        .map_err(erro)?;
    Ok(())
}

/// "Aceitar" aplica a ação inteira, e só se ela puder entrar inteira AGORA.
///
/// A decisão é marcada resolvida antes dos membros entrarem: enquanto aberta, ela mesma faria o
/// preflight de cada membro recusar a exclusão. Qualquer recusa desfaz a resolução inteira. A
/// âncora sai como efeito com base na exclusão dela: a revisão final é uma só em todo lugar.
fn aceitar_o_grupo(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    membros: &[EventEnvelope],
) -> DatabaseCommandResult<()> {
    let mut sobreviventes_editados = Vec::new();
    for membro in membros {
        exigir_sem_outra_pendencia(
            m,
            &membro.aggregate_type,
            &membro.aggregate_id,
            &divergencia.id,
        )?;
        let agregado = AggregateRef::new(&membro.aggregate_type, &membro.aggregate_id);
        let e_efeito_sobre_sobrevivente = membro.grupo.kind == "delete_tree"
            && membro.operation == Operation::Upsert
            && !(membro.aggregate_type == membro.grupo.root_type
                && membro.aggregate_id == membro.grupo.root_id);
        if e_efeito_sobre_sobrevivente
            && sync_codec::revisao_corrente(m.tx(), &agregado)?.as_deref()
                != Some(membro.base_rev.as_str())
        {
            sobreviventes_editados.push((agregado, membro.new_rev.clone()));
            continue;
        }
        sync_apply::aplicar_membro_decidido(m.tx(), membro)?;
    }
    for (sobrevivente, rev_da_origem) in sobreviventes_editados {
        if sync_codec::ler_canonico(m.tx(), &sobrevivente)?.is_none() {
            continue;
        }
        m.tx()
            .execute(
                "UPDATE sync_aggregate_state SET current_rev = ?3
                  WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                rusqlite::params![
                    &sobrevivente.aggregate_type,
                    &sobrevivente.aggregate_id,
                    &rev_da_origem
                ],
            )
            .map_err(erro)?;
        m.gravou(&sobrevivente.aggregate_type, &sobrevivente.aggregate_id)?;
    }
    let universo = universo_do_conflito(m.tx(), divergencia)?;
    m.efeito(
        divergencia.agregado.clone(),
        Operation::Delete,
        &divergencia.remote_rev,
        &divergencia.local_rev,
        &universo,
    )
}

fn exigir_sem_outra_pendencia(
    m: &Mutacao<'_, '_>,
    tipo: &str,
    id: &str,
    divergencia: &str,
) -> DatabaseCommandResult<()> {
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
            rusqlite::params![tipo, id, divergencia],
            |row| row.get(0),
        )
        .map_err(erro)?;
    if outra_pendencia {
        return Err(DatabaseCommandError::conflict(format!(
            "{tipo} {id} tem outra decisão ou alteração pendente. Resolva isso antes. Nada foi \
             aplicado."
        )));
    }
    Ok(())
}

fn manter_local(m: &mut Mutacao<'_, '_>, divergencia: &Divergencia) -> DatabaseCommandResult<()> {
    let agregado = &divergencia.agregado;
    let Some(estado) = sync_codec::ler_canonico(m.tx(), agregado)? else {
        return Err(DatabaseCommandError::storage(format!(
            "{} {} não existe mais aqui; não há o que manter. Nada foi alterado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    };
    // A restauração parte da revisão da exclusão: quem a recebe sabe que ela viu o delete.
    m.efeito(
        agregado.clone(),
        Operation::Upsert,
        &divergencia.remote_rev,
        &divergencia.local_rev,
        &estado.universe_id,
    )?;
    // Os sobreviventes que a exclusão reescreveria (a ordem do livro, para capítulo) também são
    // declarados: o outro aparelho já os reescreveu sem este agregado.
    for impacto in sync_codec::impactos_da_exclusao(m.tx(), agregado)? {
        if let sync_codec::Impacto::Reescrito(sobrevivente) = impacto {
            if sync_codec::ler_canonico(m.tx(), &sobrevivente)?.is_some() {
                m.gravou(&sobrevivente.aggregate_type, &sobrevivente.aggregate_id)?;
            }
        }
    }
    Ok(())
}

fn aceitar_exclusao(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<()> {
    let agregado = &divergencia.agregado;
    let atual = sync_codec::revisao_corrente(m.tx(), agregado)?;
    if atual.as_deref() != Some(divergencia.local_rev.as_str()) {
        return Err(DatabaseCommandError::conflict(format!(
            "{} {} foi alterado depois que a exclusão chegou. Aceitar agora apagaria essa alteração. \
             Nada foi apagado.",
            agregado.aggregate_type, agregado.aggregate_id
        )));
    }
    exigir_sem_outra_pendencia(
        m,
        &agregado.aggregate_type,
        &agregado.aggregate_id,
        &divergencia.id,
    )?;
    let universo = universo_do_conflito(m.tx(), divergencia)?;
    let sobreviventes: Vec<AggregateRef> = sync_codec::impactos_da_exclusao(m.tx(), agregado)?
        .into_iter()
        .filter_map(|impacto| match impacto {
            sync_codec::Impacto::Reescrito(sobrevivente) => Some(sobrevivente),
            _ => None,
        })
        .collect();
    // O preflight da exclusão roda de novo lá dentro, AGORA.
    sync_apply::aplicar_exclusao_bloqueada(m.tx(), &divergencia.remote_event_id)?;
    for sobrevivente in sobreviventes {
        if sync_codec::ler_canonico(m.tx(), &sobrevivente)?.is_some() {
            m.gravou(&sobrevivente.aggregate_type, &sobrevivente.aggregate_id)?;
        }
    }
    m.efeito(
        agregado.clone(),
        Operation::Delete,
        &divergencia.remote_rev,
        &divergencia.local_rev,
        &universo,
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// tag_name_conflict
// ═══════════════════════════════════════════════════════════════════════════

/// A tag daqui (materializada) e a de lá (esperando o nome ficar livre).
struct DuasTags {
    daqui: ConflictParticipant,
    de_la: ConflictParticipant,
}

fn duas_tags(m: &Mutacao<'_, '_>, divergencia: &Divergencia) -> DatabaseCommandResult<DuasTags> {
    let daqui = divergencia.local().clone();
    let de_la = divergencia.remoto().clone();
    let tag_daqui = AggregateRef::new("content_tag", &daqui.aggregate_id);
    exigir_cabeca_do_conflito(m, &tag_daqui, &daqui.revision)?;
    if sync_codec::ler_canonico(
        m.tx(),
        &AggregateRef::new("content_tag", &de_la.aggregate_id),
    )?
    .is_some()
    {
        return Err(DatabaseCommandError::storage(
            "As duas tags já existem aqui; o conflito de nome não é mais este. Nada foi alterado.",
        ));
    }
    Ok(DuasTags { daqui, de_la })
}

/// Insere a tag de lá no domínio a partir do evento dela, com o nome dado (ou o dela).
fn materializar_tag(
    m: &Mutacao<'_, '_>,
    participante: &ConflictParticipant,
    nome: Option<&str>,
) -> DatabaseCommandResult<String> {
    let agregado = AggregateRef::new("content_tag", &participante.aggregate_id);
    let mut evento =
        evento_da_revisao(m.tx(), &agregado, &participante.revision)?.ok_or_else(|| {
            DatabaseCommandError::storage("A tag do outro aparelho não está neste aparelho.")
        })?;
    if let Some(nome) = nome {
        let mut tag: sync_codec::conhecimento::TagCanonica = serde_json::from_str(&evento.payload)
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        tag.name = nome.to_string();
        evento.payload = serde_json::to_string(&tag)
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    }
    sync_codec::aplicar(m.tx(), &evento).map_err(|erro| {
        DatabaseCommandError::conflict(format!(
            "A tag não pôde ser criada com esse nome: {}. Nada foi alterado.",
            erro.message
        ))
    })?;
    Ok(evento.universe_id)
}

fn renomear_tag(
    m: &mut Mutacao<'_, '_>,
    divergencia: &Divergencia,
    tag_id: &str,
    nome: &str,
) -> DatabaseCommandResult<()> {
    let nome = nome.trim();
    if nome.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O nome novo não pode ser vazio.",
        ));
    }
    let tags = duas_tags(m, divergencia)?;
    let escolha_de = |p: &ConflictParticipant| {
        if p == &divergencia.a {
            "rename:a"
        } else {
            "rename:b"
        }
    };
    if tag_id == tags.daqui.aggregate_id {
        declarar(m, divergencia, escolha_de(&tags.daqui), "manual")?;
        let alterada = m
            .tx()
            .execute(
                "UPDATE content_tags SET name = ?1 WHERE id = ?2",
                [nome, tag_id],
            )
            .map_err(|_| {
                DatabaseCommandError::conflict(
                    "Já existe outra tag com esse nome neste universo. Nada foi alterado.",
                )
            })?;
        if alterada != 1 {
            return Err(DatabaseCommandError::storage(
                "A tag daqui sumiu. Nada foi alterado.",
            ));
        }
        let universo = sync_codec::ler_canonico(m.tx(), &AggregateRef::new("content_tag", tag_id))?
            .map(|e| e.universe_id)
            .unwrap_or_default();
        m.efeito(
            AggregateRef::new("content_tag", tag_id),
            Operation::Upsert,
            &tags.daqui.revision,
            "",
            &universo,
        )?;
        // O nome ficou livre: a tag de lá entra, com o nome dela.
        let universo = materializar_tag(m, &tags.de_la, None)?;
        m.efeito(
            AggregateRef::new("content_tag", &tags.de_la.aggregate_id),
            Operation::Upsert,
            &tags.de_la.revision,
            "",
            &universo,
        )
    } else if tag_id == tags.de_la.aggregate_id {
        declarar(m, divergencia, escolha_de(&tags.de_la), "manual")?;
        let universo = materializar_tag(m, &tags.de_la, Some(nome))?;
        m.efeito(
            AggregateRef::new("content_tag", &tags.de_la.aggregate_id),
            Operation::Upsert,
            &tags.de_la.revision,
            "",
            &universo,
        )
    } else {
        Err(DatabaseCommandError::validation(
            "Essa tag não é nenhuma das duas deste conflito. Nada foi alterado.",
        ))
    }
}

/// **Mescla**: fica a tag de `participant_a`; as marcações da outra — inclusive as que ainda
/// esperavam a tag existir — passam para ela, e a outra é excluída.
fn mesclar_tags(m: &mut Mutacao<'_, '_>, divergencia: &Divergencia) -> DatabaseCommandResult<()> {
    let tags = duas_tags(m, divergencia)?;
    let sobrevivente = divergencia.a.clone();
    let absorvida = divergencia.b.clone();
    declarar(m, divergencia, "merge", "manual")?;
    let tag_absorvida = AggregateRef::new("content_tag", &absorvida.aggregate_id);
    let tag_sobrevivente = AggregateRef::new("content_tag", &sobrevivente.aggregate_id);
    let absorvida_existe_aqui = absorvida.aggregate_id == tags.daqui.aggregate_id;

    // 1. As marcações da absorvida, e com elas os donos que passam para a sobrevivente.
    let mut donos: Vec<(String, String)> = Vec::new();
    if absorvida_existe_aqui {
        let mut consulta = m
            .tx()
            .prepare("SELECT owner_type, owner_id FROM content_tag_assignments WHERE tag_id = ?1 ORDER BY owner_type, owner_id")
            .map_err(erro)?;
        let linhas = consulta
            .query_map([&absorvida.aggregate_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(erro)?;
        donos = linhas.collect::<Result<_, _>>().map_err(erro)?;
        drop(consulta);
        // 2. A absorvida sai pelo preflight normal (as marcações saem antes dela, como filhas).
        m.excluir_como_efeito(
            "content_tag",
            &absorvida.aggregate_id,
            &absorvida.revision,
            "",
        )?;
        apagar_do_dominio(m, &tag_absorvida)?;
    } else {
        // As marcações da absorvida estão aqui só como eventos pendentes, esperando a tag existir.
        // Passam a ser conhecidas — superadas por esta decisão — e saem com base nelas.
        for pendente in marcacoes_pendentes(m, &absorvida.aggregate_id)? {
            let atribuicao: sync_codec::manuscrito::AtribuicaoDeTag =
                serde_json::from_str(&pendente.payload)
                    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            donos.push((atribuicao.owner_type, atribuicao.owner_id));
            sync_apply::superar_evento_pendente(m.tx(), &pendente)?;
            m.efeito(
                AggregateRef::new(&pendente.aggregate_type, &pendente.aggregate_id),
                Operation::Delete,
                &pendente.new_rev,
                "",
                &pendente.universe_id,
            )?;
        }
        let universo = universo_de(m, &tag_absorvida, &absorvida.revision)?;
        m.efeito(
            tag_absorvida,
            Operation::Delete,
            &absorvida.revision,
            "",
            &universo,
        )?;
    }

    // 3. A sobrevivente: materializada aqui se ainda não estava (o nome ficou livre).
    let universo = if sync_codec::ler_canonico(m.tx(), &tag_sobrevivente)?.is_none() {
        materializar_tag(m, &sobrevivente, None)?
    } else {
        universo_de(m, &tag_sobrevivente, &sobrevivente.revision)?
    };
    m.efeito(
        tag_sobrevivente,
        Operation::Upsert,
        &sobrevivente.revision,
        "",
        &universo,
    )?;

    // 4. As marcações passam para a sobrevivente.
    donos.sort();
    donos.dedup();
    for (tipo, dono) in donos {
        let id = sync_codec::manuscrito::id_da_atribuicao(&sobrevivente.aggregate_id, &tipo, &dono);
        let ja: bool = m
            .tx()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM content_tag_assignments
                                WHERE tag_id = ?1 AND owner_type = ?2 AND owner_id = ?3)",
                [&sobrevivente.aggregate_id, &tipo, &dono],
                |row| row.get(0),
            )
            .map_err(erro)?;
        if ja {
            continue;
        }
        let envelope = EventEnvelope {
            event_id: String::new(),
            device_id: String::new(),
            seq: 0,
            universe_id: universo.clone(),
            aggregate_type: "tag_assignment".into(),
            aggregate_id: id.clone(),
            operation: Operation::Upsert,
            payload: serde_json::to_string(&sync_codec::manuscrito::AtribuicaoDeTag {
                tag_id: sobrevivente.aggregate_id.clone(),
                owner_type: tipo.clone(),
                owner_id: dono.clone(),
            })
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?,
            base_rev: String::new(),
            new_rev: String::new(),
            signature: String::new(),
            grupo: Default::default(),
        };
        if sync_codec::dependencias(m.tx(), &envelope)?.is_some() {
            return Err(DatabaseCommandError::conflict(format!(
                "O conteúdo marcado ({tipo} {dono}) ainda não chegou a este aparelho. Sincronize \
                 e mescle de novo; nada foi alterado."
            )));
        }
        sync_codec::aplicar(m.tx(), &envelope)?;
        m.gravou("tag_assignment", &id)?;
    }
    Ok(())
}

/// As marcações de uma tag que chegaram e esperam a tag existir, a mais recente de cada uma.
fn marcacoes_pendentes(
    m: &Mutacao<'_, '_>,
    tag_id: &str,
) -> DatabaseCommandResult<Vec<EventEnvelope>> {
    let prefixo = format!("{tag_id}:");
    let mut consulta = m
        .tx()
        .prepare(&format!(
            "SELECT {} FROM sync_events e
              WHERE e.aggregate_type = 'tag_assignment' AND e.operation = 'upsert'
                AND substr(e.aggregate_id, 1, length(?1)) = ?1
                AND NOT EXISTS (SELECT 1 FROM sync_applied_events a WHERE a.event_id = e.event_id)
              ORDER BY e.aggregate_id, e.rowid",
            sync_repository::colunas_do_envelope("e.")
        ))
        .map_err(erro)?;
    let todos = consulta
        .query_map([&prefixo], sync_repository::envelope_da_linha)
        .map_err(erro)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(erro)?;
    let mut ultimos: Vec<EventEnvelope> = Vec::new();
    for evento in todos {
        match ultimos.last_mut() {
            Some(ultimo) if ultimo.aggregate_id == evento.aggregate_id => *ultimo = evento,
            _ => ultimos.push(evento),
        }
    }
    Ok(ultimos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::canvas_service;
    use crate::application::mutacao::tests::{
        exclusao_remota_do_capitulo, exclusao_remota_do_capitulo_no_grupo, Aparelho,
    };
    use crate::domain::identity::DeviceIdentity;
    use crate::domain::sync::{EventEnvelope, Operation};
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::sync_session::{
        receber_eventos_sem_conferir_blobs as receber_eventos, Relatorio,
    };
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
            // A origem exclui a posição do capítulo e o capítulo, como UMA ação (B2.2).
            let [posicao, exclusao] = exclusao_remota_do_capitulo(&outra, &rev_conhecida);
            let cenario = Self {
                aparelho,
                outra,
                exclusao,
                rev_conhecida,
            };
            let relatorio = cenario.entregar(&[posicao, cenario.exclusao.clone()]);
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
            2,
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
        // Nada mais da origem espera: a exclusão bloqueada é decisão registrada, não pendência.
        assert_eq!(cenario.cursor(), 2);
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
            eventos_antes + 3,
            "a decisão (etapa F), a restauração do capítulo e a ordem daqui, que o cita"
        );
        assert_eq!(
            eventos[eventos_antes].0, "conflict_resolution",
            "a decisão vem primeiro no grupo"
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
        // A raiz é restaurada antes do que depende dela: a posição de c1 não materializa sem c1, e
        // o receptor seguraria a ação inteira esperando uma dependência que vem dentro dela.
        let restauracao: Vec<String> = {
            let connection = cenario.aparelho.conexao();
            let mut consulta = connection
                .prepare(
                    "SELECT aggregate_type FROM sync_events WHERE device_id = ?1
                      ORDER BY seq DESC LIMIT 2",
                )
                .expect("consulta");
            let mut tipos: Vec<String> = consulta
                .query_map([cenario.aparelho.eu.device_id()], |row| row.get(0))
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("tipos");
            tipos.reverse();
            tipos
        };
        assert_eq!(restauracao, vec!["chapter", "chapter_position"]);

        // A exclusão chegando de novo não reabre nada.
        let relatorio = cenario.entregar(std::slice::from_ref(&cenario.exclusao));
        // Nada reabre. Até a B2.2 havia aqui uma ordem inteira do livro, sem c1, que ficava pendente e
        // virava decisão ao ser retransmitida; com posição por item esse evento não existe, e a ação
        // já decidida não produz decisão nenhuma. Nenhuma divergência nova no capítulo.
        assert_eq!(relatorio.divergencias, 0, "{relatorio:?}");
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

    /// **O grupo é `(origem, mutation_id)`.** Outra origem com o mesmo `mutation_id` é outro grupo:
    /// a decisão sobre a ação da primeira não captura os membros da segunda.
    #[test]
    fn mesmo_mutation_id_em_duas_origens_sao_dois_grupos() {
        let cenario = Cenario::bloqueado();
        let terceira = {
            let connection = cenario.aparelho.banco.database.write().expect("escrita");
            origem_remota_confiavel(&connection, &cenario.aparelho.eu)
        };
        let mutation_id = cenario.exclusao.grupo.mutation_id.clone();
        let da_terceira =
            exclusao_remota_do_capitulo_no_grupo(&terceira, &cenario.rev_conhecida, &mutation_id);
        let relatorio = cenario.entregar(&da_terceira);
        assert_eq!(relatorio.divergencias, 0, "{relatorio:?}");
        let com_o_mesmo_id: i64 = cenario
            .aparelho
            .conexao()
            .query_row(
                "SELECT COUNT(*) FROM sync_events WHERE mutation_id = ?1",
                [&mutation_id],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(
            com_o_mesmo_id, 4,
            "as duas ações estão no log com o mesmo id"
        );

        canvas_service::delete_attachment(
            &cenario.aparelho.banco.database,
            &cenario.aparelho.eu,
            "a-concorrente",
        )
        .expect("excluir o anexo");
        assert_eq!(
            cenario
                .resolver(Escolha::AceitarRemoto)
                .expect("aceitar aplica só a ação da origem da decisão"),
            Resolucao::ExclusaoConcluida
        );
        assert!(!cenario.aparelho.existe("chapters", "c1"));
        cenario.aparelho.coerente("chapter", "c1");
        cenario.aparelho.coerente("chapter_position", "c1");
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
        // Etapa F: a exclusão aceita termina numa revisão final que DESCENDE da remota — a mesma
        // em todos os aparelhos que receberem a decisão.
        let base_do_final: String = cenario
            .aparelho
            .conexao()
            .query_row(
                "SELECT base_rev FROM sync_events WHERE new_rev = ?1",
                [&tombstone],
                |row| row.get(0),
            )
            .expect("o evento da exclusão final");
        assert_eq!(
            base_do_final, cenario.exclusao.new_rev,
            "a exclusão final não parte da exclusão remota"
        );

        // Nenhum irmão esperava c1 sair: a posição é por item, e o cursor já tinha passado.
        cenario.entregar(&[]);
        assert_eq!(cenario.cursor(), 2);
        cenario.aparelho.coerente("chapter_position", "c1");
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
        let [posicao, exclusao] = exclusao_remota_do_capitulo(&outra, &rev);
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
        receber_eventos(&mut connection, &[posicao, exclusao]).expect("exclusão");
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
