//! **A caixa de recuperação do legado** (etapa H, H-R3).
//!
//! O Sync V1 saiu do runtime na etapa G. O que ficou dele, `sync_conflicts`, guarda uma coisa que
//! nenhuma outra linha do banco guarda:
//!
//! ```text
//! conteúdo materializado = A
//! local_value            = A
//! remote_value           = B    ← a versão do outro aparelho, só nesta linha
//! ```
//!
//! B não é acervo: não tem revisão, não viaja no bootstrap e o V1 nunca teve tela para decidir
//! sobre ele. Se o último aparelho que o guarda sair de uso, B some sem ninguém ter escolhido —
//! e perda silenciosa é justamente o que a etapa H proíbe.
//!
//! O desenho, então:
//!
//! ```text
//! migrations
//!    ↓
//! conversão de mídia do legado (ADR 0010)   ← B ganha as referências de blob
//!    ↓
//! importar()                                ← AQUI, ainda antes de `Ready`
//!    ↓
//! Ready                                     ← depois daqui, ninguém lê sync_conflicts (G11)
//! ```
//!
//! O import é idempotente por `source_conflict_id` (`ON CONFLICT DO NOTHING`, nunca `REPLACE`):
//! um item já decidido não volta a `pending` no próximo arranque. E `sync_conflicts` não é
//! alterada em nenhum momento — ela é auditoria histórica, e continua imutável.
//!
//! O escritor decide item a item:
//!
//! ```text
//! preservar   B vira um capítulo NOVO (id novo), por `Mutacao` normal — evento, outbox, Sync V2
//! descartar   a pendência sai da caixa; a linha histórica fica onde está
//! ```
//!
//! Não existe "substituir o capítulo atual": não há causalidade V2 para uma decisão histórica, e
//! inventar uma seria o oposto do que as etapas F e G construíram.

use serde::{Deserialize, Serialize};

use crate::application::mutacao::Mutacao;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::{new_id, now_timestamp};
use crate::infrastructure::sqlite::SqliteDatabase;
use rusqlite::{Connection, OptionalExtension, Transaction};

/// Um item da caixa, como a tela o vê.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeRecuperacao {
    pub id: String,
    /// `pending`, `preserved` ou `discarded`.
    pub status: String,
    /// O tipo do agregado no legado (o V1 conhecido só produzia `chapter`).
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub field: String,
    /// O título do capítulo original, quando ele ainda existe aqui.
    pub titulo_atual: String,
    /// O livro do capítulo original, quando ele ainda existe aqui.
    pub livro_atual: String,
    /// O que está no acervo hoje (vazio quando o capítulo não existe mais).
    pub versao_atual: String,
    /// A versão antiga que só existe nesta linha.
    pub versao_antiga: String,
    pub registrado_em: String,
    pub resolvido_em: String,
    /// O capítulo criado pela preservação, quando houve.
    pub capitulo_preservado: String,
}

/// Um livro que pode receber um capítulo recuperado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinoPossivel {
    pub book_id: String,
    pub book_name: String,
    pub story_name: String,
    pub universe_id: String,
    pub universe_name: String,
    /// É o livro do capítulo original — a tela pode pré-selecionar, e o escritor confirma.
    pub e_o_livro_original: bool,
}

/// O que a preservação recebe da tela.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PedidoDePreservacao {
    pub id: String,
    pub book_id: String,
    pub titulo: String,
}

fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// **O import, antes de `Ready`.** Copia os conflitos V1 ainda pendentes para a caixa.
///
/// Roda depois da conversão de mídia: o que entra aqui já está no contrato do ADR 0010. Só os
/// abertos (`resolved_at = ''`) viram pendência — decisão já tomada no passado não volta a
/// perguntar. `sync_conflicts` não é tocada.
///
/// O V1 conhecido só gravava `chapter`/`content`. Se aparecer outra forma histórica, ela é
/// preservada como item genérico: a tela mostra, e nenhuma ação automática de domínio é inventada
/// para ela.
pub fn importar(database: &SqliteDatabase) -> DatabaseCommandResult<usize> {
    let mut connection = database.write()?;
    let tx = connection.transaction().map_err(erro)?;
    let importados = importar_na_transacao(&tx)?;
    tx.commit().map_err(erro)?;
    Ok(importados)
}

