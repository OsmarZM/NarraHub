use crate::database::error::DatabaseCommandResult;
use crate::domain::canvas::{
    Attachment, CanvasEdge, CanvasEntityPosition, CanvasNode, CanvasNodePatch,
};
use rusqlite::{Connection, Transaction};

use super::connection::map_sqlite_error;

// ── Elementos livres ─────────────────────────────────────────────────────

pub fn list_nodes(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasNode>> {
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, kind, text, image, color, position_x, position_y,
                    created_at, updated_at
               FROM canvas_nodes WHERE universe_id = ?1 ORDER BY created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(CanvasNode {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                kind: row.get("kind")?,
                text: row.get("text")?,
                image: row.get("image")?,
                color: row.get("color")?,
                position_x: row.get("position_x")?,
                position_y: row.get("position_y")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn insert_node(connection: &Connection, node: &CanvasNode) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO canvas_nodes
               (id, universe_id, kind, text, image, color, position_x, position_y,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            rusqlite::params![
                node.id,
                node.universe_id,
                node.kind,
                node.text,
                node.image,
                node.color,
                node.position_x,
                node.position_y,
                node.created_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn update_node(
    connection: &Connection,
    id: &str,
    patch: &CanvasNodePatch,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let mut assignments = vec!["updated_at = ?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(updated_at.to_string())];

    for (column, value) in [
        ("text", patch.text.as_ref()),
        ("image", patch.image.as_ref()),
        ("color", patch.color.as_ref()),
    ] {
        if let Some(value) = value {
            values.push(Box::new(value.clone()));
            assignments.push(format!("{column} = ?{}", values.len()));
        }
    }

    values.push(Box::new(id.to_string()));
    let sql = format!(
        "UPDATE canvas_nodes SET {} WHERE id = ?{}",
        assignments.join(", "),
        values.len()
    );
    let parameters: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    let affected = connection
        .execute(&sql, parameters.as_slice())
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

/// Apaga o elemento e as ligações dele — o que a FK faria se as pontas não
/// fossem polimórficas. Recebe a transação porque as duas coisas precisam
/// acontecer juntas: sem isso, uma falha no meio deixaria ligação apontando
/// para elemento que não existe mais.
pub fn delete_node(transaction: &Transaction<'_>, id: &str) -> DatabaseCommandResult<bool> {
    transaction
        .execute(
            "DELETE FROM canvas_edges
              WHERE (source_kind = 'canvas' AND source_id = ?1)
                 OR (target_kind = 'canvas' AND target_id = ?1)",
            [id],
        )
        .map_err(map_sqlite_error)?;
    let affected = transaction
        .execute("DELETE FROM canvas_nodes WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

pub fn save_node_position(
    connection: &Connection,
    id: &str,
    x: f64,
    y: f64,
    updated_at: &str,
) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute(
            "UPDATE canvas_nodes SET position_x = ?1, position_y = ?2, updated_at = ?3
              WHERE id = ?4",
            rusqlite::params![x, y, updated_at, id],
        )
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

// ── Layout das entidades ─────────────────────────────────────────────────

pub fn list_entity_positions(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasEntityPosition>> {
    let mut statement = connection
        .prepare(
            "SELECT entity_id, position_x, position_y
               FROM canvas_entity_positions WHERE universe_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(CanvasEntityPosition {
                entity_id: row.get("entity_id")?,
                position_x: row.get("position_x")?,
                position_y: row.get("position_y")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn save_entity_position(
    connection: &Connection,
    universe_id: &str,
    entity_id: &str,
    x: f64,
    y: f64,
    updated_at: &str,
) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO canvas_entity_positions
               (universe_id, entity_id, position_x, position_y, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(universe_id, entity_id) DO UPDATE SET
               position_x = excluded.position_x,
               position_y = excluded.position_y,
               updated_at = excluded.updated_at",
            rusqlite::params![universe_id, entity_id, x, y, updated_at],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn clear_layout(connection: &Connection, universe_id: &str) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "DELETE FROM canvas_entity_positions WHERE universe_id = ?1",
            [universe_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

// ── Ligações ─────────────────────────────────────────────────────────────

/// Descarta ligação órfã: sem FK nas pontas polimórficas, uma delas pode ter
/// sido excluída por fora do canvas — apagar a entidade, por exemplo.
pub fn list_edges(
    connection: &Connection,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasEdge>> {
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, source_kind, source_id, target_kind, target_id,
                    label, created_at
               FROM canvas_edges
              WHERE universe_id = ?1
                AND ((source_kind = 'entity'
                      AND source_id IN (SELECT id FROM entities WHERE universe_id = ?1))
                  OR (source_kind = 'canvas'
                      AND source_id IN (SELECT id FROM canvas_nodes WHERE universe_id = ?1)))
                AND ((target_kind = 'entity'
                      AND target_id IN (SELECT id FROM entities WHERE universe_id = ?1))
                  OR (target_kind = 'canvas'
                      AND target_id IN (SELECT id FROM canvas_nodes WHERE universe_id = ?1)))
              ORDER BY created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id], |row| {
            Ok(CanvasEdge {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                source_kind: row.get("source_kind")?,
                source_id: row.get("source_id")?,
                target_kind: row.get("target_kind")?,
                target_id: row.get("target_id")?,
                label: row.get("label")?,
                created_at: row.get("created_at")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

pub fn insert_edge(connection: &Connection, edge: &CanvasEdge) -> DatabaseCommandResult<()> {
    connection
        .execute(
            "INSERT INTO canvas_edges
               (id, universe_id, source_kind, source_id, target_kind, target_id, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                edge.id,
                edge.universe_id,
                edge.source_kind,
                edge.source_id,
                edge.target_kind,
                edge.target_id,
                edge.label,
                edge.created_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub fn delete_edge(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM canvas_edges WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

/// A ponta existe? É a checagem que substitui a FK ausente, feita antes de
/// gravar em vez de só na leitura.
pub fn endpoint_exists(
    connection: &Connection,
    universe_id: &str,
    kind: &str,
    id: &str,
) -> DatabaseCommandResult<bool> {
    let table = if kind == "entity" {
        "entities"
    } else {
        "canvas_nodes"
    };
    let found: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE id = ?1 AND universe_id = ?2"),
            [id, universe_id],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)?;
    Ok(found > 0)
}

// ── Anexos ───────────────────────────────────────────────────────────────

pub fn list_attachments(
    connection: &Connection,
    universe_id: &str,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<Attachment>> {
    let mut statement = connection
        .prepare(
            "SELECT id, universe_id, owner_type, owner_id, data_url, caption, sort_order,
                    created_at
               FROM attachments
              WHERE universe_id = ?1 AND owner_type = ?2 AND owner_id = ?3
              ORDER BY sort_order, created_at",
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([universe_id, owner_type, owner_id], |row| {
            Ok(Attachment {
                id: row.get("id")?,
                universe_id: row.get("universe_id")?,
                owner_type: row.get("owner_type")?,
                owner_id: row.get("owner_id")?,
                data_url: row.get("data_url")?,
                caption: row.get("caption")?,
                sort_order: row.get("sort_order")?,
                created_at: row.get("created_at")?,
            })
        })
        .map_err(map_sqlite_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_sqlite_error)
}

/// A posição sai de uma subquery no próprio `INSERT`. O caminho antigo lia
/// `MAX(sort_order)` numa consulta separada — duas imagens enviadas na mesma
/// ação ficavam com a mesma posição na galeria.
pub fn insert_attachment(
    connection: &Connection,
    attachment: &Attachment,
) -> DatabaseCommandResult<i64> {
    connection
        .execute(
            "INSERT INTO attachments
               (id, universe_id, owner_type, owner_id, data_url, caption, sort_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                     (SELECT COALESCE(MAX(sort_order), -1) + 1
                        FROM attachments
                       WHERE universe_id = ?2 AND owner_type = ?3 AND owner_id = ?4),
                     ?7)",
            rusqlite::params![
                attachment.id,
                attachment.universe_id,
                attachment.owner_type,
                attachment.owner_id,
                attachment.data_url,
                attachment.caption,
                attachment.created_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    connection
        .query_row(
            "SELECT sort_order FROM attachments WHERE id = ?1",
            [&attachment.id],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)
}

pub fn delete_attachment(connection: &Connection, id: &str) -> DatabaseCommandResult<bool> {
    let affected = connection
        .execute("DELETE FROM attachments WHERE id = ?1", [id])
        .map_err(map_sqlite_error)?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{migrated_memory_database, seed_universe};
    use super::*;

    fn node(id: &str, universe_id: &str) -> CanvasNode {
        CanvasNode {
            id: id.into(),
            universe_id: universe_id.into(),
            kind: "note".into(),
            text: "anotacao".into(),
            image: String::new(),
            color: String::new(),
            position_x: 10.0,
            position_y: 20.0,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
        }
    }

    fn edge(id: &str, source: (&str, &str), target: (&str, &str)) -> CanvasEdge {
        CanvasEdge {
            id: id.into(),
            universe_id: "u1".into(),
            source_kind: source.0.into(),
            source_id: source.1.into(),
            target_kind: target.0.into(),
            target_id: target.1.into(),
            label: String::new(),
            created_at: "2026-01-01 00:00:00".into(),
        }
    }

    fn seed_entity(connection: &Connection, id: &str) {
        connection
            .execute(
                "INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                 VALUES (?1, 'u1', 'Personagem', ?1, '2026-01-01 00:00:00', '2026-01-01 00:00:00')",
                [id],
            )
            .expect("semear entidade");
    }

    #[test]
    fn posicao_do_elemento_e_fracionaria_como_o_schema() {
        // position_x/y sao REAL. Arredondar para inteiro faria o elemento
        // pular de lugar a cada recarga do canvas.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        let mut livre = node("n1", "u1");
        livre.position_x = 12.5;
        livre.position_y = -7.25;
        insert_node(&connection, &livre).expect("inserir");

        let nodes = list_nodes(&connection, "u1").expect("listar");
        assert_eq!(nodes[0].position_x, 12.5);
        assert_eq!(nodes[0].position_y, -7.25);
    }

    #[test]
    fn tipo_de_elemento_fora_do_check_e_recusado_pelo_schema() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        let mut invalido = node("n1", "u1");
        invalido.kind = "desenho".into();

        let error = insert_node(&connection, &invalido).expect_err("CHECK deveria recusar");
        assert_eq!(
            error.kind,
            crate::database::error::DatabaseErrorKind::Conflict
        );
    }

    #[test]
    fn excluir_elemento_leva_as_ligacoes_dele() {
        // Sem FK nas pontas polimorficas, essa limpeza e manual — e e o que
        // impede ligacao apontando para elemento que nao existe mais.
        let mut connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_entity(&connection, "e1");
        insert_node(&connection, &node("n1", "u1")).expect("inserir");
        insert_edge(&connection, &edge("l1", ("canvas", "n1"), ("entity", "e1"))).expect("ligar");

        {
            let transaction = connection.transaction().expect("transacao");
            assert!(delete_node(&transaction, "n1").expect("excluir"));
            transaction.commit().expect("commit");
        }

        let total: i64 = connection
            .query_row("SELECT COUNT(*) FROM canvas_edges", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(total, 0, "a ligacao tinha que sair junto");
    }

    #[test]
    fn ligacao_com_ponta_excluida_por_fora_nao_e_listada() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_entity(&connection, "e1");
        insert_node(&connection, &node("n1", "u1")).expect("inserir");
        insert_edge(&connection, &edge("l1", ("canvas", "n1"), ("entity", "e1"))).expect("ligar");

        connection
            .execute("DELETE FROM entities WHERE id = 'e1'", [])
            .expect("excluir entidade por fora do canvas");

        assert!(
            list_edges(&connection, "u1").expect("listar").is_empty(),
            "ligacao orfa nao pode aparecer no diagrama"
        );
    }

    #[test]
    fn posicao_de_entidade_e_regravada_em_vez_de_duplicada() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_entity(&connection, "e1");

        save_entity_position(&connection, "u1", "e1", 1.0, 2.0, "2026-01-01 00:00:00")
            .expect("salvar");
        save_entity_position(&connection, "u1", "e1", 9.5, 8.5, "2026-06-01 00:00:00")
            .expect("regravar");

        let positions = list_entity_positions(&connection, "u1").expect("listar");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].position_x, 9.5);
    }

    #[test]
    fn limpar_layout_nao_apaga_os_elementos_livres() {
        // Voltar ao arranjo automatico esquece as posicoes das entidades; os
        // elementos que a pessoa criou nao podem sumir junto.
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_entity(&connection, "e1");
        insert_node(&connection, &node("n1", "u1")).expect("inserir");
        save_entity_position(&connection, "u1", "e1", 1.0, 2.0, "2026-01-01 00:00:00")
            .expect("salvar");

        clear_layout(&connection, "u1").expect("limpar");

        assert!(list_entity_positions(&connection, "u1")
            .expect("listar")
            .is_empty());
        assert_eq!(list_nodes(&connection, "u1").expect("listar").len(), 1);
    }

    #[test]
    fn ponta_de_outro_universo_nao_e_reconhecida_como_existente() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        seed_universe(&connection, "u2");
        seed_entity(&connection, "e1");

        assert!(endpoint_exists(&connection, "u1", "entity", "e1").expect("checar"));
        assert!(!endpoint_exists(&connection, "u2", "entity", "e1").expect("checar"));
        assert!(!endpoint_exists(&connection, "u1", "canvas", "n1").expect("checar"));
    }

    #[test]
    fn anexos_recebem_posicoes_distintas_na_mesma_galeria() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");

        let mut orders = Vec::new();
        for id in ["a1", "a2", "a3"] {
            orders.push(
                insert_attachment(
                    &connection,
                    &Attachment {
                        id: id.into(),
                        universe_id: "u1".into(),
                        owner_type: "entity".into(),
                        owner_id: "e1".into(),
                        data_url: "data:,".into(),
                        caption: String::new(),
                        sort_order: 0,
                        created_at: "2026-01-01 00:00:00".into(),
                    },
                )
                .expect("inserir anexo"),
            );
        }

        assert_eq!(orders, vec![0, 1, 2]);
        let listed = list_attachments(&connection, "u1", "entity", "e1").expect("listar");
        assert_eq!(listed.len(), 3);
    }

    #[test]
    fn galeria_de_um_dono_nao_mostra_anexo_de_outro() {
        let connection = migrated_memory_database();
        seed_universe(&connection, "u1");
        for (id, owner) in [("a1", "e1"), ("a2", "e2")] {
            insert_attachment(
                &connection,
                &Attachment {
                    id: id.into(),
                    universe_id: "u1".into(),
                    owner_type: "entity".into(),
                    owner_id: owner.into(),
                    data_url: "data:,".into(),
                    caption: String::new(),
                    sort_order: 0,
                    created_at: "2026-01-01 00:00:00".into(),
                },
            )
            .expect("inserir");
        }

        let listed = list_attachments(&connection, "u1", "entity", "e1").expect("listar");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "a1");
    }
}
