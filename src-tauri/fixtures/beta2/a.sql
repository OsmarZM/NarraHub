-- Banco REAL da 0.10.0-beta.2 (app-v0.10.0-beta.2, ccf2db2), schema 20 — o aparelho A: criou o acervo, editou, apagou, recebeu a edição de B.
-- Gerado pelo código da beta.2 num worktree separado (etapa E, E0-beta) e despejado com
-- `sqlite3.Connection.iterdump`. NÃO editar à mão: é a evidência de como a beta deixava o banco.
-- Cenário e ids em `ids.txt`; identidade e blobs em `dados-a/`.

BEGIN TRANSACTION;
CREATE TABLE attachments (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    owner_type TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    data_url TEXT NOT NULL,
    caption TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL, blob_hash TEXT NOT NULL DEFAULT '', mime_type TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
INSERT INTO "attachments" VALUES('461383e0-b1e3-4071-a840-00cb46b5001d','9fce1c54-f5d8-4aa1-8403-f82984fcd492','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','','fica',0,'2026-09-21 16:50:40','ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb','image/png');
CREATE TABLE blob_migration_issues (
    id TEXT PRIMARY KEY NOT NULL,
    -- O numero da superficie no ADR 0010, para que documento e banco falem a
    -- mesma lingua.
    surface INTEGER NOT NULL CHECK (surface BETWEEN 1 AND 10),
    table_name TEXT NOT NULL CHECK (table_name <> ''),
    row_id TEXT NOT NULL CHECK (row_id <> ''),
    -- A coluna, que nas superficies de dois lados identifica QUAL lado falhou.
    -- Falha num lado nao autoriza tocar no outro.
    side TEXT NOT NULL CHECK (side <> ''),
    reason TEXT NOT NULL CHECK (reason IN (
        'legacy_unrecognized',
        'document_unparseable',
        'node_unrecognized',
        'blob_write_failed',
        'blob_missing'
    )),
    -- Diagnostico curto, para o escritor entender o que ficou pendente.
    --
    -- O limite de tamanho e uma regra, nao um palpite: a issue NAO guarda os
    -- bytes. Sem o CHECK, o caminho mais natural do mundo -- "grava o valor
    -- que nao deu para converter, para nao perder" -- traria a base64 inteira
    -- de volta para dentro do SQLite, que e exatamente o problema que esta
    -- etapa existe para resolver. O valor legado continua onde sempre esteve.
    detail TEXT NOT NULL DEFAULT '' CHECK (length(detail) <= 200),
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT NOT NULL DEFAULT '',
    -- A identidade da pendencia e (superficie, linha, lado, motivo).
    --
    -- E o que torna o registro idempotente por ESTRUTURA, e nao por disciplina
    -- de quem escreve o backfill: rodar o backfill dez vezes sobre o mesmo
    -- legado invalido nao pode produzir dez pendencias.
    UNIQUE (surface, row_id, side, reason)
);
CREATE TABLE books (
    id TEXT PRIMARY KEY NOT NULL,
    story_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')), cover_image TEXT NOT NULL DEFAULT '', cover_blob_hash TEXT NOT NULL DEFAULT '', cover_mime_type TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (story_id) REFERENCES stories(id) ON DELETE CASCADE
);
INSERT INTO "books" VALUES('bc759499-8ea2-4e92-9cf8-caf926d35bd8','2eea10c0-3822-41f0-94aa-508d0a1b2353','Livro','',0,'2026-09-21 16:50:39','2026-09-21 16:50:39','','','');
CREATE TABLE canvas_edges (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE,
    CHECK (source_kind IN ('entity', 'canvas')),
    CHECK (target_kind IN ('entity', 'canvas'))
);
CREATE TABLE canvas_entity_positions (
    universe_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    position_x REAL NOT NULL,
    position_y REAL NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (universe_id, entity_id),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE TABLE canvas_nodes (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'note',
    text TEXT NOT NULL DEFAULT '',
    image TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '',
    position_x REAL NOT NULL DEFAULT 0,
    position_y REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')), image_blob_hash TEXT NOT NULL DEFAULT '', image_mime_type TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE,
    CHECK (kind IN ('title', 'image', 'note'))
);
CREATE TABLE change_log (
    id TEXT PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    field TEXT NOT NULL DEFAULT '',
    old_value TEXT NOT NULL DEFAULT '',
    new_value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
, universe_id TEXT NOT NULL DEFAULT '');
INSERT INTO "change_log" VALUES('5d9128a101956b8b11d6528850192686','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','create','','','C1','2026-09-21 16:50:39','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('a71bf2eb56779619e37ce86f7827a7ae','chapter','0a083133-8c45-460f-8a49-68e589318d39','create','','','C2','2026-09-21 16:50:39','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('2467a2327de4305dd173c8a1c168f4c2','chapter','4751d6ce-b56a-40cd-bea7-098ae24ee6d9','create','','','C3','2026-09-21 16:50:39','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('fed0cab5acaa189b728b3dc5b984c560','chapter','9657a65a-e58c-4a81-8ebd-10f15c66f5a0','create','','','C4-nunca-editado','2026-09-21 16:50:40','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('474232c9fd7e7956fd305243dad79bfb','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','update','content','0','2','2026-09-21 16:50:40','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('fab3f6781a8b09c552ff8ed377158101','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','update','content','2','2','2026-09-21 16:50:40','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('da723ca8ab490e81c24fa7f305711105','chapter','4751d6ce-b56a-40cd-bea7-098ae24ee6d9','update','content','0','4','2026-09-21 16:50:40','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('6792d42585f8d8cc4bdbfb67cc17364a','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','update','content','2','2','2026-09-21 16:50:42','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
INSERT INTO "change_log" VALUES('37c79430f49b9d9b6b8cac2324c0c395','chapter','0a083133-8c45-460f-8a49-68e589318d39','update','content','0','4','2026-09-21 16:50:42','9fce1c54-f5d8-4aa1-8403-f82984fcd492');
CREATE TABLE chapter_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    chapter_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE
);
INSERT INTO "chapter_revisions" VALUES('3af4da85c73ed6c035aee0d0d9c8647a','4f7f4952-6821-4749-9b6c-0be2e8de5712','C1','',0,'2026-09-21 16:50:40');
INSERT INTO "chapter_revisions" VALUES('c7d6cb9b170bb5f913cf3a6c8fe86e08','4f7f4952-6821-4749-9b6c-0be2e8de5712','C1','<p>C1 primeira</p>',2,'2026-09-21 16:50:40');
INSERT INTO "chapter_revisions" VALUES('132c2cbe68d643d3fa762a6b015982a6','4f7f4952-6821-4749-9b6c-0be2e8de5712','C1','<p>C1 segunda</p>',2,'2026-09-21 16:50:42');
INSERT INTO "chapter_revisions" VALUES('dc4acbf29da351148966d0bf8f011afb','0a083133-8c45-460f-8a49-68e589318d39','C2','',0,'2026-09-21 16:50:42');
CREATE TABLE chapters (
    id TEXT PRIMARY KEY NOT NULL,
    book_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    word_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'IDEIA',
    canon_status TEXT NOT NULL DEFAULT 'CANON',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')), summary TEXT NOT NULL DEFAULT '', scene_origin TEXT NOT NULL DEFAULT '', scene_destination TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);
INSERT INTO "chapters" VALUES('4f7f4952-6821-4749-9b6c-0be2e8de5712','bc759499-8ea2-4e92-9cf8-caf926d35bd8','C1','<p>C1 terceira</p>',2,'IDEIA','CANON',0,'2026-09-21 16:50:39','2026-09-21 16:50:42','','','');
INSERT INTO "chapters" VALUES('0a083133-8c45-460f-8a49-68e589318d39','bc759499-8ea2-4e92-9cf8-caf926d35bd8','C2','<p>C2 escrito no celular</p>',4,'IDEIA','CANON',1,'2026-09-21 16:50:39','2026-09-21 16:50:42','','','');
INSERT INTO "chapters" VALUES('9657a65a-e58c-4a81-8ebd-10f15c66f5a0','bc759499-8ea2-4e92-9cf8-caf926d35bd8','C4-nunca-editado','',0,'IDEIA','CANON',3,'2026-09-21 16:50:40','2026-09-21 16:50:40','','','');
CREATE TABLE collaboration_contributions (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    contributor TEXT NOT NULL DEFAULT 'Convidado',
    kind TEXT NOT NULL CHECK(kind IN ('edit','note')),
    universe_id TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK(target_type IN ('universe','chapter','entity')),
    target_id TEXT NOT NULL,
    target_label TEXT NOT NULL,
    field TEXT NOT NULL DEFAULT '',
    original_value TEXT NOT NULL DEFAULT '',
    proposed_value TEXT NOT NULL DEFAULT '',
    message TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','noted')),
    created_at TEXT NOT NULL,
    reviewed_at TEXT,
    UNIQUE(session_id, sequence),
    FOREIGN KEY (session_id) REFERENCES collaboration_sessions(id) ON DELETE CASCADE
);
CREATE TABLE collaboration_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    permission TEXT NOT NULL CHECK(permission IN ('view','comment','edit')),
    universe_ids TEXT NOT NULL DEFAULT '[]',
    encryption_key TEXT NOT NULL,
    revoke_token TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','ended','revoked')),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    ended_at TEXT
);
CREATE TABLE content_custom_fields (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    owner_type TEXT NOT NULL CHECK(owner_type IN ('universe','story','book','chapter','entity')),
    owner_id TEXT NOT NULL,
    key TEXT NOT NULL COLLATE NOCASE,
    value TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(owner_type, owner_id, key),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
CREATE TABLE "content_tag_assignments" (
    id TEXT PRIMARY KEY NOT NULL,
    tag_id TEXT NOT NULL,
    owner_type TEXT NOT NULL CHECK(owner_type IN ('universe','story','book','chapter','entity','timeline','planning')),
    owner_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(tag_id, owner_type, owner_id),
    FOREIGN KEY (tag_id) REFERENCES content_tags(id) ON DELETE CASCADE
);
CREATE TABLE content_tags (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE,
    color TEXT NOT NULL DEFAULT '#7d3650',
    created_at TEXT NOT NULL,
    UNIQUE(universe_id, name),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
CREATE TABLE devices (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);
CREATE TABLE entities (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    image TEXT NOT NULL DEFAULT '',
    canon_status TEXT NOT NULL DEFAULT 'CANON',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')), summary TEXT NOT NULL DEFAULT '', image_blob_hash TEXT NOT NULL DEFAULT '', image_mime_type TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
CREATE TABLE entity_attributes (
    id TEXT PRIMARY KEY NOT NULL,
    entity_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE TABLE entity_templates (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    attribute_key TEXT NOT NULL,
    default_value TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
CREATE TABLE mentions (
    id TEXT PRIMARY KEY NOT NULL,
    chapter_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE TABLE planning_field_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE,
    field_type TEXT NOT NULL CHECK(field_type IN (
        'text','long_text','number','checkbox','yes_no','select','multi_select','tags','story','character'
    )),
    options_json TEXT NOT NULL DEFAULT '[]'
        CHECK(json_valid(options_json) AND json_type(options_json) = 'array'),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, scope TEXT NOT NULL DEFAULT 'universal'
    CHECK(scope IN ('universal', 'card')), owner_item_id TEXT
    REFERENCES planning_items(id) ON DELETE CASCADE,
    UNIQUE(universe_id, name),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
CREATE TABLE planning_field_links (
    id TEXT PRIMARY KEY NOT NULL,
    planning_item_id TEXT NOT NULL,
    field_definition_id TEXT NOT NULL,
    story_id TEXT,
    entity_id TEXT,
    tag_id TEXT,
    created_at TEXT NOT NULL,
    CHECK (
        (story_id IS NOT NULL) + (entity_id IS NOT NULL) + (tag_id IS NOT NULL) = 1
    ),
    FOREIGN KEY (planning_item_id) REFERENCES planning_items(id) ON DELETE CASCADE,
    FOREIGN KEY (field_definition_id) REFERENCES planning_field_definitions(id) ON DELETE CASCADE,
    FOREIGN KEY (story_id) REFERENCES stories(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES content_tags(id) ON DELETE CASCADE
);
CREATE TABLE planning_items (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    chapter_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'IDEIAS' CHECK(status IN ('IDEIAS','PLANEJADO','ESCREVENDO','REVISAO','FINALIZADO')),
    target_words INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, image TEXT NOT NULL DEFAULT '', custom_field_values TEXT NOT NULL DEFAULT '{}'
    CHECK(json_valid(custom_field_values) AND json_type(custom_field_values) = 'object'), image_blob_hash TEXT NOT NULL DEFAULT '', image_mime_type TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE SET NULL
);
CREATE TABLE relations (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'custom',
    label TEXT NOT NULL,
    bidirectional INTEGER NOT NULL DEFAULT 0,
    importance TEXT NOT NULL DEFAULT 'normal',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE,
    FOREIGN KEY (source_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES entities(id) ON DELETE CASCADE
);
CREATE TABLE stories (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
INSERT INTO "stories" VALUES('2eea10c0-3822-41f0-94aa-508d0a1b2353','9fce1c54-f5d8-4aa1-8403-f82984fcd492','Saga','',0,'2026-09-21 16:50:39','2026-09-21 16:50:39');
CREATE TABLE sync_aggregate_state (
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    current_rev TEXT NOT NULL,
    PRIMARY KEY (aggregate_type, aggregate_id)
);
INSERT INTO "sync_aggregate_state" VALUES('chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','4a078c1f4b3296ae45e3d0d7055b68f1e763e7e962662d46538d549a3ab23d24');
INSERT INTO "sync_aggregate_state" VALUES('chapter','4751d6ce-b56a-40cd-bea7-098ae24ee6d9','ab0d89175660a68aaf0fcc973dfa26dfee9c34f96f2a2cefe0252a9fd88bb16d');
INSERT INTO "sync_aggregate_state" VALUES('attachment','461383e0-b1e3-4071-a840-00cb46b5001d','8a016be5f9ec40e9d406f18ecba0253b0a10acbf490a136b0708f500380cbd94');
INSERT INTO "sync_aggregate_state" VALUES('chapter','0a083133-8c45-460f-8a49-68e589318d39','cde9e7155147bc9e66cfc3d278b6263800e34756ac60115001009ef3416e819d');
CREATE TABLE sync_applied_events (
    event_id TEXT PRIMARY KEY NOT NULL REFERENCES sync_events(event_id),
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO "sync_applied_events" VALUES('7f78ea57-6c00-4142-b188-37092c17296a','2026-09-21 16:50:40');
INSERT INTO "sync_applied_events" VALUES('f5c06b3b-91b6-43cf-9262-83de72bb284a','2026-09-21 16:50:40');
INSERT INTO "sync_applied_events" VALUES('1432daa0-5b9d-4cca-a286-c9befe4620a9','2026-09-21 16:50:40');
INSERT INTO "sync_applied_events" VALUES('745718a4-7b60-4360-a20a-94671f832850','2026-09-21 16:50:41');
INSERT INTO "sync_applied_events" VALUES('4e82e203-8c4d-4681-92da-a132c9e81477','2026-09-21 16:50:41');
INSERT INTO "sync_applied_events" VALUES('bc9d441a-9841-44d6-8e3d-6e75f304ce05','2026-09-21 16:50:41');
INSERT INTO "sync_applied_events" VALUES('1ce95c2c-473c-499d-8113-57991555ad45','2026-09-21 16:50:42');
INSERT INTO "sync_applied_events" VALUES('0c09f9d3-e77e-4651-b494-aa5fcf940222','2026-09-21 16:50:42');
CREATE TABLE sync_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    field TEXT NOT NULL,
    local_value TEXT NOT NULL,
    remote_value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE sync_cursors (
    origin_device_id TEXT PRIMARY KEY NOT NULL REFERENCES sync_devices(device_id),
    -- Ate onde o conteudo daquela origem chegou por SNAPSHOT, sem passar pelo
    -- log. Zero quando o pareamento foi comum: nesse caso tudo veio por evento.
    -- Vem do vetor de sequencias lido na mesma transacao do snapshot (ADR 0009
    -- secao 14), e por isso e exatamente o ponto a partir do qual o incremental
    -- comeca a valer.
    baseline_seq INTEGER NOT NULL DEFAULT 0 CHECK (baseline_seq >= 0),
    last_seq_applied INTEGER NOT NULL DEFAULT 0 CHECK (last_seq_applied >= 0),
    -- O cursor nunca fica atras da semente: o conteudo ate ali ja esta no banco.
    CHECK (last_seq_applied >= baseline_seq)
);
INSERT INTO "sync_cursors" VALUES('T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',0,7);
INSERT INTO "sync_cursors" VALUES('QOQELGFBXMU275NTH35UDZ4DZFEG6S2F',0,1);
CREATE TABLE sync_devices (
    device_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    ed25519_public TEXT NOT NULL,
    x25519_public TEXT NOT NULL DEFAULT '',
    -- retired destrava a poda futura de tombstones; revoked sinaliza para
    -- revisao humana sem apagar o que foi feito antes do comprometimento.
    state TEXT NOT NULL DEFAULT 'active'
        CHECK (state IN ('active', 'retired', 'revoked')),
    -- device_id de quem introduziu no roster. Vazio = pareamento direto.
    introduced_by TEXT NOT NULL DEFAULT '',
    -- este aparelho, entre os do roster. No maximo uma linha pode ter 1.
    is_self INTEGER NOT NULL DEFAULT 0 CHECK (is_self IN (0, 1)),
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    state_changed_at TEXT NOT NULL DEFAULT ''
, exit_reason TEXT NOT NULL DEFAULT ''
    CHECK (exit_reason IN ('', 'clean', 'abandoned')));
INSERT INTO "sync_devices" VALUES('T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM','','2IZJGPROSQHXEW4PTSAHUZW3JO2HTJ2LZPZZOHTX3KFZVHZS3FJA','','active','',1,'2026-09-21 16:50:37','','');
INSERT INTO "sync_devices" VALUES('QOQELGFBXMU275NTH35UDZ4DZFEG6S2F','b','A35Q3FUI3ARMM276MZ4OKQL3FY4SYBXI73DSHZA6HQFCJNBBFYGA','','active','',0,'2026-09-21 16:50:41','','');
CREATE TABLE sync_divergences (
    id TEXT PRIMARY KEY NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    base_rev TEXT NOT NULL,
    local_rev TEXT NOT NULL,
    remote_rev TEXT NOT NULL,
    remote_event_id TEXT NOT NULL,
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT NOT NULL DEFAULT '',
    resolution TEXT NOT NULL DEFAULT ''
        CHECK (resolution IN ('', 'local', 'remote', 'manual'))
, local_operation TEXT NOT NULL DEFAULT ''
    CHECK (local_operation IN ('', 'upsert', 'delete')), remote_operation TEXT NOT NULL DEFAULT ''
    CHECK (remote_operation IN ('', 'upsert', 'delete')));
CREATE TABLE sync_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    -- ORIGEM do evento, nao quem entregou. O relay nao se sobrescreve aqui.
    device_id TEXT NOT NULL REFERENCES sync_devices(device_id),
    seq INTEGER NOT NULL CHECK (seq > 0),
    -- sem FK para universes: um evento pode ser guardado por um relay que nem
    -- conhece aquele universo, e o evento que CRIA o universo chegaria antes
    -- da linha que ele referencia.
    universe_id TEXT NOT NULL DEFAULT '',
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    payload TEXT NOT NULL DEFAULT '',
    -- vazio = primeira revisao do agregado. Nao e "desconhecido".
    base_rev TEXT NOT NULL DEFAULT '',
    new_rev TEXT NOT NULL,
    -- Ed25519 da ORIGEM sobre a representacao canonica. Sem DEFAULT de
    -- proposito: quem insere e obrigado a dizer o que esta colocando aqui.
    signature TEXT NOT NULL,
    -- diagnostico, jamais causalidade. Nenhuma decisao de ordem le esta coluna.
    logged_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO "sync_events" VALUES('7f78ea57-6c00-4142-b188-37092c17296a','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',1,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','upsert','{"id":"4f7f4952-6821-4749-9b6c-0be2e8de5712","book_id":"bc759499-8ea2-4e92-9cf8-caf926d35bd8","title":"C1","content":"<p>C1 primeira</p>","summary":"","scene_origin":"","scene_destination":"","word_count":2,"status":"IDEIA","canon_status":"CANON","sort_order":0,"created_at":"2026-09-21 16:50:39","updated_at":"2026-09-21 16:50:40"}','','dbf4b9ef5fe62ede34da19915dd54974a20868c645c2f1d95ec316acc5f0ed1b','ICHTM22UY5NTUBIIADN7AQOTNN2OZEVLPUL5KENKKAWJXNZZKDNN5I7GMMIH7YP3LP4ONLLARBHSF5ME2E5WSYGCFAT4RY3XEECZEBI','2026-09-21 16:50:40');
INSERT INTO "sync_events" VALUES('f5c06b3b-91b6-43cf-9262-83de72bb284a','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',2,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','upsert','{"id":"4f7f4952-6821-4749-9b6c-0be2e8de5712","book_id":"bc759499-8ea2-4e92-9cf8-caf926d35bd8","title":"C1","content":"<p>C1 segunda</p>","summary":"","scene_origin":"","scene_destination":"","word_count":2,"status":"IDEIA","canon_status":"CANON","sort_order":0,"created_at":"2026-09-21 16:50:39","updated_at":"2026-09-21 16:50:40"}','dbf4b9ef5fe62ede34da19915dd54974a20868c645c2f1d95ec316acc5f0ed1b','cf91fdcced24cf50d8b7f287746e5400ad198272385346d308ca3be5196cda84','23LDHBIXYWUB6BSL7RIJG774YWGYK2LSAAWKGHGU27RNXQ235W74UX74AGK34JLNTAGGUZBISYB4UGEA3VMJUVF66H7PF5TJP4HTWAI','2026-09-21 16:50:40');
INSERT INTO "sync_events" VALUES('1432daa0-5b9d-4cca-a286-c9befe4620a9','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',3,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','chapter','4751d6ce-b56a-40cd-bea7-098ae24ee6d9','upsert','{"id":"4751d6ce-b56a-40cd-bea7-098ae24ee6d9","book_id":"bc759499-8ea2-4e92-9cf8-caf926d35bd8","title":"C3","content":"<p>C3 antes de apagar</p>","summary":"","scene_origin":"","scene_destination":"","word_count":4,"status":"IDEIA","canon_status":"CANON","sort_order":2,"created_at":"2026-09-21 16:50:39","updated_at":"2026-09-21 16:50:40"}','','ab0d89175660a68aaf0fcc973dfa26dfee9c34f96f2a2cefe0252a9fd88bb16d','524A3NVZOO7GB4JGVSEWX5U2BFXLLUTUVIKUA52DFHUKGJQK53GJHZQLCJVINFW7X5PNZFWFRKX6LLVGAFLCU5KYXOJS3C7FCZA7SDI','2026-09-21 16:50:40');
INSERT INTO "sync_events" VALUES('745718a4-7b60-4360-a20a-94671f832850','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',4,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','attachment','461383e0-b1e3-4071-a840-00cb46b5001d','upsert','{"id":"461383e0-b1e3-4071-a840-00cb46b5001d","universe_id":"9fce1c54-f5d8-4aa1-8403-f82984fcd492","owner_type":"chapter","owner_id":"4f7f4952-6821-4749-9b6c-0be2e8de5712","data_url":"","blob_hash":"ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb","mime_type":"image/png","caption":"fica","sort_order":0,"created_at":"2026-09-21 16:50:40"}','','8a016be5f9ec40e9d406f18ecba0253b0a10acbf490a136b0708f500380cbd94','7LNIYO57VLEES2VIMEMY6X2XH4VILWLFR7VR2GDJT6364VQCBFBCX5PP2XPX3KEAAXRQVHTEHIMRT3QYJNUOH5VZV7TN6LPPQUMT4AI','2026-09-21 16:50:41');
INSERT INTO "sync_events" VALUES('4e82e203-8c4d-4681-92da-a132c9e81477','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',5,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','attachment','5e7f4ee7-d37c-4238-abaf-e8cb5683dc23','upsert','{"id":"5e7f4ee7-d37c-4238-abaf-e8cb5683dc23","universe_id":"9fce1c54-f5d8-4aa1-8403-f82984fcd492","owner_type":"chapter","owner_id":"4f7f4952-6821-4749-9b6c-0be2e8de5712","data_url":"","blob_hash":"3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d","mime_type":"image/png","caption":"sai","sort_order":1,"created_at":"2026-09-21 16:50:41"}','','ab1256e0becdc6032fe6c14044a6a03f42f20605359c7969fbd166a146d9fddc','3DIPFIKPZIW3EM54E5XPFVCDIAK3KZG4RTYPN6X36RL5OC7272CSXZZQ6S6GBRO7R7ECYCUK5OCTFY66UVLGNDPEXNGNRZTE6FIYGBY','2026-09-21 16:50:41');
INSERT INTO "sync_events" VALUES('bc9d441a-9841-44d6-8e3d-6e75f304ce05','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',6,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','attachment','5e7f4ee7-d37c-4238-abaf-e8cb5683dc23','delete','','ab1256e0becdc6032fe6c14044a6a03f42f20605359c7969fbd166a146d9fddc','bc31ebd25f011c3d608b0b87237a7cdeb542653c86db05581304fc0deb3e69ed','3XOLRNT6BRZYBHGDSAP3RUTDTMQJRC2KVLPEL4QUYSBBYDGYI3XMPXW4Y52HMCJDUKMIQMWZ2WAICAOFJXYCZLNLEL223ZZCKEAZKBQ','2026-09-21 16:50:41');
INSERT INTO "sync_events" VALUES('1ce95c2c-473c-499d-8113-57991555ad45','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',7,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','upsert','{"id":"4f7f4952-6821-4749-9b6c-0be2e8de5712","book_id":"bc759499-8ea2-4e92-9cf8-caf926d35bd8","title":"C1","content":"<p>C1 terceira</p>","summary":"","scene_origin":"","scene_destination":"","word_count":2,"status":"IDEIA","canon_status":"CANON","sort_order":0,"created_at":"2026-09-21 16:50:39","updated_at":"2026-09-21 16:50:42"}','cf91fdcced24cf50d8b7f287746e5400ad198272385346d308ca3be5196cda84','4a078c1f4b3296ae45e3d0d7055b68f1e763e7e962662d46538d549a3ab23d24','UPW7QQXSFSYRYD4CZY7VJDP6OJIRST5HYDHYD3VT442IE6CKPMBJYDZEM7UFNCYVKXLIE4APB7ETHFF5T6MUK5IRKC62OKYJM7NCSAY','2026-09-21 16:50:42');
INSERT INTO "sync_events" VALUES('0c09f9d3-e77e-4651-b494-aa5fcf940222','QOQELGFBXMU275NTH35UDZ4DZFEG6S2F',1,'9fce1c54-f5d8-4aa1-8403-f82984fcd492','chapter','0a083133-8c45-460f-8a49-68e589318d39','upsert','{"id":"0a083133-8c45-460f-8a49-68e589318d39","book_id":"bc759499-8ea2-4e92-9cf8-caf926d35bd8","title":"C2","content":"<p>C2 escrito no celular</p>","summary":"","scene_origin":"","scene_destination":"","word_count":4,"status":"IDEIA","canon_status":"CANON","sort_order":1,"created_at":"2026-09-21 16:50:39","updated_at":"2026-09-21 16:50:42"}','','cde9e7155147bc9e66cfc3d278b6263800e34756ac60115001009ef3416e819d','KSRNQKATBO5IHCF4JDATM5SDUQDMAJYSONTHTM6JAZ7JBHV3SZZARVPPLCIZNFVE44EXJKL2WUUILZZDSKUDLXKW7TA7AXEVFQ4RGAI','2026-09-21 16:50:42');
CREATE TABLE sync_peer_vectors (
    peer_device_id TEXT NOT NULL REFERENCES sync_devices(device_id),
    origin_device_id TEXT NOT NULL REFERENCES sync_devices(device_id),
    last_seq_confirmed INTEGER NOT NULL DEFAULT 0 CHECK (last_seq_confirmed >= 0),
    PRIMARY KEY (peer_device_id, origin_device_id)
);
CREATE TABLE sync_peers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    address TEXT NOT NULL DEFAULT '',
    trusted_at TEXT NOT NULL,
    last_sync_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE sync_revision_history (
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    rev TEXT NOT NULL,
    base_rev TEXT NOT NULL DEFAULT '',
    event_id TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (aggregate_type, aggregate_id, rev)
);
INSERT INTO "sync_revision_history" VALUES('chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','dbf4b9ef5fe62ede34da19915dd54974a20868c645c2f1d95ec316acc5f0ed1b','','7f78ea57-6c00-4142-b188-37092c17296a');
INSERT INTO "sync_revision_history" VALUES('chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','cf91fdcced24cf50d8b7f287746e5400ad198272385346d308ca3be5196cda84','dbf4b9ef5fe62ede34da19915dd54974a20868c645c2f1d95ec316acc5f0ed1b','f5c06b3b-91b6-43cf-9262-83de72bb284a');
INSERT INTO "sync_revision_history" VALUES('chapter','4751d6ce-b56a-40cd-bea7-098ae24ee6d9','ab0d89175660a68aaf0fcc973dfa26dfee9c34f96f2a2cefe0252a9fd88bb16d','','1432daa0-5b9d-4cca-a286-c9befe4620a9');
INSERT INTO "sync_revision_history" VALUES('attachment','461383e0-b1e3-4071-a840-00cb46b5001d','8a016be5f9ec40e9d406f18ecba0253b0a10acbf490a136b0708f500380cbd94','','745718a4-7b60-4360-a20a-94671f832850');
INSERT INTO "sync_revision_history" VALUES('attachment','5e7f4ee7-d37c-4238-abaf-e8cb5683dc23','ab1256e0becdc6032fe6c14044a6a03f42f20605359c7969fbd166a146d9fddc','','4e82e203-8c4d-4681-92da-a132c9e81477');
INSERT INTO "sync_revision_history" VALUES('attachment','5e7f4ee7-d37c-4238-abaf-e8cb5683dc23','bc31ebd25f011c3d608b0b87237a7cdeb542653c86db05581304fc0deb3e69ed','ab1256e0becdc6032fe6c14044a6a03f42f20605359c7969fbd166a146d9fddc','bc9d441a-9841-44d6-8e3d-6e75f304ce05');
INSERT INTO "sync_revision_history" VALUES('chapter','4f7f4952-6821-4749-9b6c-0be2e8de5712','4a078c1f4b3296ae45e3d0d7055b68f1e763e7e962662d46538d549a3ab23d24','cf91fdcced24cf50d8b7f287746e5400ad198272385346d308ca3be5196cda84','1ce95c2c-473c-499d-8113-57991555ad45');
INSERT INTO "sync_revision_history" VALUES('chapter','0a083133-8c45-460f-8a49-68e589318d39','cde9e7155147bc9e66cfc3d278b6263800e34756ac60115001009ef3416e819d','','0c09f9d3-e77e-4651-b494-aa5fcf940222');
CREATE TABLE sync_tombstones (
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    deleted_rev TEXT NOT NULL,
    deleted_at TEXT NOT NULL DEFAULT (datetime('now')), origin_device_id TEXT NOT NULL DEFAULT '', origin_seq INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (aggregate_type, aggregate_id)
);
INSERT INTO "sync_tombstones" VALUES('attachment','5e7f4ee7-d37c-4238-abaf-e8cb5683dc23','bc31ebd25f011c3d608b0b87237a7cdeb542653c86db05581304fc0deb3e69ed','2026-09-21 16:50:41','T62TGN4Z4LWVKHXMZOBYCZBNSPBCU6QM',6);
CREATE TABLE timeline_events (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    event_type TEXT NOT NULL DEFAULT 'MARCO',
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, entity_id TEXT REFERENCES entities(id) ON DELETE SET NULL, display_date TEXT NOT NULL DEFAULT '', sort_key REAL NOT NULL DEFAULT 0,
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);
CREATE TABLE universes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    cover_image TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, cover_blob_hash TEXT NOT NULL DEFAULT '', cover_mime_type TEXT NOT NULL DEFAULT '');
INSERT INTO "universes" VALUES('9fce1c54-f5d8-4aa1-8403-f82984fcd492','Terra','','','2026-09-21 16:50:38','2026-09-21 16:50:38','','');
CREATE INDEX idx_stories_universe ON stories(universe_id);
CREATE INDEX idx_books_story ON books(story_id);
CREATE INDEX idx_chapters_book ON chapters(book_id);
CREATE INDEX idx_entities_universe ON entities(universe_id);
CREATE INDEX idx_entities_type ON entities(universe_id, type);
CREATE INDEX idx_entity_attrs_entity ON entity_attributes(entity_id);
CREATE INDEX idx_entity_templates ON entity_templates(universe_id, entity_type);
CREATE INDEX idx_relations_universe ON relations(universe_id);
CREATE INDEX idx_relations_source ON relations(source_id);
CREATE INDEX idx_relations_target ON relations(target_id);
CREATE INDEX idx_mentions_chapter ON mentions(chapter_id);
CREATE INDEX idx_mentions_entity ON mentions(entity_id);
CREATE INDEX idx_changelog_entity ON change_log(entity_type, entity_id);
CREATE INDEX idx_timeline_universe_date ON timeline_events(universe_id, start_date);
CREATE INDEX idx_planning_universe_status ON planning_items(universe_id, status, sort_order);
CREATE INDEX idx_revisions_chapter_date ON chapter_revisions(chapter_id, created_at DESC);
CREATE INDEX idx_change_log_universe_date ON change_log(universe_id, created_at DESC);
CREATE INDEX idx_timeline_universe_sort ON timeline_events(universe_id, sort_key, start_date);
CREATE INDEX idx_attachments_owner ON attachments(universe_id, owner_type, owner_id, sort_order);
CREATE INDEX idx_content_tags_universe ON content_tags(universe_id, name);
CREATE INDEX idx_content_fields_owner ON content_custom_fields(owner_type, owner_id, sort_order);
CREATE INDEX idx_collaboration_sessions_status ON collaboration_sessions(status, created_at DESC);
CREATE INDEX idx_collaboration_contributions_review ON collaboration_contributions(session_id, status, sequence);
CREATE INDEX idx_content_tag_owner ON content_tag_assignments(owner_type, owner_id);
CREATE INDEX idx_planning_fields_universe_order
    ON planning_field_definitions(universe_id, sort_order, created_at);
CREATE UNIQUE INDEX idx_planning_field_story_link
    ON planning_field_links(planning_item_id, field_definition_id, story_id)
    WHERE story_id IS NOT NULL;
CREATE UNIQUE INDEX idx_planning_field_entity_link
    ON planning_field_links(planning_item_id, field_definition_id, entity_id)
    WHERE entity_id IS NOT NULL;
CREATE UNIQUE INDEX idx_planning_field_tag_link
    ON planning_field_links(planning_item_id, field_definition_id, tag_id)
    WHERE tag_id IS NOT NULL;
CREATE INDEX idx_planning_field_links_card
    ON planning_field_links(planning_item_id, field_definition_id);
CREATE INDEX idx_canvas_nodes_universe ON canvas_nodes(universe_id);
CREATE INDEX idx_canvas_edges_universe ON canvas_edges(universe_id);
CREATE INDEX idx_planning_fields_owner
    ON planning_field_definitions(owner_item_id)
    WHERE owner_item_id IS NOT NULL;
CREATE UNIQUE INDEX idx_sync_devices_self ON sync_devices(is_self) WHERE is_self = 1;
CREATE UNIQUE INDEX idx_sync_events_origin_seq ON sync_events(device_id, seq);
CREATE INDEX idx_sync_events_aggregate
    ON sync_events(aggregate_type, aggregate_id);
CREATE INDEX idx_sync_revision_base
    ON sync_revision_history(aggregate_type, aggregate_id, base_rev);
CREATE INDEX idx_sync_divergences_abertas
    ON sync_divergences(aggregate_type, aggregate_id)
    WHERE resolved_at = '';
CREATE INDEX idx_tombstones_origem ON sync_tombstones(origin_device_id, origin_seq);
CREATE INDEX idx_blob_migration_issues_abertas
    ON blob_migration_issues(surface, table_name)
    WHERE resolved_at = '';
CREATE TRIGGER trg_chapter_revision
BEFORE UPDATE OF content, title ON chapters
WHEN OLD.content <> NEW.content OR OLD.title <> NEW.title
BEGIN
  INSERT INTO chapter_revisions (id, chapter_id, title, content, word_count, created_at)
  VALUES (lower(hex(randomblob(16))), OLD.id, OLD.title, OLD.content, OLD.word_count, datetime('now'));
END;
CREATE TRIGGER trg_chapter_history_insert
AFTER INSERT ON chapters
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, new_value, created_at)
  SELECT lower(hex(randomblob(16))), s.universe_id, 'chapter', NEW.id, 'create', '', NEW.title, datetime('now')
  FROM books b JOIN stories s ON s.id = b.story_id WHERE b.id = NEW.book_id;
END;
CREATE TRIGGER trg_chapter_history_update
AFTER UPDATE OF content, title, status ON chapters
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, old_value, new_value, created_at)
  SELECT lower(hex(randomblob(16))), s.universe_id, 'chapter', NEW.id, 'update',
         CASE WHEN OLD.title <> NEW.title THEN 'title' WHEN OLD.status <> NEW.status THEN 'status' ELSE 'content' END,
         CASE WHEN OLD.title <> NEW.title THEN OLD.title WHEN OLD.status <> NEW.status THEN OLD.status ELSE CAST(OLD.word_count AS TEXT) END,
         CASE WHEN OLD.title <> NEW.title THEN NEW.title WHEN OLD.status <> NEW.status THEN NEW.status ELSE CAST(NEW.word_count AS TEXT) END,
         datetime('now')
  FROM books b JOIN stories s ON s.id = b.story_id WHERE b.id = NEW.book_id;
END;
CREATE TRIGGER trg_entity_history_insert
AFTER INSERT ON entities
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, new_value, created_at)
  VALUES (lower(hex(randomblob(16))), NEW.universe_id, 'entity', NEW.id, 'create', '', NEW.name, datetime('now'));
END;
CREATE TRIGGER trg_entity_history_update
AFTER UPDATE ON entities
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, old_value, new_value, created_at)
  VALUES (lower(hex(randomblob(16))), NEW.universe_id, 'entity', NEW.id, 'update', 'record', OLD.name, NEW.name, datetime('now'));
END;
CREATE TRIGGER trg_entity_attachments_delete
AFTER DELETE ON entities
BEGIN
  DELETE FROM attachments WHERE owner_type = 'entity' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_chapter_attachments_delete
AFTER DELETE ON chapters
BEGIN
  DELETE FROM attachments WHERE owner_type = 'chapter' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_story_metadata_delete AFTER DELETE ON stories BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'story' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'story' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_book_metadata_delete AFTER DELETE ON books BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'book' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'book' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_chapter_metadata_delete AFTER DELETE ON chapters BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'chapter' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'chapter' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_entity_metadata_delete AFTER DELETE ON entities BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'entity' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'entity' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_timeline_metadata_delete AFTER DELETE ON timeline_events BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'timeline' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_planning_metadata_delete AFTER DELETE ON planning_items BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'planning' AND owner_id = OLD.id;
END;
CREATE TRIGGER trg_planning_field_definition_delete
AFTER DELETE ON planning_field_definitions
BEGIN
    UPDATE planning_items
    SET custom_field_values = json_remove(custom_field_values, '$."' || OLD.id || '"'),
        updated_at = datetime('now')
    WHERE universe_id = OLD.universe_id
      AND json_type(custom_field_values, '$."' || OLD.id || '"') IS NOT NULL;
END;
CREATE TRIGGER trg_planning_field_link_validate
BEFORE INSERT ON planning_field_links
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1
        FROM planning_items p
        JOIN planning_field_definitions f ON f.id = NEW.field_definition_id
        WHERE p.id = NEW.planning_item_id AND p.universe_id = f.universe_id
    ) THEN RAISE(ABORT, 'planning field and card must belong to the same universe') END;
    SELECT CASE WHEN NEW.story_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM stories s
        JOIN planning_items p ON p.id = NEW.planning_item_id
        JOIN planning_field_definitions f ON f.id = NEW.field_definition_id
        WHERE s.id = NEW.story_id AND s.universe_id = p.universe_id AND f.field_type = 'story'
    ) THEN RAISE(ABORT, 'invalid planning story relation') END;
    SELECT CASE WHEN NEW.entity_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM entities e
        JOIN planning_items p ON p.id = NEW.planning_item_id
        JOIN planning_field_definitions f ON f.id = NEW.field_definition_id
        WHERE e.id = NEW.entity_id AND e.universe_id = p.universe_id
          AND e.type = 'Personagem' AND f.field_type = 'character'
    ) THEN RAISE(ABORT, 'invalid planning character relation') END;
    SELECT CASE WHEN NEW.tag_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM content_tags t
        JOIN planning_items p ON p.id = NEW.planning_item_id
        JOIN planning_field_definitions f ON f.id = NEW.field_definition_id
        WHERE t.id = NEW.tag_id AND t.universe_id = p.universe_id AND f.field_type = 'tags'
    ) THEN RAISE(ABORT, 'invalid planning tag relation') END;
END;
CREATE TRIGGER trg_planning_field_scope_insert
BEFORE INSERT ON planning_field_definitions
BEGIN
    SELECT RAISE(ABORT, 'Um campo restrito a um card precisa de owner_item_id.')
     WHERE NEW.scope = 'card' AND NEW.owner_item_id IS NULL;
    SELECT RAISE(ABORT, 'Um campo universal nao pode ter owner_item_id.')
     WHERE NEW.scope = 'universal' AND NEW.owner_item_id IS NOT NULL;
END;
CREATE TRIGGER trg_planning_field_scope_update
BEFORE UPDATE OF scope, owner_item_id ON planning_field_definitions
BEGIN
    SELECT RAISE(ABORT, 'Um campo restrito a um card precisa de owner_item_id.')
     WHERE NEW.scope = 'card' AND NEW.owner_item_id IS NULL;
    SELECT RAISE(ABORT, 'Um campo universal nao pode ter owner_item_id.')
     WHERE NEW.scope = 'universal' AND NEW.owner_item_id IS NOT NULL;
END;
CREATE TRIGGER trg_sync_events_sem_update
BEFORE UPDATE ON sync_events
BEGIN
    SELECT RAISE(ABORT, 'sync_events e append-only: evento nao se edita.');
END;
CREATE TRIGGER trg_sync_events_sem_delete
BEFORE DELETE ON sync_events
BEGIN
    SELECT RAISE(ABORT, 'sync_events e append-only: evento nao se apaga.');
END;
CREATE TRIGGER trg_sync_events_payload_coerente
BEFORE INSERT ON sync_events
BEGIN
    SELECT RAISE(ABORT, 'Evento upsert precisa de payload.')
     WHERE NEW.operation = 'upsert' AND NEW.payload = '';
    SELECT RAISE(ABORT, 'Evento delete nao carrega payload.')
     WHERE NEW.operation = 'delete' AND NEW.payload <> '';
    SELECT RAISE(ABORT, 'Evento precisa de new_rev.')
     WHERE NEW.new_rev = '';
END;
CREATE TRIGGER trg_sync_cursor_contiguo_update
BEFORE UPDATE OF last_seq_applied ON sync_cursors
BEGIN
    SELECT RAISE(ABORT, 'Cursor nao retrocede.')
     WHERE NEW.last_seq_applied < OLD.last_seq_applied;

    SELECT RAISE(ABORT, 'Cursor so avanca ate a maior sequencia contigua aplicada.')
     WHERE NEW.last_seq_applied > OLD.last_seq_applied
       AND (SELECT COUNT(*)
              FROM sync_events e
              JOIN sync_applied_events a ON a.event_id = e.event_id
             WHERE e.device_id = NEW.origin_device_id
               AND e.seq > OLD.last_seq_applied
               AND e.seq <= NEW.last_seq_applied)
           <> NEW.last_seq_applied - OLD.last_seq_applied;
END;
CREATE TRIGGER trg_sync_cursor_baseline_imutavel
BEFORE UPDATE OF baseline_seq ON sync_cursors
BEGIN
    SELECT RAISE(ABORT, 'Baseline se semeia uma vez: snapshot sobre cursor existente e regressao ao V1.')
     WHERE NEW.baseline_seq <> OLD.baseline_seq;
END;
CREATE TRIGGER trg_sync_cursor_contiguo_insert
BEFORE INSERT ON sync_cursors
BEGIN
    SELECT RAISE(ABORT, 'Cursor so avanca ate a maior sequencia contigua aplicada.')
     WHERE NEW.last_seq_applied > NEW.baseline_seq
       AND (SELECT COUNT(*)
              FROM sync_events e
              JOIN sync_applied_events a ON a.event_id = e.event_id
             WHERE e.device_id = NEW.origin_device_id
               AND e.seq > NEW.baseline_seq
               AND e.seq <= NEW.last_seq_applied)
           <> NEW.last_seq_applied - NEW.baseline_seq;
END;
CREATE TRIGGER trg_peer_vector_nao_retrocede
BEFORE UPDATE OF last_seq_confirmed ON sync_peer_vectors
BEGIN
    SELECT RAISE(ABORT, 'A confirmacao de um peer nao retrocede.')
     WHERE NEW.last_seq_confirmed < OLD.last_seq_confirmed;
END;
CREATE TRIGGER trg_exit_reason_coerente_insert
BEFORE INSERT ON sync_devices
BEGIN
    SELECT RAISE(ABORT, 'Dispositivo ativo nao tem motivo de saida.')
     WHERE NEW.state = 'active' AND NEW.exit_reason <> '';
END;
CREATE TRIGGER trg_exit_reason_coerente_update
BEFORE UPDATE OF state, exit_reason ON sync_devices
BEGIN
    SELECT RAISE(ABORT, 'Dispositivo ativo nao tem motivo de saida.')
     WHERE NEW.state = 'active' AND NEW.exit_reason <> '';
END;
CREATE TRIGGER trg_motivo_de_saida_so_para_quem_saiu_insert
BEFORE INSERT ON sync_devices
BEGIN
    SELECT RAISE(ABORT, 'Motivo de saida so existe para dispositivo aposentado.')
     WHERE NEW.state <> 'retired' AND NEW.exit_reason <> '';
END;
CREATE TRIGGER trg_motivo_de_saida_so_para_quem_saiu_update
BEFORE UPDATE OF state, exit_reason ON sync_devices
BEGIN
    SELECT RAISE(ABORT, 'Motivo de saida so existe para dispositivo aposentado.')
     WHERE NEW.state <> 'retired' AND NEW.exit_reason <> '';
END;
CREATE TRIGGER trg_sync_cursor_nao_se_apaga
BEFORE DELETE ON sync_cursors
BEGIN
    SELECT RAISE(ABORT, 'Cursor nao se apaga: apagar e reinserir re-semeia o baseline, e o intervalo pulado nunca mais e pedido.');
END;
COMMIT;
