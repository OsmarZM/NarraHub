-- Fixture anônima e determinística de um banco **criado nativamente no schema 27**.
--
-- A v27 registra a **época causal do protocolo 1** (Sync V2, etapa E). A forma que só um banco
-- nativo no 27 tem é a linha de `sync_epoca`: o banco fala o protocolo 1. Esta fixture é a de um
-- aparelho que veio da beta e GIROU a época — o caso que carrega as três peças:
--
--   * `sync_epoca` com origem 'rotacao', a identidade anterior e as contagens do que foi arquivado;
--   * `sync_legado`: o passado pré-Hello, uma linha JSON por linha arquivada, com `protocolo = 0`;
--   * o roster só com o `self` novo — o pareamento da beta deixou de existir.
--
-- Um banco que chegou ao 27 por migração tem `sync_epoca` VAZIA: marcar a época é um ato do
-- arranque (instalação nova ou rotação), não uma coluna que a migration preenche.
--
-- Só dados fictícios.

INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx27-dev-novo', 'Celular', 'fx27-ed-novo', '', 'active', '', 1, '');

INSERT INTO sync_epoca (protocolo, device_id, origem, identidade_anterior, linhas_arquivadas, exclusoes_preservadas, iniciada_em) VALUES
  (1, 'fx27-dev-novo', 'rotacao', 'fx27-dev-beta', 3, 1, '2026-09-21 12:00:00');

INSERT INTO sync_legado (protocolo, tabela, linha, arquivada_em) VALUES
  (0, 'sync_devices', '{"device_id":"fx27-dev-beta","name":"","is_self":1,"state":"active"}', '2026-09-21 12:00:00'),
  (0, 'sync_events', '{"event_id":"fx27-ev-1","device_id":"fx27-dev-beta","seq":1,"aggregate_type":"attachment","operation":"delete","mutation_id":""}', '2026-09-21 12:00:00'),
  (0, 'sync_tombstones', '{"aggregate_type":"attachment","aggregate_id":"fx27-anexo","deleted_rev":"fx27-rev-del"}', '2026-09-21 12:00:00');
