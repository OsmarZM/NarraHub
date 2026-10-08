-- Fixture anônima e determinística de um banco **criado nativamente no schema 24**.
--
-- A v24 dá ao conflito uma identidade que **atravessa aparelhos**. A forma que só um banco nativo
-- no 24 tem é uma divergência que carrega, ao mesmo tempo:
--
--   identidade portátil   conflict_key, participant_a, participant_b  — iguais nos dois aparelhos
--   perspectiva local     local_rev, remote_rev                        — de quem detectou
--
-- Um banco que chegou ao 24 por migração tem as colunas novas **vazias** nas divergências antigas,
-- e é assim que tem de ser: a chave não pode ser inventada para trás sem as revisões reais dos dois
-- lados. Só um banco nativo tem divergência nascida com a identidade portátil preenchida.
--
-- As três formas que importam estão aqui:
--
--   1. `concurrent`               dois lados do MESMO agregado
--   2. `parent_deletion_blocked`  exclusão contra edição, também do mesmo agregado
--   3. `tag_name_conflict`        dois agregados DIFERENTES — o caso que derruba qualquer chave
--                                 feita de (aggregateType, aggregateId, revisões)
--
-- Os `participant_a`/`participant_b` estão na codificação canônica: cada campo precedido do seu
-- tamanho em bytes, na ordem (aggregateType, aggregateId, revision, operation). Ordenados entre si,
-- que é o que apaga a diferença de perspectiva.
--
-- A decisão do `parent_deletion_blocked` guarda também o `mutation_id` da ação que a gerou (v23):
-- as duas identidades convivem, e nenhuma delas substitui a outra. A identidade do GRUPO continua
-- sendo `(origem, mutation_id)` — o id sozinho não identifica ação nenhuma, e por isso o evento
-- âncora está aqui, com a origem dele.
--
-- O formato nativo dos grupos de mutação está em `schema23_native.sql`.
--
-- Só dados fictícios.

INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx24-dev-pc',      'Desktop',  'fx24-ed-pc',  'fx24-x-pc',  'active', '', 1, ''),
  ('fx24-dev-android', 'Celular',  'fx24-ed-an',  'fx24-x-an',  'active', '', 0, '');

INSERT INTO universes (id, name, description, cover_image, created_at, updated_at) VALUES
  ('fx24-uni', 'Arquipelago de Vento', '', '', '2026-03-01 09:00:00', '2026-03-01 09:00:00');

INSERT INTO content_tags (id, universe_id, name, color, created_at) VALUES
  ('fx24-tag-pc', 'fx24-uni', 'Mar', '#7d3650', '2026-03-01 10:00:00');

-- O evento âncora da decisão de exclusão: é dele que sai a origem do grupo.
INSERT INTO sync_events
  (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation, payload,
   base_rev, new_rev, signature, logged_at,
   mutation_id, mutation_index, mutation_count, mutation_kind, mutation_root_type, mutation_root_id)
VALUES
  ('fx24-evt-2', 'fx24-dev-android', 1, 'fx24-uni', 'entity', 'fx24-ent-1', 'delete', '',
   'fx24-rev-base2', 'fx24-rev-morta', 'fx24-sig-2', '2026-03-01 11:05:00',
   'fx24-m-entidade', 0, 1, 'delete_tree', 'entity', 'fx24-ent-1');

-- ── 1. Concorrente: mesmo agregado, duas revisões ───────────────────────────
INSERT INTO sync_divergences
  (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id,
   detected_at, resolved_at, resolution, local_operation, remote_operation, kind,
   related_aggregate_id, mutation_id, conflict_key, participant_a, participant_b)
VALUES
  ('fx24-div-conc', 'chapter', 'fx24-cap-1', 'fx24-rev-base',
   'fx24-rev-pc', 'fx24-rev-an', 'fx24-evt-1',
   '2026-03-01 11:00:00', '', '', 'upsert', 'upsert', 'concurrent', '', '',
   'fx24-chave-concorrente',
   '7:chapter10:fx24-cap-111:fx24-rev-an6:upsert',
   '7:chapter10:fx24-cap-111:fx24-rev-pc6:upsert');

-- ── 2. Exclusão de pai bloqueada: edição aqui contra exclusão que chegou ────
INSERT INTO sync_divergences
  (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id,
   detected_at, resolved_at, resolution, local_operation, remote_operation, kind,
   related_aggregate_id, mutation_id, conflict_key, participant_a, participant_b)
VALUES
  ('fx24-div-del', 'entity', 'fx24-ent-1', 'fx24-rev-base2',
   'fx24-rev-viva', 'fx24-rev-morta', 'fx24-evt-2',
   '2026-03-01 11:05:00', '', '', 'upsert', 'delete', 'parent_deletion_blocked', '',
   'fx24-m-entidade',
   'fx24-chave-exclusao',
   '6:entity10:fx24-ent-113:fx24-rev-viva6:upsert',
   '6:entity10:fx24-ent-114:fx24-rev-morta6:delete');

-- ── 3. Tag homônima: DOIS agregados diferentes ──────────────────────────────
--
-- Aqui `aggregate_id` é a tag que CHEGOU e `related_aggregate_id` é a tag daqui. No outro aparelho
-- os dois campos aparecem trocados — e mesmo assim `conflict_key` é a mesma, porque os
-- participantes são ordenados antes de entrar no hash.
INSERT INTO sync_divergences
  (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id,
   detected_at, resolved_at, resolution, local_operation, remote_operation, kind,
   related_aggregate_id, mutation_id, conflict_key, participant_a, participant_b)
VALUES
  ('fx24-div-tag', 'content_tag', 'fx24-tag-an', '',
   '', 'fx24-rev-tag-an', 'fx24-evt-3',
   '2026-03-01 11:10:00', '', '', '', 'upsert', 'tag_name_conflict', 'fx24-tag-pc', '',
   'fx24-chave-tag',
   '11:content_tag11:fx24-tag-an15:fx24-rev-tag-an6:upsert',
   '11:content_tag11:fx24-tag-pc15:fx24-rev-tag-pc6:upsert');
