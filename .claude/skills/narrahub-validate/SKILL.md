---
name: narrahub-validate
description: Roda a checagem local rápida do NarraHub (build Angular, testes de fronteira/arquitetura, testes Rust) e reporta um veredito curto de pass/fail por etapa. Use SEMPRE antes de dar uma mudança como pronta, antes de um commit, ou quando o usuário perguntar algo como "isso tá seguro?", "valida essas mudanças", "roda os testes", "posso commitar?". Isto é uma checagem LOCAL — não confirma que o app funciona empacotado no Tauri nem que um upgrade real funciona; deixe isso explícito no veredito em vez de implicar "release pronta".
---

# Validação local do NarraHub

Esta skill roda o que já existe hoje no `package.json` — não invente comando
novo (`cargo clippy`, `cargo fmt --check`, etc. não estão configurados neste
repo; se quiser rodá-los como extra, avise que é manual e fora do fluxo
padrão, não trate como um gate que já existe).

## Ordem de execução

Rode cada etapa e capture pass/fail antes de seguir para a próxima — não
pare no primeiro erro sem reportar, mas também não continue tentando
"consertar tudo de uma vez" sem mostrar o estado real primeiro.

1. **Build Angular** — `npm run build`. Falhas de tipo, template ou imports
   aparecem aqui. Os dois avisos de orçamento de bundle (`bundle initial` e
   `app.css`) já existiam antes de qualquer mudança de feature — não são
   regressão a menos que o valor tenha crescido bastante.
2. **Fronteira de arquitetura** — `npm run test:architecture` (rotas,
   `frontend-boundaries.test.mjs`, erro tipado do command Tauri). Se a
   mudança tocou uma feature extraída (veja [[narrahub-feature-extraction]]),
   confirme que os arquivos novos foram incluídos nesse teste.
3. **Testes específicos do que foi tocado** — rode o `npm run test:<área>`
   relevante se existir (`test:planning`, `test:ai`, `test:navigation`).
   Não rode a suíte inteira de testes Node só por rodar; rode o que cobre a
   área mudada.
4. **Rust** — se algo em `src-tauri/` mudou, `cargo test --manifest-path
   src-tauri/Cargo.toml`. Se só o frontend mudou, `npm run check` (que já
   encadeia `npm run build` + `cargo check`) é suficiente como sinal rápido
   de que nada do lado Rust quebrou por causa de um tipo compartilhado.
5. **Diff limpo** — `git diff --check` (espaço em branco/whitespace) antes
   de sugerir commit.
6. **UI observável** — se a mudança é visível (layout, componente, texto),
   siga o fluxo de verificação padrão: abra o preview (`ng serve` via
   `.claude/launch.json`, se existir, ou o skill `run`), confira console e
   texto renderizado. Lembre que `ng serve` **não tem banco local** — qualquer
   coisa que dependa de `isTauri()` mostrando dados reais só é validável de
   verdade no app Tauri empacotado; diga isso explicitamente em vez de
   deixar implícito que a UI "passou" por completo.

## Formato do veredito

Reporte em uma tabela curta, não em prosa longa:

```text
NarraHub — validação local

Build Angular             ✅
test:architecture         ✅
test:<área tocada>        ✅ / — (não se aplica)
Rust (test ou check)      ✅ / — (não se aplica)
git diff --check          ✅
Preview (ng serve)        ✅ sem erro de console — dados reais não testados (sem Tauri)

Pendências fora desta checagem: runtime Tauri empacotado, upgrade a partir
da versão publicada anterior, backup/restore real. Isso é gate de release,
não desta validação local.
```

Se algo falhar, mostre o erro real (não resuma como "deu problema") e pare
de avançar para as etapas seguintes até decidir com o usuário se corrige
agora ou registra como pendência.
