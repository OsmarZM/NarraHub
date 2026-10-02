//! **Conflitos para a tela** (etapa F): o que o escritor vê e escolhe, já interpretado.
//!
//! O frontend não lê `sync_divergences`, `sync_events` nem envelope nenhum. Ele recebe daqui:
//!
//! ```text
//! lista      chave, tipo, item afetado com título legível, universo, estado, ações possíveis
//! detalhe    a versão deste aparelho e a do outro, campo a campo; diff de texto para o conteúdo;
//!            "excluído" quando um dos lados é uma exclusão; as ações com rótulo
//! ```
//!
//! "Deste aparelho" e "do outro" são só apresentação. A ação que volta é portátil (`ficarComA`,
//! `restaurar`, …): dois aparelhos que escolhem a mesma versão escolhem a mesma coisa.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::application::resolucao_divergencia::{
    acoes_permitidas, ler_por_chave, payload_da_revisao, Divergencia,
};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::conflito::ConflictParticipant;
use crate::domain::sync::AggregateRef;
use crate::infrastructure::sqlite::sync_codec;

/// O filtro da lista. Campo vazio = sem filtro.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FiltroDeConflitos {
    pub universe_id: String,
    pub aggregate_type: String,
    pub kind: String,
    /// `aberto`, `resolvido` ou vazio (todos).
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumoDoConflito {
    pub conflict_key: String,
    pub kind: String,
    /// Frase curta do que aconteceu, para a lista.
    pub descricao: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    /// O que o agregado é, em português ("Capítulo", "Tag", …).
    pub rotulo_do_tipo: String,
    /// Título, nome ou identificação legível do item, quando existe.
    pub titulo: String,
    pub universe_id: String,
    /// `aberto` ou `resolvido`.
    pub status: String,
    pub detectado_em: String,
    pub resolvido_em: String,
    /// A decisão faz parte de uma ação maior do outro aparelho (grupo de mutação).
    pub acao_inteira: bool,
}

