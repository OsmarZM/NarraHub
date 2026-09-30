# NH-085 — Auditoria mobile V1

## Objetivo e evidência

Auditar a apresentação do NarraHub no Galaxy S23 e definir propostas de composição mobile
antes de alterar as telas. Em 2026-09-30 o usuário forneceu 14 screenshots no arquivo
`WhatsApp Unknown 2026-09-30 at 09.32.58.zip`. Originais preservados em
`audit-2026-09-30/screenshots/`. A tela de atualizações mostra `0.10.0-beta.13`.

Todos os prints são de retrato e tema escuro. Não provam comportamento de toque, teclado,
persistência, gestos, desempenho, paisagem ou tema claro. Esta é auditoria visual parcial de M0,
não qualificação M15 nem avaliação de todas as rotas existentes.

## Decisões do usuário

- Adiar mDNS, BLE e tela unificada da NH-084 e avançar para a Fase 5.
- Gerar uma proposta de design por página/estado enviado.
- Manter o **menu de relógio lateral**, sem menus inferiores. A primeira exploração com
  navegação inferior foi descartada após a correção explícita do usuário.

## Achados por página

Nomes de arquivos abaixo abreviam o prefixo `WhatsApp Image 2026-09-30 at ` e mantêm os sufixos.
Classificações descrevem a evidência estática, sem inferir ações não executadas.

| # | Print | Página | Classificação | Evidência e proposta |
| --- | --- | --- | --- | --- |
| 01 | 09.24.30.jpeg | Biblioteca | Desconfortável | Hero e capa ocupam grande parte da tela; apenas um universo fica inteiro à vista. Cards compactos, capas menores e acesso claro ao capítulo recente. |
| 02 | 09.24.30 (1).jpeg | Menu de relógio | Desconfortável | Itens distantes do centro ficam pequenos e escuros; instrução inferior pouco legível. Preservar roda lateral e gestos, aumentar legibilidade e oferecer fechamento explícito. |
| 03 | 09.24.30 (2).jpeg | Criar universo | Desconfortável | Upload de capa ocupa área excessiva e o conteúdo de fundo compete através da folha. Capa opcional compacta, superfície opaca e ações fora do formulário. |
| 04 | 09.24.30 (3).jpeg | Escrita | Desktop espremido | Toolbar chega cortada à direita; título e decoração ocupam o início do editor. Cabeçalho compacto, área de texto limpa e ações secundárias em folha. Teclado aberto na proposta é cenário projetado, ausente dos prints. |
| 05 | 09.24.30 (4).jpeg | Entidades | Desktop espremido | “Nova entidade” cobre as categorias. Separar filtro, busca e criação, usar seleção de categoria em folha. Prioridade alta. |
| 06 | 09.24.31.jpeg | Relações | Desktop espremido | Contagem cortada e controles pequenos em uma linha longa; grafo vazio ocupa a tela. Lista padrão, grafo sob demanda em tela cheia. Trata relações narrativas, não conexões entre aparelhos. |
| 07 | 09.24.31 (1).jpeg | Timeline | Desconfortável | Busca estreita ao lado de criação e duas ações de criar no estado vazio. Busca em linha própria, um CTA e timeline vertical quando houver dados. |
| 08 | 09.24.31 (2).jpeg | Planejamento | Desktop espremido | Colunas vizinhas cortadas, cabeçalhos repetidos e várias ações de criar. Uma coluna por vez, seletor de etapa, criação contextual. |
| 09 | 09.24.31 (3).jpeg | Histórico | Desconfortável | Atualizar disputa largura com busca; explicação expõe armazenamento técnico. Busca larga, atualização compacta e mensagem de usuário. |
| 10 | 09.24.31 (4).jpeg | Geral | Desktop espremido | Abas de configurações excedem largura; cabeçalho duplicado e backup usa termos SQLite/WAL. Seletor de seção, aparência compacta e backup em linguagem simples. |
| 11 | 09.24.32.jpeg | Inteligência | Desktop espremido | Recomendação e modo “Neste computador” no Android. O backend `local_ai_status` só suporta instalação gerenciada em Windows x86_64; mostrar limitação real e modos existentes. |
| 12 | 09.24.32 (1).jpeg | Sincronização | Desconfortável | Texto longo e ações acumuladas; “Código” fica abaixo do enquadramento. Organizar escuta, QR e campos manuais em sequência vertical. Não inferir que conteúdo fora do print seja inacessível. |
| 13 | 09.24.32 (2).jpeg | Compartilhar | Desktop espremido | Abas horizontais e explicação técnica extensa. Simplificar sessão temporária, consentimento e revisões pendentes. |
| 14 | 09.24.32 (3).jpeg | Atualizações | Desconfortável | Cabeçalho duplicado, abas largas e espaço pouco aproveitado. Estado da versão, ação de verificar e informação do canal. |

