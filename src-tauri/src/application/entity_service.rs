use crate::application::blob_fields;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::entity::{
    default_attributes_for, Entity, EntityAttribute, EntityUpdate, EntityWithDetails, NewEntity,
};
use crate::domain::ids::{new_id, now_timestamp};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::{entity_repository, SqliteDatabase};

/// Prefixo que a tela usa para um atributo que ainda não foi gravado. Ele
/// nunca chega ao banco: `save_attribute` reconhece e cria em vez de tentar
/// atualizar um id que não existe.
const TEMPORARY_ID_PREFIX: &str = "temp_";

pub fn list(
    database: &SqliteDatabase,
    store: &BlobStore,
    universe_id: &str,
) -> DatabaseCommandResult<Vec<Entity>> {
    let connection = database.read()?;
    let mut entidades = entity_repository::list(&connection, universe_id)?;
    // A imagem vem do blob store, reconstruída como transporte. Ver
    // `blob_fields`, e a dívida `NH-069`: uma lista grande paga uma leitura de
    // disco por entidade com imagem, e o caminho definitivo é o frontend
    // resolver por hash.
    for entidade in entidades.iter_mut() {
        entidade.image =
            blob_fields::ler_asset_direto(&connection, store, "entities", &entidade.id)?;
    }
    Ok(entidades)
}

pub fn get_with_details(
    database: &SqliteDatabase,
    store: &BlobStore,
    id: &str,
) -> DatabaseCommandResult<Option<EntityWithDetails>> {
    let connection = database.read()?;
    let Some(mut entity) = entity_repository::get(&connection, id)? else {
        return Ok(None);
    };
    entity.image = blob_fields::ler_asset_direto(&connection, store, "entities", id)?;
    let attributes = entity_repository::list_attributes(&connection, id)?;
    let relations = entity_repository::list_relations_for_entity(&connection, &entity)?;
    let mentions = entity_repository::list_mentions_for_entity(&connection, id)?;
    Ok(Some(EntityWithDetails {
        entity,
        attributes,
        relations,
        mentions,
    }))
}

/// Cria a entidade com a ficha já montada, numa transação.
///
/// O caminho antigo eram N gravações soltas: entidade, um `INSERT` por
/// atributo padrão, um por template do universo, mais um `UPDATE` para a
/// imagem. Qualquer falha no meio deixava uma entidade pela metade no arquivo
/// do usuário — e sem transação não havia como desfazer.
///
/// A ordem dos atributos importa e é a mesma de antes: primeiro os padrões do
/// tipo, depois os templates do universo que o padrão não cobre, e por último
/// o que veio no formulário.
pub fn create(
    database: &SqliteDatabase,
    store: &BlobStore,
    input: NewEntity,
) -> DatabaseCommandResult<Entity> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(DatabaseCommandError::validation(
            "A entidade precisa de um nome.",
        ));
    }

    let timestamp = now_timestamp();
    let entity = Entity {
        id: new_id(),
        universe_id: input.universe_id.clone(),
        entity_type: input.entity_type.clone(),
        name: name.to_string(),
        description: input.description.trim().to_string(),
        summary: String::new(),
        image: input.image.clone(),
        canon_status: "CANON".to_string(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };

    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    entity_repository::insert(&transaction, &entity)?;
    // Na mesma transação do `INSERT`: os bytes que chegaram na coluna legada
    // são trocados por referência antes de qualquer commit.
    blob_fields::gravar_asset_direto(&transaction, store, "entities", &entity.id, &input.image)?;

    let defaults = default_attributes_for(&input.entity_type);
    for (index, key) in defaults.iter().enumerate() {
        entity_repository::insert_attribute(
            &transaction,
            &new_id(),
            &entity.id,
            key,
            "",
            index as i64,
        )?;
    }

    let templates =
        entity_repository::list_templates(&transaction, &input.universe_id, &input.entity_type)?;
    for (key, default_value, sort_order) in templates {
        if defaults.contains(&key.as_str()) {
            continue;
        }
        entity_repository::insert_attribute(
            &transaction,
            &new_id(),
            &entity.id,
            &key,
            &default_value,
            defaults.len() as i64 + sort_order,
        )?;
    }

    for attribute in &input.attributes {
        let key = attribute.key.trim();
        if key.is_empty() {
            continue;
        }
        entity_repository::set_attribute_value(
            &transaction,
            &new_id(),
            &entity.id,
            key,
            attribute.value.trim(),
        )?;
    }

    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(entity)
}

