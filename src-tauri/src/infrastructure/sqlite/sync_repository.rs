//! Sync V2 — leitura e escrita do log de eventos (ADR 0009, etapa 2).
//!
//! Esta camada só sabe gravar e ler o log. Ela **não** liga o log aos
//! caminhos de escrita do domínio (etapa 3) nem aplica evento recebido
//! (etapa 4).
//!
//! ## A armadilha do `seq`, e por que ela não se resolve com a `UNIQUE`
//!
//! O índice `UNIQUE(device_id, seq)` impede corrupção, mas não impede o erro:
//!
//! ```text
//! Thread A  →  SELECT MAX(seq) = 100  →  escolhe 101
//! Thread B  →  SELECT MAX(seq) = 100  →  escolhe 101   →  uma das duas falha
//! ```
//!
//! O índice transforma corrupção em erro, que é o certo. Mas um erro aqui é
//! uma **escrita local sem evento**: o dado existe neste aparelho e nunca sai
//! dele. Nenhuma sincronização conserta isso depois, porque nada registra que
//! faltou.
//!
//! Duas medidas, e as duas são necessárias:
//!
//! 1. **`BEGIN IMMEDIATE`.** Uma transação `DEFERRED` — o padrão — só pega o
//!    lock de escrita no primeiro `INSERT`. O `MAX(seq)` seria lido antes
//!    disso, outra conexão poderia commitar no meio, e em WAL o commit
//!    falharia com `SQLITE_BUSY_SNAPSHOT` **depois** de o trabalho estar
//!    feito. `IMMEDIATE` pega o lock antes da leitura, então `MAX(seq)` já é
//!    lido sob exclusividade.
//! 2. **Retry dentro de um orçamento.** Com `IMMEDIATE`, a concorrência vira
//!    espera em vez de conflito, e o `busy_timeout` cobre a espera. O retry
//!    existe para o que sobra: timeout estourado sob carga. Desistir em
//!    silêncio seria de novo a escrita sem evento.
//!
//! **A ordem entre as duas importa, e foi medida.** Com `DEFERRED` e o retry
//! mantido em cinco tentativas, o gate de concorrência continua reprovando:
//! o erro é corretamente classificado como contenção (`database is locked`),
//! e as cinco tentativas disputam de novo com a mesma probabilidade de
//! perder. Retry não substitui modo de transação — ele só cobre o resíduo de
//! um desenho que já está certo.
//!
//! ## O orçamento de espera é explícito, e é curto
//!
//! Contar tentativas era a métrica errada. Cinco tentativas contra o
//! `busy_timeout` padrão de oito segundos permitem um pior caso perto de
//! **quarenta segundos** — e este caminho é o do autosave. Um editor que
//! trava quarenta segundos é pior que um que avisa que não conseguiu salvar.
//!
//! O que se limita aqui é o **tempo total**, não o número de tentativas:
//!
//! ```text
//! ESPERA_INICIAL   250ms   primeira fatia; dobra a cada tentativa
//! ORCAMENTO_TOTAL   10s    teto do bloqueio percebido pelo escritor
//! ```
//!
//! ### Por que a fatia cresce: fatia curta demais causa inanição
//!
//! A primeira versão usava 250ms fixos em toda tentativa. O gate de
//! concorrência passava isolado e **reprovou na suíte completa**, sob carga:
//!
//! ```text
//! Não foi possível registrar o evento em 10s (25 tentativas).
//! Último erro: database is locked
//! ```
//!
//! Vinte e cinco tentativas, todas perdidas. Não era violação de `UNIQUE` —
//! o `IMMEDIATE` estava funcionando. Era o oposto do problema original: com
//! uma fatia curta, a thread desiste de esperar e volta ao fim da disputa,
//! repetidamente, enquanto as outras avançam. Com os oito segundos de antes
//! ela ficava na fila e eventualmente ganhava.
//!
//! Dobrar a fatia recupera esse comportamento sem levantar o teto: as
//! primeiras tentativas são baratas — e é o que mantém o teste do retry na
//! casa das centenas de milissegundos — e as últimas esperam de verdade.
//!
//! O `busy_timeout` da conexão é ajustado durante o append e devolvido ao
//! valor de origem no fim, inclusive quando a operação falha.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::new_id;
use crate::domain::sync::{
    compute_revision, AggregateHistory, AggregateRef, EventEnvelope, Operation,
};
use crate::infrastructure::sqlite::connection::BUSY_TIMEOUT;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use std::time::{Duration, Instant};

