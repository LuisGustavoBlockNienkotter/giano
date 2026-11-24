"""Copy every row of one Giano database into another, empty one (e.g. local file -> Turso).

Both databases are already migrated to the same schema (``Database`` migrates on open).
Rows keep their ids, so revisions, versions and tags stay linked. ``note_fts`` is filled
by the ``notes`` triggers; ``item_fts`` is copied (it is written by the repository).
"""

from __future__ import annotations

from giano.db.connection import Database

# Parents before children (foreign keys).
TABLES = (
    "projects",
    "notes",
    "items",
    "revisions",
    "revision_changes",
    "item_versions",
    "tags",
    "item_version_tags",
    "settings",
    "repo_checks",
)
_MAX_PARAMS = 900  # stay below SQLite's oldest limit (999 bound values per statement)


def is_empty(db: Database) -> bool:
    """No user data (settings alone don't count: they are overwritten by the copy)."""
    return not any(db.query_one(f"SELECT 1 FROM {t} LIMIT 1") for t in TABLES if t != "settings")


def _columns(db: Database, table: str) -> list[str]:
    return [r["name"] for r in db.query(f"PRAGMA table_info({table})")]


def _insert_rows(
    dst: Database, table: str, columns: list[str], rows: list[tuple], verb: str = "INSERT"
) -> None:
    """Multi-row INSERTs: one round trip per chunk instead of per row (matters on Turso)."""
    per_chunk = max(1, _MAX_PARAMS // len(columns))
    cols = ", ".join(columns)
    one = "(" + ", ".join("?" for _ in columns) + ")"
    for start in range(0, len(rows), per_chunk):
        chunk = rows[start : start + per_chunk]
        values = [v for row in chunk for v in row]
        dst.execute(f"{verb} INTO {table} ({cols}) VALUES {', '.join([one] * len(chunk))}", values)


def copy_all(src: Database, dst: Database, skip_settings: tuple[str, ...] = ()) -> dict[str, int]:
    """Copy all rows from ``src`` to ``dst`` in one transaction. Returns rows per table."""
    counts: dict[str, int] = {}
    with dst.transaction():
        for table in TABLES:
            columns = _columns(src, table)
            rows = [tuple(r) for r in src.query(f"SELECT {', '.join(columns)} FROM {table}")]
            if table == "settings":
                rows = [r for r in rows if r[columns.index("key")] not in skip_settings]
            verb = "INSERT OR REPLACE" if table == "settings" else "INSERT"
            _insert_rows(dst, table, columns, rows, verb)
            counts[table] = len(rows)
        fts = [tuple(r) for r in src.query("SELECT rowid, title, body, tags FROM item_fts")]
        _insert_rows(dst, "item_fts", ["rowid", "title", "body", "tags"], fts)
    return counts
