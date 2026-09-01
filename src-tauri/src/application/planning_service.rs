use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::planning::{
    is_known_field_scope, is_known_status, PlanningCardPlacement, PlanningFieldDefinition,
    PlanningItem, SCOPE_UNIVERSAL,
};
use crate::infrastructure::sqlite::{planning_repository, SqliteDatabase};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

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

pub fn list(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<PlanningItem>> {
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
        return Err(DatabaseCommandError::validation(
            "O card precisa de um título.",
        ));
    }
    let id = new_id();
    let connection = database.write()?;
    planning_repository::insert_card(
        &connection,
        &planning_repository::NewPlanningCard {
            id: &id,
            universe_id,
            title,
            description: description.trim(),
            chapter_id,
            image,
            timestamp: &now_timestamp(),
        },
    )?;
    Ok(id)
}

/// Exclui o card e, junto, as propriedades que só existiam dentro dele.
///
/// Numa transação porque as duas escritas formam uma operação só: um card
/// excluído deixando para trás um campo órfão apareceria no catálogo do
/// universo sem dono e sem ficha onde ser editado.
pub fn delete(database: &SqliteDatabase, id: &str, universe_id: &str) -> DatabaseCommandResult<()> {
    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    planning_repository::delete_field_definitions_owned_by(&transaction, id)?;
    if !planning_repository::delete_card(&transaction, id, universe_id)? {
        // Sai sem commit: o Drop do rusqlite reverte.
        return Err(DatabaseCommandError::not_found(
            "O card não existe mais neste universo.",
        ));
    }
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
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
    card_id: Option<&str>,
) -> DatabaseCommandResult<Vec<PlanningFieldDefinition>> {
    let connection = database.read()?;
    planning_repository::list_field_definitions(&connection, universe_id, card_id)
}

/// Traduz o par (alcance, card) vindo da tela para o que a v15 aceita.
///
/// Um campo restrito precisa de um card que exista neste universo; um campo
/// universal nunca guarda dono, mesmo que a tela mande um por engano.
fn resolve_field_owner<'a>(
    connection: &rusqlite::Connection,
    universe_id: &str,
    scope: &str,
    card_id: Option<&'a str>,
) -> DatabaseCommandResult<Option<&'a str>> {
    if !is_known_field_scope(scope) {
        return Err(DatabaseCommandError::validation(format!(
            "Alcance de campo desconhecido: {scope}."
        )));
    }
    if scope == SCOPE_UNIVERSAL {
        return Ok(None);
    }
    let card_id = card_id.filter(|value| !value.is_empty()).ok_or_else(|| {
        DatabaseCommandError::validation("Um campo restrito precisa de um card de origem.")
    })?;
    if !planning_repository::card_belongs_to_universe(connection, card_id, universe_id)? {
        return Err(DatabaseCommandError::not_found(
            "O card não existe mais neste universo.",
        ));
    }
    Ok(Some(card_id))
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
    scope: &str,
    card_id: Option<&str>,
) -> DatabaseCommandResult<PlanningFieldDefinition> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O campo precisa de um nome.",
        ));
    }
    if !FIELD_TYPES.contains(&field_type) {
        return Err(DatabaseCommandError::validation(format!(
            "Tipo de campo desconhecido: {field_type}."
        )));
    }
    let options_json = serde_json::to_string(options)
        .map_err(|error| DatabaseCommandError::validation(error.to_string()))?;

    let timestamp = now_timestamp();
    let connection = database.write()?;
    let owner_item_id = resolve_field_owner(&connection, universe_id, scope, card_id)?;
    let definition = PlanningFieldDefinition {
        id: new_id(),
        universe_id: universe_id.to_string(),
        name: name.to_string(),
        field_type: field_type.to_string(),
        options_json,
        sort_order: 0,
        scope: scope.to_string(),
        owner_item_id: owner_item_id.map(str::to_string),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };

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
        return Err(DatabaseCommandError::validation(
            "O campo precisa de um nome.",
        ));
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

