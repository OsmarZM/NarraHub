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
FASE 3 — Consolidar Rust Core
```

A **Fase 2 fechou em 2026-09-01**. O gate é executável — `GATE DA FASE 2: as quatro
proibições do WorkspaceLayout` — e cada proibição foi verificada por mutação:

| Proibição | Onde foi parar |
| --- | --- |
| conhecer implementação de gateway | continua proibido; o layout fala com stores e serviços |
| montar payload de compartilhamento | `WorkspaceShareService` |
| carregar domínios manualmente | `WorkspaceSessionService` |
| executar regra de domínio | `WorkspaceSyncService`; SQL nunca |

`WorkspaceLayout` foi de **523 para 434 linhas** — mas o número é consequência. O que mudou é
que ele deixou de conhecer a lista de domínios, a persistência da sessão, a compressão de
imagem e a sequência de releitura após colaboração.

Duas tarefas da fase não precisaram de código, e verificar antes economizou o trabalho:
`NH-021` foi absorvida pela `NH-020` porque separá-las deixaria um estado pior; `NH-022` foi
resolvida pela `NH-020`, já que o acoplamento que ela atacaria saiu junto com a sessão.

**Não foi criado event bus, CQRS nem mediator** — application services explícitos deram conta.

## Lição que a Fase 2 deixou

O gate passou na primeira tentativa **com a violação reintroduzida de propósito**. A causa
não estava na regra: as expressões regulares foram escritas por script Python, e `\b` virou um
caractere de **backspace literal** (0x08) — invisível no `grep`, e que nunca casa.

Uma varredura achou 18 ocorrências em 5 asserções, três delas de testes anteriores que já
tinham sido declarados verificados. Todas corrigidas e reverificadas.

**Regra prática:** teste que nunca falhou não é teste. Antes de confiar num gate novo, quebre
a regra de propósito e confirme que ele acusa — e, se a asserção foi gerada por script,
confira os bytes.

## O que a Fase 3 precisa resolver

`src-tauri/src/commands/` (9 arquivos) coexiste com `interface/tauri/` (9 arquivos): dois
caminhos completos até o banco. A fase remove o antigo.

Regras da fase, do roadmap: todo comando segue `DTO → application → domain → repository`;
o contrato de erros (`validation`, `not_found`, `conflict`, `storage`, `unavailable`) é
mantido; operação multi-estrutura é atômica; **sem ORM**; e `sync.rs`, `online_share.rs` e
`local_ai.rs` só se modularizam quando expandirem, não porque cresceram.

**Gate:** nenhum caminho antigo paralelo sobrevive.

## Status arquitetural

| Área | Status |
| --- | --- |
| SQL no frontend | **Eliminado** — proibido por `tests/frontend-boundaries.test.mjs` |
| Migração de Router | **Concluída** |
| Rust Application Core | **Concluído** — resta limpar `src-tauri/src/commands/` legado |
| Validador de versão | Roda no CI comum; cobre os 3 manifests + README + CHANGELOG |
| CI | `ci.yml` cobre Angular + Rust em PR e push |
| `WorkspaceLayout` | **Resolvido** na Fase 2, com gate executável |
| `commands/` legado | **Foco atual** — Fase 3 |
| Sync V2 | **Não iniciado** |
| Context Engine / IA | **Não iniciado** |
| Qualification harness | **Concluído.** Migration, backup, restore e rollback cobertos por `cargo test` no CI |
| Ciclo de atualização empacotado | **Concluído.** Roteiro, checklist de release e três execuções reais |

## Versões e schema

Para escolher o par de versões de qualquer teste de upgrade, o que importa é cruzar
migration — não pegar a versão mais recente:

| Versão | Schema |
| --- | --- |
| 0.7.6 | 14 |
| 0.8.0 | 14 |
| 0.9.0 e 0.9.1 | 15 |
| `main` hoje | 15 |

Consequência prática: **uma 0.9.2 publicada hoje não exercitaria migration nenhuma** num
upgrade a partir da 0.9.1. O par útil hoje é `0.8.0 → 0.9.1`, e as duas já estão publicadas
com instalador e assinatura.

## Ambiente de desenvolvimento

`TMP`/`TEMP` desta máquina apontam para o `C:`, que está com 87% de uso. Uma
recompilação completa das dependências Rust derruba o `rustc` com
`STATUS_STACK_BUFFER_OVERRUN` em crates de terceiros — erro que parece bug de toolchain
e é falta de espaço. Rode cargo com os temporários no `D:`:

```bash
TMP='D:\DevTools\NarraHubTmp' TEMP='D:\DevTools\NarraHubTmp' cargo test --manifest-path src-tauri/Cargo.toml
```

O CI (Ubuntu) nunca reproduziu isso. Crash estranho de compilador aqui: suspeitar de
disco antes de suspeitar do código.

## Dívida arquitetural conhecida

- A tela de recuperação de schema (`NH-015`) **nunca foi vista rodando** — só testada. Para
  vê-la, aponte um perfil descartável para um banco de schema maior que o
  `LATEST_SCHEMA_VERSION` e rode `npm run desktop:qualification`.
- **Versões já publicadas continuam sem a tela de recuperação.** A 0.8.0 é imutável: quem
  voltar para ela seguirá com um app que não abre. O portão só protege downgrades feitos a
  partir da primeira versão que o contiver.

- `src-tauri/src/commands/` legado coexiste com `interface/tauri/` — **9 arquivos de comando
  de cada lado**, ou seja, dois caminhos completos até o banco, não uma sobra pequena.
- Sync V1 não tem transporte criptografado, identidade de dispositivo, outbox nem
  tombstones.
- Sem teste de tokens de design — foi a causa do bug 0.9.0/0.9.1 (`var(--nh-glass-panel)`
  usado sem definição). Checagem ad hoc em 2026-08-31: 34 tokens definidos, 22 usados sem
  valor de reserva, **zero** usados sem definição. O estado hoje está são; nada impede a
  regressão de voltar. É a `NH-050`.
- O tema claro ganhou arte de fundo própria na PR #9 e **ainda não foi visto rodando** — o
  app precisa do runtime Tauri. Vale olhar com `npm run desktop:dev` antes de qualquer
  release.
- `public/assets/narrahub-logo-full.png` (1 MB) ficou sem referência depois da PR #9.
- Não existe escala compartilhada de breakpoints: 12 valores diferentes e 7 arquivos sem
  nenhuma media query (`NH-052`, Fase 5). O defeito de conteúdo inalcançável em 1366×768
  (`NH-051`) era outra coisa e já foi corrigido.

## Não trabalhar ainda

```text
Sync V2
Context Engine / embeddings
decomposição de features (Planning, Writing, Entities)
design system hardening
remoção do commands/ legado
```

A Fase 1 fechou, então a rede de segurança existe: mudanças agora são provadas contra
migration, backup e restauração automaticamente. O que continua bloqueado está bloqueado
pela ordem do roadmap, não por falta de rede.
