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
FASE 1 — Qualification e segurança de atualização
```

A **Fase 0 fechou em 2026-08-31**. Gate conferido item a item:

| Critério do gate | Evidência |
| --- | --- |
| `main` contém a versão publicada mais recente | PR #5; `main` em 0.9.1 |
| `main` é o default do repositório | trocado no GitHub; branches mescladas apagadas |
| Versões sincronizadas | `release:validate-version` verde |
| README representa o produto atual | 0.9.1, com as capacidades de 0.8.0/0.9.0 |
| ARCHITECTURE representa o código atual | fluxo real documentado; afirmação falsa sobre SQL removida |
| CI reprova versão inconsistente | passo no `ci.yml`, exercitado na própria PR #5 |
| CI verde | Angular + Core Rust |

Fases 2 a 7: **não iniciar**. Ver `docs/ai/ROADMAP.md`.

## O que a Fase 1 precisa provar

```text
mudar → compilar → instalar → atualizar → recuperar
```

sem perder o livro de ninguém. Enquanto isso não estiver automatizado, toda mudança nas
fases seguintes é feita no escuro — é por isso que a Qualification vem antes do Sync V2 e
não depois.

O que já existe para aproveitar: `docs/PHASE_0_1_QUALIFICATION.md` (evidência **manual** de
0.7.4, schema 10 → 13), os Gates 1 a 5 e a Regra de publicação em
`docs/ARCHITECTURE_EVOLUTION_PLAN.md`, e 167 testes no core Rust.

O que falta é o que nenhum deles cobre: repetir aquilo sozinho, a cada PR, em bancos de
versões anteriores.

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
| Qualification harness | **Foco atual.** Só existe evidência manual de 0.7.4 em `docs/PHASE_0_1_QUALIFICATION.md` |

## Dívida arquitetural conhecida

- Nenhuma invariante de domínio aponta para o teste que a prova, e nenhum teste diz qual
  invariante defende (ver `NH-007`).
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
Workspace hardening
remoção do commands/ legado
```

Bloqueado até a Fase 1 (Qualification) fechar seu gate. A ordem não é burocracia: todos
esses itens mexem em código que hoje ninguém consegue provar que continua migrando bancos
antigos corretamente.
