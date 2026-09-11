//! A fronteira de upgrade dos assets.
//!
//! ADR 0010. O backfill existe desde a fatia 5, e até aqui ele era
//! infraestrutura sem chamador — a mesma lacuna que a revisão da etapa 2.5
//! apontou para `load_or_create`: funciona em teste e nunca roda no
//! aplicativo.
//!
//! ```text
//! ARRANQUE DO APP
//!    │
//!    ▼
//! compatibilidade de schema      portão do ADR 0007
//!    │
//!    ▼
//! db.init()                      o plugin-sql aplica as migrations
//!    │
//!    ▼
//! preparar_assets()              ◄── AQUI
//!    │                               publica blobs, grava refs,
//!    │                               limpa o inline, registra pendências
//!    ▼
//! universes.load()               primeiro consumo do acervo
//! ```
//!
//! É uma fronteira, não um passo de cada operação. O backfill é idempotente,
//! mas varrer o acervo a cada gravação de capítulo seria pagar por um upgrade
//! que já aconteceu.
//!
//! ## Por que não bloqueia o aplicativo
//!
//! Legado que não pôde ser convertido vira `blob_migration_issue` e o escritor
//! continua trabalhando. Quem exige o contrato completo é o **bootstrap**, e
//! ele já sabe recusar — `FalhaDeCaptura::AssetNaoConvertido`. Travar a
//! abertura por uma imagem antiga que ninguém consegue ler seria transformar
//! um problema de mídia em perda de acesso ao texto.
//!
//! ## Por que há uma verificação barata antes
//!
//! O backfill completo confere **cada blob no disco** para decidir se o inline
//! pode sair. Num acervo já migrado isso é uma leitura de arquivo por imagem,
//! em todo arranque, para descobrir que não há nada a fazer.
//!
//! A detecção pergunta outra coisa, e pergunta barato: **existe alguma coluna
//! legada preenchida?** É `EXISTS` com saída no primeiro acerto, e num acervo
//! migrado ela custa uma varredura das colunas curtas mais um `LIKE` nos
//! documentos — dezenas de milissegundos, não segundos.
//!
//! Nada de marcador novo e nada de segundo sistema de migration: a pergunta é
//! respondida pelo próprio estado, que é o que não pode mentir.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::blob_backfill::{
    migrar_campos_diretos, migrar_documentos, pendencias_abertas, Resumo,
};
use crate::infrastructure::sqlite::blob_surfaces::{Forma, CATALOGO};
use crate::infrastructure::sqlite::SqliteDatabase;
use rusqlite::Connection;
use serde::Serialize;

/// O que o arranque fez com os assets.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumoDoArranque {
    /// Havia legado para converter?
    pub havia_trabalho: bool,
    /// Assets publicados no blob store nesta passada.
    pub migrados: usize,
    /// Referências que existiam com o blob ausente e o inline salvou.
    pub reconstruidos: usize,
    /// Linhas cujo inline saiu por haver blob válido.
    pub inline_limpo: usize,
    /// Pendências abertas **depois** da passada — o que não pôde ser
    /// convertido e continua preservado.
    pub pendencias_abertas: usize,
}

/// **Converte o legado de mídia, uma vez por arranque.**
///
/// Idempotente por construção (ver `blob_backfill`), e seguro contra queda: o
/// hash entra antes de o inline sair, então uma interrupção deixa a linha num
/// estado que a próxima passada termina.
pub fn preparar_assets(
    database: &SqliteDatabase,
    store: &BlobStore,
) -> DatabaseCommandResult<ResumoDoArranque> {
    {
        let connection = database.read()?;
        if !ha_legado_para_converter(&connection)? {
            // Nada a fazer. Ainda assim reporta as pendências abertas: elas são
            // o que o bootstrap consulta, e a tela precisa poder mostrá-las.
            return Ok(ResumoDoArranque {
                havia_trabalho: false,
                pendencias_abertas: pendencias_abertas(&connection)?.len(),
                ..ResumoDoArranque::default()
            });
        }
    }

    let mut connection = database.write()?;
    let diretos = migrar_campos_diretos(&connection, store)?;
    // Documentos depois dos campos diretos, e não por acaso: a mesma imagem
    // costuma aparecer nos dois, e publicar uma vez basta — a deduplicação é
    // pelo endereço, então a segunda passada só acha o arquivo já lá.
    let documentos = migrar_documentos(&mut connection, store)?;

    let total = somar(diretos, documentos);
    Ok(ResumoDoArranque {
        havia_trabalho: true,
        migrados: total.migrados,
        reconstruidos: total.reconstruidos,
        inline_limpo: total.inline_limpo,
        pendencias_abertas: pendencias_abertas(&connection)?.len(),
    })
}

