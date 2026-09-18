//! Sync V2 — ordem, lacunas e avanço do cursor (ADR 0009 §13, etapa 5).
//!
//! A etapa 4 sabe aplicar **um** evento. Esta sabe em que ordem, o que fazer
//! com o que chegou adiantado, e quando o cursor pode andar.
//!
//! ## O cursor é a maior sequência CONTÍGUA aplicada
//!
//! ```text
//! cursor da origem = 100
//! chega seq 102   (o 101 ainda não chegou)
//!
//! ERRADO:   cursor = 102     →  o 101 nunca mais é pedido. Perda silenciosa.
//! CERTO:    cursor = 100, e o 102 fica PENDENTE
//!           chega o 101  →  aplica 101, aplica 102  →  cursor = 102
//! ```
//!
//! ## Pendente não é tabela
//!
//! Pendente é **estar no log e não estar em `sync_applied_events`**. Uma
//! tabela separada teria que ser mantida em sincronia com o log, e
//! sincronizar duas fontes da mesma verdade é como se perde a verdade.
//!
//! E é por isso que o pendente sobrevive ao processo de graça: ele mora em
//! disco desde o instante em que o envelope foi guardado. No Android, onde o
//! sistema mata o aplicativo sem avisar, um buffer em memória perderia
//! exatamente o que estava esperando a lacuna fechar.
//!
//! ## O que trava o avanço além da lacuna
//!
//! Um evento pode ser contíguo por `seq` e ainda assim não aplicável: se o
//! `base_rev` dele for desconhecido, aplicá-lo escreveria em cima de uma
//! história que não temos. Nesse caso o cursor **para**, e o relatório diz
//! qual agregado precisa de reconciliação — em vez de fingir progresso.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::sync::EventEnvelope;
use crate::infrastructure::sqlite::sync_apply::{
    apply_remote_event, registrar_decisao_do_grupo, Applied,
};
use crate::infrastructure::sqlite::sync_trust::{verificar_origem, Recusa};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::collections::BTreeMap;

/// O que a sessão fez com o que recebeu.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Relatorio {
    /// Eventos aplicados nesta sessão, inclusive pendentes antigos que
    /// puderam entrar porque a lacuna fechou.
    pub aplicados: usize,
    /// Eventos guardados e ainda não aplicáveis.
    pub pendentes: usize,
    /// Divergências abertas — o escritor vai precisar decidir.
    pub divergencias: usize,
    /// Agregados cuja história não conhecemos. Não é conflito: falta o meio.
    pub precisam_reconciliar: Vec<String>,
    /// Envelopes barrados na cadeia de confiança da etapa 7. **Nenhum deles
    /// entrou no log**: um evento que não passou pela verificação não pode ser
    /// retransmitido nem aplicado, e guardá-lo daria a ele a aparência de
    /// legítimo na próxima sessão.
    ///
    /// Recusas descartáveis entram aqui **até um teto** — ver
    /// `TETO_DE_DESCARTES_REGISTRADOS`.
    pub recusados: Vec<Recusa>,
    /// Quantos envelopes foram descartados por origem desconhecida, contando
    /// inclusive os que não couberam em `recusados`.
    descartes: usize,
}

/// Quantas recusas descartáveis a sessão registra antes de parar de anotar.
///
/// O que importa saber é "aconteceu, e nesta ordem de grandeza" — não a lista
/// inteira. Guardar todas deixaria um peer hostil escolher quanta memória
/// desta sessão ele ocupa.
const TETO_DE_DESCARTES_REGISTRADOS: usize = 16;

impl Relatorio {
    /// Recusas que merecem atenção. **Não inclui origem desconhecida.**
    ///
    /// Um envelope solto de origem desconhecida é descartado sem virar tela
    /// (NH-057). Pedido de entrada visível nasce do pareamento autenticado, em
    /// `sync_pairing::PedidoDeEntrada`, onde houve intenção humana dos dois
    /// lados.
    pub fn incidentes(&self) -> Vec<&Recusa> {
        self.recusados
            .iter()
            .filter(|recusa| recusa.e_incidente())
            .collect()
    }

