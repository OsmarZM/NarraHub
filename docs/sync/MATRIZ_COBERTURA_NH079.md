# NH-079 — matriz de cobertura do Sync V2 e proposta de fronteira

> 2026-09-15. Pedido antes de implementar a etapa B. Medido no código (`application/*_service.rs`,
> schema final montado aplicando as 20 migrations). **Nada implementado ainda.**

## 1. Onde o domínio é escrito

Todo comando de escrita passa por um serviço de `application/`: nenhum comando Tauri chama
repositório direto. São **50 funções públicas de escrita em 8 serviços**. Hoje:

| serviço | escritas | com transação | geram evento V2 |
| --- | --- | --- | --- |
| `manuscript_service` | 10 | 2 | **1** (`update_chapter`) |
| `canvas_service` | 11 | 5 | **1** (anexos: `create`/`delete_attachment`) |
| `entity_service` | 5 | 3 | 0 |
| `planning_service` | 8 | 4 | 0 |
| `universe_service` | 3 | 2 | 0 |
| `workspace_service` | 5 | 0 | 0 |
| `knowledge_service` | 4 | 1 | 0 |
| `collaboration_service` | 6 | 2 | 0 |

Além dos serviços, mudam domínio **por baixo**:

- **cascata de chave estrangeira** (`ON DELETE CASCADE` / `SET NULL`) em 17 tabelas;
- **6 gatilhos**: `trg_chapter_attachments_delete`, `trg_entity_attachments_delete` (apagam anexos),
  `trg_{story,book,chapter,entity,timeline,planning}_metadata_delete` (apagam atribuições de tag),
  `trg_planning_field_definition_delete`;
- `blob_backfill` (arranque): troca imagem inline por referência de blob em linhas existentes.

## 2. Matriz por agregado

Legenda: ✅ existe · ❌ não existe · — não se aplica.

| agregado (proposto) | tabelas | cria evento local | payload | aplicação remota | create | update | delete / tombstone | depende de | cascata ao apagar |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **universe** | `universes`, `content_custom_fields` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — | tudo do universo (16 tabelas) |
| **story** | `stories` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | universe | books → chapters → attachments, mentions, revisions; tag assignments (gatilho) |
| **book** | `books` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | story | chapters → …; tag assignments |
| **chapter** | `chapters` | ⚠️ só `update_chapter` | ✅ (struct `Chapter`) | ✅ | ❌ | ✅ | ❌ | book | attachments (gatilho), mentions, revisions, tag assignments; `planning_items.chapter_id` → NULL |
| **chapter order** | `chapters.position` | ❌ (`reorder_chapters`) | — | — | — | ❌ | — | book | — |
| **entity** | `entities` + `entity_attributes` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | universe | attributes, relations, mentions, canvas positions, attachments (gatilho), tag assignments; `timeline_events.entity_id` → NULL |
| **entity_template** | `entity_templates` | ❌ (sem serviço de escrita visto) | ❌ | ❌ | — | — | — | universe | — |
| **relation** | `relations` | ❌ | ❌ | ❌ | ❌ | — | ❌ | 2 entities | — |
| **timeline_event** | `timeline_events` | ❌ | ❌ | ❌ | ❌ | ❌ (`rename`) | ❌ | universe, entity? | tag assignments |
| **planning_item** | `planning_items` (+ `custom_field_values`) + `planning_field_links` | ❌ | ❌ | ❌ | ❌ | ❌ (`save_card`, `save_order`) | ❌ | universe, chapter? | field definitions próprias, links, tag assignments |
| **planning_field_definition** | `planning_field_definitions` | ❌ | ❌ | ❌ | ❌ | ❌ (`rename`, `scope`) | ❌ | universe, item? | links |
| **content_tag** | `content_tags` | ❌ | ❌ | ❌ | ❌ | — | ❌ | universe | assignments, planning links |
| **tag_assignment** | `content_tag_assignments` | ❌ (`set_tag`) | ❌ | ❌ | ❌ | — | ❌ | tag + dono | — |
| **canvas_node** | `canvas_nodes` | ❌ | ❌ | ❌ | ❌ | ❌ (+ posição) | ❌ | universe | edges (validação), attachments? |
| **canvas_edge** | `canvas_edges` | ❌ | ❌ | ❌ | ❌ | — | ❌ | 2 pontas | — |
| **canvas_entity_position** | `canvas_entity_positions` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ (`clear_layout`) | entity | — |
| **attachment** | `attachments` | ✅ | ✅ | ✅ | ✅ | — | ✅ | dono (chapter/entity/node) | — |

**Fora do Sync, de propósito** (local ou derivado):

| tabela | por quê |
| --- | --- |
| `mentions` | derivada do texto do capítulo; cada aparelho recalcula (`sync_chapter_mentions`) |
| `chapter_revisions`, `change_log` | histórico local de edição |
| `collaboration_sessions`, `collaboration_contributions` | sessão de colaboração é deste aparelho; **aplicar** uma contribuição muda capítulo/entidade e essa mudança é sincronizável |
| `devices`, `blob_migration_issues` | locais |
| `sync_*`, `sync_peers`, `sync_conflicts` | protocolo; as duas últimas são do V1 |

## 3. Proposta de fronteira

