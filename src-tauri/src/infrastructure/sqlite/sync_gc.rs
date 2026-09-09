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

/// Até onde **este** aparelho conhece a sequência de uma origem.
///
/// Isto não é o high-water mark daquele aparelho: é só o que nós vimos. A
/// distância entre as duas coisas é exatamente o buraco que a revisão 11.1
/// fechou, e por isso a função tem este nome e não `eventos_nao_descarregados`.
pub fn conhecido_ate(connection: &Connection, device_id: &str) -> DatabaseCommandResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM sync_events WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

/// A maior confirmação que existe para uma origem, vinda de alguém que não é
/// ela própria: o cursor deste aparelho, ou o vetor de um peer ainda `active`.
///
/// Um dispositivo confirmando os próprios eventos não é evidência de nada.
fn melhor_confirmacao(connection: &Connection, origem: &str) -> DatabaseCommandResult<i64> {
    let nosso_cursor: i64 = connection
        .query_row(
            "SELECT COALESCE(last_seq_applied, 0) FROM sync_cursors WHERE origin_device_id = ?1",
            [origem],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .unwrap_or(0);

    let de_peer: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(v.last_seq_confirmed), 0)
               FROM sync_peer_vectors v
               JOIN sync_devices d ON d.device_id = v.peer_device_id
              WHERE v.origin_device_id = ?1
                AND d.state = 'active'
                AND d.device_id <> ?1",
            [origem],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    Ok(nosso_cursor.max(de_peer))
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeSaida {
    /// O aparelho declarou ter escrito até `declarado`, e ninguém no conjunto
    /// confirma ter recebido tudo isso.
    ///
    /// A UI precisa oferecer as duas coisas aqui: sincronizar mais uma vez, ou
    /// **abandonar** aceitando a perda. Escolher por conta própria seria
    /// decidir sobre o conteúdo do escritor.
    FaltaSincronizarFinal {
        declarado: i64,
        confirmado_ate: i64,
    },
    /// O aparelho declarou um high-water mark **menor** do que o que já
    /// conhecemos dele.
    ///
    /// Ou o estado dele regrediu — restauração de um backup velho, banco
    /// corrompido — ou é uma declaração conveniente, cortada para caber na
    /// prova. Nos dois casos a saída limpa não pode acontecer.
    MarcaDeclaradaAbaixoDoConhecido {
        declarado: i64,
        conhecido: i64,
    },
    NaoEstaNoConjunto,
    /// O aparelho **já saiu** do conjunto, e sair não se desfaz.
    ///
    /// Sem isto, as três portas viravam três formas de reescrever a saída de
    /// quem já tinha saído:
    ///
    /// ```text
    /// retired/'clean'  --abandonar-->  retired/'abandoned'
    /// retired/'clean'  --revogar---->  revoked/''
    /// ```
    ///
    /// Nenhum dos dois é recusado por incoerência de estado — os dois lados são
    /// combinações estruturalmente válidas, e é justamente por isso que a
    /// migration 18 não pega o caso. O que se perde é o **registro de como o
    /// aparelho saiu**: uma aposentadoria que teve prova de sincronização final
    /// vira, sem aviso, um abandono com perda aceita.
    DispositivoJaSaiu {
        estado: String,
        motivo: String,
    },
    /// Aposentar a si mesmo deixaria o aparelho sem conseguir gravar.
    NaoPodeSairSozinho,
    /// Quem pediu a mudança de estado não é membro ativo do conjunto.
    ///
    /// Abandonar e revogar são decisões sobre outro aparelho. Um dispositivo já
    /// aposentado, revogado ou desconhecido não as toma por ninguém.
    QuemDecideNaoEMembroAtivo,
}

impl std::fmt::Display for FalhaDeSaida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDeSaida::FaltaSincronizarFinal {
                declarado,
                confirmado_ate,
            } => write!(
                f,
                "Este aparelho diz ter escrito até a alteração {declarado}, e o conjunto só \
                 confirmou ter recebido até a {confirmado_ate}. Sincronize uma última vez antes \
                 de aposentá-lo — ou use \"abandonar\", aceitando que o que falta se perde."
            ),
            FalhaDeSaida::MarcaDeclaradaAbaixoDoConhecido {
                declarado,
                conhecido,
            } => write!(
                f,
                "O aparelho declarou ter escrito até {declarado}, mas este conjunto já conhece \
                 alterações dele até {conhecido}. A aposentadoria limpa exige que ele saiba de \
                 tudo que produziu."
            ),
            FalhaDeSaida::NaoEstaNoConjunto => f.write_str("O dispositivo não está no conjunto."),
            FalhaDeSaida::DispositivoJaSaiu { estado, motivo } => {
                let como = match motivo.as_str() {
                    "" => String::new(),
                    outro => format!(" ({outro})"),
                };
                write!(
                    f,
                    "Este aparelho já saiu do conjunto: {estado}{como}. Uma saída não se \
                     desfaz nem se reescreve — se ele voltar a ser usado, entra como \
                     identidade nova."
                )
            }
            FalhaDeSaida::NaoPodeSairSozinho => f.write_str(
                "Este aparelho não pode se aposentar: ele deixaria de conseguir gravar as \
                 próprias alterações.",
            ),
            FalhaDeSaida::QuemDecideNaoEMembroAtivo => {
                f.write_str("Só um dispositivo ativo do conjunto pode abandonar ou revogar outro.")
            }
        }
    }
}

