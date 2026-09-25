-- Fixture anônima e determinística de um banco **criado nativamente no schema 28**.
--
-- A v28 dá à decisão sobre um conflito a forma de fato causal (Sync V2, etapa F): o agregado
-- `conflict_resolution/<conflictKey>`, cujo estado materializado é a linha de `conflict_resolutions`
-- com o certificado canônico. O índice local (`sync_divergences`) passa a apontar para a revisão
-- dessa decisão em `resolution_rev`.
--
-- O que esta fixture fixa como contrato:
--
--   * uma decisão por conflito: `conflict_key` é a chave primária;
--   * o certificado é texto canônico (sem relógio, sem aparelho) — é ele que o evento carrega;
--   * a divergência fechada por uma decisão aponta para ela; uma fechada antes da F tem o campo vazio.
--
-- Só dados fictícios.

INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx28-dev-pc', 'Desktop', 'fx28-ed-pc', '', 'active', '', 1, '');

INSERT INTO sync_epoca (protocolo, device_id, origem, iniciada_em) VALUES
  (1, 'fx28-dev-pc', 'instalacao', '2026-09-21 12:00:00');

INSERT INTO conflict_resolutions (conflict_key, universe_id, kind, certificate) VALUES
  ('fx28-chave', 'fx28-uni', 'concurrent',
   '{"conflictKey":"fx28-chave","kind":"concurrent","participantA":{"aggregateType":"chapter","aggregateId":"fx28-cap","revision":"fx28-r1","operation":"upsert"},"participantB":{"aggregateType":"chapter","aggregateId":"fx28-cap","revision":"fx28-r2","operation":"upsert"},"choice":"a","results":[{"aggregateType":"chapter","aggregateId":"fx28-cap","operation":"upsert","baseRev":"fx28-r1","otherRev":"fx28-r2","resultRev":"fx28-r3"}]}');

INSERT INTO sync_divergences
  (id, aggregate_type, aggregate_id, base_rev, local_rev, remote_rev, remote_event_id,
   detected_at, resolved_at, resolution, local_operation, remote_operation, kind,
   conflict_key, participant_a, participant_b, resolution_rev) VALUES
  ('fx28-div', 'chapter', 'fx28-cap', 'fx28-r0', 'fx28-r1', 'fx28-r2', 'fx28-ev',
   '2026-09-21 11:00:00', '2026-09-21 12:00:00', 'local', 'upsert', 'upsert', 'concurrent',
   'fx28-chave', '7:chapter8:fx28-cap7:fx28-r16:upsert', '7:chapter8:fx28-cap7:fx28-r26:upsert',
   'fx28-rev-da-decisao'),
  ('fx28-antiga', 'chapter', 'fx28-cap2', 'fx28-r0', 'fx28-r1', 'fx28-r2', 'fx28-ev2',
   '2026-09-01 11:00:00', '2026-09-01 12:00:00', 'local', 'upsert', 'upsert', 'concurrent',
   '', '', '', '');
