-- Fixture anônima e determinística de um banco **criado nativamente no schema 22**.
--
-- A v22 não cria tabela nem coluna: ela cria os dois gatilhos que fazem a aresta do canvas
-- morrer junto com a própria ponta, e apaga de uma vez as órfãs que o período sem gatilho
-- deixou. A forma que só um banco nativo no 22 tem é, por isso, uma **ausência**: nenhuma
-- aresta apontando para ponta que não existe.
--
-- O que esta fixture congela é o canvas completo, com as quatro combinações de ponta:
--
--   entity → entity     duas fichas ligadas
--   entity → canvas     ficha ligada a uma nota livre
--   canvas → entity     nota livre ligada a uma ficha
--   canvas → canvas     duas notas livres
--
-- Sem as quatro, um gatilho que cuidasse só de `source` (ou só de `target`) passaria verde.
-- Junto vem o conhecimento da mesma etapa — tag, marcação de tag e anexo —, porque é o
-- acervo que a B5 passa a sincronizar e que a gênese da etapa C vai ter de reproduzir.
--
-- Só dados fictícios.

INSERT INTO universes (id, name, description, cover_image, created_at, updated_at) VALUES
  ('fx22-uni', 'Arquipelago de Vento', 'Universo da fixture do schema 22', '', '2026-02-01 09:00:00', '2026-02-01 09:00:00');

-- ── Fichas ──────────────────────────────────────────────────────────────────
INSERT INTO entities (id, universe_id, type, name, created_at, updated_at) VALUES
  ('fx22-ent-a', 'fx22-uni', 'Personagem', 'Cartografa', '2026-02-01 09:10:00', '2026-02-01 09:10:00'),
  ('fx22-ent-b', 'fx22-uni', 'Local',      'Porto Sul',  '2026-02-01 09:11:00', '2026-02-01 09:11:00');

-- ── Elementos livres ────────────────────────────────────────────────────────
INSERT INTO canvas_nodes (id, universe_id, kind, text, image, color, position_x, position_y, created_at, updated_at) VALUES
  ('fx22-node-a', 'fx22-uni', 'note',  'Rota comercial antiga', '', '#7d3650', 120.5, -40.25, '2026-02-01 09:20:00', '2026-02-01 09:20:00'),
  ('fx22-node-b', 'fx22-uni', 'title', 'Primeiro ato',          '', '',          0.0,   0.0,  '2026-02-01 09:21:00', '2026-02-01 09:21:00');

-- Posição persistida de entidade: autoral, e agregado próprio desde a B3.
INSERT INTO canvas_entity_positions (universe_id, entity_id, position_x, position_y, updated_at) VALUES
  ('fx22-uni', 'fx22-ent-a', 310.0, 88.5, '2026-02-01 09:25:00');

-- ── As quatro combinações de ponta ──────────────────────────────────────────
INSERT INTO canvas_edges (id, universe_id, source_kind, source_id, target_kind, target_id, label, created_at) VALUES
  ('fx22-edge-ee', 'fx22-uni', 'entity', 'fx22-ent-a',  'entity', 'fx22-ent-b',  'mora em',     '2026-02-01 09:30:00'),
  ('fx22-edge-en', 'fx22-uni', 'entity', 'fx22-ent-a',  'canvas', 'fx22-node-a', 'percorreu',   '2026-02-01 09:31:00'),
  ('fx22-edge-ne', 'fx22-uni', 'canvas', 'fx22-node-a', 'entity', 'fx22-ent-b',  'termina em',  '2026-02-01 09:32:00'),
  ('fx22-edge-nn', 'fx22-uni', 'canvas', 'fx22-node-a', 'canvas', 'fx22-node-b', '',            '2026-02-01 09:33:00');

-- ── Conhecimento ────────────────────────────────────────────────────────────
INSERT INTO content_tags (id, universe_id, name, color, created_at) VALUES
  ('fx22-tag-rever', 'fx22-uni', 'Reescrever', '#7d3650', '2026-02-01 09:40:00'),
  ('fx22-tag-mar',   'fx22-uni', 'Mar',        '#2f6f7d', '2026-02-01 09:41:00');

-- A identidade causal da marcação é (tag, dono) desde a B2: o `id` da linha é local.
INSERT INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at) VALUES
  ('fx22-asg-1', 'fx22-tag-mar',   'entity',   'fx22-ent-b', '2026-02-01 09:45:00'),
  ('fx22-asg-2', 'fx22-tag-rever', 'universe', 'fx22-uni',   '2026-02-01 09:46:00');

-- ── Anexo no contrato de referência por hash (ADR 0010) ─────────────────────
INSERT INTO attachments (id, universe_id, owner_type, owner_id, data_url, blob_hash, mime_type, caption, sort_order, created_at) VALUES
  ('fx22-anexo-1', 'fx22-uni', 'entity', 'fx22-ent-a', '', '5f2b4a6c8d0e1f3a5b7c9d1e3f5a7b9c1d3e5f7a9b1c3d5e7f9a1b3c5d7e9f11', 'image/png', 'Retrato', 0, '2026-02-01 09:50:00'),
  ('fx22-anexo-2', 'fx22-uni', 'entity', 'fx22-ent-a', '', '6a3c5b7d9e1f2a4b6c8d0e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4d6e8f0a12', 'image/png', '',        1, '2026-02-01 09:51:00');