    /// Quantos envelopes foram descartados por origem desconhecida.
    ///
    /// Um número, não uma lista de aparelhos para o escritor conferir. Serve
    /// para diagnóstico e para a decisão de cortar a sessão — não para a tela.
    pub fn descartados(&self) -> usize {
        self.descartes
    }
}

/// Recebe um lote de eventos e avança o que der.
///
/// Tudo numa transação: os quatro efeitos da seção 12 do ADR precisam
/// acontecer juntos, e o cursor é o quarto.
pub fn receber_eventos(
    connection: &mut Connection,
    envelopes: &[EventEnvelope],
) -> DatabaseCommandResult<Relatorio> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // Guarda tudo primeiro, sem aplicar nada. Um evento adiantado precisa
    // estar no log antes de a lacuna fechar — senão ele seria descartado e
    // pedido de novo, e "de novo" pode ser daqui a semanas.
    let mut relatorio = Relatorio::default();
    let mut origens: BTreeMap<String, ()> = BTreeMap::new();
    for envelope in envelopes {
        // A verificação vem ANTES de guardar. Um envelope que não passou pela
        // cadeia de confiança não entra no log — se entrasse, o relay o
        // repassaria adiante e a próxima sessão o encontraria já lá dentro,
        // com aparência de legítimo.
        match verificar_origem(&tx, envelope)? {
            Ok(()) => {}
            Err(recusa) => {
                if recusa.e_descartavel() {
                    // Descarta, conta, e não deixa um peer hostil escolher
                    // quanta memória desta sessão ele ocupa.
                    relatorio.descartes += 1;
                    if relatorio.recusados.len() < TETO_DE_DESCARTES_REGISTRADOS {
                        relatorio.recusados.push(recusa);
                    }
                } else {
                    relatorio.recusados.push(recusa);
                }
                continue;
            }
        }
        // A forma do grupo é conferida antes de o evento entrar no log: um `mutation_count`
        // absurdo nunca vira grupo guardado, nem chega a dimensionar nada. A assinatura é válida
        // (é a origem confiável que emitiu algo que nenhum emissor legítimo produz), e a sessão
        // inteira falha fechada, sem guardar nada.
        if let Err(motivo) = envelope.grupo.validar() {
            return Err(DatabaseCommandError::storage(format!(
                "O evento {} da origem {} tem um grupo de mutação inválido: {motivo}. Nada desta \
                 sessão foi guardado.",
                envelope.seq, envelope.device_id
            )));
        }
        guardar(&tx, envelope)?;
        origens.insert(envelope.device_id.clone(), ());
    }

    // E também as origens que já tinham pendentes de sessões anteriores: a
    // lacuna pode ter fechado com o que acabou de chegar.
    for origem in origens_com_pendentes(&tx)? {
        origens.insert(origem, ());
    }

    // Até não haver progresso. Uma origem pode depender de outra (a ordem de A cita um capítulo
    // que é de C): drenar cada uma uma vez, na ordem das chaves, deixaria A pendente se ela viesse
    // antes de C. Cada volta só termina quando nenhuma origem aplicou nada; como cada evento é
    // aplicado no máximo uma vez, isto termina.
    let mut tocados: BTreeMap<(String, String), ()> = BTreeMap::new();
    loop {
        let aplicados_antes = contar_aplicados(&tx)?;
        relatorio.precisam_reconciliar.clear();
        for origem in origens.keys() {
            drenar_origem(&tx, origem, &mut relatorio, &mut tocados)?;
        }
        if contar_aplicados(&tx)? == aplicados_antes {
            break;
        }
    }
    relatorio.pendentes = contar_pendentes(&tx)?;

    // Nenhuma revisão corrente é confirmada sem estar no banco (ver `atravessar_ponte`).
    for (tipo, id) in tocados.keys() {
        crate::infrastructure::sqlite::sync_apply::conferir_revisao_corrente(
            &tx,
            &crate::domain::sync::AggregateRef::new(tipo, id),
        )?;
    }

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(relatorio)
}

