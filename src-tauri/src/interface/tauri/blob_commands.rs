//! A fronteira do blob store com o frontend.
//!
//! ADR 0010. Dois comandos, e o contrato deles é estreito de propósito:
//!
//! ```text
//! blob_put(base64)  →  hash
//! blob_read(hash)   →  { base64, mime_type }
//! ```
//!
//! **Nenhum caminho de arquivo atravessa esta fronteira.** O frontend nunca
//! recebe onde o blob mora, nem poderia usar isso: o mesmo documento tem que
//! funcionar no Windows e no Android, e caminho é exatamente o que não viaja.
//! O que ele recebe é o conteúdo, e o que ele guarda é o hash.
//!
//! ## O base64 aqui é transporte, e some
//!
//! O IPC do Tauri carrega texto. Bytes precisam atravessar de alguma forma, e
//! base64 é a forma que existe. O que é proibido é **persistir** isso: o
//! `blob_put` devolve um hash, e é o hash que vai ao documento.
//!
//! Um dia isto pode virar `asset://` ou um protocolo customizado, e nada do
//! banco muda — a referência já é o hash. Registrado como dívida, não como
//! bloqueio.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::data_url::{codificar_base64, decodificar_base64, MAIOR_ASSET};
use serde::Serialize;
use tauri::AppHandle;

/// O que o frontend recebe para renderizar.
///
/// Sem caminho, sem URL: só o conteúdo e o tipo. Quem monta a URL é o
/// frontend, em memória, e ela morre com a aba.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobLido {
    pub hash: String,
    pub base64: String,
    pub size_bytes: usize,
}

/// Publica bytes e devolve o endereço deles.
///
/// O tamanho é conferido **aqui**, e não só na tela: validação que existe só
/// no frontend é sugestão, e este comando é chamável por qualquer caminho.
#[tauri::command]
pub fn blob_put(app: AppHandle, base64: String) -> DatabaseCommandResult<String> {
    // O base64 cresce um terço sobre os bytes. Conferir o texto antes de
    // decodificar evita alocar o dobro de um arquivo que vai ser recusado.
    if base64.len() > MAIOR_ASSET / 3 * 4 + 1024 {
        return Err(DatabaseCommandError::validation(format!(
            "A imagem passa do limite de {} MB.",
            MAIOR_ASSET / 1024 / 1024
        )));
    }

    let bytes = decodificar_base64(&base64).map_err(|motivo| {
        DatabaseCommandError::validation(format!("A imagem não pôde ser lida: {motivo}."))
    })?;
    if bytes.is_empty() {
        return Err(DatabaseCommandError::validation(
            "A imagem está vazia.".to_string(),
        ));
    }
    if bytes.len() > MAIOR_ASSET {
        return Err(DatabaseCommandError::validation(format!(
            "A imagem tem {} MB e o limite é {} MB.",
            bytes.len() / 1024 / 1024,
            MAIOR_ASSET / 1024 / 1024
        )));
    }

    super::blob_store(&app)?.put(&bytes)
}

/// Devolve os bytes daquele endereço, conferidos.
///
/// `BlobStore::read` recalcula o hash antes de devolver, então blob corrompido
/// no disco chega como erro e não como imagem errada na tela. Blob ausente
/// chega como `not_found`, que é estado normal no incremental: a referência
/// pode chegar antes do arquivo.
#[tauri::command]
pub fn blob_read(app: AppHandle, hash: String) -> DatabaseCommandResult<BlobLido> {
    let store = super::blob_store(&app)?;
    let bytes = store.read(&hash)?;
    Ok(BlobLido {
        hash,
        base64: codificar_base64(&bytes),
        size_bytes: bytes.len(),
    })
}

/// O blob está aqui?
///
/// Presença, para a tela decidir entre renderizar e mostrar indisponível sem
/// pagar a leitura do arquivo inteiro.
#[tauri::command]
pub fn blob_has(app: AppHandle, hash: String) -> DatabaseCommandResult<bool> {
    super::blob_store(&app)?.has(&hash)
}
