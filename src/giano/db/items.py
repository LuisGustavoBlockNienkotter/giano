"""Repository: items, append-only item versions, tags and revisions.

This module only knows SQL. The revision rules (when to create a revision,
summaries, diffs) live in :mod:`giano.services.items`.
"""

from __future__ import annotations

import sqlite3
from dataclasses import replace

from giano.db.connection import Database
from giano.domain import (
    ItemData,
    ItemStatus,
    ItemType,
    ItemView,
    Priority,
    Revision,
    RevisionChange,
)

# ---------------------------------------------------------------------------
# Revisions
# ---------------------------------------------------------------------------


def latest_revision_number(db: Database, project_id: int) -> int:
    r = db.query_one("SELECT MAX(number) FROM revisions WHERE project_id = ?", (project_id,))
    return int(r[0] or 0)


def last_revision(db: Database, project_id: int) -> tuple[int, int, bool] | None:
    """``(id, number, is_open)`` of the latest revision (an open revision is always the
    latest), or ``None`` for a project without revisions. One query."""
    r = db.query_one(
        "SELECT id, number, is_open FROM revisions WHERE project_id = ? ORDER BY number DESC LIMIT 1",
        (project_id,),
    )
    return (int(r["id"]), int(r["number"]), bool(r["is_open"])) if r else None


def insert_revision(
    db: Database, project_id: int, number: int, now: str, summary: str, is_open: bool = False
) -> int:
    cur = db.execute(
        """INSERT INTO revisions (project_id, number, created_at, summary, is_open)
           VALUES (?, ?, ?, ?, ?)""",
        (project_id, number, now, summary, int(is_open)),
    )
    return int(cur.lastrowid)


def update_revision(
    db: Database, revision_id: int, summary: str, is_open: bool, created_at: str | None = None
) -> None:
    if created_at is None:
        db.execute(
            "UPDATE revisions SET summary = ?, is_open = ? WHERE id = ?",
            (summary, int(is_open), revision_id),
        )
    else:
        db.execute(
            "UPDATE revisions SET summary = ?, is_open = ?, created_at = ? WHERE id = ?",
            (summary, int(is_open), created_at, revision_id),
        )


def delete_revision(db: Database, revision_id: int) -> None:
    """Only for a revision that ended up with no change (no version points to it)."""
    db.execute("DELETE FROM revision_changes WHERE revision_id = ?", (revision_id,))
    db.execute("DELETE FROM revisions WHERE id = ?", (revision_id,))


def set_change(db: Database, revision_id: int, item_id: int, summary: str) -> None:
    """Record (or replace) the net change of an item in a revision."""
    db.execute(
        """INSERT INTO revision_changes (revision_id, item_id, summary) VALUES (?, ?, ?)
           ON CONFLICT (revision_id, item_id) DO UPDATE SET summary = excluded.summary""",
        (revision_id, item_id, summary),
    )


def delete_change(db: Database, revision_id: int, item_id: int) -> None:
    db.execute(
        "DELETE FROM revision_changes WHERE revision_id = ? AND item_id = ?",
        (revision_id, item_id),
    )


def change_lines(db: Database, revision_id: int) -> list[str]:
    rows = db.query(
        "SELECT summary FROM revision_changes WHERE revision_id = ? ORDER BY id", (revision_id,)
    )
    return [r[0] for r in rows]


_REVISION_COLUMNS = (
    "r.id, r.project_id, r.number, r.created_at, r.summary, r.is_open, "
    "c.item_id AS change_item, c.summary AS change_summary"
)
_REVISION_JOIN = "FROM revisions r LEFT JOIN revision_changes c ON c.revision_id = r.id"


def _revisions(rows) -> list[Revision]:
    """Group the rows of revisions LEFT JOIN changes (ordered by revision, then change)."""
    out: list[Revision] = []
    changes: dict[int, list[RevisionChange]] = {}
    for r in rows:
        if not out or out[-1].number != r["number"]:
            out.append(
                Revision(
                    r["project_id"], r["number"], r["created_at"], r["summary"], bool(r["is_open"])
                )
            )
            changes[r["number"]] = []
        if r["change_item"] is not None:
            changes[r["number"]].append(RevisionChange(r["change_item"], r["change_summary"]))
    return [replace(rev, changes=tuple(changes[rev.number])) for rev in out]


def list_revisions(db: Database, project_id: int) -> list[Revision]:
    """All revisions of a project with their changes, newest first. One query."""
    rows = db.query(
        f"SELECT {_REVISION_COLUMNS} {_REVISION_JOIN} WHERE r.project_id = ? "
        "ORDER BY r.number DESC, c.id",
        (project_id,),
    )
    return _revisions(rows)


