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
use crate::infrastructure::blob_document;
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

// ═══════════════════════════════════════════════════════════════════════════
// As quatro superfícies de documento
// ═══════════════════════════════════════════════════════════════════════════

/// A ordem em que os documentos são migrados, e ela **não** é alfabética.
///
/// `chapters` vem antes de `chapter_revisions` por causa do gatilho — ver
/// [`migrar_documentos`].
///
/// **Inverter esta ordem não quebra nenhum gate, e isso é esperado.** A
/// obrigação existia enquanto o backfill fabricava revisões: as linhas que o
/// próprio `UPDATE` criasse ficariam para trás, com a base64 dentro, se
/// `chapter_revisions` já tivesse passado. Com a supressão funcionando,
/// nenhuma linha nova é criada, e a ordem deixa de ter efeito observável.
///
/// Fica como margem: no dia em que a supressão falhar — gatilho novo, versão
/// do SQLite que numere `rowid` de outro jeito, alguém removendo o `DELETE` —
/// a ordem é o que impede o defeito de virar perda de dado. Uma segunda
/// camada que só aparece quando a primeira cede não tem como ser provada por
/// mutação enquanto a primeira estiver de pé.
const ORDEM_DOS_DOCUMENTOS: &[&str] = &[
    "chapters",
    "chapter_revisions",
    "sync_conflicts",
    "collaboration_contributions",
];

/// Migra os documentos: `chapters`, `chapter_revisions`, e os dois lados de
/// `sync_conflicts` e `collaboration_contributions`.
///
/// ## O gatilho de revisão transforma a migração numa edição
///
/// ```sql
/// CREATE TRIGGER trg_chapter_revision
/// BEFORE UPDATE OF content, title ON chapters
/// WHEN OLD.content <> NEW.content OR OLD.title <> NEW.title
/// BEGIN
///   INSERT INTO chapter_revisions (…) VALUES (OLD.…);
/// END;
/// ```
///
/// Gravar o documento convertido é um `UPDATE` de `content`, então o gatilho
/// dispara e guarda a versão **anterior** — a base64 — como revisão nova:
///
/// ```text
/// UPDATE chapters SET content = '<img data-narrahub-blob=…>'
///   →  INSERT INTO chapter_revisions (content) VALUES (<a base64>)
/// ```
///
/// Duas consequências. A primeira é de ordem: `chapters` tem que ser migrada
/// **antes** de `chapter_revisions`, senão as linhas que o próprio backfill
/// acabou de criar ficam para trás com os bytes dentro.
///
/// A segunda é que o backfill **fabricaria uma revisão por capítulo**, datada
/// de hoje, cujo conteúdo é a mesma versão em outra codificação. Isso não é
/// mudança do escritor, e o histórico dele não deve registrar como se fosse.
/// Então a revisão fabricada é apagada — dentro da **mesma transação de
/// escrita** do `UPDATE`, e só as linhas cujo `id` não existia antes dele.
///
/// Apagar linha é o oposto da disciplina desta etapa, e a exceção precisa do
/// argumento: o que se apaga aqui foi criado pelo próprio backfill, segundos
/// antes, na mesma transação. Nada que o escritor tenha produzido é tocado. E
/// como o SQLite serializa escritores, não há terceiro capaz de inserir uma
/// revisão legítima naquela janela.
pub fn migrar_documentos(
    connection: &mut Connection,
    store: &BlobStore,
) -> DatabaseCommandResult<Resumo> {
    let mut resumo = Resumo::default();
    for nome in ORDEM_DOS_DOCUMENTOS {
        let Some(superficie) = CATALOGO.iter().find(|s| &s.tabela == nome) else {
            return Err(DatabaseCommandError::storage(format!(
                "a ordem de migração cita {nome}, que não está no catálogo"
            )));
        };
        if superficie.forma != Forma::DocumentoTiptap {
            return Err(DatabaseCommandError::storage(format!(
                "{nome} não é superfície de documento"
            )));
        }
        migrar_uma_tabela_de_documento(connection, store, superficie, &mut resumo)?;
    }
    Ok(resumo)
}

