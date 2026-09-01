//! Mapa das invariantes de domínio para os testes que as provam.
//!
//! Este arquivo não contém regra: ele existe para responder, sem reler o core inteiro,
//! a pergunta "a invariante N está protegida?".
//!
//! O problema que ele resolve é específico. O core tem 176 testes e vários cobrem as
//! invariantes — mas sob nomes que não as citam. Sem mapa, ninguém consegue verificar a
//! cobertura, e **uma invariante que ninguém consegue verificar é indistinguível de uma
//! que ninguém implementou**. É assim que uma delas morre em silêncio numa refatoração:
//! o teste que a protegia é renomeado ou apagado por outro motivo, e a suíte segue verde.
//!
//! O mapa é dado, e o gate confere que ele não apodreceu: se um teste listado aqui deixar
//! de existir, a suíte reprova nomeando a invariante que ficou órfã.
//!
//! A lista das invariantes está em `docs/DOMAIN_INVARIANTS.md`.

/// Uma invariante e os testes que a provam.
///
/// Só existe em build de teste: o mapa é ferramenta de verificação, não regra de runtime.
/// Deixá-lo fora disso faria o `clippy` acusar código morto — com razão.
#[cfg(test)]
struct Cobertura {
    numero: u8,
    resumo: &'static str,
    testes: &'static [&'static str],
}

