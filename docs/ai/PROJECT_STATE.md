# NarraHub — Estado corrente de engenharia

> Fonte da verdade sobre "onde estamos". Qualquer agente lê este arquivo antes de agir.
> Atualize-o ao fechar uma tarefa que mude versão, fase ou dívida conhecida.

Atualizado em: 2026-08-31

## Versão

| Item | Valor |
| --- | --- |
| Última tag publicada | `app-v0.9.1` |
| Manifests em `feat/native-app-foundation` | 0.9.1 (package.json, Cargo.toml, tauri.conf.json) |
| `main` | **0.7.6** |
| README.md | **0.7.4** (desatualizado) |

## Branch canônica

**Atenção — a situação real é o inverso do que se supunha.**

- O default do `origin` é `feat/native-app-foundation`, e é essa branch que contém
  0.8.0, 0.9.0 e 0.9.1.
- `main` parou em 0.7.6 e é a branch órfã.

Portanto a Fase 0 não é "reintegrar a release na main": é **promover a linha de
desenvolvimento real para `main`** e passar a tratar `main` como default do repositório.
Ver `NH-001` em `TASKS.md`.

## Fase ativa

```text
FASE 0 — Higiene de release / main canônica
```

Fases 1 a 7: **não iniciar**. Ver `docs/ai/ROADMAP.md`.

## Prioridades imediatas

1. `NH-001` — tornar `main` canônica e default.
2. `NH-002` — plugar o validador de versão no CI comum.
3. `NH-003` — cobrir README no validador de versão.
4. `NH-004` — atualizar README e ARCHITECTURE.md para o estado corrente.

## Status arquitetural

| Área | Status |
| --- | --- |
| SQL no frontend | **Eliminado** — proibido por `tests/frontend-boundaries.test.mjs` |
| Migração de Router | **Concluída** |
| Rust Application Core | **Concluído** — resta limpar `src-tauri/src/commands/` legado |
| Validador de versão | Script existe (`scripts/validate-release-version.mjs`), **não roda no CI comum** |
| CI | `ci.yml` cobre Angular + Rust em PR e push |
| Sync V2 | **Não iniciado** |
| Context Engine / IA | **Não iniciado** |
| Qualification harness | Parcial — `docs/PHASE_0_1_QUALIFICATION.md` é evidência manual de 0.7.4 |

## Dívida arquitetural conhecida

- `main` e `README.md` não representam o produto publicado.
- `ARCHITECTURE.md` descreve o estado 0.7.x; afirma coisas que hoje são falsas
  (ex.: CRUD via SQL no Angular).
- `WorkspaceLayout` orquestra domínios demais (navegação, preload, busca, sharing,
  imagens, backup, updates, colaboração).
- `src-tauri/src/commands/` legado coexiste com `interface/tauri/`.
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
