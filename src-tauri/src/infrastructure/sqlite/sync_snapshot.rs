//! Sync V2 — bootstrap de um dispositivo vazio (ADR 0009 §14, etapa 12).
//!
//! ## Por que existe
//!
//! Um Android recém-instalado não tem nada. Reconstruir o acervo por dez mil
//! eventos seria lento e frágil. O primeiro pareamento com um aparelho vazio
//! pode usar snapshot como semente — **e nunca mais depois disso**.
//!
//! ```text
//! pareamento com acervo vazio
//!         ↓
//! snapshot  +  o vetor de sequências do MESMO instante
//!         ↓
//! a partir daqui, só eventos incrementais
//! ```
//!
//! ## Snapshot não é sincronização
//!
//! Se ele voltar ao fluxo normal, todos os problemas do V1 voltam junto: sem
//! exclusão propagada, sem detecção de concorrência, e com custo proporcional
//! ao acervo em vez de proporcional à mudança. Por isso as duas pontas são
//! fechadas: a captura recusa acervo com decisão pendente, e a semeadura recusa
//! qualquer receptor que não esteja provadamente virgem.
//!
//! ## O que viaja, e por quê
//!
//! A pergunta que decide o bundle inteiro é uma só: **de onde
//! [`super::sync_repository::aggregate_history`] lê?** É ela que alimenta
//! [`crate::domain::sync::classify`], e o que faltar ali muda a classificação
//! do primeiro evento incremental. `Unknown` não avança o cursor — o snapshot
//! entregaria o conteúdo e mataria a replicação no mesmo ato.
//!
//! Ela lê de três tabelas, e as três viajam:
//!
//! ```text
//! sync_aggregate_state   → current_rev   sem ela, todo incremental cai em Unknown
//! sync_revision_history  → known_revs    sem ela, some AlreadyPresent e Concurrent
//! sync_tombstones        → deleted_rev   sem ela, exclusão apagada ressuscita
//! ```
//!
//! O que torna isso barato: `sync_revision_history.event_id` é
//! `TEXT NOT NULL DEFAULT ''`, **sem FK** para `sync_events`. A cadeia de
//! revisões viaja sem arrastar o log.
//!
//! E o log fica de fora justamente porque o baseline existe para substituí-lo:
//! o gatilho de contiguidade da v16 só cobra densidade **acima** do baseline.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::collections::{BTreeMap, BTreeSet};

/// O destino de cada tabela do banco no bootstrap.
///
/// A matriz abaixo é conferida contra `sqlite_master` por um gate: tabela nova
/// sem classificação reprova. Sem isso, a decisão de transferir ou não vira
/// escolha de quem escreveu o `INSERT`, tomada uma tabela por vez, sem ninguém
/// olhando o conjunto — que é como um acervo perde uma tabela inteira no
/// bootstrap sem nenhum teste ficar vermelho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Categoria {
    /// Viaja no bundle.
    TransferidaNoBundle,
    /// Do protocolo V2, e deliberadamente **não** viaja: é o passado que o
    /// baseline substitui, ou evidência que só vale de primeira mão.
    ProtocoloNaoTransferido,
    /// Não viaja, e a presença de qualquer linha **impede** o bootstrap: são
    /// decisões pendentes ou trabalho local que o seed destruiria.
    BloqueiaBootstrap,
    /// Local, não viaja, e não impede — tabelas do V1 que ninguém mais escreve.
    LocalNaoTransferida,
    /// Fora do escopo desta etapa.
    EtapaPosterior,
}

use Categoria::*;

/// Toda tabela do banco, com destino declarado.
pub const CATALOGO: &[(&str, Categoria)] = &[
    // ── domínio: o acervo do escritor ──────────────────────────────────────
    ("universes", TransferidaNoBundle),
    ("stories", TransferidaNoBundle),
    ("books", TransferidaNoBundle),
    ("chapters", TransferidaNoBundle),
    ("entities", TransferidaNoBundle),
    ("entity_attributes", TransferidaNoBundle),
    ("entity_templates", TransferidaNoBundle),
    ("relations", TransferidaNoBundle),
    ("mentions", TransferidaNoBundle),
    ("content_tags", TransferidaNoBundle),
    ("content_tag_assignments", TransferidaNoBundle),
    ("content_custom_fields", TransferidaNoBundle),
    ("planning_items", TransferidaNoBundle),
    ("planning_field_definitions", TransferidaNoBundle),
    ("planning_field_links", TransferidaNoBundle),
    ("canvas_nodes", TransferidaNoBundle),
    ("canvas_edges", TransferidaNoBundle),
    ("canvas_entity_positions", TransferidaNoBundle),
    ("timeline_events", TransferidaNoBundle),
    // ── sync: roster, estado causal e o vetor ──────────────────────────────
    ("sync_devices", TransferidaNoBundle),
    ("sync_aggregate_state", TransferidaNoBundle),
    ("sync_revision_history", TransferidaNoBundle),
    ("sync_tombstones", TransferidaNoBundle),
    ("sync_cursors", TransferidaNoBundle),
    // ── o passado que o baseline substitui ─────────────────────────────────
    ("sync_events", ProtocoloNaoTransferido),
    ("sync_applied_events", ProtocoloNaoTransferido),
    // Evidência de primeira mão: a etapa 11 exigiu `SessaoAutenticada` para
    // escrever aqui, justamente para um peer não anunciar o vetor de outro.
    // Copiá-la no bundle faria o doador declarar, por terceiros, confirmações
    // que o receptor nunca ouviu — e essas confirmações autorizam poda.
    ("sync_peer_vectors", ProtocoloNaoTransferido),
    // ── decisão pendente ou trabalho local: bloqueiam ──────────────────────
    ("sync_divergences", BloqueiaBootstrap),
    ("sync_conflicts", BloqueiaBootstrap),
    ("change_log", BloqueiaBootstrap),
    ("chapter_revisions", BloqueiaBootstrap),
    ("collaboration_sessions", BloqueiaBootstrap),
    ("collaboration_contributions", BloqueiaBootstrap),
    // Asset que não pôde ser convertido para o contrato do ADR 0010.
    //
    // Bloqueia dos dois lados, por motivos diferentes:
    //
    //   doador    tem mídia que não viaja no contrato novo, e o aparelho novo
    //             nasceria sem ela — sem ninguém ter decidido isso
    //   receptor  um aparelho virgem não tem histórico de migração nenhum;
    //             linha aqui significa que ele não está virgem
    //
    // A tabela não viaja: a pendência é sobre os bytes DESTE aparelho, e o
    // aparelho novo vai descobrir as próprias ao rodar o backfill.
    ("blob_migration_issues", BloqueiaBootstrap),
    // ── V1, sem escritor vivo ──────────────────────────────────────────────
    ("devices", LocalNaoTransferida),
    ("sync_peers", LocalNaoTransferida),
    // ── etapa 13 ───────────────────────────────────────────────────────────
    ("attachments", EtapaPosterior),
];

/// A ordem em que as tabelas do bundle são inseridas.
///
/// **Não é a ordem em que foram escritas no catálogo**, e a diferença já custou
/// um erro: a primeira versão desta lista punha `planning_field_definitions`
/// antes de `planning_items`, porque a FK entre as duas nasceu num
/// `ALTER TABLE` da migration 15 e não aparece no `CREATE TABLE`:
///
/// ```text
/// ALTER TABLE planning_field_definitions ADD COLUMN owner_item_id TEXT
///     REFERENCES planning_items(id) ON DELETE CASCADE;
/// ```
///
/// Um campo com `scope = 'card'` aponta para o item que o criou. Semear as
/// definições antes dos itens quebra a FK — e só para os acervos que usam
/// campos de card, que é o tipo de defeito que passa por todo teste feito com
/// dado simples.
///
/// Por isso esta lista é conferida contra `PRAGMA foreign_key_list` por um
/// gate, em vez de mantida à mão: uma FK nova reprova sozinha.
pub const ORDEM_DE_SEMEADURA: &[&str] = &[
    "universes",
    "stories",
    "books",
    "chapters",
    "entities",
    "entity_attributes",
    "entity_templates",
    "relations",
    "mentions",
    "content_tags",
    "content_tag_assignments",
    "content_custom_fields",
    "planning_items",
    "planning_field_definitions",
    "planning_field_links",
    "canvas_nodes",
    "canvas_edges",
    "canvas_entity_positions",
    "timeline_events",
    "sync_devices",
    "sync_aggregate_state",
    "sync_revision_history",
    "sync_tombstones",
    "sync_cursors",
];

/// As tabelas de uma categoria.
pub fn tabelas_de(categoria: Categoria) -> Vec<&'static str> {
    CATALOGO
        .iter()
        .filter(|(_, c)| *c == categoria)
        .map(|(nome, _)| *nome)
        .collect()
}

/// As tabelas que existem de fato no banco aberto.
pub fn tabelas_do_banco(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
              ORDER BY name",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let linhas = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    linhas
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

// ═══════════════════════════════════════════════════════════════════════════
// O bundle
// ═══════════════════════════════════════════════════════════════════════════

/// As linhas de uma tabela, do jeito que saíram do banco.
///
/// Genérico de propósito. Um struct por tabela seriam vinte e quatro structs
/// que precisariam ser lembrados a cada migration — e "lembrar" é exatamente o
/// que o gate do catálogo existe para não depender.
#[derive(Debug, Clone, PartialEq)]
pub struct Tabela {
    pub nome: String,
    pub colunas: Vec<String>,
    pub linhas: Vec<Vec<Value>>,
}

/// Um membro do roster, sem `is_self`.
///
/// A ausência daquela coluna aqui é o contrato: **quem é o `self` é decisão do
/// receptor**, e um bundle não tem como opinar. Se `is_self` viajasse, um
/// roster restaurado cru daria dois donos à mesma chave, ou faria o doador
/// assinar no lugar do receptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembroDoRoster {
    pub device_id: String,
    pub name: String,
    pub ed25519_public: String,
    pub x25519_public: String,
    pub state: String,
    pub introduced_by: String,
    pub exit_reason: String,
}

/// Semente de um aparelho vazio: conteúdo, confiança e o ponto de partida.
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapBundle {
    /// Domínio e estado causal, na ordem de semeadura.
    pub tabelas: Vec<Tabela>,
    pub roster: Vec<MembroDoRoster>,
    /// De onde o incremental começa, por origem. Vira `baseline_seq` e
    /// `last_seq_applied` ao mesmo tempo no receptor.
    pub vetor: BTreeMap<String, i64>,
}

/// As tabelas copiadas linha a linha.
///
/// É a ordem de semeadura menos as duas que não se copiam:
///
/// - `sync_devices` é **mesclado**, porque `is_self` é do receptor;
/// - `sync_cursors` é **derivado do vetor**, porque o cursor do doador é o
///   progresso dele. O que o receptor precisa é do ponto de partida, e é isso
///   que o vetor significa.
fn tabelas_copiadas() -> Vec<&'static str> {
    ORDEM_DE_SEMEADURA
        .iter()
        .copied()
        .filter(|t| *t != "sync_devices" && *t != "sync_cursors")
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeCaptura {
    /// O acervo tem decisão pendente do escritor, no V2.
    DivergenciaAberta { quantas: i64 },
    /// O acervo tem decisão pendente do escritor, no **V1**.
    ///
    /// Variante separada de propósito. A mensagem da divergência fala do
    /// payload que vive em `sync_events`, e isso descreve o V2: lá o conflito
    /// guarda duas revisões do agregado inteiro. O V1 é outra coisa — registra
    /// conflito **por campo**, com `local_value` e `remote_value` na própria
    /// linha. Dizer a mesma frase para os dois mandaria o escritor procurar a
    /// versão perdida no lugar errado.
    ConflitoV1Aberto { quantas: i64 },
    /// O acervo tem mídia que não pôde ser convertida (ADR 0010).
    ///
    /// Variante separada pelo mesmo motivo das duas de cima: aqui não há
    /// decisão pendente do escritor sobre qual versão vale. Há um arquivo que
    /// o aparelho não conseguiu ler — URL externa, caminho local, base64
    /// quebrada — e que continua guardado exatamente como estava. Mandar o
    /// escritor "resolver o conflito" seria mandá-lo procurar uma decisão que
    /// ninguém precisa tomar.
    AssetNaoConvertido { quantas: i64 },
}

