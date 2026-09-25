//! **O enumerador canônico da adoção** (NH-079 etapa C).
//!
//! A gênese precisa responder duas perguntas, e nenhuma delas pode ser respondida por uma lista
//! escrita à mão em outro lugar:
//!
//! ```text
//! quais agregados cobertos existem no domínio?     → o enumerador de cada tipo
//! em que ordem eles podem ser emitidos?            → a precedência das fases
//! ```
//!
//! ## Por que a ordem importa mais do que parece
//!
//! O apply remoto já sabe esperar: um evento cuja dependência não chegou vira
//! `PrecisaReconciliar` e fica pendente. Mas o cursor de uma origem é **contíguo** — ele só anda
//! sobre o que foi aplicado. Se o evento `seq = 10` depende do `seq = 30` da mesma origem, o
//! cursor para no 9 **para sempre**: o 30 nunca é alcançado, porque alcançá-lo exigiria passar
//! pelo 10. Confiar na espera seria confiar num destravamento que não existe.
//!
//! Por isso a emissão é topologicamente ordenada pelas dependências **reais** dos codecs, e há um
//! gate que prova que nenhum evento da gênese aponta para um `seq` posterior.
//!
//! ## O ciclo que existe de verdade
//!
//! Um card pode ter valor num campo **exclusivo dele** (`scope = card`, `owner_item_id = card`):
//!
//! ```text
//! planning_item C  → precisa do campo F (campo_utilizavel)
//! planning_field F → precisa do card C (alvo_no_universo)
//! ```
//!
//! Não é ordenável entre tipos, nem entre instâncias. A saída é a mesma que o caminho incremental
//! produz naturalmente, e que a etapa B4 já testa: o card nasce **sem** os valores dos campos que
//! ele mesmo possui, o campo exclusivo nasce em seguida, e o card recebe uma segunda revisão que o
//! completa. Três eventos, causalmente encadeados, nenhum deles apontando para frente.
//!
//! Nada disso é exceção da gênese: é a mesma sequência que aconteceria se o escritor tivesse
//! criado tudo hoje, pela `Mutacao`.

use rusqlite::Connection;

use super::{planejamento, EstadoDoAgregado};
use crate::database::error::DatabaseCommandResult;

/// Se a fase cria o agregado ou completa uma criação que saiu reduzida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Primeira revisão, `base_rev` = raiz.
    Criar,
    /// Segunda revisão do mesmo agregado, sobre a primeira. Só existe para desfazer um ciclo.
    Completar,
}

/// Uma fase da adoção: um tipo, um recorte dele, e o modo de emissão.
pub struct Fase {
    pub tipo: &'static str,
    /// O recorte, quando o tipo entra em mais de uma fase. Vazio quando é o tipo inteiro.
    pub recorte: &'static str,
    pub modo: Modo,
    /// Os ids deste recorte que existem no domínio, em ordem determinística.
    pub enumerar: fn(&Connection) -> DatabaseCommandResult<Vec<String>>,
}

