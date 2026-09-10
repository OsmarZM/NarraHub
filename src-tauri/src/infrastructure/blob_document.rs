//! O transformador de documento: **um só**, para as quatro superfícies.
//!
//! ADR 0010, decisão **A** da NH-065. `chapters.content` guarda **HTML**, não
//! JSON do Tiptap — `editor.getHTML()` na saída, `setContent(html)` na
//! entrada, e texto puro envelopado em `<p>` para dado antigo. A imagem é um
//! elemento, não um node com `attrs`:
//!
//! ```text
//! antes    <img src="data:image/png;base64,iVBOR…" alt="rosto.png">
//! depois   <img data-narrahub-blob="<64 hex>" data-mime-type="image/png"
//!               alt="rosto.png">
//! ```
//!
//! O hash é a **identidade portátil**. Nada de caminho absoluto, URL
//! temporária, `blob:` de navegador ou `data:` URL no que vai ao banco: o
//! mesmo HTML tem que funcionar igual no Windows e no Android, e a resolução
//! `hash → recurso local` acontece só em tempo de execução.
//!
//! ## Por que `lol_html`, e não um varredor de `<img>`
//!
//! Porque a exigência é estrutural, não textual. `content.contains("data:")`
//! encontra texto legítimo do escritor, e um varredor artesanal tropeça em
//! comentário HTML, em `<img` dentro de valor de atributo e em entidade
//! escapada. O `lol_html` repassa verbatim tudo o que não casa com o seletor,
//! que é exatamente a garantia de preservar atributo desconhecido e o HTML ao
//! redor.
//!
//! ## Duas passadas, e isso é decisão
//!
//! ```text
//! passada 1   varre e coleta os <img> em ordem de documento   — sem I/O
//! decisão     decodifica, publica no BlobStore, verifica      — fora do rewriter
//! passada 2   reescreve o N-ésimo <img> com a decisão N
//! ```
//!
//! Publicar blob dentro do handler do rewriter seria I/O de disco no meio de
//! um parser em streaming, com o erro tendo de atravessar
//! `Box<dyn Error>` e voltar como sabe-se lá o quê. Separando, a passada 1
//! serve de graça aos **guards** — quem só precisa perguntar "este documento
//! ainda tem mídia inline?" para nunca faz I/O — e o erro de publicação chega
//! ao chamador com tipo.
//!
//! A ordem de documento é o que liga as duas passadas, e ela é determinística:
//! o `lol_html` visita os elementos na ordem em que aparecem.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::data_url::{classificar, Legado};
use crate::infrastructure::blob_store::{e_hash_canonico, BlobStore};
use lol_html::html_content::Element;
use lol_html::{element, rewrite_str, RewriteStrSettings};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// Onde o hash mora no HTML persistido.
///
/// Prefixo `data-` porque é atributo de dado, não atributo de imagem do HTML:
/// nenhum navegador tenta buscar nada a partir dele, o que é justamente o que
/// se quer de uma referência que só o aplicativo sabe resolver.
pub const ATTR_BLOB: &str = "data-narrahub-blob";

/// O MIME lido do cabeçalho da `data:` URL na migração.
pub const ATTR_MIME: &str = "data-mime-type";

/// O atributo legado. Continua sendo **lido**; nunca mais é escrito com
/// `data:`.
pub const ATTR_SRC: &str = "src";

/// O que um `<img>` do documento carrega.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imagem {
    /// Já está no contrato novo: hash canônico no atributo de dado.
    Referencia { hash: String, mime: String },
    /// `src` com `data:` URL que abriu — migrável.
    InlineValida { mime: String, bytes: Vec<u8> },
    /// `src` não vazio que não é `data:` URL decodificável, **ou** um
    /// `data-narrahub-blob` que não é hash canônico.
    NaoReconhecida { motivo: String },
    /// `<img>` sem `src` e sem referência. Não é asset nenhum.
    SemFonte,
}

/// Um `<img>` na ordem em que aparece no documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoDeImagem {
    pub indice: usize,
    pub imagem: Imagem,
}

/// O que fazer com o N-ésimo `<img>` na passada de reescrita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decisao {
    /// Troca `src` por referência de blob.
    Referenciar { hash: String, mime: String },
    /// Não toca em nada. É o que o legado inválido recebe.
    Preservar,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeDocumento {
    /// O HTML não pôde ser processado.
    ///
    /// Não é "documento inválido": HTML mal formado é normal e o parser lida
    /// com quase tudo. Isto cobre o que o `lol_html` recusa de verdade, e o
    /// tratamento é o mesmo do legado inválido — preservar e registrar.
    NaoProcessavel { motivo: String },
}

impl std::fmt::Display for FalhaDeDocumento {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDeDocumento::NaoProcessavel { motivo } => write!(
                f,
                "O texto deste capítulo não pôde ser lido como documento: {motivo}. Ele \
                 continua preservado exatamente como está."
            ),
        }
    }
}

