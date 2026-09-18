use crate::application::mutacao::Mutacao;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
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
    identidade: &DeviceIdentity,
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
    Mutacao::executar(database, identidade, |m| {
        nome_livre(m.tx(), universe_id, name, &tag.id)?;
        knowledge_repository::insert_tag(m.tx(), &tag)?;
        m.gravou("content_tag", &tag.id)
    })?;
    Ok(tag)
}

/// Renomeia ou recolore a tag.
///
/// **Não existia até a B5.** O app criava e apagava tag, e nada mais — o payload canônico carrega
/// nome e cor, então uma tag que chega de outro aparelho com nome diferente precisa ter um caminho
/// local equivalente, senão o codec representaria um estado que nenhuma mão daqui produz. É também
/// o que resolve `tag_name_conflict`: a saída para duas tags homônimas é renomear uma.
pub fn update_tag(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
    name: &str,
    color: &str,
) -> DatabaseCommandResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation(
            "A tag precisa de um nome.",
        ));
    }
    Mutacao::executar(database, identidade, |m| {
        let universe_id: String = m
            .tx()
            .query_row(
                "SELECT universe_id FROM content_tags WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseCommandError::not_found("A tag não existe mais."))?;
        nome_livre(m.tx(), &universe_id, name, id)?;
        m.tx()
            .execute(
                "UPDATE content_tags SET name = ?1, color = ?2 WHERE id = ?3",
                [name, color, id],
            )
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        m.gravou("content_tag", id)
    })
}

/// `UNIQUE(universe_id, name COLLATE NOCASE)` recusaria isso com erro de constraint. A mensagem
/// legível vem daqui, antes de o banco reclamar.
fn nome_livre(
    connection: &rusqlite::Connection,
    universe_id: &str,
    name: &str,
    id: &str,
) -> DatabaseCommandResult<()> {
    let ocupado: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM content_tags
                            WHERE universe_id = ?1 AND name = ?2 COLLATE NOCASE AND id <> ?3)",
            [universe_id, name, id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if ocupado {
        return Err(DatabaseCommandError::validation(format!(
            "Já existe uma tag chamada \"{name}\" neste universo."
        )));
    }
    Ok(())
}

/// Marca ou desmarca a tag no dono.
///
/// A identidade causal da marcação é `tagId:ownerType:ownerId` desde a B2 — o `id` aleatório da
/// linha nunca participou dela. Marcar a mesma tag no mesmo capítulo nos dois aparelhos é **a
/// mesma marcação**, e converge sem virar duas coisas nem abrir divergência.
pub fn set_tag(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    owner_type: &str,
    owner_id: &str,
    tag_id: &str,
    assigned: bool,
) -> DatabaseCommandResult<()> {
    ensure_known_owner(owner_type)?;
    let agregado = crate::infrastructure::sqlite::sync_codec::manuscrito::id_da_atribuicao(
        tag_id, owner_type, owner_id,
    );
    Mutacao::executar(database, identidade, |m| {
        if assigned {
            knowledge_repository::assign_tag(
                m.tx(),
                &new_id(),
                tag_id,
                owner_type,
                owner_id,
                &now_timestamp(),
            )?;
            m.gravou("tag_assignment", &agregado)
        } else {
            // Desmarcar o que já estava desmarcado é resultado esperado, não erro — a tela
            // alterna a marcação. Sem linha, não há exclusão para declarar.
            if !existe_marcacao(m.tx(), tag_id, owner_type, owner_id)? {
                return Ok(());
            }
            m.excluir("tag_assignment", &agregado)?;
            knowledge_repository::unassign_tag(m.tx(), tag_id, owner_type, owner_id)
        }
    })
}

fn existe_marcacao(
    connection: &rusqlite::Connection,
    tag_id: &str,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM content_tag_assignments
                            WHERE tag_id = ?1 AND owner_type = ?2 AND owner_id = ?3)",
            [tag_id, owner_type, owner_id],
            |row| row.get(0),
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

/// Exclui a tag e tudo o que o schema faz junto.
///
/// ```text
/// Reescrito(planning_item…)   card que citava a tag num campo de relação  → ANTES
/// Excluido(tag_assignment…)   cada marcação da tag
/// Excluido(content_tag)
/// ```
///
/// O card tem linha própria e sobrevive, então a reescrita dele sai antes da exclusão (B4): quem
/// recebe conhece a concorrência antes do SQL destrutivo. Card editado do outro lado bloqueia a
/// exclusão inteira, em vez de perder a ligação sem decisão.
pub fn delete_tag(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        m.excluir("content_tag", id).map_err(|erro| {
            if erro.kind == crate::database::error::DatabaseErrorKind::NotFound {
                DatabaseCommandError::not_found("A tag não existe mais.")
            } else {
                erro
            }
        })?;
        if !knowledge_repository::delete_tag(m.tx(), id)? {
            return Err(DatabaseCommandError::not_found("A tag não existe mais."));
        }
        Ok(())
    })
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
/// **Fora da `Mutacao` de propósito, e não por falta de etapa.** Menção é derivada do texto do
/// capítulo: cada aparelho a recalcula ao aplicar o capítulo, e sincronizá-la seria mandar duas
/// vezes o mesmo dado — a segunda vez com chance de discordar da primeira.
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

    /// Identidade de sincronização para os testes que emitem evento. O diretório volta no par
    /// porque soltá-lo apagaria o arquivo da chave debaixo da identidade.
    fn identidade_de_teste(
        fixture: &TemporaryDatabase,
    ) -> (std::path::PathBuf, crate::domain::identity::DeviceIdentity) {
        let raiz = std::env::temp_dir().join(format!(
            "narrahub-knowledge-id-{}",
            crate::domain::ids::new_id()
        ));
        std::fs::create_dir_all(&raiz).expect("criar raiz");
        let identidade = crate::application::sync_bootstrap::prepare(&raiz, &fixture.database)
            .expect("arranque");
        (raiz, identidade)
    }

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
        let error = set_tag(
            &fixture.database,
            &identidade_de_teste(&fixture).1,
            "inventado",
            "x",
            "t1",
            true,
        )
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
        let identidade = identidade_de_teste(&fixture);
        let tag = create_tag(
            &fixture.database,
            &identidade.1,
            "u1",
            "Reescrever",
            "#7d3650",
        )
        .expect("criar tag");

        set_tag(
            &fixture.database,
            &identidade.1,
            "chapter",
            "c1",
            &tag.id,
            false,
        )
        .expect("desmarcar");
    }
}
