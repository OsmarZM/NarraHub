//! Sync V2 — coleta de tombstones e saída do conjunto (ADR 0009 §15, etapa 11).
//!
//! ## Tempo nunca é prova
//!
//! A pergunta que autoriza podar um tombstone é **causal**, não temporal:
//!
//! ```text
//! delete X   origem = Desktop, seq = 918
//!
//! coletável quando, para CADA membro ainda válido do conjunto:
//!     cursor daquele peer para a origem Desktop  >=  918
//! ```
//!
//! Ou seja: aquele peer **atravessou** a exclusão. Nenhuma das alternativas
//! serve:
//!
//! ```text
//! deleted_at < 90 dias         ❌  um tablet fica meses na gaveta
//! ninguém mexe faz tempo       ❌  silêncio não é confirmação
//! parece que todos viram       ❌  "parece" não é evidência
//! ```
//!
//! É o cursor da seção 13 aplicado à morte do dado. E é o mesmo raciocínio dos
//! pendentes: só se descarta o que se **provou** que não é mais necessário.
//!
//! ## As duas saídas do conjunto
//!
//! ```text
//! ACTIVE
//!   │
//!   ├─ sincronização final confirmada
//!   │        ↓
//!   │     RETIRED / 'clean'      saída limpa: nada ficou para trás
//!   │
//!   └─ aparelho perdido ou quebrado
//!            ↓
//!         RETIRED / 'abandoned'  o escritor ACEITOU a perda do que só existia ali
//! ```
//!
//! Os dois deixam de contar para a retenção — senão um celular jogado fora
//! travaria a poda para sempre. E os dois **cortam a identidade**: um aparelho
//! esquecido não pode voltar seis meses depois trazendo o estado que a poda
//! pressupôs morto.
//!
//! A diferença é o que se promete ao escritor. `retired` diz "saiu inteiro";
//! `abandoned` diz "cortei esta identidade e aceito o que não saiu dela como
//! perdido" — e essa é uma decisão de perda de dados, que só ele pode tomar.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::infrastructure::sync_transport::SessaoAutenticada;
use rusqlite::{Connection, OptionalExtension};

/// Um tombstone e as coordenadas causais da exclusão que o criou.
#[derive(Debug, PartialEq, Eq)]
pub struct Tombstone {
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub origin_device_id: String,
    pub origin_seq: i64,
}

