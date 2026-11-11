"""Repository: projects."""

from __future__ import annotations

import sqlite3

from giano.db.connection import Database
from giano.domain import Project


def _row(r: sqlite3.Row) -> Project:
    return Project(
        id=r["id"],
        code=r["code"],
        name=r["name"],
        description=r["description"],
        active=bool(r["active"]),
        created_at=r["created_at"],
    )


def insert(db: Database, code: str, name: str, description: str, active: bool, now: str) -> int:
    cur = db.execute(
        "INSERT INTO projects (code, name, description, active, created_at) VALUES (?, ?, ?, ?, ?)",
        (code, name, description, int(active), now),
    )
    return int(cur.lastrowid)


def update(db: Database, project_id: int, code: str, name: str, description: str, active: bool) -> None:
    db.execute(
        "UPDATE projects SET code = ?, name = ?, description = ?, active = ? WHERE id = ?",
        (code, name, description, int(active), project_id),
    )


def get(db: Database, project_id: int) -> Project | None:
    r = db.query_one("SELECT * FROM projects WHERE id = ?", (project_id,))
    return _row(r) if r else None


def get_by_code(db: Database, code: str) -> Project | None:
    r = db.query_one("SELECT * FROM projects WHERE code = ? COLLATE NOCASE", (code,))
    return _row(r) if r else None


def list_all(db: Database, include_archived: bool = True) -> list[Project]:
    sql = "SELECT * FROM projects"
    if not include_archived:
        sql += " WHERE active = 1"
    sql += " ORDER BY active DESC, code"
    return [_row(r) for r in db.query(sql)]
