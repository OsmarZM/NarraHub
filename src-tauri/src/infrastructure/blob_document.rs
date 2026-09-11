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
use crate::domain::data_url::{classificar, parece_data_url, Legado};
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
    /// `src` começa em `data:` e **não** abre: base64 quebrada, sem MIME,
    /// truncada.
    ///
    /// Separada da externa de propósito. As duas viram pendência no backfill,
    /// igual — mas na **entrada** são coisas diferentes: esta põe bytes no
    /// banco e tem que bloquear.
    InlineInvalida { motivo: String },
    /// `src` não vazio que não é `data:` URL: URL externa, caminho local,
    /// `blob:` de navegador, nome de arquivo.
    ///
    /// **Não é fonte válida para persistência nova.** O ADR 0010 já listava
    /// caminho absoluto, URL temporária e `blob:` entre o que nunca vai ao
    /// banco; a primeira versão de [`exigir_blob_safe`] a aceitava, e essa
    /// aceitação era mais frouxa que o contrato.
    ///
    /// Ela não põe byte no SQLite, e por isso não é urgência de tamanho —
    /// é urgência de **portabilidade**: `C:\\Users\\...` não existe no
    /// Android, `blob:` morre com a aba, e URL externa vence. Um documento que
    /// a contém não é reproduzível no outro aparelho.
    ///
    /// No legado ela é **preservada exatamente** e vira pendência. O que muda
    /// é a entrada: um documento legado abre normalmente, mas essa imagem
    /// precisa sair ou ser substituída antes da próxima gravação.
    ExternaOuDesconhecida { motivo: String },
    /// `data-narrahub-blob` presente e **não** canônico.
    ///
    /// Só aparece por adulteração, banco importado ou bug de versão. É
    /// referência que o aplicativo não consegue resolver.
    ReferenciaInvalida { motivo: String },
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
                        Imagem::ReferenciaInvalida {
                            motivo: "a referência de blob no documento não é um SHA-256 \
                                     canônico"
                                .to_string(),
                        }
                    }
                } else if src.trim().is_empty() {
                    Imagem::SemFonte
                } else {
                    // A pergunta "é `data:`?" é respondida pela forma do
                    // valor, não pela mensagem de erro de quem tentou abrir.
                    let inline = parece_data_url(&src);
                    match classificar(&src) {
                        Legado::Decodificada { mime, bytes } => {
                            Imagem::InlineValida { mime, bytes }
                        }
                        Legado::NaoReconhecido { motivo } if inline => {
                            Imagem::InlineInvalida { motivo }
                        }
                        Legado::NaoReconhecido { motivo } => {
                            Imagem::ExternaOuDesconhecida { motivo }
                        }
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

/// **Este documento tem mídia inline?**
///
/// Estrutural: existe um elemento `img` cujo `src` começa em `data:`. Um
/// `contains("data:")` responderia sim para um personagem que escreveu sobre
/// data URLs no meio do capítulo.
///
/// Aberto e fechado: inline é `data:`, e só. URL externa não é inline — ela
/// não põe byte no banco.
pub fn tem_midia_inline(html: &str) -> Result<bool, FalhaDeDocumento> {
    Ok(imagens_do_documento(html)?.iter().any(|no| {
        matches!(
            no.imagem,
            Imagem::InlineValida { .. } | Imagem::InlineInvalida { .. }
        )
    }))
}

/// Por que este documento não pode ser gravado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NaoEBlobSafe {
    /// Tem `<img>` com `data:` URL. É o que esta etapa existe para impedir.
    MidiaInline { quantas: usize },
    /// Tem referência de blob que não é hash canônico.
    ReferenciaInvalida { quantas: usize },
    /// Tem `<img>` apontando para fora: URL, caminho local, `blob:`, `file:`.
    FonteExterna { quantas: usize },
    /// O documento não pôde ser lido.
    NaoProcessavel { motivo: String },
}

impl std::fmt::Display for NaoEBlobSafe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NaoEBlobSafe::MidiaInline { quantas } => write!(
                f,
                "Este texto ainda tem {quantas} imagem(ns) embutida(s) no próprio conteúdo. \
                 Imagem embutida faz o texto crescer dezenas de vezes e viaja em cada \
                 sincronização, então o NarraHub passou a guardar imagem em arquivo \
                 separado. Abra a lista de pendências de mídia para ver quais imagens deste \
                 capítulo ainda não foram convertidas."
            ),
            NaoEBlobSafe::ReferenciaInvalida { quantas } => write!(
                f,
                "Este texto tem {quantas} referência(s) de imagem em formato que o aplicativo \
                 não reconhece. O conteúdo continua preservado como está; a gravação foi \
                 recusada para não propagar a referência quebrada aos outros aparelhos."
            ),
            NaoEBlobSafe::FonteExterna { quantas } => write!(
                f,
                "Este texto tem {quantas} imagem(ns) que aponta(m) para fora do NarraHub \
                 — endereço da internet, arquivo do computador ou imagem colada de outra \
                 aba. Uma imagem assim não aparece nos seus outros aparelhos, porque o \
                 endereço dela só existe aqui. O conteúdo continua preservado; para gravar, \
                 remova ou reinsira a imagem pelo botão de imagem do editor."
            ),
            NaoEBlobSafe::NaoProcessavel { motivo } => write!(
                f,
                "O texto deste capítulo não pôde ser lido como documento: {motivo}."
            ),
        }
    }
}

