use crate::application::blob_fields;
use crate::application::mutacao::Mutacao;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
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

/// Cria o universo numa gravação só, pela `Mutacao`: linha, capa no blob store e evento.
pub fn create(
    database: &SqliteDatabase,
    store: &BlobStore,
    identidade: &DeviceIdentity,
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
    // O `INSERT` do repositório grava o valor recebido na coluna legada, e a normalização o troca
    // por referência antes do commit — e antes de o estado canônico ser lido para o evento.
    Mutacao::executar(database, identidade, |m| {
        universe_repository::insert(m.tx(), &universe)?;
        blob_fields::gravar_asset_direto(m.tx(), store, "universes", &universe.id, cover_image)?;
        m.gravou("universe", &universe.id)?;
        m.gravou("story_order", &universe.id)
    })?;
    // O que volta para a tela é o transporte, não o que ficou no banco.
    Ok(universe)
}

pub fn update(
    database: &SqliteDatabase,
    store: &BlobStore,
    identidade: &DeviceIdentity,
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
    Mutacao::executar(database, identidade, |m| {
        if !universe_repository::update(m.tx(), id, &patch, &now_timestamp())? {
            return Err(DatabaseCommandError::not_found("Universo não encontrado."));
        }
        if let Some(capa) = patch.cover_image.as_deref() {
            blob_fields::gravar_asset_direto(m.tx(), store, "universes", id, capa)?;
        }
        m.gravou("universe", id)
    })
}

/// Mensagem da recusa temporária de `delete`.
pub const EXCLUSAO_DE_UNIVERSO_INDISPONIVEL: &str =
    "Esta operação ainda depende de tipos que estão sendo migrados para o Sync V2.";

/// **Recusada até a árvore inteira do universo ser coberta** (NH-079).
///
/// Excluir um universo apaga, por cascata, entidades, relações, linha do tempo, planejamento, tags
/// e canvas — agregados que ainda não têm codec. Apagar sem evento seria perda silenciosa de
/// conteúdo causal nos outros aparelhos; recusar é o fail closed. O universo precisa existir para a
/// recusa ser a certa: um id que não existe continua sendo "não encontrado".
pub fn delete(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.read()?;
    if universe_repository::get(&connection, id)?.is_none() {
        return Err(DatabaseCommandError::not_found("Universo não encontrado."));
    }
    Err(DatabaseCommandError::conflict(
        EXCLUSAO_DE_UNIVERSO_INDISPONIVEL,
    ))
}
