"""Database connection (local SQLite file or Turso) and explicit transactions."""

from __future__ import annotations

import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

from giano.db.migrations import migrate
from giano.db.remote import RemoteConnection, is_remote_url


class Database:
    """Thin wrapper around a ``sqlite3.Connection`` (or :class:`RemoteConnection`).

    ``location`` is a SQLite file (or ``:memory:``) or a Turso/libSQL URL
    (``libsql://...``), which needs ``auth_token``.

    The connection runs in autocommit mode; every write goes through
    :meth:`transaction`, which opens ``BEGIN IMMEDIATE`` so that a whole change
    (e.g. a new revision) is atomic and serialized against other processes
    (TUI and CLI may run at the same time, on this or on other computers).
    """

    def __init__(self, location: str | Path, auth_token: str | None = None):
        location = str(location)
        self.remote = is_remote_url(location)
        self.path = location
        if self.remote:
            self.conn = RemoteConnection(location, auth_token or "")
            self.conn.execute("PRAGMA foreign_keys = ON")
        else:
            if location != ":memory:":
                Path(location).parent.mkdir(parents=True, exist_ok=True)
            self.conn = sqlite3.connect(location, isolation_level=None, check_same_thread=False)
            self.conn.row_factory = sqlite3.Row
            self.conn.execute("PRAGMA foreign_keys = ON")
            self.conn.execute("PRAGMA busy_timeout = 5000")
            if location != ":memory:":
                self.conn.execute("PRAGMA journal_mode = WAL")
        self._depth = 0
        migrate(self)

    @contextmanager
    def transaction(self) -> Iterator[sqlite3.Connection]:
        """Run a block in a single transaction. Nested calls join the outer one."""
        if self._depth:
            self._depth += 1
            try:
                yield self.conn
            finally:
                self._depth -= 1
            return
        self.conn.execute("BEGIN IMMEDIATE")
        self._depth = 1
        try:
            yield self.conn
            self.conn.execute("COMMIT")
        except BaseException:
            self._rollback()
            raise
        finally:
            self._depth = 0

    def _rollback(self) -> None:
        # Never hide the original error (e.g. the network dropped and ROLLBACK fails too).
        try:
            self.conn.execute("ROLLBACK")
        except Exception:  # no transaction left open (e.g. COMMIT failed and SQLite rolled back)
            pass

    def execute(self, sql: str, params=()) -> sqlite3.Cursor:
        return self.conn.execute(sql, params)

    def query(self, sql: str, params=()) -> list[sqlite3.Row]:
        return self.conn.execute(sql, params).fetchall()

    def query_one(self, sql: str, params=()) -> sqlite3.Row | None:
        return self.conn.execute(sql, params).fetchone()

    def close(self) -> None:
        self.conn.close()
