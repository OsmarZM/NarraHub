use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::manuscript::{
    Book, BookOption, BookUpdate, Chapter, ChapterOption, ChapterUpdate, Story, StoryUpdate,
};
use crate::infrastructure::sqlite::{manuscript_repository, SqliteDatabase};

// ── História ─────────────────────────────────────────────────────────────

pub fn list_stories(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<Story>> {
    let connection = database.read()?;
    manuscript_repository::list_stories(&connection, universe_id)
}

pub fn create_story(
    database: &SqliteDatabase,
    universe_id: &str,
    name: &str,
) -> DatabaseCommandResult<Story> {
    let name = require_name(name, "A história precisa de um nome.")?;
    let connection = database.write()?;
    manuscript_repository::insert_story(
        &connection,
        &new_id(),
        universe_id,
        &name,
        "",
        &now_timestamp(),
    )
}

pub fn update_story(
    database: &SqliteDatabase,
    id: &str,
    patch: StoryUpdate,
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
            "A história precisa de um nome.",
        ));
    }
    let connection = database.write()?;
    if !manuscript_repository::update_story(&connection, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("História não encontrada."));
    }
    Ok(())
}

pub fn delete_story(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !manuscript_repository::delete_story(&connection, id)? {
        return Err(DatabaseCommandError::not_found("História não encontrada."));
    }
    Ok(())
}

// ── Livro ────────────────────────────────────────────────────────────────

pub fn list_books_by_story(
    database: &SqliteDatabase,
    story_id: &str,
) -> DatabaseCommandResult<Vec<Book>> {
    let connection = database.read()?;
    manuscript_repository::list_books_by_story(&connection, story_id)
}

pub fn list_books_by_universe(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<BookOption>> {
    let connection = database.read()?;
    manuscript_repository::list_books_by_universe(&connection, universe_id)
}

pub fn create_book(
    database: &SqliteDatabase,
    story_id: &str,
    name: &str,
) -> DatabaseCommandResult<Book> {
    let name = require_name(name, "O livro precisa de um nome.")?;
    let connection = database.write()?;
    manuscript_repository::insert_book(
        &connection,
        &new_id(),
        story_id,
        &name,
        "",
        &now_timestamp(),
    )
}

pub fn update_book(
    database: &SqliteDatabase,
    id: &str,
    patch: BookUpdate,
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
            "O livro precisa de um nome.",
        ));
    }
    let connection = database.write()?;
    if !manuscript_repository::update_book(&connection, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Livro não encontrado."));
    }
    Ok(())
}

pub fn delete_book(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !manuscript_repository::delete_book(&connection, id)? {
        return Err(DatabaseCommandError::not_found("Livro não encontrado."));
    }
    Ok(())
}

// ── Capítulo ─────────────────────────────────────────────────────────────

pub fn list_chapters_by_book(
    database: &SqliteDatabase,
    book_id: &str,
) -> DatabaseCommandResult<Vec<Chapter>> {
    let connection = database.read()?;
    manuscript_repository::list_chapters_by_book(&connection, book_id)
}

