//! **A época causal do protocolo 1** (etapa E, item E0-beta).
//!
//! O Sync V2 anterior ao `Hello` (0.10.0-beta.1/beta.2) deixou em cada instalação uma causalidade
//! que o protocolo 1 não fala. A auditoria (`docs/sync/AUDITORIA_E0_BETA.md`) mediu com os bancos
//! reais da beta.2: envelopes com assinatura válida e payload que o codec recusa, revisões
//! correntes que não descrevem o domínio, capítulos apagados sem tombstone. Filtrar na saída deixa
//! lacuna de `seq`; reemitir sob a mesma identidade dá duas coisas com a mesma coordenada causal.
//!
//! A saída decidida é uma **época nova, com identidade nova**:
//!
//! ```text
//! evidências de exclusão (antes de tudo)   tombstone, ou estado de agregado que sumiu do domínio
//! nova DeviceIdentity                      sync-identity.next.json, durável
//! ── uma transação ─────────────────────────────────────────────────────────────────────
//! o passado inteiro → sync_legado          inclusive o relay de outras origens
//! estado vivo esvaziado                    log, história, estado, tombstones, cursores, roster…
//! self novo                                o roster legado deixa de existir: pareia-se de novo
//! gênese canônica completa                 assinada pela identidade nova
//! delete-genesis das exclusões com prova   ausência sem prova continua não sendo exclusão
//! sync_epoca = 1, origem 'rotacao'
//! ── fim da transação ──────────────────────────────────────────────────────────────────
//! rename next → atual
//! ```
//!
//! Recuperável dos dois lados da fronteira: ver `identity_store` para a ordem e
//! [`concluir_rotacao_pendente`] para o arranque que encontra o meio do caminho.
//!
//! ## Por que as evidências vêm antes, e por que só elas
//!
//! Um reset ingênuo esquece que algo foi apagado — e um aparelho que ainda o tenha o traz de volta
//! pela própria gênese. Por isso, antes de arquivar, cada exclusão **provada** vira um delete-genesis
//! da época nova (`base_rev` = raiz, determinístico: dois aparelhos com o mesmo passado produzem a
//! mesma revisão). Quem ainda tiver o item recebe o delete como decisão, não como perda silenciosa.
//!
//! Prova é uma de duas: `sync_tombstones`, ou `sync_aggregate_state` de um agregado que não existe
//! mais no domínio (a beta apagava capítulo sem tombstone). **Ausência sem prova não é exclusão**:
//! um item que este aparelho nunca viu não é apagado em ninguém.

use std::path::Path;

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::application::genese;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::sync::{AggregateRef, GrupoDeMutacao, Operation};
use crate::infrastructure::identity_store;
use crate::infrastructure::sqlite::sync_codec::{self, adocao};
use crate::infrastructure::sqlite::sync_repository::{append_event_in_transaction, LocalChange};
use crate::infrastructure::sqlite::SqliteDatabase;

/// A época que este código fala. É o protocolo do `Hello`.
pub const EPOCA_ATUAL: i64 = crate::application::sync_sessao::PROTOCOLO_DO_SYNC as i64;

/// O passado pré-Hello: arquivado e esvaziado, **nesta ordem** (filhos de FK antes dos pais).
///
/// O gate `todo_estado_do_protocolo_tem_destino_na_rotacao` reprova tabela `sync_*` nova que não
/// esteja aqui nem em [`PRESERVADAS_NA_ROTACAO`].
pub const TABELAS_DO_PASSADO: &[&str] = &[
    "sync_applied_events",
    "sync_peer_vectors",
    "sync_divergences",
    "sync_revision_history",
    "sync_tombstones",
    "sync_aggregate_state",
    "sync_adoptions",
    "sync_events",
    "sync_cursors",
    "sync_devices",
];

/// Tabelas `sync_*` que a rotação não toca, com o motivo.
pub const PRESERVADAS_NA_ROTACAO: &[(&str, &str)] = &[
    (
        "sync_peers",
        "endereços do Sync V1, que não é causalidade do V2",
    ),
    (
        "sync_conflicts",
        "conflitos do Sync V1, decisões locais do escritor",
    ),
    ("sync_epoca", "o próprio marcador"),
    ("sync_legado", "o destino do arquivamento"),
    ("sync_rotacao_em_curso", "a trava da rotação"),
];

