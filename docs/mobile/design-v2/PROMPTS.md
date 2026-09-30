# NarraHub — Prompts V2: páginas modernas em claro e escuro

## Geração

Ferramenta integrada de imagens, uma chamada por prancha. Cada prancha contém a mesma página nos dois temas.
A V1 é referência de conteúdo; a biblioteca V2 é referência visual comum. Não foi usado CLI/API externo.

Revisão final da prancha 14: substituir a linha “SEUS UNIVERSOS” abaixo da marca pela versão
`0.10.0-beta.13` nos dois temas, preservando todo o restante da composição.

## Padrão comum

```text
Use case: ui-mockup. Produce ONE high-fidelity DESIGN BOARD with TWO matched portrait Android screens side-by-side: left DARK theme labeled "ESCURO" above screen, right LIGHT theme labeled "CLARO" above screen. Both screens represent the SAME NarraHub mobile page, SAME data, SAME controls, SAME relative positions, adapted authentically to each theme. Board nearly square 1700x1850, two full 390x844dp portrait app canvases on subtle neutral grey outer backdrop; no phone device hardware, no perspective. Portuguese text exceptionally crisp. Modern premium 2026 mobile creative writing app, feel authored, polished, expressive: compact rounded bold geometric sans headings, balanced asymmetry inside cards, modern thin rounded icons, refined illustrative empty states, small metadata chips, modular cards, tactile layers, carefully considered hierarchy. This is a strong redesign of the previous reference, which looked like plain desktop panels. Avoid oversized traditional serif titles, huge outlined boxes, repetitive fullwidth rectangles, long explanatory paragraphs, huge dead space, dense technical copy. Serif only INSIDE narrative editor reading text or universe/book titles if subtle. Major card radius24dp, controls14dp, consistent20dp gutters and16dp body text, min48dp interactive areas.
Dark palette: deep ink navy #0B1020, layered slate/navy #171F34 with quiet indigo/lavender mesh highlight, ivory text #F4F2EC, readable secondary #ABB7CE; restrained soft gold #D8B772 for clock navigation. Light palette is a real light theme: airy pearl #F4F6FB, solid WHITE cards with delicate bluegrey shadow, dark navy text #172238, secondary #52617B, gentle lilac/peach accents, deep gold #8B6425 on clock; no dark panels except small cinematic cover thumbnail. Existing primary accent coral/rose may use more refined restrained peach-to-rose gradient with DARK navy label, not white. Subtle cosmic identity in clipped cover/illustrations or small halo; no wallpaper running behind form text. Both backgrounds must look different light/dark while preserving visual weight and readable controls.
ABSOLUTE navigation rule: keep existing RIGHT SIDE CLOCK/WHEEL MENU, visible discreet gold three-line grip at right mid-screen in its own outer gutter, never overlap content. NO bottom navigation bar, NO footer tabs, NO dock icons. Only Android OS gesture pill at very bottom and status bar at top. Explicit top clock icon may open same wheel, no new navigation system. Closed menu in all pages except menu screen. Footer actions are form actions only. No floating button on content.
Use existing functional scope only; no login, subscriptions, streaks, badges gamification, new AI engine, discovery, Bluetooth, cloud sync claims or magic automations. Fictional book covers, example narrative data can improve populated page composition, but no new capability. Match reference meaning not layout. Board shows a design proposal, not actual runtime. Do not include annotations over UI or put controls outside screen boundaries. Ensure every essential action visible/readable and consistent across both themes.
```

## 01 — Biblioteca

```text
Page01 "Biblioteca". Compact top NARRAHUB wordmark, search and clock icon. Header "Seus universos", small chip "4 universos". FEATURED continue-writing card with cinematic landscape cover clipped top half, soft overlay just under book cover with small tag "CONTINUAR", fictional universe "Crônicas do Vale", chapter title "Aquele que carrega o destino", primary pill "Continuar" and tiny saved dot. Balanced 200dp total hero maximum. Below section "Sua biblioteca" with compact sorting "Recentes". TWO modest book/universe cards side-by-side grid with vertically cropped covers and labels "Crônicas do Vale" and "Entre estrelas", short metadata "1 livro · 3 capítulos" and "2 livros · 8 capítulos"; third slim text/cover row "teste 6.1". Covers expressive original fantasy art, same in both themes. Bottom fullwidth compact button "+ Criar universo" above gesture inset. No whole page serif or cosmic wallpaper; visually modern, premium editorial library. Clock grip in reserved right edge gutter. Same content in both themes.
```