/// **A ordem da adoção.** Cada fase só depende do que veio antes dela.
///
/// ```text
/// universo → história → livro → capítulo            a espinha do manuscrito
/// posição de cada um, logo depois do item           (a posição não existe sem o item)
/// entidade → posição no grafo → relação → evento    o elenco
/// tag → marcação                                    a marcação precisa da tag e do dono
/// modelos de ficha                                  depende só do universo
/// nó → posição do nó → aresta                       o canvas
/// campos universais → card → posição do card        o planejamento
/// campos exclusivos → posição deles → card completo  o ciclo, desfeito em três eventos
/// anexo → posição do anexo                          o dono já existe (capítulo, entidade, nó)
/// ```
pub const FASES: &[Fase] = &[
    fase("universe", universos),
    fase("story", historias),
    fase("story_position", historias),
    fase("book", livros),
    fase("book_position", livros),
    fase("chapter", capitulos),
    fase("chapter_position", capitulos),
    fase("entity", entidades),
    fase("canvas_entity_position", posicoes_de_entidade),
    fase("relation", relacoes),
    fase("timeline_event", eventos_da_linha),
    fase("content_tag", tags),
    fase("tag_assignment", marcacoes),
    fase("entity_template_set", conjuntos_de_modelos),
    fase("canvas_node", nos),
    fase("canvas_node_position", nos),
    fase("canvas_edge", arestas),
    Fase {
        tipo: "planning_field_definition",
        recorte: "universais",
        modo: Modo::Criar,
        enumerar: campos_universais,
    },
    fase("planning_item", cards),
    fase("planning_item_position", cards),
    Fase {
        tipo: "planning_field_definition",
        recorte: "exclusivos do card",
        modo: Modo::Criar,
        enumerar: campos_exclusivos,
    },
    fase("planning_field_position", campos),
    Fase {
        tipo: "planning_item",
        recorte: "cards que citam campo próprio",
        modo: Modo::Completar,
        enumerar: planejamento::cards_com_campo_proprio_citado,
    },
    fase("attachment", anexos),
    fase("attachment_position", anexos),
    // Etapa F. Uma resolução só nasce pela `Mutacao`, então um acervo antigo não tem nenhuma —
    // a fase existe para a regra "todo tipo coberto tem fase" continuar sem exceção.
    fase("conflict_resolution", super::resolucao::enumerar),
];

const fn fase(
    tipo: &'static str,
    enumerar: fn(&Connection) -> DatabaseCommandResult<Vec<String>>,
) -> Fase {
    Fase {
        tipo,
        recorte: "",
        modo: Modo::Criar,
        enumerar,
    }
}

/// O payload que a fase emite para este agregado.
///
/// `Criar` de um card que cita campo próprio sai **reduzido**; `Completar` traz o estado inteiro.
/// Todo o resto é o estado canônico, sem recorte nenhum.
pub fn payload_da_fase(
    connection: &Connection,
    fase: &Fase,
    id: &str,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let agregado = crate::domain::sync::AggregateRef::new(fase.tipo, id);
    let Some(estado) = super::ler_canonico(connection, &agregado)? else {
        return Ok(None);
    };
    if fase.modo == Modo::Criar && fase.tipo == "planning_item" {
        return Ok(Some(EstadoDoAgregado {
            payload: planejamento::payload_sem_os_campos_do_proprio_card(
                connection,
                id,
                &estado.payload,
            )?,
            ..estado
        }));
    }
    Ok(Some(estado))
}

// ── Enumeradores ─────────────────────────────────────────────────────────

/// Os ids de cada tabela cuja identidade de agregado é o `id` da linha.
///
/// Uma função por tabela, e não um nome de tabela interpolado: o SQL da adoção é escrito aqui,
/// não montado a partir de texto que outro lugar escolhe.
macro_rules! enumerador {
    ($nome:ident, $sql:literal) => {
        fn $nome(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
            coluna(connection, $sql)
        }
    };
}

enumerador!(universos, "SELECT id FROM universes ORDER BY id");
enumerador!(historias, "SELECT id FROM stories ORDER BY id");
enumerador!(livros, "SELECT id FROM books ORDER BY id");
enumerador!(capitulos, "SELECT id FROM chapters ORDER BY id");
enumerador!(entidades, "SELECT id FROM entities ORDER BY id");
enumerador!(relacoes, "SELECT id FROM relations ORDER BY id");
enumerador!(
    eventos_da_linha,
    "SELECT id FROM timeline_events ORDER BY id"
);
enumerador!(tags, "SELECT id FROM content_tags ORDER BY id");
enumerador!(nos, "SELECT id FROM canvas_nodes ORDER BY id");
enumerador!(arestas, "SELECT id FROM canvas_edges ORDER BY id");
enumerador!(cards, "SELECT id FROM planning_items ORDER BY id");
enumerador!(
    campos,
    "SELECT id FROM planning_field_definitions ORDER BY id"
);
enumerador!(anexos, "SELECT id FROM attachments ORDER BY id");

