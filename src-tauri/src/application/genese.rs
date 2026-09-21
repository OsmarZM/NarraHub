//! **A gênese: a adoção versionada de um acervo pré-existente** (NH-079 etapa C).
//!
//! ```text
//! estado legado
//!   → backfills obrigatórios (blob_backfill)      o que muda o estado que a gênese representa
//!   → GÊNESE                                      cada agregado coberto ganha a 1ª revisão
//!   → baseline / snapshot / bootstrap             o bundle passa a ser honesto
//! ```
//!
//! Um acervo criado antes do Sync V2 tem linhas de domínio sem nenhuma revisão que as explique. O
//! log só conhece o que foi escrito depois da fronteira `Mutacao` existir. A gênese fecha essa
//! lacuna: para cada agregado coberto que existe no domínio e não tem revisão corrente, emite a
//! **primeira** revisão dele, com `base_rev` = raiz.
//!
//! ## O que ela não faz
//!
//! Não muda domínio — escreve só evento e estado causal. Não inventa história anterior, não atribui
//! autoria a quem escreveu antes e não tem opinião sobre quando o texto foi escrito. A revisão de
//! gênese diz "este era o estado quando este aparelho adotou o acervo", e nada além disso.
//!
//! ## Determinismo
//!
//! ```text
//! mesma identidade de agregado + mesmo payload canônico + base_rev = raiz
//!   → mesma revisão de gênese
//! ```
//!
//! Isso vale porque `compute_revision` é função dos três, e de mais nada. Dois aparelhos que adotem
//! cópias do mesmo acervo produzem eventos diferentes (ids e assinaturas são de cada um) com as
//! **mesmas** revisões — e, ao parear, cada lado reconhece as revisões do outro em vez de divergir.
//! Não é uma afirmação sobre "estado igual" em geral: é sobre estes três campos.
//!
//! ## Forma do evento
//!
//! Evento normal de criação. O grupo tem um membro só, e o `kind` registra a origem sem mudar
//! nenhuma decisão de aplicação:
//!
//! ```text
//! operation = upsert · base_rev = raiz
//! mutation_id = novo · index = 0 · count = 1 · kind = "genesis" · root_type/root_id vazios
//! ```
//!
//! O receptor aplica pelo caminho de sempre. Um grupo grande seria pior de duas formas: estouraria
//! o teto de membros num acervo real e transformaria a adoção inteira numa unidade de decisão.
//!
//! ## Atomicidade
//!
//! Uma transação `IMMEDIATE`: eventos, estado causal, `sync_applied_events` e a linha da adoção
//! entram juntos. **Adoção pela metade não é um estado que o banco pode estar** — e, como a linha
//! da adoção entra na mesma transação, a queda desfaz tudo, inclusive ela. A recuperação é
//! simplesmente: versão não concluída ⇒ executar de novo.

use rusqlite::{Connection, TransactionBehavior};

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::sync::{AggregateRef, GrupoDeMutacao, Operation};
use crate::infrastructure::sqlite::sync_codec::{self, adocao};
use crate::infrastructure::sqlite::sync_repository::{append_event_in_transaction, LocalChange};
use crate::infrastructure::sqlite::SqliteDatabase;

/// A versão do formato canônico que a etapa C adota.
///
/// A adoção é sempre a adoção de uma **versão**. Se um dia o formato passar a cobrir estado que
/// hoje fica de fora, isso declara uma adoção nova, com versão nova — e não se disfarça de "órfão
/// adotado automaticamente".
///
/// **É o mesmo número do fio, por construção** (etapa E). A adoção versiona o formato canônico, e
/// o `Hello` compara esse formato entre dois aparelhos: se fossem dois números, alguém poderia subir
/// a adoção por um motivo operacional e quebrar a compatibilidade sem perceber — ou mudar o formato
/// e esquecer a adoção. Mudar isto é mudar `FORMATO_CANONICO_ATUAL`, e isso É mudança de protocolo.
pub const VERSAO_DA_ADOCAO: i64 = crate::infrastructure::sqlite::sync_codec::FORMATO_CANONICO_ATUAL;

/// O que a adoção fez.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumoDaAdocao {
    /// A versão já estava adotada quando isto rodou?
    pub ja_estava_adotado: bool,
    /// Quantos agregados ganharam a primeira revisão.
    pub adotados: usize,
    /// Quantos eventos a adoção emitiu (maior que `adotados` quando um ciclo exigiu completar).
    pub eventos: usize,
    pub primeiro_seq: i64,
    pub ultimo_seq: i64,
}

