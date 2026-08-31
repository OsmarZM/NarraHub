# Handoffs

Cada trabalho concluído deixa um arquivo aqui. É assim que Codex, Claude e Gemini
"conversam" sem existir uma conversa ativa entre eles e sem o humano copiar e colar nada.

## Nomenclatura

```text
AAAA-MM-DD-NH-XXX-<agente>.md
```

Exemplo:

```text
2026-08-31-NH-001-codex.md
```

## Por que arquivos separados

Se os três agentes editarem o mesmo arquivo de status ao mesmo tempo, o resultado é
conflito de merge. `TASKS.md` guarda só a fila estável e cada agente edita apenas a sua
entrada; o detalhe do trabalho mora aqui, num arquivo que ninguém mais toca.

## Handoff não é ADR

| | Handoff | ADR |
| --- | --- | --- |
| Responde | "fiz isso, descobri isso, o próximo passo é esse" | "decidimos X em vez de Y, e por quê" |
| Vida útil | efêmero, específico da tarefa | permanente |
| Onde | `docs/handoffs/` | `docs/ADR/` |

Use `_TEMPLATE.md`.
