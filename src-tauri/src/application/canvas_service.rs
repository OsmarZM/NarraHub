use crate::application::blob_fields;
use crate::application::mutacao::Mutacao;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::canvas::{
    is_known_attachment_owner, is_known_endpoint_kind, is_known_node_kind, Attachment, CanvasEdge,
    CanvasEndpoint, CanvasEntityPosition, CanvasNode, CanvasNodePatch,
};
use crate::domain::identity::DeviceIdentity;
use crate::domain::ids::{new_id, now_timestamp};
use crate::infrastructure::blob_store::BlobStore;
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
/// Cria o elemento livre. **Duas revisões, de propósito:** o conteúdo e a posição são agregados
/// diferentes desde a B5, então nascer já é dizer as duas coisas.
#[allow(clippy::too_many_arguments)]
pub fn create_node(
    database: &SqliteDatabase,
    store: &BlobStore,
    identidade: &DeviceIdentity,
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
    Mutacao::executar(database, identidade, |m| {
        canvas_repository::insert_node(m.tx(), &node)?;
        blob_fields::gravar_asset_direto(m.tx(), store, "canvas_nodes", &node.id, image)?;
        m.gravou("canvas_node", &node.id)?;
        m.gravou("canvas_node_position", &node.id)
    })?;
    Ok(node)
}

/// Edita texto, imagem ou cor. **Não move o elemento**: posição é outro agregado.
pub fn update_node(
    database: &SqliteDatabase,
    store: &BlobStore,
    identidade: &DeviceIdentity,
    id: &str,
    patch: CanvasNodePatch,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    Mutacao::executar(database, identidade, |m| {
        if !canvas_repository::update_node(m.tx(), id, &patch, &now_timestamp())? {
            return Err(DatabaseCommandError::not_found(
                "O elemento não existe mais no canvas.",
            ));
        }
        if let Some(imagem) = patch.image.as_deref() {
            blob_fields::gravar_asset_direto(m.tx(), store, "canvas_nodes", id, imagem)?;
        }
        m.gravou("canvas_node", id)
    })
}

/// Exclui o elemento, as ligações dele e a posição — cada um com o seu evento.
///
/// ```text
/// Excluido(canvas_edge…)          `trg_canvas_node_edges_delete` (migration 22)
/// Excluido(canvas_node_position)  mora nas colunas do nó
/// Excluido(canvas_node)
/// ```
///
/// A limpeza das arestas era manual aqui até a B5. Virou gatilho de schema porque o mesmo efeito
/// precisa acontecer quando quem sai é a **entidade** da outra ponta — e isso não passava por
/// função nenhuma deste serviço.
pub fn delete_node(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        m.excluir("canvas_node", id).map_err(|erro| {
            if erro.kind == crate::database::error::DatabaseErrorKind::NotFound {
                DatabaseCommandError::not_found("O elemento não existe mais no canvas.")
            } else {
                erro
            }
        })?;
        if !canvas_repository::delete_node(m.tx(), id)? {
            return Err(DatabaseCommandError::not_found(
                "O elemento não existe mais no canvas.",
            ));
        }
        Ok(())
    })
}

/// Arrastar o elemento é revisão da **posição**, não do conteúdo. Mover num aparelho e escrever
/// no outro não pode virar conflito: não colidiu nada de verdade.
pub fn save_node_position(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        if !canvas_repository::save_node_position(m.tx(), id, x, y, &now_timestamp())? {
            return Err(DatabaseCommandError::not_found(
                "O elemento não existe mais no canvas.",
            ));
        }
        m.gravou("canvas_node_position", id)
    })
}

pub fn list_entity_positions(
    database: &SqliteDatabase,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<CanvasEntityPosition>> {
    let connection = database.read()?;
    canvas_repository::list_entity_positions(&connection, universe_id)
}

/// A posição que o escritor arrastou é autoral e sincroniza (B3). Zoom, pan, seleção e hover não
/// passam por aqui: eles vivem na memória do componente.
pub fn save_entity_position(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    universe_id: &str,
    entity_id: &str,
    x: f64,
    y: f64,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        canvas_repository::save_entity_position(
            m.tx(),
            universe_id,
            entity_id,
            x,
            y,
            &now_timestamp(),
        )?;
        m.gravou("canvas_entity_position", entity_id)
    })
}