/// Promove um campo de card para universal, ou o restringe de volta.
///
/// O `card_id` só é lido no alcance `card`; para `universal` ele é ignorado,
/// já que um campo de todos os cards não tem dono.
pub fn set_field_definition_scope(
    database: &SqliteDatabase,
    id: &str,
    universe_id: &str,
    scope: &str,
    card_id: Option<&str>,
) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    let owner_item_id = resolve_field_owner(&connection, universe_id, scope, card_id)?;
    if !planning_repository::set_field_definition_scope(
        &connection,
        id,
        universe_id,
        scope,
        owner_item_id,
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

const VALID_STATUSES: &[&str] = &["IDEIAS", "PLANEJADO", "ESCREVENDO", "REVISAO", "FINALIZADO"];
/// Tipos cujo valor não é escalar: eles viram linhas em `planning_field_links`, e não JSON
/// dentro do card. Guardar um ID de entidade como texto perderia a foreign key.
const RELATION_FIELD_TYPES: &[&str] = &["story", "character", "tags"];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningCardSaveRequest {
    pub id: String,
    pub universe_id: String,
    pub title: String,
    pub description: String,
    pub image: String,
    pub status: String,
    pub chapter_id: Option<String>,
    pub field_values: Value,
}

/// Grava a ficha inteira do card numa transação.
///
/// Estava em `database/planning.rs`, com o `#[tauri::command]`, a validação, a transação e o
/// SQL no mesmo arquivo — um caminho paralelo que contradizia
/// `interface → application → domain → repository`. Ver ADR 0008 e a Fase 3 do roadmap.
pub fn save_card(
    database: &SqliteDatabase,
    request: PlanningCardSaveRequest,
) -> DatabaseCommandResult<()> {
    let mut connection = database.write()?;
    save_card_with(&mut connection, request)
}

/// A gravação em si, sobre uma conexão já obtida.
///
/// Separada de `save_card` para poder ser exercitada contra um banco em memória: os testes
/// desta operação existem desde antes da migração e continuam sendo a rede que a protege.
pub(crate) fn save_card_with(
    connection: &mut Connection,
    request: PlanningCardSaveRequest,
) -> DatabaseCommandResult<()> {
    validate_request(&request)?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if !planning_repository::card_exists_in_universe(
        &transaction,
        &request.id,
        &request.universe_id,
    )? {
        return Err(DatabaseCommandError::not_found(
            "O card não existe mais neste universo.",
        ));
    }
    if let Some(chapter_id) = request.chapter_id.as_deref() {
        if !planning_repository::chapter_belongs_to_universe(
            &transaction,
            chapter_id,
            &request.universe_id,
        )? {
            return Err(DatabaseCommandError::validation(
                "O capítulo relacionado não pertence a este universo.",
            ));
        }
    }

    let definitions = planning_repository::field_definitions_for_card(
        &transaction,
        &request.universe_id,
        &request.id,
    )?;
    let values = request.field_values.as_object().ok_or_else(|| {
        DatabaseCommandError::validation(
            "Os valores personalizados do card devem formar um objeto.",
        )
    })?;

    let mut scalar_values = Map::new();
    let mut links = Vec::new();
    for (field_id, value) in values {
        if value.is_null() {
            continue;
        }
        let definition = definitions.get(field_id).ok_or_else(|| {
            DatabaseCommandError::validation(format!(
                "O campo {field_id} foi removido, é de outro universo ou é exclusivo de outro card."
            ))
        })?;
        if RELATION_FIELD_TYPES.contains(&definition.field_type.as_str()) {
            for target_id in validated_relation_ids(value, &definition.field_type)? {
                links.push((field_id.clone(), definition.field_type.clone(), target_id));
            }
        } else {
            scalar_values.insert(
                field_id.clone(),
                validate_scalar_value(value, definition, field_id)?,
            );
        }
    }

    // As relações são reescritas por inteiro: comparar o que mudou custaria mais que
    // regravar, e deixaria espaço para divergência entre o JSON e as linhas.
    planning_repository::delete_field_links(&transaction, &request.id)?;
    for (field_id, field_type, target_id) in links {
        planning_repository::insert_field_link(
            &transaction,
            &request.id,
            &field_id,
            &field_type,
            &target_id,
        )?;
    }

    let scalar_json = serde_json::to_string(&Value::Object(scalar_values))
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let atualizado = planning_repository::update_card_sheet(
        &transaction,
        &planning_repository::CardSheetUpdate {
            id: &request.id,
            universe_id: &request.universe_id,
            title: request.title.trim(),
            description: request.description.trim(),
            image: &request.image,
            status: &request.status,
            chapter_id: request.chapter_id.as_deref(),
            scalar_values_json: &scalar_json,
        },
    )?;
    if !atualizado {
        // Sai sem commit: o Drop do rusqlite reverte.
        return Err(DatabaseCommandError::not_found(
            "O card não existe mais neste universo.",
        ));
    }
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

fn validate_request(request: &PlanningCardSaveRequest) -> DatabaseCommandResult<()> {
    if request.id.is_empty() || request.universe_id.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O card e o universo são obrigatórios.",
        ));
    }
    if request.title.trim().is_empty() || request.title.chars().count() > 500 {
        return Err(DatabaseCommandError::validation(
            "O título do card deve ter entre 1 e 500 caracteres.",
        ));
    }
    if !VALID_STATUSES.contains(&request.status.as_str()) {
        return Err(DatabaseCommandError::validation(
            "A etapa escolhida para o card é inválida.",
        ));
    }
    if request.image.len() > 6_000_000 {
        return Err(DatabaseCommandError::validation(
            "A imagem do card ultrapassa o limite local permitido.",
        ));
    }
    if !request.image.is_empty() && !request.image.starts_with("data:image/") {
        return Err(DatabaseCommandError::validation(
            "A imagem do card deve ser um arquivo local válido.",
        ));
    }
    if !request.field_values.is_object() {
        return Err(DatabaseCommandError::validation(
            "Os valores personalizados do card são inválidos.",
        ));
    }
    Ok(())
}

