//! **As referências de blob que um evento carrega** (etapa D, item 12).
//!
//! O blob é dependência de materialização, como o pai de um capítulo: um evento que cita uma
//! imagem que este aparelho não tem não pode virar estado. Para a drenagem saber disso, ela precisa
//! de uma resposta única para "que blobs este payload referencia?" — e é esta.
//!
//! ```text
//! campo direto   a propriedade do payload É o hash         capa, imagem, anexo
//! documento      a propriedade é um HTML com 0..N <img>    texto do capítulo
//! ```
//!
//! ## Uma tabela, e não uma função por codec
//!
//! As superfícies binárias do banco já estão num catálogo só (`blob_surfaces::CATALOGO`, ADR
//! 0010). Esta tabela é a projeção dele no formato do payload: para cada superfície que viaja em
//! evento, qual agregado e qual propriedade. O gate ao lado do catálogo reprova superfície nova que
//! não esteja aqui nem em [`FORA_DO_EVENTO`] — a mesma disciplina que já impediu uma coluna de
//! guardar bytes sem ninguém decidir.
//!
//! ## Tolerante de propósito, e só aqui
//!
//! Payload que não abre, hash que não é SHA-256 canônico, documento que o parser recusa: nada disso
//! vira referência. Não é esta função que julga o payload — é o `aplicar` do codec, que recusa cada
//! um desses casos com a mensagem certa. Se a extração inventasse uma referência a partir de lixo,
//! o evento esperaria para sempre por um blob que não existe, em vez de falhar alto.

use std::collections::BTreeSet;

use crate::domain::sync::{EventEnvelope, Operation};
use crate::infrastructure::blob_document::hashes_do_documento;
use crate::infrastructure::blob_store::e_hash_canonico;

/// Como a referência mora na propriedade do payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormaNoPayload {
    /// A propriedade é o hash, ou vazia.
    Hash,
    /// A propriedade é um documento HTML; os hashes estão nos `<img>`.
    Documento,
}

/// Uma propriedade de payload que referencia blob.
#[derive(Debug, Clone, Copy)]
pub struct CampoDeBlob {
    pub tipo: &'static str,
    /// A superfície do ADR 0010 de onde o valor sai — o gate confere contra o catálogo.
    pub tabela: &'static str,
    pub campo: &'static str,
    pub forma: FormaNoPayload,
}

/// Toda propriedade de payload de evento que referencia blob.
pub const CAMPOS_DE_BLOB: &[CampoDeBlob] = &[
    CampoDeBlob {
        tipo: "attachment",
        tabela: "attachments",
        campo: "blobHash",
        forma: FormaNoPayload::Hash,
    },
    CampoDeBlob {
        tipo: "universe",
        tabela: "universes",
        campo: "coverBlobHash",
        forma: FormaNoPayload::Hash,
    },
    CampoDeBlob {
        tipo: "entity",
        tabela: "entities",
        campo: "imageBlobHash",
        forma: FormaNoPayload::Hash,
    },
    CampoDeBlob {
        tipo: "canvas_node",
        tabela: "canvas_nodes",
        campo: "imageBlobHash",
        forma: FormaNoPayload::Hash,
    },
    CampoDeBlob {
        tipo: "book",
        tabela: "books",
        campo: "coverBlobHash",
        forma: FormaNoPayload::Hash,
    },
    CampoDeBlob {
        tipo: "planning_item",
        tabela: "planning_items",
        campo: "imageBlobHash",
        forma: FormaNoPayload::Hash,
    },
    CampoDeBlob {
        tipo: "chapter",
        tabela: "chapters",
        campo: "content",
        forma: FormaNoPayload::Documento,
    },
];

/// Superfícies do ADR 0010 que **não** viajam em evento, com o motivo.
pub const FORA_DO_EVENTO: &[(&str, &str)] = &[
    (
        "chapter_revisions",
        "histórico local escrito pelo gatilho trg_chapter_revision; não é agregado do Sync V2",
    ),
    (
        "sync_conflicts",
        "conflito do protocolo V1, local a este aparelho",
    ),
    (
        "collaboration_contributions",
        "contribuição de convidado, local a este aparelho até ser aceita como edição",
    ),
];

/// Os blobs que este evento precisa ter aqui para virar estado.
///
/// Exclusão não referencia nada: o que ela materializa é a ausência.
pub fn blobs_do_evento(envelope: &EventEnvelope) -> BTreeSet<String> {
    if envelope.operation != Operation::Upsert {
        return BTreeSet::new();
    }
    blobs_do_payload(&envelope.aggregate_type, &envelope.payload)
}

