//! Codecs de agregado do Sync V2 (NH-079).
//!
//! Um codec responde, para **um** tipo de agregado, as perguntas de que a fronteira `Mutacao`, a
//! aplicação remota e — na etapa C — a gênese precisam. É a mesma função nos três lugares:
//!
//! ```text
//! ler_canonico           estado atual no formato do payload do evento        (None = não existe)
//! impactos_da_exclusao   o que a exclusão deste agregado faz com OUTROS agregados, antes do DELETE
//! pai_ausente            o upsert remoto depende de algo que ainda não chegou?
//! aplicar                escreve no domínio o que o evento diz (o estado causal é de sync_apply)
//! ```
//!
//! **O contrato é em duas partes, e elas não são a mesma frase:**
//!
//! ```text
//! mesmo estado semântico  →  mesmo payload canônico
//! mesma revisão           →  mesmos inputs causais, base_rev inclusive
//! ```
//!
//! Dois aparelhos com o mesmo conteúdo produzem o mesmo payload. Isso **não** basta para a revisão
//! ser igual: a revisão é função do payload E da base de onde ele partiu. Duas histórias diferentes
//! que chegam ao mesmo texto têm revisões diferentes — e é isso que separa "convergiram" de "é a
//! mesma escrita". A gênese usa a outra ponta da regra: base raiz nos dois lados, payload igual,
//! revisão igual.
//!
//! O payload canônico não carrega relógio local
//! (`created_at`, `updated_at`), cache, contagem derivável nem posição que pertence a outro agregado.
//! Os formatos estão em `docs/sync/MATRIZ_COBERTURA_NH079.md`, seção 8.
//!
//! ## Impacto de exclusão: excluir não é o único efeito
//!
//! ```text
//! Excluido(agregado)    some junto (FK CASCADE ou gatilho)  → evento delete, filhos antes do pai
//! Reescrito(agregado)   sobrevive, mas muda (SET NULL, lista que perde um item)
//!                                                          → estado relido depois do SQL → upsert
//! Bloqueado(motivo)     o efeito atinge agregado ainda não coberto → a exclusão é recusada
//! ```
//!
//! O catálogo de todos os efeitos do schema — FK e gatilho — está em [`catalogo`], com gate que
//! reprova qualquer FK ou gatilho novo sem classificação.

use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

pub mod adocao;
mod anexo;
pub mod canvas;
pub mod catalogo;
pub mod conhecimento;
pub mod efemero;
pub mod entidades;
pub mod manuscrito;
pub mod midia;
pub mod modelos;
pub mod palavras;
pub mod planejamento;
pub mod posicao;
pub mod resolucao;

/// **A versão do formato canônico dos payloads** — e do conjunto de tipos cobertos.
///
/// Dois aparelhos só trocam eventos se concordarem nisto: um payload é aplicado pelo `aplicar` do
/// codec com `deny_unknown_fields`, e um formato diferente derruba a sessão inteira no meio, com a
/// causa errada. O `Hello` da etapa E compara este número antes de qualquer escrita.
///
/// Sobe quando muda qualquer vetor de canonicalização, o conjunto `TIPOS_COBERTOS`, ou o que um
/// payload significa. E a adoção sobe junto: `genese::VERSAO_DA_ADOCAO` é este número, por
/// definição, e não um segundo número que alguém lembra de manter igual.
///
/// **2** (etapa F): o agregado `conflict_resolution` entrou em `TIPOS_COBERTOS`. Um aparelho no
/// formato 1 não saberia aplicar a decisão de um conflito — e aplicaria os efeitos dela como
/// escritas comuns, sem conferir o certificado.
pub const FORMATO_CANONICO_ATUAL: i64 = 2;

/// O estado de um agregado como o evento o carrega.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDoAgregado {
    pub universe_id: String,
    pub payload: String,
}

/// O que a exclusão de um agregado faz com outro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impacto {
    /// Some junto com o agregado excluído.
    Excluido(AggregateRef),
    /// Continua existindo, com estado diferente.
    Reescrito(AggregateRef),
    /// O efeito atinge algo que o Sync V2 ainda não sabe representar. Fail closed.
    Bloqueado(String),
}

