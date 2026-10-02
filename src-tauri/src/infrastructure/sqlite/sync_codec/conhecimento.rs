//! Codec de `content_tag` (B5).
//!
//! ## A tag e a marcação são agregados diferentes
//!
//! ```text
//! content_tag      identity = tag.id          nome e cor
//! tag_assignment   identity = tagId:ownerType:ownerId   "esta tag está neste dono"
//! ```
//!
//! A marcação já existe desde a B2 e mora em [`super::manuscrito`], junto dos gatilhos do
//! manuscrito que a apagam — ela não muda de lugar aqui para o diff da B5 continuar legível.
//! A identidade dela **nunca** foi o `id` aleatório da linha: marcar a mesma tag no mesmo
//! capítulo em dois aparelhos é a mesma marcação, e converge sem divergência.
//!
//! ## Excluir uma tag não é só apagar a linha
//!
//! ```text
//! content_tag_assignments.tag_id CASCADE  → Excluido(tag_assignment…)
//! planning_field_links.tag_id    CASCADE  → Reescrito(planning_item…)   o card perde a ligação
//! ```
//!
//! O card sobrevive, então ele é reescrito **antes** da exclusão (a regra de ordem de emissão da
//! B4): quem recebe conhece a concorrência antes do SQL destrutivo, e um card editado do outro
//! lado bloqueia a exclusão da tag em vez de ser alterado sem decisão.
//!
//! ## Tag homônima: o schema recusa, e isso vira decisão do escritor
//!
//! `UNIQUE(universe_id, name COLLATE NOCASE)`. Como a identidade é o `id`, criar "Mar" no PC e
//! "Mar" no Android produz **dois agregados** com o mesmo nome, e o segundo a chegar não cabe na
//! tabela. Nada é aplicado e nada é alterado: abre-se uma divergência `tag_name_conflict` e o
//! escritor decide. Ver `sync_apply::conflito_de_nome_de_tag`.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{de_json, erro, existe, para_json, EstadoDoAgregado, Impacto};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::ids::now_timestamp;
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

/// A tag como o evento a carrega. Sem `created_at`: relógio local não é conteúdo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TagCanonica {
    pub id: String,
    pub universe_id: String,
    pub name: String,
    pub color: String,
}

// ── Leitura canônica ─────────────────────────────────────────────────────

pub fn ler(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let Some((universe_id, name, color)) = connection
        .query_row(
            "SELECT universe_id, name, color FROM content_tags WHERE id = ?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(erro)?
    else {
        return Ok(None);
    };
    let payload = para_json(&TagCanonica {
        id: id.to_string(),
        universe_id: universe_id.clone(),
        name,
        color,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

// ── Impactos de exclusão ─────────────────────────────────────────────────

pub fn impactos_da_tag(connection: &Connection, id: &str) -> DatabaseCommandResult<Vec<Impacto>> {
    // `planning_field_links.tag_id ON DELETE CASCADE`: o card sobrevive sem a ligação.
    let mut impactos = super::planejamento::cards_que_perdem_ligacao(connection, "tag_id", id)?;

    // `content_tag_assignments.tag_id ON DELETE CASCADE`: cada marcação some.
    let mut consulta = connection
        .prepare(
            "SELECT owner_type, owner_id FROM content_tag_assignments
              WHERE tag_id = ?1 ORDER BY owner_type, owner_id",
        )
        .map_err(erro)?;
    let donos: Vec<(String, String)> = consulta
        .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;
    for (owner_type, owner_id) in donos {
        impactos.push(Impacto::Excluido(AggregateRef::new(
            "tag_assignment",
            super::manuscrito::id_da_atribuicao(id, &owner_type, &owner_id),
        )));
    }
    Ok(impactos)
}

// ── Validação (a mesma no local e no remoto) ─────────────────────────────

pub(super) fn validar(
    connection: &Connection,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    let tag: TagCanonica = serde_json::from_str(payload)
        .map_err(|error| DatabaseCommandError::storage(format!("Tag ilegível: {error}")))?;
    if tag.name.trim().is_empty() {
        return Err(DatabaseCommandError::storage(format!(
            "A tag {} está sem nome. Estado incompatível.",
            tag.id
        )));
    }
    // O universo da tag é imutável: uma tag não muda de universo, ela é outra tag.
    if let Some(atual) = connection
        .query_row(
            "SELECT universe_id FROM content_tags WHERE id = ?1",
            [&tag.id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(erro)?
    {
        if atual != tag.universe_id {
            return Err(DatabaseCommandError::storage(format!(
                "A tag {} está no universo {atual}, e o evento diz {}. Uma tag não troca de \
                 universo: estado incompatível.",
                tag.id, tag.universe_id
            )));
        }
    }
    if !existe(connection, "universes", &tag.universe_id)? {
        return Ok(Some(format!("universe {}", tag.universe_id)));
    }
    Ok(None)
}

pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    validar(connection, &envelope.payload)
}

/// A tag daqui que ocupa o nome que o evento traz, se for outra tag.
///
/// Não é dependência (esperar não resolve) nem inconsistência permanente (o estado dos dois lados
/// é legítimo): é decisão do escritor.
pub fn tag_homonima(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    if envelope.operation != Operation::Upsert {
        return Ok(None);
    }
    let tag: TagCanonica = de_json(envelope)?;
    connection
        .query_row(
            "SELECT id FROM content_tags
              WHERE universe_id = ?1 AND name = ?2 COLLATE NOCASE AND id <> ?3",
            [&tag.universe_id, &tag.name, &tag.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

// ── Aplicação ────────────────────────────────────────────────────────────

pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    if envelope.operation == Operation::Delete {
        tx.execute(
            "DELETE FROM content_tags WHERE id = ?1",
            [&envelope.aggregate_id],
        )
        .map_err(erro)?;
        return Ok(());
    }
    let tag: TagCanonica = de_json(envelope)?;
    if tag.id != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(format!(
            "O payload descreve a tag {}, e o envelope é de {}.",
            tag.id, envelope.aggregate_id
        )));
    }
    tx.execute(
        "INSERT INTO content_tags (id, universe_id, name, color, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color",
        rusqlite::params![
            &tag.id,
            &tag.universe_id,
            &tag.name,
            &tag.color,
            now_timestamp()
        ],
    )
    .map_err(erro)?;
    Ok(())
}
