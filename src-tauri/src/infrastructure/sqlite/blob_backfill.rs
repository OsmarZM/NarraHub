//! Mover os bytes para o blob store sem nunca ficar sem cópia.
//!
//! ADR 0010. Esta fatia cobre as **seis superfícies de campo direto** — a
//! coluna é a `data:` URL e nada mais. As quatro de documento têm forma
//! própria e vêm depois.
//!
//! ## Os quatro estados, e o quarto é o que exige cuidado
//!
//! ```text
//! hash      inline     ação
//! ────────  ─────────  ──────────────────────────────────────────────────────
//! vazio     vazio      nada a migrar
//! vazio     válido     publica, verifica, grava o hash, e SÓ ENTÃO limpa
//! vazio     inválido   preserva o valor exato, registra pendência
//! cheio     cheio      confere o blob no disco:
//!                        válido   → limpa o inline
//!                        ausente e inline válido → reconstrói
//!                        ausente e inline inválido → preserva, pendência
//! ```
//!
//! **Nunca limpar os bytes antigos antes de haver blob válido publicado.**
//! Enquanto o hash está vazio, o inline é a única cópia daquele arquivo.
//!
//! ## Duas gravações por linha, de propósito
//!
//! ```text
//! commit 1   grava hash e MIME     ← o inline ainda está lá
//! commit 2   limpa o inline
//! ```
//!
//! Uma transação só seria mais simples e continuaria correta, mas o estado
//! intermediário é pior: entre publicar o blob e commitar tudo, uma queda
//! deixaria a linha inteira por fazer. Com duas, o intermediário é
//! **recuperável dos dois lados** — a linha tem hash e ainda tem os bytes, e
//! uma segunda passada cai no quarto estado, confere o blob e termina. É por
//! isso que o quarto estado existe na tabela acima, e não é um detalhe de
//! implementação.
//!
//! Morrer no meio, portanto, nunca perde arquivo. E rodar de novo é seguro:
//! a publicação é deduplicada pelo endereço, e a pendência é única por
//! `(superfície, linha, lado, motivo)` no schema.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::data_url::{classificar, Legado};
use crate::infrastructure::blob_store::{e_hash_canonico, BlobStore};
use crate::infrastructure::sqlite::blob_surfaces::{Forma, Superficie, CATALOGO};
use rusqlite::Connection;

/// O que a passada fez. Serve ao relatório e aos gates.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Resumo {
    /// Inline válido que virou blob nesta passada.
    pub migrados: usize,
    /// Referência que existia com o blob ausente, e o inline salvou.
    pub reconstruidos: usize,
    /// Linhas cujo inline foi limpo por haver blob válido.
    pub inline_limpo: usize,
    /// Linhas que já estavam prontas: hash válido, inline vazio.
    pub ja_prontos: usize,
    /// Pendências registradas ou reabertas.
    pub pendencias: usize,
}

/// Uma pendência aberta, como o bootstrap precisa ler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pendencia {
    pub superficie: u8,
    pub tabela: String,
    pub row_id: String,
    pub lado: String,
    pub motivo: String,
    pub detalhe: String,
}

/// Migra as seis superfícies de campo direto.
///
/// Idempotente: chamar duas vezes seguidas não muda nada na segunda, e chamar
/// depois de uma queda termina o que ficou pela metade.
pub fn migrar_campos_diretos(
    connection: &Connection,
    store: &BlobStore,
) -> DatabaseCommandResult<Resumo> {
    let mut resumo = Resumo::default();
    for superficie in CATALOGO
        .iter()
        .filter(|superficie| superficie.forma == Forma::CampoDireto)
    {
        migrar_uma_superficie(connection, store, superficie, &mut resumo)?;
    }
    Ok(resumo)
}

