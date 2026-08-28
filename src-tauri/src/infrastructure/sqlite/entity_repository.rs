use crate::database::error::DatabaseCommandResult;
use crate::domain::entity::{
    Entity, EntityAttribute, EntityRelation, EntityUpdate, MentionWithChapter, RelatedEntity,
};
use rusqlite::{Connection, Row};

use super::connection::map_sqlite_error;

const ENTITY_COLUMNS: &str =
    "id, universe_id, type, name, description, summary, image, canon_status, created_at, updated_at";

fn entity_from_row(row: &Row<'_>) -> rusqlite::Result<Entity> {
    Ok(Entity {
        id: row.get("id")?,
        universe_id: row.get("universe_id")?,
        entity_type: row.get("type")?,
        name: row.get("name")?,
        description: row.get("description")?,
        summary: row.get("summary")?,
        image: row.get("image")?,
        canon_status: row.get("canon_status")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn list(connection: &Connection, universe_id: &str) -> DatabaseCommandResult<Vec<Entity>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {ENTITY_COLUMNS} FROM entities WHERE universe_id = ?1 ORDER BY type, name"
        ))
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], entity_from_row)
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn get(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<Entity>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {ENTITY_COLUMNS} FROM entities WHERE id = ?1"
        ))
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query_map([id], entity_from_row)
        .map_err(map_sqlite_error)?;
    match rows.next() {
        Some(row) => Ok(Some(row.map_err(map_sqlite_error)?)),
        None => Ok(None),
    }
}

