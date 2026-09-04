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
use crate::domain::sync::{EventEnvelope, Operation};
use crate::infrastructure::sqlite::sync_apply::{apply_remote_event, Applied};
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
    let mut origens: BTreeMap<String, ()> = BTreeMap::new();
    for envelope in envelopes {
        guardar(&tx, envelope)?;
        origens.insert(envelope.device_id.clone(), ());
    }

    // E também as origens que já tinham pendentes de sessões anteriores: a
    // lacuna pode ter fechado com o que acabou de chegar.
    for origem in origens_com_pendentes(&tx)? {
        origens.insert(origem, ());
    }

    let mut relatorio = Relatorio::default();
    for origem in origens.keys() {
        drenar_origem(&tx, origem, &mut relatorio)?;
    }
    relatorio.pendentes = contar_pendentes(&tx)?;

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(relatorio)
}

/// Aplica, em ordem, tudo o que estiver contíguo a partir do cursor.
fn drenar_origem(
    tx: &Transaction<'_>,
    origem: &str,
    relatorio: &mut Relatorio,
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

        match apply_remote_event(tx, &envelope)? {
            Applied::Aplicado => relatorio.aplicados += 1,
            Applied::JaAplicado => {}
            Applied::Divergente { .. } => relatorio.divergencias += 1,
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
    tx.execute(
        "INSERT INTO sync_cursors (origin_device_id, baseline_seq, last_seq_applied)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(origin_device_id) DO UPDATE SET last_seq_applied = excluded.last_seq_applied",
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
        "SELECT event_id, device_id, seq, universe_id, aggregate_type, aggregate_id,
                operation, payload, base_rev, new_rev, signature
           FROM sync_events WHERE device_id = ?1 AND seq = ?2",
        rusqlite::params![origem, seq],
        |row| {
            let operacao: String = row.get(6)?;
            // Falha fechada, como em `outbox_since`. Tratar operação
            // desconhecida como `upsert` faria um evento corrompido virar
            // escrita de conteúdo, e um `delete` ilegível viraria
            // ressurreição silenciosa do agregado.
            let operation = Operation::parse(&operacao).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    format!("operação de sincronização desconhecida no log: {operacao:?}").into(),
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
        },
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
    tx.execute(
        "INSERT OR IGNORE INTO sync_events
            (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id,
             operation, payload, base_rev, new_rev, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            &envelope.event_id,
            &envelope.device_id,
            envelope.seq,
            &envelope.universe_id,
            &envelope.aggregate_type,
            &envelope.aggregate_id,
            envelope.operation.as_str(),
            &envelope.payload,
            &envelope.base_rev,
            &envelope.new_rev,
            &envelope.signature,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sync::AggregateRef;
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

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

    fn capitulo(id: &str, titulo: &str) -> String {
        format!(
            r#"{{"id":"{id}","book_id":"b1","title":"{titulo}","content":"texto","summary":"","scene_origin":"","scene_destination":"","word_count":1,"status":"rascunho","canon_status":"canon","sort_order":0,"created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
        )
    }

    /// Uma cadeia de eventos independentes, cada um criando um capítulo.
    fn cadeia(quantos: i64) -> Vec<EventEnvelope> {
        (1..=quantos)
            .map(|seq| {
                let id = format!("cap-{seq}");
                envelope_de_origem(
                    ORIGEM,
                    seq,
                    "u1",
                    &AggregateRef::new("chapter", &id),
                    Operation::Upsert,
                    &capitulo(&id, &format!("Capitulo {seq}")),
                    "",
                )
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
        let mut connection = preparar(&fixture);

        let relatorio = receber_eventos(&mut connection, &cadeia(3)).expect("receber");
        assert_eq!(relatorio.aplicados, 3);
        assert_eq!(relatorio.pendentes, 0);
        assert_eq!(cursor(&connection, ORIGEM), 3);
    }

    /// GATE DA ETAPA 5: o cursor não atravessa a lacuna, e o adiantado espera.
    #[test]
    fn evento_adiantado_fica_pendente_e_o_cursor_para_antes_da_lacuna() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let todos = cadeia(3);
        // Chegam o 1 e o 3. O 2 se perdeu no caminho.
        let sem_o_dois = vec![todos[0].clone(), todos[2].clone()];

        let relatorio = receber_eventos(&mut connection, &sem_o_dois).expect("receber");
        assert_eq!(relatorio.aplicados, 1, "só o seq 1 podia entrar");
        assert_eq!(relatorio.pendentes, 1, "o seq 3 tem que ficar guardado");
        assert_eq!(
            cursor(&connection, ORIGEM),
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
                [ORIGEM],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(guardado, 1);
    }

    /// E quando a lacuna fecha, os dois consolidam de uma vez.
    #[test]
    fn quando_a_lacuna_fecha_o_pendente_entra_junto() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let todos = cadeia(3);
        receber_eventos(&mut connection, &[todos[0].clone(), todos[2].clone()])
            .expect("primeiro lote");
        assert_eq!(cursor(&connection, ORIGEM), 1);

        // Chega só o que faltava.
        let relatorio =
            receber_eventos(&mut connection, &[todos[1].clone()]).expect("segundo lote");
        assert_eq!(
            relatorio.aplicados, 2,
            "o 2 e o 3 precisam entrar na mesma passada"
        );
        assert_eq!(relatorio.pendentes, 0);
        assert_eq!(cursor(&connection, ORIGEM), 3);

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
        let todos = cadeia(3);
        {
            let mut connection = preparar(&fixture);
            receber_eventos(&mut connection, &[todos[0].clone(), todos[2].clone()])
                .expect("primeiro lote");
        }

        // Conexão nova, como se o app tivesse sido morto e reaberto.
        let mut connection = fixture.database.write().expect("reabrir");
        let relatorio =
            receber_eventos(&mut connection, &[todos[1].clone()]).expect("depois de reabrir");
        assert_eq!(
            relatorio.aplicados, 2,
            "o pendente não sobreviveu ao fechamento do app"
        );
        assert_eq!(cursor(&connection, ORIGEM), 3);
    }

    /// Duas origens não interferem uma na outra.
    ///
    /// Uma lacuna no Android não pode travar o que veio do Notebook — é a
    /// razão de o cursor ser um vetor por origem, e não um número só.
    #[test]
    fn a_lacuna_de_uma_origem_nao_trava_a_outra() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        connection
            .execute(
                "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                 VALUES ('dev-outro', 'Outro', 'CHAVE', 0)",
                [],
            )
            .expect("registrar outra origem");

        let da_origem = cadeia(3);
        let da_outra: Vec<EventEnvelope> = (1..=2)
            .map(|seq| {
                let id = format!("out-{seq}");
                envelope_de_origem(
                    "dev-outro",
                    seq,
                    "u1",
                    &AggregateRef::new("chapter", &id),
                    Operation::Upsert,
                    &capitulo(&id, &format!("Outro {seq}")),
                    "",
                )
            })
            .collect();

        let mut lote = vec![da_origem[0].clone(), da_origem[2].clone()];
        lote.extend(da_outra.iter().cloned());

        let relatorio = receber_eventos(&mut connection, &lote).expect("receber");
        assert_eq!(relatorio.aplicados, 3, "1 da origem travada + 2 da outra");
        assert_eq!(cursor(&connection, ORIGEM), 1);
        assert_eq!(cursor(&connection, "dev-outro"), 2);
    }

    /// Contíguo por `seq` e sem história: o cursor para e o relatório diz por
    /// quê, em vez de fingir progresso.
    #[test]
    fn base_desconhecida_trava_o_cursor_e_aparece_no_relatorio() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);

        let orfao = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &AggregateRef::new("chapter", "cap-x"),
            Operation::Upsert,
            &capitulo("cap-x", "Orfao"),
            "revisao-que-nunca-vimos",
        );

        let relatorio = receber_eventos(&mut connection, &[orfao]).expect("receber");
        assert_eq!(relatorio.aplicados, 0);
        assert_eq!(relatorio.precisam_reconciliar, vec!["cap-x".to_string()]);
        assert_eq!(
            cursor(&connection, ORIGEM),
            0,
            "o cursor não pode passar por cima do que não conseguiu aplicar"
        );
    }

    /// Receber o mesmo lote duas vezes não muda nada.
    #[test]
    fn reenviar_o_mesmo_lote_e_inofensivo() {
        let fixture = TemporaryDatabase::new();
        let mut connection = preparar(&fixture);
        let lote = cadeia(3);

        receber_eventos(&mut connection, &lote).expect("primeira vez");
        let segunda = receber_eventos(&mut connection, &lote).expect("segunda vez");

        assert_eq!(segunda.aplicados, 0);
        assert_eq!(segunda.pendentes, 0);
        assert_eq!(cursor(&connection, ORIGEM), 3);

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
        let mut connection = preparar(&fixture);

        let todos = cadeia(3);
        let embaralhado = vec![todos[2].clone(), todos[0].clone(), todos[1].clone()];

        let relatorio = receber_eventos(&mut connection, &embaralhado).expect("receber");
        assert_eq!(relatorio.aplicados, 3);
        assert_eq!(cursor(&connection, ORIGEM), 3);
    }
}
