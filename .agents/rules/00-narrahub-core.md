---
activation: always
---

# NarraHub — regra núcleo

As instruções canônicas do projeto são:

@AGENTS.md

Estado corrente do desenvolvimento (versão, fase ativa, dívida conhecida):

@docs/ai/PROJECT_STATE.md

Fila de tarefas e donos:

@TASKS.md

Decisões arquiteturais já tomadas:

@docs/ADR/README.md

Como os agentes trabalham juntos (branches, worktrees, handoffs, papéis):

@docs/ai/WORKFLOW.md

## Antes de qualquer alteração

1. Identificar a fase ativa em `PROJECT_STATE.md` e não implementar trabalho de fase futura.
2. Confirmar em `TASKS.md` que nenhum outro agente é dono da tarefa nem dos arquivos.
3. Nenhum SQL no Angular. Nenhum `invoke()` fora de gateway autorizado.
4. Migration publicada é imutável.
5. Padrão arquitetural novo exige ADR antes do código.

## Ao terminar

Escreva um handoff em `docs/handoffs/` e atualize o status da tarefa em `TASKS.md`.
Build verde não significa tarefa concluída.