```text
comando Tauri
   │
serviço de aplicação         valida, decide o que muda
   │
Mutacao::executar(database, identidade, |m| { ... })       ◄── fronteira nova, única
   │   BEGIN IMMEDIATE
   │   ├─ o serviço chama os repositórios, como hoje, e DECLARA o que mudou:
   │   │     m.gravou(AggregateRef("chapter", id))
   │   │     m.vai_excluir(AggregateRef("book", id))   ← ANTES do DELETE
   │   ├─ ao fim do closure, para cada agregado declarado, em ordem de dependência:
   │   │     estado = Codec[tipo].ler_canonico(tx, id)    lido DEPOIS da escrita, na mesma tx
   │   │     Some(payload) → evento upsert   ·   None → evento delete + tombstone
   │   └─ append_event_in_transaction(tx, ...)          já existe, assina, grava cursor
   │   COMMIT                                            domínio e eventos juntos, ou nada
   ▼
```

### Por que um ponto só, e não `emit_event()` em 50 lugares

- **Uma ação lógica = um conjunto coerente de eventos.** `save_card` mexe em card, valores e links: o
  serviço declara **um** agregado (`planning_item`) e o codec lê o card inteiro com valores e links. Os
  SQLs intermediários não viram eventos.
- **Payload canônico num lugar.** Hoje o capítulo serializa a struct `Chapter` inteira, com
  `updated_at` e `word_count`. Com o codec, cada tipo tem uma função canônica, que é a mesma que a
  gênese (etapa C) vai usar — gênese e edição não podem discordar sobre o que é "o mesmo conteúdo".
- **Serviço não sabe de sync.** Ele declara "mudei isto"; quem transforma em evento é a fronteira.

### Cascata

`m.vai_excluir(agregado)` consulta `Codec[tipo].descendentes(tx, id)` **antes** do `DELETE`: a
cascata de chave estrangeira e os gatilhos apagariam as linhas antes de alguém lê-las. A fronteira
emite os deletes **dos filhos antes do pai**, do mais profundo para cima. No outro aparelho, cada filho
é removido pela própria regra causal (com tombstone) antes de o pai chegar.

**Risco que a proposta precisa fechar:** um pai apagado aqui e um filho editado lá ao mesmo tempo. O
filho vira `ConcurrentComExclusao` e fica — mas o delete do pai, ao ser aplicado, dispararia a cascata
SQL e apagaria o filho em silêncio. A aplicação remota de delete de pai precisa **recusar a cascata
enquanto houver descendente com divergência aberta ou estado desconhecido**: o pai fica, e a
divergência sobe para o pai. Sem isso, perda silenciosa.

### Garantia de atomicidade — resposta direta

**Sim: domínio e evento na mesma transação `IMMEDIATE`**, como `update_chapter` já faz. O `COMMIT` do
SQLite (WAL) é atômico: após uma queda existe **ou** o domínio alterado **e** o evento, **ou** nenhum
dos dois. Os dois estados proibidos não são alcançáveis por construção, e não dependem de disciplina do
chamador — a fronteira é o único jeito de obter a `Transaction` de escrita num serviço sincronizável.

### Como a regra vira gate, e não promessa

1. **Estrutural:** um teste proíbe `database.write()` nos serviços sincronizáveis fora de
   `Mutacao::executar` (lista explícita de exceções: leituras, `mentions`, colaboração local).
2. **Comportamental:** para cada comando de escrita, um teste roda a operação e confere, para **todo**
   agregado sincronizável do banco, que `Codec::ler_canonico` == payload do último evento aplicado
   (`sync_aggregate_state` → revisão → evento). Linha de domínio sem representação causal, ou
   representação sem linha, reprova nomeando o agregado.
3. **Aplicação remota:** o mesmo teste roda nos dois sentidos com dois bancos: operação no A, eventos
   para o B, e o estado canônico do B igual ao do A.

### O que a proposta não muda

`sync_events`, assinatura, `seq` contíguo, cursores, `classify`, divergências, tombstones, bundle de
bootstrap e roster ficam como estão. A fronteira usa `append_event_in_transaction` e `apply_remote_event`
existentes; o que se acrescenta são **codecs por tipo** (ler canônico, descendentes, aplicar upsert,
aplicar delete) e a unidade `Mutacao`.

## 4. Pontos que dependem de decisão

1. **Granularidade:** `entity` inclui atributos; `planning_item` inclui valores e links; `universe` inclui
   campos personalizados. Alternativa: cada linha filha como agregado próprio (mais eventos, menos
   conflito por agregado inteiro). Proposta: **agregado = o que o usuário edita como uma coisa só**.
2. **Ordem de capítulos:** `position` dentro do payload do capítulo (reordenar gera N upserts e
   conflita com edição de texto concorrente) **ou** agregado `chapter_order` por livro (um evento, sem
   conflitar com texto). Proposta: **`chapter_order` por livro**; o mesmo para ordem do planejamento.
3. **Posições de canvas:** sincronizar layout (proposta: sim, é trabalho do usuário) ou manter local.
4. **`blob_backfill`:** roda antes da gênese (etapa C); depois dela, se ainda achar legado para
   converter, precisa passar pela fronteira.
5. **Versão do payload canônico:** entra no hello da etapa E; dois aparelhos com codecs diferentes não
   podem trocar eventos.

## 5. Subdivisão da B em PRs

Nenhuma subdivisão declara cobertura completa; a matriz acima fica no repositório e cada PR atualiza as
suas linhas.

```text
B1  fronteira Mutacao + codecs de chapter e attachment migrados para ela + gates 1–3   (sem cobertura nova)
B2  manuscrito: universe, story, book, chapter (create/delete/ordem), cascata e recusa de cascata remota
B3  entidades: entity (+atributos), relation, timeline_event
B4  planejamento: planning_item, field_definition, ordem
B5  conhecimento e canvas: tag, tag_assignment, canvas_node, canvas_edge, entity_position
B6  colaboração aplicada e blob_backfill passando pela fronteira; matriz 100% e gate de cobertura total
```
