# NarraHub no celular

> Arquitetura, regras e testes da experiência mobile. A decisão está no **ADR 0011**; o
> levantamento que motivou o trabalho, em `AUDITORIA.md`; o teste no aparelho, em
> `ROTEIRO_ANDROID.md`.

## Arquitetura

```text
RootLayout
 ├── <ng-template #appContent>  avisos, atualização e o ÚNICO <router-outlet>
 ├── app-shell (desktop)        titlebar, sidebar, cabeçalhos de área
 └── app-mobile-shell           barra de cima, conteúdo, navegação gestual, folha de ações
```

Os dois shells usam o mesmo Router, os mesmos stores, serviços, guards e handlers. Só a composição
muda. Quem escolhe é o `ViewportState`.

| peça | onde | papel |
| --- | --- | --- |
| `ViewportState` | `shell/state/viewport.state.ts` | decide o shell; põe `nh-mobile` no `<html>` |
| `breakpoints.ts` | `shell/state/` | escala oficial e critério do mobile |
| `app-mobile-shell` | `shell/mobile-shell/` | composição mobile e céu leve |
| `app-mobile-topbar` | `shell/mobile-topbar/` | `‹ universo  🔍  •••` |
| `app-mobile-sheet` | `shell/mobile-sheet/` | folha de baixo reutilizável |
| `ShellActionsState` | `shell/state/shell-actions.state.ts` | ações da tela como dados, camadas `page` e `area` |
| `app-mobile-navigation` | `shell/mobile-navigation/` | alça e roda de cartões |
| `mobile.css` | `src/styles/` | base de página inteira, diálogos e perfil de desempenho |

### Como uma tela ganha apresentação mobile

1. **Ações secundárias** → publique em `ShellActionsState` (mesmo handler do botão do desktop).
   Exemplos: `WorkspaceLayout` (camada `area`) e `ConnectionsPage` (camada `page`).
2. **Painel lateral ou lista auxiliar** → declare o conteúdo num `ng-template` e coloque-o no painel
   do desktop ou numa `app-mobile-sheet`. Exemplos: árvore e resumo da escrita.
3. **Diálogo** → `data-nh-dialog` no fundo e `data-nh-dialog-panel` no painel. Nada mais.
4. **Estilo** → no CSS do componente, sob `:host-context(html.nh-mobile)`; em componentes
   `ViewEncapsulation.None`, sob `html.nh-mobile app-nome-do-componente`.
5. **Ferramenta que só faz sentido no desktop** (modo foco, tela cheia) → `@if (!viewport.isMobile())`.

Não crie CSS global que alcance classes internas de outra feature.

## Breakpoints

| faixa | regra |
| --- | --- |
| **mobile** | toque (`pointer: coarse`) **e** lado menor da tela ≤ 760px |
| tablet | largura ≤ 1024px |
| desktop | largura > 1024px |

O lado menor, e não a largura: celular deitado continua celular. O toque, e não o user agent:
janela estreita de desktop com mouse continua desktop.

O legado tem 14 breakpoints (520 a 1450). Não foi migrado de uma vez; todo CSS novo segue esta
escala.

## Viewport, zoom e altura

**Zoom fechado em três camadas** — nenhuma sozinha basta:

| camada | onde | o que faz |
| --- | --- | --- |
| meta viewport | `src/index.html` | `viewport-fit=cover, interactive-widget=resizes-content` |
| WebView | `MainActivity.onWebViewCreate` | `setSupportZoom(false)`, sem controles de zoom |
| `touch-action` | `html.nh-mobile { touch-action: pan-x pan-y }` | pinça e duplo toque não escalam; rolar continua |

Sem `user-scalable=no`. A alça (`touch-action: none`) e o editor continuam recebendo o toque.

**Altura: cadeia de `100%`, não `dvh`.** `100vh` no Android inclui as barras e não encolhe com o
teclado. `100dvh` acompanha, mas dentro de uma variável CSS o Chromium não recalcula quando a altura
muda: medido, o shell ficou com 844px numa tela já com 480px. `100%` é refeito a cada resize.

## Safe area e teclado

A fonte da verdade são os **insets do Android**. O `MainActivity` aplica barra de status, barra de
gestos, recorte da câmera e teclado como padding do contêiner da WebView, e pinta a faixa com a cor
do fundo. Dentro do APK nada fica atrás das barras e a área do app encolhe quando o teclado abre.

O CSS usa `--nh-safe-top/right/bottom/left`, definidas como `env(safe-area-inset-*, 0px)`: no APK
valem 0, e num navegador móvel comum trazem os valores reais. Componente mobile usa as variáveis,
nunca `env()` direto.

## Gestos

| gesto | onde | efeito |
| --- | --- | --- |
| tocar na alça | borda direita, 60% da altura | abre |
| arrastar da alça para a esquerda | idem | abre acompanhando o dedo |
| peteleco curto e rápido | idem | abre (velocidade > 0,55 px/ms) |
| arrastar na vertical | navegador aberto | gira a roda de cartões; assenta num cartão |
| empurrar para a direita | navegador aberto | fecha |
| tocar num cartão | navegador aberto | traz para a frente ou navega |

A alça tem três barras douradas (visual de ~10px, alvo de 48×132px). O `MainActivity` desliga o
"voltar" do Android numa faixa de 48dp com o **mesmo centro de 60%** — só ali; o voltar funciona
nas duas bordas fora dela. A dica "← Puxe para navegar" aparece uma vez por versão
(`narrahub.mobileNavigationHintSeen`). Vibração leve ao tocar, abrir e a cada cartão da roda.

## Desempenho

- Durante o gesto, só `transform`, `opacity`, `z-index` e `visibility` são escritos por quadro; o
  `pointermove` roda fora do Angular e não lê layout.
- `will-change` só enquanto o navegador está aberto.
- Sem `backdrop-filter` no conteúdo e nos diálogos do shell mobile.
- Céu do celular: arte estática + uma camada de estrelas que só anima opacidade (o desktop tem três).
- Entrada de tela sem `transform` (criaria bloco de contenção para as folhas).

## Testes

```bash
npm run test:architecture
```

`tests/mobile-shell.test.mjs`: critério do mobile, zoom, altura, um só outlet, paridade de ações,
cobertura de destinos, desempenho do gesto, alça × faixa do Android, contrato de diálogos, telas
especiais, ações sem hover.

```bash
npm run test:mobile-e2e
```

`tests/e2e/mobile-shell.spec.mjs` (Playwright, Chromium): 375×667, 390×844, 412×915, 430×932 com
toque, e desktop 1366×768. Shell, toque/arrasto/peteleco/roda/fechar, dica, "•••", escrita e folhas,
diálogo com teclado simulado, nenhuma rolagem horizontal, desktop intacto.

O Chromium não emula teclado real, pinça, gesto voltar do sistema nem os insets do WebView: esses
itens estão em `ROTEIRO_ANDROID.md`.
