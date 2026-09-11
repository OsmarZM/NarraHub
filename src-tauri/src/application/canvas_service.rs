use crate::application::blob_fields;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::{
    is_known_attachment_owner, is_known_endpoint_kind, is_known_node_kind, Attachment, CanvasEdge,
    CanvasEndpoint, CanvasEntityPosition, CanvasNode, CanvasNodePatch,
};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::{new_id, now_timestamp};
use crate::domain::sync::{AggregateRef, Operation};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::sync_repository::{append_event_in_transaction, LocalChange};
use crate::infrastructure::sqlite::{canvas_repository, SqliteDatabase};

pub fn list_nodes(
    database: &SqliteDatabase,
    store: &BlobStore,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasNode>> {
    let connection = database.read()?;
    let mut nodes = canvas_repository::list_nodes(&connection, universe_id)?;
    for node in nodes.iter_mut() {
        node.image = blob_fields::ler_asset_direto(&connection, store, "canvas_nodes", &node.id)?;
    }
    Ok(nodes)
}

// Oito parametros, um a mais que o limite do clippy, e o que passou do limite
// foi o `store`. Agrupar num struct seria um refactor do contrato do comando
// no meio do fechamento da etapa 13 -- registrado como divida (NH-070).
#[allow(clippy::too_many_arguments)]
pub fn create_node(
    database: &SqliteDatabase,
    store: &BlobStore,
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
    let mut connection = database.write()?;
    let tx = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    canvas_repository::insert_node(&tx, &node)?;
    blob_fields::gravar_asset_direto(&tx, store, "canvas_nodes", &node.id, image)?;
    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(node)
}

pub fn update_node(
    database: &SqliteDatabase,
    store: &BlobStore,
    id: &str,
    patch: CanvasNodePatch,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    let mut conexao = database.write()?;
    let connection = conexao
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if !canvas_repository::update_node(&connection, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found(
            "O elemento não existe mais no canvas.",
        ));
    }
    if let Some(imagem) = patch.image.as_deref() {
        blob_fields::gravar_asset_direto(&connection, store, "canvas_nodes", id, imagem)?;
    }
    connection
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
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
    store: &BlobStore,
    universe_id: &str,
    owner_type: &str,
    owner_id: &str,
) -> DatabaseCommandResult<Vec<Attachment>> {
    ensure_attachment_owner(owner_type)?;
    let connection = database.read()?;
    let mut anexos =
        canvas_repository::list_attachments(&connection, universe_id, owner_type, owner_id)?;
    for anexo in anexos.iter_mut() {
        anexo.data_url =
            blob_fields::ler_asset_direto(&connection, store, "attachments", &anexo.id)?;
    }
    Ok(anexos)
}

/// Cria o anexo e **emite o evento**, na mesma transação.
///
/// ADR 0010, fatia 7. O payload é metadado mais `blob_hash` — nunca bytes:
///
/// ```text
/// { id, universe_id, owner_type, owner_id, blob_hash, mime_type, caption, … }
///                                          └─ 64 hex, e o arquivo viaja fora
/// ```
///
/// O `data_url` vai **vazio** no payload. Ele é transporte na leitura local, e
/// deixá-lo entrar no evento devolveria a base64 ao log assinado — que é
/// append-only e não se reescreve.
///
/// A transação é uma só para o dado e o evento, como no capítulo: duas
/// transações deixariam o anexo salvo aqui e invisível para os outros
/// aparelhos, sem nada registrando a falta.
#[allow(clippy::too_many_arguments)]
pub fn create_attachment(
    database: &SqliteDatabase,
    store: &BlobStore,
    identidade: &DeviceIdentity,
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
        // O valor recebido é transporte: `gravar_asset_direto` o troca por
        // referência antes do commit.
        data_url: data_url.to_string(),
        blob_hash: String::new(),
        mime_type: String::new(),
        caption: caption.to_string(),
        sort_order: 0,
        created_at: now_timestamp(),
    };
    let mut conexao = database.write()?;
    let tx = conexao
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    // A posição é calculada pelo banco numa subquery do `INSERT`, então ela
    // volta de lá — devolver o zero que montamos aqui mostraria a imagem no
    // começo da galeria até a próxima recarga.
    attachment.sort_order = canvas_repository::insert_attachment(&tx, &attachment)?;
    blob_fields::gravar_asset_direto(&tx, store, "attachments", &attachment.id, data_url)?;

    // O payload é lido do banco depois da escrita, dentro da transação: é
    // assim que ele carrega a referência em vez do que a tela mandou.
    let gravado = canvas_repository::get_attachment(&tx, &attachment.id)?
        .ok_or_else(|| DatabaseCommandError::storage("O anexo não foi encontrado após gravar."))?;
    emitir_evento_de_anexo(&tx, identidade, &gravado, Operation::Upsert)?;

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    // O que volta para a tela leva a referência; a `data:` URL de exibição é
    // montada na leitura seguinte.
    attachment.blob_hash = gravado.blob_hash;
    attachment.mime_type = gravado.mime_type;
    attachment.data_url = String::new();
    Ok(attachment)
}

