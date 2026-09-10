//! Ler uma `data:` URL sem inventar nada a partir dela.
//!
//! ADR 0010. O que este módulo responde é uma pergunta só, e com três
//! respostas possíveis:
//!
//! ```text
//! ""                              →  Vazio            nada a migrar
//! data:image/png;base64,iVBOR…    →  Decodificada     migra
//! qualquer outra coisa não vazia  →  NaoReconhecido   preserva + pendência
//! ```
//!
//! **Falha de leitura não transforma dado desconhecido em ausência.** É a
//! política do legado inválido, e ela é o motivo de este módulo não ter
//! nenhuma heurística: não baixa URL externa, não abre caminho local, não
//! tenta adivinhar tipo de arquivo por conteúdo, e não calcula hash de string
//! desconhecida. Qualquer valor que não seja uma `data:` URL base64
//! decodificável é preservado exatamente como está, e a pendência vai para
//! `blob_migration_issues`.
//!
//! ## O decodificador é escrito à mão, como o base32 da identidade
//!
//! Não há dependência de base64 no projeto, e `domain/identity.rs` já abriu
//! esse precedente com base32. A alternativa era uma dependência nova para
//! sessenta linhas de tabela — e um decodificador **estrito** é o que esta
//! etapa precisa: aceitar base64 quebrada e devolver bytes truncados
//! produziria um blob válido de um arquivo corrompido, com o inline limpo em
//! seguida por haver hash. Estrito aqui significa que o erro vira pendência,
//! e a pendência preserva o valor.

/// O que aquele valor inline é.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Legado {
    /// Nada a migrar.
    Vazio,
    /// Uma `data:` URL base64 que abriu.
    Decodificada { mime: String, bytes: Vec<u8> },
    /// Não vazio e não reconhecido: URL externa, caminho local, base64
    /// quebrada, ou qualquer outra coisa.
    ///
    /// O `motivo` é curto e **nunca** contém o valor. `blob_migration_issues`
    /// tem `CHECK (length(detail) <= 200)` justamente para que a tentação de
    /// "guardar o valor que não deu para converter" não traga a base64 de
    /// volta para dentro do SQLite.
    NaoReconhecido { motivo: String },
}

/// O maior valor que vale a pena tentar decodificar.
///
/// O editor já limita a imagem a 8 MB, e base64 cresce um terço. A folga cobre
/// cabeçalho e quebras de linha. Acima disso o valor é preservado com
/// pendência em vez de virar uma alocação de tamanho desconhecido no meio de
/// um backfill que roda sobre o acervo inteiro.
const MAIOR_INLINE: usize = 12 * 1024 * 1024;

pub fn classificar(valor: &str) -> Legado {
    let aparado = valor.trim();
    if aparado.is_empty() {
        return Legado::Vazio;
    }
    if aparado.len() > MAIOR_INLINE {
        return nao_reconhecido("valor inline maior que o limite de leitura");
    }

    // O esquema é insensível a caixa por RFC 2397; o resto não é.
    let Some(resto) = aparado
        .get(..5)
        .filter(|inicio| inicio.eq_ignore_ascii_case("data:"))
        .and_then(|_| aparado.get(5..))
    else {
        return nao_reconhecido("valor inline não é uma data URL");
    };

    let Some((cabecalho, carga)) = resto.split_once(',') else {
        return nao_reconhecido("data URL sem a vírgula que separa cabeçalho e conteúdo");
    };

    let mut mime = String::new();
    let mut e_base64 = false;
    for parametro in cabecalho.split(';') {
        let parametro = parametro.trim();
        if parametro.eq_ignore_ascii_case("base64") {
            e_base64 = true;
        } else if mime.is_empty() && parametro.contains('/') {
            mime = parametro.to_ascii_lowercase();
        }
    }

    if !e_base64 {
        // Percent-encoding é legal na RFC e o aplicativo nunca produziu isso.
        // Preservar é mais honesto que escrever um segundo decodificador para
        // um formato que ninguém gerou.
        return nao_reconhecido("data URL não declara base64");
    }
    if mime.is_empty() {
        // Sem MIME não há como servir o arquivo de volta, e adivinhar tipo a
        // partir de bytes é como se serve XSS por engano.
        return nao_reconhecido("data URL sem tipo declarado");
    }

    let bytes = match decodificar_base64(carga) {
        Ok(bytes) => bytes,
        Err(motivo) => return nao_reconhecido(&motivo),
    };
    if bytes.is_empty() {
        return nao_reconhecido("data URL sem conteúdo");
    }

    Legado::Decodificada { mime, bytes }
}