fn validate_scalar_value(
    value: &Value,
    definition: &planning_repository::CardFieldDefinition,
    field_id: &str,
) -> DatabaseCommandResult<Value> {
    let invalido = |mensagem: String| DatabaseCommandError::validation(mensagem);
    match definition.field_type.as_str() {
        "text" | "long_text" => value
            .as_str()
            .map(|text| Value::String(text.to_string()))
            .ok_or_else(|| invalido(format!("O campo {field_id} exige texto."))),
        "number" => {
            let text = value
                .as_str()
                .ok_or_else(|| invalido(format!("O campo {field_id} exige um número.")))?;
            if !text.is_empty() && text.parse::<f64>().is_err() {
                return Err(invalido(format!(
                    "O campo {field_id} contém um número inválido."
                )));
            }
            Ok(Value::String(text.to_string()))
        }
        "checkbox" => value
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| invalido(format!("O campo {field_id} exige verdadeiro ou falso."))),
        "yes_no" => {
            let text = value
                .as_str()
                .ok_or_else(|| invalido(format!("O campo {field_id} exige sim ou não.")))?;
            if !["", "yes", "no"].contains(&text) {
                return Err(invalido(format!("O campo {field_id} exige sim ou não.")));
            }
            Ok(Value::String(text.to_string()))
        }
        "select" => {
            let text = value
                .as_str()
                .ok_or_else(|| invalido(format!("O campo {field_id} exige uma opção.")))?;
            if !text.is_empty() && !definition.options.iter().any(|option| option == text) {
                return Err(invalido(format!(
                    "A opção do campo {field_id} não existe mais."
                )));
            }
            Ok(Value::String(text.to_string()))
        }
        "multi_select" => {
            let values = string_array(value, field_id)?;
            if values
                .iter()
                .any(|selected| !definition.options.iter().any(|option| option == selected))
            {
                return Err(invalido(format!(
                    "Uma opção do campo {field_id} não existe mais."
                )));
            }
            Ok(Value::Array(
                values.into_iter().map(Value::String).collect(),
            ))
        }
        _ => Err(invalido(format!(
            "O tipo do campo {field_id} não é suportado."
        ))),
    }
}

/// Ordena e remove repetidos: a mesma entidade escolhida duas vezes vira uma linha só.
fn validated_relation_ids(value: &Value, field_type: &str) -> DatabaseCommandResult<Vec<String>> {
    let values = string_array(value, field_type)?;
    Ok(values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn string_array(value: &Value, field_id: &str) -> DatabaseCommandResult<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| {
            DatabaseCommandError::validation(format!("O campo {field_id} exige uma lista."))
        })?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .filter(|text| !text.is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| {
                    DatabaseCommandError::validation(format!(
                        "O campo {field_id} contém uma referência inválida."
                    ))
                })
        })
        .collect()
}

