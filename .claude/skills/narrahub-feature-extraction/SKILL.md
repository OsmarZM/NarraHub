---
name: narrahub-feature-extraction
description: O padrão de feature do NarraHub — gateway → store → página roteada. Use SEMPRE que for criar uma feature nova, mover código para dentro de uma feature existente, ou revisar um diff que atravessa a fronteira entre feature e persistência. A extração do antigo `App` monolítico TERMINOU (Fases 2, 2.1 e 3); esta skill agora descreve o padrão vigente e as armadilhas que já custaram bugs reais, não um roteiro de migração.
---

# Padrão de feature (gateway → store → página roteada)

**A extração acabou.** Não existe mais `app.html` nem `app.css`, e `app.ts` é
só `<router-outlet />`. Todo domínio já vive em `src/app/features/<domínio>/`
e toda seção do workspace é uma rota lazy. Se um pedido falar em "tirar do
App", "reduzir o app.ts" ou "extrair do monólito", o pressuposto está velho —
confirme o que a pessoa quer de fato antes de mexer.

O que esta skill cobre agora: **criar feature nova** e **manter as existentes
dentro da fronteira**.

## Referências vivas (leia antes de escrever código)

Mais confiáveis que qualquer resumo aqui:

- `features/history/` — o mais simples: só leitura, gateway com um método.
- `features/timeline/` — página roteada que injeta stores de outros domínios em vez de receber `@Input`.
- `features/connections/` — gateway cobrindo dois serviços legados (relações + canvas) e um componente de apresentação puro por dentro (`connections-graph`).
- `features/settings/` — domínio **sem** gateway. Ver "Gateway é opcional".
- `application/workspace-sync.service.ts` — onde mora coordenação entre domínios.

## As três camadas

```text
src/app/features/<domínio>/
├── gateways/
│   ├── <domínio>.gateway.ts          (abstract class — o contrato)
│   └── legacy-<domínio>.gateway.ts   (@Injectable, chama o serviço Angular existente)
├── state/
│   └── <domínio>.store.ts            (@Injectable providedIn:'root', Signals)
└── <domínio>-page.component.{ts,html,css}
```

**Gateway**: métodos nomeados pelo que o domínio faz (`list`, `create`,
`rename`). Entrada em `camelCase` mesmo que a coluna seja `snake_case` — o
contrato não vaza nome de coluna. O adapter legado só traduz; não reescreve
SQL. Ele é temporário por definição: some quando a Fase 4 trocar por Rust.

**Store**: Signals (`items`, `busy`, `error`), um `loadRevision` para descartar
resposta atrasada depois de trocar de universo, e o `universeId` guardado
internamente — métodos de mutação **não** recebem `universeId` por parâmetro.

**Página**: injeta o store e lê o que precisa de outros domínios pelos stores
deles. Modais são próprios.

### Registre o gateway em `app.config.ts`

```ts
{ provide: <Dominio>Gateway, useExisting: Legacy<Dominio>Gateway }
```

Esquecer isso compila normalmente: o erro só aparece em runtime
(`NG0201`), e só quando algo que injeta o gateway é instanciado. Já aconteceu
na fatia de Colaboração. Por isso [[narrahub-validate]] não é opcional mesmo
com build verde.

## Gateway é opcional

Nem todo domínio precisa. O gateway abstrai a fronteira SQL-vs-Rust — se o
domínio já fala só com comandos Tauri nativos (`BackupService`, `SyncService`,
`UpdateService`), não há fronteira para abstrair e o store injeta os serviços
direto, como `SettingsStore` faz. A única exceção sancionada a "feature não
conhece `DatabaseService`" é ciclo de vida do pool (fechar/reabrir na
restauração de backup) — documente e teste a exceção.

## Armadilhas que já custaram bug real

**1. `@Input()` numa página roteada.** `withComponentInputBinding()`
**sobrescreve com `undefined`** todo input sem correspondente em params/data
da rota — o inicializador da classe não protege. Quebrou o template de
Conexões com `reading 'length' of undefined`. Página roteada declara **apenas**
`@Input() universeId`; o resto vem de store, por getter. Há teste de fronteira.

**2. `@Output()` numa página roteada.** Ninguém escuta — o pai é o Router. Se
a página precisa avisar alguém: mensagem vai para `ShellState`
(`showInfo`/`showError`), navegação vai para o `Router`, efeito em outro
domínio vai para `WorkspaceSyncService`.

**3. `@ViewChild` de página, a partir do layout.** Não alcança o que vem pelo
outlet. O cabeçalho persistente pega a instância ativa pelo `(activate)` do
`<router-outlet>` e chama `openCreate()` (contrato `supportsCreate`).

**4. Coordenação entre domínios dentro de um componente.** Salvar capítulo
reindexa menções e atualiza estatísticas; excluir entidade recarrega conexões.
Isso vive em `application/workspace-sync.service.ts`, nunca num layout ou
página — desmontar um monólito não pode empurrar a coordenação para outro.

**5. Duas fontes de verdade para "qual tela mostrar".** `AppState` não
representa mais a rota. Se precisar da seção ativa, leia
`AppNavigationService.activeData()`, derivado de `route.data`. Manter cópia
disso em signal já causou tela em branco quando os dois discordaram.

## Atualize o teste de fronteira

Em `tests/frontend-boundaries.test.mjs`:

- adicione os arquivos novos à lista que não pode conter `DatabaseService`,
  SQL cru, nem serviço legado do domínio;
- confirme que o `legacy-<domínio>.gateway.ts` referencia o serviço legado;
- se a página é roteada, ela cai automaticamente no teste que exige só
  `@Input() universeId`.

## Regras sem exceção

- Não altere migration publicada nem formato salvo — isso é
  [[narrahub-database-safety]].
- Não escreva no mesmo dado por dois caminhos (gateway novo + serviço antigo
  chamado direto em outro lugar).
- Valide com [[narrahub-validate]] antes de dar por pronto, e registre a fatia
  em `docs/ARCHITECTURE_EVOLUTION_PLAN.md`.