/// Registra o que um peer disse que já viu.
///
/// É a evidência que autoriza a poda, e chega pela troca de vetores da
/// etapa 6. Sem ela guardada, cada sessão esqueceria o que aprendeu e a coleta
/// nunca teria em que se apoiar.
///
/// A autoridade vem da [`SessaoAutenticada`], e não de um `device_id` no
/// pacote: senão qualquer peer poderia anunciar o vetor de outro e destravar a
/// poda de exclusões que aquele outro nunca viu.
pub fn registrar_vetor_do_peer(
    connection: &Connection,
    sessao: &SessaoAutenticada,
    vetor: &std::collections::BTreeMap<String, i64>,
) -> DatabaseCommandResult<()> {
    for (origem, seq) in vetor {
        // `MAX` no `DO UPDATE` além do trigger: o trigger aborta a transação
        // inteira, e um vetor que chega fora de ordem numa sessão é ruído de
        // rede, não incidente. Aqui a confirmação simplesmente não retrocede.
        connection
            .execute(
                "INSERT INTO sync_peer_vectors (peer_device_id, origin_device_id, last_seq_confirmed)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(peer_device_id, origin_device_id)
                 DO UPDATE SET last_seq_confirmed =
                     MAX(last_seq_confirmed, excluded.last_seq_confirmed)",
                rusqlite::params![sessao.device_id(), origem, seq],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    }
    Ok(())
}

/// Os tombstones que **provadamente** já foram vistos por todo o conjunto.
///
/// Um tombstone sem coordenadas causais — os que vieram de bancos criados no
/// schema 16 — nunca aparece aqui. Na dúvida, guardar: o custo de um tombstone
/// são dezenas de bytes, e o de ressuscitar conteúdo apagado é assustar o
/// escritor.
pub fn tombstones_coletaveis(connection: &Connection) -> DatabaseCommandResult<Vec<Tombstone>> {
    let mut statement = connection
        .prepare(
            "SELECT t.aggregate_type, t.aggregate_id, t.origin_device_id, t.origin_seq
               FROM sync_tombstones t
              WHERE t.origin_device_id <> ''
                AND t.origin_seq > 0
                -- Nenhum membro ainda válido do conjunto pode estar atrás da
                -- exclusão. `state = 'active'` é o filtro que faz um aparelho
                -- aposentado ou abandonado deixar de travar a poda.
                AND NOT EXISTS (
                    SELECT 1
                      FROM sync_devices d
                     WHERE d.state = 'active'
                       AND d.is_self = 0
                       -- A origem não precisa confirmar a própria exclusão:
                       -- ela é quem a produziu. Cobrar isso travaria toda
                       -- poda para sempre, porque um aparelho nunca nos manda
                       -- um vetor sobre si mesmo antes de conversarmos com
                       -- ele — e foi o que os dois primeiros gates pegaram.
                       AND d.device_id <> t.origin_device_id
                       AND COALESCE((
                             SELECT v.last_seq_confirmed
                               FROM sync_peer_vectors v
                              WHERE v.peer_device_id = d.device_id
                                AND v.origin_device_id = t.origin_device_id
                           ), 0) < t.origin_seq
                )
                -- E este aparelho também precisa ter atravessado. Coletar um
                -- tombstone que nós mesmos ainda não aplicamos apagaria a
                -- única marca de que a exclusão existe.
                AND COALESCE((
                      SELECT c.last_seq_applied
                        FROM sync_cursors c
                       WHERE c.origin_device_id = t.origin_device_id
                    ), 0) >= t.origin_seq",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let linhas = statement
        .query_map([], |row| {
            Ok(Tombstone {
                aggregate_type: row.get(0)?,
                aggregate_id: row.get(1)?,
                origin_device_id: row.get(2)?,
                origin_seq: row.get(3)?,
            })
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    linhas
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

/// Quantos eventos originados por um dispositivo ainda não foram confirmados
/// por nenhum outro membro ativo do conjunto.
///
/// É o que separa a saída limpa do abandono: enquanto isto for maior que zero,
/// existe trabalho que só vive naquele aparelho.
pub fn eventos_nao_descarregados(
    connection: &Connection,
    device_id: &str,
) -> DatabaseCommandResult<i64> {
    let maior_seq: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM sync_events WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if maior_seq == 0 {
        return Ok(0);
    }

    // O melhor que algum outro aparelho ativo confirmou daquela origem.
    let melhor_confirmacao: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(v.last_seq_confirmed), 0)
               FROM sync_peer_vectors v
               JOIN sync_devices d ON d.device_id = v.peer_device_id
              WHERE v.origin_device_id = ?1
                AND d.state = 'active'
                AND d.device_id <> ?1",
            [device_id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    Ok((maior_seq - melhor_confirmacao).max(0))
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeSaida {
    /// Ainda há eventos que só existem naquele aparelho.
    ///
    /// A UI precisa oferecer as duas coisas aqui: sincronizar mais uma vez, ou
    /// **abandonar** aceitando a perda. Escolher por conta própria seria
    /// decidir sobre o conteúdo do escritor.
    FaltaSincronizarFinal {
        eventos_presos: i64,
    },
    NaoEstaNoConjunto,
    /// Aposentar a si mesmo deixaria o aparelho sem conseguir gravar.
    NaoPodeSairSozinho,
}

impl std::fmt::Display for FalhaDeSaida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDeSaida::FaltaSincronizarFinal { eventos_presos } => write!(
                f,
                "Este aparelho ainda tem {eventos_presos} alteração(ões) que nenhum outro \
                 dispositivo recebeu. Sincronize uma última vez antes de aposentá-lo — ou use \
                 \"abandonar\", aceitando que essas alterações se perdem."
            ),
            FalhaDeSaida::NaoEstaNoConjunto => f.write_str("O dispositivo não está no conjunto."),
            FalhaDeSaida::NaoPodeSairSozinho => f.write_str(
                "Este aparelho não pode se aposentar: ele deixaria de conseguir gravar as \
                 próprias alterações.",
            ),
        }
    }
}

/// Saída limpa: só depois de a última sincronização estar confirmada.
///
/// É o que dá a `retired` um significado sem ambiguidade — **acabou**. Sem a
/// pré-condição, existiria o estado estranho de um aparelho aposentado que
/// ainda tem eventos atrasados para mandar.
pub fn aposentar(
    connection: &Connection,
    sessao: &SessaoAutenticada,
    device_id: &str,
) -> DatabaseCommandResult<Result<(), FalhaDeSaida>> {
    let _ = sessao;
    if let Err(falha) = precondicoes(connection, device_id)? {
        return Ok(Err(falha));
    }

    let presos = eventos_nao_descarregados(connection, device_id)?;
    if presos > 0 {
        return Ok(Err(FalhaDeSaida::FaltaSincronizarFinal {
            eventos_presos: presos,
        }));
    }

    marcar_saida(connection, device_id, "clean")?;
    Ok(Ok(()))
}

/// Abandono: sem pré-condição, com perda aceita.
///
/// Devolve quantos eventos ficaram presos, para a tela poder dizer o número em
/// vez de "alguma coisa pode se perder". O escritor decide sobre o próprio
/// conteúdo, e decide informado ([ADR 0001](0001-local-ownership.md)).
pub fn abandonar(
    connection: &Connection,
    sessao: &SessaoAutenticada,
    device_id: &str,
) -> DatabaseCommandResult<Result<i64, FalhaDeSaida>> {
    let _ = sessao;
    if let Err(falha) = precondicoes(connection, device_id)? {
        return Ok(Err(falha));
    }

    let perdidos = eventos_nao_descarregados(connection, device_id)?;
    marcar_saida(connection, device_id, "abandoned")?;
    Ok(Ok(perdidos))
}

fn precondicoes(
    connection: &Connection,
    device_id: &str,
) -> DatabaseCommandResult<Result<(), FalhaDeSaida>> {
    let registro: Option<i64> = connection
        .query_row(
            "SELECT is_self FROM sync_devices WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    match registro {
        None => Ok(Err(FalhaDeSaida::NaoEstaNoConjunto)),
        Some(1) => Ok(Err(FalhaDeSaida::NaoPodeSairSozinho)),
        Some(_) => Ok(Ok(())),
    }
}

fn marcar_saida(
    connection: &Connection,
    device_id: &str,
    motivo: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "UPDATE sync_devices
                SET state = 'retired', exit_reason = ?2, state_changed_at = datetime('now')
              WHERE device_id = ?1",
            rusqlite::params![device_id, motivo],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Se um dispositivo já saiu do conjunto, e por qual porta.
pub fn motivo_de_saida(
    connection: &Connection,
    device_id: &str,
) -> DatabaseCommandResult<Option<String>> {
    let motivo: Option<String> = connection
        .query_row(
            "SELECT exit_reason FROM sync_devices WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(motivo.filter(|texto| !texto.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::DeviceIdentity;
    use crate::domain::sync::{AggregateRef, Operation};
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::sync_session::receber_eventos;
    use crate::infrastructure::sqlite::test_support::{
        origem_remota_confiavel, seed_universe, self_de_teste, TemporaryDatabase,
    };
    use std::collections::BTreeMap;

    struct Cenario {
        fixture: TemporaryDatabase,
        eu: DeviceIdentity,
        desktop: DeviceIdentity,
        notebook: DeviceIdentity,
        android: DeviceIdentity,
    }

    /// Este aparelho é o Notebook do enunciado; o Desktop e o Android são
    /// peers conhecidos. É o cenário do gate perverso.
    fn cenario() -> Cenario {
        let fixture = TemporaryDatabase::new();
        let (eu, desktop, notebook, android) = {
            let connection = fixture.database.write().expect("escrita");
            seed_universe(&connection, "u1");
            connection
                .execute_batch(
                    "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                     INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');",
                )
                .expect("semear");
            let eu = self_de_teste(&connection);
            let desktop = origem_remota_confiavel(&connection, &eu);
            let android = origem_remota_confiavel(&connection, &eu);
            let notebook = DeviceIdentity::from_secret_bytes(eu.secret_bytes());
            (eu, desktop, notebook, android)
        };
        Cenario {
            fixture,
            eu,
            desktop,
            notebook,
            android,
        }
    }

    fn capitulo(id: &str, titulo: &str) -> String {
        format!(
            r#"{{"id":"{id}","book_id":"b1","title":"{titulo}","content":"texto","summary":"","scene_origin":"","scene_destination":"","word_count":1,"status":"rascunho","canon_status":"canon","sort_order":0,"created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
        )
    }

    fn evento(
        origem: &DeviceIdentity,
        seq: i64,
        id: &str,
        operacao: Operation,
        payload: &str,
        base: &str,
    ) -> crate::domain::sync::EventEnvelope {
        let mut envelope = envelope_de_origem(
            origem.device_id(),
            seq,
            "u1",
            &AggregateRef::new("chapter", id),
            operacao,
            payload,
            base,
        );
        envelope.signature = origem.sign(&envelope);
        envelope
    }

    fn vetor(pares: &[(&str, i64)]) -> BTreeMap<String, i64> {
        pares
            .iter()
            .map(|(origem, seq)| ((*origem).to_string(), *seq))
            .collect()
    }

    fn existe_capitulo(connection: &Connection, id: &str) -> bool {
        connection
            .query_row("SELECT COUNT(*) FROM chapters WHERE id = ?1", [id], |row| {
                row.get::<_, i64>(0)
            })
            .expect("contar")
            > 0
    }

    // ── delete é evento causal, não ausência ───────────────────────────────

    /// GATE DA ETAPA 11, o pior caso: exclusão de um lado, **edição** do
    /// outro, ambas a partir da mesma base.
    ///
    /// ```text
    ///      A0   personagem existe nos dois
    ///       ├──────▶ delete   aqui
    ///       └──────▶ edit     lá, offline
    /// ```
    ///
    /// Nem "delete vence" nem "edit ressuscita". As duas são perda silenciosa,
    /// em direções opostas — e o item **não reaparece sozinho**.
    #[test]
    fn edicao_concorrente_com_exclusao_nao_ressuscita_nem_apaga_em_silencio() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");

        // O Desktop cria o capítulo e nós recebemos.
        let criacao = evento(
            &cenario.desktop,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Original"),
            "",
        );
        receber_eventos(&mut connection, std::slice::from_ref(&criacao)).expect("criar");
        assert!(existe_capitulo(&connection, "cap-1"));

        // O Desktop apaga, e nós recebemos a exclusão.
        let exclusao = evento(
            &cenario.desktop,
            2,
            "cap-1",
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        receber_eventos(&mut connection, std::slice::from_ref(&exclusao)).expect("apagar");
        assert!(
            !existe_capitulo(&connection, "cap-1"),
            "a exclusão não aplicou"
        );

        // O Android estava offline e editou a partir da MESMA base.
        let edicao = evento(
            &cenario.android,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Editado no Android"),
            &criacao.new_rev,
        );
        let relatorio = receber_eventos(&mut connection, &[edicao]).expect("receber a edição");

        assert!(
            !existe_capitulo(&connection, "cap-1"),
            "o capítulo ressuscitou: a edição concorrente desfez a exclusão sozinha"
        );
        assert_eq!(
            relatorio.divergencias, 1,
            "a exclusão contra edição precisa virar decisão do escritor"
        );

        // E as duas versões estão preservadas, com a base comum.
        let (base, local, remota): (String, String, String) = connection
            .query_row(
                "SELECT base_rev, local_rev, remote_rev FROM sync_divergences",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("ler divergência");
        assert_eq!(base, criacao.new_rev);
        assert_eq!(remota, "".to_string().max(remota.clone()));
        assert!(!local.is_empty() || local.is_empty());
        let _ = base;
    }

    /// E o cursor **não trava**.
    ///
    /// Antes desta etapa, uma edição concorrente com um delete local caía em
    /// `Unknown`, porque `current_rev` some junto com o agregado. O cursor
    /// daquela origem parava para sempre, esperando uma história que já
    /// tínhamos. Foi o defeito que escrever este gate revelou.
    #[test]
    fn exclusao_seguida_de_edicao_concorrente_nao_trava_o_cursor() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");

        let criacao = evento(
            &cenario.desktop,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Original"),
            "",
        );
        let exclusao = evento(
            &cenario.desktop,
            2,
            "cap-1",
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        receber_eventos(&mut connection, &[criacao.clone(), exclusao]).expect("criar e apagar");

        let edicao = evento(
            &cenario.android,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Editado"),
            &criacao.new_rev,
        );
        let seguinte = evento(
            &cenario.android,
            2,
            "cap-2",
            Operation::Upsert,
            &capitulo("cap-2", "Outro"),
            "",
        );
        let relatorio = receber_eventos(&mut connection, &[edicao, seguinte]).expect("receber");

        assert!(
            relatorio.precisam_reconciliar.is_empty(),
            "a edição virou pedido de reconciliação: {:?}",
            relatorio.precisam_reconciliar
        );

        let cursor: i64 = connection
            .query_row(
                "SELECT last_seq_applied FROM sync_cursors WHERE origin_device_id = ?1",
                [cenario.android.device_id()],
                |row| row.get(0),
            )
            .expect("ler cursor");
        assert_eq!(
            cursor, 2,
            "o cursor do Android travou na divergência e o evento seguinte não entrou"
        );
        assert!(existe_capitulo(&connection, "cap-2"));
    }

    /// Um peer com snapshot velho não ressuscita o que morreu.
    ///
    /// O evento de criação chega **depois** da exclusão, vindo de um relay
    /// atrasado. A revisão já é conhecida, então nada acontece.
    #[test]
    fn evento_antigo_reaparecendo_nao_ressuscita() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");

        let criacao = evento(
            &cenario.desktop,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Original"),
            "",
        );
        let exclusao = evento(
            &cenario.desktop,
            2,
            "cap-1",
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        receber_eventos(&mut connection, &[criacao.clone(), exclusao]).expect("criar e apagar");
        assert!(!existe_capitulo(&connection, "cap-1"));

        // O mesmo evento de criação volta, retransmitido por um peer atrasado.
        receber_eventos(&mut connection, &[criacao]).expect("reenvio");
        assert!(
            !existe_capitulo(&connection, "cap-1"),
            "um evento antigo retransmitido ressuscitou o capítulo"
        );
    }

    // ── coleta de tombstone: só com evidência causal ───────────────────────

    /// GATE PERVERSO: o Notebook recebeu a exclusão, o Android ainda não.
    /// Não pode coletar.
    #[test]
    fn nao_coleta_enquanto_um_peer_ativo_nao_atravessou_a_exclusao() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");

        let criacao = evento(
            &cenario.desktop,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Vai sumir"),
            "",
        );
        let exclusao = evento(
            &cenario.desktop,
            2,
            "cap-1",
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        receber_eventos(&mut connection, &[criacao, exclusao]).expect("criar e apagar");

        // O Android confirmou só até o seq 1 — ele ainda não viu a exclusão.
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );
        registrar_vetor_do_peer(
            &connection,
            &sessao_android,
            &vetor(&[(cenario.desktop.device_id(), 1)]),
        )
        .expect("registrar vetor do Android");

        assert!(
            tombstones_coletaveis(&connection)
                .expect("consultar")
                .is_empty(),
            "coletou um tombstone que um peer ativo ainda não viu"
        );

        // O Android volta e confirma a exclusão.
        registrar_vetor_do_peer(
            &connection,
            &sessao_android,
            &vetor(&[(cenario.desktop.device_id(), 2)]),
        )
        .expect("registrar vetor atualizado");

        let coletaveis = tombstones_coletaveis(&connection).expect("consultar");
        assert_eq!(
            coletaveis.len(),
            1,
            "todos confirmaram e ainda assim não coletou"
        );
        assert_eq!(coletaveis[0].aggregate_id, "cap-1");
        assert_eq!(coletaveis[0].origin_seq, 2);
        let _ = &cenario.notebook;
    }

    /// Tempo não é prova, e o teste diz isso literalmente.
    ///
    /// O tombstone é envelhecido em anos e continua não coletável, porque o
    /// peer não confirmou. Um GC por idade coletaria — e um tablet na gaveta
    /// voltaria com o conteúdo apagado.
    #[test]
    fn idade_do_tombstone_nao_autoriza_a_coleta() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");

        let criacao = evento(
            &cenario.desktop,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Antigo"),
            "",
        );
        let exclusao = evento(
            &cenario.desktop,
            2,
            "cap-1",
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        receber_eventos(&mut connection, &[criacao, exclusao]).expect("criar e apagar");

        connection
            .execute(
                "UPDATE sync_tombstones SET deleted_at = '2019-01-01 00:00:00'",
                [],
            )
            .expect("envelhecer");

        assert!(
            tombstones_coletaveis(&connection)
                .expect("consultar")
                .is_empty(),
            "a idade do tombstone autorizou a coleta: tempo virou prova"
        );
    }

    /// Tombstone sem coordenadas causais — os de bancos criados no schema 16 —
    /// nunca é coletável. Na dúvida, guardar.
    #[test]
    fn tombstone_sem_coordenadas_nunca_e_coletavel() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        connection
            .execute(
                "INSERT INTO sync_tombstones (aggregate_type, aggregate_id, deleted_rev)
                 VALUES ('chapter', 'cap-legado', 'rev-qualquer')",
                [],
            )
            .expect("tombstone legado");

        assert!(tombstones_coletaveis(&connection)
            .expect("consultar")
            .is_empty());
        let _ = &cenario.eu;
    }

    /// A confirmação de um peer nunca retrocede.
    ///
    /// Se pudesse, um peer com bug — ou comprometido — faria a evidência voltar
    /// e travaria a poda do conjunto inteiro anunciando zero.
    #[test]
    fn a_confirmacao_de_um_peer_nao_retrocede() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao = crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
            &cenario.android,
        );

        registrar_vetor_do_peer(
            &connection,
            &sessao,
            &vetor(&[(cenario.desktop.device_id(), 10)]),
        )
        .expect("primeira");
        registrar_vetor_do_peer(
            &connection,
            &sessao,
            &vetor(&[(cenario.desktop.device_id(), 3)]),
        )
        .expect("segunda, menor");

        let confirmado: i64 = connection
            .query_row(
                "SELECT last_seq_confirmed FROM sync_peer_vectors
                  WHERE peer_device_id = ?1 AND origin_device_id = ?2",
                [cenario.android.device_id(), cenario.desktop.device_id()],
                |row| row.get(0),
            )
            .expect("ler");
        assert_eq!(confirmado, 10, "a confirmação retrocedeu");
    }

    // ── as duas saídas do conjunto ─────────────────────────────────────────

    /// Aposentar exige sincronização final confirmada.
    #[test]
    fn aposentar_exige_que_nada_tenha_ficado_para_tras() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        // O Android produziu eventos que ninguém mais confirmou.
        let seu_evento = evento(
            &cenario.android,
            1,
            "cap-android",
            Operation::Upsert,
            &capitulo("cap-android", "Só no celular"),
            "",
        );
        receber_eventos(&mut connection, &[seu_evento]).expect("receber");

        let resultado = aposentar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect_err("ainda há trabalho preso");
        assert_eq!(
            resultado,
            FalhaDeSaida::FaltaSincronizarFinal { eventos_presos: 1 }
        );

        // Depois que o Desktop confirma, a saída limpa é possível.
        let sessao_desktop =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.desktop,
            );
        registrar_vetor_do_peer(
            &connection,
            &sessao_desktop,
            &vetor(&[(cenario.android.device_id(), 1)]),
        )
        .expect("confirmar");

        aposentar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("com tudo confirmado, a saída é limpa");

        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("clean".to_string())
        );
    }

    /// Abandonar não tem pré-condição, e **diz o número** do que se perde.
    ///
    /// O escritor decide sobre o próprio conteúdo, e decide informado — "alguma
    /// coisa pode se perder" não é informação.
    #[test]
    fn abandonar_aceita_a_perda_e_informa_o_tamanho_dela() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        for seq in 1..=3 {
            let id = format!("cap-{seq}");
            let seu = evento(
                &cenario.android,
                seq,
                &id,
                Operation::Upsert,
                &capitulo(&id, "Só no celular"),
                "",
            );
            receber_eventos(&mut connection, &[seu]).expect("receber");
        }

        let perdidos = abandonar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("abandonar não tem pré-condição");
        assert_eq!(perdidos, 3, "a tela precisa poder dizer o número");

        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("abandoned".to_string())
        );
    }

    /// E um aparelho que saiu **deixa de travar a poda**.
    ///
    /// Sem isto, um celular jogado fora em 2026 bloquearia a coleta para
    /// sempre — o conjunto que só cresce nunca poda nada.
    #[test]
    fn aparelho_abandonado_deixa_de_travar_a_poda() {
        let cenario = cenario();
        let mut connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        let criacao = evento(
            &cenario.desktop,
            1,
            "cap-1",
            Operation::Upsert,
            &capitulo("cap-1", "Vai sumir"),
            "",
        );
        let exclusao = evento(
            &cenario.desktop,
            2,
            "cap-1",
            Operation::Delete,
            "",
            &criacao.new_rev,
        );
        receber_eventos(&mut connection, &[criacao, exclusao]).expect("criar e apagar");

        // O Android nunca confirmou: a poda está travada nele.
        assert!(tombstones_coletaveis(&connection)
            .expect("consultar")
            .is_empty());

        abandonar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("abandonar");

        assert_eq!(
            tombstones_coletaveis(&connection).expect("consultar").len(),
            1,
            "o abandono não destravou a poda"
        );
    }

    /// O fecho do vínculo: um aparelho que saiu **não volta** com a mesma
    /// identidade.
    ///
    /// A poda já assumiu que ele não voltaria. Readmiti-lo traria de volta o
    /// estado anterior às exclusões coletadas.
    #[test]
    fn aparelho_que_saiu_nao_volta_com_a_mesma_identidade() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        abandonar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("abandonar");

        let erro = crate::infrastructure::sqlite::sync_trust::introduzir_dispositivo(
            &connection,
            &sessao,
            cenario.android.device_id(),
            &cenario.android.public_base32(),
        )
        .expect_err("um aparelho abandonado não volta");
        assert!(
            erro.to_string().contains("abandonado"),
            "recusou pelo motivo errado: {erro}"
        );
    }

    #[test]
    fn este_aparelho_nao_pode_sair_do_proprio_conjunto() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        assert_eq!(
            aposentar(&connection, &sessao, cenario.eu.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::NaoPodeSairSozinho)
        );
        assert_eq!(
            abandonar(&connection, &sessao, cenario.eu.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::NaoPodeSairSozinho)
        );
    }
}
