# NarraHub — Design mobile V2

## Objetivo

Revisar as propostas V1 após o usuário considerar as páginas simples demais. V2 acrescenta
hierarquia, composição, capas, miniaturas e detalhes de interação sem ampliar o escopo funcional.
Cada página tem versão clara e escura. O **relógio lateral permanece**, sem navegação inferior.

## Entrega e leitura

- 14 pranchas PNG, uma por página/estado de referência.
- Cada prancha contém dois temas da mesma página: **escuro à esquerda, claro à direita**.
- `index.html`: galeria com seleção de página, ampliação e comparação opcional com a V1.
- `PROMPTS.md`: padrão comum e especificação de cada página.
- As pranchas são referências rasterizadas; não são telas implementadas nem prova de comportamento.

## Linguagem visual

### Composição

Cabeçalhos compactos, cartões com alturas diferentes conforme a responsabilidade e agrupamento
por contexto. Evitar grandes retângulos repetidos, títulos serifados em toda a interface e longas
explicações ocupando o início da página. Biblioteca pode usar títulos literários; interfaces de
controle usam sans-serif. Editor privilegia texto e teclado.

Capas e retratos ganham espaço dentro de cards delimitados. Ilustrações de estado vazio devem
ser compactas e subordinadas à ação principal. Background cósmico não atravessa formulários.

### Dois temas

| Papel | Escuro | Claro |
| --- | --- | --- |
| Fundo | Azul profundo `#0B1020` | Perolado `#F4F6FB` |
| Superfície | Azul ardósia `#171F34` | Branco `#FFFFFF` |
| Texto principal | Marfim `#F4F2EC` | Azul escuro `#172238` |
| Texto secundário | Azul claro `#ABB7CE` | Azul acinzentado `#52617B` |
| Relógio/seleção | Dourado `#D8B772` | Dourado profundo `#8B6425` |
| Ação principal | Coral/rosa com texto azul escuro | Pêssego/rosa com texto azul escuro |

Paleta orientativa; ao implementar, mapear papéis para os tokens oficiais e medir contraste na
superfície efetiva. Nenhum token novo sem definição e sem cobertura do tema claro/escuro/sistema.
O modo Sistema já existente escolhe o tema; não criar terceiro layout ou terceira navegação.

Claro tem superfícies, bordas, sombras e ícones próprios. Não reutilizar card escuro com texto
escuro nem inverter automaticamente as cores de capa/ilustração. Dados e ações permanecem iguais
entre temas, inclusive estados de seleção, erro, carregamento e desativação.

### Controles e geometria

- Corpo de texto: referência 16px; editor 17px com entrelinha confortável.
- Alvos de toque: referência mínima 48px. Bordas arredondadas e ícones não substituem medição.
- Cards: raio orientativo 24px; campos/botões 14px; gutters de aproximadamente 20px.
- A alça do relógio fica em área própria; nenhuma ação sob ela. Toque e giro existentes preservados.
- Sem tab bar inferior. A barra de gestos do Android não é navegação do aplicativo.
- Campos e CTAs em sequência acessível com teclado, sem sobreposição nem página larga.
- Movimento/háptica não podem ser provados pelas pranchas: validar durante implementação.

## Conteúdo de exemplo e limites

Crônicas do Vale, Entre estrelas, Elara, Cael, Mira e marcos/cards de história são exemplos
fictícios para demonstrar composição. Não inserir exemplos no banco do usuário nem criar campos
de domínio apenas porque aparecem na imagem. Adaptar cada rótulo aos contratos já existentes.
As pranchas podem conter artefatos de texto; regras do relatório e capacidades do backend prevalecem.

Biblioteca mantém continuar/criar; entidades mantêm CRUD/categorias; relações mantêm edição/grafo;
planejamento mantém suas etapas; sincronização mantém escuta/PIN/QR. Nenhuma descoberta mDNS/BLE.
IA local gerenciada permanece Windows x86_64; API própria pode enviar contexto ao provedor.

## Implementação posterior

V2 substitui V1 como referência visual. Implementar por fatias no ADR 0011, compartilhando
Router, stores, handlers e domínio. Preservar controles e comportamentos qualificados.
Ordem segue auditoria: sobreposições/configuração, escrita/teclado, biblioteca/formulários,
planejamento/timeline/relações, histórico e acabamento.

Validar cada fatia no tema claro e escuro, teclado aberto/fechado, retrato/paisagem, conteúdo vazio
e preenchido e desktop. M0 e M15 não são concluídos por geração de imagens.
