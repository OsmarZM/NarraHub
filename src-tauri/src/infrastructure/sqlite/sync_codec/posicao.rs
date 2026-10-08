//! **Posição por item** (B2.2): a unidade de conflito da ordem é o item, não a lista.
//!
//! ## O defeito que isto corrige
//!
//! Até a B2.2 a ordem era um agregado de **lista inteira** — `chapter_order(livro)` com todos os
//! capítulos dentro. Isso junta duas coisas no mesmo histórico:
//!
//! ```text
//! pertencimento   quais capítulos estão no livro
//! posição         em que ordem
//! ```
//!
//! Com as duas no mesmo histórico, dois aparelhos que só **criam** um capítulo cada, sem reordenar
//! nada, produzem duas revisões da mesma lista a partir da mesma base: `Concurrent`. Uma decisão
//! para o escritor sobre um conflito que não existe. Os testes da B6 mostraram isso em todas as
//! ordens — história, livro, capítulo, propriedade e galeria.
//!
//! ## O modelo
//!
//! ```text
//! story_position(storyId)             { storyId, universeId, sortOrder }
//! book_position(bookId)               { bookId, storyId, sortOrder }
//! chapter_position(chapterId)         { chapterId, bookId, sortOrder }
//! planning_field_position(fieldId)    { fieldId, universeId, sortOrder }
//! attachment_position(attachmentId)   { attachmentId, ownerType, ownerId, sortOrder }
//! planning_item_position(itemId)      { itemId, universeId, status, sortOrder }
//! ```
//!
//! O pertencimento continua onde sempre esteve causalmente: no create/delete do próprio item. A
//! posição é um número do item, e só conflita quando **o mesmo item** é movido nos dois lados.
//!
//! ```text
//! criar   item + posição(item)               MAX(sort_order) + 1 — empate entre aparelhos é válido
//! excluir posição(item) + item               os irmãos NÃO ganham revisão; nada é compactado
//! mover   só as posições que mudaram         o conflito acontece na unidade certa
//! listar  ORDER BY sort_order, id            empate resolvido pelo id, igual em todo aparelho
//! ```
//!
//! ## Conteúdo e posição continuam separados
//!
//! O payload do capítulo não tem `sortOrder`; o do card não tem `status` nem `sortOrder`; o do
//! anexo não tem `sortOrder`. Editar o texto e mover o item são revisões de agregados diferentes.
//!
//! ## A posição é autoral, e não tem linha própria
//!
//! Existe uma ação independente do escritor que muda cada uma delas (arrastar, reordenar, trocar a
//! etapa do card) — e, onde essa ação ainda não existe na tela (história, livro, propriedade, anexo),
//! o `sortOrder` escolhido na criação é autoral do mesmo jeito. Por isso a posição conflita inclusive
//! contra a exclusão do item.
//!
//! "Sem linha própria" ([`super::existencia_derivada`]) é outra coisa: diz só que o número mora na
//! linha do item. Serve para a exclusão da posição, que chega **antes** da exclusão do item, ser
//! materializada quando o item sair. Não autoriza descartar revisão nenhuma.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{erro, para_json, EstadoDoAgregado};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::sync::{EventEnvelope, Operation};

/// Como cada tipo de posição fica no banco.
#[derive(Debug, Clone, Copy)]
pub struct TipoDePosicao {
    pub tipo: &'static str,
    /// O agregado cujo número de posição isto é.
    pub item: &'static str,
    pub tabela: &'static str,
    /// A identidade parental do item. Imutável: mover para outro pai não existe no app.
    pub colunas_do_pai: &'static [&'static str],
    /// Só o card: a etapa do quadro faz parte do lugar dele.
    pub com_status: bool,
    pub sql_do_universo: &'static str,
}

