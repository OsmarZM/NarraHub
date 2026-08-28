use uuid::Uuid;

/// Id de criação normal. O plano da Fase 4 é explícito: quem gera é o Rust.
/// Import e sync têm fluxo próprio e idempotente, e por isso trazem o id de
/// fora em vez de chamar esta função.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Carimbo no mesmo formato que o frontend gravava (`YYYY-MM-DD HH:MM:SS`,
/// UTC). Mudar para ISO-8601 aqui quebraria a ordenação lexicográfica de
/// `ORDER BY created_at` contra as linhas que já estão no banco.
pub fn now_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_distinct_ids() {
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn timestamps_keep_the_format_already_gravado_no_banco() {
        let stamp = now_timestamp();
        assert_eq!(stamp.len(), 19, "esperado YYYY-MM-DD HH:MM:SS, veio {stamp}");
        assert_eq!(stamp.as_bytes()[10], b' ', "o separador precisa ser espaço, não T");
    }
}