/// Saída limpa: o **próprio dispositivo que sai** prova até onde escreveu.
///
/// # O buraco que esta assinatura fechou
///
/// A etapa 11 recebia um `device_id` solto e comparava duas coisas que *nós*
/// sabemos:
///
/// ```text
/// MAX(seq) que ESTE aparelho conhece do Android   = 100
/// melhor confirmação de outro peer ativo          = 100
///                                     ──────────────────
///                                     0 presos → saída limpa
/// ```
///
/// E o Android tinha 101..105 gravados offline, que ninguém aqui jamais viu. A
/// conta dava zero **porque a ignorância era simétrica** — e a aposentadoria
/// limpa apagava do conjunto um aparelho com cinco alterações que nunca mais
/// sairiam dele. `retired`/`clean` significa *acabou*; ali significaria
/// *desistimos de saber*.
///
/// # A forma da prova
///
/// O parâmetro é a [`SessaoAutenticada`] **de quem está saindo**, não um
/// identificador que a camada de aplicação escolhe. Não existe chamada possível
/// em que um peer aposente outro como limpo: o dispositivo é
/// `sessao.device_id()`, e a sessão só existe porque a chave Ed25519
/// correspondente assinou o hash do handshake.
///
/// ```text
/// 1. quem sai declara, dentro da sessão autenticada, seu high-water mark
/// 2. o valor declarado não pode ser MENOR do que o que já conhecemos dele
/// 3. alguém que não é ele confirma ter recebido até aquele ponto
/// ```
///
/// O passo 2 é o que impede a declaração conveniente: um aparelho não encolhe o
/// próprio passado para caber na prova. O passo 3 é o que impede a saída limpa
/// por ignorância: a confirmação vem do nosso cursor ou do vetor de um peer
/// ainda válido, nunca da própria origem.
pub fn aposentar_clean(
    connection: &Connection,
    sessao_de_quem_sai: &SessaoAutenticada,
    high_water_mark: i64,
) -> DatabaseCommandResult<Result<(), FalhaDeSaida>> {
    let device_id = sessao_de_quem_sai.device_id();

    if let Err(falha) = precondicoes(connection, device_id)? {
        return Ok(Err(falha));
    }

    let conhecido = conhecido_ate(connection, device_id)?;
    if high_water_mark < conhecido {
        return Ok(Err(FalhaDeSaida::MarcaDeclaradaAbaixoDoConhecido {
            declarado: high_water_mark,
            conhecido,
        }));
    }

    let confirmado_ate = melhor_confirmacao(connection, device_id)?;
    if confirmado_ate < high_water_mark {
        return Ok(Err(FalhaDeSaida::FaltaSincronizarFinal {
            declarado: high_water_mark,
            confirmado_ate,
        }));
    }

    marcar_saida(connection, device_id, "clean")?;
    Ok(Ok(()))
}

/// Abandono: sem prova de sincronização, com a perda aceita **e medida**.
///
/// Aqui a sessão é a de **quem decide**, não a de quem sai — o aparelho perdido
/// não liga mais, então ele não assina coisa nenhuma. Quem decide precisa ser
/// membro `active`: abandonar é decidir sobre o conteúdo do escritor
/// ([ADR 0001](0001-local-ownership.md)), e um aparelho já fora do conjunto não
/// decide isso.
///
/// Devolve quantos eventos **conhecidos** ficam sem confirmação. É um piso, não
/// um total: o aparelho pode ter escrito coisas que nunca chegaram aqui, e por
/// definição não há como contá-las. A tela precisa dizer "pelo menos N".
pub fn abandonar(
    connection: &Connection,
    sessao_de_quem_decide: &SessaoAutenticada,
    device_id: &str,
) -> DatabaseCommandResult<Result<i64, FalhaDeSaida>> {
    if !e_membro_ativo(connection, sessao_de_quem_decide.device_id())? {
        return Ok(Err(FalhaDeSaida::QuemDecideNaoEMembroAtivo));
    }
    if let Err(falha) = precondicoes(connection, device_id)? {
        return Ok(Err(falha));
    }

    let conhecido = conhecido_ate(connection, device_id)?;
    let confirmado = melhor_confirmacao(connection, device_id)?;
    marcar_saida(connection, device_id, "abandoned")?;
    Ok(Ok((conhecido - confirmado).max(0)))
}

