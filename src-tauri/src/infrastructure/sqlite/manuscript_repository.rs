use crate::database::error::DatabaseCommandResult;
use crate::domain::manuscript::{
    Book, BookOption, BookUpdate, Chapter, ChapterOption, ChapterUpdate, Story, StoryUpdate,
};
use rusqlite::{Connection, Row, Transaction};

use super::connection::map_sqlite_error;

const CHAPTER_COLUMNS: &str = "id, book_id, title, content, summary, scene_origin, \
     scene_destination, word_count, status, canon_status, sort_order, created_at, updated_at";

/// Monta um `UPDATE` só com as colunas que vieram, devolvendo se atingiu
/// alguma linha. Compartilhado pelas três tabelas do manuscrito porque a regra
/// é a mesma: `None` é "não mexer", não "gravar vazio".
fn update_columns(
    connection: &Connection,
    table: &str,
    id: &str,
    columns: Vec<(&str, Box<dyn rusqlite::ToSql>)>,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut assignments = vec!["updated_at = ?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(updated_at.to_string())];

    for (column, value) in columns {
        values.push(value);
        assignments.push(format!("{column} = ?{}", values.len()));
    }

    values.push(Box::new(id.to_string()));
    let sql = format!(
        "UPDATE {table} SET {} WHERE id = ?{}",
        assignments.join(", "),
        values.len()
    );
    let parameters: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    let affected = connection
        .execute(&sql, parameters.as_slice())
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

// ── História ─────────────────────────────────────────────────────────────

fn story_from_row(row: &Row<'_>) -> rusqlite::Result<Story> {
    Ok(Story {
        id: row.get("id")?,
        universe_id: row.get("universe_id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn list_stories(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<Story>> {
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, name, description, sort_order, created_at, updated_at
               FROM stories WHERE universe_id = ?1 ORDER BY sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], story_from_row)
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// A posição sai de uma subquery no próprio `INSERT`, e não de um `SELECT MAX`
/// separado — o caminho antigo dava a mesma posição a duas histórias criadas
/// ao mesmo tempo.
pub fn insert_story(
    connection: &Connection,
    id: &str,
    universe_id: &str,
    name: &str,
    description: &str,
    timestamp: &str,
) -> DatabaseCommandResult<Story> {
    connection
        .execute(
            "INSERT INTO stories (id, universe_id, name, description, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4,
                     (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM stories WHERE universe_id = ?2),
                     ?5, ?5)",
            rusqlite::params![id, universe_id, name, description, timestamp],
        )
        .map_err(map_sqlite_error)?;
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, name, description, sort_order, created_at, updated_at
               FROM stories WHERE id = ?1",
        )
        .map_err(map_sqlite_error)?;
    statement
        .query_row([id], story_from_row)
        .map_err(map_sqlite_error)
}

pub fn update_story(
    connection: &Connection,
    id: &str,
    patch: &StoryUpdate,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut columns: Vec<(&str, Box<dyn rusqlite::ToSql>)> = Vec::new();
    if let Some(name) = &patch.name {
        columns.push(("name", Box::new(name.clone())));
    }
    if let Some(description) = &patch.description {
        columns.push(("description", Box::new(description.clone())));
    }
    update_columns(connection, "stories", id, columns, updated_at)
}

pub fn delete_story(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM stories WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

// ── Livro ────────────────────────────────────────────────────────────────

fn book_from_row(row: &Row<'_>) -> rusqlite::Result<Book> {
    Ok(Book {
        id: row.get("id")?,
        story_id: row.get("story_id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        cover_image: row.get("cover_image")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn list_books_by_story(
    connection: &Connection,
    story_id: &str,
) -> DatabaseCommandResult<Vec<Book>> {
    let mut statement = connection
        .prepare(
            "SELECT id, story_id, name, description, cover_image, sort_order, created_at, updated_at
               FROM books WHERE story_id = ?1 ORDER BY sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([story_id], book_from_row)
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn list_books_by_universe(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<BookOption>> {
    let mut statement = connection
        .prepare(
            "SELECT b.id, b.story_id, b.name, b.description, b.cover_image, b.sort_order,
                    b.created_at, b.updated_at, s.name AS story_name
               FROM books b
               JOIN stories s ON s.id = b.story_id
              WHERE s.universe_id = ?1
              ORDER BY s.sort_order, b.sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(BookOption {
                book: book_from_row(row)?,
                story_name: row.get("story_name")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn insert_book(
    connection: &Connection,
    id: &str,
    story_id: &str,
    name: &str,
    description: &str,
    timestamp: &str,
) -> DatabaseCommandResult<Book> {
    connection
        .execute(
            "INSERT INTO books (id, story_id, name, description, cover_image, sort_order,
                                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '',
                     (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM books WHERE story_id = ?2),
                     ?5, ?5)",
            rusqlite::params![id, story_id, name, description, timestamp],
        )
        .map_err(map_sqlite_error)?;
    let mut statement = connection
        .prepare(
            "SELECT id, story_id, name, description, cover_image, sort_order, created_at, updated_at
               FROM books WHERE id = ?1",
        )
        .map_err(map_sqlite_error)?;
    statement
        .query_row([id], book_from_row)
        .map_err(map_sqlite_error)
}

pub fn update_book(
    connection: &Connection,
    id: &str,
    patch: &BookUpdate,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut columns: Vec<(&str, Box<dyn rusqlite::ToSql>)> = Vec::new();
    if let Some(name) = &patch.name {
        columns.push(("name", Box::new(name.clone())));
    }
    if let Some(description) = &patch.description {
        columns.push(("description", Box::new(description.clone())));
    }
    if let Some(cover_image) = &patch.cover_image {
        columns.push(("cover_image", Box::new(cover_image.clone())));
    }
    update_columns(connection, "books", id, columns, updated_at)
}

pub fn delete_book(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM books WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

// ── Capítulo ─────────────────────────────────────────────────────────────

fn chapter_from_row(row: &Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get("id")?,
        book_id: row.get("book_id")?,
        title: row.get("title")?,
        content: row.get("content")?,
        summary: row.get("summary")?,
        scene_origin: row.get("scene_origin")?,
        scene_destination: row.get("scene_destination")?,
        word_count: row.get("word_count")?,
        status: row.get("status")?,
        canon_status: row.get("canon_status")?,
        sort_order: row.get("sort_order")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn list_chapters_by_book(
    connection: &Connection,
    book_id: &str,
) -> DatabaseCommandResult<Vec<Chapter>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {CHAPTER_COLUMNS} FROM chapters WHERE book_id = ?1 ORDER BY sort_order ASC"
        ))
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([book_id], chapter_from_row)
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn list_chapters_by_universe(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<ChapterOption>> {
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.book_id, c.title, c.content, c.summary, c.scene_origin,
                    c.scene_destination, c.word_count, c.status, c.canon_status, c.sort_order,
                    c.created_at, c.updated_at,
                    b.name AS book_name, b.story_id, s.name AS story_name
               FROM chapters c
               JOIN books b ON b.id = c.book_id
               JOIN stories s ON s.id = b.story_id
              WHERE s.universe_id = ?1
              ORDER BY s.sort_order, b.sort_order, c.sort_order",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(ChapterOption {
                chapter: chapter_from_row(row)?,
                book_name: row.get("book_name")?,
                story_id: row.get("story_id")?,
                story_name: row.get("story_name")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn get_chapter(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<Chapter>> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {CHAPTER_COLUMNS} FROM chapters WHERE id = ?1"
        ))
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query_map([id], chapter_from_row)
        .map_err(map_sqlite_error)?;
    match rows.next() {
        Some(row) => Ok(Some(row.map_err(map_sqlite_error)?)),
        None => Ok(None),
    }
}

pub fn insert_chapter(
    connection: &Connection,
    id: &str,
    book_id: &str,
    title: &str,
    timestamp: &str,
) -> DatabaseCommandResult<Chapter> {
    connection
        .execute(
            "INSERT INTO chapters (id, book_id, title, content, word_count, status, canon_status,
                                   sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, '', 0, 'IDEIA', 'CANON',
                     (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM chapters WHERE book_id = ?2),
                     ?4, ?4)",
            rusqlite::params![id, book_id, title, timestamp],
        )
        .map_err(map_sqlite_error)?;
    get_chapter(connection, id)?.ok_or_else(|| {
        crate::database::error::DatabaseCommandError::storage(
            "O capítulo criado não pôde ser lido de volta.",
        )
    })
}

pub fn update_chapter(
    connection: &Connection,
    id: &str,
    patch: &ChapterUpdate,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut columns: Vec<(&str, Box<dyn rusqlite::ToSql>)> = Vec::new();
    if let Some(title) = &patch.title {
        columns.push(("title", Box::new(title.clone())));
    }
    if let Some(content) = &patch.content {
        columns.push(("content", Box::new(content.clone())));
    }
    if let Some(word_count) = patch.word_count {
        columns.push(("word_count", Box::new(word_count)));
    }
    if let Some(summary) = &patch.summary {
        columns.push(("summary", Box::new(summary.clone())));
    }
    if let Some(scene_origin) = &patch.scene_origin {
        columns.push(("scene_origin", Box::new(scene_origin.clone())));
    }
    if let Some(scene_destination) = &patch.scene_destination {
        columns.push(("scene_destination", Box::new(scene_destination.clone())));
    }
    if let Some(status) = &patch.status {
        columns.push(("status", Box::new(status.clone())));
    }
    if let Some(canon_status) = &patch.canon_status {
        columns.push(("canon_status", Box::new(canon_status.clone())));
    }
    update_columns(connection, "chapters", id, columns, updated_at)
}

/// Reordena os capítulos de um livro. Devolve quantos foram atingidos para
/// quem chama poder recusar a operação quando a lista não bate com o livro.
pub fn reorder_chapters(
    transaction: &Transaction<'_>,
    book_id: &str,
    chapter_ids: &[String],
) -> DatabaseCommandResult<usize> {
    let mut statement = transaction
        .prepare("UPDATE chapters SET sort_order = ?1 WHERE id = ?2 AND book_id = ?3")
        .map_err(map_sqlite_error)?;
    let mut affected = 0;
    for (index, chapter_id) in chapter_ids.iter().enumerate() {
        affected += statement
            .execute(rusqlite::params![index as i64, chapter_id, book_id])
            .map_err(map_sqlite_error)?;
    }
    Ok(affected)
}

pub fn delete_chapter(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM chapters WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{migrated_memory_database, seed_universe};
    use super::*;
    use crate::database::error::DatabaseErrorKind;

    fn seed_tree(connection: &Connection) -> (String, String) {
        seed_universe(connection, "u1");
        let story = insert_story(
            connection,
            "s1",
            "u1",
            "Historia",
            "",
            "2026-01-01 00:00:00",
        )
        .expect("criar historia");
        let book = insert_book(
            connection,
            "b1",
            &story.id,
            "Livro",
            "",
            "2026-01-01 00:00:00",
        )
        .expect("criar livro");
        (story.id, book.id)
    }

    #[test]
    fn historia_livro_e_capitulo_nascem_no_fim_da_lista() {
        let connection = migrated_memory_database();
        let (story_id, book_id) = seed_tree(&connection);

        let segunda = insert_story(&connection, "s2", "u1", "Outra", "", "2026-01-01 00:00:00")
            .expect("criar segunda historia");
        assert_eq!(segunda.sort_order, 1);

        let segundo_livro = insert_book(
            &connection,
            "b2",
            &story_id,
            "Livro 2",
            "",
            "2026-01-01 00:00:00",
        )
        .expect("criar segundo livro");
        assert_eq!(segundo_livro.sort_order, 1);

        insert_chapter(&connection, "c1", &book_id, "Cap 1", "2026-01-01 00:00:00").expect("criar");
        let segundo_cap =
            insert_chapter(&connection, "c2", &book_id, "Cap 2", "2026-01-01 00:00:00")
                .expect("criar");
        assert_eq!(segundo_cap.sort_order, 1);
    }

    #[test]
    fn capitulo_novo_nasce_com_os_padroes_do_caminho_antigo() {
        let connection = migrated_memory_database();
        let (_, book_id) = seed_tree(&connection);

        let chapter = insert_chapter(&connection, "c1", &book_id, "Cap 1", "2026-01-01 00:00:00")
            .expect("criar");
        assert_eq!(chapter.content, "");
        assert_eq!(chapter.word_count, 0);
        assert_eq!(chapter.status, "IDEIA");
        assert_eq!(chapter.canon_status, "CANON");
    }

    #[test]
    fn salvar_so_o_resumo_nao_encosta_no_texto() {
        // Cada tela salva um pedaco do capitulo. Se o UPDATE gravasse tudo, o
        // inspetor salvando o resumo apagaria o que o editor acabou de digitar.
        let connection = migrated_memory_database();
        let (_, book_id) = seed_tree(&connection);
        insert_chapter(&connection, "c1", &book_id, "Cap 1", "2026-01-01 00:00:00").expect("criar");
        update_chapter(
            &connection,
            "c1",
            &ChapterUpdate {
                content: Some("texto do editor".into()),
                word_count: Some(3),
                ..Default::default()
            },
            "2026-02-01 00:00:00",
        )
        .expect("salvar texto");

        update_chapter(
            &connection,
            "c1",
            &ChapterUpdate {
                summary: Some("resumo".into()),
                ..Default::default()
            },
            "2026-03-01 00:00:00",
        )
        .expect("salvar resumo");

        let chapter = get_chapter(&connection, "c1")
            .expect("buscar")
            .expect("existe");
        assert_eq!(chapter.content, "texto do editor");
        assert_eq!(chapter.word_count, 3);
        assert_eq!(chapter.summary, "resumo");
    }

    #[test]
    fn excluir_historia_leva_livros_e_capitulos_junto() {
        let connection = migrated_memory_database();
        let (story_id, book_id) = seed_tree(&connection);
        insert_chapter(&connection, "c1", &book_id, "Cap 1", "2026-01-01 00:00:00").expect("criar");

        assert!(delete_story(&connection, &story_id).expect("excluir"));
        assert!(list_books_by_story(&connection, &story_id)
            .expect("listar")
            .is_empty());
        assert!(get_chapter(&connection, "c1").expect("buscar").is_none());
    }

    #[test]
    fn livro_de_historia_inexistente_e_recusado_pela_foreign_key() {
        let connection = migrated_memory_database();
        let error = insert_book(
            &connection,
            "b1",
            "fantasma",
            "Livro",
            "",
            "2026-01-01 00:00:00",
        )
        .expect_err("FK deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);
    }

    #[test]
    fn reordenar_conta_so_os_capitulos_do_livro_pedido() {
        let connection = migrated_memory_database();
        let (story_id, book_id) = seed_tree(&connection);
        let outro = insert_book(
            &connection,
            "b2",
            &story_id,
            "Outro",
            "",
            "2026-01-01 00:00:00",
        )
        .expect("criar outro livro");
        insert_chapter(&connection, "c1", &book_id, "Cap 1", "2026-01-01 00:00:00").expect("criar");
        insert_chapter(&connection, "c2", &book_id, "Cap 2", "2026-01-01 00:00:00").expect("criar");
        insert_chapter(
            &connection,
            "alheio",
            &outro.id,
            "Alheio",
            "2026-01-01 00:00:00",
        )
        .expect("criar");

        let mut connection = connection;
        let transaction = connection.transaction().expect("transacao");
        let affected = reorder_chapters(
            &transaction,
            &book_id,
            &["c2".into(), "c1".into(), "alheio".into()],
        )
        .expect("reordenar");
        transaction.commit().expect("commit");

        assert_eq!(affected, 2, "o capitulo do outro livro nao pode contar");
        let chapters = list_chapters_by_book(&connection, &book_id).expect("listar");
        assert_eq!(
            chapters.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["c2", "c1"]
        );
    }

    #[test]
    fn listagem_do_universo_segue_a_ordem_de_leitura() {
        let connection = migrated_memory_database();
        let (story_id, book_id) = seed_tree(&connection);
        let outro = insert_book(
            &connection,
            "b2",
            &story_id,
            "Livro 2",
            "",
            "2026-01-01 00:00:00",
        )
        .expect("criar");
        insert_chapter(
            &connection,
            "c2",
            &outro.id,
            "Do livro 2",
            "2026-01-01 00:00:00",
        )
        .expect("criar");
        insert_chapter(
            &connection,
            "c1",
            &book_id,
            "Do livro 1",
            "2026-01-01 00:00:00",
        )
        .expect("criar");

        let chapters = list_chapters_by_universe(&connection, "u1").expect("listar");
        assert_eq!(
            chapters
                .iter()
                .map(|c| c.chapter.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c1", "c2"]
        );
        assert_eq!(chapters[0].story_name, "Historia");
        assert_eq!(chapters[0].book_name, "Livro");
    }
}
