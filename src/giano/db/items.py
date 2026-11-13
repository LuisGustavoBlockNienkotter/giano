"""Repository: items, append-only item versions, tags and revisions.

This module only knows SQL. The revision rules (when to create a revision,
summaries, diffs) live in :mod:`giano.services.items`.
"""

from __future__ import annotations

import sqlite3

from giano.db.connection import Database
from giano.domain import ItemData, ItemStatus, ItemType, ItemView, Priority, Revision

# ---------------------------------------------------------------------------
# Revisions
# ---------------------------------------------------------------------------


def latest_revision_number(db: Database, project_id: int) -> int:
    r = db.query_one("SELECT MAX(number) FROM revisions WHERE project_id = ?", (project_id,))
    return int(r[0] or 0)


def insert_revision(
    db: Database, project_id: int, number: int, now: str, summary: str, item_id: int | None
) -> None:
    db.execute(
        "INSERT INTO revisions (project_id, number, created_at, summary, item_id) VALUES (?, ?, ?, ?, ?)",
        (project_id, number, now, summary, item_id),
    )


def _rev(r: sqlite3.Row) -> Revision:
    return Revision(r["project_id"], r["number"], r["created_at"], r["summary"], r["item_id"])


def list_revisions(db: Database, project_id: int) -> list[Revision]:
    rows = db.query(
        "SELECT * FROM revisions WHERE project_id = ? ORDER BY number DESC", (project_id,)
    )
    return [_rev(r) for r in rows]


def get_revision(db: Database, project_id: int, number: int) -> Revision | None:
    r = db.query_one(
        "SELECT * FROM revisions WHERE project_id = ? AND number = ?", (project_id, number)
    )
    return _rev(r) if r else None


# ---------------------------------------------------------------------------
# Items and versions
# ---------------------------------------------------------------------------


def next_item_number(db: Database, project_id: int) -> int:
    r = db.query_one("SELECT MAX(number) FROM items WHERE project_id = ?", (project_id,))
    return int(r[0] or 0) + 1


def insert_item(
    db: Database, project_id: int, number: int, source_note_id: int | None, now: str
) -> int:
    cur = db.execute(
        "INSERT INTO items (project_id, number, source_note_id, created_at) VALUES (?, ?, ?, ?)",
        (project_id, number, source_note_id, now),
    )
    return int(cur.lastrowid)


def _tag_id(db: Database, name: str) -> int:
    r = db.query_one("SELECT id FROM tags WHERE name = ?", (name,))
    if r:
        return int(r[0])
    return int(db.execute("INSERT INTO tags (name) VALUES (?)", (name,)).lastrowid)


def insert_version(db: Database, item_id: int, rev_start: int, data: ItemData) -> int:
    cur = db.execute(
        """INSERT INTO item_versions
               (item_id, rev_start, rev_end, title, type, priority, difficulty, status, body,
                archived)
           VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?)""",
        (
            item_id,
            rev_start,
            data.title,
            data.type.value,
            data.priority.value,
            data.difficulty,
            data.status.value,
            data.body,
            int(data.archived),
        ),
    )
    version_id = int(cur.lastrowid)
    for tag in data.tags:
        db.execute(
            "INSERT INTO item_version_tags (version_id, tag_id) VALUES (?, ?)",
            (version_id, _tag_id(db, tag)),
        )
    db.execute(
        "INSERT INTO item_fts (rowid, title, body, tags) VALUES (?, ?, ?, ?)",
        (version_id, data.title, data.body, " ".join(data.tags)),
    )
    return version_id


def close_version(db: Database, version_id: int, rev_end: int) -> None:
    cur = db.execute(
        "UPDATE item_versions SET rev_end = ? WHERE id = ? AND rev_end IS NULL",
        (rev_end, version_id),
    )
    if cur.rowcount != 1:
        raise RuntimeError(f"Versão {version_id} não está vigente")


_SELECT = """
SELECT i.id AS item_id, i.project_id, p.code AS project_code, i.number, i.source_note_id,
       v.id AS version_id, v.rev_start, v.rev_end, v.title, v.type, v.priority,
       v.difficulty, v.status, v.body, v.archived,
       (SELECT group_concat(name, char(31)) FROM (
            SELECT t.name FROM item_version_tags vt JOIN tags t ON t.id = vt.tag_id
            WHERE vt.version_id = v.id ORDER BY t.name)) AS tags
FROM items i
JOIN projects p ON p.id = i.project_id
JOIN item_versions v ON v.item_id = i.id
"""

_AT_REVISION = "v.rev_start <= :rev AND (v.rev_end IS NULL OR v.rev_end > :rev)"


def _view(r: sqlite3.Row) -> ItemView:
    tags = tuple(sorted(r["tags"].split("\x1f"))) if r["tags"] else ()
    return ItemView(
        item_id=r["item_id"],
        project_id=r["project_id"],
        project_code=r["project_code"],
        number=r["number"],
        version_id=r["version_id"],
        rev_start=r["rev_start"],
        rev_end=r["rev_end"],
        source_note_id=r["source_note_id"],
        data=ItemData(
            title=r["title"],
            type=ItemType(r["type"]),
            priority=Priority(r["priority"]),
            difficulty=r["difficulty"],
            status=ItemStatus(r["status"]),
            tags=tags,
            body=r["body"],
            archived=bool(r["archived"]),
        ),
    )


def list_at_revision(db: Database, project_id: int, rev: int) -> list[ItemView]:
    rows = db.query(
        _SELECT + f" WHERE i.project_id = :project AND {_AT_REVISION} ORDER BY i.number",
        {"project": project_id, "rev": rev},
    )
    return [_view(r) for r in rows]


def get_current(db: Database, item_id: int) -> ItemView | None:
    r = db.query_one(_SELECT + " WHERE i.id = ? AND v.rev_end IS NULL", (item_id,))
    return _view(r) if r else None


def get_at_revision(db: Database, item_id: int, rev: int) -> ItemView | None:
    r = db.query_one(
        _SELECT + f" WHERE i.id = :item AND {_AT_REVISION}", {"item": item_id, "rev": rev}
    )
    return _view(r) if r else None


def get_current_by_number(db: Database, project_id: int, number: int) -> ItemView | None:
    r = db.query_one(
        _SELECT + " WHERE i.project_id = ? AND i.number = ? AND v.rev_end IS NULL",
        (project_id, number),
    )
    return _view(r) if r else None


def item_history(db: Database, item_id: int) -> list[ItemView]:
    rows = db.query(_SELECT + " WHERE i.id = ? ORDER BY v.rev_start", (item_id,))
    return [_view(r) for r in rows]


def all_tags(db: Database) -> list[str]:
    """Tags used by at least one current version (for autocomplete)."""
    rows = db.query(
        """SELECT DISTINCT t.name FROM tags t
           JOIN item_version_tags vt ON vt.tag_id = t.id
           JOIN item_versions v ON v.id = vt.version_id
           WHERE v.rev_end IS NULL
           ORDER BY t.name"""
    )
    return [r[0] for r in rows]
