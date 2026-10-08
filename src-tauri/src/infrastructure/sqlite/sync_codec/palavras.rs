//! Contagem de palavras do capítulo, idêntica à do editor.
//!
//! `word_count` fica **fora** do payload canônico: é derivável do `content`. Quem recebe um
//! capítulo recalcula aqui, com a mesma regra do frontend
//! (`manuscript.store.ts` e `writing-page.component.ts`, `countWords`):
//!
//! ```js
//! const normalized = content.replace(/<[^>]+>/g, ' ').trim();
//! return normalized ? normalized.split(/\s+/u).length : 0;
//! ```
//!
//! O `\s` do JavaScript não é o `char::is_whitespace` do Rust (U+0085 entra num e não no outro;
//! U+FEFF, o contrário). Por isso o conjunto está escrito à mão, e os vetores em
//! `fixtures/contagem_de_palavras.json` são conferidos pelos dois lados.

/// `\s` do JavaScript: WhiteSpace + LineTerminator (ECMA-262), com Zs do Unicode.
fn espaco_js(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

pub fn contar(content: &str) -> i64 {
    // `/<[^>]+>/g`: a partir de um `<`, o primeiro `>` com pelo menos um caractere no meio.
    let caracteres: Vec<char> = content.chars().collect();
    let mut normalizado = String::with_capacity(content.len());
    let mut i = 0;
    while i < caracteres.len() {
        if caracteres[i] == '<' {
            if let Some(fim) = caracteres[i + 1..].iter().position(|&c| c == '>') {
                if fim > 0 {
                    normalizado.push(' ');
                    i += fim + 2;
                    continue;
                }
            }
        }
        normalizado.push(caracteres[i]);
        i += 1;
    }
    normalizado
        .split(espaco_js)
        .filter(|parte| !parte.is_empty())
        .count() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Vetor {
        content: String,
        palavras: i64,
    }

    /// Os mesmos vetores que `tests/word-count-parity.test.mjs` roda contra a regex do frontend.
    #[test]
    fn contagem_bate_com_os_vetores_do_frontend() {
        let vetores: Vec<Vetor> = serde_json::from_str(include_str!(
            "../../../../fixtures/contagem_de_palavras.json"
        ))
        .expect("vetores");
        assert!(vetores.len() >= 10);
        for vetor in vetores {
            assert_eq!(
                contar(&vetor.content),
                vetor.palavras,
                "{:?}",
                vetor.content
            );
        }
    }
}
