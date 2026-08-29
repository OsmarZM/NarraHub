use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::{
    is_known_attachment_owner, is_known_endpoint_kind, is_known_node_kind, Attachment, CanvasEdge,
    CanvasEndpoint, CanvasEntityPosition, CanvasNode, CanvasNodePatch,
};
use crate::domain::ids::{new_id, now_timestamp};
use crate::infrastructure::sqlite::{canvas_repository, SqliteDatabase};

pub fn list_nodes(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasNode>> {
    let connection = database.read()?;
    canvas_repository::list_nodes(&connection, universe_id)
}

pub fn create_node(
    database: &SqliteDatabase,
    universe_id: &str,
    kind: &str,
    text: &str,
    image: &str,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<CanvasNode> {
    if !is_known_node_kind(kind) {
        return Err(DatabaseCommandError::validation(format!(
            "Tipo de elemento desconhecido no canvas: {kind}."
        )));
    }
    let timestamp = now_timestamp();
    let node = CanvasNode {
        id: new_id(),
        universe_id: universe_id.to_string(),
        kind: kind.to_string(),
        text: text.to_string(),
        image: image.to_string(),
        color: String::new(),
        position_x: x,
        position_y: y,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    let connection = database.write()?;
    canvas_repository::insert_node(&connection, &node)?;
    Ok(node)
}

pub fn update_node(
    database: &SqliteDatabase,
    id: &str,
    patch: CanvasNodePatch,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    let connection = database.write()?;
    if !canvas_repository::update_node(&connection, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found(
            "O elemento não existe mais no canvas.",
        ));
    }
    Ok(())
}

/// Exclui o elemento e as ligações dele na mesma transação.
///
/// As pontas das ligações são polimórficas, então não há FK para cuidar disso.
/// Sem a transação, uma falha entre os dois `DELETE` deixaria ligação apontando
/// para elemento que não existe mais — e ela sumiria da tela pelo filtro da
/// leitura, mas continuaria no arquivo para sempre.
pub fn delete_node(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if !canvas_repository::delete_node(&transaction, id)? {
        return Err(DatabaseCommandError::not_found(
            "O elemento não existe mais no canvas.",
        ));
    }
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

pub fn save_node_position(
    database: &SqliteDatabase,
    id: &str,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !canvas_repository::save_node_position(&connection, id, x, y, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found(
            "O elemento não existe mais no canvas.",
        ));
    }
    Ok(())
}

pub fn list_entity_positions(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasEntityPosition>> {
    let connection = database.read()?;
    canvas_repository::list_entity_positions(&connection, universe_id)
}

pub fn save_entity_position(
    database: &SqliteDatabase,
    universe_id: &str,
    entity_id: &str,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    canvas_repository::save_entity_position(
        &connection,
        universe_id,
        entity_id,
        x,
        y,
        &now_timestamp(),
    )
}

pub fn clear_layout(database: &SqliteDatabase, universe_id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    canvas_repository::clear_layout(&connection, universe_id)
}

pub fn list_edges(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasEdge>> {
    let connection = database.read()?;
    canvas_repository::list_edges(&connection, universe_id)
}

/// Cria a ligação depois de conferir que as duas pontas existem **neste**
/// universo.
///
/// Não há FK para conferir isso: as pontas são polimórficas. A checagem na
/// gravação é nova — antes a integridade era garantida só na leitura, o que
/// deixava a ligação inválida morar no arquivo para sempre, invisível.
pub fn create_edge(
    database: &SqliteDatabase,
    universe_id: &str,
    source: &CanvasEndpoint,
    target: &CanvasEndpoint,
    label: &str,
) -> DatabaseCommandResult<CanvasEdge> {
    for endpoint in [source, target] {
        if !is_known_endpoint_kind(&endpoint.kind) {
            return Err(DatabaseCommandError::validation(format!(
                "Ponta de ligação desconhecida: {}.",
                endpoint.kind
            )));
        }
    }
    if source.kind == target.kind && source.id == target.id {
        return Err(DatabaseCommandError::validation(
            "Uma ligação precisa de duas pontas diferentes.",
        ));
    }

    let connection = database.write()?;
    for endpoint in [source, target] {
        if !canvas_repository::endpoint_exists(
            &connection,
            universe_id,
            &endpoint.kind,
            &endpoint.id,
        )? {
            return Err(DatabaseCommandError::not_found(
                "Uma das pontas da ligação não existe mais neste universo.",
            ));
        }
    }

    let edge = CanvasEdge {
        id: new_id(),
        universe_id: universe_id.to_string(),
        source_kind: source.kind.clone(),
        source_id: source.id.clone(),
        target_kind: target.kind.clone(),
        target_id: target.id.clone(),
        label: label.trim().to_string(),
        created_at: now_timestamp(),
    };
    canvas_repository::insert_edge(&connection, &edge)?;
    Ok(edge)
}

pub fn delete_edge(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !canvas_repository::delete_edge(&connection, id)? {
        return Err(DatabaseCommandError::not_found(
            "A ligação não existe mais.",
        ));
    }
    Ok(())
}

// ── Anexos ───────────────────────────────────────────────────────────────

pub fn list_attachments(
    database: &SqliteDatabase,
    universe_id: &str,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<Attachment>> {
    ensure_attachment_owner(owner_type)?;
    let connection = database.read()?;
    canvas_repository::list_attachments(&connection, universe_id, owner_type, owner_id)
}

pub fn create_attachment(
    database: &SqliteDatabase,
    universe_id: &str,
    owner_type: &str,
    owner_id: &str,
    data_url: &str,
    caption: &str,
) -> DatabaseCommandResult<Attachment> {
    ensure_attachment_owner(owner_type)?;
    if data_url.trim().is_empty() {
        return Err(DatabaseCommandError::validation("O anexo está vazio."));
    }
    let mut attachment = Attachment {
        id: new_id(),
        universe_id: universe_id.to_string(),
        owner_type: owner_type.to_string(),
        owner_id: owner_id.to_string(),
        data_url: data_url.to_string(),
        caption: caption.to_string(),
        sort_order: 0,
        created_at: now_timestamp(),
    };
    let connection = database.write()?;
    // A posição é calculada pelo banco numa subquery do `INSERT`, então ela
    // volta de lá — devolver o zero que montamos aqui mostraria a imagem no
    // começo da galeria até a próxima recarga.
    attachment.sort_order = canvas_repository::insert_attachment(&connection, &attachment)?;
    Ok(attachment)
}

pub fn delete_attachment(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !canvas_repository::delete_attachment(&connection, id)? {
        return Err(DatabaseCommandError::not_found("O anexo não existe mais."));
    }
    Ok(())
}

fn ensure_attachment_owner(owner_type: &str) -> DatabaseCommandResult<()> {
    if is_known_attachment_owner(owner_type) {
        return Ok(());
    }
    Err(DatabaseCommandError::validation(format!(
        "Tipo de dono desconhecido para anexo: {owner_type}."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    fn endpoint(kind: &str, id: &str) -> CanvasEndpoint {
        CanvasEndpoint {
            kind: kind.into(),
            id: id.into(),
        }
    }

    fn seed(fixture: &TemporaryDatabase) {
        let connection = fixture.connection();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear entidade");
    }

    #[test]
    fn ligacao_para_ponta_inexistente_e_recusada_na_gravacao() {
        // Antes a integridade era so na leitura: a ligacao invalida entrava no
        // arquivo, sumia da tela pelo filtro e ficava la para sempre.
        let fixture = TemporaryDatabase::new();
        seed(&fixture);
        let node = create_node(&fixture.database, "u1", "note", "x", "", 0.0, 0.0).expect("criar");

        let error = create_edge(
            &fixture.database,
            "u1",
            &endpoint("canvas", &node.id),
            &endpoint("entity", "nao-existe"),
            "liga",
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::NotFound);

        assert!(list_edges(&fixture.database, "u1")
            .expect("listar")
            .is_empty());
    }

    #[test]
    fn ligacao_de_um_elemento_com_ele_mesmo_e_recusada() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);
        let node = create_node(&fixture.database, "u1", "note", "x", "", 0.0, 0.0).expect("criar");

        let error = create_edge(
            &fixture.database,
            "u1",
            &endpoint("canvas", &node.id),
            &endpoint("canvas", &node.id),
            "",
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    #[test]
    fn tipo_de_elemento_invalido_e_recusado_antes_de_tocar_no_banco() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);

        let error = create_node(&fixture.database, "u1", "desenho", "x", "", 0.0, 0.0)
            .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
        assert!(error.message.contains("desenho"));
    }

    #[test]
    fn excluir_elemento_leva_as_ligacoes_e_nao_sobra_orfa() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);
        let node = create_node(&fixture.database, "u1", "note", "x", "", 0.0, 0.0).expect("criar");
        create_edge(
            &fixture.database,
            "u1",
            &endpoint("canvas", &node.id),
            &endpoint("entity", "e1"),
            "liga",
        )
        .expect("ligar");

        delete_node(&fixture.database, &node.id).expect("excluir");

        let total: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM canvas_edges", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(total, 0);
    }

    #[test]
    fn anexo_devolve_a_posicao_calculada_pelo_banco() {
        // Devolver o zero montado na memoria faria a imagem nova aparecer no
        // comeco da galeria ate a proxima recarga.
        let fixture = TemporaryDatabase::new();
        seed(&fixture);

        let primeiro = create_attachment(&fixture.database, "u1", "entity", "e1", "data:,a", "")
            .expect("criar");
        let segundo = create_attachment(&fixture.database, "u1", "entity", "e1", "data:,b", "")
            .expect("criar");

        assert_eq!(primeiro.sort_order, 0);
        assert_eq!(segundo.sort_order, 1);
    }

    #[test]
    fn dono_de_anexo_desconhecido_e_recusado() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);

        let error = create_attachment(&fixture.database, "u1", "planning", "p1", "data:,a", "")
            .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    #[test]
    fn anexo_vazio_e_recusado() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);

        let error = create_attachment(&fixture.database, "u1", "entity", "e1", "   ", "")
            .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }
}