/// Os tipos que a fronteira sabe representar.
pub const TIPOS_COBERTOS: &[&str] = &[
    "universe",
    "story",
    "book",
    "chapter",
    "entity",
    "relation",
    "timeline_event",
    "canvas_entity_position",
    "planning_item",
    "planning_field_definition",
    "attachment",
    "tag_assignment",
    "content_tag",
    "canvas_node",
    "canvas_node_position",
    "canvas_edge",
    "story_position",
    "book_position",
    "chapter_position",
    "planning_field_position",
    "attachment_position",
    "planning_item_position",
    // B6 item 1: os atributos padrão de um tipo de ficha, o conjunto inteiro como um agregado.
    "entity_template_set",
    // Etapa F: a decisão sobre um conflito, com `aggregateId = conflictKey`.
    "conflict_resolution",
];

pub fn coberto(tipo: &str) -> bool {
    TIPOS_COBERTOS.contains(&tipo)
}

pub fn nao_coberto(tipo: &str) -> DatabaseCommandError {
    DatabaseCommandError::storage(format!(
        "O agregado '{tipo}' ainda não é coberto pelo Sync V2 (NH-079). Uma mutação dele não pode \
         ser declarada: o evento não teria como ser lido nem verificado."
    ))
}

pub(crate) fn erro(error: rusqlite::Error) -> DatabaseCommandError {
    DatabaseCommandError::storage(error.to_string())
}

/// Agregado **sem linha própria**: o estado dele mora na linha de outro.
///
/// ```text
/// *_position               o sort_order (e a etapa, no card) mora na linha do item (B2.2)
/// canvas_node_position     position_x/y moram na linha do elemento livre (B5)
/// ```
///
/// Isto diz só **onde o estado está**, e serve a duas coisas mecânicas: a exclusão dele, que chega
/// antes da exclusão do item, materializa quando o item sair; e, na exclusão remota do item, ele só
/// conta como vivo se ainda tiver revisão corrente.
///
/// **Não é classificação semântica.** Estes agregados são autorais — existe ação do escritor que os
/// muda sozinhos (arrastar, reordenar, trocar etapa) — e conflitam como qualquer outro, inclusive
/// contra a exclusão do item. Nunca use isto para decidir que uma revisão pode ser descartada.
pub fn existencia_derivada(tipo: &str) -> bool {
    posicao::tipo_de_posicao(tipo).is_some() || tipo == "canvas_node_position"
}

/// O estado canônico atual, lido na conexão/transação recebida.
pub fn ler_canonico(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let id = agregado.aggregate_id.as_str();
    if let Some(tipo) = posicao::tipo_de_posicao(&agregado.aggregate_type) {
        return posicao::ler(connection, tipo, id);
    }
    match agregado.aggregate_type.as_str() {
        "universe" => manuscrito::ler_universo(connection, id),
        "story" => manuscrito::ler_historia(connection, id),
        "book" => manuscrito::ler_livro(connection, id),
        "chapter" => manuscrito::ler_capitulo(connection, id),
        "tag_assignment" => manuscrito::ler_atribuicao(connection, id),
        "entity" => entidades::ler_entidade(connection, id),
        "entity_template_set" => modelos::ler(connection, id),
        "relation" => entidades::ler_relacao(connection, id),
        "timeline_event" => entidades::ler_evento(connection, id),
        "canvas_entity_position" => entidades::ler_posicao(connection, id),
        "planning_item" => planejamento::ler_card(connection, id),
        "planning_field_definition" => planejamento::ler_campo(connection, id),
        "attachment" => anexo::ler(connection, id),
        "content_tag" => conhecimento::ler(connection, id),
        "canvas_node" => canvas::ler_no(connection, id),
        "conflict_resolution" => resolucao::ler(connection, id),
        "canvas_node_position" => canvas::ler_posicao(connection, id),
        "canvas_edge" => canvas::ler_aresta(connection, id),
        outro => Err(nao_coberto(outro)),
    }
}

