# Invariantes do domínio

Estas regras são **corrente**, não plano. Elas valiam antes das fases concluídas, valem
agora e continuam valendo depois do Sync V2 e do Context Engine. Moraram por muito tempo
dentro do plano de evolução, o que as fazia parecer trabalho futuro — não são.

Invariante é regra executável, não orientação de implementação. Comandos Rust, imports,
sync e colaboração respeitam o mesmo conjunto. Quando uma regra envolve mais de uma
escrita, a validação e a alteração acontecem **na mesma transação**.

1. Um `Chapter` pertence a exatamente um `Book` existente.
2. Um `Book` pertence a exatamente uma `Story`, e a `Story` pertence a exatamente um `Universe` existente.
3. Uma `Entity` pertence a exatamente um `Universe` existente.
4. Uma `Relation` referencia duas entidades existentes no mesmo universo da relação.
5. Excluir uma entidade nunca deixa relações ou menções quebradas; referências opcionais usam `NULL` explícito e referências obrigatórias são removidas na mesma operação.
6. Uma revisão ou proposta nunca substitui conteúdo canônico sem um comando explícito de aprovação.
7. Uma sessão de compartilhamento nunca escreve diretamente em conteúdo canônico; ela cria anotações ou propostas pendentes.
8. Uma resposta de IA nunca altera conteúdo canônico antes da confirmação do escritor.
9. Uma migration publicada nunca é alterada; correções de esquema são sempre migrations novas.
10. Uma operação de domínio falha por inteiro ou é confirmada por inteiro.
11. IDs persistidos são estáveis; renomear um item não altera sua identidade nem quebra referências.

## Por que isso não é só foreign key

As foreign keys do SQLite ajudam, mas não substituem invariante de domínio. As FKs
garantem que as duas pontas de uma relação existam; a regra de que **ambas pertencem ao
mesmo universo** nenhuma FK expressa — ela é do caso de uso.

Esse é o padrão a procurar quando alguém propuser "o banco já garante isso": pergunte
qual invariante o banco realmente garante, e qual só parece garantida.

## Onde elas vivem hoje

A validação pertence à camada `application/` do core Rust, junto da transação. Um
`#[tauri::command]` não valida invariante por conta própria — ele delega. Ver
[`ARCHITECTURE.md`](ARCHITECTURE.md).

## Cobertura de teste

Cada invariante aponta para os testes que a provam, e cada teste listado é conferido por um
gate. O mapa executável vive em `src-tauri/src/domain/invariant_coverage.rs`; a tabela abaixo
é a versão legível dele.

| # | Provada por |
| --- | --- |
| 1 | `healthy_database_satisfies_core_invariants`, `capitulo_inexistente_avisa_em_vez_de_gravar_no_vazio`, `excluir_historia_leva_livros_e_capitulos_junto` |
| 2 | `healthy_database_satisfies_core_invariants`, `livro_de_historia_inexistente_e_recusado_pela_foreign_key`, `excluir_universo_leva_junto_o_que_pendura_nele` |
| 3 | `healthy_database_satisfies_core_invariants`, `universo_inexistente_nao_deixa_entidade_pela_metade`, `contagem_por_tipo_nao_vaza_entre_universos_na_listagem` |
| 4 | `relacao_com_entidade_inexistente_e_recusada_pela_foreign_key`, `relation_cannot_cross_universe_without_health_failure`, `invalid_cross_universe_relation_rolls_back_the_whole_card`, `ponta_de_outro_universo_nao_e_reconhecida_como_existente` |
| 5 | `deleting_entity_preserves_referential_integrity`, `excluir_entidade_leva_os_atributos_junto`, `excluir_elemento_leva_as_ligacoes_e_nao_sobra_orfa`, `ligacao_com_ponta_excluida_por_fora_nao_e_listada` |
| 6 | `aprovar_aplica_a_mudanca_e_registra_no_historico`, `recusar_nao_toca_no_universo_mas_marca_a_proposta`, `aprovar_duas_vezes_nao_reaplica` |
| 7 | `campo_fora_do_escopo_nao_grava_nada_e_a_proposta_segue_pendente`, `proposta_para_item_ja_excluido_avisa_em_vez_de_marcar_aprovada`, `campo_fora_da_lista_nao_tem_coluna_para_escrever` |
| 8 | **Ausência de caminho de escrita.** Ver abaixo. |
| 9 | `modified_applied_migration_is_rejected_before_restore`, `v14_canvas_statements_match_the_shipped_schema`, `tipos_do_canvas_batem_com_o_check_da_migration_14` |
| 10 | `card_save_is_atomic_and_relations_are_normalized`, `reordenar_desfaz_tudo_quando_a_transacao_e_revertida`, `criacao_monta_a_ficha_inteira_numa_transacao`, `reordenar_com_lista_que_nao_bate_nao_grava_nada` |
| 11 | `generates_distinct_ids`, `insert_recusa_id_duplicado_como_conflito`, `historico_resolve_o_nome_atual_da_entidade`, `historico_cai_para_o_id_quando_o_alvo_foi_excluido` |

Um mesmo teste aparece em mais de uma linha porque invariantes se cruzam:
`universo_inexistente_nao_deixa_entidade_pela_metade` prova tanto que a entidade exige um
universo existente (3) quanto que a operação falha por inteiro (10).

### A invariante 8 é diferente das outras dez

Ela não tem teste no core Rust, e não deveria ter. **A IA não altera conteúdo canônico
porque não existe caminho por onde ela escreveria:** `AiService` não injeta store, gateway
nem `DatabaseService`. A garantia é estrutural, não comportamental.

O risco correspondente também é estrutural: o dia em que alguém injetar um store ali — por
conveniência, para "já salvar" — a garantia desaparece sem que nenhum outro teste perceba.
Por isso ela é travada em `tests/frontend-boundaries.test.mjs`, no teste
`invariante 8: a IA não tem por onde escrever conteúdo canônico`.

### Os gates que impedem o mapa de apodrecer

O problema que eles resolvem não é encontrar um bug hoje; é impedir que a cobertura envelheça
em silêncio. Um teste renomeado ou apagado por outro motivo deixaria uma invariante sem prova,
e a suíte seguiria verde.

| Gate | O que reprova |
| --- | --- |
| `todo_teste_do_mapa_de_invariantes_existe` | um teste listado no mapa deixou de existir |
| `o_mapa_cobre_as_invariantes_documentadas` | uma invariante ficou fora do mapa, ou entrou sem teste |
| `invariante 8: a IA não tem por onde escrever conteúdo canônico` | alguém deu ao `AiService` um caminho de escrita |

Os três foram verificados por mutação: renomear um teste mapeado, e injetar um store no
`AiService`, fazem a suíte reprovar nomeando o que quebrou.

**Regra daqui para frente:** invariante nova nasce com o teste que a prova e com a linha no
mapa. Se uma invariante ficar sem prova, que seja uma decisão consciente e registrada — não
um efeito colateral de refatoração.
