-- Fixture anônima e determinística de um banco **criado nativamente no schema 15**.
--
-- Ela existe porque a fixture de schema 10 não consegue produzir estas formas. Um banco que
-- CHEGOU ao 15 por migração tem todo campo de planejamento universal — é exatamente o que a
-- migration 15 faz com o que veio antes. Um banco que NASCEU no 15 pode ter campo com
-- `scope = 'card'` e `owner_item_id` preenchido, `custom_field_values` com conteúdo, e nós e
-- arestas de canvas ligando coisas que não são fichas.
--
-- Qualquer migration 16 vai encontrar os dois formatos no mundo real. Sem esta fixture, só o
-- formato migrado seria testado, e o nativo — que é o da maioria dos usuários daqui para
-- frente — chegaria à produção sem nunca ter passado por um upgrade em teste.
--
-- Só dados fictícios.
--
-- Colunas opcionais usam string vazia, e não NULL: várias delas são `NOT NULL DEFAULT ''`
-- no schema, então NULL explícito quebra a constraint. Escrever `''` deixa a fixture igual
-- ao que o aplicativo grava de verdade.

INSERT INTO universes (id, name, description) VALUES
  ('fx15-u1', 'Oficina dos Ventos', 'Universo fictício nascido no schema 15');

INSERT INTO stories (id, universe_id, name, description) VALUES
  ('fx15-s1', 'fx15-u1', 'O Relojoeiro Surdo', 'História de teste');

INSERT INTO books (id, story_id, name, description, cover_image) VALUES
  ('fx15-b1', 'fx15-s1', 'Livro dos Pêndulos', 'Livro de teste', '');

INSERT INTO chapters (
  id, book_id, title, content, word_count, status, summary, scene_origin, scene_destination
) VALUES
  ('fx15-c1', 'fx15-b1', 'A engrenagem parada',
   '<p>O relógio da praça parou às quatro, e ninguém percebeu.</p>', 11, 'RASCUNHO',
   'O tempo da cidade trava.', 'Praça do Sino', 'Oficina'),
  ('fx15-c2', 'fx15-b1', 'O aprendiz',
   '<p>Ela ouviu o silêncio antes de ouvir a explicação.</p>', 9, 'REVISAO',
   'Chega quem vai consertar.', 'Oficina', 'Torre');

INSERT INTO chapter_revisions (id, chapter_id, title, content, word_count, created_at) VALUES
  ('fx15-r1', 'fx15-c1', 'A engrenagem parada',
   '<p>O relógio da praça parou.</p>', 5, '2026-01-10 09:00:00');

INSERT INTO entities (id, universe_id, type, name, description, summary) VALUES
  ('fx15-e1', 'fx15-u1', 'Personagem', 'Vera', 'Relojoeira que ficou surda na infância.', 'Protagonista'),
  ('fx15-e2', 'fx15-u1', 'Lugar', 'Torre do Sino', 'Torre que abriga o mecanismo antigo.', 'Cenário central'),
  ('fx15-e3', 'fx15-u1', 'Organizacao', 'Guilda dos Pêndulos', 'Mantém os relógios da cidade.', 'Antagonista');

INSERT INTO entity_attributes (id, entity_id, key, value, sort_order) VALUES
  ('fx15-a1', 'fx15-e1', 'Idade', '34', 0),
  ('fx15-a2', 'fx15-e1', 'Ofício', 'Relojoeira', 1),
  ('fx15-a3', 'fx15-e2', 'Altura', '40 metros', 0);

INSERT INTO entity_templates (id, universe_id, entity_type, attribute_key, default_value, sort_order) VALUES
  ('fx15-t1', 'fx15-u1', 'Personagem', 'Idade', '', 0);

INSERT INTO relations (id, universe_id, source_id, target_id, type, label, bidirectional, importance, created_at) VALUES
  ('fx15-rel1', 'fx15-u1', 'fx15-e1', 'fx15-e3', 'custom', 'foi expulsa da', 0, 3, '2026-01-11 10:00:00'),
  ('fx15-rel2', 'fx15-u1', 'fx15-e1', 'fx15-e2', 'custom', 'trabalha na', 1, 2, '2026-01-11 10:05:00');

INSERT INTO mentions (id, chapter_id, entity_id, created_at) VALUES
  ('fx15-m1', 'fx15-c1', 'fx15-e2', '2026-01-12 08:00:00'),
  ('fx15-m2', 'fx15-c2', 'fx15-e1', '2026-01-12 08:05:00');

