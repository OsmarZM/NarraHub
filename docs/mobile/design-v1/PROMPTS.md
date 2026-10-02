# NarraHub — Prompts de design mobile V1

## Ferramenta e referências

Ferramenta integrada `image_gen`, uma chamada por proposta, sem API/CLI externo.
Referências: print correspondente (conteúdo) e proposta 01 (paleta e composição de cards).
A exploração com navegação inferior foi descartada; só propostas com relógio lateral compõem este conjunto.

Revisões finais: na imagem 03, remover a frase extra que afirma armazenamento exclusivo,
pois existe sincronização; na 11, explicitar que API própria envia contexto ao provedor e
simplificar a limitação de instalação gerenciada no Android. Layout das imagens preservado.

## Padrão comum

```text
Use case: ui-mockup. Create ONE high fidelity portrait Android mobile app screen for NarraHub, a local-first creative writing/worldbuilding app. Flat front-on screenshot, no physical phone bezel, no perspective, no outside annotations. Canvas portrait aspect 9:19.5, designed as 390x844dp. Portuguese text crisp and correctly spelled. Shared design system: near-black navy #0A0F1C background with very subtle sparse stars ONLY in small decorative header, solid navy #141B2D readable cards, warm ivory #EDE7D9 main text, legible muted bluegrey #ACB7C8 secondary text, restrained gold #D8B772 selection icons and lines, existing coral-to-muted-rose gradient only for primary action. Inter-like sans text 16dp minimum body; refined literary serif only for book/universe titles; 20dp horizontal gutters, consistent 12-16dp corner radii, 48dp touch targets, subtle dividers and generous but practical spacing. Android status bar top and safe system gesture bar bottom. Navigation MUST preserve user's existing RIGHT SIDE CLOCK/WHEEL MENU. Absolutely NO bottom tab bar, NO bottom application navigation icons or labels. Android system gesture pill is allowed. Show only a narrow discreet gold right edge clock-menu handle around 58% of screen height, positioned in a reserved outer gutter OUTSIDE the content, never covering buttons, text, input fields or keyboard. Inside header a small labeled compass/clock menu icon may provide explicit accessible opening of the same wheel. Content width leaves handle space. No overlaid FAB, no clipped horizontal page content, no crowded toolbar, no oversized cosmic planets behind text. All primary controls should have contrast; use dark navy text on coral/rose primary buttons. Preserve screenshot's real functional scope; references are current broken UI content, redesign composition for mobile. This is a proposed design with placeholder narrative content, not claim real saved user data. Never add AI generation, devices discovery, social feeds, achievements, online login or unsupported functions. Refined calm premium writing app, practical not ornamental.
```

## 01 — Biblioteca

```text
Screen01: Biblioteca / Início. Compact gold brand NARRAHUB and search icon. Main title "Seus universos" modest24dp serif, small "4 universos", concise subline "Um lugar para suas histórias." Compact continuing card "CONTINUAR ESCREVENDO", universe "teste 6.1", chapter "Aquele que carrega o destino", primary "Continuar". Section "Biblioteca" plus small "Recentes" filter. Three COMPACT readable universe cards small illustrative cover thumbnails: "teste 6.1", "teste", "Crônicas do Vale", metadata books/chapters and trailing overflow button. Fullwidth "＋ Criar universo" above safe Android gesture pill. No bottom nav anywhere. Maintain right edge clock-menu handle in gutter away from content. Brand and universe titles elegant, body readable. Don't overfill viewport.
```

## 02 — navegacao

Referência: `WhatsApp Image 2026-09-30 at 09.24.30 (1).jpeg`.

```text
Screen02: OPEN existing NarraHub SIDE CLOCK / WHEEL MENU, the user explicitly prefers this mechanism. No bottom navigation. Background subtly dimmed solid dark navy with small NARRAHUB logo and "teste 6.1" universe context. At right edge the wheel pivot has a gold small clock ring, right-hand thumb reachable. Main mobile clockwise navigation selector is a curved vertical wheel of FIVE stacked compact readable pill cards with clear moderate scale changes, not a grid, not a straight list, no tiny unreadable extremes. CENTER selected card "Personagens" gold outlined, larger at 60dp height, bright label and person icon. Above it "História" and "Universos"; below it "Relações" and "Linha do tempo". A thin gold arc to the right joins alignment like a clock segment. Below clock a short visible "Mais opções" allows accessing Planejamento, Histórico, Configurações through same wheel. Top right small explicit "Fechar" X with 48dp area; bottom quiet readable hint "Gire para escolher · Toque para abrir", with no footer tabs. Preserve wheel conceptual layout from reference screenshot; improve label contrast, remove huge dark fades and huge card gaps. User can open and close explicit controls as well as gesture. No navigation tab bar.
```

## 03 — criar-universo

Referência: `WhatsApp Image 2026-09-30 at 09.24.30 (2).jpeg`.

