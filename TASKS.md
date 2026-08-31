# NarraHub — Fila de tarefas

`AGENTS.md` diz **como** trabalhar. Este arquivo diz **no que** trabalhar.

Fase ativa: **FASE 0 — Higiene de release**. Ver `docs/ai/PROJECT_STATE.md`.

## Regras deste arquivo

- Status: `READY` · `IN_PROGRESS` · `BLOCKED` · `REVIEW` · `DONE` · `BACKLOG`.
- Uma tarefa tem **um** dono ativo. Só pegue tarefas `READY` sem `Owner`.
- Edite **apenas a sua entrada** e commite essa mudança sozinha, para o merge ser trivial.
- Detalhe do trabalho não vai aqui — vai no handoff (`docs/handoffs/`).
- `DONE` exige validação executada, não só build verde.

---

## ACTIVE

### NH-001 — Tornar `main` canônica

```text
Owner:  —
Status: READY
Branch: <agente>/NH-001-main-canonica
Fase:   0
```

**Contexto (verificado em 2026-08-31):** o default do `origin` é
`feat/native-app-foundation`, que contém 0.8.0, 0.9.0 e 0.9.1. A `main` parou em **0.7.6**.
Ou seja, o diagnóstico se inverte: não é a release que saiu da linha, é a `main` que ficou
para trás.

**Objetivo:** `main` passa a conter a versão publicada mais recente e vira o default do
repositório no GitHub. Release nasce de `main` a partir daí.

**Restrições:** sem force push. Sem copiar arquivos entre branches às cegas — a
reintegração é por merge, com o diff revisado.

**Decisão que precisa do humano antes de executar:** merge de
`feat/native-app-foundation` em `main`, ou promover a branch a `main` e arquivar a antiga.
Registrar a escolha em um ADR ou no handoff.

**Validação:**

```bash
npm run release:validate-version && npm run build && npm run test:architecture
```

**Não tocar:** Sync, IA, páginas grandes.

---

### NH-002 — Validação de versão no CI comum

```text
Owner:  —
Status: READY
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
Owner:  —
Status: READY
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
Owner:  —
Status: BLOCKED
Branch: <agente>/NH-004-doc-baseline
Fase:   0
Bloqueada por: NH-001
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
Owner:  —
Status: READY
Branch: <agente>/NH-005-adr-index
Fase:   0
```

**Objetivo:** manter `docs/ADR/README.md` como índice vivo dos ADRs 0001–0006 e de todos os
seguintes, com status. Já criado nesta fase — a tarefa é revisar se os status registrados
batem com a realidade do código.

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

### NH-000 — Protocolo de desenvolvimento multiagente

```text
Owner:  Claude
Status: DONE
Fase:   -1
```

Criados `AGENTS.md` (constituição), `CLAUDE.md`, `GEMINI.md`, `.agents/rules/`,
`TASKS.md`, `docs/ai/PROJECT_STATE.md`, `docs/ai/ROADMAP.md`, `docs/ai/WORKFLOW.md`,
`docs/handoffs/` e `docs/ADR/README.md`.
