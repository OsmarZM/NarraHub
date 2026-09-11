//! As seis superfícies de campo direto, com **uma** implementação.
//!
//! ADR 0010. `chapters.content` é documento e tem o seu transformador; aqui
//! estão os campos em que a coluna **é** o asset: capa de universo, capa de
//! livro, imagem de entidade, node do canvas, imagem de cena e anexo.
//!
//! ## O que muda, e o que não muda
//!
//! ```text
//! antes   serviço recebe data URL  →  coluna legada guarda os bytes
//! agora   serviço recebe data URL  →  BlobStore  →  coluna guarda o hash
//!                                                    legado fica vazio
//! ```
//!
//! A **entrada** do aplicativo continua aceitando `data:` URL, e isso é
//! deliberado: é o que a tela já manda, e mudar seis telas de uma vez seria
//! trocar o risco de lugar no fim de uma etapa. O que mudou é que aquilo
//! agora é **transporte** — atravessa o IPC e morre na fronteira.
//!
//! A **leitura** reconstrói a `data:` URL a partir do blob, pelo mesmo
//! motivo. Nenhuma tela precisa saber o que aconteceu, e o frontend migra
//! para o hash quando houver tempo (registrado como dívida `NH-069`).
//!
//! O que a invariante exige está cumprido: **os bytes não estão no SQLite.**
//!
//! ## Por que catálogo em vez de seis funções
//!
//! As seis diferem só nos nomes das colunas, e o catálogo já os tem. Seis
//! cópias divergiriam — e divergir aqui significa uma superfície voltando a
//! guardar bytes sem ninguém perceber, que é exatamente o que o gate de
//! catálogo da fatia 2 existe para impedir.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::data_url::{classificar, codificar_base64, Legado, MAIOR_ASSET};
use crate::infrastructure::blob_store::{e_hash_canonico, BlobStore};
use crate::infrastructure::sqlite::blob_surfaces::{superficie_de, Forma, Referencia};
use rusqlite::Connection;

/// O par de colunas daquela superfície, ou erro se ela não é campo direto.
fn referencia_de(tabela: &str) -> DatabaseCommandResult<&'static Referencia> {
    let Some(superficie) = superficie_de(tabela) else {
        return Err(DatabaseCommandError::storage(format!(
            "{tabela} não é superfície binária do ADR 0010"
        )));
    };
    if superficie.forma != Forma::CampoDireto {
        return Err(DatabaseCommandError::storage(format!(
            "{tabela} é documento, não campo direto"
        )));
    }
    superficie
        .referencias
        .first()
        .ok_or_else(|| DatabaseCommandError::storage(format!("{tabela} não declara par hash/MIME")))
}

/// **Grava o asset daquela linha como referência.**
///
/// Chamado **depois** de a linha existir, porque a referência é um `UPDATE`
/// por `id`.
///
/// ```text
/// vazio             →  limpa tudo: o escritor removeu a imagem
/// data: URL válida  →  publica, verifica, grava hash e MIME, legado vazio
/// hash canônico     →  aceita direto: já é o contrato
/// qualquer outra    →  RECUSA
/// ```
///
/// A recusa é a diferença entre entrada e backfill. O backfill preserva o que
/// não entende, porque aquilo já estava no acervo e destruir seria pior.
/// Aqui o valor está **chegando agora**: aceitar seria criar legado novo, e
/// pendência de migração é para o passado.
pub fn gravar_asset_direto(
    connection: &Connection,
    store: &BlobStore,
    tabela: &str,
    row_id: &str,
    valor: &str,
) -> DatabaseCommandResult<()> {
    let referencia = referencia_de(tabela)?;
    let (hash, mime) = normalizar(store, valor)?;

    // Hash primeiro, legado depois — a mesma ordem do backfill. Aqui as duas
    // colunas vão na mesma instrução, então a ordem é do SQLite; o que
    // importa é não haver estado em que o legado saiu e o hash não entrou.
    connection
        .execute(
            &format!(
                "UPDATE {tabela} SET {} = ?1, {} = ?2, {} = '' WHERE id = ?3",
                referencia.hash, referencia.mime, referencia.legada
            ),
            rusqlite::params![hash, mime, row_id],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// O valor recebido, como referência.
fn normalizar(store: &BlobStore, valor: &str) -> DatabaseCommandResult<(String, String)> {
    let aparado = valor.trim();
    if aparado.is_empty() {
        return Ok((String::new(), String::new()));
    }
    if e_hash_canonico(aparado) {
        // Já é o contrato. Não confere presença no disco: no incremental a
        // referência pode chegar antes do arquivo, e recusar aqui impediria
        // gravar um item cujo blob ainda está a caminho.
        return Ok((aparado.to_string(), String::new()));
    }
    match classificar(aparado) {
        Legado::Decodificada { mime, bytes } => {
            if bytes.len() > MAIOR_ASSET {
                return Err(DatabaseCommandError::validation(format!(
                    "A imagem tem {} MB e o limite é {} MB.",
                    bytes.len() / 1024 / 1024,
                    MAIOR_ASSET / 1024 / 1024
                )));
            }
            let hash = store.put(&bytes)?;
            if !store.verify(&hash)? {
                return Err(DatabaseCommandError::storage(
                    "A imagem não conferiu depois de ser gravada. Nada foi salvo.".to_string(),
                ));
            }
            Ok((hash, mime))
        }
        Legado::NaoReconhecido { motivo } => Err(DatabaseCommandError::validation(format!(
            "Esta imagem não pôde ser guardada: {motivo}."
        ))),
        Legado::Vazio => Ok((String::new(), String::new())),
    }
}

/// **A `data:` URL de transporte daquela linha.**
///
/// Reconstruída do blob a cada leitura. É o que mantém as telas funcionando
/// sem que os bytes voltem ao banco — e o dia em que o frontend resolver por
/// hash, esta função sai sem nada mudar na persistência.
///
/// Devolve o valor legado quando o hash está vazio: é o acervo que ainda não
/// passou pelo backfill, ou o legado que não pôde ser convertido. Preservado,
/// como manda a política.
pub fn ler_asset_direto(
    connection: &Connection,
    store: &BlobStore,
    tabela: &str,
    row_id: &str,
) -> DatabaseCommandResult<String> {
    let referencia = referencia_de(tabela)?;
    let (hash, mime, legado): (String, String, String) = connection
        .query_row(
            &format!(
                "SELECT {}, {}, {} FROM {tabela} WHERE id = ?1",
                referencia.hash, referencia.mime, referencia.legada
            ),
            [row_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if hash.is_empty() {
        return Ok(legado);
    }
    let Ok(bytes) = store.read(&hash) else {
        // Blob ausente ou corrompido. A tela mostra o asset como
        // indisponível, e a referência continua no banco esperando o arquivo.
        return Ok(String::new());
    };
    let tipo = if mime.is_empty() {
        "application/octet-stream"
    } else {
        &mime
    };
    Ok(format!("data:{tipo};base64,{}", codificar_base64(&bytes)))
}