fn migrar_uma_tabela_de_documento(
    connection: &mut Connection,
    store: &BlobStore,
    superficie: &Superficie,
    resumo: &mut Resumo,
) -> DatabaseCommandResult<()> {
    let tabela = superficie.tabela;
    let recorte = superficie.clausula_do_recorte();

    for lado in superficie.colunas_legadas {
        // Uma leitura por lado. Os lados são independentes: falha num não
        // autoriza tocar no outro, e ler os dois juntos convidaria a tratá-los
        // como um par.
        let linhas: Vec<(String, String)> = connection
            .prepare(&format!(
                "SELECT id, {lado} FROM {tabela} WHERE ({recorte}) AND {lado} <> ''"
            ))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

        for (id, documento) in linhas {
            let conversao = match blob_document::converter(&documento, store) {
                Ok(conversao) => conversao,
                Err(erro) => {
                    // Documento que o parser recusa: preserva e registra. É a
                    // mesma política do legado inválido, um nível acima.
                    registrar_pendencia(
                        connection,
                        superficie.numero,
                        tabela,
                        &id,
                        lado,
                        "document_unparseable",
                        &recortar(&erro.message),
                    )?;
                    resumo.pendencias += 1;
                    continue;
                }
            };

            if conversao.mudou() {
                gravar_documento(connection, tabela, lado, &id, &conversao.html)?;
                resumo.migrados += conversao.convertidas;
                resumo.inline_limpo += 1;
            } else if conversao.pendencias.is_empty() {
                resumo.ja_prontos += 1;
            }

            if conversao.pendencias.is_empty() {
                resolver_pendencias(connection, superficie.numero, &id, lado)?;
            } else {
                for (indice, motivo) in &conversao.pendencias {
                    registrar_pendencia(
                        connection,
                        superficie.numero,
                        tabela,
                        &id,
                        lado,
                        "node_unrecognized",
                        &recortar(&format!("imagem {indice}: {motivo}")),
                    )?;
                }
                resumo.pendencias += conversao.pendencias.len();
            }
        }
    }
    Ok(())
}

