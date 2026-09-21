# E0-beta — o estado causal que a 0.10.0-beta.1/beta.2 deixa depois do upgrade

Etapa E, item 6. **Auditoria, não decisão.** Nenhuma migration, reset ou filtro foi implementado.

## Como foi medido

Nada aqui é leitura de código apenas. Os bancos foram **gerados pelo código real da beta.2**
(`app-v0.10.0-beta.2`, `ccf2db2`) num worktree separado e depois passados pelo caminho de upgrade
do aplicativo atual: migrations 21..26 aplicadas uma a uma, como o plugin faz; identidade carregada
do arquivo da beta; e `arranque::preparar_acervo`, que faz o backfill e a gênese.

Cenário gerado com a beta.2 — dois aparelhos, A e B:

```text
A  cria universo, história, livro, capítulos C1..C4 (sem evento: a beta não emitia criação)
A  edita C1 duas vezes                 → create causal (base raiz) + edit
A  edita C3 e depois APAGA C3          → a beta não emitia exclusão de capítulo
A  cria anexo X1; cria X2 e APAGA X2   → upsert, upsert, delete com tombstone
B  (virgem) pareia com A por PIN       → bootstrap
B  edita C2; A edita C1; sincronizam   → cada lado recebe um evento remoto
```

O harness está fora do repositório (`D:\DevTools\NarraHubTmp\auditoria_beta.rs` e
`beta2-wt/…/gerador_beta.rs`). Ele vira gate quando a correção for decidida.

## O que a beta escrevia no log

Só dois caminhos de produção emitiam evento:

| ação | evento | payload |
| --- | --- | --- |
| `update_chapter` | `chapter` upsert (o primeiro, com base raiz, é o "create" causal) | `Chapter` serializado **inteiro, em snake_case** (`book_id`, `word_count`, `sort_order`, `created_at`…) |
| `create_attachment` | `attachment` upsert | `Attachment` em snake_case (`universe_id`, `owner_type`, `blob_hash`, `data_url` vazio) |
| `delete_attachment` | `attachment` delete + `sync_tombstones` | vazio |
| criar/apagar capítulo, livro, história, universo | **nenhum** | — |

Sem `mutation_id`: todo evento beta é "isolado", e a assinatura dele não cobre grupo.

## Tabela por tabela, depois do upgrade

| tabela | o que sobrevive | problema |
| --- | --- | --- |
| `sync_events` | **todos os envelopes beta, intactos**. A migration 23 só acrescenta colunas de grupo com os valores padrão (`mutation_id = ''`) | a assinatura continua **válida** para o protocolo 1 (`canonical_bytes` sem grupo), e o payload está num formato que o codec atual **recusa** |
| `sync_revision_history` | as revisões beta de `chapter` e `attachment` | a revisão corrente de agregado editado na beta aponta para payload beta |
| `sync_aggregate_state` | `current_rev` beta para C1, C2, C3 e X1 | a gênese **pula** quem já tem revisão corrente: C1, C2 e X1 ficam com revisão que **não descreve** o banco (payload beta ≠ canônico). C3 tem estado sem domínio e sem tombstone |
| `sync_tombstones` | o tombstone beta de X2 | não é problema em si; é por causa dele que um reset ingênuo é perigoso (ver adiante) |
| `sync_applied_events` | marca os eventos beta como aplicados | coerente com o log local |
| `sync_cursors` | cursores beta por origem (em B: baseline do bootstrap + seq) | pisos que um peer do protocolo 1 não conhece |
| `sync_divergences` | vazio no cenário; migrations 21/22/24 só acrescentam colunas | nada medido de anormal |
| roster (`sync_devices`) | os dois aparelhos `active`, pareados na beta | um par que continua beta fica `active` para sempre: o `Hello` recusa a sessão, e nada o aposenta sozinho |

Revisão corrente × domínio em A upgradado (medido):

```text
attachment/X1   NÃO descreve (payload beta)
chapter/C2      NÃO descreve (payload beta — a edição veio de B)
chapter/C1      NÃO descreve (payload beta)
chapter/C3      domínio NÃO existe (apagado na beta sem tombstone)
todo o resto    descreve (gênese de agora)
```

## Por cenário

