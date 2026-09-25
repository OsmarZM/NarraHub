//! **`conflict_resolution/<conflictKey>`: a decisão sobre um conflito como fato causal** (etapa F).
//!
//! Uma resolução é uma `Mutacao` normal cujo grupo (kind `resolution`) carrega:
//!
//! ```text
//! membro 0     conflict_resolution/<conflictKey>   upsert, base = raiz, payload = certificado
//! membros 1..N os efeitos reais da decisão sobre os agregados
//! ```
//!
//! O certificado declara o conflito (tipo e os dois participantes, exatamente como a chave os
//! canonicaliza), a escolha, e **a lista exata dos efeitos** — cada um com a base de onde partiu, a
//! outra cabeça que ele substitui (quando substitui uma) e a revisão que produziu:
//!
//! ```text
//! { conflictKey, kind, participantA, participantB, choice,
//!   results: [ { aggregateType, aggregateId, operation, baseRev, otherRev, resultRev } ] }
//! ```
//!
//! Nada de relógio, nada de aparelho, nada local: **a mesma decisão lógica, tomada em aparelhos
//! diferentes, produz o mesmo payload** — e, com base na raiz, a mesma revisão. Repetir é
//! idempotente; decidir diferente é `concurrent` sobre este agregado, pelo mecanismo de sempre.
//!
//! ## O certificado é conferido inteiro, antes de qualquer membro materializar
//!
//! O grupo `resolution` é o único caminho em que um efeito pode ser tratado como sequencial sem que
//! a base dele seja a revisão corrente daqui (a regra de dois pais, em `sync_apply`). Por isso
//! `kind = resolution` **não autoriza nada sozinho**: [`validar_grupo`] recalcula a chave, confere
//! os participantes, a forma de cada efeito, a revisão que cada um diz ter produzido e a
//! correspondência exata entre `results[]` e os membros. Qualquer diferença falha fechada.

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::{erro, EstadoDoAgregado};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::conflito::{chave_do_conflito, ConflictParticipant};
use crate::domain::sync::{compute_revision, AggregateRef, EventEnvelope, Operation};

/// O tipo de agregado. É também o `root_type` do grupo de uma resolução.
pub const TIPO: &str = "conflict_resolution";

/// O `kind` do grupo de mutação de uma resolução.
pub const KIND_DO_GRUPO: &str = "resolution";

/// Os tipos de conflito que uma resolução pode fechar.
pub const TIPOS_DE_CONFLITO: &[&str] =
    &["concurrent", "parent_deletion_blocked", "tag_name_conflict"];

/// Um participante, sem perspectiva — a mesma estrutura da chave (`domain::conflito`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParticipanteCanonico {
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub revision: String,
    pub operation: String,
}

impl ParticipanteCanonico {
    pub fn do_conflito(participante: &ConflictParticipant) -> Self {
        Self {
            aggregate_type: participante.aggregate_type.clone(),
            aggregate_id: participante.aggregate_id.clone(),
            revision: participante.revision.clone(),
            operation: participante.operation.clone(),
        }
    }

    pub fn para_o_conflito(&self) -> ConflictParticipant {
        ConflictParticipant::new(
            &self.aggregate_type,
            &self.aggregate_id,
            &self.revision,
            &self.operation,
        )
    }

    fn e_do_agregado(&self, tipo: &str, id: &str) -> bool {
        self.aggregate_type == tipo && self.aggregate_id == id
    }
}

/// Um efeito da decisão: um membro do grupo, descrito sem o payload (que está no próprio membro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EfeitoCanonico {
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub operation: String,
    pub base_rev: String,
    /// A outra cabeça que este efeito substitui — o outro lado daquele agregado no conflito. Vazio
    /// quando o efeito é uma escrita comum, que segue a regra causal de sempre.
    pub other_rev: String,
    pub result_rev: String,
}

impl EfeitoCanonico {
    fn chave(&self) -> (&str, &str) {
        (&self.aggregate_type, &self.aggregate_id)
    }
}

/// O certificado de uma resolução — o payload canônico de `conflict_resolution/<conflictKey>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Certificado {
    pub conflict_key: String,
    pub kind: String,
    pub participant_a: ParticipanteCanonico,
    pub participant_b: ParticipanteCanonico,
    pub choice: String,
    pub results: Vec<EfeitoCanonico>,
}

