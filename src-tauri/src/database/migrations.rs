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
}
