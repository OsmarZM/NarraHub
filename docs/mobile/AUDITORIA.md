# Auditoria da experiência mobile — antes da reestruturação

> Levantamento feito em 2026-09-14, na branch `mobile-shell` (a partir de `mobile-navegacao-gestual`,
> commit `d4e9271`), antes de qualquer alteração. Números medidos com busca no código, não estimados.

## Onde estamos

O APK roda, mas o celular recebe o **shell do desktop**. O que existe de mobile é:

- `ViewportState` + `app-mobile-navigation` (alça, pilha de cartões, roda vertical);
- `app-mobile-topbar` e `src/app/shell/android/android-shell.css` (commit `d4e9271`).

O `android-shell.css` foi uma correção de emergência depois do teste no S23. **Ele é exatamente o
tipo de solução que este trabalho deve substituir:** uma camada de 300+ linhas que sobrescreve
seletores de outras features por especificidade (`html.nh-android .writing-layout …`), com
`!important`, e que depende de nomes de classes internos de cada tela. Funciona, mas qualquer
mudança numa feature pode quebrá-la em silêncio.

## Premissas do plano que não batem com o código

| premissa | realidade | consequência |
| --- | --- | --- |
| breakpoint mobile ≈ `max-width: 760px` | desde `d4e9271` é Android + `max-width: 900px` | decisão do humano: **toque + lado menor ≤ 760px** |
| o toque na alça é ignorado | corrigido em `d4e9271` (`pulling` sem arrasto abre); não verificado no aparelho | manter e cobrir com teste |
| "o projeto usa `enableEdgeToEdge()`, então o web deve respeitar `env(safe-area-*)`" | desde `d4e9271` o `MainActivity` aplica os insets do sistema e do teclado como **padding nativo**: a WebView não desenha atrás das barras e `env()` vale 0 | decisão do humano: **insets nativos como fonte, expostos ao CSS como variáveis**, `env()` de reserva |
| `npm test` roda os testes | `npm test` roda `cargo test`; os testes do frontend são scripts `test:*` em `node:test`, todos estáticos | decisão do humano: **Playwright + job na CI** para os testes em runtime |
| testes automatizados de teclado, zoom e safe area | o Chromium headless emula tamanho e toque, mas não teclado Android, pinça real, gesto voltar nem insets do WebView | esses itens vão para o **roteiro físico** |
| todos os destinos da sidebar precisam estar no navegador gestual | a sidebar e o navegador gestual leem a mesma lista (`AppNavigationService.navigationItems`) | a cobertura já é total por construção; vira teste para continuar assim |

## Achados

### 1. Viewport, zoom e altura

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| `src/index.html` | pinça e duplo toque dão zoom na interface | meta viewport só com `width=device-width, initial-scale=1`; WebView com zoom habilitado por padrão | meta com `viewport-fit=cover, interactive-widget=resizes-content`; no `MainActivity`, `setSupportZoom(false)` e sem controles de zoom; `touch-action: manipulation` no shell mobile (duplo toque) | baixo; o editor não usa zoom |
| 14 arquivos, 55 ocorrências de `vh`/`vw` | `100vh` no Android inclui barras e não encolhe com teclado | `app-shell` (`height: 100vh`, `calc(100vh - …)`), editor (`min-height: calc(100vh - 300px)`), modais (`max-height: 90vh`) | tokens `--nh-viewport-height` (`100dvh`) no shell mobile; desktop mantém o que tem | médio: telas herdadas podem depender de `vh`; migrar só no caminho mobile |
| `MainActivity.kt` | teclado cobre campos | edge-to-edge sem tratar IME | já tratado em `d4e9271` (padding com `ime()`); falta expor os insets ao CSS | baixo |

### 2. Shell

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| `root-layout.component.*` | um só shell com `@if` para a barra de cima | não há composição separada | `RootLayout` decide entre `app-desktop-shell` e `app-mobile-shell`; os dois recebem o mesmo `<router-outlet>` e os mesmos serviços | médio: o outlet não pode ser duplicado — um só outlet, projetado |
| `root-layout.component.css` (284 linhas, `ViewEncapsulation.None`) | CSS global de várias features, com 7 breakpoints próprios | legado da época do app monolítico | não migrar tudo agora; o shell mobile não depende dele | — |
| `workspace-layout.component.html` | cabeçalho com trilha + 5–7 botões em linha | layout de desktop | o cabeçalho vira apresentação trocável: desktop mantém a linha; mobile publica **ações** para a barra do shell mobile (`•••` → folha) | médio: as ações precisam virar dados (label + handler), sem duplicar lógica |
| `app-universe-sidebar` | escondida por CSS no celular | regra `display: none` | no mobile ela nem é renderizada | baixo |

