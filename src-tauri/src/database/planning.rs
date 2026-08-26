use rusqlite::{params, Connection, OpenFlags, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;
use uuid::Uuid;

const VALID_STATUSES: &[&str] = &["IDEIAS", "PLANEJADO", "ESCREVENDO", "REVISAO", "FINALIZADO"];
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

#[derive(Debug)]
struct FieldDefinition {
    field_type: String,
    options: Vec<String>,
}

#[tauri::command]
pub fn planning_save_card(app: AppHandle, request: PlanningCardSaveRequest) -> Result<(), String> {
    let path = super::app_database_path(&app)?;
    let mut connection = open_write_connection(&path)?;
    save_card(&mut connection, request)
}

fn open_write_connection(path: &Path) -> Result<Connection, String> {
    if !path.is_file() {
        return Err("O banco local do NarraHub não foi encontrado.".into());
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Não foi possível abrir o banco do planejamento: {error}"))?;
    connection
        .busy_timeout(Duration::from_secs(8))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

fn save_card(connection: &mut Connection, request: PlanningCardSaveRequest) -> Result<(), String> {
    validate_request(&request)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    ensure_card_and_chapter_scope(&transaction, &request)?;
    let definitions = load_definitions(&transaction, &request.universe_id)?;
    let values = request
        .field_values
        .as_object()
        .ok_or_else(|| "Os valores personalizados do card devem formar um objeto.".to_string())?;
    let mut scalar_values = Map::new();
    let mut links = Vec::new();

    for (field_id, value) in values {
        if value.is_null() {
            continue;
        }
        let definition = definitions.get(field_id).ok_or_else(|| {
            format!("O campo {field_id} foi removido ou não pertence a este universo.")
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

    transaction
        .execute(
            "DELETE FROM planning_field_links WHERE planning_item_id = ?1",
            [&request.id],
        )
        .map_err(|error| error.to_string())?;
    for (field_id, field_type, target_id) in links {
        insert_link(
            &transaction,
            &request.id,
            &field_id,
            &field_type,
            &target_id,
        )?;
    }

    let scalar_json =
        serde_json::to_string(&Value::Object(scalar_values)).map_err(|error| error.to_string())?;
    let updated = transaction
        .execute(
            r#"UPDATE planning_items
               SET title = ?1,
                   description = ?2,
                   image = ?3,
                   sort_order = CASE WHEN status <> ?4 THEN (
                       SELECT COALESCE(MAX(other.sort_order), -1) + 1
                       FROM planning_items other
                       WHERE other.universe_id = ?9 AND other.status = ?4 AND other.id <> ?8
                   ) ELSE sort_order END,
                   status = ?4,
                   chapter_id = ?5,
                   custom_field_values = ?6,
                   updated_at = ?7
               WHERE id = ?8 AND universe_id = ?9"#,
            params![
                request.title.trim(),
                request.description.trim(),
                request.image,
                request.status,
                request.chapter_id,
                scalar_json,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                request.id,
                request.universe_id,
            ],
        )
        .map_err(|error| format!("Não foi possível salvar a ficha do card: {error}"))?;
    if updated != 1 {
        return Err("O card não existe mais neste universo.".into());
    }
    transaction.commit().map_err(|error| error.to_string())
}

fn validate_request(request: &PlanningCardSaveRequest) -> Result<(), String> {
    if request.id.is_empty() || request.universe_id.is_empty() {
        return Err("O card e o universo são obrigatórios.".into());
    }
    if request.title.trim().is_empty() || request.title.chars().count() > 500 {
        return Err("O título do card deve ter entre 1 e 500 caracteres.".into());
    }
    if !VALID_STATUSES.contains(&request.status.as_str()) {
        return Err("A etapa escolhida para o card é inválida.".into());
    }
    if request.image.len() > 6_000_000 {
        return Err("A imagem do card ultrapassa o limite local permitido.".into());
    }
    if !request.image.is_empty() && !request.image.starts_with("data:image/") {
        return Err("A imagem do card deve ser um arquivo local válido.".into());
    }
    if !request.field_values.is_object() {
        return Err("Os valores personalizados do card são inválidos.".into());
    }
    Ok(())
}

fn ensure_card_and_chapter_scope(
    transaction: &Transaction<'_>,
    request: &PlanningCardSaveRequest,
) -> Result<(), String> {
    let card_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM planning_items WHERE id = ?1 AND universe_id = ?2)",
            params![request.id, request.universe_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !card_exists {
        return Err("O card não existe mais neste universo.".into());
    }
    if let Some(chapter_id) = request.chapter_id.as_deref() {
        let chapter_exists: bool = transaction
            .query_row(
                r#"SELECT EXISTS(
                       SELECT 1 FROM chapters c
                       JOIN books b ON b.id = c.book_id
                       JOIN stories s ON s.id = b.story_id
                       WHERE c.id = ?1 AND s.universe_id = ?2
                   )"#,
                params![chapter_id, request.universe_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !chapter_exists {
            return Err("O capítulo relacionado não pertence a este universo.".into());
        }
    }
    Ok(())
}

fn load_definitions(
    transaction: &Transaction<'_>,
    universe_id: &str,
) -> Result<HashMap<String, FieldDefinition>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT id, field_type, options_json FROM planning_field_definitions WHERE universe_id = ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut definitions = HashMap::new();
    for row in rows {
        let (id, field_type, options_json) = row.map_err(|error| error.to_string())?;
        let options = serde_json::from_str::<Vec<String>>(&options_json)
            .map_err(|_| format!("As opções do campo {id} estão inválidas."))?;
        definitions.insert(
            id,
            FieldDefinition {
                field_type,
                options,
            },
        );
    }
    Ok(definitions)
}

fn validate_scalar_value(
    value: &Value,
    definition: &FieldDefinition,
    field_id: &str,
) -> Result<Value, String> {
    match definition.field_type.as_str() {
        "text" | "long_text" => value
            .as_str()
            .map(|text| Value::String(text.to_string()))
            .ok_or_else(|| format!("O campo {field_id} exige texto.")),
        "number" => {
            let text = value
                .as_str()
                .ok_or_else(|| format!("O campo {field_id} exige um número."))?;
            if !text.is_empty() && text.parse::<f64>().is_err() {
                return Err(format!("O campo {field_id} contém um número inválido."));
            }
            Ok(Value::String(text.to_string()))
        }
        "checkbox" => value
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| format!("O campo {field_id} exige verdadeiro ou falso.")),
        "yes_no" => {
            let text = value
                .as_str()
                .ok_or_else(|| format!("O campo {field_id} exige sim ou não."))?;
            if !["", "yes", "no"].contains(&text) {
                return Err(format!("O campo {field_id} exige sim ou não."));
            }
            Ok(Value::String(text.to_string()))
        }
        "select" => {
            let text = value
                .as_str()
                .ok_or_else(|| format!("O campo {field_id} exige uma opção."))?;
            if !text.is_empty() && !definition.options.iter().any(|option| option == text) {
                return Err(format!("A opção do campo {field_id} não existe mais."));
            }
            Ok(Value::String(text.to_string()))
        }
        "multi_select" => {
            let values = string_array(value, field_id)?;
            if values
                .iter()
                .any(|selected| !definition.options.iter().any(|option| option == selected))
            {
                return Err(format!("Uma opção do campo {field_id} não existe mais."));
            }
            Ok(Value::Array(
                values.into_iter().map(Value::String).collect(),
            ))
        }
        _ => Err(format!("O tipo do campo {field_id} não é suportado.")),
    }
}

fn validated_relation_ids(value: &Value, field_type: &str) -> Result<Vec<String>, String> {
    let values = string_array(value, field_type)?;
    Ok(values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn string_array(value: &Value, field_id: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("O campo {field_id} exige uma lista."))?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .filter(|text| !text.is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| format!("O campo {field_id} contém uma referência inválida."))
        })
        .collect()
}

fn insert_link(
    transaction: &Transaction<'_>,
    card_id: &str,
    field_id: &str,
    field_type: &str,
    target_id: &str,
) -> Result<(), String> {
    let (story_id, entity_id, tag_id) = match field_type {
        "story" => (Some(target_id), None, None),
        "character" => (None, Some(target_id), None),
        "tags" => (None, None, Some(target_id)),
        _ => return Err("Tipo de relação personalizada inválido.".into()),
    };
    transaction
        .execute(
            r#"INSERT INTO planning_field_links
               (id, planning_item_id, field_definition_id, story_id, entity_id, tag_id, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            params![
                Uuid::new_v4().to_string(),
                card_id,
                field_id,
                story_id,
                entity_id,
                tag_id,
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ],
        )
        .map_err(|error| format!("Uma relação do card é inválida: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::{
        MIGRATION_V1, MIGRATION_V10, MIGRATION_V11, MIGRATION_V12, MIGRATION_V2, MIGRATION_V3,
        MIGRATION_V6,
    };

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
        save_card(&mut connection, request).expect("save card");

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
        assert!(save_card(&mut connection, request).is_err());
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
