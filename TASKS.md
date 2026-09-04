# NarraHub — Fila de tarefas

`AGENTS.md` diz **como** trabalhar. Este arquivo diz **no que** trabalhar.

Fase ativa: **FASE 4 — Sync V2**. Ver `docs/ai/PROJECT_STATE.md`.

## Regras deste arquivo

- Status: `READY` · `IN_PROGRESS` · `BLOCKED` · `REVIEW` · `DONE` · `BACKLOG`.
- Uma tarefa tem **um** dono ativo. Só pegue tarefas `READY` sem `Owner`.
- Edite **apenas a sua entrada** e commite essa mudança sozinha, para o merge ser trivial.
- Detalhe do trabalho não vai aqui — vai no handoff (`docs/handoffs/`).
- `DONE` exige validação executada, não só build verde.
- Desde a PR #5, `main` é a linha canônica: branch curta a partir de `main`, PR de volta
  para `main`. `main` é protegida — promoção só por PR, nunca por push direto.

---

## ACTIVE

### NH-001 — Tornar `main` canônica

```text
Owner:  Claude
Status: REVIEW — mesclada; falta trocar o default do repositório
PR:     #5 (mesclada em 35ef0fa)
Fase:   0
```

**Contexto (verificado em 2026-08-31):** o default do `origin` é
`feat/native-app-foundation`, que contém 0.8.0, 0.9.0 e 0.9.1. A `main` parou em **0.7.6**.
Ou seja, o diagnóstico se inverte: não é a release que saiu da linha, é a `main` que ficou
para trás.

**Objetivo:** `main` passa a conter a versão publicada mais recente e vira o default do
repositório no GitHub. Release nasce de `main` a partir daí.

**Restrições:** sem force push, sem desativar proteção de branch, sem copiar arquivos entre
branches às cegas.

**Verificado em 2026-08-31, contra `origin/main` (a `main` local estava desatualizada):**

- `origin/main` está em **0.8.0** e tem **um** commit que a branch de trabalho não tem:
  `fd2e88c`, o merge da PR #1. Esse commit **não tem conteúdo próprio** — a árvore dele é
  idêntica à da base comum. Ou seja, `main` não tem uma única linha que a branch de
  trabalho não tenha; ela tem só um artefato de histórico.
- Portanto **não é fast-forward**, mas o merge é limpo. Confirmado localmente: depois de
  `git merge --no-ff feat/native-app-foundation` em `main`, a árvore de `main` fica
  **idêntica** à da branch de trabalho. Nada se perde, nada conflita.
- `main` **já está protegida**: `git push origin main` é recusado com
  `push declined due to repository rule violations`. A promoção precisa passar por Pull
  Request — o que é o comportamento correto e não deve ser contornado.

**Feito:** PR #5 mesclada em 2026-08-31 com o CI verde no commit exato (`98c36ef`).
`main` está em 0.9.1 e sua árvore é idêntica à de `feat/native-app-foundation`.

**Falta, e é decisão humana:**

1. Trocar o default do repositório no GitHub para `main`.
2. Decidir o destino de `feat/native-app-foundation`. Hoje as duas branches são idênticas;
   mantê-la viva como linha de trabalho recria exatamente a divergência que esta tarefa
   acabou de fechar. A recomendação é aposentá-la e passar a abrir branches curtas a
   partir de `main`.

Nenhum agente faz nem 1 nem 2 sozinho, e nenhum agente deve usar force push ou desativar a
proteção da `main` para contornar uma recusa de push.

**Validação:**

```bash
npm run release:validate-version && npm run build && npm run test:architecture
```

**Não tocar:** Sync, IA, páginas grandes.

---

### NH-002 — Validação de versão no CI comum

```text
Owner:  Claude
Status: DONE (4b31646)
Branch: <agente>/NH-002-version-ci
Fase:   0
Depende de: nenhuma
```

**Contexto:** `scripts/validate-release-version.mjs` **já existe** e já compara
`package.json`, `Cargo.toml` e `tauri.conf.json`. Ele só não roda no `ci.yml` — hoje é
usado apenas na pipeline de release.

**Objetivo:** adicionar `npm run release:validate-version` ao job `frontend` do
`.github/workflows/ci.yml`, para versão divergente reprovar em PR.

**Arquivos:** `.github/workflows/ci.yml`

**Validação:** CI verde em um PR; e um PR de teste com versão divergente deve ficar
vermelho.

---

### NH-003 — Cobrir README no validador de versão

```text
Owner:  Claude
Status: DONE (4b31646)
Branch: <agente>/NH-003-readme-version-check
Fase:   0
Depende de: NH-002
```

**Contexto:** o `README.md` anuncia **0.7.4** enquanto os manifests estão em 0.9.1. O
validador atual não olha para o README, então a divergência passou despercebida por três
releases.

**Objetivo:** estender `scripts/validate-release-version.mjs` para exigir que a versão
citada no README bata com a dos manifests.

**Arquivos:** `scripts/validate-release-version.mjs`, `README.md`

---

### NH-004 — Baseline de documentação

```text
Owner:  Claude
Status: DONE (16a7db4)
Branch: <agente>/NH-004-doc-baseline
Fase:   0
Nota: destravada — corrigir a documentação não dependia da main virar canônica.
```

**Objetivo:** `README.md` e `docs/ARCHITECTURE.md` passam a descrever o **estado corrente**.

**Correções conhecidas:**

- README ainda diz 0.7.4.
- `ARCHITECTURE.md` descreve o estado 0.7.x e afirma que parte do CRUD ainda usa SQL no
  Angular — hoje isso é falso e proibido por `tests/frontend-boundaries.test.mjs`.
- Documentar o fluxo real:
  `Angular → Store → Gateway → RustCoreService → Tauri command → Application → Repository → SQLite`.
- Fixar a regra: `CHANGELOG.md` e `docs/releases/` guardam histórico; README e ARCHITECTURE
  descrevem o presente.

---

### NH-005 — Índice de ADRs e adoção do template

```text
Owner:  Claude
Status: DONE
Branch: <agente>/NH-005-adr-index
Fase:   0
```

**Objetivo:** manter `docs/ADR/README.md` como índice vivo dos ADRs 0001–0006 e de todos os
seguintes, com status. Já criado nesta fase — a tarefa é revisar se os status registrados
batem com a realidade do código.

---

### NH-006 — Reconciliar o plano de evolução antigo

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-006-reconciliar-plano
Fase:   0
```

**Contexto:** `docs/ARCHITECTURE_EVOLUTION_PLAN.md` usa a numeração de fases antiga, em que
"Fase 5" era o Context Engine. Ela agora conflita com `docs/ai/ROADMAP.md`, onde Context
Engine é a Fase 6. Um agente que abrir o documento errado implementa a fase errada — que é
exatamente o que a constituição proíbe.

**Objetivo:** o plano antigo passa a ser explicitamente histórico. Ele mantém o registro
valioso que tem (invariantes de domínio, histórico fatia a fatia) e deixa de ser lido como
fila de execução.

**Sugestão:** cabeçalho no topo apontando para o roadmap como única ordem válida, e mover
a seção de invariantes de domínio para um lugar que não seja um plano — elas são regra
corrente, não plano.

**Arquivos:** `docs/ARCHITECTURE_EVOLUTION_PLAN.md`

---

### NH-007 — Mapear cada invariante ao teste que a prova

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-007-mapa-invariantes
Fase:   1
```

**Resultado:** as 11 invariantes estão mapeadas. Dez têm prova no core Rust; a 8 é garantida
pela **ausência** de caminho de escrita — `AiService` não injeta store nem gateway — e por
isso é travada no frontend.

O mapa é executável (`src-tauri/src/domain/invariant_coverage.rs`) e vem com três gates que
impedem a cobertura de envelhecer em silêncio: teste sumido do mapa reprova, invariante fora
do mapa reprova, e injetar um store no `AiService` reprova. Verificados por mutação.

A cobertura já existia; o que não existia era **poder verificá-la**. Antes disso, responder
"a invariante 5 está protegida?" exigia reler o core inteiro.

---

## FASE 1 — Qualification

> **Leia antes de pegar qualquer uma destas.** O plano original supunha que a Fase 1
> começava do zero. Não começa. Levantamento feito em 2026-08-31:
>
> - `src-tauri/fixtures/schema10_representative.sql` **já existe**, com 18 tabelas
>   povoadas (universos, histórias, livros, capítulos, entidades, atributos, relações,
>   menções, tags, timeline, planning, attachments, devices, sync_events, colaboração).
> - `representative_schema10_fixture_upgrades_without_data_loss` **já testa** o upgrade
>   dessa fixture sem perda de dados.
> - Existem testes por migration de v7 a v15, com `pragma_foreign_key_check` em vários.
> - `full_migration_chain_creates_a_reopenable_file_database` roda a cadeia 1→15 num
>   arquivo real, reabre e confere `integrity_check`.
> - `backup.rs` testa backup online com WAL, rejeição por hash divergente, path traversal
>   no manifesto, staging interrompido e retenção que preserva backups pré-restore.
> - Tudo isso **já roda no CI** via `cargo test`.
>
> Ou seja: a rede de segurança de migration e backup existe e é boa. O que falta é menor,
> mais específico, e mais difícil — está listado abaixo. **Não recrie o que já existe.**