def get_revision(db: Database, project_id: int, number: int) -> Revision | None:
    rows = db.query(
        f"SELECT {_REVISION_COLUMNS} {_REVISION_JOIN} WHERE r.project_id = ? AND r.number = ? "
        "ORDER BY c.id",
        (project_id, number),
    )
    revs = _revisions(rows)
    return revs[0] if revs else None


def get_open_revision(db: Database, project_id: int) -> Revision | None:
    last = last_revision(db, project_id)
    return get_revision(db, project_id, last[1]) if last and last[2] else None


# ---------------------------------------------------------------------------
# Items and versions
# ---------------------------------------------------------------------------


def next_item_number(db: Database, project_id: int) -> int:
    r = db.query_one("SELECT MAX(number) FROM items WHERE project_id = ?", (project_id,))
    return int(r[0] or 0) + 1


def next_addendum_number(db: Database, project_id: int, number: int) -> int:
    r = db.query_one(
        "SELECT MAX(addendum) FROM items WHERE project_id = ? AND number = ?", (project_id, number)
    )
    return int(r[0] or 0) + 1


def insert_item(
    db: Database,
    project_id: int,
    number: int,
    source_note_id: int | None,
    now: str,
    addendum: int = 0,
) -> int:
    cur = db.execute(
        """INSERT INTO items (project_id, number, addendum, source_note_id, created_at)
           VALUES (?, ?, ?, ?, ?)""",
        (project_id, number, addendum, source_note_id, now),
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


def set_control_date(db: Database, item_ids: list[int], day: str | None) -> None:
    """Set (or clear, ``None``) the control date of these items. Not versioned. One
    statement for all of them (each statement is a round trip on Turso)."""
    if not item_ids:
        return
    marks = ", ".join("?" * len(item_ids))
    db.execute(f"UPDATE items SET control_date = ? WHERE id IN ({marks})", (day, *item_ids))


def close_version(db: Database, version_id: int, rev_end: int) -> None:
    cur = db.execute(
        "UPDATE item_versions SET rev_end = ? WHERE id = ? AND rev_end IS NULL",
        (rev_end, version_id),
    )
    if cur.rowcount != 1:
        raise RuntimeError(f"Versão {version_id} não está vigente")


def reopen_version(db: Database, version_id: int) -> None:
    """Make a closed version current again (its successor, created in a revision that is
    still collecting changes, was undone)."""
    db.execute("UPDATE item_versions SET rev_end = NULL WHERE id = ?", (version_id,))


def delete_version(db: Database, version_id: int) -> None:
    """Remove a version created by a revision that is still collecting changes (an open
    revision or a batch), when the same item changes again in that revision. Versions of
    finished revisions are never deleted."""
    db.execute("DELETE FROM item_version_tags WHERE version_id = ?", (version_id,))
    db.execute("DELETE FROM item_fts WHERE rowid = ?", (version_id,))
    db.execute("DELETE FROM item_versions WHERE id = ?", (version_id,))


_SELECT = """
SELECT i.id AS item_id, i.project_id, p.code AS project_code, i.number, i.addendum,
       i.source_note_id, i.created_at, i.control_date,
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
        addendum=r["addendum"],
        created_at=r["created_at"],
        control_date=r["control_date"],
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
        _SELECT + f" WHERE i.project_id = :project AND {_AT_REVISION} ORDER BY i.number, i.addendum",
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


def get_current_by_number(
    db: Database, project_id: int, number: int, addendum: int = 0
) -> ItemView | None:
    r = db.query_one(
        _SELECT
        + " WHERE i.project_id = ? AND i.number = ? AND i.addendum = ? AND v.rev_end IS NULL",
        (project_id, number, addendum),
    )
    return _view(r) if r else None


def get_by_number_at_revision(
    db: Database, project_id: int, number: int, addendum: int, rev: int
) -> ItemView | None:
    r = db.query_one(
        _SELECT
        + f""" WHERE i.project_id = :project AND i.number = :number AND i.addendum = :addendum
               AND {_AT_REVISION}""",
        {"project": project_id, "number": number, "addendum": addendum, "rev": rev},
    )
    return _view(r) if r else None


def addenda_at_revision(db: Database, project_id: int, number: int, rev: int) -> list[ItemView]:
    """The addenda of item ``number`` (``number.1``, ``number.2``…) at revision ``rev``."""
    rows = db.query(
        _SELECT
        + f""" WHERE i.project_id = :project AND i.number = :number AND i.addendum > 0
               AND {_AT_REVISION} ORDER BY i.addendum""",
        {"project": project_id, "number": number, "rev": rev},
    )
    return [_view(r) for r in rows]


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