### 3. Navegação gestual

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| `mobile-navigation.component.css` | alça quase invisível | cápsula de 3–4px com opacidade baixa | três barras verticais (6–14px visuais), área de toque ≥ 44px, estado de toque que cresce e brilha | baixo |
| `mobile-navigation.component.ts` | só abre por arrasto longo (toque corrigido em `d4e9271`) | limiar de abertura por distância | abrir também por peteleco curto (velocidade) | baixo; já existe `settleOpen` com velocidade |
| — | ninguém descobre a navegação | não há dica | dica de primeira execução versionada (`narrahub.mobileNavigationHintSeen`) | baixo |
| pilha de cartões | profundidade só por `scale`/`translate` | — | `rotateX` + `translate3d` para o efeito de roda; continua só `transform`/`opacity` por quadro | médio: perspectiva custa GPU; medir |
| `MainActivity.kt` | faixa sem "voltar" deve coincidir com a alça | já calculada com os insets (`d4e9271`) | manter; ajustar à nova altura da alça | baixo |

### 4. Toque e hover

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| 266 declarações de `width`/`height` entre 20 e 43px | alvos pequenos para o dedo | desenho de desktop | no mobile, alvos ≥ 44px pelos componentes do shell e das folhas; telas herdadas por token de alvo | médio |
| `writing-page`, `entities-page`, `universe-picker`, `root-layout` (14 regras `opacity: 0` reveladas por `:hover`) | ações (excluir, arrastar, menu do cartão) invisíveis sem mouse | `:hover` como único caminho | em `(hover: none)` ou mobile: ação visível ou `•••` | baixo |
| 398 `font-size` abaixo de 12px (110 só em `root-layout.component.css`) | texto minúsculo | escala tipográfica do desktop | tokens de tipografia mobile; **não** `zoom`/`scale` | alto se feito de uma vez; por tela, a começar pelas do plano |

### 5. Modais

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| 10 templates com `modal-backdrop` próprio (`share-modal`, `connections`, `entities`, `tags-modal`, `library`, `writing-page`, `planning`, `settings`, `timeline`, `workspace-layout`) | modal central de desktop no celular | cada feature desenha o seu | no mobile, `.modal-backdrop`/`.modal` viram folha de baixo por uma regra do shell mobile, respeitando teclado; componente `app-mobile-sheet` para o que é novo | médio: 10 variações de markup |

### 6. Editor

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| `writing-editor.component.*` | barra de ferramentas quebra em várias linhas e alarga a tela | `flex-wrap` + grupos com largura mínima; ancestrais flex sem `min-width: 0` | no mobile, uma linha que rola por dentro; ancestrais com `min-width: 0` | baixo |
| `writing-page.component.*` | árvore, editor e resumo lado a lado | grid de 3 colunas | no mobile, editor na tela; árvore e resumo em folhas (hoje via `android-shell.css`, passa para a apresentação da própria página) | médio |

### 7. Telas especiais

| arquivos | problema | mudança |
| --- | --- | --- |
| `planning-board` (47 fontes < 12px) | kanban em colunas lado a lado | uma coluna por vez, deslizar entre colunas |
| `timeline-page` (`min-width: 290px`) | layout de desktop | lista vertical |
| `connections-graph` (Cytoscape) | grafo dentro de painel | canvas em tela cheia |
| `settings-page` | navegação de abas rola para o lado (intencional) | manter; aumentar alvos |

### 8. Performance

| arquivos | problema | causa | mudança | risco |
| --- | --- | --- | --- | --- |
| `app-shell.component.css` | 3 camadas de estrelas em tela cheia com `radial-gradient` e animação infinita, `will-change` permanente | fundo cósmico do desktop | no mobile: arte estática + 1 camada leve animada só com `opacity` | baixo |
| 12 arquivos, 46 `backdrop-filter` | blur caro no WebView | glassmorphism | no mobile, translucidez sem blur | baixo |
| 20 animações infinitas | trabalho contínuo de GPU | pulsos e brilhos decorativos | no mobile, desligar as decorativas | baixo |
| `.page-enter` (`transform` com fill `both`) | cria bloco de contenção para `position: fixed` | animação de entrada | no mobile, entrada só com `opacity` | baixo |

### 9. Breakpoints

14 valores distintos em `@media`: 520, 680 (18×), 700, 760, 860, 900, 920, 1050, 1080, 1100, 1180,
1200, 1260, 1450. Proposta: `mobile ≤ 760` (lado menor, com toque), `tablet ≤ 1024`,
`desktop > 1024`, documentados e exportados de um só lugar. **Não migrar o legado agora**; o que é
novo segue a escala.

## Ordem de implementação

1. viewport, zoom, safe area, `dvh`
2. critério mobile e `MobileShell`
3. barra superior e cabeçalho do universo
4. navegação gestual
5. editor
6. modais e folhas
7. telas especiais
8. performance
9. testes (Playwright) — os testes de cada etapa entram com a etapa sempre que possível
10. APK real e roteiro físico

Cada etapa: `npm run build`, testes afetados, `cargo check` quando Rust/Kotlin mudar, commit, push,
CI verde antes da próxima.
