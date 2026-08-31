# NarraHub — Roadmap arquitetural até a 1.0

Princípio que ordena tudo:

> Primeiro garantir que conseguimos mudar sem quebrar; depois reduzir os acoplamentos;
> só então construir as próximas fundações grandes.

E a mudança de postura em relação ao plano antigo:

> Não vamos "refatorar o NarraHub". Vamos remover **um risco arquitetural específico por
> vez**, provar que ele foi removido e deixar um teste que impeça ele de voltar.

```text
FASE -1 → Protocolo de desenvolvimento multiagente   (este scaffolding)
FASE  0 → Higiene de release / main canônica
FASE  1 → Qualification e segurança de atualização
FASE  2 → Hardening do frontend / Workspace
FASE  3 → Consolidação do Rust Core
FASE  4 → Sync V2
FASE  5 → Features e Design System
FASE  6 → Context Engine / IA
FASE  7 → Release Candidate 1.0
```

Cada fase só fecha quando o **gate** passa. Gate é obstáculo, não recomendação.

---

## FASE -1 — Protocolo multiagente

Objetivo: Codex, Claude e Gemini executarem o resto do plano com o mesmo cérebro
arquitetural, sem o humano servir de mensageiro entre eles.

Entregas: `AGENTS.md`, adaptadores por ferramenta, `TASKS.md`, `PROJECT_STATE.md`,
`WORKFLOW.md`, `docs/handoffs/`, índice e template de ADR.

**Gate:** um agente novo, sem contexto de conversa, consegue abrir o repo e descobrir
sozinho a versão atual, a fase ativa, o que está em andamento e por quem.

---

## FASE 0 — Higiene de release

Objetivo: uma única fonte da verdade do projeto.

- 0.1 Tornar `main` canônica e default do `origin`. Release nasce de `main`.
- 0.2 Garantir `main >= 0.9.1` sem force push e sem cópia cega.
- 0.3 Validação de versão obrigatória no CI comum, não só na pipeline de release.
- 0.4 `ARCHITECTURE.md` representa o código atual, não o de 0.7.
- 0.5 Regra de documentação: `CHANGELOG.md` e `docs/releases/` guardam histórico;
  `README.md` e `ARCHITECTURE.md` descrevem o estado corrente.

**Gate:** `main` contém a versão publicada mais recente; versões sincronizadas; README e
ARCHITECTURE representam o produto atual; CI reprova versão inconsistente; CI verde.

**Release sugerida:** `0.9.2` (sem feature nova).

---

## FASE 1 — Qualification / E2E

Vem **antes** do Sync V2, porque vamos mexer muito no projeto e precisamos provar o ciclo
`mudar -> compilar -> instalar -> atualizar -> recuperar` sem matar o livro de alguém.

- 1.1 Fixtures de bancos históricos anonimizados (`schema-v10/13/14/15`).
- 1.2 Qualification harness: banco antigo → versão nova → migration →
  `integrity_check` → `foreign_key_check` → dados preservados.
- 1.3 Teste de atualização N → N+1 com conteúdo real.
- 1.4 Teste de backup/restore.
- 1.5 Teste de **falha** no restore com rollback.
- 1.6 Checklist de release desktop como gate.

**Gate:** `COMPILOU != PASSOU`. Só passa com CI, testes Rust, migration, backup, restore e
app empacotado verdes.

---

## FASE 2 — Workspace Hardening

Problema real: `WorkspaceLayout` (~524 linhas) acumula navegação, lifecycle, preload,
busca, sharing, imagens, backup, updates, colaboração, universo e tags. Está virando um
segundo `AppComponent`.

- 2.1 `WorkspaceSessionService` — abrir/fechar/trocar universo, resetar stores.
- 2.2 Tirar o preload multi-domínio do layout.
- 2.3 `GlobalSearchService` assume o próprio lifecycle.
- 2.4 `WorkspaceShareService` — o layout não monta payload nem comprime WebP.
- 2.5 Ampliar `WorkspaceSyncService` para coordenação cross-domain.
- 2.6 **Não** criar event bus / CQRS / mediator ainda. Application services explícitos
  primeiro.

**Gate:** teste de fronteira provando que `WorkspaceLayout` não conhece gateways, não monta
payload de sharing, não carrega domínios manualmente e não executa regra de domínio.
Não usar contagem de linhas como regra — linhas são consequência, não arquitetura.

---

## FASE 3 — Consolidar Rust Core

- 3.1 Remover `src-tauri/src/commands/` legado; caminho único é `interface/tauri/`.
- 3.2 Todo comando: DTO → application → domain → repository. Nunca SQL gigante dentro do
  `#[tauri::command]`.
- 3.3 Manter o contrato de erros (`validation`, `not_found`, `conflict`, `storage`,
  `unavailable`); ampliar só conscientemente.