Nenhuma tela é classificada “inutilizável” só pelo screenshot: sobreposição visual comprova
o defeito de composição, mas precisamos executar o fluxo para afirmar bloqueio funcional.

## Padrão para as propostas

- Manter identidade azul-marinho, dourado e acento coral/rosa existente; reduzir o ruído cósmico
  sob texto e formulários. Cards suficientemente opacos, sem planetas atrás dos campos.
- Manter o relógio lateral. Reservar área junto à alça; não encobrir ações. Sem tab bar inferior.
- Títulos de seção compactos, serifada literária só onde favorece conteúdo; corpo mínimo 16px.
- Alvos de toque de 48px; gutters regulares e safe areas reais do Android. Imagem não prova medidas.
- Uma ação principal por contexto. Busca e filtros em linhas próprias quando necessário.
- Folhas para ações secundárias e detalhes. Formulários verticais; capa não domina a criação.
- Escrita prioritária; teclado não cobre cursor ou ações. Ocultar navegação contextual durante
  edição quando isso for necessário para evitar sobreposição; preservar acesso ao menu ao sair.
- Estado vazio orienta uma ação. Exemplos e capas nas propostas são ilustrativos; não representam
  dados novos ou alterações no acervo.
- Textos de capacidades condicionados à plataforma. Sem hardware/recomendação de desktop no Android.

## Imagens e aplicação

14 propostas em `design-v1/`, índice `design-v1/index.html`. Geradas pela ferramenta integrada
de imagens, com prompts preservados em `design-v1/PROMPTS.md`. A imagem 01 é referência de
paleta/card; demais propostas usam a própria página e essa referência. Arquivos finais preservados
no workspace. Algumas palavras, ícones e proporções podem variar na rasterização: os critérios
textuais deste documento prevalecem sobre artefatos do gerador.

As propostas são referência de composição, não telas funcionais. Implementar por fatias sobre
o ADR 0011: apresentação própria no mobile, handlers/stores/rotas compartilhados; nenhuma duplicação
de domínio ou regra global sobrescrevendo internals de outras features.

## Ordem de implementação

1. Sobreposição de entidades, overflow de toolbars, seletor de configurações e área da alça.
2. Escrita: teclado, seleção/copiar/colar, texto longo, detalhes e paisagem.
3. Biblioteca, criação e formulários de entidades.
4. Planejamento, timeline e relações com conteúdo real suficiente para testar densidade.
5. Histórico e configurações; adequação de IA à plataforma sem criar novo recurso de IA.
6. Claro/escuro, outros viewports e regressão desktop; qualificação física M15.

## Pendências de M0/M15

- Capturar formulários de entidade/marco/card, capítulos, detalhes, conteúdo preenchido, menu e erros.
- S23: teclado aberto, paisagem, tema claro, seleção/copiar/colar e persistência após navegar.
- Confirmar em runtime alcance das ações, comportamento da alça e retorno do Android.
- Medir áreas de toque/contraste e sobreposição com geometria real; mockup não é gate.
- Provar zero rolagem horizontal da página; o giro intencional do relógio e swipe de etapa não
  equivalem a overflow acidental.
- Manter PIN/QR e sincronização qualificados; não adicionar descoberta adiada.