fn contar_aplicados(tx: &Transaction<'_>) -> DatabaseCommandResult<i64> {
    tx.query_row("SELECT COUNT(*) FROM sync_applied_events", [], |row| {
        row.get(0)
    })
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

/// Aplica, em ordem, tudo o que estiver contíguo a partir do cursor.
fn drenar_origem(
    tx: &Transaction<'_>,
    origem: &str,
    relatorio: &mut Relatorio,
    tocados: &mut BTreeMap<(String, String), ()>,
) -> DatabaseCommandResult<()> {
    let (baseline, mut cursor) = cursor_de(tx, origem)?;

    loop {
        let proximo = cursor + 1;
        let Some(envelope) = evento_de(tx, origem, proximo)? else {
            // A lacuna não fechou. O que estiver acima fica pendente, e a
            // sessão seguinte pede a partir daqui de novo. Retransmitir é
            // barato; perder não é.
            break;
        };

        // Uma ação de vários eventos entra inteira ou não entra (B2.2).
        if !envelope.grupo.e_isolado() {
            match aplicar_grupo(tx, origem, &envelope, relatorio, tocados)? {
                Grupo::Entrou(total) | Grupo::VirouDecisao(total) => {
                    cursor = proximo + total - 1;
                    gravar_cursor(tx, origem, baseline, cursor)?;
                    continue;
                }
                Grupo::Espera => break,
            }
        }

        let resultado = apply_remote_event(tx, &envelope)?;
        if matches!(resultado, Applied::Aplicado) {
            tocados.insert(
                (
                    envelope.aggregate_type.clone(),
                    envelope.aggregate_id.clone(),
                ),
                (),
            );
        }
        match resultado {
            Applied::Aplicado => relatorio.aplicados += 1,
            Applied::JaAplicado => {}
            Applied::Divergente { .. } => relatorio.divergencias += 1,
            // A exclusão bloqueada é decisão pendente como qualquer divergência: conta junto.
            Applied::ExclusaoDoPaiBloqueada { .. } => relatorio.divergencias += 1,
            // Tag homônima: nada foi aplicado, e o escritor decide. Também é decisão pendente.
            Applied::ConflitoDeNomeDeTag { .. } => relatorio.divergencias += 1,
            // Exclusão contra edição também é divergência — e é a que mais
            // assusta o escritor, porque um dos lados é "isto sumiu".
            Applied::DivergenteComExclusao { .. } => relatorio.divergencias += 1,
            Applied::PrecisaReconciliar => {
                // Contíguo por `seq` e sem história para se apoiar. Parar aqui
                // é o certo: aplicar escreveria sobre uma história que não
                // temos, e pular deixaria o cursor mentir.
                relatorio
                    .precisam_reconciliar
                    .push(envelope.aggregate_id.clone());
                break;
            }
        }

        cursor = proximo;
        gravar_cursor(tx, origem, baseline, cursor)?;
    }

    Ok(())
}

/// O que aconteceu com um grupo de mutação.
enum Grupo {
    /// Todos os membros entraram. Carrega o total.
    Entrou(i64),
    /// Nenhum entrou, e a ação virou uma decisão. O cursor anda: os membros estão no log e na
    /// história, e a decisão está registrada.
    VirouDecisao(i64),
    /// Falta membro, ou um membro depende de algo que não chegou. Nada entrou, e o cursor espera.
    Espera,
}

/// **Aplica uma ação inteira, ou nada dela.**
///
/// ```text
/// grupo incompleto                        → não aplica nada, o cursor espera
/// grupo completo, num SAVEPOINT, membro a membro:
///   todos aplicados                       → confirma o grupo
///   um membro espera dependência          → desfaz tudo, o grupo espera
///   um membro diverge ou é bloqueado      → desfaz tudo, UMA decisão sobre a ação
///   erro                                  → a sessão inteira falha fechada
/// ```
///
/// Os membros são aplicados em ordem dentro do savepoint porque um depende do anterior: a exclusão
/// da posição de um capítulo precisa já ter entrado quando a do capítulo confere se sobrou
/// descendente vivo. Desfazer o savepoint devolve o domínio exatamente ao que era.
fn aplicar_grupo(
    tx: &Transaction<'_>,
    origem: &str,
    primeiro: &EventEnvelope,
    relatorio: &mut Relatorio,
    tocados: &mut BTreeMap<(String, String), ()>,
) -> DatabaseCommandResult<Grupo> {
    let grupo = &primeiro.grupo;
    if grupo.index != 0 {
        return Err(DatabaseCommandError::storage(format!(
            "O evento {} da origem {origem} é o membro {} de um grupo de mutação, e o cursor chegou \
             nele sem passar pelo primeiro. O log está incoerente; nada foi aplicado.",
            primeiro.seq, grupo.index
        )));
    }

    // O log já só guarda grupo válido, mas nada aqui confia nisso: o total é conferido, convertido
    // sem truncar e somado à seq sem estourar, ANTES de dimensionar ou iterar.
    let invalido = |motivo: String| {
        DatabaseCommandError::storage(format!(
            "O evento {} da origem {origem} tem um grupo de mutação inválido: {motivo}. Nada foi \
             aplicado.",
            primeiro.seq
        ))
    };
    grupo.validar().map_err(invalido)?;
    let total = usize::try_from(grupo.count)
        .map_err(|_| invalido(format!("total {} não cabe na memória", grupo.count)))?;
    primeiro
        .seq
        .checked_add(grupo.count - 1)
        .ok_or_else(|| invalido("a última seq do grupo estoura".into()))?;

    let mut membros = Vec::with_capacity(total);
    for deslocamento in 0..grupo.count {
        let Some(membro) = evento_de(tx, origem, primeiro.seq + deslocamento)? else {
            // O resto da ação ainda não chegou. Nenhum membro altera o domínio antes disso — nem
            // os que já estão aqui.
            return Ok(Grupo::Espera);
        };
        let coerente = membro.grupo.mutation_id == grupo.mutation_id
            && membro.grupo.index == deslocamento
            && membro.grupo.count == grupo.count
            && membro.grupo.kind == grupo.kind
            && membro.grupo.root_type == grupo.root_type
            && membro.grupo.root_id == grupo.root_id;
        if !coerente {
            return Err(DatabaseCommandError::storage(format!(
                "O evento {} da origem {origem} deveria ser o membro {deslocamento} do grupo {} e \
                 não é. O grupo está incoerente; nada foi aplicado.",
                membro.seq, grupo.mutation_id
            )));
        }
        membros.push(membro);
    }

    tx.execute_batch("SAVEPOINT grupo_de_mutacao")
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let mut aplicados = 0usize;
    let mut recusa: Option<(usize, Applied)> = None;
    for (indice, membro) in membros.iter().enumerate() {
        let resultado = apply_remote_event(tx, membro)?;
        match resultado {
            Applied::Aplicado => aplicados += 1,
            Applied::JaAplicado => {}
            outro => {
                recusa = Some((indice, outro));
                break;
            }
        }
        falha_de_grupo::verificar(indice)?;
    }

    let Some((ancora, resultado)) = recusa else {
        tx.execute_batch("RELEASE grupo_de_mutacao")
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        relatorio.aplicados += aplicados;
        for membro in &membros {
            tocados.insert(
                (membro.aggregate_type.clone(), membro.aggregate_id.clone()),
                (),
            );
        }
        return Ok(Grupo::Entrou(grupo.count));
    };

    tx.execute_batch("ROLLBACK TO grupo_de_mutacao; RELEASE grupo_de_mutacao")
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if matches!(resultado, Applied::PrecisaReconciliar) {
        relatorio
            .precisam_reconciliar
            .push(membros[ancora].aggregate_id.clone());
        return Ok(Grupo::Espera);
    }
    // A decisão é da AÇÃO: numa exclusão composta, o escritor decide sobre a raiz, não sobre o
    // efeito que tropeçou (o card reescrito, a posição do capítulo).
    let raiz = membros.iter().position(|membro| {
        grupo.kind == "delete_tree"
            && membro.operation == crate::domain::sync::Operation::Delete
            && membro.aggregate_type == grupo.root_type
            && membro.aggregate_id == grupo.root_id
    });
    let (ancora, resultado) = match raiz {
        Some(raiz) if raiz != ancora => (
            raiz,
            Applied::ExclusaoDoPaiBloqueada {
                id_divergencia: String::new(),
            },
        ),
        _ => (ancora, resultado),
    };
    registrar_decisao_do_grupo(tx, &membros, ancora, &resultado)?;
    relatorio.divergencias += 1;
    Ok(Grupo::VirouDecisao(grupo.count))
}

/// Falha injetada no meio de um grupo, só em teste: prova que o savepoint não deixa membro aplicado.
pub(crate) mod falha_de_grupo {
    use crate::database::error::DatabaseCommandResult;

    #[cfg(test)]
    thread_local! {
        static DEPOIS_DO_MEMBRO: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub fn armar(membro: Option<usize>) {
        DEPOIS_DO_MEMBRO.with(|armada| armada.set(membro));
    }

    #[cfg(test)]
    pub fn verificar(membro: usize) -> DatabaseCommandResult<()> {
        if DEPOIS_DO_MEMBRO.with(|armada| armada.get()) == Some(membro) {
            armar(None);
            return Err(crate::database::error::DatabaseCommandError::storage(
                format!("falha injetada depois do membro {membro} do grupo"),
            ));
        }
        Ok(())
    }

    #[cfg(not(test))]
    #[inline(always)]
    pub fn verificar(_membro: usize) -> DatabaseCommandResult<()> {
        Ok(())
    }
}

fn cursor_de(tx: &Transaction<'_>, origem: &str) -> DatabaseCommandResult<(i64, i64)> {
    let existente: Option<(i64, i64)> = tx
        .query_row(
            "SELECT baseline_seq, last_seq_applied FROM sync_cursors WHERE origin_device_id = ?1",
            [origem],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(existente.unwrap_or((0, 0)))
}

fn gravar_cursor(
    tx: &Transaction<'_>,
    origem: &str,
    baseline: i64,
    cursor: i64,
) -> DatabaseCommandResult<()> {
    // UPDATE primeiro pelo mesmo motivo do `append_event_in_transaction`: o gatilho `BEFORE
    // INSERT` de contiguidade conta desde o baseline, e pagar isso a cada avanço de cursor é O(n)
    // por evento aplicado.
    let atualizadas = tx
        .execute(
            "UPDATE sync_cursors SET last_seq_applied = ?2 WHERE origin_device_id = ?1",
            rusqlite::params![origem, cursor],
        )
        .map_err(|error| {
            DatabaseCommandError::storage(format!(
                "O banco recusou o avanço do cursor da origem {origem} para {cursor}. O trigger \
                 de contiguidade da migration 16 é o que cobra isso: {error}"
            ))
        })?;
    if atualizadas > 0 {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO sync_cursors (origin_device_id, baseline_seq, last_seq_applied)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![origem, baseline, cursor],
    )
    .map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O banco recusou o avanço do cursor da origem {origem} para {cursor}. \
             O trigger de contiguidade da migration 16 é o que cobra isso: {error}"
        ))
    })?;
    Ok(())
}