/// O que a rotação fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoDaRotacao {
    pub identidade_anterior: String,
    pub identidade_nova: String,
    pub linhas_arquivadas: usize,
    pub exclusoes_preservadas: usize,
    pub adocao: genese::ResumoDaAdocao,
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// A identidade que abriu a época atual, se ela já foi marcada.
pub fn marcada(connection: &Connection) -> DatabaseCommandResult<Option<String>> {
    rusqlite::OptionalExtension::optional(connection.query_row(
        "SELECT device_id FROM sync_epoca WHERE protocolo = ?1",
        [EPOCA_ATUAL],
        |row| row.get(0),
    ))
    .map_err(erro)
}

/// Existe causalidade viva que não é desta época?
///
/// O `self` sozinho não conta (todo banco o tem desde o arranque), e a linha de `sync_adoptions`
/// sozinha também não: ela registra que a adoção rodou, e num aparelho vazio rodou sobre nada.
pub fn residuo(connection: &Connection) -> DatabaseCommandResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events)
                 OR EXISTS(SELECT 1 FROM sync_revision_history)
                 OR EXISTS(SELECT 1 FROM sync_aggregate_state)
                 OR EXISTS(SELECT 1 FROM sync_tombstones)
                 OR EXISTS(SELECT 1 FROM sync_cursors)
                 OR EXISTS(SELECT 1 FROM sync_divergences)
                 OR EXISTS(SELECT 1 FROM sync_peer_vectors)
                 OR EXISTS(SELECT 1 FROM sync_devices WHERE is_self = 0)",
            [],
            |row| row.get(0),
        )
        .map_err(erro)
}

/// Este banco carrega passado pré-Hello e ainda não girou?
pub fn rotacao_pendente(connection: &Connection) -> DatabaseCommandResult<bool> {
    Ok(marcada(connection)?.is_none() && residuo(connection)?)
}

/// **O que a sessão cobra:** a época do protocolo 1 marcada. Sem ela, este banco ainda pode ter
/// envelope pré-Hello no log — e o relay o mandaria adiante.
pub fn exigir_epoca(connection: &Connection) -> DatabaseCommandResult<()> {
    if marcada(connection)?.is_some() {
        return Ok(());
    }
    Err(DatabaseCommandError::conflict(
        "Este aparelho ainda guarda a sincronização de uma versão beta antiga do NarraHub. Feche e \
         abra o aplicativo para concluir a atualização; depois, pareie os aparelhos de novo.",
    ))
}

/// **Primeiro passo de todo arranque:** conclui a troca de identidade que ficou no meio.
///
/// ```text
/// next existe, época marcada com ELA     o banco comitou e o rename não → promove
/// next existe, época marcada com outra   sobra de uma passada já resolvida → descarta
/// next existe, época não marcada         o banco não comitou → a próxima espera a rotação
/// ```
///
/// Roda antes de `reconcile_self`: com o banco já falando a identidade nova e o arquivo ainda na
/// antiga, a reconciliação rebaixaria a nova.
pub fn concluir_rotacao_pendente(
    app_data: &Path,
    database: &SqliteDatabase,
) -> DatabaseCommandResult<()> {
    let Some(proxima) = identity_store::proxima(app_data)? else {
        return Ok(());
    };
    let marcador = {
        let connection = database.read()?;
        marcada(&connection)?
    };
    match marcador {
        Some(device_id) if device_id == proxima.device_id() => {
            identity_store::promover_proxima(app_data)
        }
        Some(_) => identity_store::descartar_proxima(app_data),
        None => Ok(()),
    }
}

/// **Banco sem passado nasce na época atual.** Só marca: sem resíduo, não há o que girar.
pub fn marcar_instalacao_nova(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
) -> DatabaseCommandResult<()> {
    {
        let connection = database.read()?;
        if marcada(&connection)?.is_some() || residuo(&connection)? {
            return Ok(());
        }
    }
    let mut connection = database.write()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(erro)?;
    if marcada(&tx)?.is_none() && !residuo(&tx)? {
        tx.execute(
            "INSERT INTO sync_epoca (protocolo, device_id, origem, iniciada_em)
             VALUES (?1, ?2, 'instalacao', ?3)",
            rusqlite::params![EPOCA_ATUAL, identidade.device_id(), now_timestamp()],
        )
        .map_err(erro)?;
    }
    tx.commit().map_err(erro)
}