fn somar(um: Resumo, outro: Resumo) -> Resumo {
    Resumo {
        migrados: um.migrados + outro.migrados,
        reconstruidos: um.reconstruidos + outro.reconstruidos,
        inline_limpo: um.inline_limpo + outro.inline_limpo,
        ja_prontos: um.ja_prontos + outro.ja_prontos,
        pendencias: um.pendencias + outro.pendencias,
    }
}

/// **Existe alguma coluna legada preenchida?**
///
/// Derivada do catálogo, para uma superfície nova entrar sozinha. Sai no
/// primeiro acerto: a pergunta é "há trabalho?", não "quanto trabalho há".
///
/// Para documento a busca é `LIKE '%src="data:%'`. É textual de propósito, e é
/// o único lugar desta etapa em que isso é aceitável: aqui a pergunta é
/// **onde procurar**, não **o que é imagem**. Um falso positivo custa uma
/// passada do backfill, que é idempotente e vai concluir que não havia nada —
/// e quem decide o que é mídia continua sendo o parser estrutural.
fn ha_legado_para_converter(connection: &Connection) -> DatabaseCommandResult<bool> {
    for superficie in CATALOGO {
        match superficie.forma {
            Forma::CampoDireto => {
                for coluna in superficie.colunas_legadas {
                    if existe(
                        connection,
                        &format!(
                            "SELECT 1 FROM {} WHERE {coluna} <> '' LIMIT 1",
                            superficie.tabela
                        ),
                    )? {
                        return Ok(true);
                    }
                }
            }
            Forma::DocumentoTiptap => {
                let recorte = superficie.clausula_do_recorte();
                for coluna in superficie.colunas_legadas {
                    if existe(
                        connection,
                        &format!(
                            "SELECT 1 FROM {} WHERE ({recorte}) AND {coluna} LIKE '%src=\"data:%' \
                             LIMIT 1",
                            superficie.tabela
                        ),
                    )? {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

fn existe(connection: &Connection, sql: &str) -> DatabaseCommandResult<bool> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    statement
        .exists([])
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::path::PathBuf;

    struct Acervo {
        banco: TemporaryDatabase,
        raiz: PathBuf,
        store: BlobStore,
    }

    impl Acervo {
        fn novo() -> Self {
            let raiz =
                std::env::temp_dir().join(format!("narrahub-up-{}", crate::domain::ids::new_id()));
            std::fs::create_dir_all(&raiz).expect("criar raiz");
            Self {
                banco: TemporaryDatabase::new(),
                store: BlobStore::new(&raiz),
                raiz,
            }
        }

        fn conexao(&self) -> Connection {
            self.banco.connection()
        }

        fn ler(&self, sql: &str) -> String {
            self.conexao()
                .query_row(sql, [], |row| row.get(0))
                .unwrap_or_else(|erro| panic!("{sql}: {erro}"))
        }

        fn blobs_no_disco(&self) -> usize {
            fn varrer(dir: &std::path::Path, n: &mut usize) {
                let Ok(filhos) = std::fs::read_dir(dir) else {
                    return;
                };
                for filho in filhos.flatten() {
                    let caminho = filho.path();
                    if caminho.is_dir() {
                        varrer(&caminho, n);
                    } else {
                        *n += 1;
                    }
                }
            }
            let mut n = 0;
            varrer(&self.store.raiz(), &mut n);
            n
        }
    }

    impl Drop for Acervo {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.raiz).ok();
        }
    }

    fn inline(conteudo: &str) -> (String, Vec<u8>) {
        let bytes = conteudo.as_bytes().to_vec();
        let texto = crate::domain::data_url::codificar_base64(&bytes);
        (format!("data:image/png;base64,{texto}"), bytes)
    }

    /// Um acervo como o de quem atualizou de uma versão antiga: capa e
    /// capítulo com a imagem embutida no próprio dado.
    fn semear_acervo_legado(acervo: &Acervo, url: &str) {
        acervo
            .conexao()
            .execute_batch(&format!(
                "INSERT INTO universes (id, name, cover_image, created_at, updated_at)
                    VALUES ('u1','Universo','{url}','2026-01-01','2026-01-01');
                 INSERT INTO stories (id, universe_id, name, created_at, updated_at)
                    VALUES ('s1','u1','S','2026-01-01','2026-01-01');
                 INSERT INTO books (id, story_id, name, created_at, updated_at)
                    VALUES ('b1','s1','L','2026-01-01','2026-01-01');
                 INSERT INTO chapters (id, book_id, title, content, word_count)
                    VALUES ('cap1','b1','Cap',
                            '<p>texto</p><img src=\"{url}\" alt=\"x\">', 5);"
            ))
            .expect("semear acervo legado");
    }

    /// **1. Banco legado abre, e o backfill roda de verdade.**
    ///
    /// É o gate que faltava para a etapa 13 estar entregue: o backfill existia
    /// e não tinha chamador. Aqui ele roda pela porta do arranque, e o efeito
    /// é medido no banco e no disco.
    #[test]
    fn acervo_legado_e_convertido_no_arranque() {
        let acervo = Acervo::novo();
        let (url, bytes) = inline("a-imagem-do-escritor");
        semear_acervo_legado(&acervo, &url);

        let resumo = preparar_assets(&acervo.banco.database, &acervo.store).expect("arranque");

        assert!(resumo.havia_trabalho, "havia legado e ele foi reconhecido");
        assert_eq!(resumo.migrados, 2, "a capa e a imagem do capítulo");
        assert_eq!(resumo.pendencias_abertas, 0);

        let hash = crate::infrastructure::blob_store::hash_dos_bytes(&bytes);
        assert_eq!(acervo.ler("SELECT cover_blob_hash FROM universes"), hash);
        assert_eq!(acervo.ler("SELECT cover_image FROM universes"), "");
        assert!(acervo
            .ler("SELECT content FROM chapters WHERE id = 'cap1'")
            .contains(&hash));
        assert!(!acervo
            .ler("SELECT content FROM chapters WHERE id = 'cap1'")
            .contains("data:"));

        // Um arquivo: a mesma imagem nas duas superfícies deduplica.
        assert_eq!(acervo.blobs_no_disco(), 1);
        assert_eq!(acervo.store.read(&hash).expect("ler o blob"), bytes);
    }

    /// **5. Depois do arranque, o capítulo legado já está blob-safe.**
    ///
    /// A consequência que importa: `update_chapter` recusa documento com mídia
    /// inline, então um acervo antigo ficaria sem poder ser salvo se o arranque
    /// não tivesse convertido.
    #[test]
    fn depois_do_arranque_o_capitulo_legado_passa_na_barreira() {
        let acervo = Acervo::novo();
        let (url, _) = inline("imagem");
        semear_acervo_legado(&acervo, &url);

        let antes = acervo.ler("SELECT content FROM chapters WHERE id = 'cap1'");
        assert!(
            crate::infrastructure::blob_document::exigir_blob_safe(&antes).is_err(),
            "o cenário precisa começar fora do contrato"
        );

        preparar_assets(&acervo.banco.database, &acervo.store).expect("arranque");

        let depois = acervo.ler("SELECT content FROM chapters WHERE id = 'cap1'");
        assert_eq!(
            crate::infrastructure::blob_document::exigir_blob_safe(&depois),
            Ok(()),
            "o capítulo tinha que ficar salvável: {depois}"
        );
    }

    /// **2. Banco já migrado: o arranque não altera dado nenhum.**
    ///
    /// E não paga o preço de conferir cada blob no disco — a detecção pergunta
    /// "há coluna legada preenchida?", que é barato, em vez de "todo blob está
    /// íntegro?", que é uma leitura de arquivo por imagem.
    #[test]
    fn acervo_ja_migrado_nao_e_tocado_no_arranque() {
        let acervo = Acervo::novo();
        let (url, _) = inline("imagem");
        semear_acervo_legado(&acervo, &url);
        preparar_assets(&acervo.banco.database, &acervo.store).expect("primeiro arranque");

        let capa = acervo.ler("SELECT cover_blob_hash FROM universes");
        let conteudo = acervo.ler("SELECT content FROM chapters WHERE id = 'cap1'");
        let arquivos = acervo.blobs_no_disco();

        let segundo = preparar_assets(&acervo.banco.database, &acervo.store).expect("segundo");

        assert!(!segundo.havia_trabalho, "não havia mais legado");
        assert_eq!(segundo.migrados, 0);
        assert_eq!(acervo.ler("SELECT cover_blob_hash FROM universes"), capa);
        assert_eq!(
            acervo.ler("SELECT content FROM chapters WHERE id = 'cap1'"),
            conteudo
        );
        assert_eq!(acervo.blobs_no_disco(), arquivos);
    }

    /// **3. Interromper e repetir converge.**
    ///
    /// O estado intermediário é o do desenho: hash gravado, inline ainda lá.
    /// Uma queda ali é o caso real — e a segunda passada tem que terminar o
    /// serviço sem perder arquivo.
    #[test]
    fn arranque_interrompido_converge_na_proxima_vez() {
        let acervo = Acervo::novo();
        let (url, bytes) = inline("imagem");
        semear_acervo_legado(&acervo, &url);

        // Reproduz a interrupção: o blob foi publicado e a referência gravada,
        // e o processo morreu antes de o inline sair.
        let hash = acervo.store.put(&bytes).expect("publicar");
        acervo
            .conexao()
            .execute(
                "UPDATE universes SET cover_blob_hash = ?1, cover_mime_type = 'image/png'",
                [&hash],
            )
            .expect("como se o commit 1 tivesse acontecido");
        assert_ne!(
            acervo.ler("SELECT cover_image FROM universes"),
            "",
            "o cenário precisa do inline ainda presente"
        );

        let resumo = preparar_assets(&acervo.banco.database, &acervo.store).expect("retomar");

        assert!(resumo.havia_trabalho);
        assert_eq!(acervo.ler("SELECT cover_image FROM universes"), "");
        assert_eq!(acervo.ler("SELECT cover_blob_hash FROM universes"), hash);
        assert_eq!(acervo.store.read(&hash).expect("ler"), bytes);
        assert_eq!(acervo.blobs_no_disco(), 1, "nada foi duplicado");
    }

    /// **4. Legado não migrável é preservado e não impede a abertura.**
    ///
    /// O arranque conclui, o valor original continua exatamente onde estava, e
    /// a pendência fica registrada para o bootstrap consultar. Travar a
    /// abertura por uma imagem antiga ilegível transformaria um problema de
    /// mídia em perda de acesso ao texto.
    #[test]
    fn legado_nao_migravel_vira_pendencia_sem_travar_o_arranque() {
        let acervo = Acervo::novo();
        let externa = "https://cdn.exemplo.com/capa-antiga.png";
        semear_acervo_legado(&acervo, externa);

        let resumo = preparar_assets(&acervo.banco.database, &acervo.store)
            .expect("o arranque não pode falhar por causa de mídia");

        assert!(resumo.havia_trabalho);
        assert_eq!(resumo.migrados, 0);
        assert!(
            resumo.pendencias_abertas > 0,
            "a pendência tinha que existir"
        );

        assert_eq!(
            acervo.ler("SELECT cover_image FROM universes"),
            externa,
            "o valor original tinha que ser preservado"
        );
        assert_eq!(acervo.blobs_no_disco(), 0, "nada foi baixado");
    }

    /// **6. Instalação nova continua funcionando.**
    ///
    /// Banco vazio: o arranque é quase um no-op, e não deixa nada para trás.
    #[test]
    fn instalacao_nova_nao_tem_o_que_migrar() {
        let acervo = Acervo::novo();

        let resumo = preparar_assets(&acervo.banco.database, &acervo.store).expect("arranque");

        assert_eq!(resumo, ResumoDoArranque::default());
        assert_eq!(acervo.blobs_no_disco(), 0);
    }

    /// A detecção reconhece as duas formas de legado.
    ///
    /// Gate do gate: uma detecção que nunca acha nada faria o arranque virar
    /// no-op silencioso, e o backfill voltaria a ser código sem chamador — que
    /// é exatamente o defeito que esta fatia veio corrigir.
    #[test]
    fn a_deteccao_reconhece_campo_direto_e_documento() {
        let acervo = Acervo::novo();
        assert!(
            !ha_legado_para_converter(&acervo.conexao()).expect("detectar"),
            "banco vazio não tem legado"
        );

        let (url, _) = inline("imagem");
        acervo
            .conexao()
            .execute(
                "INSERT INTO universes (id, name, cover_image, created_at, updated_at)
                 VALUES ('u1','U',?1,'2026-01-01','2026-01-01')",
                [&url],
            )
            .expect("campo direto");
        assert!(
            ha_legado_para_converter(&acervo.conexao()).expect("detectar"),
            "campo direto preenchido é legado"
        );

        let outro = Acervo::novo();
        outro
            .conexao()
            .execute_batch(&format!(
                "INSERT INTO universes (id, name, created_at, updated_at)
                    VALUES ('u1','U','2026-01-01','2026-01-01');
                 INSERT INTO stories (id, universe_id, name, created_at, updated_at)
                    VALUES ('s1','u1','S','2026-01-01','2026-01-01');
                 INSERT INTO books (id, story_id, name, created_at, updated_at)
                    VALUES ('b1','s1','L','2026-01-01','2026-01-01');
                 INSERT INTO chapters (id, book_id, title, content, word_count)
                    VALUES ('cap1','b1','Cap','<img src=\"{url}\">', 1);"
            ))
            .expect("documento");
        assert!(
            ha_legado_para_converter(&outro.conexao()).expect("detectar"),
            "documento com imagem embutida é legado"
        );
    }
}