fn evento_de(
    tx: &Transaction<'_>,
    origem: &str,
    seq: i64,
) -> DatabaseCommandResult<Option<EventEnvelope>> {
    tx.query_row(
        &format!(
            "SELECT {} FROM sync_events WHERE device_id = ?1 AND seq = ?2",
            crate::infrastructure::sqlite::sync_repository::colunas_do_envelope("")
        ),
        rusqlite::params![origem, seq],
        crate::infrastructure::sqlite::sync_repository::envelope_da_linha,
    )
    .optional()
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

fn origens_com_pendentes(tx: &Transaction<'_>) -> DatabaseCommandResult<Vec<String>> {
    let mut statement = tx
        .prepare(
            "SELECT DISTINCT e.device_id
               FROM sync_events e
          LEFT JOIN sync_applied_events a ON a.event_id = e.event_id
              WHERE a.event_id IS NULL",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let origens = statement
        .query_map([], |row| row.get(0))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(origens)
}

fn contar_pendentes(tx: &Transaction<'_>) -> DatabaseCommandResult<usize> {
    let total: i64 = tx
        .query_row(
            "SELECT COUNT(*)
               FROM sync_events e
          LEFT JOIN sync_applied_events a ON a.event_id = e.event_id
              WHERE a.event_id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(total as usize)
}

fn guardar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    crate::infrastructure::sqlite::sync_repository::gravar_envelope(tx, envelope, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::DeviceIdentity;
    use crate::domain::sync::AggregateRef;
    use crate::domain::sync::Operation;
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::test_support::{
        origem_remota_confiavel, seed_universe, self_de_teste, TemporaryDatabase,
    };

    /// O banco pronto, com o `self` e uma origem remota **confiável de
    /// verdade**: chave que deriva o `device_id`, introduzida pelo `self`.
    ///
    /// Desde a etapa 7 não existe atalho: uma origem com chave inventada é
    /// recusada na cadeia de confiança, e com razão.
    fn preparar(fixture: &TemporaryDatabase) -> (Connection, DeviceIdentity, DeviceIdentity) {
        let connection = fixture.database.write().expect("abrir escrita");
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                 INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');",
            )
            .expect("semear");
        let eu = self_de_teste(&connection);
        let remota = origem_remota_confiavel(&connection, &eu);
        (connection, eu, remota)
    }

    fn capitulo(id: &str, titulo: &str) -> String {
        format!(
            r#"{{"id":"{id}","bookId":"b1","title":"{titulo}","content":"texto","summary":"","sceneOrigin":"","sceneDestination":"","status":"rascunho","canonStatus":"canon","customFields":[]}}"#
        )
    }

    /// Uma cadeia de eventos independentes, cada um criando um capítulo,
    /// **assinados pela origem** — como chegariam pela rede.
    fn cadeia(origem: &DeviceIdentity, quantos: i64) -> Vec<EventEnvelope> {
        (1..=quantos)
            .map(|seq| {
                let id = format!("cap-{seq}");
                let mut envelope = envelope_de_origem(
                    origem.device_id(),
                    seq,
                    "u1",
                    &AggregateRef::new("chapter", &id),
                    Operation::Upsert,
                    &capitulo(&id, &format!("Capitulo {seq}")),
                    "",
                );
                envelope.signature = origem.sign(&envelope);
                envelope
            })
            .collect()
    }

    fn cursor(connection: &Connection, origem: &str) -> i64 {
        connection
            .query_row(
                "SELECT last_seq_applied FROM sync_cursors WHERE origin_device_id = ?1",
                [origem],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }

    #[test]
    fn lote_em_ordem_aplica_tudo_e_avanca_o_cursor() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);

        let relatorio = receber_eventos(&mut connection, &cadeia(&remota, 3)).expect("receber");
        assert_eq!(relatorio.aplicados, 3);
        assert_eq!(relatorio.pendentes, 0);
        assert_eq!(cursor(&connection, remota.device_id()), 3);
    }

    /// GATE DA ETAPA 5: o cursor não atravessa a lacuna, e o adiantado espera.
    #[test]
    fn evento_adiantado_fica_pendente_e_o_cursor_para_antes_da_lacuna() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);

        let todos = cadeia(&remota, 3);
        // Chegam o 1 e o 3. O 2 se perdeu no caminho.
        let sem_o_dois = vec![todos[0].clone(), todos[2].clone()];

        let relatorio = receber_eventos(&mut connection, &sem_o_dois).expect("receber");
        assert_eq!(relatorio.aplicados, 1, "só o seq 1 podia entrar");
        assert_eq!(relatorio.pendentes, 1, "o seq 3 tem que ficar guardado");
        assert_eq!(
            cursor(&connection, remota.device_id()),
            1,
            "avançar até 3 faria o 2 nunca mais ser pedido"
        );

        // O capítulo 3 não pode ter aparecido no acervo.
        let existe: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM chapters WHERE id = 'cap-3'",
                [],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(existe, 0, "aplicou um evento fora de ordem");

        // Mas o envelope está no log, esperando.
        let guardado: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_events WHERE device_id = ?1 AND seq = 3",
                [remota.device_id()],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(guardado, 1);
    }

    /// E quando a lacuna fecha, os dois consolidam de uma vez.
    #[test]
    fn quando_a_lacuna_fecha_o_pendente_entra_junto() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);

        let todos = cadeia(&remota, 3);
        receber_eventos(&mut connection, &[todos[0].clone(), todos[2].clone()])
            .expect("primeiro lote");
        assert_eq!(cursor(&connection, remota.device_id()), 1);

        // Chega só o que faltava.
        let relatorio =
            receber_eventos(&mut connection, &[todos[1].clone()]).expect("segundo lote");
        assert_eq!(
            relatorio.aplicados, 2,
            "o 2 e o 3 precisam entrar na mesma passada"
        );
        assert_eq!(relatorio.pendentes, 0);
        assert_eq!(cursor(&connection, remota.device_id()), 3);

        for id in ["cap-1", "cap-2", "cap-3"] {
            let existe: i64 = connection
                .query_row("SELECT COUNT(*) FROM chapters WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .expect("contar");
            assert_eq!(existe, 1, "{id} não chegou ao acervo");
        }
    }

    /// O pendente mora em disco, não em memória.
    ///
    /// No Android o sistema mata o aplicativo sem avisar. Um buffer em
    /// memória perderia exatamente o que estava esperando a lacuna fechar — e
    /// como o cursor não avançou, o evento seria pedido de novo; mas o
    /// desperdício não é o ponto: o ponto é que ele **não pode** depender de o
    /// processo continuar vivo.
    #[test]
    fn o_pendente_sobrevive_ao_processo() {
        let fixture = TemporaryDatabase::new();
        let (todos, remota) = {
            let (mut connection, _eu, remota) = preparar(&fixture);
            let todos = cadeia(&remota, 3);
            receber_eventos(&mut connection, &[todos[0].clone(), todos[2].clone()])
                .expect("primeiro lote");
            (todos, remota)
        };

        // Conexão nova, como se o app tivesse sido morto e reaberto.
        let mut connection = fixture.database.write().expect("reabrir");
        let relatorio =
            receber_eventos(&mut connection, &[todos[1].clone()]).expect("depois de reabrir");
        assert_eq!(
            relatorio.aplicados, 2,
            "o pendente não sobreviveu ao fechamento do app"
        );
        assert_eq!(cursor(&connection, remota.device_id()), 3);
    }

    /// Duas origens não interferem uma na outra.
    ///
    /// Uma lacuna no Android não pode travar o que veio do Notebook — é a
    /// razão de o cursor ser um vetor por origem, e não um número só.
    #[test]
    fn a_lacuna_de_uma_origem_nao_trava_a_outra() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, eu, remota) = preparar(&fixture);
        let outra = origem_remota_confiavel(&connection, &eu);

        let da_origem = cadeia(&remota, 3);
        let da_outra: Vec<EventEnvelope> = (1..=2)
            .map(|seq| {
                let id = format!("out-{seq}");
                let mut envelope = envelope_de_origem(
                    outra.device_id(),
                    seq,
                    "u1",
                    &AggregateRef::new("chapter", &id),
                    Operation::Upsert,
                    &capitulo(&id, &format!("Outro {seq}")),
                    "",
                );
                envelope.signature = outra.sign(&envelope);
                envelope
            })
            .collect();

        let mut lote = vec![da_origem[0].clone(), da_origem[2].clone()];
        lote.extend(da_outra.iter().cloned());

        let relatorio = receber_eventos(&mut connection, &lote).expect("receber");
        assert_eq!(relatorio.aplicados, 3, "1 da origem travada + 2 da outra");
        assert_eq!(cursor(&connection, remota.device_id()), 1);
        assert_eq!(cursor(&connection, outra.device_id()), 2);
    }

    /// Contíguo por `seq` e sem história: o cursor para e o relatório diz por
    /// quê, em vez de fingir progresso.
    #[test]
    fn base_desconhecida_trava_o_cursor_e_aparece_no_relatorio() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);

        let mut orfao = envelope_de_origem(
            remota.device_id(),
            1,
            "u1",
            &AggregateRef::new("chapter", "cap-x"),
            Operation::Upsert,
            &capitulo("cap-x", "Orfao"),
            "revisao-que-nunca-vimos",
        );
        orfao.signature = remota.sign(&orfao);

        let relatorio = receber_eventos(&mut connection, &[orfao]).expect("receber");
        assert_eq!(relatorio.aplicados, 0);
        assert_eq!(relatorio.precisam_reconciliar, vec!["cap-x".to_string()]);
        assert_eq!(
            cursor(&connection, remota.device_id()),
            0,
            "o cursor não pode passar por cima do que não conseguiu aplicar"
        );
    }

    /// Receber o mesmo lote duas vezes não muda nada.
    #[test]
    fn reenviar_o_mesmo_lote_e_inofensivo() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);
        let lote = cadeia(&remota, 3);

        receber_eventos(&mut connection, &lote).expect("primeira vez");
        let segunda = receber_eventos(&mut connection, &lote).expect("segunda vez");

        assert_eq!(segunda.aplicados, 0);
        assert_eq!(segunda.pendentes, 0);
        assert_eq!(cursor(&connection, remota.device_id()), 3);

        let capitulos: i64 = connection
            .query_row("SELECT COUNT(*) FROM chapters", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(capitulos, 3);
    }

    /// Lote fora de ordem chega ordenado ao aplicador.
    ///
    /// A rede não garante ordem, e depender dela seria depender de sorte.
    #[test]
    fn lote_embaralhado_e_aplicado_na_ordem_certa() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);

        let todos = cadeia(&remota, 3);
        let embaralhado = vec![todos[2].clone(), todos[0].clone(), todos[1].clone()];

        let relatorio = receber_eventos(&mut connection, &embaralhado).expect("receber");
        assert_eq!(relatorio.aplicados, 3);
        assert_eq!(cursor(&connection, remota.device_id()), 3);
    }

    fn grupo(mutation_id: &str, index: i64, count: i64) -> crate::domain::sync::GrupoDeMutacao {
        crate::domain::sync::GrupoDeMutacao {
            mutation_id: mutation_id.into(),
            index,
            count,
            kind: String::new(),
            root_type: String::new(),
            root_id: String::new(),
        }
    }

    fn eventos_no_log(connection: &Connection, origem: &str) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM sync_events WHERE device_id = ?1",
                [origem],
                |row| row.get(0),
            )
            .expect("contar")
    }

    /// **Grupo de forma absurda é recusado na entrada**, assinado ou não: nada entra no log e nada é
    /// dimensionado pelo `mutation_count` que veio de fora.
    #[test]
    fn grupo_de_forma_absurda_e_recusado_sem_entrar_no_log() {
        use crate::domain::sync::MAXIMO_DE_MEMBROS_DO_GRUPO;
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);
        for (descricao, forma) in [
            ("total máximo de i64", grupo("m", 0, i64::MAX)),
            (
                "um acima do teto",
                grupo("m", 0, MAXIMO_DE_MEMBROS_DO_GRUPO + 1),
            ),
            ("total negativo", grupo("m", 0, -1)),
            ("total zero com id", grupo("m", 0, 0)),
            ("índice igual ao total", grupo("m", 2, 2)),
            ("índice negativo", grupo("m", -1, 2)),
            ("grupo sem id", grupo("", 0, 3)),
            ("id gigante", grupo(&"x".repeat(129), 0, 2)),
        ] {
            let mut evento = cadeia(&remota, 1).remove(0);
            evento.grupo = forma;
            evento.signature = remota.sign(&evento);
            let erro = receber_eventos(&mut connection, std::slice::from_ref(&evento))
                .expect_err(descricao);
            assert!(
                erro.message.contains("grupo de mutação inválido"),
                "{descricao}: {}",
                erro.message
            );
            assert_eq!(
                eventos_no_log(&connection, remota.device_id()),
                0,
                "{descricao}: entrou no log"
            );
        }
        // Contraprova: o mesmo evento com grupo coerente entra.
        let mut evento = cadeia(&remota, 1).remove(0);
        evento.grupo = grupo("m", 0, 1);
        evento.signature = remota.sign(&evento);
        receber_eventos(&mut connection, &[evento]).expect("grupo coerente");
        assert_eq!(eventos_no_log(&connection, remota.device_id()), 1);
    }

    /// Um grupo absurdo que já esteja no log (banco adulterado, versão antiga com bug) não é
    /// iterado nem dimensionado: a drenagem recusa antes.
    #[test]
    fn grupo_absurdo_no_log_nao_e_iterado() {
        let fixture = TemporaryDatabase::new();
        let (mut connection, _eu, remota) = preparar(&fixture);
        let mut evento = cadeia(&remota, 1).remove(0);
        evento.grupo = grupo("m", 0, i64::MAX);
        evento.signature = remota.sign(&evento);
        crate::infrastructure::sqlite::sync_repository::gravar_envelope(
            &connection,
            &evento,
            false,
        )
        .expect("gravar direto, sem a checagem da entrada");

        let erro = receber_eventos(&mut connection, &[]).expect_err("grupo absurdo no log");
        assert!(
            erro.message.contains("grupo de mutação inválido"),
            "{}",
            erro.message
        );
        assert_eq!(cursor(&connection, remota.device_id()), 0);
    }
}