pub fn list_chapters_by_universe(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<ChapterOption>> {
    let connection = database.read()?;
    manuscript_repository::list_chapters_by_universe(&connection, universe_id)
}

pub fn get_chapter(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<Option<Chapter>> {
    let connection = database.read()?;
    manuscript_repository::get_chapter(&connection, id)
}

pub fn create_chapter(
    database: &SqliteDatabase,
    book_id: &str,
    title: &str,
) -> DatabaseCommandResult<Chapter> {
    let title = require_name(title, "O capítulo precisa de um título.")?;
    let connection = database.write()?;
    manuscript_repository::insert_chapter(&connection, &new_id(), book_id, &title, &now_timestamp())
}

/// Grava só os campos que a tela mexeu.
///
/// Um `content` sem `word_count` é recusado: a estatística do universo soma
/// `word_count`, então gravar um sem o outro faz o total mentir até o próximo
/// salvamento. É o tipo de inconsistência que o caminho antigo não tinha como
/// impedir, porque o método era `updateContent(id, content, wordCount)` e nada
/// obrigava quem chamava a recontar.
pub fn update_chapter(
    database: &SqliteDatabase,
    id: &str,
    patch: ChapterUpdate,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    if patch.content_without_word_count() {
        return Err(DatabaseCommandError::validation(
            "Salvar o texto exige também a contagem de palavras.",
        ));
    }
    if patch
        .title
        .as_deref()
        .is_some_and(|title| title.trim().is_empty())
    {
        return Err(DatabaseCommandError::validation(
            "O capítulo precisa de um título.",
        ));
    }
    let connection = database.write()?;
    if !manuscript_repository::update_chapter(&connection, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Capítulo não encontrado."));
    }
    Ok(())
}

/// Reordena os capítulos do livro numa transação.
///
/// Se a lista não bater com o livro, nada é gravado: reordenar metade
/// deixaria capítulos com a mesma posição, e a árvore passaria a mostrar uma
/// ordem que ninguém pediu.
pub fn reorder_chapters(
    database: &SqliteDatabase,
    book_id: &str,
    chapter_ids: &[String],
) -> DatabaseCommandResult<()> {
    if chapter_ids.is_empty() {
        return Ok(());
    }
    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let affected = manuscript_repository::reorder_chapters(&transaction, book_id, chapter_ids)?;
    if affected != chapter_ids.len() {
        return Err(DatabaseCommandError::conflict(
            "A lista de capítulos mudou. Atualize e tente novamente.",
        ));
    }
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

pub fn delete_chapter(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !manuscript_repository::delete_chapter(&connection, id)? {
        return Err(DatabaseCommandError::not_found("Capítulo não encontrado."));
    }
    Ok(())
}

fn require_name(value: &str, message: &str) -> DatabaseCommandResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DatabaseCommandError::validation(message));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    fn seed_tree(fixture: &TemporaryDatabase) -> (Story, Book) {
        seed_universe(&fixture.connection(), "u1");
        let story = create_story(&fixture.database, "u1", "Historia").expect("criar historia");
        let book = create_book(&fixture.database, &story.id, "Livro").expect("criar livro");
        (story, book)
    }

    #[test]
    fn salvar_texto_sem_contagem_e_recusado() {
        // A estatistica do universo soma word_count. Gravar o texto sem
        // recontar faz o total mentir ate o proximo salvamento.
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");

        let error = update_chapter(
            &fixture.database,
            &chapter.id,
            ChapterUpdate {
                content: Some("texto".into()),
                ..Default::default()
            },
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    #[test]
    fn autosave_sucessivo_nao_perde_o_resumo_nem_o_titulo() {
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");

        update_chapter(
            &fixture.database,
            &chapter.id,
            ChapterUpdate {
                summary: Some("resumo".into()),
                ..Default::default()
            },
        )
        .expect("salvar resumo");
        for texto in ["um", "um dois", "um dois tres"] {
            update_chapter(
                &fixture.database,
                &chapter.id,
                ChapterUpdate {
                    content: Some(texto.into()),
                    word_count: Some(texto.split(' ').count() as i64),
                    ..Default::default()
                },
            )
            .expect("autosave");
        }

        let saved = get_chapter(&fixture.database, &chapter.id)
            .expect("buscar")
            .expect("existe");
        assert_eq!(saved.content, "um dois tres");
        assert_eq!(saved.word_count, 3);
        assert_eq!(saved.summary, "resumo");
        assert_eq!(saved.title, "Cap 1");
    }

    #[test]
    fn reordenar_com_lista_que_nao_bate_nao_grava_nada() {
        // Exigencia do plano: transacao revertida no meio nao pode deixar a
        // arvore com capitulos na mesma posicao.
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let primeiro = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");
        let segundo = create_chapter(&fixture.database, &book.id, "Cap 2").expect("criar");

        let error = reorder_chapters(
            &fixture.database,
            &book.id,
            &[segundo.id.clone(), primeiro.id.clone(), "fantasma".into()],
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);

        let chapters = list_chapters_by_book(&fixture.database, &book.id).expect("listar");
        assert_eq!(
            chapters.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec![primeiro.id.as_str(), segundo.id.as_str()],
            "a ordem original tinha que ter sido preservada"
        );
    }

    #[test]
    fn capitulo_inexistente_avisa_em_vez_de_gravar_no_vazio() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let error = update_chapter(
            &fixture.database,
            "fantasma",
            ChapterUpdate {
                title: Some("x".into()),
                ..Default::default()
            },
        )
        .expect_err("deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::NotFound);
    }

    #[test]
    fn patch_vazio_nao_carimba_a_linha() {
        // A tela chama o salvamento mesmo quando nada mudou. Carimbar ali
        // faria a sincronizacao achar que o capitulo mudou a cada foco.
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");

        update_chapter(&fixture.database, &chapter.id, ChapterUpdate::default())
            .expect("patch vazio e no-op");

        let saved = get_chapter(&fixture.database, &chapter.id)
            .expect("buscar")
            .expect("existe");
        assert_eq!(saved.updated_at, chapter.updated_at);
    }
}
