//! O mapa das superfícies que guardam binário no banco.
//!
//! ADR 0010. Dez lugares, e a lista foi construída por método, não por
//! lembrança: as tentativas por nome de coluna encontraram seis, a varredura
//! por `PRAGMA` sobre todas as tabelas encontrou mais duas, seguir o dado
//! encontrou a nona, e procurar **escritores em SQL** encontrou a décima. A
//! premissa mais errada do caminho foi minha: declarei `chapter_revisions`
//! tabela morta porque nenhum Rust e nenhum TypeScript escrevem nela. Quem
//! escreve é o gatilho `trg_chapter_revision`, dentro do banco — buscar
//! escritor em código-fonte não encontra escritor em SQL.
//!
//! ```text
//! 1..6   campo direto        a coluna é o binário
//! 7..8   documento Tiptap    0..N nodes image dentro de um JSON
//! 9..10  dois documentos     a linha carrega dois lados independentes
//! ```
//!
//! ## Por que existe um catálogo em vez de dez `INSERT` cuidadosos
//!
//! É o mesmo argumento da matriz de bootstrap da etapa 12: sem o conjunto
//! declarado num lugar só, a decisão de migrar ou não vira escolha de quem
//! escreveu cada consulta, tomada uma superfície por vez, sem ninguém olhar o
//! todo. Foi assim que quatro varreduras manhas minhas erraram a contagem
//! antes de o método mudar.
//!
//! O gate desta lista **não** tenta classificar todo `TEXT` do banco. Campo
//! textual continua texto: um personagem pode legitimamente escrever
//! `data:image/png;base64,…` dentro do resumo de um capítulo, e transformar
//! isso em asset seria inventar arquivo a partir de prosa. O que o gate faz é
//! obrigar uma **decisão** sempre que aparecer coluna com cara de binário —
//! classificada no catálogo, ou declarada em [`NAO_E_BINARIO`] com o motivo.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use rusqlite::Connection;

/// Como o binário se esconde naquela coluna.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forma {
    /// A coluna **é** o binário: uma `data:` URL e nada mais.
    ///
    /// Migra para um par de colunas de referência, e o valor antigo é limpo
    /// depois de haver blob válido publicado.
    CampoDireto,
    /// A coluna é um documento Tiptap em JSON, com 0..N nodes `image`.
    ///
    /// Não ganha coluna de referência: o hash mora **dentro** do node, senão
    /// um documento com três imagens precisaria de três colunas.
    DocumentoTiptap,
}

/// Quem escreve naquela coluna. Nem toda superfície tem comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escritor {
    /// Comando Rust na camada de aplicação.
    ComandoRust,
    /// **Gatilho SQL.** O escritor mora no banco, e nenhuma busca em
    /// código-fonte o encontra.
    GatilhoSql(&'static str),
    /// O protocolo antigo de sincronização, ainda em produção.
    ProtocoloV1,
    /// Valor que vem de fora, de um convidado com link.
    Colaboracao,
}

/// O recorte, quando a superfície não é a tabela inteira.
///
/// `sync_conflicts` guarda conflito de qualquer campo de qualquer agregado, e
/// só `chapter`/`content` carrega documento. Migrar a tabela toda trataria o
/// título de um capítulo como se fosse um documento Tiptap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorte {
    pub coluna: &'static str,
    pub valor: &'static str,
}

