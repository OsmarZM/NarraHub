use serde::{Deserialize, Serialize};

/// Ordem das colunas no quadro. É a mesma lista que a migration 5 impõe por
/// `CHECK`, repetida aqui porque a ordenação da tela não é a ordem alfabética
/// nem a de inserção — é a do fluxo de trabalho.
pub const STATUS_ORDER: &[&str] = &["IDEIAS", "PLANEJADO", "ESCREVENDO", "REVISAO", "FINALIZADO"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningItem {
    pub id: String,
    pub universe_id: String,
    pub chapter_id: Option<String>,
    pub title: String,
    pub description: String,
    pub image: String,
    pub custom_field_values: String,
    pub status: String,
    pub target_words: i64,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Vêm de `LEFT JOIN`: um card sem capítulo vinculado não tem nenhum dos
    /// três, e a tela precisa distinguir isso de "capítulo sem nome".
    pub chapter_title: Option<String>,
    pub book_name: Option<String>,
    pub story_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningFieldDefinition {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub field_type: String,
    pub options_json: String,
    pub sort_order: i64,
    /// `universal` aparece em todos os cards do universo; `card`, só naquele
    /// que o criou. Ver `SCOPE_UNIVERSAL`/`SCOPE_CARD`.
    pub scope: String,
    /// O card dono, obrigatório em `card` e sempre `None` em `universal`.
    pub owner_item_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub const SCOPE_UNIVERSAL: &str = "universal";
pub const SCOPE_CARD: &str = "card";

pub fn is_known_field_scope(scope: &str) -> bool {
    scope == SCOPE_UNIVERSAL || scope == SCOPE_CARD
}

/// Uma posição do card no quadro: em que coluna e em que altura dela.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningCardPlacement {
    pub id: String,
    pub status: String,
    pub sort_order: i64,
}

pub fn is_known_status(status: &str) -> bool {
    STATUS_ORDER.contains(&status)
}

/// Índice da coluna, usado no `ORDER BY`. Status desconhecido vai para o fim
/// em vez de sumir — dado estranho no banco não pode esconder o card da tela.
pub fn status_rank(status: &str) -> usize {
    STATUS_ORDER
        .iter()
        .position(|known| *known == status)
        .unwrap_or(STATUS_ORDER.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ordem_das_colunas_e_a_do_fluxo_nao_a_alfabetica() {
        assert_eq!(status_rank("IDEIAS"), 0);
        assert_eq!(status_rank("FINALIZADO"), 4);
        assert!(status_rank("IDEIAS") < status_rank("ESCREVENDO"));
    }

    #[test]
    fn status_desconhecido_vai_para_o_fim_em_vez_de_sumir() {
        assert_eq!(status_rank("QUALQUER_COISA"), STATUS_ORDER.len());
        assert!(!is_known_status("QUALQUER_COISA"));
    }
}
