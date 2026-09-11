use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::collaboration::{
    attribute_key, writable_column, CollaborationContribution, CollaborationSession,
    IncomingContribution, NewCollaborationSession, MAX_ATTRIBUTE_KEY,
};
use crate::domain::ids::{new_id, now_timestamp};
use crate::infrastructure::blob_document;
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::{collaboration_repository, SqliteDatabase};

pub fn list_sessions(
    database: &SqliteDatabase,
) -> DatabaseCommandResult<Vec<CollaborationSession>> {
    let connection = database.read()?;
    collaboration_repository::list_sessions(&connection)
}

pub fn list_contributions(
    database: &SqliteDatabase,
    session_id: Option<&str>,
) -> DatabaseCommandResult<Vec<CollaborationContribution>> {
    let connection = database.read()?;
    collaboration_repository::list_contributions(&connection, session_id)
}

pub fn save_session(
    database: &SqliteDatabase,
    session: NewCollaborationSession,
) -> DatabaseCommandResult<()> {
    let universe_ids_json = serde_json::to_string(&session.universe_ids)
        .map_err(|error| DatabaseCommandError::validation(error.to_string()))?;
    let connection = database.write()?;
    collaboration_repository::upsert_session(
        &connection,
        &session,
        &universe_ids_json,
        &now_timestamp(),
    )
}

/// Guarda o que chegou de um convidado. Recado entra já como `noted`; edição
/// entra como `pending`, porque ela só toca no universo depois de aprovada.
/// **É documento de capítulo?**
///
/// O recorte do ADR 0010 para as superfícies 9 e 10. Campo de texto comum
/// continua texto: um convidado pode legitimamente propor um resumo que
/// menciona data URLs.
fn e_documento_de_capitulo(target_type: &str, field: &str) -> bool {
    target_type == "chapter" && field == "content"
}

/// PRIMEIRA BARREIRA (ADR 0010): normaliza na fronteira, antes do `INSERT`.
///
/// O valor vem de fora, de um convidado com link, e pode chegar no formato
/// antigo. A regra é a mesma dos dois lados da linha, e eles são
/// independentes:
///
/// ```text
/// inline válida     →  publica no BlobStore, grava referência
/// inline inválida   →  RECUSA. Dado novo não ganha pendência de migração:
///                      pendência é para legado que já estava no acervo.
/// blob inválido     →  RECUSA
/// externa           →  RECUSA. Não põe byte no banco, mas o endereço dela só
///                      existe no aparelho de quem propôs: URL vence, caminho
///                      local não existe no Android, `blob:` morre com a aba.
///                      Uma proposta sobre capítulo que já tem imagem remota
///                      antiga precisa remover ou reinserir essa imagem — é o
///                      *fail-closed* do ADR 0010.
/// ```
///
/// Sem fallback para inline: se a transformação falhar, o erro sobe. Gravar
/// "o que deu" seria a porta que as três barreiras existem para fechar.
fn normalizar_documento(
    store: &BlobStore,
    lado: &str,
    valor: &str,
) -> DatabaseCommandResult<String> {
    if valor.trim().is_empty() {
        return Ok(valor.to_string());
    }
    let conversao = blob_document::converter(valor, store)?;
    if let Err(motivo) = blob_document::exigir_blob_safe(&conversao.html) {
        return Err(DatabaseCommandError::validation(format!(
            "A proposta não pôde ser aceita ({lado}): {motivo}"
        )));
    }
    Ok(conversao.html)
}

pub fn store_contribution(
    database: &SqliteDatabase,
    store: &BlobStore,
    session_id: &str,
    sequence: i64,
    incoming: IncomingContribution,
) -> DatabaseCommandResult<bool> {
    let status = if incoming.kind == "note" {
        "noted"
    } else {
        "pending"
    };
    let contributor = if incoming.contributor.trim().is_empty() {
        "Convidado".to_string()
    } else {
        incoming.contributor.clone()
    };
    let created_at = if incoming.created_at.trim().is_empty() {
        now_timestamp()
    } else {
        incoming.created_at.clone()
    };

    // Os dois lados são normalizados de forma independente, e antes de a
    // linha existir: uma falha aqui não deixa meia contribuição no banco.
    let (original_value, proposed_value) =
        if e_documento_de_capitulo(&incoming.target_type, &incoming.field) {
            (
                normalizar_documento(store, "versão original", &incoming.original_value)?,
                normalizar_documento(store, "proposta", &incoming.proposed_value)?,
            )
        } else {
            (
                incoming.original_value.clone(),
                incoming.proposed_value.clone(),
            )
        };

    let contribution = CollaborationContribution {
        id: incoming.id,
        session_id: session_id.to_string(),
        sequence,
        contributor,
        kind: incoming.kind,
        universe_id: incoming.universe_id,
        target_type: incoming.target_type,
        target_id: incoming.target_id,
        target_label: incoming.target_label,
        field: incoming.field,
        original_value,
        proposed_value,
        message: incoming.message,
        status: status.to_string(),
        created_at,
        reviewed_at: None,
    };

    let connection = database.write()?;
    collaboration_repository::insert_contribution(&connection, &contribution)
}

