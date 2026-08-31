# NarraHub — Estado corrente de engenharia

> Fonte da verdade sobre "onde estamos". Qualquer agente lê este arquivo antes de agir.
> Atualize-o ao fechar uma tarefa que mude versão, fase ou dívida conhecida.

Atualizado em: 2026-08-31

## Versão

| Item | Valor |
| --- | --- |
| Última tag publicada | `app-v0.9.1` |
| Manifests em `feat/native-app-foundation` | 0.9.1 (package.json, Cargo.toml, tauri.conf.json) |
| `origin/main` | **0.9.1** — canônica desde a PR #5; protegida, promoção só por PR |
| README.md | 0.9.1 — validado pelo CI desde 4b31646 |

## Branch canônica

`main`, desde a PR #5 (2026-08-31).

`origin/main` estava em 0.8.0, parada na PR #1, enquanto 0.9.0 e 0.9.1 saíam de
`feat/native-app-foundation` — que era o default do `origin`. A branch órfã era a `main`.
A PR #5 devolveu a ela o papel de linha oficial, sem force push: hoje as árvores das duas
são idênticas.

- `main` é protegida: push direto é recusado, promoção só por Pull Request.
- Branch nova nasce de `main` e volta para `main` por PR.
- **Pendente (humano):** trocar o default do repositório no GitHub para `main` e decidir o
  destino de `feat/native-app-foundation`.

Ao verificar o estado das branches, compare sempre contra `origin/main` depois de
`git fetch` — a `main` local pode estar atrasada e dar um diagnóstico errado.

## Fase ativa

```text
FASE 0 — Higiene de release / main canônica
```

Fases 1 a 7: **não iniciar**. Ver `docs/ai/ROADMAP.md`.

## Prioridades imediatas

A Fase 0 está **substancialmente fechada**. NH-001 a NH-005 concluídas; resta:

1. `NH-001` — trocar o default do repositório para `main` e aposentar
   `feat/native-app-foundation`. **Decisão humana, não faça sozinho.**
2. `NH-006` — reconciliar `ARCHITECTURE_EVOLUTION_PLAN.md` com o roadmap novo.

Depois disso, o gate da Fase 0 fecha e a **Fase 1 (Qualification)** abre. Não comece a
Fase 1 antes disso.

## Status arquitetural

| Área | Status |
| --- | --- |
| SQL no frontend | **Eliminado** — proibido por `tests/frontend-boundaries.test.mjs` |
| Migração de Router | **Concluída** |
| Rust Application Core | **Concluído** — resta limpar `src-tauri/src/commands/` legado |
| Validador de versão | Roda no CI comum; cobre os 3 manifests + README + CHANGELOG |
| CI | `ci.yml` cobre Angular + Rust em PR e push |
| Sync V2 | **Não iniciado** |
| Context Engine / IA | **Não iniciado** |
| Qualification harness | Parcial — `docs/PHASE_0_1_QUALIFICATION.md` é evidência manual de 0.7.4 |

## Dívida arquitetural conhecida

- `docs/ARCHITECTURE_EVOLUTION_PLAN.md` usa a numeração de fases antiga e conflita com o
  roadmap novo (ver `NH-006`).
- `WorkspaceLayout` orquestra domínios demais (navegação, preload, busca, sharing,
  imagens, backup, updates, colaboração).
- `src-tauri/src/commands/` legado coexiste com `interface/tauri/` — **9 arquivos de comando
  de cada lado**, ou seja, dois caminhos completos até o banco, não uma sobra pequena.
- Sync V1 não tem transporte criptografado, identidade de dispositivo, outbox nem
  tombstones.
- Sem teste de tokens de design — foi a causa do bug 0.9.0/0.9.1 (`var(--nh-glass-panel)`
  usado sem definição).
- Árvore de trabalho local com modificações não commitadas em `src-tauri/Cargo.toml`,
  `src/index.html`, `src/styles.css` e assets novos em `public/`.

## Não trabalhar ainda

```text
Sync V2
Context Engine / embeddings
decomposição de features (Planning, Writing, Entities)
design system hardening
```

Bloqueado até a Fase 0 e a Fase 1 (Qualification) fecharem seus gates.