### NH-010 — Fixture nativa do schema mais recente

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-010-fixture-nativa
Fase:   1
```

**O escopo mudou depois de olhar o problema de perto**, e ficou mais útil.

A tarefa dizia "fixtures povoadas nos schemas 13, 14 e 15". Mas migrar a fixture de schema
10 pela cadeia já produz um banco em 15 — o que ela **não** produz é o formato **nativo**. A
migration 15 converte todo campo de planejamento pré-existente para universal; só um banco
que nasceu no 15 tem `scope = 'card'` com `owner_item_id`, `custom_field_values` preenchido,
e arestas de canvas ligando entidade a nó.

Qualquer migration 16 vai encontrar os dois formatos no mundo real. Sem a fixture nativa, só
o migrado seria testado — e o nativo, que é o da maioria dos usuários daqui para frente,
chegaria à produção sem nunca ter passado por um upgrade em teste.

Fixtures em 13 e 14 não foram criadas: elas só reproduziriam estados que a cadeia já cobre,
e cada fixture a mais é manutenção a mais. **Se uma migration futura provar o contrário, aí
elas se justificam.**

**Entregue:** `src-tauri/fixtures/schema15_native.sql`, com as formas que só o nativo tem, e
dois testes — um que carrega e confere integridade, FK e essas formas, e o **gate**
`existe_fixture_nativa_para_o_schema_mais_recente`, que reprova quando o
`LATEST_SCHEMA_VERSION` sobe sem ganhar fixture. Verificado por mutação: escondendo o
arquivo, o gate acusa nomeando o que falta.

**Descoberta útil para quem escrever a próxima fixture:** ela quebrou quatro vezes por
motivos que só o schema conhece — `NOT NULL DEFAULT ''` recusando `NULL` explícito,
`owner_type` que aceita `'timeline'` e não `'timeline_event'`, `canvas_edges` que aceita
`'canvas'` e não `'node'`, `planning_items.status` sem `'RASCUNHO'`, e ordem de inserção
importando por causa de `owner_item_id`. Vale dumpar CHECKs e NOT NULLs antes de escrever,
em vez de descobrir um por rodada.

---

### NH-011 — Falha no restore com rollback

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-011-restore-rollback
Fase:   1
```

**Resultado:** o rollback **já existia e já era bom** — `SwapProgress` rastreia cada passo
da troca e `rollback_swap` desfaz exatamente o que foi feito. Faltava cobertura, não
implementação. Agora há dois pontos de falha injetáveis (`AfterActiveMoved`, que cobre o
estado em que o usuário fica sem banco nenhum no disco, e `AfterInstall`) e asserções de
`integrity_check`, `foreign_key_check`, sidecars, assets e limpeza. Verificado por mutação:
sabotar o `rollback_swap` faz os testes acusarem; o teste antigo não acusava.

---

### NH-012 — Ciclo de atualização no app empacotado

```text
Owner:  Claude
Status: DONE — roteiro pronto e três execuções registradas
Branch: claude/NH-012-qualificacao-upgrade
Fase:   1
```

**Entregue:** `docs/QUALIFICATION_UPGRADE.md` — roteiro reproduzível, a fronteira explícita
entre o que o CI já cobre e o que só humano faz, e a tabela de evidência por release.

**Descoberta que muda a execução:** o par a testar é **0.8.0 → 0.9.1**, não a versão mais
recente. 0.7.6 e 0.8.0 são schema 14; 0.9.0 e 0.9.1 são schema 15. Uma 0.9.2 hoje teria
schema 15 e o upgrade **não cruzaria migration nenhuma**. As duas versões já estão
publicadas com instalador, assinatura e `latest.json`, então **não é preciso publicar
release para rodar este teste**.

**Executado em 2026-08-31:** `0.7.4 → 0.9.1` pelo updater interno, no perfil de produção.
Cinco migrations (11 a 15), zero perda em 16 tabelas, texto dos capítulos idêntico por hash,
relações/timeline/menções/tags idênticas linha a linha. Evidência em
`docs/qualification/2026-08-31-upgrade-0.7.4-para-0.9.1.md`.

Segundo boot e conferência na tela: **feitos e aprovados** — schema v15 sem reaplicar, e a
interface abre o capítulo com o texto.

**Segunda execução, 2026-09-01:** o autor repetiu o ciclo **na máquina de outra pessoa** e
passou, sem corromper arquivo. Ambiente alheio vale mais que o do desenvolvedor, porque não
tem o histórico de instalações e perfis que só existe na máquina de quem constrói o produto.
Evidência relatada, não medida — as contagens não foram comparadas lá.

**Terceira execução, 2026-09-01 — a que fecha a tarefa:** o autor rodou `0.8.0 → 0.9.1`, com
o universo contendo propriedades personalizadas de planejamento. Nada se perdeu, o backup foi
criado corretamente, e as propriedades sobreviveram com os valores.

Isso exercita a promessa da migration 15, que as duas execuções anteriores não alcançavam: no
schema 14 a coluna `scope` **não existia**, então toda propriedade era universal por natureza;
a migration adiciona `scope NOT NULL DEFAULT 'universal'`. O autor confirmou ainda que, já na
0.9.1, campos por card e universais convivem corretamente — ou seja, a capacidade nova
funciona **sobre um banco migrado**, que é onde ela teria mais chance de falhar.

**Nunca no perfil de uso diário** quando a origem for mais antiga que o banco instalado: o
app recusa banco de schema mais novo, e a migração é de mão única.

---

### NH-013 — Checklist de release desktop como gate

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-013-checklist-release
Fase:   1
```

**Entregue:** `docs/RELEASE_CHECKLIST.md` e o comando `npm run release:preflight`.

O checklist separa três coisas que o roadmap tratava como uma: o que o CI já garante em todo
PR, o que o workflow de release garante, e o que só humano consegue.

**Correção feita no próprio documento antes de commitar:** a primeira versão dizia "release
sem esta tabela preenchida não é publicável", mas metade dos itens — instalação limpa,
updater detectando, artefatos no destino — **exige que os artefatos já existam**. O gate era
impossível de cumprir como escrito. A tabela humana virou duas: `2.1` sobre um instalador
local, que bloqueia a publicação, e `2.2` sobre a release no ar, que bloqueia o anúncio.

`release:preflight` roda exatamente a lista do job de release. Preflight verde não garante a
release, mas preflight vermelho garante que ela vai falhar — e falhar em minutos aqui é
melhor que falhar no runner.

---

### NH-014 — Quando o próprio rollback falha

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-014-rollback-falho
Fase:   1
```

**Resultado:** `SwapFailurePoint::AfterInstallWithBrokenRollback` faz a restauração falhar
**e** o rollback falhar junto. O teste cobra as quatro coisas que separam o usuário da perda
do livro: a mensagem avisa que o rollback não deu conta, **nomeia** o diretório preservado,
a base anterior continua lá e íntegra com o conteúdo certo, e o manifesto do rollback não é
apagado. Verificado por mutação: tirar o diretório da mensagem faz o teste falhar dizendo
exatamente qual nome faltou.

---

### NH-015 — Modo de recuperação por schema incompatível

```text
Owner:  Claude
Status: DONE — ADR aprovado e implementado em 2026-09-01
Branch: claude/NH-015-recuperacao-schema
Fase:   1
```

**Entregue:** comando `database_compatibility` (Rust, read-only, barato, não falha em
instalação nova), portão no `AppBootstrapService` antes de `db.init()`, e a tela
`SchemaRecoveryComponent` com atualização, restauração de backup **compatível** e a
explicação de que os dados estão intactos.

**Gates:** 5 testes Rust incluindo banco em `LATEST + 1`, e um teste de fronteira que
compara a posição das duas chamadas no bootstrap — inverter a ordem reprova, verificado por
mutação.

**Não verificado:** a aparência da tela rodando. O app precisa do runtime Tauri e o tooling
de browser desta sessão não consegue dirigi-lo. Para ver a tela de verdade, aponte um perfil
descartável para um banco de schema maior que o `LATEST_SCHEMA_VERSION` e rode
`npm run desktop:qualification`.

**Origem:** incidente real de 2026-09-01. O autor instalou a 0.8.0 sobre um banco em schema
15 e **o app não abriu** — sem janela, sem mensagem. Os dados estavam intactos o tempo todo;
a percepção de perda foi total.

**Por que o alerta que já existe não apareceu:** a falha acontece no
`provideAppInitializer`, antes de a interface existir. O `root-layout` tem um alerta pronto
para esse erro, mas ele nunca é renderizado. **Tratamento de erro que só funciona depois que
o app subiu não serve para o erro que impede o app de subir.**

**Desenho aprovado no ADR:** ver
[`ADR 0007`](docs/ADR/0007-modo-de-recuperacao-por-schema-incompativel.md). Não implemente
antes de o ADR sair de `Proposed`.

**As peças já existem**, o que torna isto mais barato do que parece:

- `database_health` → `inspect_database` abre o arquivo com `rusqlite` **independente do
  pool** e devolve `schemaVersion`;
- `LATEST_SCHEMA_VERSION` é conhecido pelo app;
- o manifesto de backup já carrega `schemaVersion`, e a tela de Ajustes já o exibe — então
  filtrar backups compatíveis é imediato.

**Gate:** teste que abre um banco de schema `LATEST + 1` e prova que o pool **não** é
aberto, que o modo de recuperação é ativado, e que a lista de backups exclui os
incompatíveis.