pub fn end_all_active(database: &SqliteDatabase, status: &str) -> DatabaseCommandResult<()> {
    ensure_end_status(status)?;
    let connection = database.write()?;
    collaboration_repository::end_all_active(&connection, status, &now_timestamp())
}

pub fn end_session(database: &SqliteDatabase, id: &str, status: &str) -> DatabaseCommandResult<()> {
    ensure_end_status(status)?;
    let connection = database.write()?;
    if !collaboration_repository::end_session(&connection, id, status, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Sessão não encontrada."));
    }
    Ok(())
}

/// Aprova ou recusa uma proposta.
///
/// Tudo numa transação: aplicar a mudança, registrar no histórico e marcar a
/// proposta como decidida. O caminho antigo fazia as três coisas em comandos
/// soltos — se a marcação falhasse depois de aplicar, a proposta continuava
/// pendente e podia ser aplicada de novo, sobrescrevendo o que o autor tivesse
/// escrito no meio.
///
/// Proposta que não está mais pendente é silêncio, não erro: dois cliques no
/// mesmo botão não devem virar mensagem de falha.
pub fn review(database: &SqliteDatabase, id: &str, decision: &str) -> DatabaseCommandResult<()> {
    if decision != "approved" && decision != "rejected" {
        return Err(DatabaseCommandError::validation(
            "Decisão inválida para uma proposta.",
        ));
    }

    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let Some(contribution) = collaboration_repository::pending_edit(&transaction, id)? else {
        return Ok(());
    };

    let timestamp = now_timestamp();
    if decision == "approved" {
        // SEGUNDA BARREIRA (ADR 0010). Fail-closed, e não é redundante.
        //
        // A proposta pode ter entrado antes da barreira existir, ter vindo de
        // um banco importado, ou ter sido adulterada na linha. Aprovar é
        // copiar `proposed_value` para `chapters.content`, e dali o gatilho de
        // revisão e o evento assinado seguem sozinhos.
        //
        // O erro desfaz a transação: a contribuição continua `pending`, o
        // capítulo não muda, nenhuma revisão é criada e nenhum evento nasce.
        // Não normaliza aqui de propósito — publicar blob no meio de uma
        // aprovação transformaria "revisar" em "migrar", e o escritor não pediu
        // isso. A proposta fica pendente com a mensagem dizendo por quê.
        if e_documento_de_capitulo(&contribution.target_type, &contribution.field) {
            if let Err(motivo) = blob_document::exigir_blob_safe(&contribution.proposed_value) {
                return Err(DatabaseCommandError::validation(format!(
                    "Esta proposta não pode ser aplicada: {motivo}"
                )));
            }
        }
        apply(&transaction, &contribution, &timestamp)?;
        collaboration_repository::log_applied_change(
            &transaction,
            &new_id(),
            &contribution,
            &timestamp,
        )?;
    }
    collaboration_repository::mark_reviewed(&transaction, id, decision, &timestamp)?;

    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Aprova todas as pendentes da sessão e devolve quantas foram.
///
/// Cada proposta tem a própria transação, de propósito: uma proposta que
/// aponta para um capítulo já excluído não pode derrubar a aprovação das
/// outras. Quem não passou continua pendente e visível na tela.
pub fn approve_all(database: &SqliteDatabase, session_id: &str) -> DatabaseCommandResult<i64> {
    let pending = {
        let connection = database.read()?;
        collaboration_repository::pending_edits_of_session(&connection, session_id)?
    };

    let mut approved = 0;
    for id in pending {
        if review(database, &id, "approved").is_ok() {
            approved += 1;
        }
    }
    Ok(approved)
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    contribution: &CollaborationContribution,
    timestamp: &str,
) -> DatabaseCommandResult<()> {
    if let Some(key) = attribute_key(&contribution.target_type, &contribution.field) {
        if key.is_empty() || key.chars().count() > MAX_ATTRIBUTE_KEY {
            return Err(DatabaseCommandError::validation("Campo de ficha inválido."));
        }
        return collaboration_repository::apply_attribute_change(
            transaction,
            &new_id(),
            &contribution.target_id,
            key,
            &contribution.proposed_value,
            timestamp,
        );
    }

    let Some((table, column)) = writable_column(&contribution.target_type, &contribution.field)
    else {
        return Err(DatabaseCommandError::validation(
            "Alteração colaborativa fora do escopo permitido.",
        ));
    };

    if !collaboration_repository::apply_column_change(
        transaction,
        table,
        column,
        &contribution.proposed_value,
        &contribution.target_id,
        timestamp,
    )? {
        return Err(DatabaseCommandError::not_found(
            "O item da proposta não existe mais.",
        ));
    }
    Ok(())
}

fn ensure_end_status(status: &str) -> DatabaseCommandResult<()> {
    if status == "ended" || status == "revoked" {
        return Ok(());
    }
    Err(DatabaseCommandError::validation(
        "Uma sessão só pode ser encerrada ou revogada.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;
    use crate::infrastructure::blob_store::BlobStore;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    fn seed_session(fixture: &TemporaryDatabase) {
        seed_universe(&fixture.connection(), "u1");
        save_current_session(fixture);
    }

    /// Regrava a sessão sem semear o universo de novo — semear duas vezes
    /// esbarra no PRIMARY KEY de `universes`, não na regra que o teste quer ver.
    fn save_current_session(fixture: &TemporaryDatabase) {
        save_session(
            &fixture.database,
            NewCollaborationSession {
                id: "sess".into(),
                title: "Leitura".into(),
                permission: "edit".into(),
                universe_ids: vec!["u1".into()],
                encryption_key: "chave".into(),
                revoke_token: "token".into(),
                expires_at: "2026-12-31 00:00:00".into(),
            },
        )
        .expect("criar sessao");
    }

    fn incoming(id: &str, field: &str, value: &str, target_id: &str) -> IncomingContribution {
        IncomingContribution {
            id: id.into(),
            contributor: String::new(),
            kind: "edit".into(),
            universe_id: "u1".into(),
            target_type: "universe".into(),
            target_id: target_id.into(),
            target_label: "Universo".into(),
            field: field.into(),
            original_value: "antes".into(),
            proposed_value: value.into(),
            message: String::new(),
            created_at: String::new(),
        }
    }

    /// Um blob store temporario para os testes que atravessam a fronteira.
    ///
    /// `store_contribution` publica blob quando o valor e documento de
    /// capitulo, e publicar precisa de um lugar no disco.
    struct LojaDeTeste {
        raiz: std::path::PathBuf,
        store: BlobStore,
    }

    impl LojaDeTeste {
        fn nova() -> Self {
            let raiz = std::env::temp_dir()
                .join(format!("narrahub-colab-{}", crate::domain::ids::new_id()));
            std::fs::create_dir_all(&raiz).expect("criar raiz");
            Self {
                store: BlobStore::new(&raiz),
                raiz,
            }
        }
    }

    impl Drop for LojaDeTeste {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.raiz).ok();
        }
    }

    /// Uma `data:` URL de verdade, com os bytes dela.
    fn inline_de_teste(conteudo: &str) -> (String, Vec<u8>) {
        let bytes = conteudo.as_bytes().to_vec();
        let texto = crate::domain::data_url::codificar_base64(&bytes);
        (format!("data:image/png;base64,{texto}"), bytes)
    }

    #[test]
    fn contribuicao_sem_nome_entra_como_convidado() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);

        assert!(store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Novo", "u1")
        )
        .expect("guardar"));

        let contributions = list_contributions(&fixture.database, Some("sess")).expect("listar");
        assert_eq!(contributions[0].contributor, "Convidado");
        assert_eq!(contributions[0].status, "pending");
    }

    #[test]
    fn reenvio_da_mesma_sequencia_nao_duplica_nem_falha() {
        // A mesma contribuicao pode chegar duas vezes pela rede. O retorno
        // false e o que diz a tela que nao ha novidade para avisar.
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);

        assert!(store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Novo", "u1")
        )
        .expect("primeira"));
        assert!(!store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Novo", "u1")
        )
        .expect("reenvio"));
        assert_eq!(
            list_contributions(&fixture.database, Some("sess"))
                .expect("listar")
                .len(),
            1
        );
    }

    #[test]
    fn aprovar_aplica_a_mudanca_e_registra_no_historico() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Renomeado", "u1"),
        )
        .expect("guardar");

        review(&fixture.database, "c1", "approved").expect("aprovar");

        let connection = fixture.connection();
        let name: String = connection
            .query_row("SELECT name FROM universes WHERE id = 'u1'", [], |row| {
                row.get(0)
            })
            .expect("ler universo");
        assert_eq!(name, "Renomeado");

        let logged: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM change_log WHERE entity_id = 'u1'",
                [],
                |row| row.get(0),
            )
            .expect("contar historico");
        assert_eq!(logged, 1, "a aprovacao precisa aparecer no historico");
    }

    #[test]
    fn aprovar_duas_vezes_nao_reaplica() {
        // O segundo clique nao pode sobrescrever o que o autor escreveu depois
        // da primeira aprovacao.
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Renomeado", "u1"),
        )
        .expect("guardar");
        review(&fixture.database, "c1", "approved").expect("aprovar");

        fixture
            .connection()
            .execute(
                "UPDATE universes SET name = 'Escrito depois' WHERE id = 'u1'",
                [],
            )
            .expect("autor edita depois");

        review(&fixture.database, "c1", "approved").expect("segundo clique e silencio");

        let name: String = fixture
            .connection()
            .query_row("SELECT name FROM universes WHERE id = 'u1'", [], |row| {
                row.get(0)
            })
            .expect("ler");
        assert_eq!(name, "Escrito depois");
    }

    #[test]
    fn recusar_nao_toca_no_universo_mas_marca_a_proposta() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Renomeado", "u1"),
        )
        .expect("guardar");

        review(&fixture.database, "c1", "rejected").expect("recusar");

        let name: String = fixture
            .connection()
            .query_row("SELECT name FROM universes WHERE id = 'u1'", [], |row| {
                row.get(0)
            })
            .expect("ler");
        assert_eq!(name, "u1", "o nome semeado nao pode ter mudado");

        let contributions = list_contributions(&fixture.database, Some("sess")).expect("listar");
        assert_eq!(contributions[0].status, "rejected");
        assert!(contributions[0].reviewed_at.is_some());
    }

    #[test]
    fn campo_fora_do_escopo_nao_grava_nada_e_a_proposta_segue_pendente() {
        // A transacao e o que garante isso: sem ela, a proposta ficaria
        // marcada como aprovada sem nada ter sido aplicado.
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "cover_image", "x.png", "u1"),
        )
        .expect("guardar");

        let error = review(&fixture.database, "c1", "approved").expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);

        let contributions = list_contributions(&fixture.database, Some("sess")).expect("listar");
        assert_eq!(
            contributions[0].status, "pending",
            "a proposta tem que seguir pendente"
        );
    }

    #[test]
    fn proposta_para_item_ja_excluido_avisa_em_vez_de_marcar_aprovada() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("c1", "name", "Novo", "ja-excluido"),
        )
        .expect("guardar");

        let error = review(&fixture.database, "c1", "approved").expect_err("deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::NotFound);

        let contributions = list_contributions(&fixture.database, Some("sess")).expect("listar");
        assert_eq!(contributions[0].status, "pending");
    }

    #[test]
    fn aprovar_tudo_conta_so_o_que_passou() {
        // Uma proposta invalida no meio nao pode derrubar as outras — cada uma
        // tem a propria transacao.
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            incoming("boa", "name", "Novo nome", "u1"),
        )
        .expect("guardar");
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            2,
            incoming("ruim", "cover_image", "x", "u1"),
        )
        .expect("guardar");
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            3,
            incoming("boa2", "description", "Nova", "u1"),
        )
        .expect("guardar");

        let approved = approve_all(&fixture.database, "sess").expect("aprovar tudo");
        assert_eq!(approved, 2);

        let contributions = list_contributions(&fixture.database, Some("sess")).expect("listar");
        let ruim = contributions.iter().find(|c| c.id == "ruim").expect("ruim");
        assert_eq!(
            ruim.status, "pending",
            "a invalida continua visivel na tela"
        );
    }

    #[test]
    fn atributo_de_ficha_e_criado_quando_ainda_nao_existe() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        fixture
            .connection()
            .execute_batch(
                "INSERT INTO entities (id, universe_id, type, name, created_at, updated_at)
                   VALUES ('e1', 'u1', 'Personagem', 'Frodo', '2026-01-01 00:00:00', '2026-01-01 00:00:00');",
            )
            .expect("semear entidade");

        let mut proposta = incoming("c1", "attribute:Apelido", "Sr. Subaperto", "e1");
        proposta.target_type = "entity".into();
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            proposta,
        )
        .expect("guardar");

        review(&fixture.database, "c1", "approved").expect("aprovar");

        let value: String = fixture
            .connection()
            .query_row(
                "SELECT value FROM entity_attributes WHERE entity_id = 'e1' AND key = 'Apelido'",
                [],
                |row| row.get(0),
            )
            .expect("ler atributo");
        assert_eq!(value, "Sr. Subaperto");
    }

    #[test]
    fn nome_de_atributo_gigante_e_recusado() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        let mut proposta = incoming("c1", &format!("attribute:{}", "a".repeat(200)), "x", "e1");
        proposta.target_type = "entity".into();
        store_contribution(
            &fixture.database,
            &LojaDeTeste::nova().store,
            "sess",
            1,
            proposta,
        )
        .expect("guardar");

        let error = review(&fixture.database, "c1", "approved").expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    #[test]
    fn reabrir_sessao_encerrada_limpa_a_data_de_fim() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        end_session(&fixture.database, "sess", "ended").expect("encerrar");

        save_current_session(&fixture);

        let sessions = list_sessions(&fixture.database).expect("listar");
        assert_eq!(sessions[0].status, "active");
        assert!(
            sessions[0].ended_at.is_none(),
            "data de fim antiga nao pode sobrar"
        );
    }

    #[test]
    fn status_de_encerramento_invalido_e_recusado() {
        let fixture = TemporaryDatabase::new();
        let error = end_all_active(&fixture.database, "pausada").expect_err("deveria recusar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // As duas barreiras de colaboração (ADR 0010)
    // ═══════════════════════════════════════════════════════════════════════

    /// Semeia o mínimo para haver capítulo alvo.
    fn semear_capitulo_alvo(fixture: &TemporaryDatabase, conteudo: &str) {
        fixture
            .connection()
            .execute_batch(&format!(
                "INSERT OR IGNORE INTO universes (id, name, created_at, updated_at)
                    VALUES ('u1','U','2026-01-01','2026-01-01');
                 INSERT OR IGNORE INTO stories (id, universe_id, name, created_at, updated_at)
                    VALUES ('s1','u1','S','2026-01-01','2026-01-01');
                 INSERT OR IGNORE INTO books (id, story_id, name, created_at, updated_at)
                    VALUES ('b1','s1','L','2026-01-01','2026-01-01');
                 INSERT INTO chapters (id, book_id, title, content, word_count)
                    VALUES ('cap1','b1','Cap','{conteudo}', 5);"
            ))
            .expect("semear capítulo");
    }

    fn proposta_de_capitulo(id: &str, original: &str, proposto: &str) -> IncomingContribution {
        IncomingContribution {
            id: id.into(),
            contributor: "Convidada".into(),
            kind: "edit".into(),
            universe_id: "u1".into(),
            target_type: "chapter".into(),
            target_id: "cap1".into(),
            target_label: "Cap".into(),
            field: "content".into(),
            original_value: original.into(),
            proposed_value: proposto.into(),
            message: String::new(),
            created_at: String::new(),
        }
    }

    /// **`store_contribution` normaliza a inline válida antes do `INSERT`.**
    ///
    /// Nada de inline novo no banco: o valor do convidado entra já como
    /// referência, e os bytes vão para o blob store.
    #[test]
    fn store_contribution_normaliza_inline_antes_de_gravar() {
        let fixture = TemporaryDatabase::new();
        let loja = LojaDeTeste::nova();
        seed_session(&fixture);
        let (url, bytes) = inline_de_teste("a-imagem-da-convidada");

        assert!(store_contribution(
            &fixture.database,
            &loja.store,
            "sess",
            1,
            proposta_de_capitulo(
                "c1",
                &format!("<p>antes</p><img src=\"{url}\">"),
                &format!("<p>proposta</p><img src=\"{url}\">"),
            ),
        )
        .expect("guardar"));

        let hash = crate::infrastructure::blob_store::hash_dos_bytes(&bytes);
        let (original, proposto): (String, String) = fixture
            .connection()
            .query_row(
                "SELECT original_value, proposed_value FROM collaboration_contributions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler");

        for (lado, valor) in [("original", &original), ("proposta", &proposto)] {
            assert!(valor.contains(&hash), "{lado} sem referência: {valor}");
            assert!(
                !valor.contains("data:") && !valor.contains("base64"),
                "{lado} ainda tem inline: {valor}"
            );
        }
        assert!(loja.store.verify(&hash).expect("integridade"));
    }

    /// **Inline inválida é recusada, e nada entra no banco.**
    ///
    /// Dado novo não ganha pendência de migração: pendência é para legado que
    /// já estava no acervo. O que não dá para converter na fronteira é
    /// recusado ali.
    #[test]
    fn store_contribution_recusa_inline_invalida_sem_gravar() {
        let fixture = TemporaryDatabase::new();
        let loja = LojaDeTeste::nova();
        seed_session(&fixture);

        let erro = store_contribution(
            &fixture.database,
            &loja.store,
            "sess",
            1,
            proposta_de_capitulo(
                "c1",
                "<p>antes</p>",
                "<img src=\"data:image/png;base64,!!!nao-abre\">",
            ),
        )
        .expect_err("a proposta não pode entrar");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);

        let quantas: i64 = fixture
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collaboration_contributions",
                [],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(quantas, 0, "nada podia ter sido gravado");
    }

    /// Referência de blob torta também é recusada na fronteira.
    #[test]
    fn store_contribution_recusa_referencia_torta() {
        let fixture = TemporaryDatabase::new();
        let loja = LojaDeTeste::nova();
        seed_session(&fixture);

        let erro = store_contribution(
            &fixture.database,
            &loja.store,
            "sess",
            1,
            proposta_de_capitulo(
                "c1",
                "<p>antes</p>",
                "<img data-narrahub-blob=\"../../../etc/passwd\">",
            ),
        )
        .expect_err("referência torta");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);
    }

    /// **Contribuição nova não consegue persistir fonte externa.**
    ///
    /// O caminho da colaboração é o que mais importa fechar: o valor vem de
    /// outro aparelho, e `original_value` chega junto — um documento legado com
    /// imagem externa poderia atravessar a fronteira nos dois lados.
    ///
    /// Os dois lados são verificados, e nada é gravado: `COUNT(*) = 0`.
    #[test]
    fn store_contribution_recusa_fonte_externa_nos_dois_lados() {
        for fora in [
            "https://cdn.exemplo.com/capa.png",
            "C:\\Users\\alguem\\capa.png",
            "/home/alguem/capa.png",
            "file:///home/alguem/capa.png",
            "blob:http://localhost:4200/9f2c-4b1e",
        ] {
            let imagem = format!("<img src=\"{fora}\">");

            for (original, proposto) in [
                ("<p>antes</p>".to_string(), imagem.clone()),
                (imagem.clone(), "<p>depois</p>".to_string()),
            ] {
                let fixture = TemporaryDatabase::new();
                let loja = LojaDeTeste::nova();
                seed_session(&fixture);

                let erro = store_contribution(
                    &fixture.database,
                    &loja.store,
                    "sess",
                    1,
                    proposta_de_capitulo("c1", &original, &proposto),
                )
                .expect_err("fonte externa não pode entrar");
                assert_eq!(erro.kind, DatabaseErrorKind::Validation);

                let quantas: i64 = fixture
                    .connection()
                    .query_row(
                        "SELECT COUNT(*) FROM collaboration_contributions",
                        [],
                        |row| row.get(0),
                    )
                    .expect("contar");
                assert_eq!(quantas, 0, "nada podia ter sido gravado: {fora:?}");
            }
        }
    }

    /// Campo de texto comum continua texto.
    ///
    /// Um convidado pode propor um resumo que menciona data URLs. O recorte é
    /// `chapter`/`content`, e transformar prosa em asset seria inventar
    /// arquivo.
    #[test]
    fn campo_de_texto_comum_nao_passa_pela_normalizacao() {
        let fixture = TemporaryDatabase::new();
        let loja = LojaDeTeste::nova();
        seed_session(&fixture);
        let prosa = "Ele explicava o que era data:image/png;base64,AAAA para a turma.";

        let mut proposta = proposta_de_capitulo("c1", "antes", prosa);
        proposta.field = "summary".into();
        assert!(
            store_contribution(&fixture.database, &loja.store, "sess", 1, proposta)
                .expect("guardar"),
        );

        let proposto: String = fixture
            .connection()
            .query_row(
                "SELECT proposed_value FROM collaboration_contributions",
                [],
                |row| row.get(0),
            )
            .expect("ler");
        assert_eq!(proposto, prosa, "a prosa do convidado não podia ser tocada");
    }

    /// **`review(approved)` recusa proposta legada com inline, e ela continua
    /// `pending`.**
    ///
    /// O cenário salta a primeira barreira de propósito, gravando direto no
    /// banco: é o que um banco importado, uma versão antiga ou uma linha
    /// adulterada produzem. Aprovar copiaria os bytes para o capítulo, e dali
    /// o gatilho de revisão e o evento assinado seguiriam sozinhos.
    #[test]
    fn review_recusa_proposta_com_inline_e_ela_continua_pendente() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);
        semear_capitulo_alvo(&fixture, "<p>o texto do escritor</p>");
        let (url, _) = inline_de_teste("bytes");

        // Entra pela porta de trás, como legado.
        fixture
            .connection()
            .execute(
                "INSERT INTO collaboration_contributions
                    (id, session_id, sequence, kind, universe_id, target_type, target_id,
                     target_label, field, original_value, proposed_value, status, created_at)
                 VALUES ('c1','sess',1,'edit','u1','chapter','cap1','Cap','content',
                         '<p>antes</p>', ?1, 'pending','2026-01-01')",
                [&format!("<p>proposta</p><img src=\"{url}\">")],
            )
            .expect("legado gravado direto");

        let erro = review(&fixture.database, "c1", "approved").expect_err("não pode aplicar");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);

        let conexao = fixture.connection();
        let status: String = conexao
            .query_row(
                "SELECT status FROM collaboration_contributions WHERE id = 'c1'",
                [],
                |row| row.get(0),
            )
            .expect("ler status");
        assert_eq!(
            status, "pending",
            "a contribuição tinha que continuar pendente"
        );

        let conteudo: String = conexao
            .query_row(
                "SELECT content FROM chapters WHERE id = 'cap1'",
                [],
                |row| row.get(0),
            )
            .expect("ler capítulo");
        assert_eq!(
            conteudo, "<p>o texto do escritor</p>",
            "o capítulo não podia mudar"
        );

        let revisoes: i64 = conexao
            .query_row("SELECT COUNT(*) FROM chapter_revisions", [], |row| {
                row.get(0)
            })
            .expect("contar revisões");
        assert_eq!(revisoes, 0, "o gatilho não podia criar revisão");

        let eventos: i64 = conexao
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar eventos");
        assert_eq!(eventos, 0, "nenhum evento podia nascer");
    }

    /// E a proposta blob-safe é aplicada normalmente.
    ///
    /// Senão o gate de cima passaria por vácuo: uma barreira que recusa tudo
    /// também recusa o inline.
    #[test]
    fn review_aplica_proposta_blob_safe() {
        let fixture = TemporaryDatabase::new();
        let loja = LojaDeTeste::nova();
        seed_session(&fixture);
        semear_capitulo_alvo(&fixture, "<p>antes</p>");
        let (url, bytes) = inline_de_teste("imagem");

        store_contribution(
            &fixture.database,
            &loja.store,
            "sess",
            1,
            proposta_de_capitulo(
                "c1",
                "<p>antes</p>",
                &format!("<p>depois</p><img src=\"{url}\">"),
            ),
        )
        .expect("guardar");

        review(&fixture.database, "c1", "approved").expect("aprovar");

        let conteudo: String = fixture
            .connection()
            .query_row(
                "SELECT content FROM chapters WHERE id = 'cap1'",
                [],
                |row| row.get(0),
            )
            .expect("ler capítulo");
        let hash = crate::infrastructure::blob_store::hash_dos_bytes(&bytes);
        assert!(conteudo.contains(&hash), "{conteudo}");
        assert!(!conteudo.contains("data:"));
    }
}