/// Um campo de uma versão, com rótulo.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Campo {
    pub campo: String,
    pub rotulo: String,
    pub valor: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LadoDoConflito {
    /// `a` ou `b`: a letra portátil deste lado.
    pub letra: String,
    pub deste_aparelho: bool,
    /// Este lado é a exclusão do item.
    pub excluido: bool,
    pub titulo: String,
    pub campos: Vec<Campo>,
    /// O conteúdo não está neste aparelho (ex.: semeado sem o log que o produziu).
    pub indisponivel: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diferenca {
    pub campo: String,
    pub rotulo: String,
    pub deste_aparelho: Value,
    pub do_outro: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinhaDeDiff {
    /// `igual`, `so_deste` ou `so_do_outro`.
    pub tipo: String,
    pub texto: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcaoDisponivel {
    /// O `tipo` da ação que o comando de resolver recebe.
    pub acao: String,
    pub rotulo: String,
    pub descricao: String,
    /// A ação pede um nome novo (renomear tag).
    pub pede_nome: bool,
    /// Quando a ação é sobre uma das duas tags: qual.
    pub tag_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetalheDoConflito {
    pub resumo: ResumoDoConflito,
    pub deste_aparelho: LadoDoConflito,
    pub do_outro: LadoDoConflito,
    pub diferencas: Vec<Diferenca>,
    /// Diff linha a linha do texto longo (conteúdo de capítulo), quando há.
    pub diff_de_texto: Vec<LinhaDeDiff>,
    /// Vazio quando o conflito se resolve sozinho, ou já foi resolvido.
    pub acoes: Vec<AcaoDisponivel>,
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// **A lista de conflitos**, os abertos primeiro, mais recentes antes.
pub fn listar(
    connection: &rusqlite::Connection,
    filtro: &FiltroDeConflitos,
) -> DatabaseCommandResult<Vec<ResumoDoConflito>> {
    let chaves: Vec<String> = {
        let mut consulta = connection
            .prepare(
                "SELECT conflict_key FROM sync_divergences
                  WHERE conflict_key <> ''
                  GROUP BY conflict_key
                  ORDER BY MIN(resolved_at <> '') , MAX(detected_at) DESC",
            )
            .map_err(erro)?;
        let linhas = consulta.query_map([], |row| row.get(0)).map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)?
    };
    let mut resumos = Vec::new();
    for chave in chaves {
        let Some(divergencia) = ler_por_chave(connection, &chave, false)? else {
            continue;
        };
        let resumo = resumir(connection, &divergencia)?;
        let passa = (filtro.universe_id.is_empty() || filtro.universe_id == resumo.universe_id)
            && (filtro.aggregate_type.is_empty() || filtro.aggregate_type == resumo.aggregate_type)
            && (filtro.kind.is_empty() || filtro.kind == resumo.kind)
            && (filtro.status.is_empty() || filtro.status == resumo.status);
        if passa {
            resumos.push(resumo);
        }
    }
    Ok(resumos)
}

/// Quantos conflitos abertos há — o número que a tela de sincronização mostra.
pub fn contar_abertos(connection: &rusqlite::Connection) -> DatabaseCommandResult<i64> {
    connection
        .query_row(
            "SELECT COUNT(DISTINCT conflict_key) FROM sync_divergences
              WHERE resolved_at = '' AND conflict_key <> ''",
            [],
            |row| row.get(0),
        )
        .map_err(erro)
}

/// **O detalhe de um conflito**: as duas versões, o que mudou e o que dá para fazer.
pub fn inspecionar(
    connection: &rusqlite::Connection,
    chave: &str,
) -> DatabaseCommandResult<DetalheDoConflito> {
    let divergencia = ler_por_chave(connection, chave, false)?.ok_or_else(|| {
        DatabaseCommandError::not_found("Este conflito não existe neste aparelho.".to_string())
    })?;
    let resumo = resumir(connection, &divergencia)?;
    let local = divergencia.local().clone();
    let remoto = divergencia.remoto().clone();
    let (deste, payload_deste) = lado(connection, &divergencia, &local, true)?;
    let (outro, payload_do_outro) = lado(connection, &divergencia, &remoto, false)?;

    let mut diferencas = Vec::new();
    let mut diff_de_texto = Vec::new();
    if let (Some(Value::Object(daqui)), Some(Value::Object(de_la))) =
        (&payload_deste, &payload_do_outro)
    {
        let mut campos: Vec<&String> = daqui.keys().chain(de_la.keys()).collect();
        campos.sort();
        campos.dedup();
        for campo in campos {
            if campo == "id" {
                continue;
            }
            let (v1, v2) = (
                daqui.get(campo).cloned().unwrap_or(Value::Null),
                de_la.get(campo).cloned().unwrap_or(Value::Null),
            );
            if v1 == v2 {
                continue;
            }
            if campo == "content" {
                if let (Value::String(t1), Value::String(t2)) = (&v1, &v2) {
                    diff_de_texto = diff_de_linhas(&linhas_de_texto(t1), &linhas_de_texto(t2));
                }
            }
            diferencas.push(Diferenca {
                campo: campo.clone(),
                rotulo: rotulo_do_campo(campo),
                deste_aparelho: v1,
                do_outro: v2,
            });
        }
    }

    let acoes = if resumo.status == "aberto" {
        rotular_acoes(connection, &divergencia)?
    } else {
        Vec::new()
    };
    Ok(DetalheDoConflito {
        resumo,
        deste_aparelho: deste,
        do_outro: outro,
        diferencas,
        diff_de_texto,
        acoes,
    })
}

fn resumir(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<ResumoDoConflito> {
    let (status, resolvido_em): (String, String) = connection
        .query_row(
            "SELECT CASE WHEN resolved_at = '' THEN 'aberto' ELSE 'resolvido' END, resolved_at
               FROM sync_divergences WHERE id = ?1",
            [&divergencia.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(erro)?;
    let titulo = titulo_do_agregado(connection, &divergencia.agregado, divergencia)?;
    Ok(ResumoDoConflito {
        conflict_key: divergencia.conflict_key.clone(),
        kind: divergencia.kind.clone(),
        descricao: descrever(divergencia),
        aggregate_type: divergencia.agregado.aggregate_type.clone(),
        aggregate_id: divergencia.agregado.aggregate_id.clone(),
        rotulo_do_tipo: rotulo_do_tipo(&divergencia.agregado.aggregate_type),
        titulo,
        universe_id: universo(connection, divergencia)?,
        status,
        detectado_em: divergencia.detected_at.clone(),
        resolvido_em,
        acao_inteira: !divergencia.mutation_id.is_empty(),
    })
}

fn descrever(divergencia: &Divergencia) -> String {
    match divergencia.kind.as_str() {
        "parent_deletion_blocked" => {
            "O outro aparelho excluiu este item, e aqui ele ainda tem conteúdo que dependia dele."
                .into()
        }
        "tag_name_conflict" => "Os dois aparelhos criaram uma tag com o mesmo nome.".into(),
        _ if divergencia.agregado.aggregate_type == sync_codec::resolucao::TIPO => {
            "Os dois aparelhos resolveram o mesmo conflito de formas diferentes.".into()
        }
        _ => match (
            divergencia.local_operation.as_str(),
            divergencia.remote_operation.as_str(),
        ) {
            ("upsert", "delete") => "O item foi editado aqui e excluído no outro aparelho.".into(),
            ("delete", "upsert") => "O item foi excluído aqui e editado no outro aparelho.".into(),
            ("delete", "delete") => "Os dois aparelhos excluíram o item.".into(),
            _ => "Os dois aparelhos editaram o mesmo item.".into(),
        },
    }
}

fn universo(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<String> {
    let do_evento: Option<String> = rusqlite::OptionalExtension::optional(connection.query_row(
        "SELECT universe_id FROM sync_events WHERE event_id = ?1",
        [&divergencia.remote_event_id],
        |row| row.get(0),
    ))
    .map_err(erro)?;
    if let Some(universo) = do_evento.filter(|u| !u.is_empty()) {
        return Ok(universo);
    }
    Ok(sync_codec::ler_canonico(connection, &divergencia.agregado)?
        .map(|e| e.universe_id)
        .unwrap_or_default())
}

/// O payload de um lado, e o lado já montado.
fn lado(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
    participante: &ConflictParticipant,
    deste_aparelho: bool,
) -> DatabaseCommandResult<(LadoDoConflito, Option<Value>)> {
    let letra = if participante == &divergencia.a {
        "a"
    } else {
        "b"
    };
    let agregado = AggregateRef::new(&participante.aggregate_type, &participante.aggregate_id);
    let mut excluido = participante.operation == "delete";
    let mut payload = if excluido {
        None
    } else {
        payload_da_revisao(connection, &agregado, &participante.revision)?
            .and_then(|texto| serde_json::from_str::<Value>(&texto).ok())
    };
    // I-BUG-07: um conflito entre DECISÕES mostrava o certificado cru (`choice`, `results`, revisões).
    // O que o escritor precisa ver é o que cada decisão produz: o item afetado naquela revisão.
    if agregado.aggregate_type == sync_codec::resolucao::TIPO {
        if let Some(efeito) = payload.as_ref().and_then(efeito_principal) {
            excluido = efeito.operacao == "delete";
            payload = if excluido {
                None
            } else {
                payload_da_revisao(connection, &efeito.agregado, &efeito.revisao)?
                    .and_then(|texto| serde_json::from_str::<Value>(&texto).ok())
            };
        }
    }
    let campos = match &payload {
        Some(Value::Object(mapa)) => mapa
            .iter()
            .filter(|(campo, _)| campo.as_str() != "id")
            .map(|(campo, valor)| Campo {
                campo: campo.clone(),
                rotulo: rotulo_do_campo(campo),
                valor: valor.clone(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let titulo = payload
        .as_ref()
        .and_then(titulo_do_payload)
        .unwrap_or_default();
    Ok((
        LadoDoConflito {
            letra: letra.into(),
            deste_aparelho,
            excluido,
            titulo,
            campos,
            indisponivel: !excluido && payload.is_none(),
        },
        payload,
    ))
}

/// O efeito de uma decisão que interessa a quem lê: o do próprio item, não o da posição dele.
struct EfeitoDaDecisao {
    agregado: AggregateRef,
    revisao: String,
    operacao: String,
}

fn efeito_principal(certificado: &Value) -> Option<EfeitoDaDecisao> {
    let efeitos = certificado.get("results")?.as_array()?;
    let ler = |efeito: &Value| -> Option<EfeitoDaDecisao> {
        Some(EfeitoDaDecisao {
            agregado: AggregateRef::new(
                efeito.get("aggregateType")?.as_str()?,
                efeito.get("aggregateId")?.as_str()?,
            ),
            revisao: efeito.get("resultRev")?.as_str()?.to_string(),
            operacao: efeito
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or("upsert")
                .to_string(),
        })
    };
    efeitos
        .iter()
        .filter_map(ler)
        .find(|e| tipo_do_item_da_posicao(&e.agregado.aggregate_type).is_none())
        .or_else(|| efeitos.iter().find_map(ler))
}

fn titulo_do_payload(payload: &Value) -> Option<String> {
    ["title", "name", "caption", "label", "text"]
        .iter()
        .find_map(|campo| payload.get(*campo).and_then(|v| v.as_str()))
        .filter(|texto| !texto.trim().is_empty())
        .map(|texto| texto.chars().take(120).collect())
}

fn titulo_do_agregado(
    connection: &rusqlite::Connection,
    agregado: &AggregateRef,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<String> {
    // A posição e a decisão são "de" outra coisa: o título é o do item a que se referem.
    if let Some(tipo_do_item) = tipo_do_item_da_posicao(&agregado.aggregate_type) {
        let item = AggregateRef::new(tipo_do_item, &agregado.aggregate_id);
        return titulo_do_agregado(connection, &item, divergencia);
    }
    if agregado.aggregate_type == sync_codec::resolucao::TIPO {
        for revisao in [&divergencia.remote_rev, &divergencia.local_rev] {
            let certificado = payload_da_revisao(connection, agregado, revisao)?
                .and_then(|texto| serde_json::from_str::<Value>(&texto).ok());
            if let Some(efeito) = certificado.as_ref().and_then(efeito_principal) {
                let titulo = titulo_do_agregado(connection, &efeito.agregado, divergencia)?;
                if !titulo.is_empty() {
                    return Ok(titulo);
                }
            }
        }
    }
    if let Some(estado) = sync_codec::ler_canonico(connection, agregado)? {
        if let Ok(valor) = serde_json::from_str::<Value>(&estado.payload) {
            if let Some(titulo) = titulo_do_payload(&valor) {
                return Ok(titulo);
            }
        }
    }
    for revisao in [&divergencia.remote_rev, &divergencia.local_rev] {
        if let Some(payload) = payload_da_revisao(connection, agregado, revisao)? {
            if let Ok(valor) = serde_json::from_str::<Value>(&payload) {
                if let Some(titulo) = titulo_do_payload(&valor) {
                    return Ok(titulo);
                }
            }
        }
    }
    Ok(String::new())
}

fn tipo_do_item_da_posicao(tipo: &str) -> Option<&'static str> {
    match tipo {
        "story_position" => Some("story"),
        "book_position" => Some("book"),
        "chapter_position" => Some("chapter"),
        "attachment_position" => Some("attachment"),
        "planning_item_position" => Some("planning_item"),
        "planning_field_position" => Some("planning_field_definition"),
        "canvas_node_position" => Some("canvas_node"),
        _ => None,
    }
}

pub fn rotulo_do_tipo(tipo: &str) -> String {
    match tipo {
        "universe" => "Universo",
        "story" => "História",
        "book" => "Livro",
        "chapter" => "Capítulo",
        "entity" => "Ficha",
        "relation" => "Relação",
        "timeline_event" => "Evento da linha do tempo",
        "canvas_entity_position" => "Posição da ficha no quadro",
        "planning_item" => "Card do planejamento",
        "planning_field_definition" => "Propriedade do planejamento",
        "attachment" => "Anexo",
        "tag_assignment" => "Marcação de tag",
        "content_tag" => "Tag",
        "canvas_node" => "Elemento do quadro",
        "canvas_node_position" => "Posição de um elemento do quadro",
        "canvas_edge" => "Ligação do quadro",
        "story_position" => "Ordem de uma história",
        "book_position" => "Ordem de um livro",
        "chapter_position" => "Ordem de um capítulo",
        "planning_field_position" => "Ordem de uma propriedade",
        "attachment_position" => "Ordem de um anexo",
        "planning_item_position" => "Posição de um card",
        "entity_template_set" => "Modelo de ficha",
        "conflict_resolution" => "Decisão de conflito",
        outro => outro,
    }
    .to_string()
}

fn rotulo_do_campo(campo: &str) -> String {
    match campo {
        "title" => "Título",
        "name" => "Nome",
        "content" => "Texto",
        "summary" => "Resumo",
        "description" => "Descrição",
        "color" => "Cor",
        "status" => "Estado",
        "canonStatus" => "Canonicidade",
        "sceneOrigin" => "Origem da cena",
        "sceneDestination" => "Destino da cena",
        "customFields" => "Campos personalizados",
        "sortOrder" => "Posição",
        "caption" => "Legenda",
        "text" => "Texto",
        "attributes" => "Atributos",
        "choice" => "Escolha",
        "results" => "Efeitos",
        outro => outro,
    }
    .to_string()
}

/// Um documento HTML simples como linhas de texto, para o diff.
fn linhas_de_texto(html: &str) -> Vec<String> {
    let mut texto = String::with_capacity(html.len());
    let mut dentro_de_tag = false;
    let mut tag = String::new();
    for c in html.chars() {
        match c {
            '<' => {
                dentro_de_tag = true;
                tag.clear();
            }
            '>' if dentro_de_tag => {
                dentro_de_tag = false;
                let nome = tag
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if matches!(
                    nome,
                    "p" | "br" | "br/" | "h1" | "h2" | "h3" | "li" | "blockquote" | "div"
                ) && !texto.ends_with('\n')
                {
                    texto.push('\n');
                }
            }
            _ if dentro_de_tag => tag.push(c),
            _ => texto.push(c),
        }
    }
    texto
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .lines()
        .map(|linha| linha.trim().to_string())
        .filter(|linha| !linha.is_empty())
        .collect()
}

/// Diff de linhas por subsequência comum mais longa.
fn diff_de_linhas(deste: &[String], do_outro: &[String]) -> Vec<LinhaDeDiff> {
    let (n, m) = (deste.len(), do_outro.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if deste[i] == do_outro[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut saida = Vec::new();
    let linha = |tipo: &str, texto: &str| LinhaDeDiff {
        tipo: tipo.into(),
        texto: texto.into(),
    };
    while i < n && j < m {
        if deste[i] == do_outro[j] {
            saida.push(linha("igual", &deste[i]));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            saida.push(linha("so_deste", &deste[i]));
            i += 1;
        } else {
            saida.push(linha("so_do_outro", &do_outro[j]));
            j += 1;
        }
    }
    saida.extend(deste[i..].iter().map(|t| linha("so_deste", t)));
    saida.extend(do_outro[j..].iter().map(|t| linha("so_do_outro", t)));
    saida
}

fn rotular_acoes(
    connection: &rusqlite::Connection,
    divergencia: &Divergencia,
) -> DatabaseCommandResult<Vec<AcaoDisponivel>> {
    let a_e_deste = divergencia.local() == &divergencia.a;
    let acao = |acao: &str, rotulo: &str, descricao: &str| AcaoDisponivel {
        acao: acao.into(),
        rotulo: rotulo.into(),
        descricao: descricao.into(),
        pede_nome: false,
        tag_id: String::new(),
    };
    let mut rotuladas = Vec::new();
    for permitida in acoes_permitidas(connection, divergencia)? {
        match permitida {
            "ficarComA" | "ficarComB" => {
                let deste = (permitida == "ficarComA") == a_e_deste;
                rotuladas.push(if deste {
                    acao(
                        permitida,
                        "Ficar com a versão deste aparelho",
                        "A versão do outro aparelho é descartada nos dois.",
                    )
                } else {
                    acao(
                        permitida,
                        "Ficar com a versão do outro aparelho",
                        "A versão deste aparelho é substituída nos dois.",
                    )
                });
            }
            "restaurar" => rotuladas.push(acao(
                permitida,
                "Restaurar o item",
                "O conteúdo editado volta nos dois aparelhos.",
            )),
            "manterExclusao" => rotuladas.push(acao(
                permitida,
                "Manter excluído",
                "O item continua excluído nos dois aparelhos, e a edição é descartada.",
            )),
            "manterLocal" => rotuladas.push(acao(
                permitida,
                "Manter o item",
                "O item e o que depende dele ficam, nos dois aparelhos.",
            )),
            "aceitarExclusao" => rotuladas.push(acao(
                permitida,
                "Aceitar a exclusão",
                "O item é excluído aqui também — se nada mais depender dele.",
            )),
            "mesclar" => rotuladas.push(acao(
                permitida,
                "É a mesma tag",
                "As duas viram uma só, e as marcações das duas passam para ela.",
            )),
            "renomear" => {
                for participante in [divergencia.local(), divergencia.remoto()] {
                    let deste = participante == divergencia.local();
                    rotuladas.push(AcaoDisponivel {
                        acao: permitida.into(),
                        rotulo: if deste {
                            "Renomear a tag deste aparelho".into()
                        } else {
                            "Renomear a tag do outro aparelho".into()
                        },
                        descricao: "As duas continuam existindo, com nomes diferentes.".into(),
                        pede_nome: true,
                        tag_id: participante.aggregate_id.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(rotuladas)
}

/// O aviso da migração da época (E0-beta), para a tela de sincronização.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvisoDeEpoca {
    /// A atualização girou a época: os pareamentos anteriores deixaram de valer.
    pub pareamentos_invalidados: bool,
    /// Divergências da versão beta que estavam abertas e foram arquivadas (histórico, não conflito).
    pub divergencias_arquivadas: i64,
    pub iniciada_em: String,
}

pub fn aviso_de_epoca(connection: &rusqlite::Connection) -> DatabaseCommandResult<AvisoDeEpoca> {
    let epoca: Option<(String, String)> =
        rusqlite::OptionalExtension::optional(connection.query_row(
            "SELECT origem, iniciada_em FROM sync_epoca ORDER BY protocolo DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ))
        .map_err(erro)?;
    let (origem, iniciada_em) = epoca.unwrap_or_default();
    let divergencias_arquivadas: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sync_legado
              WHERE tabela = 'sync_divergences'
                AND COALESCE(json_extract(linha, '$.resolved_at'), '') = ''",
            [],
            |row| row.get(0),
        )
        .map_err(erro)?;
    Ok(AvisoDeEpoca {
        pareamentos_invalidados: origem == "rotacao",
        divergencias_arquivadas,
        iniciada_em,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_diff_de_texto_marca_o_que_so_existe_de_cada_lado() {
        let deste = linhas_de_texto("<p>um</p><p>dois</p><p>três</p>");
        let outro = linhas_de_texto("<p>um</p><p>DOIS</p><p>três</p>");
        let diff = diff_de_linhas(&deste, &outro);
        let tipos: Vec<(&str, &str)> = diff
            .iter()
            .map(|l| (l.tipo.as_str(), l.texto.as_str()))
            .collect();
        assert_eq!(
            tipos,
            vec![
                ("igual", "um"),
                ("so_deste", "dois"),
                ("so_do_outro", "DOIS"),
                ("igual", "três")
            ]
        );
    }
}