fn coluna(connection: &Connection, sql: &str) -> DatabaseCommandResult<Vec<String>> {
    let mut consulta = connection.prepare(sql).map_err(super::erro)?;
    let linhas = consulta
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(super::erro)?;
    linhas.collect::<Result<_, _>>().map_err(super::erro)
}

fn posicoes_de_entidade(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    coluna(
        connection,
        "SELECT entity_id FROM canvas_entity_positions ORDER BY entity_id",
    )
}

/// A identidade da marcação é `tagId:ownerType:ownerId` — a mesma que o codec decompõe.
fn marcacoes(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    coluna(
        connection,
        "SELECT tag_id || ':' || owner_type || ':' || owner_id
           FROM content_tag_assignments ORDER BY tag_id, owner_type, owner_id",
    )
}

fn conjuntos_de_modelos(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    super::modelos::conjuntos(connection)
}

fn campos_universais(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    coluna(
        connection,
        "SELECT id FROM planning_field_definitions
          WHERE owner_item_id IS NULL OR owner_item_id = '' ORDER BY id",
    )
}

fn campos_exclusivos(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    coluna(
        connection,
        "SELECT id FROM planning_field_definitions
          WHERE owner_item_id IS NOT NULL AND owner_item_id <> '' ORDER BY id",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Todo tipo coberto tem enumerador e precedência.**
    ///
    /// Sem este gate, um agregado novo entraria no Sync V2 pela `Mutacao` e ficaria **fora da
    /// adoção**: acervo antigo nasceria sem ele, e ninguém perceberia até o pareamento.
    #[test]
    fn todo_tipo_coberto_tem_fase_de_adocao() {
        let sem_fase: Vec<&str> = super::super::TIPOS_COBERTOS
            .iter()
            .copied()
            .filter(|tipo| {
                !FASES
                    .iter()
                    .any(|fase| fase.tipo == *tipo && fase.modo == Modo::Criar)
            })
            .collect();
        assert!(
            sem_fase.is_empty(),
            "estes tipos são cobertos e não têm fase de criação na adoção: {sem_fase:?}"
        );
    }

    /// A precedência é uma ordem, não um conjunto: cada fase aparece uma vez, e `Completar` só
    /// existe depois da `Criar` do mesmo tipo.
    #[test]
    fn a_precedencia_e_uma_ordem_sem_repeticao() {
        let mut vistas: Vec<(&str, &str)> = Vec::new();
        for fase in FASES {
            let chave = (fase.tipo, fase.recorte);
            assert!(
                !vistas.contains(&chave),
                "fase repetida: {} {}",
                fase.tipo,
                fase.recorte
            );
            vistas.push(chave);
            if fase.modo == Modo::Completar {
                assert!(
                    FASES
                        .iter()
                        .take_while(|anterior| !std::ptr::eq(*anterior, fase))
                        .any(|anterior| anterior.tipo == fase.tipo && anterior.modo == Modo::Criar),
                    "{} completa sem ter sido criado antes",
                    fase.tipo
                );
            }
        }
    }

    /// Todo tipo coberto é enumerável num banco vazio sem erro — o enumerador existe de verdade,
    /// e não é um `unreachable!()` esperando o primeiro acervo real.
    #[test]
    fn todo_enumerador_responde_em_banco_vazio() {
        let connection = crate::infrastructure::sqlite::test_support::migrated_memory_database();
        for fase in FASES {
            let ids = (fase.enumerar)(&connection)
                .unwrap_or_else(|erro| panic!("{} {}: {erro:?}", fase.tipo, fase.recorte));
            assert!(ids.is_empty(), "{} devolveu ids em banco vazio", fase.tipo);
        }
    }
}