impl Certificado {
    /// O payload canônico: a ordem dos campos é a do struct, e `results[]` sai ordenado por
    /// `(aggregateType, aggregateId)`.
    pub fn canonico(&self) -> DatabaseCommandResult<String> {
        let mut ordenado = self.clone();
        ordenado.results.sort_by(|a, b| a.chave().cmp(&b.chave()));
        serde_json::to_string(&ordenado)
            .map_err(|error| DatabaseCommandError::storage(error.to_string()))
    }

    /// Lê um payload, e só aceita a forma canônica **exata**: um certificado que reordena, repete ou
    /// acrescenta qualquer coisa é outro texto, e não passa.
    pub fn ler(payload: &str) -> Result<Self, String> {
        let certificado: Certificado = serde_json::from_str(payload)
            .map_err(|error| format!("certificado ilegível: {error}"))?;
        let canonico = certificado.canonico().map_err(|erro| erro.message)?;
        if canonico != payload {
            return Err("o certificado não está na forma canônica".into());
        }
        Ok(certificado)
    }

    /// A chave recalculada a partir do tipo e dos participantes declarados.
    pub fn chave_recalculada(&self) -> String {
        chave_do_conflito(
            &self.kind,
            &self.participant_a.para_o_conflito(),
            &self.participant_b.para_o_conflito(),
        )
    }

    fn participante_de(&self, tipo: &str, id: &str) -> Vec<&ParticipanteCanonico> {
        [&self.participant_a, &self.participant_b]
            .into_iter()
            .filter(|p| p.e_do_agregado(tipo, id))
            .collect()
    }
}

/// O que o apply precisa saber de cada efeito, depois de o certificado passar inteiro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParDoEfeito {
    /// A outra cabeça que o efeito substitui (vazio = escrita comum).
    pub outra: String,
    /// O efeito é sobre um agregado participante do conflito.
    pub de_participante: bool,
}