/// Os efeitos **diretos** da exclusão sobre outros agregados, lidos ANTES do SQL destrutivo.
///
/// Só um nível: quem precisa da árvore desce pelos `Excluido`. Estado interno do próprio agregado
/// (campos personalizados, por exemplo) não é impacto — some com ele e não tem identidade própria.
pub fn impactos_da_exclusao(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let id = agregado.aggregate_id.as_str();
    if posicao::tipo_de_posicao(&agregado.aggregate_type).is_some() {
        return Ok(Vec::new());
    }
    let mut impactos = impactos_do_item(connection, agregado)?;
    // **A posição do item sai junto, e sai daqui.** Um lugar só, para nenhum codec esquecer: foi
    // exatamente "cobertura declarada num lugar e esquecida em outro" que a B6 encontrou três vezes.
    // Vem antes dos outros impactos, e todos saem antes do próprio item.
    if let Some(tipo) = posicao::posicao_do_item(&agregado.aggregate_type) {
        impactos.insert(0, Impacto::Excluido(AggregateRef::new(tipo, id)));
    }
    Ok(impactos)
}

fn impactos_do_item(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Vec<Impacto>> {
    let id = agregado.aggregate_id.as_str();
    match agregado.aggregate_type.as_str() {
        "universe" => Ok(manuscrito::impactos_do_universo()),
        "story" => manuscrito::impactos_da_historia(connection, id),
        "book" => manuscrito::impactos_do_livro(connection, id),
        "chapter" => manuscrito::impactos_do_capitulo(connection, id),
        "entity" => entidades::impactos_da_entidade(connection, id),
        "timeline_event" => entidades::impactos_do_evento(connection, id),
        "planning_item" => planejamento::impactos_do_card(connection, id),
        "planning_field_definition" => planejamento::impactos_do_campo(connection, id),
        "content_tag" => conhecimento::impactos_da_tag(connection, id),
        "canvas_node" => canvas::impactos_do_no(connection, id),
        // O conjunto de modelos não tem descendente: apagá-lo é apagar as linhas dele.
        "entity_template_set"
        | "attachment"
        | "tag_assignment"
        | "relation"
        | "canvas_entity_position"
        | "canvas_node_position"
        | "canvas_edge"
        | "conflict_resolution" => Ok(Vec::new()),
        outro => Err(nao_coberto(outro)),
    }
}

/// Para upsert remoto: tudo de que o estado final depende está aqui e é coerente?
///
/// `Ok(Some(falta))` é história incompleta — quem chama não aplica, não marca como aplicado, e o
/// cursor espera. `Err` é inconsistência que nunca fica válida (pai diferente, ordem com capítulo
/// repetido ou de outro livro).
pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    if envelope.operation != Operation::Upsert {
        return Ok(None);
    }
    if let Some(tipo) = posicao::tipo_de_posicao(&envelope.aggregate_type) {
        return posicao::dependencias(connection, tipo, envelope);
    }
    match envelope.aggregate_type.as_str() {
        "universe" => Ok(None),
        "story" | "book" | "chapter" | "tag_assignment" => {
            manuscrito::dependencias(connection, envelope)
        }
        "attachment" => anexo::pai_ausente(connection, envelope),
        "entity" | "relation" | "timeline_event" | "canvas_entity_position" => {
            entidades::dependencias(connection, envelope)
        }
        "planning_item" | "planning_field_definition" => {
            planejamento::dependencias(connection, envelope)
        }
        "content_tag" => conhecimento::dependencias(connection, envelope),
        "entity_template_set" => modelos::dependencias(connection, envelope),
        "canvas_node" | "canvas_node_position" | "canvas_edge" => {
            canvas::dependencias(connection, envelope)
        }
        _ => Ok(None),
    }
}