/// **Gira a época**, se houver passado pré-Hello. `Ok(None)` quando não havia o que girar.
///
/// Devolve a identidade nova dentro do resultado; quem chamou com a antiga deve passar a usar esta.
pub fn rotacionar(
    database: &SqliteDatabase,
    app_data: &Path,
    antiga: &DeviceIdentity,
) -> DatabaseCommandResult<Option<(DeviceIdentity, ResultadoDaRotacao)>> {
    {
        let connection = database.read()?;
        if !rotacao_pendente(&connection)? {
            return Ok(None);
        }
        // A gênese da época nova precisa da mídia convertida, pelo mesmo motivo da etapa C.
        if let Some(falha) = genese::impedimento(&connection)? {
            return Err(DatabaseCommandError::conflict(falha.to_string()));
        }
    }

    // 1. A identidade nova, no disco, antes de assinar qualquer coisa.
    let nova = identity_store::proxima_ou_preparar(app_data)?;
    if nova.device_id() == antiga.device_id() {
        return Err(DatabaseCommandError::storage(
            "A identidade da época nova é a mesma da antiga. Nada foi alterado.",
        ));
    }
    falha::verificar(falha::Ponto::DepoisDaIdentidade)?;

    // 2. Uma transação: arquivar, esvaziar, self novo, gênese, delete-genesis, marcador.
    let resultado = {
        let mut connection = database.write()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(erro)?;
        if !rotacao_pendente(&tx)? {
            return Ok(None);
        }
        let resultado = girar_na_transacao(&tx, antiga, &nova)?;
        falha::verificar(falha::Ponto::AntesDoCommit)?;
        tx.commit().map_err(erro)?;
        resultado
    };
    falha::verificar(falha::Ponto::DepoisDoCommit)?;

    // 3. O arquivo acompanha o banco.
    identity_store::promover_proxima(app_data)?;
    Ok(Some((nova, resultado)))
}

