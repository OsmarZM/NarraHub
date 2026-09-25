//! **Autoral × efêmero** (B5): o que está no banco e o que é estado de tela.
//!
//! ```text
//! sincroniza      nós, arestas, posições persistidas, tags, marcações, anexos
//! não sincroniza  zoom, pan, viewport, seleção, hover, painel aberto
//! ```
//!
//! O segundo grupo **não está no banco**, e é por isso que ele não sincroniza. A regra não é "a
//! gente combinou de não mandar": é que ele vive na memória do componente Angular e morre com a
//! tela. Uma regra combinada seria esquecida no dia em que alguém achasse prático guardar o zoom
//! numa coluna "só para lembrar onde o escritor estava" — e o zoom do notebook começaria a mexer o
//! canvas do celular.
//!
//! Este módulo põe a regra onde ela não pode ser esquecida: em dois gates sobre o **schema real**.
//!
//! ```text
//! 1  nenhuma coluna com cara de estado de tela, em tabela nenhuma, sem decisão escrita
//! 2  cada coluna das tabelas da B5 classificada: está num payload, ou é local com motivo
//! ```
//!
//! O gate 2 é o que morde de verdade. Acrescentar uma coluna a `canvas_nodes` passa a exigir
//! dizer o que ela é — e "faz parte do canvas_node" é uma frase que alguém precisa escrever de
//! propósito, não um silêncio que passa despercebido.

/// Onde o valor da coluna aparece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    /// Entra no payload canônico deste agregado.
    Payload(&'static str),
    /// Fica aqui. Relógio local, número físico, coluna legada — nunca viaja.
    Local,
}

#[derive(Debug, Clone, Copy)]
pub struct ClassificacaoDeColuna {
    pub tabela: &'static str,
    pub coluna: &'static str,
    pub destino: Destino,
    pub motivo: &'static str,
}

macro_rules! coluna {
    ($tabela:literal . $coluna:literal => $agregado:literal) => {
        ClassificacaoDeColuna {
            tabela: $tabela,
            coluna: $coluna,
            destino: Destino::Payload($agregado),
            motivo: "",
        }
    };
    ($tabela:literal . $coluna:literal => local, $motivo:literal) => {
        ClassificacaoDeColuna {
            tabela: $tabela,
            coluna: $coluna,
            destino: Destino::Local,
            motivo: $motivo,
        }
    };
}

