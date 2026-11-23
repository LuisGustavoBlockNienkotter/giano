//! Schema migrations controlled by the `schema_version` table.
//!
//! These are the SAME scripts as `src/giano/db/migrations.py` (the Python TUI and this app
//! share the database). To change the schema, append a new entry to both lists, with the
//! same number; never edit an already released one.

use crate::db::Database;
use crate::error::{Error, Result};
use crate::params;

pub const V1: &str = r#"
CREATE TABLE projects (
    id          INTEGER PRIMARY KEY,
    code        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    active      INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL
);

-- One row per change of a project's list. Revision 0 is the implicit empty list.
CREATE TABLE revisions (
    id         INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    number     INTEGER NOT NULL,
    created_at TEXT    NOT NULL,
    summary    TEXT    NOT NULL,
    item_id    INTEGER REFERENCES items(id),
    UNIQUE (project_id, number)
);

CREATE TABLE notes (
    id         INTEGER PRIMARY KEY,
    note_date  TEXT    NOT NULL,
    name       TEXT    NOT NULL,
    project_id INTEGER REFERENCES projects(id),
    body       TEXT    NOT NULL DEFAULT '',
    status     TEXT    NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending', 'processed', 'archived')),
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL
);
CREATE INDEX notes_by_project ON notes (project_id, note_date);

-- Stable identity of an item (never updated, never deleted).
CREATE TABLE items (
    id             INTEGER PRIMARY KEY,
    project_id     INTEGER NOT NULL REFERENCES projects(id),
    number         INTEGER NOT NULL,
    source_note_id INTEGER REFERENCES notes(id) ON DELETE SET NULL,
    created_at     TEXT    NOT NULL,
    UNIQUE (project_id, number)
);

-- Append-only temporal content. A version is visible at revision N when
-- rev_start <= N AND (rev_end IS NULL OR rev_end > N).
CREATE TABLE item_versions (
    id         INTEGER PRIMARY KEY,
    item_id    INTEGER NOT NULL REFERENCES items(id),
    rev_start  INTEGER NOT NULL,
    rev_end    INTEGER,
    title      TEXT    NOT NULL,
    type       TEXT    NOT NULL CHECK (type IN ('improvement', 'discussion', 'adjustment')),
    priority   TEXT    NOT NULL CHECK (priority IN ('critical', 'high', 'medium', 'low')),
    difficulty INTEGER NOT NULL CHECK (difficulty BETWEEN 1 AND 5),
    status     TEXT    NOT NULL CHECK (status IN
                   ('open', 'analysis', 'approved', 'in_progress', 'done', 'discarded')),
    body       TEXT    NOT NULL DEFAULT '',
    CHECK (rev_end IS NULL OR rev_end > rev_start)
);
CREATE INDEX item_versions_by_item ON item_versions (item_id, rev_start);
CREATE UNIQUE INDEX item_versions_one_current ON item_versions (item_id) WHERE rev_end IS NULL;

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

-- Tags belong to a version, not to the item.
CREATE TABLE item_version_tags (
    version_id INTEGER NOT NULL REFERENCES item_versions(id),
    tag_id     INTEGER NOT NULL REFERENCES tags(id),
    PRIMARY KEY (version_id, tag_id)
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Full-text search. item_fts rowid = item_versions.id (maintained by the repository).
CREATE VIRTUAL TABLE item_fts USING fts5(
    title, body, tags, tokenize = 'unicode61 remove_diacritics 2'
);
-- note_fts rowid = notes.id (maintained by triggers).
CREATE VIRTUAL TABLE note_fts USING fts5(
    name, body, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER notes_ai AFTER INSERT ON notes BEGIN
    INSERT INTO note_fts (rowid, name, body) VALUES (new.id, new.name, new.body);
END;
CREATE TRIGGER notes_au AFTER UPDATE OF name, body ON notes BEGIN
    DELETE FROM note_fts WHERE rowid = old.id;
    INSERT INTO note_fts (rowid, name, body) VALUES (new.id, new.name, new.body);
END;
CREATE TRIGGER notes_ad AFTER DELETE ON notes BEGIN
    DELETE FROM note_fts WHERE rowid = old.id;
END;
"#;

/// Archiving an item = a new version with archived = 1 (same content otherwise).
pub const V2: &str = r#"
ALTER TABLE item_versions
    ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1));
"#;