pub const POSICOES: &[TipoDePosicao] = &[
    TipoDePosicao {
        tipo: "story_position",
        item: "story",
        tabela: "stories",
        colunas_do_pai: &["universe_id"],
        com_status: false,
        sql_do_universo: "SELECT universe_id FROM stories WHERE id = ?1",
    },
    TipoDePosicao {
        tipo: "book_position",
        item: "book",
        tabela: "books",
        colunas_do_pai: &["story_id"],
        com_status: false,
        sql_do_universo: "SELECT s.universe_id FROM books b JOIN stories s ON s.id = b.story_id \
                          WHERE b.id = ?1",
    },
    TipoDePosicao {
        tipo: "chapter_position",
        item: "chapter",
        tabela: "chapters",
        colunas_do_pai: &["book_id"],
        com_status: false,
        sql_do_universo: "SELECT s.universe_id FROM chapters c JOIN books b ON b.id = c.book_id \
                          JOIN stories s ON s.id = b.story_id WHERE c.id = ?1",
    },
    TipoDePosicao {
        tipo: "planning_field_position",
        item: "planning_field_definition",
        tabela: "planning_field_definitions",
        colunas_do_pai: &["universe_id"],
        com_status: false,
        sql_do_universo: "SELECT universe_id FROM planning_field_definitions WHERE id = ?1",
    },
    TipoDePosicao {
        tipo: "attachment_position",
        item: "attachment",
        tabela: "attachments",
        colunas_do_pai: &["owner_type", "owner_id"],
        com_status: false,
        sql_do_universo: "SELECT universe_id FROM attachments WHERE id = ?1",
    },
    TipoDePosicao {
        tipo: "planning_item_position",
        item: "planning_item",
        tabela: "planning_items",
        colunas_do_pai: &["universe_id"],
        com_status: true,
        sql_do_universo: "SELECT universe_id FROM planning_items WHERE id = ?1",
    },
];

pub fn tipo_de_posicao(tipo: &str) -> Option<&'static TipoDePosicao> {
    POSICOES.iter().find(|posicao| posicao.tipo == tipo)
}

/// O tipo de posição de um item, para quem cria ou exclui o item.
pub fn posicao_do_item(item: &str) -> Option<&'static str> {
    POSICOES
        .iter()
        .find(|posicao| posicao.item == item)
        .map(|posicao| posicao.tipo)
}