/// **Confere o certificado de um grupo `resolution` contra os membros do grupo.** Nenhuma escrita.
///
/// Devolve, na ordem dos membros 1..N, o par que o apply usa na regra de dois pais.
pub fn validar_grupo(membros: &[EventEnvelope]) -> Result<Vec<ParDoEfeito>, String> {
    let Some(primeiro) = membros.first() else {
        return Err("grupo de resolução vazio".into());
    };
    if primeiro.aggregate_type != TIPO
        || primeiro.operation != Operation::Upsert
        || !primeiro.base_rev.is_empty()
    {
        return Err("o primeiro membro não é o upsert-raiz de conflict_resolution".into());
    }
    let certificado = Certificado::ler(&primeiro.payload)?;
    if certificado.conflict_key != primeiro.aggregate_id {
        return Err("o certificado declara outro conflito que o do agregado".into());
    }
    if certificado.chave_recalculada() != certificado.conflict_key {
        return Err("a chave do conflito não corresponde aos participantes e ao tipo".into());
    }
    if !TIPOS_DE_CONFLITO.contains(&certificado.kind.as_str()) {
        return Err(format!(
            "tipo de conflito desconhecido: {}",
            certificado.kind
        ));
    }
    let (a, b) = (
        certificado.participant_a.para_o_conflito().canonico(),
        certificado.participant_b.para_o_conflito().canonico(),
    );
    if a.as_bytes() > b.as_bytes() {
        return Err("os participantes não estão na ordem canônica".into());
    }
    if primeiro.grupo.root_type != TIPO || primeiro.grupo.root_id != certificado.conflict_key {
        return Err("o grupo não é o desta resolução".into());
    }

    let efeitos = &membros[1..];
    if certificado.results.len() != efeitos.len() {
        return Err(format!(
            "o certificado declara {} efeito(s), e o grupo tem {}",
            certificado.results.len(),
            efeitos.len()
        ));
    }
    let mut vistos = std::collections::BTreeSet::new();
    let mut pares = Vec::with_capacity(efeitos.len());
    for membro in efeitos {
        if !vistos.insert((membro.aggregate_type.clone(), membro.aggregate_id.clone())) {
            return Err(format!(
                "{} {} aparece duas vezes no grupo",
                membro.aggregate_type, membro.aggregate_id
            ));
        }
        let Some(declarado) = certificado.results.iter().find(|r| {
            r.aggregate_type == membro.aggregate_type && r.aggregate_id == membro.aggregate_id
        }) else {
            return Err(format!(
                "{} {} é membro do grupo e não está no certificado",
                membro.aggregate_type, membro.aggregate_id
            ));
        };
        if declarado.operation != membro.operation.as_str()
            || declarado.base_rev != membro.base_rev
            || declarado.result_rev != membro.new_rev
        {
            return Err(format!(
                "{} {}: o membro não é o efeito que o certificado declara",
                membro.aggregate_type, membro.aggregate_id
            ));
        }
        let agregado = AggregateRef::new(&membro.aggregate_type, &membro.aggregate_id);
        if compute_revision(
            &membro.base_rev,
            &agregado,
            membro.operation,
            &membro.payload,
        ) != membro.new_rev
        {
            return Err(format!(
                "{} {}: a revisão declarada não é a que base e payload produzem",
                membro.aggregate_type, membro.aggregate_id
            ));
        }
        let participantes =
            certificado.participante_de(&membro.aggregate_type, &membro.aggregate_id);
        let de_participante = !participantes.is_empty();
        if de_participante {
            let revisoes: Vec<&str> = participantes.iter().map(|p| p.revision.as_str()).collect();
            if !revisoes.contains(&membro.base_rev.as_str()) {
                return Err(format!(
                    "{} {}: o efeito sobre um participante não parte de nenhum dos lados do conflito",
                    membro.aggregate_type, membro.aggregate_id
                ));
            }
            if !declarado.other_rev.is_empty() {
                // Conflito sobre o mesmo agregado: o par do efeito é exatamente o par do conflito.
                let par = [membro.base_rev.as_str(), declarado.other_rev.as_str()];
                let esperado = [
                    certificado.participant_a.revision.as_str(),
                    certificado.participant_b.revision.as_str(),
                ];
                let mesmo_par = (par[0] == esperado[0] && par[1] == esperado[1])
                    || (par[0] == esperado[1] && par[1] == esperado[0]);
                if !mesmo_par || participantes.len() != 2 {
                    return Err(format!(
                        "{} {}: o par do efeito não é o par do conflito",
                        membro.aggregate_type, membro.aggregate_id
                    ));
                }
            }
        }
        if de_participante && participantes.len() == 2 {
            conferir_escolha(&certificado, membro)?;
        }
        pares.push(ParDoEfeito {
            outra: declarado.other_rev.clone(),
            de_participante,
        });
    }
    if !ESCOLHAS
        .iter()
        .any(|(kind, escolha)| *kind == certificado.kind && *escolha == certificado.choice)
    {
        return Err(format!(
            "escolha '{}' não existe para conflito '{}'",
            certificado.choice, certificado.kind
        ));
    }
    Ok(pares)
}

/// As escolhas que cada tipo de conflito admite.
const ESCOLHAS: &[(&str, &str)] = &[
    ("concurrent", "a"),
    ("concurrent", "b"),
    ("concurrent", "auto"),
    ("parent_deletion_blocked", "a"),
    ("parent_deletion_blocked", "b"),
    ("tag_name_conflict", "rename:a"),
    ("tag_name_conflict", "rename:b"),
    ("tag_name_conflict", "merge"),
];

/// **O efeito sobre o agregado do conflito é o que a escolha diz.**
///
/// ```text
/// concurrent a|b             parte do participante escolhido, com a operação dele
/// concurrent auto            parte de participant_a (a base canônica), com a operação dele
/// parent_deletion_blocked    a operação do escolhido (manter = upsert, aceitar = delete); a base é
///                            a da exclusão, que é de onde a restauração precisa partir
/// ```
fn conferir_escolha(certificado: &Certificado, membro: &EventEnvelope) -> Result<(), String> {
    let escolhido = match certificado.choice.as_str() {
        "a" | "auto" => &certificado.participant_a,
        "b" => &certificado.participant_b,
        _ => return Ok(()),
    };
    let base_confere = match certificado.kind.as_str() {
        "parent_deletion_blocked" => true,
        _ => membro.base_rev == escolhido.revision,
    };
    if !base_confere || membro.operation.as_str() != escolhido.operation {
        return Err(format!(
            "{} {}: o efeito não é o da escolha declarada",
            membro.aggregate_type, membro.aggregate_id
        ));
    }
    Ok(())
}