/// As tabelas que a B5 passou a sincronizar, coluna por coluna.
pub const COLUNAS: &[ClassificacaoDeColuna] = &[
    // ── canvas_nodes: conteúdo e posição são agregados DIFERENTES ─────────
    coluna!("canvas_nodes"."id" => "canvas_node"),
    coluna!("canvas_nodes"."universe_id" => "canvas_node"),
    coluna!("canvas_nodes"."kind" => "canvas_node"),
    coluna!("canvas_nodes"."text" => "canvas_node"),
    coluna!("canvas_nodes"."color" => "canvas_node"),
    coluna!("canvas_nodes"."image_blob_hash" => "canvas_node"),
    coluna!("canvas_nodes"."image_mime_type" => "canvas_node"),
    coluna!("canvas_nodes"."position_x" => "canvas_node_position"),
    coluna!("canvas_nodes"."position_y" => "canvas_node_position"),
    coluna!("canvas_nodes"."image" => local, "coluna legada do ADR 0010; vazia depois da migração"),
    coluna!("canvas_nodes"."created_at" => local, "relógio local"),
    coluna!("canvas_nodes"."updated_at" => local, "relógio local"),
    // ── canvas_edges ──────────────────────────────────────────────────────
    coluna!("canvas_edges"."id" => "canvas_edge"),
    coluna!("canvas_edges"."universe_id" => "canvas_edge"),
    coluna!("canvas_edges"."source_kind" => "canvas_edge"),
    coluna!("canvas_edges"."source_id" => "canvas_edge"),
    coluna!("canvas_edges"."target_kind" => "canvas_edge"),
    coluna!("canvas_edges"."target_id" => "canvas_edge"),
    coluna!("canvas_edges"."label" => "canvas_edge"),
    coluna!("canvas_edges"."created_at" => local, "relógio local"),
    // ── canvas_entity_positions (B3) ──────────────────────────────────────
    coluna!("canvas_entity_positions"."universe_id" => "canvas_entity_position"),
    coluna!("canvas_entity_positions"."entity_id" => "canvas_entity_position"),
    coluna!("canvas_entity_positions"."position_x" => "canvas_entity_position"),
    coluna!("canvas_entity_positions"."position_y" => "canvas_entity_position"),
    coluna!("canvas_entity_positions"."updated_at" => local, "relógio local"),
    // ── content_tags ──────────────────────────────────────────────────────
    coluna!("content_tags"."id" => "content_tag"),
    coluna!("content_tags"."universe_id" => "content_tag"),
    coluna!("content_tags"."name" => "content_tag"),
    coluna!("content_tags"."color" => "content_tag"),
    coluna!("content_tags"."created_at" => local, "relógio local"),
    // ── content_tag_assignments: a identidade É o conteúdo ────────────────
    coluna!("content_tag_assignments"."tag_id" => "tag_assignment"),
    coluna!("content_tag_assignments"."owner_type" => "tag_assignment"),
    coluna!("content_tag_assignments"."owner_id" => "tag_assignment"),
    coluna!("content_tag_assignments"."id" => local,
        "id aleatório da linha. A identidade causal é tagId:ownerType:ownerId desde a B2: \
         marcar a mesma tag no mesmo dono nos dois aparelhos é a MESMA marcação"),
    coluna!("content_tag_assignments"."created_at" => local, "relógio local"),
    // ── attachments: payload definitivo da B5 ─────────────────────────────
    coluna!("attachments"."id" => "attachment"),
    coluna!("attachments"."universe_id" => "attachment"),
    coluna!("attachments"."owner_type" => "attachment"),
    coluna!("attachments"."owner_id" => "attachment"),
    coluna!("attachments"."blob_hash" => "attachment"),
    coluna!("attachments"."mime_type" => "attachment"),
    coluna!("attachments"."caption" => "attachment"),
    coluna!("attachments"."data_url" => local,
        "coluna legada do ADR 0010; o que viaja é blob_hash, e byte nenhum entra no log"),
    coluna!("attachments"."sort_order" => local,
        "número FÍSICO local (MAX+1 no INSERT). O que converge é `attachment_position`, um \
         agregado por anexo (B2.2) — esta coluna é a materialização dele, não conteúdo do anexo"),
    coluna!("attachments"."created_at" => local, "relógio local"),
];

/// Coluna com cara de estado de tela que exista mesmo assim, com o motivo. **Vazia de propósito.**
pub const ESTADO_DE_TELA_ACEITO: &[(&str, &str, &str)] = &[];

/// Palavras que denunciam estado de viewport. Comparadas como palavra inteira: `expanded` contém
/// "pan" por acidente, e um gate que casasse substring viraria ruído no primeiro dia.
const PALAVRAS_DE_TELA: &[&str] = &[
    "zoom",
    "pan",
    "viewport",
    "scroll",
    "selection",
    "selected",
    "hover",
    "collapsed",
    "expanded",
    "panel",
    "sidebar",
    "camera",
    "focused",
];

