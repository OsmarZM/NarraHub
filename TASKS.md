# NarraHub — Fila de tarefas

`AGENTS.md` diz **como** trabalhar. Este arquivo diz **no que** trabalhar.

Fase ativa: **FASE 1 — Qualification**. Ver `docs/ai/PROJECT_STATE.md`.

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
Owner:  —
Status: READY
Branch: <agente>/NH-007-mapa-invariantes
Fase:   1
```

**Contexto:** as 11 invariantes de domínio vivem agora em `docs/DOMAIN_INVARIANTS.md`. O
core Rust tem 167 testes e vários cobrem essas regras — mas sob nomes que não as citam.
Não existe mapa nas duas direções: a invariante não aponta para o teste, e o teste não diz
qual invariante defende.

Consequência prática: não dá para responder "a invariante 5 está protegida?" sem reler o
core. Uma invariante que ninguém consegue verificar é indistinguível de uma que ninguém
implementou — e é assim que uma delas morre em silêncio numa refatoração.

**Objetivo:** cada uma das 11 invariantes aponta para o teste que a prova; cada teste
correspondente cita o número da invariante. Onde não houver teste, escrever o teste ou
registrar a lacuna explicitamente — **não** inventar cobertura no papel.

**Pistas já levantadas:**

| Invariante | Teste existente |
| --- | --- |
| 4 (relação no mesmo universo) | `relation_cannot_cross_universe_without_health_failure`, `invalid_cross_universe_relation_rolls_back_the_whole_card` |
| 6 e 7 (proposta não vira cânone) | `campo_fora_do_escopo_nao_grava_nada_e_a_proposta_segue_pendente` |
| 10 (tudo ou nada) | `card_save_is_atomic_and_relations_are_normalized` |

As demais estão sem mapeamento — o que não significa sem cobertura, significa não
verificado.

**Arquivos:** `docs/DOMAIN_INVARIANTS.md`, testes em `src-tauri/src/`

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

### NH-010 — Fixture representativa nas versões que os usuários têm hoje

```text
Owner:  —
Status: READY
Branch: <agente>/NH-010-fixtures
Fase:   1
```

**Contexto:** só existe fixture povoada no schema 10. Um usuário que instalou a 0.9.1 está
no schema 15. A próxima migration (16) será exercitada a partir de um banco 15 que só
existe sinteticamente, pela cadeia 10→15 — o que é razoável, mas não é a mesma coisa que um
banco que viveu num app real, com dados irregulares.

**Objetivo:** fixtures povoadas com ponto de partida em 13, 14 e 15, além da de 10. E um
teste que reprove quando uma migration nova não tiver fixture de origem.

**Cuidado:** fixtures são anonimizadas por construção — dados inventados, nunca conteúdo
real de escritor. Se algum dia partirmos de um banco real, a anonimização vem antes do
commit, não depois.

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
Status: REVIEW — roteiro pronto; 1 execução registrada; falta fechar o escopo
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

**Falta para fechar:**

1. backup e restauração já na 0.9.1;
2. `0.8.0 → 0.9.1`, porque a 0.7.4 não cria propriedades tipadas de planejamento e por isso
   **não exercita a promessa da migration 15**.

**Nunca no perfil de uso diário** quando a origem for mais antiga que o banco instalado: o
app recusa banco de schema mais novo, e a migração é de mão única.

---

### NH-013 — Checklist de release desktop como gate

```text
Owner:  —
Status: READY
Branch: <agente>/NH-013-checklist-release
Fase:   1
Depende de: NH-012 (o roteiro já existe; falta a execução)
```

**Objetivo:** transformar o checklist do roadmap (instalação limpa, upgrade, banco antigo,
backup, restore, restart, autosave, compartilhamento, tema claro, tema escuro, updater) em
gate versionado, com espaço para registrar a evidência de cada item por release.

Base pronta: os **Gates 1 a 5** e a **Regra de publicação** em
`docs/ARCHITECTURE_EVOLUTION_PLAN.md` já descrevem isso em prosa. O trabalho é torná-los
verificáveis, não reescrevê-los.

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

## BACKLOG

Registrado, **não implementar** antes de a fase correspondente abrir.

| ID | Tarefa | Fase |
| --- | --- | --- |
| NH-020 | Extrair `WorkspaceSessionService` | 2 |
| NH-021 | Mover orquestração de preload para fora do layout | 2 |
| NH-022 | Lifecycle próprio do `GlobalSearchService` | 2 |
| NH-023 | Extrair `WorkspaceShareService` | 2 |
| NH-024 | Ampliar `WorkspaceSyncService` (cross-domain) | 2 |
| NH-025 | Teste de fronteira do `WorkspaceLayout` | 2 |
| NH-030 | Remover `src-tauri/src/commands/` legado | 3 |
| NH-040 | ADR do transporte do Sync V2 (threat model + Noise vs TLS) | 4 |
| NH-050 | Teste de tokens de design (`var(--*)` sem definição reprova o CI) | 5 |
| NH-051 | Escala de breakpoints e responsividade em telas menores | 5 |
| NH-060 | Contrato `AIContext v1` e orçamento de contexto | 6 |

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