**Cuidado que precisa sobreviver:** a verificação só protege downgrades feitos **a partir**
da versão que a contiver. Voltar para a 0.8.0 continuará sem tela de recuperação, porque a
0.8.0 já está publicada e é imutável.

---

## FASE 2 — Workspace Hardening

> **Leia antes de pegar qualquer uma.** O gate desta fase é um teste de fronteira, não uma
> contagem de linhas: `WorkspaceLayout` não pode conhecer implementação de gateway, montar
> payload de compartilhamento, carregar domínios manualmente nem executar regra de domínio.
> Linhas são consequência, não arquitetura.
>
> **Não crie event bus, CQRS nem mediator.** Application services explícitos primeiro. Se
> eles explodirem de dependências, aí revisamos — com ADR, não no meio de um PR de extração.
>
> E o padrão desta sessão vale aqui também: **verifique antes de implementar.** Cinco vezes
> seguidas o repositório estava em estado melhor do que o plano supunha.

### NH-020 — Extrair `WorkspaceSessionService` (absorve a NH-021)

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-020-workspace-session
Fase:   2
```

**O recorte do plano não sobreviveu ao código, e foi corrigido.** A NH-021 (tirar o preload)
separava `loadWorkspaceData` do `openUniverse` que a chama. Em dois PRs, o estado intermediário
seria pior que o atual: o serviço de sessão abriria o universo mas dependeria do layout para
carregá-lo. As duas viraram uma extração só.

**Resultado:** `WorkspaceSessionService` é dono de abrir, fechar, trocar e esquecer universo;
de zerar os sete stores; da época que descarta pré-carga de universo anterior; e da pré-carga
em si. O layout caiu de 523 para 501 linhas — mas o número não é o ponto, e sim que ele deixou
de saber **quais** stores existem e em que ordem zerá-los.

**Duas fronteiras que o serviço respeita, e que o teste cobra:** ele não conhece Router — quem
navega é o layout — e não conhece SQL. Erro de pré-carga é **devolvido**, não exibido: uma
pré-carga que falha não aborta a abertura, e quem chamou decide como avisar.

**Gate:** `WorkspaceLayout não é dono do ciclo de vida da sessão de universo (Fase 2)`.
Verificado por mutação — devolver `entityStore.reset()` ao layout reprova.

**Nota de asserção:** `universeStore.load()` continua no layout de propósito, e o teste diz
isso. É a lista da biblioteca, não a pré-carga de um universo aberto.

**Objetivo:** abrir, fechar e trocar universo; resetar stores; descartar resposta assíncrona
de um universo anterior. Hoje parte disso vive no layout.

**Comece por aqui:** é o que segura as outras extrações. Sem um dono do ciclo de vida da
sessão, cada extração seguinte teria que inventar o seu.

**Cuidado:** já existe `universeResolver` cuidando da seleção de universo depois do
bootstrap, com teste de fronteira que proíbe ele de tocar em stores de domínio. A extração
não pode duplicar essa responsabilidade — leia `app.routes.ts` e o resolver antes.

---

### NH-021 — Tirar o preload multi-domínio do layout

```text
Owner:  Claude
Status: DONE — absorvida pela NH-020
Fase:   2
```

Separar do `openUniverse` que a chama deixaria um estado intermediário pior que o original.
Ver a NH-020.

---

### NH-022 — `GlobalSearchService` assume o próprio lifecycle

```text
Owner:  Claude
Status: DONE — resolvida pela NH-020, sem código novo
Fase:   2
```

**Verificado antes de implementar, e o problema já não existia.**

A tarefa queria dar ao serviço `initializeUniverse()`, `refresh()`, `query()` e `reset()`
para que o layout não precisasse saber quais stores carregar para pesquisar. Mas:

- o serviço tem **58 linhas** e expõe `results` como `computed` sobre os stores — ele não
  tem estado próprio para inicializar nem resetar; zera junto quando os stores zeram;
- o layout só lê `globalSearch.results`;
- e quem carrega os stores agora é o `WorkspaceSessionService`, desde a NH-020. **O
  acoplamento que esta tarefa atacaria saiu junto com a sessão.**

Acrescentar quatro métodos de ciclo de vida a um `computed` seria cerimônia: API maior,
mesmo comportamento, mais uma coisa para manter em sincronia.

O que sobra no layout é `openGlobalSearchResult`, que **navega** para o resultado — e navegar
é do layout, não do serviço de busca.

**Se um dia a busca ganhar índice próprio** — o `TODO(Fase 4)` no antigo preload aponta para
isso — aí ela passa a ter estado e a tarefa se justifica de novo.

---

### NH-023 — Extrair `WorkspaceShareService`

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-022-023-busca-e-share
Fase:   2
```

**Esta era real.** O layout montava `OnlineShareDocument` e `SharedUniverse`, achatava
entidade com atributos, e **redimensionava imagem em canvas para WebP**. Compressão de imagem
num componente de layout significa que qualquer mudança no formato do documento compartilhado
passa pelo arquivo mais movimentado do frontend.

**Entregue:** `WorkspaceShareService` é dono da montagem do documento e da preparação de
imagem, com os três limites que estavam soltos no código agora nomeados
(`MAX_INLINE_IMAGE_BYTES`, `MAX_IMAGE_EDGE`, `WEBP_QUALITY`).

O layout continua dono do que é dele: abrir a sessão, cifrar, copiar o link e avisar o
usuário. O serviço não conhece `ShellState` — o teste cobra isso.

---

### NH-024 — Ampliar `WorkspaceSyncService` (cross-domain)

```text
Owner:  Claude
Status: DONE
Branch: claude/NH-024-025-cross-domain
Fase:   2
```

Capítulo e entidade já eram coordenados pelo serviço. O que faltava era a revisão de
colaboração aprovada, que mexe em manuscrito, entidades e estatísticas — e vivia no layout,
onde só quem passasse por ali conseguiria reaproveitá-la. Virou `onCollaborationReviewApplied`.

**Achado:** o layout tinha um `refreshUniverseStats` **cópia byte a byte** do que já existia
no serviço, e **ninguém o chamava**. Código morto duplicado, do tipo que só aparece quando se
vai extrair.

---

### NH-025 — Teste de fronteira do `WorkspaceLayout`

```text
Owner:  Claude
Status: DONE — gate da Fase 2
Branch: claude/NH-024-025-cross-domain
Fase:   2
```

`GATE DA FASE 2: as quatro proibições do WorkspaceLayout` prova as quatro regras do roadmap
como código, e não como contagem de linhas. Cinco mutações, cinco reprovações.

**E foi ele que expôs um problema maior.** A primeira versão passava com a violação de
gateway reintroduzida. A causa: eu escrevi as expressões regulares por script Python, e o
`\b` de borda de palavra virou um **caractere de backspace literal** (0x08) — invisível no
`grep`, e que nunca casa com nada.

Uma varredura achou **18 ocorrências, em 5 asserções**, incluindo três de testes anteriores
que eu havia declarado verificados:

| Asserção | Estava morta |
| --- | --- |
| layout não conhece gateway | sim |
| layout não executa SQL | sim |
| sessão não conhece Router | sim |
| sessão não executa SQL | sim |
| `AiService` não contém SQL (invariante 8) | sim |

Todas corrigidas e reverificadas por mutação. **Regra que fica:** escrever regex por script
com escape em camadas é frágil; confira o resultado com `cat -A` ou varra caracteres de
controle antes de confiar num teste novo.

---

## FASE 3 e 3.5 — concluídas em 2026-09-01

### NH-030 — Consolidar o Rust Core

```text
Owner:  Claude
Status: DONE
Fase:   3
```

**A descrição da fase estava errada nas duas direções**, e a revisão arquitetural do autor
mostrou isso:

| O que o roadmap dizia | O que era |
| --- | --- |
| "dois caminhos completos até o banco" | `commands/` tinha **35 linhas**; oito dos dez arquivos eram só comentários de placeholder, e o único comando registrado (`get_app_info`) **não era chamado por ninguém** |
| gate = "sem `commands/` legado" | não pegaria `planning_save_card`, um comando de domínio em `database/planning.rs` com validação, transação e SQL juntos |

**Feito:** `commands/` removido inteiro; `get_app_info` removido em vez de migrado, por ser
código morto; e `planning_save_card` migrado para
`interface/tauri → application → repository`, com os dois testes que o protegiam movidos
junto e passando.

**Gate:** `comando de domínio só nasce em interface/tauri`, com exceções nomeadas para
infraestrutura e a exigência de que cada uma traga o motivo escrito. Duas mutações, duas
reprovações.

---

### NH-031 — Fronteira nativa do frontend

```text
Owner:  Claude
Status: DONE
Fase:   3.5
ADR:    0008
```

Portas separadas para domínio e plataforma; seis serviços movidos para `core/native/`;
`NativeWindowService` criado. O gate varre `src/app` inteiro atrás de `invoke()`,
`api/window` e plugins — substituindo um teste que prometia proteger isso e só procurava a
string `RustCoreService`.

---

### NH-039 — Reconciliar a documentação com a `main`

```text
Owner:  Claude
Status: DONE
Fase:   4 (preparação)
```

Pedida pelo autor antes da NH-040. **Sem mudança de arquitetura nem de código.**

**Contradições encontradas e corrigidas:**

