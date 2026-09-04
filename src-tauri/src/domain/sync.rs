//! Sync V2 — envelope do evento e causalidade por revisão encadeada.
//!
//! ADR 0009, seções 10 e 11. Esta camada não conhece `rusqlite` nem `tauri`:
//! a regra de causalidade precisa ser testável sem banco, porque ela é o que
//! decide se duas edições são sequenciais ou concorrentes — e errar isso
//! apaga trabalho do escritor.
//!
//! O que **não** está aqui: alocação de `seq`, que depende do log e portanto
//! do banco; e a geração da assinatura Ed25519, que é a etapa 2.5. O campo já
//! existe no envelope, e a etapa 2.5 vem antes da 3 pelo motivo registrado na
//! seção 23 do ADR: `sync_events` é append-only, e a etapa 3 é a que começa a
//! gerar eventos reais.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Separador de domínio. Sem ele, o mesmo hash poderia ser produzido por
/// outra estrutura do sistema que por acaso concatenasse os mesmos bytes.
const REVISION_DOMAIN: &[u8] = b"narrahub.sync.v2.revision\x00";

/// Primeira revisão de um agregado. Não é "desconhecida" — é a raiz da
/// cadeia, e o único `base_rev` que legitimamente não aponta para nada.
pub const ROOT_REVISION: &str = "";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Upsert,
    Delete,
}

impl Operation {
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::Upsert => "upsert",
            Operation::Delete => "delete",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "upsert" => Some(Operation::Upsert),
            "delete" => Some(Operation::Delete),
            _ => None,
        }
    }
}

/// Qual coisa do universo o evento descreve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateRef {
    pub aggregate_type: String,
    pub aggregate_id: String,
}

impl AggregateRef {
    pub fn new(aggregate_type: impl Into<String>, aggregate_id: impl Into<String>) -> Self {
        Self {
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
        }
    }
}

/// O envelope da seção 10 do ADR.
///
/// `device_id` é a ORIGEM, não quem entregou: um relay repassa o envelope
/// inteiro sem se sobrescrever aqui (ADR 0009 §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub device_id: String,
    pub seq: i64,
    pub universe_id: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub operation: Operation,
    /// JSON opaco. **Nunca é reserializado**: `new_rev` é o hash destes
    /// bytes, e quem recebe recalcula sobre os bytes que chegaram. Um
    /// serializador no meio do caminho — reordenando chaves, normalizando
    /// espaço — produziria hash diferente do mesmo conteúdo, e o evento
    /// seria recusado como se tivesse sido adulterado. Coberto por
    /// `payload_atravessa_o_log_sem_reserializacao`.
    pub payload: String,
    pub base_rev: String,
    pub new_rev: String,
    /// Ed25519 da origem. Vazio **apenas durante a etapa 2**: a partir da
    /// etapa 2.5 todo evento local nasce assinado, e a etapa 3 — que liga o
    /// log às escritas reais — não pode chegar antes disso. `sync_events` é
    /// append-only, então evento nascido sem assinatura não pode ser assinado
    /// depois. A etapa 7 cuida da **verificação** de origem de terceiros.
    pub signature: String,
}

