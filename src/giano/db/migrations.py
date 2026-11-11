"""Simple schema migrations controlled by the ``schema_version`` table.

To change the schema, append a new entry to ``MIGRATIONS``; never edit an
already released one.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from giano.db.connection import Database

V1 = """
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
"""

MIGRATIONS: list[tuple[int, str]] = [
    (1, V1),
]


def current_version(db: Database) -> int:
    db.conn.execute("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")
    row = db.conn.execute("SELECT MAX(version) FROM schema_version").fetchone()
    return row[0] or 0


def migrate(db: Database) -> int:
    """Apply pending migrations. Returns the resulting schema version."""
    version = current_version(db)
    for target, script in MIGRATIONS:
        if target <= version:
            continue
        # executescript commits implicitly; wrap the script itself in a transaction.
        try:
            db.conn.executescript(
                f"BEGIN IMMEDIATE;\n{script}\n"
                f"INSERT INTO schema_version (version) VALUES ({target});\nCOMMIT;"
            )
        except Exception:
            if db.conn.in_transaction:
                db.conn.execute("ROLLBACK")
            raise
        version = target
    return version
