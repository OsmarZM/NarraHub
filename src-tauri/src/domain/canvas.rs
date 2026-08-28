use serde::{Deserialize, Serialize};

/// Tipos de elemento livre do canvas, iguais ao `CHECK` da migration 14.
pub const NODE_KINDS: &[&str] = &["title", "image", "note"];

/// Uma ponta de ligação é uma entidade cadastrada ou um elemento livre.
pub const ENDPOINT_KINDS: &[&str] = &["entity", "canvas"];

pub fn is_known_node_kind(kind: &str) -> bool {
    NODE_KINDS.contains(&kind)
}

pub fn is_known_endpoint_kind(kind: &str) -> bool {
    ENDPOINT_KINDS.contains(&kind)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: String,
    pub universe_id: String,
    pub kind: String,
    pub text: String,
    pub image: String,
    pub color: String,
    pub position_x: f64,
    pub position_y: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEntityPosition {
    pub entity_id: String,
    pub position_x: f64,
    pub position_y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub universe_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasEndpoint {
    pub kind: String,
    pub id: String,
}

/// `None` é "não mexer". Um elemento pode ter só a cor trocada sem que o texto
/// digitado seja reenviado junto.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasNodePatch {
    pub text: Option<String>,
    pub image: Option<String>,
    pub color: Option<String>,
}

impl CanvasNodePatch {
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.image.is_none() && self.color.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub universe_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub data_url: String,
    pub caption: String,
    pub sort_order: i64,
    pub created_at: String,
}

/// Donos que podem ter anexo. Diferente das tags, `attachments` não tem
/// `CHECK` na coluna — a validação existe só aqui, então tirá-la deixaria
/// qualquer texto virar tipo de dono.
pub const ATTACHMENT_OWNER_TYPES: &[&str] = &["entity", "chapter", "universe"];

pub fn is_known_attachment_owner(owner_type: &str) -> bool {
    ATTACHMENT_OWNER_TYPES.contains(&owner_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipos_do_canvas_batem_com_o_check_da_migration_14() {
        assert!(is_known_node_kind("note"));
        assert!(!is_known_node_kind("desenho"));
        assert!(is_known_endpoint_kind("canvas"));
        assert!(!is_known_endpoint_kind("chapter"));
    }

    #[test]
    fn dono_de_anexo_e_validado_aqui_porque_o_schema_nao_valida() {
        assert!(is_known_attachment_owner("entity"));
        assert!(!is_known_attachment_owner("planning"));
    }

    #[test]
    fn patch_vazio_e_reconhecido_como_vazio() {
        assert!(CanvasNodePatch::default().is_empty());
        assert!(!CanvasNodePatch {
            color: Some("#fff".into()),
            ..Default::default()
        }
        .is_empty());
    }
}