/// Quanto o SQLite segura a PRIMEIRA tentativa antes de devolver "locked".
/// Dobra a cada tentativa, sempre limitada pelo que resta do orçamento.
const ESPERA_INICIAL: Duration = Duration::from_millis(250);

/// Teto do bloqueio que o escritor pode perceber ao salvar.
const ORCAMENTO_TOTAL: Duration = Duration::from_secs(10);

/// Operação que não é `upsert` nem `delete`. O CHECK do schema impede que
/// ela nasça, então chegar aqui significa banco adulterado ou corrompido —
/// motivo de sobra para parar em vez de adivinhar.
#[derive(Debug)]
struct OperacaoDesconhecida(String);

impl std::fmt::Display for OperacaoDesconhecida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "operação de sincronização desconhecida no log: {:?}",
            self.0
        )
    }
}

impl std::error::Error for OperacaoDesconhecida {}

/// Uma escrita local a registrar no log.
pub struct LocalChange<'a> {
    pub universe_id: &'a str,
    pub aggregate: AggregateRef,
    pub operation: Operation,
    /// JSON já serializado. **Não é reserializado em lugar nenhum** — veja
    /// `payload_atravessa_o_log_sem_reserializacao`.
    pub payload: &'a str,
}

/// Quem é este aparelho, no roster.
pub fn self_device_id(connection: &Connection) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT device_id FROM sync_devices WHERE is_self = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

/// Registra uma escrita local no log e devolve o envelope gravado.
///
/// A transação é `IMMEDIATE` e cobre `MAX(seq)`, o `INSERT` no log, a
/// história de revisões e o estado do agregado — os quatro ou nada.
pub fn append_local_event(
    connection: &mut Connection,
    change: &LocalChange<'_>,
) -> DatabaseCommandResult<EventEnvelope> {
    // Quem ajusta o `busy_timeout` é o laço, por tentativa. Aqui só se
    // garante a devolução ao valor de origem nas duas saídas: sem isso, uma
    // saída por erro deixaria a conexão com a fatia do append, e todo o resto
    // do aplicativo passaria a desistir cedo.
    let resultado = append_dentro_do_orcamento(connection, change);
    let _ = connection.busy_timeout(BUSY_TIMEOUT);
    resultado
}

fn append_dentro_do_orcamento(
    connection: &mut Connection,
    change: &LocalChange<'_>,
) -> DatabaseCommandResult<EventEnvelope> {
    let inicio = Instant::now();
    let mut tentativas = 0_u32;
    let mut espera = ESPERA_INICIAL;

    // O erro que sai do laço é sempre o da última tentativa que estourou o
    // orçamento — não há valor inicial a descartar.
    let ultimo_erro = loop {
        let restante = ORCAMENTO_TOTAL.saturating_sub(inicio.elapsed());

        // A fatia cresce a cada tentativa, sempre limitada pelo que resta do
        // orçamento. É o que impede a inanição descrita no cabeçalho sem
        // levantar o teto de espera do escritor.
        connection
            .busy_timeout(espera.min(restante))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

        tentativas += 1;
        match tentar_append(connection, change) {
            Ok(envelope) => return Ok(envelope),
            Err(erro) if e_contencao(&erro) => {
                if inicio.elapsed() >= ORCAMENTO_TOTAL {
                    break erro;
                }
                espera = (espera * 2).min(ORCAMENTO_TOTAL);
            }
            // Erro que não é contenção não melhora com repetição.
            Err(erro) => return Err(erro),
        }
    };

    Err(DatabaseCommandError::storage(format!(
        "Não foi possível registrar o evento de sincronização em {}s ({tentativas} tentativas). \
         A alteração NÃO foi gravada, de propósito: gravar o dado sem o evento deixaria a \
         alteração presa neste aparelho para sempre. Último erro: {ultimo_erro}",
        ORCAMENTO_TOTAL.as_secs(),
    )))
}