pub fn parece_estado_de_tela(coluna: &str) -> bool {
    coluna
        .split('_')
        .any(|palavra| PALAVRAS_DE_TELA.contains(&palavra))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::collections::BTreeSet;

    fn colunas_do_schema(connection: &rusqlite::Connection) -> Vec<(String, String)> {
        let tabelas: Vec<String> = connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .expect("tabelas")
            .query_map([], |row| row.get(0))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("nomes");
        let mut todas = Vec::new();
        for tabela in tabelas {
            let mut consulta = connection
                .prepare(&format!("PRAGMA table_info({tabela})"))
                .expect("colunas");
            let colunas: Vec<String> = consulta
                .query_map([], |row| row.get(1))
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("nomes");
            for coluna in colunas {
                todas.push((tabela.clone(), coluna));
            }
        }
        todas
    }

    /// **Nenhuma tabela guarda estado de viewport.**
    ///
    /// Vale para o banco inteiro, não só para o canvas: o dia em que alguém guardar o zoom numa
    /// coluna, este gate reprova antes de a coluna chegar a um aparelho de verdade.
    #[test]
    fn nenhuma_coluna_do_banco_guarda_estado_de_tela() {
        let banco = TemporaryDatabase::new();
        let aceitas: BTreeSet<(&str, &str)> = ESTADO_DE_TELA_ACEITO
            .iter()
            .map(|(tabela, coluna, _)| (*tabela, *coluna))
            .collect();

        for (tabela, coluna) in colunas_do_schema(&banco.connection()) {
            if !parece_estado_de_tela(&coluna) {
                continue;
            }
            assert!(
                aceitas.contains(&(tabela.as_str(), coluna.as_str())),
                "{tabela}.{coluna} parece estado de tela (zoom, pan, seleção, painel…). \
                 Estado de viewport vive na memória do componente e morre com a tela: guardá-lo \
                 no banco faz o zoom de um aparelho mexer o canvas do outro. Se ele precisa mesmo \
                 existir aqui, declare em ESTADO_DE_TELA_ACEITO com o motivo."
            );
        }
    }

    /// **Toda coluna das tabelas da B5 tem destino declarado.**
    #[test]
    fn coluna_nova_nas_tabelas_da_b5_precisa_de_decisao() {
        let banco = TemporaryDatabase::new();
        let nossas: BTreeSet<&str> = COLUNAS.iter().map(|c| c.tabela).collect();
        let classificadas: BTreeSet<(&str, &str)> =
            COLUNAS.iter().map(|c| (c.tabela, c.coluna)).collect();

        let do_schema: BTreeSet<(String, String)> = colunas_do_schema(&banco.connection())
            .into_iter()
            .filter(|(tabela, _)| nossas.contains(tabela.as_str()))
            .collect();

        for (tabela, coluna) in &do_schema {
            assert!(
                classificadas.contains(&(tabela.as_str(), coluna.as_str())),
                "{tabela}.{coluna} não tem destino declarado. Ela entra no payload de algum \
                 agregado, ou é local? Sem a resposta, o padrão vira o silêncio — e o silêncio \
                 aqui significa 'não sincroniza e ninguém percebeu'."
            );
        }
        for (tabela, coluna) in &classificadas {
            assert!(
                do_schema.contains(&(tabela.to_string(), coluna.to_string())),
                "{tabela}.{coluna} está classificada e não existe mais no schema"
            );
        }
    }

    /// Todo agregado citado por uma coluna é um agregado que a fronteira conhece.
    #[test]
    fn os_agregados_citados_existem() {
        for classificacao in COLUNAS {
            if let Destino::Payload(agregado) = classificacao.destino {
                assert!(
                    super::super::coberto(agregado),
                    "{}.{} aponta para o agregado '{agregado}', que a fronteira não conhece",
                    classificacao.tabela,
                    classificacao.coluna
                );
            } else {
                assert!(
                    !classificacao.motivo.is_empty(),
                    "{}.{} é local sem motivo escrito",
                    classificacao.tabela,
                    classificacao.coluna
                );
            }
        }
    }

    /// A comparação é por palavra inteira: `expanded` contém "pan" e não pode virar alarme falso.
    #[test]
    fn palavra_inteira_em_vez_de_substring() {
        assert!(parece_estado_de_tela("zoom_level"));
        assert!(parece_estado_de_tela("last_viewport"));
        assert!(parece_estado_de_tela("expanded"));
        // "pan" aparece dentro destas por acidente, e nenhuma é estado de tela.
        assert!(!parece_estado_de_tela("companion_id"));
        assert!(!parece_estado_de_tela("expansion_rate"));
        assert!(!parece_estado_de_tela("panorama"));
    }
}
