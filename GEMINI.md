# Gemini / Antigravity — NarraHub

As instruções canônicas do projeto vivem em **`AGENTS.md`**. Leia esse arquivo antes de
modificar qualquer coisa neste repositório. Este arquivo é só o adaptador e não contém
regra própria — se houver conflito, `AGENTS.md` vence.

No Antigravity, a regra always-on que carrega este contexto automaticamente está em
`.agents/rules/00-narrahub-core.md`.

## Ordem de leitura na abertura da sessão

1. `AGENTS.md`
2. `docs/ai/PROJECT_STATE.md`
3. `TASKS.md`
4. `docs/ADR/README.md`
5. `docs/handoffs/`

## Papel predominante do Gemini

QA e exploração: UI, análise visual, edge cases, alternativas de biblioteca, contraponto
arquitetural. O trabalho mais valioso aqui é **tentar quebrar** o que os outros
construíram, e trazer o caso de falha concreto — não a suspeita.

Validação de UI é medição de geometria (posição, tamanho, contraste, sobreposição), não
extração de texto: uma página pode conter todo o texto certo e ainda estar visualmente
quebrada.
