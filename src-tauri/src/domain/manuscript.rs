use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Story {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub description: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub story_id: String,
    pub name: String,
    pub description: String,
    pub cover_image: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookOption {
    #[serde(flatten)]
    pub book: Book,
    pub story_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub book_id: String,
    pub title: String,
    /// JSON do Tiptap. O core não interpreta: guardar e devolver o texto do
    /// editor é justamente o que não pode ganhar esperteza.
    pub content: String,
    pub summary: String,
    pub scene_origin: String,
    pub scene_destination: String,
    pub word_count: i64,
    pub status: String,
    pub canon_status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterOption {
    #[serde(flatten)]
    pub chapter: Chapter,
    pub book_name: String,
    pub story_id: String,
    pub story_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl StoryUpdate {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.description.is_none()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cover_image: Option<String>,
}

impl BookUpdate {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.description.is_none() && self.cover_image.is_none()
    }
}

/// Campos do capítulo que a tela edita separadamente. Cada tela salva um
/// pedaço — o editor salva conteúdo, o inspetor salva resumo, a árvore salva
/// título — então um `None` aqui significa "esta tela não mexeu nisso".
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterUpdate {
    pub title: Option<String>,
    pub content: Option<String>,
    pub word_count: Option<i64>,
    pub summary: Option<String>,
    pub scene_origin: Option<String>,
    pub scene_destination: Option<String>,
    pub status: Option<String>,
    pub canon_status: Option<String>,
}

impl ChapterUpdate {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.content.is_none()
            && self.word_count.is_none()
            && self.summary.is_none()
            && self.scene_origin.is_none()
            && self.scene_destination.is_none()
            && self.status.is_none()
            && self.canon_status.is_none()
    }

    /// Conteúdo sem contagem é inconsistente: a estatística do universo soma
    /// `word_count`, então gravar um sem o outro faz o total mentir até o
    /// próximo salvamento.
    pub fn content_without_word_count(&self) -> bool {
        self.content.is_some() && self.word_count.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_option_serializa_achatado_como_o_modelo_typescript() {
        let value = serde_json::to_value(ChapterOption {
            chapter: Chapter {
                id: "c1".into(),
                book_id: "b1".into(),
                title: "Cap 1".into(),
                content: String::new(),
                summary: String::new(),
                scene_origin: String::new(),
                scene_destination: String::new(),
                word_count: 0,
                status: "IDEIA".into(),
                canon_status: "CANON".into(),
                sort_order: 0,
                created_at: "2026-01-01 00:00:00".into(),
                updated_at: "2026-01-01 00:00:00".into(),
            },
            book_name: "Livro".into(),
            story_id: "s1".into(),
            story_name: "História".into(),
        })
        .expect("serializar capítulo");

        assert_eq!(value["id"], "c1");
        assert_eq!(value["book_name"], "Livro");
        assert!(
            value.get("chapter").is_none(),
            "não pode aninhar em `chapter`"
        );
    }

    #[test]
    fn conteudo_sem_contagem_e_reconhecido_como_inconsistente() {
        let so_conteudo = ChapterUpdate {
            content: Some("texto".into()),
            ..Default::default()
        };
        assert!(so_conteudo.content_without_word_count());

        let completo = ChapterUpdate {
            content: Some("texto".into()),
            word_count: Some(1),
            ..Default::default()
        };
        assert!(!completo.content_without_word_count());

        let so_titulo = ChapterUpdate {
            title: Some("x".into()),
            ..Default::default()
        };
        assert!(!so_titulo.content_without_word_count());
    }
}