/// Grava o documento convertido, sem deixar o gatilho fabricar revisão.
///
/// Para qualquer tabela que não seja `chapters` é um `UPDATE` simples. Para
/// `chapters`, o `UPDATE` e a limpeza da revisão fabricada acontecem na mesma
/// transação — ver [`migrar_documentos`].
fn gravar_documento(
    connection: &mut Connection,
    tabela: &str,
    lado: &str,
    id: &str,
    html: &str,
) -> DatabaseCommandResult<()> {
    if tabela != "chapters" {
        connection
            .execute(
                &format!("UPDATE {tabela} SET {lado} = ?1 WHERE id = ?2"),
                rusqlite::params![html, id],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        return Ok(());
    }

    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // A fronteira: tudo o que aparecer acima deste `rowid` nasceu do `UPDATE`
    // abaixo. Guardar a marca é mais barato que guardar a lista de ids, e não
    // depende da extensão JSON do SQLite.
    let marca: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(rowid), 0) FROM chapter_revisions",
            [],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    tx.execute(
        "UPDATE chapters SET content = ?1 WHERE id = ?2",
        rusqlite::params![html, id],
    )
    .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // A revisão que o gatilho acabou de fabricar, e **só** ela.
    let apagadas = tx
        .execute(
            "DELETE FROM chapter_revisions WHERE chapter_id = ?1 AND rowid > ?2",
            rusqlite::params![id, marca],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    // O gatilho insere exatamente uma linha por `UPDATE` de conteúdo. Se
    // apareceu mais de uma, alguma coisa aconteceu que este código não
    // entende — e apagar por atacado seria exatamente o erro que esta etapa
    // inteira existe para não cometer. Reprovar desfaz a transação.
    if apagadas > 1 {
        return Err(DatabaseCommandError::storage(format!(
            "o backfill ia apagar {apagadas} revisões do capítulo {id}, e o gatilho cria \
             uma por gravação. Nada foi apagado: a transação foi desfeita."
        )));
    }

    tx.commit()
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

    // ═══════════════════════════════════════════════════════════════════════
    // As quatro superfícies de documento
    // ═══════════════════════════════════════════════════════════════════════

    /// Semeia o mínimo para haver capítulo, e devolve o `id` dele.
    fn semear_capitulo(conexao: &Connection, conteudo: &str) -> String {
        conexao
            .execute_batch(
                "INSERT OR IGNORE INTO universes (id, name, created_at, updated_at)
                    VALUES ('u1','Universo','2026-01-01','2026-01-01');
                 INSERT OR IGNORE INTO stories (id, universe_id, name, created_at, updated_at)
                    VALUES ('s1','u1','Historia','2026-01-01','2026-01-01');
                 INSERT OR IGNORE INTO books (id, story_id, name, created_at, updated_at)
                    VALUES ('b1','s1','Livro','2026-01-01','2026-01-01');",
            )
            .expect("semear a hierarquia");
        conexao
            .execute(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('cap1','b1','Capitulo', ?1, 10)",
                [conteudo],
            )
            .expect("semear capítulo");
        "cap1".to_string()
    }

    fn conta(conexao: &Connection, sql: &str) -> i64 {
        conexao
            .query_row(sql, [], |row| row.get(0))
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    }

    fn ler_um(conexao: &Connection, sql: &str) -> String {
        conexao
            .query_row(sql, [], |row| row.get(0))
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    }

    /// **O documento do capítulo migra, e a referência fica no HTML.**
    #[test]
    fn documento_de_capitulo_migra_para_referencia() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("a-imagem-do-capitulo");
        semear_capitulo(
            &conexao,
            &format!("<p>Antes</p><img src=\"{url}\" alt=\"x\"><p>Depois</p>"),
        );

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.migrados, 1);
        assert_eq!(resumo.pendencias, 0);

        let hash = hash_dos_bytes(&bytes);
        let conteudo = ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'");
        assert!(conteudo.contains(&hash), "faltou a referência: {conteudo}");
        assert!(!conteudo.contains("data:"), "{conteudo}");
        assert!(conteudo.contains("<p>Antes</p>") && conteudo.contains("<p>Depois</p>"));
        assert_eq!(acervo.store.read(&hash).expect("ler o blob"), bytes);
    }

    /// **O backfill não fabrica revisão ao migrar o capítulo.**
    ///
    /// O gatilho `trg_chapter_revision` dispara em qualquer `UPDATE` de
    /// `content` e guarda a versão anterior. Sem cuidado, o backfill deixaria
    /// uma revisão por capítulo, datada de hoje, com a base64 dentro — e o
    /// histórico do escritor registraria como edição o que foi só troca de
    /// codificação.
    ///
    /// O gate mede o histórico inteiro: quantidade e conteúdo.
    #[test]
    fn o_backfill_nao_fabrica_revisao_ao_migrar_o_capitulo() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, _) = inline_valido("imagem");
        semear_capitulo(&conexao, &format!("<img src=\"{url}\" alt=\"x\">"));

        // Uma edição de verdade do escritor, para haver histórico legítimo.
        conexao
            .execute(
                "UPDATE chapters SET content = ?1 WHERE id = 'cap1'",
                [&format!("<p>revisado</p><img src=\"{url}\" alt=\"x\">")],
            )
            .expect("edição do escritor");
        let legitimas = conta(&conexao, "SELECT COUNT(*) FROM chapter_revisions");
        assert_eq!(legitimas, 1, "o cenário precisa de uma revisão legítima");
        let conteudo_legitimo = ler_um(&conexao, "SELECT content FROM chapter_revisions");

        migrar_documentos(&mut conexao, &acervo.store).expect("migrar");

        assert_eq!(
            conta(&conexao, "SELECT COUNT(*) FROM chapter_revisions"),
            legitimas,
            "o backfill fabricou revisão. Troca de codificação não é edição, e o histórico \
             do escritor não pode registrar como se fosse."
        );
        // E a revisão legítima continua lá — migrada, não apagada.
        let agora = ler_um(&conexao, "SELECT content FROM chapter_revisions");
        assert_ne!(
            agora, conteudo_legitimo,
            "a revisão legítima devia ter migrado"
        );
        assert!(!agora.contains("data:"), "{agora}");
        assert!(
            agora.contains("<img"),
            "e continua sendo o documento: {agora}"
        );
    }

    /// **Revisão que já existia com base64 migra.**
    ///
    /// A tabela é escrita pelo gatilho, e o histórico do escritor pode ter anos
    /// de documentos com imagem embutida. `chapter_revisions.content` é a
    /// superfície 8 do ADR 0010 exatamente por isso.
    #[test]
    fn revisao_pre_existente_com_base64_migra() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("imagem-antiga");
        semear_capitulo(&conexao, "<p>sem imagem hoje</p>");
        conexao
            .execute(
                "INSERT INTO chapter_revisions (id, chapter_id, title, content, word_count)
                 VALUES ('rev1','cap1','Capitulo', ?1, 9)",
                [&format!(
                    "<p>versão antiga</p><img src=\"{url}\" alt=\"y\">"
                )],
            )
            .expect("revisão histórica");

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.migrados, 1);

        let revisao = ler_um(
            &conexao,
            "SELECT content FROM chapter_revisions WHERE id = 'rev1'",
        );
        assert!(revisao.contains(&hash_dos_bytes(&bytes)), "{revisao}");
        assert!(!revisao.contains("data:"));
        assert!(revisao.contains("<p>versão antiga</p>"));
    }

    /// **Depois da migração, o gatilho passa a copiar referência.**
    ///
    /// É a consequência que fecha a superfície 8 sem precisar de código novo:
    /// o gatilho copia o que estiver em `chapters.content`, e a partir daqui
    /// isso é uma referência.
    #[test]
    fn depois_da_migracao_o_gatilho_copia_referencia() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("imagem");
        semear_capitulo(&conexao, &format!("<img src=\"{url}\" alt=\"x\">"));
        migrar_documentos(&mut conexao, &acervo.store).expect("migrar");

        // O escritor edita o texto ao redor, mantendo a imagem.
        let migrado = ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'");
        conexao
            .execute(
                "UPDATE chapters SET content = ?1 WHERE id = 'cap1'",
                [&format!("<p>parágrafo novo</p>{migrado}")],
            )
            .expect("edição do escritor");

        let revisao = ler_um(&conexao, "SELECT content FROM chapter_revisions");
        assert!(
            revisao.contains(&hash_dos_bytes(&bytes)),
            "o gatilho tinha que copiar a referência: {revisao}"
        );
        assert!(
            !revisao.contains("data:") && !revisao.contains("base64"),
            "a revisão nova voltou a carregar bytes: {revisao}"
        );
    }

    /// **Os dois lados de um conflito migram de forma independente, e o
    /// conflito não é resolvido.**
    #[test]
    fn os_dois_lados_do_conflito_migram_sem_resolver_nada() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (minha, bytes_minha) = inline_valido("a-minha-versao");
        let (dele, bytes_dele) = inline_valido("a-versao-dele");
        conexao
            .execute(
                "INSERT INTO sync_conflicts
                    (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                 VALUES ('c1','chapter','cap1','content', ?1, ?2)",
                [
                    &format!("<p>meu</p><img src=\"{minha}\">"),
                    &format!("<p>dele</p><img src=\"{dele}\">"),
                ],
            )
            .expect("conflito aberto");

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.migrados, 2, "os dois lados");

        let (local, remoto, resolvido): (String, String, String) = conexao
            .query_row(
                "SELECT local_value, remote_value, resolved_at FROM sync_conflicts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("ler o conflito");

        assert!(local.contains(&hash_dos_bytes(&bytes_minha)));
        assert!(remoto.contains(&hash_dos_bytes(&bytes_dele)));
        assert!(local.contains("<p>meu</p>") && remoto.contains("<p>dele</p>"));
        assert_eq!(
            resolvido, "",
            "migrar não é decidir: o conflito continua esperando o escritor"
        );
    }

    /// O recorte do conflito é respeitado: só `chapter`/`content`.
    ///
    /// `sync_conflicts` guarda conflito de qualquer campo. Sem recorte, o
    /// título de um capítulo seria tratado como documento — e como título não
    /// é HTML com imagem, cada linha viraria pendência de migração e o
    /// bootstrap ficaria bloqueado por dado que nunca foi mídia.
    #[test]
    fn o_recorte_do_conflito_deixa_os_outros_campos_em_paz() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        conexao
            .execute_batch(
                "INSERT INTO sync_conflicts
                    (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                 VALUES ('c-titulo','chapter','cap1','title','Meu titulo','Titulo dele');
                 INSERT INTO sync_conflicts
                    (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                 VALUES ('c-entidade','entity','e1','description','Minha','Dele');",
            )
            .expect("conflitos fora do recorte");

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");
        assert_eq!(
            resumo,
            Resumo::default(),
            "nada fora do recorte podia ser tocado"
        );
        assert!(pendencias_abertas(&conexao).expect("ler").is_empty());
        assert_eq!(
            ler_um(
                &conexao,
                "SELECT local_value FROM sync_conflicts WHERE id = 'c-titulo'"
            ),
            "Meu titulo"
        );
    }

    /// **Os dois lados da contribuição migram, e ela continua `pending`.**
    #[test]
    fn os_dois_lados_da_contribuicao_migram_sem_aprovar_nada() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (antes, bytes_antes) = inline_valido("como-estava");
        let (proposta, bytes_proposta) = inline_valido("como-o-convidado-quer");
        // Sem `.ok()`: semeadura que falha tem que estourar o teste, senão o
        // gate mede um cenário que não existe.
        conexao
            .execute_batch(
                "INSERT INTO collaboration_sessions
                    (id, title, permission, encryption_key, revoke_token, created_at,
                     expires_at)
                 VALUES ('sess1','Sessao','edit','chave','revogar','2026-01-01',
                         '2026-12-31');",
            )
            .expect("semear a sessão de colaboração");
        conexao
            .execute(
                "INSERT INTO collaboration_contributions
                    (id, session_id, sequence, kind, universe_id, target_type, target_id,
                     target_label, field, original_value, proposed_value, created_at)
                 VALUES ('con1','sess1',1,'edit','u1','chapter','cap1','Capitulo','content',
                         ?1, ?2, '2026-01-01')",
                [
                    &format!("<p>como estava</p><img src=\"{antes}\">"),
                    &format!("<p>proposta</p><img src=\"{proposta}\">"),
                ],
            )
            .expect("contribuição pendente");

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.migrados, 2);

        let (original, proposto, status): (String, String, String) = conexao
            .query_row(
                "SELECT original_value, proposed_value, status FROM collaboration_contributions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("ler a contribuição");
        assert!(original.contains(&hash_dos_bytes(&bytes_antes)));
        assert!(proposto.contains(&hash_dos_bytes(&bytes_proposta)));
        assert_eq!(status, "pending", "migrar não é revisar");
    }

    /// **Falha num lado não autoriza tocar no outro.**
    ///
    /// E a pendência nomeia qual lado ficou de fora, senão o escritor
    /// resolveria um acreditando ter resolvido os dois.
    #[test]
    fn lado_invalido_nao_autoriza_tocar_no_outro() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (boa, bytes) = inline_valido("essa-abre");
        let quebrado = "<p>dele</p><img src=\"data:image/png;base64,!!!\">";
        conexao
            .execute(
                "INSERT INTO sync_conflicts
                    (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                 VALUES ('c1','chapter','cap1','content', ?1, ?2)",
                [&format!("<p>meu</p><img src=\"{boa}\">"), quebrado],
            )
            .expect("um lado bom, um ruim");

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");
        assert_eq!(resumo.migrados, 1, "só o lado bom");
        assert_eq!(resumo.pendencias, 1);

        let (local, remoto): (String, String) = conexao
            .query_row(
                "SELECT local_value, remote_value FROM sync_conflicts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler");
        assert!(local.contains(&hash_dos_bytes(&bytes)));
        assert_eq!(
            remoto, quebrado,
            "o lado que não abriu tinha que ficar intacto"
        );

        let pendencias = pendencias_abertas(&conexao).expect("ler");
        assert_eq!(pendencias.len(), 1);
        assert_eq!(
            pendencias[0].lado, "remote_value",
            "a pendência nomeia o lado"
        );
        assert_eq!(pendencias[0].superficie, 9);
    }

    /// Documento sem imagem não é reescrito no banco.
    ///
    /// E como não há `UPDATE`, o gatilho de revisão também não dispara — o
    /// capítulo do escritor não ganha nem linha nova nem `updated_at` novo por
    /// causa de uma migração que não tinha o que migrar.
    #[test]
    fn documento_sem_imagem_nao_e_reescrito_no_banco() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let texto = "<h2>Capítulo</h2><p>Ela abriu a porta.</p>";
        semear_capitulo(&conexao, texto);

        let resumo = migrar_documentos(&mut conexao, &acervo.store).expect("migrar");

        // `ja_prontos`, e não `default()`. Eu escrevi `default()` aqui e o gate
        // reprovou — com razão: o documento **foi** examinado, e está
        // blob-safe. "Nada a fazer" e "não olhei" são estados diferentes, e o
        // resumo precisa distingui-los para o relatório da migração significar
        // alguma coisa.
        assert_eq!(
            resumo,
            Resumo {
                ja_prontos: 1,
                ..Resumo::default()
            }
        );
        assert_eq!(
            ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'"),
            texto,
            "o documento sem imagem não podia ser reescrito"
        );
        assert_eq!(conta(&conexao, "SELECT COUNT(*) FROM chapter_revisions"), 0);
        assert!(acervo.blobs_no_disco().is_empty());
    }

    /// **A deduplicação atravessa documento e campo direto.**
    ///
    /// A mesma arte como capa de universo e dentro de um capítulo: um arquivo.
    /// É o objetivo da etapa aparecendo entre superfícies de formas
    /// diferentes.
    #[test]
    fn a_deduplicacao_atravessa_documento_e_campo_direto() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, bytes) = inline_valido("a-mesma-arte");
        semear_capitulo(&conexao, &format!("<img src=\"{url}\" alt=\"x\">"));
        conexao
            .execute("UPDATE universes SET cover_image = ?1", [&url])
            .expect("a mesma imagem na capa");

        migrar_documentos(&mut conexao, &acervo.store).expect("documentos");
        migrar_campos_diretos(&conexao, &acervo.store).expect("campos diretos");

        let hash = hash_dos_bytes(&bytes);
        assert_eq!(
            ler_um(&conexao, "SELECT cover_blob_hash FROM universes"),
            hash
        );
        assert!(ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'").contains(&hash));
        assert_eq!(
            acervo.blobs_no_disco().len(),
            1,
            "duas superfícies de formas diferentes, um arquivo"
        );
    }

    /// Rodar de novo não muda nada.
    #[test]
    fn migrar_documentos_duas_vezes_nao_muda_nada() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, _) = inline_valido("imagem");
        semear_capitulo(&conexao, &format!("<p>a</p><img src=\"{url}\">"));

        let primeira = migrar_documentos(&mut conexao, &acervo.store).expect("primeira");
        assert_eq!(primeira.migrados, 1);
        let depois_da_primeira = ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'");
        let arquivos = acervo.blobs_no_disco().len();

        let segunda = migrar_documentos(&mut conexao, &acervo.store).expect("segunda");
        assert_eq!(segunda.migrados, 0);
        assert_eq!(segunda.ja_prontos, 1);
        assert_eq!(
            ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'"),
            depois_da_primeira
        );
        assert_eq!(acervo.blobs_no_disco().len(), arquivos);
        assert_eq!(conta(&conexao, "SELECT COUNT(*) FROM chapter_revisions"), 0);
    }

    /// **Apagar mais de uma revisão é erro, e desfaz a transação.**
    ///
    /// A limpeza da revisão fabricada apaga o que apareceu acima de uma marca
    /// de `rowid`. Se aparecer mais de uma linha, alguma coisa aconteceu que
    /// este código não entende — e apagar por atacado o histórico do escritor
    /// seria exatamente o erro que esta etapa existe para não cometer.
    ///
    /// O cenário instala um segundo gatilho, que é a forma mais direta de a
    /// suposição "uma linha por gravação" deixar de valer.
    #[test]
    fn apagar_mais_de_uma_revisao_e_erro_e_desfaz_tudo() {
        let acervo = Acervo::novo();
        let mut conexao = acervo.banco.connection();
        let (url, _) = inline_valido("imagem");
        let conteudo = format!("<img src=\"{url}\" alt=\"x\">");
        semear_capitulo(&conexao, &conteudo);
        conexao
            .execute_batch(
                "CREATE TRIGGER trg_segunda_copia
                 BEFORE UPDATE OF content ON chapters
                 WHEN OLD.content <> NEW.content
                 BEGIN
                     INSERT INTO chapter_revisions
                        (id, chapter_id, title, content, word_count)
                     VALUES (lower(hex(randomblob(16))), OLD.id, OLD.title, OLD.content, 0);
                 END;",
            )
            .expect("um segundo escritor de revisão");

        let erro = migrar_documentos(&mut conexao, &acervo.store)
            .expect_err("duas revisões por gravação não é o que o código entende");
        assert!(
            erro.message.contains("revisões") || erro.message.contains("revisoes"),
            "recusou pelo motivo errado: {}",
            erro.message
        );

        // A transação foi desfeita: o conteúdo não mudou e nada foi apagado.
        assert_eq!(
            ler_um(&conexao, "SELECT content FROM chapters WHERE id = 'cap1'"),
            conteudo,
            "o capítulo mudou apesar do erro"
        );
        assert_eq!(conta(&conexao, "SELECT COUNT(*) FROM chapter_revisions"), 0);
    }
}
