//! A forma do texto que o escritor lê.
//!
//! Este arquivo não tem regra de runtime: existe para um gate, como
//! `command_placement`.
//!
//! ## O defeito que ele fecha
//!
//! Mensagem longa em Rust se escreve com continuação de linha, e o compilador
//! junta as partes descartando a quebra **e a indentação**:
//!
//! ```text
//! "O dispositivo foi abandonado e não pode voltar com a \
//!  mesma identidade."
//!
//! →  "O dispositivo foi abandonado e não pode voltar com a mesma identidade."
//! ```
//!
//! Quando a barra se perde na edição — e ela se perdeu oito vezes neste
//! repositório, por uma ferramenta de escrita que consumia a barra invertida —
//! o resultado **compila**, `cargo fmt` aceita, e nenhum teste de
//! comportamento reclama. O que muda é só o texto:
//!
//! ```text
//! "…não pode voltar com a                  mesma identidade."
//!                        ^^^^^^^^^^^^^^^^^^
//! ```
//!
//! Dezoito espaços no meio da frase, na tela do escritor, no momento em que
//! ele está tentando entender por que o pareamento falhou. Duas das oito
//! ocorrências chegaram à `main` e sobreviveram a duas etapas.
//!
//! É defeito de forma do texto, então o instrumento é textual — o mesmo
//! argumento que fez o gate da migration 20 procurar `UPDATE` no SQL em vez de
//! esperar um efeito observável.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Uma corrida de espaços **dentro** de um literal.
    ///
    /// Quatro é o limite porque duas colunas de espaço aparecem em texto
    /// legítimo (depois de ponto, em tabela curta) e a assinatura do defeito é
    /// a indentação de uma continuação perdida, que neste repositório nunca é
    /// menor que dezesseis.
    const CORRIDA_MINIMA: usize = 5;

    /// O texto dentro do primeiro literal da linha, se a linha começar com um.
    ///
    /// Só as linhas que **começam** com aspas, que é a forma de uma
    /// continuação de mensagem. Uma linha com código antes da string teria a
    /// corrida de espaços do alinhamento de comentário à direita, fora do
    /// literal — foi o falso positivo que a primeira versão desta regra
    /// produziu oito vezes.
    fn conteudo_do_literal(despido: &str) -> Option<String> {
        let mut caracteres = despido.chars();
        if caracteres.next() != Some('"') {
            return None;
        }
        let mut dentro = String::new();
        while let Some(atual) = caracteres.next() {
            match atual {
                // Escape: consome o par inteiro. Uma barra no fim da linha é
                // continuação legítima, e aí o literal simplesmente acaba aqui.
                '\\' => {
                    if let Some(seguinte) = caracteres.next() {
                        dentro.push(atual);
                        dentro.push(seguinte);
                    } else {
                        return Some(dentro);
                    }
                }
                '"' => return Some(dentro),
                _ => dentro.push(atual),
            }
        }
        Some(dentro)
    }

    fn tem_corrida(texto: &str) -> bool {
        let mut seguidos = 0usize;
        for caractere in texto.chars() {
            if caractere == ' ' {
                seguidos += 1;
                if seguidos >= CORRIDA_MINIMA {
                    return true;
                }
            } else {
                seguidos = 0;
            }
        }
        false
    }

    fn percorrer(diretorio: &Path, raiz: &Path, achados: &mut Vec<String>, lidos: &mut usize) {
        let Ok(filhos) = std::fs::read_dir(diretorio) else {
            return;
        };
        for filho in filhos.flatten() {
            let caminho = filho.path();
            if caminho.is_dir() {
                percorrer(&caminho, raiz, achados, lidos);
                continue;
            }
            if caminho.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(conteudo) = std::fs::read_to_string(&caminho) else {
                continue;
            };
            *lidos += 1;
            for (numero, linha) in conteudo.lines().enumerate() {
                let despido = linha.trim_start();
                if despido.starts_with("//") {
                    continue;
                }
                if let Some(dentro) = conteudo_do_literal(despido) {
                    if tem_corrida(&dentro) {
                        let relativo = caminho
                            .strip_prefix(raiz)
                            .unwrap_or(&caminho)
                            .to_string_lossy()
                            .replace('\\', "/");
                        achados.push(format!("{relativo}:{}", numero + 1));
                    }
                }
            }
        }
    }

    /// **Nenhuma mensagem carrega uma continuação de linha perdida.**
    #[test]
    fn nenhuma_mensagem_tem_corrida_de_espacos_dentro_do_literal() {
        let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut achados = Vec::new();
        let mut lidos = 0usize;
        percorrer(&raiz, &raiz, &mut achados, &mut lidos);

        // A varredura achou mesmo alguma coisa para varrer?
        //
        // Sem esta parte o gate passaria por vácuo — diretório errado, extensão
        // errada, e a lista de pendências fica vazia por não ter lido nada. É o
        // mesmo defeito que o gate de catálogo do ADR 0010 tinha antes de ganhar
        // a exigência de encontrar as seis colunas conhecidas.
        assert!(
            lidos > 30,
            "a varredura leu só {lidos} arquivos .rs; ela quebrou, e um gate que não lê \
             nada aprova tudo"
        );

        assert!(
            achados.is_empty(),
            "estas mensagens têm uma corrida de espaços dentro do literal, que é a \
             assinatura de uma continuação de linha perdida: {achados:?}\n\n\
             O texto chega ao escritor com dezoito espaços no meio da frase. Escreva a \
             continuação com barra invertida no fim da linha, e a parte seguinte começa \
             depois da indentação — o compilador descarta a quebra e os espaços."
        );
    }

    /// E a regra encontra o defeito quando ele existe.
    ///
    /// Gate do gate. Sem isto, uma regra que nunca casa com nada é
    /// indistinguível de uma árvore limpa — e foi assim que a heurística de
    /// nome do catálogo quase passou por vácuo.
    #[test]
    fn a_regra_reconhece_a_assinatura_do_defeito() {
        let quebrada = "\"O dispositivo foi abandonado e não pode voltar com a                  mesma identidade.\"";
        assert!(
            tem_corrida(&conteudo_do_literal(quebrada).expect("é um literal")),
            "a regra deixou de reconhecer o defeito que ela existe para pegar"
        );

        // E não acusa o que é legítimo.
        for aceitavel in [
            "\"Uma frase normal, com espaço simples.\"",
            "\"Duas colunas depois do ponto.  Assim.\"",
            "\"continuação de verdade, sem corrida \\",
        ] {
            let dentro = conteudo_do_literal(aceitavel).expect("é um literal");
            assert!(!tem_corrida(&dentro), "falso positivo em {aceitavel:?}");
        }

        // Linha com código antes da string não é analisada: a corrida ali é o
        // alinhamento do comentário à direita, fora do literal.
        assert!(
            conteudo_do_literal("let x = \"curta\",          // alinhado").is_none(),
            "a regra precisa ignorar linha que não começa com aspas"
        );
    }
}
