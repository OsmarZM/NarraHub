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
FASE  3.5 → Fronteira nativa do frontend
FASE  4 → Sync V2                               FECHADA — QUALIFIED (2026-09-25)
FASE  4.5 → Device Discovery & Pairing UX       ← ativa
FASE  5 → Mobile UX + Product/Design Hardening
FASE  6 → Context Engine / IA                   → 1.1, não bloqueia a 1.0
FASE  7 → Release Candidate 1.0
```

**Revisão de 2026-09-25, depois da qualificação física do Sync V2.** O caminho até a 1.0 passou a
ser `4.5 → 5 → 7`. A Fase 6 continua planejada e vai para a 1.1. Objetivo da 1.0:

> NarraHub 1.0 = local-first, confiável, mobile-native e simples de conectar.

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

**Gate revisto após revisão arquitetural:** a formulação original — "nenhum caminho antigo
paralelo sobrevive" — era fraca nas duas direções. Alarmista sobre `commands/`, que tinha 35
linhas e oito arquivos de placeholder; e frouxa sobre o que importava, porque não pegaria o
`#[tauri::command]` de domínio que vivia em `database/planning.rs` com validação, transação e
SQL no mesmo arquivo.

O gate real é sobre **colocação**:

> Comando de domínio só nasce em `interface/tauri`.

Com exceções nomeadas para infraestrutura genuína — backup, recovery, health, réplica, IA,
share e sync — cada uma com o motivo escrito. Exceção sem justificativa é violação com
permissão. Ver `interface/command_placement.rs`.

---

## FASE 3.5 — Fronteira nativa do frontend

Acrescentada em 2026-09-01, a partir de revisão arquitetural. Pequena, e precisa vir antes do
Sync V2 — que vai adicionar capacidade de plataforma nova e precisa de um lugar óbvio para
ela.

**O problema que ela resolve:** a documentação afirmava que `RustCoreService` era a única
porta Tauri do frontend. Nunca foi — sete arquivos falavam com o Tauri, e `getCurrentWindow()`
estava solto em mais quatro. Não era o código errado: era a documentação forçando
**persistência de domínio** e **capacidade de plataforma** dentro da mesma abstração.

```text
Feature Store → Domain Gateway → RustCoreService   o que o escritor cria
                                 core/native/*     o que o sistema oferece
```

**Feito:** portas separadas, seis serviços movidos para `core/native/`, `NativeWindowService`
criado, e um gate que varre `src/app` inteiro atrás de `invoke()`, `api/window` e plugins.
Ver [ADR 0008](../ADR/0008-fronteira-nativa-e-portas-de-plataforma.md).

**O que sobra para quando o Sync V2 chegar:** cada capacidade nova entra como porta em
`core/native/`, com uma linha no gate e a justificativa de por que é plataforma e não domínio.

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

