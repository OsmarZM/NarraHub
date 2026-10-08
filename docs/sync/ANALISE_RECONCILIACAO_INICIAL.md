# Análise — reconciliação inicial entre acervos independentes

> 2026-09-15, branch `sync-v2-lan`. Resposta às 13 perguntas antes de implementar. Tudo abaixo foi
> conferido no código; onde for inferência, está dito. Complementa `DIAGNOSTICO_SYNC_V2_LAN.md`.

## Resumo em uma frase

O problema existe, **e é maior do que o cenário descrito**: hoje o Sync V2 só replica **edição de
capítulo e anexos**. Os demais agregados (universo, história, livro, entidade, relação, timeline,
planejamento, tags…) não geram evento nem sabem ser aplicados vindos de outro aparelho — é a
`NH-079`, já registrada. Sem resolvê-la, nem a reconciliação inicial nem a remoção do V1 conseguem
entregar "PC final == Android final".

## As 13 respostas

### 1. O problema existe no código atual?

Sim. `sync_sessao::depois_do_pin` decide o papel por "fresco" de cada lado. Com os dois lados com
acervo, o papel é `Par`: os dois se **admitem no roster** e rodam só o incremental
(`sync_sessao.rs`, `Papel::Par => para_admitir(...)`). Nada compara o estado, nada transfere o que
não é evento, e o resultado da sessão vira "Sincronizado com …".

### 2. Que conteúdo fica invisível ao V2?

Dois conjuntos, somados:

| conjunto | por quê |
| --- | --- |
| **tudo o que existia antes do V2** | nenhuma migration cria baseline; `sync_aggregate_state` só é escrito por evento |
| **tudo o que não é capítulo nem anexo, mesmo criado depois** | só 2 de ~47 escritas públicas chamam `append_event_in_transaction` (`manuscript_service::update_chapter` e `canvas_service`, anexos); `sync_apply::aplicar_no_agregado` só conhece `"chapter"` e `"attachment"` e recusa o resto |

Isso inclui **criar** capítulo, livro, história ou universo: só a *edição* de capítulo gera evento.

### 3. Alguma migration cria baseline para dado legado?

Não. `sync_aggregate_state` e `sync_revision_history` só recebem linhas em
`sync_repository::append_event_in_transaction` (evento local), em `sync_apply` (evento remoto) e no
`semear` do bootstrap. A migration do V2 cria as tabelas vazias.

### 4. Como `aggregate_history()` se comporta num objeto legado?

Devolve tudo vazio: `current_rev = None`, `known_revs = []`, `deleted_rev = None` — **idêntico a um
agregado que nunca existiu**. A primeira edição local de um capítulo legado grava `base_rev = ""`
(raiz), como se o capítulo nascesse ali, embora o texto já existisse.

### 5. Como `classify()` trata um agregado sem histórico V2?

Com `current_rev = None` e evento remoto de base raiz → **`Sequential`** → `aplicar_capitulo` faz
`INSERT … ON CONFLICT(id) DO UPDATE`. **Isso já é perda silenciosa hoje**, num caso real: dois
aparelhos que usaram o **Sync V1** têm os **mesmos ids**. O celular edita o capítulo `c1` (evento de
base raiz); o PC tem `c1` legado com outro texto e nenhum estado V2 → `Sequential` → o texto do PC é
sobrescrito sem divergência registrada.

### 6. O `BootstrapBundle` atual serve para a reconciliação inicial?

Não sozinho. Ele transporta domínio + `sync_devices` + `sync_aggregate_state` +
`sync_revision_history` + `sync_tombstones` + `sync_cursors`, tudo de uma vez, e o `semear` **exige
receptor provadamente vazio**. É semente de um lado para o outro, não união. O codec de linhas
(`sync_bundle_wire`, REAL em bits, BLOB em base64) e a matriz de tabelas (`CATALOGO`) são
reutilizáveis.

### 7. O que precisa ser acrescentado?

