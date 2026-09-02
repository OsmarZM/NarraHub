# Decisões Arquiteturais (ADRs)

Memória arquitetural permanente do NarraHub. Um ADR responde **por que** decidimos algo, e
sobrevive à pessoa e ao agente que decidiu. Se um agente precisa perguntar "por que não
fizemos Y?", a resposta deveria estar aqui.

## Índice

| ADR | Título | Status |
| --- | --- | --- |
| [0001](0001-local-ownership.md) | Local Ownership | Accepted |
| [0002](0002-modular-monolith-rust-core.md) | Monólito modular com núcleo Rust | Accepted |
| [0003](0003-zero-cloud-persistence-sharing.md) | Compartilhamento sem persistência em nuvem | Accepted |
| [0004](0004-immutable-migrations-and-updates.md) | Migrations imutáveis e atualização recuperável | Accepted |
| [0005](0005-versioned-ai-context.md) | Contexto de IA limitado e versionado | Accepted |
| [0006](0006-backup-as-critical-infrastructure.md) | Backup como infraestrutura crítica | Accepted |
| [0007](0007-modo-de-recuperacao-por-schema-incompativel.md) | Modo de recuperação por schema incompatível | Accepted |
| [0008](0008-fronteira-nativa-e-portas-de-plataforma.md) | Fronteira nativa: domínio e plataforma são portas diferentes | Accepted |
| [0009](0009-sync-v2.md) | Sync V2: peers simétricos, identidade e replicação incremental | **Accepted** (na 3ª revisão) |

Os ADRs 0001–0006 foram escritos sem campo `Status` explícito; todos estão em vigor. Os
próximos devem usar `_TEMPLATE.md`, que inclui o campo.

> Nota de leitura: o **Contexto** de um ADR descreve o mundo no momento da decisão e não é
> atualizado depois. O ADR 0002, por exemplo, cita SQL no frontend — isso era verdade na
> época e hoje não é mais. Para o estado atual, leia `docs/ai/PROJECT_STATE.md`.

## Quando escrever um ADR

- Escolha entre tecnologias ou protocolos (ex.: Noise Protocol vs TLS + certificate pinning).
- Introdução de um padrão arquitetural novo.
- Mudança de fronteira entre camadas.
- Decisão de **não** fazer algo que parece óbvio (isso é o mais valioso: evita que o mesmo
  debate volte a cada seis meses).

## Quando não escrever

- "Fiz isso e descobri aquilo" → é handoff (`docs/handoffs/`).
- Escolha local de implementação sem consequência para outras camadas.

## Fluxo

1. Agente propõe o ADR com `Status: Proposed`.
2. Humano aprova → `Status: Accepted`.
3. Código só depois. Padrão arquitetural novo não entra em silêncio dentro de um PR de
   feature.
4. Decisão superada não é apagada: vira `Status: Superseded by ADR-XXXX`.

Numeração sequencial, nunca reutilizada.
