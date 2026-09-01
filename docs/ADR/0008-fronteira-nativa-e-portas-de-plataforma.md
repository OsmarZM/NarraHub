# ADR 0008 — Fronteira nativa: domínio e plataforma são portas diferentes

```text
Status:   Accepted
Data:     2026-09-01
Fase:     3 — Consolidar Rust Core
Proposto por: revisão arquitetural do autor, confirmada no código
```

## Contexto

`AGENTS.md` e `ARCHITECTURE.md` afirmavam:

```text
Feature Store → Gateway tipado → RustCoreService → invoke()
```

com a regra "nenhum `invoke()` fora de gateway autorizado", tratada na prática como
"`RustCoreService` é a única porta Tauri".

**O código nunca foi assim.** Sete arquivos falam com o Tauri:

| Arquivo | O que aciona |
| --- | --- |
| `rust-core.service.ts` | comandos de domínio |
| `sync.service.ts` | sincronização entre dispositivos |
| `online-share.service.ts` | sessão de compartilhamento |
| `ai.service.ts` | runtime de IA local |
| `backup.service.ts` | backup e restauração |
| `update.service.ts` | updater assinado |
| `production-replica.service.ts` | réplica de produção |

E `getCurrentWindow()` estava direto em quatro arquivos — serviço de tema, os dois layouts e
a página de escrita.

Isso não é o código estando errado. É a documentação forçando **duas coisas diferentes**
dentro da mesma abstração:

```text
persistência de domínio      o que o escritor cria e o app guarda
capacidades da plataforma    o que o sistema operacional oferece
```

Uma regra que o código contradiz não protege nada — ela só ensina a ignorar a documentação.

## Opções consideradas

1. **Fazer tudo passar por `RustCoreService`.** Transformaria a porta de domínio num
   despachante genérico de sistema: minimizar janela viraria comando de domínio. Rejeitada —
   agrava exatamente a confusão que causou o problema.
2. **Aceitar o espalhamento e remover a regra.** Honesto sobre o presente, mas deixa o
   projeto sem nenhuma fronteira: `invoke()` volta para dentro de componente na primeira
   pressa.
3. **Separar as duas portas e tornar a fronteira verificável.**

## Decisão

Adotar a opção 3.

```text
                        Angular
                           │
              ┌────────────┴────────────┐
        Domínio do produto        Plataforma nativa
              │                          │
        Feature Store              core/native/*
              │                    sync · share · IA
        Domain Gateway             backup · update
              │                    janela · réplica
        RustCoreService                  │
              └───── fronteira Tauri ────┘
                           │
                          Rust
```

A regra passa a ser:

> **Componentes, stores, layouts e serviços de aplicação nunca falam com o Tauri. Só as
> portas falam.**

Isso é mais forte que "só o `RustCoreService` pode chamar `invoke()`", porque corresponde ao
produto real — e por isso pode ser cobrado por teste.

### Duas coisas que a regra deliberadamente **não** alcança

**`isTauri()` é livre.** Detecção de ambiente não é capacidade: um componente precisa saber
se está no desktop para decidir o que mostrar, e empurrar essa pergunta para dentro de uma
porta seria cerimônia.

**`DatabaseService` continua onde está.** Ele abre e fecha o pool do `tauri-plugin-sql` e não
executa SQL — é ciclo de vida de conexão, com teste próprio garantindo que continue assim.

## Consequências

**Passa a ser possível** acrescentar capacidade de plataforma sem tocar na porta de domínio,
e vice-versa. Uma porta nova é um arquivo em `core/native/` mais uma linha na lista do gate —
com a justificativa de por que é plataforma e não domínio.

**Passa a ser proibido** importar `@tauri-apps/api/window`, qualquer `@tauri-apps/plugin-*`
ou chamar `invoke()` fora das portas.

**Precisou ser migrado:** os seis serviços de plataforma saíram de `core/services/` para
`core/native/`, e `getCurrentWindow()` virou `NativeWindowService`.

**Gate:** `só as portas nativas falam com o Tauri`, em `tests/rust-core-contract.test.mjs`.
Verificado por mutação: `invoke()` num layout, janela numa página e plugin num serviço de
aplicação reprovam.

## O teste que este ADR substitui

Existia um teste chamado `o core Rust não é chamado por fora do adaptador de gateway`. Ele
procurava a string `RustCoreService` dentro de `features/` — e **nunca procurava `invoke()`**.
Uma chamada direta a `invoke('sync_start')` num componente passava sem ser vista.

O nome prometia uma proteção maior que a asserção entregava. É o mesmo padrão encontrado no
gate da Fase 2, onde um `\b` corrompido tornava cinco asserções incapazes de falhar.

Vale como regra geral: **o nome de um teste é uma promessa; a asserção é o contrato.** Quando
os dois divergem, o time confia no nome — e é aí que a proteção deixa de existir sem ninguém
perceber.

## Revisitar quando

Se uma capacidade precisar atravessar as duas metades — por exemplo, o Sync V2 escrevendo
diretamente no banco por um caminho que não é o `RustCoreService`. Aí a fronteira precisa de
um desenho explícito, não de uma exceção na lista.
