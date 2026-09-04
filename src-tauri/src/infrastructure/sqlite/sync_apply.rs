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
use crate::domain::sync::{
    classify, compute_revision, AggregateRef, Causality, EventEnvelope, Operation,
};
use crate::infrastructure::sqlite::sync_repository::aggregate_history;
use rusqlite::{OptionalExtension, Transaction};

/// O que aconteceu com o evento que chegou.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    /// Aplicado: o agregado mudou e o evento entrou no log.
    Aplicado,
    /// Já conhecíamos este evento. Repetição é normal em rede, e não é erro.
    JaAplicado,
    /// Duas edições a partir da mesma base. **Nada foi sobrescrito** — as
    /// duas revisões ficam preservadas e a divergência é registrada.
    Divergente { id_divergencia: String },
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
            aplicar_no_agregado(tx, envelope)?;
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
        Causality::Unknown { .. } => {
            // NÃO marca como aplicado: quando a história intermediária
            // chegar, este evento precisa ser reavaliado. O envelope fica
            // guardado, então nada se perde.
            Ok(Applied::PrecisaReconciliar)
        }
    }
}

fn guardar_envelope(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
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

fn registrar_divergencia(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
    base_rev: &str,
    historia: &crate::domain::sync::AggregateHistory,
) -> DatabaseCommandResult<String> {
    let id = crate::domain::ids::new_id();
    tx.execute(
        "INSERT INTO sync_divergences
            (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            &id,
            &envelope.aggregate_type,
            &envelope.aggregate_id,
            base_rev,
            historia.current_rev.as_deref().unwrap_or_default(),
            &envelope.new_rev,
            &envelope.event_id,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(id)
}

/// Escreve o estado novo na tabela do agregado.
///
/// **Falha fechada em tipo desconhecido.** Ignorar um agregado que ainda não
/// sabemos aplicar produziria o pior estado possível: o cursor avançaria, o
/// evento constaria como aplicado, e o dado nunca chegaria — sem nada
/// registrando a falta. Recusar faz a sessão parar e o problema aparecer.
fn aplicar_no_agregado(
    tx: &Transaction<'_>,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    match envelope.aggregate_type.as_str() {
        "chapter" => aplicar_capitulo(tx, envelope),
        outro => Err(DatabaseCommandError::storage(format!(
            "Agregado '{outro}' ainda não tem aplicação de evento implementada. A sessão para \
             aqui de propósito: avançar marcaria o evento como aplicado sem que o dado tivesse \
             chegado, e ninguém saberia que faltou."
        ))),
    }
}

fn aplicar_capitulo(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    if envelope.operation == Operation::Delete {
        tx.execute(
            "DELETE FROM chapters WHERE id = ?1",
            [&envelope.aggregate_id],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        tx.execute(
            "INSERT INTO sync_tombstones (aggregate_type, aggregate_id, deleted_rev)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(aggregate_type, aggregate_id)
             DO UPDATE SET deleted_rev = excluded.deleted_rev",
            rusqlite::params![
                &envelope.aggregate_type,
                &envelope.aggregate_id,
                &envelope.new_rev,
            ],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        tx.execute(
            "DELETE FROM sync_aggregate_state WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&envelope.aggregate_type, &envelope.aggregate_id],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        return Ok(());
    }

    let capitulo: crate::domain::manuscript::Chapter = serde_json::from_str(&envelope.payload)
        .map_err(|error| {
            DatabaseCommandError::storage(format!(
                "O payload do capítulo não descreve um capítulo: {error}"
            ))
        })?;

    if capitulo.id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(
            "O payload descreve um capítulo diferente do agregado do envelope.",
        ));
    }

    tx.execute(
        "INSERT INTO chapters
            (id, book_id, title, content, summary, scene_origin, scene_destination,
             word_count, status, canon_status, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
            book_id = excluded.book_id,
            title = excluded.title,
            content = excluded.content,
            summary = excluded.summary,
            scene_origin = excluded.scene_origin,
            scene_destination = excluded.scene_destination,
            word_count = excluded.word_count,
            status = excluded.status,
            canon_status = excluded.canon_status,
            sort_order = excluded.sort_order,
            updated_at = excluded.updated_at",
        rusqlite::params![
            &capitulo.id,
            &capitulo.book_id,
            &capitulo.title,
            &capitulo.content,
            &capitulo.summary,
            &capitulo.scene_origin,
            &capitulo.scene_destination,
            capitulo.word_count,
            &capitulo.status,
            &capitulo.canon_status,
            capitulo.sort_order,
            &capitulo.created_at,
            &capitulo.updated_at,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    tx.execute(
        "INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(aggregate_type, aggregate_id)
         DO UPDATE SET current_rev = excluded.current_rev",
        rusqlite::params![
            &envelope.aggregate_type,
            &envelope.aggregate_id,
            &envelope.new_rev,
        ],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            r#"{{"id":"cap-1","book_id":"b1","title":"{titulo}","content":"{conteudo}",
                 "summary":"","scene_origin":"","scene_destination":"","word_count":3,
                 "status":"rascunho","canon_status":"canon","sort_order":0,
                 "created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
        )
        .replace('\n', "")
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
            &AggregateRef::new("entity", "ent-1"),
            Operation::Upsert,
            r#"{"id":"ent-1"}"#,
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
            erro.to_string().contains("capítulo diferente"),
            "recusou pelo motivo errado: {erro}"
        );
    }
}