## 02 — navegacao

Referência de conteúdo: `../design-v1/02-navegacao.png`.

```text
Page02 OPEN side CLOCK MENU, preserve user's wheel mechanism. Small "NARRAHUB" wordmark, universe selector "Crônicas do Vale", explicit X close. Five readable pill cards follow gentle arc along right-hand circular wheel axis: "Universos", "História", CENTER selected "Personagens", "Relações", "Linha do tempo". Selected card slightly larger, modern frosted-indigo/tinted-lilac surface and restrained gold inner outline with crisp icon. Other four fully legible, moderate scale, no opacity near zero. Gold thin arc with small radial tick marks like precision watch, not giant drawn analog clock occupying screen. Bottom hint "Gire para escolher · Toque para abrir"; existing wheel may rotate to Planejamento/Histórico/Configurações beyond viewport; no extra submenu introduced. Dark wheel card surfaces midnight translucent; light wheel opaque WHITE with pastel lavender shadows; labels deep navy in light. Same wheel positions and selected destination in both. No bottom menu whatsoever.
```

## 03 — criar-universo

Referência de conteúdo: `../design-v1/03-criar-universo.png`.

```text
Page03 modern focused mobile form "Criar universo". Single compact top back / title / close, no global brand header above form. Small creative halo illustration on leading header margin only. Optional cover picker horizontal card with SMALL thumbnail and "Adicionar capa" / "Opcional". Label "Nome do universo", field "Ex.: Crônicas do Vale". Label "Descrição" optional, 3-line field "Uma ideia para começar…". Surface/input grouping with large comfortable touch spacing, understated inset backgrounds rather than huge borders. Slim privacy inline lock note "Salvo neste aparelho." Lower pinned safe form action primary "+ Criar universo" and secondary "Cancelar". No added stepper since source single form, no invented template selection. Modern visual detail, subtle corner glints; no decorative wallpaper. Clock grip in reserved right gutter away from inputs. Same empty fields in both themes.
```

## 04 — escrita

Referência de conteúdo: `../design-v1/04-escrita.png`.

```text
Page04 writing editor with Android KEYBOARD OPEN on both dark/light screens, keyboard matches theme. Maximize editing space, compact header back, small "Crônicas do Vale", overflow, lower small chapter picker "Capítulo 1" / "Capítulos". Book chapter title "Aquele que carrega o destino" in restrained22dp medium serif or clean sans, NOT massive hero. Readable narrative text at17dp with generous line spacing "A manhã chegou em silêncio. Entre as árvores, uma luz anunciava o início de uma longa jornada." caret. Plain paper-like very light warm editor in light; quiet navy editor dark. Slim status line "Salvo" green dot and "32 palavras". Sleek rounded accessory formatting toolbar immediately ABOVE KEYBOARD, with Aa, B, I, list, undo, more; 6 large touch targets that fit. No fullwidth decorative card enclosing entire editor, no AI toolbar or generatedwriting button. Keyboard plausible Portuguese, not product UI to implement. Right gold side clock handle high enough to not overlap keys or content. Same header, editor scroll/caret and keyboard arrangement in both themes.
```

## 05 — entidades

Referência de conteúdo: `../design-v1/05-entidades.png`.

```text
Page05 worldbuilding "Personagens", small Crônicas do Vale context. Modern pill category dropdown "Personagens", separate small count "3 registros"; search full width and compact filter icon. Populated fictional sample demonstrates density: hero card portrait crop of fictional fantasy character "Elara" tag "Protagonista" and short role "Guardiã do Vale"; next TWO compact horizontal rows avatars "Cael" / "Mentor" and "Mira" / "Aliada". Portraits tasteful cinematic illustration not random real photos, SAME in both themes. Fullwidth primary "+ Criar personagem" above system gesture inset. NO new entity types/features beyond character role fields. No overlap with category/search, no top sticky FAB. Clear differentiated cards, refined grouped metadata not giant identical rectangles. Right clock grip outside cards.
```

