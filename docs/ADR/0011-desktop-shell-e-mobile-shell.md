# ADR 0011 — DesktopShell e MobileShell: mesmo domínio, composições visuais diferentes

```text
Status:   Accepted
Data:     2026-09-14
Fase:     produto (NH-080, experiência mobile)
Proposto por: humano
```

## Contexto

O APK Android rodava o shell do desktop dentro do WebView: barra de título com controles de janela,
sidebar do universo, cabeçalho com cinco a sete botões em linha, painéis lado a lado e modais
centrais. Media queries escondiam pedaços e encolhiam o resto. No aparelho (S23, Android 16) o
resultado foi "um site desktop espremido": texto de 8–10px, alvos de 22px, ações só com hover, o
resumo cobrindo o editor, conteúdo atrás da barra de status e zoom por pinça.

Uma correção de emergência (`android-shell.css`, commit `d4e9271`) sobrescrevia seletores internos
de outras features por especificidade e `!important`. Funcionava, mas qualquer mudança numa feature
podia quebrá-la sem aviso.

## Opções consideradas

1. **Continuar com o shell único e media queries** — sem arquitetura nova, mas cada tela vira uma
   pilha de exceções por largura, e o desktop fica exposto a regressões de regras mobile.
2. **App mobile separado (outro projeto, outras rotas)** — liberdade total de UI, ao custo de
   duplicar stores, serviços, navegação e contratos com o Rust.
3. **Dois shells de apresentação sobre a mesma aplicação** — o `RootLayout` escolhe
   `app-shell` (desktop) ou `app-mobile-shell`; Router, stores, serviços, navegação e ações são os
   mesmos; cada tela decide a própria apresentação mobile.

## Decisão

DesktopShell e MobileShell compartilham domínio e navegação, mas não precisam compartilhar a mesma
composição visual.

## Por quê

A opção 1 foi o que produziu o problema. A opção 2 dobraria a superfície de manutenção e criaria
duas verdades para regras que hoje moram num lugar só (salvar antes de navegar, guards de rota,
validação de universo). A opção 3 separa exatamente o que é diferente — a composição — e mantém
junto o que é igual.

## Consequências

- `RootLayout` tem um só `<router-outlet>`, declarado num `ng-template` e colocado no shell ativo.
- Ações secundárias viram dados (`ShellActionsState`) com os mesmos handlers dos botões do
  desktop; o celular as mostra no "•••". Nenhuma lógica de negócio é duplicada.
- Conteúdo com duas apresentações (árvore de capítulos, resumo, lista de ligações) é declarado uma
  vez em `ng-template` e colocado no painel do desktop ou numa `app-mobile-sheet`.
- Diálogos seguem o contrato `data-nh-dialog` / `data-nh-dialog-panel`; uma regra única os torna
  folhas de baixo no celular.
- Regra mobile vive no componente (`:host-context(html.nh-mobile)`, ou o seletor do elemento em
  componentes `ViewEncapsulation.None`). CSS global que sobrescreve outras features é proibido.
- Critério do shell mobile: toque + lado menor ≤ 760px (`shell/state/breakpoints.ts`).
- Gates: `tests/mobile-shell.test.mjs` (estrutura, contratos, desempenho do gesto) e
  `tests/e2e/mobile-shell.spec.mjs` (Playwright: 4 celulares e desktop).

## Revisitar quando

- Um tablet Android passar a ser alvo de produto (hoje recebe o shell do desktop).
- O desktop também precisar de ações como dados (então `ShellActionsState` serve os dois).
