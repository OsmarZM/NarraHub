//! **Catálogo dos efeitos de exclusão do schema** — toda FK com ação e todo gatilho que escreve.
//!
//! É a tabela da seção 3 da matriz em forma de código. O gate [`tests`] lê o schema migrado e
//! reprova se aparecer FK ou gatilho que não esteja aqui, ou se uma entrada daqui sumir do schema.
//! Ninguém consegue acrescentar um `ON DELETE CASCADE` sem decidir o que ele significa para o sync.

/// O que o mecanismo faz com o agregado atingido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Efeito {
    /// O agregado atingido some.
    Delete,
    /// O agregado atingido sobrevive, com estado diferente.
    Rewrite,
    /// Estado interno do próprio agregado de origem (some com ele, sem identidade própria).
    Interno,
    /// Tabela fora do sync (histórico local, derivada, sessão efêmera).
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mecanismo {
    /// `FOREIGN KEY (tabela.coluna) REFERENCES origem ON DELETE <acao>`.
    Fk {
        tabela: &'static str,
        coluna: &'static str,
        acao: &'static str,
    },
    Gatilho(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct EfeitoDeExclusao {
    /// Tabela cuja linha é excluída.
    pub origem: &'static str,
    pub mecanismo: Mecanismo,
    /// Tabela escrita pelo mecanismo.
    pub tabela_afetada: &'static str,
    /// Agregado atingido ("—" quando `Local`).
    pub agregado_afetado: &'static str,
    pub efeito: Efeito,
    /// Etapa em que o agregado atingido passa a ser coberto.
    pub etapa: &'static str,
    /// O que acontece enquanto não é coberto.
    pub enquanto_nao_coberto: &'static str,
}

const UNIVERSO_RECUSADO: &str =
    "exclusão de universo recusada (delete_universe) até a árvore inteira ser coberta";

macro_rules! fk {
    ($origem:literal, $tabela:literal . $coluna:literal $acao:literal => $agregado:literal, $efeito:ident, $etapa:literal, $acao_hoje:expr) => {
        EfeitoDeExclusao {
            origem: $origem,
            mecanismo: Mecanismo::Fk {
                tabela: $tabela,
                coluna: $coluna,
                acao: $acao,
            },
            tabela_afetada: $tabela,
            agregado_afetado: $agregado,
            efeito: Efeito::$efeito,
            etapa: $etapa,
            enquanto_nao_coberto: $acao_hoje,
        }
    };
}

macro_rules! gatilho {
    ($origem:literal, $nome:literal, $tabela:literal => $agregado:literal, $efeito:ident, $etapa:literal, $acao_hoje:expr) => {
        EfeitoDeExclusao {
            origem: $origem,
            mecanismo: Mecanismo::Gatilho($nome),
            tabela_afetada: $tabela,
            agregado_afetado: $agregado,
            efeito: Efeito::$efeito,
            etapa: $etapa,
            enquanto_nao_coberto: $acao_hoje,
        }
    };
}

pub const EFEITOS: &[EfeitoDeExclusao] = &[
    // ── universe ──────────────────────────────────────────────────────────
    fk!("universes", "stories"."universe_id" "CASCADE" => "story", Delete, "B2", UNIVERSO_RECUSADO),
    fk!("universes", "attachments"."universe_id" "CASCADE" => "attachment", Delete, "B1", UNIVERSO_RECUSADO),
    fk!("universes", "content_custom_fields"."universe_id" "CASCADE" => "universe/story/book/chapter/entity", Interno, "B2 (manuscrito), B3 (entity)", UNIVERSO_RECUSADO),
    fk!("universes", "entities"."universe_id" "CASCADE" => "entity", Delete, "B3", UNIVERSO_RECUSADO),
    // `entity_templates` não tem escritor no app: é acervo legado lido por `create` de entidade.
    // Precisa de codec antes da gênese; entra no gate de cobertura total da B6.
    fk!("universes", "entity_templates"."universe_id" "CASCADE" => "entity_template", Delete, "B6", UNIVERSO_RECUSADO),
    fk!("universes", "relations"."universe_id" "CASCADE" => "relation", Delete, "B3", UNIVERSO_RECUSADO),
    fk!("universes", "timeline_events"."universe_id" "CASCADE" => "timeline_event", Delete, "B3", UNIVERSO_RECUSADO),
    fk!("universes", "canvas_entity_positions"."universe_id" "CASCADE" => "canvas_entity_position", Delete, "B3", UNIVERSO_RECUSADO),
    fk!("universes", "planning_items"."universe_id" "CASCADE" => "planning_item", Delete, "B4", UNIVERSO_RECUSADO),
    fk!("universes", "planning_field_definitions"."universe_id" "CASCADE" => "planning_field_definition", Delete, "B4", UNIVERSO_RECUSADO),
    fk!("universes", "content_tags"."universe_id" "CASCADE" => "content_tag", Delete, "B5", UNIVERSO_RECUSADO),
    fk!("universes", "canvas_nodes"."universe_id" "CASCADE" => "canvas_node", Delete, "B5", UNIVERSO_RECUSADO),
    fk!("universes", "canvas_edges"."universe_id" "CASCADE" => "canvas_edge", Delete, "B5", UNIVERSO_RECUSADO),
    // ── story ─────────────────────────────────────────────────────────────
    fk!("stories", "books"."story_id" "CASCADE" => "book", Delete, "B2", "—"),
    fk!("stories", "planning_field_links"."story_id" "CASCADE" => "planning_item", Rewrite, "B4", "—"),
    gatilho!("stories", "trg_story_metadata_delete", "content_tag_assignments" => "tag_assignment", Delete, "B2", "—"),
    gatilho!("stories", "trg_story_metadata_delete", "content_custom_fields" => "story", Interno, "B2", "—"),
    // ── book ──────────────────────────────────────────────────────────────
    fk!("books", "chapters"."book_id" "CASCADE" => "chapter", Delete, "B2", "—"),
    gatilho!("books", "trg_book_metadata_delete", "content_tag_assignments" => "tag_assignment", Delete, "B2", "—"),
    gatilho!("books", "trg_book_metadata_delete", "content_custom_fields" => "book", Interno, "B2", "—"),
    // ── chapter ───────────────────────────────────────────────────────────
    fk!("chapters", "planning_items"."chapter_id" "SET NULL" => "planning_item", Rewrite, "B4", "—"),
    fk!("chapters", "chapter_revisions"."chapter_id" "CASCADE" => "—", Local, "fora do sync", "—"),
    fk!("chapters", "mentions"."chapter_id" "CASCADE" => "—", Local, "fora do sync", "—"),
    gatilho!("chapters", "trg_chapter_attachments_delete", "attachments" => "attachment", Delete, "B1", "—"),
    gatilho!("chapters", "trg_chapter_metadata_delete", "content_tag_assignments" => "tag_assignment", Delete, "B2", "—"),
    gatilho!("chapters", "trg_chapter_metadata_delete", "content_custom_fields" => "chapter", Interno, "B2", "—"),
    // ── entity ────────────────────────────────────────────────────────────
    fk!("entities", "entity_attributes"."entity_id" "CASCADE" => "entity", Interno, "B3", "—"),
    fk!("entities", "relations"."source_id" "CASCADE" => "relation", Delete, "B3", "—"),
    fk!("entities", "relations"."target_id" "CASCADE" => "relation", Delete, "B3", "—"),
    fk!("entities", "timeline_events"."entity_id" "SET NULL" => "timeline_event", Rewrite, "B3", "—"),
    fk!("entities", "canvas_entity_positions"."entity_id" "CASCADE" => "canvas_entity_position", Delete, "B3", "—"),
    fk!("entities", "planning_field_links"."entity_id" "CASCADE" => "planning_item", Rewrite, "B4", "—"),
    fk!("entities", "mentions"."entity_id" "CASCADE" => "—", Local, "fora do sync", "—"),
    gatilho!("entities", "trg_entity_canvas_edges_delete", "canvas_edges" => "canvas_edge", Delete, "B5", "—"),
    gatilho!("entities", "trg_entity_attachments_delete", "attachments" => "attachment", Delete, "B3", "—"),
    gatilho!("entities", "trg_entity_metadata_delete", "content_tag_assignments" => "tag_assignment", Delete, "B3", "—"),
    gatilho!("entities", "trg_entity_metadata_delete", "content_custom_fields" => "entity", Interno, "B3", "—"),
    // ── timeline / planning ───────────────────────────────────────────────
    gatilho!("timeline_events", "trg_timeline_metadata_delete", "content_tag_assignments" => "tag_assignment", Delete, "B3", "—"),
    fk!("planning_items", "planning_field_links"."planning_item_id" "CASCADE" => "planning_item", Interno, "B4", "—"),
    fk!("planning_items", "planning_field_definitions"."owner_item_id" "CASCADE" => "planning_field_definition", Delete, "B4", "—"),
    gatilho!("planning_items", "trg_planning_metadata_delete", "content_tag_assignments" => "tag_assignment", Delete, "B4", "—"),
    fk!("planning_field_definitions", "planning_field_links"."field_definition_id" "CASCADE" => "planning_item", Rewrite, "B4", "—"),
    gatilho!("planning_field_definitions", "trg_planning_field_definition_delete", "planning_items" => "planning_item", Rewrite, "B4", "—"),
    // ── canvas ────────────────────────────────────────────────────────────
    gatilho!("canvas_nodes", "trg_canvas_node_edges_delete", "canvas_edges" => "canvas_edge", Delete, "B5", "—"),
    // ── tags ──────────────────────────────────────────────────────────────
    fk!("content_tags", "content_tag_assignments"."tag_id" "CASCADE" => "tag_assignment", Delete, "B5", "—"),
    fk!("content_tags", "planning_field_links"."tag_id" "CASCADE" => "planning_item", Rewrite, "B4", "—"),
    // ── colaboração ───────────────────────────────────────────────────────
    fk!("collaboration_sessions", "collaboration_contributions"."session_id" "CASCADE" => "—", Local, "fora do sync", "—"),
];

/// Gatilhos que escrevem sem ser exclusão: histórico local. Listados para o gate saber que foram
/// vistos.
pub const GATILHOS_LOCAIS_DE_ESCRITA: &[&str] = &[
    "trg_chapter_history_insert",
    "trg_chapter_history_update",
    "trg_chapter_revision",
    "trg_entity_history_insert",
    "trg_entity_history_update",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::collections::BTreeSet;

    /// **Todo efeito do schema está classificado, e toda classificação existe no schema.**
    #[test]
    fn toda_fk_com_acao_e_todo_gatilho_que_escreve_estao_no_catalogo() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();

        let tabelas: Vec<String> = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .expect("tabelas")
            .query_map([], |row| row.get(0))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("nomes");

        let mut fks_do_schema = BTreeSet::new();
        for tabela in &tabelas {
            let mut consulta = connection
                .prepare(&format!("PRAGMA foreign_key_list({tabela})"))
                .expect("fk");
            let linhas: Vec<(String, String, String)> = consulta
                .query_map([], |row| Ok((row.get(2)?, row.get(3)?, row.get(6)?)))
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("fks");
            for (origem, coluna, acao) in linhas {
                if matches!(acao.as_str(), "NO ACTION" | "RESTRICT") {
                    continue;
                }
                fks_do_schema.insert((origem, tabela.clone(), coluna, acao));
            }
        }
        let fks_do_catalogo: BTreeSet<(String, String, String, String)> = EFEITOS
            .iter()
            .filter_map(|efeito| match efeito.mecanismo {
                Mecanismo::Fk {
                    tabela,
                    coluna,
                    acao,
                } => Some((
                    efeito.origem.to_string(),
                    tabela.to_string(),
                    coluna.to_string(),
                    acao.to_string(),
                )),
                Mecanismo::Gatilho(_) => None,
            })
            .collect();
        assert_eq!(
            fks_do_schema
                .difference(&fks_do_catalogo)
                .collect::<Vec<_>>(),
            Vec::<&(String, String, String, String)>::new(),
            "FK com ação sem classificação no catálogo de efeitos de exclusão"
        );
        assert_eq!(
            fks_do_catalogo
                .difference(&fks_do_schema)
                .collect::<Vec<_>>(),
            Vec::<&(String, String, String, String)>::new(),
            "o catálogo cita FK que não existe mais"
        );

        let gatilhos: Vec<(String, String, String)> = connection
            .prepare("SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'trigger'")
            .expect("gatilhos")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("sql");
        let mut que_escrevem = BTreeSet::new();
        for (nome, tabela, sql) in gatilhos {
            // Texto entre aspas (mensagens de RAISE) não é instrução.
            let mut sem_literais = String::new();
            let mut dentro = false;
            for c in sql.chars() {
                if c == '\'' {
                    dentro = !dentro;
                } else if !dentro {
                    sem_literais.push(c);
                }
            }
            let corpo = sem_literais
                .to_uppercase()
                .split_once("BEGIN")
                .map(|(_, corpo)| corpo.to_string())
                .unwrap_or_default();
            let escreve = ["INSERT ", "UPDATE ", "DELETE "]
                .iter()
                .any(|verbo| corpo.contains(verbo));
            if escreve {
                que_escrevem.insert((nome, tabela));
            }
        }
        let classificados: BTreeSet<(String, String)> = EFEITOS
            .iter()
            .filter_map(|efeito| match efeito.mecanismo {
                Mecanismo::Gatilho(nome) => Some((nome.to_string(), efeito.origem.to_string())),
                Mecanismo::Fk { .. } => None,
            })
            .collect();
        for (nome, tabela) in &que_escrevem {
            let conhecido = classificados.contains(&(nome.clone(), tabela.clone()))
                || GATILHOS_LOCAIS_DE_ESCRITA.contains(&nome.as_str());
            assert!(
                conhecido,
                "gatilho {nome} em {tabela} escreve e não está classificado"
            );
        }
        for (nome, tabela) in &classificados {
            assert!(
                que_escrevem.contains(&(nome.clone(), tabela.clone())),
                "o catálogo cita o gatilho {nome} em {tabela}, que não existe ou não escreve"
            );
        }
    }

    /// Todo efeito sobre agregado ainda não coberto diz o que acontece enquanto isso.
    #[test]
    fn efeito_nao_coberto_tem_acao_declarada() {
        for efeito in EFEITOS {
            let coberto =
                crate::infrastructure::sqlite::sync_codec::coberto(efeito.agregado_afetado);
            if matches!(efeito.efeito, Efeito::Delete | Efeito::Rewrite) && !coberto {
                assert_ne!(
                    efeito.enquanto_nao_coberto, "—",
                    "{} → {}: agregado não coberto sem ação declarada",
                    efeito.origem, efeito.agregado_afetado
                );
            }
        }
    }
}