/// Addenda: an addendum is an item of its own that keeps the number of the original item and
/// adds a sub-number (MERC-33.1). SQLite cannot drop the old UNIQUE (project_id, number), so
/// the table is rebuilt (foreign keys are off while migrating; see `migrate`).
pub const V3: &str = r#"
CREATE TABLE items_new (
    id             INTEGER PRIMARY KEY,
    project_id     INTEGER NOT NULL REFERENCES projects(id),
    number         INTEGER NOT NULL,
    addendum       INTEGER NOT NULL DEFAULT 0 CHECK (addendum >= 0),
    source_note_id INTEGER REFERENCES notes(id) ON DELETE SET NULL,
    created_at     TEXT    NOT NULL,
    UNIQUE (project_id, number, addendum)
);
INSERT INTO items_new (id, project_id, number, addendum, source_note_id, created_at)
    SELECT id, project_id, number, 0, source_note_id, created_at FROM items;
DROP TABLE items;
ALTER TABLE items_new RENAME TO items;
"#;

/// Revisions with several changes: a block of changes = one revision. `revision_changes` has
/// one line per item changed by a revision (its net change); it replaces `revisions.item_id`
/// (kept, no longer written). A project may have one OPEN revision (`is_open = 1`, always its
/// latest): every change goes into it until it is closed.
pub const V4: &str = r#"
ALTER TABLE revisions
    ADD COLUMN is_open INTEGER NOT NULL DEFAULT 0 CHECK (is_open IN (0, 1));
CREATE UNIQUE INDEX revisions_one_open ON revisions (project_id) WHERE is_open = 1;
CREATE TABLE revision_changes (
    id          INTEGER PRIMARY KEY,
    revision_id INTEGER NOT NULL REFERENCES revisions(id),
    item_id     INTEGER NOT NULL REFERENCES items(id),
    summary     TEXT    NOT NULL,
    UNIQUE (revision_id, item_id)
);
INSERT INTO revision_changes (revision_id, item_id, summary)
    SELECT id, item_id, summary FROM revisions WHERE item_id IS NOT NULL;
"#;

/// Control date: a per-item date the user keeps for their own control (shown as DD/MM/YY in
/// the list). Set to today by the HTML export, or by hand. Not versioned: the only UPDATE on
/// `items` (its identity columns never change).
pub const V5: &str = r#"
ALTER TABLE items ADD COLUMN control_date TEXT;
"#;

pub const MIGRATIONS: &[(i64, &str)] = &[(1, V1), (2, V2), (3, V3), (4, V4), (5, V5)];

pub fn current_version(db: &Database) -> Result<i64> {
    db.execute("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)", params![])?;
    let row = db.query_one("SELECT MAX(version) FROM schema_version", params![])?;
    Ok(row.and_then(|r| r.at::<Option<i64>>(0)).unwrap_or(0))
}

/// Apply pending migrations. Returns the resulting schema version.
///
/// Foreign keys are switched off while migrating (the pragma is a no-op inside a
/// transaction), so a script can rebuild a referenced table; each migration is checked with
/// `PRAGMA foreign_key_check` before it commits.
pub fn migrate(db: &Database) -> Result<i64> {
    db.session(|| {
        let mut version = current_version(db)?;
        let pending: Vec<_> = MIGRATIONS.iter().filter(|(t, _)| *t > version).collect();
        if pending.is_empty() {
            return Ok(version);
        }
        let fk_on = db
            .query_one("PRAGMA foreign_keys", params![])?
            .map(|r| r.at::<i64>(0) != 0)
            .unwrap_or(true);
        db.execute("PRAGMA foreign_keys = OFF", params![])?;
        let result: Result<i64> = (|| {
            for (target, script) in pending {
                let applied: Result<()> = (|| {
                    db.execute_script(&format!(
                        "BEGIN IMMEDIATE;\n{script}\nINSERT INTO schema_version (version) VALUES ({target});"
                    ))?;
                    let broken = db.query("PRAGMA foreign_key_check", params![])?;
                    if !broken.is_empty() {
                        let sample: Vec<String> =
                            broken.iter().take(5).map(|r| format!("{:?}", r.values())).collect();
                        return Err(Error::Database(format!(
                            "Migração {target}: chaves estrangeiras inválidas {}",
                            sample.join(", ")
                        )));
                    }
                    db.execute("COMMIT", params![])?;
                    Ok(())
                })();
                if let Err(e) = applied {
                    let _ = db.execute("ROLLBACK", params![]);
                    return Err(e);
                }
                version = *target;
            }
            Ok(version)
        })();
        let restore = db.execute(
            if fk_on { "PRAGMA foreign_keys = ON" } else { "PRAGMA foreign_keys = OFF" },
            params![],
        );
        let version = result?;
        restore?;
        Ok(version)
    })
}