/// O par de colunas que a migration 20 acrescenta a um campo direto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Referencia {
    /// A coluna que hoje guarda os bytes.
    pub legada: &'static str,
    /// Onde o `SHA-256` canônico vai morar.
    pub hash: &'static str,
    /// O MIME lido do cabeçalho da `data:` URL.
    ///
    /// Guardado porque o blob é só bytes: sem ele, servir o arquivo de volta
    /// exigiria adivinhar o tipo pelo conteúdo, e adivinhar tipo de arquivo a
    /// partir de bytes é como se serve XSS por engano.
    pub mime: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Superficie {
    /// O número do ADR 0010, para que documento e código falem a mesma língua.
    pub numero: u8,
    pub tabela: &'static str,
    /// As colunas que hoje guardam o binário.
    ///
    /// Mais de uma quando a linha tem lados independentes, e "independentes" é
    /// literal: falha num lado não autoriza tocar no outro.
    pub colunas_legadas: &'static [&'static str],
    /// Vazio para documento — a referência mora dentro do JSON.
    pub referencias: &'static [Referencia],
    pub forma: Forma,
    /// Vazio quando a superfície é a tabela inteira.
    pub recorte: &'static [Recorte],
    pub escritor: Escritor,
}

impl Superficie {
    /// Tem dois lados independentes na mesma linha?
    pub fn tem_dois_lados(&self) -> bool {
        self.colunas_legadas.len() > 1
    }

    /// O `WHERE` do recorte, ou `1 = 1` quando é a tabela inteira.
    ///
    /// Montado com valores literais do catálogo — constantes de compilação,
    /// nunca texto de fora — porque interpolar campo em SQL foi exatamente o
    /// defeito que a camada de colaboração já tinha.
    pub fn clausula_do_recorte(&self) -> String {
        if self.recorte.is_empty() {
            return "1 = 1".to_string();
        }
        self.recorte
            .iter()
            .map(|corte| format!("{} = '{}'", corte.coluna, corte.valor))
            .collect::<Vec<_>>()
            .join(" AND ")
    }
}

