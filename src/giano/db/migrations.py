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

# Archiving an item = a new version with archived = 1 (same content otherwise).
V2 = """
ALTER TABLE item_versions
    ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1));
"""

# Addenda: an addendum is an item of its own (own versions, status, revisions) that
# keeps the number of the original item and adds a sub-number: MERC-33.1, MERC-33.2.
# addendum = 0 is a normal item. SQLite cannot drop the old UNIQUE (project_id, number),
# so the table is rebuilt (foreign keys are off while migrating; see migrate()).
V3 = """
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
"""

MIGRATIONS: list[tuple[int, str]] = [
    (1, V1),
    (2, V2),
    (3, V3),
]


def current_version(db: Database) -> int:
    db.conn.execute("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")
    row = db.conn.execute("SELECT MAX(version) FROM schema_version").fetchone()
    return row[0] or 0


def migrate(db: Database) -> int:
    """Apply pending migrations. Returns the resulting schema version.

    Foreign keys are switched off while migrating (the pragma is a no-op inside a
    transaction), so a script can rebuild a referenced table; each migration is
    checked with ``PRAGMA foreign_key_check`` before it commits.
    """
    version = current_version(db)
    pending = [(t, s) for t, s in MIGRATIONS if t > version]
    if not pending:
        return version
    fk_on = db.conn.execute("PRAGMA foreign_keys").fetchone()[0]
    db.conn.execute("PRAGMA foreign_keys = OFF")
    try:
        for target, script in pending:
            # executescript commits a pending transaction first: open it in the script.
            try:
                db.conn.executescript(
                    f"BEGIN IMMEDIATE;\n{script}\n"
                    f"INSERT INTO schema_version (version) VALUES ({target});"
                )
                broken = db.conn.execute("PRAGMA foreign_key_check").fetchall()
                if broken:
                    raise RuntimeError(
                        f"Migração {target}: chaves estrangeiras inválidas {[tuple(r) for r in broken[:5]]}"
                    )
                db.conn.execute("COMMIT")
            except Exception:
                if db.conn.in_transaction:
                    db.conn.execute("ROLLBACK")
                raise
            version = target
    finally:
        db.conn.execute(f"PRAGMA foreign_keys = {'ON' if fk_on else 'OFF'}")
    return version
