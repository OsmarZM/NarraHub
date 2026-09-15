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
| **B2** | integrada (#59) | `universe` create/update; `story`, `book`, `chapter` create/update/delete; `chapter_order` (create/delete de capítulo e reorder); campos personalizados dentro desses agregados; `tag_assignment` apagado pelos gatilhos do manuscrito; impacto de exclusão `Excluido`/`Reescrito`/`Bloqueado` (4.3); catálogo de efeitos com gate (3); payload canônico definitivo (8); dependência de criação pai → filho na aplicação remota; capa de livro pelo blob store |
| **B2.1** | implementada (branch `sync-b2-1-ordem`), em revisão | `story_order(universe)` e `book_order(story)` com o contrato de `chapter_order`; revisão de ordem **superada** (8.1), que destrava ordens entre três origens — inclusive o travamento que existia em `chapter_order` desde a B2 |
| B3–B6 | não iniciadas | ver seção 7 |

**Fora da B2, dito às claras:**

- `delete_universe` **recusa sempre** com "Esta operação ainda depende de tipos que estão sendo migrados
  para o Sync V2." A cascata do universo atinge entidades, relações, linha do tempo, planejamento, tags e
  canvas, que ainda não têm codec. Uma exclusão de universo recebida de outro aparelho vira
  `parent_deletion_blocked` e não apaga nada. Volta quando a última dessas etapas integrar (B5).
- Excluir capítulo ligado a card do planejamento, ou história usada num campo de card, é **recusado**
  (seção 3.2). Volta na B4, quando `planning_item` tiver codec e o efeito virar `Reescrito`.
- Marcar e desmarcar tag (`knowledge_service::set_tag`) ainda não emite evento (B5). A B2 só emite o fim
  das marcações que os gatilhos do manuscrito apagam.
- `attachment` mantém o payload da B1 (com `created_at` e `sortOrder`); a revisão dele é da B5.

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
| `story_order` | `stories.sort_order` de um universo | `universe.id` | universe, stories | **só a ordem** (B2.1) |
| `book_order` | `books.sort_order` de uma história | `story.id` | story, books | **só a ordem** (B2.1) |
| `chapter_order` | `chapters.sort_order` de um livro | `book.id` | book, chapters | **só a ordem**; reordenar não conflita com texto |
| `entity` | `entities`, `entity_attributes`, `content_custom_fields` (owner entity) | `entities.id` | universe | atributos são internos |
| `relation` | `relations` | `relations.id` | 2 entities | independente da entidade |
| `timeline_event` | `timeline_events` | `timeline_events.id` | universe; entity (opcional, `SET NULL`) | |
| `planning_item` | `planning_items` (sem `sort_order`), valores em `custom_field_values`, `planning_field_links` | `planning_items.id` | universe; chapter (opcional) | valores e links são internos |
| `planning_order` | `planning_items.status` + `sort_order` do universo | `universe.id` | planning_items | ordem e coluna do quadro |
| `planning_field_definition` | `planning_field_definitions` | `id` | universe; planning_item (se escopo card) | |
| `content_tag` | `content_tags` | `id` | universe | |
| `tag_assignment` | `content_tag_assignments` | `(tag_id, owner_type, owner_id)` | tag + dono | aresta com identidade própria |
| `canvas_node` | `canvas_nodes` (inclui `x`, `y` persistidos) | `id` | universe | |
| `canvas_edge` | `canvas_edges` | `id` | universe; 2 pontas | |
| `canvas_entity_position` | `canvas_entity_positions` | `entity_id` | entity | posição persistida da entidade no grafo |
| `attachment` | `attachments` (referência de blob) | `id` | dono (chapter / entity / node) | já sincroniza |

**Ordem de dependência** (criação em ordem, exclusão na ordem inversa):

```text
universe
 ├─ story ─ book ─ chapter ─ chapter_order(book)
 ├─ entity ─ relation, canvas_entity_position
 ├─ timeline_event
 ├─ content_tag ─ tag_assignment(dono)
 ├─ planning_field_definition
 ├─ planning_item ─ planning_order(universe)
 ├─ canvas_node ─ canvas_edge
 └─ attachment(dono)
```

### 2.2 Canvas: autoral × efêmero

| estado | onde vive | sincroniza |
| --- | --- | --- |
| posição de nó (`canvas_nodes.x/y`) | banco | **sim** |
| posição persistida de entidade (`canvas_entity_positions`) | banco | **sim** |
| conteúdo de nó, arestas | banco | **sim** |
| zoom, pan, viewport | memória do componente | não |
| seleção, hover, painel aberto | memória do componente | não |

Regra: **só o que está no banco e foi feito pelo usuário sincroniza.** Um gate na B5 confere que nenhuma
tabela sincronizável guarda estado de viewport.

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
| story | book_order(story) | derivado (a ordem existe enquanto a história existe) | Delete | B2.1 | — |
| story | story_order(universe) | derivado (a lista perde o id) | **Rewrite** | B2.1 | — |
| story | planning_item (link interno) | FK `planning_field_links.story_id` CASCADE | **Rewrite** | B4 | **exclusão da história recusada** enquanto houver card com a história num campo |
| story | tag_assignment | gatilho `trg_story_metadata_delete` | Delete | B2 | — |
| story | (interno) | gatilho `trg_story_metadata_delete` → `content_custom_fields` | Interno | B2 | — |
| book | chapter | FK `chapters.book_id` CASCADE | Delete | B2 | — |
| book | chapter_order(book) | derivado (a ordem existe enquanto o livro existe) | Delete | B2 | — |
| book | book_order(story) | derivado (a lista perde o id) | **Rewrite** | B2.1 | — |
| book | tag_assignment | gatilho `trg_book_metadata_delete` | Delete | B2 | — |
| book | (interno) | gatilho `trg_book_metadata_delete` → `content_custom_fields` | Interno | B2 | — |
| chapter | planning_item | FK `planning_items.chapter_id` **SET NULL** | **Rewrite** | B4 | **exclusão do capítulo recusada** enquanto houver card ligado |
| chapter | chapter_order(book) | derivado (a lista perde o id) | **Rewrite** | B2 | — |
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
Excluido(agregado)    some junto (FK CASCADE, gatilho, existência derivada)
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
  Excluido  → precisa ter sumido → evento delete (descendentes antes do pai)
  Reescrito → precisa continuar existindo → estado canônico relido → evento upsert
  excluídos são emitidos ANTES dos reescritos
  agregado cujo canônico já é o payload da revisão corrente → nenhum evento
COMMIT
```

Na B2 o `Reescrito` real é `chapter_order(livro)` quando um capítulo é excluído. Os `Reescrito` por
`SET NULL` (planning, timeline) são `Bloqueado` até a etapa que cobre o agregado.

**Por que excluídos antes dos reescritos.** Quem recebe materializa cada evento exatamente (seção 8.1).
A ordem do livro sem o capítulo só pode ser materializada quando o capítulo já saiu; se a reescrita viesse
antes, ela esperaria um capítulo sumir que ainda está no banco.

**Exclusão remota com sobrevivente.** Quem recebe a exclusão não bloqueia porque o sobrevivente vai mudar —
a reescrita dele é o evento seguinte da mesma origem. Bloqueia se o sobrevivente tem **decisão aberta** ou
**evento pendente de outra história** aqui (`estado_concorrente_para_evento`). Eventos da mesma origem com
`seq` maior não contam: o lote é guardado inteiro antes de aplicar, e eles são a continuação da mesma
mutação. Entre os dois eventos, o sobrevivente fica momentaneamente diferente da revisão dele; a asserção
geral de materialização vale em repouso (8.1).

**Ordem da cascata do livro:** `chapter_order` (excluído, existência derivada) sai primeiro, depois os capítulos. Quando a exclusão de um
capítulo chega, a ordem já está excluída causalmente (sem revisão corrente) e não conta como sobrevivente
nem como filho vivo.

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
9. **Materialização exata (B2):** cada evento aplicado um por vez com o estado conferido
   (`cada_aplicado_materializa_o_proprio_evento`); asserção geral em repouso depois de toda sessão dos testes
   de dois aparelhos; causalidade cruzada A/B/C (`ordem_que_cita_capitulo_de_outra_origem_espera_o_capitulo_chegar`);
   ordem com capítulo repetido, de outro livro, inexistente e não citado; pai trocado em `story`/`book`/`chapter`.

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

## 7. Subdivisão da B

Nenhuma PR declara cobertura completa; cada uma atualiza as suas linhas da seção 2.

```text
B1  infraestrutura: Mutacao, exclusão com preflight, exclusão remota bloqueada;
    chapter (update, delete) + attachment migrados; gates 1–4        — sem cobertura nova além disso
B2  manuscrito: universe, story, book, chapter create, chapter_order, custom fields, tag assignments por gatilho
B2.1 story_order(universe), book_order(story) — antes da C
B3  entidades: entity (+atributos), relation, timeline_event, canvas_entity_position
B4  planejamento: planning_item, planning_order, planning_field_definition (gatilho que reescreve cards)
B5  conhecimento e canvas: content_tag, tag_assignment, canvas_node, canvas_edge; gate autoral × efêmero
B6  conteúdo final de colaboração aprovada e conversões de legado pela Mutacao; gate de cobertura total
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
| `story` | `stories.id` | `{id, universeId, name, description, customFields}` | `sort_order` (posição local; não há reordenação), timestamps |
| `book` | `books.id` | `{id, storyId, name, description, coverBlobHash, coverMimeType, customFields}` | `sort_order`, timestamps, `cover_image` |
| `chapter` | `chapters.id` | `{id, bookId, title, content, summary, sceneOrigin, sceneDestination, status, canonStatus, customFields}` | `sort_order` (→ `chapter_order`), `word_count` (derivável), timestamps |
| `story_order` | `universes.id` | `{universeId, storyIds}` — ids na ordem `sort_order, id` | posições numéricas |
| `book_order` | `stories.id` | `{storyId, bookIds}` — ids na ordem `sort_order, id` | posições numéricas |
| `chapter_order` | `books.id` | `{bookId, chapterIds}` — ids na ordem `sort_order, id` | posições numéricas |
| `tag_assignment` | `tagId:ownerType:ownerId` | `{tagId, ownerType, ownerId}` | `id` da linha (local), `created_at` |

Decisões que valem conferir na revisão:

- **`word_count` fora.** Quem recebe recalcula em Rust com a mesma regra do editor
  (`sync_codec/palavras.rs`); a paridade é conferida pelos dois lados sobre
  `src-tauri/fixtures/contagem_de_palavras.json` (Rust e `tests/word-count-parity.test.mjs`).
- **Ordem de história e livro: `story_order` e `book_order` (B2.1).** Mesmo contrato de `chapter_order`,
  materializados no `sort_order` que já existe — sem tabela nova. Nos bancos legados, a ordem atual de
  `stories.sort_order`/`books.sort_order` **é** o estado desses agregados; a gênese (C) adota
  `story_order`, `book_order` e `chapter_order` junto com o resto do manuscrito.

  ```text
  create universe  → universe + story_order(universe)
  create story     → story + story_order(universe) + book_order(story)
  create book      → book + book_order(story) + chapter_order(book)
  delete story     → book_order(story), livros…, story; story_order(universe) reescrita
  delete book      → chapter_order(book), capítulos…, book; book_order(story) reescrita
  reorder (futuro) → só a ordem
  ```

- **Capa legada bloqueia.** Capa ainda em base64 (`hash` vazio) não vira payload: a leitura canônica recusa
  e a mutação falha sem alterar nada, até o backfill converter.
- **`chapter_order` existe enquanto o livro existe**, inclusive vazia. `create_book` emite a ordem vazia;
  `create_chapter`/`delete_chapter` a reescrevem na mesma mutação; `reorder_chapters` altera só ela.

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
| pai ainda não existe (`story` sem universo, `book` sem história, `chapter`/`chapter_order` sem livro, `tag_assignment` sem tag ou dono) | `PrecisaReconciliar` |
| agregado já existe aqui com **outro pai** (`story.universeId`, `book.storyId`, `chapter.bookId`) | erro — **o pai é imutável**: nenhuma escrita do app move história, livro ou capítulo; um evento que diga outro pai descreve outra árvore |
| ordem (`story_order`, `book_order`, `chapter_order`) cita filho que não existe aqui | `PrecisaReconciliar` (pode ser de outra origem que ainda não chegou) |
| ordem cita filho repetido | erro |
| ordem cita filho de outro pai | erro (pai imutável: nunca fica válido) |
| existe filho deste pai que a ordem não cita | `PrecisaReconciliar` — nunca "vai para o fim"; **salvo** revisão superada (abaixo) |
| tudo presente | a ordem materializada é **exatamente** a lista |

**Revisão de ordem superada.** Espera pura travava entre três origens:

```text
C cria c3                  ordem Rc = [c1, c2, c3]
A recebe, cria c4          ordem Ra = [c1, c2, c3, c4], base Rc
B recebe tudo:  c4 aplica · Ra espera (base Rc desconhecida) · c3 aplica · Rc espera (c4 não citado)
                → nenhuma destrava a outra
```

`c4` é causalmente posterior a `Rc`, mas eventos de agregados diferentes não carregam essa relação; a
história da própria ordem carrega: há no log um evento cuja base é `Rc`. Regra: ordem **sequencial** que não
pode ser materializada **e** tem sucessor guardado (`base_rev == new_rev` dela) entra na história como
revisão corrente, marcada como aplicada, **sem escrever no domínio** (`Applied::Superado`). O sucessor vira
sequencial e materializa exatamente. Sem sucessor, continua esperando. Ordem concorrente (mesma base que a
daqui) continua virando decisão — superar não é merge. Só vale para agregado de ordem. Uma superada não é
`Aplicado`: a asserção por evento não se aplica a ela, e em repouso a revisão corrente já é a do sucessor.

Se mover capítulo entre livros virar funcionalidade, o contrato muda para "atualiza a FK na mesma
transação" — e a checagem de materialização continua a mesma.

**Asserção geral de materialização:**

- **Por evento (em produção):** após todo `Applied::Aplicado`, o agregado aplicado é o payload do evento.
  Existência derivada (`chapter_order`) é exceção só no delete: ela some com o livro, que vem depois.
- **Em repouso (nos testes):** depois de cada sessão, todo agregado da B2 com revisão corrente e sem decisão
  aberta nem evento pendente tem `ler_canonico == payload da revisão corrente`. Não vale **entre** dois
  eventos de uma mesma mutação para os OUTROS agregados dela (capítulo já excluído, ordem ainda por chegar):
  uma mutação vira vários eventos, e o receptor os aplica um de cada vez. Tornar isso atômico exige agrupar
  eventos por mutação no protocolo — fica registrado para a etapa E.

**Sessão com dependência entre origens.** (B2) A drenagem repete as origens até nenhuma aplicar nada. Antes,
cada origem era drenada uma vez em ordem de `device_id`: a ordem de A que cita um capítulo de C ficava
pendente até a sessão seguinte se A viesse antes de C. O teste de três aparelhos encontrou isso.