## 06 — relacoes

Referência de conteúdo: `../design-v1/06-relacoes.png`.

```text
Page06 narrative RELATIONS "Relações", Crônicas do Vale. Modern slim segmented control "Lista" active / "Grafo", fullwidth search, count "3 relações". Fictional relationship sample list each compact card avatars/pair endpoints, clear relation title: "Elara → Cael" subtitle "Aprendiz de", "Elara → Mira" subtitle "Aliada de", "Cael → Vale" subtitle "Protege". Actual relation labels descriptive UI examples, not newly invented backend fields. A small closed geometric miniature graph illustration header corner can establish identity but no huge graph canvas; "Abrir grafo" secondary pill. Primary "+ Criar relação". Modern connections color-coded lavender/coral chips semantically linked, same data both themes. No device/network connections, no trust or nearbydevices. Clock side grip outside content.
```

## 07 — timeline

Referência de conteúdo: `../design-v1/07-timeline.png`.

```text
Page07 "Linha do tempo", Crônicas do Vale context. Fullwidth search "Buscar acontecimentos", compact filter "Todos". Show populated fictional timeline VERTICAL, THREE modest polished rounded event cards connected by a thin accent line with gold/coral/lavender dots, no giant emptyillustration: "A chegada ao Vale" / "Início da jornada", "O encontro com Cael" / "Uma nova aliança", "O portal se abre" / "O destino muda". Small "3 marcos" top, no unverified specific date/calendar semantics. Compact colored illustration thumbnails corner of first/third card, ample reading area, same sample content both themes. Fullwidth "+ Criar marco" safe bottom. No duplicative actiontop, no time/era scale invented. Modern visual rhythm with alternating small metadata blocks but all cards within one column. Right side clock handle reserved gutter.
```

## 08 — planejamento

Referência de conteúdo: `../design-v1/08-planejamento.png`.

```text
Page08 "Planejamento", Crônicas do Vale. FULL width search "Buscar cards". ONE stage at a time mobile board, compact gold active pill "Ideias · 3" with left/right arrow48dp each, 4 tiny stage progressdots under and small "1 de 4". Populate3 sample cards stacked varied hierarchy: "A abertura da história" / small tag "Cena", "O segredo de Elara" / tag "Personagem", "A travessia do portal" / tag "Objetivo". Simple short descriptions one line each, small existing stage/tag concepts not checklists or attachments new features. Premium tactile cards with fine left colored edge and small 6dot drag handle or overflow; no desktop columns, no horizontallyclipped adjacent boards. Modest primary "+ Adicionar card". Secondary "Trocar etapa" or stage label opens existing stage choice, don't add unrelated filterfeature. Right clock handle gutter, both themes same arrangement.
```

## 09 — historico

Referência de conteúdo: `../design-v1/09-historico.png`.

```text
Page09 "Histórico" Crônicas do Vale. Modern compact header trailing refresh, fullwidth search "Buscar alterações", compact pill "Todos os tipos" and "3 registros". A grouped timeline-like list with small "Hoje" section label, THREE fictional example activity rows inside one modular white/navy card: icon pencil "Capítulo atualizado" / "Aquele que carrega o destino", iconperson "Personagem criado" / "Elara", iconlink "Relação adicionada" / "Elara e Cael". Different colored round icon tiles, readable contextual secondary text, understated separators. Small explanatory text "Mudanças deste universo, neste aparelho." No unverified restore/undo actions, no cloud activities. Avoid pointless emptyintro or longcopy. Same examples both themes. Right side clock grip.
```

## 10 — configuracoes-geral

Referência de conteúdo: `../design-v1/10-configuracoes-geral.png`.

