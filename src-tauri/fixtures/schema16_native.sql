-- Fixture anônima e determinística de um banco **criado nativamente no schema 16**.
--
-- A v16 é a primeira migration que traz estrutura de Sync V2, e a fixture existe para
-- congelar as formas que só aparecem depois dela — nenhuma delas é produzível por um banco
-- que chegou ao 16 por migração, porque antes do 16 nada disso existia:
--
--   * um log com eventos de MAIS DE UMA origem no mesmo lugar, que é o que store-and-forward
--     produz e o que um outbox comum nunca produziria;
--   * uma LACUNA persistida: evento presente no log e ausente de `sync_applied_events`,
--     com o cursor daquela origem parado antes dele. É o caso da correção 4 do ADR 0009,
--     gravado em disco em vez de descrito em prosa;
--   * um dispositivo `retired` que ainda é origem de eventos históricos legítimos;
--   * uma divergência aberta, com as duas revisões preservadas e nenhuma escolhida.
--
-- Um banco real vai ter todas essas formas ao mesmo tempo. Se a fixture só tivesse o caminho
-- feliz, o dia em que a lacuna aparecesse em produção seria o primeiro dia em que ela é
-- testada.
--
-- Só dados fictícios.

-- ── Roster ──────────────────────────────────────────────────────────────────
-- Quatro aparelhos. Três ativos é o mínimo para store-and-forward significar alguma coisa:
-- com dois, todo evento chega direto e a retransmissão nunca é exercitada.
INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx16-dev-desk',  'Desktop do escritorio', 'fx16-ed-desk',  'fx16-x-desk',  'active',  '',              1, ''),
  ('fx16-dev-note',  'Notebook',              'fx16-ed-note',  'fx16-x-note',  'active',  '',              0, ''),
  -- Introduzido pelo Notebook, nunca pareado diretamente com este aparelho: é o caminho de
  -- introdução autorizada da seção 5.2, e o motivo de `introduced_by` existir.
  ('fx16-dev-droid', 'Android do Arthur',     'fx16-ed-droid', 'fx16-x-droid', 'active',  'fx16-dev-note', 0, ''),
  -- Aposentado, e ainda assim origem de eventos que continuam válidos. Apagar a linha
  -- quebraria a FK do log e reescreveria a história; é por isso que o estado existe em vez
  -- de o aparelho simplesmente sumir do roster.
  ('fx16-dev-velho', 'Tablet antigo',         'fx16-ed-velho', '',             'retired', '',              0, '2026-08-20 10:00:00');

-- ── Log ─────────────────────────────────────────────────────────────────────
-- Origem local: duas escritas deste aparelho.
INSERT INTO sync_events (event_id, device_id, seq, universe_id, aggregate_type, aggregate_id, operation, payload, base_rev, new_rev, signature, logged_at) VALUES
  ('fx16-e-001', 'fx16-dev-desk', 1, 'fx16-u1', 'universe',  'fx16-u1', 'upsert', '{"name":"Oficina dos Ventos"}', '',              'fx16-rev-u1-a', 'fx16-sig-001', '2026-08-25 09:00:00'),
  ('fx16-e-002', 'fx16-dev-desk', 2, 'fx16-u1', 'character', 'fx16-c1', 'upsert', '{"name":"Relojoeiro"}',         '',              'fx16-rev-c1-a', 'fx16-sig-002', '2026-08-25 09:05:00'),

-- Origem Notebook: três em sequência densa, todas aplicadas. É o caso normal, e serve de
-- contraste com o que vem depois.
  ('fx16-e-101', 'fx16-dev-note', 1, 'fx16-u1', 'character', 'fx16-c1', 'upsert', '{"name":"Relojoeiro Surdo"}',   'fx16-rev-c1-a', 'fx16-rev-c1-b', 'fx16-sig-101', '2026-08-26 11:00:00'),
  ('fx16-e-102', 'fx16-dev-note', 2, 'fx16-u1', 'location',  'fx16-l1', 'upsert', '{"name":"Torre do Relogio"}',   '',              'fx16-rev-l1-a', 'fx16-sig-102', '2026-08-26 11:02:00'),
  ('fx16-e-103', 'fx16-dev-note', 3, 'fx16-u1', 'location',  'fx16-l2', 'upsert', '{"name":"Praca Velha"}',        '',              'fx16-rev-l2-a', 'fx16-sig-103', '2026-08-26 11:04:00'),

-- Origem Android, seq 1: editou o MESMO personagem partindo da MESMA base que o Notebook
-- (`fx16-rev-c1-a`), offline. Duas revisões filhas da mesma revisão pai é a definição de
-- concorrência, e é o que `updated_at` não distingue de "uma é mais nova".
  ('fx16-e-201', 'fx16-dev-droid', 1, 'fx16-u1', 'character', 'fx16-c1', 'upsert', '{"name":"O Relojoeiro"}',      'fx16-rev-c1-a', 'fx16-rev-c1-x', 'fx16-sig-201', '2026-08-27 20:00:00'),

-- Origem Android, seq 3: chegou; o **seq 2 nunca chegou**. Repare que não existe linha para
-- o 2 — a lacuna é uma ausência, e é por isso que o cursor não pode ser calculado olhando
-- para o maior seq presente.
  ('fx16-e-203', 'fx16-dev-droid', 3, 'fx16-u1', 'planning',  'fx16-p1', 'upsert', '{"title":"Cena da ponte"}',    '',              'fx16-rev-p1-a', 'fx16-sig-203', '2026-08-27 20:10:00'),