// ── Payloads canônicos ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoDaHistoria {
    pub story_id: String,
    pub universe_id: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoDoLivro {
    pub book_id: String,
    pub story_id: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoDoCapitulo {
    pub chapter_id: String,
    pub book_id: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoDaPropriedade {
    pub field_id: String,
    pub universe_id: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoDoAnexo {
    pub attachment_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosicaoDoCard {
    pub item_id: String,
    pub universe_id: String,
    pub status: String,
    pub sort_order: i64,
}

/// A forma comum às seis, para a leitura, a validação e a aplicação não se repetirem por tipo.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Posicao {
    id: String,
    pais: Vec<String>,
    status: Option<String>,
    sort_order: i64,
}

fn para_payload(tipo: &TipoDePosicao, posicao: Posicao) -> DatabaseCommandResult<String> {
    let Posicao {
        id,
        mut pais,
        status,
        sort_order,
    } = posicao;
    let primeiro = pais.first().cloned().unwrap_or_default();
    match tipo.tipo {
        "story_position" => para_json(&PosicaoDaHistoria {
            story_id: id,
            universe_id: primeiro,
            sort_order,
        }),
        "book_position" => para_json(&PosicaoDoLivro {
            book_id: id,
            story_id: primeiro,
            sort_order,
        }),
        "chapter_position" => para_json(&PosicaoDoCapitulo {
            chapter_id: id,
            book_id: primeiro,
            sort_order,
        }),
        "planning_field_position" => para_json(&PosicaoDaPropriedade {
            field_id: id,
            universe_id: primeiro,
            sort_order,
        }),
        "attachment_position" => {
            let owner_id = pais.pop().unwrap_or_default();
            para_json(&PosicaoDoAnexo {
                attachment_id: id,
                owner_type: primeiro,
                owner_id,
                sort_order,
            })
        }
        _ => para_json(&PosicaoDoCard {
            item_id: id,
            universe_id: primeiro,
            status: status.unwrap_or_default(),
            sort_order,
        }),
    }
}

fn ilegivel(tipo: &str, error: serde_json::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(format!(
        "A posição {tipo} não está no formato canônico: {error}"
    ))
}

fn do_payload(tipo: &TipoDePosicao, payload: &str) -> DatabaseCommandResult<Posicao> {
    let nome = tipo.tipo;
    Ok(match nome {
        "story_position" => {
            let p: PosicaoDaHistoria =
                serde_json::from_str(payload).map_err(|e| ilegivel(nome, e))?;
            Posicao {
                id: p.story_id,
                pais: vec![p.universe_id],
                status: None,
                sort_order: p.sort_order,
            }
        }
        "book_position" => {
            let p: PosicaoDoLivro = serde_json::from_str(payload).map_err(|e| ilegivel(nome, e))?;
            Posicao {
                id: p.book_id,
                pais: vec![p.story_id],
                status: None,
                sort_order: p.sort_order,
            }
        }
        "chapter_position" => {
            let p: PosicaoDoCapitulo =
                serde_json::from_str(payload).map_err(|e| ilegivel(nome, e))?;
            Posicao {
                id: p.chapter_id,
                pais: vec![p.book_id],
                status: None,
                sort_order: p.sort_order,
            }
        }
        "planning_field_position" => {
            let p: PosicaoDaPropriedade =
                serde_json::from_str(payload).map_err(|e| ilegivel(nome, e))?;
            Posicao {
                id: p.field_id,
                pais: vec![p.universe_id],
                status: None,
                sort_order: p.sort_order,
            }
        }
        "attachment_position" => {
            let p: PosicaoDoAnexo = serde_json::from_str(payload).map_err(|e| ilegivel(nome, e))?;
            Posicao {
                id: p.attachment_id,
                pais: vec![p.owner_type, p.owner_id],
                status: None,
                sort_order: p.sort_order,
            }
        }
        _ => {
            let p: PosicaoDoCard = serde_json::from_str(payload).map_err(|e| ilegivel(nome, e))?;
            Posicao {
                id: p.item_id,
                pais: vec![p.universe_id],
                status: Some(p.status),
                sort_order: p.sort_order,
            }
        }
    })
}

/// Os pais e o status gravados na linha do item, se ela existir.
fn linha_do_item(
    connection: &Connection,
    tipo: &TipoDePosicao,
    id: &str,
) -> DatabaseCommandResult<Option<Posicao>> {
    let mut colunas: Vec<&str> = tipo.colunas_do_pai.to_vec();
    if tipo.com_status {
        colunas.push("status");
    }
    colunas.push("sort_order");
    let sql = format!(
        "SELECT {} FROM {} WHERE id = ?1",
        colunas.join(", "),
        tipo.tabela
    );
    let quantos_pais = tipo.colunas_do_pai.len();
    connection
        .query_row(&sql, [id], |row| {
            let mut pais = Vec::with_capacity(quantos_pais);
            for indice in 0..quantos_pais {
                pais.push(row.get::<_, String>(indice)?);
            }
            let (status, sort_order) = if tipo.com_status {
                (
                    Some(row.get::<_, String>(quantos_pais)?),
                    row.get::<_, i64>(quantos_pais + 1)?,
                )
            } else {
                (None, row.get::<_, i64>(quantos_pais)?)
            };
            Ok(Posicao {
                id: id.to_string(),
                pais,
                status,
                sort_order,
            })
        })
        .optional()
        .map_err(erro)
}

// ── Leitura canônica ─────────────────────────────────────────────────────

pub fn ler(
    connection: &Connection,
    tipo: &TipoDePosicao,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some(posicao) = linha_do_item(connection, tipo, id)? else {
        return Ok(None);
    };
    let universe_id: Option<String> = connection
        .query_row(tipo.sql_do_universo, [id], |row| row.get(0))
        .optional()
        .map_err(erro)?;
    let Some(universe_id) = universe_id else {
        return Err(DatabaseCommandError::storage(format!(
            "{} {id} existe e não está ligado a um universo. Estado incompatível.",
            tipo.item
        )));
    };
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload: para_payload(tipo, posicao)?,
    }))
}