fn e_contencao(erro: &DatabaseCommandError) -> bool {
    let texto = erro.to_string();
    texto.contains("database is locked")
        || texto.contains("database table is locked")
        || texto.contains("busy")
        || texto.contains("BUSY")
}

fn tentar_append(
    connection: &mut Connection,
    change: &LocalChange<'_>,
) -> DatabaseCommandResult<EventEnvelope> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let device_id: String = tx
        .query_row(
            "SELECT device_id FROM sync_devices WHERE is_self = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .ok_or_else(|| {
            DatabaseCommandError::unavailable(
                "Este aparelho ainda não tem identidade de sincronização registrada.",
            )
        })?;

    // Derivado, e não guardado num contador. O motivo está no comentário da
    // migration v16: um backup restaurado em outra máquina vira um
    // dispositivo NOVO, cuja origem começa do 1 — um contador restaurado
    // continuaria de onde o aparelho antigo parou, assinando com uma chave
    // diferente sob um device_id que não é o seu.
    let proximo_seq: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM sync_events WHERE device_id = ?1",
            [&device_id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let base_rev: String = tx
        .query_row(
            "SELECT current_rev FROM sync_aggregate_state
              WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [
                &change.aggregate.aggregate_type,
                &change.aggregate.aggregate_id,
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .unwrap_or_default();

    let new_rev = compute_revision(
        &base_rev,
        &change.aggregate,
        change.operation,
        change.payload,
    );
    let event_id = new_id();

    tx.execute(
        "INSERT INTO sync_events
            (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id,
             operation, payload, base_rev, new_rev, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, '')",
        rusqlite::params![
            &event_id,
            &device_id,
            proximo_seq,
            change.universe_id,
            &change.aggregate.aggregate_type,
            &change.aggregate.aggregate_id,
            change.operation.as_str(),
            change.payload,
            &base_rev,
            &new_rev,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // Escrita local entra em `sync_applied_events` também: "aplicado" quer
    // dizer "refletido nos agregados", e uma escrita local está refletida por
    // construção. Sem isto, todo evento local pareceria pendente.
    tx.execute(
        "INSERT INTO sync_applied_events (event_id) VALUES (?1)",
        [&event_id],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    tx.execute(
        "INSERT INTO sync_revision_history
            (aggregate_type, aggregate_id, rev, base_rev, event_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            &change.aggregate.aggregate_type,
            &change.aggregate.aggregate_id,
            &new_rev,
            &base_rev,
            &event_id,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    match change.operation {
        Operation::Upsert => {
            tx.execute(
                "INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(aggregate_type, aggregate_id)
                 DO UPDATE SET current_rev = excluded.current_rev",
                rusqlite::params![
                    &change.aggregate.aggregate_type,
                    &change.aggregate.aggregate_id,
                    &new_rev,
                ],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        }
        Operation::Delete => {
            // O agregado sai do estado corrente e vira tombstone. A história
            // de revisões permanece: é ela que impede a ressurreição quando
            // um peer atrasado reaparecer com uma revisão anterior.
            tx.execute(
                "DELETE FROM sync_aggregate_state
                  WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                [
                    &change.aggregate.aggregate_type,
                    &change.aggregate.aggregate_id,
                ],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            tx.execute(
                "INSERT INTO sync_tombstones (aggregate_type, aggregate_id, deleted_rev)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(aggregate_type, aggregate_id)
                 DO UPDATE SET deleted_rev = excluded.deleted_rev",
                rusqlite::params![
                    &change.aggregate.aggregate_type,
                    &change.aggregate.aggregate_id,
                    &new_rev,
                ],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        }
    }

    // O cursor da própria origem acompanha o log local. Como cada `seq` local
    // nasce contíguo, o trigger de densidade da v16 aceita o avanço de 1.
    tx.execute(
        "INSERT INTO sync_cursors (origin_device_id, baseline_seq, last_seq_applied)
         VALUES (?1, 0, ?2)
         ON CONFLICT(origin_device_id)
         DO UPDATE SET last_seq_applied = excluded.last_seq_applied",
        rusqlite::params![&device_id, proximo_seq],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    Ok(EventEnvelope {
        event_id,
        device_id,
        seq: proximo_seq,
        universe_id: change.universe_id.to_string(),
        aggregate_type: change.aggregate.aggregate_type.clone(),
        aggregate_id: change.aggregate.aggregate_id.clone(),
        operation: change.operation,
        payload: change.payload.to_string(),
        base_rev,
        new_rev,
        signature: String::new(),
    })
}

/// A história local de um agregado, para a classificação da seção 11.
pub fn aggregate_history(
    connection: &Connection,
    aggregate: &AggregateRef,
) -> DatabaseCommandResult<AggregateHistory> {
    let current_rev: Option<String> = connection
        .query_row(
            "SELECT current_rev FROM sync_aggregate_state
              WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&aggregate.aggregate_type, &aggregate.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let mut statement = connection
        .prepare(
            "SELECT rev FROM sync_revision_history
              WHERE aggregate_type = ?1 AND aggregate_id = ?2",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let known_revs = statement
        .query_map(
            [&aggregate.aggregate_type, &aggregate.aggregate_id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    Ok(AggregateHistory {
        current_rev,
        known_revs,
    })
}

/// O outbox. Não é tabela: é o recorte do log originado aqui.
pub fn outbox_since(
    connection: &Connection,
    device_id: &str,
    depois_de: i64,
) -> DatabaseCommandResult<Vec<EventEnvelope>> {
    let mut statement = connection
        .prepare(
            "SELECT event_id, device_id, seq, universe_id, aggregate_type, aggregate_id,
                    operation, payload, base_rev, new_rev, signature
               FROM sync_events
              WHERE device_id = ?1 AND seq > ?2
           ORDER BY seq",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let eventos = statement
        .query_map(rusqlite::params![device_id, depois_de], |row| {
            let operacao: String = row.get(6)?;
            // Falha fechada. Tratar operação desconhecida como `upsert` faria
            // um evento corrompido virar uma escrita de conteúdo — e um
            // `delete` ilegível viraria ressurreição silenciosa do agregado.
            let operation = Operation::parse(&operacao).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(OperacaoDesconhecida(operacao.clone())),
                )
            })?;
            Ok(EventEnvelope {
                event_id: row.get(0)?,
                device_id: row.get(1)?,
                seq: row.get(2)?,
                universe_id: row.get(3)?,
                aggregate_type: row.get(4)?,
                aggregate_id: row.get(5)?,
                operation,
                payload: row.get(7)?,
                base_rev: row.get(8)?,
                new_rev: row.get(9)?,
                signature: row.get(10)?,
            })
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(eventos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    fn registrar_identidade(connection: &Connection, device_id: &str, is_self: i64) {
        connection
            .execute(
                "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                 VALUES (?1, ?1, 'chave-de-teste', ?2)",
                rusqlite::params![device_id, is_self],
            )
            .expect("registrar dispositivo");
    }

    fn mudanca<'a>(id: &'a str, payload: &'a str) -> LocalChange<'a> {
        LocalChange {
            universe_id: "u1",
            aggregate: AggregateRef::new("chapter", id),
            operation: Operation::Upsert,
            payload,
        }
    }

    /// GATE DA ETAPA 2.
    ///
    /// Duas escritas locais concorrentes produzem `seq` diferentes, e nenhuma
    /// alteração fica sem evento.
    ///
    /// A `UNIQUE(device_id, seq)` sozinha não garante isto: ela transforma a
    /// corrida em erro, e um erro mal tratado seria uma escrita local sem
    /// evento — o dado ficaria preso neste aparelho para sempre, e nada
    /// registraria que faltou. Este teste roda escritas de verdade, em
    /// threads de verdade, contra um banco em arquivo.
    ///
    /// ## Por que ele exige ZERO falhas
    ///
    /// Uma versão intermediária tolerava falha por contenção, com o argumento
    /// de que falha limpa não é alteração perdida. O argumento é verdadeiro e
    /// a mudança **cegou o gate**: sob `DEFERRED` as escritas falham
    /// justamente por contenção, e o teste passou a aceitar isso como normal.
    /// A mutação provou — trocar `IMMEDIATE` por `DEFERRED` deixou de reprovar.
    ///
    /// Falha por contenção é exatamente o sintoma do defeito. O gate exige
    /// que **nenhuma** aconteça, e a carga foi reduzida em vez da exigência:
    /// 4×4 em vez de 8×6, porque o defeito aparece com dois escritores e as
    /// 48 escritas de antes só estouravam o orçamento sob a suíte inteira.
    #[test]
    fn escritas_locais_concorrentes_recebem_sequencias_distintas() {
        let temporario = TemporaryDatabase::new();
        {
            let connection = temporario.database.write().expect("abrir para escrita");
            registrar_identidade(&connection, "dev-local", 1);
        }

        // Quatro escritores já produzem a corrida — o defeito que este gate
        // pega aparece com dois. Oito só aumentavam a chance de o orçamento
        // estourar sob carga, sem provar nada a mais.
        const ESCRITORES: usize = 4;
        const POR_ESCRITOR: usize = 4;

        let barreira = Arc::new(Barrier::new(ESCRITORES));
        let caminho = temporario.database.clone();

        let mut linhas = Vec::new();
        for escritor in 0..ESCRITORES {
            let barreira = Arc::clone(&barreira);
            let banco = caminho.clone();
            linhas.push(std::thread::spawn(move || {
                let mut connection = banco.write().expect("abrir para escrita");
                // Todas largam juntas: sem isto, as escritas se enfileirariam
                // naturalmente e o teste não exercitaria concorrência nenhuma.
                barreira.wait();
                let mut sequencias = Vec::new();
                for indice in 0..POR_ESCRITOR {
                    let id = format!("cap-{escritor}-{indice}");
                    let payload = format!("{{\"titulo\":\"{id}\"}}");
                    match append_local_event(&mut connection, &mudanca(&id, &payload)) {
                        Ok(envelope) => sequencias.push(envelope.seq),
                        // Falhar aqui é o sintoma do defeito, contenção
                        // inclusive: com `IMMEDIATE` a disputa vira espera, e
                        // espera dentro do orçamento termina em sucesso.
                        Err(erro) => panic!("toda escrita local precisa virar evento: {erro}"),
                    }
                }
                sequencias
            }));
        }

        let mut todas = Vec::new();
        for linha in linhas {
            todas.extend(
                linha
                    .join()
                    .expect("thread de escrita não pode entrar em pânico"),
            );
        }

        assert_eq!(
            todas.len(),
            ESCRITORES * POR_ESCRITOR,
            "alguma escrita não virou evento"
        );

        let distintas: HashSet<i64> = todas.iter().copied().collect();
        assert_eq!(
            distintas.len(),
            todas.len(),
            "duas escritas receberam o mesmo seq: {todas:?}"
        );

        // Seq local nasce contíguo, e é isso que permite ao cursor da própria
        // origem avançar. Uma escrita que falhou não consome sequência.
        let mut ordenadas: Vec<i64> = distintas.into_iter().collect();
        ordenadas.sort_unstable();
        let esperadas: Vec<i64> = (1..=(ESCRITORES * POR_ESCRITOR) as i64).collect();
        assert_eq!(ordenadas, esperadas, "a sequência local ficou com buraco");

        let connection = temporario.database.read().expect("abrir para leitura");

        // Um evento por sucesso, e nenhum a mais: a falha não deixou rastro.
        let gravados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar eventos");
        assert_eq!(
            gravados as usize,
            todas.len(),
            "o número de eventos não bate com o de escritas bem-sucedidas"
        );

        // E nenhum evento ficou sem a marca de aplicado — os quatro efeitos da
        // transação entraram juntos ou não entraram.
        let sem_aplicacao: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_events e
                  LEFT JOIN sync_applied_events a ON a.event_id = e.event_id
                  WHERE a.event_id IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("procurar evento sem aplicação");
        assert_eq!(sem_aplicacao, 0, "evento gravado sem a marca de aplicado");

        let cursor: i64 = connection
            .query_row(
                "SELECT last_seq_applied FROM sync_cursors WHERE origin_device_id = 'dev-local'",
                [],
                |row| row.get(0),
            )
            .expect("ler cursor da própria origem");
        assert_eq!(
            cursor as usize,
            todas.len(),
            "o cursor não pode estar à frente do que foi gravado"
        );
    }

    /// GATE DO MODO DE TRANSAÇÃO, sem o retry por cima.
    ///
    /// O gate integrado acima deixou de distinguir `IMMEDIATE` de `DEFERRED`
    /// assim que o retry ganhou espera crescente: o laço passou a absorver as
    /// falhas do modo errado, e a mutação deixou de reprovar. Um gate que não
    /// reprova quando o mecanismo cai não protege nada.
    ///
    /// Este exercita **uma tentativa só**, com o `busy_timeout` de origem:
    ///
    /// ```text
    /// IMMEDIATE  →  o lock é pego antes da leitura de MAX(seq).
    ///               As tentativas fazem fila e todas terminam.
    ///
    /// DEFERRED   →  MAX(seq) é lido antes de o lock existir. Outra conexão
    ///               commita no meio, o snapshot fica velho, e o commit falha
    ///               DEPOIS de o trabalho estar feito.
    /// ```
    #[test]
    fn uma_tentativa_sozinha_nao_perde_a_corrida() {
        let temporario = TemporaryDatabase::new();
        {
            let connection = temporario.database.write().expect("abrir para escrita");
            registrar_identidade(&connection, "dev-local", 1);
        }

        const ESCRITORES: usize = 8;
        let barreira = Arc::new(Barrier::new(ESCRITORES));
        let caminho = temporario.database.clone();

        let mut linhas = Vec::new();
        for escritor in 0..ESCRITORES {
            let barreira = Arc::clone(&barreira);
            let banco = caminho.clone();
            linhas.push(std::thread::spawn(move || {
                let mut connection = banco.write().expect("abrir para escrita");
                barreira.wait();
                let id = format!("cap-{escritor}");
                let payload = format!("{{\"titulo\":\"{id}\"}}");
                // Sem retry: é o modo de transação que tem que segurar.
                tentar_append(&mut connection, &mudanca(&id, &payload)).map(|envelope| envelope.seq)
            }));
        }

        let mut sequencias = Vec::new();
        for linha in linhas {
            let resultado = linha.join().expect("thread não pode entrar em pânico");
            sequencias.push(resultado.unwrap_or_else(|erro| {
                panic!(
                    "uma tentativa perdeu a corrida — o lock não foi pego antes da leitura: {erro}"
                )
            }));
        }

        let distintas: HashSet<i64> = sequencias.iter().copied().collect();
        assert_eq!(distintas.len(), ESCRITORES, "seq repetido: {sequencias:?}");
    }

    /// O retry deixa de ser caminho não exercitado.
    ///
    /// Eu tinha escrito que testar isto exigiria segurar o lock por mais de
    /// oito segundos. Estava errado por assumir o `busy_timeout` da conexão
    /// como dado: o append agora tem espera **própria** por tentativa, e
    /// basta segurar o lock por mais tempo que ela.
    ///
    /// ```text
    /// outra conexão segura BEGIN IMMEDIATE por ~600ms
    ///   tentativa 1  →  espera 250ms  →  "database is locked"
    ///   tentativa 2  →  espera 250ms  →  "database is locked"
    ///   lock libera
    ///   tentativa 3  →  sucesso
    /// ```
    #[test]
    fn o_retry_atravessa_um_lock_temporario() {
        let temporario = TemporaryDatabase::new();
        {
            let connection = temporario.database.write().expect("abrir para escrita");
            registrar_identidade(&connection, "dev-local", 1);
        }

        const SEGURAR_POR: Duration = Duration::from_millis(600);
        let liberado = Arc::new(Barrier::new(2));

        let banco = temporario.database.clone();
        let sinal = Arc::clone(&liberado);
        let bloqueador = std::thread::spawn(move || {
            let mut connection = banco.write().expect("abrir para escrita");
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("segurar o lock de escrita");
            // Só depois de o lock existir é que o append pode começar; senão o
            // teste poderia passar sem nunca ter havido contenção.
            sinal.wait();
            std::thread::sleep(SEGURAR_POR);
            tx.rollback().expect("soltar o lock");
        });

        let mut connection = temporario.database.write().expect("abrir para escrita");
        liberado.wait();
        let comeco = Instant::now();
        let envelope = append_local_event(&mut connection, &mudanca("cap-1", r#"{"t":"a"}"#))
            .expect("o retry precisa atravessar um lock temporário");
        let decorrido = comeco.elapsed();

        bloqueador.join().expect("thread do lock");

        assert_eq!(envelope.seq, 1);
        assert!(
            decorrido >= ESPERA_INICIAL,
            "terminou em {decorrido:?}: rápido demais para ter havido contenção, \
             e o teste estaria passando sem exercitar o retry"
        );
        assert!(
            decorrido < ORCAMENTO_TOTAL,
            "o orçamento existe para não deixar o escritor esperando: {decorrido:?}"
        );
    }

    /// O `busy_timeout` curto é do append, não da conexão.
    ///
    /// Sem devolver o valor de origem, uma única chamada contaminaria a
    /// conexão e todo o resto do aplicativo passaria a desistir em 250ms —
    /// inclusive numa saída por erro, que é o caminho mais fácil de esquecer.
    #[test]
    fn o_append_devolve_o_busy_timeout_da_conexao() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");

        // Sem identidade: caminho de erro.
        append_local_event(&mut connection, &mudanca("cap-1", "{}"))
            .expect_err("sem is_self a escrita falha");

        // `busy_timeout` não tem getter; medimos o efeito. Com 250ms residual,
        // esta espera terminaria muito antes do que o valor de origem manda.
        let banco = temporario.database.clone();
        let liberado = Arc::new(Barrier::new(2));
        let sinal = Arc::clone(&liberado);
        let bloqueador = std::thread::spawn(move || {
            let mut outra = banco.write().expect("abrir para escrita");
            let tx = outra
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("segurar o lock");
            sinal.wait();
            std::thread::sleep(Duration::from_millis(900));
            tx.rollback().expect("soltar o lock");
        });

        liberado.wait();
        let comeco = Instant::now();
        let resultado = connection.execute(
            "INSERT INTO sync_devices (device_id, ed25519_public) VALUES ('x', 'y')",
            [],
        );
        let decorrido = comeco.elapsed();
        bloqueador.join().expect("thread do lock");

        assert!(
            resultado.is_ok(),
            "a conexão desistiu antes de o lock soltar: o busy_timeout curto do append vazou"
        );
        assert!(
            decorrido >= Duration::from_millis(500),
            "a escrita não chegou a esperar: {decorrido:?}"
        );
    }

    /// Sem identidade registrada, a escrita **falha** em vez de gravar um
    /// evento órfão. É o oposto do que a conveniência pediria, e é o certo:
    /// um evento sem origem conhecida não pode ser retransmitido nem
    /// verificado.
    #[test]
    fn sem_identidade_a_escrita_nao_acontece() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");
        let erro = append_local_event(&mut connection, &mudanca("cap-1", "{}"))
            .expect_err("sem is_self não há origem para o evento");
        assert!(
            erro.to_string().contains("identidade"),
            "erro inesperado: {erro}"
        );

        let gravados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar eventos");
        assert_eq!(gravados, 0);
    }

    /// A cadeia de revisões: cada escrita parte da anterior.
    #[test]
    fn escritas_seguidas_encadeiam_as_revisoes() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");
        registrar_identidade(&connection, "dev-local", 1);

        let primeiro = append_local_event(&mut connection, &mudanca("cap-1", r#"{"t":"a"}"#))
            .expect("primeira escrita");
        assert_eq!(primeiro.base_rev, "", "a primeira revisão parte da raiz");

        let segundo = append_local_event(&mut connection, &mudanca("cap-1", r#"{"t":"b"}"#))
            .expect("segunda escrita");
        assert_eq!(
            segundo.base_rev, primeiro.new_rev,
            "a segunda escrita precisa declarar de onde partiu"
        );
        assert_ne!(segundo.new_rev, primeiro.new_rev);

        let historia = aggregate_history(&connection, &AggregateRef::new("chapter", "cap-1"))
            .expect("ler história");
        assert_eq!(
            historia.current_rev.as_deref(),
            Some(segundo.new_rev.as_str())
        );
        assert!(
            historia.knows(&primeiro.new_rev),
            "a revisão anterior continua conhecida"
        );
    }

    /// O payload atravessa o log **byte a byte**.
    ///
    /// Isto não é detalhe de implementação: `new_rev` é o hash do payload, e
    /// o aparelho que recebe recalcula o hash sobre os bytes que chegaram. Um
    /// `serde_json` reserializando no meio do caminho — reordenando chaves,
    /// normalizando espaço, mudando notação de número — produziria um hash
    /// diferente do mesmo conteúdo, e o evento seria recusado como se
    /// tivesse sido adulterado.
    #[test]
    fn payload_atravessa_o_log_sem_reserializacao() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");
        registrar_identidade(&connection, "dev-local", 1);

        // Chaves fora de ordem alfabética, espaço irregular e número com
        // notação incomum: tudo que um reserializador "arrumaria".
        let original = r#"{"zeta":1,  "alfa" : 2.50, "texto":"linha\ncom quebra"}"#;
        let envelope =
            append_local_event(&mut connection, &mudanca("cap-1", original)).expect("gravar");

        let guardado: String = connection
            .query_row(
                "SELECT payload FROM sync_events WHERE event_id = ?1",
                [&envelope.event_id],
                |row| row.get(0),
            )
            .expect("ler payload");
        assert_eq!(guardado, original, "o payload foi alterado no caminho");

        let recalculado = compute_revision(
            &envelope.base_rev,
            &AggregateRef::new("chapter", "cap-1"),
            Operation::Upsert,
            &guardado,
        );
        assert_eq!(
            recalculado, envelope.new_rev,
            "quem recebe recalcula o hash sobre os bytes que chegaram; eles têm que bater"
        );
    }

    #[test]
    fn delete_deixa_tombstone_e_tira_o_agregado_do_estado_corrente() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");
        registrar_identidade(&connection, "dev-local", 1);

        append_local_event(&mut connection, &mudanca("cap-1", r#"{"t":"a"}"#)).expect("criar");
        let apagado = append_local_event(
            &mut connection,
            &LocalChange {
                universe_id: "u1",
                aggregate: AggregateRef::new("chapter", "cap-1"),
                operation: Operation::Delete,
                payload: "",
            },
        )
        .expect("apagar");

        let historia = aggregate_history(&connection, &AggregateRef::new("chapter", "cap-1"))
            .expect("ler história");
        assert_eq!(
            historia.current_rev, None,
            "o agregado saiu do estado corrente"
        );
        assert!(
            historia.knows(&apagado.new_rev),
            "a história permanece: é ela que impede a ressurreição"
        );

        let tombstone: String = connection
            .query_row(
                "SELECT deleted_rev FROM sync_tombstones
                  WHERE aggregate_type = 'chapter' AND aggregate_id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .expect("ler tombstone");
        assert_eq!(tombstone, apagado.new_rev);
    }

    /// O outbox é um recorte do log, não uma tabela — e só devolve o que
    /// nasceu aqui.
    #[test]
    fn o_outbox_e_o_recorte_da_propria_origem() {
        let temporario = TemporaryDatabase::new();
        let mut connection = temporario.database.write().expect("abrir para escrita");
        registrar_identidade(&connection, "dev-local", 1);
        registrar_identidade(&connection, "dev-remoto", 0);

        append_local_event(&mut connection, &mudanca("cap-1", r#"{"t":"a"}"#)).expect("local 1");
        append_local_event(&mut connection, &mudanca("cap-2", r#"{"t":"b"}"#)).expect("local 2");

        // Um evento recebido de outra origem, gravado direto no log como a
        // etapa 4 fará. Ele não pode aparecer no outbox deste aparelho.
        connection
            .execute(
                "INSERT INTO sync_events
                    (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id,
                     operation, payload, base_rev, new_rev, signature)
                 VALUES ('ev-remoto', 'dev-remoto', 1, 'u1', 'chapter', 'cap-9',
                         'upsert', '{}', '', 'rev-remota', 'assinatura')",
                [],
            )
            .expect("gravar evento remoto");

        let saida = outbox_since(&connection, "dev-local", 0).expect("ler outbox");
        assert_eq!(saida.len(), 2);
        assert!(
            saida.iter().all(|evento| evento.device_id == "dev-local"),
            "o outbox devolveu evento de outra origem"
        );

        let depois = outbox_since(&connection, "dev-local", 1).expect("ler outbox parcial");
        assert_eq!(depois.len(), 1);
        assert_eq!(depois[0].seq, 2);
    }
}