pub(crate) fn importar_na_transacao(tx: &Transaction<'_>) -> DatabaseCommandResult<usize> {
    let pendentes: Vec<(String, String, String, String, String, String, String)> = {
        let mut consulta = tx
            .prepare(
                "SELECT id, aggregate_type, aggregate_id, field, local_value, remote_value,
                        created_at
                   FROM sync_conflicts
                  WHERE resolved_at = ''
                  ORDER BY created_at, id",
            )
            .map_err(erro)?;
        let linhas = consulta
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)?
    };

    let mut importados = 0usize;
    for (origem, tipo, id, campo, local, remoto, criado_em) in pendentes {
        // `DO NOTHING`, nunca `REPLACE`: um item já preservado ou descartado não volta a pendente.
        importados += tx
            .execute(
                "INSERT INTO legacy_recovery_items
                    (id, source_conflict_id, aggregate_type, aggregate_id, field,
                     local_value, remote_value, source_created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(source_conflict_id) DO NOTHING",
                rusqlite::params![new_id(), origem, tipo, id, campo, local, remoto, criado_em],
            )
            .map_err(erro)?;
    }
    Ok(importados)
}

/// Quantos itens ainda esperam decisão. É o número do aviso.
pub fn contar_pendentes(connection: &Connection) -> DatabaseCommandResult<i64> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM legacy_recovery_items WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .map_err(erro)
}

