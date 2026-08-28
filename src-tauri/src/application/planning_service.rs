use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::planning::{
    is_known_status, PlanningCardPlacement, PlanningFieldDefinition, PlanningItem,
};
use crate::infrastructure::sqlite::{planning_repository, SqliteDatabase};
use std::collections::BTreeMap;

/// Tipos de campo que a migration 11 aceita. Validar aqui devolve
/// `validation` com uma frase legível; deixar passar devolveria a mensagem
/// crua do `CHECK` do SQLite para a tela.
const FIELD_TYPES: &[&str] = &[
    "text",
    "long_text",
    "number",
    "checkbox",
    "yes_no",
    "select",
    "multi_select",
    "tags",
    "story",
    "character",
];

pub fn list(database: &SqliteDatabase, universe_id: &str) -> DatabaseCommandResult<Vec<PlanningItem>> {
    let connection = database.read()?;
    planning_repository::list(&connection, universe_id)
}

pub fn create(
    database: &SqliteDatabase,
    universe_id: &str,
    title: &str,
    description: &str,
    chapter_id: Option<&str>,
    image: &str,
) -> DatabaseCommandResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(DatabaseCommandError::validation("O card precisa de um título."));
    }
    let id = new_id();
    let connection = database.write()?;
    planning_repository::insert_card(
        &connection,
        &id,
        universe_id,
        title,
        description.trim(),
        chapter_id,
        image,
        &now_timestamp(),
    )?;
    Ok(id)
}

pub fn delete(database: &SqliteDatabase, id: &str, universe_id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !planning_repository::delete_card(&connection, id, universe_id)? {
        return Err(DatabaseCommandError::not_found(
            "O card não existe mais neste universo.",
        ));
    }
    Ok(())
}

/// Reposiciona os cards em bloco.
///
/// Se o número de linhas atingidas não bater com o número de cards enviados, o
/// quadro mudou entre o arrasto e a gravação — a transação é revertida e o
/// erro pede recarregar, em vez de gravar meia reordenação.
pub fn save_order(
    database: &SqliteDatabase,
    universe_id: &str,
    placements: &[PlanningCardPlacement],
) -> DatabaseCommandResult<()> {
    if placements.is_empty() {
        return Ok(());
    }
    if let Some(unknown) = placements
        .iter()
        .find(|placement| !is_known_status(&placement.status))
    {
        return Err(DatabaseCommandError::validation(format!(
            "Coluna desconhecida no quadro: {}.",
            unknown.status
        )));
    }

    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let affected =
        planning_repository::save_order(&transaction, universe_id, placements, &now_timestamp())?;
    if affected != placements.len() {
        // Sai sem commit: o Drop do rusqlite reverte.
        return Err(DatabaseCommandError::conflict(
            "O quadro mudou enquanto o card era movido. Atualize e tente novamente.",
        ));
    }
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

pub fn list_field_links(
    database: &SqliteDatabase,
    card_id: &str,
) -> DatabaseCommandResult<BTreeMap<String, Vec<String>>> {
    let connection = database.read()?;
    planning_repository::list_field_links(&connection, card_id)
}

pub fn list_field_definitions(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<PlanningFieldDefinition>> {
    let connection = database.read()?;
    planning_repository::list_field_definitions(&connection, universe_id)
}

/// Cria a definição e devolve a linha já gravada.
///
/// Devolver o que foi lido de volta, e não o que montamos na memória, é o que
/// garante que o `sort_order` calculado pelo banco chegue na tela — ele sai de
/// uma subquery no `INSERT`, então aqui nós não o conhecemos.
pub fn create_field_definition(
    database: &SqliteDatabase,
    universe_id: &str,
    name: &str,
    field_type: &str,
    options: &[String],
) -> DatabaseCommandResult<PlanningFieldDefinition> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation("O campo precisa de um nome."));
    }
    if !FIELD_TYPES.contains(&field_type) {
        return Err(DatabaseCommandError::validation(format!(
            "Tipo de campo desconhecido: {field_type}."
        )));
    }
    let options_json = serde_json::to_string(options)
        .map_err(|error| DatabaseCommandError::validation(error.to_string()))?;

    let timestamp = now_timestamp();
    let definition = PlanningFieldDefinition {
        id: new_id(),
        universe_id: universe_id.to_string(),
        name: name.to_string(),
        field_type: field_type.to_string(),
        options_json,
        sort_order: 0,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };

    let connection = database.write()?;
    planning_repository::insert_field_definition(&connection, &definition)?;
    planning_repository::get_field_definition(&connection, &definition.id, universe_id)?
        .ok_or_else(|| DatabaseCommandError::storage("O campo criado não pôde ser lido de volta."))
}

pub fn rename_field_definition(
    database: &SqliteDatabase,
    id: &str,
    universe_id: &str,
    name: &str,
) -> DatabaseCommandResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation("O campo precisa de um nome."));
    }
    let connection = database.write()?;
    if !planning_repository::rename_field_definition(
        &connection,
        id,
        universe_id,
        name,
        &now_timestamp(),
    )? {
        return Err(DatabaseCommandError::not_found(
            "O campo não existe mais neste universo.",
        ));
    }
    Ok(())
}

pub fn delete_field_definition(
    database: &SqliteDatabase,
    id: &str,
    universe_id: &str,
) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !planning_repository::delete_field_definition(&connection, id, universe_id)? {
        return Err(DatabaseCommandError::not_found(
            "O campo não existe mais neste universo.",
        ));
    }
    Ok(())
}