/// Os blobs que um payload canônico daquele tipo referencia.
pub fn blobs_do_payload(tipo: &str, payload: &str) -> BTreeSet<String> {
    let mut hashes = BTreeSet::new();
    let campos: Vec<&CampoDeBlob> = CAMPOS_DE_BLOB.iter().filter(|c| c.tipo == tipo).collect();
    if campos.is_empty() {
        return hashes;
    }
    let Ok(valor) = serde_json::from_str::<serde_json::Value>(payload) else {
        return hashes;
    };
    for campo in campos {
        let Some(texto) = valor.get(campo.campo).and_then(|v| v.as_str()) else {
            continue;
        };
        match campo.forma {
            FormaNoPayload::Hash => {
                if e_hash_canonico(texto) {
                    hashes.insert(texto.to_string());
                }
            }
            FormaNoPayload::Documento => {
                if let Ok(encontrados) = hashes_do_documento(texto) {
                    hashes.extend(encontrados);
                }
            }
        }
    }
    hashes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::blob_store::hash_dos_bytes;
    use crate::infrastructure::sqlite::blob_surfaces::{Forma, CATALOGO};

    /// **Cobertura: toda superfície binária tem destino decidido.**
    ///
    /// Superfície nova no catálogo do ADR 0010 que não esteja nem aqui nem em `FORA_DO_EVENTO`
    /// reprova. Sem isto, um agregado novo com imagem aplicaria sem esperar pelo arquivo — que é
    /// exatamente o defeito da etapa D, item 12.
    #[test]
    fn toda_superficie_binaria_esta_classificada() {
        for superficie in CATALOGO {
            let no_evento = CAMPOS_DE_BLOB
                .iter()
                .filter(|c| c.tabela == superficie.tabela)
                .count();
            let fora = FORA_DO_EVENTO
                .iter()
                .filter(|(tabela, _)| *tabela == superficie.tabela)
                .count();
            assert_eq!(
                no_evento + fora,
                1,
                "a superfície {} ({}) precisa estar em exatamente um dos dois lugares",
                superficie.numero,
                superficie.tabela
            );
            if no_evento == 1 {
                let campo = CAMPOS_DE_BLOB
                    .iter()
                    .find(|c| c.tabela == superficie.tabela)
                    .expect("campo");
                let esperada = match superficie.forma {
                    Forma::CampoDireto => FormaNoPayload::Hash,
                    Forma::DocumentoTiptap => FormaNoPayload::Documento,
                };
                assert_eq!(campo.forma, esperada, "{}", superficie.tabela);
                assert!(
                    super::super::coberto(campo.tipo),
                    "{} não é tipo coberto",
                    campo.tipo
                );
            }
        }
        for campo in CAMPOS_DE_BLOB {
            assert!(
                CATALOGO.iter().any(|s| s.tabela == campo.tabela),
                "{} não é superfície do ADR 0010",
                campo.tabela
            );
        }
    }

    #[test]
    fn extrai_hash_direto_documento_e_ignora_o_resto() {
        let a = hash_dos_bytes(b"a");
        let b = hash_dos_bytes(b"b");
        assert_eq!(
            blobs_do_payload("universe", &format!(r#"{{"coverBlobHash":"{a}"}}"#)),
            BTreeSet::from([a.clone()])
        );
        let documento = format!(
            "<p>x</p><img data-narrahub-blob=\\\"{a}\\\"><img data-narrahub-blob=\\\"{b}\\\">"
        );
        assert_eq!(
            blobs_do_payload("chapter", &format!(r#"{{"content":"{documento}"}}"#)),
            BTreeSet::from([a.clone(), b.clone()])
        );
        // Vazio, torto, payload que não abre, tipo sem blob: nada.
        assert!(blobs_do_payload("universe", r#"{"coverBlobHash":""}"#).is_empty());
        assert!(blobs_do_payload("universe", r#"{"coverBlobHash":"abc"}"#).is_empty());
        assert!(blobs_do_payload("universe", "não é json").is_empty());
        assert!(blobs_do_payload("story", &format!(r#"{{"coverBlobHash":"{a}"}}"#)).is_empty());
        // Hash escrito no resumo de um capítulo é prosa, não referência.
        assert!(blobs_do_payload("chapter", &format!(r#"{{"summary":"{a}"}}"#)).is_empty());
    }
}
