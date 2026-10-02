-- Fixture anônima e determinística de um banco **criado nativamente no schema 29**.
--
-- A v29 acrescenta a caixa de recuperação do legado (etapa H, H-R3): o inventário do que o Sync V1
-- deixou pendente NESTE aparelho e ainda precisa de decisão do escritor.
--
-- O que esta fixture fixa como contrato:
--
--   * um item por conflito de origem — `source_conflict_id` é UNIQUE, e é isso que faz o import
--     do arranque ser idempotente (reabrir o aplicativo não cria pendência nova);
--   * os três estados possíveis, e só eles: `pending`, `preserved`, `discarded`;
--   * a evidência sobrevive sozinha: sem FK para conteúdo vivo, o item continua legível depois de
--     o capítulo original desaparecer;
--   * `preserved` guarda o capítulo NOVO que a preservação criou — nunca o id do capítulo original.
--
-- Só dados fictícios.

INSERT INTO sync_devices (device_id, name, ed25519_public, x25519_public, state, introduced_by, is_self, state_changed_at) VALUES
  ('fx29-dev-pc', 'Desktop', 'fx29-ed-pc', '', 'active', '', 1, '');

INSERT INTO sync_epoca (protocolo, device_id, origem, iniciada_em) VALUES
  (1, 'fx29-dev-pc', 'instalacao', '2026-09-22 12:00:00');

-- A linha histórica do V1 que deu origem ao item pendente. Ela continua imutável: a caixa copia,
-- nunca altera.
INSERT INTO sync_conflicts (id, aggregate_type, aggregate_id, field, local_value, remote_value, created_at, resolved_at) VALUES
  ('fx29-conflito-1', 'chapter', 'fx29-cap', 'content', '<p>a versão que ficou</p>', '<p>a versão do outro aparelho</p>', '2025-02-02 10:00:00', ''),
  ('fx29-conflito-2', 'chapter', 'fx29-cap-sumido', 'content', '<p>local</p>', '<p>remota</p>', '2025-03-03 10:00:00', '');

INSERT INTO legacy_recovery_items
  (id, source_conflict_id, aggregate_type, aggregate_id, field, local_value, remote_value,
   source_created_at, status, resolved_at, preserved_chapter_id) VALUES
  ('fx29-item-pendente', 'fx29-conflito-1', 'chapter', 'fx29-cap', 'content',
   '<p>a versão que ficou</p>', '<p>a versão do outro aparelho</p>',
   '2025-02-02 10:00:00', 'pending', '', ''),
  -- O capítulo original deste já não existe: a evidência histórica continua de pé sozinha.
  ('fx29-item-preservado', 'fx29-conflito-2', 'chapter', 'fx29-cap-sumido', 'content',
   '<p>local</p>', '<p>remota</p>',
   '2025-03-03 10:00:00', 'preserved', '2026-09-22 13:00:00', 'fx29-cap-recuperado'),
  ('fx29-item-descartado', 'fx29-conflito-3', 'chapter', 'fx29-cap', 'content',
   '<p>local</p>', '<p>remota</p>',
   '2025-04-04 10:00:00', 'discarded', '2026-09-22 13:30:00', '');
