use crate::application::blob_fields;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::universe::{Universe, UniverseStats, UniverseUpdate, UniverseWithStats};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::{universe_repository, SqliteDatabase};

pub fn list_with_stats(
    database: &SqliteDatabase,
    store: &BlobStore,
) -> DatabaseCommandResult<Vec<UniverseWithStats>> {
    let connection = database.read()?;
    let mut universos = universe_repository::list_with_stats(&connection)?;
    // A capa vive no blob store; o que a tela recebe é reconstruído aqui, como
    // transporte. Ver `blob_fields` — e `NH-069`, que é o dia em que o
    // frontend resolver por hash e esta linha sair.
    for universo in universos.iter_mut() {
        universo.universe.cover_image =
            blob_fields::ler_asset_direto(&connection, store, "universes", &universo.universe.id)?;
    }
    Ok(universos)
}

pub fn get(
    database: &SqliteDatabase,
    store: &BlobStore,
    id: &str,
) -> DatabaseCommandResult<Option<Universe>> {
    let connection = database.read()?;
    let Some(mut universo) = universe_repository::get(&connection, id)? else {
        return Ok(None);
    };
    universo.cover_image = blob_fields::ler_asset_direto(&connection, store, "universes", id)?;
    Ok(Some(universo))
}

pub fn stats(database: &SqliteDatabase, universe_id: &str) -> DatabaseCommandResult<UniverseStats> {
    let connection = database.read()?;
    universe_repository::stats(&connection, universe_id)
}

/// Cria o universo numa gravação só.
///
/// O caminho antigo inseria e depois fazia um `UPDATE` separado quando havia
/// capa — duas idas ao banco, e uma janela em que o universo existia sem capa.
pub fn create(
    database: &SqliteDatabase,
    store: &BlobStore,
    name: &str,
    description: &str,
    cover_image: &str,
) -> DatabaseCommandResult<Universe> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O universo precisa de um nome.",
        ));
    }
    let timestamp = now_timestamp();
    let universe = Universe {
        id: new_id(),
        name: name.to_string(),
        description: description.to_string(),
        cover_image: cover_image.to_string(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    let mut connection = database.write()?;
    // Numa transação: o `INSERT` do repositório grava o valor recebido na
    // coluna legada, e a normalização o troca por referência antes do commit.
    // Fora de transação haveria um instante — curto, mas real — em que os
    // bytes estariam gravados.
    let tx = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    universe_repository::insert(&tx, &universe)?;
    blob_fields::gravar_asset_direto(&tx, store, "universes", &universe.id, cover_image)?;
    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    // O que volta para a tela é o transporte, não o que ficou no banco.
    Ok(universe)
}

pub fn update(
    database: &SqliteDatabase,
    store: &BlobStore,
    id: &str,
    patch: UniverseUpdate,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    if patch
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(DatabaseCommandError::validation(
            "O universo precisa de um nome.",
        ));
    }
    let mut connection = database.write()?;
    let tx = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if !universe_repository::update(&tx, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Universo não encontrado."));
    }
    if let Some(capa) = patch.cover_image.as_deref() {
        blob_fields::gravar_asset_direto(&tx, store, "universes", id, capa)?;
    }
    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Exclui o universo e, por tabela em cascata, tudo que pendura nele.
///
/// A cascata só acontece porque esta conexão liga `foreign_keys` — o
/// `tauri-plugin-sql` não liga, então o caminho antigo deixava histórias,
/// entidades e capítulos órfãos no arquivo depois de excluir o universo.
pub fn delete(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !universe_repository::delete(&connection, id)? {
        return Err(DatabaseCommandError::not_found("Universo não encontrado."));
    }
    Ok(())
}
