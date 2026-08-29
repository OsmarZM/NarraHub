use serde::{Deserialize, Serialize};

/// Prefixo que marca uma proposta sobre um atributo de ficha, em vez de uma
/// coluna. Tudo depois dele é o nome do atributo, escolhido pelo convidado.
pub const ATTRIBUTE_FIELD_PREFIX: &str = "attribute:";

/// Nome de atributo mais longo que isto é recusado. O limite existe para uma
/// proposta não conseguir criar uma chave gigante na ficha de outra pessoa.
pub const MAX_ATTRIBUTE_KEY: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationSession {
    pub id: String,
    pub title: String,
    pub permission: String,
    pub universe_ids: String,
    pub encryption_key: String,
    pub revoke_token: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
    pub ended_at: Option<String>,
    pub pending_count: i64,
    pub note_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationContribution {
    pub id: String,
    pub session_id: String,
    pub sequence: i64,
    pub contributor: String,
    pub kind: String,
    pub universe_id: String,
    pub target_type: String,
    pub target_id: String,
    pub target_label: String,
    pub field: String,
    pub original_value: String,
    pub proposed_value: String,
    pub message: String,
    pub status: String,
    pub created_at: String,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCollaborationSession {
    pub id: String,
    pub title: String,
    pub permission: String,
    pub universe_ids: Vec<String>,
    pub encryption_key: String,
    pub revoke_token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingContribution {
    pub id: String,
    pub contributor: String,
    pub kind: String,
    pub universe_id: String,
    pub target_type: String,
    pub target_id: String,
    pub target_label: String,
    #[serde(default)]
    pub field: String,
    #[serde(default)]
    pub original_value: String,
    #[serde(default)]
    pub proposed_value: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub created_at: String,
}

/// Onde uma proposta aprovada pode escrever.
///
/// Isto é fronteira de segurança, não conveniência: o valor vem de fora, de um
/// convidado com link. A tabela e a coluna saem **desta lista**, nunca do texto
/// recebido — o caminho antigo interpolava `item.field` direto no `UPDATE`, e
/// era só a checagem da lista que separava isso de uma injeção de SQL.
pub fn writable_column(target_type: &str, field: &str) -> Option<(&'static str, &'static str)> {
    let table = match target_type {
        "universe" => "universes",
        "chapter" => "chapters",
        "entity" => "entities",
        _ => return None,
    };
    let column = match (target_type, field) {
        ("universe", "name") => "name",
        ("universe", "description") => "description",
        ("chapter", "title") => "title",
        ("chapter", "content") => "content",
        ("chapter", "summary") => "summary",
        ("entity", "name") => "name",
        ("entity", "description") => "description",
        ("entity", "summary") => "summary",
        ("entity", "canon_status") => "canon_status",
        _ => return None,
    };
    Some((table, column))
}

/// Nome do atributo de ficha que a proposta quer mexer, se for esse o caso.
pub fn attribute_key<'a>(target_type: &str, field: &'a str) -> Option<&'a str> {
    if target_type != "entity" {
        return None;
    }
    field.strip_prefix(ATTRIBUTE_FIELD_PREFIX).map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campo_fora_da_lista_nao_tem_coluna_para_escrever() {
        assert_eq!(
            writable_column("entity", "name"),
            Some(("entities", "name"))
        );
        assert_eq!(
            writable_column("universe", "cover_image"),
            None,
            "capa nao esta na lista: proposta nao pode trocar a capa do universo"
        );
        assert_eq!(writable_column("chapter", "word_count"), None);
        assert_eq!(writable_column("inventado", "name"), None);
    }

    #[test]
    fn tentativa_de_injecao_no_nome_do_campo_nao_casa_com_nada() {
        // O caminho antigo montava `SET ${item.field} = ?`. Era so a checagem
        // da lista que separava isso de uma injecao — aqui a coluna nem existe
        // fora da lista, entao nao ha texto de fora entrando no SQL.
        assert_eq!(writable_column("entity", "name = '' , canon_status"), None);
        assert_eq!(
            writable_column("chapter", "content; DROP TABLE chapters"),
            None
        );
    }

    #[test]
    fn atributo_de_ficha_e_reconhecido_so_para_entidade() {
        assert_eq!(attribute_key("entity", "attribute: Idade "), Some("Idade"));
        assert_eq!(attribute_key("entity", "name"), None);
        assert_eq!(
            attribute_key("chapter", "attribute:Idade"),
            None,
            "capitulo nao tem ficha de atributos"
        );
    }
}
