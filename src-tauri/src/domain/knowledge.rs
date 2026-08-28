use serde::{Deserialize, Serialize};

/// Donos que podem receber tag. A migration 10 reconstruiu a tabela para
/// incluir `timeline` e `planning`; a lista está aqui para o erro de dono
/// inválido chegar como `validation` e não como texto cru do `CHECK`.
pub const TAG_OWNER_TYPES: &[&str] = &[
    "universe",
    "story",
    "book",
    "chapter",
    "entity",
    "timeline",
    "planning",
];

pub fn is_known_owner_type(owner_type: &str) -> bool {
    TAG_OWNER_TYPES.contains(&owner_type)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTag {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    /// Quantos donos usam a tag. Só a listagem do universo calcula; nas outras
    /// consultas fica ausente do JSON, como no modelo TypeScript, onde o campo
    /// é opcional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTagAssignment {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub owner_type: String,
    pub owner_id: String,
}

/// Menção de uma entidade num capítulo, com o caminho até a história.
///
/// Os `sort_order` das três camadas vêm junto porque a ordenação da tela é a
/// ordem de leitura da obra, não a ordem em que a menção foi criada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionOccurrence {
    pub id: String,
    pub chapter_id: String,
    pub entity_id: String,
    pub created_at: String,
    pub chapter_title: String,
    pub book_name: String,
    pub story_name: String,
    pub story_id: String,
    pub book_id: String,
    pub chapter_sort_order: i64,
    pub book_sort_order: i64,
    pub story_sort_order: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_e_planning_sao_donos_validos_desde_a_migration_10() {
        assert!(is_known_owner_type("timeline"));
        assert!(is_known_owner_type("planning"));
        assert!(!is_known_owner_type("inventado"));
    }

    #[test]
    fn contagem_ausente_nao_aparece_no_json() {
        // O modelo TypeScript declara `assigned?`. Mandar `null` faria a tela
        // renderizar "0 usos" onde ela hoje não mostra nada.
        let value = serde_json::to_value(ContentTag {
            id: "t1".into(),
            universe_id: "u1".into(),
            name: "Reescrever".into(),
            color: "#7d3650".into(),
            created_at: "2026-01-01 00:00:00".into(),
            assigned: None,
        })
        .expect("serializar tag");
        assert!(value.get("assigned").is_none());
    }
}