| Onde | Dizia | Realidade |
| --- | --- | --- |
| `ARCHITECTURE.md` | `commands/` "ainda presente", remoção é a Fase 3 | removido na Fase 3 |
| `ARCHITECTURE.md` | `commands/` coexiste, "dois caminhos até o banco" | um caminho |
| `ARCHITECTURE.md` | `WorkspaceLayout` "ainda orquestra domínios demais" | resolvido na Fase 2 |
| `PROJECT_STATE.md` | "resta limpar `commands/` legado" | e três linhas abaixo, "Removido na Fase 3" — o arquivo se contradizia |
| `PROJECT_STATE.md` | dívida: "9 arquivos de comando de cada lado" | zero |
| `PROJECT_STATE.md` | "não trabalhar ainda: Sync V2" | Sync V2 **é** a fase ativa |

A última era a pior: o mesmo arquivo declarava a fase ativa e, algumas linhas abaixo, mandava
não trabalhar nela. Um agente que lê de cima para baixo obedece a última instrução que viu.

**E o pedido de tornar as afirmações verificáveis** virou `tests/docs-consistency.test.mjs`,
no `test:architecture`:

| Gate | Reprova quando |
| --- | --- |
| documento descreve `commands/` como existente | o diretório não existe e a prosa diz que sim |
| fase ativa na lista de "não trabalhar ainda" | o arquivo se contradiz |
| versão do `PROJECT_STATE` diverge do manifesto | a memória compartilhada envelhece |
| ADR citado ausente do índice | referência a decisão que não existe |

Três mutações, três reprovações.

**O que estes gates não fazem, dito de frente:** eles conferem as poucas afirmações com
contraparte no disco. Prosa desatualizada que não menciona arquivo, versão ou fase continua
passando — nenhum teste substitui reler o documento ao fechar uma fase.

---

### NH-040 — ADR do Sync V2

```text
Owner:  Claude
Status: REVIEW — terceira revisão, aguardando decisão humana
Fase:   4
ADR:    0009 (Proposed)
```

**Nenhuma linha de código de sync foi escrita.** O roadmap exige o ADR antes, e o autor
reforçou o pedido.

O escopo pedido era mais amplo que "Noise vs TLS", e o ADR decide ou documenta os catorze
pontos: threat model, identidade e ciclo de vida das chaves, pairing, transporte, replay,
versionamento e handshake, envelope do evento, outbox transacional, cursores por peer,
idempotência, tombstones e retenção, matriz de conflitos por agregado, compatibilidade e
attachments por hash.

**Revisão 2, 2026-09-01 — premissa de produto corrigida pelo autor.** O Sync V2 não é
desktop → aparelho secundário: o caso principal é **bidirecional entre instalações,
especialmente Windows ↔ Android**, num conjunto de aparelhos confiáveis do mesmo acervo
(`Desktop ↔ Notebook ↔ Android ↔ Tablet`).

A correção não foi de redação, foi de modelo. A primeira versão dizia *"B pede a A: me mande
tudo com `seq > 1802` do seu `device_id`"* — o que funciona para dois aparelhos e **quebra em
três**: se cada peer só enviasse os próprios eventos, o Desktop e o Android precisariam se
encontrar diretamente, e o conteúdo se perderia em silêncio quando não se encontrassem.

O ADR agora separa explicitamente:

- **papel de transporte**, assimétrico por sessão — um abre a porta, o outro conecta;
- **papel de replicação**, sempre simétrico — quem abriu a porta não é servidor, não é
  autoritativo e não é dono do dado.

E o cursor virou **vetor de sequências por origem**, com store-and-forward: o Notebook carrega
para o Android o que recebeu do Desktop. Isso dá propagação transitiva.

**A decisão central continua sendo a causalidade.** `updated_at` está proibido como mecanismo
principal: ele não distingue o caso que interessa — duas edições offline a partir da mesma
versão, uma no Windows e outra no Android. Em seu lugar, revisão encadeada por agregado
(`new_rev = H(base_rev ‖ …)`), no modelo do Git.

**Snapshot vira semente, não mecanismo.** Um aparelho vazio pode ser inicializado por snapshot
acompanhado do vetor de sequências daquele instante; depois disso, só incremental. Snapshot com
cursor existente é regressão ao V1, e é gate.

**Android: interrupção é o caso normal**, não exceção. Processo morto no meio da sessão não
pode deixar evento aplicado pela metade, e reconexão retoma do cursor.

**Achado que reduz a fase:** `devices`, `sync_peers` e `sync_events` já existem no schema
desde as primeiras migrations e **nunca foram usadas**. A forma do outbox foi antecipada e
ficou dormindo — a migration é aditiva. O que falta nelas é justamente a causalidade.

**Sem CRDT**, e o motivo completo está no ADR: só texto de capítulo em edição simultânea é
candidato legítimo, e a autoria canônica sob controle do escritor reduz a pressão por
convergência automática.

**Revisão 3, 2026-09-01 — cinco correções do autor.** A direção da revisão 2 foi aprovada; o
vetor de sequências por origem fica como está, sem otimização de tamanho.

| # | O que estava errado | Correção |
| --- | --- | --- |
| 1 | `sync_events` era outbox local e `sync_applied_events` só registrava IDs — **depois de aplicar, o relay não teria mais o envelope para repassar** | `sync_events` vira log imutável com eventos locais **e** recebidos; outbox é a consulta `WHERE device_id = self` |
| 2 | store-and-forward sem autenticidade: Noise autentica o peer da conexão, não a origem de um evento retransmitido | assinatura Ed25519 no envelope, relay repassa sem reassinar, e **roster** de origens confiáveis |
| 3 | PIN de 8 dígitos usado direto como `psk` do `XXpsk0` | QR com 32 bytes de CSPRNG como principal; PIN por PAKE como alternativa |
| 4 | cursor avançava para o maior `seq` visto | cursor é a maior sequência **contígua**; lacuna deixa o evento pendente |
| 5 | snapshot e vetor lidos separadamente no bootstrap | os dois saem da **mesma transação de leitura** |

O ponto 2 é o mais afiado, e a falha era minha de um jeito específico: o gate "assinatura
inválida" já estava na matriz da revisão 2 **sem nenhuma contraparte no envelope**. Prometia
proteger algo que o contrato não definia — o mesmo padrão que esta sessão já pegou no `\b`
corrompido e no teste que dizia procurar `invoke()` e não procurava.

Também entrou **ciclo de vida do dispositivo** (`active`, `retired`, `revoked`), sem o qual um
aparelho abandonado contaria para sempre na retenção de tombstones e travaria qualquer poda
futura.

**Aceito em 2026-09-02**, na terceira revisão, junto com a ordem de implementação de catorze
etapas — registrada na seção 23 do ADR.

---

### NH-041 — Sync V2, etapa 1: migration e estruturas

```text
Owner:  Claude
Status: DONE
Fase:   4  (etapa 1 de 14)
Schema: 16
```

A etapa cria o **lugar** onde a replicação vai morar. Nenhum evento é gerado, enviado ou
aplicado ainda.

A decisão de método vale mais que a lista de tabelas: **os invariantes do ADR viraram triggers
e chaves estrangeiras**, e não convenção. O ADR descreve em prosa que o log é imutável e que o
cursor é contíguo; o banco agora recusa a linha que viola isso. A alternativa era confiar que
ninguém escreveria errado, e confiar não é mecanismo.

| Invariante do ADR | Como o banco segura |
| --- | --- |
| log imutável (§12) | triggers que abortam `UPDATE` e `DELETE` em `sync_events` |
| não se aplica o que não está no log (§12) | FK de `sync_applied_events` → `sync_events` |
| origem precisa estar no roster (§5.2) | FK de `sync_events.device_id` → `sync_devices` |
| cursor é a maior sequência **contígua** (§13) | trigger que exige densidade só no intervalo `(último_anterior, último_novo]`, e só acima de `baseline_seq` |
| envelope tem assinatura (§10) | `signature TEXT NOT NULL`, **sem `DEFAULT`** |
| `updated_at` não é causalidade (§11, §20) | nenhuma tabela do V2 tem a coluna, e um teste varre as oito |

O trigger de contiguidade é o mais interessante dos seis: ele é a única formulação que pega a
lacuna. Com o cursor em 100 e a chegada do 102, não adianta perguntar "o 101 foi aplicado?" —
o 101 nunca chegou, e não está em lugar nenhum para ser consultado. O que funciona é cobrar
**densidade**.

**Correção pedida pelo autor antes da etapa 2.** A primeira versão desse trigger cobrava os seq
`1..N` no log, e isso **contradizia a seção 14 do próprio ADR**: um aparelho semeado por
snapshot recebe `Desktop = 1803` e por definição não tem esses 1803 eventos — é para não
transferi-los que o snapshot existe. O gate teria tornado impossível gravar o cursor semeado,
ou forçado a transferência da história inteira, anulando o bootstrap.

O cursor passou a ter duas marcas: `baseline_seq`, até onde o conteúdo chegou por snapshot, e
`last_seq_applied`. A densidade vale **acima do baseline**. No pareamento comum o baseline é 0 e
nada muda. `baseline_seq` é imutável depois de escrito — um baseline mutável seria snapshot por
cima de cursor existente, que a seção 14 já chama de regressão ao V1.

A verificação também passou a olhar só o **intervalo** entre o cursor antigo e o novo. O estado
anterior já foi validado quando foi gravado; recontar desde o começo custava O(N) por evento
aplicado, num log que só cresce.

