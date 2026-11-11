"""Repository: quick notes (not versioned)."""

from __future__ import annotations

import sqlite3

from giano.db.connection import Database
from giano.domain import Note, NoteStatus

_SELECT = """
SELECT n.*, p.code AS project_code
FROM notes n LEFT JOIN projects p ON p.id = n.project_id
"""


def _row(r: sqlite3.Row) -> Note:
    return Note(
        id=r["id"],
        note_date=r["note_date"],
        name=r["name"],
        project_id=r["project_id"],
        body=r["body"],
        status=NoteStatus(r["status"]),
        created_at=r["created_at"],
        updated_at=r["updated_at"],
        project_code=r["project_code"],
    )


def insert(
    db: Database, note_date: str, name: str, project_id: int | None, body: str, now: str
) -> int:
    cur = db.execute(
        """INSERT INTO notes (note_date, name, project_id, body, status, created_at, updated_at)
           VALUES (?, ?, ?, ?, 'pending', ?, ?)""",
        (note_date, name, project_id, body, now, now),
    )
    return int(cur.lastrowid)


def update(
    db: Database,
    note_id: int,
    note_date: str,
    name: str,
    project_id: int | None,
    body: str,
    status: NoteStatus,
    now: str,
) -> None:
    db.execute(
        """UPDATE notes SET note_date = ?, name = ?, project_id = ?, body = ?, status = ?,
                            updated_at = ?
           WHERE id = ?""",
        (note_date, name, project_id, body, status.value, now, note_id),
    )


def set_status(db: Database, note_id: int, status: NoteStatus, now: str) -> None:
    db.execute(
        "UPDATE notes SET status = ?, updated_at = ? WHERE id = ?", (status.value, now, note_id)
    )


def delete(db: Database, note_id: int) -> None:
    db.execute("DELETE FROM notes WHERE id = ?", (note_id,))


def get(db: Database, note_id: int) -> Note | None:
    r = db.query_one(_SELECT + " WHERE n.id = ?", (note_id,))
    return _row(r) if r else None


def list_notes(
    db: Database,
    project_id: int | None = None,
    statuses: list[NoteStatus] | None = None,
    since: str | None = None,
    include_unassigned: bool = False,
    oldest_first: bool = False,
) -> list[Note]:
    where, params = [], []
    if project_id is not None:
        if include_unassigned:
            where.append("(n.project_id = ? OR n.project_id IS NULL)")
        else:
            where.append("n.project_id = ?")
        params.append(project_id)
    if statuses:
        where.append(f"n.status IN ({', '.join('?' * len(statuses))})")
        params.extend(s.value for s in statuses)
    if since:
        where.append("n.note_date >= ?")
        params.append(since)
    sql = _SELECT + (" WHERE " + " AND ".join(where) if where else "")
    order = "ASC" if oldest_first else "DESC"
    sql += f" ORDER BY n.note_date {order}, n.created_at {order}, n.id {order}"
    return [_row(r) for r in db.query(sql, params)]


def linked_items(db: Database, note_id: int) -> list[tuple[str, int]]:
    """(project code, item number) of items created from this note."""
    rows = db.query(
        """SELECT p.code, i.number FROM items i JOIN projects p ON p.id = i.project_id
           WHERE i.source_note_id = ? ORDER BY p.code, i.number""",
        (note_id,),
    )
    return [(r[0], r[1]) for r in rows]
