use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub universe_id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub name: String,
    pub description: String,
    pub summary: String,
    pub image: String,
    pub canon_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityAttribute {
    pub id: String,
    pub entity_id: String,
    pub key: String,
    pub value: String,
    pub sort_order: i64,
}

/// Uma ponta de relação vista da ficha: a entidade do outro lado, resumida.
/// O modelo TypeScript declara `Entity` inteira aqui, mas só quatro campos são
/// lidos — devolver a linha completa custaria um join a mais por relação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedEntity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRelation {
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
    pub source: RelatedEntity,
    pub target: RelatedEntity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionWithChapter {
    pub id: String,
    pub chapter_id: String,
    pub entity_id: String,
    pub created_at: String,
    pub chapter_title: String,
    pub book_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityWithDetails {
    #[serde(flatten)]
    pub entity: Entity,
    pub attributes: Vec<EntityAttribute>,
    pub relations: Vec<EntityRelation>,
    pub mentions: Vec<MentionWithChapter>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntityAttribute {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntity {
    pub universe_id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub attributes: Vec<NewEntityAttribute>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub summary: Option<String>,
    pub image: Option<String>,
    pub canon_status: Option<String>,
}

impl EntityUpdate {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.summary.is_none()
            && self.image.is_none()
            && self.canon_status.is_none()
    }
}

/// Fichas em branco que uma entidade nova ganha, por tipo.
///
/// Esta lista também existe em `core/models/index.ts`, onde a tela a usa para
/// desenhar o formulário de criação. Duas cópias é uma dívida assumida: passar
/// a lista do frontend para o comando deixaria o cliente decidir o formato do
/// dado gravado. Há teste de fronteira comparando as duas — se divergirem, ele
/// falha, que é o que impede a duplicação de virar bug silencioso.
pub const DEFAULT_ATTRIBUTES: &[(&str, &[&str])] = &[
    (
        "Personagem",
        &[
            "Idade",
            "Nascimento",
            "Cidade natal",
            "Localização atual",
            "Nacionalidade",
            "Ocupação",
            "Arco do personagem",
            "Personalidade",
            "Qualidades",
            "Defeitos",
            "Motivações e objetivos",
            "Background",
            "Família",
            "Observação",
        ],
    ),
    (
        "Lugar",
        &["Tipo", "População", "Governo", "Governante", "Clima"],
    ),
    ("Evento", &["Data", "Duração", "Local", "Consequências"]),
    (
        "Objeto",
        &["Tipo", "Material", "Propriedades", "Dono atual", "Origem"],
    ),
    (
        "Organização",
        &["Tipo", "Líder", "Sede", "Membros", "Objetivo"],
    ),
];

/// Tipo desconhecido não é erro: a entidade nasce sem atributo padrão, e a
/// pessoa preenche o que quiser. Recusar travaria tipo customizado.
pub fn default_attributes_for(entity_type: &str) -> &'static [&'static str] {
    DEFAULT_ATTRIBUTES
        .iter()
        .find(|(kind, _)| *kind == entity_type)
        .map(|(_, attributes)| *attributes)
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipo_desconhecido_nasce_sem_atributo_em_vez_de_falhar() {
        assert!(default_attributes_for("Conceito inventado").is_empty());
        assert_eq!(default_attributes_for("Lugar").len(), 5);
    }

    #[test]
    fn entity_with_details_serializa_achatado_como_o_modelo_typescript() {
        let value = serde_json::to_value(EntityWithDetails {
            entity: Entity {
                id: "e1".into(),
                universe_id: "u1".into(),
                entity_type: "Personagem".into(),
                name: "Frodo".into(),
                description: String::new(),
                summary: String::new(),
                image: String::new(),
                canon_status: "CANON".into(),
                created_at: "2026-01-01 00:00:00".into(),
                updated_at: "2026-01-01 00:00:00".into(),
            },
            attributes: Vec::new(),
            relations: Vec::new(),
            mentions: Vec::new(),
        })
        .expect("serializar ficha");

        assert_eq!(value["id"], "e1");
        assert_eq!(value["type"], "Personagem");
        assert!(value.get("entity").is_none(), "não pode aninhar em `entity`");
        assert!(value["attributes"].is_array());
    }
}