/// O envelope do anexo, com o payload sem bytes.
fn emitir_evento_de_anexo(
    tx: &rusqlite::Transaction<'_>,
    identidade: &DeviceIdentity,
    anexo: &Attachment,
    operacao: Operation,
) -> DatabaseCommandResult<()> {
    // **Delete não carrega payload**, e é o schema que cobra:
    //
    //   RAISE(ABORT, 'Evento delete nao carrega payload.')
    //    WHERE NEW.operation = 'delete' AND NEW.payload <> ''
    //
    // A regra existe porque um delete com payload sugeriria que há o que
    // restaurar. Não há: o tombstone é a informação inteira. Eu serializava o
    // anexo nas duas operações, e o gate reprovou.
    //
    // No upsert, a cópia tem `data_url` vazio de propósito e não por acidente:
    // o campo existe no struct para transporte de leitura, e o evento é o
    // lugar em que ele não pode aparecer.
    let payload = if operacao == Operation::Delete {
        String::new()
    } else {
        serde_json::to_string(&Attachment {
            data_url: String::new(),
            ..anexo.clone()
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
    };

    append_event_in_transaction(
        tx,
        identidade,
        &LocalChange {
            universe_id: &anexo.universe_id,
            aggregate: AggregateRef::new("attachment", &anexo.id),
            operation: operacao,
            payload: &payload,
        },
    )?;
    Ok(())
}

/// Remove o anexo e emite o tombstone causal.
///
/// O blob **não** é apagado: não há GC nesta etapa (ADR 0010, item 11), e blob
/// órfão é aceitável. Apagar aqui seria pior do que aceitável — a mesma imagem
/// pode estar referenciada por outro anexo, por uma capa, ou por um capítulo,
/// e a deduplicação é o objetivo declarado da etapa.
pub fn delete_attachment(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
) -> DatabaseCommandResult<()> {
    let mut connection = database.write()?;
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let Some(anexo) = canvas_repository::get_attachment(&tx, id)? else {
        return Err(DatabaseCommandError::not_found("O anexo não existe mais."));
    };
    if !canvas_repository::delete_attachment(&tx, id)? {
        return Err(DatabaseCommandError::not_found("O anexo não existe mais."));
    }
    emitir_evento_de_anexo(&tx, identidade, &anexo, Operation::Delete)?;

    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
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

    /// Identidade de sincronizacao para os testes que emitem evento.
    ///
    /// O diretorio volta no par porque solta-lo apagaria o arquivo da chave
    /// debaixo da identidade.
    fn identidade_de_teste(
        fixture: &TemporaryDatabase,
    ) -> (std::path::PathBuf, crate::domain::identity::DeviceIdentity) {
        let raiz = std::env::temp_dir().join(format!(
            "narrahub-canvas-id-{}",
            crate::domain::ids::new_id()
        ));
        std::fs::create_dir_all(&raiz).expect("criar raiz");
        let identidade = crate::application::sync_bootstrap::prepare(&raiz, &fixture.database)
            .expect("arranque");
        (raiz, identidade)
    }

    /// Um blob store temporario para os testes.
    ///
    /// A raiz vive em `PathBuf` dentro do par porque soltar o diretorio
    /// apagaria o chao debaixo do store no meio do teste.
    fn loja_de_teste() -> (
        std::path::PathBuf,
        crate::infrastructure::blob_store::BlobStore,
    ) {
        let raiz = std::env::temp_dir().join(format!(
            "narrahub-svc-blob-{}",
            crate::domain::ids::new_id()
        ));
        std::fs::create_dir_all(&raiz).expect("criar raiz");
        let store = crate::infrastructure::blob_store::BlobStore::new(&raiz);
        (raiz, store)
    }
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
        let node = create_node(
            &fixture.database,
            &loja_de_teste().1,
            "u1",
            "note",
            "x",
            "",
            0.0,
            0.0,
        )
        .expect("criar");

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
        let node = create_node(
            &fixture.database,
            &loja_de_teste().1,
            "u1",
            "note",
            "x",
            "",
            0.0,
            0.0,
        )
        .expect("criar");

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

        let error = create_node(
            &fixture.database,
            &loja_de_teste().1,
            "u1",
            "desenho",
            "x",
            "",
            0.0,
            0.0,
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
        assert!(error.message.contains("desenho"));
    }

    #[test]
    fn excluir_elemento_leva_as_ligacoes_e_nao_sobra_orfa() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);
        let node = create_node(
            &fixture.database,
            &loja_de_teste().1,
            "u1",
            "note",
            "x",
            "",
            0.0,
            0.0,
        )
        .expect("criar");
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

        let primeiro = create_attachment(
            &fixture.database,
            &loja_de_teste().1,
            &identidade_de_teste(&fixture).1,
            "u1",
            "entity",
            "e1",
            "data:image/png;base64,YQ==",
            "",
        )
        .expect("criar");
        let segundo = create_attachment(
            &fixture.database,
            &loja_de_teste().1,
            &identidade_de_teste(&fixture).1,
            "u1",
            "entity",
            "e1",
            "data:image/png;base64,Yg==",
            "",
        )
        .expect("criar");

        assert_eq!(primeiro.sort_order, 0);
        assert_eq!(segundo.sort_order, 1);
    }

    #[test]
    fn dono_de_anexo_desconhecido_e_recusado() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);

        let error = create_attachment(
            &fixture.database,
            &loja_de_teste().1,
            &identidade_de_teste(&fixture).1,
            "u1",
            "planning",
            "p1",
            "data:image/png;base64,YQ==",
            "",
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    #[test]
    fn anexo_vazio_e_recusado() {
        let fixture = TemporaryDatabase::new();
        seed(&fixture);

        let error = create_attachment(
            &fixture.database,
            &loja_de_teste().1,
            &identidade_de_teste(&fixture).1,
            "u1",
            "entity",
            "e1",
            "   ",
            "",
        )
        .expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Anexo no contrato novo (ADR 0010, fatia 7)
    // ═══════════════════════════════════════════════════════════════════════

    /// Uma `data:` URL de verdade, com os bytes dela.
    fn inline_de_teste(conteudo: &str) -> (String, Vec<u8>) {
        let bytes = conteudo.as_bytes().to_vec();
        let texto = crate::domain::data_url::codificar_base64(&bytes);
        (format!("data:image/png;base64,{texto}"), bytes)
    }

    /// **O anexo guarda referência, e o evento não carrega bytes.**
    ///
    /// As duas metades da fatia 7 no mesmo gate, porque elas só valem juntas:
    /// guardar o hash e mandar a base64 no evento devolveria o problema pelo
    /// caminho mais caro — o log é assinado e append-only, então o que entra
    /// ali viaja para todos os aparelhos, para sempre.
    #[test]
    fn o_anexo_guarda_referencia_e_o_evento_nao_carrega_bytes() {
        let fixture = TemporaryDatabase::new();
        let loja = loja_de_teste();
        let identidade = identidade_de_teste(&fixture);
        seed(&fixture);
        let (url, bytes) = inline_de_teste("os-bytes-do-anexo");

        let anexo = create_attachment(
            &fixture.database,
            &loja.1,
            &identidade.1,
            "u1",
            "entity",
            "e1",
            &url,
            "legenda",
        )
        .expect("criar anexo");

        let esperado = crate::infrastructure::blob_store::hash_dos_bytes(&bytes);
        assert_eq!(anexo.blob_hash, esperado);
        assert_eq!(anexo.mime_type, "image/png");
        assert_eq!(loja.1.read(&esperado).expect("ler o blob"), bytes);

        let conexao = fixture.connection();

        // No banco: referência, e a coluna legada vazia.
        let (guardado, hash, mime): (String, String, String) = conexao
            .query_row(
                "SELECT data_url, blob_hash, mime_type FROM attachments WHERE id = ?1",
                [&anexo.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("ler o anexo");
        assert_eq!(guardado, "", "a coluna legada tinha que ficar vazia");
        assert_eq!(hash, esperado);
        assert_eq!(mime, "image/png");

        // No evento: referência, e nada de bytes.
        let (tipo, payload): (String, String) = conexao
            .query_row(
                "SELECT aggregate_type, payload FROM sync_events ORDER BY seq DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler o evento");
        assert_eq!(tipo, "attachment");
        assert!(
            payload.contains(&esperado),
            "faltou a referência: {payload}"
        );
        for proibido in ["data:image", "base64,"] {
            assert!(
                !payload.contains(proibido),
                "o payload do evento carrega {proibido}: {payload}"
            );
        }
    }

    /// Remover o anexo emite tombstone, e **não** apaga o blob.
    ///
    /// Não há GC nesta etapa (item 11 do contrato), e apagar seria pior que
    /// aceitar órfão: a mesma imagem pode estar referenciada por outro anexo,
    /// por uma capa ou por um capítulo — a deduplicação é o objetivo
    /// declarado.
    #[test]
    fn remover_anexo_emite_tombstone_e_preserva_o_blob() {
        let fixture = TemporaryDatabase::new();
        let loja = loja_de_teste();
        let identidade = identidade_de_teste(&fixture);
        seed(&fixture);
        let (url, bytes) = inline_de_teste("imagem-compartilhada");

        let anexo = create_attachment(
            &fixture.database,
            &loja.1,
            &identidade.1,
            "u1",
            "entity",
            "e1",
            &url,
            "",
        )
        .expect("criar");
        let hash = crate::infrastructure::blob_store::hash_dos_bytes(&bytes);

        delete_attachment(&fixture.database, &identidade.1, &anexo.id).expect("remover");

        let conexao = fixture.connection();
        let sobrou: i64 = conexao
            .query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(sobrou, 0);

        let operacao: String = conexao
            .query_row(
                "SELECT operation FROM sync_events ORDER BY seq DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("ler o evento");
        assert_eq!(operacao, "delete");

        assert!(
            loja.1.verify(&hash).expect("integridade"),
            "o blob não podia ser apagado: não há GC nesta etapa, e a mesma imagem pode              estar referenciada em outro lugar"
        );
    }

    /// Anexo vazio continua recusado, e nada é emitido.
    #[test]
    fn anexo_vazio_nao_emite_evento() {
        let fixture = TemporaryDatabase::new();
        let loja = loja_de_teste();
        let identidade = identidade_de_teste(&fixture);
        seed(&fixture);

        create_attachment(
            &fixture.database,
            &loja.1,
            &identidade.1,
            "u1",
            "entity",
            "e1",
            "   ",
            "",
        )
        .expect_err("anexo vazio");

        let eventos: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(eventos, 0);
    }
}