- 3.4 Toda operação de domínio multi-estrutura é atômica.
- 3.5 Sem ORM. `rusqlite` resolve.
- 3.6 Modularizar `sync.rs` / `online_share.rs` / `local_ai.rs` só quando expandirem.

**Gate:** nenhum caminho antigo paralelo sobrevive.

---

## FASE 4 — Sync V2

- 4.0 **ADR antes de código**, com threat model explícito: o alvo é outro dispositivo na
  mesma rede capturando tráfego, se passando por peer ou fazendo replay — não é alguém que
  já desbloqueou fisicamente o Windows da máquina.
- 4.1 Identidade persistente do dispositivo (chave privada nunca viaja).
- 4.2 Pairing com código temporário — para o humano, não como única segurança.
- 4.3 Transporte criptografado (Noise Protocol vs TLS + certificate pinning; decidir no ADR).
- 4.4 **Outbox**: mudança de dado e evento de sync na mesma transação.
- 4.5 Cursores por peer — sync incremental.
- 4.6 Idempotência: mesmo evento duas vezes = mesmo estado.
- 4.7 Tombstones, sem GC agressivo. Melhor guardar demais que ressuscitar conteúdo.
- 4.8 Matriz de conflitos documentada: capítulo = conflito explícito; campo simples =
  política determinística; tombstone vence update antigo; relações/tags = operações de
  conjunto idempotentes.
- 4.9 Attachments por hash SHA-256, sem reenvio.
- 4.10 Descoberta mDNS, IP manual como fallback.
- 4.11 Handshake de compatibilidade — incompatível não sincroniza e diz por quê.
- 4.12 Testes de dois dispositivos incluindo offline, reconexão, evento duplicado,
  fora de ordem, evento antigo após tombstone, peer inválido, assinatura inválida.

**Gate:** todos os itens acima verdes. **Release sugerida:** `0.11.0`.

---

## FASE 5 — Features e Design System

- 5.1 Decompor `PlanningBoardComponent` (~607 linhas).
- 5.2 Decompor a Writing Page. Tiptap continua independente.
- 5.3 Extrair responsabilidades crescentes de Entities.
- 5.4 **Teste de design tokens**: escanear `var(--*)` e comparar com as definições.
  Variável usada sem definição reprova o CI. Isso impede para sempre o bug 0.9.0/0.9.1.
- 5.5 Regressão visual por snapshot nos temas claro e escuro.

**Gate:** nenhuma refatoração muda comportamento. Se mudou comportamento, é feature e vai
para issue/PR separado.

---

## FASE 6 — Context Engine

- 6.1 Contrato `AIContext v1`.
- 6.2 Orçamento de contexto — contexto tem budget; não se manda o universo inteiro.
- 6.3 Memória sai do `localStorage` e vai para o SQLite, com escopo `global`/`universe`/`story`.
- 6.4 Separar fato canônico de preferência do escritor.
- 6.5 Proveniência obrigatória. Inferência não vira cânone em silêncio.
- 6.6 Context Builder único; provider local ou API compatível recebem o mesmo input.
- 6.7 Embeddings **depois**. Embeddings não consertam arquitetura de contexto ruim, só a
  deixam mais sofisticada.

---

## FASE 7 — 1.0 RC

- 7.1 Migration matrix de todas as versões suportadas até a atual.
- 7.2 Security review: sharing, sync, updater, sidecars, file paths, backup, restore,
  capabilities do Tauri, API keys.
- 7.3 Recovery drill: migration ruim, update quebrado, banco corrompido, restore.
- 7.4 Canary `1.0.0-rc.1` → grupo pequeno → `rc.2` → stable.
- 7.5 Definition of Done 1.0:

```text
□ Nenhum P0 aberto
□ Nenhum P1 conhecido de perda de dados
□ Migration matrix verde       □ Sync V2 verde
□ Backup/restore verde         □ Upgrade verde
□ Updater verde                □ Sharing verde
□ Windows qualification verde  □ Android qualification verde
□ CI verde                     □ Documentação atual
□ Rollback de release documentado
```

---

## Milestones sugeridos no GitHub

```text
0.9.2  — Baseline
0.10.0 — Architecture Hardening (Fases 2 e 3)
0.11.0 — Sync V2
0.12.0 — Context Engine
1.0.0  — Production Hardening
```

Milestones com issues pequenas. Nunca uma mega branch.

## O que NÃO faremos

```text
microservices   cloud database   Kafka        event bus global
CQRS completo   ORM Rust         GraphQL      Kubernetes
CRDT            embeddings em tudo            vector database
```

A arquitetura desejada continua sendo `modular monolith + local-first + Rust core +
SQLite`. Isso é vantagem, não limitação.
