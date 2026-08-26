-- Fixture anônima e determinística da última release pública (schema 10).
-- Ela contém somente dados fictícios e existe exclusivamente para testes de upgrade.
INSERT INTO universes (id, name, description) VALUES
  ('fixture-u1', 'Arquivo das Marés', 'Universo fictício para qualificação de migrations');
INSERT INTO stories (id, universe_id, name, description) VALUES
  ('fixture-s1', 'fixture-u1', 'A Cidade Submersa', 'História de teste');
INSERT INTO books (id, story_id, name, description, cover_image) VALUES
  ('fixture-b1', 'fixture-s1', 'Livro das Pontes', 'Livro de teste', 'data:image/png;base64,fixture');
INSERT INTO chapters (
  id, book_id, title, content, word_count, status, summary, scene_origin, scene_destination
) VALUES (
  'fixture-c1', 'fixture-b1', 'O farol', '<p>Lia atravessou a ponte ao amanhecer.</p>', 7, 'RASCUNHO',
  'Lia encontra o farol.', 'Porto Antigo', 'Farol de Sal'
);
INSERT INTO entities (id, universe_id, type, name, description, summary) VALUES
  ('fixture-e1', 'fixture-u1', 'Personagem', 'Lia', 'Cartógrafa das marés.', 'Protagonista'),
  ('fixture-e2', 'fixture-u1', 'Lugar', 'Farol de Sal', 'Farol construído sobre ruínas.', 'Destino da cena'),
  ('fixture-e3', 'fixture-u1', 'Evento', 'Maré Cinzenta', 'A maré que isolou a cidade.', 'Marco histórico');
INSERT INTO entity_attributes (id, entity_id, key, value, sort_order) VALUES
  ('fixture-a1', 'fixture-e1', 'Objetivo', 'Encontrar o mapa perdido', 0);
INSERT INTO entity_templates (id, universe_id, entity_type, attribute_key, default_value, sort_order) VALUES
  ('fixture-tpl1', 'fixture-u1', 'Personagem', 'Objetivo', '', 0);
INSERT INTO relations (id, universe_id, source_id, target_id, label) VALUES
  ('fixture-r1', 'fixture-u1', 'fixture-e1', 'fixture-e2', 'procura');
INSERT INTO mentions (id, chapter_id, entity_id) VALUES
  ('fixture-m1', 'fixture-c1', 'fixture-e1'),
  ('fixture-m2', 'fixture-c1', 'fixture-e2');
INSERT INTO timeline_events (
  id, universe_id, title, description, start_date, created_at, updated_at, entity_id, display_date, sort_key
) VALUES (
  'fixture-time1', 'fixture-u1', 'A Maré Cinzenta', 'A cidade perde contato com o continente.',
  '0000-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'fixture-e3', 'Ano 12', 12
);
INSERT INTO planning_items (
  id, universe_id, chapter_id, title, description, status, target_words, sort_order, created_at, updated_at
) VALUES (
  'fixture-p1', 'fixture-u1', 'fixture-c1', 'Revelar o mapa', 'Card editorial de teste',
  'PLANEJADO', 1200, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
);
INSERT INTO attachments (id, universe_id, owner_type, owner_id, data_url, caption, created_at) VALUES
  ('fixture-att1', 'fixture-u1', 'entity', 'fixture-e1', 'data:image/png;base64,fixture', 'Retrato fictício', '2026-01-01T00:00:00Z');
INSERT INTO content_tags (id, universe_id, name, color, created_at) VALUES
  ('fixture-tag1', 'fixture-u1', 'Mistério', '#7d3650', '2026-01-01T00:00:00Z');
INSERT INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at) VALUES
  ('fixture-ta1', 'fixture-tag1', 'chapter', 'fixture-c1', '2026-01-01T00:00:00Z'),
  ('fixture-ta2', 'fixture-tag1', 'timeline', 'fixture-time1', '2026-01-01T00:00:00Z'),
  ('fixture-ta3', 'fixture-tag1', 'planning', 'fixture-p1', '2026-01-01T00:00:00Z');
INSERT INTO devices (id, name, created_at, last_seen_at) VALUES
  ('fixture-d1', 'Dispositivo de teste', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO sync_events (id, universe_id, aggregate_type, aggregate_id, operation, payload, device_id) VALUES
  ('fixture-se1', 'fixture-u1', 'chapter', 'fixture-c1', 'update', '{"wordCount":7}', 'fixture-d1');
INSERT INTO collaboration_sessions (
  id, title, permission, universe_ids, encryption_key, revoke_token, status, created_at, expires_at
) VALUES (
  'fixture-collab1', 'Leitura fictícia', 'comment', '["fixture-u1"]', 'fixture-key', 'fixture-revoke',
  'ended', '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z'
);
INSERT INTO collaboration_contributions (
  id, session_id, sequence, contributor, kind, universe_id, target_type, target_id,
  target_label, message, status, created_at
) VALUES (
  'fixture-contrib1', 'fixture-collab1', 1, 'Leitor fictício', 'note', 'fixture-u1', 'chapter',
  'fixture-c1', 'O farol', 'Rever a ambientação.', 'noted', '2026-01-01T01:00:00Z'
);