impl std::fmt::Display for FalhaDeCaptura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDeCaptura::AssetNaoConvertido { quantas } => write!(
                f,
                "Este acervo tem {quantas} imagem(ns) que o aplicativo não conseguiu converter \
                 para o formato novo. Elas continuam preservadas neste aparelho, mas não \
                 viajam no pareamento: o aparelho novo nasceria sem elas. Veja a lista de \
                 pendências de mídia, resolva o que der, e pareie de novo."
            ),
            FalhaDeCaptura::ConflitoV1Aberto { quantas } => write!(
                f,
                "Este acervo tem {quantas} conflito(s) do sistema antigo esperando decisão. O \
                 bootstrap não pode acontecer agora: a versão que veio do outro aparelho está \
                 guardada só naquela linha de conflito, e não no conteúdo. Ela não viaja no \
                 pareamento, então o aparelho novo nasceria sem ela — sem ninguém ter \
                 escolhido. Resolva os conflitos e pareie de novo."
            ),
            FalhaDeCaptura::DivergenciaAberta { quantas } => write!(
                f,
                "Este acervo tem {quantas} divergência(s) esperando decisão. O bootstrap não \
                 pode acontecer agora: a versão perdedora de cada uma vive no payload de um \
                 evento, e o bundle não carrega o log. O aparelho novo nasceria sem ela, sem \
                 ninguém ter escolhido. Resolva as divergências e pareie de novo."
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeSemeadura {
    /// O receptor não está virgem. Diz **qual** tabela tem linha.
    ReceptorNaoEstaVazio { tabela: String, linhas: i64 },
    /// O receptor não sabe quem ele é.
    SemIdentidadeLocal,
    /// O bundle traz o `device_id` do receptor com outra chave.
    IdentidadeLocalDivergente { esperado: String, no_bundle: String },
    /// O conjunto já conhece um passado causal desta identidade, e o bundle
    /// não traz o log que o sustenta.
    IdentidadeComPassadoNoBundle { device_id: String, seq: i64 },
    /// O bundle é internamente incoerente.
    BundleIncoerente { motivo: String },
}

impl std::fmt::Display for FalhaDeSemeadura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDeSemeadura::ReceptorNaoEstaVazio { tabela, linhas } => write!(
                f,
                "Este aparelho já tem trabalho: {linhas} linha(s) em {tabela}. O bootstrap é \
                 semente de aparelho vazio, e semear por cima apagaria o que está aqui. \
                 Sincronize normalmente em vez de parear como novo."
            ),
            FalhaDeSemeadura::SemIdentidadeLocal => f.write_str(
                "Este aparelho ainda não tem identidade própria no roster. Sem saber quem é o \
                 `self`, o roster do bundle entraria sem dono — e `self` é quem assina.",
            ),
            FalhaDeSemeadura::IdentidadeLocalDivergente {
                esperado,
                no_bundle,
            } => write!(
                f,
                "O bundle traz este aparelho com outra chave pública. Esperado {esperado}, \
                 recebido {no_bundle}. Ou o bundle veio de um conjunto que conheceu outra \
                 instalação com este mesmo identificador, ou foi adulterado."
            ),
            FalhaDeSemeadura::IdentidadeComPassadoNoBundle { device_id, seq } => write!(
                f,
                "O conjunto já conhece {seq} alteração(ões) feitas por esta identidade \
                 ({device_id}), e o bootstrap não traz o log que as contém. Se este aparelho \
                 continuasse com ela, a próxima escrita nasceria como a alteração número 1 e \
                 colidiria com uma que já existe por aí — duas coisas diferentes com a mesma \
                 coordenada. Gere uma identidade nova para este aparelho antes de parear."
            ),
            FalhaDeSemeadura::BundleIncoerente { motivo } => {
                write!(f, "O bundle não descreve um estado válido: {motivo}")
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Elegibilidade
// ═══════════════════════════════════════════════════════════════════════════

/// As tabelas que precisam estar vazias para o bootstrap acontecer.
///
/// # Cursor não prova vazio
///
/// A primeira versão desta checagem olhava só `sync_cursors`, com o argumento
/// de que uma escrita local cria o cursor da própria origem. O argumento cobre
/// `update_chapter` e não cobre `create_chapter`, que grava conteúdo **sem
/// evento e sem cursor** enquanto a NH-053 estiver aberta:
///
/// ```text
/// aparelho novo, escritor cria um capítulo antes de parear
///     chapters        1 linha
///     sync_events     vazio
///     sync_cursors    vazio      ← "está virgem", diria a checagem antiga
/// ```
///
/// O snapshot passaria por cima. Por isso a pergunta virou "há trabalho aqui?",
/// e a resposta vem de todas as tabelas que podem contê-lo — inclusive as que
/// sobrevivem a um domínio vazio, como `change_log` e as sessões de
/// colaboração.
fn tabelas_que_bloqueiam() -> Vec<&'static str> {
    let mut tabelas: Vec<&'static str> = tabelas_de(TransferidaNoBundle)
        .into_iter()
        // O roster é a exceção justificada: o receptor precisa ter a própria
        // identidade ali antes de semear. A coerência dele é checada à parte.
        .filter(|t| *t != "sync_devices")
        .collect();
    tabelas.extend(tabelas_de(ProtocoloNaoTransferido));
    tabelas.extend(tabelas_de(BloqueiaBootstrap));
    tabelas.sort_unstable();
    tabelas
}

/// O receptor está provadamente vazio?
///
/// Roda **dentro** da transação de semeadura. Fora dela, uma escrita local
/// entre a checagem e o `INSERT` passaria despercebida — e seria justamente a
/// escrita que o seed apagaria.
fn bootstrap_eligible(tx: &Transaction<'_>) -> DatabaseCommandResult<Result<(), FalhaDeSemeadura>> {
    for tabela in tabelas_que_bloqueiam() {
        let linhas: i64 = tx
            .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        if linhas > 0 {
            return Ok(Err(FalhaDeSemeadura::ReceptorNaoEstaVazio {
                tabela: tabela.to_string(),
                linhas,
            }));
        }
    }

    // O roster pode ter exatamente o `self`, e mais ninguém.
    let estranhos: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sync_devices WHERE is_self = 0",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if estranhos > 0 {
        return Ok(Err(FalhaDeSemeadura::ReceptorNaoEstaVazio {
            tabela: "sync_devices".to_string(),
            linhas: estranhos,
        }));
    }

    Ok(Ok(()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Validação semântica do bundle
// ═══════════════════════════════════════════════════════════════════════════

/// **O bundle tem exatamente as tabelas e colunas que o receptor espera?**
///
/// Esta checagem existe porque a primeira versão da semeadura fazia isto:
///
/// ```text
/// let Some(tabela) = bundle.tabelas.iter().find(|t| t.nome == nome) else {
///     continue;                                    // ← tabela some em silêncio
/// };
/// ```
///
/// Um bundle sem `chapters` era aceito sem uma palavra: o receptor nascia com
/// universo, livros, roster, estado causal e cursores corretos — e **sem os
/// capítulos**. Pior que um erro, porque tudo o mais parece certo, e o cursor
/// semeado faz o aparelho nunca pedir o que não veio.
///
/// A validação causal também tratava tabela ausente como tabela vazia, e a
/// conferência final só comparava as três tabelas de estado causal. Nenhuma das
/// três camadas olhava as dezenove de domínio.
///
/// As colunas vêm de `PRAGMA table_info` **do receptor**, dentro da transação —
/// não de uma segunda lista mantida à mão, que é como a ordem de semeadura já
/// errou uma vez.
fn validar_estrutura(
    tx: &Transaction<'_>,
    bundle: &BootstrapBundle,
) -> DatabaseCommandResult<Result<(), FalhaDeSemeadura>> {
    let incoerente = |motivo: String| Ok(Err(FalhaDeSemeadura::BundleIncoerente { motivo }));

    let esperadas: BTreeSet<&str> = tabelas_copiadas().into_iter().collect();
    let mut vistas: BTreeSet<&str> = BTreeSet::new();
    for tabela in &bundle.tabelas {
        if !vistas.insert(tabela.nome.as_str()) {
            return incoerente(format!(
                "o bundle traz a tabela {} duas vezes. Qual das duas vale é uma pergunta que \
                 ninguém deveria precisar responder no meio de um seed",
                tabela.nome
            ));
        }
    }

    let faltando: Vec<&&str> = esperadas.difference(&vistas).collect();
    if !faltando.is_empty() {
        return incoerente(format!(
            "faltam tabelas no bundle: {faltando:?}. O receptor nasceria sem essa parte do \
             acervo, com o cursor já semeado — e cursor semeado faz o aparelho nunca pedir \
             o que não veio"
        ));
    }
    let sobrando: Vec<&&str> = vistas.difference(&esperadas).collect();
    if !sobrando.is_empty() {
        return incoerente(format!(
            "o bundle traz tabelas que não pertencem a ele: {sobrando:?}. Ou o catálogo mudou \
             de um lado só, ou alguém está mandando estado local disfarçado de acervo"
        ));
    }

    for tabela in &bundle.tabelas {
        let mut statement = tx
            .prepare(&format!("PRAGMA table_info({})", tabela.nome))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        let colunas_do_receptor: Vec<String> = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

        if colunas_do_receptor != tabela.colunas {
            return incoerente(format!(
                "as colunas de {} não batem. O receptor tem {:?} e o bundle traz {:?}. Uma \
                 coluna a menos entraria pelo DEFAULT do schema, em silêncio, e o dado do \
                 escritor viraria o valor padrão",
                tabela.nome, colunas_do_receptor, tabela.colunas
            ));
        }

        for (i, linha) in tabela.linhas.iter().enumerate() {
            if linha.len() != tabela.colunas.len() {
                return incoerente(format!(
                    "a linha {i} de {} tem {} valor(es) para {} coluna(s)",
                    tabela.nome,
                    linha.len(),
                    tabela.colunas.len()
                ));
            }
        }
    }

    Ok(Ok(()))
}

/// O que as FKs não conseguem cobrar.
///
/// Uma FK garante que a linha apontada existe. Não garante que a **cadeia
/// causal** faz sentido: `sync_aggregate_state.current_rev` é texto livre, e um
/// bundle onde a revisão corrente não está na história produz um receptor em
/// que `classify` nunca reconhece a base de nada. O aparelho nasceria com
/// conteúdo e sem futuro.
///
/// Roda antes da primeira mutação, dentro da transação.
fn validar_bundle(bundle: &BootstrapBundle) -> Result<(), FalhaDeSemeadura> {
    let incoerente = |motivo: String| FalhaDeSemeadura::BundleIncoerente { motivo };

    // Acesso por NOME de coluna, e não por posição.
    //
    // A primeira versão lia `linha[2]` — acoplada à ordem física que o
    // `SELECT *` devolveu no doador. Um `ALTER TABLE ADD COLUMN` numa migration
    // futura muda essa ordem, e a validação passaria a comparar campo errado
    // **sem erro de compilação e sem erro de SQL**: leria um id onde espera uma
    // revisão, concluiria que a revisão não está na história, e reprovaria
    // bundles bons. Ou, pior, o contrário.
    let campo =
        |tabela: &Tabela, linha: &[Value], coluna: &str| -> Result<String, FalhaDeSemeadura> {
            let Some(i) = tabela.colunas.iter().position(|nome| nome == coluna) else {
                return Err(incoerente(format!(
                    "a tabela {} do bundle não tem a coluna {coluna}",
                    tabela.nome
                )));
            };
            Ok(match linha.get(i) {
                Some(Value::Text(texto)) => texto.clone(),
                _ => String::new(),
            })
        };
    let tabela_de =
        |nome: &str| -> Option<&Tabela> { bundle.tabelas.iter().find(|t| t.nome == nome) };

    // (tipo, id, rev) conhecidos.
    let mut revisoes: BTreeSet<(String, String, String)> = BTreeSet::new();
    if let Some(historia) = tabela_de("sync_revision_history") {
        for linha in &historia.linhas {
            revisoes.insert((
                campo(historia, linha, "aggregate_type")?,
                campo(historia, linha, "aggregate_id")?,
                campo(historia, linha, "rev")?,
            ));
        }
    }

    if let Some(estado) = tabela_de("sync_aggregate_state") {
        for linha in &estado.linhas {
            let tipo = campo(estado, linha, "aggregate_type")?;
            let id = campo(estado, linha, "aggregate_id")?;
            let atual = campo(estado, linha, "current_rev")?;
            if !revisoes.contains(&(tipo.clone(), id.clone(), atual.clone())) {
                return Err(incoerente(format!(
                    "a revisão corrente de {tipo}/{id} é {atual}, que não está na história de \
                     revisões. O receptor não reconheceria a base de nenhum evento seguinte, e \
                     todo incremental daquele agregado cairia em Unknown — conteúdo entregue, \
                     replicação morta"
                )));
            }
        }
    }

    let no_roster: BTreeSet<&str> = bundle.roster.iter().map(|m| m.device_id.as_str()).collect();

    if let Some(tumulos) = tabela_de("sync_tombstones") {
        for linha in &tumulos.linhas {
            let tipo = campo(tumulos, linha, "aggregate_type")?;
            let id = campo(tumulos, linha, "aggregate_id")?;
            let rev = campo(tumulos, linha, "deleted_rev")?;
            if !revisoes.contains(&(tipo.clone(), id.clone(), rev.clone())) {
                return Err(incoerente(format!(
                    "a exclusão de {tipo}/{id} aponta para a revisão {rev}, que não está na \
                     história. Sem ela o receptor não distingue \"foi apagado\" de \"nunca \
                     existiu\", e uma edição concorrente ressuscita o item"
                )));
            }
            let origem = campo(tumulos, linha, "origin_device_id")?;
            if !origem.is_empty() && !no_roster.contains(origem.as_str()) {
                return Err(incoerente(format!(
                    "a exclusão de {tipo}/{id} veio da origem {origem}, que não está no roster \
                     do bundle. A poda pergunta se cada membro válido atravessou aquela \
                     exclusão, e essa pergunta não teria a quem ser feita"
                )));
            }
        }
    }

    for (origem, seq) in &bundle.vetor {
        if !no_roster.contains(origem.as_str()) {
            return Err(incoerente(format!(
                "o vetor traz a origem {origem}, que não está no roster. O cursor não teria \
                 para onde apontar — é o que a FK de sync_cursors cobra, e chegar nela \
                 depois de meia semeadura seria descobrir tarde"
            )));
        }
        if *seq < 0 {
            return Err(incoerente(format!(
                "o vetor traz {origem} com sequência negativa ({seq})"
            )));
        }
    }

    let mut vistos: BTreeMap<&str, &str> = BTreeMap::new();
    for membro in &bundle.roster {
        if let Some(anterior) = vistos.insert(&membro.device_id, &membro.ed25519_public) {
            if anterior != membro.ed25519_public {
                return Err(incoerente(format!(
                    "o roster traz {} com duas chaves públicas diferentes",
                    membro.device_id
                )));
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Captura
// ═══════════════════════════════════════════════════════════════════════════

/// Tira a semente, **tudo do mesmo instante**.
///
/// # Uma transação, não várias leituras
///
/// A seção 14 do ADR descreve o modo de errar:
///
/// ```text
/// captura o conteúdo        (o capítulo novo ainda não existe)
///         ↓
///    ESCRITA CONCORRENTE    (o escritor salva; seq vai a 1803)
///         ↓
/// lê o vetor  →  1803
///
/// o aparelho novo recebe conteúdo até 1802 e cursor dizendo 1803
///                          → nunca pede o 1803. O capítulo some.
/// ```
///
/// E o problema não se limita ao par conteúdo/vetor: o estado causal tem a
/// mesma exposição. Conteúdo do instante A1 com `sync_aggregate_state` de A2
/// produz um receptor cuja revisão corrente descreve um texto que ele não tem.
///
/// Por isso **tudo** sai da mesma transação de leitura — domínio, roster,
/// estado causal, tombstones e vetor. Em WAL o escritor continua trabalhando
/// durante a captura; o que ele escreve simplesmente fica fora da visão desta
/// transação, por inteiro. É a garantia que o gate cobra: uma escrita
/// concorrente aparece no snapshot **ou** no incremental, nunca entre os dois.
///
/// Não usa `IMMEDIATE`: captura não escreve, e travar o banco do escritor para
/// tirar uma cópia seria transformar o bootstrap em pausa do aplicativo.
pub fn capturar(
    connection: &mut Connection,
) -> DatabaseCommandResult<Result<BootstrapBundle, FalhaDeCaptura>> {
    let tx = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // Decisão pendente do escritor não viaja, e por isso bloqueia. A versão
    // perdedora de uma divergência vive no payload de um evento, e o bundle
    // não carrega o log: semear agora apagaria semanticamente uma escolha que
    // ninguém fez ainda.
    // Aberta é `resolved_at = ''`, que é como o schema define — tem até índice
    // parcial com esse predicado (`idx_sync_divergences_abertas`). A primeira
    // versão contava a tabela inteira, e o efeito era pior do que parece:
    // bastava o escritor ter resolvido uma divergência **uma vez na vida** para
    // o bootstrap ficar impossível para sempre naquele acervo. Divergência
    // resolvida é decisão tomada, e decisão tomada já está no conteúdo.
    let divergencias: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sync_divergences WHERE resolved_at = ''",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if divergencias > 0 {
        return Ok(Err(FalhaDeCaptura::DivergenciaAberta {
            quantas: divergencias,
        }));
    }

    // O V1 ainda está em produção, e `sync.rs` ainda escreve `sync_conflicts`.
    //
    // Aqui havia uma assimetria: `sync_conflicts` é `BloqueiaBootstrap` no
    // catálogo, o que impede o **receptor** de ser semeado com conflito V1
    // pendente — e nada impedia o **doador** de capturar com um. O efeito é o
    // mesmo da divergência V2, por um caminho que ninguém estava olhando:
    //
    // ```text
    // sync_conflicts   campo, local_value, remote_value
    //                                      └─ a versão do outro aparelho,
    //                                         que só existe nesta linha
    // ```
    //
    // O conteúdo materializado tem o lado local. O `remote_value` não está no
    // acervo e não viaja no bundle: capturar agora faria a versão pendente
    // desaparecer do mundo do aparelho novo, sem decisão de ninguém.
    let conflitos_v1: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE resolved_at = ''",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if conflitos_v1 > 0 {
        return Ok(Err(FalhaDeCaptura::ConflitoV1Aberto {
            quantas: conflitos_v1,
        }));
    }

    // Só as ABERTAS, como nas duas checagens acima. Pendência resolvida é
    // história da migração deste aparelho, e história não impede pareamento —
    // foi exatamente esse o defeito que a revisão da etapa 12 encontrou na
    // contagem de divergências.
    let assets_pendentes: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM blob_migration_issues WHERE resolved_at = ''",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if assets_pendentes > 0 {
        return Ok(Err(FalhaDeCaptura::AssetNaoConvertido {
            quantas: assets_pendentes,
        }));
    }

    let mut tabelas = Vec::new();
    for nome in tabelas_copiadas() {
        tabelas.push(ler_tabela(&tx, nome)?);
    }

    let mut roster = Vec::new();
    {
        let mut statement = tx
            .prepare(
                "SELECT device_id, name, ed25519_public, x25519_public, state,
                        introduced_by, exit_reason
                   FROM sync_devices ORDER BY device_id",
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        let linhas = statement
            .query_map([], |row| {
                Ok(MembroDoRoster {
                    device_id: row.get(0)?,
                    name: row.get(1)?,
                    ed25519_public: row.get(2)?,
                    x25519_public: row.get(3)?,
                    state: row.get(4)?,
                    introduced_by: row.get(5)?,
                    exit_reason: row.get(6)?,
                })
            })
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        for linha in linhas {
            roster.push(linha.map_err(|error| DatabaseCommandError::storage(error.to_string()))?);
        }
    }

    // A MESMA função da sessão normal, dentro da MESMA transação. Vetor de
    // bootstrap e vetor de sessão precisam significar a mesma coisa — duas
    // definições divergiriam no primeiro caso de borda, e o caso de borda aqui
    // é perda de dados.
    let vetor = crate::infrastructure::sqlite::sync_exchange::vetor_local(&tx)?;

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    Ok(Ok(BootstrapBundle {
        tabelas,
        roster,
        vetor,
    }))
}

fn ler_tabela(tx: &Transaction<'_>, nome: &str) -> DatabaseCommandResult<Tabela> {
    let mut statement = tx
        .prepare(&format!("SELECT * FROM {nome}"))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let colunas: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let quantas = colunas.len();
    let linhas = statement
        .query_map([], |row| {
            let mut valores = Vec::with_capacity(quantas);
            for i in 0..quantas {
                valores.push(row.get::<_, Value>(i)?);
            }
            Ok(valores)
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    Ok(Tabela {
        nome: nome.to_string(),
        colunas,
        linhas,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Semeadura
// ═══════════════════════════════════════════════════════════════════════════

/// Semeia um aparelho vazio. **Construtivo, nunca destrutivo.**
///
/// Não existe `DELETE` de tabela do bundle aqui. Se houvesse, a função
/// carregaria a capacidade de apagar o acervo — e bastaria uma pré-condição
/// mal escrita, num futuro qualquer, para essa capacidade encontrar um acervo
/// cheio. O seed só sabe inserir; quando encontra estado inesperado, falha
/// fechado e a transação inteira volta atrás.
///
/// ```text
/// BEGIN IMMEDIATE
///   quem é o self, e a chave bate?
///   o receptor está vazio?              ← dentro da transação, não antes
///   o bundle é coerente?                ← antes da primeira mutação
///   domínio → roster → causal → baselines
///   apaga o change_log que os gatilhos criaram AGORA
///   confere os invariantes
/// COMMIT
/// ```
///
/// A elegibilidade roda **dentro** do `IMMEDIATE` porque fora dele existe uma
/// janela: entre a checagem e o primeiro `INSERT`, o escritor pode salvar um
/// capítulo — e seria exatamente esse capítulo que o seed enterraria.
pub fn semear(
    connection: &mut Connection,
    identidade_local: &crate::domain::identity::DeviceIdentity,
    bundle: &BootstrapBundle,
) -> DatabaseCommandResult<Result<(), FalhaDeSemeadura>> {
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // ── 1. quem é o receptor ────────────────────────────────────────────────
    let self_local: Option<(String, String)> = tx
        .query_row(
            "SELECT device_id, ed25519_public FROM sync_devices WHERE is_self = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let Some((self_id, self_chave)) = self_local else {
        return Ok(Err(FalhaDeSemeadura::SemIdentidadeLocal));
    };
    if self_id != identidade_local.device_id() || self_chave != identidade_local.public_base32() {
        return Ok(Err(FalhaDeSemeadura::IdentidadeLocalDivergente {
            esperado: identidade_local.public_base32(),
            no_bundle: self_chave,
        }));
    }

    // ── 2. o receptor está vazio? ───────────────────────────────────────────
    if let Err(falha) = bootstrap_eligible(&tx)? {
        return Ok(Err(falha));
    }

    // ── 3. o bundle é coerente? ─────────────────────────────────────────────
    //
    // Estrutura primeiro: não adianta validar a causalidade de um bundle a que
    // falta uma tabela inteira.
    if let Err(falha) = validar_estrutura(&tx, bundle)? {
        return Ok(Err(falha));
    }
    if let Err(falha) = validar_bundle(bundle) {
        return Ok(Err(falha));
    }
    // E não tenta trocar a identidade do receptor.
    for membro in &bundle.roster {
        if membro.device_id == self_id && membro.ed25519_public != self_chave {
            return Ok(Err(FalhaDeSemeadura::IdentidadeLocalDivergente {
                esperado: self_chave,
                no_bundle: membro.ed25519_public.clone(),
            }));
        }
    }

    // ── 3.1 a identidade do receptor não pode ter passado ───────────────────
    //
    // O próximo `seq` local **não** sai do cursor. Sai do log:
    //
    // ```text
    // SELECT COALESCE(MAX(seq), 0) + 1 FROM sync_events WHERE device_id = ?1
    // ```
    //
    // E o bootstrap não traz o log. Então um receptor cuja identidade já
    // aparece no vetor com 57 alterações começaria a escrever assim:
    //
    // ```text
    // o conjunto conhece   (R, 1) .. (R, 57)     eventos assinados, por aí
    // este aparelho tem    sync_events vazio
    //          ↓
    // próxima escrita  →  (R, 1)                 já existe, com outro conteúdo
    // ```
    //
    // Duas coisas diferentes com a mesma coordenada causal. O `UNIQUE` local
    // não vê — os eventos antigos não estão aqui — e cada peer que já tinha o
    // `(R, 1)` verdadeiro vai recusar o novo por idempotência, sem que nada
    // registre a recusa. A escrita do escritor desaparece em silêncio, num
    // aparelho que parece estar sincronizando normalmente.
    //
    // Semear `cursor[R] = 57` não resolve: cursor não participa da geração de
    // `seq`. O que resolve é o aparelho entrar com identidade nova — que é o
    // que uma instalação nova produz de qualquer forma.
    //
    // Zero é ausência: o conjunto conhece a identidade e não conhece escrita
    // nenhuma dela. Segue sem cursor próprio, como qualquer receptor novo.
    if let Some(&seq) = bundle.vetor.get(&self_id) {
        if seq > 0 {
            return Ok(Err(FalhaDeSemeadura::IdentidadeComPassadoNoBundle {
                device_id: self_id,
                seq,
            }));
        }
    }

    // ── 4. domínio e estado causal, na ordem das FKs ────────────────────────
    for nome in tabelas_copiadas() {
        // `validar_estrutura` já provou que está aqui. O `else` continua
        // existindo porque a alternativa era `continue`, e foi assim que uma
        // tabela inteira podia sumir sem ninguém reclamar.
        let Some(tabela) = bundle.tabelas.iter().find(|t| t.nome == nome) else {
            return Ok(Err(FalhaDeSemeadura::BundleIncoerente {
                motivo: format!("a tabela {nome} sumiu entre a validação e a inserção"),
            }));
        };
        inserir_tabela(&tx, tabela)?;
    }

    // ── 5. roster: merge de confiança, com o self preservado ────────────────
    for membro in &bundle.roster {
        if membro.device_id == self_id {
            // Já existe, e é quem assina aqui. O bundle não redefine isso.
            continue;
        }
        tx.execute(
            "INSERT INTO sync_devices
                (device_id, name, ed25519_public, x25519_public, state, introduced_by,
                 is_self, exit_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
             ON CONFLICT(device_id) DO NOTHING",
            rusqlite::params![
                &membro.device_id,
                &membro.name,
                &membro.ed25519_public,
                &membro.x25519_public,
                &membro.state,
                &membro.introduced_by,
                &membro.exit_reason,
            ],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    }

    // ── 6. os baselines ─────────────────────────────────────────────────────
    //
    // `baseline_seq = last_seq_applied` é a definição de semente: o conteúdo
    // até ali chegou por snapshot, sem passar pelo log. O gatilho de
    // contiguidade da v16 aceita porque o intervalo cobrado é vazio.
    for (origem, seq) in &bundle.vetor {
        if origem == &self_id {
            // O receptor não semeia cursor da própria origem: ele não tem
            // passado nenhum, e o seq local começa do zero.
            continue;
        }
        tx.execute(
            "INSERT INTO sync_cursors (origin_device_id, baseline_seq, last_seq_applied)
             VALUES (?1, ?2, ?2)",
            rusqlite::params![origem, seq],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    }

    // ── 7. o histórico que os gatilhos fabricaram ───────────────────────────
    //
    // `AFTER INSERT ON chapters` e `AFTER INSERT ON entities` escrevem em
    // `change_log`. Semear conteúdo dispara os dois, e o aparelho novo nasceria
    // com um histórico dizendo que o acervo inteiro foi criado no instante do
    // pareamento — uma biografia falsa, escrita por efeito colateral.
    //
    // Apagar TODO o `change_log` aqui é preciso, e não aproximado: a
    // elegibilidade do passo 2 provou, dentro desta mesma transação, que a
    // tabela estava vazia. Tudo o que está nela agora nasceu destes INSERTs.
    tx.execute("DELETE FROM change_log", [])
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // ── 8. conferência antes do COMMIT ──────────────────────────────────────
    if let Err(falha) = conferir_invariantes(&tx, &self_id, bundle)? {
        return Ok(Err(falha));
    }

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(Ok(()))
}

fn inserir_tabela(tx: &Transaction<'_>, tabela: &Tabela) -> DatabaseCommandResult<()> {
    if tabela.linhas.is_empty() {
        return Ok(());
    }
    let colunas = tabela.colunas.join(", ");
    let marcadores = (1..=tabela.colunas.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {} ({colunas}) VALUES ({marcadores})",
        tabela.nome
    );
    let mut statement = tx
        .prepare(&sql)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    for linha in &tabela.linhas {
        statement
            .execute(rusqlite::params_from_iter(linha.iter()))
            .map_err(|error| {
                DatabaseCommandError::storage(format!(
                    "o banco recusou uma linha de {}: {error}",
                    tabela.nome
                ))
            })?;
    }
    Ok(())
}

/// O que precisa ser verdade antes do `COMMIT`.
///
/// Cada item aqui é uma forma conhecida de o seed sair errado sem erro de SQL.
fn conferir_invariantes(
    tx: &Transaction<'_>,
    self_id: &str,
    bundle: &BootstrapBundle,
) -> DatabaseCommandResult<Result<(), FalhaDeSemeadura>> {
    let incoerente = |motivo: String| Ok(Err(FalhaDeSemeadura::BundleIncoerente { motivo }));

    let violacoes: i64 = tx
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if violacoes > 0 {
        return incoerente(format!("{violacoes} referência(s) apontando para o vazio"));
    }

    let donos: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sync_devices WHERE is_self = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if donos != 1 {
        return incoerente(format!(
            "o roster ficou com {donos} dispositivos marcados como `self`; `self` é quem \
             assina, e precisa ser exatamente um"
        ));
    }
    let dono: String = tx
        .query_row(
            "SELECT device_id FROM sync_devices WHERE is_self = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if dono != self_id {
        return incoerente(format!(
            "o `self` deixou de ser {self_id} e passou a ser {dono}: o doador assumiu a \
             identidade do receptor"
        ));
    }

    for tabela in ["sync_events", "sync_applied_events", "sync_peer_vectors"] {
        let linhas: i64 = tx
            .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        if linhas > 0 {
            return incoerente(format!(
                "{linhas} linha(s) em {tabela} depois do bootstrap. Essa tabela não viaja: \
                 o passado é o que o baseline substitui, e confirmação de peer só vale de \
                 primeira mão"
            ));
        }
    }

    let historico: i64 = tx
        .query_row("SELECT COUNT(*) FROM change_log", [], |row| row.get(0))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if historico > 0 {
        return incoerente(format!(
            "{historico} linha(s) de histórico fabricadas pelos gatilhos sobreviveram ao seed"
        ));
    }

    let desalinhados: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sync_cursors WHERE baseline_seq <> last_seq_applied",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if desalinhados > 0 {
        return incoerente(format!(
            "{desalinhados} cursor(es) com baseline diferente do cursor. No bootstrap os dois \
             são o mesmo ponto: nada veio por evento"
        ));
    }

    // O estado causal chegou inteiro.
    for nome in [
        "sync_aggregate_state",
        "sync_revision_history",
        "sync_tombstones",
    ] {
        let esperado = bundle
            .tabelas
            .iter()
            .find(|t| t.nome == nome)
            .map(|t| t.linhas.len() as i64)
            .unwrap_or(0);
        let tem: i64 = tx
            .query_row(&format!("SELECT COUNT(*) FROM {nome}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        if tem != esperado {
            return incoerente(format!(
                "{nome} ficou com {tem} linha(s) e o bundle trazia {esperado}"
            ));
        }
    }

    Ok(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::DeviceIdentity;
    use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};
    use crate::infrastructure::sqlite::sync_repository::{
        append_event_in_transaction, LocalChange,
    };
    use crate::infrastructure::sqlite::sync_session::receber_eventos;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    // ═══════════════════════════════════════════════════════════════════════
    // Apoio: dois aparelhos de verdade, cada um com banco e chave próprios
    // ═══════════════════════════════════════════════════════════════════════

    struct Aparelho {
        banco: TemporaryDatabase,
        identidade: DeviceIdentity,
        _dados: std::path::PathBuf,
    }

    impl Drop for Aparelho {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self._dados);
        }
    }

    impl Aparelho {
        /// Um aparelho recém-instalado: identidade própria, banco vazio.
        fn novo() -> Self {
            let banco = TemporaryDatabase::new();
            let dados = std::env::temp_dir().join(format!("nh-boot-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dados).expect("criar diretório");
            let identidade = crate::application::sync_bootstrap::prepare(&dados, &banco.database)
                .expect("arranque");
            Self {
                banco,
                identidade,
                _dados: dados,
            }
        }

        /// Um doador com acervo: universo, história, livro e capítulos escritos
        /// com evento, do jeito que o serviço de domínio faz.
        fn doador_com_acervo(capitulos: usize) -> Self {
            let aparelho = Self::novo();
            {
                let connection = aparelho.banco.database.write().expect("escrita");
                seed_universe(&connection, "u1");
                connection
                    .execute_batch(
                        "INSERT INTO stories (id, universe_id, name) VALUES ('s1','u1','Historia');
                         INSERT INTO books (id, story_id, name) VALUES ('b1','s1','Livro');",
                    )
                    .expect("semear");
            }
            for i in 1..=capitulos {
                aparelho.escrever(&format!("cap-{i}"), &format!("Capítulo {i}"));
            }
            aparelho
        }

        fn escrever(&self, id: &str, titulo: &str) -> EventEnvelope {
            let payload = format!(
                r#"{{"id":"{id}","book_id":"b1","title":"{titulo}","content":"texto","summary":"","scene_origin":"","scene_destination":"","word_count":1,"status":"rascunho","canon_status":"canon","sort_order":0,"created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
            );
            // Dado e evento na MESMA transação, como `update_chapter` faz.
            //
            // A primeira versão deste apoio dava dois commits — `INSERT` num,
            // evento no outro — e o gate de concorrência da captura reprovou
            // por causa disso, acusando 6 capítulos com vetor em 5. Estava
            // certo: entre os dois commits existe um instante em que o
            // capítulo existe e o evento dele não. O defeito era do apoio, não
            // da captura, e vale registrar porque a conclusão fácil seria
            // afrouxar o gate.
            let mut connection = self.banco.database.write().expect("escrita");
            let tx = connection.transaction().expect("transação");
            tx.execute(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES (?1, 'b1', ?2, 'texto', 1)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title",
                rusqlite::params![id, titulo],
            )
            .expect("gravar capítulo");
            let envelope = append_event_in_transaction(
                &tx,
                &self.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", id),
                    operation: Operation::Upsert,
                    payload: &payload,
                },
            )
            .expect("evento");
            tx.commit().expect("commit");
            envelope
        }

        fn conta(&self, tabela: &str) -> i64 {
            let connection = self.banco.database.write().expect("escrita");
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |row| {
                    row.get(0)
                })
                .expect("contar")
        }

        fn cursor(&self, origem: &str) -> Option<(i64, i64)> {
            let connection = self.banco.database.write().expect("escrita");
            connection
                .query_row(
                    "SELECT baseline_seq, last_seq_applied FROM sync_cursors
                      WHERE origin_device_id = ?1",
                    [origem],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .expect("consultar")
        }
    }

    fn capturar_de(doador: &Aparelho) -> BootstrapBundle {
        let mut connection = doador.banco.database.write().expect("escrita");
        capturar(&mut connection)
            .expect("capturar")
            .expect("o acervo do doador não tem divergência aberta")
    }

    fn semear_em(receptor: &Aparelho, bundle: &BootstrapBundle) -> Result<(), FalhaDeSemeadura> {
        let mut connection = receptor.banco.database.write().expect("escrita");
        semear(&mut connection, &receptor.identidade, bundle).expect("semear")
    }

    // ═══════════════════════════════════════════════════════════════════════
    // A identidade do receptor
    // ═══════════════════════════════════════════════════════════════════════

    /// O caminho feliz, e o que ele fixa sobre `seq`.
    ///
    /// O receptor recebe o baseline do doador e **nenhum cursor próprio**. A
    /// primeira escrita dele nasce como a alteração 1 daquela identidade —
    /// porque é mesmo a primeira: o aparelho é novo, e a identidade também.
    #[test]
    fn o_receptor_novo_comeca_a_propria_sequencia_do_zero() {
        let doador = Aparelho::doador_com_acervo(40);
        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);

        semear_em(&receptor, &bundle).expect("o receptor está vazio");

        assert_eq!(
            receptor.cursor(doador.identidade.device_id()),
            Some((40, 40)),
            "o baseline do doador precisa ser o ponto de partida do incremental"
        );
        assert_eq!(
            receptor.cursor(receptor.identidade.device_id()),
            None,
            "o receptor não tem passado, e cursor da própria origem seria inventar um"
        );

        let envelope = receptor.escrever("cap-do-receptor", "Primeiro daqui");
        assert_eq!(envelope.seq, 1, "a primeira escrita do receptor não é a 1");

        let connection = receptor.banco.database.write().expect("escrita");
        let vetor =
            crate::infrastructure::sqlite::sync_exchange::vetor_local(&connection).expect("vetor");
        assert_eq!(vetor.get(receptor.identidade.device_id()).copied(), Some(1));
    }

    /// Zero no vetor é ausência, não passado.
    ///
    /// O conjunto conhece a identidade — ela está no roster — e não conhece
    /// escrita nenhuma dela. Recusar aqui seria impedir o bootstrap de um
    /// aparelho que foi apresentado ao conjunto antes de escrever qualquer
    /// coisa, que é a ordem normal do pareamento.
    #[test]
    fn identidade_conhecida_sem_escrita_nenhuma_nao_bloqueia() {
        let doador = Aparelho::doador_com_acervo(3);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        bundle.roster.push(MembroDoRoster {
            device_id: receptor.identidade.device_id().to_string(),
            name: "Receptor".to_string(),
            ed25519_public: receptor.identidade.public_base32(),
            x25519_public: String::new(),
            state: "active".to_string(),
            introduced_by: doador.identidade.device_id().to_string(),
            exit_reason: String::new(),
        });
        bundle
            .vetor
            .insert(receptor.identidade.device_id().to_string(), 0);

        semear_em(&receptor, &bundle).expect("zero é ausência de passado");

        assert_eq!(receptor.cursor(receptor.identidade.device_id()), None);
        assert_eq!(receptor.escrever("cap-novo", "Primeiro").seq, 1);
    }

    /// **O guard P0: identidade com passado causal não pode ser reusada.**
    ///
    /// O conjunto conhece 57 alterações feitas por R. O bundle não traz o log
    /// que as contém — é a razão de o bootstrap existir. Se R continuasse
    /// sendo R, a próxima escrita nasceria como a alteração 1:
    ///
    /// ```text
    /// SELECT COALESCE(MAX(seq), 0) + 1 FROM sync_events WHERE device_id = R
    ///        └─ log vazio aqui  →  1
    /// ```
    ///
    /// E `(R, 1)` já existe por aí, com outro conteúdo. Cada peer que tem o
    /// verdadeiro recusa o novo por idempotência, **sem registrar a recusa**.
    /// O capítulo do escritor some num aparelho que parece sincronizar bem.
    ///
    /// Semear `cursor[R] = 57` não resolveria: cursor não entra na geração de
    /// `seq`. O que resolve é identidade nova.
    #[test]
    fn identidade_com_passado_no_conjunto_nao_pode_ser_semeada() {
        let doador = Aparelho::doador_com_acervo(3);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        bundle.roster.push(MembroDoRoster {
            device_id: receptor.identidade.device_id().to_string(),
            name: "Receptor com passado".to_string(),
            ed25519_public: receptor.identidade.public_base32(),
            x25519_public: String::new(),
            state: "active".to_string(),
            introduced_by: doador.identidade.device_id().to_string(),
            exit_reason: String::new(),
        });
        bundle
            .vetor
            .insert(receptor.identidade.device_id().to_string(), 57);

        let falha = semear_em(&receptor, &bundle)
            .expect_err("o conjunto conhece 57 escritas desta identidade");
        assert_eq!(
            falha,
            FalhaDeSemeadura::IdentidadeComPassadoNoBundle {
                device_id: receptor.identidade.device_id().to_string(),
                seq: 57,
            }
        );

        // Rollback integral: nada entrou, e o aparelho continua elegível.
        assert_eq!(
            receptor.conta("chapters"),
            0,
            "o seed deixou conteúdo para trás"
        );
        assert_eq!(receptor.conta("sync_cursors"), 0);
        assert_eq!(
            receptor.conta("sync_devices"),
            1,
            "só o próprio self deveria estar no roster"
        );
    }

    /// **O doador não vira dono do receptor.**
    ///
    /// No banco do doador, ele mesmo é `is_self = 1`. O bundle não carrega essa
    /// coluna — o contrato de [`MembroDoRoster`] a exclui — e por isso o doador
    /// chega ao receptor como o que ele é ali: um peer.
    ///
    /// Restaurar o roster cru daria dois donos à mesma chave, ou faria o doador
    /// assinar no lugar do receptor. `self` é quem assina.
    #[test]
    fn o_doador_entra_como_peer_e_o_receptor_continua_sendo_o_dono() {
        let doador = Aparelho::doador_com_acervo(40);
        let receptor = Aparelho::novo();

        let era_dono_la: i64 = {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .query_row(
                    "SELECT is_self FROM sync_devices WHERE device_id = ?1",
                    [doador.identidade.device_id()],
                    |row| row.get(0),
                )
                .expect("ler")
        };
        assert_eq!(era_dono_la, 1, "no banco do doador, ele é o self");

        let bundle = capturar_de(&doador);
        semear_em(&receptor, &bundle).expect("semear");

        let connection = receptor.banco.database.write().expect("escrita");
        let donos: Vec<String> = connection
            .prepare("SELECT device_id FROM sync_devices WHERE is_self = 1")
            .expect("preparar")
            .query_map([], |row| row.get(0))
            .expect("consultar")
            .collect::<Result<Vec<_>, _>>()
            .expect("coletar");
        assert_eq!(
            donos,
            vec![receptor.identidade.device_id().to_string()],
            "o receptor precisa ser o único dono depois do bootstrap"
        );

        let doador_la: i64 = connection
            .query_row(
                "SELECT is_self FROM sync_devices WHERE device_id = ?1",
                [doador.identidade.device_id()],
                |row| row.get(0),
            )
            .expect("ler o doador no receptor");
        assert_eq!(doador_la, 0, "o doador assumiu a identidade do receptor");

        drop(connection);
        assert_eq!(
            receptor.cursor(doador.identidade.device_id()),
            Some((40, 40))
        );
        assert_eq!(receptor.cursor(receptor.identidade.device_id()), None);
    }

    /// **Toda tabela do banco tem destino declarado.**
    ///
    /// A pergunta "essa tabela vai no bootstrap?" precisa ter resposta antes de
    /// alguém escrever o `INSERT`, e precisa ter resposta para a tabela que
    /// ainda não existe. Sem este gate, uma migration futura acrescenta uma
    /// tabela de domínio, o bootstrap simplesmente não a carrega, e o aparelho
    /// novo nasce sem aquela parte do acervo — com a suíte inteira verde,
    /// porque nenhum teste antigo conhece a tabela nova.
    ///
    /// A lista vem de `sqlite_master`, e não de um levantamento à mão. As três
    /// tentativas manuais que fiz antes deste gate erraram: uma contou 37
    /// tabelas onde havia 36, outra somou 24 itens e escreveu "20", e a
    /// terceira perdeu uma FK criada por `ALTER TABLE`.
    #[test]
    fn toda_tabela_do_banco_tem_destino_declarado_no_bootstrap() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.database.write().expect("escrita");

        let reais: BTreeSet<String> = tabelas_do_banco(&connection)
            .expect("ler sqlite_master")
            .into_iter()
            .collect();
        let catalogadas: BTreeSet<String> =
            CATALOGO.iter().map(|(nome, _)| nome.to_string()).collect();

        let sem_destino: Vec<_> = reais.difference(&catalogadas).collect();
        assert!(
            sem_destino.is_empty(),
            "estas tabelas existem no banco e não têm destino declarado no bootstrap: {sem_destino:?}. \
             Classifique cada uma em Categoria — transferir por engano expõe estado local, e \
             esquecer de transferir faz o aparelho novo nascer sem parte do acervo."
        );

        let fantasmas: Vec<_> = catalogadas.difference(&reais).collect();
        assert!(
            fantasmas.is_empty(),
            "estas tabelas estão no catálogo e não existem mais no banco: {fantasmas:?}. \
             Um catálogo que cita defunto dá falsa sensação de cobertura."
        );
    }

    /// O catálogo não classifica a mesma tabela duas vezes.
    #[test]
    fn nenhuma_tabela_aparece_duas_vezes_no_catalogo() {
        let mut vistas = BTreeMap::new();
        for (nome, categoria) in CATALOGO {
            if let Some(anterior) = vistas.insert(*nome, *categoria) {
                panic!("{nome} está no catálogo duas vezes: {anterior:?} e {categoria:?}");
            }
        }
    }

    /// **A ordem de semeadura respeita as FKs de verdade, não as de memória.**
    ///
    /// Deriva as dependências de `PRAGMA foreign_key_list` — que enxerga tanto
    /// as FKs do `CREATE TABLE` quanto as que chegaram por `ALTER TABLE`. Foi
    /// exatamente uma destas últimas que a primeira versão da ordem perdeu:
    /// `planning_field_definitions.owner_item_id → planning_items`.
    #[test]
    fn a_ordem_de_semeadura_respeita_as_chaves_estrangeiras_reais() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.database.write().expect("escrita");

        let posicao: BTreeMap<&str, usize> = ORDEM_DE_SEMEADURA
            .iter()
            .enumerate()
            .map(|(i, nome)| (*nome, i))
            .collect();

        let transferidas: BTreeSet<&str> = tabelas_de(TransferidaNoBundle).into_iter().collect();
        let na_ordem: BTreeSet<&str> = ORDEM_DE_SEMEADURA.iter().copied().collect();
        assert_eq!(
            transferidas, na_ordem,
            "a ordem de semeadura e a categoria TransferidaNoBundle discordam. Uma tabela que \
             está numa e não na outra ou não é semeada, ou é semeada sem estar no bundle."
        );

        for tabela in ORDEM_DE_SEMEADURA {
            let mut statement = connection
                .prepare(&format!("PRAGMA foreign_key_list({tabela})"))
                .expect("ler foreign_key_list");
            let alvos: Vec<String> = statement
                .query_map([], |row| row.get::<_, String>(2))
                .expect("consultar")
                .collect::<Result<Vec<_>, _>>()
                .expect("coletar");

            for alvo in alvos {
                // Auto-referência não impõe ordem entre tabelas.
                if alvo == *tabela {
                    continue;
                }
                let Some(&pos_alvo) = posicao.get(alvo.as_str()) else {
                    // A FK aponta para fora do bundle. Isso é um problema
                    // diferente e mais grave: a linha semeada não teria para
                    // onde apontar.
                    panic!(
                        "{tabela} tem FK para {alvo}, que não é semeada. Ou {alvo} entra no \
                         bundle, ou {tabela} não pode entrar — semear uma referência para o \
                         vazio quebra a FK no COMMIT."
                    );
                };
                let pos_origem = posicao[*tabela];
                assert!(
                    pos_alvo < pos_origem,
                    "a ordem de semeadura põe {tabela} (posição {pos_origem}) antes de {alvo} \
                     (posição {pos_alvo}), e {tabela} tem FK para {alvo}. Com as foreign keys \
                     ligadas, o INSERT falha — e falha só nos acervos que usam essa relação."
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Concorrência: na captura e na semeadura
    // ═══════════════════════════════════════════════════════════════════════

    /// **O bundle nunca mistura dois instantes.**
    ///
    /// A seção 14 do ADR descreve a perda que isso causa:
    ///
    /// ```text
    /// captura o conteúdo       (o capítulo novo ainda não existe)
    ///         ↓
    ///    ESCRITA CONCORRENTE   (o escritor salva; seq vai a 1803)
    ///         ↓
    /// lê o vetor  →  1803      o receptor nunca vai pedir o 1803
    /// ```
    ///
    /// Aqui o invariante fica observável porque cada capítulo do doador nasce
    /// de exatamente um evento: **o vetor tem que valer o número de capítulos
    /// no bundle**. Se a captura misturasse instantes, os dois discordariam.
    ///
    /// O escritor trabalha numa conexão própria durante as capturas — o banco
    /// está em WAL desde a migration 1, então ele não fica esperando. Nenhuma
    /// das capturas pode ver um estado intermediário.
    ///
    /// # Por que duzentas capturas, e não uma
    ///
    /// A janela que a regressão abriria é curta: se o vetor fosse lido fora da
    /// transação, ele veria um estado posterior ao conteúdo só quando uma
    /// escrita caísse entre o  e a leitura. Com doze tentativas a
    /// mutação **sobreviveu** — o gate existia e não reprovava, que é
    /// exatamente o defeito que esta série de revisões vem perseguindo.
    ///
    /// Duzentas tentativas contra um escritor sem pausa tornam o encontro
    /// praticamente certo. É um gate estatístico, e está assumido como tal:
    /// foi visto reprovando a mutação de forma consistente antes de entrar.
    #[test]
    fn a_captura_nunca_mistura_dois_instantes() {
        let doador = Aparelho::doador_com_acervo(5);
        let caminho = doador.banco.database.path().to_path_buf();
        let parar = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let escritor = {
            let parar = parar.clone();
            let identidade = DeviceIdentity::from_secret_bytes(doador.identidade.secret_bytes());
            let banco = crate::infrastructure::sqlite::connection::SqliteDatabase::new(caminho);
            std::thread::spawn(move || {
                let mut i = 100;
                while !parar.load(std::sync::atomic::Ordering::Relaxed) {
                    let id = format!("cap-conc-{i}");
                    let payload = format!(
                        r#"{{"id":"{id}","book_id":"b1","title":"Concorrente","content":"texto","summary":"","scene_origin":"","scene_destination":"","word_count":1,"status":"rascunho","canon_status":"canon","sort_order":0,"created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
                    );
                    let mut connection = banco.write().expect("escrita concorrente");
                    let tx = connection.transaction().expect("transação");
                    tx.execute(
                        "INSERT INTO chapters (id, book_id, title, content, word_count)
                         VALUES (?1, 'b1', 'Concorrente', 'texto', 1)",
                        [&id],
                    )
                    .expect("gravar capítulo");
                    append_event_in_transaction(
                        &tx,
                        &identidade,
                        &LocalChange {
                            universe_id: "u1",
                            aggregate: AggregateRef::new("chapter", &id),
                            operation: Operation::Upsert,
                            payload: &payload,
                        },
                    )
                    .expect("evento concorrente");
                    tx.commit().expect("commit concorrente");
                    i += 1;
                }
                i
            })
        };

        for tentativa in 1..=200 {
            let bundle = capturar_de(&doador);
            let capitulos = bundle
                .tabelas
                .iter()
                .find(|t| t.nome == "chapters")
                .map(|t| t.linhas.len() as i64)
                .expect("chapters no bundle");
            let anunciado = bundle
                .vetor
                .get(doador.identidade.device_id())
                .copied()
                .unwrap_or(0);

            assert_eq!(
                anunciado, capitulos,
                "captura {tentativa}: o vetor anuncia {anunciado} e o bundle traz {capitulos} \
                 capítulos. Conteúdo de um instante com vetor de outro — o receptor nasceria \
                 com um cursor à frente do que recebeu, e o que faltou nunca seria pedido."
            );

            // E o estado causal veio do mesmo instante que o conteúdo.
            let estados = bundle
                .tabelas
                .iter()
                .find(|t| t.nome == "sync_aggregate_state")
                .map(|t| t.linhas.len() as i64)
                .expect("estado no bundle");
            assert_eq!(
                estados, capitulos,
                "captura {tentativa}: {capitulos} capítulos e {estados} revisões correntes. \
                 Um agregado sem revisão corrente cai em Unknown no primeiro incremental."
            );
        }

        parar.store(true, std::sync::atomic::Ordering::Relaxed);
        let escritas = escritor.join().expect("o escritor terminou");
        assert!(
            escritas > 100,
            "o escritor não chegou a escrever nada durante as capturas; o teste não provou \
             concorrência nenhuma"
        );
    }

    /// **Bootstrap é inicialização, não sincronização.** A segunda vez é
    /// recusada.
    ///
    /// Se o seed pudesse rodar de novo, o snapshot voltaria a ser mecanismo de
    /// sincronização — e com ele todos os problemas do V1: sem exclusão
    /// propagada, sem detecção de concorrência, custo proporcional ao acervo.
    #[test]
    fn o_segundo_bootstrap_e_recusado() {
        let doador = Aparelho::doador_com_acervo(4);
        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);

        semear_em(&receptor, &bundle).expect("o primeiro seed acontece");
        let falha = semear_em(&receptor, &bundle).expect_err("o segundo não pode acontecer");

        assert!(
            matches!(falha, FalhaDeSemeadura::ReceptorNaoEstaVazio { .. }),
            "recusou pelo motivo errado: {falha}"
        );
        assert_eq!(
            receptor.conta("chapters"),
            4,
            "a segunda tentativa mexeu no acervo que já estava aqui"
        );
    }

    /// E o que **impede** a segunda vez não impede a segunda **tentativa**.
    ///
    /// Um seed que falhou e voltou atrás não pode deixar o aparelho num estado
    /// que recuse o retry — senão um erro de rede transitório inutilizaria o
    /// aparelho para sempre, e a única saída seria reinstalar.
    #[test]
    fn seed_que_falhou_deixa_o_aparelho_elegivel_para_tentar_de_novo() {
        let doador = Aparelho::doador_com_acervo(3);
        let receptor = Aparelho::novo();
        let bom = capturar_de(&doador);

        // Um bundle quebrado: a revisão corrente não está na história.
        let mut ruim = bom.clone();
        if let Some(estado) = ruim
            .tabelas
            .iter_mut()
            .find(|t| t.nome == "sync_aggregate_state")
        {
            if let Some(linha) = estado.linhas.first_mut() {
                linha[2] = Value::Text("revisao-que-nao-existe".to_string());
            }
        }

        let falha = semear_em(&receptor, &ruim).expect_err("o bundle é incoerente");
        assert!(matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }));

        assert_eq!(
            receptor.conta("chapters"),
            0,
            "o seed falho deixou conteúdo"
        );
        assert_eq!(receptor.conta("sync_cursors"), 0);
        assert_eq!(receptor.conta("sync_aggregate_state"), 0);

        semear_em(&receptor, &bom).expect("o retry com bundle bom precisa concluir");
        assert_eq!(receptor.conta("chapters"), 3);
    }

    /// **O capítulo criado offline bloqueia o seed.**
    ///
    /// É o caso que a checagem por cursor não pegava. `create_chapter` grava
    /// conteúdo sem evento e sem cursor enquanto a NH-053 estiver aberta:
    ///
    /// ```text
    /// chapters       1 linha       ← o trabalho do escritor
    /// sync_events    vazio
    /// sync_cursors   vazio         ← "está virgem", diria a checagem antiga
    /// ```
    ///
    /// O seed passaria por cima daquele capítulo, e nada registraria a perda.
    #[test]
    fn conteudo_criado_sem_evento_bloqueia_o_seed() {
        let doador = Aparelho::doador_com_acervo(3);
        let receptor = Aparelho::novo();
        {
            let connection = receptor.banco.database.write().expect("escrita");
            seed_universe(&connection, "u-local");
            connection
                .execute_batch(
                    "INSERT INTO stories (id, universe_id, name) VALUES ('s-l','u-local','Minha');
                     INSERT INTO books (id, story_id, name) VALUES ('b-l','s-l','Livro');
                     INSERT INTO chapters (id, book_id, title, content, word_count)
                        VALUES ('meu-cap','b-l','Escrito no avião','texto meu',2);",
                )
                .expect("o escritor usou o aparelho antes de parear");
        }

        assert_eq!(
            receptor.conta("sync_events"),
            0,
            "create_chapter não emite evento"
        );
        assert_eq!(receptor.conta("sync_cursors"), 0, "e não cria cursor");

        let bundle = capturar_de(&doador);
        let falha = semear_em(&receptor, &bundle).expect_err("há trabalho local aqui");
        assert!(
            matches!(falha, FalhaDeSemeadura::ReceptorNaoEstaVazio { .. }),
            "recusou pelo motivo errado: {falha}"
        );

        let titulo: String = {
            let connection = receptor.banco.database.write().expect("escrita");
            connection
                .query_row(
                    "SELECT title FROM chapters WHERE id = 'meu-cap'",
                    [],
                    |row| row.get(0),
                )
                .expect("o capítulo do escritor precisa continuar aqui")
        };
        assert_eq!(titulo, "Escrito no avião");
        assert_eq!(receptor.conta("chapters"), 1, "o seed acrescentou conteúdo");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Depois do bootstrap: o incremental continua valendo
    // ═══════════════════════════════════════════════════════════════════════

    /// **O baseline substitui o passado, e só o passado.**
    ///
    /// Acima dele, a densidade continua sendo cobrada. É o que separa "semente"
    /// de "cursor solto": o receptor nasce em 40 sem ter os eventos 1..40, e a
    /// partir do 41 cada seq precisa ter passado pelo log como sempre.
    ///
    /// ```text
    /// baseline 40   ← veio por snapshot, nada é cobrado abaixo
    ///          41   ← aplica, cursor vai a 41
    ///          43   ← chega sem o 42: o cursor NÃO avança
    /// ```
    #[test]
    fn acima_do_baseline_a_densidade_continua_sendo_cobrada() {
        let doador = Aparelho::doador_com_acervo(40);
        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);
        semear_em(&receptor, &bundle).expect("semear");
        assert_eq!(
            receptor.cursor(doador.identidade.device_id()),
            Some((40, 40))
        );

        let quarenta_e_um = doador.escrever("cap-41", "Depois da semente");
        let _quarenta_e_dois = doador.escrever("cap-42", "Nunca entregue");
        let quarenta_e_tres = doador.escrever("cap-43", "Fora de ordem");

        {
            let mut connection = receptor.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &[quarenta_e_um]).expect("receber o 41");
        }
        assert_eq!(
            receptor.cursor(doador.identidade.device_id()),
            Some((40, 41)),
            "o primeiro incremental acima do baseline precisa aplicar e avançar"
        );

        {
            let mut connection = receptor.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &[quarenta_e_tres]).expect("receber o 43");
        }
        assert_eq!(
            receptor.cursor(doador.identidade.device_id()),
            Some((40, 41)),
            "o cursor passou por cima de uma lacuna: o 42 nunca mais seria pedido"
        );
    }

    /// **A exclusão viaja, e continua impedindo a ressurreição.**
    ///
    /// O tombstone é o que dá ao receptor a diferença entre "nunca existiu" e
    /// "foi apagado". Sem ele no bundle, uma edição antiga que chegasse depois
    /// recriaria o capítulo — e o escritor veria voltar algo que ele apagou.
    #[test]
    fn a_exclusao_viaja_no_bundle_e_impede_a_ressurreicao() {
        let doador = Aparelho::doador_com_acervo(2);
        let criacao = doador.escrever("cap-condenado", "Vai sumir");

        // O doador apaga, e o tombstone nasce com a exclusão local.
        {
            let mut connection = doador.banco.database.write().expect("escrita");
            let tx = connection.transaction().expect("transação");
            tx.execute("DELETE FROM chapters WHERE id = 'cap-condenado'", [])
                .expect("apagar o capítulo");
            append_event_in_transaction(
                &tx,
                &doador.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", "cap-condenado"),
                    operation: Operation::Delete,
                    payload: "",
                },
            )
            .expect("evento de exclusão");
            tx.commit().expect("commit");
        }
        assert_eq!(
            doador.conta("sync_tombstones"),
            1,
            "a exclusão precisa deixar tombstone"
        );

        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);
        semear_em(&receptor, &bundle).expect("semear");

        assert_eq!(
            receptor.conta("sync_tombstones"),
            1,
            "o tombstone não viajou: o receptor não sabe que aquilo foi apagado"
        );

        // Um peer atrasado repassa a criação antiga.
        {
            let mut connection = receptor.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &[criacao]).expect("receber o evento velho");
        }

        let existe: i64 = {
            let connection = receptor.banco.database.write().expect("escrita");
            connection
                .query_row(
                    "SELECT COUNT(*) FROM chapters WHERE id = 'cap-condenado'",
                    [],
                    |row| row.get(0),
                )
                .expect("contar")
        };
        assert_eq!(existe, 0, "o capítulo apagado ressuscitou no aparelho novo");
    }

    /// **Confirmação de peer não viaja, e a poda não ganha autoridade de graça.**
    ///
    /// `sync_peer_vectors` guarda o que cada peer nos disse que viu, e a etapa
    /// 11 exigiu [`crate::infrastructure::sync_transport::SessaoAutenticada`]
    /// para escrever ali — justamente para um peer não anunciar o vetor de
    /// outro. Copiá-la no bundle faria o doador declarar, por terceiros,
    /// confirmações que o receptor nunca ouviu. E essas confirmações autorizam
    /// **poda**: o receptor apagaria tombstones apoiado em evidência de segunda
    /// mão.
    #[test]
    fn o_receptor_nao_herda_confirmacoes_de_terceiros() {
        let doador = Aparelho::doador_com_acervo(3);
        let terceiro = Aparelho::novo();

        // O doador acumula confirmação de um terceiro.
        {
            let connection = doador.banco.database.write().expect("escrita");
            let sessao = crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                &terceiro.identidade,
            );
            crate::infrastructure::sqlite::sync_trust::introduzir_dispositivo(
                &connection,
                &crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                    &doador.identidade,
                ),
                terceiro.identidade.device_id(),
                &terceiro.identidade.public_base32(),
            )
            .expect("apresentar o terceiro");
            let mut vetor = std::collections::BTreeMap::new();
            vetor.insert(doador.identidade.device_id().to_string(), 3);
            crate::infrastructure::sqlite::sync_gc::registrar_vetor_do_peer(
                &connection,
                &sessao,
                &vetor,
            )
            .expect("registrar confirmação");
        }
        assert!(doador.conta("sync_peer_vectors") > 0);

        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);
        semear_em(&receptor, &bundle).expect("semear");

        assert_eq!(
            receptor.conta("sync_peer_vectors"),
            0,
            "o receptor herdou confirmações que ninguém lhe deu — a poda passaria a se \
             apoiar em evidência de segunda mão"
        );
        assert_eq!(
            receptor.conta("sync_events"),
            0,
            "o log histórico viajou: é o que o baseline existe para substituir"
        );
        assert_eq!(receptor.conta("sync_applied_events"), 0);
    }

    /// **As cinco classificações sobrevivem ao bootstrap.**
    ///
    /// É o gate que resume o contrato do bundle: se o estado causal chega
    /// inteiro, `classify` responde no receptor exatamente o que responderia no
    /// doador. Qualquer uma das três tabelas que faltasse mudaria pelo menos
    /// uma destas respostas — e `Unknown` é a que congela o cursor.
    #[test]
    fn a_classificacao_causal_e_a_mesma_antes_e_depois_do_bootstrap() {
        use crate::domain::sync::{classify, Causality};

        let doador = Aparelho::doador_com_acervo(1);
        let criacao = doador.escrever("cap-hist", "Original");
        let edicao = doador.escrever("cap-hist", "Editado");
        doador.escrever("cap-morto", "Vai sumir");
        let criacao_morta = {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .query_row(
                    "SELECT rev FROM sync_revision_history WHERE aggregate_id = 'cap-morto'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("revisão do condenado")
        };
        {
            let mut connection = doador.banco.database.write().expect("escrita");
            let tx = connection.transaction().expect("transação");
            tx.execute("DELETE FROM chapters WHERE id = 'cap-morto'", [])
                .expect("apagar");
            append_event_in_transaction(
                &tx,
                &doador.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", "cap-morto"),
                    operation: Operation::Delete,
                    payload: "",
                },
            )
            .expect("exclusão");
            tx.commit().expect("commit");
        }

        let casos: Vec<(&str, &str, &str, &str)> = vec![
            // (nome, agregado, base_rev, new_rev)
            (
                "AlreadyPresent",
                "cap-hist",
                &criacao.new_rev,
                &edicao.new_rev,
            ),
            ("Sequential", "cap-hist", &edicao.new_rev, "rev-seguinte"),
            ("Concurrent", "cap-hist", &criacao.new_rev, "rev-paralela"),
            ("Unknown", "cap-hist", "rev-desconhecida", "rev-qualquer"),
            (
                "ConcurrentComExclusao",
                "cap-morto",
                &criacao_morta,
                "rev-editada-la",
            ),
        ];

        let ler = |aparelho: &Aparelho, id: &str| {
            let connection = aparelho.banco.database.write().expect("escrita");
            crate::infrastructure::sqlite::sync_repository::aggregate_history(
                &connection,
                &AggregateRef::new("chapter", id),
            )
            .expect("história")
        };

        let antes: Vec<Causality> = casos
            .iter()
            .map(|(_, id, base, nova)| classify(&ler(&doador, id), base, nova))
            .collect();

        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);
        semear_em(&receptor, &bundle).expect("semear");

        let depois: Vec<Causality> = casos
            .iter()
            .map(|(_, id, base, nova)| classify(&ler(&receptor, id), base, nova))
            .collect();

        for (i, (nome, ..)) in casos.iter().enumerate() {
            assert_eq!(
                antes[i], depois[i],
                "a classificação de {nome} mudou com o bootstrap: {:?} virou {:?}. O estado \
                 causal não chegou inteiro, e o incremental vai se comportar diferente no \
                 aparelho novo.",
                antes[i], depois[i]
            );
        }
        assert!(
            matches!(depois[0], Causality::AlreadyPresent),
            "o caso de reentrega deixou de ser reconhecido: {:?}",
            depois[0]
        );
        assert!(
            matches!(depois[4], Causality::ConcurrentComExclusao { .. }),
            "a exclusão deixou de ser vista como exclusão: {:?}",
            depois[4]
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Validação semântica do bundle, e a captura que se recusa
    // ═══════════════════════════════════════════════════════════════════════

    /// Revisão corrente fora da história: o receptor nasceria sem futuro.
    ///
    /// Nenhuma FK cobra isto — `current_rev` é texto. E o efeito não aparece no
    /// seed: aparece no primeiro evento que chegar, quando `classify` não
    /// reconhecer a base e devolver `Unknown` para sempre.
    #[test]
    fn revisao_corrente_fora_da_historia_reprova_o_bundle() {
        let doador = Aparelho::doador_com_acervo(2);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        let estado = bundle
            .tabelas
            .iter_mut()
            .find(|t| t.nome == "sync_aggregate_state")
            .expect("estado no bundle");
        estado.linhas[0][2] = Value::Text("rev-que-ninguem-conhece".to_string());

        let falha = semear_em(&receptor, &bundle).expect_err("bundle incoerente");
        assert!(
            matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }),
            "recusou pelo motivo errado: {falha}"
        );
        assert_eq!(receptor.conta("chapters"), 0, "rollback incompleto");
    }

    /// Exclusão apontando para revisão inexistente.
    #[test]
    fn exclusao_fora_da_historia_reprova_o_bundle() {
        let doador = Aparelho::doador_com_acervo(1);
        doador.escrever("cap-morto", "Vai sumir");
        {
            let mut connection = doador.banco.database.write().expect("escrita");
            let tx = connection.transaction().expect("transação");
            tx.execute("DELETE FROM chapters WHERE id = 'cap-morto'", [])
                .expect("apagar");
            append_event_in_transaction(
                &tx,
                &doador.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", "cap-morto"),
                    operation: Operation::Delete,
                    payload: "",
                },
            )
            .expect("exclusão");
            tx.commit().expect("commit");
        }

        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);
        let tumulos = bundle
            .tabelas
            .iter_mut()
            .find(|t| t.nome == "sync_tombstones")
            .expect("tombstones no bundle");
        tumulos.linhas[0][2] = Value::Text("rev-de-exclusao-inventada".to_string());

        let falha = semear_em(&receptor, &bundle).expect_err("bundle incoerente");
        assert!(matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }));
        assert_eq!(receptor.conta("chapters"), 0);
        assert_eq!(receptor.conta("sync_tombstones"), 0);
    }

    /// Exclusão vinda de uma origem que não está no roster.
    ///
    /// A poda pergunta, para cada membro ainda válido, se ele atravessou a
    /// exclusão. Uma origem fora do roster torna a pergunta insolúvel — e o
    /// tombstone ficaria preso para sempre, ou seria coletado sem prova.
    #[test]
    fn exclusao_de_origem_fora_do_roster_reprova_o_bundle() {
        let doador = Aparelho::doador_com_acervo(1);
        doador.escrever("cap-morto", "Vai sumir");
        {
            let mut connection = doador.banco.database.write().expect("escrita");
            let tx = connection.transaction().expect("transação");
            tx.execute("DELETE FROM chapters WHERE id = 'cap-morto'", [])
                .expect("apagar");
            append_event_in_transaction(
                &tx,
                &doador.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", "cap-morto"),
                    operation: Operation::Delete,
                    payload: "",
                },
            )
            .expect("exclusão");
            tx.commit().expect("commit");
        }

        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);
        let tumulos = bundle
            .tabelas
            .iter_mut()
            .find(|t| t.nome == "sync_tombstones")
            .expect("tombstones no bundle");
        tumulos.linhas[0][4] = Value::Text("aparelho-que-nao-esta-no-roster".to_string());

        let falha = semear_em(&receptor, &bundle).expect_err("origem desconhecida");
        assert!(matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }));
    }

    /// Origem do vetor fora do roster: a FK pegaria, mas tarde demais.
    ///
    /// Chegar até o `INSERT` de cursores com meia semeadura feita e descobrir
    /// ali funciona — a transação volta atrás — mas o erro que chega ao humano
    /// é `FOREIGN KEY constraint failed`, que não diz nada sobre o que está
    /// errado no bundle.
    #[test]
    fn origem_do_vetor_fora_do_roster_reprova_antes_de_qualquer_escrita() {
        let doador = Aparelho::doador_com_acervo(2);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);
        bundle.vetor.insert("origem-fantasma".to_string(), 7);

        let falha = semear_em(&receptor, &bundle).expect_err("origem fora do roster");
        assert!(matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }));
        assert_eq!(receptor.conta("chapters"), 0, "escreveu antes de validar");
    }

    /// **A captura se recusa enquanto houver decisão pendente.**
    ///
    /// A versão perdedora de uma divergência vive no payload de um evento, e o
    /// bundle não carrega o log. `sync_revision_history` guarda o hash e a
    /// ancestralidade — não o conteúdo. Semear agora faria a versão perdedora
    /// desaparecer semanticamente do aparelho novo, sem ninguém ter escolhido.
    #[test]
    fn a_captura_se_recusa_com_divergencia_aberta() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO sync_divergences
                        (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                         local_operation, remote_operation, remote_event_id)
                     VALUES ('d1','chapter','cap-1','base','local','remota','upsert','upsert','e1')",
                    [],
                )
                .expect("registrar divergência");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        let falha = capturar(&mut connection)
            .expect("consultar")
            .expect_err("há decisão pendente");
        assert_eq!(falha, FalhaDeCaptura::DivergenciaAberta { quantas: 1 });
    }

    /// **Evento pendente não vira baseline.**
    ///
    /// Fecha o ciclo do defeito de `vetor_local`: um evento guardado e não
    /// aplicado não é progresso, e o bundle não pode transformá-lo em semente.
    /// Se virasse, o receptor nasceria com `baseline = 2` acreditando que o
    /// snapshot cobre um conteúdo que ninguém aplicou — e nunca pediria o que
    /// falta.
    #[test]
    fn evento_pendente_no_doador_nao_vira_baseline_no_receptor() {
        let doador = Aparelho::doador_com_acervo(2);
        let estranho = Aparelho::doador_com_acervo(0);
        {
            let connection = doador.banco.database.write().expect("escrita");
            crate::infrastructure::sqlite::sync_trust::introduzir_dispositivo(
                &connection,
                &crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(
                    &doador.identidade,
                ),
                estranho.identidade.device_id(),
                &estranho.identidade.public_base32(),
            )
            .expect("apresentar");
        }

        // O estranho escreve dois eventos; só o segundo chega ao doador.
        let _primeiro = estranho.escrever("cap-e1", "Primeiro");
        let segundo = estranho.escrever("cap-e2", "Segundo");
        {
            let mut connection = doador.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &[segundo]).expect("receber fora de ordem");
        }

        let bundle = capturar_de(&doador);
        assert_eq!(
            bundle
                .vetor
                .get(estranho.identidade.device_id())
                .copied()
                .unwrap_or(0),
            0,
            "o vetor do bundle transformou um evento pendente em semente"
        );

        let receptor = Aparelho::novo();
        semear_em(&receptor, &bundle).expect("semear");
        assert_eq!(
            receptor.cursor(estranho.identidade.device_id()),
            None,
            "o receptor nasceu com baseline de conteúdo que ninguém aplicou"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Integridade estrutural do bundle
    // ═══════════════════════════════════════════════════════════════════════

    /// **Bundle truncado é recusado, não completado em silêncio.**
    ///
    /// Este é o pior formato de defeito que a etapa 12 podia ter: o receptor
    /// nasceria com universo, história, livros, roster, estado causal e
    /// cursores todos corretos — e **sem os capítulos**. Nada pareceria errado,
    /// e o cursor semeado faria o aparelho nunca pedir o que não veio.
    ///
    /// A primeira versão da semeadura fazia `continue` quando a tabela não
    /// estava no bundle. A validação causal tratava ausente como vazia, e a
    /// conferência final só comparava as três tabelas causais. As três camadas
    /// olhavam para o lado ao mesmo tempo.
    #[test]
    fn bundle_sem_uma_tabela_inteira_e_recusado() {
        let doador = Aparelho::doador_com_acervo(4);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        let antes = bundle.tabelas.len();
        bundle.tabelas.retain(|t| t.nome != "chapters");
        assert_eq!(
            bundle.tabelas.len(),
            antes - 1,
            "o cenário precisa remover mesmo"
        );

        let falha = semear_em(&receptor, &bundle).expect_err("falta uma tabela inteira");
        // O assert é sobre a mensagem da validação ESTRUTURAL, não sobre
        // qualquer recusa que cite "chapters".
        //
        // A primeira versão aceitava as duas: a estrutural ("faltam tabelas") e
        // a rede de segurança do laço de inserção ("sumiu entre a validação e a
        // inserção"). Com isso, mutar uma das camadas deixava o gate verde pela
        // outra, e nenhuma das duas ficava provada. São camadas independentes de
        // propósito — a de cima explica ao humano o que está errado no bundle, a
        // de baixo existe para um caminho futuro não chegar ao INSERT sem passar
        // pela primeira — e cada uma precisa do próprio veredito.
        match &falha {
            FalhaDeSemeadura::BundleIncoerente { motivo } => assert!(
                motivo.contains("faltam tabelas") && motivo.contains("chapters"),
                "a recusa precisa vir da validação estrutural, nomeando o que falta: {motivo}"
            ),
            outra => panic!("recusou pelo motivo errado: {outra}"),
        }

        assert_eq!(receptor.conta("chapters"), 0);
        assert_eq!(receptor.conta("universes"), 0, "rollback incompleto");
        assert_eq!(receptor.conta("sync_cursors"), 0);
        assert_eq!(receptor.conta("sync_devices"), 1, "só o próprio self");
    }

    /// Tabela repetida no bundle: qual das duas vale não é pergunta para o meio
    /// de um seed.
    #[test]
    fn bundle_com_tabela_duplicada_e_recusado() {
        let doador = Aparelho::doador_com_acervo(2);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        let copia = bundle
            .tabelas
            .iter()
            .find(|t| t.nome == "chapters")
            .cloned()
            .expect("chapters no bundle");
        bundle.tabelas.push(copia);

        let falha = semear_em(&receptor, &bundle).expect_err("tabela duplicada");
        match &falha {
            FalhaDeSemeadura::BundleIncoerente { motivo } => assert!(
                motivo.contains("duas vezes"),
                "recusou pelo motivo errado: {motivo}"
            ),
            outra => panic!("recusou pelo motivo errado: {outra}"),
        }
        assert_eq!(receptor.conta("chapters"), 0);
    }

    /// Tabela que não pertence ao bundle também reprova.
    ///
    /// O caso perigoso não é a tabela inventada: é uma tabela **local** viajando
    /// disfarçada de acervo, num bundle montado por outra versão do aplicativo.
    #[test]
    fn bundle_com_tabela_estranha_e_recusado() {
        let doador = Aparelho::doador_com_acervo(2);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        bundle.tabelas.push(Tabela {
            nome: "change_log".to_string(),
            colunas: vec!["id".to_string()],
            linhas: vec![vec![Value::Text("x".to_string())]],
        });

        let falha = semear_em(&receptor, &bundle).expect_err("tabela fora do contrato");
        assert!(matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }));
        assert_eq!(receptor.conta("change_log"), 0);
    }

    /// **Coluna faltando não entra pelo DEFAULT.**
    ///
    /// É o caso silencioso da família: `chapters.word_count` tem
    /// `DEFAULT 0`, então um `INSERT` sem ela funcionaria — e o acervo inteiro
    /// chegaria com contagem de palavras zerada. A estatística do universo soma
    /// `word_count`; o escritor veria o próprio trabalho valendo zero palavra.
    ///
    /// As colunas esperadas vêm de `PRAGMA table_info` **do receptor**, dentro
    /// da transação, e não de uma segunda lista mantida à mão.
    #[test]
    fn coluna_faltando_no_bundle_nao_entra_pelo_default() {
        let doador = Aparelho::doador_com_acervo(3);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        let capitulos = bundle
            .tabelas
            .iter_mut()
            .find(|t| t.nome == "chapters")
            .expect("chapters no bundle");
        let posicao = capitulos
            .colunas
            .iter()
            .position(|c| c == "word_count")
            .expect("a coluna existe no schema");
        capitulos.colunas.remove(posicao);
        for linha in capitulos.linhas.iter_mut() {
            linha.remove(posicao);
        }

        let falha = semear_em(&receptor, &bundle).expect_err("falta uma coluna");
        match &falha {
            FalhaDeSemeadura::BundleIncoerente { motivo } => assert!(
                motivo.contains("word_count") || motivo.contains("colunas de chapters"),
                "recusou pelo motivo errado: {motivo}"
            ),
            outra => panic!("recusou pelo motivo errado: {outra}"),
        }
        assert_eq!(receptor.conta("chapters"), 0);
    }

    /// Linha com número de valores diferente do de colunas.
    #[test]
    fn linha_com_aridade_errada_e_recusada() {
        let doador = Aparelho::doador_com_acervo(2);
        let receptor = Aparelho::novo();
        let mut bundle = capturar_de(&doador);

        let capitulos = bundle
            .tabelas
            .iter_mut()
            .find(|t| t.nome == "chapters")
            .expect("chapters no bundle");
        capitulos.linhas[0].pop();

        let falha = semear_em(&receptor, &bundle).expect_err("aridade errada");
        assert!(matches!(falha, FalhaDeSemeadura::BundleIncoerente { .. }));
        assert_eq!(receptor.conta("chapters"), 0);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Divergência: aberta bloqueia, resolvida não
    // ═══════════════════════════════════════════════════════════════════════

    /// **Decisão já tomada não bloqueia o bootstrap.**
    ///
    /// O schema define aberta como `resolved_at = ''` — tem índice parcial com
    /// esse predicado. A primeira versão da captura contava a tabela inteira, e
    /// o efeito era pior do que uma recusa a mais: bastava o escritor ter
    /// resolvido **uma** divergência na vida daquele acervo para nunca mais
    /// conseguir parear um aparelho novo. Uma divergência resolvida é uma
    /// escolha que já está no conteúdo.
    #[test]
    fn divergencia_resolvida_nao_bloqueia_a_captura() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO sync_divergences
                        (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                         local_operation, remote_operation, remote_event_id, resolved_at,
                         resolution)
                     VALUES ('d-velha','chapter','cap-1','b','l','r','upsert','upsert','e1',
                             '2026-09-01 10:00:00','local')",
                    [],
                )
                .expect("divergência já resolvida");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        capturar(&mut connection)
            .expect("consultar")
            .expect("decisão já tomada não impede o bootstrap");
    }

    /// E uma aberta ao lado de resolvidas continua bloqueando, contando só a
    /// aberta.
    #[test]
    fn a_captura_conta_apenas_as_divergencias_abertas() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute_batch(
                    "INSERT INTO sync_divergences
                        (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                         local_operation, remote_operation, remote_event_id, resolved_at,
                         resolution)
                     VALUES ('d-velha','chapter','cap-1','b','l','r','upsert','upsert','e1',
                             '2026-09-01 10:00:00','local');
                     INSERT INTO sync_divergences
                        (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                         local_operation, remote_operation, remote_event_id)
                     VALUES ('d-aberta','chapter','cap-2','b','l','r','upsert','upsert','e2');",
                )
                .expect("uma resolvida e uma aberta");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        let falha = capturar(&mut connection)
            .expect("consultar")
            .expect_err("há uma decisão pendente");
        assert_eq!(
            falha,
            FalhaDeCaptura::DivergenciaAberta { quantas: 1 },
            "a contagem precisa ignorar as já resolvidas"
        );
    }

    /// Conflito do V1 **já resolvido** não bloqueia.
    ///
    /// Decisão tomada já está no conteúdo, como no V2.
    #[test]
    fn conflito_v1_resolvido_nao_bloqueia_a_captura() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value,
                         resolved_at)
                     VALUES ('c-velho','chapter','cap-1','title','Meu','Dele',
                             '2026-09-01 10:00:00')",
                    [],
                )
                .expect("conflito V1 já resolvido");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        capturar(&mut connection)
            .expect("consultar")
            .expect("decisão já tomada não impede o bootstrap");
    }

    /// **O conflito V1 aberto guarda uma versão que não está no acervo.**
    ///
    /// O caso completo: o capítulo materializado diz "A", e a linha de conflito
    /// guarda `remote_value = "B"` — a versão que veio do outro aparelho e que
    /// o escritor ainda não escolheu.
    ///
    /// ```text
    /// chapters.title        "A"          viaja no bundle
    /// sync_conflicts        local "A"    NÃO viaja
    ///                       remote "B"   ← some do mundo do aparelho novo
    /// ```
    ///
    /// A tabela é `BloqueiaBootstrap` no catálogo, o que impedia o receptor de
    /// ser semeado tendo um conflito. Faltava a outra ponta: impedir o doador
    /// de capturar tendo um.
    #[test]
    fn conflito_v1_aberto_recusa_a_captura() {
        let doador = Aparelho::doador_com_acervo(1);
        doador.escrever("cap-disputado", "A");
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                     VALUES ('c-aberto','chapter','cap-disputado','title','A','B')",
                    [],
                )
                .expect("conflito V1 aberto");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        let falha = capturar(&mut connection)
            .expect("consultar")
            .expect_err("há uma versão pendente que não viaja");
        assert_eq!(falha, FalhaDeCaptura::ConflitoV1Aberto { quantas: 1 });

        // E a mensagem fala do V1, não do log de eventos do V2.
        let texto = falha.to_string();
        assert!(
            texto.contains("sistema antigo"),
            "a mensagem precisa mandar o escritor procurar no lugar certo: {texto}"
        );
    }

    /// E a contagem ignora os já resolvidos.
    #[test]
    fn a_captura_conta_apenas_os_conflitos_v1_abertos() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute_batch(
                    "INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value,
                         resolved_at)
                     VALUES ('c-velho','chapter','cap-1','title','Meu','Dele',
                             '2026-09-01 10:00:00');
                     INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                     VALUES ('c-aberto','chapter','cap-2','title','Meu','Dele');",
                )
                .expect("um resolvido e um aberto");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        let falha = capturar(&mut connection)
            .expect("consultar")
            .expect_err("há um conflito pendente");
        assert_eq!(
            falha,
            FalhaDeCaptura::ConflitoV1Aberto { quantas: 1 },
            "a contagem precisa ignorar os já resolvidos"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Mídia que não pôde ser convertida (ADR 0010)
    // ═══════════════════════════════════════════════════════════════════════

    /// **Asset não convertido recusa a captura.**
    ///
    /// A tabela `blob_migration_issues` chegou no schema 20 e o gate de
    /// catálogo desta etapa a pegou sem destino declarado — que é literalmente
    /// para isso que ele existe. A classificação é `BloqueiaBootstrap`, e o
    /// lado do doador é este:
    ///
    /// ```text
    /// entities.image = "https://cdn.exemplo.com/rosto.png"
    ///                   └─ o aplicativo não sabe abrir isso, e não vai tentar
    ///                      baixar. O valor fica; o hash fica vazio.
    /// ```
    ///
    /// O acervo continua utilizável — a imagem aparece na tela como sempre
    /// apareceu. O que não pode acontecer é o pareamento: o aparelho novo
    /// receberia a linha com o hash vazio e sem nenhum blob, e a imagem
    /// simplesmente não existiria lá.
    #[test]
    fn asset_nao_convertido_recusa_a_captura() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO blob_migration_issues
                        (id, surface, table_name, row_id, side, reason, detail)
                     VALUES ('i1', 3, 'entities', 'e1', 'image', 'legacy_unrecognized',
                             'valor inline não é uma data URL')",
                    [],
                )
                .expect("pendência de mídia");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        let falha = capturar(&mut connection)
            .expect("consultar")
            .expect_err("há mídia que não viaja no contrato novo");
        assert_eq!(falha, FalhaDeCaptura::AssetNaoConvertido { quantas: 1 });

        // E a mensagem não manda o escritor resolver conflito nenhum: não há
        // decisão pendente aqui, há arquivo que o aplicativo não soube ler.
        let texto = falha.to_string();
        assert!(
            texto.contains("converter") && !texto.contains("conflito"),
            "a mensagem precisa falar de conversão, não de decisão: {texto}"
        );
    }

    /// Pendência já resolvida não bloqueia.
    ///
    /// É história da migração deste aparelho, e história não impede pareamento
    /// — o mesmo defeito que a revisão da etapa 12 encontrou na contagem de
    /// divergências, que somava as resolvidas.
    #[test]
    fn pendencia_de_midia_resolvida_nao_bloqueia_a_captura() {
        let doador = Aparelho::doador_com_acervo(2);
        {
            let connection = doador.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO blob_migration_issues
                        (id, surface, table_name, row_id, side, reason, resolved_at)
                     VALUES ('i1', 3, 'entities', 'e1', 'image', 'legacy_unrecognized',
                             '2026-09-10 13:00:00')",
                    [],
                )
                .expect("pendência já resolvida");
        }

        let mut connection = doador.banco.database.write().expect("escrita");
        capturar(&mut connection)
            .expect("consultar")
            .expect("mídia já convertida não impede o bootstrap");
    }

    /// **E o receptor com pendência de mídia não é virgem.**
    ///
    /// O outro lado da classificação, e ele vem de graça: `bootstrap_eligible`
    /// deriva da categoria, então declarar a tabela como `BloqueiaBootstrap`
    /// basta. Um aparelho recém-instalado não tem histórico de migração
    /// nenhum; linha aqui significa que ele já rodou o backfill sobre um acervo
    /// que era dele.
    #[test]
    fn receptor_com_pendencia_de_midia_nao_e_semeado() {
        let doador = Aparelho::doador_com_acervo(3);
        let receptor = Aparelho::novo();
        let bundle = capturar_de(&doador);
        {
            let connection = receptor.banco.database.write().expect("escrita");
            connection
                .execute(
                    "INSERT INTO blob_migration_issues
                        (id, surface, table_name, row_id, side, reason)
                     VALUES ('i1', 1, 'attachments', 'a1', 'data_url', 'legacy_unrecognized')",
                    [],
                )
                .expect("o receptor tem passado");
        }

        let falha = semear_em(&receptor, &bundle).expect_err("o receptor não está virgem");
        match &falha {
            FalhaDeSemeadura::ReceptorNaoEstaVazio { tabela, linhas } => {
                assert_eq!(tabela, "blob_migration_issues");
                assert_eq!(*linhas, 1);
            }
            outra => panic!("recusou pelo motivo errado: {outra}"),
        }
        assert_eq!(receptor.conta("chapters"), 0, "rollback incompleto");
    }
}