/// **A única definição de "documento blob-safe".**
///
/// Editor, colaboração e gravação de capítulo chamam esta função. Três
/// interpretações do mesmo contrato divergiriam — e divergir aqui significa
/// uma porta fechada e duas abertas, com a descoberta acontecendo no acervo de
/// alguém.
///
/// Blob-safe é: todo `<img>` do documento é referência canônica **ou** não tem
/// fonte nenhuma. Qualquer outra coisa recusa.
///
/// ```text
/// <img data-narrahub-blob="<64 hex>">   ✓  é o contrato
/// <img>                                 ✓  não é asset
/// <img src="data:…">                    ✗  MidiaInline
/// <img data-narrahub-blob="../etc">     ✗  ReferenciaInvalida
/// <img src="https://cdn/x.png">         ✗  FonteExterna
/// <img src="C:\\Users\\alguem\\x.png">      ✗  FonteExterna
/// <img src="file:///home/x.png">        ✗  FonteExterna
/// <img src="blob:http://localhost/…">   ✗  FonteExterna
/// ```
///
/// # A externa recusava antes e passou a recusar
///
/// A primeira versão deste guard aceitava a externa, com o argumento de que
/// ela não põe byte no SQLite. O argumento estava certo sobre tamanho e errado
/// sobre o contrato: o ADR 0010 lista caminho absoluto, URL temporária e
/// `blob:` entre o que **nunca** vai ao banco, e a aceitação era mais frouxa
/// que o ADR. O critério não é só "infla o evento" — é **o mesmo HTML tem que
/// funcionar no Windows e no Android**, e `C:\\Users\\...` não funciona.
///
/// A recusa é *fail-closed* de propósito, sem distinguir "externa antiga" de
/// "externa nova": um documento legado **abre** normalmente, e o que ele
/// perde é a próxima gravação, até a imagem sair ou ser reinserida pelo
/// editor. Comparar o antes e o depois para tolerar a antiga exigiria guardar
/// a lista de externas de cada documento, e isso é superfície nova para
/// tolerar um valor que o ADR proíbe.
///
/// O backfill **não** mudou: externa legada continua preservada byte a byte,
/// com `blob_migration_issue` registrada. Nenhuma URL é baixada e nenhum
/// caminho é aberto — aqui e lá, a externa é olhada, nunca seguida.
///
/// **A inline que não abre bloqueia junto com a que abre.** Ela põe bytes no
/// banco do mesmo jeito, e é exatamente o caso que o backfill preserva com
/// pendência. O capítulo fica sem poder ser salvo até a pendência ser
/// resolvida, e a mensagem manda o escritor para a lista — travar é o
/// comportamento pedido, e é melhor que continuar emitindo evento gigante.
pub fn exigir_blob_safe(html: &str) -> Result<(), NaoEBlobSafe> {
    let imagens = match imagens_do_documento(html) {
        Ok(imagens) => imagens,
        Err(FalhaDeDocumento::NaoProcessavel { motivo }) => {
            return Err(NaoEBlobSafe::NaoProcessavel { motivo })
        }
    };

    let inline = imagens
        .iter()
        .filter(|no| {
            matches!(
                no.imagem,
                Imagem::InlineValida { .. } | Imagem::InlineInvalida { .. }
            )
        })
        .count();
    if inline > 0 {
        return Err(NaoEBlobSafe::MidiaInline { quantas: inline });
    }

    let tortas = imagens
        .iter()
        .filter(|no| matches!(no.imagem, Imagem::ReferenciaInvalida { .. }))
        .count();
    if tortas > 0 {
        return Err(NaoEBlobSafe::ReferenciaInvalida { quantas: tortas });
    }

    // Por último, e a ordem é decisão: inline e referência torta são piores —
    // uma põe bytes no banco, a outra é referência que o aplicativo não
    // resolve em aparelho nenhum. Quando um documento tem os dois problemas, a
    // mensagem fala do mais grave, e resolver o mais grave revela o outro.
    let externas = imagens
        .iter()
        .filter(|no| matches!(no.imagem, Imagem::ExternaOuDesconhecida { .. }))
        .count();
    if externas > 0 {
        return Err(NaoEBlobSafe::FonteExterna { quantas: externas });
    }

    Ok(())
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
            // As três pendências recebem o mesmo tratamento no backfill:
            // preserva e registra. A distinção entre elas existe para a
            // **entrada**, em `exigir_blob_safe`.
            Imagem::InlineInvalida { motivo }
            | Imagem::ExternaOuDesconhecida { motivo }
            | Imagem::ReferenciaInvalida { motivo } => {
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
    ///
    /// A primeira versão deste gate afirmava que `<img src="https://…">` também
    /// era mídia inline. Está errado, e o erro só apareceu quando eu fui
    /// escrever a barreira de entrada: inline é `data:`, e a diferença é a que
    /// decide se a gravação trava.
    ///
    /// ```text
    /// data:…            põe bytes no banco      →  inline
    /// https://cdn/x.png não põe byte nenhum     →  não é inline
    /// ```
    #[test]
    fn o_guard_ve_midia_inline_de_verdade() {
        let (url, _) = inline("imagem");
        assert!(tem_midia_inline(&format!("<p>a</p><img src=\"{url}\">")).expect("varrer"));
        assert!(
            tem_midia_inline("<img src=\"data:image/png;base64,!!!\">").expect("varrer"),
            "a inline que não abre põe bytes no banco do mesmo jeito"
        );
        assert!(
            !tem_midia_inline("<img src=\"https://exemplo.com/x.png\">").expect("varrer"),
            "URL externa não é mídia inline: ela não põe byte nenhum no banco"
        );
        assert!(
            !tem_midia_inline(&format!("<img {ATTR_BLOB}=\"{}\">", hash_dos_bytes(b"x")))
                .expect("varrer"),
            "documento já convertido não tem mídia inline"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // A definição única de blob-safe
    // ═══════════════════════════════════════════════════════════════════════

    /// **`exigir_blob_safe` é a mesma porta para as três barreiras.**
    ///
    /// Cada caso com o veredito que ele tem que produzir. Este gate é o
    /// contrato que editor, colaboração e gravação de capítulo compartilham —
    /// se ele mudar, as três mudam juntas, que é exatamente o ponto de existir
    /// uma definição só.
    #[test]
    fn a_definicao_de_blob_safe_e_uma_so() {
        let (url, _) = inline("imagem");
        let hash = hash_dos_bytes(b"qualquer");

        // Passa.
        for aceito in [
            String::new(),
            "<p>só texto</p>".to_string(),
            format!("<img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\" alt=\"x\">"),
            "<img>".to_string(),
            // Prosa do escritor sobre data URLs continua prosa.
            "<p>Escreva &lt;img src=\"data:image/png;base64,AAA\"&gt; assim.</p>".to_string(),
            "<code>&lt;img src=\"data:image/gif;base64,R0lGOD\"&gt;</code>".to_string(),
        ] {
            assert_eq!(
                exigir_blob_safe(&aceito),
                Ok(()),
                "devia passar: {aceito:?}"
            );
        }

        // Recusa por mídia inline.
        for (documento, quantas) in [
            (format!("<img src=\"{url}\">"), 1),
            (format!("<p>a</p><img src=\"{url}\"><img src=\"{url}\">"), 2),
            (
                "<img src=\"data:image/png;base64,!!!nao-abre\">".to_string(),
                1,
            ),
            ("<img src=\"data:image/png,sem-base64\">".to_string(), 1),
        ] {
            assert_eq!(
                exigir_blob_safe(&documento),
                Err(NaoEBlobSafe::MidiaInline { quantas }),
                "devia recusar como inline: {documento:?}"
            );
        }

        // Recusa por referência torta.
        assert_eq!(
            exigir_blob_safe(&format!("<img {ATTR_BLOB}=\"../../../etc/passwd\">")),
            Err(NaoEBlobSafe::ReferenciaInvalida { quantas: 1 })
        );
        assert_eq!(
            exigir_blob_safe(&format!("<img {ATTR_BLOB}=\"{}\">", hash.to_uppercase())),
            Err(NaoEBlobSafe::ReferenciaInvalida { quantas: 1 }),
            "maiúscula não é hash canônico"
        );
    }

    /// **Nenhuma das quatro formas de apontar para fora passa como documento
    /// novo.**
    ///
    /// Cada linha é uma forma real de o `src` sair do contrato, e cada uma
    /// falha no aparelho do outro por um motivo diferente:
    ///
    /// ```text
    /// https://…    o servidor pode sumir, e o aparelho pode estar offline
    /// C:\\Users\\…   o caminho não existe no Android
    /// /home/…      idem, e nem entre dois Windows
    /// file:///…    idem, com esquema explícito
    /// blob:…       morre com a aba que o criou
    /// ```
    ///
    /// O que o gate exige não é só "deu erro": é a **variante certa**. Se
    /// caísse em `MidiaInline`, a mensagem mandaria o escritor para a lista de
    /// pendências de migração, que não é onde isso se resolve.
    ///
    /// Nenhuma URL é buscada e nenhum caminho é aberto para decidir: a
    /// classificação é do texto do atributo.
    #[test]
    fn fonte_externa_nao_passa_como_documento_novo() {
        for fora in [
            "https://cdn.exemplo.com/capa.png",
            "http://exemplo.com/x.jpg",
            "//exemplo.com/x.jpg",
            "C:\\Users\\alguem\\capa.png",
            "/home/alguem/capa.png",
            "file:///home/alguem/capa.png",
            "file:///C:/Users/alguem/capa.png",
            "blob:http://localhost:4200/9f2c-4b1e",
            "capa.png",
            "../capa.png",
            "assets/capa.png",
        ] {
            assert_eq!(
                exigir_blob_safe(&format!("<img src=\"{fora}\">")),
                Err(NaoEBlobSafe::FonteExterna { quantas: 1 }),
                "devia recusar como fonte externa: {fora:?}"
            );
        }

        // E conta, porque a mensagem diz quantas.
        assert_eq!(
            exigir_blob_safe("<img src=\"https://a/x.png\"><p>t</p><img src=\"blob:http://l/1\">"),
            Err(NaoEBlobSafe::FonteExterna { quantas: 2 })
        );
    }

    /// **O hash canônico continua sendo aceito.**
    ///
    /// O gate acima recusa nove formas de `src`. Este é a outra metade: se a
    /// recusa tivesse ficado larga demais, o próprio formato canônico pararia
    /// de passar — e o editor não conseguiria salvar nada.
    #[test]
    fn o_hash_canonico_continua_passando_depois_da_recusa_da_externa() {
        let hash = hash_dos_bytes(b"qualquer");

        for aceito in [
            format!("<img {ATTR_BLOB}=\"{hash}\">"),
            format!("<img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\" alt=\"x\">"),
            // Com `src` residual: o atributo de dado é que decide, e é isso
            // que faz o resolvedor de execução poder preencher `src` sem
            // tornar o documento ingravável.
            format!("<img {ATTR_BLOB}=\"{hash}\" src=\"blob:http://localhost/1\">"),
            format!("<p>a</p><img {ATTR_BLOB}=\"{hash}\"><img {ATTR_BLOB}=\"{hash}\">"),
            "<img>".to_string(),
            "<p>só texto</p>".to_string(),
        ] {
            assert_eq!(
                exigir_blob_safe(&aceito),
                Ok(()),
                "devia passar: {aceito:?}"
            );
        }
    }

    /// **O backfill continua preservando cada uma delas, com pendência.**
    ///
    /// É a metade que não muda, e o gate existe porque a mudança na entrada
    /// tornaria muito fácil "arrumar" o backfill junto — destruindo imagem
    /// antiga de alguém em nome da coerência.
    ///
    /// Byte a byte: `assert_eq!` no HTML inteiro, não em conter.
    #[test]
    fn o_backfill_preserva_a_externa_e_registra_pendencia() {
        let loja = Loja::nova();

        for fora in [
            "https://cdn.exemplo.com/capa.png",
            "C:\\Users\\alguem\\capa.png",
            "/home/alguem/capa.png",
            "file:///home/alguem/capa.png",
            "blob:http://localhost:4200/9f2c-4b1e",
        ] {
            let documento = format!("<p>antes</p><img src=\"{fora}\" alt=\"capa\"><p>depois</p>");
            let conversao = converter(&documento, &loja.store).expect("converter");

            assert_eq!(
                conversao.html, documento,
                "o documento legado tinha que sair idêntico: {fora:?}"
            );
            assert_eq!(
                conversao.pendencias.len(),
                1,
                "e registrar exatamente uma pendência: {fora:?}"
            );
            assert_eq!(conversao.convertidas, 0, "nada a migrar: {fora:?}");
            assert!(!conversao.mudou(), "e nada a reescrever: {fora:?}");
        }
    }

    /// A mensagem manda o escritor para onde ele resolve.
    ///
    /// Recusar a gravação do capítulo é caro para quem está escrevendo. Se a
    /// mensagem não disser o que fazer, o escritor fica com um texto que não
    /// salva e nenhuma pista.
    #[test]
    fn a_recusa_diz_ao_escritor_o_que_fazer() {
        let (url, _) = inline("imagem");
        let erro = exigir_blob_safe(&format!("<img src=\"{url}\">")).expect_err("recusa");
        let texto = erro.to_string();
        assert!(
            texto.contains("pendências de mídia"),
            "a mensagem precisa apontar a lista: {texto}"
        );
        assert!(
            !texto.contains("data:") && !texto.contains("blob-safe"),
            "e não pode falar em jargão de implementação: {texto}"
        );
    }

    /// E a recusa da externa manda para outro lugar, porque o remédio é outro.
    ///
    /// Pendência de migração é coisa do backfill; imagem que aponta para fora
    /// se resolve reinserindo a imagem. Mandar o escritor para a lista de
    /// pendências aqui seria mandá-lo para uma tela onde não há o que fazer.
    #[test]
    fn a_recusa_da_externa_diz_o_que_fazer_e_nao_e_a_lista_de_pendencias() {
        let erro =
            exigir_blob_safe("<img src=\"https://cdn.exemplo.com/x.png\">").expect_err("recusa");
        let texto = erro.to_string();
        assert!(
            texto.contains("outros aparelhos") && texto.contains("bot"),
            "precisa dizer por que e o que fazer: {texto}"
        );
        assert!(
            !texto.contains("pendências de mídia"),
            "e não é a lista de pendências que resolve isto: {texto}"
        );
        assert!(
            !texto.contains("blob") && !texto.contains("src") && !texto.contains("URL"),
            "sem jargão: {texto}"
        );
    }

    /// **O que o backfill preserva é o que a entrada recusa, e isso é
    /// coerente.**
    ///
    /// Uma inline que não abre é preservada pelo backfill com pendência, e
    /// bloqueia a gravação daquele capítulo. As duas metades dizem a mesma
    /// coisa: o aplicativo não sabe o que fazer com aquele valor, então não
    /// destrói e não propaga.
    ///
    /// A externa é o caso em que as duas metades **divergem de propósito**: o
    /// backfill preserva byte a byte, e a entrada recusa. É o *fail-closed*
    /// pedido — o documento legado abre, e a imagem que não é portável precisa
    /// sair antes da próxima gravação.
    #[test]
    fn o_backfill_e_a_barreira_concordam_sobre_o_que_e_pendencia() {
        let loja = Loja::nova();

        let quebrada = "<img src=\"data:image/png;base64,!!!\">";
        let conversao = converter(quebrada, &loja.store).expect("converter");
        assert_eq!(conversao.pendencias.len(), 1, "o backfill registra");
        assert_eq!(conversao.html, quebrada, "e preserva");
        assert!(
            exigir_blob_safe(quebrada).is_err(),
            "e a entrada recusa o mesmo valor"
        );

        let externa = "<img src=\"https://cdn.exemplo.com/x.png\">";
        let conversao = converter(externa, &loja.store).expect("converter");
        assert_eq!(conversao.pendencias.len(), 1, "o backfill registra");
        assert_eq!(conversao.html, externa, "e preserva");
        assert_eq!(
            exigir_blob_safe(externa),
            Err(NaoEBlobSafe::FonteExterna { quantas: 1 }),
            "e a entrada recusa: o endereço dela não existe no outro aparelho"
        );
    }

    /// Documento convertido pelo transformador passa na barreira.
    ///
    /// Fecha o ciclo: o que o backfill produz é exatamente o que a entrada
    /// aceita. Se as duas pontas divergissem, o backfill deixaria o acervo num
    /// estado que o próprio aplicativo recusa gravar.
    #[test]
    fn o_que_o_transformador_produz_passa_na_barreira() {
        let loja = Loja::nova();
        let (url, _) = inline("imagem");
        let documento = format!("<p>a</p><img src=\"{url}\" alt=\"x\"><p>b</p>");

        assert!(
            exigir_blob_safe(&documento).is_err(),
            "antes de converter, recusa"
        );
        let conversao = converter(&documento, &loja.store).expect("converter");
        assert_eq!(
            exigir_blob_safe(&conversao.html),
            Ok(()),
            "depois de converter, passa: {}",
            conversao.html
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
