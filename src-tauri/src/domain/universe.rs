use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Universe {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cover_image: String,
    pub created_at: String,
    pub updated_at: String,
}

/// `BTreeMap` e não `HashMap`: a ordem entra em JSON e em asserção de teste,
/// e ordem instável transformaria um teste de contrato em teste intermitente.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniverseStats {
    pub total_words: i64,
    pub total_chapters: i64,
    pub total_stories: i64,
    pub total_books: i64,
    pub total_entities: i64,
    pub entity_counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniverseWithStats {
    #[serde(flatten)]
    pub universe: Universe,
    pub stats: UniverseStats,
}

/// Campos que o frontend pode alterar. `None` significa "não mexer", que é
/// diferente de "gravar vazio" — o `UPDATE` monta o SET só com o que veio.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniverseUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cover_image: Option<String>,
}

impl UniverseUpdate {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.description.is_none() && self.cover_image.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universe_with_stats_serializa_achatado_como_o_modelo_typescript() {
        // O TS declara `UniverseWithStats extends Universe`, ou seja, os campos
        // do universo ficam na raiz do objeto — não aninhados em `universe`.
        let value = serde_json::to_value(UniverseWithStats {
            universe: Universe {
                id: "u1".into(),
                name: "Terra Média".into(),
                description: String::new(),
                cover_image: String::new(),
                created_at: "2026-01-01 00:00:00".into(),
                updated_at: "2026-01-01 00:00:00".into(),
            },
            stats: UniverseStats::default(),
        })
        .expect("serializar universo com estatísticas");

        assert_eq!(value["id"], "u1");
        assert_eq!(value["name"], "Terra Média");
        assert!(value.get("universe").is_none(), "não pode aninhar em `universe`");
        assert_eq!(value["stats"]["total_words"], 0);
    }

    #[test]
    fn update_vazio_e_reconhecido_como_vazio() {
        assert!(UniverseUpdate::default().is_empty());
        assert!(!UniverseUpdate { name: Some("x".into()), ..Default::default() }.is_empty());
    }
}
