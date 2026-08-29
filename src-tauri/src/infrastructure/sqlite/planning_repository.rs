use crate::database::error::DatabaseCommandResult;
use crate::domain::planning::{PlanningCardPlacement, PlanningFieldDefinition, PlanningItem};
use rusqlite::{Connection, Transaction};
use std::collections::BTreeMap;

use super::connection::map_sqlite_error;

pub fn list(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<PlanningItem>> {
    let mut statement = connection
        .prepare(
            "SELECT p.id, p.universe_id, p.chapter_id, p.title, p.description, p.image,
                    p.custom_field_values, p.status, p.target_words, p.sort_order,
                    p.created_at, p.updated_at,
                    c.title AS chapter_title, b.name AS book_name, s.name AS story_name
               FROM planning_items p
               LEFT JOIN chapters c ON c.id = p.chapter_id
               LEFT JOIN books b ON b.id = c.book_id
               LEFT JOIN stories s ON s.id = b.story_id
              WHERE p.universe_id = ?1
              ORDER BY CASE p.status
                         WHEN 'IDEIAS' THEN 0
                         WHEN 'PLANEJADO' THEN 1
                         WHEN 'ESCREVENDO' THEN 2
                         WHEN 'REVISAO' THEN 3
                         ELSE 4
                       END,
                       p.sort_order, p.created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(PlanningItem {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                chapter_id: row.get("chapter_id")?,
                title: row.get("title")?,
                description: row.get("description")?,
                image: row.get("image")?,
                custom_field_values: row.get("custom_field_values")?,
                status: row.get("status")?,
                target_words: row.get("target_words")?,
                sort_order: row.get("sort_order")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                chapter_title: row.get("chapter_title")?,
                book_name: row.get("book_name")?,
                story_name: row.get("story_name")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// O card nasce no fim da coluna IDEIAS. O `sort_order` sai de uma subquery em
/// vez de um `SELECT MAX` separado para que dois cards criados ao mesmo tempo
/// não recebam a mesma posição.
/// Dados de um card recém-criado. Agrupados num struct porque a lista solta
/// de parâmetros já passava de oito — e sete strings em sequência é convite
/// para trocar duas de lugar sem o compilador reclamar.
pub struct NewPlanningCard<'a> {
    pub id: &'a str,
    pub universe_id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub chapter_id: Option<&'a str>,
    pub image: &'a str,
    pub timestamp: &'a str,
}

pub fn insert_card(
    connection: &Connection,
    card: &NewPlanningCard<'_>,
) -> DatabaseCommandResult<()> {
    let NewPlanningCard {
        id,
        universe_id,
        title,
        description,
        chapter_id,
        image,
        timestamp,
    } = card;
    connection
        .execute(
            "INSERT INTO planning_items
               (id, universe_id, chapter_id, title, description, image,
                custom_field_values, status, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}', 'IDEIAS',
                     (SELECT COALESCE(MAX(sort_order), -1) + 1
                        FROM planning_items
                       WHERE universe_id = ?2 AND status = 'IDEIAS'),
                     ?7, ?7)",
            rusqlite::params![
                id,
                universe_id,
                chapter_id,
                title,
                description,
                image,
                timestamp
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn delete_card(
    connection: &Connection,
    id: &str,
    universe_id: &str,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "DELETE FROM planning_items WHERE id = ?1 AND universe_id = ?2",
            [id, universe_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

/// Reposiciona os cards numa transação e devolve quantos foram atingidos.
///
/// O caminho antigo montava um `CASE` gigante com um placeholder por card e
/// conferia `rowsAffected` no final. Aqui a conferência continua — quem chama
/// compara com o total esperado — mas cada card é um `UPDATE` dentro da mesma
/// transação, o que torna o SQL legível e mantém tudo ou nada.
pub fn save_order(
    transaction: &Transaction<'_>,
    universe_id: &str,
    placements: &[PlanningCardPlacement],
    timestamp: &str,
) -> DatabaseCommandResult<usize> {
    let mut statement = transaction
        .prepare(
            "UPDATE planning_items
                SET status = ?1, sort_order = ?2, updated_at = ?3
              WHERE id = ?4 AND universe_id = ?5",
        )
        .map_err(map_sqlite_error)?;
    let mut affected = 0;
    for placement in placements {
        affected += statement
            .execute(rusqlite::params![
                placement.status,
                placement.sort_order,
                timestamp,
                placement.id,
                universe_id,
            ])
            .map_err(map_sqlite_error)?;
    }
    Ok(affected)
}

/// Valores de campo de relação de um card, agrupados por definição.
///
/// As três colunas de destino são exclusivas entre si por `CHECK` desde a
/// migration 12 — o `COALESCE` é o que transforma isso num id só.
pub fn list_field_links(
    connection: &Connection,
    card_id: &str,
) -> DatabaseCommandResult<BTreeMap<String, Vec<String>>> {
    let mut statement = connection
        .prepare(
            "SELECT field_definition_id, COALESCE(story_id, entity_id, tag_id) AS target_id
               FROM planning_field_links
              WHERE planning_item_id = ?1
              ORDER BY created_at, id",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([card_id], |row| {
            Ok((
                row.get::<_, String>("field_definition_id")?,
                row.get::<_, String>("target_id")?,
            ))
        })
        .map_err(map_sqlite_error)?;

    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (field_id, target_id) = row.map_err(map_sqlite_error)?;
        values.entry(field_id).or_default().push(target_id);
    }
    Ok(values)
}

/// Lista as definições do universo.
///
/// `card_id` filtra o que uma ficha pode mostrar: os campos universais mais os
/// que pertencem àquele card. Sem ele a lista é o catálogo inteiro do universo,
/// que é o que a tela usa para gerenciar as propriedades.
pub fn list_field_definitions(
    connection: &Connection,
    universe_id: &str,
    card_id: Option<&str>,
) -> DatabaseCommandResult<Vec<PlanningFieldDefinition>> {
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, name, field_type, options_json, sort_order,
                    scope, owner_item_id, created_at, updated_at
               FROM planning_field_definitions
              WHERE universe_id = ?1
                AND (?2 IS NULL OR owner_item_id IS NULL OR owner_item_id = ?2)
              ORDER BY sort_order, created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map(rusqlite::params![universe_id, card_id], |row| {
            Ok(PlanningFieldDefinition {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                name: row.get("name")?,
                field_type: row.get("field_type")?,
                options_json: row.get("options_json")?,
                sort_order: row.get("sort_order")?,
                scope: row.get("scope")?,
                owner_item_id: row.get("owner_item_id")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn insert_field_definition(
    connection: &Connection,
    definition: &PlanningFieldDefinition,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO planning_field_definitions
               (id, universe_id, name, field_type, options_json, sort_order,
                scope, owner_item_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     (SELECT COALESCE(MAX(sort_order), -1) + 1
                        FROM planning_field_definitions WHERE universe_id = ?2),
                     ?6, ?7, ?8, ?8)",
            rusqlite::params![
                definition.id,
                definition.universe_id,
                definition.name,
                definition.field_type,
                definition.options_json,
                definition.scope,
                definition.owner_item_id,
                definition.created_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

/// O card existe e é deste universo?
///
/// Usado antes de amarrar um campo a um card: sem essa checagem um id de outro
/// universo criaria uma propriedade invisível, presa a um card que a ficha
/// deste universo nunca abre.
pub fn card_belongs_to_universe(
    connection: &Connection,
    card_id: &str,
    universe_id: &str,
) -> DatabaseCommandResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM planning_items WHERE id = ?1 AND universe_id = ?2)",
            [card_id, universe_id],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)
}

pub fn get_field_definition(
    connection: &Connection,
    id: &str,
    universe_id: &str,
) -> DatabaseCommandResult<Option<PlanningFieldDefinition>> {
    let definitions = list_field_definitions(connection, universe_id, None)?;
    Ok(definitions.into_iter().find(|item| item.id == id))
}

/// Troca o alcance de um campo já existente.
///
/// É a promoção de "só neste card" para "todos os cards" e a volta. Os valores
/// já gravados não são tocados: promover expõe o campo nas outras fichas vazio,
/// e restringir mantém o que o card dono já tinha preenchido.
pub fn set_field_definition_scope(
    connection: &Connection,
    id: &str,
    universe_id: &str,
    scope: &str,
    owner_item_id: Option<&str>,
    timestamp: &str,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "UPDATE planning_field_definitions
                SET scope = ?1, owner_item_id = ?2, updated_at = ?3
              WHERE id = ?4 AND universe_id = ?5",
            rusqlite::params![scope, owner_item_id, timestamp, id, universe_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

/// Apaga as definições que só existiam dentro de um card.
///
/// O `ON DELETE CASCADE` da v15 já cobre isso nas conexões do core, que ligam
/// `foreign_keys`. A limpeza explícita existe porque o pool do
/// `tauri-plugin-sql` não liga a pragma: sem ela, um caminho que ainda passe
/// por lá deixaria um campo órfão preso a um card que não existe mais.
pub fn delete_field_definitions_owned_by(
    connection: &Connection,
    card_id: &str,
) -> DatabaseCommandResult<usize> {
    connection
        .execute(
            "DELETE FROM planning_field_definitions WHERE owner_item_id = ?1",
            [card_id],
        )
        .map_err(map_sqlite_error)
}

pub fn rename_field_definition(
    connection: &Connection,
    id: &str,
    universe_id: &str,
    name: &str,
    timestamp: &str,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "UPDATE planning_field_definitions
                SET name = ?1, updated_at = ?2
              WHERE id = ?3 AND universe_id = ?4",
            rusqlite::params![name, timestamp, id, universe_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn delete_field_definition(
    connection: &Connection,
    id: &str,
    universe_id: &str,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "DELETE FROM planning_field_definitions WHERE id = ?1 AND universe_id = ?2",
            [id, universe_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{migrated_memory_database, seed_universe};
    use super::*;
    use crate::database::error::DatabaseErrorKind;

    /// Card minimo: so o que os testes deste arquivo variam.
    fn card<'a>(id: &'a str, title: &'a str) -> NewPlanningCard<'a> {
        NewPlanningCard {
            id,
            universe_id: "u1",
            title,
            description: "",
            chapter_id: None,
            image: "",
            timestamp: "2026-01-01 00:00:00",
        }
    }

    fn definition(id: &str, universe_id: &str, name: &str) -> PlanningFieldDefinition {
        PlanningFieldDefinition {
            id: id.into(),
            universe_id: universe_id.into(),
            name: name.into(),
            field_type: "text".into(),
            options_json: "[]".into(),
            sort_order: 0,
            scope: "universal".into(),
            owner_item_id: None,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        }
    }

    /// Mesma definicao, mas restrita ao card informado.
    fn card_definition(
        id: &str,
        universe_id: &str,
        name: &str,
        owner: &str,
    ) -> PlanningFieldDefinition {
        PlanningFieldDefinition {
            scope: "card".into(),
            owner_item_id: Some(owner.into()),
            ..definition(id, universe_id, name)
        }
    }

    #[test]
    fn cards_saem_na_ordem_das_colunas_do_quadro() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO planning_items (id, universe_id, title, status, sort_order, created_at, updated_at)
                   VALUES ('c3', 'u1', 'Finalizado', 'FINALIZADO', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('c1', 'u1', 'Ideia', 'IDEIAS', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('c2', 'u1', 'Escrevendo', 'ESCREVENDO', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear cards");

        let items = list(&connection, "u1").expect("listar");
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c1", "c2", "c3"]
        );
    }

    #[test]
    fn card_novo_entra_no_fim_da_coluna_ideias() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        for id in ["c1", "c2"] {
            insert_card(&connection, &card(id, id)).expect("inserir");
        }

        let items = list(&connection, "u1").expect("listar");
        assert_eq!(items[0].sort_order, 0);
        assert_eq!(items[1].sort_order, 1);
        assert_eq!(items[1].custom_field_values, "{}");
    }

    #[test]
    fn card_sem_capitulo_nao_inventa_nome_de_capitulo() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_card(&connection, &card("c1", "Solto")).expect("inserir");

        let items = list(&connection, "u1").expect("listar");
        assert_eq!(items[0].chapter_title, None);
        assert_eq!(items[0].book_name, None);
    }

    #[test]
    fn reordenar_conta_so_os_cards_do_universo_pedido() {
        // A contagem devolvida e o que permite quem chama recusar a operacao
        // quando o quadro mudou por baixo. Card de outro universo nao pode
        // entrar nessa conta e fazer a checagem passar por engano.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_universe(&connection, "u2");
        connection
            .execute_batch(
                "INSERT INTO planning_items (id, universe_id, title, status, sort_order, created_at, updated_at)
                   VALUES ('meu', 'u1', 'Meu', 'IDEIAS', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('alheio', 'u2', 'Alheio', 'IDEIAS', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear");

        let mut connection = connection;
        let transaction = connection.transaction().expect("abrir transacao");
        let placements = vec![
            PlanningCardPlacement {
                id: "meu".into(),
                status: "ESCREVENDO".into(),
                sort_order: 3,
            },
            PlanningCardPlacement {
                id: "alheio".into(),
                status: "ESCREVENDO".into(),
                sort_order: 4,
            },
        ];
        let affected =
            save_order(&transaction, "u1", &placements, "2026-06-01 00:00:00").expect("reordenar");
        transaction.commit().expect("commit");

        assert_eq!(affected, 1, "so o card de u1 pode ter sido movido");
        let items = list(&connection, "u1").expect("listar");
        assert_eq!(items[0].status, "ESCREVENDO");
        assert_eq!(items[0].sort_order, 3);
    }

    #[test]
    fn reordenar_desfaz_tudo_quando_a_transacao_e_revertida() {
        // Exigencia do plano: transacao revertida diante de erro no meio da
        // operacao nao pode deixar metade do quadro movido.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO planning_items (id, universe_id, title, status, sort_order, created_at, updated_at)
                   VALUES ('c1', 'u1', 'Um', 'IDEIAS', 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00'),
                          ('c2', 'u1', 'Dois', 'IDEIAS', 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear");

        let mut connection = connection;
        {
            let transaction = connection.transaction().expect("abrir transacao");
            let placements = vec![
                PlanningCardPlacement {
                    id: "c1".into(),
                    status: "REVISAO".into(),
                    sort_order: 9,
                },
                PlanningCardPlacement {
                    id: "c2".into(),
                    status: "REVISAO".into(),
                    sort_order: 8,
                },
            ];
            save_order(&transaction, "u1", &placements, "2026-06-01 00:00:00").expect("reordenar");
            transaction.rollback().expect("reverter");
        }

        let items = list(&connection, "u1").expect("listar");
        assert!(
            items.iter().all(|item| item.status == "IDEIAS"),
            "o rollback tinha que devolver os dois cards para IDEIAS"
        );
    }

    #[test]
    fn status_invalido_e_recusado_pelo_check_do_schema() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_card(&connection, &card("c1", "Um")).expect("inserir");

        let mut connection = connection;
        let transaction = connection.transaction().expect("abrir transacao");
        let placements = vec![PlanningCardPlacement {
            id: "c1".into(),
            status: "INVENTADO".into(),
            sort_order: 0,
        }];
        let error = save_order(&transaction, "u1", &placements, "2026-06-01 00:00:00")
            .expect_err("o CHECK do schema deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);
    }

    #[test]
    fn definicao_de_campo_recebe_a_proxima_posicao_do_universo() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_field_definition(&connection, &definition("f1", "u1", "Primeiro")).expect("inserir");
        insert_field_definition(&connection, &definition("f2", "u1", "Segundo")).expect("inserir");

        let definitions = list_field_definitions(&connection, "u1", None).expect("listar");
        assert_eq!(definitions[0].sort_order, 0);
        assert_eq!(definitions[1].sort_order, 1);
    }

    #[test]
    fn nome_de_campo_repetido_no_mesmo_universo_e_conflito() {
        // O schema tem UNIQUE(universe_id, name) com COLLATE NOCASE: repetir
        // so mudando a caixa tambem e repetir.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_field_definition(&connection, &definition("f1", "u1", "Arco")).expect("inserir");

        let error = insert_field_definition(&connection, &definition("f2", "u1", "arco"))
            .expect_err("nome repetido deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);
    }

    #[test]
    fn renomear_e_excluir_campo_de_outro_universo_nao_encosta_na_linha() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_universe(&connection, "u2");
        insert_field_definition(&connection, &definition("f1", "u1", "Arco")).expect("inserir");

        assert!(
            !rename_field_definition(&connection, "f1", "u2", "Outro", "2026-06-01 00:00:00")
                .expect("renomear")
        );
        assert!(!delete_field_definition(&connection, "f1", "u2").expect("excluir"));
        assert_eq!(
            list_field_definitions(&connection, "u1", None)
                .expect("listar")
                .len(),
            1
        );
    }

    #[test]
    fn campo_de_card_so_aparece_na_ficha_do_dono() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_card(&connection, &card("p1", "Card um")).expect("inserir card");
        insert_card(&connection, &card("p2", "Card dois")).expect("inserir card");
        insert_field_definition(&connection, &definition("f1", "u1", "Universal"))
            .expect("inserir");
        insert_field_definition(&connection, &card_definition("f2", "u1", "So do p1", "p1"))
            .expect("inserir");

        let catalogo = list_field_definitions(&connection, "u1", None).expect("listar");
        assert_eq!(catalogo.len(), 2, "sem card, a lista e o catalogo do universo");

        let do_dono = list_field_definitions(&connection, "u1", Some("p1")).expect("listar");
        assert_eq!(do_dono.len(), 2);

        let do_outro = list_field_definitions(&connection, "u1", Some("p2")).expect("listar");
        assert_eq!(do_outro.len(), 1);
        assert_eq!(do_outro[0].id, "f1");
    }

    #[test]
    fn alcance_e_dono_precisam_combinar() {
        // A v15 recusa os dois estados que quebrariam a leitura da ficha:
        // campo de card sem dono some de todas, e universal com dono confunde.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_card(&connection, &card("p1", "Card um")).expect("inserir card");

        let orfao = PlanningFieldDefinition {
            scope: "card".into(),
            owner_item_id: None,
            ..definition("f1", "u1", "Sem dono")
        };
        assert!(insert_field_definition(&connection, &orfao).is_err());

        let universal_com_dono = PlanningFieldDefinition {
            scope: "universal".into(),
            owner_item_id: Some("p1".into()),
            ..definition("f2", "u1", "Com dono")
        };
        assert!(insert_field_definition(&connection, &universal_com_dono).is_err());
    }

    #[test]
    fn promover_campo_para_universal_solta_o_dono() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_card(&connection, &card("p1", "Card um")).expect("inserir card");
        insert_card(&connection, &card("p2", "Card dois")).expect("inserir card");
        insert_field_definition(&connection, &card_definition("f1", "u1", "Prioridade", "p1"))
            .expect("inserir");

        assert!(set_field_definition_scope(
            &connection,
            "f1",
            "u1",
            "universal",
            None,
            "2026-06-01 00:00:00"
        )
        .expect("promover"));

        let do_outro = list_field_definitions(&connection, "u1", Some("p2")).expect("listar");
        assert_eq!(do_outro.len(), 1);
        assert_eq!(do_outro[0].scope, "universal");
        assert_eq!(do_outro[0].owner_item_id, None);
    }

    #[test]
    fn excluir_campos_do_card_nao_encosta_nos_universais() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        insert_card(&connection, &card("p1", "Card um")).expect("inserir card");
        insert_field_definition(&connection, &definition("f1", "u1", "Universal"))
            .expect("inserir");
        insert_field_definition(&connection, &card_definition("f2", "u1", "So do p1", "p1"))
            .expect("inserir");

        assert_eq!(
            delete_field_definitions_owned_by(&connection, "p1").expect("limpar"),
            1
        );
        let restantes = list_field_definitions(&connection, "u1", None).expect("listar");
        assert_eq!(restantes.len(), 1);
        assert_eq!(restantes[0].id, "f1");
    }
}
