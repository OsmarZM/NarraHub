# NarraHub — Estado corrente de engenharia

> Fonte da verdade sobre "onde estamos". Qualquer agente lê este arquivo antes de agir.
> Atualize-o ao fechar uma tarefa que mude versão, fase ou dívida conhecida.

Atualizado em: 2026-08-31

## Versão

| Item | Valor |
| --- | --- |
| Última tag publicada | `app-v0.9.1` |
| Manifests em `feat/native-app-foundation` | 0.9.1 (package.json, Cargo.toml, tauri.conf.json) |
| `origin/main` | **0.8.0** — protegida, promoção só por Pull Request |
| README.md | 0.9.1 — validado pelo CI desde 4b31646 |

## Branch canônica

**Atenção — a situação real é o inverso do que se supunha.**

- O default do `origin` é `feat/native-app-foundation`, e é essa branch que contém
  0.8.0, 0.9.0 e 0.9.1.
- `origin/main` parou em 0.8.0, na PR #1, e é a branch órfã.

Cuidado ao verificar isto: a `main` **local** pode estar ainda mais atrasada (estava em
0.7.6). Compare sempre contra `origin/main` depois de `git fetch`.

Portanto a Fase 0 não é "reintegrar a release na main": é **promover a linha de
desenvolvimento real para `main`** e passar a tratar `main` como default do repositório.

Verificado em 2026-08-31: o único commit exclusivo de `origin/main` é o merge da PR #1,
que não tem conteúdo próprio. O merge foi testado localmente e deixa a árvore de `main`
idêntica à da branch de trabalho. `main` já está protegida, então a promoção passa por
Pull Request. Ver `NH-001` em `TASKS.md`.

## Fase ativa

```text
FASE 0 — Higiene de release / main canônica
```

Fases 1 a 7: **não iniciar**. Ver `docs/ai/ROADMAP.md`.

## Prioridades imediatas

1. `NH-001` — tornar `main` canônica e default. **Falta a parte do GitHub (humano).**
2. `NH-002` — validador de versão no CI comum. **Concluída.**
3. `NH-003` — README coberto pelo validador. **Concluída.**
4. `NH-004` — README e ARCHITECTURE.md no estado corrente. **Concluída.**
5. `NH-006` — reconciliar `ARCHITECTURE_EVOLUTION_PLAN.md` com o roadmap novo.

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

- `main` não representa o produto publicado.
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
