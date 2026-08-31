# NarraHub — Workflow multiagente

Como Codex, Claude e Gemini trabalham no mesmo repositório sem se atropelar e sem o
humano virar mensageiro entre eles.

```text
                    NARRAHUB REPO
                         │
              ┌──────────┴──────────┐
          AGENTS.md              TASKS.md
      "como trabalhamos"      "o que está sendo feito"
              └──────────┬──────────┘
                         │
              docs/ai/PROJECT_STATE.md
              docs/ADR/
              docs/handoffs/
                         │
             ┌───────────┼───────────┐
          CODEX        CLAUDE      GEMINI
             └───────────┼───────────┘
                         │
                 GitHub Issues / PRs
```

O Git é a camada de comunicação. Não existe reunião entre LLMs: existe um quadro branco
compartilhado e um protocolo de handoff.

---

## 1. Papéis

Não são exclusivos, mas são a função predominante de cada um.

| Agente | Papel | Onde brilha |
| --- | --- | --- |
| **Codex** | Implementador / Integrador | mudanças no repo, refactors controlados, Git, PR, testes |
| **Claude** | Arquiteto / Reviewer | planejamento, contratos, risco, Rust, decisões cross-domain, revisão longa |
| **Gemini / Antigravity** | QA / Exploração | UI, análise visual, edge cases, alternativas, contraponto arquitetural |

```text
Claude: "essa arquitetura faz sentido?"
Codex:  "implemente."
Gemini: "tente quebrar."
```

Isso vale muito mais que três agentes tentando ser "o programador".

---

## 2. Propriedade de tarefa

```text
1 tarefa = 1 dono ativo
```

Pode existir: Codex implementa, Claude revisa, Gemini valida.
Não pode existir: os três implementando ao mesmo tempo — isso produz três soluções
concorrentes e um merge impossível.

Antes de pegar uma tarefa, confirme em `TASKS.md` que ela está `READY` e sem `Owner`.

---

## 3. Branches

Uma branch por agente por tarefa, prefixada pelo agente:

```text
codex/NH-001-main-canonica
claude/NH-004-doc-baseline
gemini/NH-002-version-ci
```

Nunca commitar direto em `main`. Release nasce de `main`.

---

## 4. Worktrees

Se dois ou mais agentes trabalham **ao mesmo tempo**, worktrees deixam de ser conforto e
viram obrigação — senão um sobrescreve o arquivo não commitado do outro.

```bash
git worktree add ../NarraHub-codex -b codex/NH-001-main-canonica
```

```bash
git worktree add ../NarraHub-claude -b claude/NH-004-doc-baseline
```

```bash
git worktree add ../NarraHub-gemini -b gemini/NH-002-version-ci
```

Layout resultante:

```text
Projetos MVP/
├── NarraHub/            ← checkout principal
├── NarraHub-codex/
├── NarraHub-claude/
└── NarraHub-gemini/
```

Mesmo repositório Git, árvores de trabalho fisicamente separadas. Cada worktree precisa do
próprio `npm ci` (o `node_modules` não é compartilhado).

Ao terminar:

```bash
git worktree remove ../NarraHub-codex
```

---

## 5. Handoff — onde eles realmente conversam

Ao terminar um trabalho, o agente escreve um arquivo em `docs/handoffs/`:

```text
docs/handoffs/2026-08-31-NH-001-codex.md
```

O próximo agente lê e continua. Codex "falou" com Claude sem existir conversa ativa entre
os dois, e sem o humano copiar e colar nada.

Template: `docs/handoffs/_TEMPLATE.md`.

**Por que arquivos separados por tarefa:** se os três editarem `TASKS.md` ao mesmo tempo,
o resultado é `<<<<<<< HEAD`. `TASKS.md` guarda só a fila estável; o detalhe do trabalho
mora no handoff, que ninguém mais toca.

---

## 6. Handoff não é ADR

Distinção essencial:

| | Handoff | ADR |
| --- | --- | --- |
| Responde | "fiz isso, descobri isso, o próximo passo é esse" | "decidimos X em vez de Y, e por quê" |
| Vida útil | efêmero, específico da tarefa | permanente |
| Onde | `docs/handoffs/` | `docs/ADR/` |

Seis meses depois, um agente não deve perguntar "por que não usamos TLS?". Ele lê o ADR.

---

## 7. Ordem canônica de leitura

```text
1. AGENTS.md
2. docs/ai/PROJECT_STATE.md      ← versão, fase ativa, dívida
3. TASKS.md                      ← o que fazer, quem já está fazendo
4. docs/ADR/README.md            ← o que já foi decidido
5. docs/handoffs/                ← o que acabou de acontecer
6. docs/ai/ROADMAP.md            ← o que vem depois (não implementar ainda)
```

---

## 8. Quando adicionar MCP Agent Mail

Existem servidores MCP de correio entre agentes (inbox, threads, reserva de arquivos) com
suporte a Claude Code, Codex CLI e Gemini CLI. Eles resolvem `file locks`, `task leases`,
`presence` e mensagens em tempo real.

**Não comece por aí.** Montar mailboxes, leases e um servidor antes de existir um protocolo
simples é overengineering de ferramenta para evitar overengineering de projeto.

- **Etapa A (agora):** só Git — este scaffolding + worktrees. Resolve ~80–90% do problema.
- **Etapa B (depois):** quando você passar a usar os três **simultaneamente** em vez de
  alternadamente, e a dor de lock/lease for concreta, aí sim avalie o Agent Mail.

Antes de adotar, revise a procedência do servidor MCP: ele recebe acesso ao repositório e
ao contexto dos agentes.
