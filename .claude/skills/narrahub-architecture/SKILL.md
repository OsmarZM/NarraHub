---
name: narrahub-architecture
description: Guardas a arquitetura-alvo do NarraHub (monólito modular local-first, Angular → Gateway → Tauri → Rust → SQLite). Use SEMPRE antes de uma refatoração estrutural, antes de implementar ou avançar uma fase do docs/ARCHITECTURE_EVOLUTION_PLAN.md, ou ao revisar um diff que toca persistência, migrations, IA, sync ou compartilhamento — mesmo que o pedido não mencione "arquitetura" explicitamente (ex.: "cria uma tabela nova", "chama o SQL direto daqui", "adiciona um microserviço", "faz dual-write"). Não serve para implementar a feature em si, só para checar se o caminho escolhido respeita os limites do projeto.
---

# Arquitetura do NarraHub

O NarraHub é local-first: o SQLite (e os arquivos do dispositivo) são a fonte
canônica de verdade. Toda decisão de arquitetura existe para proteger isso —
nunca o contrário. Antes de aceitar um plano, releia mentalmente por que ele
existe, não só a regra em si.

Antes de decidir algo estrutural, releia os documentos reais — este SKILL.md
resume os pontos estáveis, mas o "Estado de implementação" de cada fase muda
a cada sessão:

- [`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) — visão geral das camadas.
- [`docs/ARCHITECTURE_EVOLUTION_PLAN.md`](../../../docs/ARCHITECTURE_EVOLUTION_PLAN.md) — o plano de fases, gates e o que já foi feito em cada uma.
- [`docs/ADR/`](../../../docs/ADR/) — decisões permanentes já tomadas (não reabra essas discussões sem um motivo novo).

## A camada-alvo (para onde tudo caminha)

```text
Angular UI (component)
   ↓
Feature Store (Signals)
   ↓
Typed Gateway (abstract class)
   ↓
Legacy Adapter (hoje) ──ou── Tauri Adapter (Fase 4+)
   ↓
Serviço SQL Angular (hoje) ──ou── Rust Command → Application → Domain → SQLite (Fase 4+)
```

Hoje (Fase 2) a maior parte do app ainda usa o serviço SQL Angular por baixo
do gateway — isso é esperado e não é dívida a "corrigir na mesma tarefa". A
troca para Rust é uma fase própria (Fase 4), feita depois de cada domínio já
estar isolado atrás de um gateway.

## Princípios não negociáveis

- SQLite e arquivos locais são a fonte canônica; a nuvem nunca persiste conteúdo narrativo.
- A UI não acessa persistência diretamente na arquitetura-alvo — sempre via gateway.
- Uma migration publicada nunca é alterada; toda evolução de esquema é uma migration nova.
- Atualização de aplicativo e atualização de banco são eventos diferentes (uma pode acontecer sem a outra).
- IA nunca altera conteúdo canônico sem confirmação explícita do escritor.
- Compartilhamento/colaboração são temporários e nunca escrevem direto no conteúdo canônico — só criam propostas pendentes.
- Sync nunca resolve conflito silenciosamente.
- Não introduza microserviço, CRDT, embeddings, ORM ou banco novo antes de existir necessidade comprovada (a mesma regra vale para ferramentas de processo, não só para o produto).
- Nunca dual-write: um command usa exatamente um adaptador por vez; a troca de adaptador só acontece depois de um teste de contrato comparando os dois caminhos.

## Os 11 invariantes de domínio

São regras executáveis, não só orientação — commands Rust, adapters legados,
imports, sync e colaboração precisam respeitar o mesmo conjunto:

1. Um `Chapter` pertence a exatamente um `Book` existente.
2. Um `Book` pertence a exatamente uma `Story`, que pertence a exatamente um `Universe` existente.
3. Uma `Entity` pertence a exatamente um `Universe` existente.
4. Uma `Relation` referencia duas entidades existentes do mesmo universo da relação.
5. Excluir uma entidade nunca deixa relação/menção quebrada (`NULL` explícito ou remoção na mesma operação).
6. Uma revisão/proposta nunca substitui conteúdo canônico sem um comando explícito de aprovação.
7. Uma sessão de compartilhamento nunca escreve direto no canônico — só cria anotações/propostas pendentes.
8. Uma resposta de IA nunca altera conteúdo canônico antes da confirmação do escritor.
9. Uma migration publicada nunca é alterada.
10. Uma operação de domínio falha por inteiro ou é confirmada por inteiro (sem estado parcial).
11. IDs persistidos são estáveis; renomear um item não muda sua identidade nem quebra referências.

Se uma mudança propõe violar qualquer um destes, pare e sinalize — mesmo que
o pedido pareça pequeno.

## Sinais de alerta ao revisar um diff

Trate qualquer um destes como motivo para parar e perguntar, não para seguir em frente:

- SQL novo (`SELECT`/`INSERT`/`UPDATE`/`DELETE`) dentro de um component ou feature já extraída — deveria estar só no `LegacyXGateway`.
- Um component ou store importando `DatabaseService`/`WorkspaceService` diretamente — veja [[narrahub-feature-extraction]].
- Uma regra crítica (validação de invariante, transação multi-tabela) implementada só no frontend.
- Dois adaptadores escrevendo no mesmo command, ou um "modo de transição" que grava nos dois bancos/caminhos ao mesmo tempo.
- Uma migration já publicada sendo editada em vez de uma nova sendo criada — veja [[narrahub-database-safety]].
- Introdução de uma dependência nova de infraestrutura (fila, cache distribuído, ORM, banco secundário) sem uma necessidade já comprovada em produção.

## Os 5 gates de cada fase

Nenhuma fase está "pronta" só porque compilou. Os gates completos (código,
banco, runtime Tauri, atualização, aceitação funcional) estão descritos em
detalhe no plano — releia a seção "Gates obrigatórios de cada fase" antes de
declarar uma fase concluída. Regra de publicação, resumida:

```text
COMPILA ≠ FUNCIONA ≠ ESTÁ SEGURO ≠ É PUBLICÁVEL
```

Build local, instalador gerado, push no Git e release remota são evidências
diferentes — não trate uma como prova da outra.