// ── Validação (a mesma no local e no remoto) ─────────────────────────────

/// Item ainda não chegou → dependência. Pai diferente, etapa ou dono desconhecido → erro.
pub fn validar(
    connection: &Connection,
    tipo: &TipoDePosicao,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    let recebida = do_payload(tipo, payload)?;
    if let Some(status) = recebida.status.as_deref() {
        if !crate::domain::planning::is_known_status(status) {
            return Err(DatabaseCommandError::storage(format!(
                "A posição do card {} traz a etapa desconhecida '{status}'. Estado incompatível.",
                recebida.id
            )));
        }
    }
    if tipo.tipo == "attachment_position"
        && !crate::domain::canvas::is_known_attachment_owner(&recebida.pais[0])
    {
        return Err(DatabaseCommandError::storage(format!(
            "A posição do anexo {} traz o dono desconhecido '{}'. Estado incompatível.",
            recebida.id, recebida.pais[0]
        )));
    }
    let Some(aqui) = linha_do_item(connection, tipo, &recebida.id)? else {
        return Ok(Some(format!("{} {}", tipo.item, recebida.id)));
    };
    if aqui.pais != recebida.pais {
        return Err(DatabaseCommandError::storage(format!(
            "{} {} está em {:?} aqui, e a posição recebida diz {:?}. O pai é imutável: a posição \
             não é aplicada.",
            tipo.item, recebida.id, aqui.pais, recebida.pais
        )));
    }
    Ok(None)
}

pub fn dependencias(
    connection: &Connection,
    tipo: &TipoDePosicao,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    validar(connection, tipo, &envelope.payload)
}

// ── Aplicação ────────────────────────────────────────────────────────────