/// Calcula a revisão que uma mudança produz.
///
/// O ADR escreve `SHA-256(base_rev ‖ aggregate_id ‖ operation ‖ payload)`, e
/// a concatenação nua tem um problema clássico: `"ab" + "c"` e `"a" + "bc"`
/// produzem os mesmos bytes. Dois eventos diferentes poderiam colidir de
/// propósito ou por acidente. Cada campo entra precedido do seu tamanho, o
/// que torna a decomposição única.
pub fn compute_revision(
    base_rev: &str,
    aggregate: &AggregateRef,
    operation: Operation,
    payload: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(REVISION_DOMAIN);
    for campo in [
        base_rev,
        aggregate.aggregate_type.as_str(),
        aggregate.aggregate_id.as_str(),
        operation.as_str(),
        payload,
    ] {
        hasher.update((campo.len() as u64).to_be_bytes());
        hasher.update(campo.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// O que a revisão que chegou significa em relação ao que temos.
///
/// Esta é a decisão que o ADR 0009 §11 proíbe de tomar por relógio de
/// parede: `updated_at` maior não significa "veio depois **daquela**
/// versão", e o caso que importa — duas edições offline a partir da mesma
/// base — é exatamente o que ele não distingue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Causality {
    /// A origem partiu de onde estamos. Aplica direto.
    Sequential,
    /// Já aplicamos esta revisão. Repetição é normal em rede.
    AlreadyPresent,
    /// A origem partiu de uma revisão nossa que já foi superada: os dois
    /// lados editaram a partir da mesma base. **Ninguém ganha por ser mais
    /// novo** (ADR 0009 §16).
    Concurrent { base_rev: String },
    /// `base_rev` que não conhecemos. Não é conflito — é pedido de
    /// reconciliação daquele agregado. Tratar como concorrente inventaria uma
    /// divergência que talvez não exista.
    Unknown { base_rev: String },
}

/// O que sabemos localmente sobre um agregado.
#[derive(Debug, Clone, Default)]
pub struct AggregateHistory {
    /// Revisão atual. `None` quando o agregado ainda não existe aqui.
    pub current_rev: Option<String>,
    /// Revisões conhecidas daquele agregado, atual inclusive.
    pub known_revs: Vec<String>,
}

impl AggregateHistory {
    pub fn knows(&self, rev: &str) -> bool {
        rev == ROOT_REVISION || self.known_revs.iter().any(|conhecida| conhecida == rev)
    }
}

/// Classifica um evento que chegou contra a história local do agregado.
pub fn classify(historia: &AggregateHistory, base_rev: &str, new_rev: &str) -> Causality {
    if historia.knows(new_rev) {
        return Causality::AlreadyPresent;
    }

    match historia.current_rev.as_deref() {
        // O agregado não existe aqui. Só uma criação a partir da raiz é
        // sequencial; qualquer outra base descreve uma história que não temos.
        None => {
            if base_rev == ROOT_REVISION {
                Causality::Sequential
            } else {
                Causality::Unknown {
                    base_rev: base_rev.to_string(),
                }
            }
        }
        Some(atual) if atual == base_rev => Causality::Sequential,
        Some(_) => {
            if historia.knows(base_rev) {
                Causality::Concurrent {
                    base_rev: base_rev.to_string(),
                }
            } else {
                Causality::Unknown {
                    base_rev: base_rev.to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agregado() -> AggregateRef {
        AggregateRef::new("chapter", "cap-1")
    }

    #[test]
    fn a_revisao_e_deterministica() {
        let um = compute_revision("", &agregado(), Operation::Upsert, r#"{"t":"a"}"#);
        let outro = compute_revision("", &agregado(), Operation::Upsert, r#"{"t":"a"}"#);
        assert_eq!(
            um, outro,
            "o mesmo evento precisa produzir a mesma revisão em qualquer aparelho"
        );
    }

    #[test]
    fn a_revisao_depende_da_base() {
        // É isto que faz a cadeia ser cadeia: a mesma escrita a partir de
        // bases diferentes é uma revisão diferente. Sem isso, dois aparelhos
        // que escrevessem o mesmo texto sobre históricos distintos
        // pareceriam ter convergido, e a divergência sumiria em silêncio.
        let de_a = compute_revision("rev-a", &agregado(), Operation::Upsert, r#"{"t":"x"}"#);
        let de_b = compute_revision("rev-b", &agregado(), Operation::Upsert, r#"{"t":"x"}"#);
        assert_ne!(de_a, de_b);
    }

    /// Concatenação nua faria `"ab"+"c"` colidir com `"a"+"bc"`. Cada campo
    /// entra precedido do tamanho justamente para impedir isso.
    #[test]
    fn campos_diferentes_nao_colidem_por_concatenacao() {
        let um = compute_revision(
            "",
            &AggregateRef::new("chapter", "ab"),
            Operation::Upsert,
            "c",
        );
        let outro = compute_revision(
            "",
            &AggregateRef::new("chapter", "a"),
            Operation::Upsert,
            "bc",
        );
        assert_ne!(
            um, outro,
            "dois eventos distintos não podem compartilhar revisão por acidente de concatenação"
        );

        let tipo_movido = compute_revision(
            "",
            &AggregateRef::new("chapte", "rab"),
            Operation::Upsert,
            "c",
        );
        assert_ne!(um, tipo_movido);
    }

    #[test]
    fn upsert_e_delete_do_mesmo_estado_sao_revisoes_diferentes() {
        let escreve = compute_revision("r0", &agregado(), Operation::Upsert, "");
        let apaga = compute_revision("r0", &agregado(), Operation::Delete, "");
        assert_ne!(
            escreve, apaga,
            "apagar não pode produzir a mesma revisão que escrever"
        );
    }

    #[test]
    fn agregado_novo_so_aceita_criacao_a_partir_da_raiz() {
        let vazio = AggregateHistory::default();
        assert_eq!(classify(&vazio, ROOT_REVISION, "r1"), Causality::Sequential);
        assert_eq!(
            classify(&vazio, "r-desconhecida", "r2"),
            Causality::Unknown {
                base_rev: "r-desconhecida".into()
            },
            "sem o agregado aqui, uma base qualquer descreve história que não temos"
        );
    }

    #[test]
    fn partir_da_revisao_atual_e_sequencial() {
        let historia = AggregateHistory {
            current_rev: Some("r1".into()),
            known_revs: vec!["r0".into(), "r1".into()],
        };
        assert_eq!(classify(&historia, "r1", "r2"), Causality::Sequential);
    }

    /// O caso que o `updated_at` não distingue, e que este ADR existe para
    /// pegar: os dois editaram a partir de `r0`, offline.
    #[test]
    fn partir_de_uma_base_ja_superada_e_concorrencia() {
        let historia = AggregateHistory {
            current_rev: Some("r1-windows".into()),
            known_revs: vec!["r0".into(), "r1-windows".into()],
        };
        assert_eq!(
            classify(&historia, "r0", "r1-android"),
            Causality::Concurrent {
                base_rev: "r0".into()
            }
        );
    }

    #[test]
    fn revisao_ja_conhecida_nao_reaplica() {
        let historia = AggregateHistory {
            current_rev: Some("r2".into()),
            known_revs: vec!["r0".into(), "r1".into(), "r2".into()],
        };
        assert_eq!(classify(&historia, "r1", "r2"), Causality::AlreadyPresent);
        // Inclusive uma revisão antiga que já passou por aqui.
        assert_eq!(classify(&historia, "r0", "r1"), Causality::AlreadyPresent);
    }

    /// Base desconhecida não é conflito. Chamar de conflito inventaria uma
    /// divergência que pode não existir — o mais provável é que faltem
    /// eventos intermediários que ainda não chegaram.
    #[test]
    fn base_desconhecida_pede_reconciliacao_e_nao_conflito() {
        let historia = AggregateHistory {
            current_rev: Some("r1".into()),
            known_revs: vec!["r0".into(), "r1".into()],
        };
        assert_eq!(
            classify(&historia, "r-de-outro-aparelho", "r9"),
            Causality::Unknown {
                base_rev: "r-de-outro-aparelho".into()
            }
        );
    }
}