```text
Screen 03: Create universe form as full-height focused mobile sheet, no No bottom navigation. Top back arrow, "Criar universo", discreet X. Optional cover is compact HORIZONTAL row with small cover preview icon plus "Adicionar capa" and "Opcional", not a giant dropzone. Below field label "Nome do universo" and empty text field placeholder "Ex.: As Crônicas do Vale"; field label "Descrição" with small "Opcional", modest 3-line text area placeholder "Uma ideia para começar…". Brief small note with lock "Seu universo fica neste aparelho." Generous empty breathing space below form. Fixed safe action area above gesture indicator: primary full-width "Criar universo", secondary text "Cancelar". No underlying page bleeding through form, no duplicate buttons and no false filled data.
```

## 04 — escrita

Referência: `WhatsApp Image 2026-09-30 at 09.24.30 (3).jpeg`.

```text
Screen 04: Writing editor during actual mobile editing with Android dark keyboard OPEN occupying lower 32% screen. No No bottom navigation. Compact top row back "teste 6.1" with overflow icon, second context "Capítulo 1" and touchable "Capítulos" icon button. "Aquele que carrega o destino" in balanced 24dp literary serif, no big stars, no ornamental frame. Plain dark solid editor with generous readable line spacing and 17dp ivory body text "A manhã chegou em silêncio. Entre as árvores, uma luz anunciava o início de uma longa jornada." Show caret in text. Above keyboard ONE formatting accessory strip at least48dp, only "Aa", bold B, italic I, list icon, undo, trailing more; each fits width with clear spacing. Thin save status "Salvo neste aparelho" with small green dot and "32 palavras". Keyboard plausible Portuguese Android layout with spacebar "Português (BR)". Main editor content remains visible above keyboard, no background space scene under text. Single compact top action "Detalhes" accessible in overflow if needed.
```

## 05 — entidades

Referência: `WhatsApp Image 2026-09-30 at 09.24.30 (4).jpeg`.

```text
Screen 05: Worldbuilding entity list EMPTY same real screenshot state. Header back, small universe "teste 6.1", page title "Personagens e lugares". Search fullwidth "Buscar no universo" below header. Second row category dropdown "Personagens" and small "0 registros", categories hidden in dropdown/sheet instead of wide horizontal tabs. Center modest quiet outlined person/book illustration, title "Quem vive neste mundo?", small two-line "Crie personagens, lugares e outros elementos da sua história." One central coral/rose CTA "＋ Criar personagem". No duplicate Nova entidade fixed on top of tabs. Lower small secondary "Ver outras categorias". No bottom navigation; preserve right side clock-menu handle. Empty view still purposeful without huge dead universe scene.
```

## 06 — relacoes

Referência: `WhatsApp Image 2026-09-30 at 09.24.31.jpeg`.

```text
Screen06: narrative RELATIONS between entities, NOT devices. Back with universe teste6.1, title "Relações". Compact segmented switch "Lista" selected / "Grafo". Small count "0 relações" and full-width search "Buscar relações". EMPTY state with small linked nodes outline, "As histórias se conectam", copy "Relacione personagens, lugares e acontecimentos do seu universo." Fullwidth primary "＋ Criar relação" in content; compact secondary "Abrir grafo" with expand icon. Brief hint "Adicione entidades para começar." No fake populated relationships, no massive blank full-screen grid or tiny toolbar. No bottom navigation; preserve right side clock-menu handle. Graph remains available as separate full-screen drilldown but not rendered as default.
```

## 07 — timeline

Referência: `WhatsApp Image 2026-09-30 at 09.24.31 (1).jpeg`.

```text
Screen07: "Linha do tempo". Back / universe teste6.1 / small overflow. Search field FULL width "Buscar acontecimentos". Compact below "0 acontecimentos" and "Filtros" button. EMPTY state with understated vertical timeline illustration, header "Dê ordem à sua história", brief "Organize os acontecimentos do seu universo em uma linha do tempo." Primary "＋ Criar primeiro marco". Lower subtle 3-step illustrative vertical markers "Começo", "Mudança", "Consequência" shown as non-data faded guide, NOT real invented milestones or dates. No duplicate new event top/right colliding with search. No bottom navigation; preserve right side clock-menu handle.
```

## 08 — planejamento

Referência: `WhatsApp Image 2026-09-30 at 09.24.31 (2).jpeg`.

```text
Screen08: Mobile planning board with ONE column at a time. Back / teste6.1 / title "Planejamento". Fullwidth search "Buscar cards". Stage control fullwidth one row left arrow, gold dot "Ideias", count "0", right arrow; under it subtle segmented progress dots and caption "Etapa 1 de 4". All fit screen, no neighbouring clipped columns, no horizontal page scroll. Main column a modest solid navy panel: "Nenhuma ideia por aqui", small "Guarde cenas, objetivos e pontos para desenvolver." Fullwidth primary "＋ Adicionar card". Compact secondary "Ver etapas" to select stage, explanatory "Use as setas para mudar de etapa." Keep empty actual state; don't add invented cards. No bottom navigation; preserve right side clock-menu handle. No giant empty panel to bottom or multiple duplicate plus buttons.
```

## 09 — historico

Referência: `WhatsApp Image 2026-09-30 at 09.24.31 (3).jpeg`.