/// Escreve no domínio o que o evento diz. Tombstone, revisão corrente e cursor são de `sync_apply`.
///
/// **Falha fechada em tipo desconhecido.** Ignorar um agregado que ainda não sabemos aplicar
/// produziria o pior estado possível: o evento constaria como aplicado e o dado nunca chegaria.
pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    if let Some(tipo) = posicao::tipo_de_posicao(&envelope.aggregate_type) {
        return posicao::aplicar(tx, tipo, envelope);
    }
    match envelope.aggregate_type.as_str() {
        "universe" | "story" | "book" | "chapter" | "tag_assignment" => {
            manuscrito::aplicar(tx, envelope)
        }
        "entity" | "relation" | "timeline_event" | "canvas_entity_position" => {
            entidades::aplicar(tx, envelope)
        }
        "planning_item" | "planning_field_definition" => planejamento::aplicar(tx, envelope),
        "attachment" => anexo::aplicar(tx, envelope),
        "content_tag" => conhecimento::aplicar(tx, envelope),
        "entity_template_set" => modelos::aplicar(tx, envelope),
        "canvas_node" | "canvas_node_position" | "canvas_edge" => canvas::aplicar(tx, envelope),
        "conflict_resolution" => resolucao::aplicar(tx, envelope),
        outro => Err(DatabaseCommandError::storage(format!(
            "Agregado '{outro}' ainda não tem aplicação de evento implementada. A sessão para \
             aqui de propósito: avançar marcaria o evento como aplicado sem que o dado tivesse \
             chegado, e ninguém saberia que faltou."
        ))),
    }
}

/// **Um evento local não pode ser estruturalmente inválido para o próprio apply remoto.**
///
/// A `Mutacao` chama isto com o payload que acabou de ler do banco, antes de emitir. É a MESMA
/// função que a aplicação remota usa em `dependencias` — nenhum caminho local escapa de uma regra
/// que o outro lado cobra. Aqui, "dependência faltando" também é erro: localmente o estado já está
/// escrito, então faltar é inconsistência, não espera.
pub fn validar_para_emissao(
    connection: &Connection,
    agregado: &AggregateRef,
    payload: &str,
) -> DatabaseCommandResult<()> {
    let falta = match agregado.aggregate_type.as_str() {
        tipo if posicao::tipo_de_posicao(tipo).is_some() => posicao::validar(
            connection,
            posicao::tipo_de_posicao(tipo).expect("tipo de posição"),
            payload,
        )?,
        "entity" | "relation" | "timeline_event" | "canvas_entity_position" => {
            entidades::validar(connection, &agregado.aggregate_type, payload)?
        }
        "planning_item" | "planning_field_definition" => {
            planejamento::validar(connection, &agregado.aggregate_type, payload)?
        }
        "attachment" => anexo::validar(connection, payload)?,
        "tag_assignment" => manuscrito::validar_atribuicao(connection, payload)?,
        "content_tag" => conhecimento::validar(connection, payload)?,
        "entity_template_set" => modelos::validar(connection, payload)?,
        "canvas_node" | "canvas_node_position" | "canvas_edge" => {
            canvas::validar(connection, &agregado.aggregate_type, payload)?
        }
        "conflict_resolution" => resolucao::validar(payload)?,
        _ => None,
    };
    match falta {
        None => Ok(()),
        Some(falta) => Err(DatabaseCommandError::storage(format!(
            "{} {} não pode ser sincronizado: falta {falta}. Nada foi confirmado.",
            agregado.aggregate_type, agregado.aggregate_id
        ))),
    }
}

/// O payload de um evento, no tipo canônico. Campo desconhecido é recusado: outro formato de
/// canonicalização não pode ser aplicado como se fosse este.
pub(crate) fn de_json<T: serde::de::DeserializeOwned>(
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<T> {
    serde_json::from_str(&envelope.payload).map_err(|error| {
        DatabaseCommandError::storage(format!(
            "O payload de {} {} não está no formato canônico: {error}",
            envelope.aggregate_type, envelope.aggregate_id
        ))
    })
}

pub fn para_json<T: serde::Serialize>(valor: &T) -> DatabaseCommandResult<String> {
    serde_json::to_string(valor).map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

pub(crate) fn existe(
    connection: &Connection,
    tabela: &str,
    id: &str,
) -> DatabaseCommandResult<bool> {
    connection
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {tabela} WHERE id = ?1)"),
            [id],
            |row| row.get(0),
        )
        .map_err(erro)
}