pub fn insert(connection: &Connection, entity: &Entity) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO entities
               (id, universe_id, type, name, description, summary, image, canon_status,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                entity.id,
                entity.universe_id,
                entity.entity_type,
                entity.name,
                entity.description,
                entity.summary,
                entity.image,
                entity.canon_status,
                entity.created_at,
                entity.updated_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn update(
    connection: &Connection,
    id: &str,
    patch: &EntityUpdate,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut assignments = vec!["updated_at = ?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(updated_at.to_string())];

    for (column, value) in [
        ("name", patch.name.as_ref()),
        ("description", patch.description.as_ref()),
        ("summary", patch.summary.as_ref()),
        ("image", patch.image.as_ref()),
        ("canon_status", patch.canon_status.as_ref()),
    ] {
        if let Some(value) = value {
            values.push(Box::new(value.clone()));
            assignments.push(format!("{column} = ?{}", values.len()));
        }
    }

    values.push(Box::new(id.to_string()));
    let sql = format!(
        "UPDATE entities SET {} WHERE id = ?{}",
        assignments.join(", "),
        values.len()
    );
    let parameters: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    let affected = connection
        .execute(&sql, parameters.as_slice())
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn delete(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM entities WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn touch(connection: &Connection, id: &str, updated_at: &str) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "UPDATE entities SET updated_at = ?1 WHERE id = ?2",
            [updated_at, id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

// ── Atributos ────────────────────────────────────────────────────────────

pub fn list_attributes(
    connection: &Connection,
    entity_id: &str,
) -> DatabaseCommandResult<Vec<EntityAttribute>> {
    let mut statement = connection
        .prepare(
            "SELECT id, entity_id, key, value, sort_order
               FROM entity_attributes WHERE entity_id = ?1 ORDER BY sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([entity_id], |row| {
            Ok(EntityAttribute {
                id: row.get("id")?,
                entity_id: row.get("entity_id")?,
                key: row.get("key")?,
                value: row.get("value")?,
                sort_order: row.get("sort_order")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn insert_attribute(
    connection: &Connection,
    id: &str,
    entity_id: &str,
    key: &str,
    value: &str,
    sort_order: i64,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, entity_id, key, value, sort_order],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

/// Grava um valor por chave, criando o atributo se ele ainda não existir.
///
/// A posição de um atributo novo sai de uma subquery no próprio `INSERT`, e
/// não de um `SELECT MAX` separado: dois atributos criados na mesma ação
/// recebiam a mesma posição no caminho antigo.
pub fn set_attribute_value(
    connection: &Connection,
    new_id: &str,
    entity_id: &str,
    key: &str,
    value: &str,
) -> DatabaseCommandResult<()> {
    let affected = connection
        .execute(
            "UPDATE entity_attributes SET value = ?1 WHERE entity_id = ?2 AND key = ?3",
            [value, entity_id, key],
        )
        .map_err(map_sqlite_error)?;
    if affected > 0 {
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
             VALUES (?1, ?2, ?3, ?4,
                     (SELECT COALESCE(MAX(sort_order), -1) + 1
                        FROM entity_attributes WHERE entity_id = ?2))",
            rusqlite::params![new_id, entity_id, key, value],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn update_attribute(
    connection: &Connection,
    attribute: &EntityAttribute,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "UPDATE entity_attributes SET key = ?1, value = ?2, sort_order = ?3
              WHERE id = ?4 AND entity_id = ?5",
            rusqlite::params![
                attribute.key,
                attribute.value,
                attribute.sort_order,
                attribute.id,
                attribute.entity_id,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn delete_attribute(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM entity_attributes WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

/// Chaves de template do universo para um tipo, já sem as que o padrão cobre.
pub fn list_templates(
    connection: &Connection,
    universe_id: &str,
    entity_type: &str,
) -> DatabaseCommandResult<Vec<(String, String, i64)>> {
    let mut statement = connection
        .prepare(
            "SELECT attribute_key, default_value, sort_order
               FROM entity_templates
              WHERE universe_id = ?1 AND entity_type = ?2
              ORDER BY sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id, entity_type], |row| {
            Ok((
                row.get::<_, String>("attribute_key")?,
                row.get::<_, String>("default_value")?,
                row.get::<_, i64>("sort_order")?,
            ))
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

// ── Ficha completa ───────────────────────────────────────────────────────

/// Relações das duas pontas numa consulta só.
///
/// O caminho antigo fazia dois `SELECT` e juntava em memória. Aqui um `UNION
/// ALL` traz as duas direções já com o nome e a imagem do outro lado, e a
/// coluna `is_source` diz de que lado a entidade pedida está — sem ela, uma
/// relação de uma entidade com ela mesma apareceria invertida.
pub fn list_relations_for_entity(
    connection: &Connection,
    entity: &Entity,
) -> DatabaseCommandResult<Vec<EntityRelation>> {
    let mut statement = connection
        .prepare(
            "SELECT r.id, r.universe_id, r.source_id, r.target_id, r.type, r.label,
                    r.bidirectional, r.importance, r.created_at,
                    1 AS is_source, other.name, other.type AS other_type, other.image
               FROM relations r
               JOIN entities other ON other.id = r.target_id
              WHERE r.source_id = ?1
             UNION ALL
             SELECT r.id, r.universe_id, r.source_id, r.target_id, r.type, r.label,
                    r.bidirectional, r.importance, r.created_at,
                    0 AS is_source, other.name, other.type AS other_type, other.image
               FROM relations r
               JOIN entities other ON other.id = r.source_id
              WHERE r.target_id = ?1",
        )
        .map_err(map_sqlite_error)?;

    let rows = statement
        .query_map([&entity.id], |row| {
            let other = RelatedEntity {
                id: if row.get::<_, i64>("is_source")? == 1 {
                    row.get("target_id")?
                } else {
                    row.get("source_id")?
                },
                name: row.get("name")?,
                entity_type: row.get("other_type")?,
                image: row.get("image")?,
            };
            let self_ref = RelatedEntity {
                id: entity.id.clone(),
                name: entity.name.clone(),
                entity_type: entity.entity_type.clone(),
                image: entity.image.clone(),
            };
            let is_source = row.get::<_, i64>("is_source")? == 1;
            Ok(EntityRelation {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                source_id: row.get("source_id")?,
                target_id: row.get("target_id")?,
                relation_type: row.get("type")?,
                label: row.get("label")?,
                bidirectional: row.get::<_, i64>("bidirectional")? != 0,
                importance: row.get("importance")?,
                created_at: row.get("created_at")?,
                source: if is_source {
                    self_ref.clone()
                } else {
                    other.clone()
                },
                target: if is_source { other } else { self_ref },
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn list_mentions_for_entity(
    connection: &Connection,
    entity_id: &str,
) -> DatabaseCommandResult<Vec<MentionWithChapter>> {
    let mut statement = connection
        .prepare(
            "SELECT m.id, m.chapter_id, m.entity_id, m.created_at,
                    c.title AS chapter_title, b.name AS book_name
               FROM mentions m
               JOIN chapters c ON m.chapter_id = c.id
               JOIN books b ON c.book_id = b.id
              WHERE m.entity_id = ?1
              ORDER BY c.sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([entity_id], |row| {
            Ok(MentionWithChapter {
                id: row.get("id")?,
                chapter_id: row.get("chapter_id")?,
                entity_id: row.get("entity_id")?,
                created_at: row.get("created_at")?,
                chapter_title: row.get("chapter_title")?,
                book_name: row.get("book_name")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{migrated_memory_database, seed_universe};
    use super::*;
    use crate::database::error::DatabaseErrorKind;

    fn entity(id: &str, universe_id: &str, kind: &str, name: &str) -> Entity {
        Entity {
            id: id.into(),
            universe_id: universe_id.into(),
            entity_type: kind.into(),
            name: name.into(),
            description: String::new(),
            summary: String::new(),
            image: String::new(),
            canon_status: "CANON".into(),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        }
    }

    #[test]
    fn atributo_novo_entra_no_fim_sem_repetir_posicao() {
        // O caminho antigo lia MAX(sort_order) numa consulta separada, entao
        // dois atributos criados na mesma acao ficavam com a mesma posicao.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert(&connection, &entity("e1", "u1", "Personagem", "Frodo")).expect("inserir");

        for (id, key) in [("a1", "Idade"), ("a2", "Origem"), ("a3", "Arco")] {
            set_attribute_value(&connection, id, "e1", key, "").expect("gravar atributo");
        }

        let attributes = list_attributes(&connection, "e1").expect("listar");
        let orders: Vec<i64> = attributes.iter().map(|a| a.sort_order).collect();
        assert_eq!(orders, vec![0, 1, 2]);
    }

    #[test]
    fn gravar_a_mesma_chave_duas_vezes_atualiza_em_vez_de_duplicar() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert(&connection, &entity("e1", "u1", "Personagem", "Frodo")).expect("inserir");

        set_attribute_value(&connection, "a1", "e1", "Idade", "50").expect("gravar");
        set_attribute_value(&connection, "a2", "e1", "Idade", "51").expect("regravar");

        let attributes = list_attributes(&connection, "e1").expect("listar");
        assert_eq!(attributes.len(), 1, "nao pode duplicar a chave");
        assert_eq!(attributes[0].value, "51");
    }

    #[test]
    fn excluir_entidade_leva_os_atributos_junto() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert(&connection, &entity("e1", "u1", "Personagem", "Frodo")).expect("inserir");
        set_attribute_value(&connection, "a1", "e1", "Idade", "50").expect("gravar");

        assert!(delete(&connection, "e1").expect("excluir"));
        assert!(list_attributes(&connection, "e1")
            .expect("listar")
            .is_empty());
    }

    #[test]
    fn atributo_de_entidade_inexistente_e_recusado_pela_foreign_key() {
        let connection = migrated_memory_database();
        let error = insert_attribute(&connection, "a1", "fantasma", "Idade", "", 0)
            .expect_err("FK deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);
    }

    #[test]
    fn update_parcial_da_entidade_nao_apaga_o_resto() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        let mut frodo = entity("e1", "u1", "Personagem", "Frodo");
        frodo.summary = "Portador do anel".into();
        frodo.image = "frodo.png".into();
        insert(&connection, &frodo).expect("inserir");

        let patch = EntityUpdate {
            name: Some("Frodo Bolseiro".into()),
            ..Default::default()
        };
        assert!(update(&connection, "e1", &patch, "2026-06-01 00:00:00").expect("atualizar"));

        let saved = get(&connection, "e1").expect("buscar").expect("existe");
        assert_eq!(saved.name, "Frodo Bolseiro");
        assert_eq!(saved.summary, "Portador do anel");
        assert_eq!(saved.image, "frodo.png");
    }

    #[test]
    fn ficha_traz_relacao_das_duas_pontas_com_o_lado_certo() {
        // A entidade pedida tem que aparecer como origem numa relacao em que
        // ela e origem, e como destino na outra. Trocar os lados inverteria o
        // sentido do rotulo na tela ("pai de" viraria "filho de").
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        for (id, name) in [("e1", "Frodo"), ("e2", "Sam"), ("e3", "Bilbo")] {
            insert(&connection, &entity(id, "u1", "Personagem", name)).expect("inserir");
        }
        connection
            .execute_batch(
                "INSERT INTO relations (id, universe_id, source_id, target_id, type, label,
                                        bidirectional, importance, created_at)
                   VALUES ('r1', 'u1', 'e1', 'e2', 'custom', 'amigo de', 0, 'normal', '2026-01-01 00:00:00'),
                          ('r2', 'u1', 'e3', 'e1', 'custom', 'tio de', 0, 'normal', '2026-01-01 00:00:00');",
            )
            .expect("semear relacoes");

        let frodo = get(&connection, "e1").expect("buscar").expect("existe");
        let relations = list_relations_for_entity(&connection, &frodo).expect("listar relacoes");
        assert_eq!(relations.len(), 2);

        let como_origem = relations.iter().find(|r| r.id == "r1").expect("r1");
        assert_eq!(como_origem.source.id, "e1");
        assert_eq!(como_origem.target.name, "Sam");

        let como_destino = relations.iter().find(|r| r.id == "r2").expect("r2");
        assert_eq!(como_destino.source.name, "Bilbo");
        assert_eq!(como_destino.target.id, "e1");
    }

    #[test]
    fn mencao_traz_capitulo_e_livro() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert(&connection, &entity("e1", "u1", "Personagem", "Frodo")).expect("inserir");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name, created_at, updated_at)
                   VALUES ('s1', 'u1', 'Historia', '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO books (id, story_id, name, created_at, updated_at)
                   VALUES ('b1', 's1', 'Livro', '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO chapters (id, book_id, title, content, word_count, sort_order, created_at, updated_at)
                   VALUES ('c1', 'b1', 'Cap 1', '', 0, 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO mentions (id, chapter_id, entity_id, created_at)
                   VALUES ('m1', 'c1', 'e1', '2026-01-01 00:00:00');",
            )
            .expect("semear mencao");

        let mentions = list_mentions_for_entity(&connection, "e1").expect("listar");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].chapter_title, "Cap 1");
        assert_eq!(mentions[0].book_name, "Livro");
    }
}
