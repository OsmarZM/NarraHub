# NarraHub — Constituição dos Agentes

Este é o documento canônico para qualquer agente (Claude Code, Codex, Gemini/Antigravity)
que modifique este repositório. `CLAUDE.md`, `GEMINI.md` e `.agents/rules/` são apenas
adaptadores: eles apontam para cá e não contêm regra própria.

Se algo aqui conflitar com outro arquivo de instrução, **este arquivo vence**.

---

## 1. Arquitetura

```text
Frontend
Feature Component -> Feature Store -> Gateway tipado -> RustCoreService -> invoke()

Backend
interface/tauri -> application -> domain -> repository -> SQLite
```

- O NarraHub é local-first. O SQLite local é a fonte canônica dos dados do usuário.
- O produto é um monólito modular. Não introduza microserviços, ORM, CRDT, event bus
  global, banco vetorial ou reescrita ampla sem ADR aprovado.
- A nuvem pode servir código estático ou transportar bytes; ela nunca persiste
  conteúdo narrativo.

## 2. Regras não negociáveis

1. Nenhum SQL no Angular. Código de UI não conhece tabelas nem o plugin SQLite.
2. Nenhum `invoke()` fora de um gateway/adaptador autorizado.
3. Migration publicada é imutável. Toda correção de schema é uma migration nova.
4. Mutação crítica que toca várias tabelas é transacional: tudo ou nada.
5. O Router é a fonte da verdade da navegação.
6. IA nunca altera conteúdo canônico sem confirmação explícita do escritor.
7. Sync nunca resolve conflito silenciosamente.
8. Compatibilidade com bancos, backups, configurações e updates existentes é obrigatória.
9. Não troque framework, banco ou layout global dentro de uma extração arquitetural.
10. Padrão arquitetural novo exige ADR antes do código.

Adapters legados podem encapsular serviços Angular/SQL durante a migração incremental —
mas só reduzindo a superfície, nunca ampliando.

## 3. Disciplina de fase

O roadmap vive em `docs/ai/ROADMAP.md`. A fase ativa vive em `docs/ai/PROJECT_STATE.md`.

> **Não implemente trabalho de fase futura.**

Se você identificar algo valioso fora da fase ativa, registre como tarefa em `TASKS.md`
com status `BACKLOG` e siga o trabalho atual. Não "aproveite que já estava ali".

## 4. Protocolo de sessão

### ON SESSION START

1. `git fetch --all --tags`
2. Ler este arquivo.
3. Ler `docs/ai/PROJECT_STATE.md` — versão atual, fase ativa, dívida conhecida.
4. Ler `TASKS.md` — pegar uma tarefa `READY` sem dono, ou continuar a sua.
5. Ler os ADRs relevantes (`docs/ADR/README.md`).
6. Ler os handoffs em `docs/handoffs/` posteriores ao último commit relevante.
7. Confirmar que nenhum outro agente é dono da tarefa nem dos arquivos afetados.

### ANTES DE MUDANÇA ARQUITETURAL

1. Procurar ADR existente que já decida o assunto.
2. Se não existir, **propor um ADR** (`status: Proposed`) e esperar aprovação humana.
3. Não introduzir padrão arquitetural novo em silêncio dentro de um PR de feature.

### AFTER WORK

1. Rodar a validação exigida pela tarefa (ver seção 6).
2. Commitar na sua branch.
3. Escrever um handoff em `docs/handoffs/`.
4. Atualizar o status da tarefa em `TASKS.md`.
5. Não marcar `DONE` se a validação de runtime estiver pendente.

## 5. Propriedade e concorrência

- Uma tarefa tem **exatamente um dono ativo**. Papéis diferentes (implementar, revisar,
  validar) podem ser de agentes diferentes; implementação simultânea não.
- Cada agente trabalha na própria branch: `codex/NH-001-...`, `claude/NH-002-...`,
  `gemini/NH-003-...`.
- Quando dois ou mais agentes trabalham ao mesmo tempo, use `git worktree`
  (ver `docs/ai/WORKFLOW.md`).
- Nunca edite arquivos listados em `Files:` de uma tarefa `IN_PROGRESS` de outro agente.
- `TASKS.md` é o único arquivo compartilhado quente. Edite só a **sua** entrada e
  commite essa mudança sozinha, para o merge ser trivial.

## 6. Definição de pronto

`Build verde não significa tarefa concluída.`

Mínimo para qualquer PR:

```bash
npm run build
npm run test:architecture
cargo test --manifest-path src-tauri/Cargo.toml
```

Conforme o que a mudança toca, some:

| Mudou | Rode também |
| --- | --- |
| schema / migration | teste de migration + `integrity_check` + `foreign_key_check` |
| versão / release | `npm run release:validate-version` |
| planning | `npm run test:planning` |
| prompts / IA | `npm run test:ai` |
| share API | `npm run share-api:test` |
| Rust | `cargo fmt --check` e `cargo clippy -- -D warnings` |
| UI / tema | validação visual real (geometria, não extração de texto) |

Mudança arquitetural exige teste de fronteira que impeça a regressão de voltar.
Mudança de schema exige teste de migration.

## 7. Formato de PR

Todo PR estrutural responde quatro perguntas no corpo:

```text
Antes:     A -> B
Depois:    A -> C -> B
Por quê:   qual risco arquitetural foi removido
Risco:     o que poderia quebrar
Validação: quais testes provaram o comportamento
```

PR estrutural é fatiado: cria a fronteira, migra uma responsabilidade, remove o caminho
antigo, adiciona o gate. Nunca um PR "refactor architecture" com 74 arquivos.

## 8. Idioma

Documentação, commits, ADRs e handoffs em **português**. Código, identificadores e
mensagens de erro técnicas em inglês, como já é a convenção do repositório.

## 9. Mapa de arquivos

| Arquivo | Papel |
| --- | --- |
| `AGENTS.md` | esta constituição — regra universal |
| `CLAUDE.md` / `GEMINI.md` / `.agents/rules/` | adaptadores por ferramenta |
| `docs/ai/PROJECT_STATE.md` | estado corrente: versão, fase ativa, dívida |
| `docs/ai/ROADMAP.md` | fases 0 a 7 até a 1.0 |
| `docs/ai/WORKFLOW.md` | branches, worktrees, handoff, papéis |
| `TASKS.md` | fila de trabalho e donos |
| `docs/ADR/` | decisões arquiteturais permanentes |
| `docs/handoffs/` | o que cada agente fez e descobriu |
| `docs/ARCHITECTURE.md` | arquitetura corrente (não histórico) |
