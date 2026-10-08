//! **Identidade portátil de um conflito** (NH-079 B6, item 9).
//!
//! ## O problema que isto resolve
//!
//! Hoje um conflito é uma linha em `sync_divergences` com `id` vindo de `new_id()`. O mesmo
//! conflito, visto de dois aparelhos, vira **duas linhas sem relação nenhuma** — e as colunas que
//! descrevem os lados são `local_rev` e `remote_rev`, que trocam de papel conforme quem olha.
//!
//! Enquanto for assim, uma resolução não tem como atravessar o store-and-forward: o aparelho que
//! recebe "o conflito X foi decidido assim" não consegue dizer qual conflito dele é o X.
//!
//! ## Participantes canônicos, não perspectiva
//!
//! ```text
//! ConflictParticipant { aggregateType, aggregateId, revision, operation }
//! participants = sort(participantA, participantB)
//! conflictKey  = SHA256(domainSeparator + conflictKind + canonical(participants))
//! ```
//!
//! A ordenação é o que faz os dois aparelhos chegarem à mesma chave: cada um enxerga um lado como
//! "seu", e ordenar apaga essa diferença.
//!
//! **Por que não `hash(type, id, sort(revA, revB))`.** Essa fórmula parece bastar e não basta:
//! ela assume que os dois lados do conflito são o mesmo agregado. `tag_name_conflict` não é. Lá,
//! o PC registra `aggregateId = T2` com `relatedAggregateId = T1`, e o Android registra
//! exatamente o contrário — a mesma colisão produziria duas chaves diferentes. Só tratando os
//! **dois participantes como iguais em estrutura** a canonicalização funciona para conflito entre
//! agregados diferentes.
//!
//! ## Separação de responsabilidades
//!
//! ```text
//! sync_divergences     índice/estado LOCAL do conflito. Guarda também a perspectiva
//!                      (local_rev/remote_rev), que o resolvedor precisa operacionalmente.
//! conflict_resolution  fato causal replicável (etapa F). É a fonte de verdade distribuída.
//! ```
//!
//! Um `UPDATE sync_divergences SET resolved_at = …` **nunca** será a fonte de verdade entre
//! aparelhos: ele não viaja, não é assinado e não diz quem decidiu. Ver a seção 9 da matriz.

use sha2::{Digest, Sha256};

/// O separador de domínio, **versionado**. Mudar a forma de canonicalizar exige mudar isto junto,
/// senão duas versões do app calculariam chaves diferentes para o mesmo conflito e cada uma acharia
/// que a decisão da outra é de um conflito desconhecido.
pub const SEPARADOR_DE_DOMINIO: &str = "narrahub-conflict-v1";

/// Um lado do conflito, descrito sem perspectiva.
///
/// `revision` é a revisão **real** daquele lado, não "a minha" ou "a dele". `operation` entra
/// porque exclusão contra edição é um conflito diferente de edição contra edição, mesmo entre as
/// mesmas revisões.
///
/// **Não derive `Ord` aqui.** A ordem entre participantes é a da codificação canônica
/// ([`ordenar`]), e só ela. Duas noções de ordem — a dos campos e a do texto — deixariam a chave
/// calculada por uma e a coluna gravada pela outra, e elas podem discordar por causa dos prefixos
/// de tamanho (`13:fx23-rev-viva` contra `14:fx23-rev-morta` compara o `3` com o `4` antes de
/// chegar ao conteúdo). Com uma noção só, `participant_a < participant_b` vira invariante
/// verificável direto no SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictParticipant {
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub revision: String,
    pub operation: String,
}

impl ConflictParticipant {
    pub fn new(
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        revision: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        Self {
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            revision: revision.into(),
            operation: operation.into(),
        }
    }

