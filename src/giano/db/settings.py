"""Repository: key/value settings."""

from __future__ import annotations

from giano.db.connection import Database


def get(db: Database, key: str) -> str | None:
    r = db.query_one("SELECT value FROM settings WHERE key = ?", (key,))
    return r[0] if r else None


def set(db: Database, key: str, value: str) -> None:  # noqa: A001 - mirrors dict API
    db.execute(
        "INSERT INTO settings (key, value) VALUES (?, ?) "
        "ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        (key, value),
    )


def delete(db: Database, key: str) -> None:
    db.execute("DELETE FROM settings WHERE key = ?", (key,))