/// Desfaz o layout do universo: cada posição persistida é excluída, e cada exclusão é um evento.
pub fn clear_layout(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    universe_id: &str,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        let entidades: Vec<String> = {
            let mut consulta = m
                .tx()
                .prepare(
                    "SELECT entity_id FROM canvas_entity_positions WHERE universe_id = ?1
                      ORDER BY entity_id",
                )
                .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            let linhas = consulta
                .query_map([universe_id], |row| row.get(0))
                .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
            linhas
                .collect::<Result<_, _>>()
                .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        };
        for entidade in &entidades {
            m.excluir("canvas_entity_position", entidade)?;
        }
        canvas_repository::clear_layout(m.tx(), universe_id)
    })
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
    identidade: &DeviceIdentity,
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
    Mutacao::executar(database, identidade, |m| {
        // A checagem das pontas é a MESMA regra que o apply remoto cobra (`sync_codec::canvas`),
        // e ela roda de novo na emissão: nenhum evento local sai estruturalmente inválido.
        for endpoint in [source, target] {
            if !canvas_repository::endpoint_exists(
                m.tx(),
                universe_id,
                &endpoint.kind,
                &endpoint.id,
            )? {
                return Err(DatabaseCommandError::not_found(
                    "Uma das pontas da ligação não existe mais neste universo.",
                ));
            }
        }
        canvas_repository::insert_edge(m.tx(), &edge)?;
        m.gravou("canvas_edge", &edge.id)
    })?;
    Ok(edge)
}

pub fn delete_edge(
    database: &SqliteDatabase,
    identidade: &DeviceIdentity,
    id: &str,
) -> DatabaseCommandResult<()> {
    Mutacao::executar(database, identidade, |m| {
        m.excluir("canvas_edge", id).map_err(|erro| {
            if erro.kind == crate::database::error::DatabaseErrorKind::NotFound {
                DatabaseCommandError::not_found("A ligação não existe mais.")
            } else {
                erro
            }
        })?;
        if !canvas_repository::delete_edge(m.tx(), id)? {
            return Err(DatabaseCommandError::not_found(
                "A ligação não existe mais.",
            ));
        }
        Ok(())
    })
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
    Mutacao::executar(database, identidade, |m| {
        // A posição é calculada pelo banco numa subquery do `INSERT`, então ela volta de lá —
        // devolver o zero que montamos aqui mostraria a imagem no começo da galeria.
        attachment.sort_order = canvas_repository::insert_attachment(m.tx(), &attachment)?;
        blob_fields::gravar_asset_direto(m.tx(), store, "attachments", &attachment.id, data_url)?;
        let gravado =
            canvas_repository::get_attachment(m.tx(), &attachment.id)?.ok_or_else(|| {
                DatabaseCommandError::storage("O anexo não foi encontrado após gravar.")
            })?;
        // O evento é lido pela fronteira do banco, com a referência de blob e sem `data_url`.
        m.gravou("attachment", &attachment.id)?;
        // O que volta para a tela leva a referência; a `data:` URL é montada na leitura seguinte.
        attachment.blob_hash = gravado.blob_hash;
        attachment.mime_type = gravado.mime_type;
        attachment.data_url = String::new();
        Ok(attachment)
    })
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
    Mutacao::executar(database, identidade, |m| {
        if canvas_repository::get_attachment(m.tx(), id)?.is_none() {
            return Err(DatabaseCommandError::not_found("O anexo não existe mais."));
        }
        m.excluir("attachment", id)?;
        if !canvas_repository::delete_attachment(m.tx(), id)? {
            return Err(DatabaseCommandError::not_found("O anexo não existe mais."));
        }
        Ok(())
    })
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
            &identidade_de_teste(&fixture).1,
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
            &identidade_de_teste(&fixture).1,
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
            &identidade_de_teste(&fixture).1,
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
            &identidade_de_teste(&fixture).1,
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
            &identidade_de_teste(&fixture).1,
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
            &identidade_de_teste(&fixture).1,
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
            &identidade_de_teste(&fixture).1,
            "u1",
            &endpoint("canvas", &node.id),
            &endpoint("entity", "e1"),
            "liga",
        )
        .expect("ligar");

        delete_node(
            &fixture.database,
            &identidade_de_teste(&fixture).1,
            &node.id,
        )
        .expect("excluir");

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
            "o blob não podia ser apagado: não há GC nesta etapa, e a mesma imagem pode estar \
             referenciada em outro lugar"
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