/// Por que o acervo não pode ser adotado agora.
#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDaAdocao {
    /// Há mídia legada por converter. A gênese representaria um estado que o backfill vai mudar.
    BackfillPendente { pendencias: usize },
    /// Um agregado existe no domínio e não tem estado canônico legível.
    AcervoInconsistente { agregado: String, motivo: String },
}

impl std::fmt::Display for FalhaDaAdocao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDaAdocao::BackfillPendente { pendencias } => write!(
                f,
                "Este acervo tem {pendencias} pendência(s) de conversão de mídia. A adoção \
                 precisa acontecer depois delas: ela registra o estado como ele é, e a conversão \
                 ainda vai mudar esse estado. Resolva as pendências e tente de novo."
            ),
            FalhaDaAdocao::AcervoInconsistente { agregado, motivo } => write!(
                f,
                "O acervo tem um item que o aplicativo não consegue representar: {agregado} \
                 ({motivo}). Nada foi adotado — adotar só o que deu deixaria o resto invisível \
                 para a sincronização, sem ninguém saber."
            ),
        }
    }
}

/// A adoção desta versão já concluiu?
pub fn adotado(connection: &Connection) -> DatabaseCommandResult<bool> {
    let existe: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_adoptions WHERE canonical_format_version = ?1)",
            [VERSAO_DA_ADOCAO],
            |row| row.get(0),
        )
        .map_err(erro)?;
    Ok(existe)
}