#[cfg(test)]
mod card_save_tests {
    use super::*;
    use crate::database::migrations::{
        MIGRATION_V1, MIGRATION_V10, MIGRATION_V11, MIGRATION_V12, MIGRATION_V15, MIGRATION_V2,
        MIGRATION_V3, MIGRATION_V6,
    };
    use rusqlite::Connection;

    fn seeded_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        for migration in [
            MIGRATION_V1,
            MIGRATION_V2,
            MIGRATION_V3,
            MIGRATION_V6,
            MIGRATION_V10,
            MIGRATION_V11,
            MIGRATION_V12,
            MIGRATION_V15,
        ] {
            connection
                .execute_batch(migration)
                .expect("apply migration");
        }
        connection.execute_batch(
            r#"
            INSERT INTO universes (id, name) VALUES ('u1', 'Um'), ('u2', 'Dois');
            INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Principal'), ('s2', 'u2', 'Externa');
            INSERT INTO entities (id, universe_id, type, name) VALUES ('e1', 'u1', 'Personagem', 'Lia');
            INSERT INTO content_tags (id, universe_id, name, created_at) VALUES ('t1', 'u1', 'Urgente', datetime('now'));
            INSERT INTO planning_items (id, universe_id, title, created_at, updated_at)
                VALUES ('p1', 'u1', 'Original', datetime('now'), datetime('now'));
            INSERT INTO planning_field_definitions (id, universe_id, name, field_type, options_json, created_at, updated_at) VALUES
                ('note', 'u1', 'Nota', 'text', '[]', datetime('now'), datetime('now')),
                ('priority', 'u1', 'Prioridade', 'select', '["Alta","Baixa"]', datetime('now'), datetime('now')),
                ('stories', 'u1', 'Histórias', 'story', '[]', datetime('now'), datetime('now')),
                ('characters', 'u1', 'Personagens', 'character', '[]', datetime('now'), datetime('now')),
                ('tags', 'u1', 'Tags', 'tags', '[]', datetime('now'), datetime('now'));
            "#,
        ).expect("seed planning data");
        connection
    }

    #[test]
    fn card_save_is_atomic_and_relations_are_normalized() {
        let mut connection = seeded_connection();
        let request = PlanningCardSaveRequest {
            id: "p1".into(),
            universe_id: "u1".into(),
            title: "Atualizado".into(),
            description: "Detalhes".into(),
            image: "data:image/png;base64,card".into(),
            status: "PLANEJADO".into(),
            chapter_id: None,
            field_values: serde_json::json!({
                "note": "Preparar pistas",
                "priority": "Alta",
                "stories": ["s1"],
                "characters": ["e1"],
                "tags": ["t1"]
            }),
        };
        save_card_with(&mut connection, request).expect("save card");

        let (title, values): (String, String) = connection
            .query_row(
                "SELECT title, custom_field_values FROM planning_items WHERE id = 'p1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load card");
        assert_eq!(title, "Atualizado");
        assert_eq!(
            serde_json::from_str::<Value>(&values).unwrap(),
            serde_json::json!({"note":"Preparar pistas","priority":"Alta"})
        );
        let links: i64 = connection
            .query_row("SELECT COUNT(*) FROM planning_field_links", [], |row| {
                row.get(0)
            })
            .expect("count links");
        assert_eq!(links, 3);

        connection
            .execute("DELETE FROM stories WHERE id = 's1'", [])
            .unwrap();
        connection
            .execute("DELETE FROM entities WHERE id = 'e1'", [])
            .unwrap();
        connection
            .execute("DELETE FROM content_tags WHERE id = 't1'", [])
            .unwrap();
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM planning_field_links", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn invalid_cross_universe_relation_rolls_back_the_whole_card() {
        let mut connection = seeded_connection();
        let request = PlanningCardSaveRequest {
            id: "p1".into(),
            universe_id: "u1".into(),
            title: "Não deve salvar".into(),
            description: String::new(),
            image: String::new(),
            status: "PLANEJADO".into(),
            chapter_id: None,
            field_values: serde_json::json!({"stories":["s2"]}),
        };
        assert!(save_card_with(&mut connection, request).is_err());
        let title: String = connection
            .query_row(
                "SELECT title FROM planning_items WHERE id = 'p1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "Original");
        let links: i64 = connection
            .query_row("SELECT COUNT(*) FROM planning_field_links", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(links, 0);
    }
}