| cenário da beta | depois do upgrade |
| --- | --- |
| **create** (1ª edição de capítulo, criação de anexo) | envelope beta no log, com revisão corrente beta; payload recusado pelo codec |
| **edit** | idem; a cadeia de revisões beta continua sendo a história local do agregado |
| **delete de anexo** | tombstone beta preservado; o envelope delete é o único que o codec aceita (não tem payload) |
| **delete de capítulo** | sem evento e sem tombstone; fica `sync_aggregate_state` órfão. Se o envelope beta de criação de C3 fosse aplicado por alguém, **ressuscitaria** o capítulo |
| **recebeu evento remoto** | o envelope de B fica no log de A e é **retransmitido** por A (origem B) |
| **dois devices pareados** | A e B upgradados sincronizam entre si sem cruzar evento beta (os vetores já cobrem), e as gêneses são deterministas (mesma revisão). Se só um atualizar, o `Hello` recusa |

## O que sai por uma sessão do protocolo 1 hoje

**A regra de saída — "nenhum envelope incompatível da beta pode ser retransmitido como protocolo 1"
— é violada hoje.** Medido com A upgradado falando com um aparelho novo Z (vetor vazio, papel Par):

```text
eventos_para(A → Z)            18 envelopes, 8 deles beta
assinatura dos 8 beta em Z     ACEITA
Z recebe o lote                a sessão inteira falha: "payload de chapter … não está no formato
                               canônico: unknown field `book_id`"
```

Não há dano ao domínio de Z: a transação é desfeita. Mas **toda** sessão Par entre A e qualquer
aparelho sem esse histórico falha para sempre, com a causa errada.

E **filtrar os envelopes beta na saída não resolve**, por dois motivos medidos:

1. **Lacuna de `seq`.** Os eventos do protocolo 1 de A (gênese, edições) têm `seq` depois dos beta.
   Sem os beta, Z guarda os 10 (ou 11, depois de uma edição) e aplica **zero**: o cursor contíguo
   espera `seq 1` para sempre.
2. **Base beta.** A primeira edição feita depois do upgrade num capítulo editado na beta sai com
   `base_rev` = revisão beta (medido: `base=4a078c1f`). Um peer sem a história beta classifica isso
   como `Unknown` e fica em `PrecisaReconciliar`.

**O bootstrap não retransmite envelope beta** (o bundle não leva log, e o incremental começa no
baseline), mas leva adiante o `sync_aggregate_state` beta: o receptor nasce com a revisão corrente
de C1, C2 e X1 apontando para payload beta, e com o estado órfão de C3.

## Por que um reset causal ingênuo é perigoso

- **Tombstone de X2.** Apagar o histórico beta e refazer a gênese sobre o domínio atual esquece que
  X2 foi apagado. Um terceiro aparelho beta que ainda tenha X2 e faça a própria gênese o criaria de
  novo em A — ressurreição de algo apagado de propósito.
- **Capítulos apagados na beta nunca tiveram tombstone.** C3 sumiu do domínio de A sem registro
  causal. Qualquer aparelho que ainda tenha C3 e faça gênese o traz de volta, e nada no protocolo 1
  sabe dizer que ele foi apagado. Isso existe **mesmo sem reset**: é consequência de a beta não
  emitir exclusão.
- **Cursores.** Zerar cursores sem trocar a identidade da origem reabre lacunas nos peers que já
  viram os `seq` beta.

## O que precisa ser decidido (não decidido aqui)

As formas possíveis, com o que cada uma custa — para a decisão, não como recomendação fechada:

1. **Nova época de identidade no upgrade.** O aparelho que carrega log beta passa a assinar com
   identidade nova. As origens beta viram "legado" e nunca saem pelo protocolo 1, e não há lacuna,
   porque a origem nova começa no 1. As revisões correntes beta são substituídas por gênese nova
   sobre o domínio atual. Custo: preservar os tombstones beta como fatos do protocolo 1 (X2), e
   decidir o que fazer com exclusões que a beta nunca registrou (C3).
2. **Reemissão sob a mesma identidade.** Custo alto: seq já usado e assinado não pode ser reemitido
   com outro conteúdo (é exatamente o problema de "duas coisas com a mesma coordenada causal" da
   etapa 12).
3. **Piso negociado por origem** (o `Hello` ou o vetor declara "seq ≤ N da origem X é legado").
   Resolve a lacuna, não resolve a base beta nem o `current_rev` que não descreve o banco.

Nas três, o gate de saída é o mesmo que falha hoje: A upgradado + aparelho novo em papel Par =
nenhum envelope beta enviado, nenhuma lacuna, nenhuma base desconhecida, e nenhum agregado
apagado na beta ressuscitado.