```text
Screen09: History, universe teste6.1, title "Histórico". Top trailing circular refresh icon with enough touch area no text button crushing search. Fullwidth search "Buscar alterações". Small type filter "Todos os tipos" and count "0 registros". Empty state restrained clock/return outline, "Sua história começa aqui", copy "As alterações feitas neste universo aparecerão nesta lista." Below quiet info card lock icon "Histórico neste aparelho" / "Acompanhe mudanças em capítulos e entidades." No SQLite/database technical copy, no restore functionality invented. No bottom navigation; preserve right side clock-menu handle.
```

## 10 — configuracoes-geral

Referência: `WhatsApp Image 2026-09-30 at 09.24.31 (4).jpeg`.

```text
Screen10: General settings. Compact page header back and "Configurações". Current section selector fullwidth dropdown "Geral" with icon and chevron, small caption "Aparência e backup". No horizontal settings tab strip. Card "Aparência", short "Escolha como o NarraHub aparece." Three equal compact readable options sun "Claro", moon "Escuro", device "Sistema"; Sistema selected gold outline. Second card "Backup" small green status "Dados íntegros", text "Guarde uma cópia dos seus universos." Fullwidth primary "Criar backup", below secondary row "Restaurar backup" chevron ONLY if source supports (yes existing app backup restore). Small supporting "Último backup: ainda não criado" clearly sample empty state. Hide raw SQLite,WAL,hash technical jargon. No bottom navigation; preserve right side clock-menu handle. Enough spacing all actions above nav.
```

## 11 — configuracoes-ia

Referência: `WhatsApp Image 2026-09-30 at 09.24.32.jpeg`.

```text
Screen11: Intelligence settings Android. Header back "Configurações", section dropdown "Inteligência". Title within content "Assistência de escrita". Realistic mode selection two stacked fullwidth selectable rows: "Desativada" selected and "API própria" unselected, each large radio & short explanatory line. Separate subdued card "IA local" with muted status "Somente Windows" and copy "A instalação gerenciada não está disponível no Android." Backend confirms Windows x86_64 only. No misleading local install controls or hardware recommendation on Android. Show "Orientações de escrita" as expandable row, only existing settings. Privacy note "Você controla quando usar a IA." No API credentials displayed, no AI text creation button or context engine feature. No bottom navigation; preserve right side clock-menu handle. This proposal must show clear disabled mode and platform dependent local engine pending verification.
```

## 12 — configuracoes-sync

Referência: `WhatsApp Image 2026-09-30 at 09.24.32 (1).jpeg`.

```text
Screen12: existing Sync settings only, no new unified discovery flow. Header back "Configurações", dropdown "Sincronização". Compact first card "Este aparelho" input label "Nome" value "Galaxy S23", modest outlined "Salvar nome" button. Next card "Sincronização" with status "Desligada". Existing action outlined fullwidth "Escutar nesta rede", below tiny note "Mantenha o app aberto durante a conexão." Divider small "CONECTAR A OUTRO APARELHO". Primary fullwidth "Escanear QR". Then manual alternative clear fields vertically: "Endereço" placeholder "192.168.1.10:45870", "Código de 8 dígitos" placeholder "00000000", outlined fullwidth "Parear por código". Use illustrative private address only not exact user's IP. Visible bottom app nav Mais selected. Compact spacing enough all fields not clipped; screen may naturally scroll but buttons never overlap. No nearby devices, no Bluetooth, no mDNS, no new trust/security protocol. Additional conflict status can be simple "Nenhum conflito pendente" small info.
```

## 13 — configuracoes-compartilhar

Referência: `WhatsApp Image 2026-09-30 at 09.24.32 (2).jpeg`.

```text
Screen13: sharing settings. Header back Configurações, dropdown "Compartilhar". Card title "Compartilhar para revisão", state "Desativado". Clear short explanation "Convide alguém para ler, anotar ou sugerir mudanças." Primary "Disponibilizar temporariamente". Small lock note "Você aprova as sugestões antes de alterar o texto." No automatic edit, no social or messaging/send invite affordance. Lower heading "Revisões recebidas" with "0 pendentes", small empty outline annotation icon, "Nenhuma revisão por enquanto", subcopy "As contribuições aparecerão aqui." Existing temporary session only, don't show made-up collaborator or received data. No bottom navigation; preserve right side clock-menu handle.
```

## 14 — configuracoes-atualizacoes

Referência: `WhatsApp Image 2026-09-30 at 09.24.32 (3).jpeg`.

```text
Screen14: update settings. Header back Configurações, dropdown "Atualizações". Quiet centered app icon with N or simple book/orbit glyph, brand NarraHub, title "Seu app está atualizado". Legible version badge "0.10.0-beta.13", descriptive "Você está usando a versão mais recente publicada." One fullwidth outlined primary action "Verificar atualizações" with refresh icon. Lower compact informational card "Sobre esta versão" / "Canal beta" / "Android", no invented release notes or dates. Small reassuring text "Seus universos ficam neste aparelho." No bottom navigation; preserve right side clock-menu handle. Calm purposeful layout, avoid huge duplicate Configurações heading or giant sparse dark blank.
```