fn migrar_uma_superficie(
    connection: &Connection,
    store: &BlobStore,
    superficie: &Superficie,
    resumo: &mut Resumo,
) -> DatabaseCommandResult<()> {
    // Uma superfície de campo direto tem exatamente um par, e o catálogo tem
    // gate para isso. O `else` existe para não silenciar um catálogo que mude.
    let Some(referencia) = superficie.referencias.first() else {
        return Err(DatabaseCommandError::storage(format!(
            "a superfície {} é campo direto e não declara par hash/MIME",
            superficie.numero
        )));
    };
    let tabela = superficie.tabela;
    let (legada, coluna_hash, coluna_mime) = (referencia.legada, referencia.hash, referencia.mime);

    // Lê tudo de uma vez. O acervo de um escritor cabe em memória para as
    // colunas de identificação, e ler linha a linha enquanto grava na mesma
    // conexão é o que faz um cursor SQLite ver metade do próprio trabalho.
    let linhas: Vec<(String, String, String)> = connection
        .prepare(&format!(
            "SELECT id, {legada}, {coluna_hash} FROM {tabela}
              WHERE {legada} <> '' OR {coluna_hash} <> ''"
        ))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    for (id, inline, hash) in linhas {
        let legado = classificar(&inline);

        // ── quarto estado: já há referência ─────────────────────────────────
        if !hash.is_empty() {
            let blob_esta_la = e_hash_canonico(&hash) && store.verify(&hash)?;
            if blob_esta_la {
                if !inline.is_empty() {
                    limpar_inline(connection, tabela, legada, &id)?;
                    resumo.inline_limpo += 1;
                } else {
                    resumo.ja_prontos += 1;
                }
                resolver_pendencias(connection, superficie.numero, &id, legada)?;
                continue;
            }

            // Referência sem blob. O inline é a chance de reconstruir.
            match &legado {
                Legado::Decodificada { mime, bytes } => {
                    if publicar_e_referenciar(
                        connection,
                        store,
                        superficie,
                        &id,
                        legada,
                        coluna_hash,
                        coluna_mime,
                        mime,
                        bytes,
                        resumo,
                    )? {
                        resumo.reconstruidos += 1;
                    }
                }
                _ => {
                    let detalhe = match &legado {
                        Legado::Vazio => {
                            "a referência não tem blob no disco e não há inline para reconstruir"
                                .to_string()
                        }
                        Legado::NaoReconhecido { motivo } => {
                            format!("a referência não tem blob no disco, e {motivo}")
                        }
                        Legado::Decodificada { .. } => unreachable!("tratado acima"),
                    };
                    registrar_pendencia(
                        connection,
                        superficie.numero,
                        tabela,
                        &id,
                        legada,
                        "blob_missing",
                        &detalhe,
                    )?;
                    resumo.pendencias += 1;
                }
            }
            continue;
        }

        // ── segundo e terceiro estados ──────────────────────────────────────
        match &legado {
            Legado::Vazio => {}
            Legado::Decodificada { mime, bytes } => {
                if publicar_e_referenciar(
                    connection,
                    store,
                    superficie,
                    &id,
                    legada,
                    coluna_hash,
                    coluna_mime,
                    mime,
                    bytes,
                    resumo,
                )? {
                    resumo.migrados += 1;
                }
            }
            Legado::NaoReconhecido { motivo } => {
                registrar_pendencia(
                    connection,
                    superficie.numero,
                    tabela,
                    &id,
                    legada,
                    "legacy_unrecognized",
                    motivo,
                )?;
                resumo.pendencias += 1;
            }
        }
    }
    Ok(())
}

