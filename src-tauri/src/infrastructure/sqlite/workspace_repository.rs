use crate::database::error::DatabaseCommandResult;
use crate::domain::workspace::{HistoryEntry, RelationCard, TimelineEvent};
use rusqlite::Connection;

use super::connection::map_sqlite_error;

/// Quantas linhas do histórico o painel mostra. Era um `LIMIT 100` solto no
/// SQL do frontend; virou constante nomeada para não ser confundido com
/// paginação — não há paginação, é um teto de exibição.
const HISTORY_LIMIT: i64 = 100;

pub fn list_timeline(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<TimelineEvent>> {
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, title, description, event_type, start_date, end_date,
                    entity_id, COALESCE(display_date, '') AS display_date,
                    COALESCE(sort_key, 0.0) AS sort_key, created_at, updated_at
               FROM timeline_events
              WHERE universe_id = ?1
              ORDER BY COALESCE(sort_key, 0.0), start_date, created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(TimelineEvent {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                title: row.get("title")?,
                description: row.get("description")?,
                event_type: row.get("event_type")?,
                start_date: row.get("start_date")?,
                end_date: row.get("end_date")?,
                entity_id: row.get("entity_id")?,
                display_date: row.get("display_date")?,
                sort_key: row.get("sort_key")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn list_relations(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<RelationCard>> {
    let mut statement = connection
        .prepare(
            "SELECT r.id, r.universe_id, r.source_id, r.target_id, r.type, r.label,
                    r.bidirectional, r.importance, r.created_at,
                    source.name AS source_name, source.type AS source_type,
                    target.name AS target_name, target.type AS target_type
               FROM relations r
               JOIN entities source ON source.id = r.source_id
               JOIN entities target ON target.id = r.target_id
              WHERE r.universe_id = ?1
              ORDER BY r.created_at DESC",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(RelationCard {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                source_id: row.get("source_id")?,
                target_id: row.get("target_id")?,
                relation_type: row.get("type")?,
                label: row.get("label")?,
                bidirectional: row.get::<_, i64>("bidirectional")? != 0,
                importance: row.get("importance")?,
                created_at: row.get("created_at")?,
                source_name: row.get("source_name")?,
                source_type: row.get("source_type")?,
                target_name: row.get("target_name")?,
                target_type: row.get("target_type")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn list_history(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<HistoryEntry>> {
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.universe_id, c.entity_type, c.entity_id, c.action, c.field,
                    c.old_value, c.new_value, c.created_at,
                    COALESCE(u.name, s.name, b.name, ch.title, e.name, c.entity_id) AS display_name
               FROM change_log c
               LEFT JOIN universes u ON c.entity_type = 'universe' AND u.id = c.entity_id
               LEFT JOIN stories s ON c.entity_type = 'story' AND s.id = c.entity_id
               LEFT JOIN books b ON c.entity_type = 'book' AND b.id = c.entity_id
               LEFT JOIN chapters ch ON c.entity_type = 'chapter' AND ch.id = c.entity_id
               LEFT JOIN entities e ON c.entity_type = 'entity' AND e.id = c.entity_id
              WHERE c.universe_id = ?1
              ORDER BY c.created_at DESC
              LIMIT ?2",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map(rusqlite::params![universe_id, HISTORY_LIMIT], |row| {
            Ok(HistoryEntry {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                entity_type: row.get("entity_type")?,
                entity_id: row.get("entity_id")?,
                action: row.get("action")?,
                field: row.get("field")?,
                old_value: row.get("old_value")?,
                new_value: row.get("new_value")?,
                created_at: row.get("created_at")?,
                display_name: row.get("display_name")?,
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

    #[test]
    fn timeline_ordena_por_sort_key_e_depois_por_data() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO timeline_events (id, universe_id, title, description, event_type,
                                              start_date, end_date, sort_key, created_at, updated_at)
                   VALUES ('t3', 'u1', 'Terceiro', '', 'MARCO', '0100', '', 5, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('t1', 'u1', 'Primeiro', '', 'MARCO', '0050', '', 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('t2', 'u1', 'Segundo', '', 'MARCO', '0900', '', 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear timeline");

        let events = list_timeline(&connection, "u1").expect("listar timeline");
        assert_eq!(
            events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["t1", "t2", "t3"]
        );
    }

    #[test]
    fn timeline_de_outro_universo_nao_vaza() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_universe(&connection, "u2");
        connection
            .execute_batch(
                "INSERT INTO timeline_events (id, universe_id, title, description, event_type,
                                              start_date, end_date, created_at, updated_at)
                   VALUES ('t1', 'u2', 'De outro', '', 'MARCO', '0001', '', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear");

        assert!(list_timeline(&connection, "u1").expect("listar").is_empty());
    }

    #[test]
    fn relacao_traz_nome_das_duas_pontas() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('e2', 'u1', 'Personagem', 'Sam', '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO relations (id, universe_id, source_id, target_id, type, label,
                                        bidirectional, importance, created_at)
                   VALUES ('r1', 'u1', 'e1', 'e2', 'custom', 'amigo de', 1, 'normal', '2026-01-01 00:00:00');",
            )
            .expect("semear relacao");

        let relations = list_relations(&connection, "u1").expect("listar relacoes");
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].source_name, "Frodo");
        assert_eq!(relations[0].target_name, "Sam");
        assert!(relations[0].bidirectional, "1 no SQLite precisa virar true");
    }

    #[test]
    fn historico_cai_para_o_id_quando_o_alvo_foi_excluido() {
        // O change_log sobrevive a exclusao do que descreve. Sem o COALESCE
        // final, a tela mostraria linha em branco justamente no evento de
        // exclusao, que e o que a pessoa foi procurar no historico.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field,
                                         old_value, new_value, created_at)
                   VALUES ('h1', 'u1', 'entity', 'ja-excluida', 'delete', 'name', 'Boromir', '', '2026-01-01 00:00:00');",
            )
            .expect("semear historico");

        let history = list_history(&connection, "u1").expect("listar historico");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].display_name, "ja-excluida");
    }

    #[test]
    fn historico_resolve_o_nome_atual_da_entidade() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00');
                 INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field,
                                         old_value, new_value, created_at)
                   VALUES ('h1', 'u1', 'entity', 'e1', 'update', 'name', 'Frodo B.', 'Frodo', '2026-01-01 00:00:00');",
            )
            .expect("semear");

        let history = list_history(&connection, "u1").expect("listar");
        assert_eq!(history[0].display_name, "Frodo");
    }
}