/// O resultado de converter um documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversao {
    /// O HTML de saída. Igual à entrada quando nada mudou.
    pub html: String,
    /// Quantos `<img>` inline viraram referência.
    pub convertidas: usize,
    /// Quantos já estavam no contrato novo.
    pub ja_eram_referencia: usize,
    /// Um motivo por `<img>` que não pôde ser convertido, com o índice.
    pub pendencias: Vec<(usize, String)>,
}

impl Conversao {
    pub fn mudou(&self) -> bool {
        self.convertidas > 0
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Passada 1: varrer, sem I/O
// ═══════════════════════════════════════════════════════════════════════════

/// Os `<img>` do documento, em ordem, classificados.
///
/// Não toca em disco. É o que os guards usam.
pub fn imagens_do_documento(html: &str) -> Result<Vec<NoDeImagem>, FalhaDeDocumento> {
    let encontradas: RefCell<Vec<NoDeImagem>> = RefCell::new(Vec::new());

    // O seletor é `img`, e não `img[src]`: um documento já convertido tem
    // `<img data-narrahub-blob=…>` **sem** `src`, e ele precisa ser visto.
    let resultado = rewrite_str(
        html,
        RewriteStrSettings::new().append_element_content_handler(element!(
            "img",
            |el: &mut Element| {
                let hash = el.get_attribute(ATTR_BLOB).unwrap_or_default();
                let mime = el.get_attribute(ATTR_MIME).unwrap_or_default();
                let src = el.get_attribute(ATTR_SRC).unwrap_or_default();

                let imagem = if !hash.is_empty() {
                    if e_hash_canonico(&hash) {
                        Imagem::Referencia { hash, mime }
                    } else {
                        Imagem::NaoReconhecida {
                            motivo: "a referência de blob no documento não é um SHA-256 \
                                     canônico"
                                .to_string(),
                        }
                    }
                } else if src.trim().is_empty() {
                    Imagem::SemFonte
                } else {
                    match classificar(&src) {
                        Legado::Decodificada { mime, bytes } => {
                            Imagem::InlineValida { mime, bytes }
                        }
                        Legado::NaoReconhecido { motivo } => Imagem::NaoReconhecida { motivo },
                        // `classificar` só devolve Vazio para string vazia, e
                        // isso já foi tratado acima.
                        Legado::Vazio => Imagem::SemFonte,
                    }
                };

                let mut lista = encontradas.borrow_mut();
                let indice = lista.len();
                lista.push(NoDeImagem { indice, imagem });
                Ok(())
            }
        )),
    );

    match resultado {
        Ok(_) => Ok(encontradas.into_inner()),
        Err(erro) => Err(FalhaDeDocumento::NaoProcessavel {
            motivo: erro.to_string(),
        }),
    }
}

/// **Este documento ainda tem mídia inline?**
///
/// O guard fail-closed das barreiras de entrada. Estrutural: pergunta se
/// existe um elemento `img` cujo `src` é `data:`. Um `contains("data:")`
/// responderia sim para um personagem que escreveu sobre data URLs no meio do
/// capítulo.
pub fn tem_midia_inline(html: &str) -> Result<bool, FalhaDeDocumento> {
    Ok(imagens_do_documento(html)?.iter().any(|no| {
        matches!(
            no.imagem,
            Imagem::InlineValida { .. } | Imagem::NaoReconhecida { .. }
        )
    }))
}

/// Os hashes referenciados por este documento.
///
/// Conjunto, e não lista: a mesma imagem usada cem vezes é um hash. É o que o
/// manifesto de blobs do bootstrap consome.
pub fn hashes_do_documento(html: &str) -> Result<BTreeSet<String>, FalhaDeDocumento> {
    Ok(imagens_do_documento(html)?
        .into_iter()
        .filter_map(|no| match no.imagem {
            Imagem::Referencia { hash, .. } => Some(hash),
            _ => None,
        })
        .collect())
}

// ═══════════════════════════════════════════════════════════════════════════
// Passada 2: reescrever
// ═══════════════════════════════════════════════════════════════════════════

/// Aplica uma decisão por `<img>`, na ordem do documento.
///
/// Só os atributos do asset mudam. `alt`, `title`, dimensões e **todo**
/// atributo desconhecido continuam onde estavam, porque o rewriter parte do
/// elemento real em vez de reconstruí-lo a partir de um modelo simplificado —
/// que é como um campo futuro seria jogado fora sem ninguém notar.
pub fn reescrever(html: &str, decisoes: &[Decisao]) -> Result<String, FalhaDeDocumento> {
    let proximo = RefCell::new(0usize);

    let resultado = rewrite_str(
        html,
        RewriteStrSettings::new().append_element_content_handler(element!(
            "img",
            |el: &mut Element| {
                let indice = {
                    let mut contador = proximo.borrow_mut();
                    let atual = *contador;
                    *contador += 1;
                    atual
                };
                match decisoes.get(indice) {
                    Some(Decisao::Referenciar { hash, mime }) => {
                        el.set_attribute(ATTR_BLOB, hash)?;
                        el.set_attribute(ATTR_MIME, mime)?;
                        // O `src` sai do que é persistido. Quem renderiza o
                        // resolve em tempo de execução, a partir do hash.
                        el.remove_attribute(ATTR_SRC);
                    }
                    // Sem decisão para este índice é o mesmo que preservar: o
                    // documento pode ter ganhado imagem entre as duas passadas
                    // se alguém salvou no meio, e a alternativa a preservar
                    // seria escrever a decisão de outra imagem nesta.
                    Some(Decisao::Preservar) | None => {}
                }
                Ok(())
            }
        )),
    );

    resultado.map_err(|erro| FalhaDeDocumento::NaoProcessavel {
        motivo: erro.to_string(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// As duas juntas
// ═══════════════════════════════════════════════════════════════════════════

/// Converte um documento inteiro: varre, publica o que der, reescreve.
///
/// Falha de publicação de **uma** imagem não impede as outras: aquela vira
/// pendência e o `src` dela fica intacto. É a política do legado inválido
/// aplicada node a node — falha de migração não transforma dado desconhecido
/// em ausência.
pub fn converter(html: &str, store: &BlobStore) -> DatabaseCommandResult<Conversao> {
    let imagens = imagens_do_documento(html)
        .map_err(|falha| DatabaseCommandError::validation(falha.to_string()))?;

    let mut decisoes = Vec::with_capacity(imagens.len());
    let mut pendencias = Vec::new();
    let mut convertidas = 0usize;
    let mut ja_eram_referencia = 0usize;

    for no in &imagens {
        match &no.imagem {
            Imagem::Referencia { .. } => {
                ja_eram_referencia += 1;
                decisoes.push(Decisao::Preservar);
            }
            Imagem::SemFonte => decisoes.push(Decisao::Preservar),
            Imagem::NaoReconhecida { motivo } => {
                pendencias.push((no.indice, motivo.clone()));
                decisoes.push(Decisao::Preservar);
            }
            Imagem::InlineValida { mime, bytes } => match publicar(store, bytes) {
                Ok(hash) => {
                    convertidas += 1;
                    decisoes.push(Decisao::Referenciar {
                        hash,
                        mime: mime.clone(),
                    });
                }
                Err(motivo) => {
                    pendencias.push((no.indice, motivo));
                    decisoes.push(Decisao::Preservar);
                }
            },
        }
    }

    // Nada a trocar: devolve a entrada sem passar pelo rewriter.
    //
    // ISTO É ECONOMIA, NÃO GARANTIA, e a distinção foi medida. Eu escrevi
    // aqui, primeiro, que o curto-circuito impedia o documento de "ser
    // normalizado por acidente". A mutação que o desliga **sobreviveu**, então
    // fui procurar um HTML em que o rewriter divergisse: tag sem fechar,
    // maiúsculas, aspas simples, entidades, `<script>` com `<` solto,
    // `<textarea>`, comentário ESI, emoji, `<table>` sem `</tr>`. Dezessete
    // casos adversariais, todos byte a byte idênticos — o gate
    // `o_rewriter_e_fiel_quando_nenhum_handler_dispara` guarda essa medição.
    //
    // Então o que este atalho compra é o parser inteiro não rodar para a
    // maioria esmagadora dos capítulos, que não têm imagem. A promessa de
    // preservação vem do rewriter, não daqui.
    if convertidas == 0 {
        return Ok(Conversao {
            html: html.to_string(),
            convertidas: 0,
            ja_eram_referencia,
            pendencias,
        });
    }

    let novo = reescrever(html, &decisoes)
        .map_err(|falha| DatabaseCommandError::validation(falha.to_string()))?;
    Ok(Conversao {
        html: novo,
        convertidas,
        ja_eram_referencia,
        pendencias,
    })
}

fn publicar(store: &BlobStore, bytes: &[u8]) -> Result<String, String> {
    let hash = store.put(bytes).map_err(|erro| erro.message)?;
    // Confere antes de o documento passar a apontar para lá, como o backfill
    // dos campos diretos faz.
    if !store.verify(&hash).map_err(|erro| erro.message)? {
        return Err("o blob publicado não conferiu na releitura do disco".to_string());
    }
    Ok(hash)
}

/// Injeta o `src` resolvido **para exibição**, sem tocar no documento
/// guardado.
///
/// Existe para o gate 8 do desenho da NH-065: a URL resolvida nunca volta ao
/// banco. Quem chama isto recebe uma cópia para renderizar, e o valor que o
/// banco guarda continua sendo só o hash — que é o que faz o mesmo HTML
/// funcionar no Windows e no Android.
pub fn resolver_para_exibicao(
    html: &str,
    resolver: impl FnMut(&str) -> Option<String>,
) -> Result<String, FalhaDeDocumento> {
    let resolver = RefCell::new(resolver);
    // O resultado é amarrado a uma variável antes do fim do bloco de
    // propósito: os `Settings` do rewriter emprestam `resolver`, e devolver a
    // expressão direto faria o temporário deles ser destruído **depois** do
    // `resolver` que eles emprestam. O compilador recusa, e está certo.
    let resultado = rewrite_str(
        html,
        RewriteStrSettings::new().append_element_content_handler(element!(
            "img",
            |el: &mut Element| {
                let hash = el.get_attribute(ATTR_BLOB).unwrap_or_default();
                if !e_hash_canonico(&hash) {
                    return Ok(());
                }
                if let Some(url) = resolver.borrow_mut()(&hash) {
                    el.set_attribute(ATTR_SRC, &url)?;
                }
                Ok(())
            }
        )),
    );
    resultado.map_err(|erro| FalhaDeDocumento::NaoProcessavel {
        motivo: erro.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::blob_store::hash_dos_bytes;
    use std::path::PathBuf;

    struct Loja {
        raiz: PathBuf,
        store: BlobStore,
    }

    impl Loja {
        fn nova() -> Self {
            let raiz = std::env::temp_dir().join(format!("narrahub-doc-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&raiz).expect("criar raiz");
            Self {
                store: BlobStore::new(&raiz),
                raiz,
            }
        }
    }

    impl Drop for Loja {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.raiz).ok();
        }
    }

    /// Uma `data:` URL de verdade, com os bytes que ela carrega.
    ///
    /// Codifica à mão e confere contra o decodificador de produção: um cenário
    /// que semeia base64 inválida acreditando ser válida prova o contrário do
    /// que pretende.
    fn inline(conteudo: &str) -> (String, Vec<u8>) {
        const ALFABETO: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = conteudo.as_bytes().to_vec();
        let mut texto = String::new();
        for grupo in bytes.chunks(3) {
            let mut acumulado = 0u32;
            for (posicao, byte) in grupo.iter().enumerate() {
                acumulado |= u32::from(*byte) << (16 - 8 * posicao);
            }
            for indice in 0..=grupo.len() {
                texto.push(ALFABETO[((acumulado >> (18 - 6 * indice)) & 0x3F) as usize] as char);
            }
            for _ in grupo.len()..3 {
                texto.push('=');
            }
        }
        assert_eq!(
            crate::domain::data_url::decodificar_base64(&texto)
                .expect("o cenário precisa de base64 válida"),
            bytes
        );
        (format!("data:image/png;base64,{texto}"), bytes)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 1 — HTML sem imagem sai como entrou
    // ═══════════════════════════════════════════════════════════════════════

    /// **Documento sem imagem nenhuma não é tocado.**
    ///
    /// O acervo inteiro do escritor passa por este caminho, e a esmagadora
    /// maioria dos capítulos não tem imagem. Normalizar HTML por acidente —
    /// reordenar atributo, trocar aspas, fechar tag solta — seria reescrever
    /// texto que ninguém pediu para mudar, num campo que o gatilho de revisão
    /// copia a cada salvamento.
    ///
    /// A garantia aqui é forte de propósito: **byte a byte**. Ela vem de
    /// `converter` devolver a entrada sem passar pelo rewriter quando não há
    /// nada a trocar.
    #[test]
    fn documento_sem_imagem_sai_byte_a_byte_igual() {
        let loja = Loja::nova();
        let documentos = [
            "<p>Um parágrafo simples.</p>",
            "<h2>Capítulo 1</h2><p>Ela <strong>abriu</strong> a porta.</p><blockquote>Enfim.</blockquote>",
            "<p>Aspas simples no atributo: <span class='destaque'>assim</span></p>",
            "<p>Tag solta:<br>e uma lista</p><ul><li>um</li><li>dois</li></ul>",
            "<p>Entidades: &amp; &lt; &gt; &nbsp; &#233;</p>",
            "<!-- um comentário --><p>depois do comentário</p>",
            "<p>Atributo desconhecido: <span data-coisa-futura=\"7\" hidden>x</span></p>",
            "texto puro sem tag nenhuma",
            "",
        ];

        for documento in documentos {
            let conversao = converter(documento, &loja.store).expect("converter");
            assert_eq!(
                conversao.html, documento,
                "o documento mudou e não havia imagem para mudar"
            );
            assert!(!conversao.mudou());
            assert_eq!(conversao.convertidas, 0);
            assert!(conversao.pendencias.is_empty());
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 2 — data URL válida vira referência
    // ═══════════════════════════════════════════════════════════════════════

    /// **`<img>` com data URL válida vira referência de blob.**
    ///
    /// E o que sai não carrega nada do que é proibido persistir: nem `data:`,
    /// nem base64, nem caminho, nem `blob:` de navegador. Só o hash, que é a
    /// identidade portátil — o mesmo HTML tem que funcionar no Windows e no
    /// Android.
    #[test]
    fn img_com_data_url_valida_vira_referencia_de_blob() {
        let loja = Loja::nova();
        let (url, bytes) = inline("os-bytes-do-rosto");
        let documento = format!("<p>Antes</p><img src=\"{url}\" alt=\"rosto.png\"><p>Depois</p>");

        let conversao = converter(&documento, &loja.store).expect("converter");
        assert_eq!(conversao.convertidas, 1);
        assert!(conversao.pendencias.is_empty());

        let esperado = hash_dos_bytes(&bytes);
        assert!(
            conversao
                .html
                .contains(&format!("{ATTR_BLOB}=\"{esperado}\"")),
            "faltou a referência: {}",
            conversao.html
        );
        assert!(conversao
            .html
            .contains(&format!("{ATTR_MIME}=\"image/png\"")));

        for proibido in ["data:", "base64", "src=", "blob:", "file:", "C:\\"] {
            assert!(
                !conversao.html.contains(proibido),
                "o HTML persistido carrega {proibido:?}: {}",
                conversao.html
            );
        }

        // E o texto ao redor continua intacto.
        assert!(conversao.html.contains("<p>Antes</p>"));
        assert!(conversao.html.contains("<p>Depois</p>"));

        // O blob está no disco, com os bytes do escritor.
        assert_eq!(loja.store.read(&esperado).expect("ler o blob"), bytes);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 3 — atributos sobrevivem, inclusive os desconhecidos
    // ═══════════════════════════════════════════════════════════════════════

    /// **`alt`, `title`, dimensões e atributo desconhecido sobrevivem.**
    ///
    /// É o argumento que descartou reconstruir o elemento a partir de um
    /// modelo simplificado: um campo que a extensão do editor ganhar amanhã
    /// seria jogado fora hoje, em silêncio, no acervo inteiro. Só os atributos
    /// do asset mudam.
    #[test]
    fn atributos_conhecidos_e_desconhecidos_sobrevivem() {
        let loja = Loja::nova();
        let (url, _) = inline("imagem");
        let documento = format!(
            "<img src=\"{url}\" alt=\"o rosto dela\" title=\"Retrato\" width=\"320\" \
             height=\"240\" loading=\"lazy\" class=\"nh-img\" \
             data-narrahub-futuro=\"ainda-nao-existe\" draggable=\"true\">"
        );

        let conversao = converter(&documento, &loja.store).expect("converter");
        assert_eq!(conversao.convertidas, 1);

        for sobrevivente in [
            "alt=\"o rosto dela\"",
            "title=\"Retrato\"",
            "width=\"320\"",
            "height=\"240\"",
            "loading=\"lazy\"",
            "class=\"nh-img\"",
            "data-narrahub-futuro=\"ainda-nao-existe\"",
            "draggable=\"true\"",
        ] {
            assert!(
                conversao.html.contains(sobrevivente),
                "perdeu {sobrevivente}: {}",
                conversao.html
            );
        }
        assert!(!conversao.html.contains("src="), "{}", conversao.html);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 4 — várias imagens
    // ═══════════════════════════════════════════════════════════════════════

    /// **Três imagens, três referências — e a repetida é um arquivo só.**
    ///
    /// A deduplicação atravessa o documento pelo mesmo motivo que atravessa as
    /// superfícies: o endereço é o conteúdo, e o store não sabe de onde os
    /// bytes vieram.
    #[test]
    fn multiplas_imagens_sao_migradas_e_a_repetida_dedupa() {
        let loja = Loja::nova();
        let (uma, bytes_uma) = inline("primeira");
        let (outra, bytes_outra) = inline("segunda");
        let documento = format!(
            "<img src=\"{uma}\" alt=\"a\"><p>meio</p><img src=\"{outra}\" alt=\"b\">\
             <p>fim</p><img src=\"{uma}\" alt=\"a de novo\">"
        );

        let conversao = converter(&documento, &loja.store).expect("converter");
        assert_eq!(conversao.convertidas, 3, "as três precisam ser tratadas");

        let hash_uma = hash_dos_bytes(&bytes_uma);
        let hash_outra = hash_dos_bytes(&bytes_outra);
        assert_eq!(
            conversao.html.matches(&hash_uma).count(),
            2,
            "a imagem repetida aparece nas duas posições"
        );
        assert_eq!(conversao.html.matches(&hash_outra).count(), 1);

        // Duas imagens distintas, dois blobs — não três.
        let hashes = hashes_do_documento(&conversao.html).expect("ler os hashes");
        assert_eq!(hashes.len(), 2, "conjunto único: {hashes:?}");
        assert!(loja.store.verify(&hash_uma).expect("integridade"));
        assert!(loja.store.verify(&hash_outra).expect("integridade"));

        // E a ordem foi respeitada: cada alt continua com a sua imagem.
        let posicao_a = conversao.html.find("alt=\"a\"").expect("a");
        let posicao_b = conversao.html.find("alt=\"b\"").expect("b");
        let posicao_c = conversao.html.find("alt=\"a de novo\"").expect("c");
        assert!(posicao_a < posicao_b && posicao_b < posicao_c);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 5 — legado inválido não é destruído
    // ═══════════════════════════════════════════════════════════════════════

    /// **Data URL inválida é preservada, com pendência.**
    ///
    /// Falha de migração não transforma dado desconhecido em ausência. E o
    /// vizinho válido migra do mesmo jeito: uma imagem que não abre não pode
    /// travar o documento inteiro.
    #[test]
    fn data_url_invalida_nao_e_destruida_e_o_vizinho_migra() {
        let loja = Loja::nova();
        let (boa, bytes) = inline("essa-abre");
        let quebrada = "data:image/png;base64,!!!nao-abre!!!";
        let documento =
            format!("<img src=\"{quebrada}\" alt=\"ruim\"><img src=\"{boa}\" alt=\"boa\">");

        let conversao = converter(&documento, &loja.store).expect("converter");
        assert_eq!(conversao.convertidas, 1, "só a boa migra");
        assert_eq!(conversao.pendencias.len(), 1);
        assert_eq!(conversao.pendencias[0].0, 0, "a pendência é da primeira");
        assert!(
            !conversao.pendencias[0].1.is_empty() && !conversao.pendencias[0].1.contains("!!!"),
            "o motivo não pode carregar o valor: {:?}",
            conversao.pendencias[0].1
        );

        assert!(
            conversao.html.contains(quebrada),
            "o valor original tinha que ficar exatamente onde estava: {}",
            conversao.html
        );
        assert!(conversao.html.contains(&hash_dos_bytes(&bytes)));
    }

    /// URL externa e caminho local também são preservados, sem download.
    #[test]
    fn url_externa_e_caminho_local_nao_sao_baixados() {
        let loja = Loja::nova();
        for valor in [
            "https://cdn.exemplo.com/capa.png",
            "C:\\Users\\alguem\\capa.png",
            "/home/alguem/capa.png",
            "blob:http://localhost/9f2c",
        ] {
            let documento = format!("<img src=\"{valor}\" alt=\"x\">");
            let conversao = converter(&documento, &loja.store).expect("converter");
            assert_eq!(conversao.convertidas, 0, "{valor} não devia migrar");
            assert_eq!(conversao.pendencias.len(), 1);
            assert_eq!(
                conversao.html, documento,
                "nada podia mudar em {valor}: {}",
                conversao.html
            );
        }
    }

    /// Referência de blob torta no documento vira pendência, e não caminho.
    ///
    /// A coluna é texto e o documento vem de fora — banco importado, versão
    /// antiga, arquivo adulterado. Um `data-narrahub-blob` que não é hash
    /// canônico não pode virar `join` de caminho.
    #[test]
    fn referencia_torta_no_documento_vira_pendencia() {
        let loja = Loja::nova();
        let documento = format!("<img {ATTR_BLOB}=\"../../../etc/passwd\" alt=\"x\">");

        let conversao = converter(&documento, &loja.store).expect("converter");
        assert_eq!(conversao.convertidas, 0);
        assert_eq!(conversao.pendencias.len(), 1);
        assert_eq!(conversao.html, documento, "preservado como estava");
        assert!(
            hashes_do_documento(&documento).expect("ler").is_empty(),
            "e não entra no manifesto de blobs"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 6 — texto escapado não é elemento
    // ═══════════════════════════════════════════════════════════════════════

    /// **`&lt;img ...&gt;` escrito pelo escritor é texto, não elemento.**
    ///
    /// É o gate que separa validação estrutural de busca por substring. Um
    /// personagem pode estar explicando o que é uma data URL, e um
    /// `contains("data:")` — ou um varredor artesanal de `<img` — trataria a
    /// prosa dele como mídia inline: registraria pendência falsa, bloquearia o
    /// bootstrap, e no pior caso reescreveria o texto.
    #[test]
    fn img_escapado_no_texto_nao_e_tratado_como_elemento() {
        let loja = Loja::nova();
        let documentos = [
            "<p>Escreva &lt;img src=\"data:image/png;base64,AAAA\"&gt; para embutir.</p>",
            "<p>O código dela era <code>&lt;img src=\"data:image/gif;base64,R0lGOD\"&gt;</code>.</p>",
            "<!-- <img src=\"data:image/png;base64,AAAA\"> dentro de comentário -->",
            "<p title=\"um &lt;img src=data:x&gt; no atributo\">texto</p>",
            "<p>Ele falava de data:image/png sem parar.</p>",
        ];

        for documento in documentos {
            assert!(
                !tem_midia_inline(documento).expect("varrer"),
                "o guard viu mídia onde havia texto: {documento}"
            );
            let imagens = imagens_do_documento(documento).expect("varrer");
            assert!(
                imagens.is_empty(),
                "encontrou {} imagem(ns) em texto escapado: {documento}",
                imagens.len()
            );
            let conversao = converter(documento, &loja.store).expect("converter");
            assert_eq!(conversao.html, documento, "reescreveu prosa do escritor");
            assert!(conversao.pendencias.is_empty());
        }
    }

    /// E o guard **vê** mídia de verdade — senão o de cima passaria por vácuo.
    #[test]
    fn o_guard_ve_midia_inline_de_verdade() {
        let (url, _) = inline("imagem");
        assert!(tem_midia_inline(&format!("<p>a</p><img src=\"{url}\">")).expect("varrer"));
        assert!(
            tem_midia_inline("<img src=\"https://exemplo.com/x.png\">").expect("varrer"),
            "legado não reconhecido também é mídia que não está no contrato novo"
        );
        assert!(
            !tem_midia_inline(&format!("<img {ATTR_BLOB}=\"{}\">", hash_dos_bytes(b"x")))
                .expect("varrer"),
            "documento já convertido não tem mídia inline"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 7 — o Tiptap consegue reler
    // ═══════════════════════════════════════════════════════════════════════

    /// **O HTML blob-safe é relegível como imagem.**
    ///
    /// A parte que o Rust consegue provar: o elemento continua sendo um `img`,
    /// com a referência e o MIME recuperáveis, e uma segunda conversão é
    /// idempotente.
    ///
    /// A outra metade depende da extensão do editor, e ela **não** está
    /// pronta: `parseHTML() { return [{ tag: 'img[src]' }] }` exige `src`, e o
    /// documento novo não tem. Enquanto isso não virar `tag: 'img'` com os
    /// atributos de dado declarados, o Tiptap descarta o elemento ao carregar.
    /// Registrado na `NH-064`; o gate do lado JS vem com a fatia do editor.
    #[test]
    fn o_html_blob_safe_e_relegivel_e_a_conversao_e_idempotente() {
        let loja = Loja::nova();
        let (url, bytes) = inline("imagem");
        let documento = format!("<p>a</p><img src=\"{url}\" alt=\"rosto.png\"><p>b</p>");

        let primeira = converter(&documento, &loja.store).expect("primeira");
        let hash = hash_dos_bytes(&bytes);

        // Relido como imagem, com referência e MIME no lugar.
        let imagens = imagens_do_documento(&primeira.html).expect("reler");
        assert_eq!(imagens.len(), 1);
        assert_eq!(
            imagens[0].imagem,
            Imagem::Referencia {
                hash: hash.clone(),
                mime: "image/png".to_string()
            }
        );

        // E converter de novo não muda nada.
        let segunda = converter(&primeira.html, &loja.store).expect("segunda");
        assert_eq!(segunda.html, primeira.html, "a conversão não é idempotente");
        assert_eq!(segunda.convertidas, 0);
        assert_eq!(segunda.ja_eram_referencia, 1);
        assert!(segunda.pendencias.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Gate 8 — a URL resolvida não volta ao banco
    // ═══════════════════════════════════════════════════════════════════════

    /// **A resolução para exibição não grava nada de volta.**
    ///
    /// O documento guardado tem só o hash. A URL local que a tela usa é
    /// produzida em tempo de execução, e é específica daquela máquina — gravá-la
    /// quebraria o acervo restaurado noutro aparelho, que é exatamente o que o
    /// item 4 do contrato proíbe.
    ///
    /// O gate mede as duas coisas: a cópia para exibir tem a URL, e o valor
    /// original continua sem ela.
    #[test]
    fn a_url_resolvida_nao_volta_para_o_documento_guardado() {
        let loja = Loja::nova();
        let (url, bytes) = inline("imagem");
        let conversao =
            converter(&format!("<img src=\"{url}\" alt=\"x\">"), &loja.store).expect("converter");
        let guardado = conversao.html.clone();
        let hash = hash_dos_bytes(&bytes);

        let pedidos = std::cell::RefCell::new(Vec::new());
        let para_exibir = resolver_para_exibicao(&guardado, |pedido| {
            pedidos.borrow_mut().push(pedido.to_string());
            Some(format!("nh-asset://local/{pedido}"))
        })
        .expect("resolver");

        assert_eq!(pedidos.into_inner(), vec![hash.clone()]);
        assert!(
            para_exibir.contains(&format!("src=\"nh-asset://local/{hash}\"")),
            "a cópia de exibição precisa da URL: {para_exibir}"
        );
        assert!(
            para_exibir.contains(ATTR_BLOB),
            "e a referência continua lá, para a próxima renderização"
        );

        // O valor que o banco guarda não mudou.
        assert!(!guardado.contains("nh-asset://"));
        assert!(!guardado.contains("src="));
        assert_eq!(
            converter(&guardado, &loja.store).expect("reconverter").html,
            guardado
        );
    }

    /// Blob ausente: a tela fica sem `src`, e não com um caminho inventado.
    ///
    /// Item 10 do contrato — referência e presença física são estados
    /// distintos. No incremental a linha chega antes do arquivo.
    #[test]
    fn blob_ausente_nao_ganha_src_inventado() {
        let hash = hash_dos_bytes(b"nunca-publicado");
        let guardado = format!("<img {ATTR_BLOB}=\"{hash}\" alt=\"x\">");

        let para_exibir = resolver_para_exibicao(&guardado, |_| None).expect("resolver");
        assert_eq!(
            para_exibir, guardado,
            "sem blob, nada é injetado: {para_exibir}"
        );
        assert!(!para_exibir.contains("src="));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // O transformador é um só
    // ═══════════════════════════════════════════════════════════════════════

    /// **As quatro superfícies passam pela mesma função.**
    ///
    /// O gate é sobre arquitetura: quatro parsers seriam quatro
    /// interpretações do mesmo formato, envelhecendo em quatro velocidades. O
    /// conteúdo de capítulo, de revisão, dos dois lados de um conflito e dos
    /// dois lados de uma contribuição é o mesmo HTML, e sai igual pelos
    /// quatro caminhos.
    #[test]
    fn o_mesmo_documento_sai_igual_pelas_quatro_superficies() {
        let loja = Loja::nova();
        let (url, _) = inline("a-mesma-imagem-nas-quatro");
        let documento = format!("<p>texto</p><img src=\"{url}\" alt=\"x\">");

        // Os quatro chamadores da fatia seguinte, aqui representados pelo que
        // de fato compartilham: a chamada.
        let resultados: Vec<String> = [
            "chapters.content",
            "chapter_revisions.content",
            "sync_conflicts.local_value",
            "collaboration_contributions.proposed_value",
        ]
        .iter()
        .map(|_| converter(&documento, &loja.store).expect("converter").html)
        .collect();

        assert_eq!(
            resultados
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            1,
            "as quatro superfícies precisam produzir o mesmo HTML: {resultados:?}"
        );
    }

    /// **O rewriter é fiel quando nenhum handler dispara.**
    ///
    /// É a medição em que toda a promessa de preservação desta etapa se apoia:
    /// "o `lol_html` repassa verbatim tudo o que não casa com o seletor". Eu
    /// afirmei isso antes de medir, e a mutação que desligou o curto-circuito
    /// de `converter` sobreviveu justamente porque a afirmação é verdadeira.
    ///
    /// Os casos são adversariais de propósito — são as formas em que um parser
    /// que constrói árvore normalizaria a entrada. Se um dia uma versão nova
    /// do crate passar a fechar tag, baixar caixa ou trocar aspas, este gate
    /// reprova antes de o acervo de alguém ser reescrito.
    #[test]
    fn o_rewriter_e_fiel_quando_nenhum_handler_dispara() {
        let candidatos = [
            "<p>a",
            "<p>a</p",
            "<P CLASS=X>maiusculas</P>",
            "<p class = 'aspas simples' >x</p>",
            "<p>&nbsp;&#233;&amp;</p>",
            "<!--esi <esi:vars>$(HTTP_HOST)</esi:vars> -->",
            "<div><span>sem fechar",
            "<p>tag desconhecida <coisa-nova atributo>x</coisa-nova></p>",
            "<script>if (a<b) { }</script>",
            "<textarea><p>nao e tag</p></textarea>",
            "<p>emoji \u{1F600} e acento ção</p>",
            "<table><tr><td>a</table>",
            "<p>espaço   interno   preservado</p>",
        ];

        let mut divergentes = Vec::new();
        for entrada in candidatos {
            // Sem decisão nenhuma: nenhum `<img>` é tocado.
            let saida = reescrever(entrada, &[]).expect("reescrever");
            if saida != entrada {
                divergentes.push(format!("{entrada:?} → {saida:?}"));
            }
        }
        assert!(
            divergentes.is_empty(),
            "o rewriter deixou de ser fiel, e a promessa de preservar o HTML ao redor cai \
             com isso:\n{}",
            divergentes.join("\n")
        );
    }
}