**A mutação isolada pegou um gate falso meu.** `o_cursor_nao_nasce_abaixo_do_baseline` passava
mesmo com a `CHECK` removida, porque o `INSERT` usava um dispositivo que já tinha linha de
cursor: falhava por chave primária, sem nunca tocar no invariante. Quarta ocorrência do mesmo
padrão nesta sessão, e a primeira encontrada por mutação de um mecanismo só — a mutação em
bloco derrubara dezessete testes ao mesmo tempo e escondeu esse.

**Três coisas foram deliberadamente deixadas de fora**, e o comentário da migration diz por quê:

- **chave privada** — mora fora do banco, porque o banco vai para backup;
- **contador de `seq`** — derivado de `MAX(seq)` da própria origem. É o que faz a restauração
  de backup funcionar: o aparelho novo começa do 1 como origem nova, e um contador guardado
  estaria errado exatamente aí;
- **tabela de pendentes** — pendente é "está no log e não está em aplicados". Duas fontes da
  mesma verdade é como se perde a verdade.

**Ajuste de ordem, feito com justificativa:** o campo `signature` e a identidade Ed25519 entram
aqui, e não na etapa 7. A etapa 6 constrói o relay que repassa envelope de terceiros; se a
assinatura só nascesse na 7, o gate da 6 passaria **provando a coisa insegura**. A 7 continua
sendo verificação, roster e ciclo de vida.

**Os seis gates foram verificados por mutação**, não por leitura — cada mecanismo foi removido
e a suíte foi vista reprovando exatamente os testes correspondentes, e nenhum outro. Sem isso
não há como distinguir um gate que protege de um gate que só existe.

Suíte: 195 testes Rust, 0 falhas.

---

### NH-046 — Sync V2, etapa 2: event log, `seq` e revisões

```text
Owner:  Claude
Status: DONE
Fase:   4  (etapa 2 de 14)
```

Primeira vez que o schema 16 ganha comportamento. Entrega o log com `seq`, a cadeia
`base_rev`/`new_rev` e a classificação de causalidade. **Não** liga o log aos caminhos de
escrita do domínio (etapa 3) nem aplica evento recebido (etapa 4).

**`domain/sync.rs`** — puro, sem `rusqlite` e sem `tauri`. A regra de causalidade precisa ser
testável sem banco, porque é ela que decide se duas edições são sequenciais ou concorrentes, e
errar isso apaga trabalho do escritor.

Duas correções ao texto do ADR, feitas na implementação:

- **Concatenação nua colide.** O ADR escreve `SHA-256(base_rev ‖ aggregate_id ‖ operation ‖
  payload)`, e `"ab" + "c"` produz os mesmos bytes que `"a" + "bc"`. Cada campo entra precedido
  do tamanho, o que torna a decomposição única. Coberto por teste.
- **Separador de domínio.** Sem ele, outra estrutura do sistema que por acaso concatenasse os
  mesmos bytes produziria o mesmo hash.

**O ADR contradizia esta implementação, e o ADR é que estava errado.** Ele dizia que *"evento
cujo `base_rev` seja ancestral conhecido é aplicável"*. Com `A0 → A1 → A2` local e a chegada de
`B1` com `base_rev = A0`, `A0` é ancestral conhecido — e aplicar direto sobrescreveria `A1` e
`A2` sem ninguém decidir nada. Ancestral conhecido significa apenas que **sabemos de onde ele
partiu**, o que separa concorrência de história desconhecida, não aplicável de não aplicável. O
§11 foi reescrito com as quatro saídas antes do merge, porque é dele que os três agentes vão
tirar a regra.

A classificação tem **quatro** saídas, e a quarta é a que evita inventar problema:

| Saída | Quando |
| --- | --- |
| `Sequential` | a origem partiu de onde estamos |
| `AlreadyPresent` | já conhecemos essa revisão — repetição é normal em rede |
| `Concurrent` | partiu de uma base nossa já superada: os dois editaram do mesmo ponto |
| `Unknown` | `base_rev` que não conhecemos. **Não é conflito** — é pedido de reconciliação |

Tratar `Unknown` como conflito inventaria uma divergência que provavelmente não existe: o mais
provável é que faltem eventos intermediários que ainda não chegaram.

**`infrastructure/sqlite/sync_repository.rs`** — grava e lê o log. O outbox não é tabela: é
`outbox_since`, o recorte da própria origem.

#### O gate de concorrência, e o que a mutação revelou

A `UNIQUE(device_id, seq)` transforma corrida em erro, e um erro aqui é **escrita local sem
evento**: o dado existe no aparelho e nunca sai dele. Duas medidas — `BEGIN IMMEDIATE` e retry
— e a mutação mostrou que **elas não são intercambiáveis**:

| Mutação | Resultado |
| --- | --- |
| `DEFERRED`, sem retry | 7 de 8 threads morrem em "toda escrita local precisa virar evento" |
| `DEFERRED`, **com** os 5 retries | continua reprovando |

O segundo caso é o informativo. O erro é corretamente classificado como contenção (`database is
locked`) e o retry roda as cinco vezes — e perde as cinco, porque cada tentativa disputa de
novo com a mesma probabilidade. **Retry não substitui modo de transação.** Com `DEFERRED`, o
`MAX(seq)` é lido antes de o lock de escrita existir; `IMMEDIATE` pega o lock antes da leitura.

#### O orçamento de espera, e um erro meu que o autor desfez

Eu tinha escrito que testar o retry exigiria segurar o lock por mais de oito segundos, e que o
custo não pagava. **Estava errado, por tomar o `busy_timeout` da conexão como dado.** O append
passou a ter espera própria por tentativa, e aí basta segurar o lock por algumas centenas de
milissegundos.

Contar tentativas também era a métrica errada: 5 tentativas × 8s de `busy_timeout` permitem um
pior caso perto de **quarenta segundos**, e este é o caminho do autosave. Um editor que trava
quarenta segundos é pior que um que avisa que não conseguiu salvar.

```text
ESPERA_INICIAL   250ms   primeira fatia; dobra a cada tentativa
ORCAMENTO_TOTAL   10s    teto do bloqueio percebido pelo escritor
```

O limite passou a ser **tempo total**, não número de tentativas. O `busy_timeout` é devolvido ao
valor de origem nas duas saídas — sem isso, uma saída por erro contaminaria a conexão e o resto
do aplicativo passaria a desistir cedo. Os dois viraram teste, e os dois foram vistos
reprovando quando o mecanismo foi removido.

#### E a primeira versão do orçamento causou inanição

Fatia fixa de 250 ms. O gate de concorrência **passou isolado e reprovou na suíte completa**:

```text
Não foi possível registrar o evento em 10s (25 tentativas).
Último erro: database is locked
```

Vinte e cinco tentativas, todas perdidas. Diagnóstico antes de mexer: **não** era violação de
`UNIQUE` — o `IMMEDIATE` estava funcionando. Era o problema oposto ao original: com fatia curta,
a thread desiste de esperar e volta ao fim da disputa repetidamente enquanto as outras avançam.
Com os oito segundos de antes ela ficava na fila e acabava ganhando.

A fatia passou a dobrar a cada tentativa, sempre limitada pelo que resta do orçamento. Recupera
a espera longa sem levantar o teto, e mantém as primeiras tentativas baratas — que é o que
deixa o teste do retry na casa das centenas de milissegundos.

A lição operacional: **um teste de concorrência que só roda isolado não provou nada.** Este só
falhou quando a máquina estava sob a carga da suíte inteira.

#### E consertar o orçamento cegou o gate — o gate precisou ser dividido

Com espera crescente, o laço passou a **absorver** as falhas do `DEFERRED`. Trocar `IMMEDIATE`
por `DEFERRED` deixou de reprovar: a mutação passou verde com 8×6, com 4×4 e com 8×2. O retry
consertado escondia o defeito que o gate existia para pegar.

Uma tentativa intermediária piorou isso: tolerar falha por contenção "porque falha limpa não é
alteração perdida". O argumento é verdadeiro e a mudança foi um erro — **falha por contenção é
o sintoma do defeito**, e aceitá-la é aceitar o defeito.

O gate virou dois, cada um cobrando uma coisa:

| Teste | Cobra | Reprova quando |
| --- | --- | --- |
| `escritas_locais_concorrentes_recebem_sequencias_distintas` | o caminho integrado: seq único e contíguo, evento por escrita, nenhum evento sem aplicação, cursor consistente | qualquer escrita não vira evento |
| `uma_tentativa_sozinha_nao_perde_a_corrida` | o **modo de transação**, chamando `tentar_append` sem o retry por cima | `DEFERRED` — 8 tentativas simultâneas, e quem lê `MAX(seq)` antes do lock perde |

O segundo é o que a mutação exige. Sem ele, `IMMEDIATE` poderia ter sido trocado por
`DEFERRED` e a suíte inteira continuaria verde — o desenho ficaria certo por acidente, sustentado
por um retry que gasta dez segundos disfarçando o problema.

O gate roda 8 threads × 6 escritas contra banco em arquivo, com barreira para largarem juntas.
Sem a barreira as escritas se enfileirariam sozinhas e o teste não exercitaria nada.

#### O payload atravessa o log byte a byte

