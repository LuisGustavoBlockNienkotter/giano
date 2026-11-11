"""Repository: FTS5 full-text search over current items and notes."""

from __future__ import annotations

import re

from giano.db.connection import Database

_TOKEN_RE = re.compile(r"\w+", re.UNICODE)


def fts_query(text: str) -> str | None:
    """Turn free user text into a safe FTS5 query (AND of prefix terms)."""
    tokens = _TOKEN_RE.findall(text)
    if not tokens:
        return None
    return " ".join(f'"{t}"*' for t in tokens)


def search_items(db: Database, text: str, project_id: int | None = None, limit: int = 100):
    q = fts_query(text)
    if q is None:
        return []
    sql = """
        SELECT i.id AS item_id, p.code || '-' || i.number AS code, p.id AS project_id,
               v.title, v.status,
               snippet(item_fts, -1, '[', ']', '…', 12) AS snippet
        FROM item_fts
        JOIN item_versions v ON v.id = item_fts.rowid AND v.rev_end IS NULL
        JOIN items i ON i.id = v.item_id
        JOIN projects p ON p.id = i.project_id
        WHERE item_fts MATCH ?
    """
    params: list = [q]
    if project_id is not None:
        sql += " AND p.id = ?"
        params.append(project_id)
    sql += " ORDER BY bm25(item_fts) LIMIT ?"
    params.append(limit)
    return db.query(sql, params)


def search_notes(db: Database, text: str, project_id: int | None = None, limit: int = 100):
    q = fts_query(text)
    if q is None:
        return []
    sql = """
        SELECT n.id AS note_id, n.note_date, n.name, n.status, p.code AS project_code,
               snippet(note_fts, -1, '[', ']', '…', 12) AS snippet
        FROM note_fts
        JOIN notes n ON n.id = note_fts.rowid
        LEFT JOIN projects p ON p.id = n.project_id
        WHERE note_fts MATCH ?
    """
    params: list = [q]
    if project_id is not None:
        sql += " AND n.project_id = ?"
        params.append(project_id)
    sql += " ORDER BY bm25(note_fts) LIMIT ?"
    params.append(limit)
    return db.query(sql, params)