/// As dez, na ordem do ADR 0010.
pub const CATALOGO: &[Superficie] = &[
    // ── 1..6: campo direto ──────────────────────────────────────────────────
    Superficie {
        numero: 1,
        tabela: "attachments",
        colunas_legadas: &["data_url"],
        referencias: &[Referencia {
            legada: "data_url",
            hash: "blob_hash",
            mime: "mime_type",
        }],
        forma: Forma::CampoDireto,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    Superficie {
        numero: 2,
        tabela: "universes",
        colunas_legadas: &["cover_image"],
        referencias: &[Referencia {
            legada: "cover_image",
            hash: "cover_blob_hash",
            mime: "cover_mime_type",
        }],
        forma: Forma::CampoDireto,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    Superficie {
        numero: 3,
        tabela: "entities",
        colunas_legadas: &["image"],
        referencias: &[Referencia {
            legada: "image",
            hash: "image_blob_hash",
            mime: "image_mime_type",
        }],
        forma: Forma::CampoDireto,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    Superficie {
        numero: 4,
        tabela: "canvas_nodes",
        colunas_legadas: &["image"],
        referencias: &[Referencia {
            legada: "image",
            hash: "image_blob_hash",
            mime: "image_mime_type",
        }],
        forma: Forma::CampoDireto,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    Superficie {
        numero: 5,
        tabela: "books",
        colunas_legadas: &["cover_image"],
        referencias: &[Referencia {
            legada: "cover_image",
            hash: "cover_blob_hash",
            mime: "cover_mime_type",
        }],
        forma: Forma::CampoDireto,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    Superficie {
        numero: 6,
        tabela: "planning_items",
        colunas_legadas: &["image"],
        referencias: &[Referencia {
            legada: "image",
            hash: "image_blob_hash",
            mime: "image_mime_type",
        }],
        forma: Forma::CampoDireto,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    // ── 7..8: documento Tiptap ──────────────────────────────────────────────
    Superficie {
        numero: 7,
        tabela: "chapters",
        colunas_legadas: &["content"],
        referencias: &[],
        forma: Forma::DocumentoTiptap,
        recorte: &[],
        escritor: Escritor::ComandoRust,
    },
    Superficie {
        numero: 8,
        tabela: "chapter_revisions",
        colunas_legadas: &["content"],
        referencias: &[],
        forma: Forma::DocumentoTiptap,
        recorte: &[],
        // A correção da premissa falsa, em código: o escritor é o gatilho.
        escritor: Escritor::GatilhoSql("trg_chapter_revision"),
    },
    // ── 9..10: dois documentos independentes na mesma linha ─────────────────
    Superficie {
        numero: 9,
        tabela: "sync_conflicts",
        colunas_legadas: &["local_value", "remote_value"],
        referencias: &[],
        forma: Forma::DocumentoTiptap,
        recorte: &[
            Recorte {
                coluna: "aggregate_type",
                valor: "chapter",
            },
            Recorte {
                coluna: "field",
                valor: "content",
            },
        ],
        escritor: Escritor::ProtocoloV1,
    },
    Superficie {
        numero: 10,
        tabela: "collaboration_contributions",
        colunas_legadas: &["original_value", "proposed_value"],
        referencias: &[],
        forma: Forma::DocumentoTiptap,
        recorte: &[
            Recorte {
                coluna: "target_type",
                valor: "chapter",
            },
            Recorte {
                coluna: "field",
                valor: "content",
            },
        ],
        escritor: Escritor::Colaboracao,
    },
];

/// As palavras que fazem um nome de coluna parecer binário.
///
/// Serve para o gate de regressão, e não para descobrir superfície: uma coluna
/// batizada sem nenhuma dessas palavras não seria pega aqui, e foi justamente
/// assim que `chapters.content` escapou das primeiras varreduras. O que este
/// gate garante é o outro lado — coluna nova com cara de binário não passa sem
/// alguém decidir o que ela é.
pub const PALAVRAS_DE_BINARIO: &[&str] = &[
    "image",
    "cover",
    "data_url",
    "dataurl",
    "avatar",
    "thumb",
    "picture",
    "photo",
    "blob",
    "attachment",
    "media",
    "icon",
    "banner",
    "logo",
    "base64",
    "bytes",
    "binary",
];

/// Colunas que a heurística de nome pega e que **não** guardam binário.
///
/// Cada linha é uma decisão registrada, com o motivo — porque a alternativa é
/// alguém apagar a palavra da heurística para o gate ficar verde, e aí o gate
/// deixa de existir para todas as colunas futuras de uma vez.
pub const NAO_E_BINARIO: &[(&str, &str, &str)] = &[];

/// Toda coluna de toda tabela do banco aberto, como `(tabela, coluna)`.
///
/// Derivado de `sqlite_master` e `PRAGMA table_info`, nunca de lista à mão: as
/// listas à mão que eu escrevi neste repositório erraram quatro vezes.
pub fn colunas_do_banco(connection: &Connection) -> DatabaseCommandResult<Vec<(String, String)>> {
    let mut tabelas = connection
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
              ORDER BY name",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let nomes: Vec<String> = tabelas
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let mut encontradas = Vec::new();
    for tabela in nomes {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({tabela})"))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        let colunas: Vec<String> = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        for coluna in colunas {
            encontradas.push((tabela.clone(), coluna));
        }
    }
    Ok(encontradas)
}

/// A superfície daquela tabela, se houver.
pub fn superficie_de(tabela: &str) -> Option<&'static Superficie> {
    CATALOGO
        .iter()
        .find(|superficie| superficie.tabela == tabela)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::collections::BTreeSet;

    /// As dez do ADR 0010, numeradas de 1 a 10, sem lacuna e sem repetição.
    ///
    /// A numeração existe para que documento e código falem a mesma língua: um
    /// achado descrito como "superfície 9" no ADR tem que ser localizável aqui
    /// sem tradução.
    #[test]
    fn o_catalogo_declara_as_dez_superficies_do_adr() {
        assert_eq!(CATALOGO.len(), 10, "o ADR 0010 fechou em dez superfícies");
        let numeros: Vec<u8> = CATALOGO.iter().map(|s| s.numero).collect();
        assert_eq!(numeros, (1..=10).collect::<Vec<u8>>());

        let tabelas: BTreeSet<&str> = CATALOGO.iter().map(|s| s.tabela).collect();
        assert_eq!(
            tabelas.len(),
            10,
            "duas superfícies na mesma tabela precisariam de recorte diferente, e o \
             catálogo não modela isso hoje"
        );
    }

    /// **Toda tabela e coluna do catálogo existe no banco de verdade.**
    ///
    /// Pega a deriva na direção contrária à do gate seguinte: coluna renomeada
    /// ou removida por uma migration futura deixaria o catálogo apontando para
    /// o nada, e o backfill varreria uma superfície vazia sem erro nenhum —
    /// silêncio indistinguível de "não havia o que migrar".
    ///
    /// A lista vem de `PRAGMA table_info`, não de levantamento à mão.
    #[test]
    fn toda_tabela_e_coluna_do_catalogo_existe_no_schema() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();
        let reais: BTreeSet<(String, String)> = colunas_do_banco(&connection)
            .expect("ler o schema")
            .into_iter()
            .collect();

        for superficie in CATALOGO {
            let colunas: BTreeSet<&String> = reais
                .iter()
                .filter(|(tabela, _)| tabela == superficie.tabela)
                .map(|(_, coluna)| coluna)
                .collect();
            assert!(
                !colunas.is_empty(),
                "a superfície {} aponta para a tabela {}, que não existe no schema",
                superficie.numero,
                superficie.tabela
            );

            for legada in superficie.colunas_legadas {
                assert!(
                    colunas.iter().any(|coluna| coluna.as_str() == *legada),
                    "a superfície {} aponta para {}.{legada}, e essa coluna não existe",
                    superficie.numero,
                    superficie.tabela
                );
            }
            for corte in superficie.recorte {
                assert!(
                    colunas.iter().any(|coluna| coluna.as_str() == corte.coluna),
                    "o recorte da superfície {} filtra por {}.{}, e essa coluna não existe",
                    superficie.numero,
                    superficie.tabela,
                    corte.coluna
                );
            }
        }
    }

    /// **Nenhuma coluna com cara de binário fica sem classificação.**
    ///
    /// Este é o gate contra a décima-primeira superfície. Ele não descobre
    /// superfície — `chapters.content` escapou de todas as varreduras por nome,
    /// e escaparia desta também. O que ele garante é que ninguém acrescente
    /// `entities.avatar` ou `books.banner_image` sem **decidir** o que aquilo
    /// é: ou entra no catálogo, ou entra em `NAO_E_BINARIO` com o motivo
    /// escrito.
    ///
    /// E não tenta classificar todo `TEXT`, de propósito. Campo textual
    /// continua texto: um personagem pode escrever `data:image/png;base64,…`
    /// dentro do resumo de um capítulo, e virar asset a partir de prosa seria
    /// inventar arquivo.
    #[test]
    fn nenhuma_coluna_com_cara_de_binario_fica_sem_classificacao() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();

        let classificadas: BTreeSet<(&str, &str)> = CATALOGO
            .iter()
            .flat_map(|superficie| {
                superficie
                    .colunas_legadas
                    .iter()
                    .map(move |coluna| (superficie.tabela, *coluna))
                    .chain(superficie.referencias.iter().flat_map(move |referencia| {
                        [
                            (superficie.tabela, referencia.hash),
                            (superficie.tabela, referencia.mime),
                        ]
                    }))
            })
            .collect();
        let declaradas: BTreeSet<(&str, &str)> = NAO_E_BINARIO
            .iter()
            .map(|(tabela, coluna, _)| (*tabela, *coluna))
            .collect();

        let mut sem_decisao = Vec::new();
        let mut suspeitas = BTreeSet::new();
        for (tabela, coluna) in colunas_do_banco(&connection).expect("ler o schema") {
            let minuscula = coluna.to_ascii_lowercase();
            if !PALAVRAS_DE_BINARIO
                .iter()
                .any(|palavra| minuscula.contains(palavra))
            {
                continue;
            }
            suspeitas.insert(format!("{tabela}.{coluna}"));
            let par = (tabela.as_str(), coluna.as_str());
            if classificadas.contains(&par) || declaradas.contains(&par) {
                continue;
            }
            sem_decisao.push(format!("{tabela}.{coluna}"));
        }

        // A varredura acha mesmo o que ela deveria achar?
        //
        // Sem esta parte o gate passaria por vácuo: bastaria a heurística não
        // casar com nada — palavra escrita errado, coluna lida do lugar errado
        // — para a lista de pendências ficar vazia e o teste ficar verde sem ter
        // olhado o banco. É o defeito que já apareceu duas vezes nesta suíte,
        // gate passando pelo motivo errado.
        for esperada in [
            "attachments.data_url",
            "universes.cover_image",
            "entities.image",
            "canvas_nodes.image",
            "books.cover_image",
            "planning_items.image",
        ] {
            assert!(
                suspeitas.contains(esperada),
                "a varredura por nome não encontrou {esperada}, que está no schema e é \
                 superfície conhecida. A heurística parou de funcionar, e um gate que não \
                 encontra nada aprova tudo. Suspeitas encontradas: {suspeitas:?}"
            );
        }

        assert!(
            sem_decisao.is_empty(),
            "estas colunas têm nome de binário e não têm decisão registrada: {sem_decisao:?}. \
             Classifique no CATALOGO do ADR 0010, ou declare em NAO_E_BINARIO com o motivo. \
             Apagar a palavra de PALAVRAS_DE_BINARIO para o gate ficar verde desliga a \
             proteção para todas as colunas futuras de uma vez."
        );
    }

    /// A declaração de "não é binário" precisa de motivo, e precisa ser real.
    #[test]
    fn o_que_e_declarado_como_nao_binario_existe_e_tem_motivo() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();
        let reais: BTreeSet<(String, String)> = colunas_do_banco(&connection)
            .expect("ler o schema")
            .into_iter()
            .collect();

        for (tabela, coluna, motivo) in NAO_E_BINARIO {
            assert!(
                reais.contains(&(tabela.to_string(), coluna.to_string())),
                "{tabela}.{coluna} está declarada como não-binária e não existe no schema. \
                 Declaração órfã esconde a próxima coluna que se chamar assim."
            );
            assert!(
                motivo.len() > 20,
                "{tabela}.{coluna} foi declarada sem motivo de verdade: {motivo:?}"
            );
        }
    }

    /// **`chapter_revisions` tem escritor vivo, e ele é SQL.**
    ///
    /// O gate pedido explicitamente. `docs/ARCHITECTURE_EVOLUTION_PLAN.md`
    /// afirmava que a tabela "nunca teve escrita nenhuma, nem no frontend nem
    /// no Rust" — e a segunda metade da frase é verdadeira, o que é o que faz a
    /// primeira ser convincente. Quem escreve é o gatilho, dentro do banco:
    ///
    /// ```text
    /// grep no código-fonte     →  nenhum INSERT em chapter_revisions
    /// sqlite_master            →  trg_chapter_revision, desde a migration 1
    /// ```
    ///
    /// Enquanto o gatilho existir, a tabela é uma superfície binária: se o
    /// capítulo guarda imagem em base64, o gatilho copia a base64 a cada
    /// salvamento.
    #[test]
    fn chapter_revisions_tem_escritor_vivo() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();

        let gatilhos: Vec<String> = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'trigger' AND tbl_name = 'chapters'",
            )
            .expect("preparar")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("consultar")
            .collect::<Result<Vec<_>, _>>()
            .expect("ler gatilhos");
        assert!(
            gatilhos.iter().any(|nome| nome == "trg_chapter_revision"),
            "o gatilho sumiu. Se isso foi deliberado, a superfície 8 precisa ser \
             reclassificada — e a documentação, revista de novo. Gatilhos hoje: {gatilhos:?}"
        );

        let superficie = superficie_de("chapter_revisions").expect(
            "enquanto trg_chapter_revision existir, chapter_revisions é superfície binária",
        );
        assert_eq!(superficie.numero, 8);
        assert_eq!(
            superficie.escritor,
            Escritor::GatilhoSql("trg_chapter_revision"),
            "o catálogo tem que nomear o escritor real, senão a próxima busca por \
             escritores em código-fonte conclui 'tabela morta' de novo"
        );
    }

    /// Campo direto ganha par de referência; documento não ganha coluna.
    ///
    /// Um documento com três imagens precisaria de três colunas, então o hash
    /// mora dentro do node. Confundir as duas formas faria a migration 20
    /// acrescentar uma coluna que nunca seria preenchida — e o backfill
    /// procuraria o hash no lugar errado.
    #[test]
    fn campo_direto_tem_par_de_referencia_e_documento_nao_tem() {
        for superficie in CATALOGO {
            match superficie.forma {
                Forma::CampoDireto => {
                    assert_eq!(
                        superficie.colunas_legadas.len(),
                        1,
                        "campo direto tem uma coluna só (superfície {})",
                        superficie.numero
                    );
                    assert_eq!(
                        superficie.referencias.len(),
                        1,
                        "campo direto ganha exatamente um par hash/mime (superfície {})",
                        superficie.numero
                    );
                    let referencia = &superficie.referencias[0];
                    assert_eq!(referencia.legada, superficie.colunas_legadas[0]);
                    assert!(!superficie.tem_dois_lados());
                }
                Forma::DocumentoTiptap => {
                    assert!(
                        superficie.referencias.is_empty(),
                        "documento não ganha coluna de referência: o hash mora no node \
                         (superfície {})",
                        superficie.numero
                    );
                }
            }
        }
    }

    /// Os nomes de referência não colidem entre si nem com a coluna legada.
    ///
    /// A migration 20 vai criar essas colunas por nome. Um par repetido faria
    /// duas superfícies gravarem no mesmo lugar, e um nome igual ao da coluna
    /// legada faria o `ALTER TABLE` falhar no meio da migration — no aparelho
    /// do escritor, não aqui.
    #[test]
    fn os_nomes_de_referencia_nao_colidem() {
        for superficie in CATALOGO {
            let mut vistos: BTreeSet<&str> = superficie.colunas_legadas.iter().copied().collect();
            for referencia in superficie.referencias {
                assert!(
                    vistos.insert(referencia.hash),
                    "{}.{} está declarada duas vezes",
                    superficie.tabela,
                    referencia.hash
                );
                assert!(
                    vistos.insert(referencia.mime),
                    "{}.{} está declarada duas vezes",
                    superficie.tabela,
                    referencia.mime
                );
                assert!(
                    referencia.hash.contains("hash") && referencia.mime.contains("mime"),
                    "os nomes precisam dizer o que guardam: {:?}",
                    referencia
                );
            }
        }
    }

    /// Só as superfícies de dois lados têm recorte, e o recorte é o do ADR.
    ///
    /// `sync_conflicts` guarda conflito de qualquer campo de qualquer agregado.
    /// Sem recorte, o backfill trataria o título de um capítulo como documento
    /// Tiptap — e o título não é JSON, então toda linha viraria issue de
    /// migração e o bootstrap ficaria bloqueado por dado que nunca foi mídia.
    #[test]
    fn so_as_superficies_de_dois_lados_tem_recorte() {
        for superficie in CATALOGO {
            assert_eq!(
                superficie.tem_dois_lados(),
                !superficie.recorte.is_empty(),
                "superfície {}: dois lados e recorte andam juntos — a tabela é compartilhada \
                 com dado que não é mídia",
                superficie.numero
            );
        }

        let conflitos = superficie_de("sync_conflicts").expect("superfície 9");
        assert_eq!(
            conflitos.clausula_do_recorte(),
            "aggregate_type = 'chapter' AND field = 'content'"
        );
        assert_eq!(conflitos.colunas_legadas, &["local_value", "remote_value"]);

        let contribuicoes = superficie_de("collaboration_contributions").expect("superfície 10");
        assert_eq!(
            contribuicoes.clausula_do_recorte(),
            "target_type = 'chapter' AND field = 'content'"
        );
        assert_eq!(
            contribuicoes.colunas_legadas,
            &["original_value", "proposed_value"]
        );

        let capitulos = superficie_de("chapters").expect("superfície 7");
        assert_eq!(
            capitulos.clausula_do_recorte(),
            "1 = 1",
            "capítulo é documento sempre: a tabela inteira é a superfície"
        );
    }

    /// **`sync_events` fica fora, e isso é decisão, não esquecimento.**
    ///
    /// O payload histórico é assinado, imutável e append-only. Reescrever para
    /// trocar base64 por hash invalidaria a assinatura de todo evento já
    /// trocado com outro aparelho — e a assinatura é o que faz o log ser fonte
    /// de verdade no ADR 0009. O que a etapa 13 garante é o futuro: evento
    /// **novo** nunca carrega bytes.
    #[test]
    fn sync_events_nao_e_superficie_de_migracao() {
        assert!(
            superficie_de("sync_events").is_none(),
            "o payload histórico é assinado: migrar invalidaria a assinatura de todo evento \
             já trocado. Se isso mudar, muda o ADR 0009 antes."
        );
        assert!(superficie_de("sync_revision_history").is_none());
    }

    /// **A migration 20 criou toda coluna de referência que o catálogo declara.**
    ///
    /// O outro lado de `toda_tabela_e_coluna_do_catalogo_existe_no_schema`, e
    /// só pôde existir depois do schema 20. Sem ele, o backfill gravaria o hash
    /// numa coluna inexistente — e descobriria isso no aparelho do escritor, no
    /// meio da conversão, com o inline já contado como migrado.
    ///
    /// As colunas vêm de `PRAGMA table_info`, para que a comparação não seja
    /// entre duas listas escritas à mão.
    #[test]
    fn a_migration_20_criou_toda_coluna_de_referencia_do_catalogo() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();
        let reais: BTreeSet<(String, String)> = colunas_do_banco(&connection)
            .expect("ler o schema")
            .into_iter()
            .collect();

        let mut conferidas = 0;
        for superficie in CATALOGO {
            for referencia in superficie.referencias {
                for coluna in [referencia.hash, referencia.mime] {
                    assert!(
                        reais.contains(&(superficie.tabela.to_string(), coluna.to_string())),
                        "o catálogo declara {}.{coluna} e o schema não tem essa coluna. O \
                         backfill gravaria o hash no vazio.",
                        superficie.tabela
                    );
                    conferidas += 1;
                }
            }
        }
        assert_eq!(
            conferidas, 12,
            "seis superfícies diretas, um par hash/MIME cada. Se este número caiu, o catálogo \
             perdeu uma referência e o gate passaria conferindo menos."
        );
    }

    // Aqui havia um gate `coluna_de_referencia_nasce_vazia`, e ele passava por
    // vácuo: `TemporaryDatabase` não semeia nenhuma dessas seis tabelas, então
    // o `COUNT(*) WHERE coluna <> ''` era zero por não haver linha, e não por a
    // coluna nascer vazia. Um gate que não encontra nada aprova tudo.
    //
    // A propriedade está provada onde há linha de verdade, em
    // `migrations::tests::banco_nascido_no_19_migra_para_o_20_e_ganha_referencia_sem_perder_byte`:
    // seis linhas semeadas no schema 19, migradas, e cada referência conferida
    // como vazia com o valor legado intacto ao lado.
}
