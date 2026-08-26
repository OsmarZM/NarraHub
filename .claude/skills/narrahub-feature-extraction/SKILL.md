---
name: narrahub-feature-extraction
description: Procedimento passo a passo para extrair um domínio de dentro do `App` (app.ts/app.html) do NarraHub para sua própria feature (component + store + gateway), sem tocar em banco ou comportamento. Use SEMPRE que o pedido for para modularizar, extrair, isolar ou "tirar do App" um domínio (ex.: entidades, configurações, colaboração, história/livro/capítulo, editor), continuar a Fase 2 do docs/ARCHITECTURE_EVOLUTION_PLAN.md, ou reduzir o tamanho do app.ts. Segue exatamente o padrão já usado em Timeline, Histórico e Biblioteca de universos — leia esses três primeiro como referência viva antes de inventar uma variação.
---

# Extração de feature (Fase 2)

Objetivo desta fase: modularizar o Angular **sem mudar persistência**. Nenhuma
migration, formato salvo ou comportamento observável pode mudar — só a
organização do código do frontend. Se a tarefa parece exigir mudar o banco,
pare: isso pertence à Fase 4 (Rust) ou a uma migration própria, não a esta.

## Referências vivas (leia antes de escrever código)

Os três exemplos já existentes são a fonte da verdade, mais confiável que
qualquer resumo aqui — a estrutura pode evoluir sutilmente entre eles:

- `src/app/features/timeline/` — o exemplo mais completo (gateway com múltiplos métodos, store com mutate genérico, page component com modal próprio).
- `src/app/features/history/` — o mais simples (só leitura, sem mutação).
- `src/app/features/library/` — hospeda um component de apresentação pura (`universe-picker`) por dentro; mostra como separar "burro" de "com estado".

## Passo a passo

### 1. Mapeie o domínio antes de mexer

No `app.ts`, ache todos os signals, campos de formulário e métodos que só
dizem respeito a esse domínio (ex.: `newEntityName`, `createEntity`,
`beginCreateEntity`...). Note também os pontos onde esse domínio **cruza**
com outro ainda não extraído (ex.: abrir uma entidade dispara recarregar
tags, que é Knowledge; salvar um capítulo atualiza estatísticas do universo).
Esses pontos de cruzamento **ficam no `App`** por enquanto — não tente
extrair dois domínios de uma vez só porque eles se tocam.

### 2. Crie a estrutura de arquivos

```text
src/app/features/<dominio>/
├── gateways/
│   ├── <dominio>.gateway.ts          (abstract class — o contrato)
│   └── legacy-<dominio>.gateway.ts   (@Injectable, chama o serviço Angular existente)
├── state/
│   └── <dominio>.store.ts            (@Injectable providedIn:'root', Signals)
├── <dominio>-page.component.ts
├── <dominio>-page.component.html
└── <dominio>-page.component.css
```

**Gateway**: métodos nomeados pelo que o domínio faz (`list`, `create`,
`rename`, `delete`...), tipos de entrada em `camelCase` mesmo que a tabela
SQL use `snake_case` (ex.: `coverImage` no input, `cover_image` no modelo
lido do banco) — o contrato do gateway não deveria vazar nome de coluna.

**Legacy adapter**: só chama o serviço Angular que já existia
(`WorkspaceService`, `EntityService`, etc.). Não reescreve a lógica SQL, só
traduz entre o contrato novo e o método antigo. É temporário por definição —
será trocado por um `TauriAdapter` na Fase 4, sem o resto do app perceber.

**Store**: Signals (`list`, `busy`, `error`), um contador `loadRevision` para
descartar respostas de `load()` que chegam atrasadas depois de trocar de
universo (veja o padrão em `timeline.store.ts`), e um `mutate()` privado que
roda a operação, recarrega a lista e trata erro de forma consistente.

**Page component**: injeta o store, expõe `@Input()`/`@Output()` para o que
ainda precisa vir de fora (ex.: `universeId`, tags carregadas por outro
domínio), e possui seus próprios modais/CSS — não reaproveite o modal
compartilhado do `app.html`, ele está sendo desmontado aos poucos.

### 3. Registre o gateway em `app.config.ts`

```ts
{ provide: <Dominio>Gateway, useExisting: Legacy<Dominio>Gateway }
```

### 4. Enxugue o `App`

Remova do `app.ts`/`app.html` tudo que migrou para a feature. O que **fica**
no `App`:

- Orquestração entre domínios (ex.: abrir um universo ainda dispara carregar
  histórias/capítulos/entidades, porque esses domínios não foram extraídos ainda).
- Reação a `@Output()` da feature nova para esse tipo de orquestração
  (ex.: `(deleted)="onXDeleted($event)"` fazendo limpeza de `localStorage` ou navegação).
- Um alias de leitura quando fizer sentido, no mesmo padrão de
  `readonly timeline = this.timelineStore.events;` — evita reescrever toda
  leitura no template, mas as **escritas** (criar/editar/excluir) devem ir
  pelo store, não por `.set()`/`.update()` direto no signal do `App`.

Se o serviço Angular legado (ex.: `EntityService`) não é mais injetado
diretamente em nenhum lugar do `app.ts`, remova o import — é o sinal mais
simples de que a extração está completa.

### 5. Preserve o comportamento de `ng serve` sem Tauri

Qualquer `ngOnInit`/efeito que chama o gateway pela primeira vez precisa
verificar `isTauri()` antes (importado de `@tauri-apps/api/core`), do
contrário `ng serve` (usado para iteração rápida de UI, sem Tauri) mostra um
erro de "banco indisponível" em vez do estado vazio esperado. Veja
`library-page.component.ts` para o padrão exato.

### 6. Atualize o teste de fronteira

Em `tests/frontend-boundaries.test.mjs`:
- adicione os três arquivos novos (`*.gateway.ts`, `*.store.ts`,
  `*-page.component.ts`) à lista que não pode conter `DatabaseService`,
  `WorkspaceService`, SQL cru, nem o serviço legado específico do domínio;
- confirme que o `legacy-<dominio>.gateway.ts` referencia o serviço legado
  (prova de que ele ainda existe, só que isolado);
- adicione uma asserção de que `app.ts` não referencia mais o serviço legado
  diretamente, se ele deixou de ser usado lá.

### 7. Valide

Rode [[narrahub-validate]] antes de considerar a fatia pronta.

### 8. Documente a fatia

Adicione um parágrafo objetivo (o que foi feito, o que ficou pendente) na
seção "Estado de implementação" da Fase 2 em
`docs/ARCHITECTURE_EVOLUTION_PLAN.md` — no mesmo formato factual das fatias
anteriores (Planejamento, Timeline/Histórico, Biblioteca). Não marque a fase
inteira como concluída; liste o que ainda falta na "Ordem" do plano.

## Regras que não têm exceção nesta fase

- Não altere nenhuma migration nem o formato salvo no banco.
- Não escreva no mesmo dado por dois caminhos (gateway novo + serviço antigo
  chamado direto em outro lugar) — se algo mais no app ainda chama o serviço
  legado direto, é sinal de que a extração está incompleta, não de que dá pra
  ter os dois convivendo por muito tempo.
- Não comece a migrar esse domínio para Rust nesta tarefa — isso é
  [[narrahub-database-safety]] e a Fase 4, feitas depois que **todos** os
  domínios estiverem atrás de gateway.
- Um domínio por vez. Cruzamentos com domínios não extraídos ficam no `App`.
