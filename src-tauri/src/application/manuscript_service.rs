use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::manuscript::{
    Book, BookOption, BookUpdate, Chapter, ChapterOption, ChapterUpdate, Story, StoryUpdate,
};
use crate::domain::sync::{AggregateRef, Operation};
use crate::infrastructure::blob_document;
use crate::infrastructure::sqlite::sync_repository::{append_event_in_transaction, LocalChange};
use crate::infrastructure::sqlite::{manuscript_repository, SqliteDatabase};
use rusqlite::TransactionBehavior;

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
    identidade: &DeviceIdentity,
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

    // TERCEIRA BARREIRA (ADR 0010). Fail-closed, antes de qualquer escrita.
    //
    // O caminho que isto fecha tem quatro passos, e cada um piora o anterior:
    //
    //   inline  →  chapters  →  trg_chapter_revision  →  sync_events
    //              grava        copia os bytes          assina e propaga
    //
    // O evento é assinado e append-only: base64 que entra ali não sai mais, e
    // viaja para todos os aparelhos em cada sincronização. Recusar aqui é o
    // último ponto em que ainda há como não gravar.
    //
    // Parece redundante depois da barreira do editor, e não é: cobre legado
    // não migrado, banco importado, versão antiga do frontend, e o caminho de
    // colaboração que grava capítulo por outra porta.
    if let Some(content) = patch.content.as_deref() {
        if let Err(motivo) = blob_document::exigir_blob_safe(content) {
            return Err(DatabaseCommandError::validation(motivo.to_string()));
        }
    }
    let mut connection = database.write()?;

    // `IMMEDIATE`, e uma transação só para o dado E o evento. Duas transações
    // — salvar e depois registrar — reconstroem o buraco que o outbox existe
    // para fechar: uma queda no meio deixa o capítulo salvo neste aparelho e
    // invisível para todos os outros, sem nada registrando que faltou.
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if !manuscript_repository::update_chapter(&tx, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Capítulo não encontrado."));
    }

    // O payload é o estado NOVO do agregado, lido depois da escrita e dentro
    // da mesma transação. Montá-lo a partir do patch descreveria só o que a
    // tela mexeu, e quem recebe precisa do capítulo inteiro para convergir.
    let chapter = manuscript_repository::get_chapter(&tx, id)?
        .ok_or_else(|| DatabaseCommandError::not_found("Capítulo não encontrado."))?;
    let universe_id = universo_do_capitulo(&tx, id)?;
    let payload = serde_json::to_string(&chapter)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    append_event_in_transaction(
        &tx,
        identidade,
        &LocalChange {
            universe_id: &universe_id,
            aggregate: AggregateRef::new("chapter", id),
            operation: Operation::Upsert,
            payload: &payload,
        },
    )?;

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
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