/// **Adota o acervo, ou não muda nada.**
///
/// Idempotente: com a versão já adotada, é um no-op que devolve `ja_estava_adotado`.
pub fn adotar(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<ResumoDaAdocao> {
    // **Caminho rápido, e só isso.** A leitura fora da transação evita abrir escrita à toa no
    // arranque de todo dia; ela não é a decisão. Tudo o que importa é conferido de novo dentro do
    // `BEGIN IMMEDIATE`, onde o estado não pode mudar debaixo da checagem.
    {
        let connection = database.read()?;
        if adotado(&connection)? {
            // Já adotada **não** é sinônimo de coerente: se ficou agregado sem revisão, isto é
            // inconsistência, e a resposta é erro — nunca um no-op silencioso que a esconde.
            exigir_acervo_adotado(&connection)?;
            return Ok(ResumoDaAdocao {
                ja_estava_adotado: true,
                ..ResumoDaAdocao::default()
            });
        }
        if let Some(falha) = impedimento(&connection)? {
            return Err(DatabaseCommandError::conflict(falha.to_string()));
        }
    }

    entre_o_fast_path_e_a_transacao::executar(database)?;

    let mut connection = database.write()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(erro)?;

    // Outra adoção pode ter confirmado entre a leitura e o `BEGIN IMMEDIATE`.
    if adotado(&tx)? {
        exigir_acervo_adotado(&tx)?;
        return Ok(ResumoDaAdocao {
            ja_estava_adotado: true,
            ..ResumoDaAdocao::default()
        });
    }

    // E o backfill é conferido de novo AQUI, com a escrita já tomada: entre a leitura de fora e
    // este ponto, uma passada de conversão de mídia pode ter aberto pendência. Emitir a gênese
    // depois disso registraria um estado que a conversão ainda vai mudar.
    if let Some(falha) = impedimento(&tx)? {
        return Err(DatabaseCommandError::conflict(falha.to_string()));
    }

    let Emitido {
        adotados,
        eventos,
        primeiro_seq,
        ultimo_seq,
    } = emitir(&tx, identidade)?;

    // O acervo inteiro, e não "o que deu": depois da adoção, agregado coberto sem revisão corrente
    // é falha, aqui dentro, com a transação ainda podendo ser desfeita.
    if let Some(orfao) = primeiro_orfao(&tx)? {
        return Err(DatabaseCommandError::storage(format!(
            "A adoção terminou e {} {} continua sem revisão. Nada foi confirmado.",
            orfao.aggregate_type, orfao.aggregate_id
        )));
    }

    tx.execute(
        "INSERT INTO sync_adoptions
            (canonical_format_version, device_id, completed_at, aggregates, first_seq, last_seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            VERSAO_DA_ADOCAO,
            identidade.device_id(),
            now_timestamp(),
            adotados as i64,
            primeiro_seq,
            ultimo_seq
        ],
    )
    .map_err(erro)?;

    tx.commit().map_err(erro)?;
    Ok(ResumoDaAdocao {
        ja_estava_adotado: false,
        adotados,
        eventos,
        primeiro_seq,
        ultimo_seq,
    })
}

/// Quanto a emissão fez.
struct Emitido {
    adotados: usize,
    eventos: usize,
    primeiro_seq: i64,
    ultimo_seq: i64,
}

/// **O laço da gênese**: percorre as fases na ordem topológica e emite a primeira revisão de cada
/// agregado que ainda não tem passado causal.
///
/// Separado de [`adotar`] porque o teste precisa dele sem a contabilidade da versão: uma fixture
/// que insere domínio por SQL depois da adoção precisa poder torná-lo explicável, e a regra de
/// produção — órfão depois da adoção é falha fechada — existe justamente para isso não acontecer
/// em produção.
fn emitir(
    tx: &rusqlite::Transaction<'_>,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<Emitido> {
    let mut adotados = 0usize;
    let mut eventos = 0usize;
    let mut primeiro_seq = 0i64;
    let mut ultimo_seq = 0i64;

    for fase in adocao::FASES {
        for id in (fase.enumerar)(tx)? {
            let agregado = AggregateRef::new(fase.tipo, &id);
            let ja_tem_revisao = sync_codec::revisao_corrente(tx, &agregado)?.is_some();
            match fase.modo {
                // Criar é só para quem ainda não tem passado causal: um acervo que já usava o
                // Sync V2 parcialmente não é reescrito.
                adocao::Modo::Criar if ja_tem_revisao => continue,
                // Completar só existe para o que esta adoção acabou de criar.
                adocao::Modo::Completar if !ja_tem_revisao => continue,
                _ => {}
            }
            let Some(estado) = adocao::payload_da_fase(tx, fase, &id)? else {
                continue;
            };
            if fase.modo == adocao::Modo::Completar
                && sync_codec::payload_da_revisao_corrente(tx, &agregado)?.as_deref()
                    == Some(estado.payload.as_str())
            {
                // O card nasceu completo: não havia ciclo para desfazer.
                continue;
            }

            // A mesma validação que o apply remoto cobra. Falhar aqui é falhar a adoção inteira.
            sync_codec::validar_para_emissao(tx, &agregado, &estado.payload)?;

            let envelope = append_event_in_transaction(
                tx,
                identidade,
                &LocalChange {
                    universe_id: &estado.universe_id,
                    aggregate: agregado.clone(),
                    operation: Operation::Upsert,
                    payload: &estado.payload,
                    grupo: GrupoDeMutacao {
                        mutation_id: new_id(),
                        index: 0,
                        count: 1,
                        kind: "genesis".into(),
                        root_type: String::new(),
                        root_id: String::new(),
                    },
                },
            )?;
            eventos += 1;
            if fase.modo == adocao::Modo::Criar {
                adotados += 1;
            }
            if primeiro_seq == 0 {
                primeiro_seq = envelope.seq;
            }
            ultimo_seq = envelope.seq;
            falha_no_meio::verificar(eventos)?;
        }
    }

    Ok(Emitido {
        adotados,
        eventos,
        primeiro_seq,
        ultimo_seq,
    })
}

/// **Adoção sem contabilidade de versão, só para fixtures de teste.**
///
/// Dá passado causal ao que uma fixture inseriu por SQL. Produção nunca passa por aqui: lá, órfão
/// depois da adoção é falha fechada, e é isso que [`exigir_acervo_adotado`] cobra.
#[cfg(test)]
pub(crate) fn adotar_orfaos_de_teste(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<usize> {
    let mut connection = database.write()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(erro)?;
    let emitido = emitir(&tx, identidade)?;
    tx.commit().map_err(erro)?;
    Ok(emitido.adotados)
}

/// O que impede a adoção agora, se algo impedir.
///
/// A ordem obrigatória vive aqui: **backfill antes da gênese**. Adotar antes da conversão de mídia
/// registraria um estado que o backfill ainda vai mudar — e a revisão de gênese ficaria descrevendo
/// um acervo que deixou de existir logo depois.
pub fn impedimento(connection: &Connection) -> DatabaseCommandResult<Option<FalhaDaAdocao>> {
    let pendencias =
        crate::infrastructure::sqlite::blob_backfill::pendencias_abertas(connection)?.len();
    if pendencias > 0 {
        return Ok(Some(FalhaDaAdocao::BackfillPendente { pendencias }));
    }
    Ok(None)
}

/// **Depois de adotada, órfão é falha fechada** — nunca adoção automática.
///
/// Um agregado coberto sem revisão corrente, num acervo já adotado, significa uma de duas coisas: a
/// cobertura cresceu sem declarar adoção nova, ou algo escreveu no domínio por fora da `Mutacao`.
/// As duas precisam de gente decidindo; adotar sozinho transformaria o bug numa criação assinada
/// por este aparelho.
pub fn exigir_acervo_adotado(connection: &Connection) -> DatabaseCommandResult<()> {
    if !adotado(connection)? {
        return Err(DatabaseCommandError::conflict(
            "Este acervo ainda não foi adotado pela sincronização. A adoção acontece no arranque, \
             depois da conversão de mídia; nada foi feito.",
        ));
    }
    if let Some(orfao) = primeiro_orfao(connection)? {
        return Err(DatabaseCommandError::storage(format!(
            "{} {} existe no acervo e não tem revisão, num banco já adotado (versão {}). Isto é \
             inconsistência, não item novo: uma cobertura nova exige adoção versionada própria.",
            orfao.aggregate_type, orfao.aggregate_id, VERSAO_DA_ADOCAO
        )));
    }
    Ok(())
}

/// **O que a captura de bundle cobra: nenhum agregado sem revisão.**
///
/// A linha de `sync_adoptions` é contabilidade do arranque; o que torna o bundle honesto é outra
/// coisa — que todo item do acervo tenha um evento que o explique. Um acervo legado (nunca adotado)
/// reprova aqui pelo motivo certo, e um banco novo, cujo acervo nasceu inteiro pela `Mutacao`,
/// passa sem depender de quando o arranque rodou.
pub fn exigir_acervo_sem_orfaos(connection: &Connection) -> DatabaseCommandResult<()> {
    if let Some(orfao) = primeiro_orfao(connection)? {
        return Err(DatabaseCommandError::conflict(format!(
            "{} {} não tem revisão: este acervo ainda não foi adotado pela sincronização.",
            orfao.aggregate_type, orfao.aggregate_id
        )));
    }
    Ok(())
}

/// O primeiro agregado coberto que existe no domínio e não tem revisão corrente.
pub fn primeiro_orfao(connection: &Connection) -> DatabaseCommandResult<Option<AggregateRef>> {
    for fase in adocao::FASES {
        if fase.modo != adocao::Modo::Criar {
            continue;
        }
        for id in (fase.enumerar)(connection)? {
            let agregado = AggregateRef::new(fase.tipo, &id);
            if sync_codec::revisao_corrente(connection, &agregado)?.is_none() {
                return Ok(Some(agregado));
            }
        }
    }
    Ok(None)
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// O que acontece **entre** o caminho rápido e o `BEGIN IMMEDIATE`, só em teste.
///
/// É a janela que o fast path não cobre: entre a leitura de fora e a tomada da escrita, o estado
/// pode mudar. Sem um gancho aqui, a repetição das checagens dentro da transação seria código que
/// nenhum teste distingue de código morto.
pub(crate) mod entre_o_fast_path_e_a_transacao {
    use crate::database::error::DatabaseCommandResult;
    use crate::infrastructure::sqlite::SqliteDatabase;

    #[cfg(test)]
    thread_local! {
        static ABRIR_PENDENCIA: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Faz a próxima adoção encontrar uma pendência de mídia que não existia no caminho rápido.
    #[cfg(test)]
    pub fn abrir_pendencia_de_midia(armar: bool) {
        ABRIR_PENDENCIA.with(|marca| marca.set(armar));
    }

    #[cfg(test)]
    pub fn executar(database: &SqliteDatabase) -> DatabaseCommandResult<()> {
        if !ABRIR_PENDENCIA.with(|marca| marca.get()) {
            return Ok(());
        }
        abrir_pendencia_de_midia(false);
        let connection = database.write()?;
        connection
            .execute(
                "INSERT INTO blob_migration_issues
                   (id, surface, table_name, row_id, side, reason, detail)
                 VALUES ('pendencia-tardia', 1, 'universes', 'u', 'cover_image',
                         'legacy_unrecognized', '')",
                [],
            )
            .map_err(|error| {
                crate::database::error::DatabaseCommandError::storage(error.to_string())
            })?;
        Ok(())
    }

    #[cfg(not(test))]
    #[inline(always)]
    pub fn executar(_database: &SqliteDatabase) -> DatabaseCommandResult<()> {
        Ok(())
    }
}

/// Falha injetada no meio da adoção, só em teste: prova que a transação não deixa meia gênese.
pub(crate) mod falha_no_meio {
    use crate::database::error::DatabaseCommandResult;

    #[cfg(test)]
    thread_local! {
        static DEPOIS_DO_EVENTO: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub fn armar(evento: Option<usize>) {
        DEPOIS_DO_EVENTO.with(|armada| armada.set(evento));
    }

    #[cfg(test)]
    pub fn verificar(evento: usize) -> DatabaseCommandResult<()> {
        if DEPOIS_DO_EVENTO.with(|armada| armada.get()) == Some(evento) {
            armar(None);
            return Err(crate::database::error::DatabaseCommandError::storage(
                format!("falha injetada depois do evento {evento} da adoção"),
            ));
        }
        Ok(())
    }

    #[cfg(not(test))]
    #[inline(always)]
    pub fn verificar(_evento: usize) -> DatabaseCommandResult<()> {
        Ok(())
    }
}
