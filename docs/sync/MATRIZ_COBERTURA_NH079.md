# NH-079 — cobertura do Sync V2: agregados, fronteira `Mutacao` e exclusão

> Documento de arquitetura da etapa B. Revisão 2 (2026-09-15), com os ajustes da revisão humana:
> agregado = unidade de consistência e conflito; exclusão com preflight **antes** do SQL destrutivo;
> gatilhos classificados; canvas autoral × efêmero; negociação de canonicalização. Medido no código
> e no schema final (20 migrations aplicadas).

## 1. Onde o domínio é escrito

Todo comando de escrita passa por um serviço de `application/`; nenhum comando Tauri chama repositório
direto. **50 funções públicas de escrita em 8 serviços.** Hoje só capítulo (edição) e anexo geram evento.

| serviço | escritas | com transação | geram evento V2 |
| --- | --- | --- | --- |
| `manuscript_service` | 10 | 2 | 1 (`update_chapter`) |
| `canvas_service` | 11 | 5 | 1 (anexos) |
| `entity_service` | 5 | 3 | 0 |
| `planning_service` | 8 | 4 | 0 |
| `universe_service` | 3 | 2 | 0 |
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
| **B1** | implementada (PR da branch `sync-b1-mutacao`) | fronteira `Mutacao`; `chapter` update e **delete** (com os anexos por gatilho); `attachment` create e delete; exclusão remota de pai bloqueada e a resolução dela (4.4.1); fronteira com o blob store (4.4.2); migration 21 (`sync_divergences.kind`) |
| B2–B6 | não iniciadas | ver seção 7 |

**Limite conhecido da B1, dito às claras:** excluir um capítulo ainda apaga, pelo gatilho
`trg_chapter_metadata_delete`, as atribuições de tag e os campos personalizados dele sem evento — esses
agregados só entram na B2. A B1 prova a infraestrutura; não declara o capítulo como totalmente coberto.

Criar capítulo (`create_chapter`) também continua sem evento até a B2, e o payload do capítulo ainda
carrega `sort_order`, que sai para o agregado `chapter_order` na B2.

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
| `chapter` | `chapters` (sem `position`), `content_custom_fields` (owner chapter) | `chapters.id` | book | conteúdo, título, status, resumo |
| `chapter_order` | `chapters.position` de um livro | `book.id` | book, chapters | **só a ordem**; reordenar não conflita com texto |
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

## 3. Gatilhos

11 gatilhos **escrevem**; 5 só validam (`RAISE`). Nenhuma escrita por gatilho pode ser invisível à
fronteira.

| gatilho | dispara em | escreve em | classificação | tratamento |
| --- | --- | --- | --- | --- |
| `trg_chapter_revision` | UPDATE `chapters` | `chapter_revisions` | local, fora do sync | nenhum |
| `trg_chapter_history_insert` / `_update` | INSERT/UPDATE `chapters` | `change_log` | local | nenhum |
| `trg_entity_history_insert` / `_update` | INSERT/UPDATE `entities` | `change_log` | local | nenhum |
| `trg_story_metadata_delete` | DELETE `stories` | `content_tag_assignments`, `content_custom_fields` | **outro agregado** (`tag_assignment`) + interno | preflight inclui as atribuições |
| `trg_book_metadata_delete` | DELETE `books` | idem | idem | idem |
| `trg_chapter_metadata_delete` | DELETE `chapters` | idem | idem | idem |
| `trg_entity_metadata_delete` | DELETE `entities` | idem | idem | idem |
| `trg_timeline_metadata_delete` | DELETE `timeline_events` | `content_tag_assignments` | **outro agregado** | preflight |
| `trg_planning_metadata_delete` | DELETE `planning_items` | `content_tag_assignments` | **outro agregado** | preflight |
| `trg_chapter_attachments_delete` | DELETE `chapters` | `attachments` | **outro agregado** (`attachment`) | preflight — **coberto na B1** |
| `trg_entity_attachments_delete` | DELETE `entities` | `attachments` | **outro agregado** | preflight (B3) |
| `trg_planning_field_definition_delete` | DELETE `planning_field_definitions` | `planning_items.custom_field_values` **e `updated_at = datetime('now')`** | **reescreve outros agregados** (todos os cards com o campo) | a exclusão de campo declara upsert de cada card afetado; o `updated_at` por relógio local fica fora do payload canônico (B4) |

Validação apenas: `trg_planning_field_scope_insert`, `_scope_update`, `trg_planning_field_link_validate`,
`trg_sync_events_*`, gatilhos de `sync_devices` / `sync_peer_vectors`.

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

### 4.3 Exclusão: preflight antes do SQL

Depois de `DELETE FROM parent`, a cascata de chave estrangeira e os gatilhos já apagaram os descendentes —
ninguém consegue mais lê-los. Por isso a exclusão é outra sequência, **dentro da mesma transação**:

```text
BEGIN IMMEDIATE
  m.excluir(agregado)
    1 identificar afetados    Codec::descendentes: filhos por FK + agregados apagados por gatilho
    2 carregar causal         current_rev de cada afetado (para o base_rev do delete)
    3 preflight               algum afetado com divergência aberta ou evento pendente (Unknown)?
                                → recusa a exclusão local (erro claro; nada é apagado)
    4 preparar                eventos delete: filhos antes do pai, do mais profundo para cima
  serviço executa o DELETE    cascata e gatilhos apagam as linhas
  fim: confere que cada afetado sumiu (se não sumiu, ROLLBACK)
  persiste tombstones + eventos
COMMIT
```

**Proibido:** descobrir os afetados depois da cascata.

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
B3  entidades: entity (+atributos), relation, timeline_event, canvas_entity_position
B4  planejamento: planning_item, planning_order, planning_field_definition (gatilho que reescreve cards)
B5  conhecimento e canvas: content_tag, tag_assignment, canvas_node, canvas_edge; gate autoral × efêmero
B6  conteúdo final de colaboração aprovada e conversões de legado pela Mutacao; gate de cobertura total
```
