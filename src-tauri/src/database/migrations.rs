/// NarraHub — Database Migrations
/// Creates all tables on first run

pub const LATEST_SCHEMA_VERSION: i64 = 13;

pub fn sql_for_version(version: i64) -> Option<&'static str> {
    match version {
        1 => Some(MIGRATION_V1),
        2 => Some(MIGRATION_V2),
        3 => Some(MIGRATION_V3),
        4 => Some(MIGRATION_V4),
        5 => Some(MIGRATION_V5),
        6 => Some(MIGRATION_V6),
        7 => Some(MIGRATION_V7),
        8 => Some(MIGRATION_V8),
        9 => Some(MIGRATION_V9),
        10 => Some(MIGRATION_V10),
        11 => Some(MIGRATION_V11),
        12 => Some(MIGRATION_V12),
        13 => Some(MIGRATION_V13),
        _ => None,
    }
}

pub const MIGRATION_V1: &str = r#"
-- ============================================
-- NarraHub Database Schema v1
-- ============================================

-- Universo: o container raiz de tudo
CREATE TABLE IF NOT EXISTS universes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    cover_image TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- História/Série dentro de um universo
CREATE TABLE IF NOT EXISTS stories (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

-- Livro dentro de uma história
CREATE TABLE IF NOT EXISTS books (
    id TEXT PRIMARY KEY NOT NULL,
    story_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (story_id) REFERENCES stories(id) ON DELETE CASCADE
);

-- Capítulo dentro de um livro
CREATE TABLE IF NOT EXISTS chapters (
    id TEXT PRIMARY KEY NOT NULL,
    book_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    word_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'IDEIA',
    canon_status TEXT NOT NULL DEFAULT 'CANON',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

-- Entidade genérica (Personagem, Lugar, Evento, Objeto, etc.)
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    image TEXT NOT NULL DEFAULT '',
    canon_status TEXT NOT NULL DEFAULT 'CANON',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

-- Atributos dinâmicos de entidades
CREATE TABLE IF NOT EXISTS entity_attributes (
    id TEXT PRIMARY KEY NOT NULL,
    entity_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);

-- Templates de atributos por tipo de entidade (por universo)
CREATE TABLE IF NOT EXISTS entity_templates (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    attribute_key TEXT NOT NULL,
    default_value TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

-- Relações entre entidades
CREATE TABLE IF NOT EXISTS relations (
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

-- Menções de entidades em capítulos
CREATE TABLE IF NOT EXISTS mentions (
    id TEXT PRIMARY KEY NOT NULL,
    chapter_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);

-- Histórico de alterações
CREATE TABLE IF NOT EXISTS change_log (
    id TEXT PRIMARY KEY NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    field TEXT NOT NULL DEFAULT '',
    old_value TEXT NOT NULL DEFAULT '',
    new_value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================
-- Índices
-- ============================================

CREATE INDEX IF NOT EXISTS idx_stories_universe ON stories(universe_id);
CREATE INDEX IF NOT EXISTS idx_books_story ON books(story_id);
CREATE INDEX IF NOT EXISTS idx_chapters_book ON chapters(book_id);
CREATE INDEX IF NOT EXISTS idx_entities_universe ON entities(universe_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(universe_id, type);
CREATE INDEX IF NOT EXISTS idx_entity_attrs_entity ON entity_attributes(entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_templates ON entity_templates(universe_id, entity_type);
CREATE INDEX IF NOT EXISTS idx_relations_universe ON relations(universe_id);
CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
CREATE INDEX IF NOT EXISTS idx_mentions_chapter ON mentions(chapter_id);
CREATE INDEX IF NOT EXISTS idx_mentions_entity ON mentions(entity_id);
CREATE INDEX IF NOT EXISTS idx_changelog_entity ON change_log(entity_type, entity_id);

-- Enable WAL mode for better concurrent access
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
"#;

pub const MIGRATION_V2: &str = r#"
ALTER TABLE change_log ADD COLUMN universe_id TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS timeline_events (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    event_type TEXT NOT NULL DEFAULT 'MARCO',
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS planning_items (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    chapter_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'IDEIAS' CHECK(status IN ('IDEIAS','PLANEJADO','ESCREVENDO','REVISAO','FINALIZADO')),
    target_words INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE,
    FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS chapter_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    chapter_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_peers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    address TEXT NOT NULL DEFAULT '',
    trusted_at TEXT NOT NULL,
    last_sync_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS sync_events (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL DEFAULT '',
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    device_id TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    applied_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS sync_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    field TEXT NOT NULL,
    local_value TEXT NOT NULL,
    remote_value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_timeline_universe_date ON timeline_events(universe_id, start_date);
CREATE INDEX IF NOT EXISTS idx_planning_universe_status ON planning_items(universe_id, status, sort_order);
CREATE INDEX IF NOT EXISTS idx_revisions_chapter_date ON chapter_revisions(chapter_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_change_log_universe_date ON change_log(universe_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_sync_events_created ON sync_events(created_at, id);

CREATE TRIGGER IF NOT EXISTS trg_chapter_revision
BEFORE UPDATE OF content, title ON chapters
WHEN OLD.content <> NEW.content OR OLD.title <> NEW.title
BEGIN
  INSERT INTO chapter_revisions (id, chapter_id, title, content, word_count, created_at)
  VALUES (lower(hex(randomblob(16))), OLD.id, OLD.title, OLD.content, OLD.word_count, datetime('now'));
END;

CREATE TRIGGER IF NOT EXISTS trg_chapter_history_insert
AFTER INSERT ON chapters
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, new_value, created_at)
  SELECT lower(hex(randomblob(16))), s.universe_id, 'chapter', NEW.id, 'create', '', NEW.title, datetime('now')
  FROM books b JOIN stories s ON s.id = b.story_id WHERE b.id = NEW.book_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_chapter_history_update
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

CREATE TRIGGER IF NOT EXISTS trg_entity_history_insert
AFTER INSERT ON entities
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, new_value, created_at)
  VALUES (lower(hex(randomblob(16))), NEW.universe_id, 'entity', NEW.id, 'create', '', NEW.name, datetime('now'));
END;

CREATE TRIGGER IF NOT EXISTS trg_entity_history_update
AFTER UPDATE ON entities
BEGIN
  INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, old_value, new_value, created_at)
  VALUES (lower(hex(randomblob(16))), NEW.universe_id, 'entity', NEW.id, 'update', 'record', OLD.name, NEW.name, datetime('now'));
END;
"#;

pub const MIGRATION_V3: &str = r#"
ALTER TABLE timeline_events ADD COLUMN entity_id TEXT REFERENCES entities(id) ON DELETE SET NULL;
ALTER TABLE timeline_events ADD COLUMN display_date TEXT NOT NULL DEFAULT '';
ALTER TABLE timeline_events ADD COLUMN sort_key REAL NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_timeline_universe_sort ON timeline_events(universe_id, sort_key, start_date);
"#;

pub const MIGRATION_V4: &str = r#"
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    owner_type TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    data_url TEXT NOT NULL,
    caption TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_attachments_owner ON attachments(universe_id, owner_type, owner_id, sort_order);

CREATE TRIGGER IF NOT EXISTS trg_entity_attachments_delete
AFTER DELETE ON entities
BEGIN
  DELETE FROM attachments WHERE owner_type = 'entity' AND owner_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_chapter_attachments_delete
AFTER DELETE ON chapters
BEGIN
  DELETE FROM attachments WHERE owner_type = 'chapter' AND owner_id = OLD.id;
END;
"#;

pub const MIGRATION_V5: &str = r#"
ALTER TABLE books ADD COLUMN cover_image TEXT NOT NULL DEFAULT '';
"#;

pub const MIGRATION_V6: &str = r#"
ALTER TABLE chapters ADD COLUMN summary TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS content_tags (
    id TEXT PRIMARY KEY NOT NULL,
    universe_id TEXT NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE,
    color TEXT NOT NULL DEFAULT '#7d3650',
    created_at TEXT NOT NULL,
    UNIQUE(universe_id, name),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS content_tag_assignments (
    id TEXT PRIMARY KEY NOT NULL,
    tag_id TEXT NOT NULL,
    owner_type TEXT NOT NULL CHECK(owner_type IN ('universe','story','book','chapter','entity')),
    owner_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(tag_id, owner_type, owner_id),
    FOREIGN KEY (tag_id) REFERENCES content_tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS content_custom_fields (
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

CREATE INDEX IF NOT EXISTS idx_content_tags_universe ON content_tags(universe_id, name);
CREATE INDEX IF NOT EXISTS idx_content_tag_owner ON content_tag_assignments(owner_type, owner_id);
CREATE INDEX IF NOT EXISTS idx_content_fields_owner ON content_custom_fields(owner_type, owner_id, sort_order);

CREATE TRIGGER IF NOT EXISTS trg_story_metadata_delete AFTER DELETE ON stories BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'story' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'story' AND owner_id = OLD.id;
END;
CREATE TRIGGER IF NOT EXISTS trg_book_metadata_delete AFTER DELETE ON books BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'book' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'book' AND owner_id = OLD.id;
END;
CREATE TRIGGER IF NOT EXISTS trg_chapter_metadata_delete AFTER DELETE ON chapters BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'chapter' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'chapter' AND owner_id = OLD.id;
END;
CREATE TRIGGER IF NOT EXISTS trg_entity_metadata_delete AFTER DELETE ON entities BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'entity' AND owner_id = OLD.id;
  DELETE FROM content_custom_fields WHERE owner_type = 'entity' AND owner_id = OLD.id;
END;
"#;

pub const MIGRATION_V7: &str = r#"
-- Campos de contexto do capítulo são dados próprios do capítulo, não metadados genéricos.
ALTER TABLE chapters ADD COLUMN scene_origin TEXT NOT NULL DEFAULT '';
ALTER TABLE chapters ADD COLUMN scene_destination TEXT NOT NULL DEFAULT '';

UPDATE chapters
SET scene_origin = COALESCE((
    SELECT value FROM content_custom_fields
    WHERE owner_type = 'chapter' AND owner_id = chapters.id AND key = 'Origem da cena'
    LIMIT 1
), scene_origin),
scene_destination = COALESCE((
    SELECT value FROM content_custom_fields
    WHERE owner_type = 'chapter' AND owner_id = chapters.id AND key = 'Destino da cena'
    LIMIT 1
), scene_destination);

-- Campos antigos de entidade passam a usar a estrutura canônica da ficha.
INSERT OR IGNORE INTO entity_attributes (id, entity_id, key, value, sort_order)
SELECT id, owner_id, key, value, sort_order
FROM content_custom_fields
WHERE owner_type = 'entity'
  AND EXISTS (SELECT 1 FROM entities WHERE entities.id = content_custom_fields.owner_id)
  AND NOT EXISTS (
      SELECT 1 FROM entity_attributes
      WHERE entity_attributes.entity_id = content_custom_fields.owner_id
        AND entity_attributes.key = content_custom_fields.key COLLATE NOCASE
  );
"#;

pub const MIGRATION_V8: &str = r#"
ALTER TABLE entities ADD COLUMN summary TEXT NOT NULL DEFAULT '';
"#;

pub const MIGRATION_V9: &str = r#"
CREATE TABLE IF NOT EXISTS collaboration_sessions (
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

CREATE TABLE IF NOT EXISTS collaboration_contributions (
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

CREATE INDEX IF NOT EXISTS idx_collaboration_sessions_status ON collaboration_sessions(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_collaboration_contributions_review ON collaboration_contributions(session_id, status, sequence);
"#;

pub const MIGRATION_V10: &str = r#"
CREATE TABLE content_tag_assignments_v10 (
    id TEXT PRIMARY KEY NOT NULL,
    tag_id TEXT NOT NULL,
    owner_type TEXT NOT NULL CHECK(owner_type IN ('universe','story','book','chapter','entity','timeline','planning')),
    owner_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(tag_id, owner_type, owner_id),
    FOREIGN KEY (tag_id) REFERENCES content_tags(id) ON DELETE CASCADE
);

INSERT INTO content_tag_assignments_v10 (id, tag_id, owner_type, owner_id, created_at)
SELECT id, tag_id, owner_type, owner_id, created_at FROM content_tag_assignments;

DROP TRIGGER IF EXISTS trg_story_metadata_delete;
DROP TRIGGER IF EXISTS trg_book_metadata_delete;
DROP TRIGGER IF EXISTS trg_chapter_metadata_delete;
DROP TRIGGER IF EXISTS trg_entity_metadata_delete;
DROP TABLE content_tag_assignments;
ALTER TABLE content_tag_assignments_v10 RENAME TO content_tag_assignments;
CREATE INDEX idx_content_tag_owner ON content_tag_assignments(owner_type, owner_id);

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
CREATE TRIGGER IF NOT EXISTS trg_timeline_metadata_delete AFTER DELETE ON timeline_events BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'timeline' AND owner_id = OLD.id;
END;
CREATE TRIGGER IF NOT EXISTS trg_planning_metadata_delete AFTER DELETE ON planning_items BEGIN
  DELETE FROM content_tag_assignments WHERE owner_type = 'planning' AND owner_id = OLD.id;
END;
"#;

pub const MIGRATION_V11: &str = r#"
ALTER TABLE planning_items ADD COLUMN image TEXT NOT NULL DEFAULT '';
ALTER TABLE planning_items ADD COLUMN custom_field_values TEXT NOT NULL DEFAULT '{}'
    CHECK(json_valid(custom_field_values) AND json_type(custom_field_values) = 'object');

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
    updated_at TEXT NOT NULL,
    UNIQUE(universe_id, name),
    FOREIGN KEY (universe_id) REFERENCES universes(id) ON DELETE CASCADE
);

CREATE INDEX idx_planning_fields_universe_order
    ON planning_field_definitions(universe_id, sort_order, created_at);

CREATE TRIGGER trg_planning_field_definition_delete
AFTER DELETE ON planning_field_definitions
BEGIN
    UPDATE planning_items
    SET custom_field_values = json_remove(custom_field_values, '$."' || OLD.id || '"'),
        updated_at = datetime('now')
    WHERE universe_id = OLD.universe_id
      AND json_type(custom_field_values, '$."' || OLD.id || '"') IS NOT NULL;
END;
"#;

pub const MIGRATION_V12: &str = r#"
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
"#;

pub const MIGRATION_V13: &str = r#"
-- Move relation arrays temporarily written by the schema 11 development build
-- into the normalized schema introduced by migration 12.
INSERT OR IGNORE INTO planning_field_links
    (id, planning_item_id, field_definition_id, story_id, created_at)
SELECT lower(hex(randomblob(16))), p.id, f.id, s.id, datetime('now')
FROM planning_items p
JOIN planning_field_definitions f
  ON f.universe_id = p.universe_id AND f.field_type = 'story'
JOIN json_each(p.custom_field_values, '$."' || f.id || '"') legacy
JOIN stories s ON s.id = legacy.value AND s.universe_id = p.universe_id
WHERE json_type(p.custom_field_values, '$."' || f.id || '"') = 'array'
  AND legacy.type = 'text';

INSERT OR IGNORE INTO planning_field_links
    (id, planning_item_id, field_definition_id, entity_id, created_at)
SELECT lower(hex(randomblob(16))), p.id, f.id, e.id, datetime('now')
FROM planning_items p
JOIN planning_field_definitions f
  ON f.universe_id = p.universe_id AND f.field_type = 'character'
JOIN json_each(p.custom_field_values, '$."' || f.id || '"') legacy
JOIN entities e
  ON e.id = legacy.value AND e.universe_id = p.universe_id AND e.type = 'Personagem'
WHERE json_type(p.custom_field_values, '$."' || f.id || '"') = 'array'
  AND legacy.type = 'text';

INSERT OR IGNORE INTO planning_field_links
    (id, planning_item_id, field_definition_id, tag_id, created_at)
SELECT lower(hex(randomblob(16))), p.id, f.id, t.id, datetime('now')
FROM planning_items p
JOIN planning_field_definitions f
  ON f.universe_id = p.universe_id AND f.field_type = 'tags'
JOIN json_each(p.custom_field_values, '$."' || f.id || '"') legacy
JOIN content_tags t ON t.id = legacy.value AND t.universe_id = p.universe_id
WHERE json_type(p.custom_field_values, '$."' || f.id || '"') = 'array'
  AND legacy.type = 'text';

UPDATE planning_items
SET custom_field_values = COALESCE((
        SELECT json_group_object(
            legacy.key,
            CASE legacy.type
                WHEN 'true' THEN json('true')
                WHEN 'false' THEN json('false')
                WHEN 'array' THEN json(legacy.value)
                WHEN 'object' THEN json(legacy.value)
                ELSE legacy.value
            END
        )
        FROM json_each(planning_items.custom_field_values) legacy
        WHERE NOT EXISTS (
            SELECT 1
            FROM planning_field_definitions f
            WHERE f.universe_id = planning_items.universe_id
              AND f.id = legacy.key
              AND f.field_type IN ('story', 'character', 'tags')
        )
    ), '{}'),
    updated_at = datetime('now')
WHERE EXISTS (
    SELECT 1
    FROM json_each(planning_items.custom_field_values) legacy
    JOIN planning_field_definitions f
      ON f.universe_id = planning_items.universe_id
     AND f.id = legacy.key
     AND f.field_type IN ('story', 'character', 'tags')
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn v7_and_v8_move_legacy_fields_and_add_entity_summaries() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(MIGRATION_V1)
            .expect("apply migration v1");
        connection
            .execute_batch(MIGRATION_V2)
            .expect("apply migration v2");
        connection
            .execute_batch(MIGRATION_V3)
            .expect("apply migration v3");
        connection
            .execute_batch(MIGRATION_V4)
            .expect("apply migration v4");
        connection
            .execute_batch(MIGRATION_V5)
            .expect("apply migration v5");
        connection
            .execute_batch(MIGRATION_V6)
            .expect("apply migration v6");

        connection
            .execute_batch(
                r#"
                INSERT INTO universes (id, name) VALUES ('u1', 'Mundo');
                INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'História');
                INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');
                INSERT INTO chapters (id, book_id, title) VALUES ('c1', 'b1', 'Capítulo');
                INSERT INTO entities (id, universe_id, type, name) VALUES ('e1', 'u1', 'Personagem', 'Lia');
                INSERT INTO content_custom_fields (id, universe_id, owner_type, owner_id, key, value, created_at, updated_at)
                VALUES
                  ('f1', 'u1', 'chapter', 'c1', 'Origem da cena', 'Porto', datetime('now'), datetime('now')),
                  ('f2', 'u1', 'chapter', 'c1', 'Destino da cena', 'Torre', datetime('now'), datetime('now')),
                  ('f3', 'u1', 'entity', 'e1', 'Maior medo', 'Ser esquecida', datetime('now'), datetime('now'));
                "#,
            )
            .expect("seed legacy metadata");

        connection
            .execute_batch(MIGRATION_V7)
            .expect("apply migration v7");
        connection
            .execute_batch(MIGRATION_V8)
            .expect("apply migration v8");

        let route: (String, String) = connection
            .query_row(
                "SELECT scene_origin, scene_destination FROM chapters WHERE id = 'c1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load native scene route");
        assert_eq!(route, ("Porto".into(), "Torre".into()));

        let attribute: (String, String) = connection
            .query_row(
                "SELECT key, value FROM entity_attributes WHERE entity_id = 'e1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load migrated entity attribute");
        assert_eq!(attribute, ("Maior medo".into(), "Ser esquecida".into()));

        let summary: String = connection
            .query_row("SELECT summary FROM entities WHERE id = 'e1'", [], |row| {
                row.get(0)
            })
            .expect("load entity summary");
        assert!(summary.is_empty());
    }

    #[test]
    fn v9_persists_collaboration_review_queue() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(MIGRATION_V9)
            .expect("apply migration v9");
        connection.execute(
            "INSERT INTO collaboration_sessions (id, title, permission, universe_ids, encryption_key, revoke_token, created_at, expires_at) VALUES ('s1', 'Leitura beta', 'edit', '[\"u1\"]', 'local-key', 'revoke', datetime('now'), datetime('now', '+1 day'))",
            [],
        ).expect("insert session");
        connection.execute(
            "INSERT INTO collaboration_contributions (id, session_id, sequence, contributor, kind, universe_id, target_type, target_id, target_label, field, original_value, proposed_value, status, created_at) VALUES ('c1', 's1', 1, 'Bia', 'edit', 'u1', 'chapter', 'ch1', 'Capitulo', 'content', 'antes', 'depois', 'pending', datetime('now'))",
            [],
        ).expect("insert contribution");
        let status: String = connection
            .query_row(
                "SELECT status FROM collaboration_contributions WHERE id = 'c1'",
                [],
                |row| row.get(0),
            )
            .expect("load contribution");
        assert_eq!(status, "pending");
    }

    #[test]
    fn v10_allows_tags_on_timeline_and_planning_previews() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(MIGRATION_V1)
            .expect("apply migration v1");
        connection
            .execute_batch(MIGRATION_V2)
            .expect("apply migration v2");
        connection
            .execute_batch(MIGRATION_V3)
            .expect("apply migration v3");
        connection
            .execute_batch(MIGRATION_V6)
            .expect("apply migration v6");
        connection
            .execute_batch(MIGRATION_V10)
            .expect("apply migration v10");
        connection
            .execute(
                "INSERT INTO universes (id, name) VALUES ('u1', 'Mundo')",
                [],
            )
            .expect("insert universe");
        connection.execute(
            "INSERT INTO content_tags (id, universe_id, name, created_at) VALUES ('t1', 'u1', 'Importante', datetime('now'))",
            [],
        ).expect("insert tag");
        connection.execute(
            "INSERT INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at) VALUES ('a1', 't1', 'timeline', 'event1', datetime('now'))",
            [],
        ).expect("tag timeline");
        connection.execute(
            "INSERT INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at) VALUES ('a2', 't1', 'planning', 'plan1', datetime('now'))",
            [],
        ).expect("tag planning");
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM content_tag_assignments WHERE owner_type IN ('timeline','planning')",
            [],
            |row| row.get(0),
        ).expect("count assignments");
        assert_eq!(count, 2);
    }

    #[test]
    fn v11_adds_typed_planning_cards_and_cleans_deleted_field_values() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        connection
            .execute_batch(MIGRATION_V1)
            .expect("apply migration v1");
        connection
            .execute_batch(MIGRATION_V2)
            .expect("apply migration v2");
        connection
            .execute_batch(MIGRATION_V11)
            .expect("apply migration v11");

        connection
            .execute_batch(
                r#"
                INSERT INTO universes (id, name) VALUES ('u1', 'Mundo');
                INSERT INTO planning_items (id, universe_id, title, created_at, updated_at)
                VALUES ('p1', 'u1', 'Revelar o segredo', datetime('now'), datetime('now'));
                INSERT INTO planning_field_definitions
                    (id, universe_id, name, field_type, options_json, created_at, updated_at)
                VALUES
                    ('f1', 'u1', 'Personagens', 'character', '[]', datetime('now'), datetime('now')),
                    ('f2', 'u1', 'Prioridade', 'select', '["Baixa","Alta"]', datetime('now'), datetime('now'));
                UPDATE planning_items
                SET image = 'data:image/png;base64,card',
                    custom_field_values = '{"f1":["e1"],"f2":"Alta"}'
                WHERE id = 'p1';
                "#,
            )
            .expect("seed typed planning card");

        let card: (String, String) = connection
            .query_row(
                "SELECT image, json_extract(custom_field_values, '$.f2') FROM planning_items WHERE id = 'p1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load planning card");
        assert_eq!(card, ("data:image/png;base64,card".into(), "Alta".into()));

        connection
            .execute("DELETE FROM planning_field_definitions WHERE id = 'f1'", [])
            .expect("delete field definition");
        let removed_value: Option<String> = connection
            .query_row(
                "SELECT json_extract(custom_field_values, '$.f1') FROM planning_items WHERE id = 'p1'",
                [],
                |row| row.get(0),
            )
            .expect("load cleaned card values");
        assert!(removed_value.is_none());

        let invalid_type = connection.execute(
            "INSERT INTO planning_field_definitions (id, universe_id, name, field_type, created_at, updated_at) VALUES ('bad', 'u1', 'Inválido', 'unknown', datetime('now'), datetime('now'))",
            [],
        );
        assert!(invalid_type.is_err());
    }

    #[test]
    fn v12_keeps_planning_links_inside_the_universe_and_cascades_deletions() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        for migration in [
            MIGRATION_V1,
            MIGRATION_V2,
            MIGRATION_V3,
            MIGRATION_V6,
            MIGRATION_V10,
            MIGRATION_V11,
            MIGRATION_V12,
        ] {
            connection
                .execute_batch(migration)
                .expect("apply migration");
        }
        connection.execute_batch(
            r#"
            INSERT INTO universes (id, name) VALUES ('u1', 'Um'), ('u2', 'Dois');
            INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'História'), ('s2', 'u2', 'Outra');
            INSERT INTO entities (id, universe_id, type, name) VALUES
                ('e1', 'u1', 'Personagem', 'Lia'), ('e2', 'u1', 'Lugar', 'Porto');
            INSERT INTO content_tags (id, universe_id, name, created_at) VALUES ('t1', 'u1', 'Urgente', datetime('now'));
            INSERT INTO planning_items (id, universe_id, title, created_at, updated_at)
                VALUES ('p1', 'u1', 'Card', datetime('now'), datetime('now'));
            INSERT INTO planning_field_definitions (id, universe_id, name, field_type, created_at, updated_at) VALUES
                ('fs', 'u1', 'Histórias', 'story', datetime('now'), datetime('now')),
                ('fc', 'u1', 'Personagens', 'character', datetime('now'), datetime('now')),
                ('ft', 'u1', 'Marcadores', 'tags', datetime('now'), datetime('now'));
            INSERT INTO planning_field_links (id, planning_item_id, field_definition_id, story_id, created_at)
                VALUES ('ls', 'p1', 'fs', 's1', datetime('now'));
            INSERT INTO planning_field_links (id, planning_item_id, field_definition_id, entity_id, created_at)
                VALUES ('lc', 'p1', 'fc', 'e1', datetime('now'));
            INSERT INTO planning_field_links (id, planning_item_id, field_definition_id, tag_id, created_at)
                VALUES ('lt', 'p1', 'ft', 't1', datetime('now'));
            "#,
        ).expect("seed valid planning links");

        let cross_universe = connection.execute(
            "INSERT INTO planning_field_links (id, planning_item_id, field_definition_id, story_id, created_at) VALUES ('bad1', 'p1', 'fs', 's2', datetime('now'))",
            [],
        );
        assert!(cross_universe.is_err());
        let wrong_entity_type = connection.execute(
            "INSERT INTO planning_field_links (id, planning_item_id, field_definition_id, entity_id, created_at) VALUES ('bad2', 'p1', 'fc', 'e2', datetime('now'))",
            [],
        );
        assert!(wrong_entity_type.is_err());

        connection
            .execute("DELETE FROM stories WHERE id = 's1'", [])
            .expect("delete story");
        connection
            .execute("DELETE FROM entities WHERE id = 'e1'", [])
            .expect("delete character");
        connection
            .execute("DELETE FROM content_tags WHERE id = 't1'", [])
            .expect("delete tag");
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM planning_field_links", [], |row| {
                row.get(0)
            })
            .expect("count remaining links");
        assert_eq!(remaining, 0);
        let foreign_key_failures: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("check foreign keys");
        assert_eq!(foreign_key_failures, 0);
    }

    #[test]
    fn v13_moves_legacy_relation_arrays_without_changing_scalar_values() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        for migration in [
            MIGRATION_V1,
            MIGRATION_V2,
            MIGRATION_V3,
            MIGRATION_V6,
            MIGRATION_V10,
            MIGRATION_V11,
            MIGRATION_V12,
        ] {
            connection
                .execute_batch(migration)
                .expect("apply migration");
        }
        connection
            .execute_batch(
                r#"
                INSERT INTO universes (id, name) VALUES ('u1', 'Um'), ('u2', 'Dois');
                INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Principal'), ('s2', 'u2', 'Externa');
                INSERT INTO entities (id, universe_id, type, name) VALUES ('e1', 'u1', 'Personagem', 'Lia');
                INSERT INTO content_tags (id, universe_id, name, created_at) VALUES ('t1', 'u1', 'Urgente', datetime('now'));
                INSERT INTO planning_field_definitions (id, universe_id, name, field_type, options_json, created_at, updated_at) VALUES
                    ('note', 'u1', 'Nota', 'text', '[]', datetime('now'), datetime('now')),
                    ('done', 'u1', 'Concluído', 'checkbox', '[]', datetime('now'), datetime('now')),
                    ('multi', 'u1', 'Opções', 'multi_select', '["A","B"]', datetime('now'), datetime('now')),
                    ('stories', 'u1', 'Histórias', 'story', '[]', datetime('now'), datetime('now')),
                    ('characters', 'u1', 'Personagens', 'character', '[]', datetime('now'), datetime('now')),
                    ('tags', 'u1', 'Tags', 'tags', '[]', datetime('now'), datetime('now'));
                INSERT INTO planning_items
                    (id, universe_id, title, custom_field_values, created_at, updated_at)
                VALUES
                    ('p1', 'u1', 'Card', '{"note":"Preservar","done":true,"multi":["A","B"],"stories":["s1","s2","missing"],"characters":["e1"],"tags":["t1"]}', datetime('now'), datetime('now'));
                "#,
            )
            .expect("seed legacy planning values");

        connection
            .execute_batch(MIGRATION_V13)
            .expect("apply migration v13");

        let links: i64 = connection
            .query_row("SELECT COUNT(*) FROM planning_field_links", [], |row| {
                row.get(0)
            })
            .expect("count migrated links");
        assert_eq!(links, 3);
        let values: String = connection
            .query_row(
                "SELECT custom_field_values FROM planning_items WHERE id = 'p1'",
                [],
                |row| row.get(0),
            )
            .expect("load scalar values");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&values).expect("valid JSON"),
            serde_json::json!({"note":"Preservar","done":true,"multi":["A","B"]})
        );
        let cross_universe_links: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM planning_field_links WHERE story_id = 's2'",
                [],
                |row| row.get(0),
            )
            .expect("count invalid migrated links");
        assert_eq!(cross_universe_links, 0);
    }
}