`new_rev` é o hash do payload, e quem recebe recalcula sobre os bytes que chegaram. Um
`serde_json` reserializando no meio — reordenando chaves, normalizando espaço — produziria hash
diferente do mesmo conteúdo, e o evento seria recusado **como se tivesse sido adulterado**. O
teste usa JSON com chaves fora de ordem e espaço irregular, exatamente o que um reserializador
"arrumaria".

O `outbox_since` **falha fechado** em operação desconhecida: tratar como `upsert` faria um
evento corrompido virar escrita de conteúdo, e um `delete` ilegível viraria ressurreição
silenciosa do agregado.

#### Registrado para as etapas seguintes

**Etapa 2.5, antes da 3.** A etapa 2 grava `signature = ''` porque não existe chave para
assinar. Enquanto os eventos só nascem em teste, tudo bem; a etapa 3 liga o log às escritas
reais, e aí passam a nascer eventos de verdade. Como `sync_events` é append-only por trigger,
**evento nascido sem assinatura não pode ser assinado depois** — o começo do log ficaria
permanentemente inverificável e precisaria de uma exceção eterna no caminho de verificação de
assinatura, que é o pior lugar possível para ter uma.

A 2.5 é pequena de propósito: gerar o par Ed25519 local, guardar a privada fora do banco,
registrar o `self` e assinar no nascimento. A etapa 7 continua responsável por **verificar**
origem de terceiros, roster e ciclo de vida — assinar o que é meu e verificar o que é dos
outros são trabalhos diferentes, e só o primeiro tem prazo ditado pela imutabilidade do log.

**Etapa 3 usa a transação do domínio.** `append_local_event` abre a própria transação
`IMMEDIATE`, o que serve enquanto ele é chamado sozinho. Na etapa 3, `UPDATE chapter; COMMIT`
seguido de `append_local_event()` recriaria exatamente o buraco que o outbox existe para
fechar. A função precisa ser quebrada em uma que opere sobre uma `&Transaction` recebida de
fora, com o `append_local_event` autônomo virando uma casca fina. O gate cobra os **dois**
sentidos: falhou o evento, o dado volta; falhou o dado, o evento não existe.

---

### NH-043 — Reconciliação: "fonte da verdade" não é o último aparelho

```text
Owner:  Claude
Status: DONE — só ADR, sem código
Fase:   4  (afeta as etapas 2, 4 e principalmente 16)
```

Correção de filosofia trazida pelo autor. **Não muda o schema 16**; muda como a causalidade e a
matriz de conflitos devem ser implementadas.

A regra anterior do ADR — *"texto de capítulo: conflito explícito, nunca resolve sozinho"* —
protegia contra `last-write-wins` e acertava nisso, mas errava para o outro lado: interrompia o
escritor por qualquer vírgula divergente.

A regra nova:

> Texto de capítulo nunca perde uma revisão em silêncio. Alterações não sobrepostas são
> mescladas automaticamente por **três vias**; sobrepostas exigem decisão humana. As duas
> revisões originais permanecem preservadas.

O que mudou no ADR:

- **§16 reescrito.** Se PC e celular ficaram offline e os dois receberam alterações legítimas,
  **os dois são verdade**. O sistema reconcilia revisões, não escolhe aparelho.
- **Granularidade por bloco**, não pelo capítulo inteiro. "PC mexeu no bloco B, Android no
  bloco E" não é conflito.
- **Nenhuma revisão perdedora é apagada**, nem depois de merge automático: `A0`, `A1`, `A2` e
  `A3` ficam recuperáveis. Para texto, armazenamento é barato perto do valor do conteúdo.
- **Degrau por importância mantido e explicitado**: capítulo ganha três vias e preservação;
  nome de personagem cai numa central de conflitos simples; posição de card se resolve por
  desempate e ninguém precisa saber.
- **§21 reforçado, não enfraquecido.** Poderia parecer que reconciliar texto exige CRDT. CRDT
  resolve edição concorrente **em tempo real**, sem ponto de decisão; o NarraHub tem edição
  alternada offline, que possui justamente o que falta ao caso do CRDT — uma **base comum
  identificável**. `base_rev` + grafo + três vias + preservação resolve, com muito menos
  maquinaria.
- Cinco gates novos de reconciliação, mais um de fronteira: `last-write-wins` em texto de
  capítulo **reprova**.

**Pré-requisito descoberto ao revisar isso:** o merge por bloco precisa saber se *este*
parágrafo do Android é o mesmo *daquele* do PC — e hoje não sabe. `chapters.content` guarda o
HTML de `editor.getHTML()` com StarterKit puro, sem identidade nos nós. Um parágrafo inserido
acima desloca todos os outros, e o alinhamento por posição concluiria que o capítulo inteiro
mudou dos dois lados.

O ADR passou a exigir **id estável de bloco antes de qualquer evento ou revisão de capítulo**,
com id novo em bloco colado, e um fallback assimétrico de propósito: casamento ambíguo ou sem
id cai em **conflito do capítulo inteiro**, nunca em merge adivinhado. Degradar para conflito é
chato; degradar para adivinhação apaga trabalho.

É trabalho de produto além do transporte, então não pertence às etapas 1–14 — mas bloqueia a
reconciliação de texto. Até existir, texto de capítulo se comporta como conflito explícito
inteiro.

---

### NH-044 — Três armadilhas registradas nas etapas certas

```text
Owner:  Claude
Status: DONE — só ADR, sem código
Fase:   4  (etapas 2, 7 e 12)
```

Levantadas pelo autor ao revisar a etapa 1. Nenhuma é problema da migration — duas existem
justamente porque o schema está obrigando a fazer certo.

| Etapa | Armadilha | Gate |
| --- | --- | --- |
| 2 | `SELECT MAX(seq)+1` seguido de `INSERT` deixa duas threads escolherem 101. A `UNIQUE` transforma corrupção em erro, mas sem serialização ou retry o resultado é **escrita local sem evento** — o dado existe aqui e nunca sai daqui | duas escritas concorrentes produzem `seq` diferentes e nenhuma alteração fica sem evento |
| 12 | a FK `sync_cursors → sync_devices` obriga o roster a chegar **antes** do vetor. Bootstrap é roster + snapshot + vetor, na mesma transação | vetor citando origem ausente do roster reprova, por integridade referencial |
| 7 | chave privada fica fora do backup, `sync_devices` fica dentro: um backup restaurado afirma ser um dispositivo cuja chave não existe mais ali | restore rebaixa o `self` antigo, registra a identidade nova, e o primeiro evento local nasce com `seq = 1` |

A terceira fecha o argumento de por que `seq` é **derivado** de `MAX(seq)` e não guardado num
contador: um contador restaurado continuaria de onde o aparelho antigo parou, assinando com
uma chave diferente sob um `device_id` que não é o seu.

---

### NH-042 — Gate contra caractere de controle literal

```text
Owner:  Claude
Status: DONE
Fase:   4  (transversal)
```

Terceira aparição do mesmo bug nesta sessão, e a mais irônica: o byte `0x08` estava dentro da
frase do próprio `TASKS.md` que descrevia o episódio anterior — `` `\b` `` renderizado como
`` `` ``, invisível no terminal.

A origem é sempre a mesma: uma sequência de escape atravessa uma camada a mais (shell, Python,
JSON) e vira o byte de verdade. Numa expressão regular, `/Gateway\b/u` passa a casar outra
coisa e **continua parecendo certa na tela**. Foi assim que cinco asserções foram corrompidas —
três delas depois de eu já ter declarado que estavam verificadas.

O gate não é inteligente, e não precisa ser: varre todo arquivo versionado e reprova qualquer
caractere de controle fora de tabulação e quebra de linha, apontando arquivo, linha e o
codepoint. Verificado por mutação.

---

## BACKLOG

Registrado, **não implementar** antes de a fase correspondente abrir.

| ID | Tarefa | Fase |
| --- | --- | --- |
| NH-045 | Identidade persistente de blocos do editor (detalhado abaixo) | 4, fora das etapas 1–14 |
| NH-048 | Gravação atômica do arquivo de identidade (detalhado abaixo) | 4, hardening antes do Sync V2 sair |
| NH-050 | Teste de tokens de design (`var(--*)` sem definição reprova o CI) | 5 |
| NH-051 | Escala de breakpoints e responsividade em telas menores | 5 |
| NH-060 | Contrato `AIContext v1` e orçamento de contexto | 6 |

---

### NH-049 — Sync V2, etapa 3: outbox transacional

```text
Owner:  Claude
Status: DONE
Fase:   4  (etapa 3 de 14)
```

A etapa que tira o Sync V2 do estado de infraestrutura sem chamador. A revisão da 2.5 apontou
a lacuna com precisão: `load_or_create`, `reconcile_self` e `append_local_event` só apareciam
dentro dos próprios módulos.

#### A transação é uma só, e é a do domínio

`append_event_in_transaction` opera sobre uma `&Transaction` recebida de fora;
`append_local_event` virou a casca fina que abre a transação e chama a outra.

```text
ERRADO                             CERTO
------                             -----
BEGIN; UPDATE chapter; COMMIT      BEGIN IMMEDIATE
append_local_event()                 UPDATE chapter
   ← duas transações                 append_event_in_transaction()
   ← queda no meio deixa o         COMMIT
     dado salvo e sem evento
```

O primeiro caminho real ligado é o **salvamento de capítulo** — o do autosave, que é onde uma
perda dói mais.

Duas decisões dentro dele:

