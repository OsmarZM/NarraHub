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

O plano de evolução previa cinco testes nomeados (`chapter_cannot_reference_missing_book`,
`relation_cannot_reference_missing_or_foreign_universe_entity`,
`deleting_entity_preserves_domain_integrity`,
`collaboration_proposal_never_changes_canonical_content`,
`failed_domain_operation_rolls_back_every_write`).

**Nenhum deles existe com esse nome.** O que existe é melhor e pior ao mesmo tempo: há 167
testes no core Rust, e vários cobrem estas invariantes sob outros nomes — por exemplo
`relation_cannot_cross_universe_without_health_failure` e
`invalid_cross_universe_relation_rolls_back_the_whole_card` (invariante 4),
`card_save_is_atomic_and_relations_are_normalized` (invariante 10),
`campo_fora_do_escopo_nao_grava_nada_e_a_proposta_segue_pendente` (invariantes 6 e 7).

O que **não** existe é o mapa: nenhuma invariante desta lista aponta para o teste que a
prova, e nenhum teste diz qual invariante defende. Sem isso não dá para responder "a
invariante 5 está protegida?" sem reler o core inteiro — e uma invariante que ninguém
consegue verificar é indistinguível de uma que ninguém implementou.

Fechar essa lacuna é a tarefa `NH-007`. A regra até lá: **invariante nova nasce com o
teste que a prova, e o teste cita o número da invariante.**