/// O universo a que um capítulo pertence, pela cadeia livro → história.
///
/// O evento carrega `universe_id` porque ele é o escopo da replicação, e o
/// capítulo não guarda essa coluna — a informação vive na história.
fn universo_do_capitulo(
    connection: &rusqlite::Connection,
    chapter_id: &str,
) -> DatabaseCommandResult<String> {
    connection
        .query_row(
            "SELECT s.universe_id
               FROM chapters c
               JOIN books b ON b.id = c.book_id
               JOIN stories s ON s.id = b.story_id
              WHERE c.id = ?1",
            [chapter_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            DatabaseCommandError::storage(format!(
                "Não foi possível descobrir o universo do capítulo: {error}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    /// Diretório de dados de teste, com a identidade dentro. Some no `Drop`.
    struct DadosDoApp(std::path::PathBuf);

    impl DadosDoApp {
        fn novo() -> Self {
            let caminho =
                std::env::temp_dir().join(format!("narrahub-svc-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&caminho).expect("criar diretório");
            Self(caminho)
        }
    }

    impl Drop for DadosDoApp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Roda a sequência de arranque e devolve a identidade pronta para
    /// assinar — o mesmo caminho que o comando Tauri percorre.
    fn arrancar(fixture: &TemporaryDatabase) -> (DadosDoApp, DeviceIdentity) {
        let dados = DadosDoApp::novo();
        let identidade = crate::application::sync_bootstrap::prepare(&dados.0, &fixture.database)
            .expect("arranque");
        (dados, identidade)
    }

    fn seed_tree(fixture: &TemporaryDatabase) -> (Story, Book) {
        seed_universe(&fixture.connection(), "u1");
        let story = create_story(&fixture.database, "u1", "Historia").expect("criar historia");
        let book = create_book(&fixture.database, &story.id, "Livro").expect("criar livro");
        (story, book)
    }

    /// GATE DA ETAPA 3, primeiro sentido: falhou o dado, o evento não existe.
    #[test]
    fn dado_recusado_nao_deixa_evento_para_tras() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let (_dados, identidade) = arrancar(&fixture);

        update_chapter(
            &fixture.database,
            &identidade,
            "fantasma",
            ChapterUpdate {
                title: Some("x".into()),
                ..Default::default()
            },
        )
        .expect_err("capítulo inexistente");

        let eventos: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar eventos");
        assert_eq!(
            eventos, 0,
            "um evento sobrou descrevendo uma alteração que não aconteceu"
        );
    }

    /// GATE DA ETAPA 3, segundo sentido: falhou o evento, o dado volta.
    ///
    /// Este é o lado que só a transação compartilhada garante, e o mais fácil
    /// de perder — `UPDATE chapter; COMMIT` seguido de gravar o evento
    /// pareceria funcionar em todo teste feliz e deixaria o capítulo salvo e
    /// invisível para os outros aparelhos no dia em que o evento falhasse.
    ///
    /// A falha é forçada de forma realista: a identidade deixa de ser o
    /// `self` do roster — exatamente o que um banco restaurado de outro
    /// aparelho produz antes da reconciliação.
    #[test]
    fn evento_recusado_desfaz_a_alteracao_do_dado() {
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");
        let (_dados, identidade) = arrancar(&fixture);

        // O banco passa a declarar outro aparelho como `self`.
        {
            let connection = fixture.database.write().expect("abrir escrita");
            connection
                .execute(
                    "UPDATE sync_devices SET is_self = 0 WHERE device_id = ?1",
                    [identidade.device_id()],
                )
                .expect("rebaixar");
        }

        update_chapter(
            &fixture.database,
            &identidade,
            &chapter.id,
            ChapterUpdate {
                title: Some("Título que não pode sobreviver".into()),
                ..Default::default()
            },
        )
        .expect_err("sem `self` não há origem, e sem origem não há evento");

        let salvo = get_chapter(&fixture.database, &chapter.id)
            .expect("buscar")
            .expect("existe");
        assert_eq!(
            salvo.title, "Cap 1",
            "o título mudou apesar de o evento ter falhado: o dado ficaria preso neste aparelho"
        );

        let eventos: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar eventos");
        assert_eq!(eventos, 0);
    }

    /// A escrita real produz o evento certo: agregado, universo e payload com
    /// o estado NOVO — não o patch.
    #[test]
    fn salvar_capitulo_produz_evento_assinado_com_o_estado_novo() {
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");
        let (_dados, identidade) = arrancar(&fixture);

        update_chapter(
            &fixture.database,
            &identidade,
            &chapter.id,
            ChapterUpdate {
                content: Some("Ela entrou na cidade.".into()),
                word_count: Some(4),
                ..Default::default()
            },
        )
        .expect("salvar");

        let connection = fixture.connection();
        let (tipo, agregado, universo, payload): (String, String, String, String) = connection
            .query_row(
                "SELECT aggregate_type, aggregate_id, universe_id, payload FROM sync_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("ler evento");

        assert_eq!(tipo, "chapter");
        assert_eq!(agregado, chapter.id);
        assert_eq!(
            universo, "u1",
            "o evento precisa carregar o escopo da replicação"
        );

        // O payload é o capítulo inteiro, não só o campo que a tela mexeu:
        // quem recebe precisa do estado completo para convergir. O título
        // continua lá mesmo sem ter sido tocado neste salvamento.
        assert!(payload.contains("Ela entrou na cidade."));
        assert!(
            payload.contains("Cap 1"),
            "o payload descreveu só o patch: {payload}"
        );

        let saida = crate::infrastructure::sqlite::sync_repository::outbox_since(
            &connection,
            identidade.device_id(),
            0,
        )
        .expect("ler outbox");
        assert_eq!(saida.len(), 1);
        assert!(
            crate::domain::identity::verify(&saida[0], &identidade.public_base32()),
            "o evento que sai pelo outbox não verifica"
        );
    }

    /// Patch vazio não gera evento.
    ///
    /// A tela chama o salvamento mesmo quando nada mudou. Um evento por foco
    /// no editor encheria o log de ruído e faria os outros aparelhos
    /// reaplicarem o mesmo estado sem parar.
    #[test]
    fn patch_vazio_nao_gera_evento() {
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");
        let (_dados, identidade) = arrancar(&fixture);

        update_chapter(
            &fixture.database,
            &identidade,
            &chapter.id,
            ChapterUpdate::default(),
        )
        .expect("no-op");

        let eventos: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(eventos, 0);
    }

    #[test]
    fn salvar_texto_sem_contagem_e_recusado() {
        // A estatistica do universo soma word_count. Gravar o texto sem
        // recontar faz o total mentir ate o proximo salvamento.
        let fixture = TemporaryDatabase::new();
        let (_, book) = seed_tree(&fixture);
        let chapter = create_chapter(&fixture.database, &book.id, "Cap 1").expect("criar");

        let (_dados, identidade) = arrancar(&fixture);
        let error = update_chapter(
            &fixture.database,
            &identidade,
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
        let (_dados, identidade) = arrancar(&fixture);

        update_chapter(
            &fixture.database,
            &identidade,
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
                &identidade,
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
        let (_dados, identidade) = arrancar(&fixture);
        let error = update_chapter(
            &fixture.database,
            &identidade,
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

        let (_dados, identidade) = arrancar(&fixture);
        update_chapter(
            &fixture.database,
            &identidade,
            &chapter.id,
            ChapterUpdate::default(),
        )
        .expect("patch vazio e no-op");

        let saved = get_chapter(&fixture.database, &chapter.id)
            .expect("buscar")
            .expect("existe");
        assert_eq!(saved.updated_at, chapter.updated_at);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // A terceira barreira (ADR 0010)
    // ═══════════════════════════════════════════════════════════════════════

    fn semear_capitulo(fixture: &TemporaryDatabase, conteudo: &str) {
        fixture
            .connection()
            .execute_batch(&format!(
                "INSERT OR IGNORE INTO stories (id, universe_id, name, created_at, updated_at)
                    VALUES ('s1','u1','S','2026-01-01','2026-01-01');
                 INSERT OR IGNORE INTO books (id, story_id, name, created_at, updated_at)
                    VALUES ('b1','s1','L','2026-01-01','2026-01-01');
                 INSERT INTO chapters (id, book_id, title, content, word_count)
                    VALUES ('cap1','b1','Cap','{conteudo}', 5);"
            ))
            .expect("semear capítulo");
    }

    /// **`update_chapter` recusa mídia inline, e nada a jusante acontece.**
    ///
    /// O caminho que isto fecha tem quatro passos:
    ///
    /// ```text
    /// inline  →  chapters  →  trg_chapter_revision  →  sync_events
    ///                                                  assinado, append-only
    /// ```
    ///
    /// O evento é assinado e não se reescreve: base64 que entra ali viaja para
    /// todos os aparelhos, em cada sincronização, para sempre. O gate mede os
    /// quatro passos, não só a recusa.
    #[test]
    fn update_chapter_recusa_midia_inline() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let (_dados, identidade) = arrancar(&fixture);
        semear_capitulo(&fixture, "<p>o texto do escritor</p>");

        let base64 = crate::domain::data_url::codificar_base64(b"bytes-de-imagem");
        let inline = format!("<p>novo</p><img src=\"data:image/png;base64,{base64}\">");

        let erro = update_chapter(
            &fixture.database,
            &identidade,
            "cap1",
            ChapterUpdate {
                content: Some(inline),
                word_count: Some(2),
                ..Default::default()
            },
        )
        .expect_err("mídia inline não pode ser gravada");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);
        assert!(
            erro.message.contains("pendências de mídia"),
            "a mensagem precisa dizer ao escritor o que fazer: {}",
            erro.message
        );

        let conexao = fixture.connection();
        let conteudo: String = conexao
            .query_row(
                "SELECT content FROM chapters WHERE id = 'cap1'",
                [],
                |row| row.get(0),
            )
            .expect("ler capítulo");
        assert_eq!(conteudo, "<p>o texto do escritor</p>", "o capítulo mudou");

        let revisoes: i64 = conexao
            .query_row("SELECT COUNT(*) FROM chapter_revisions", [], |row| {
                row.get(0)
            })
            .expect("contar");
        assert_eq!(revisoes, 0, "o gatilho criou revisão");

        let eventos: i64 = conexao
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(eventos, 0, "um evento nasceu com bytes dentro");
    }

    /// Inline que não abre também é recusada.
    ///
    /// Ela põe bytes no banco do mesmo jeito. É o caso que o backfill preserva
    /// com pendência, e o capítulo fica sem poder ser salvo até a pendência ser
    /// resolvida — travar é melhor que continuar emitindo evento gigante.
    #[test]
    fn update_chapter_recusa_inline_que_nao_abre() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let (_dados, identidade) = arrancar(&fixture);
        semear_capitulo(&fixture, "<p>antes</p>");

        let erro = update_chapter(
            &fixture.database,
            &identidade,
            "cap1",
            ChapterUpdate {
                content: Some("<img src=\"data:image/png;base64,!!!\">".to_string()),
                word_count: Some(0),
                ..Default::default()
            },
        )
        .expect_err("inline quebrada também é inline");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);
    }

    /// **E o documento blob-safe passa, com evento nascendo sem bytes.**
    ///
    /// Senão o gate de cima passaria por vácuo: uma barreira que recusa tudo
    /// também recusa inline, e o escritor perderia a capacidade de salvar.
    #[test]
    fn update_chapter_aceita_blob_safe_e_o_evento_nasce_sem_bytes() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let (_dados, identidade) = arrancar(&fixture);
        semear_capitulo(&fixture, "<p>antes</p>");

        let hash = crate::infrastructure::blob_store::hash_dos_bytes(b"a-imagem");
        let blob_safe = format!(
            "<p>depois</p><img data-narrahub-blob=\"{hash}\" data-mime-type=\"image/png\">"
        );

        update_chapter(
            &fixture.database,
            &identidade,
            "cap1",
            ChapterUpdate {
                content: Some(blob_safe.clone()),
                word_count: Some(2),
                ..Default::default()
            },
        )
        .expect("documento blob-safe pode ser salvo");

        let conexao = fixture.connection();
        assert_eq!(
            conexao
                .query_row::<String, _, _>(
                    "SELECT content FROM chapters WHERE id = 'cap1'",
                    [],
                    |row| row.get(0)
                )
                .expect("ler"),
            blob_safe
        );

        // O evento nasceu, e o payload dele não carrega bytes.
        let payload: String = conexao
            .query_row("SELECT payload FROM sync_events", [], |row| row.get(0))
            .expect("ler o payload do evento");
        assert!(
            payload.contains(&hash),
            "o evento tem que carregar a referência: {payload}"
        );
        for proibido in ["data:image", "base64,"] {
            assert!(
                !payload.contains(proibido),
                "o payload do evento carrega {proibido}: {payload}"
            );
        }

        // E a revisão que o gatilho criou também é só referência.
        let revisao: String = conexao
            .query_row("SELECT content FROM chapter_revisions", [], |row| {
                row.get(0)
            })
            .expect("ler a revisão");
        assert_eq!(
            revisao, "<p>antes</p>",
            "a revisão guarda a versão anterior"
        );
    }

    /// Texto do escritor que **fala** de data URLs continua salvável.
    ///
    /// É o gate que separa validação estrutural de busca por substring. Um
    /// `contains("data:image")` travaria o capítulo de quem está escrevendo um
    /// tutorial.
    #[test]
    fn update_chapter_aceita_texto_que_menciona_data_url() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let (_dados, identidade) = arrancar(&fixture);
        semear_capitulo(&fixture, "<p>antes</p>");

        let prosa = "<p>Escreva &lt;img src=\"data:image/png;base64,AAAA\"&gt; para embutir.</p>";
        update_chapter(
            &fixture.database,
            &identidade,
            "cap1",
            ChapterUpdate {
                content: Some(prosa.to_string()),
                word_count: Some(7),
                ..Default::default()
            },
        )
        .expect("prosa do escritor não é mídia inline");

        assert_eq!(
            fixture
                .connection()
                .query_row::<String, _, _>(
                    "SELECT content FROM chapters WHERE id = 'cap1'",
                    [],
                    |row| row.get(0)
                )
                .expect("ler"),
            prosa
        );
    }
}