-- Origem aposentada: um upsert antigo e o delete que o encerrou. Continuam no log porque o
-- log é imutável, e a exclusão precisa seguir se propagando para quem ainda não a viu.
  ('fx16-e-301', 'fx16-dev-velho', 1, 'fx16-u1', 'character', 'fx16-c9', 'upsert', '{"name":"Figurante"}',         '',              'fx16-rev-c9-a', 'fx16-sig-301', '2026-08-10 08:00:00'),
  ('fx16-e-302', 'fx16-dev-velho', 2, 'fx16-u1', 'character', 'fx16-c9', 'delete', '',                             'fx16-rev-c9-a', 'fx16-rev-c9-b', 'fx16-sig-302', '2026-08-11 08:00:00');

-- ── Aplicados ───────────────────────────────────────────────────────────────
-- Eventos locais entram aqui também: "aplicado" quer dizer "refletido nos agregados", e uma
-- escrita local está refletida por construção. Sem isso, todo evento local pareceria pendente.
INSERT INTO sync_applied_events (event_id, applied_at) VALUES
  ('fx16-e-001', '2026-08-25 09:00:00'),
  ('fx16-e-002', '2026-08-25 09:05:00'),
  ('fx16-e-101', '2026-08-26 11:30:00'),
  ('fx16-e-102', '2026-08-26 11:30:00'),
  ('fx16-e-103', '2026-08-26 11:30:00'),
  ('fx16-e-201', '2026-08-27 21:00:00'),
  ('fx16-e-301', '2026-08-10 08:30:00'),
  ('fx16-e-302', '2026-08-11 08:30:00');
-- `fx16-e-203` fica de fora de propósito: está no log e não está aplicado. É o pendente, e
-- ele sobrevive ao processo porque mora em disco, não em memória.

-- ── Cursores ────────────────────────────────────────────────────────────────
INSERT INTO sync_cursors (origin_device_id, last_seq_applied) VALUES
  ('fx16-dev-desk',  2),
  ('fx16-dev-note',  3),
  -- 1, e não 3. O seq 2 do Android nunca chegou; avançar até o 3 faria o 2 nunca mais ser
  -- pedido, e ninguém saberia que ele existiu.
  ('fx16-dev-droid', 1),
  ('fx16-dev-velho', 2);

-- ── Causalidade ─────────────────────────────────────────────────────────────
-- O personagem c1 continua em `fx16-rev-c1-b`: a divergência está aberta e nenhuma versão
-- foi escolhida. `fx16-p1` não aparece aqui porque o evento que o criaria está pendente.
INSERT INTO sync_aggregate_state (aggregate_type, aggregate_id, current_rev) VALUES
  ('universe',  'fx16-u1', 'fx16-rev-u1-a'),
  ('character', 'fx16-c1', 'fx16-rev-c1-b'),
  ('location',  'fx16-l1', 'fx16-rev-l1-a'),
  ('location',  'fx16-l2', 'fx16-rev-l2-a');

-- As duas revisões filhas de `fx16-rev-c1-a` estão registradas. É o que permite responder
-- "isto é sequencial ou concorrente?" sem consultar relógio nenhum.
INSERT INTO sync_revision_history (aggregate_type, aggregate_id, rev, base_rev, event_id) VALUES
  ('universe',  'fx16-u1', 'fx16-rev-u1-a', '',              'fx16-e-001'),
  ('character', 'fx16-c1', 'fx16-rev-c1-a', '',              'fx16-e-002'),
  ('character', 'fx16-c1', 'fx16-rev-c1-b', 'fx16-rev-c1-a', 'fx16-e-101'),
  ('character', 'fx16-c1', 'fx16-rev-c1-x', 'fx16-rev-c1-a', 'fx16-e-201'),
  ('location',  'fx16-l1', 'fx16-rev-l1-a', '',              'fx16-e-102'),
  ('location',  'fx16-l2', 'fx16-rev-l2-a', '',              'fx16-e-103'),
  ('character', 'fx16-c9', 'fx16-rev-c9-a', '',              'fx16-e-301'),
  ('character', 'fx16-c9', 'fx16-rev-c9-b', 'fx16-rev-c9-a', 'fx16-e-302');

-- ── Tombstone ───────────────────────────────────────────────────────────────
-- O agregado excluído não tem linha em `sync_aggregate_state`: sumiu. O que sobra é a marca
-- da exclusão, que é o que impede a ressurreição quando um peer atrasado reaparecer.
INSERT INTO sync_tombstones (aggregate_type, aggregate_id, deleted_rev, deleted_at) VALUES
  ('character', 'fx16-c9', 'fx16-rev-c9-b', '2026-08-11 08:30:00');

-- ── Divergência aberta ──────────────────────────────────────────────────────
INSERT INTO sync_divergences (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id, detected_at, resolved_at, resolution) VALUES
  ('fx16-div-1', 'character', 'fx16-c1', 'fx16-rev-c1-a', 'fx16-rev-c1-b', 'fx16-rev-c1-x', 'fx16-e-201', '2026-08-27 21:00:00', '', '');