pub fn update(
    database: &SqliteDatabase,
    store: &BlobStore,
    id: &str,
    patch: EntityUpdate,
) -> DatabaseCommandResult<()> {
    if patch.is_empty() {
        return Ok(());
    }
    if patch
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(DatabaseCommandError::validation(
            "A entidade precisa de um nome.",
        ));
    }
    let mut connection = database.write()?;
    let tx = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    if !entity_repository::update(&tx, id, &patch, &now_timestamp())? {
        return Err(DatabaseCommandError::not_found("Entidade não encontrada."));
    }
    if let Some(imagem) = patch.image.as_deref() {
        blob_fields::gravar_asset_direto(&tx, store, "entities", id, imagem)?;
    }
    tx.commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

pub fn delete(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !entity_repository::delete(&connection, id)? {
        return Err(DatabaseCommandError::not_found("Entidade não encontrada."));
    }
    Ok(())
}

/// Grava um atributo e carimba a entidade dona.
///
/// As duas coisas vão na mesma transação: sem isso, uma falha depois do
/// atributo deixaria a ficha alterada com `updated_at` antigo, e a
/// sincronização usaria esse carimbo para decidir que nada mudou.
pub fn save_attribute(
    database: &SqliteDatabase,
    attribute: EntityAttribute,
) -> DatabaseCommandResult<()> {
    let key = attribute.key.trim();
    if key.is_empty() {
        return Err(DatabaseCommandError::validation(
            "O atributo precisa de um nome.",
        ));
    }

    let mut connection = database.write()?;
    let transaction = connection
        .transaction()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    if attribute.id.starts_with(TEMPORARY_ID_PREFIX) {
        entity_repository::set_attribute_value(
            &transaction,
            &new_id(),
            &attribute.entity_id,
            key,
            &attribute.value,
        )?;
    } else {
        let saved = EntityAttribute {
            key: key.to_string(),
            ..attribute.clone()
        };
        if !entity_repository::update_attribute(&transaction, &saved)? {
            return Err(DatabaseCommandError::not_found(
                "O atributo não existe mais nesta ficha.",
            ));
        }
    }

    entity_repository::touch(&transaction, &attribute.entity_id, &now_timestamp())?;
    transaction
        .commit()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

pub fn remove_attribute(database: &SqliteDatabase, id: &str) -> DatabaseCommandResult<()> {
    let connection = database.write()?;
    if !entity_repository::delete_attribute(&connection, id)? {
        return Err(DatabaseCommandError::not_found(
            "O atributo não existe mais nesta ficha.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {

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
    use crate::domain::entity::NewEntityAttribute;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    fn new_entity(kind: &str, name: &str) -> NewEntity {
        NewEntity {
            universe_id: "u1".into(),
            entity_type: kind.into(),
            name: name.into(),
            description: String::new(),
            image: String::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn criacao_monta_a_ficha_inteira_numa_transacao() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");

        let entity = create(
            &fixture.database,
            &loja_de_teste().1,
            new_entity("Personagem", "Frodo"),
        )
        .expect("criar entidade");
        let details = get_with_details(&fixture.database, &loja_de_teste().1, &entity.id)
            .expect("buscar ficha")
            .expect("ficha existe");

        assert_eq!(
            details.attributes.len(),
            14,
            "os atributos padrao de Personagem"
        );
        assert_eq!(details.attributes[0].key, "Idade");
        assert!(details.attributes.iter().all(|a| a.value.is_empty()));
    }

    #[test]
    fn universo_inexistente_nao_deixa_entidade_pela_metade() {
        // Com foreign_keys ligada o INSERT falha, e a transacao garante que
        // nenhum atributo sobreviva. Sem transacao, o caminho antigo podia
        // deixar lixo no arquivo do usuario.
        let fixture = TemporaryDatabase::new();

        let error = create(
            &fixture.database,
            &loja_de_teste().1,
            new_entity("Personagem", "Orfa"),
        )
        .expect_err("universo inexistente deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::Conflict);

        let connection = fixture.connection();
        let total: i64 = connection
            .query_row("SELECT COUNT(*) FROM entity_attributes", [], |row| {
                row.get(0)
            })
            .expect("contar atributos");
        assert_eq!(total, 0, "nenhum atributo pode ter sobrado");
    }

    #[test]
    fn template_do_universo_entra_depois_do_padrao_e_sem_repetir() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.connection();
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO entity_templates (id, universe_id, entity_type, attribute_key, default_value, sort_order)
                   VALUES ('t1', 'u1', 'Lugar', 'Moeda', 'ouro', 0),
                          ('t2', 'u1', 'Lugar', 'Clima', 'ignorado', 1);",
            )
            .expect("semear templates");

        let entity = create(
            &fixture.database,
            &loja_de_teste().1,
            new_entity("Lugar", "Condado"),
        )
        .expect("criar");
        let details = get_with_details(&fixture.database, &loja_de_teste().1, &entity.id)
            .expect("buscar")
            .expect("existe");

        let keys: Vec<&str> = details.attributes.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(
            keys.iter().filter(|key| **key == "Clima").count(),
            1,
            "template que repete um padrao nao pode duplicar a linha"
        );
        assert_eq!(
            keys.last(),
            Some(&"Moeda"),
            "o template entra depois do padrao"
        );
        let moeda = details
            .attributes
            .iter()
            .find(|a| a.key == "Moeda")
            .expect("Moeda");
        assert_eq!(
            moeda.value, "ouro",
            "o valor padrao do template precisa ser gravado"
        );
    }

    #[test]
    fn atributo_do_formulario_sobrescreve_o_padrao_em_branco() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");

        let mut input = new_entity("Personagem", "Frodo");
        input.attributes = vec![NewEntityAttribute {
            key: " Idade ".into(),
            value: " 50 ".into(),
        }];
        let entity = create(&fixture.database, &loja_de_teste().1, input).expect("criar");

        let details = get_with_details(&fixture.database, &loja_de_teste().1, &entity.id)
            .expect("buscar")
            .expect("existe");
        let idade = details
            .attributes
            .iter()
            .find(|a| a.key == "Idade")
            .expect("Idade");
        assert_eq!(idade.value, "50", "o valor precisa vir sem espaco em volta");
        assert_eq!(
            details
                .attributes
                .iter()
                .filter(|a| a.key == "Idade")
                .count(),
            1,
            "nao pode criar uma segunda linha para a mesma chave"
        );
    }

    #[test]
    fn nome_em_branco_e_recusado_antes_de_tocar_no_banco() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");

        let error = create(
            &fixture.database,
            &loja_de_teste().1,
            new_entity("Personagem", "   "),
        )
        .expect_err("nome vazio deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::Validation);
    }

    #[test]
    fn salvar_atributo_carimba_a_entidade_dona() {
        // A sincronizacao usa updated_at para decidir o que mudou. Atributo
        // gravado sem carimbar a ficha some do proximo sync.
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let entity = create(
            &fixture.database,
            &loja_de_teste().1,
            new_entity("Personagem", "Frodo"),
        )
        .expect("criar");

        let attribute = EntityAttribute {
            id: "temp_novo".into(),
            entity_id: entity.id.clone(),
            key: "Apelido".into(),
            value: "Sr. Subaperto".into(),
            sort_order: 0,
        };
        // O carimbo tem resolucao de um segundo, entao envelhecer a ficha de
        // proposito e o que torna a asserção determinística — comparar com o
        // create direto passaria a depender de o teste cruzar o segundo.
        fixture
            .connection()
            .execute(
                "UPDATE entities SET updated_at = '2020-01-01 00:00:00' WHERE id = ?1",
                [&entity.id],
            )
            .expect("envelhecer a ficha");

        save_attribute(&fixture.database, attribute).expect("salvar atributo");

        let details = get_with_details(&fixture.database, &loja_de_teste().1, &entity.id)
            .expect("buscar")
            .expect("existe");
        assert!(details.attributes.iter().any(|a| a.key == "Apelido"));
        assert_ne!(
            details.entity.updated_at, "2020-01-01 00:00:00",
            "a ficha precisa ter sido carimbada"
        );
    }

    #[test]
    fn atributo_que_nao_existe_mais_avisa_em_vez_de_gravar_no_vazio() {
        let fixture = TemporaryDatabase::new();
        seed_universe(&fixture.connection(), "u1");
        let entity = create(
            &fixture.database,
            &loja_de_teste().1,
            new_entity("Personagem", "Frodo"),
        )
        .expect("criar");

        let attribute = EntityAttribute {
            id: "fantasma".into(),
            entity_id: entity.id,
            key: "Idade".into(),
            value: "50".into(),
            sort_order: 0,
        };
        let error = save_attribute(&fixture.database, attribute).expect_err("deveria falhar");
        assert_eq!(error.kind, DatabaseErrorKind::NotFound);
    }
}