/// Grava o número de posição (e a etapa, no card) na linha do item.
///
/// A exclusão não escreve nada: a posição some com o item, que chega no evento seguinte da mesma
/// origem. E nenhum irmão é tocado — é isso que faz duas criações concorrentes não colidirem.
pub fn aplicar(
    tx: &Transaction<'_>,
    tipo: &TipoDePosicao,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<()> {
    if envelope.operation == Operation::Delete {
        return Ok(());
    }
    let posicao = do_payload(tipo, &envelope.payload)?;
    if posicao.id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(format!(
            "O payload descreve a posição de {}, e o envelope é de {}.",
            posicao.id, envelope.aggregate_id
        )));
    }
    let alterou = if tipo.com_status {
        tx.execute(
            &format!(
                "UPDATE {} SET status = ?1, sort_order = ?2 WHERE id = ?3",
                tipo.tabela
            ),
            rusqlite::params![posicao.status, posicao.sort_order, &posicao.id],
        )
    } else {
        tx.execute(
            &format!("UPDATE {} SET sort_order = ?1 WHERE id = ?2", tipo.tabela),
            rusqlite::params![posicao.sort_order, &posicao.id],
        )
    }
    .map_err(erro)?;
    if alterou != 1 {
        return Err(DatabaseCommandError::storage(format!(
            "{} {} sumiu no meio da aplicação da posição.",
            tipo.item, posicao.id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    /// **Vetores canônicos.** Estes bytes são o formato que a gênese (etapa C) vai reproduzir.
    /// Mudar um deles é mudar o protocolo, e precisa de decisão — não de ajuste de teste.
    #[test]
    fn vetores_canonicos_das_seis_posicoes() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name, sort_order, created_at, updated_at)
                   VALUES ('s1', 'u1', 'Saga', 3, '2026-01-01', '2026-01-01');
                 INSERT INTO books (id, story_id, name, sort_order, created_at, updated_at)
                   VALUES ('b1', 's1', 'Livro', 1, '2026-01-01', '2026-01-01');
                 INSERT INTO chapters (id, book_id, title, content, word_count, sort_order, created_at, updated_at)
                   VALUES ('c1', 'b1', 'Um', '', 0, 7, '2026-01-01', '2026-01-01');
                 INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01', '2026-01-01');
                 INSERT INTO attachments (id, universe_id, owner_type, owner_id, data_url, sort_order, created_at)
                   VALUES ('a1', 'u1', 'entity', 'e1', '', 2, '2026-01-01');
                 INSERT INTO planning_items (id, universe_id, title, status, sort_order, created_at, updated_at)
                   VALUES ('p1', 'u1', 'Cena', 'ESCREVENDO', 4, '2026-01-01', '2026-01-01');
                 INSERT INTO planning_field_definitions (id, universe_id, name, field_type, sort_order, created_at, updated_at)
                   VALUES ('f1', 'u1', 'Tom', 'text', 5, '2026-01-01', '2026-01-01');",
            )
            .expect("semear");

        let esperado = [
            (
                "story_position",
                "s1",
                r#"{"storyId":"s1","universeId":"u1","sortOrder":3}"#,
            ),
            (
                "book_position",
                "b1",
                r#"{"bookId":"b1","storyId":"s1","sortOrder":1}"#,
            ),
            (
                "chapter_position",
                "c1",
                r#"{"chapterId":"c1","bookId":"b1","sortOrder":7}"#,
            ),
            (
                "planning_field_position",
                "f1",
                r#"{"fieldId":"f1","universeId":"u1","sortOrder":5}"#,
            ),
            (
                "attachment_position",
                "a1",
                r#"{"attachmentId":"a1","ownerType":"entity","ownerId":"e1","sortOrder":2}"#,
            ),
            (
                "planning_item_position",
                "p1",
                r#"{"itemId":"p1","universeId":"u1","status":"ESCREVENDO","sortOrder":4}"#,
            ),
        ];
        for (nome, id, payload) in esperado {
            let tipo = tipo_de_posicao(nome).expect("tipo");
            let estado = ler(&connection, tipo, id).expect("ler").expect("existe");
            assert_eq!(estado.payload, payload, "{nome}");
            assert_eq!(estado.universe_id, "u1", "{nome}");
            // A volta: o payload canônico é aceito pela validação do próprio tipo.
            assert_eq!(validar(&connection, tipo, payload).expect("validar"), None);
        }
    }

    /// Pai trocado na posição recebida é erro, não mudança de pai.
    #[test]
    fn posicao_com_outro_pai_e_recusada() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name, sort_order, created_at, updated_at)
                   VALUES ('s1', 'u1', 'Saga', 0, '2026-01-01', '2026-01-01');
                 INSERT INTO books (id, story_id, name, sort_order, created_at, updated_at)
                   VALUES ('b1', 's1', 'Livro', 0, '2026-01-01', '2026-01-01');
                 INSERT INTO chapters (id, book_id, title, content, word_count, sort_order, created_at, updated_at)
                   VALUES ('c1', 'b1', 'Um', '', 0, 0, '2026-01-01', '2026-01-01');",
            )
            .expect("semear");
        let tipo = tipo_de_posicao("chapter_position").expect("tipo");
        let erro = validar(
            &connection,
            tipo,
            r#"{"chapterId":"c1","bookId":"outro-livro","sortOrder":0}"#,
        )
        .expect_err("pai imutável");
        assert!(erro.message.contains("imutável"), "{}", erro.message);
    }

    /// Item que ainda não chegou é dependência, e a posição espera por ele.
    #[test]
    fn posicao_de_item_ausente_e_dependencia() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        let tipo = tipo_de_posicao("chapter_position").expect("tipo");
        assert_eq!(
            validar(
                &connection,
                tipo,
                r#"{"chapterId":"c9","bookId":"b1","sortOrder":0}"#
            )
            .expect("validar"),
            Some("chapter c9".to_string())
        );
    }
}