fn nao_reconhecido(motivo: &str) -> Legado {
    Legado::NaoReconhecido {
        motivo: motivo.to_string(),
    }
}

/// `true` quando o valor é uma `data:` URL, decodificável ou não.
///
/// Serve aos guards de entrada, que precisam recusar mídia inline sem se
/// importar se ela abriria.
pub fn parece_data_url(valor: &str) -> bool {
    valor
        .trim_start()
        .get(..5)
        .is_some_and(|inicio| inicio.eq_ignore_ascii_case("data:"))
}

/// Base64 padrão da RFC 4648, estrito no alfabeto e no enchimento.
///
/// Espaço em branco é ignorado, porque `data:` URL guardada em HTML costuma
/// vir com quebra de linha — e ignorar isso é o que faz o mesmo PNG gravado
/// por dois caminhos produzir o mesmo hash. Todo o resto reprova.
pub fn decodificar_base64(texto: &str) -> Result<Vec<u8>, String> {
    let mut simbolos: Vec<u8> = Vec::with_capacity(texto.len());
    let mut enchimento = 0usize;
    for byte in texto.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            enchimento += 1;
            continue;
        }
        if enchimento > 0 {
            return Err("base64 com caractere depois do enchimento".to_string());
        }
        let valor = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            // Nada de base64url aqui: `-` e `_` não aparecem em `data:` URL de
            // navegador, e aceitar os dois alfabetos faria a mesma imagem ter
            // duas representações textuais aceitas — sem mudar o hash, porque
            // o hash é dos bytes, mas escondendo um formato que ninguém gera.
            _ => return Err("base64 com caractere fora do alfabeto".to_string()),
        };
        simbolos.push(valor);
    }

    if enchimento > 2 {
        return Err("base64 com enchimento demais".to_string());
    }
    let sobra = simbolos.len() % 4;
    if sobra == 1 {
        return Err("base64 com comprimento impossível".to_string());
    }
    if sobra != 0 && enchimento != 0 && sobra + enchimento != 4 {
        return Err("base64 com enchimento incoerente".to_string());
    }

    let mut bytes = Vec::with_capacity(simbolos.len() / 4 * 3 + 2);
    for grupo in simbolos.chunks(4) {
        let mut acumulado = 0u32;
        for (posicao, simbolo) in grupo.iter().enumerate() {
            acumulado |= u32::from(*simbolo) << (18 - 6 * posicao);
        }
        // Um grupo de N símbolos carrega N-1 bytes: 4→3, 3→2, 2→1.
        for indice in 0..grupo.len().saturating_sub(1) {
            bytes.push(((acumulado >> (16 - 8 * indice)) & 0xFF) as u8);
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vetores da RFC 4648, para que o decodificador não seja conferido pela
    /// função que o acompanha.
    #[test]
    fn o_decodificador_bate_com_os_vetores_da_rfc() {
        for (codificado, esperado) in [
            ("", ""),
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ] {
            let bytes = decodificar_base64(codificado).expect(codificado);
            assert_eq!(
                String::from_utf8(bytes).expect("utf-8"),
                esperado,
                "vetor {codificado}"
            );
        }
    }

    /// Quebra de linha no meio não muda o resultado.
    ///
    /// É o que faz o mesmo PNG gravado por dois caminhos do aplicativo dar o
    /// mesmo endereço. `data:` URL guardada dentro de HTML costuma vir
    /// quebrada, e tratar isso como erro transformaria imagem boa em pendência.
    #[test]
    fn espaco_em_branco_no_base64_e_ignorado() {
        let direto = decodificar_base64("Zm9vYmFy").expect("direto");
        let quebrado = decodificar_base64("Zm9v\n  YmFy\r\n").expect("quebrado");
        assert_eq!(direto, quebrado);
    }

    /// Base64 quebrada reprova em vez de devolver bytes truncados.
    ///
    /// Devolver o que deu para ler produziria um blob **válido** de um arquivo
    /// **corrompido** — e o backfill limparia o inline em seguida, porque
    /// existe hash. A imagem do escritor viraria um arquivo que abre pela
    /// metade, sem nenhuma pendência registrada.
    #[test]
    fn base64_quebrada_reprova_em_vez_de_truncar() {
        for quebrada in [
            "Zm9vY",      // comprimento impossível
            "Zm9v!mFy",   // fora do alfabeto
            "Zm9vYmFy=x", // caractere depois do enchimento
            "Zg===",      // enchimento demais
            "Zm9-YmFy",   // base64url não é aceito
            "Zm9_YmFy",
        ] {
            assert!(
                decodificar_base64(quebrada).is_err(),
                "{quebrada:?} passou e não devia"
            );
        }
    }

    #[test]
    fn valor_vazio_nao_tem_o_que_migrar() {
        assert_eq!(classificar(""), Legado::Vazio);
        assert_eq!(classificar("   \n  "), Legado::Vazio);
    }

    /// Uma `data:` URL de verdade abre, com o MIME do cabeçalho e os bytes
    /// decodificados.
    #[test]
    fn data_url_valida_devolve_mime_e_bytes() {
        let Legado::Decodificada { mime, bytes } = classificar("data:image/png;base64,Zm9vYmFy")
        else {
            panic!("devia ter aberto");
        };
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, b"foobar");
    }

    /// O MIME vem do cabeçalho, e o esquema é insensível a caixa.
    #[test]
    fn o_cabecalho_e_lido_como_a_rfc_manda() {
        let Legado::Decodificada { mime, .. } =
            classificar("DATA:IMAGE/JPEG;charset=utf-8;BASE64,Zm9v")
        else {
            panic!("devia ter aberto");
        };
        assert_eq!(mime, "image/jpeg", "o MIME é normalizado para minúsculo");
    }

    /// **Nada de heurística sobre o que não é `data:` URL.**
    ///
    /// URL externa não é baixada, caminho local não é aberto, e string
    /// arbitrária não vira arquivo. Cada um destes é preservado com pendência.
    #[test]
    fn o_que_nao_e_data_url_vira_pendencia_e_nao_download() {
        for valor in [
            "https://cdn.exemplo.com/capa.png",
            "C:\\Users\\alguem\\capa.png",
            "/home/alguem/capa.png",
            "file:///tmp/capa.png",
            "capa.png",
            "{\"type\":\"doc\"}",
            "data:image/png,nao-declara-base64",
            "data:;base64,Zm9v",
            "data:image/png;base64",
            "data:image/png;base64,",
            "data:image/png;base64,!!!!",
        ] {
            let classificado = classificar(valor);
            assert!(
                matches!(classificado, Legado::NaoReconhecido { .. }),
                "{valor:?} foi classificado como {classificado:?}"
            );
        }
    }

    /// **O motivo da pendência nunca carrega o valor.**
    ///
    /// A pendência diz que há um valor não convertido e por quê; o valor
    /// continua onde sempre esteve. Se o motivo carregasse a string, a base64
    /// voltaria para dentro do SQLite por uma porta lateral — e o
    /// `CHECK (length(detail) <= 200)` do schema recusaria a gravação no meio
    /// do backfill.
    #[test]
    fn o_motivo_nao_carrega_o_valor() {
        let valor = format!("data:image/png;base64,{}", "!".repeat(5000));
        let Legado::NaoReconhecido { motivo } = classificar(&valor) else {
            panic!("devia ter reprovado");
        };
        assert!(motivo.len() <= 200, "motivo longo demais: {}", motivo.len());
        assert!(
            !motivo.contains('!'),
            "o motivo copiou parte do valor: {motivo}"
        );
    }

    /// Valor gigante é preservado sem tentativa de decodificação.
    #[test]
    fn valor_maior_que_o_limite_e_preservado() {
        let gigante = format!("data:image/png;base64,{}", "A".repeat(MAIOR_INLINE));
        assert!(matches!(
            classificar(&gigante),
            Legado::NaoReconhecido { .. }
        ));
    }

    /// O guard de entrada só quer saber se parece mídia inline.
    #[test]
    fn parece_data_url_responde_sem_decodificar() {
        assert!(parece_data_url("data:image/png;base64,!!!"));
        assert!(parece_data_url("  DATA:image/png;base64,x"));
        assert!(!parece_data_url("https://exemplo.com/x.png"));
        assert!(!parece_data_url(""));
        assert!(
            !parece_data_url("o personagem escreveu data:image/png no caderno"),
            "só conta no começo do valor: texto do escritor não é mídia"
        );
    }
}
