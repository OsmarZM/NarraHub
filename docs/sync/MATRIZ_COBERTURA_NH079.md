# NH-079 — cobertura do Sync V2: agregados, fronteira `Mutacao` e exclusão

> Documento de arquitetura da etapa B. Revisão 3 (2026-09-15): modelo de impacto de exclusão
> (`Excluido` / `Reescrito` / `Bloqueado`), auditoria completa de FK e gatilhos (seção 3), payloads
> canônicos do manuscrito (seção 8) e B2. Revisão 2, com os ajustes da revisão humana:
> agregado = unidade de consistência e conflito; exclusão com preflight **antes** do SQL destrutivo;
> gatilhos classificados; canvas autoral × efêmero; negociação de canonicalização. Medido no código
> e no schema final (20 migrations aplicadas).

## 1. Onde o domínio é escrito

Todo comando de escrita passa por um serviço de `application/`; nenhum comando Tauri chama repositório
direto. **50 funções públicas de escrita em 8 serviços.** Com a B2, manuscrito, universo (exceto exclusão) e anexos geram evento.

| serviço | escritas | com transação | geram evento V2 |
| --- | --- | --- | --- |
| `manuscript_service` | 10 | 10 (`Mutacao`, B2) | 10 |
| `canvas_service` | 11 | 5 | 2 (anexos, B1) |
| `entity_service` | 5 | 3 | 0 |
| `planning_service` | 8 | 4 | 0 |
| `universe_service` | 3 | 2 (`Mutacao`, B2) | 2 (`delete` recusa até B5) |
| `workspace_service` | 5 | 0 | 0 |
| `knowledge_service` | 4 | 1 | 0 |
| `collaboration_service` | 6 | 2 | 0 |

**50 escritas não são 50 emissores.** O modelo é:

```text
mutação de aplicação (uma ação do usuário ou do sistema)
        ↓
agregados afetados            uma ação que mexe em 5 tabelas de UM agregado = 1 revisão
        ↓                     uma ação que muda 3 agregados independentes   = 3 revisões, mesma transação
revisões / eventos
```

## Estado da cobertura