/// Revogação: a chave daquele aparelho não é mais confiável.
///
/// Não é uma terceira porta de saída. Aposentar e abandonar falam sobre
/// *dados* — o que ficou para trás. Revogar fala sobre *identidade*: o aparelho
/// foi roubado, ou a chave vazou, e nada que ele assinar daqui em diante vale.
/// Por isso `revoked` nunca carrega motivo de saída: não houve saída, houve
/// corte.
pub fn revogar(
    connection: &Connection,
    sessao_de_quem_decide: &SessaoAutenticada,
    device_id: &str,
) -> DatabaseCommandResult<Result<(), FalhaDeSaida>> {
    if !e_membro_ativo(connection, sessao_de_quem_decide.device_id())? {
        return Ok(Err(FalhaDeSaida::QuemDecideNaoEMembroAtivo));
    }
    if let Err(falha) = precondicoes(connection, device_id)? {
        return Ok(Err(falha));
    }

    let linhas = connection
        .execute(
            "UPDATE sync_devices
                SET state = 'revoked', exit_reason = '', state_changed_at = datetime('now')
              WHERE device_id = ?1
                AND state = 'active'",
            [device_id],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if linhas != 1 {
        return Err(DatabaseCommandError::validation(format!(
            "Revogação recusada: {device_id} não estava ativo no conjunto."
        )));
    }
    Ok(Ok(()))
}

fn e_membro_ativo(connection: &Connection, device_id: &str) -> DatabaseCommandResult<bool> {
    let estado: Option<String> = connection
        .query_row(
            "SELECT state FROM sync_devices WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(estado.as_deref() == Some("active"))
}

/// O que toda porta de saída exige do **alvo**, antes de qualquer prova.
///
/// O estado entrou aqui na revisão 11.2. Sem ele, `sair` era uma operação que
/// se podia repetir: um aparelho já aposentado com prova de sincronização final
/// podia ser abandonado por cima, e o registro de que a saída tinha sido limpa
/// desaparecia num `UPDATE` que ninguém consideraria perigoso.
fn precondicoes(
    connection: &Connection,
    device_id: &str,
) -> DatabaseCommandResult<Result<(), FalhaDeSaida>> {
    let registro: Option<(i64, String, String)> = connection
        .query_row(
            "SELECT is_self, state, exit_reason FROM sync_devices WHERE device_id = ?1",
            [device_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    match registro {
        None => Ok(Err(FalhaDeSaida::NaoEstaNoConjunto)),
        Some((1, _, _)) => Ok(Err(FalhaDeSaida::NaoPodeSairSozinho)),
        Some((_, estado, motivo)) if estado != "active" => {
            Ok(Err(FalhaDeSaida::DispositivoJaSaiu { estado, motivo }))
        }
        Some(_) => Ok(Ok(())),
    }
}

/// Escreve a saída, e **só a partir de `active`**.
///
/// O `AND state = 'active'` repete o que [`precondicoes`] já checou, de
/// propósito. As duas verificações protegem coisas diferentes: a de cima
/// devolve um erro que a tela sabe explicar; esta impede que um caminho futuro
/// — um comando novo, uma correção com pressa — chegue ao `UPDATE` sem passar
/// por ela. Estado terminal por convenção não é estado terminal.
fn marcar_saida(
    connection: &Connection,
    device_id: &str,
    motivo: &str,
) -> DatabaseCommandResult<()> {
    let linhas = connection
        .execute(
            "UPDATE sync_devices
                SET state = 'retired', exit_reason = ?2, state_changed_at = datetime('now')
              WHERE device_id = ?1
                AND state = 'active'",
            rusqlite::params![device_id, motivo],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if linhas != 1 {
        return Err(DatabaseCommandError::validation(format!(
            "Saída recusada: {device_id} não estava ativo no conjunto. Uma saída não se \
             reescreve — o registro de como o aparelho saiu se perderia."
        )));
    }
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
        let edicao_rev = edicao.new_rev.clone();
        let relatorio = receber_eventos(&mut connection, &[edicao]).expect("receber a edição");

        assert!(
            !existe_capitulo(&connection, "cap-1"),
            "o capítulo ressuscitou: a edição concorrente desfez a exclusão sozinha"
        );
        assert_eq!(
            relatorio.divergencias, 1,
            "a exclusão contra edição precisa virar decisão do escritor"
        );

        // E a divergência registrada descreve a escolha REAL do escritor:
        // de que ponto os dois lados partiram, o que cada lado fez, e para
        // onde cada um foi. Sem os três, a caixa de conciliação não sabe qual
        // par de botões oferecer.
        let (base, local, remota, op_local, op_remota): (String, String, String, String, String) =
            connection
                .query_row(
                    "SELECT base_rev, local_rev, remote_rev, local_operation, remote_operation
                   FROM sync_divergences",
                    [],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .expect("ler divergência");

        assert_eq!(base, criacao.new_rev, "a base comum não é a criação");
        assert_eq!(
            local, exclusao.new_rev,
            "o lado local precisa ser a revisão DA EXCLUSÃO — exclusão é revisão \
             causal, não ausência de uma"
        );
        assert_eq!(
            op_local, "delete",
            "a tela não saberia que aqui foi apagado"
        );
        assert_eq!(
            remota, edicao_rev,
            "o lado remoto não é a edição do Android"
        );
        assert_eq!(op_remota, "upsert");
        assert!(
            !existe_capitulo(&connection, "cap-1"),
            "o agregado precisa continuar excluído até o humano decidir"
        );
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

    /// Prepara o Android com **um** evento que nós recebemos, e o Desktop
    /// confirmando esse mesmo evento.
    ///
    /// É o estado em que a conta antiga dava "zero pendentes": tudo o que este
    /// aparelho conhece do Android já foi confirmado por outro peer ativo.
    fn android_com_um_evento_confirmado(cenario: &Cenario) {
        let mut connection = cenario.fixture.database.write().expect("escrita");
        let seu_evento = evento(
            &cenario.android,
            1,
            "cap-android",
            Operation::Upsert,
            &capitulo("cap-android", "Só no celular"),
            "",
        );
        receber_eventos(&mut connection, &[seu_evento]).expect("receber");

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
    }

    /// **O gate central da revisão 11.1.**
    ///
    /// O Android escreveu até a alteração 5 no avião. Este aparelho viu a 1, e
    /// o Desktop confirmou a 1. Pela conta antiga:
    ///
    /// ```text
    /// MAX(seq) conhecido = 1     confirmado por peer = 1     presos = 0
    /// ```
    ///
    /// Zero presos, saída limpa, e as alterações 2..5 somem do mundo. A conta
    /// dava zero **porque a ignorância era simétrica** — nenhum dos dois lados
    /// tinha como saber do que faltava.
    ///
    /// Com a marca declarada pelo próprio aparelho que sai, a mentira fica
    /// impossível de contar por omissão: ele diz 5, e o conjunto só consegue
    /// provar 1.
    #[test]
    fn saida_limpa_recusa_quando_o_conjunto_nao_alcancou_a_marca_de_quem_sai() {
        let cenario = cenario();
        android_com_um_evento_confirmado(&cenario);
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );

        let falha = aposentar_clean(&connection, &sessao_android, 5)
            .expect("consultar")
            .expect_err("o conjunto só alcançou a alteração 1");
        assert_eq!(
            falha,
            FalhaDeSaida::FaltaSincronizarFinal {
                declarado: 5,
                confirmado_ate: 1,
            }
        );
        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            None,
            "o aparelho saiu como limpo mesmo sem prova"
        );
    }

    /// E a marca declarada não pode **encolher o próprio passado**.
    ///
    /// Sem esta checagem, a prova viraria teatro: o aparelho que quer sair
    /// declara zero, o conjunto confirma zero, e a saída é "limpa". Um valor
    /// menor do que o que já conhecemos dele é sempre um destes dois — estado
    /// regredido por restauração de backup velho, ou declaração cortada para
    /// caber. Nenhum dos dois autoriza uma saída limpa.
    #[test]
    fn marca_declarada_nao_pode_ser_menor_que_o_ja_conhecido() {
        let cenario = cenario();
        android_com_um_evento_confirmado(&cenario);
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );

        let falha = aposentar_clean(&connection, &sessao_android, 0)
            .expect("consultar")
            .expect_err("declarou menos do que já sabemos dele");
        assert_eq!(
            falha,
            FalhaDeSaida::MarcaDeclaradaAbaixoDoConhecido {
                declarado: 0,
                conhecido: 1,
            }
        );
    }

    /// O caminho correto: a marca declarada bate com o que o conjunto provou.
    #[test]
    fn saida_limpa_acontece_quando_a_prova_fecha() {
        let cenario = cenario();
        android_com_um_evento_confirmado(&cenario);
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );

        aposentar_clean(&connection, &sessao_android, 1)
            .expect("consultar")
            .expect("a marca declarada está confirmada");

        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("clean".to_string())
        );
        assert_eq!(
            motivo_de_saida(&connection, cenario.desktop.device_id()).expect("ler"),
            None,
            "a saída de um aparelho mexeu no estado de outro"
        );
    }

    /// A confirmação **não pode vir da própria origem**.
    ///
    /// Um aparelho que confirma os próprios eventos não provou nada: é ele
    /// dizendo que recebeu o que ele mesmo escreveu. Se isso contasse, a saída
    /// limpa seria auto-outorgada e todo o resto do gate seria decoração.
    #[test]
    fn o_proprio_aparelho_nao_confirma_a_propria_saida() {
        let cenario = cenario();
        {
            let mut connection = cenario.fixture.database.write().expect("escrita");
            let seu_evento = evento(
                &cenario.android,
                1,
                "cap-android",
                Operation::Upsert,
                &capitulo("cap-android", "Só no celular"),
                "",
            );
            receber_eventos(&mut connection, &[seu_evento]).expect("receber");
        }

        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );

        // O Android confirma os próprios eventos até a 9.
        registrar_vetor_do_peer(
            &connection,
            &sessao_android,
            &vetor(&[(cenario.android.device_id(), 9)]),
        )
        .expect("registrar");

        let falha = aposentar_clean(&connection, &sessao_android, 9)
            .expect("consultar")
            .expect_err("auto-confirmação não é prova");
        assert_eq!(
            falha,
            FalhaDeSaida::FaltaSincronizarFinal {
                declarado: 9,
                confirmado_ate: 1,
            },
            "a confirmação da própria origem entrou na conta"
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

        // Ninguém mais confirmou nada do Android: os três eventos ficam presos.
        connection
            .execute(
                "DELETE FROM sync_cursors WHERE origin_device_id = ?1",
                [cenario.android.device_id()],
            )
            .expect("simular que nem nós tínhamos aplicado");

        let perdidos = abandonar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("abandonar não exige prova");
        assert_eq!(perdidos, 3, "a tela precisa poder dizer o número");

        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("abandoned".to_string())
        );
    }

    /// Abandonar é uma decisão de **perda de dados**, e só um membro ativo a
    /// toma.
    ///
    /// Um aparelho que já saiu do conjunto — ou que foi revogado porque a chave
    /// vazou — continuaria conseguindo derrubar os outros. É a mesma classe de
    /// erro do `quem_introduz: &str` da etapa 7: autoridade sem pertencimento.
    #[test]
    fn quem_ja_saiu_nao_abandona_ninguem() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_eu =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);
        let sessao_desktop =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.desktop,
            );

        abandonar(&connection, &sessao_eu, cenario.desktop.device_id())
            .expect("consultar")
            .expect("abandonar o Desktop");

        assert_eq!(
            abandonar(&connection, &sessao_desktop, cenario.android.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::QuemDecideNaoEMembroAtivo)
        );
        assert_eq!(
            revogar(&connection, &sessao_desktop, cenario.android.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::QuemDecideNaoEMembroAtivo)
        );
        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            None,
            "um aparelho que já saiu derrubou outro"
        );
    }

    /// Revogar fala de **identidade**, não de dados — e por isso não inventa
    /// um motivo de saída.
    ///
    /// `revoked` com `exit_reason = 'clean'` seria um estado que nenhuma parte
    /// do código sabe ler: a chave vazou, mas o aparelho consta como tendo
    /// saído em ordem. A migration 18 proíbe a combinação no banco; este gate
    /// prova que o caminho normal não tenta produzi-la.
    #[test]
    fn revogar_nao_produz_motivo_de_saida() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        revogar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("revogar");

        let estado: String = connection
            .query_row(
                "SELECT state FROM sync_devices WHERE device_id = ?1",
                [cenario.android.device_id()],
                |row| row.get(0),
            )
            .expect("ler estado");
        assert_eq!(estado, "revoked");
        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            None,
            "revogação não é uma porta de saída e não carrega motivo"
        );
    }

    /// Motivo de saída **só existe para quem saiu** (migration 18).
    ///
    /// A v17 já proibia `active` com motivo de saída, e parava aí. O estado que
    /// continuava representável era o do meio:
    ///
    /// ```text
    /// state = 'revoked'   exit_reason = 'clean'
    /// ```
    ///
    /// A chave vazou, e o aparelho consta como tendo saído em ordem. As duas
    /// coisas não podem ser verdade ao mesmo tempo, e nenhuma tela saberia qual
    /// das duas obedecer — a que decide sobre confiança, ou a que decide sobre
    /// retenção.
    ///
    /// Este gate escreve direto no banco de propósito. As portas de saída em
    /// Rust nunca produzem essa combinação; o que se prova aqui é que **o banco
    /// não a aceita nem quando o Rust é contornado**.
    #[test]
    fn dispositivo_revogado_nao_carrega_motivo_de_saida() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");

        let erro = connection
            .execute(
                "UPDATE sync_devices SET state = 'revoked', exit_reason = 'clean'
                  WHERE device_id = ?1",
                [cenario.android.device_id()],
            )
            .expect_err("revogado com motivo de saída limpa é um estado incoerente");
        assert!(
            erro.to_string().contains("Motivo de saida so existe"),
            "recusou pelo motivo errado: {erro}"
        );

        // E o `active` com motivo, que a v17 já pegava, continua pego.
        connection
            .execute(
                "UPDATE sync_devices SET exit_reason = 'abandoned' WHERE device_id = ?1",
                [cenario.android.device_id()],
            )
            .expect_err("um dispositivo ativo não tem motivo de saída");
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
            aposentar_clean(&connection, &sessao, 0)
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
        assert_eq!(
            revogar(&connection, &sessao, cenario.eu.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::NaoPodeSairSozinho)
        );
    }

    /// **Sair não se desfaz.** Uma aposentadoria limpa não vira abandono.
    ///
    /// Este é o caso que a migration 18 **não** pega, e vale entender por quê:
    /// as duas pontas são estados estruturalmente válidos.
    ///
    /// ```text
    /// retired / 'clean'      ← legal
    /// retired / 'abandoned'  ← legal
    /// ```
    ///
    /// Nenhum gatilho de coerência tem o que reclamar. O que se perde está na
    /// transição, não nos estados: `clean` foi conquistado com prova de
    /// sincronização final, e `abandoned` é a declaração de que o escritor
    /// aceitou perder o que só existia ali. Sobrescrever um pelo outro apaga o
    /// registro de qual das duas coisas aconteceu — e é o registro que a tela
    /// usa para dizer ao escritor se ele perdeu alguma coisa.
    #[test]
    fn aposentado_limpo_nao_pode_ser_abandonado() {
        let cenario = cenario();
        android_com_um_evento_confirmado(&cenario);
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_eu =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );

        aposentar_clean(&connection, &sessao_android, 1)
            .expect("consultar")
            .expect("a prova fecha");

        assert_eq!(
            abandonar(&connection, &sessao_eu, cenario.android.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::DispositivoJaSaiu {
                estado: "retired".to_string(),
                motivo: "clean".to_string(),
            })
        );
        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("clean".to_string()),
            "o abandono reescreveu uma saída que tinha sido limpa"
        );
    }

    /// E também não vira revogação.
    ///
    /// `revoked` fala de identidade, não de dados, e por isso limpa o motivo de
    /// saída (migration 18). Aplicá-lo sobre um aparelho já aposentado apagaria
    /// justamente o registro de como ele saiu, em nome de uma informação que
    /// não muda nada: um aparelho fora do conjunto já não tem eventos aceitos.
    #[test]
    fn aposentado_nao_pode_ser_revogado() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        abandonar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("abandonar");

        assert_eq!(
            revogar(&connection, &sessao, cenario.android.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::DispositivoJaSaiu {
                estado: "retired".to_string(),
                motivo: "abandoned".to_string(),
            })
        );
        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("abandoned".to_string()),
            "a revogação apagou o registro de como o aparelho saiu"
        );
    }

    /// E um aparelho revogado não vira abandonado.
    ///
    /// Aqui a perda seria no outro sentido: `revoked` diz que a chave caiu em
    /// mãos erradas, e o que veio dela merece revisão humana (ADR 0009 §5.1).
    /// Reescrever para `retired`/`abandoned` transformaria um incidente de
    /// segurança num aparelho que simplesmente foi trocado.
    #[test]
    fn revogado_nao_pode_ser_abandonado() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        revogar(&connection, &sessao, cenario.android.device_id())
            .expect("consultar")
            .expect("revogar");

        assert_eq!(
            abandonar(&connection, &sessao, cenario.android.device_id())
                .expect("consultar")
                .err(),
            Some(FalhaDeSaida::DispositivoJaSaiu {
                estado: "revoked".to_string(),
                motivo: String::new(),
            })
        );

        let estado: String = connection
            .query_row(
                "SELECT state FROM sync_devices WHERE device_id = ?1",
                [cenario.android.device_id()],
                |row| row.get(0),
            )
            .expect("ler estado");
        assert_eq!(
            estado, "revoked",
            "o abandono rebaixou um incidente de segurança a troca de aparelho"
        );
    }

    /// **A defesa em profundidade, provada sem passar pela porta da frente.**
    ///
    /// Os três gates acima entram por `abandonar`/`revogar` e param na
    /// pré-condição. Este chama `marcar_saida` direto — que é o que um comando
    /// novo, ou uma correção com pressa, faria sem querer.
    ///
    /// Se o único guarda fosse a pré-condição, o estado terminal seria terminal
    /// **por convenção**: bastaria alguém alcançar o `UPDATE` por outro caminho.
    /// O `AND state = 'active'` no próprio SQL é o que torna a convenção uma
    /// regra.
    #[test]
    fn saida_nao_pode_ser_reescrita_nem_por_dentro() {
        let cenario = cenario();
        android_com_um_evento_confirmado(&cenario);
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao_android =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &cenario.android,
            );

        aposentar_clean(&connection, &sessao_android, 1)
            .expect("consultar")
            .expect("a prova fecha");

        let erro = marcar_saida(&connection, cenario.android.device_id(), "abandoned")
            .expect_err("o UPDATE precisa se defender sozinho");
        assert!(
            erro.to_string().contains("não estava ativo"),
            "recusou pelo motivo errado: {erro}"
        );
        assert_eq!(
            motivo_de_saida(&connection, cenario.android.device_id()).expect("ler"),
            Some("clean".to_string())
        );
    }

    /// **A NH-058 só é declarada fechada quando a saída realmente propagar.**
    ///
    /// A primeira versão deste gate procurava `append_local_event` por regex
    /// dentro de `sync_gc.rs`. Era evidência indireta, e das ruins: uma chamada
    /// daquele nome feita para qualquer outra coisa marcaria a tarefa como
    /// pronta, e a emissão correta encapsulada noutro módulo a marcaria como
    /// ausente. Presença de identificador não é comportamento — que é
    /// exatamente o vício que a revisão 11.1 encontrou nos gates da etapa 11.
    ///
    /// Agora a pergunta é feita ao **log**, que é onde a propagação existiria:
    ///
    /// ```text
    /// TASKS.md diz DONE   →  abandonar() TEM que deixar linha em sync_events
    /// ```
    ///
    /// # A implicação é de via única, e isso é o ponto
    ///
    /// A primeira versão condicionava pelo log e cobrava o status dos dois
    /// lados: emitiu evento ⇒ tem que estar `DONE`. Está invertido em relação
    /// à regra que o próprio `TASKS.md` documenta, onde a emissão é a
    /// **primeira de seis** condições — faltam ainda B aplicar, C receber por
    /// B, idempotência, regra para evento antigo depois de `revoked`, e
    /// desempate determinístico.
    ///
    /// O estrago apareceria na NH-053, que vai ser feita em partes:
    ///
    /// ```text
    /// commit 1  abandonar() passa a emitir o evento   ← log cresce
    /// commit 2  sync_apply aprende a aplicá-lo
    /// commit 3  store-and-forward
    /// commit 4  idempotência e concorrência
    /// ```
    ///
    /// No commit 1 o gate veria o log crescer e **exigiria** `Status: DONE` —
    /// com cinco propriedades ainda por escrever. O gate feito para impedir
    /// fechamento prematuro passaria a forçá-lo. É o mesmo padrão das três
    /// revisões anteriores, encontrado antes de custar caro: gate verde
    /// provando coisa diferente do que o nome promete.
    ///
    /// Então: `DONE` obriga emissão; emissão **não** autoriza `DONE`. Quem
    /// autoriza são as propriedades comportamentais completas, e elas só podem
    /// ser escritas quando o evento existir.
    ///
    /// # O que este gate ainda NÃO prova
    ///
    /// Que a saída *chega* aos outros peers. Isso exige as propriedades que só
    /// podem ser escritas quando o evento existir, e que estão registradas como
    /// critério de aceitação da NH-058 no `TASKS.md`:
    ///
    /// ```text
    /// B recebe o evento          →  Android vira retired no banco de B
    /// C recebe depois, por B     →  e no banco de C também
    /// evento repetido            →  idempotente
    /// duas decisões concorrentes →  desempate determinístico
    /// ```
    ///
    /// Este gate é a trava que impede alguém de declarar a tarefa pronta antes
    /// disso. Ele não é a prova da tarefa.
    #[test]
    fn a_nh_058_so_e_declarada_fechada_quando_a_saida_propagar() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&cenario.eu);

        let conta_eventos = || -> i64 {
            connection
                .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
                .expect("contar eventos")
        };

        let antes = conta_eventos();
        abandonar(&connection, &sessao, cenario.desktop.device_id())
            .expect("consultar")
            .expect("abandonar");
        let propaga = conta_eventos() > antes;

        let tarefas = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../TASKS.md"),
        )
        .expect("ler o TASKS.md");
        let secao = tarefas
            .split("### NH-058")
            .nth(1)
            .expect("não achei a seção da NH-058 no TASKS.md")
            .split("\n### ")
            .next()
            .expect("seção vazia")
            .to_string();

        // O status é lido da **linha** que o declara, e não com um `contains`
        // solto no texto da seção. Três coisas mudam com isso, e todas são
        // sobre o gate continuar valendo enquanto o documento evolui:
        //
        //   - prosa que cite `Status: DONE` como exemplo deixa de decidir nada;
        //   - duas linhas de status na mesma seção reprovam, em vez de a
        //     primeira ganhar em silêncio;
        //   - nenhuma linha de status reprova, em vez de cair no `else` e
        //     passar por parecer PARCIAL.
        //
        // Repare que a linha carrega prosa depois do valor — no `TASKS.md` os
        // status são escritos como `Status: PARCIAL — as duas portas...`. Por
        // isso o que se compara é o **primeiro token**, e não a linha inteira:
        // um `^Status:\s*(DONE|PARCIAL)\s*$` não casaria com nada aqui.
        let declaracoes: Vec<&str> = secao
            .lines()
            .filter(|linha| linha.trim_start().starts_with("Status:"))
            .collect();
        assert_eq!(
            declaracoes.len(),
            1,
            "a seção da NH-058 tem {} linhas de status, e o gate precisa de exatamente uma. \
             Com duas, a primeira decidiria sozinha; com nenhuma, o gate deixaria de vigiar \
             a tarefa sem que ninguém percebesse.",
            declaracoes.len()
        );
        let status = declaracoes[0]
            .trim_start()
            .trim_start_matches("Status:")
            .split_whitespace()
            .next()
            .unwrap_or_default();

        // A condição é sobre o STATUS, não sobre o log. A ordem importa, e
        // errá-la foi o defeito da primeira versão deste bloco.
        if status == "DONE" {
            assert!(
                propaga,
                "a NH-058 está marcada como concluída, e a saída de um aparelho não colocou \
                 nada no log: nenhum evento assinado leva a mudança de estado para os outros \
                 peers. Num conjunto simétrico isso faz o roster divergir — o aparelho sai \
                 aqui e continua ativo lá. Ou implemente a propagação, ou volte para PARCIAL."
            );
        } else {
            assert_eq!(
                status, "PARCIAL",
                "a NH-058 está como \"{status}\", que não é nem PARCIAL nem DONE. Este gate \
                 só sabe ler esses dois estados, e um status que ele não entende é um status \
                 que ele não vigia."
            );
        }
    }

    /// **Gate estrutural: não existe outra porta para `retired`.**
    ///
    /// Este é o gate que a etapa 11 não tinha. Ela provava que
    /// `aposentar`/`abandonar` faziam a coisa certa, e deixava
    /// `sync_trust::mudar_estado(conn, android, "retired")` ao lado, escrevendo
    /// o mesmo estado sem prova nenhuma. Provar o caminho bonito não serve de
    /// nada enquanto o atalho continua exportado.
    ///
    /// A varredura é sobre o texto dos módulos de sincronização: só
    /// `sync_gc.rs` pode conter um `state = 'retired'`, porque só lá existem as
    /// pré-condições. Se alguém reintroduzir a transição direta em outro
    /// módulo — ou trouxer de volta um `mudar_estado` genérico — este teste
    /// reprova antes da revisão humana.
    #[test]
    fn so_o_modulo_de_saida_escreve_o_estado_de_saida() {
        let modulos: [(&str, &str); 5] = [
            ("sync_trust.rs", include_str!("sync_trust.rs")),
            ("sync_apply.rs", include_str!("sync_apply.rs")),
            ("sync_session.rs", include_str!("sync_session.rs")),
            ("sync_exchange.rs", include_str!("sync_exchange.rs")),
            ("sync_repository.rs", include_str!("sync_repository.rs")),
        ];

        for (nome, fonte) in modulos {
            // Só o código conta. O `sync_trust.rs` explica em comentário qual
            // era a porta dos fundos e como ela se chamava — e a explicação é
            // metade do valor da correção. Um gate que reprova por causa da
            // própria documentação obrigaria a apagar justamente o texto que
            // impede alguém de reintroduzir o erro por não saber que existiu.
            let codigo: String = fonte
                .lines()
                .filter(|linha| !linha.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");

            assert!(
                !codigo.contains("state = 'retired'") && !codigo.contains("state = 'revoked'"),
                "{nome} escreve estado de saída direto. As transições vivem em sync_gc.rs, \
                 onde existem as pré-condições — um UPDATE solto aqui aposenta um aparelho \
                 sem prova de sincronização final e a poda passa a ignorá-lo."
            );
            assert!(
                !codigo.contains("fn mudar_estado"),
                "{nome} reintroduziu um mudador de estado genérico. `estado: &str` como \
                 parâmetro é exatamente a porta dos fundos que a revisão 11.1 fechou."
            );
        }
    }
}
