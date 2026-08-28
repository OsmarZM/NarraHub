use crate::database::error::DatabaseCommandResult;
use crate::domain::knowledge::{ContentTag, ContentTagAssignment, MentionOccurrence};
use rusqlite::{Connection, Transaction};

use super::connection::map_sqlite_error;

pub fn list_tags(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<ContentTag>> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.universe_id, t.name, t.color, t.created_at,
                    COUNT(a.id) AS assigned
               FROM content_tags t
               LEFT JOIN content_tag_assignments a ON a.tag_id = t.id
              WHERE t.universe_id = ?1
              GROUP BY t.id
              ORDER BY t.name",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(ContentTag {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                name: row.get("name")?,
                color: row.get("color")?,
                created_at: row.get("created_at")?,
                assigned: Some(row.get("assigned")?),
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn list_owner_tags(
    connection: &Connection,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<ContentTag>> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.universe_id, t.name, t.color, t.created_at
               FROM content_tags t
               JOIN content_tag_assignments a ON a.tag_id = t.id
              WHERE a.owner_type = ?1 AND a.owner_id = ?2
              ORDER BY t.name",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([owner_type, owner_id], |row| {
            Ok(ContentTag {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                name: row.get("name")?,
                color: row.get("color")?,
                created_at: row.get("created_at")?,
                assigned: None,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// Atribuições de vários universos, opcionalmente filtradas por tipo de dono.
///
/// O `IN` com número variável de valores é montado aqui, com placeholders
/// numerados — nunca com os valores interpolados no texto do SQL.
pub fn list_assignments(
    connection: &Connection,
    universe_ids: &[String],
    owner_types: &[String],
) -> DatabaseCommandResult<Vec<ContentTagAssignment>> {
    if universe_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let universe_placeholders = universe_ids
        .iter()
        .map(|id| {
            values.push(Box::new(id.clone()));
            format!("?{}", values.len())
        })
        .collect::<Vec<_>>()
        .join(",");

    let owner_filter = if owner_types.is_empty() {
        String::new()
    } else {
        let placeholders = owner_types
            .iter()
            .map(|owner_type| {
                values.push(Box::new(owner_type.clone()));
                format!("?{}", values.len())
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(" AND a.owner_type IN ({placeholders})")
    };

    let sql = format!(
        "SELECT t.id, t.universe_id, t.name, t.color, t.created_at,
                a.owner_type, a.owner_id
           FROM content_tags t
           JOIN content_tag_assignments a ON a.tag_id = t.id
          WHERE t.universe_id IN ({universe_placeholders}){owner_filter}
          ORDER BY t.name"
    );

    let mut statement = connection.prepare(&sql).map_err(map_sqlite_error)?;
    let parameters: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    let rows = statement
        .query_map(parameters.as_slice(), |row| {
            Ok(ContentTagAssignment {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                name: row.get("name")?,
                color: row.get("color")?,
                created_at: row.get("created_at")?,
                owner_type: row.get("owner_type")?,
                owner_id: row.get("owner_id")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn insert_tag(connection: &Connection, tag: &ContentTag) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO content_tags (id, universe_id, name, color, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![tag.id, tag.universe_id, tag.name, tag.color, tag.created_at],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn assign_tag(
    connection: &Connection,
    id: &str,
    tag_id: &str,
    owner_type: &str,
    owner_id: &str,
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    // `OR IGNORE` porque o UNIQUE(tag_id, owner_type, owner_id) já garante a
    // unicidade: marcar duas vezes a mesma tag é operação idempotente, não erro.
    connection
        .execute(
            "INSERT OR IGNORE INTO content_tag_assignments
               (id, tag_id, owner_type, owner_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, tag_id, owner_type, owner_id, timestamp],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn unassign_tag(
    connection: &Connection,
    tag_id: &str,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "DELETE FROM content_tag_assignments
              WHERE tag_id = ?1 AND owner_type = ?2 AND owner_id = ?3",
            [tag_id, owner_type, owner_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn delete_tag(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM content_tags WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

// ── Menções ──────────────────────────────────────────────────────────────

pub fn list_mentions_by_universe(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<MentionOccurrence>> {
    let mut statement = connection
        .prepare(
            "SELECT m.id, m.chapter_id, m.entity_id, m.created_at,
                    c.title AS chapter_title, b.name AS book_name, s.name AS story_name,
                    s.id AS story_id, b.id AS book_id,
                    c.sort_order AS chapter_sort_order,
                    b.sort_order AS book_sort_order,
                    s.sort_order AS story_sort_order
               FROM mentions m
               JOIN chapters c ON c.id = m.chapter_id
               JOIN books b ON b.id = c.book_id
               JOIN stories s ON s.id = b.story_id
              WHERE s.universe_id = ?1
              ORDER BY s.sort_order, b.sort_order, c.sort_order, m.created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(MentionOccurrence {
                id: row.get("id")?,
                chapter_id: row.get("chapter_id")?,
                entity_id: row.get("entity_id")?,
                created_at: row.get("created_at")?,
                chapter_title: row.get("chapter_title")?,
                book_name: row.get("book_name")?,
                story_name: row.get("story_name")?,
                story_id: row.get("story_id")?,
                book_id: row.get("book_id")?,
                chapter_sort_order: row.get("chapter_sort_order")?,
                book_sort_order: row.get("book_sort_order")?,
                story_sort_order: row.get("story_sort_order")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// Deixa as menções do capítulo iguais à lista recebida.
///
/// Apaga o que saiu e insere o que entrou, numa transação. O caminho antigo
/// fazia um `DELETE` e depois um `SELECT` + `INSERT` por entidade, sem
/// transação: uma falha no meio deixava o capítulo com menos menções do que o
/// texto tem, e a ficha da entidade perdia a referência sem ninguém perceber.
///
/// A inserção é condicionada a um `NOT EXISTS`, e **não** a um `OR IGNORE`:
/// `mentions` não tem `UNIQUE(chapter_id, entity_id)` no schema, então
/// `OR IGNORE` não teria nada para ignorar e cada autosave duplicaria a linha.
/// Era o `SELECT` prévio do caminho antigo que segurava isso. Manter a linha
/// existente preserva o `created_at` original, que é o que ordena o "onde
/// apareceu pela primeira vez".
pub fn sync_chapter_mentions(
    transaction: &Transaction<'_>,
    chapter_id: &str,
    entity_ids: &[String],
    new_ids: &[String],
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    if entity_ids.is_empty() {
        transaction
            .execute("DELETE FROM mentions WHERE chapter_id = ?1", [chapter_id])
            .map_err(map_sqlite_error)?;
        return Ok(());
    }

    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(chapter_id.to_string())];
    let placeholders = entity_ids
        .iter()
        .map(|id| {
            values.push(Box::new(id.clone()));
            format!("?{}", values.len())
        })
        .collect::<Vec<_>>()
        .join(",");
    let parameters: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    transaction
        .execute(
            &format!(
                "DELETE FROM mentions WHERE chapter_id = ?1 AND entity_id NOT IN ({placeholders})"
            ),
            parameters.as_slice(),
        )
        .map_err(map_sqlite_error)?;

    let mut insert = transaction
        .prepare(
            "INSERT INTO mentions (id, chapter_id, entity_id, created_at)
             SELECT ?1, ?2, ?3, ?4
              WHERE NOT EXISTS (
                    SELECT 1 FROM mentions WHERE chapter_id = ?2 AND entity_id = ?3)",
        )
        .map_err(map_sqlite_error)?;
    for (entity_id, new_id) in entity_ids.iter().zip(new_ids) {
        insert
            .execute(rusqlite::params![new_id, chapter_id, entity_id, timestamp])
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{migrated_memory_database, seed_universe};
    use super::*;
    use crate::database::error::DatabaseErrorKind;

    fn tag(id: &str, universe_id: &str, name: &str) -> ContentTag {
        ContentTag {
            id: id.into(),
            universe_id: universe_id.into(),
            name: name.into(),
            color: "#7d3650".into(),
            created_at: "2026-01-01 00:00:00".into(),
            assigned: None,
        }
    }

    fn seed_chapter(connection: &Connection) {
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name, sort_order, created_at, updated_at)
                   VALUES ('s1', 'u1', 'Historia', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO books (id, story_id, name, sort_order, created_at, updated_at)
                   VALUES ('b1', 's1', 'Livro', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO chapters (id, book_id, title, content, word_count, sort_order, created_at, updated_at)
                   VALUES ('c1', 'b1', 'Cap 1', '', 0, 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('e2', 'u1', 'Personagem', 'Sam', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear capitulo");
    }

    #[test]
    fn listagem_do_universo_conta_quantos_usam_a_tag() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_tag(&connection, &tag("t1", "u1", "Reescrever")).expect("criar tag");
        insert_tag(&connection, &tag("t2", "u1", "Pronto")).expect("criar tag");
        assign_tag(&connection, "a1", "t1", "chapter", "c1", "2026-01-01 00:00:00").expect("marcar");
        assign_tag(&connection, "a2", "t1", "entity", "e1", "2026-01-01 00:00:00").expect("marcar");

        let tags = list_tags(&connection, "u1").expect("listar");
        let reescrever = tags.iter().find(|t| t.id == "t1").expect("t1");
        let pronto = tags.iter().find(|t| t.id == "t2").expect("t2");
        assert_eq!(reescrever.assigned, Some(2));
        assert_eq!(pronto.assigned, Some(0), "tag sem uso conta zero, nao some");
    }

    #[test]
    fn marcar_a_mesma_tag_duas_vezes_e_idempotente() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_tag(&connection, &tag("t1", "u1", "Reescrever")).expect("criar tag");

        assign_tag(&connection, "a1", "t1", "chapter", "c1", "2026-01-01 00:00:00").expect("marcar");
        assign_tag(&connection, "a2", "t1", "chapter", "c1", "2026-06-01 00:00:00").expect("remarcar");

        let tags = list_owner_tags(&connection, "chapter", "c1").expect("listar");
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn nome_de_tag_repetido_no_universo_e_conflito_mesmo_mudando_a_caixa() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_tag(&connection, &tag("t1", "u1", "Reescrever")).expect("criar tag");

        let error = insert_tag(&connection, &tag("t2", "u1", "reescrever"))
            .expect_err("COLLATE NOCASE deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);
    }

    #[test]
    fn excluir_tag_leva_as_marcacoes_junto() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_tag(&connection, &tag("t1", "u1", "Reescrever")).expect("criar tag");
        assign_tag(&connection, "a1", "t1", "chapter", "c1", "2026-01-01 00:00:00").expect("marcar");

        assert!(delete_tag(&connection, "t1").expect("excluir"));
        assert!(list_owner_tags(&connection, "chapter", "c1").expect("listar").is_empty());
    }

    #[test]
    fn atribuicoes_filtram_por_tipo_de_dono_quando_pedido() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_tag(&connection, &tag("t1", "u1", "Reescrever")).expect("criar tag");
        assign_tag(&connection, "a1", "t1", "chapter", "c1", "2026-01-01 00:00:00").expect("marcar");
        assign_tag(&connection, "a2", "t1", "entity", "e1", "2026-01-01 00:00:00").expect("marcar");

        let todas = list_assignments(&connection, &["u1".into()], &[]).expect("listar");
        assert_eq!(todas.len(), 2);

        let so_entidade = list_assignments(&connection, &["u1".into()], &["entity".into()])
            .expect("listar filtrado");
        assert_eq!(so_entidade.len(), 1);
        assert_eq!(so_entidade[0].owner_id, "e1");
    }

    #[test]
    fn atribuicoes_sem_universo_devolvem_vazio_sem_tocar_no_banco() {
        let connection = migrated_memory_database();
        assert!(list_assignments(&connection, &[], &[]).expect("listar").is_empty());
    }

    #[test]
    fn sincronizar_mencoes_preserva_o_created_at_de_quem_ja_estava() {
        // O created_at da mencao e o que ordena "onde apareceu pela primeira
        // vez". Recriar a linha a cada autosave apagaria essa informacao.
        let mut connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_chapter(&connection);

        {
            let transaction = connection.transaction().expect("transacao");
            sync_chapter_mentions(
                &transaction, "c1", &["e1".into()], &["m1".into()], "2026-01-01 00:00:00",
            )
            .expect("primeira sincronizacao");
            transaction.commit().expect("commit");
        }
        {
            let transaction = connection.transaction().expect("transacao");
            sync_chapter_mentions(
                &transaction,
                "c1",
                &["e1".into(), "e2".into()],
                &["m2".into(), "m3".into()],
                "2026-06-01 00:00:00",
            )
            .expect("segunda sincronizacao");
            transaction.commit().expect("commit");
        }

        let mentions = list_mentions_by_universe(&connection, "u1").expect("listar");
        assert_eq!(mentions.len(), 2);
        let frodo = mentions.iter().find(|m| m.entity_id == "e1").expect("e1");
        assert_eq!(frodo.created_at, "2026-01-01 00:00:00", "a mencao antiga nao pode ser recriada");
        assert_eq!(frodo.id, "m1");
    }

    #[test]
    fn sincronizar_com_lista_vazia_limpa_o_capitulo() {
        let mut connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_chapter(&connection);

        {
            let transaction = connection.transaction().expect("transacao");
            sync_chapter_mentions(
                &transaction, "c1", &["e1".into()], &["m1".into()], "2026-01-01 00:00:00",
            )
            .expect("sincronizar");
            transaction.commit().expect("commit");
        }
        {
            let transaction = connection.transaction().expect("transacao");
            sync_chapter_mentions(&transaction, "c1", &[], &[], "2026-06-01 00:00:00")
                .expect("limpar");
            transaction.commit().expect("commit");
        }

        assert!(list_mentions_by_universe(&connection, "u1").expect("listar").is_empty());
    }

    #[test]
    fn mencoes_saem_na_ordem_de_leitura_da_obra() {
        let mut connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_chapter(&connection);
        connection
            .execute_batch(
                "INSERT INTO chapters (id, book_id, title, content, word_count, sort_order, created_at, updated_at)
                   VALUES ('c0', 'b1', 'Cap 0', '', 0, -1, '2026-06-01 00:00:00', '2026-06-01 00:00:00');",
            )
            .expect("semear capitulo anterior");

        for (chapter, mention) in [("c1", "m1"), ("c0", "m2")] {
            let transaction = connection.transaction().expect("transacao");
            sync_chapter_mentions(
                &transaction, chapter, &["e1".into()], &[mention.into()], "2026-01-01 00:00:00",
            )
            .expect("sincronizar");
            transaction.commit().expect("commit");
        }

        let mentions = list_mentions_by_universe(&connection, "u1").expect("listar");
        assert_eq!(
            mentions.iter().map(|m| m.chapter_id.as_str()).collect::<Vec<_>>(),
            vec!["c0", "c1"],
            "a ordem e a de leitura, nao a de criacao da mencao"
        );
    }
}
