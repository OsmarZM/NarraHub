# NarraHub — Fila de tarefas

`AGENTS.md` diz **como** trabalhar. Este arquivo diz **no que** trabalhar.

Fase ativa: **FASE 0 — Higiene de release**. Ver `docs/ai/PROJECT_STATE.md`.

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
Fase:   0
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

## BACKLOG

Registrado, **não implementar** antes de a fase correspondente abrir.

| ID | Tarefa | Fase |
| --- | --- | --- |
| NH-010 | Fixtures de bancos históricos anonimizados | 1 |
| NH-011 | Qualification harness de migration automatizado | 1 |
| NH-012 | Teste de atualização N → N+1 | 1 |
| NH-013 | Teste de backup/restore e de falha no restore com rollback | 1 |
| NH-020 | Extrair `WorkspaceSessionService` | 2 |
| NH-021 | Mover orquestração de preload para fora do layout | 2 |
| NH-022 | Lifecycle próprio do `GlobalSearchService` | 2 |
| NH-023 | Extrair `WorkspaceShareService` | 2 |
| NH-024 | Ampliar `WorkspaceSyncService` (cross-domain) | 2 |
| NH-025 | Teste de fronteira do `WorkspaceLayout` | 2 |
| NH-030 | Remover `src-tauri/src/commands/` legado | 3 |
| NH-040 | ADR do transporte do Sync V2 (threat model + Noise vs TLS) | 4 |
| NH-050 | Teste de tokens de design (`var(--*)` sem definição reprova o CI) | 5 |
| NH-060 | Contrato `AIContext v1` e orçamento de contexto | 6 |

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
