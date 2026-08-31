# Claude Code — NarraHub

As instruções canônicas do projeto vivem em **`AGENTS.md`**. Leia esse arquivo antes de
modificar qualquer coisa neste repositório. Este arquivo é só o adaptador do Claude e não
contém regra própria — se houver conflito, `AGENTS.md` vence.

## Ordem de leitura na abertura da sessão

1. `AGENTS.md` — constituição
2. `docs/ai/PROJECT_STATE.md` — versão, fase ativa, dívida conhecida
3. `TASKS.md` — o que fazer e quem já está fazendo
4. `docs/ADR/README.md` — o que já foi decidido
5. `docs/handoffs/` — o que outro agente acabou de fazer

## Skills deste projeto

- `.claude/skills/narrahub-architecture/`
- `.claude/skills/narrahub-database-safety/`
- `.claude/skills/narrahub-feature-extraction/`
- `.claude/skills/narrahub-validate/`

## Papel predominante do Claude

Arquiteto e revisor: planejamento, contratos, análise de risco, Rust, decisões
cross-domain, revisão longa. Isso não impede o Claude de implementar — só define para onde
ele é puxado quando os três agentes estão ativos. Ver `docs/ai/WORKFLOW.md`.

## Lembretes que costumam ser esquecidos aqui

- Validação de UI é medição de geometria, não extração de texto.
- Build verde não é tarefa concluída.
- Não implemente trabalho de fase futura; registre em `TASKS.md` como `BACKLOG`.
- Ao terminar, escreva um handoff em `docs/handoffs/` e atualize o status em `TASKS.md`.