| etapa | status | o que cobre |
| --- | --- | --- |
| **B1** | integrada | fronteira `Mutacao`; `attachment` create e delete; exclusão remota bloqueada e a resolução dela (4.4.1); fronteira com o blob store (4.4.2); migration 21 (`sync_divergences.kind`) |
| **B2** | integrada (#59) | `universe` create/update; `story`, `book`, `chapter` create/update/delete; `chapter_order` (create/delete de capítulo e reorder — **substituído por `chapter_position` na B2.2**); campos personalizados dentro desses agregados; `tag_assignment` apagado pelos gatilhos do manuscrito; impacto de exclusão `Excluido`/`Reescrito`/`Bloqueado` (4.3); catálogo de efeitos com gate (3); payload canônico definitivo (8); dependência de criação pai → filho na aplicação remota; capa de livro pelo blob store |
| **B2.1** | integrada (#60), **substituída pela B2.2** | `story_order(universe)` e `book_order(story)` com o contrato de `chapter_order`; **ponte de ordem** transacional, que destrava ordens entre três origens sem nunca confirmar revisão corrente não materializada — inclusive o travamento que existia em `chapter_order` desde a B2 |
| **B3** | integrada (#61) | `entity` (ficha inteira: `entities` + `entity_attributes` + campos personalizados, uma revisão só), `relation`, `timeline_event` e `canvas_entity_position`; exclusão de entidade com árvore completa de efeitos, inclusive o `SET NULL` da linha do tempo como **reescrita**; `entity_service`, `workspace_service` e a posição do canvas pela `Mutacao` |
| **B4** | integrada (#62) | `planning_item` (card inteiro: texto, imagem, capítulo, valores escalares e relações — uma revisão), `planning_order(universe)` (coluna e posição — **substituído por `planning_item_position` na B2.2**) e `planning_field_definition`; o gatilho que reescreve vários cards declarado; os bloqueios temporários de capítulo, história e entidade viraram reescrita do card |
| **B5** | integrada (#63) | `content_tag` (create/update/delete — **`update_tag` não existia**, e foi criado aqui), `tag_assignment` create/delete pela fronteira, `canvas_node`, `canvas_node_position` e `canvas_edge`; payload **definitivo** do `attachment`; migration 22 (gatilhos que matam a aresta com a ponta + `tag_name_conflict`); `knowledge_service` entra no gate estrutural, onde **nunca esteve**; gate autoral × efêmero |
| **B2.2** | implementada (branch `sync-b2.2-posicao-por-item`), em revisão | **posição por item** no lugar das listas inteiras (`story_order`, `book_order`, `chapter_order`, `planning_order` saíram; entram `story_position`, `book_position`, `chapter_position`, `planning_field_position`, `attachment_position`, `planning_item_position`); **grupos de mutação atômicos** no envelope (migration 23): a ação que é uma transação na origem entra inteira no receptor, ou vira UMA decisão; a ponte de ordem da B2.1 saiu, sem consumidores |
| B6 | pausada até a B2.2 integrar | ver seção 7 |

**Fora da B2, dito às claras:**

- `delete_universe` **recusa sempre** com "Esta operação ainda depende de tipos que estão sendo migrados
  para o Sync V2." A cascata do universo atinge entidades, relações, linha do tempo, planejamento, tags e
  canvas, que ainda não têm codec. Uma exclusão de universo recebida de outro aparelho vira
  `parent_deletion_blocked` e não apaga nada. Volta quando a última dessas etapas integrar (B5).
- Excluir capítulo ligado a card do planejamento, ou história usada num campo de card, é **recusado**
  (seção 3.2). Volta na B4, quando `planning_item` tiver codec e o efeito virar `Reescrito`.
- ~~Marcar e desmarcar tag ainda não emite evento~~ — fechado na B5.
- ~~`attachment` mantém o payload da B1~~ — fechado na B5: `createdAt` e `sortOrder` saíram do payload.
- **Nenhum bloqueio temporário sobrou do planejamento.** Excluir capítulo, história ou entidade ligada a
  card agora **reescreve o card** (B4). Os `Bloqueado` da B2/B3 saíram do código e do catálogo.
- **Fora da B4:** `entity_templates` **não tem escritor no app** — é acervo legado que `create` de
  entidade lê; precisa de codec antes da gênese (6.1). Nó e aresta do canvas continuam na B5, e as
  funções deles estão nomeadas no gate estrutural com o motivo. Tag e marcação de tag continuam na B5:
  excluir uma tag reescreve cards que a citam, e isso só passa pela fronteira quando o
  `knowledge_service` entrar.

## 2. Agregados

**Critério:** agregado é a **unidade de consistência e de conflito** — o conjunto que precisa ser lido e
gravado inteiro para continuar válido, e que, editado em dois aparelhos, vira **um** conflito. "O que a
tela mostra junto" é sinal de UX, não fronteira: relação aparece na ficha da entidade, mas tem identidade
e concorrência próprias.

### 2.1 Raízes e dependências

| raiz | tabelas internas | identidade | depende de (precisa existir antes) | observação |
| --- | --- | --- | --- | --- |
| `universe` | `universes`, `content_custom_fields` do universo | `universes.id` | — | raiz de tudo |
| `story` | `stories`, `content_custom_fields` (owner story) | `stories.id` | universe | |
| `book` | `books`, `content_custom_fields` (owner book) | `books.id` | story | |
| `chapter` | `chapters` (sem `sort_order`), `content_custom_fields` (owner chapter) | `chapters.id` | book | conteúdo, título, status, resumo |
| `story_position` | `stories.sort_order` | `stories.id` | story | **só a posição** (B2.2) |
| `book_position` | `books.sort_order` | `books.id` | book | **só a posição** (B2.2) |
| `chapter_position` | `chapters.sort_order` | `chapters.id` | chapter | **só a posição**; mover não conflita com texto (B2.2) |
| `entity` | `entities`, `entity_attributes`, `content_custom_fields` (owner entity) | `entities.id` | universe | atributos são internos (B3) |
| `relation` | `relations` | `relations.id` | 2 entities | independente da entidade |
| `timeline_event` | `timeline_events` | `timeline_events.id` | universe; entity (opcional, `SET NULL`) | |
| `planning_item` | `planning_items` (sem `sort_order`), valores em `custom_field_values`, `planning_field_links` | `planning_items.id` | universe; chapter (opcional) | valores e links são internos |
| `planning_item_position` | `planning_items.status` + `sort_order` | `planning_items.id` | planning_item | etapa e posição do card (B2.2) |
| `planning_field_position` | `planning_field_definitions.sort_order` | `planning_field_definitions.id` | planning_field_definition | posição da propriedade (B2.2) |
| `planning_field_definition` | `planning_field_definitions` | `id` | universe; planning_item (se escopo card) | |
| `content_tag` | `content_tags` | `id` | universe | |
| `tag_assignment` | `content_tag_assignments` | `(tag_id, owner_type, owner_id)` | tag + dono | aresta com identidade própria |
| `canvas_node` | `canvas_nodes` (inclui `x`, `y` persistidos) | `id` | universe | |
| `canvas_edge` | `canvas_edges` | `id` | universe; 2 pontas | |
| `canvas_entity_position` | `canvas_entity_positions` | `entity_id` | entity | posição persistida da entidade no grafo |
| `attachment` | `attachments` (referência de blob) | `id` | dono (chapter / entity / node) | já sincroniza |
| `attachment_position` | `attachments.sort_order` | `attachments.id` | attachment | posição na galeria (B2.2) |

**Ordem de dependência** (criação em ordem, exclusão na ordem inversa):

```text
universe
 ├─ story ─ book ─ chapter          cada item com a sua *_position
 ├─ entity ─ relation, canvas_entity_position
 ├─ timeline_event
 ├─ content_tag ─ tag_assignment(dono)
 ├─ planning_field_definition
 ├─ planning_item ─ planning_item_position(item)
 ├─ canvas_node ─ canvas_edge
 └─ attachment(dono)
```

### 2.2 Canvas: autoral × efêmero

| estado | onde vive | sincroniza |
| --- | --- | --- |
| posição de nó (`canvas_nodes.position_x/y`) | banco | **sim**, como `canvas_node_position` |
| posição persistida de entidade (`canvas_entity_positions`) | banco | **sim**, como `canvas_entity_position` |
| conteúdo de nó, arestas | banco | **sim** |
| zoom, pan, viewport | memória do componente | não |
| seleção, hover, painel aberto | memória do componente | não |

Regra: **só o que está no banco e foi feito pelo usuário sincroniza.** O segundo grupo não sincroniza
porque **não está no banco** — ele vive na memória do componente e morre com a tela.

Isso agora é gate, não combinado (`sync_codec::efemero`):

```text
1  nenhuma coluna do banco INTEIRO com nome de estado de tela (zoom, pan, seleção, painel…)
   sem estar declarada em ESTADO_DE_TELA_ACEITO com o motivo — a lista está vazia
2  cada coluna das tabelas da B5 com destino declarado: payload de qual agregado, ou local com motivo
```

O gate 2 é o que morde: acrescentar coluna a `canvas_nodes` passa a exigir escrever o que ela é. O
silêncio deixa de significar "não sincroniza e ninguém percebeu".

**Conteúdo × posição do elemento livre.** `canvas_node` e `canvas_node_position` são agregados
diferentes, pelo mesmo motivo de `entity` e `canvas_entity_position`: arrastar e escrever são ações
diferentes e não podem colidir. A diferença é que a posição do nó mora em colunas do próprio
`canvas_nodes` — `canvas_node_position` é de **existência derivada**, como as ordens.

**A aresta e a ponta polimórfica.** Até a migration 22, apagar uma entidade deixava a aresta viva no
banco: ela sumia da tela pelo filtro do `list_edges` e ficava no arquivo para sempre. Invisível não é
ausente — e agora que a aresta é causal, isso seria divergência esperando para acontecer (um aparelho
com ela, outro sem). Dois gatilhos resolvem nos dois sentidos, a migration limpa as órfãs de uma vez, e
o catálogo da seção 3 passa a cobri-los.

**Tag homônima (`tag_name_conflict`).** `content_tags` tem `UNIQUE(universe_id, name COLLATE NOCASE)` e
a identidade causal da tag é o `id`. Criar "Mar" nos dois aparelhos produz dois agregados com o mesmo
nome, e o segundo a chegar não cabe na tabela — o schema recusando materializar um evento válido. Nada
é aplicado e nada é alterado: abre-se a divergência, e o escritor decide se são a mesma tag ou renomeia
uma. Foi por isso que `update_tag` precisou existir.

### 2.3 Fora do Sync, de propósito

| tabela | por quê |
| --- | --- |
| `mentions` | derivada do texto; cada aparelho recalcula |
| `chapter_revisions`, `change_log` | histórico local de edição |
| `collaboration_sessions`, `collaboration_contributions` | sessão efêmera deste aparelho. **O conteúdo que uma contribuição aprovada grava** em capítulo ou entidade é mutação sincronizável e passa pela `Mutacao` (B6) |
| `devices`, `blob_migration_issues` | locais |
| `sync_*`; `sync_peers`, `sync_conflicts` (V1) | protocolo |

## 3. Efeitos de exclusão: toda FK com ação e todo gatilho que escreve

Medido no schema migrado (21 migrations), não no texto das migrations. A tabela é a mesma de
`src-tauri/src/infrastructure/sqlite/sync_codec/catalogo.rs`, e o gate
`toda_fk_com_acao_e_todo_gatilho_que_escreve_estao_no_catalogo` reprova FK ou gatilho novo sem
classificação, ou classificação que não existe mais no schema. Não há `ON DELETE SET DEFAULT` nem
`RESTRICT` no schema; as FKs `NO ACTION` são todas das tabelas `sync_*`.

Efeito: **Delete** = o agregado atingido some; **Rewrite** = sobrevive com outro estado; **Interno** =
estado do próprio agregado de origem, sem identidade própria; **Local** = tabela fora do sync.

### 3.1 Tabela

| origem da exclusão | agregado afetado | mecanismo | efeito | etapa | enquanto não coberto |
| --- | --- | --- | --- | --- | --- |
| universe | story | FK `stories.universe_id` CASCADE | Delete | B2 | universe delete recusado |
| universe | attachment | FK `attachments.universe_id` CASCADE | Delete | B1 | universe delete recusado |
| universe | (interno de universe/story/book/chapter/entity) | FK `content_custom_fields.universe_id` CASCADE | Interno | B2 / B3 | universe delete recusado |
| universe | entity | FK `entities.universe_id` CASCADE | Delete | B3 | universe delete recusado |
| universe | entity_template | FK `entity_templates.universe_id` CASCADE | Delete | B3 | universe delete recusado |
| universe | relation | FK `relations.universe_id` CASCADE | Delete | B3 | universe delete recusado |
| universe | timeline_event | FK `timeline_events.universe_id` CASCADE | Delete | B3 | universe delete recusado |
| universe | canvas_entity_position | FK `canvas_entity_positions.universe_id` CASCADE | Delete | B3 | universe delete recusado |
| universe | planning_item | FK `planning_items.universe_id` CASCADE | Delete | B4 | universe delete recusado |
| universe | planning_field_definition | FK `planning_field_definitions.universe_id` CASCADE | Delete | B4 | universe delete recusado |
| universe | content_tag | FK `content_tags.universe_id` CASCADE | Delete | B5 | universe delete recusado |
| universe | canvas_node | FK `canvas_nodes.universe_id` CASCADE | Delete | B5 | universe delete recusado |
| universe | canvas_edge | FK `canvas_edges.universe_id` CASCADE | Delete | B5 | universe delete recusado |
| story | book | FK `books.story_id` CASCADE | Delete | B2 | — |
| story | story_position(story) | sem linha própria (o número mora na linha da história) | Delete | B2.2 | — |
| story | planning_item (link interno) | FK `planning_field_links.story_id` CASCADE | **Rewrite** | B4 | **exclusão da história recusada** enquanto houver card com a história num campo |
| story | tag_assignment | gatilho `trg_story_metadata_delete` | Delete | B2 | — |
| story | (interno) | gatilho `trg_story_metadata_delete` → `content_custom_fields` | Interno | B2 | — |
| book | chapter | FK `chapters.book_id` CASCADE | Delete | B2 | — |
| book | book_position(book) | sem linha própria | Delete | B2.2 | — |
| book | tag_assignment | gatilho `trg_book_metadata_delete` | Delete | B2 | — |
| book | (interno) | gatilho `trg_book_metadata_delete` → `content_custom_fields` | Interno | B2 | — |
| chapter | planning_item | FK `planning_items.chapter_id` **SET NULL** | **Rewrite** | B4 | **exclusão do capítulo recusada** enquanto houver card ligado |
| chapter | chapter_position(chapter) | sem linha própria | Delete | B2.2 | — |
| chapter | attachment | gatilho `trg_chapter_attachments_delete` | Delete | B1 | — |
| chapter | tag_assignment | gatilho `trg_chapter_metadata_delete` | Delete | B2 | — |
| chapter | (interno) | gatilho `trg_chapter_metadata_delete` → `content_custom_fields` | Interno | B2 | — |
| chapter | — | FK `chapter_revisions.chapter_id` CASCADE | Local | fora | — |
| chapter | — | FK `mentions.chapter_id` CASCADE | Local | fora | — |
| entity | (interno) | FK `entity_attributes.entity_id` CASCADE | Interno | B3 | entity_service fora da Mutacao até B3 |
| entity | relation | FK `relations.source_id` / `target_id` CASCADE | Delete | B3 | idem |
| entity | timeline_event | FK `timeline_events.entity_id` **SET NULL** | **Rewrite** | B3 | idem |
| entity | canvas_entity_position | FK `canvas_entity_positions.entity_id` CASCADE | Delete | B3 | idem |
| entity | planning_item (link interno) | FK `planning_field_links.entity_id` CASCADE | **Rewrite** | B4 | idem; na B3, recusa enquanto houver link |
| entity | attachment | gatilho `trg_entity_attachments_delete` | Delete | B3 | idem |
| entity | tag_assignment | gatilho `trg_entity_metadata_delete` | Delete | B3 | idem |
| entity | (interno) | gatilho `trg_entity_metadata_delete` → `content_custom_fields` | Interno | B3 | idem |
| entity | — | FK `mentions.entity_id` CASCADE | Local | fora | — |
| timeline_event | tag_assignment | gatilho `trg_timeline_metadata_delete` | Delete | B3 | fora da Mutacao até B3 |
| planning_item | (interno) | FK `planning_field_links.planning_item_id` CASCADE | Interno | B4 | planning_service fora da Mutacao até B4 |
| planning_item | planning_field_definition | FK `planning_field_definitions.owner_item_id` CASCADE | Delete | B4 | idem |
| planning_item | tag_assignment | gatilho `trg_planning_metadata_delete` | Delete | B4 | idem |
| planning_field_definition | planning_item | FK `planning_field_links.field_definition_id` CASCADE | **Rewrite** | B4 | idem |
| planning_field_definition | planning_item | gatilho `trg_planning_field_definition_delete` (`custom_field_values`) | **Rewrite** | B4 | idem |
| content_tag | tag_assignment | FK `content_tag_assignments.tag_id` CASCADE | Delete | B5 | knowledge_service fora da Mutacao até B5 |
| content_tag | planning_item (link interno) | FK `planning_field_links.tag_id` CASCADE | **Rewrite** | B4 | idem |
| collaboration_session | — | FK `collaboration_contributions.session_id` CASCADE | Local | fora | — |

Gatilhos que escrevem sem ser exclusão, todos locais: `trg_chapter_history_insert`/`_update`,
`trg_chapter_revision`, `trg_entity_history_insert`/`_update`. Os demais gatilhos só validam (`RAISE`).

**Referências sem FK** (ids em texto polimórfico): `attachments.owner_id`, `content_tag_assignments.owner_id`
e `content_custom_fields.owner_id` são cobertos pelos gatilhos acima; `canvas_edges.source_id/target_id`
(entity ou nó) não têm FK nem gatilho — a exclusão de entidade deixa aresta pendurada, e isso é da B3/B5.

### 3.2 Estratégia para efeito sobre agregado ainda não coberto: fail closed

Para `chapter DELETE → planning_items.chapter_id SET NULL` a escolha foi **bloquear até a B4** (opção 2),
não trazer `planning_item` para a B2. O payload canônico de `planning_item` inclui `custom_field_values`
(reescrito por gatilho), `planning_field_links` internos, imagem por blob e a separação entre card e
`planning_order` (status + posição). Trazer "a parte mínima" obrigaria a fixar esse formato agora — e a
gênese da etapa C reutiliza o formato; fixar metade dele é o que produziria revisões diferentes para o
mesmo card. O bloqueio custa pouco ao escritor (desvincular o card antes) e não altera nada em silêncio.

```text
local   m.excluir(chapter) → Bloqueado("…ligado a N card(s) do planejamento…") → erro, nada muda, nenhum evento
remoto  exclusão de chapter chega → mesmo impacto → parent_deletion_blocked; o card continua ligado
```

## 4. A fronteira `Mutacao`

### 4.1 Invariante

```text
mutação sincronizável confirmada
⇔
estado de domínio + estado causal (aggregate_state, revision_history, tombstone) + evento (outbox)
foram commitados na MESMA transação

após qualquer queda: tudo existe, ou nada existe
nunca: domínio sem evento          nunca: evento sem domínio
```

### 4.2 Contrato

```text
Mutacao::executar(database, identidade, |m| -> Result<T>) -> Result<T>

  BEGIN IMMEDIATE                                   única transação; aninhar é erro
  closure(m)
     m.tx()                  → &Transaction          repositórios recebem ESTA transação
     m.gravou(agregado)      → declara create/update  (lido depois, no fim)
     m.excluir(agregado)?    → preflight AGORA, antes do SQL destrutivo (ver 4.3)
  fim do closure
     para cada gravou:        estado canônico relido na mesma tx → evento upsert
     para cada exclusão:      confere que sumiu; eventos delete já preparados → persiste
     append_event_in_transaction (assina, revisão, cursor)
  COMMIT                                             erro em qualquer passo → ROLLBACK de tudo
```

Regras de implementação:

- Os repositórios usados dentro da `Mutacao` recebem `&Transaction`/`&Connection` **da mutação**; nenhum
  deles abre transação nem conexão própria. Gate estrutural na B1.
- `Mutacao::executar` dentro de outra `Mutacao` é erro (não há savepoint silencioso).
- Um agregado declarado duas vezes vira **uma** revisão (a do estado final).

### 4.3 Exclusão: impactos e preflight antes do SQL

**A fronteira não assume que todo agregado afetado some.** O codec devolve os impactos diretos:

```text
Excluido(agregado)    some junto (FK CASCADE, gatilho, posição sem linha própria)
Reescrito(agregado)   sobrevive com outro estado (SET NULL, lista que perde um item)
Bloqueado(motivo)     atinge agregado ainda não coberto → recusa a exclusão inteira
```

```text
m.excluir(parent)
  preflight (antes do SQL):
    coleta recursiva pelos Excluido; Reescrito acumulado; Bloqueado em qualquer nível → erro
    excluir vence reescrever (o mesmo agregado nos dois → só Excluido)
    TODO afetado — Excluido e Reescrito — com divergência aberta ou evento pendente → erro
    universe_id de todo afetado lido agora; vazio → erro
serviço executa o DELETE
fim da transação:
  Reescrito com linha própria → estado canônico relido → upsert, ANTES das exclusões
  Excluido  → precisa ter sumido → evento delete (descendentes antes do pai)
  todos os eventos da ação → UM grupo de mutação (mesmo mutation_id, índices 0..n)   (B2.2)
  Excluido(A) domina Reescrito(A): agregado condenado não ganha revisão intermediária
  agregado cujo canônico já é o payload da revisão corrente → nenhum evento
COMMIT
```

Desde a B2.2 excluir um item **não reescreve os irmãos**: a posição é por item e nada é compactado. O
`Reescrito` que sobra é o de quem sobrevive com linha própria (card que perde ligação, evento com entidade
em nulo).

**Sobrevivente com linha própria sai antes das exclusões.** O estado dele sem o item apagado já é
materializável, e vir primeiro faz o receptor conhecer a concorrência **antes** do SQL destrutivo: edição
concorrente abre divergência e a exclusão seguinte é bloqueada. A regra antiga de pôr "ordens" **depois**
das exclusões morreu com as listas inteiras (B2.2).

**A posição do item sai logo antes do item.** Ela é declarada num lugar só (`impactos_da_exclusao`), para
nenhum codec esquecer.

**`Excluido(A)` domina `Reescrito(A)`.** Se um agregado já vai desaparecer nesta operação, a reescrita que a
cascata causaria nele é absorvida — sem revisão intermediária. É o card que possui um campo exclusivo com
valor dentro dele: apagar o card apaga o campo, e o efeito do campo sobre o card não vira evento. A regra
está em dois lugares de propósito (`coletar` descarta, `finalizar` ignora), e um teste de mutação que
remove as duas reprova.

**Exclusão remota com sobrevivente.** Quem recebe a exclusão não bloqueia porque o sobrevivente vai mudar —
a reescrita dele é o evento seguinte da mesma origem. Bloqueia se o sobrevivente tem **decisão aberta** ou
**evento pendente de outra história** aqui (`estado_concorrente_para_evento`). Eventos da mesma origem com
`seq` maior não contam: o lote é guardado inteiro antes de aplicar, e eles são a continuação da mesma
mutação. Entre os dois eventos, o sobrevivente fica momentaneamente diferente da revisão dele; a asserção
geral de materialização vale em repouso (8.1).

**Uma ação, um grupo (B2.2).** Tudo o que a exclusão emite — posições, filhos, sobreviventes, a raiz —
sai com o mesmo `mutation_id`, `kind = delete_tree` e a raiz declarada. No receptor, ou o grupo entra
inteiro, ou nenhum membro entra (4.6).

Depois de `DELETE FROM parent`, a cascata e os gatilhos já apagaram os descendentes — ninguém consegue mais
lê-los. **Proibido:** descobrir os afetados depois da cascata.

### 4.4 Exclusão remota: nunca deixar a FK apagar um filho concorrente

Invariante:

> Uma exclusão remota de um agregado pai **não executa o `DELETE` físico** enquanto algum descendente
> que a cascata destruiria tiver estado concorrente não resolvido. A exclusão fica pendente como
> conflito (`ParentDeletionBlockedByConcurrentDescendant`), e o filho continua existindo.

```text
evento delete do pai chega
  preflight causal dos descendentes (antes de qualquer SQL destrutivo)
    descendente com divergência aberta, ConcurrentComExclusao, ou evento pendente?
      → NÃO apaga o pai
      → registra divergência do pai: exclusão bloqueada por descendente concorrente
      → o evento fica aplicado como decisão pendente (cursor anda; nada some)
    nenhum?
      → DELETE, com tombstone
```

Os eventos de exclusão dos filhos chegam **antes** do pai (ordem de emissão de 4.3). Num aparelho sem
concorrência, cada filho é removido pela própria regra causal; quando o pai chega, não resta descendente e
a cascata não apaga nada que tenha história.

**O evento bloqueado não se repete.** Ele fica marcado como aplicado e a revisão da exclusão entra na
história; o cursor da origem avança. A mesma exclusão chegando de novo é `JaAplicado`: nenhuma divergência
nova, nenhum `DELETE`.

#### 4.4.1 Resolução de `parent_deletion_blocked`

O resolvedor (`application/resolucao_divergencia.rs`) lê o `kind` antes de qualquer coisa. Tipo sem
contrato é recusado; em especial, divergência `concurrent` **não** é resolvida executando uma exclusão
(a caixa de conciliação dela é a etapa F).

```text
ManterLocal     pai e descendentes ficam
                nasce upsert do pai com base_rev = revisão da exclusão remota
                nos outros aparelhos: base == deleted_rev → Sequential → o pai volta, tombstone sai
                sobreviventes que a exclusão reescreveria (a ordem do livro) também são declarados:
                a ordem daqui, que cita o capítulo, vira revisão — a do outro lado, sem ele, vira decisão
AceitarRemoto   preflight de descendentes refeito AGORA
                  sobrou filho vivo              → recusa; nada muda; divergência continua aberta
                  pai alterado depois do bloqueio → recusa
                  outra divergência/evento pendente do pai → recusa
                  nada disso                     → DELETE + tombstone com a revisão remota;
                                                   nenhum evento novo (a revisão já é de todos)
```

O descendente se resolve antes, por mutação normal (excluir o anexo, por exemplo). Aceitar com base no
que era verdade no momento do bloqueio deixaria a cascata apagar trabalho criado depois.

**Mudança na classificação causal:** agregado excluído aqui, evento com `base_rev == deleted_rev` é
`Sequential` (restauração que viu a exclusão). Só a resolução explícita produz essa base; uma edição que
não viu a exclusão continua `ConcurrentComExclusao`, e nada ressuscita sozinho.

Testes: `resolucao_divergencia::tests` (6) e `domain::sync::tests::restauracao_a_partir_da_revisao_da_exclusao_e_sequencial`.

#### 4.4.1.1 Árvore de efeitos da exclusão de entidade (B3)

Medida no schema migrado, não suposta:

```text
delete entity
  ├─ relation (cada uma com a entidade em qualquer ponta)   → Delete   (FK CASCADE nas duas pontas)
  ├─ canvas_entity_position(entity)                         → Delete   (FK CASCADE)
  ├─ attachment (owner_type = 'entity')                     → Delete   (trg_entity_attachments_delete)
  ├─ tag_assignment (owner_type = 'entity')                 → Delete   (trg_entity_metadata_delete)
  ├─ entity_attributes, content_custom_fields               → interno  (somem com a ficha)
  ├─ timeline_event (entity_id = E)                         → REWRITE  (FK SET NULL → entityId nulo)
  ├─ mentions                                               → local, fora do sync
  └─ planning_field_links (entity_id)                       → BLOQUEADO até a B4
```

Emissão: os excluídos primeiro (relação, posição, anexo, marcação), a entidade, e por último a
reescrita de cada evento da linha do tempo — que é relido depois do `DELETE`, já com `entityId` nulo.

#### 4.4.1.2 O gatilho que reescreve vários cards (B4)

```text
delete planning_field_definition F
  ├─ trg_planning_field_definition_delete   tira a chave de F do JSON de CADA card do universo
  ├─ FK planning_field_links.field_definition_id CASCADE   apaga as relações de F em cada card
  └─ efeito declarado: Rewrite(card A), Rewrite(card B), Rewrite(card C), …
```

Os dois mecanismos atingem o mesmo agregado (o card), e a união deles é declarada como `Reescrito`
antes do `DELETE`. Cada card afetado é relido depois do SQL e ganha **revisão própria**. Card com
divergência aberta ou evento pendente recusa a exclusão inteira, como qualquer reescrita (4.3).

Sem isso, uma exclusão reescreveria quarenta cards emitindo um evento só — a escrita invisível que
abriu a NH-079.

#### 4.4.2 Fronteira SQLite × blob store

O arquivo do anexo é gravado dentro da `Mutacao`, mas o sistema de arquivos não participa do `ROLLBACK`.

```text
blob publicado → linha + evento → COMMIT     ok
blob publicado → falha → ROLLBACK            blob órfão
linha commitada → blob ausente               PROIBIDO (por isso o arquivo vem sempre antes)
```

Política:

| resto | destino |
| --- | --- |
| blob publicado sem referência (rollback, seed recusado) | fica. Endereçado por conteúdo: inofensivo e reaproveitado se a ação for repetida. Não há GC de blob publicado (ADR 0010 §11) — pode ser de um backup ou de evento que outro aparelho ainda vai pedir; GC só com regra causal, depois da G |
| `.part` em `blob-staging/` (queda no meio da escrita) | removido uma vez por arranque, antes do preparo do banco, se tiver mais de 1 h (`BlobStore::limpar_staging_abandonado`) |

Testes: `mutacao::tests::rollback_depois_do_blob_deixa_so_o_arquivo_orfao_e_repetir_o_reaproveita`,
`blob_store::tests::limpeza_de_staging_so_leva_part_abandonado`.

### 4.6 Grupos de mutação: a atomicidade atravessa a rede (B2.2)

A `Mutacao` é uma transação na origem. Até a B2.2, na rede ela virava eventos independentes, e o receptor
podia aplicar metade de uma ação:

```text
PC apaga o livro (c1, c2)        Android cria o capítulo "novo" offline
Android aplica: c1 some · c2 some · o livro é bloqueado por "novo"      → livro pela metade
```

Na B2 isso não acontecia **por acidente**: a lista inteira `chapter_order` entrava em divergência e travava
cada exclusão de capítulo. A B2.2 tirou a lista e expôs o problema real: a unidade de atomicidade era
pequena demais.

**O envelope carrega a ação** (migration 23):

```text
mutation_id      o mesmo para todo evento de uma Mutacao::executar
mutation_index   0..count, contíguo, na ordem de emissão
mutation_count   quantos membros a ação tem
mutation_kind    "delete_tree" quando a ação é uma exclusão composta
root_type/_id    a raiz da exclusão: o que a decisão apresenta ao escritor
```

**Entra na assinatura, fica fora da revisão.** O grupo descreve a ação, não o estado do agregado: apagar
c1 sozinho ou como parte do livro é o mesmo efeito sobre c1 e dá a mesma `new_rev`. Mas reagrupar eventos
no caminho — tirar um membro, mudar a contagem — invalida a assinatura. Evento sem grupo (anterior à v23)
mantém os bytes assinados de antes.

**No receptor:**

```text
grupo incompleto                   → guarda, não aplica nenhum membro, o cursor espera
grupo completo, num SAVEPOINT, membro a membro:
  todos entram                     → confirma
  um membro espera dependência     → desfaz tudo, o grupo espera
  um membro diverge ou é bloqueado → desfaz tudo; UMA decisão da ação, com o mutation_id, ancorada
                                     na RAIZ quando é uma exclusão composta (senão no membro que
                                     não entrou); revisões de todos os membros entram na
                                     história e os eventos ficam aplicados (o cursor anda)
  erro                             → a sessão inteira falha fechada
```

**Regra estrita, decidida:** membro concorrente de **qualquer** tipo — inclusive edição contra edição —
segura o grupo inteiro. Duas reordenações que colidem num capítulo não se misturam: aplicar só a parte
que não colidiu comporia uma ordem que nenhum dos dois escritores escolheu. Cada lado fica com a sua até
a decisão.

**A decisão vale para a ação.** `estado_concorrente` considera em decisão todo agregado que é membro de um
grupo com decisão aberta — não só a âncora. A resolução (4.4.1):

```text
manter o local   cada membro que existe aqui é reafirmado a partir da revisão dele no grupo
aceitar          refaz AGORA o preflight de cada membro, na ordem, e aplica todos numa transação;
                 sobrevivente reescrito pela exclusão e editado aqui depois da base NÃO recebe o
                 payload da origem: a exclusão roda sobre o estado daqui e ele ganha revisão nova,
                 descendente da reescrita da origem (a edição local fica, a origem a recebe como
                 sequencial)
```

**O lote da troca não corta uma ação ao meio.** O receptor não aplica grupo incompleto e o vetor dele só
anda pelo aplicado: um lote que terminasse no meio de um grupo maior que ele faria a sessão seguinte pedir o
mesmo começo para sempre. Ao atingir o limite dentro de um grupo, a resposta completa o grupo.

### 4.5 Gates

1. **Estrutural:** serviços sincronizáveis não chamam `database.write()` fora da `Mutacao`; repositórios da
   mutação não abrem transação.
2. **Comportamental:** para cada escrita coberta, depois da operação, todo agregado sincronizável tem
   `Codec::ler_canonico == payload da revisão corrente`; linha sem revisão, ou revisão sem linha, reprova.
3. **Dois aparelhos:** operação no A, eventos no B, estado canônico igual.
4. **Falha injetada** (só em build de teste): antes do evento, depois da mutação e antes do commit, durante
   a geração do evento → nada persiste; repetir a operação produz uma revisão só.
5. **Escopo e posse:** afetado sem `universe_id` derruba a transação inteira (nunca evento com universo
   vazio); a coleta de descendentes marca visitado antes de descer e recusa ciclo de posse.
6. **Catálogo de efeitos (B2):** toda FK com ação e todo gatilho que escreve está classificado
   (`sync_codec::catalogo`); FK/gatilho novo sem classificação reprova, e efeito sobre agregado não coberto
   precisa declarar a ação enquanto isso.
7. **Manuscrito inteiro pela fronteira (B2):** `escritas_do_manuscrito_passam_todas_pela_mutacao` varre toda
   função pública de `manuscript_service` e `universe_service`; a única exceção nomeada é
   `universe_service::delete`, que recusa e não escreve.
8. **Dois aparelhos (B2):** `application::sync_manuscrito_testes` — PC → Android e Android → PC para
   universo, história, livro, capítulo, edição, reorder, exclusão de capítulo/livro/história; união de
   conteúdo independente; pai excluído com filho concorrente; exclusão que reescreveria card (`SET NULL`)
   recusada nos dois lados; reescrita de agregado em divergência recusa a exclusão; exclusão de universo
   recusada local e remotamente; salvar o mesmo estado não gera revisão. Convergência = payload canônico
   igual nos dois **e** igual ao payload da revisão corrente de cada um.
9. **Entidades em dois e três aparelhos (B3):** criação/edição/exclusão de entidade, relação, evento e
   posição PC ↔ Android; atributo como revisão da entidade; entidade editada nos dois lados (as duas
   revisões ficam); entidade apagada num lado com relação criada no outro; com evento editado no outro;
   `SET NULL` sem concorrência (o evento sobrevive com `entityId` nulo nos dois); relação, evento e posição
   que chegam antes das entidades (de outra origem, contíguos — o que segura é a dependência, não a lacuna
   de `seq`); três origens com a relação de A citando entidade de C; `clear_layout`; evento que troca de
   entidade recusado.
10. **Planejamento em dois e três aparelhos (B4):** card, quadro e propriedades PC ↔ Android; mover card
    não revisa o conteúdo; excluir card leva os campos exclusivos e reescreve o quadro; excluir
    propriedade reescreve **todos** os cards afetados (um evento por card); excluir capítulo deixa
    `chapterId` nulo; excluir história e entidade tiram a ligação com revisão do card; card editado num
    lado × propriedade apagada no outro (decisão, com a edição preservada no log); card movido num lado ×
    ficha editada no outro (agregados diferentes, sem conflito); card de outra origem que depende de
    entidade, história e propriedade que ainda não chegaram.
11. **Conhecimento e canvas em dois e três aparelhos (B5):** mover o elemento num lado × escrever nele no
    outro (agregados diferentes, sem conflito); excluir elemento emite ligação e posição antes do nó;
    excluir **entidade** leva a ligação nos dois aparelhos; elemento apagado num lado × ligação criada no
    outro (exclusão bloqueada, ligação preservada); ligação que chega antes das pontas espera por elas
    (três origens); a mesma marcação criada dos dois lados converge sem divergência e sem duplicar linha;
    tag renomeada num lado × marcada no outro; tag homônima dos dois lados vira `tag_name_conflict` sem
    aplicar nem alterar nada; tag apagada num lado × card editado no outro bloqueia a exclusão.
12. **Autoral × efêmero (B5):** `sync_codec::efemero` — nenhuma coluna do banco inteiro com nome de estado
    de tela sem decisão escrita; toda coluna das tabelas da B5 com destino declarado (payload de qual
    agregado, ou local com motivo).
13. **Materialização exata (B2):** cada evento aplicado um por vez com o estado conferido
   (`cada_aplicado_materializa_o_proprio_evento`); asserção geral em repouso depois de toda sessão dos testes
   de dois aparelhos; causalidade cruzada A/B/C (`posicao_que_cita_capitulo_de_outra_origem_espera_o_capitulo_chegar`);
   posição exata, empatada, de item ausente e de outro pai; pai trocado em `story`/`book`/`chapter`.
14. **Posição por item (B2.2):** histórias, livros, capítulos, propriedades, anexos e cards criados ao mesmo
    tempo nos dois aparelhos convergem **sem nenhuma divergência** e mostram a **mesma ordem**; o mesmo
    capítulo movido nos dois lados diverge só nele; movido num lado e excluído no outro não perde nada;
    mover o card e editar o conteúdo não conflitam; excluir um capítulo não revisa os irmãos. Os testes
    rodaram contra listas e posições convivendo antes de remover as listas, e falharam pelo motivo certo.
15. **Grupos de mutação (B2.2):** livro excluído num lado com capítulo criado no outro fica inteiro;
    exclusão bloqueada por anexo concorrente não deixa posição órfã, e "manter o local" restaura a ação
    inteira; ação que chega pela metade não toca o domínio até completar em outra sessão; falha injetada no
    meio do grupo não deixa membro aplicado; lote menor que a ação entrega a ação inteira; o grupo entra na
    assinatura, fica fora da revisão, e evento sem grupo mantém a assinatura antiga.

## 5. Negociação de compatibilidade (etapa E)

Um número de versão genérico não detecta dois aparelhos que canonicalizam diferente. O hello troca:

| campo | recusa quando |
| --- | --- |
| `syncProtocolVersion` | diferente |
| `canonicalFormatVersion` | diferente — mudou o que entra no payload canônico |
| `hashAlgorithm` | diferente (`sha256` hoje, em `compute_revision`) |
| `schemaVersion` | incompatível pelo contrato de schema |

E, para não depender só de declaração, o hello inclui um **vetor de prova**: a revisão de um agregado
fixo e conhecido (payload canônico de referência compilado nos dois lados). Revisões diferentes ⇒
`INCOMPATIBLE_CANONICALIZATION`, antes de qualquer reconciliação.

## 6. Legado e conversões

- `blob_backfill` (imagens antigas) roda **antes** da gênese (etapa C).
- Depois que o banco entra no V2, toda mutação sincronizável causada por conversão ou migração de dados passa
  pela `Mutacao` — ou por mecanismo que produza exatamente o mesmo resultado causal, documentado e testado.

## 6.1 Decisões registradas para a B6, antes da gênese

Duas coisas precisam estar resolvidas **antes** de a etapa C adotar os bancos, e nenhuma delas entra na
B3:

1. **`UNIQUE(entity_id)` em `canvas_entity_positions`.** Hoje a PK é `(universe_id, entity_id)` e o
   agregado é identificado só pela entidade. O código já trata duas linhas como inconsistência (8.1), mas
   a restrição física só pode ser adicionada depois de **auditar bancos existentes**: se algum acervo real
   tiver duas linhas para a mesma entidade, a migration precisa decidir qual fica antes de criar o índice.
   Decisão a tomar na B6, com a auditoria na mão.
2. **`entity_template_set` em vez de linha por linha.** `entity_templates` (fichas em branco por tipo, por
   universo) não tem escritor no app e é consumido **em conjunto** pela criação de entidade. Sincronizar
   cada linha pelo `id` legado faria dois aparelhos criarem "o mesmo template" com ids diferentes. A
   modelagem a implementar na B6:

   ```text
   entity_template_set
   identity = (universeId, entityType)
   payload  = [ { key, defaultValue }, … ]    em ordem canônica (sort_order, key)
   ```

   Determinístico, sem id de linha no payload, e é o conjunto inteiro que vira uma revisão. (Diferente
   de ordem: a B2.2 mostrou que ordem como conjunto inteiro conflita onde não há conflito.)
3. ~~**`attachment_order(owner)`**~~ e 4. ~~**`planning_field_order(universe)`**~~ — **resolvidos pela
   B2.2**, e não como agregados de lista. Os testes da B6 mostraram que lista inteira transforma criações
   concorrentes compatíveis em divergência. A ordem da galeria e a das propriedades convergem agora por
   `attachment_position` e `planning_field_position`, um agregado por item.

## 7. Subdivisão da B

Nenhuma PR declara cobertura completa; cada uma atualiza as suas linhas da seção 2.

```text
B1  infraestrutura: Mutacao, exclusão com preflight, exclusão remota bloqueada;
    chapter (update, delete) + attachment migrados; gates 1–4        — sem cobertura nova além disso
B2  manuscrito: universe, story, book, chapter create, chapter_order, custom fields, tag assignments por gatilho
B2.1 story_order(universe), book_order(story) — antes da C       (substituída pela B2.2)
B2.2 posição por item no lugar das listas inteiras; grupos de mutação atômicos  ← implementada
B3  entidades: entity (+atributos), relation, timeline_event, canvas_entity_position  ← implementada
B4  planejamento: planning_item, planning_order, planning_field_definition (gatilho que reescreve cards)
B5  conhecimento e canvas: content_tag (+update_tag), tag_assignment, canvas_node,
    canvas_node_position, canvas_edge; attachment definitivo; gate autoral × efêmero  ← implementada
B6  conteúdo final de colaboração aprovada e conversões de legado pela Mutacao;
    entity_template_set; decisão do UNIQUE(entity_id) da posição; gate de cobertura total
```

## 8. Payload canônico do manuscrito (B2)

Um formato, uma função por tipo: `sync_codec::ler_canonico` é usada pela `Mutacao` e será usada pela
gênese (etapa C). **Mesmo estado ⇒ mesmo payload ⇒ mesma revisão.** O formato está fixado por vetor em
`manuscrito::tests::formato_canonico_fixado_por_vetor`; mudá-lo exige `canonicalFormatVersion` novo (seção 5).

Regras comuns: JSON compacto de `serde_json`, campos na ordem abaixo, nomes em camelCase, strings como
estão no banco (sem normalização Unicode), campo desconhecido **recusado** na leitura (`deny_unknown_fields`).
Campos personalizados: `[{key, value}]` na ordem `sort_order, key`.

| tipo | id do agregado | payload | fica de fora |
| --- | --- | --- | --- |
| `universe` | `universes.id` | `{id, name, description, coverBlobHash, coverMimeType, customFields}` | `created_at`, `updated_at`, `cover_image` (bytes legados) |
| `story` | `stories.id` | `{id, universeId, name, description, customFields}` | `sort_order` (→ `story_position`), timestamps |
| `book` | `books.id` | `{id, storyId, name, description, coverBlobHash, coverMimeType, customFields}` | `sort_order` (→ `book_position`), timestamps, `cover_image` |
| `chapter` | `chapters.id` | `{id, bookId, title, content, summary, sceneOrigin, sceneDestination, status, canonStatus, customFields}` | `sort_order` (→ `chapter_position`), `word_count` (derivável), timestamps |
| `story_position` | `stories.id` | `{storyId, universeId, sortOrder}` | timestamps |
| `book_position` | `books.id` | `{bookId, storyId, sortOrder}` | timestamps |
| `chapter_position` | `chapters.id` | `{chapterId, bookId, sortOrder}` | timestamps |
| `planning_field_position` | `planning_field_definitions.id` | `{fieldId, universeId, sortOrder}` | timestamps |
| `attachment_position` | `attachments.id` | `{attachmentId, ownerType, ownerId, sortOrder}` | `created_at` |
| `planning_item_position` | `planning_items.id` | `{itemId, universeId, status, sortOrder}` | timestamps |
| `tag_assignment` | `tagId:ownerType:ownerId` | `{tagId, ownerType, ownerId}` | `id` da linha (local), `created_at` |
| `entity` | `entities.id` | `{id, universeId, type, name, description, summary, canonStatus, imageBlobHash, imageMimeType, attributes:[{key,value}], customFields:[{key,value}]}` | timestamps, `image` legada, ids e `sort_order` das linhas internas |
| `relation` | `relations.id` | `{id, universeId, sourceId, targetId, type, label, bidirectional, importance}` | `created_at` |
| `timeline_event` | `timeline_events.id` | `{id, universeId, title, description, eventType, startDate, endDate, entityId, displayDate, sortKey}` | timestamps |
| `canvas_entity_position` | `entities.id` | `{entityId, universeId, positionX, positionY}` | `updated_at` |
| `planning_item` | `planning_items.id` | `{id, universeId, chapterId, title, description, targetWords, imageBlobHash, imageMimeType, values:[{fieldId,value}], links:[{fieldId,kind,targetId}]}` | `status` e `sort_order` (→ `planning_item_position`), timestamps, `image` legada, ids das linhas de ligação |
| `planning_field_definition` | `planning_field_definitions.id` | `{id, universeId, name, fieldType, options, scope, ownerItemId}` | timestamps, `sort_order` (→ `planning_field_position`) |

Decisões que valem conferir na revisão:

- **`word_count` fora.** Quem recebe recalcula em Rust com a mesma regra do editor
  (`sync_codec/palavras.rs`); a paridade é conferida pelos dois lados sobre
  `src-tauri/fixtures/contagem_de_palavras.json` (Rust e `tests/word-count-parity.test.mjs`).
- **Posição por item (B2.2).** A ordem é um número por item, materializado no `sort_order` que já existe —
  sem tabela nova. Toda listagem ordena por `sort_order, id`: empate é estado válido (duas criações offline
  podem receber o mesmo número) e se resolve igual em todo aparelho. Nada é compactado na exclusão.

  ```text
  criar     item + posição(item)            MAX(sort_order) + 1
  excluir   posição(item) + item            os irmãos não ganham revisão
  mover     só as posições que mudaram      conflito na unidade certa
  ```

- **Capa legada bloqueia.** Capa ainda em base64 (`hash` vazio) não vira payload: a leitura canônica recusa
  e a mutação falha sem alterar nada, até o backfill converter.

**Decisões da B3 que valem conferir:**

- **Atributos são internos.** `entity_attributes` não tem evento próprio: salvar ou remover um atributo é
  **uma revisão da entidade**, e a lista do payload substitui a daqui inteira na aplicação.
- **`entityId` do evento é mutável só para nulo.** É a única transição que o app produz (`SET NULL` na
  exclusão da entidade). Apontar para outra entidade é inconsistência.
- **A entidade de um evento se destaca, e não se reancora.** Para uma linha que já existe:

  ```text
  Some(E) → Some(E)    ok
  Some(E) → None       ok       é o SET NULL da exclusão da entidade
  Some(E1) → Some(E2)  recusa
  None → Some(E)       recusa
  None → None          ok
  ```

  Linha nova nasce com entidade ou sem, à vontade.
- **Uma entidade tem uma posição.** O agregado é identificado só pela entidade, e a PK física é
  `(universe_id, entity_id)`: o schema deixaria duas linhas. `ler_posicao` responde 0 → não existe,
  1 → estado canônico, mais de uma → **erro de inconsistência** (nunca escolhe uma). Um evento que
  criaria a segunda é recusado sem gravar.
- **Números.** `sortKey` e `positionX/Y` são `REAL`. A serialização do `serde_json` é a representação
  mínima que faz round-trip — determinística para os mesmos bits; valor não finito é recusado na leitura.
  Dois aparelhos convergem no payload quando têm o mesmo `f64`, que é o que a replicação entrega.
- **Posição do grafo é autoral e sincroniza; viewport não.** Zoom, pan, seleção e hover vivem na memória
  do componente e não passam por serviço nenhum. `clear_layout` exclui cada posição, com evento.

**Decisões da B4 que valem conferir:**

- **Valores do card: duas tabelas, um conceito, sem duplicata.** `custom_field_values` (JSON) guarda os
  **escalares**; `planning_field_links` guarda as **relações** (história, entidade, tag). A migration 13 já
  moveu as relações que a build de desenvolvimento havia escrito no JSON para a tabela normalizada e
  limpou o JSON — não há legado a migrar nem duas fontes de verdade. As duas coisas são estado interno do
  card: mudar um campo é **uma revisão do `planning_item`**, nunca um evento por linha ou por link.
- **Etapa e posição são do lugar do card, não do conteúdo.** Arrastar altera só `planning_item_position`
  do card arrastado; o conteúdo não ganha revisão. Salvar a ficha também revisa a posição quando a etapa
  muda — e não emite nada se ela não mudou. (Até a B2.2 isto era uma lista do universo, `planning_order`.)
- **`ownerItemId` é dependência causal explícita.** Um campo de escopo `card` não existe sem o card dono e
  some com ele (FK `owner_item_id ON DELETE CASCADE`), então excluir o card declara a exclusão do campo.
  Escopo `universal` com dono, ou escopo `card` sem dono, é inconsistência.
- **`sort_order` das definições ficou fora do payload do campo** e vive em `planning_field_position` (B2.2).

### 8.1 Contrato da aplicação remota (upsert)

```text
dependencias(evento)                         ANTES de escrever
  Err(inconsistência)   nunca fica válida    → erro; a sessão para; nada é marcado
  Ok(Some(falta))       ainda pode chegar    → PrecisaReconciliar; não aplica, não marca, cursor espera
  Ok(None)              → aplica
conferir_materializacao(evento)              DEPOIS de escrever, na mesma transação
  ler_canonico(agregado).payload == envelope.payload   (delete: ler_canonico == None)
  diferente → erro; a transação inteira desfaz
```

| caso | resultado |
| --- | --- |
| pai ainda não existe (`story` sem universo, `book` sem história, `chapter` sem livro, `tag_assignment` sem tag ou dono) | `PrecisaReconciliar` |
| posição de item que ainda não existe aqui | `PrecisaReconciliar` |
| posição com pai diferente do item daqui | erro — o pai é imutável |
| agregado já existe aqui com **outro pai** (`story.universeId`, `book.storyId`, `chapter.bookId`) | erro — **o pai é imutável**: nenhuma escrita do app move história, livro ou capítulo; um evento que diga outro pai descreve outra árvore |
| posição empatada com a de um irmão | aplica — empate é válido, a listagem desempata pelo `id` |

**Invariante estrutural:** `sync_aggregate_state.current_rev` só é confirmado apontando para uma revisão
materializada — nunca para conhecimento causal apenas. Por isso nenhuma escrita local parte de uma
revisão que o domínio nunca teve. Os envelopes continuam guardados no log; só não ficam falsamente
aplicados. No fim de `receber_eventos`, todo agregado tocado na sessão, sem decisão aberta nem evento
pendente, é conferido: revisão corrente presente ⇒ `ler_canonico == payload(revisão corrente)`; diferente,
a sessão inteira não é confirmada.

A **ponte de ordem** da B2.1 saiu na B2.2: ela só existia para destravar listas inteiras que citavam
filhos de outras origens. Posição por item depende só do próprio item, que é criado antes dela na mesma
ação.

Se mover capítulo entre livros virar funcionalidade, o contrato muda para "atualiza a FK na mesma
transação" — e a checagem de materialização continua a mesma.

**Uma validação, dois lados (B3).** `sync_codec::entidades::validar` é a mesma função para o apply
remoto (`dependencias`) e para a emissão local (`sync_codec::validar_para_emissao`, chamada pela
`Mutacao` antes de emitir cada evento). A invariante:

> Nenhum evento produzido localmente pode ser estruturalmente inválido para o próprio apply remoto.

No lado remoto, dependência ausente é espera (`PrecisaReconciliar`); no lado local é **erro**, porque o
estado já está escrito — e o erro acontece dentro da `Mutacao`, então domínio e evento voltam juntos.
Cobre: `relation.universeId == source.universeId == target.universeId`; evento com entidade no mesmo
universo; posição no mesmo universo da entidade.

**Entidade, relação, evento e posição (B3):**

| caso | resultado |
| --- | --- |
| `entity` com `universeId` diferente do daqui | erro — o universo da entidade é imutável (nenhuma operação do app move entidade de universo) |
| `relation` cujo `sourceId`/`targetId` não existe aqui | `PrecisaReconciliar` |
| `relation` cuja ponta existe **em outro universo** | erro — estado incompatível |
| `relation` com ponta ou universo diferente do que já está aqui | erro — as pontas são imutáveis (o app só cria e exclui relação) |
| `timeline_event` com `entityId` que não existe aqui | `PrecisaReconciliar` |
| `timeline_event` cujo `entityId` aponta para OUTRA entidade que não a daqui | erro — trocar a entidade de um evento não é operação do app |
| `timeline_event` com `entityId: null` sobre um evento que tinha entidade | aplica — é a reescrita do `SET NULL` |
| `timeline_event` com `entityId` sobre um evento que **já está nulo** aqui | erro — a entidade se destaca e não se reancora |
| `canvas_entity_position` cuja entidade não existe aqui | `PrecisaReconciliar` |
| `canvas_entity_position` cuja entidade **já tem posição em outro universo** | erro; a segunda linha não é criada |
| `sortKey`/`positionX`/`positionY` não finito | erro — não há payload canônico para NaN ou infinito |

**Concorrência nunca altera o estado vivo antes da decisão (B4).** Uma exclusão remota que reescreveria um
card com edição concorrente **não executa**:

```text
A edita o valor do campo F no card       B apaga o campo F
A recebe:  reescrita do card (sem F)  → concorrente → divergência; o card de A fica intacto
           exclusão de F              → preflight vê o card divergente → o DELETE não roda
                                      → parent_deletion_blocked
resolvendo: manter o local  → campo e valor continuam
            aceitar remoto  → campo e valor somem, e o card ganha revisão por isso
```

A resolução que aceita a exclusão **declara a reescrita dos sobreviventes**: o card muda porque o campo
deixou de existir, e isso tem de ser uma revisão dele, não uma alteração muda. A decisão sobre a
divergência do próprio card (concorrente) continua sendo da etapa F.

**Card, quadro e propriedade (B4):**

| caso | resultado |
| --- | --- |
| `planning_item` com `universeId` diferente do daqui | erro — o universo do card é imutável |
| `chapterId` que não existe aqui | `PrecisaReconciliar` |
| `chapterId` de capítulo de **outro universo** | erro |
| valor ou ligação citando `fieldId` que não existe aqui | `PrecisaReconciliar` |
| `fieldId` de outro universo, ou exclusivo de outro card | erro |
| ligação cujo alvo (história, entidade, tag) não existe aqui | `PrecisaReconciliar` |
| ligação cujo alvo está em outro universo | erro |
| `planning_item_position` de card que não existe aqui | `PrecisaReconciliar` |
| `planning_item_position` com etapa desconhecida ou card de outro universo | erro |
| `planning_field_definition` com escopo e dono incoerentes, ou `options` que não é lista | erro |

**Asserção geral de materialização:**

- **Por evento (em produção):** após todo `Applied::Aplicado`, o agregado aplicado é o payload do evento.
  Agregado sem linha própria (a posição) é exceção só no delete: ele some com o item, que vem depois
  na mesma ação.
- **Em repouso (nos testes):** depois de cada sessão, todo agregado **coberto** (a lista vem de
  `sync_codec::TIPOS_COBERTOS`, não de uma lista à mão — a primeira versão desta asserção citava só os
  tipos da B2 e por isso não cobria B3 nem B4) com revisão corrente e sem decisão
  aberta nem evento pendente tem `ler_canonico == payload da revisão corrente`. Não vale **entre** dois
  eventos de uma mesma mutação para os OUTROS agregados dela (capítulo já excluído, ordem ainda por chegar):
  uma mutação vira vários eventos. **Isso foi fechado na B2.2**: os eventos de uma mutação formam um grupo,
  e o receptor aplica o grupo inteiro num `SAVEPOINT` ou nenhum membro dele (4.6).

**Sessão com dependência entre origens.** (B2) A drenagem repete as origens até nenhuma aplicar nada. Antes,
cada origem era drenada uma vez em ordem de `device_id`: a ordem de A que cita um capítulo de C ficava
pendente até a sessão seguinte se A viesse antes de C. O teste de três aparelhos encontrou isso.
