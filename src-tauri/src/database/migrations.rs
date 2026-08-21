/// NarraHub — Database Migrations
/// Creates all tables on first run

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