INSERT INTO content_tags (id, universe_id, name, color, created_at) VALUES
  ('fx15-tag1', 'fx15-u1', 'primeiro ato', '#8b5cf6', '2026-01-09 12:00:00'),
  ('fx15-tag2', 'fx15-u1', 'reescrever', '#ef4444', '2026-01-09 12:01:00');

INSERT INTO attachments (id, universe_id, owner_type, owner_id, data_url, caption, sort_order, created_at) VALUES
  ('fx15-at1', 'fx15-u1', 'entity', 'fx15-e1', 'data:image/png;base64,fixture', 'Retrato de Vera', 0, '2026-01-09 14:00:00');

INSERT INTO timeline_events (
  id, universe_id, title, description, event_type, start_date, end_date,
  created_at, updated_at, entity_id, display_date, sort_key
) VALUES
  ('fx15-tl1', 'fx15-u1', 'A parada do relógio', 'O mecanismo trava pela primeira vez.',
   'MARCO', '1888-04-12', '', '2026-01-09 13:00:00', '2026-01-09 13:00:00',
   'fx15-e2', '12 de abril de 1888', '1888-04-12');

-- Tags em tipos de dono diferentes: capítulo, entidade e evento de timeline.
-- Vem depois de timeline_events porque uma das atribuições aponta para ele.
INSERT INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at) VALUES
  ('fx15-ta1', 'fx15-tag1', 'chapter', 'fx15-c1', '2026-01-12 09:00:00'),
  ('fx15-ta2', 'fx15-tag2', 'entity', 'fx15-e3', '2026-01-12 09:01:00'),
  ('fx15-ta3', 'fx15-tag1', 'timeline', 'fx15-tl1', '2026-01-12 09:02:00');

-- ── O que só existe num banco nascido no 15 ────────────────────────────────────────────
-- Os cards vêm antes das definições de campo: `owner_item_id` referencia o card dono,
-- e com `PRAGMA foreign_keys = ON` a ordem de inserção importa.
INSERT INTO planning_items (
  id, universe_id, chapter_id, title, description, status, target_words, sort_order,
  created_at, updated_at, image, custom_field_values
) VALUES
  ('fx15-pi1', 'fx15-u1', 'fx15-c1', 'Abertura', 'Card com campo próprio e valores preenchidos.',
   'IDEIAS', 900, 0, '2026-01-13 11:00:00', '2026-01-13 11:00:00', '',
   '{"fx15-fd1":"alta","fx15-fd2":"o sino racha"}'),
  ('fx15-pi2', 'fx15-u1', NULL, 'Cena solta', 'Card sem capítulo, só com o campo universal.',
   'PLANEJADO', 400, 1, '2026-01-13 11:05:00', '2026-01-13 11:05:00', '',
   '{"fx15-fd1":"baixa"}');

-- Um campo universal e um campo restrito a um card. A migration 15 nunca produz o segundo:
-- ela converte tudo o que existia para universal. Sem esta fixture, `scope = 'card'` jamais
-- seria o ponto de partida de um teste de upgrade.
INSERT INTO planning_field_definitions (
  id, universe_id, name, field_type, options_json, sort_order, created_at, updated_at, scope, owner_item_id
) VALUES
  ('fx15-fd1', 'fx15-u1', 'Tensão', 'text', '[]', 0,
   '2026-01-13 10:00:00', '2026-01-13 10:00:00', 'universal', NULL),
  ('fx15-fd2', 'fx15-u1', 'Ponto de virada', 'text', '[]', 1,
   '2026-01-13 10:01:00', '2026-01-13 10:01:00', 'card', 'fx15-pi1');

-- ── Canvas: anotação de diagrama, que não é fato do universo (migration 14) ─────────────
INSERT INTO canvas_nodes (id, universe_id, kind, text, image, color, position_x, position_y, created_at, updated_at) VALUES
  ('fx15-cn1', 'fx15-u1', 'note', 'Conferir se a guilda sabia antes.', '', '#f59e0b', 120.5, -40.0,
   '2026-01-14 09:00:00', '2026-01-14 09:00:00'),
  ('fx15-cn2', 'fx15-u1', 'title', 'Primeiro ato', '', '', -80.0, 15.25,
   '2026-01-14 09:01:00', '2026-01-14 09:01:00');

-- Aresta ligando uma entidade a um nó de canvas: é anotação, não relação canônica.
-- `source_kind`/`target_kind` só aceitam 'entity' e 'canvas'.
INSERT INTO canvas_edges (id, universe_id, source_kind, source_id, target_kind, target_id, label, created_at) VALUES
  ('fx15-ce1', 'fx15-u1', 'entity', 'fx15-e1', 'canvas', 'fx15-cn1', 'suspeita', '2026-01-14 09:02:00');