/// Onde cada invariante é provada hoje.
///
/// Um teste pode aparecer em mais de uma linha: `universo_inexistente_nao_deixa_entidade_pela_metade`
/// prova tanto que a entidade precisa de um universo existente (3) quanto que a operação
/// falha por inteiro (10). Isso é esperado — invariantes se cruzam.
#[cfg(test)]
const MAPA: &[Cobertura] = &[
    Cobertura {
        numero: 1,
        resumo: "Um Chapter pertence a exatamente um Book existente",
        testes: &[
            "healthy_database_satisfies_core_invariants",
            "capitulo_inexistente_avisa_em_vez_de_gravar_no_vazio",
            "excluir_historia_leva_livros_e_capitulos_junto",
        ],
    },
    Cobertura {
        numero: 2,
        resumo: "Book pertence a uma Story, e Story a um Universe existente",
        testes: &[
            "healthy_database_satisfies_core_invariants",
            "livro_de_historia_inexistente_e_recusado_pela_foreign_key",
            "excluir_universo_leva_junto_o_que_pendura_nele",
        ],
    },
    Cobertura {
        numero: 3,
        resumo: "Uma Entity pertence a exatamente um Universe existente",
        testes: &[
            "healthy_database_satisfies_core_invariants",
            "universo_inexistente_nao_deixa_entidade_pela_metade",
            "contagem_por_tipo_nao_vaza_entre_universos_na_listagem",
        ],
    },
    Cobertura {
        numero: 4,
        resumo: "Uma Relation liga duas entidades existentes do mesmo universo",
        testes: &[
            "relacao_com_entidade_inexistente_e_recusada_pela_foreign_key",
            "relation_cannot_cross_universe_without_health_failure",
            "invalid_cross_universe_relation_rolls_back_the_whole_card",
            "ponta_de_outro_universo_nao_e_reconhecida_como_existente",
        ],
    },
    Cobertura {
        numero: 5,
        resumo: "Excluir entidade não deixa relação nem menção quebrada",
        testes: &[
            "deleting_entity_preserves_referential_integrity",
            "excluir_entidade_leva_os_atributos_junto",
            "excluir_elemento_leva_as_ligacoes_e_nao_sobra_orfa",
            "ligacao_com_ponta_excluida_por_fora_nao_e_listada",
        ],
    },
    Cobertura {
        numero: 6,
        resumo: "Revisão ou proposta não substitui conteúdo canônico sem aprovação explícita",
        testes: &[
            "aprovar_aplica_a_mudanca_e_registra_no_historico",
            "recusar_nao_toca_no_universo_mas_marca_a_proposta",
            "aprovar_duas_vezes_nao_reaplica",
        ],
    },
    Cobertura {
        numero: 7,
        resumo: "Sessão compartilhada nunca escreve direto no conteúdo canônico",
        testes: &[
            "campo_fora_do_escopo_nao_grava_nada_e_a_proposta_segue_pendente",
            "proposta_para_item_ja_excluido_avisa_em_vez_de_marcar_aprovada",
            "campo_fora_da_lista_nao_tem_coluna_para_escrever",
        ],
    },
    // A invariante 8 — IA não altera conteúdo canônico antes da confirmação — não tem
    // teste no core Rust, e não deveria mesmo: ela é garantida pela ausência de caminho de
    // escrita no frontend. `AiService` não injeta store nem gateway nenhum, então não há
    // por onde ele gravar. Quem prova isso é `tests/frontend-boundaries.test.mjs`, e está
    // registrado em `docs/DOMAIN_INVARIANTS.md`. Listar aqui um teste que não existe seria
    // pior que admitir o limite.
    Cobertura {
        numero: 9,
        resumo: "Migration publicada nunca é alterada",
        testes: &[
            "modified_applied_migration_is_rejected_before_restore",
            "v14_canvas_statements_match_the_shipped_schema",
            "tipos_do_canvas_batem_com_o_check_da_migration_14",
        ],
    },
    Cobertura {
        numero: 10,
        resumo: "Operação de domínio falha por inteiro ou é confirmada por inteiro",
        testes: &[
            "card_save_is_atomic_and_relations_are_normalized",
            "reordenar_desfaz_tudo_quando_a_transacao_e_revertida",
            "criacao_monta_a_ficha_inteira_numa_transacao",
            "reordenar_com_lista_que_nao_bate_nao_grava_nada",
        ],
    },
    Cobertura {
        numero: 11,
        resumo: "IDs persistidos são estáveis; renomear não muda identidade",
        testes: &[
            "generates_distinct_ids",
            "insert_recusa_id_duplicado_como_conflito",
            "historico_resolve_o_nome_atual_da_entidade",
            "historico_cai_para_o_id_quando_o_alvo_foi_excluido",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    /// Nomes de toda função de teste do core, lidos da árvore de fontes.
    fn testes_existentes() -> HashSet<String> {
        fn percorrer(dir: &Path, encontrados: &mut HashSet<String>) {
            let Ok(entradas) = std::fs::read_dir(dir) else {
                return;
            };
            for entrada in entradas.flatten() {
                let caminho = entrada.path();
                if caminho.is_dir() {
                    percorrer(&caminho, encontrados);
                    continue;
                }
                if caminho.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                let Ok(fonte) = std::fs::read_to_string(&caminho) else {
                    continue;
                };
                let linhas: Vec<&str> = fonte.lines().collect();
                for (indice, linha) in linhas.iter().enumerate() {
                    if !linha.trim_start().starts_with("fn ") {
                        continue;
                    }
                    // Só conta como teste se houver `#[test]` logo acima, pulando atributos
                    // e comentários de doc que costumam ficar no meio.
                    let inicio = indice.saturating_sub(6);
                    let anterior_e_teste =
                        linhas[inicio..indice].iter().any(|l| l.trim() == "#[test]");
                    if !anterior_e_teste {
                        continue;
                    }
                    if let Some(nome) = linha
                        .trim_start()
                        .strip_prefix("fn ")
                        .and_then(|resto| resto.split('(').next())
                    {
                        encontrados.insert(nome.trim().to_string());
                    }
                }
            }
        }

        let mut encontrados = HashSet::new();
        percorrer(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut encontrados,
        );
        encontrados
    }

    /// O gate. Se um teste listado no mapa some ou é renomeado, a invariante que ele
    /// protegia fica órfã — e a suíte precisa dizer isso em vez de continuar verde.
    #[test]
    fn todo_teste_do_mapa_de_invariantes_existe() {
        let existentes = testes_existentes();
        assert!(
            existentes.len() > 100,
            "a varredura encontrou só {} testes; o leitor de fontes quebrou e este gate \
             passaria a aprovar qualquer coisa",
            existentes.len()
        );

        let mut orfas = Vec::new();
        for cobertura in MAPA {
            for teste in cobertura.testes {
                if !existentes.contains(*teste) {
                    orfas.push(format!(
                        "invariante {} ({}): o teste `{teste}` não existe mais",
                        cobertura.numero, cobertura.resumo
                    ));
                }
            }
        }

        assert!(
            orfas.is_empty(),
            "o mapa de invariantes ficou desatualizado:\n  {}\n\nSe o teste foi renomeado, \
             atualize o mapa. Se foi removido, a invariante ficou sem prova — e isso precisa \
             ser uma decisão consciente, não um efeito colateral.",
            orfas.join("\n  ")
        );
    }

    /// Nenhuma invariante pode entrar no mapa sem teste, e nenhuma pode sumir dele.
    #[test]
    fn o_mapa_cobre_as_invariantes_documentadas() {
        // A 8 é a exceção documentada: garantida por ausência de caminho de escrita no
        // frontend, provada em tests/frontend-boundaries.test.mjs.
        let esperadas: Vec<u8> = (1..=11).filter(|n| *n != 8).collect();
        let mapeadas: Vec<u8> = MAPA.iter().map(|c| c.numero).collect();
        assert_eq!(
            mapeadas, esperadas,
            "o mapa precisa cobrir as invariantes de docs/DOMAIN_INVARIANTS.md, em ordem"
        );

        for cobertura in MAPA {
            assert!(
                !cobertura.testes.is_empty(),
                "invariante {} entrou no mapa sem nenhum teste",
                cobertura.numero
            );
        }
    }
}