- **O payload é o estado novo do agregado**, lido depois da escrita e dentro da mesma
  transação. Montá-lo a partir do patch descreveria só o que a tela mexeu, e quem recebe
  precisa do capítulo inteiro para convergir.
- **Patch vazio não gera evento.** A tela chama o salvamento mesmo quando nada mudou; um evento
  por foco no editor encheria o log e faria os outros aparelhos reaplicarem o mesmo estado sem
  parar.

#### O gate dos dois sentidos

| Sentido | Teste | Mutação que o derruba |
| --- | --- | --- |
| falhou o dado → sem evento | `dado_recusado_nao_deixa_evento_para_tras` | — |
| falhou o evento → dado volta | `evento_recusado_desfaz_a_alteracao_do_dado` | **duas transações** |

O segundo é o que só a transação compartilhada garante, e o mais fácil de perder: `UPDATE
chapter; COMMIT` seguido de gravar o evento passa em todo teste feliz, e deixa o capítulo salvo
e invisível para os outros aparelhos no dia em que o evento falhar. Verificado — a mutação
derruba **só** esse teste, o que também prova que os outros oito não o estavam cobrindo.

A falha é forçada de forma realista: a identidade deixa de ser o `self` do roster, que é
exatamente o estado que um banco restaurado de outro aparelho produz antes da reconciliação.

#### A sequência de arranque virou gate

Pedida pelo autor, e cobrada passo a passo:

```text
identidade existe no disco  →  foi carregada  →  reconcile_self rodou
   →  exatamente um `self`  →  o `self` corresponde à chave
   →  e só então a transação de domínio produz evento
```

Com o outro lado junto: **sem o arranque, a escrita falha** em vez de gravar sem evento. A
ordem não é convenção que alguém precisa lembrar — é pré-condição que o banco cobra. Removendo
`reconcile_self` do arranque, três dos quatro testes reprovam.

**`sync_identity` não guarda estado**, pelo mesmo motivo de `database`: a restauração de backup
troca o banco debaixo do app, e uma identidade memorizada deixaria o `self` do roster apontando
para o aparelho de onde o backup veio. Rodar o arranque a cada comando de escrita é barato — a
reconciliação é uma consulta indexada quando não há nada a mudar — e dispensa invalidação de
cache num caminho onde errar significa assinar eventos com a origem de outro aparelho.

---

### NH-050 — Sync V2, etapa 4: aplicação remota e idempotência

```text
Owner:  Claude
Status: DONE
Fase:   4  (etapa 4 de 14)
```

Os efeitos da seção 12 do ADR, numa transação só. O quarto — avançar o cursor — fica para a
etapa 5, e por um motivo: decidir se o cursor pode avançar depende de a sequência ter ficado
contígua, o que é decisão da **sessão**, não de um evento isolado.

**O envelope entra no log antes de qualquer decisão sobre aplicar.** Mesmo um evento que vira
divergência precisa ficar guardado: é ele que o relay retransmite, e é dele que a resolução do
conflito lê a outra versão. Guardar só o que foi aplicado perderia metade da história.

| Causalidade | O que acontece |
| --- | --- |
| `Sequential` | aplica, registra revisão, marca aplicado |
| `AlreadyPresent` | marca aplicado e para — repetição é normal em rede |
| `Concurrent` | **nada é sobrescrito**: registra a revisão remota e abre divergência |
| `Unknown` | **não marca aplicado** — quando a história chegar, precisa ser reavaliado |

O `Unknown` é o que mais fácil se erraria: marcar como aplicado impediria a reavaliação, e o
evento ficaria guardado para sempre sem nunca entrar.

#### Falha fechada em agregado sem aplicação

Hoje só `chapter` tem aplicação — é o único que a etapa 3 produz. Qualquer outro tipo **para a
sessão**. Ignorar produziria o pior estado possível: o evento constaria como aplicado, o cursor
avançaria, e o dado nunca chegaria, sem nada registrando a falta.

#### O que a mutação encontrou

Removendo a checagem de `event_id`, **todos os testes continuavam verdes** — `classify`
devolvia `AlreadyPresent` ao reconhecer a `new_rev` em `sync_revision_history`. A checagem
parecia redundante.

Não é. O ADR §11 diz que essa tabela guarda **as últimas** revisões por agregado — ela pode ser
podada. Depois da poda, quem impede a reaplicação é o registro de `event_id`, e mais nada; sem
ele, um evento antigo reaparecendo numa reconexão sobrescreveria edições posteriores. O teste
novo poda a história e prova exatamente isso, e reprova sob a mutação.

Entrou junto um caso que ninguém cobria: **dois aparelhos fazendo a mesma edição a partir da
mesma base convergem sem divergência.** A revisão é `H(base ‖ agregado ‖ operação ‖ payload)`,
então conteúdo igual produz `new_rev` igual mesmo vindo de origens diferentes — e tratar isso
como conflito faria o escritor escolher entre dois textos idênticos.

---

### NH-047 — Sync V2, etapa 2.5: identidade local e assinatura no nascimento

```text
Owner:  Claude
Status: DONE
Fase:   4  (etapa 2.5 de 14)
```

Etapa criada pelo autor ao revisar a etapa 2, e ela tinha prazo: a etapa 3 liga o log às
escritas reais, e `sync_events` é append-only por trigger.

```text
evento nascido sem assinatura  →  não pode ser assinado depois
                               →  começo do log inverificável para sempre
                               →  exceção eterna no caminho de verificação
```

Uma exceção no caminho que decide se um evento é autêntico é o pior lugar possível para ter
uma.

**`domain/identity.rs`** — puro. Ed25519 para assinar, `device_id` = SHA-256 truncado da
pública em base32, e a representação canônica do envelope: ordem fixa, cada campo precedido do
tamanho, **menos a assinatura**. O `payload` entra como bytes opacos — reserializar mudaria os
bytes do mesmo conteúdo e o outro aparelho recusaria o evento como adulterado.

Três decisões que valem registro:

- **Separador de domínio próprio**, diferente do usado em `new_rev`. Os dois hashes cobrem
  coisas diferentes; um prefixo comum permitiria que bytes preparados para um valessem no
  outro.
- **`Debug` escrito à mão.** `#[derive(Debug)]` imprimiria a chave privada em qualquer pânico
  ou log — e chave que vaza em log deixa de ser privada mesmo estando fora do backup e fora do
  banco.
- **Base32 escrito aqui**, em vez de dependência: vinte linhas, formato fixo, e o alfabeto sem
  `0`, `1` e `8` é o que faz um fingerprint sobreviver a alguém lendo em voz alta.

**`infrastructure/identity_store.rs`** — a privada mora em arquivo, fora do banco (§5), com
`0600` no Unix. Formato versionado desde a primeira gravação, porque a chave X25519 do Noise
(etapa 8) vai morar no mesmo arquivo.

#### O restore de backup, resolvido aqui em vez de na etapa 7

O autor tinha datado isto para "etapa 7, ou onde identidade e restore se encontrarem". É aqui.

```text
chave privada   fora do backup
sync_devices    dentro do backup   →  o banco restaurado carrega
                                      old-desktop com is_self = 1
```

O banco afirma ser um dispositivo cuja chave não existe mais ali. `reconcile_self` rebaixa o
`self` antigo para `retired` e registra a identidade nova. **Rebaixa, não apaga** — os eventos
daquela origem continuam no log, a FK depende da linha, e apagar reescreveria a história de um
aparelho que existiu de verdade.

E a decisão que fecha o argumento do `seq` derivado: **a origem do evento vem da chave, não do
banco.** Se viesse do banco, o aparelho novo assinaria com a própria chave sob o `device_id` de
outro, e o `seq` continuaria de onde o antigo parou — duas sequências embaralhadas na mesma
origem.

#### Gates, todos verificados por mutação

| Mecanismo removido | Reprova |
| --- | --- |
| assinatura no nascimento | `o_evento_nasce_assinado`, `o_que_sai_pelo_outbox_verifica` |
| origem vinda da chave | `a_origem_vem_da_chave_e_nao_do_banco` |
| rebaixamento do `self` antigo | `banco_restaurado_de_outro_aparelho_rebinda_o_self` |
| `payload` na canonicalização | `alterar_qualquer_campo_invalida_a_assinatura` |

O de canonicalização cobre os **dez** campos do envelope, um a um. Campo de fora da assinatura
é campo que um relay pode alterar sem que ninguém perceba — e mudar `device_id` ou `seq` em
trânsito reescreve a origem do evento.

Mais dois gates de fronteira: a chave privada **não aparece em nenhuma tabela** do banco, e
**não entra no backup**. O segundo existe para o dia em que alguém trocar a lista explícita de
arquivos por "copie o diretório de dados inteiro" — a mudança mais natural do mundo, e a mais
cara aqui: dois aparelhos com o mesmo `device_id` fazem o cursor de todos os outros peers
significar duas coisas ao mesmo tempo.

---

### NH-048 — Gravação atômica do arquivo de identidade

```text
Owner:  —
Status: BACKLOG
Fase:   4, hardening antes de o Sync V2 sair de verdade
```

Levantado pelo autor ao revisar a etapa 2.5, e ele mesmo decidiu não reabrir a etapa por isso.