1. **Cobertura de eventos (NH-079):** todas as escritas de domínio sincronizáveis emitem evento na
   mesma transação, e `sync_apply` sabe aplicar cada tipo — inclusive exclusão em cascata com
   tombstones dos filhos.
2. **Adoção do legado:** dar a todo agregado sem estado V2 uma revisão de gênese honesta.
3. **Reconciliação no primeiro pareamento:** um passo explícito antes do incremental.
4. **"Sincronizado" calculado por convergência**, não por fila vazia.

### 8. Como diferenciar `never seen` de `deleted`?

O schema já distingue — **quando a exclusão passou pelo V2**: `sync_tombstones` guarda `deleted_rev`,
e `classify` usa isso (`ConcurrentComExclusao`). O que não existe é tombstone para exclusões feitas
fora do V2 (antes dele, ou por tipos que ainda não emitem evento). Para esses, ausência **é**
indistinguível de nunca visto, e a regra da reconciliação precisa ser: **ausência sem tombstone =
nunca visto → une**. Consequência honesta: um item que o V1 copiou para os dois aparelhos e que foi
apagado em um deles antes do V2 volta a aparecer. Isso é preferível a apagar sem prova e fica
documentado.

### 9. Como representar duas raízes independentes do mesmo agregado?

Com o que o schema já tem, sem inventar ancestral:

```text
gênese no PC       base_rev = ""  new_rev = H(""; tipo; id; upsert; payload_PC)
gênese no Android  base_rev = ""  new_rev = H(""; tipo; id; upsert; payload_Android)
```

`compute_revision` é determinística sobre o conteúdo. Então:

| caso | revisões | `classify` no outro lado |
| --- | --- | --- |
| mesmo id, **mesmo conteúdo** | mesma `new_rev` | `AlreadyPresent` — sem duplicar, sem conflito |
| mesmo id, **conteúdo diferente** | revisões diferentes, as duas da raiz | a raiz é conhecida e a corrente é outra → **`Concurrent { base_rev: "" }`** → divergência com as duas versões preservadas |
| id só de um lado | uma revisão | `Sequential` → une |

Duas raízes independentes viram exatamente "dois filhos de `""`" — que é o que elas são.

### 10. Como impedir falso "Sincronizado"?

Estado calculado, não afirmado. Um par só é "sincronizado" quando: pareamento autenticado; adoção do
legado concluída nos dois; o vetor que o outro confirmou (`sync_peer_vectors`) cobre todo o log local;
nada pendente por lacuna; **nenhuma divergência aberta**. Com divergência: "N itens precisam da sua
atenção", nunca ✓.

### 11. Qual será o algoritmo?

**Proposta: a gênese é evento de verdade, e a união passa pelo mecanismo que já existe.**

```text
ADOÇÃO (em cada aparelho, uma vez, idempotente)
  para cada agregado sincronizável SEM linha em sync_aggregate_state e SEM tombstone,
  em ordem de dependência (universo → história → livro → capítulo; universo → entidade → relação …):
    emite um evento upsert assinado por ESTE aparelho, base_rev = "", payload canônico
  → dado legado passa a ter história causal com a raiz honesta

PRIMEIRO PAREAMENTO
  PIN → Noise → prova Ed25519 → admissão direta (sem mudança)
  papel:  vazio × acervo  → bootstrap (otimização que já existe)
          acervo × acervo → sem bootstrap; segue para o incremental
  incremental (sem mudança de contrato): cada lado recebe os eventos do outro
    só num lado     → Sequential        → une
    igual           → AlreadyPresent    → nada muda
    diferente       → Concurrent("")    → divergência, duas versões guardadas
  anexos           → blobs pelo transporte da etapa 13

DEPOIS
  incremental normal; nenhuma comparação completa a cada sync
```

Por que evento, e não manifesto + baseline sem log:

