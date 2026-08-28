use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: String,
    pub universe_id: String,
    pub title: String,
    pub description: String,
    pub event_type: String,
    pub start_date: String,
    pub end_date: String,
    pub entity_id: Option<String>,
    pub display_date: String,
    /// `REAL` no schema, não inteiro: a ordenação é fracionária de propósito,
    /// para inserir um evento entre dois outros sem renumerar a timeline toda.
    pub sort_key: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationCard {
    pub id: String,
    pub universe_id: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(rename = "type")]
    pub relation_type: String,
    pub label: String,
    pub bidirectional: bool,
    pub importance: String,
    pub created_at: String,
    pub source_name: String,
    pub source_type: String,
    pub target_name: String,
    pub target_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub universe_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub created_at: String,
    /// Nome legível resolvido por join polimórfico. Cai para o próprio
    /// `entity_id` quando a linha original já foi excluída — o histórico
    /// precisa sobreviver à exclusão do que ele descreve.
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_card_serializa_o_campo_type_como_o_modelo_typescript() {
        // `type` é palavra reservada em Rust, mas o TS e a coluna se chamam
        // `type`. O rename é o que mantém o contrato igual ao do SQL direto.
        let value = serde_json::to_value(RelationCard {
            id: "r1".into(),
            universe_id: "u1".into(),
            source_id: "a".into(),
            target_id: "b".into(),
            relation_type: "custom".into(),
            label: "irmão de".into(),
            bidirectional: false,
            importance: "normal".into(),
            created_at: "2026-01-01 00:00:00".into(),
            source_name: "A".into(),
            source_type: "Personagem".into(),
            target_name: "B".into(),
            target_type: "Personagem".into(),
        })
        .expect("serializar relação");

        assert_eq!(value["type"], "custom");
        assert!(value.get("relation_type").is_none());
    }
}