fn girar_na_transacao(
    tx: &Transaction<'_>,
    antiga: &DeviceIdentity,
    nova: &DeviceIdentity,
) -> DatabaseCommandResult<ResultadoDaRotacao> {
    // As evidências ANTES de qualquer escrita: depois de arquivar, elas não estão mais aqui.
    let exclusoes = exclusoes_com_prova(tx)?;

    let agora = now_timestamp();
    let mut arquivadas = 0usize;
    for tabela in TABELAS_DO_PASSADO {
        arquivadas += arquivar(tx, tabela, &agora)?;
    }

    tx.execute("INSERT INTO sync_rotacao_em_curso (unica) VALUES (1)", [])
        .map_err(erro)?;
    for tabela in TABELAS_DO_PASSADO {
        tx.execute(&format!("DELETE FROM {tabela}"), [])
            .map_err(erro)?;
    }
    tx.execute("DELETE FROM sync_rotacao_em_curso", [])
        .map_err(erro)?;

    tx.execute(
        "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self) VALUES (?1, '', ?2, 1)",
        rusqlite::params![nova.device_id(), nova.public_base32()],
    )
    .map_err(erro)?;

    let adocao = genese::adotar_na_transacao(tx, nova)?;

    for (agregado, universo) in &exclusoes {
        append_event_in_transaction(
            tx,
            nova,
            &LocalChange {
                universe_id: universo,
                aggregate: agregado.clone(),
                operation: Operation::Delete,
                payload: "",
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
    }

    tx.execute(
        "INSERT INTO sync_epoca
            (protocolo, device_id, origem, identidade_anterior, linhas_arquivadas,
             exclusoes_preservadas, iniciada_em)
         VALUES (?1, ?2, 'rotacao', ?3, ?4, ?5, ?6)",
        rusqlite::params![
            EPOCA_ATUAL,
            nova.device_id(),
            antiga.device_id(),
            arquivadas as i64,
            exclusoes.len() as i64,
            agora
        ],
    )
    .map_err(erro)?;

    Ok(ResultadoDaRotacao {
        identidade_anterior: antiga.device_id().to_string(),
        identidade_nova: nova.device_id().to_string(),
        linhas_arquivadas: arquivadas,
        exclusoes_preservadas: exclusoes.len(),
        adocao,
    })
}

/// **As exclusões que têm prova**, na ordem em que viram delete-genesis.
///
/// Filhos antes dos pais — posições primeiro, depois a ordem inversa das fases da gênese —, para
/// que o receptor, ao aplicar a exclusão de um pai, já tenha visto a dos filhos que ele conhecia.
pub fn exclusoes_com_prova(
    connection: &Connection,
) -> DatabaseCommandResult<Vec<(AggregateRef, String)>> {
    let mut candidatos: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for sql in [
        "SELECT aggregate_type, aggregate_id FROM sync_tombstones",
        "SELECT aggregate_type, aggregate_id FROM sync_aggregate_state",
    ] {
        let mut consulta = connection.prepare(sql).map_err(erro)?;
        let linhas = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(erro)?;
        for linha in linhas {
            candidatos.insert(linha.map_err(erro)?);
        }
    }

    let mut exclusoes = Vec::new();
    for (tipo, id) in candidatos {
        let agregado = AggregateRef::new(&tipo, &id);
        if !sync_codec::coberto(&tipo) {
            return Err(DatabaseCommandError::storage(format!(
                "O passado deste aparelho registra {tipo} {id}, e o protocolo 1 não sabe \
                 representá-lo. A atualização parou sem alterar nada."
            )));
        }
        if sync_codec::ler_canonico(connection, &agregado)?.is_some() {
            // Existe: a gênese o representa. Tombstone com o item vivo é restauração, não exclusão.
            continue;
        }
        if tipo == "universe" {
            return Err(DatabaseCommandError::storage(format!(
                "O passado deste aparelho registra a exclusão do universo {id}, e exclusão de \
                 universo não se propaga. A atualização parou sem alterar nada."
            )));
        }
        let universo: String = rusqlite::OptionalExtension::optional(connection.query_row(
            "SELECT universe_id FROM sync_events
              WHERE aggregate_type = ?1 AND aggregate_id = ?2
              ORDER BY rowid DESC LIMIT 1",
            [&tipo, &id],
            |row| row.get(0),
        ))
        .map_err(erro)?
        .unwrap_or_default();
        exclusoes.push((agregado, universo));
    }

    let posicao_da_fase = |tipo: &str| -> usize {
        adocao::FASES
            .iter()
            .position(|fase| fase.tipo == tipo)
            .unwrap_or(0)
    };
    exclusoes.sort_by(|(a, _), (b, _)| {
        let derivada = |t: &str| !sync_codec::existencia_derivada(t);
        derivada(&a.aggregate_type)
            .cmp(&derivada(&b.aggregate_type))
            .then(posicao_da_fase(&b.aggregate_type).cmp(&posicao_da_fase(&a.aggregate_type)))
            .then(a.aggregate_id.cmp(&b.aggregate_id))
    });
    Ok(exclusoes)
}

/// Copia cada linha da tabela para `sync_legado`, como JSON das colunas dela.
fn arquivar(tx: &Transaction<'_>, tabela: &str, agora: &str) -> DatabaseCommandResult<usize> {
    let colunas: Vec<String> = {
        let mut consulta = tx
            .prepare(&format!("PRAGMA table_info({tabela})"))
            .map_err(erro)?;
        let nomes = consulta
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(erro)?;
        nomes.collect::<Result<_, _>>().map_err(erro)?
    };
    let mut consulta = tx
        .prepare(&format!("SELECT * FROM {tabela}"))
        .map_err(erro)?;
    let linhas: Vec<String> = consulta
        .query_map([], |row| {
            let mut objeto = serde_json::Map::new();
            for (i, coluna) in colunas.iter().enumerate() {
                let valor = match row.get::<_, rusqlite::types::Value>(i)? {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(n) => n.into(),
                    rusqlite::types::Value::Real(r) => r.into(),
                    rusqlite::types::Value::Text(t) => t.into(),
                    rusqlite::types::Value::Blob(b) => {
                        crate::domain::data_url::codificar_base64(&b).into()
                    }
                };
                objeto.insert(coluna.clone(), valor);
            }
            Ok(serde_json::Value::Object(objeto).to_string())
        })
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;
    for linha in &linhas {
        tx.execute(
            "INSERT INTO sync_legado (protocolo, tabela, linha, arquivada_em) VALUES (0, ?1, ?2, ?3)",
            rusqlite::params![tabela, linha, agora],
        )
        .map_err(erro)?;
    }
    Ok(linhas.len())
}

/// Queda injetada nos três pontos da fronteira, só em teste.
pub(crate) mod falha {
    use crate::database::error::DatabaseCommandResult;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Ponto {
        /// A próxima identidade está no disco; o banco não mudou.
        DepoisDaIdentidade,
        /// Tudo escrito na transação, que ainda não comitou.
        AntesDoCommit,
        /// O banco comitou; o arquivo ainda é o antigo.
        DepoisDoCommit,
    }

    #[cfg(test)]
    thread_local! {
        static ARMADA: std::cell::Cell<Option<Ponto>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub fn armar(ponto: Option<Ponto>) {
        ARMADA.with(|celula| celula.set(ponto));
    }

    #[cfg(test)]
    pub fn verificar(ponto: Ponto) -> DatabaseCommandResult<()> {
        if ARMADA.with(|celula| celula.get()) == Some(ponto) {
            armar(None);
            return Err(crate::database::error::DatabaseCommandError::storage(
                format!("queda injetada: {ponto:?}"),
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