/// Por que um agregado não pode ser alterado silenciosamente agora (preflight).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoConcorrente {
    /// Há divergência aberta: duas versões esperando o escritor decidir.
    DivergenciaAberta,
    /// Há evento recebido e ainda não aplicado (história que falta chegar).
    EventoPendente,
}

/// O agregado tem estado concorrente que uma exclusão ou reescrita destruiria?
pub fn estado_concorrente(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<EstadoConcorrente>> {
    // A decisão de um grupo é ancorada num membro, mas vale para a AÇÃO: todo agregado que é membro
    // de um grupo em decisão aberta está em decisão (B2.2). Sem isto, excluir aqui um capítulo que
    // faz parte de uma ação esperando decisão passaria pelo preflight.
    let divergencia: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_divergences
                            WHERE aggregate_type = ?1 AND aggregate_id = ?2 AND resolved_at = '')
                 OR EXISTS(SELECT 1 FROM sync_divergences d
                             JOIN sync_events ancora ON ancora.event_id = d.remote_event_id
                             JOIN sync_events e ON e.device_id = ancora.device_id
                                               AND e.mutation_id = d.mutation_id
                            WHERE d.mutation_id <> '' AND d.resolved_at = ''
                              AND e.aggregate_type = ?1 AND e.aggregate_id = ?2)",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    if divergencia {
        return Ok(Some(EstadoConcorrente::DivergenciaAberta));
    }
    let pendente: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events e
                            WHERE e.aggregate_type = ?1 AND e.aggregate_id = ?2
                              AND NOT EXISTS (SELECT 1 FROM sync_applied_events a
                                               WHERE a.event_id = e.event_id))",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .map_err(erro)?;
    Ok(pendente.then_some(EstadoConcorrente::EventoPendente))
}

/// Como [`estado_concorrente`], visto por um evento remoto que está sendo aplicado.
///
/// Quem recebe guarda o lote inteiro antes de aplicar. Os eventos seguintes **da mesma origem**
/// (`seq` maior) já estão no log sem estar aplicados, e não são concorrência: são a continuação
/// da mesma mutação — a reescrita da ordem que vem logo depois da exclusão do capítulo, por
/// exemplo. Pendência de outra origem, ou anterior na mesma, continua contando.
pub fn estado_concorrente_para_evento(
    connection: &Connection,
    agregado: &AggregateRef,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<EstadoConcorrente>> {
    if matches!(
        estado_concorrente(connection, agregado)?,
        Some(EstadoConcorrente::DivergenciaAberta)
    ) {
        return Ok(Some(EstadoConcorrente::DivergenciaAberta));
    }
    let pendente: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events e
                            WHERE e.aggregate_type = ?1 AND e.aggregate_id = ?2
                              AND NOT (e.device_id = ?3 AND e.seq > ?4)
                              AND NOT EXISTS (SELECT 1 FROM sync_applied_events a
                                               WHERE a.event_id = e.event_id))",
            rusqlite::params![
                &agregado.aggregate_type,
                &agregado.aggregate_id,
                &envelope.device_id,
                envelope.seq
            ],
            |row| row.get(0),
        )
        .map_err(erro)?;
    Ok(pendente.then_some(EstadoConcorrente::EventoPendente))
}

