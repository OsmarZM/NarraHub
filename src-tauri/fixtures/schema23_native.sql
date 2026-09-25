-- Fixture anônima e determinística de um banco **criado nativamente no schema 23**.
--
-- A v23 dá a cada evento a AÇÃO de onde ele saiu: todo evento de uma mesma `Mutacao` carrega o
-- mesmo `mutation_id`, com índices contíguos `0..count`. As formas que só um banco nativo no 23 tem:
--
--   * uma ação de VÁRIOS eventos (`delete_tree`) com a raiz registrada — a exclusão de um livro,
--     com as posições e os capítulos como membros, em ordem;
--   * uma ação de UM evento com `mutation_id` preenchido — a forma que toda escrita local passa a
--     ter, mesmo quando não é composta;
--   * um evento anterior à v23, sem grupo — que um banco migrado carrega e o nativo pode ter recebido
--     de um aparelho antigo: grupo de um, e a assinatura dele não muda;
--   * uma decisão de ação: a divergência ancorada num membro, com o `mutation_id` da ação inteira.
--
-- Só dados fictícios.

INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx23-dev-pc',      'Desktop', 'fx23-ed-pc', 'fx23-x-pc', 'active', '', 1, ''),
  ('fx23-dev-android', 'Celular', 'fx23-ed-an', 'fx23-x-an', 'active', '', 0, '');

-- ── Uma exclusão composta: o livro e tudo o que sai com ele ─────────────────
INSERT INTO sync_events
  (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation, payload,
   base_rev, new_rev, signature, logged_at,
   mutation_id, mutation_index, mutation_count, mutation_kind, mutation_root_type, mutation_root_id)
VALUES
  ('fx23-e-01', 'fx23-dev-pc', 1, 'fx23-u1', 'chapter_position', 'fx23-c1', 'delete', '', 'fx23-rev-pc1', 'fx23-rev-pc1-del', 'fx23-sig-01', '2026-09-01 10:00:00',
   'fx23-m-livro', 0, 4, 'delete_tree', 'book', 'fx23-b1'),
  ('fx23-e-02', 'fx23-dev-pc', 2, 'fx23-u1', 'chapter',          'fx23-c1', 'delete', '', 'fx23-rev-c1',  'fx23-rev-c1-del',  'fx23-sig-02', '2026-09-01 10:00:00',
   'fx23-m-livro', 1, 4, 'delete_tree', 'book', 'fx23-b1'),
  ('fx23-e-03', 'fx23-dev-pc', 3, 'fx23-u1', 'book_position',    'fx23-b1', 'delete', '', 'fx23-rev-pb1', 'fx23-rev-pb1-del', 'fx23-sig-03', '2026-09-01 10:00:00',
   'fx23-m-livro', 2, 4, 'delete_tree', 'book', 'fx23-b1'),
  ('fx23-e-04', 'fx23-dev-pc', 4, 'fx23-u1', 'book',             'fx23-b1', 'delete', '', 'fx23-rev-b1',  'fx23-rev-b1-del',  'fx23-sig-04', '2026-09-01 10:00:00',
   'fx23-m-livro', 3, 4, 'delete_tree', 'book', 'fx23-b1');

-- ── Uma ação de um evento só, já no formato novo ─────────────────────────────
INSERT INTO sync_events
  (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation, payload,
   base_rev, new_rev, signature, logged_at,
   mutation_id, mutation_index, mutation_count, mutation_kind, mutation_root_type, mutation_root_id)
VALUES
  ('fx23-e-05', 'fx23-dev-pc', 5, 'fx23-u1', 'universe', 'fx23-u1', 'upsert', '{"id":"fx23-u1"}', 'fx23-rev-u1', 'fx23-rev-u1-b', 'fx23-sig-05', '2026-09-01 10:05:00',
   'fx23-m-titulo', 0, 1, '', '', '');

-- ── Evento de um aparelho anterior à v23: sem grupo ──────────────────────────
INSERT INTO sync_events
  (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation, payload,
   base_rev, new_rev, signature, logged_at)
VALUES
  ('fx23-e-a1', 'fx23-dev-android', 1, 'fx23-u1', 'universe', 'fx23-u1', 'upsert', '{"id":"fx23-u1"}', 'fx23-rev-u1', 'fx23-rev-u1-an', 'fx23-sig-a1', '2026-09-01 10:06:00');

-- ── A decisão de uma ação ────────────────────────────────────────────────────
INSERT INTO sync_divergences
  (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id,
   detected_at, resolved_at, resolution, local_operation, remote_operation, kind, related_aggregate_id,
   mutation_id)
VALUES
  ('fx23-div-livro', 'book', 'fx23-b1', 'fx23-rev-b1', 'fx23-rev-b1', 'fx23-rev-b1-del', 'fx23-e-04',
   '2026-09-01 10:10:00', '', '', 'upsert', 'delete', 'parent_deletion_blocked', '',
   'fx23-m-livro');