`identity_store::gravar` usa `std::fs::write`. Uma queda de energia no meio da **primeira**
gravação pode deixar um `sync-identity.json` truncado — e um arquivo truncado não é uma
identidade parcial, é um aparelho que perdeu a identidade e precisa parear tudo de novo.

```text
write temp
   ↓
fsync temp
   ↓
rename atômico
   ↓
fsync do diretório
```

**Fica mais urgente na etapa 8**, e é por isso que está datado: hoje o arquivo é escrito uma vez
e nunca mais. Quando a X25519 do Noise entrar no mesmo formato, ele passa a ser **atualizado** —
e reescrever um arquivo existente é onde a janela de truncamento realmente machuca, porque o
conteúdo anterior era válido.

---

### NH-045 — Identidade persistente de blocos do editor

```text
Owner:  —
Status: BACKLOG
Fase:   4, fora das etapas 1–14
Bloqueia:     merge automático por bloco (ADR 0009 §16)
Não bloqueia: nada da infraestrutura do Sync V2
```

**Objetivo:** permitir reconciliação de capítulos por bloco. Sem isso o sistema não consegue
responder se *este* parágrafo do Android é o mesmo *daquele* do PC, e teria que adivinhar.

**Regras:**

- `block_id` persistente, gerado na criação;
- editar preserva o id;
- mover preserva o id;
- copiar e colar gera id **novo** — dois blocos distintos nunca compartilham identidade;
- id duplicado reprova, e é corrigido **antes** de persistir;
- conteúdo legado comprovadamente idêntico pode ser normalizado;
- conteúdo legado divergente cai em conflito integral;
- casamento ambíguo **nunca** faz merge automático.

#### A parte difícil não é gerar os ids. É inicializá-los em conteúdo que já existe.

Gerar UUID para os parágrafos existentes em cada aparelho, independentemente, é a solução
óbvia — e é catastrófica:

```text
mesmo capítulo legado, já presente nos dois aparelhos

PC       P1 = uuid-A   P2 = uuid-B   P3 = uuid-C
Android  P1 = uuid-X   P2 = uuid-Y   P3 = uuid-Z

conteúdo idêntico, e nenhum bloco corresponde a nenhum outro
```

O merge por bloco passaria a funcionar **pior** que a ausência dele: todo capítulo legado
viraria divergência total no primeiro sync, e o escritor veria conflito em texto que ninguém
tocou.

**Caminho seguro, em duas situações:**

| Situação | Como os ids nascem |
| --- | --- |
| aparelho novo recebendo o acervo | os ids vêm **no snapshot do bootstrap**, junto com o conteúdo. Trivial e sem ambiguidade |
| dois aparelhos estabelecidos com o mesmo conteúdo | fase de **normalização** antes de habilitar o merge fino |

A normalização compara o capítulo inteiro por hash:

```text
hash(PC) == hash(Android)   →  um deles fornece a identidade estrutural,
                               e os dois passam a usar os mesmos ids

hash(PC) != hash(Android)   →  não inventa correspondência
                               →  conflito de capítulo inteiro
                               →  humano resolve
                               →  o resultado recebe ids estáveis
                               →  daí em diante o merge por bloco funciona
```

#### Dois atalhos que parecem boas ideias e não são

**`block_id = hash(texto)`** — tentador porque resolveria a inicialização de graça. Mas o id
mudaria exatamente quando não pode:

```text
"João entrou na casa."            →  id X
"João entrou lentamente na casa." →  id Y      ← e era pra ser o mesmo bloco
```

**`chapter_id + índice`** — inserir um parágrafo no começo renumera todos os outros, que é
justamente o caso que a identidade existe para sobreviver.

O id precisa ser identidade, não derivação: nasce com o bloco, sobrevive à edição e à
movimentação, e só um bloco novo ganha um novo.

---

### NH-051 — Conteúdo inalcançável em 1366×768

```text
Owner:  Claude
Status: DONE — tratado como defeito, não como refinamento de Fase 5
Fase:   1 (reclassificada)
```

**Causa encontrada.** `root-layout.component.css` tem `ViewEncapsulation.None`, e duas
regras globais competiam pelo mesmo seletor:

```css
.home-view,.feature-page { height: 100%; overflow-y: auto; ... }
.feature-page            { ...; overflow: hidden; }
```

A segunda vem depois, com a mesma especificidade, e **o atalho `overflow` zera o eixo
vertical junto com o horizontal**. Resultado: uma página da altura exata da janela que corta
tudo o que passar disso, sem barra de rolagem. Em tela grande nada passava; em 1366×768, a
lista de backups dos Ajustes passava — e ficava inalcançável.

Afetava 5 páginas: Ajustes, Conexões, Histórico, Timeline e Entidades.

**Correção:** `overflow: hidden` → `overflow-x: hidden; overflow-y: auto`, preservando a
intenção de cortar na horizontal.

**Gate:** `tests/layout-scroll.test.mjs` resolve a cascata como o navegador resolve — última
declaração vence — e reprova qualquer superfície de página com altura travada na janela e
overflow vertical `hidden`. Verificado por mutação.

Contêineres de shell (`.content-stage`, `.nh-content`) ficam **de fora** do teste de
propósito: eles cortam por desenho e delegam a rolagem para a página lá dentro.

**Continua aberto, e é outra coisa:** não existe escala compartilhada de breakpoints — 12
valores diferentes no CSS e 7 arquivos sem nenhuma media query. Isso é refinamento de design
system e segue como `NH-052`, na Fase 5.

---

### NH-052 — Escala compartilhada de breakpoints

```text
Owner:  —
Status: BACKLOG — Fase 5, não implementar antes
Fase:   5
```

Doze valores de breakpoint diferentes no CSS — 520, 680, 700, 760, 860, 900, 920, 1050,
1100, 1180, 1260 e 1450px — escritos de duas formas (`max-width:680px` e
`max-width: 680px`), e 7 arquivos sem nenhuma media query, entre eles `universe-sidebar`,
`contextual-inspector`, `entity-sheet` e `library-page`.

Cada componente escolheu o próprio ponto de quebra, então o layout reorganiza por região em
vez de por tela. Isso é diferente do defeito da `NH-051`, que era conteúdo inalcançável e já
foi corrigido: aqui não há dado fora de alcance, há inconsistência visual.

**Objetivo:** escala única em tokens, aplicada às superfícies que hoje não reorganizam, e
regressão visual por snapshot nas larguras dessa escala. Faz par com a `NH-050` — as duas
são o mesmo problema, CSS sem contrato verificável.

**Reprodução, dada pelo autor em 2026-08-31:**

```text
resolução   1366 × 768
sintoma     não dá para rolar; o app fica sem espaço para exibir o conteúdo
            e a parte inferior fica muito pequena
```

1366×768 é a resolução de notebook mais comum que existe, e "não dá para rolar" significa
**conteúdo inalcançável**, não apenas apertado.

**Mecanismo confirmado no CSS:** o shell é um quadro fixo que nunca rola — `height: 100vh`
com `overflow: hidden` em `.app-shell`, `.content-stage`, `.content` e
`.workspace-route-content`. A titlebar come 64px fixos (`--nh-titlebar-height`), então a
área útil é `calc(100vh - 64px)`: em 768px de altura sobram **704px**. A rolagem foi
delegada para dentro de cada página.

Verifiquei que **todas as páginas de rota têm `overflow-y: auto`** — library, writing,
entities, connections, timeline, planning, history, settings e universe-picker. Logo o
culpado é um contêiner **aninhado** com altura travada, e não a página. Achar qual exige
medir com a janela em 1366×768; não consegui, porque o app precisa do runtime Tauri.

**O que foi verificado no CSS** (não é diagnóstico da falha, é o terreno):

- **12 valores de breakpoint diferentes** — 520, 680, 700, 760, 860, 900, 920, 1050, 1100,
  1180, 1260 e 1450px — e escritos de duas formas (`max-width:680px` e `max-width: 680px`).
  Não existe escala compartilhada; cada componente escolheu o seu ponto de quebra.
- **7 arquivos CSS sem nenhuma media query**, entre eles `universe-sidebar`,
  `contextual-inspector`, `entity-sheet` e `library-page` — que são superfícies grandes de
  layout.

Isso é consistente com o sintoma: as partes que têm breakpoint reorganizam numa largura, as
que não têm não reorganizam em nenhuma, e o resultado é um layout que se parte por regiões
em vez de por tela.

**Objetivo quando a Fase 5 abrir:** uma escala única de breakpoints em tokens, aplicada às
superfícies que hoje não reorganizam, e regressão visual por snapshot nas larguras dessa
escala (ver `NH-050`, que resolve o outro lado do mesmo problema).

**Recomendação de prioridade:** isto está catalogado na Fase 5, mas "conteúdo inalcançável
na resolução de notebook mais comum" é **defeito**, não refinamento de design system.
Recomendo tratar como defeito e corrigir antes da Fase 5 — decisão do humano.

---

## DONE

Ver `docs/handoffs/` para o detalhe de cada uma.

### NH-000 — Protocolo de desenvolvimento multiagente

```text
Owner:  Claude
Status: DONE
Fase:   -1
```

Criados `AGENTS.md` (constituição), `CLAUDE.md`, `GEMINI.md`, `.agents/rules/`,
`TASKS.md`, `docs/ai/PROJECT_STATE.md`, `docs/ai/ROADMAP.md`, `docs/ai/WORKFLOW.md`,
`docs/handoffs/` e `docs/ADR/README.md`.