/// Publica, verifica e faz a linha apontar para o blob.
///
/// Devolve `false` quando não publicou — e o `bool` não é enfeite. A primeira
/// versão devolvia `()`, registrava a pendência no caminho de falha e voltava
/// `Ok`; o chamador incrementava `migrados` logo depois, **incondicionalmente**.
/// Falha de escrita no disco era contada como migração bem-sucedida no resumo.
///
/// É a forma exata de falso verde que esta etapa existe para não produzir: o
/// relatório diria "seis assets migrados" com zero migrados e seis pendências
/// abertas.
#[allow(clippy::too_many_arguments)]
fn publicar_e_referenciar(
    connection: &Connection,
    store: &BlobStore,
    superficie: &Superficie,
    id: &str,
    legada: &str,
    coluna_hash: &str,
    coluna_mime: &str,
    mime: &str,
    bytes: &[u8],
    resumo: &mut Resumo,
) -> DatabaseCommandResult<bool> {
    // Publica primeiro. Enquanto isto não termina, o banco não sabe de nada:
    // um blob órfão é aceitável (não há GC nesta etapa), uma referência sem
    // blob não é.
    let publicado = match store.put(bytes) {
        Ok(hash) => hash,
        Err(erro) => {
            registrar_pendencia(
                connection,
                superficie.numero,
                superficie.tabela,
                id,
                legada,
                "blob_write_failed",
                &recortar(&erro.message),
            )?;
            resumo.pendencias += 1;
            return Ok(false);
        }
    };

    // Confere no disco antes de o banco passar a apontar para lá. `put` já
    // verificou o temporário, e esta é a segunda leitura de propósito: é a
    // última chance de a referência não nascer apontando para nada.
    //
    // NÃO COBERTA POR TESTE, e o motivo é estrutural. Desligar esta checagem
    // sobreviveu à mutação porque nenhum teste unitário consegue fazer `put`
    // ter sucesso e o arquivo publicado estar errado: `put` calcula o hash do
    // temporário **lendo o disco** e só então renomeia. A falha que esta linha
    // cobre — corrupção entre o `rename` e a primeira leitura, ou um `rename`
    // que reportou sucesso sem ter escrito — não é injetável de fora sem um
    // gancho de teste dentro do caminho de produção.
    //
    // Fica, e fica declarada como não provada. O custo é uma releitura por
    // asset migrado; o que ela evita é uma referência nascer apontando para
    // nada, com o inline limpo em seguida.
    if !store.verify(&publicado)? {
        registrar_pendencia(
            connection,
            superficie.numero,
            superficie.tabela,
            id,
            legada,
            "blob_write_failed",
            "o blob publicado não conferiu na releitura do disco",
        )?;
        resumo.pendencias += 1;
        return Ok(false);
    }

    // Commit 1: a referência passa a existir, com o inline ainda no lugar.
    connection
        .execute(
            &format!(
                "UPDATE {} SET {coluna_hash} = ?1, {coluna_mime} = ?2 WHERE id = ?3",
                superficie.tabela
            ),
            rusqlite::params![&publicado, mime, id],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // Commit 2: e agora sim o inline sai.
    limpar_inline(connection, superficie.tabela, legada, id)?;
    resumo.inline_limpo += 1;
    resolver_pendencias(connection, superficie.numero, id, legada)?;
    Ok(true)
}

fn limpar_inline(
    connection: &Connection,
    tabela: &str,
    legada: &str,
    id: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            &format!("UPDATE {tabela} SET {legada} = '' WHERE id = ?1"),
            [id],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Registra a pendência, ou **reabre** a que já estava lá.
///
/// A identidade é `(superfície, linha, lado, motivo)`, garantida por `UNIQUE`
/// no schema. O `detected_at` original é preservado: a pergunta "desde quando
/// este asset está pendente?" tem uma resposta só, e reescrever a data a cada
/// passada do backfill apagaria a única evidência de há quanto tempo o
/// escritor tem um arquivo que não migra.
fn registrar_pendencia(
    connection: &Connection,
    superficie: u8,
    tabela: &str,
    row_id: &str,
    lado: &str,
    motivo: &str,
    detalhe: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO blob_migration_issues
                (id, surface, table_name, row_id, side, reason, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (surface, row_id, side, reason)
             DO UPDATE SET resolved_at = '', detail = excluded.detail",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                superficie,
                tabela,
                row_id,
                lado,
                motivo,
                recortar(detalhe),
            ],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Fecha as pendências daquele lado daquela linha.
///
/// Qualquer motivo: o que estava pendente era o asset, e ele migrou.
fn resolver_pendencias(
    connection: &Connection,
    superficie: u8,
    row_id: &str,
    lado: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "UPDATE blob_migration_issues
                SET resolved_at = datetime('now')
              WHERE surface = ?1 AND row_id = ?2 AND side = ?3 AND resolved_at = ''",
            rusqlite::params![superficie, row_id, lado],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// O `detail` tem `CHECK (length(detail) <= 200)` no schema.
///
/// Recortar aqui é o que impede a gravação de estourar no meio de um backfill
/// por causa de uma mensagem de erro do sistema de arquivos mais longa que o
/// esperado. O corte é em fronteira de caractere.
fn recortar(texto: &str) -> String {
    const LIMITE: usize = 200;
    if texto.len() <= LIMITE {
        return texto.to_string();
    }
    let mut fim = LIMITE;
    while fim > 0 && !texto.is_char_boundary(fim) {
        fim -= 1;
    }
    texto[..fim].to_string()
}

/// As pendências abertas, para o bootstrap e para a tela.
pub fn pendencias_abertas(connection: &Connection) -> DatabaseCommandResult<Vec<Pendencia>> {
    connection
        .prepare(
            "SELECT surface, table_name, row_id, side, reason, detail
               FROM blob_migration_issues
              WHERE resolved_at = ''
              ORDER BY surface, table_name, row_id, side",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .query_map([], |row| {
            Ok(Pendencia {
                superficie: row.get::<_, i64>(0)? as u8,
                tabela: row.get(1)?,
                row_id: row.get(2)?,
                lado: row.get(3)?,
                motivo: row.get(4)?,
                detalhe: row.get(5)?,
            })
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::data_url::decodificar_base64;
    use crate::infrastructure::blob_store::hash_dos_bytes;
    use crate::infrastructure::sqlite::blob_surfaces::superficie_de;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    /// As seis superfícies diretas, com a coluna legada e o par de referência.
    ///
    /// Derivado do catálogo, e não de uma segunda lista: uma superfície nova
    /// entra nos gates sozinha, e uma que mude de nome os quebra na hora.
    fn diretas() -> Vec<(&'static str, &'static str, &'static str, &'static str, u8)> {
        CATALOGO
            .iter()
            .filter(|superficie| superficie.forma == Forma::CampoDireto)
            .map(|superficie| {
                let referencia = superficie.referencias[0];
                (
                    superficie.tabela,
                    referencia.legada,
                    referencia.hash,
                    referencia.mime,
                    superficie.numero,
                )
            })
            .collect()
    }

    struct Acervo {
        banco: TemporaryDatabase,
        raiz: PathBuf,
        store: BlobStore,
    }

    impl Acervo {
        fn novo() -> Self {
            let raiz =
                std::env::temp_dir().join(format!("narrahub-backfill-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&raiz).expect("criar raiz");
            Self {
                banco: TemporaryDatabase::new(),
                store: BlobStore::new(&raiz),
                raiz,
            }
        }

        /// Semeia uma linha em cada superfície direta, todas com o mesmo `id`
        /// lógico por tabela, e devolve a conexão pronta.
        fn semear(&self, conexao: &Connection, valor: &str) {
            conexao
                .execute_batch(
                    "INSERT INTO universes (id, name, created_at, updated_at)
                        VALUES ('u1','Universo','2026-01-01','2026-01-01');
                     INSERT INTO stories (id, universe_id, name, created_at, updated_at)
                        VALUES ('s1','u1','Historia','2026-01-01','2026-01-01');
                     INSERT INTO books (id, story_id, name, created_at, updated_at)
                        VALUES ('b1','s1','Livro','2026-01-01','2026-01-01');
                     INSERT INTO entities (id, universe_id, type, name)
                        VALUES ('e1','u1','character','Alguem');
                     INSERT INTO canvas_nodes (id, universe_id, kind)
                        VALUES ('c1','u1','image');
                     INSERT INTO planning_items
                        (id, universe_id, title, created_at, updated_at)
                        VALUES ('p1','u1','Cena','2026-01-01','2026-01-01');
                     INSERT INTO attachments
                        (id, universe_id, owner_type, owner_id, data_url, created_at)
                        VALUES ('a1','u1','entity','e1','','2026-01-01');",
                )
                .expect("semear o acervo");
            for (tabela, legada, _, _, _) in diretas() {
                conexao
                    .execute(&format!("UPDATE {tabela} SET {legada} = ?1"), [valor])
                    .unwrap_or_else(|error| panic!("semear {tabela}.{legada}: {error}"));
            }
        }

        fn ler(&self, conexao: &Connection, tabela: &str, coluna: &str) -> String {
            conexao
                .query_row(&format!("SELECT {coluna} FROM {tabela}"), [], |row| {
                    row.get(0)
                })
                .unwrap_or_else(|error| panic!("ler {tabela}.{coluna}: {error}"))
        }

        fn blobs_no_disco(&self) -> Vec<PathBuf> {
            fn varrer(diretorio: &std::path::Path, achados: &mut Vec<PathBuf>) {
                let Ok(filhos) = std::fs::read_dir(diretorio) else {
                    return;
                };
                for filho in filhos.flatten() {
                    let caminho = filho.path();
                    if caminho.is_dir() {
                        varrer(&caminho, achados);
                    } else {
                        achados.push(caminho);
                    }
                }
            }
            let mut achados = Vec::new();
            varrer(&self.store.raiz(), &mut achados);
            achados
        }
    }

    impl Drop for Acervo {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.raiz).ok();
        }
    }

    /// `data:image/png;base64,` de um conteúdo qualquer, com os bytes reais.
    fn inline_valido(conteudo: &str) -> (String, Vec<u8>) {
        // Codifica à mão para não depender de um codificador que não existe.
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
        let url = format!("data:image/png;base64,{texto}");
        // E confere contra o decodificador de produção, para o cenário não
        // nascer torto: um gate que semeia base64 inválida acreditando ser
        // válida prova o contrário do que pretende.
        assert_eq!(
            decodificar_base64(&texto).expect("o cenário precisa de base64 válida"),
            bytes
        );
        (url, bytes)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // O caminho principal
    // ═══════════════════════════════════════════════════════════════════════

    /// **Inline válido vira blob, e o inline sai — nessa ordem.**
    #[test]
    fn inline_valido_vira_blob_e_o_inline_sai() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("os-bytes-da-imagem");
        acervo.semear(&conexao, &url);

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.migrados, 6, "seis superfícies diretas");
        assert_eq!(resumo.pendencias, 0);

        let esperado = hash_dos_bytes(&bytes);
        for (tabela, legada, coluna_hash, coluna_mime, _) in diretas() {
            assert_eq!(
                acervo.ler(&conexao, tabela, coluna_hash),
                esperado,
                "{tabela}.{coluna_hash}"
            );
            assert_eq!(acervo.ler(&conexao, tabela, coluna_mime), "image/png");
            assert_eq!(
                acervo.ler(&conexao, tabela, legada),
                "",
                "{tabela}.{legada} tinha que ter sido limpa depois do hash"
            );
        }
        assert_eq!(
            acervo.store.read(&esperado).expect("ler o blob"),
            bytes,
            "os bytes no disco têm que ser os bytes do escritor"
        );
    }

    /// **A deduplicação atravessa as seis superfícies.**
    ///
    /// A mesma arte como capa de universo, capa de livro, imagem de entidade,
    /// node do canvas, imagem de cena e anexo: um arquivo. É o objetivo
    /// declarado da etapa, e ele só existe porque o endereço é o conteúdo — o
    /// store não sabe de qual coluna os bytes vieram.
    #[test]
    fn a_deduplicacao_atravessa_as_seis_superficies() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, _) = inline_valido("a-mesma-arte-em-tudo");
        acervo.semear(&conexao, &url);

        migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");

        let hashes: BTreeSet<String> = diretas()
            .into_iter()
            .map(|(tabela, _, coluna_hash, _, _)| acervo.ler(&conexao, tabela, coluna_hash))
            .collect();
        assert_eq!(hashes.len(), 1, "seis referências, um endereço: {hashes:?}");
        assert_eq!(
            acervo.blobs_no_disco().len(),
            1,
            "e um arquivo no disco, não seis"
        );
    }

    /// Linha sem nada não vira pendência nem trabalho.
    #[test]
    fn linha_sem_asset_nao_vira_pendencia() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        acervo.semear(&conexao, "");

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo, Resumo::default(), "não havia o que fazer");
        assert!(pendencias_abertas(&conexao).expect("ler").is_empty());
        assert!(acervo.blobs_no_disco().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Legado inválido
    // ═══════════════════════════════════════════════════════════════════════

    /// **Legado inválido é preservado exatamente, com pendência.**
    ///
    /// Falha de migração não transforma dado desconhecido em ausência. Nada de
    /// baixar a URL externa, abrir o caminho local, ou calcular hash de string
    /// desconhecida — e o valor continua byte a byte onde estava, porque
    /// enquanto o hash está vazio ele é a única cópia daquilo.
    #[test]
    fn legado_invalido_e_preservado_com_pendencia() {
        for valor in [
            "https://cdn.exemplo.com/capa.png",
            "C:\\Users\\alguem\\capa.png",
            "data:image/png;base64,!!!nao-abre!!!",
            "capa.png",
        ] {
            let acervo = Acervo::novo();
            let conexao = acervo.banco.connection();
            acervo.semear(&conexao, valor);

            let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");
            assert_eq!(resumo.migrados, 0, "{valor:?} não devia ter migrado");
            assert_eq!(resumo.pendencias, 6);
            assert!(
                acervo.blobs_no_disco().is_empty(),
                "{valor:?} não podia produzir arquivo nenhum"
            );

            for (tabela, legada, coluna_hash, coluna_mime, numero) in diretas() {
                assert_eq!(
                    acervo.ler(&conexao, tabela, legada),
                    valor,
                    "{tabela}.{legada} mudou, e era a única cópia"
                );
                assert_eq!(acervo.ler(&conexao, tabela, coluna_hash), "");
                assert_eq!(acervo.ler(&conexao, tabela, coluna_mime), "");

                let pendencia = pendencias_abertas(&conexao)
                    .expect("ler")
                    .into_iter()
                    .find(|pendencia| pendencia.tabela == tabela)
                    .unwrap_or_else(|| panic!("faltou pendência para {tabela}"));
                assert_eq!(pendencia.superficie, numero);
                assert_eq!(pendencia.lado, legada, "a pendência nomeia a coluna");
                assert_eq!(pendencia.motivo, "legacy_unrecognized");
                assert!(
                    !pendencia.detalhe.is_empty() && !pendencia.detalhe.contains(valor),
                    "o detalhe não pode carregar o valor: {:?}",
                    pendencia.detalhe
                );
            }
        }
    }

    /// **Rodar de novo não muda nada, e não move a data de detecção.**
    ///
    /// A idempotência é do backfill inteiro, não só do caminho feliz: o legado
    /// inválido tem que continuar com **uma** pendência, e com a data em que
    /// foi visto pela primeira vez. Reescrever `detected_at` a cada passada
    /// apagaria a única evidência de há quanto tempo o escritor tem um arquivo
    /// que não migra — e é a pergunta que o bootstrap faz.
    #[test]
    fn rodar_duas_vezes_nao_muda_nada() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, _) = inline_valido("imagem-boa");
        acervo.semear(&conexao, &url);
        // Uma superfície com legado inválido, para as duas metades entrarem.
        conexao
            .execute(
                "UPDATE entities SET image = 'https://exemplo.com/x.png'",
                [],
            )
            .expect("um legado inválido no meio");

        let primeira = migrar_campos_diretos(&conexao, &acervo.store).expect("primeira");
        assert_eq!(primeira.migrados, 5);
        assert_eq!(primeira.pendencias, 1);

        let deteccao_antes: String = conexao
            .query_row("SELECT detected_at FROM blob_migration_issues", [], |row| {
                row.get(0)
            })
            .expect("ler a data");
        let arquivos_antes = acervo.blobs_no_disco().len();

        let segunda = migrar_campos_diretos(&conexao, &acervo.store).expect("segunda");
        assert_eq!(
            segunda,
            Resumo {
                ja_prontos: 5,
                pendencias: 1,
                ..Resumo::default()
            },
            "a segunda passada só reconhece o que já está feito e a pendência que continua"
        );
        assert_eq!(
            acervo.blobs_no_disco().len(),
            arquivos_antes,
            "a segunda passada não pode criar arquivo"
        );

        let pendencias: i64 = conexao
            .query_row("SELECT COUNT(*) FROM blob_migration_issues", [], |row| {
                row.get(0)
            })
            .expect("contar");
        assert_eq!(pendencias, 1, "a pendência não pode duplicar");
        let deteccao_depois: String = conexao
            .query_row("SELECT detected_at FROM blob_migration_issues", [], |row| {
                row.get(0)
            })
            .expect("ler a data");
        assert_eq!(
            deteccao_antes, deteccao_depois,
            "a data da primeira detecção tem que sobreviver às passadas seguintes"
        );
    }

    /// A pendência fecha quando o escritor conserta o valor.
    #[test]
    fn pendencia_fecha_quando_o_asset_finalmente_migra() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        acervo.semear(&conexao, "https://exemplo.com/capa.png");
        migrar_campos_diretos(&conexao, &acervo.store).expect("primeira");
        assert_eq!(pendencias_abertas(&conexao).expect("ler").len(), 6);

        // O escritor troca a URL externa por uma imagem de verdade.
        let (url, _) = inline_valido("agora-vai");
        for (tabela, legada, _, _, _) in diretas() {
            conexao
                .execute(&format!("UPDATE {tabela} SET {legada} = ?1"), [&url])
                .expect("consertar");
        }

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("segunda");
        assert_eq!(resumo.migrados, 6);
        assert!(
            pendencias_abertas(&conexao).expect("ler").is_empty(),
            "as pendências tinham que ter sido fechadas"
        );
        // Fechadas, não apagadas: o histórico de que houve pendência fica.
        let total: i64 = conexao
            .query_row("SELECT COUNT(*) FROM blob_migration_issues", [], |row| {
                row.get(0)
            })
            .expect("contar");
        assert_eq!(total, 6);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // O quarto estado: referência já existe
    // ═══════════════════════════════════════════════════════════════════════

    /// **Morrer entre as duas gravações não perde arquivo.**
    ///
    /// O estado intermediário do desenho: o hash já foi gravado e o inline
    /// ainda está lá. É o que uma queda de energia entre os dois commits
    /// deixa, e a segunda passada tem que terminar o serviço — conferir o blob
    /// e limpar o inline.
    #[test]
    fn morrer_entre_as_duas_gravacoes_nao_perde_arquivo() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("imagem-do-escritor");
        acervo.semear(&conexao, &url);

        // Reproduz o estado: blob publicado, hash gravado, inline intacto.
        let hash = acervo.store.put(&bytes).expect("publicar");
        for (tabela, _, coluna_hash, coluna_mime, _) in diretas() {
            conexao
                .execute(
                    &format!("UPDATE {tabela} SET {coluna_hash} = ?1, {coluna_mime} = 'image/png'"),
                    [&hash],
                )
                .expect("gravar como se o commit 1 tivesse acontecido");
        }

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("retomar");
        assert_eq!(resumo.inline_limpo, 6);
        assert_eq!(resumo.migrados, 0, "não havia o que publicar de novo");
        for (tabela, legada, _, _, _) in diretas() {
            assert_eq!(acervo.ler(&conexao, tabela, legada), "");
        }
        assert_eq!(acervo.store.read(&hash).expect("ler"), bytes);
    }

    /// **Blob perdido do disco é reconstruído enquanto o inline ainda existir.**
    ///
    /// É a razão de o inline só sair depois de o hash entrar. Se a ordem fosse
    /// a outra, este cenário seria perda definitiva.
    #[test]
    fn blob_perdido_e_reconstruido_do_inline() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("imagem-que-o-disco-perdeu");
        acervo.semear(&conexao, &url);

        let hash = hash_dos_bytes(&bytes);
        for (tabela, _, coluna_hash, coluna_mime, _) in diretas() {
            conexao
                .execute(
                    &format!("UPDATE {tabela} SET {coluna_hash} = ?1, {coluna_mime} = 'image/png'"),
                    [&hash],
                )
                .expect("referência gravada");
        }
        assert!(
            !acervo.store.has(&hash).expect("presença"),
            "o cenário é justamente o blob NÃO estar lá"
        );

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");

        // Uma reconstrução, cinco limpezas — e não seis reconstruções.
        //
        // Escrevi 6 aqui na primeira versão e o gate reprovou. O código estava
        // certo: as seis referências apontam para o MESMO endereço, porque o
        // conteúdo é o mesmo. A primeira linha republica o arquivo e, a partir
        // dali, as outras cinco encontram o blob no disco e só limpam o inline.
        //
        // É a deduplicação aparecendo na recuperação: um arquivo perdido é
        // devolvido uma vez, e todas as referências voltam a funcionar juntas.
        assert_eq!(resumo.reconstruidos, 1);
        assert_eq!(resumo.inline_limpo, 6, "uma na reconstrução, cinco depois");
        assert!(acervo.store.verify(&hash).expect("integridade"));
        assert_eq!(acervo.store.read(&hash).expect("ler"), bytes);
        for (tabela, legada, _, _, _) in diretas() {
            assert_eq!(acervo.ler(&conexao, tabela, legada), "");
        }
        assert!(pendencias_abertas(&conexao).expect("ler").is_empty());
    }

    /// Referência sem blob e sem inline: pendência, e a referência fica.
    ///
    /// Apagar o hash seria transformar "o arquivo está faltando" em "nunca
    /// houve arquivo" — a mesma troca que a política do legado inválido
    /// proíbe, do outro lado.
    #[test]
    fn referencia_sem_blob_e_sem_inline_vira_pendencia() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        acervo.semear(&conexao, "");

        let hash = hash_dos_bytes(b"um arquivo que nunca chegou");
        for (tabela, _, coluna_hash, _, _) in diretas() {
            conexao
                .execute(&format!("UPDATE {tabela} SET {coluna_hash} = ?1"), [&hash])
                .expect("referência órfã");
        }

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.pendencias, 6);
        assert_eq!(resumo.reconstruidos, 0);
        for (tabela, _, coluna_hash, _, _) in diretas() {
            assert_eq!(
                acervo.ler(&conexao, tabela, coluna_hash),
                hash,
                "a referência não pode ser apagada: faltar arquivo não é não ter arquivo"
            );
        }
        let motivos: BTreeSet<String> = pendencias_abertas(&conexao)
            .expect("ler")
            .into_iter()
            .map(|pendencia| pendencia.motivo)
            .collect();
        assert_eq!(motivos, BTreeSet::from(["blob_missing".to_string()]));
    }

    /// **Referência que não é hash canônico não vira caminho de arquivo.**
    ///
    /// A coluna é `TEXT`, então nada no schema impede alguém — ou um banco
    /// importado, ou uma versão antiga — de gravar `../../etc/passwd` ali. O
    /// backfill valida antes de tocar no disco, e com o inline em mão
    /// reconstrói para o endereço certo.
    #[test]
    fn referencia_nao_canonica_nao_vira_caminho() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("a-imagem-certa");
        acervo.semear(&conexao, &url);

        for (tabela, _, coluna_hash, _, _) in diretas() {
            conexao
                .execute(
                    &format!("UPDATE {tabela} SET {coluna_hash} = '../../../etc/passwd'"),
                    [],
                )
                .expect("referência torta");
        }

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar sem estourar");
        assert_eq!(resumo.reconstruidos, 6);

        let certo = hash_dos_bytes(&bytes);
        for (tabela, _, coluna_hash, _, _) in diretas() {
            assert_eq!(acervo.ler(&conexao, tabela, coluna_hash), certo);
        }
        assert_eq!(
            acervo.blobs_no_disco().len(),
            1,
            "e nenhum arquivo fora da raiz dos blobs"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Fronteira desta fatia
    // ═══════════════════════════════════════════════════════════════════════

    /// **Esta fatia não toca nas quatro superfícies de documento.**
    ///
    /// `chapters.content` guarda um documento com 0..N imagens dentro, e a
    /// transformação dele é outra coisa. Se o backfill de campo direto
    /// tentasse tratá-lo como uma `data:` URL, o documento inteiro viraria
    /// `legacy_unrecognized` — e o bootstrap ficaria bloqueado por capítulo
    /// que nunca teve mídia.
    #[test]
    fn o_backfill_de_campo_direto_nao_toca_em_documento() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        acervo.semear(&conexao, "");

        let documento = "<p>Texto</p><img src=\"data:image/png;base64,Zm9v\" alt=\"x\">";
        conexao
            .execute(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('cap1','b1','Capitulo',?1, 1)",
                [documento],
            )
            .expect("semear capítulo");

        let resumo = migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo, Resumo::default());
        assert_eq!(
            acervo.ler(&conexao, "chapters", "content"),
            documento,
            "o documento não podia ser tocado por esta fatia"
        );
        assert!(pendencias_abertas(&conexao).expect("ler").is_empty());

        // E o catálogo continua sabendo que são quatro, para o dia em que a
        // fatia de documento chegar.
        let documentos = CATALOGO
            .iter()
            .filter(|superficie| superficie.forma == Forma::DocumentoTiptap)
            .count();
        assert_eq!(documentos, 4);
        assert_eq!(
            superficie_de("chapters").expect("superfície 7").forma,
            Forma::DocumentoTiptap
        );
    }

    /// **A ordem é hash primeiro, inline depois — e isto observa a ordem.**
    ///
    /// Todos os outros gates conferem o estado **final**, e o estado final das
    /// duas ordens é idêntico: hash gravado, inline vazio, blob no disco. A
    /// diferença aparece só se a máquina morrer no meio:
    ///
    /// ```text
    /// ordem certa    hash  →  queda  →  linha com hash E com os bytes, recuperável
    /// ordem errada   limpa →  queda  →  linha sem hash e SEM os bytes, perdida
    /// ```
    ///
    /// Um teste que só olha o depois não distingue as duas, e uma inversão de
    /// duas linhas passaria verde para sempre. Então a ordem é observada de
    /// dentro do banco: dois gatilhos registram o instante de cada mudança, e
    /// o gate lê a sequência.
    #[test]
    fn o_hash_e_gravado_antes_de_o_inline_sair() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, _) = inline_valido("imagem-para-observar-a-ordem");
        acervo.semear(&conexao, &url);

        conexao
            .execute_batch(
                "CREATE TABLE ordem_observada (
                     momento INTEGER PRIMARY KEY AUTOINCREMENT,
                     passo TEXT NOT NULL
                 );
                 CREATE TRIGGER obs_hash AFTER UPDATE OF blob_hash ON attachments
                 WHEN OLD.blob_hash = '' AND NEW.blob_hash <> ''
                 BEGIN
                     INSERT INTO ordem_observada (passo) VALUES ('hash');
                 END;
                 CREATE TRIGGER obs_inline AFTER UPDATE OF data_url ON attachments
                 WHEN OLD.data_url <> '' AND NEW.data_url = ''
                 BEGIN
                     INSERT INTO ordem_observada (passo) VALUES ('inline-limpo');
                 END;",
            )
            .expect("instalar os observadores");

        migrar_campos_diretos(&conexao, &acervo.store).expect("migrar");

        let sequencia: Vec<String> = conexao
            .prepare("SELECT passo FROM ordem_observada ORDER BY momento")
            .expect("preparar")
            .query_map([], |row| row.get(0))
            .expect("consultar")
            .collect::<Result<Vec<_>, _>>()
            .expect("ler");

        assert_eq!(
            sequencia,
            vec!["hash".to_string(), "inline-limpo".to_string()],
            "a referência tem que existir antes de os bytes saírem. Nesta ordem, uma queda \
             entre as duas gravações deixa a linha recuperável; na outra, o arquivo do \
             escritor simplesmente não existe mais."
        );
    }

    /// **Falha de escrita no store vira pendência, e o inline fica.**
    ///
    /// O ramo `blob_write_failed` não tinha gate nenhum. O cenário é forçado
    /// pelo caminho mais direto que existe: onde deveria haver o diretório dos
    /// blobs há um **arquivo**, então `create_dir_all` reprova e `put` devolve
    /// erro controlado.
    ///
    /// O que importa é o que NÃO acontece: nada de referência gravada, nada de
    /// inline limpo. O backfill não interrompe o acervo inteiro por causa de
    /// uma linha — ele registra e segue.
    #[test]
    fn falha_de_escrita_no_store_vira_pendencia_e_preserva_o_inline() {
        let acervo = Acervo::novo();
        let conexao = acervo.banco.connection();
        let (url, _) = inline_valido("imagem-que-nao-vai-caber");
        acervo.semear(&conexao, &url);

        // Um arquivo onde tinha que haver diretório.
        let raiz = acervo.store.raiz();
        std::fs::create_dir_all(raiz.parent().expect("pai")).expect("preparar");
        std::fs::write(&raiz, b"nao sou um diretorio").expect("bloquear a raiz");

        let resumo = migrar_campos_diretos(&conexao, &acervo.store)
            .expect("o backfill não pode abortar o acervo por causa de uma linha");
        assert_eq!(resumo.migrados, 0);
        assert_eq!(resumo.inline_limpo, 0, "nada podia ter sido limpo");
        assert_eq!(resumo.pendencias, 6);

        for (tabela, legada, coluna_hash, _, _) in diretas() {
            assert_eq!(
                acervo.ler(&conexao, tabela, legada),
                url,
                "{tabela}.{legada} é a única cópia e não podia ter sido tocada"
            );
            assert_eq!(acervo.ler(&conexao, tabela, coluna_hash), "");
        }
        let motivos: BTreeSet<String> = pendencias_abertas(&conexao)
            .expect("ler")
            .into_iter()
            .map(|pendencia| pendencia.motivo)
            .collect();
        assert_eq!(motivos, BTreeSet::from(["blob_write_failed".to_string()]));

        // E o detalhe não vaza o valor, mesmo vindo de uma mensagem do sistema
        // de arquivos.
        for pendencia in pendencias_abertas(&conexao).expect("ler") {
            assert!(pendencia.detalhe.len() <= 200);
            assert!(!pendencia.detalhe.contains(&url[..40]));
        }

        std::fs::remove_file(&raiz).ok();
    }
}
