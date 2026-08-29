use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::knowledge::{
    is_known_owner_type, ContentTag, ContentTagAssignment, MentionOccurrence,
};
use crate::infrastructure::sqlite::{knowledge_repository, SqliteDatabase};

pub fn list_tags(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<ContentTag>> {
    let connection = database.read()?;
    knowledge_repository::list_tags(&connection, universe_id)
}

pub fn list_owner_tags(
    database: &SqliteDatabase,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<ContentTag>> {
    ensure_known_owner(owner_type)?;
    let connection = database.read()?;
    knowledge_repository::list_owner_tags(&connection, owner_type, owner_id)
}

pub fn list_assignments(
    database: &SqliteDatabase,
    universe_ids: &[String],
    owner_types: &[String],
) -> DatabaseCommandResult<Vec<ContentTagAssignment>> {
    if universe_ids.is_empty() {
        return Ok(Vec::new());
    }
    for owner_type in owner_types {
        ensure_known_owner(owner_type)?;
    }
    let connection = database.read()?;
    knowledge_repository::list_assignments(&connection, universe_ids, owner_types)
}

pub fn create_tag(
    database: &SqliteDatabase,
    universe_id: &str,
    name: &str,
    color: &str,
) -> DatabaseCommandResult<ContentTag> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation(
            "A tag precisa de um nome.",
        ));
    }
    let tag = ContentTag {
        id: new_id(),
        universe_id: universe_id.to_string(),
        name: name.to_string(),
        color: color.to_string(),
        created_at: now_timestamp(),
        assigned: None,
    };
    let connection = database.write()?;
    knowledge_repository::insert_tag(&connection, &tag)?;
    Ok(tag)
}

pub fn set_tag(
    database: &SqliteDatabase,
    owner_type: &str,
    owner_id: &str,
    tag_id: &str,
    assigned: bool,
) -> DatabaseCommandResult<()> {
    ensure_known_owner(owner_type)?;
    let connection = database.write()?;
    if assigned {
        knowledge_repository::assign_tag(
            &connection,
            &new_id(),
            tag_id,
            owner_type,
            owner_id,
            &now_timestamp(),
        )
    } else {
        knowledge_repository::unassign_tag(&connection, tag_id, owner_type, owner_id)
    }
}

pub fn delete_tag(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !knowledge_repository::delete_tag(&connection, id)? {
        return Err(DatabaseCommandError::not_found("A tag não existe mais."));
    }
    Ok(())
}

pub fn list_mentions_by_universe(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<MentionOccurrence>> {
    let connection = database.read()?;
    knowledge_repository::list_mentions_by_universe(&connection, universe_id)
}

/// Deixa as menções do capítulo iguais à lista recebida, numa transação.
///
/// A lista vem do texto salvo, então repetição é esperada e não é erro — a
/// deduplicação acontece aqui, antes do banco, para não gastar um `INSERT` por
/// ocorrência da mesma entidade no capítulo.
pub fn sync_chapter_mentions(
    database: &SqliteDatabase,
    chapter_id: &str,
    entity_ids: &[String],
) -> DatabaseCommandResult<()> {
    let mut unique: Vec<String> = Vec::with_capacity(entity_ids.len());
    for entity_id in entity_ids {
        if !unique.iter().any(|seen| seen == entity_id) {
            unique.push(entity_id.clone());
        }
    }
    let new_ids: Vec<String> = unique.iter().map(|_| new_id()).collect();

    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    knowledge_repository::sync_chapter_mentions(
        &transaction,
        chapter_id,
        &unique,
        &new_ids,
        &now_timestamp(),
    )?;
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

fn ensure_known_owner(owner_type: &str) -> DatabaseCommandResult<()> {
    if is_known_owner_type(owner_type) {
        return Ok(());
    }
    Err(DatabaseCommandError::validation(format!(
        "Tipo de dono desconhecido para tag: {owner_type}."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    #[test]
    fn entidade_repetida_no_texto_vira_uma_mencao_so() {
        // O chamador manda a lista extraida do texto salvo, onde a mesma
        // entidade aparece varias vezes. Repetir nao pode virar linha
        // duplicada — a tabela mentions nao tem UNIQUE para segurar isso.
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name, sort_order, created_at, updated_at)
                   VALUES ('s1', 'u1', 'Historia', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO books (id, story_id, name, sort_order, created_at, updated_at)
                   VALUES ('b1', 's1', 'Livro', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO chapters (id, book_id, title, content, word_count, sort_order, created_at, updated_at)
                   VALUES ('c1', 'b1', 'Cap 1', '', 0, 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear");

        sync_chapter_mentions(
            &fixture.database,
            "c1",
            &["e1".into(), "e1".into(), "e1".into()],
        )
        .expect("sincronizar");

        let mentions = list_mentions_by_universe(&fixture.database, "u1").expect("listar");
        assert_eq!(mentions.len(), 1);
    }

    #[test]
    fn dono_desconhecido_e_recusado_com_mensagem_legivel() {
        let fixture = TemporaryDatabase::new();
        let error = set_tag(&fixture.database, "inventado", "x", "t1", true)
            .expect_err("dono invalido deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
        assert!(error.message.contains("inventado"));
    }

    #[test]
    fn desmarcar_tag_que_nao_estava_marcada_nao_e_erro() {
        // A tela alterna a marcacao; pedir para desmarcar o que ja esta
        // desmarcado e resultado esperado, nao falha.
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let tag = create_tag(&fixture.database, "u1", "Reescrever", "#7d3650").expect("criar tag");

        set_tag(&fixture.database, "chapter", "c1", &tag.id, false).expect("desmarcar");
    }
}
