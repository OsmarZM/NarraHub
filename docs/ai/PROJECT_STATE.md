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

**A Fase 1 não começa do zero — o plano original supunha que sim.** Levantamento de
2026-08-31:

| Já existe | Onde |
| --- | --- |
| Fixture povoada de schema 10, 18 tabelas | `src-tauri/fixtures/schema10_representative.sql` |
| Fixture **nativa** de schema 15, com as formas que a migração não produz | `src-tauri/fixtures/schema15_native.sql` |
| Gate que reprova migration nova sem fixture | `existe_fixture_nativa_para_o_schema_mais_recente` |
| Upgrade da fixture sem perda de dados | `representative_schema10_fixture_upgrades_without_data_loss` |
| Testes por migration, v7 a v15, com `foreign_key_check` | `database/migrations.rs` |
| Cadeia 1→15 em arquivo real + `integrity_check` | `full_migration_chain_creates_a_reopenable_file_database` |
| Backup com WAL, hash divergente, path traversal, staging interrompido, retenção | `database/backup.rs` |

Tudo isso já roda no CI via `cargo test`. A rede de segurança de migration e backup existe
e é boa.

O que falta é menor e mais difícil: fixtures nas versões que os usuários realmente têm
(NH-010), o rollback de um restore que falha no meio (NH-011), e o ciclo de atualização no
**app empacotado** (NH-012) — o único que teste unitário não fecha, e que hoje só existe
como evidência manual de 0.7.4.

## Prioridades imediatas

1. `NH-012` — fechar o escopo restante: backup/restore na 0.9.1, instalador por cima, e
   `0.8.0 → 0.9.1` numa VM. A execução `0.7.4 → 0.9.1` (cinco migrations, zero perda,
   segundo boot e conferência na tela aprovados) está em `docs/qualification/`.
2. `NH-007` — mapear cada invariante de domínio ao teste que a prova.
3. `NH-013` — checklist de release desktop como gate.

`NH-011` concluída: o rollback de restauração já existia e era sólido; o que faltava era
cobertura. Ver `docs/handoffs/2026-08-31-NH-011-claude.md`.

**Decisão de release tomada em 2026-08-31: segurar a 0.9.2.** O roadmap sugeria publicar ao
fechar a Fase 0, mas o que entrou desde a 0.9.1 é infraestrutura de projeto, documentação e
otimização de assets — nada que o escritor perceba. **Não reabra essa pergunta**; quando a
hora chegar, quem decide é o humano.

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
| Qualification harness | Migration, backup e restore cobertos por `cargo test` no CI |
| Ciclo de atualização empacotado | Roteiro pronto; `0.7.4 → 0.9.1` executado e aprovado; escopo restante em `NH-012` |

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

- Nenhuma invariante de domínio aponta para o teste que a prova, e nenhum teste diz qual
  invariante defende (ver `NH-007`).
- `WorkspaceLayout` orquestra domínios demais (navegação, preload, busca, sharing,
  imagens, backup, updates, colaboração).
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
Workspace hardening
remoção do commands/ legado
```

Bloqueado até a Fase 1 (Qualification) fechar seu gate. A ordem não é burocracia: todos
esses itens mexem em código que hoje ninguém consegue provar que continua migrando bancos
antigos corretamente.
