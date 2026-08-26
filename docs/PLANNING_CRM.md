# Planejamento em quadro e fichas

## Objetivo

Transformar o planejamento em um quadro de trabalho para escritores: cards podem ser movidos entre etapas, abertos como fichas e adaptados ao método de cada universo.

## Funcionamento

O quadro mantém cinco etapas: `IDEIAS`, `PLANEJADO`, `ESCREVENDO`, `REVISAO` e `FINALIZADO`. Arrastar um card recalcula a ordem de origem e destino e grava o quadro em uma única instrução SQL. As setas permanecem como alternativa acessível ao gesto de arrastar.

Cada card possui:

- título, descrição, etapa e capítulo relacionado;
- imagem principal local, limitada a 4 MB;
- tags globais do universo;
- campos personalizados definidos pelo usuário.

## Campos personalizados

As definições pertencem ao universo e aparecem em todos os seus cards. Tipos suportados:

| Tipo | Valor armazenado |
|---|---|
| Texto curto ou longo | texto |
| Número | representação numérica textual |
| Caixa de seleção | booleano |
| Sim ou não | `yes`, `no` ou vazio |
| Lista | uma opção definida pelo usuário |
| Lista múltipla | lista de opções |
| Tags | relações normalizadas com tags do universo |
| Histórias relacionadas | relações normalizadas com histórias |
| Personagens relacionados | relações normalizadas com entidades do tipo `Personagem` |

Relações usam IDs porque nomes podem ser alterados, mas não ficam soltas no JSON. Foreign keys eliminam automaticamente os vínculos quando uma história, personagem, tag, campo ou card é excluído. Excluir uma definição também limpa o eventual valor escalar correspondente nos cards.

## Persistência

As migrations 11, 12 e 13 são append-only:

- adiciona `planning_items.image`;
- adiciona `planning_items.custom_field_values`, que exige um objeto JSON válido;
- cria `planning_field_definitions` com tipo validado, ordem e opções JSON;
- cria `planning_field_links` com escopo de universo, tipo e destino validados;
- migra relações gravadas temporariamente no JSON pelo build de desenvolvimento do schema 11;
- preserva `content_tags` e `content_tag_assignments` como sistema separado de categorização.

O `PlanningService` é o gateway da feature durante a Fase 2. O salvamento completo da ficha já é um piloto da Fase 4: um comando Rust valida escopo e tipos, substitui relações, atualiza valores escalares e confirma tudo em uma única transação. A ordenação do quadro continua em uma instrução SQL atômica.

## Testes e gates

- `npm run test:planning`: ordenação entre colunas, reordenação interna e saneamento dos valores JSON;
- teste Rust `v11_adds_typed_planning_cards_and_cleans_deleted_field_values`;
- testes Rust das migrations 12/13, rollback integral e sincronização das relações;
- `npm run build`;
- inicialização Tauri e aplicação da migration em banco de desenvolvimento existente;
- interação visual de drag-and-drop no WebView ainda exige teste manual enquanto a automação de janela estiver indisponível.
