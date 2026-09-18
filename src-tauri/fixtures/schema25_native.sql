-- Fixture anônima e determinística de um banco **criado nativamente no schema 25**.
--
-- A v25 faz da identidade do agregado `canvas_entity_position` um invariante do banco: uma entidade
-- tem no máximo uma posição no grafo, e o índice único cobra isso. Antes dela, a
-- `PRIMARY KEY (universe_id, entity_id)` deixava passar duas linhas da mesma entidade em universos
-- diferentes — que o codec já recusava a ler, mas que o banco aceitava guardar.
--
-- A forma que só um banco nativo no 25 tem:
--
--   * duas entidades do mesmo universo, cada uma com a SUA posição — o caso normal, que o índice
--     único precisa continuar aceitando;
--   * uma entidade sem posição nenhuma — a posição é opcional, e o índice não pode exigi-la;
--   * a tabela de quarentena da migration existindo e VAZIA: um banco nativo nunca teve duplicata
--     para descartar.
--
-- Só dados fictícios.

INSERT INTO universes (id, name, description, cover_image, created_at, updated_at) VALUES
  ('fx25-uni-a', 'Arquipelago de Vento', '', '', '2026-05-01 09:00:00', '2026-05-01 09:00:00'),
  ('fx25-uni-b', 'Terras de Brasa',      '', '', '2026-05-01 09:00:00', '2026-05-01 09:00:00');

INSERT INTO entities (id, universe_id, name, type, description, image, created_at, updated_at) VALUES
  ('fx25-ent-1', 'fx25-uni-a', 'Maré',    'character', '', '', '2026-05-01 10:00:00', '2026-05-01 10:00:00'),
  ('fx25-ent-2', 'fx25-uni-a', 'Farol',   'location',  '', '', '2026-05-01 10:05:00', '2026-05-01 10:05:00'),
  ('fx25-ent-3', 'fx25-uni-b', 'Fornalha','location',  '', '', '2026-05-01 10:10:00', '2026-05-01 10:10:00');

-- Uma posição por entidade. `fx25-ent-3` fica de fora de propósito: posição é opcional.
INSERT INTO canvas_entity_positions (universe_id, entity_id, position_x, position_y, updated_at) VALUES
  ('fx25-uni-a', 'fx25-ent-1', 120.5, -40.25, '2026-05-01 11:00:00'),
  ('fx25-uni-a', 'fx25-ent-2', 300.0,  80.0,  '2026-05-01 11:05:00');