**Fechada em 2026-09-25.** Etapas 1–14 do ADR 0009, mais E (Hello), F (resolução de conflitos),
G (remoção do Sync V1), H (hardening) e I (qualificação física, I1–I20 PASS em Windows 11 e
Samsung Galaxy S23 — PR #74, merge `8d7562d`). **Sync V2 architecture = QUALIFIED.** O item 4.10
(descoberta mDNS) não entrou: virou a Fase 4.5. Protocolo, Noise, identidade, causalidade, grupos
de mutação, bootstrap, blobs, `conflict_resolution` e formato canônico ficam congelados salvo bug
comprovado; trabalho de sync daqui em diante é `bugfix`, `performance`, `UX`, `feature` ou
`release hardening`.

---

## FASE 4.5 — Device Discovery & Pairing UX

Objetivo: tornar o pareamento simples **sem criar um segundo protocolo**. O sync continua
`Noise + identidade + Sync V2`. Descoberta só encontra o endpoint ou o convite:

```text
BLE / mDNS / QR  →  descobre endpoint ou convite  →  pareamento existente  →  TCP/Noise  →  Sync V2
```

| Método | Papel |
| --- | --- |
| IP manual | já existe; continua como alternativa |
| QR Code | implementar |
| mDNS | lista de aparelhos na mesma rede |
| Bluetooth LE | descoberta e passagem de convite **só**; não transfere eventos, banco nem blobs |

Tela única, nos dois lados:

```text
Adicionar dispositivo                     Tornar este dispositivo disponível
  [ Escanear QR ]                           QR · PIN · validade · nome do aparelho
  Aparelhos próximos
    PC do Osmar
    Galaxy S23
  [ endereço manual ]
```

Segurança a provar com gate: **descoberta ≠ confiança**; anúncio falso não entra no roster; QR
expirado falha; PIN errado falha; aparelho fora da confiança não sincroniza. Cada gate só conta
depois de ser visto falhando com mutação ou implementação incompleta, e cada fatia repete Windows ↔
Android físico.

Fatias, uma por PR, na ordem: **QR → mDNS → BLE → tela de pareamento**. Tarefa: **NH-084**.

**Gate:** os quatro métodos levam ao mesmo pareamento; os gates de segurança acima reprovam com a
implementação incompleta; Windows ↔ Android físico verde.

---

## FASE 5 — Mobile UX + Product/Design Hardening

Premissa: **não adaptar o desktop ao celular.** O mobile tem composição própria (ADR 0011);
compartilha domínio, stores, serviços, Router e handlers, e não precisa compartilhar layout,
hierarquia visual, densidade, navegação, composição nem interação.

| Item | Conteúdo |
| --- | --- |
| M0 | **auditoria no S23 físico**, screenshots de todas as páginas classificadas em OK / desconfortável / desktop espremido / inutilizável → `docs/mobile/UX_AUDIT_V1.md`. Nenhum redesign antes dela |
| M1 | linguagem visual mobile: tipografia, espaçamento, raio, cards, sheets, navegação inferior, topbar, alvos de toque (44–48px+), iconografia, movimento, háptica, claro/escuro; inputs nunca abaixo de 16px |
| M2 | navegação primária descobrível (Início · Escrever · Mundo · Planejar · Mais); a alça lateral vira *Quick Switcher*; nada essencial só por gesto escondido |
| M3 | Home: continuar escrevendo em um toque, universos e objetos recentes, atividade e sync |
| M4 | escrita (prioridade máxima): título, editor em tela cheia, barra junto do teclado; capítulos, resumo, notas, histórico e propriedades em sheets; teclado real, seleção, copiar/colar, paisagem, texto longo |
| M5 | personagens e entidades: seções, acordeões, chips, sheets — não o formulário do desktop comprimido |
| M6 | universo como hub: livro atual, continuar escrevendo, personagens, lugares, histórias, timeline, atividade |
| M7 / M8 | planejamento uma coluna por vez com swipe; timeline vertical |
| M9 | conexões: lista de relações por padrão, grafo em tela cheia sob demanda |
| M10 | sync em linguagem de usuário (seus dispositivos, sincronizado, esperando, precisa de atenção); nada de peer, roster, vector, bootstrap, mutation, Noise |
| M11 | movimento 180–260 ms, springs leves, só `transform`/`opacity`; háptica só em evento significativo; respeitar *reduced motion* |
| M12 | design system: tokens oficiais; **`var(--token)` sem definição reprova o CI** (antiga 5.4); breakpoints novos unificados; sem mega-PR de CSS legado |
| M13 | decomposição de Planning, Writing e Entities só onde a responsabilidade é comprovadamente excessiva ou bloqueia mobile/testabilidade — nunca por contagem de linhas |
| M14 | regressão visual por snapshot: claro, escuro, desktop, mobile (antiga 5.5) |
| M15 | gate físico no S23 (retrato, paisagem, teclado aberto, claro, escuro) e Playwright em vários celulares |

**Gate (M15):** zero rolagem horizontal; nenhuma ação importante abaixo do alvo de toque; nenhuma
dependência de hover; nenhuma ação essencial só por gesto escondido; editor confortável com teclado;
nada atrás das barras do sistema; nenhuma tela que seja só o desktop comprimido; navegação principal
em no máximo duas ações. Tarefa: **NH-085**.

---

## FASE 6 — Context Engine (1.1)

Planejada, **não bloqueia a 1.0** e não é implementada antes dela. Nada de banco vetorial nem
embeddings durante o hardening da 1.0.

- 6.1 Contrato `AIContext v1`.
- 6.2 Orçamento de contexto — contexto tem budget; não se manda o universo inteiro.
- 6.3 Memória sai do `localStorage` e vai para o SQLite, com escopo `global`/`universe`/`story`.
- 6.4 Separar fato canônico de preferência do escritor.
- 6.5 Proveniência obrigatória. Inferência não vira cânone em silêncio.
- 6.6 Context Builder único; provider local ou API compatível recebem o mesmo input.
- 6.7 Embeddings **depois**. Embeddings não consertam arquitetura de contexto ruim, só a
  deixam mais sofisticada.

---

## FASE 7 — Release Candidate 1.0

Só começa com a 4.5 e a 5 fechadas. Tarefa: **NH-086**.

- **R1 Migration matrix** — todos os upgrades suportados até a 1.0. Obrigatório `0.9.2 → 1.0`
  sobre uma **cópia** do acervo real do usuário, nunca o original.
- **R2 Security review** — sync, Noise, QR, mDNS, descoberta Bluetooth, updater, sidecars, file
  paths, backup, restore, capabilities do Tauri, API keys, FileProvider do Android.
- **R3 Recovery drill** — injetar migration ruim, banco inválido, update interrompido, restore
  inválido, blob faltando e sync interrompido; provar a recuperação.
- **R4 Qualification** — de novo: Windows, Android, updater, backup/restore, sync físico,
  pareamento por QR, descoberta na LAN e descoberta Bluetooth.
- **R5 RC** — `1.0.0-rc.1` → canary pequeno → correções → `rc.2` → canary → `1.0.0`. Nunca
  estável direto.

Definition of Done da 1.0:

```text
□ nenhum P0                               □ nenhum P1 conhecido de perda de dados
□ migrations verdes                       □ backup/restore verde
□ updater verde                           □ Sync V2 verde
□ QR verde                                □ discovery verde
□ mobile UX aprovada                      □ claro e escuro aprovados
□ Windows qualification verde             □ Android qualification verde
□ CI verde                                □ rollback documentado
□ documentação atual
```

---

## Ordem de execução, revista em 2026-09-01

A revisão arquitetural propôs uma ordem que o roadmap adota:

| # | O quê | Por que nesta posição |
| --- | --- | --- |
| 1 | Fonte da verdade multiagente sob teste | três agentes leem `PROJECT_STATE.md` antes de agir; desatualizado, os três erram juntos |
| 2 | Fechar o Rust Core | comando de domínio só em `interface/tauri` |
| 3 | Fronteira nativa do frontend | o Sync V2 vai trazer capacidade nova e precisa de onde colocá-la |
| 4 | Sync V2 | ADR e threat model antes de qualquer código |
| 5 | Design system e breakpoints | depois das fundações de dados, não antes |
| 6 | Context Engine | depende de identidade de alteração e revisões, que o Sync V2 amadurece |
| 7 | Colaboração em tempo real / CRDT | só se o produto pedir edição simultânea de texto |

Os três primeiros foram concluídos em 2026-09-01, e o Sync V2 em 2026-09-25.

## Ordem de execução até a 1.0, revista em 2026-09-25

Uma fatia por PR, sem mega branch; a próxima só começa com a anterior revisada e mergeada, e nenhum
merge é automático. Antes de qualquer fatia grande, o plano vem para revisão com estado atual,
problema observado, contrato proposto, arquivos afetados, gates, riscos e rollback.

```text
PR A    roadmap e documentação (esta revisão)
PR B    QR                          PR C    mDNS
PR D    descoberta BLE              PR E    tela de pareamento
PR F    linguagem visual mobile     PR G    navegação
PR H    Home                        PR I    escrita
PR J    entidades                   PR K    planejamento e timeline
PR L    sync no mobile              PR M    gates do design system
PR N    regressão visual            PR O+   endurecimento do release candidate
```

Cada PR: hipótese → gate antes → implementação mínima → teste → gate visto falhando (mutação ou
implementação incompleta) → teste físico quando depende da plataforma → documentação.

## Milestones sugeridos no GitHub

```text
0.9.2  — Baseline
0.10.0 — Sync V2 + shell mobile (Fases 2, 3, 4 e ADR 0011)
0.11.0 — Device Discovery & Pairing UX (4.5)
0.12.0 — Mobile UX + Design Hardening (5)
1.0.0  — Release Candidate e estável (7)
1.1.0  — Context Engine (6)
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

**Sobre CRDT, com uma ressalva.** Para universos, personagens, entidades, relações, cards,
timeline, livros e metadados, `outbox + operações idempotentes + tombstones + conflitos
explícitos` resolve. CRDT ali seria custo sem benefício.

O único candidato legítimo é **texto de capítulo em edição simultânea** — e só naquele
agregado, no futuro, se o produto pedir. Transformar o SQLite inteiro em CRDT é a ideia que
parece linda e consome seis meses.

O modelo do NarraHub reduz ainda mais essa pressão: **autoria canônica permanece sob controle
do escritor**, então não há necessidade de convergência automática agressiva.
