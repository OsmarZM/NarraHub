use crate::database::error::DatabaseCommandResult;
use crate::domain::universe::{Universe, UniverseStats, UniverseUpdate, UniverseWithStats};
use rusqlite::{Connection, Row};
use std::collections::BTreeMap;

use super::connection::map_sqlite_error;

fn universe_from_row(row: &Row<'_>) -> rusqlite::Result<Universe> {
    Ok(Universe {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        cover_image: row.get("cover_image")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn list(connection: &Connection) -> DatabaseCommandResult<Vec<Universe>> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, description, cover_image, created_at, updated_at
             FROM universes ORDER BY updated_at DESC",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], universe_from_row)
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn get(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<Universe>> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, description, cover_image, created_at, updated_at
             FROM universes WHERE id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query_map([id], universe_from_row)
        .map_err(map_sqlite_error)?;
    match rows.next() {
        Some(row) => Ok(Some(row.map_err(map_sqlite_error)?)),
        None => Ok(None),
    }
}

/// As cinco contagens escalares de um universo. Um `?1` só, repetido — quem
/// chama para vários universos troca o `WHERE` por um `GROUP BY` na coluna.
const SCALAR_STATS: &str = "SELECT
       u.id AS universe_id,
       (SELECT COALESCE(SUM(c.word_count), 0)
          FROM chapters c
          JOIN books b ON c.book_id = b.id
          JOIN stories s ON b.story_id = s.id
         WHERE s.universe_id = u.id) AS total_words,
       (SELECT COUNT(*)
          FROM chapters c
          JOIN books b ON c.book_id = b.id
          JOIN stories s ON b.story_id = s.id
         WHERE s.universe_id = u.id) AS total_chapters,
       (SELECT COUNT(*) FROM stories WHERE universe_id = u.id) AS total_stories,
       (SELECT COUNT(*)
          FROM books b
          JOIN stories s ON b.story_id = s.id
         WHERE s.universe_id = u.id) AS total_books,
       (SELECT COUNT(*) FROM entities WHERE universe_id = u.id) AS total_entities
     FROM universes u";

fn scalar_stats_from_row(row: &Row<'_>) -> rusqlite::Result<(String, UniverseStats)> {
    Ok((
        row.get("universe_id")?,
        UniverseStats {
            total_words: row.get("total_words")?,
            total_chapters: row.get("total_chapters")?,
            total_stories: row.get("total_stories")?,
            total_books: row.get("total_books")?,
            total_entities: row.get("total_entities")?,
            entity_counts: Default::default(),
        },
    ))
}

/// Estatísticas de um universo: duas consultas, uma para os escalares e outra
/// para a quebra por tipo de entidade, que é a única agrupada.
pub fn stats(connection: &Connection, universe_id: &str) -> DatabaseCommandResult<UniverseStats> {
    let mut stats = connection
        .query_row(
            &format!("{SCALAR_STATS} WHERE u.id = ?1"),
            [universe_id],
            scalar_stats_from_row,
        )
        .map_err(map_sqlite_error)?
        .1;

    let mut statement = connection
        .prepare(
            "SELECT type, COUNT(*) AS total FROM entities WHERE universe_id = ?1 GROUP BY type",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok((row.get::<_, String>("type")?, row.get::<_, i64>("total")?))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (kind, total) = row.map_err(map_sqlite_error)?;
        stats.entity_counts.insert(kind, total);
    }

    Ok(stats)
}

/// A biblioteca inteira em **três** consultas, independente de quantos
/// universos existam: uma lista os universos, uma traz os escalares de todos e
/// uma traz a quebra por tipo de entidade de todos.
///
/// O caminho antigo fazia seis `SELECT` por universo. A primeira versão desta
/// função continuava chamando `stats()` em laço — duas por universo em vez de
/// seis, melhor mas ainda N+1. Uma revisão apontou que o comentário dizia que
/// o problema tinha morrido quando ele só tinha encolhido; aqui ele morre.
pub fn list_with_stats(connection: &Connection) -> DatabaseCommandResult<Vec<UniverseWithStats>> {
    let universes = list(connection)?;
    if universes.is_empty() {
        return Ok(Vec::new());
    }

    let mut statement = connection.prepare(SCALAR_STATS).map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], scalar_stats_from_row)
        .map_err(map_sqlite_error)?;
    let mut by_universe: BTreeMap<String, UniverseStats> = BTreeMap::new();
    for row in rows {
        let (universe_id, stats) = row.map_err(map_sqlite_error)?;
        by_universe.insert(universe_id, stats);
    }

    let mut statement = connection
        .prepare(
            "SELECT universe_id, type, COUNT(*) AS total FROM entities GROUP BY universe_id, type",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("universe_id")?,
                row.get::<_, String>("type")?,
                row.get::<_, i64>("total")?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in rows {
        let (universe_id, kind, total) = row.map_err(map_sqlite_error)?;
        if let Some(stats) = by_universe.get_mut(&universe_id) {
            stats.entity_counts.insert(kind, total);
        }
    }

    Ok(universes
        .into_iter()
        .map(|universe| {
            let stats = by_universe.remove(&universe.id).unwrap_or_default();
            UniverseWithStats { universe, stats }
        })
        .collect())
}

pub fn insert(connection: &Connection, universe: &Universe) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO universes (id, name, description, cover_image, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                universe.id,
                universe.name,
                universe.description,
                universe.cover_image,
                universe.created_at,
                universe.updated_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

/// `UPDATE` montado só com o que veio: `None` significa "não mexer", que é
/// diferente de "gravar vazio". Sem isso, salvar só o nome apagaria a capa.
pub fn update(
    connection: &Connection,
    id: &str,
    patch: &UniverseUpdate,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut assignments = vec!["updated_at = ?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(updated_at.to_string())];

    for (column, value) in [
        ("name", patch.name.as_ref()),
        ("description", patch.description.as_ref()),
        ("cover_image", patch.cover_image.as_ref()),
    ] {
        if let Some(value) = value {
            values.push(Box::new(value.clone()));
            assignments.push(format!("{column} = ?{}", values.len()));
        }
    }

    values.push(Box::new(id.to_string()));
    let sql = format!(
        "UPDATE universes SET {} WHERE id = ?{}",
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
        .execute("DELETE FROM universes WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{migrated_memory_database, seed_universe};
    use super::*;

    fn seed_content(connection: &Connection) {
        seed_universe(connection, "u1");
        seed_universe(connection, "u2");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name, created_at, updated_at)
                   VALUES ('s1', 'u1', 'Historia', '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO books (id, story_id, name, created_at, updated_at)
                   VALUES ('b1', 's1', 'Livro', '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO chapters (id, book_id, title, content, word_count, created_at, updated_at)
                   VALUES ('c1', 'b1', 'Cap 1', 'texto', 120, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('c2', 'b1', 'Cap 2', 'texto', 80, '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('e2', 'u1', 'Personagem', 'Sam', '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('e3', 'u1', 'Lugar', 'Condado', '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('e4', 'u2', 'Personagem', 'Outro', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear conteudo");
    }

    #[test]
    fn estatisticas_contam_apenas_o_universo_pedido() {
        let connection = migrated_memory_database();
        seed_content(&connection);

        let stats = stats(&connection, "u1").expect("estatisticas");
        assert_eq!(stats.total_words, 200);
        assert_eq!(stats.total_chapters, 2);
        assert_eq!(stats.total_stories, 1);
        assert_eq!(stats.total_books, 1);
        assert_eq!(stats.total_entities, 3, "a entidade de u2 nao pode entrar");
        assert_eq!(stats.entity_counts.get("Personagem"), Some(&2));
        assert_eq!(stats.entity_counts.get("Lugar"), Some(&1));
    }

    #[test]
    fn universo_vazio_zera_em_vez_de_falhar() {
        // Universo recem-criado nao tem capitulo: SUM devolve NULL e a query
        // depende do COALESCE, senao o comando quebra logo depois do onboarding.
        let connection = migrated_memory_database();
        seed_universe(&connection, "novo");

        let stats = stats(&connection, "novo").expect("estatisticas de universo vazio");
        assert_eq!(stats.total_words, 0);
        assert!(stats.entity_counts.is_empty());
    }

    #[test]
    fn lista_ordena_do_mais_recente_para_o_mais_antigo() {
        let connection = migrated_memory_database();
        connection
            .execute_batch(
                "INSERT INTO universes (id, name, description, created_at, updated_at)
                   VALUES ('antigo', 'Antigo', '', '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('novo', 'Novo', '', '2026-01-01 00:00:00', '2026-06-01 00:00:00');",
            )
            .expect("semear");

        let universes = list(&connection).expect("listar");
        assert_eq!(
            universes.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
            vec!["novo", "antigo"]
        );
    }

    #[test]
    fn buscar_id_inexistente_devolve_none_em_vez_de_erro() {
        let connection = migrated_memory_database();
        assert!(get(&connection, "nao-existe").expect("consulta").is_none());
    }

    #[test]
    fn update_parcial_nao_apaga_o_que_nao_veio() {
        // Salvar so o nome nao pode zerar a capa. O UPDATE monta o SET com o
        // que veio justamente por isso.
        let connection = migrated_memory_database();
        connection
            .execute_batch(
                "INSERT INTO universes (id, name, description, cover_image, created_at, updated_at)
                   VALUES ('u1', 'Antigo', 'Descricao', 'capa.png', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear");

        let patch = UniverseUpdate {
            name: Some("Novo".into()),
            ..Default::default()
        };
        assert!(update(&connection, "u1", &patch, "2026-06-01 00:00:00").expect("atualizar"));

        let universe = get(&connection, "u1").expect("buscar").expect("existe");
        assert_eq!(universe.name, "Novo");
        assert_eq!(universe.description, "Descricao");
        assert_eq!(universe.cover_image, "capa.png");
        assert_eq!(universe.updated_at, "2026-06-01 00:00:00");
    }

    #[test]
    fn update_de_id_inexistente_nao_afeta_linha() {
        let connection = migrated_memory_database();
        let patch = UniverseUpdate {
            name: Some("x".into()),
            ..Default::default()
        };
        assert!(
            !update(&connection, "fantasma", &patch, "2026-06-01 00:00:00").expect("atualizar")
        );
    }

    #[test]
    fn excluir_universo_leva_junto_o_que_pendura_nele() {
        // Cascata so funciona com foreign_keys ligada. O tauri-plugin-sql nao
        // liga, entao o caminho antigo deixava historia, livro, capitulo e
        // entidade orfaos no arquivo depois de excluir o universo.
        let connection = migrated_memory_database();
        seed_content(&connection);

        assert!(delete(&connection, "u1").expect("excluir"));

        for (table, expected) in [("stories", 0i64), ("books", 0), ("chapters", 0)] {
            let total: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("contar");
            assert_eq!(
                total, expected,
                "{table} deveria ter sido levada na cascata"
            );
        }
        let entities: i64 = connection
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .expect("contar entidades");
        assert_eq!(entities, 1, "so a entidade do outro universo pode sobrar");
    }

    #[test]
    fn excluir_id_inexistente_nao_afeta_linha() {
        let connection = migrated_memory_database();
        assert!(!delete(&connection, "fantasma").expect("excluir"));
    }

    #[test]
    fn insert_recusa_id_duplicado_como_conflito() {
        let connection = migrated_memory_database();
        let universe = Universe {
            id: "u1".into(),
            name: "Um".into(),
            description: String::new(),
            cover_image: String::new(),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        };
        insert(&connection, &universe).expect("primeiro insert");
        let error = insert(&connection, &universe).expect_err("duplicata deveria falhar");
        assert_eq!(
            error.kind,
            crate::database::error::DatabaseErrorKind::Conflict
        );
    }

    #[test]
    fn listagem_com_estatisticas_bate_com_o_calculo_por_universo() {
        // As duas consultas agrupadas de list_with_stats precisam dar
        // exatamente o mesmo que stats() dá universo a universo. Divergir aqui
        // faria a biblioteca mostrar um numero e a ficha do universo outro.
        let connection = migrated_memory_database();
        seed_content(&connection);

        let listed = list_with_stats(&connection).expect("listar com estatisticas");
        assert_eq!(listed.len(), 2);
        for item in &listed {
            let individual = stats(&connection, &item.universe.id).expect("estatisticas");
            assert_eq!(item.stats, individual, "divergiu em {}", item.universe.id);
        }
    }

    #[test]
    fn universo_sem_entidade_aparece_na_listagem_com_contagem_zerada() {
        // O GROUP BY de entidades nao devolve linha para universo sem
        // entidade. Se a listagem dependesse dele para existir, o universo
        // recem-criado sumiria da biblioteca.
        let connection = migrated_memory_database();
        seed_universe(&connection, "vazio");

        let listed = list_with_stats(&connection).expect("listar");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].stats.total_words, 0);
        assert!(listed[0].stats.entity_counts.is_empty());
    }

    #[test]
    fn contagem_por_tipo_nao_vaza_entre_universos_na_listagem() {
        let connection = migrated_memory_database();
        seed_content(&connection);

        let listed = list_with_stats(&connection).expect("listar");
        let u1 = listed
            .iter()
            .find(|item| item.universe.id == "u1")
            .expect("u1");
        let u2 = listed
            .iter()
            .find(|item| item.universe.id == "u2")
            .expect("u2");
        assert_eq!(u1.stats.entity_counts.get("Personagem"), Some(&2));
        assert_eq!(u2.stats.entity_counts.get("Personagem"), Some(&1));
        assert_eq!(u2.stats.entity_counts.get("Lugar"), None);
    }
}