// ── codec ─────────────────────────────────────────────────────────────────

pub fn ler(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    connection
        .query_row(
            "SELECT universe_id, certificate FROM conflict_resolutions WHERE conflict_key = ?1",
            [id],
            |row| {
                Ok(EstadoDoAgregado {
                    universe_id: row.get(0)?,
                    payload: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(erro)
}

pub fn validar(payload: &str) -> DatabaseCommandResult<Option<String>> {
    let certificado = Certificado::ler(payload).map_err(|motivo| {
        DatabaseCommandError::storage(format!("A resolução não pode ser emitida: {motivo}."))
    })?;
    if certificado.chave_recalculada() != certificado.conflict_key {
        return Err(DatabaseCommandError::storage(
            "A resolução não pode ser emitida: a chave não corresponde aos participantes.",
        ));
    }
    Ok(None)
}

pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    if envelope.operation == Operation::Delete {
        tx.execute(
            "DELETE FROM conflict_resolutions WHERE conflict_key = ?1",
            [&envelope.aggregate_id],
        )
        .map_err(erro)?;
        return Ok(());
    }
    let certificado = Certificado::ler(&envelope.payload).map_err(|motivo| {
        DatabaseCommandError::storage(format!("Resolução recebida ilegível: {motivo}."))
    })?;
    if certificado.conflict_key != envelope.aggregate_id {
        return Err(DatabaseCommandError::storage(
            "A resolução recebida declara outro conflito que o do envelope.",
        ));
    }
    tx.execute(
        "INSERT INTO conflict_resolutions (conflict_key, universe_id, kind, certificate)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(conflict_key) DO UPDATE SET
            universe_id = excluded.universe_id,
            kind = excluded.kind,
            certificate = excluded.certificate",
        rusqlite::params![
            &envelope.aggregate_id,
            &envelope.universe_id,
            &certificado.kind,
            &envelope.payload
        ],
    )
    .map_err(erro)?;
    Ok(())
}

pub fn enumerar(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    let mut consulta = connection
        .prepare("SELECT conflict_key FROM conflict_resolutions ORDER BY conflict_key")
        .map_err(erro)?;
    let ids = consulta
        .query_map([], |row| row.get(0))
        .map_err(erro)?
        .collect::<Result<Vec<String>, _>>()
        .map_err(erro)?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn participante(id: &str, rev: &str, op: &str) -> ParticipanteCanonico {
        ParticipanteCanonico {
            aggregate_type: "chapter".into(),
            aggregate_id: id.into(),
            revision: rev.into(),
            operation: op.into(),
        }
    }

    /// **A mesma decisão tem o mesmo payload**, qualquer que seja a ordem em que os efeitos foram
    /// listados — e só a forma canônica é lida de volta.
    #[test]
    fn o_certificado_e_canonico_e_so_a_forma_canonica_e_aceita() {
        let efeito = |id: &str| EfeitoCanonico {
            aggregate_type: "chapter".into(),
            aggregate_id: id.into(),
            operation: "upsert".into(),
            base_rev: "r1".into(),
            other_rev: "r2".into(),
            result_rev: "r3".into(),
        };
        let um = Certificado {
            conflict_key: "k".into(),
            kind: "concurrent".into(),
            participant_a: participante("c", "r1", "upsert"),
            participant_b: participante("c", "r2", "upsert"),
            choice: "a".into(),
            results: vec![efeito("b"), efeito("a")],
        };
        let mut outro = um.clone();
        outro.results.reverse();
        assert_eq!(um.canonico().expect("um"), outro.canonico().expect("outro"));
        let payload = um.canonico().expect("payload");
        assert_eq!(
            Certificado::ler(&payload).expect("ler").results[0].aggregate_id,
            "a"
        );
        let torto = payload.replacen("\"choice\":\"a\"", "\"choice\": \"a\"", 1);
        assert!(
            Certificado::ler(&torto).is_err(),
            "aceitou forma não canônica"
        );
        let a_mais = payload.replacen("{", "{\"extra\":1,", 1);
        assert!(
            Certificado::ler(&a_mais).is_err(),
            "aceitou campo desconhecido"
        );
    }
}
