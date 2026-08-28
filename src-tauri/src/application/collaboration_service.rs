use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::collaboration::{
    attribute_key, writable_column, CollaborationContribution, CollaborationSession,
    IncomingContribution, NewCollaborationSession, MAX_ATTRIBUTE_KEY,
};
use crate::domain::ids::{new_id, now_timestamp};
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
pub fn store_contribution(
    database: &SqliteDatabase,
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
        original_value: incoming.original_value,
        proposed_value: incoming.proposed_value,
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

    #[test]
    fn contribuicao_sem_nome_entra_como_convidado() {
        let fixture = TemporaryDatabase::new();
        seed_session(&fixture);

        assert!(store_contribution(
            &fixture.database,
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
            "sess",
            1,
            incoming("c1", "name", "Novo", "u1")
        )
        .expect("primeira"));
        assert!(!store_contribution(
            &fixture.database,
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
            "sess",
            1,
            incoming("boa", "name", "Novo nome", "u1"),
        )
        .expect("guardar");
        store_contribution(
            &fixture.database,
            "sess",
            2,
            incoming("ruim", "cover_image", "x", "u1"),
        )
        .expect("guardar");
        store_contribution(
            &fixture.database,
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
        store_contribution(&fixture.database, "sess", 1, proposta).expect("guardar");

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
        store_contribution(&fixture.database, "sess", 1, proposta).expect("guardar");

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
}
