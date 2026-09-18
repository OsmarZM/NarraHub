-- Fixture anônima e determinística de um banco **criado nativamente no schema 26**.
--
-- A v26 registra a **adoção versionada** do acervo: a gênese da etapa C. A forma que só um banco
-- nativo no 26 tem é a linha de `sync_adoptions` com a versão do formato canônico que já foi
-- adotada, ao lado dos eventos de gênese que ela produziu.
--
-- O que esta fixture fixa como contrato:
--
--   * a adoção é de uma VERSÃO do formato canônico (`canonical_format_version` é a chave);
--   * a linha só existe quando a adoção concluiu — não há estado "em andamento";
--   * o evento de gênese é um evento normal: `upsert`, `base_rev` vazio (a raiz), grupo de um
--     membro com `mutation_kind = 'genesis'`;
--   * a faixa `first_seq..last_seq` é contígua na sequência desta origem.
--
-- Um banco que chegou ao 26 por migração tem `sync_adoptions` VAZIA: adotar é um ato, não uma
-- coluna que a migration preenche.
--
-- Só dados fictícios.

INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx26-dev-pc', 'Desktop', 'fx26-ed-pc', 'fx26-x-pc', 'active', '', 1, '');

INSERT INTO universes (id, name, description, cover_image, created_at, updated_at) VALUES
  ('fx26-uni', 'Arquipelago de Vento', '', '', '2026-06-01 09:00:00', '2026-06-01 09:00:00');

INSERT INTO stories (id, universe_id, name, description, sort_order, created_at, updated_at) VALUES
  ('fx26-his', 'fx26-uni', 'A Travessia', '', 1, '2026-06-01 09:05:00', '2026-06-01 09:05:00');

-- ── Os três eventos da gênese: um por agregado adotado ──────────────────────
--
-- `base_rev` vazio é a raiz: criação. O grupo tem um membro só, e o `kind` diz de onde ele veio
-- sem mudar nada no caminho de aplicação — um receptor aplica isto como aplicaria qualquer criação.
INSERT INTO sync_events
  (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation, payload,
   base_rev, new_rev, signature, logged_at,
   mutation_id, mutation_index, mutation_count, mutation_kind, mutation_root_type, mutation_root_id)
VALUES
  ('fx26-e-01', 'fx26-dev-pc', 1, 'fx26-uni', 'universe', 'fx26-uni', 'upsert',
   '{"id":"fx26-uni","name":"Arquipelago de Vento","description":"","coverBlobHash":"","coverMimeType":"","customFields":[]}',
   '', 'fx26-rev-uni', 'fx26-sig-01', '2026-06-02 08:00:00',
   'fx26-m-01', 0, 1, 'genesis', '', ''),
  ('fx26-e-02', 'fx26-dev-pc', 2, 'fx26-uni', 'story', 'fx26-his', 'upsert',
   '{"id":"fx26-his","universeId":"fx26-uni","name":"A Travessia","description":"","customFields":[]}',
   '', 'fx26-rev-his', 'fx26-sig-02', '2026-06-02 08:00:00',
   'fx26-m-02', 0, 1, 'genesis', '', ''),
  ('fx26-e-03', 'fx26-dev-pc', 3, 'fx26-uni', 'story_position', 'fx26-his', 'upsert',
   '{"storyId":"fx26-his","universeId":"fx26-uni","sortOrder":1}',
   '', 'fx26-rev-pos', 'fx26-sig-03', '2026-06-02 08:00:00',
   'fx26-m-03', 0, 1, 'genesis', '', '');

INSERT INTO sync_applied_events (event_id) VALUES ('fx26-e-01'), ('fx26-e-02'), ('fx26-e-03');

INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev) VALUES
  ('universe', 'fx26-uni', 'fx26-rev-uni'),
  ('story', 'fx26-his', 'fx26-rev-his'),
  ('story_position', 'fx26-his', 'fx26-rev-pos');

INSERT INTO sync_revision_history (aggregate_type, aggregate_id, rev, base_rev, event_id) VALUES
  ('universe', 'fx26-uni', 'fx26-rev-uni', '', 'fx26-e-01'),
  ('story', 'fx26-his', 'fx26-rev-his', '', 'fx26-e-02'),
  ('story_position', 'fx26-his', 'fx26-rev-pos', '', 'fx26-e-03');

-- ── A adoção concluída ──────────────────────────────────────────────────────
INSERT INTO sync_adoptions
  (canonical_format_version, device_id, completed_at, aggregates, first_seq, last_seq)
VALUES
  (1, 'fx26-dev-pc', '2026-06-02 08:00:00', 3, 1, 3);