/// A revisão corrente do agregado, se houver.
pub fn revisao_corrente(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT current_rev FROM sync_aggregate_state
              WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

/// O payload da revisão corrente, quando ela veio de um evento que está no log.
pub fn payload_da_revisao_corrente(
    connection: &Connection,
    agregado: &AggregateRef,
) -> DatabaseCommandResult<Option<String>> {
    connection
        .query_row(
            "SELECT e.payload
               FROM sync_aggregate_state s
               JOIN sync_revision_history h
                 ON h.aggregate_type = s.aggregate_type AND h.aggregate_id = s.aggregate_id
                AND h.rev = s.current_rev
               JOIN sync_events e ON e.event_id = h.event_id
              WHERE s.aggregate_type = ?1 AND s.aggregate_id = ?2",
            [&agregado.aggregate_type, &agregado.aggregate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(erro)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    /// **A decisão de um grupo é `(origem, mutation_id)`.** Duas origens podem emitir ações com o
    /// mesmo `mutation_id` — ele é sorteado em cada aparelho, sem coordenação. Se o estado
    /// concorrente casasse só pelo id, a decisão aberta sobre a ação de A poria em decisão os
    /// agregados da ação de B, e o escritor veria o trabalho dele travado por uma decisão que não
    /// é sobre ele.
    #[test]
    fn a_decisao_de_grupo_e_da_origem_e_nao_so_do_mutation_id() {
        let fixture = TemporaryDatabase::new();
        let connection = fixture.database.write().expect("escrita");
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO sync_devices
                    (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self,
                     state_changed_at)
                 VALUES ('dev-a', 'A', 'pa', 'xa', 'active', '', 0, ''),
                        ('dev-b', 'B', 'pb', 'xb', 'active', '', 0, '');

                 -- A ação de A (mutation_id 'M'): o agregado X.
                 INSERT INTO sync_events
                    (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation,
                     payload, base_rev, new_rev, signature,
                     mutation_id, mutation_index, mutation_count, mutation_kind)
                 VALUES ('ev-a1', 'dev-a', 1, 'u1', 'chapter', 'X', 'delete', '', 'r0', 'ra',
                         'sa', 'M', 0, 1, 'delete_tree');

                 -- A ação de B, com o MESMO mutation_id: o agregado Y.
                 INSERT INTO sync_events
                    (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation,
                     payload, base_rev, new_rev, signature,
                     mutation_id, mutation_index, mutation_count, mutation_kind)
                 VALUES ('ev-b1', 'dev-b', 1, 'u1', 'chapter', 'Y', 'delete', '', 'r0', 'rb',
                         'sb', 'M', 0, 1, 'delete_tree');

                 -- Os dois eventos estão aplicados: o que sobrar de concorrência é decisão, não
                 -- pendência.
                 INSERT INTO sync_applied_events (event_id) VALUES ('ev-a1'), ('ev-b1');

                 -- A decisão aberta é a da ação de A, ancorada no evento de A.
                 INSERT INTO sync_divergences
                    (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                     remote_event_id, resolved_at, resolution, local_operation, remote_operation,
                     kind, related_aggregate_id, mutation_id)
                 VALUES ('div-a', 'chapter', 'X', 'r0', 'rx', 'ra', 'ev-a1', '', '',
                         'upsert', 'delete', 'parent_deletion_blocked', '', 'M');",
            )
            .expect("semear");

        let x = AggregateRef::new("chapter", "X");
        let y = AggregateRef::new("chapter", "Y");
        assert_eq!(
            estado_concorrente(&connection, &x).expect("X"),
            Some(EstadoConcorrente::DivergenciaAberta)
        );
        assert_eq!(
            estado_concorrente(&connection, &y).expect("Y"),
            None,
            "a decisão sobre a ação de A pôs em decisão um agregado da ação de B"
        );

        // Agora a ação de B também tem decisão aberta, ancorada no evento de B.
        connection
            .execute_batch(
                "INSERT INTO sync_divergences
                    (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev,
                     remote_event_id, resolved_at, resolution, local_operation, remote_operation,
                     kind, related_aggregate_id, mutation_id)
                 VALUES ('div-b', 'chapter', 'Y', 'r0', 'ry', 'rb', 'ev-b1', '', '',
                         'upsert', 'delete', 'parent_deletion_blocked', '', 'M');",
            )
            .expect("decisão de B");
        assert_eq!(
            estado_concorrente(&connection, &y).expect("Y"),
            Some(EstadoConcorrente::DivergenciaAberta)
        );
    }
}