```text
Page10 settings General. Single compact title "Configurações", back, clock. Section selector pill "Geral" with chevron and small active settings glyph, no horizontal tabs. First APPEARANCE card "Aparência", THREE delightful miniature THEME PREVIEW thumbnails showing schematic app, labels "Claro", "Escuro", "Sistema"; Sistema selected with readable gold check, all equal large hit areas. Not just enormous sun/moon buttons; thumbnails visually communicate theme. Second card "Backup", small green statuschip "Dados íntegros", briefcopy "Uma cópia dos seus universos." Compact rows/button primary "Criar backup", secondary "Restaurar backup", small "Nenhum backup criado". Simple modern layered group with small supporting illustrated secure book symbol, no technicalSQL. Same choices on both board screens; light is fully whitepearl. No bottom navigation. Right clock reserved gutter.
```

## 11 — configuracoes-ia

Referência de conteúdo: `../design-v1/11-configuracoes-ia.png`.

```text
Page11 Settings "Inteligência", single compact Configurações header and section selectorpill. Clean title "Assistência de escrita" modest24dp. Two tactile large selectable rows with lightweight illustrations/icon: "Desativada" selected, short "Escreva sem assistência."; "API própria", short "Use seu provedor configurado." Selection small gold check and quiet pastel highlight, no giant radio panels. Compact neutral informative strip "IA local · Somente Windows" / "Instalação gerenciada indisponível no Android." Existing accordion row "Orientações de escrita" chevron. Privacy card slim lockglyph, copy exactly "Ao usar API própria, o contexto enviado segue para o provedor configurado." No misleading on-device-only processing, no PC hardware recommendation, no installationCTA, no inventedAIfeature. More elegant visual grouping and positive hierarchy but no new capability. Both themes same; clock grip gutter.
```

## 12 — configuracoes-sync

Referência de conteúdo: `../design-v1/12-configuracoes-sync.png`.

```text
Page12 Existing Sync SETTINGS. Single compact Configurações header and "Sincronização" section selector. SMALL card "Este aparelho" with editable name "Galaxy S23" and compact "Salvar nome" action. Main section status "Desligada" and simple idle phone-to-laptop illustration small headercorner, no invented detecteddevices. Outlined fullwidth "Escutar nesta rede", smallnote "Mantenha o app aberto." Divider "Conectar a outro aparelho". Primary refined coral/rose "Escanear QR". Then manual fields "Endereço" placeholder "192.168.1.10:45870" and "Código de 8 dígitos" placeholder "00000000", outlined "Parear por código". Fit readable no giant cardheading or lengthytechnicalprose. Separate subtle "Nenhum conflito pendente" simple text. No new unified flow, BLE/mDNS/discovery nearbydevice list. Same existing controlsandpositions both themes. Right clock grip outside card.
```

## 13 — configuracoes-compartilhar

Referência de conteúdo: `../design-v1/13-configuracoes-compartilhar.png`.

```text
Page13 Settings "Compartilhar", single compact Configurações header and section selector. Feature card modern original small illustration layered document sheets with annotation marks, title "Compartilhar para revisão", statuschip "Desativado", briefcopy "Leitura, anotações e sugestões." Primary "Disponibilizar temporariamente". Inline shieldsmall "Você aprova as sugestões antes de alterar o texto." Lower card "Revisões recebidas", pill "0 pendentes", tasteful EMPTY state smallspeech/documentglyph "Nenhuma revisão por enquanto." No cloud/logins/sendemail buttons or madeupreviewer list. Great balance of illustration and productcontrols, shared pairedthemes. Right clock gutter.
```

## 14 — configuracoes-atualizacoes

Referência de conteúdo: `../design-v1/14-configuracoes-atualizacoes.png`.

```text
Page14 Settings "Atualizações", single compact Configurações header and section selector. Strong modern update status card, small original N/book icon with subtle orbital goldring, green compact checkmark status "Tudo atualizado", clear "NarraHub" and versionpill "0.10.0-beta.13". Body "Você usa a versão mais recente publicada." Refined outlined or peach accented primary "Verificar atualizações". Below TWO mini info tiles "Canal" / "Beta" and "Plataforma" / "Android". Quiet footer "Seus universos ficam neste aparelho." No fabricatedreleasenotes dates or installbutton. Modern premium composition strong layered shadows on light, subtleluminosity on dark, tiny constellation only nearlogoillustration not giant wallpaper. No bottom navigation; right clock gutter.
```