    /// Como o participante entra na chave e na coluna: campos com o tamanho na frente.
    ///
    /// **Comprimento em vez de separador.** Um `a:b:c` qualquer vira ambíguo no dia em que um dos
    /// valores contiver `:` — e `entityType` é texto livre do escritor. Com o tamanho declarado,
    /// nenhuma combinação de conteúdo produz a mesma codificação que outra.
    pub fn canonico(&self) -> String {
        let mut saida = String::new();
        for campo in [
            self.aggregate_type.as_str(),
            self.aggregate_id.as_str(),
            self.revision.as_str(),
            self.operation.as_str(),
        ] {
            saida.push_str(&format!("{}:{}", campo.len(), campo));
        }
        saida
    }
}

/// A chave portátil do conflito. **A mesma nos dois aparelhos**, qualquer que seja quem detectou.
///
/// Ela deriva **exclusivamente** da representação portátil: tipo do conflito e os dois
/// participantes ordenados. Não entra perspectiva (`local`/`remote`), não entra relógio, não entra
/// `device_id` nem papel de quem detectou — qualquer um desses faria dois aparelhos calcularem
/// chaves diferentes para o mesmo conflito, que é exatamente o que ela existe para impedir.
pub fn chave_do_conflito(
    tipo_de_conflito: &str,
    um: &ConflictParticipant,
    outro: &ConflictParticipant,
) -> String {
    let (primeiro, segundo) = ordenar(um, outro);
    let mut hasher = Sha256::new();
    hasher.update(SEPARADOR_DE_DOMINIO.as_bytes());
    for campo in [tipo_de_conflito, primeiro.as_str(), segundo.as_str()] {
        hasher.update((campo.len() as u64).to_be_bytes());
        hasher.update(campo.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Os dois participantes em ordem canônica, já codificados. É o que vai ao hash e às colunas.
///
/// **A comparação é byte a byte.** `Ord for str` em Rust compara bytes, e a coluna no SQLite é
/// `COLLATE BINARY` explícito. As duas pontas precisam concordar: se um lado ordenasse por
/// caractere e o outro por byte, dois aparelhos com acento no `entityType` gravariam a mesma dupla
/// em ordens diferentes, e a chave — que sai desta mesma ordenação — divergiria junto.
pub fn ordenar(um: &ConflictParticipant, outro: &ConflictParticipant) -> (String, String) {
    let (a, b) = (um.canonico(), outro.canonico());
    if a.as_bytes() <= b.as_bytes() {
        (a, b)
    } else {
        (b, a)
    }
}

/// Identidade composta sem ambiguidade, para agregados cuja identidade é uma tupla.
///
/// `entity_template_set` é `(universeId, entityType)`, e `entityType` é texto que o escritor
/// digitou: pode conter `:`, `|`, o que for. Concatenar com delimitador criaria duas tuplas
/// diferentes com a mesma identidade — e, num agregado que a gênese vai materializar, isso
/// fundiria dois conjuntos de modelos de ficha sem ninguém perceber.
pub fn identidade_composta(partes: &[&str]) -> String {
    partes
        .iter()
        .map(|parte| format!("{}:{}", parte.len(), parte))
        .collect()
}

/// A volta de [`identidade_composta`].
pub fn partes_da_identidade(identidade: &str) -> Option<Vec<String>> {
    let mut partes = Vec::new();
    let mut resto = identidade;
    while !resto.is_empty() {
        let (tamanho, depois) = resto.split_once(':')?;
        let tamanho: usize = tamanho.parse().ok()?;
        if depois.len() < tamanho {
            return None;
        }
        // Fatiar por bytes quebraria em caractere multibyte; o tamanho é em bytes, como foi escrito.
        let valor = depois.get(..tamanho)?;
        partes.push(valor.to_string());
        resto = &depois[tamanho..];
    }
    Some(partes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(id: &str, rev: &str) -> ConflictParticipant {
        ConflictParticipant::new("content_tag", id, rev, "upsert")
    }

    /// **A ordem em que o conflito foi visto não entra na chave.**
    ///
    /// A detecta rev1 × rev2; B detecta rev2 × rev1. É o mesmo conflito.
    #[test]
    fn a_mesma_dupla_em_ordens_diferentes_da_a_mesma_chave() {
        let um = ConflictParticipant::new("chapter", "c1", "rev1", "upsert");
        let outro = ConflictParticipant::new("chapter", "c1", "rev2", "upsert");

        assert_eq!(
            chave_do_conflito("concurrent", &um, &outro),
            chave_do_conflito("concurrent", &outro, &um)
        );
    }

    /// **O caso que derrubou a fórmula ingênua.**
    ///
    /// No PC, a tag que chegou é T2 e a daqui é T1. No Android é o contrário. Uma chave feita de
    /// `(aggregateType, aggregateId, revisões)` usaria `aggregateId` diferente em cada lado e
    /// produziria duas chaves para a mesma colisão.
    #[test]
    fn tag_homonima_vista_dos_dois_lados_da_a_mesma_chave() {
        let t1 = tag("T1", "rev-de-t1");
        let t2 = tag("T2", "rev-de-t2");

        let no_pc = chave_do_conflito("tag_name_conflict", &t2, &t1);
        let no_android = chave_do_conflito("tag_name_conflict", &t1, &t2);

        assert_eq!(no_pc, no_android);
    }

    /// Conflitos de tipos diferentes com os mesmos participantes não se confundem.
    #[test]
    fn o_tipo_do_conflito_entra_na_chave() {
        let um = ConflictParticipant::new("chapter", "c1", "rev1", "upsert");
        let outro = ConflictParticipant::new("chapter", "c1", "rev2", "delete");

        assert_ne!(
            chave_do_conflito("concurrent", &um, &outro),
            chave_do_conflito("parent_deletion_blocked", &um, &outro)
        );
    }

    /// A operação distingue "exclusão contra edição" de "edição contra edição".
    #[test]
    fn a_operacao_do_participante_entra_na_chave() {
        let edicao = ConflictParticipant::new("chapter", "c1", "rev1", "upsert");
        let exclusao = ConflictParticipant::new("chapter", "c1", "rev1", "delete");
        let outro = ConflictParticipant::new("chapter", "c1", "rev2", "upsert");

        assert_ne!(
            chave_do_conflito("concurrent", &edicao, &outro),
            chave_do_conflito("concurrent", &exclusao, &outro)
        );
    }

    /// **Comprimento em vez de delimitador.** Duas tuplas diferentes nunca colidem, mesmo quando o
    /// conteúdo carrega o caractere que seria o separador.
    #[test]
    fn identidade_composta_nao_e_ambigua_com_dois_pontos_no_valor() {
        let uma = identidade_composta(&["u1", "Tipo:Estranho"]);
        let outra = identidade_composta(&["u1:Tipo", "Estranho"]);
        assert_ne!(uma, outra);

        assert_eq!(
            partes_da_identidade(&uma).expect("desmontar"),
            vec!["u1".to_string(), "Tipo:Estranho".to_string()]
        );
        assert_eq!(
            partes_da_identidade(&outra).expect("desmontar"),
            vec!["u1:Tipo".to_string(), "Estranho".to_string()]
        );
    }

    #[test]
    fn identidade_composta_sobrevive_a_acento_e_a_vazio() {
        for partes in [
            vec!["u1", "Personagem"],
            vec!["u1", "Lugar Sagrado"],
            vec!["u1", ""],
            vec!["u1", "Órfã"],
        ] {
            let identidade = identidade_composta(&partes);
            assert_eq!(
                partes_da_identidade(&identidade).expect("desmontar"),
                partes.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
                "não fez a volta: {identidade}"
            );
        }
    }

    /// Identidade torta não vira tupla por acidente.
    #[test]
    fn identidade_invalida_e_recusada_em_vez_de_adivinhada() {
        for torta in ["", "abc", "5:oi", "2:ok3:", "x:oi"] {
            if torta.is_empty() {
                assert_eq!(partes_da_identidade(torta), Some(Vec::new()));
                continue;
            }
            assert!(
                partes_da_identidade(torta).is_none(),
                "'{torta}' foi aceita como identidade"
            );
        }
    }
}