- **uma só fonte de verdade.** O log, o cursor contíguo, a assinatura, a idempotência por `event_id`,
  a retransmissão por relay e a divergência já funcionam sobre eventos. Um manifesto que escreve
  estado causal por fora criaria o segundo mecanismo que você pediu para evitar;
- **não é evento artificial:** é o estado que aquele aparelho de fato tem, assinado por quem o tem,
  partindo da raiz — sem ancestral inventado;
- **retry de graça:** a sessão caída retoma pelo cursor; `event_id` impede duplicar.

O custo, dito às claras: o primeiro sync entre dois acervos grandes transfere o payload de todos os
agregados uma vez (inclusive os iguais, que chegam e viram `AlreadyPresent`), e o log de cada
aparelho cresce na medida do acervo. Pular o payload dos iguais quebraria a contiguidade de `seq` por
origem, que é a garantia de "nada ficou para trás". Um manifesto de revisões **antes** da troca
entra para a tela ("214 só aqui, 87 só lá, 7 divergências") e para a futura verificação de
integridade — não como caminho de escrita.

### 12. Quais migrations?

- **Nenhuma tabela nova para a gênese** — usa `sync_events`, `sync_aggregate_state`,
  `sync_revision_history`.
- A adoção é **código de aplicação** idempotente, rodando no arranque depois do `reconcile_self`, não
  migration SQL: ela precisa da identidade Ed25519 para assinar, e migration não tem.
- Provável: marca de conclusão da adoção por aparelho (para a tela e para o "sincronizado"), se não
  der para derivar barato de "todo agregado tem estado".
- **Backup automático antes de qualquer migration** (pedido seu) — mudança no `database::migrations`,
  não no sync.
- Remover o V1: `sync_peers` e `sync_conflicts` continuam existindo (migrations históricas não se
  reescrevem); uma migration nova pode esvaziá-las, se não houver leitor.

### 13. Riscos para bancos das betas

| banco | risco |
| --- | --- |
| PC vindo da 0.9.2 | adoção emite um evento por agregado no primeiro arranque: tempo e tamanho de banco proporcionais ao acervo |
| Android beta.1/2 com dados de teste | idem, pequeno |
| aparelhos que se usaram pelo **V1** | mesmos ids: conteúdo igual converge; diferente vira divergência (hoje sobrescreve em silêncio — pior); **apagado de um lado antes do V2 ressuscita** |
| capítulos já editados no V2 (têm `base_rev ""` de uma edição) | a raiz deles já existe; a adoção não mexe; o outro lado, com gênese diferente, diverge — correto |
| aparelhos pareados em testes da etapa 14 | já têm causalidade compartilhada; adoção não toca o que tem estado |

## O que isso significa para o plano

A ordem que torna cada passo verificável, sem declarar sincronização que não existe:

```text
A  backup automático antes de migration                    independente, curto
B  NH-079: todas as escritas sincronizáveis geram evento,  o maior; sem ele nada converge
   e todo tipo sabe ser aplicado (com cascata/tombstones)
C  adoção do legado (gênese) + gates dos 9 cenários
D  primeiro pareamento acervo × acervo sem bootstrap;
   "sincronizado" por convergência; contagem de divergências
E  fio: hello com versão de protocolo e schema, códigos de erro, timeouts por etapa,
   escuta que não trava, endereços LAN
F  tela: máquina de estados, etapas, detalhes da conexão, erro visível no celular
G  remover o Sync V1 (só depois de B: antes, removê-lo regride quem cria capítulo no celular)
H  release 0.10.0 estável: Windows (NSIS + MSI) + Android do mesmo commit, falha de um bloqueia os dois
I  backup do banco do seu PC, instalação e roteiro PC ↔ Android
```

`B` é o trabalho grande: ~45 escritas em 8 serviços, cada tipo com payload canônico, aplicação
remota, exclusão em cascata e testes. `E` e `F` resolvem o "nada aconteceu" e podem andar em paralelo
a `B`, mas a release 0.10.0 só faz sentido depois de `D`.