/// A caixa inteira: pendentes primeiro, mais antigos antes.
pub fn listar(connection: &Connection) -> DatabaseCommandResult<Vec<ItemDeRecuperacao>> {
    let mut consulta = connection
        .prepare(
            "SELECT i.id, i.status, i.aggregate_type, i.aggregate_id, i.field, i.remote_value,
                    i.source_created_at, i.resolved_at, i.preserved_chapter_id,
                    COALESCE(c.title, ''), COALESCE(c.content, ''), COALESCE(b.name, '')
               FROM legacy_recovery_items i
               LEFT JOIN chapters c ON c.id = i.aggregate_id AND i.aggregate_type = 'chapter'
               LEFT JOIN books b ON b.id = c.book_id
              ORDER BY (i.status <> 'pending'), i.source_created_at, i.id",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([], |row| {
            Ok(ItemDeRecuperacao {
                id: row.get(0)?,
                status: row.get(1)?,
                aggregate_type: row.get(2)?,
                aggregate_id: row.get(3)?,
                field: row.get(4)?,
                versao_antiga: row.get(5)?,
                registrado_em: row.get(6)?,
                resolvido_em: row.get(7)?,
                capitulo_preservado: row.get(8)?,
                titulo_atual: row.get(9)?,
                versao_atual: row.get(10)?,
                livro_atual: row.get(11)?,
            })
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

/// Os livros que podem receber um capítulo recuperado, com o do capítulo original marcado.
pub fn destinos(
    connection: &Connection,
    item: &str,
) -> DatabaseCommandResult<Vec<DestinoPossivel>> {
    let original: Option<String> = connection
        .query_row(
            "SELECT c.book_id FROM legacy_recovery_items i
               JOIN chapters c ON c.id = i.aggregate_id
              WHERE i.id = ?1 AND i.aggregate_type = 'chapter'",
            [item],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)?;
    let mut consulta = connection
        .prepare(
            "SELECT b.id, b.name, s.name, u.id, u.name
               FROM books b
               JOIN stories s ON s.id = b.story_id
               JOIN universes u ON u.id = s.universe_id
              ORDER BY u.name, s.sort_order, s.id, b.sort_order, b.id",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([], |row| {
            let book_id: String = row.get(0)?;
            Ok(DestinoPossivel {
                e_o_livro_original: original.as_deref() == Some(book_id.as_str()),
                book_id,
                book_name: row.get(1)?,
                story_name: row.get(2)?,
                universe_id: row.get(3)?,
                universe_name: row.get(4)?,
            })
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

/// **Preservar**: a versão antiga vira um capítulo novo, e o item sai da caixa. Tudo ou nada.
///
/// O capítulo nasce por `Mutacao`, com id novo — nunca o do capítulo em conflito —, então ele
/// ganha revisão, evento e outbox como qualquer conteúdo, e sincroniza normalmente. A marca
/// `preserved` é gravada **na mesma transação**: não existe estado em que o capítulo exista e o
/// item continue pendente, nem o contrário. Repetir a chamada depois do sucesso é recusado como
/// "já resolvido" — nunca cria uma segunda cópia.
pub fn preservar(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    pedido: &PedidoDePreservacao,
) -> DatabaseCommandResult<String> {
    let titulo = pedido.titulo.trim().to_string();
    if titulo.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O capítulo recuperado precisa de um título.",
        ));
    }
    if pedido.book_id.trim().is_empty() {
        return Err(DatabaseCommandError::validation(
            "Escolha o livro que vai receber o capítulo recuperado.",
        ));
    }
    Mutacao::executar(database, identidade, |m| {
        let (status, texto): (String, String) = m
            .tx()
            .query_row(
                "SELECT status, remote_value FROM legacy_recovery_items WHERE id = ?1",
                [&pedido.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(erro)?
            .ok_or_else(|| {
                DatabaseCommandError::not_found("Esta versão antiga não existe neste aparelho.")
            })?;
        if status != "pending" {
            return Err(DatabaseCommandError::conflict(
                "Esta versão antiga já foi resolvida. Nada foi alterado.",
            ));
        }
        let existe_livro: bool = m
            .tx()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM books WHERE id = ?1)",
                [&pedido.book_id],
                |row| row.get(0),
            )
            .map_err(erro)?;
        if !existe_livro {
            return Err(DatabaseCommandError::not_found(
                "O livro escolhido não existe neste aparelho.",
            ));
        }

        let capitulo = crate::infrastructure::sqlite::manuscript_repository::insert_chapter(
            m.tx(),
            &new_id(),
            &pedido.book_id,
            &titulo,
            &now_timestamp(),
        )?;
        let palavras = crate::infrastructure::sqlite::sync_codec::palavras::contar(&texto);
        m.tx()
            .execute(
                "UPDATE chapters SET content = ?2, word_count = ?3 WHERE id = ?1",
                rusqlite::params![&capitulo.id, &texto, palavras],
            )
            .map_err(erro)?;
        m.gravou("chapter", &capitulo.id)?;
        m.gravou("chapter_position", &capitulo.id)?;
        m.tx()
            .execute(
                "UPDATE legacy_recovery_items
                    SET status = 'preserved', resolved_at = datetime('now'),
                        preserved_chapter_id = ?2
                  WHERE id = ?1 AND status = 'pending'",
                rusqlite::params![&pedido.id, &capitulo.id],
            )
            .map_err(erro)?;
        Ok(capitulo.id)
    })
}

/// **Descartar**: a pendência sai da caixa, e só dela.
///
/// `sync_conflicts` não é tocada: a linha histórica continua no banco, e o backup continua
/// levando as duas coisas. É perda de pendência, não de evidência — e a tela pede confirmação
/// explícita antes de chegar aqui.
pub fn descartar(database: &SqliteDatabase, item: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    let mudou = connection
        .execute(
            "UPDATE legacy_recovery_items
                SET status = 'discarded', resolved_at = datetime('now')
              WHERE id = ?1 AND status = 'pending'",
            [item],
        )
        .map_err(erro)?;
    if mudou == 0 {
        return Err(DatabaseCommandError::conflict(
            "Esta versão antiga já foi resolvida. Nada foi alterado.",
        ));
    }
    Ok(())
}
