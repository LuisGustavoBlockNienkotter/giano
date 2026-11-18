"""Turso (libSQL) remote connection behind the small ``sqlite3`` surface the repositories use.

The ``libsql`` driver returns plain tuples, accepts only positional parameters and raises
``ValueError`` for SQL errors. :class:`RemoteConnection` hides those differences so the
rest of the ``db`` package works unchanged: rows support ``row["col"]`` and ``row[0]``,
``:name`` parameters are rewritten to ``?NNN``, and errors become ``sqlite3`` exceptions
(``IntegrityError`` for constraint violations). Network and access failures become
:class:`DatabaseUnavailable`, a ``ValidationError`` with a pt-BR message, so the UI shows
them like any other refused operation (the editor stays open, nothing typed is lost).
"""

from __future__ import annotations

import re
import sqlite3
from collections.abc import Iterator, Mapping, Sequence
from typing import Any

from giano.domain import ValidationError

REMOTE_SCHEMES = ("libsql://", "https://", "http://", "wss://", "ws://")


def is_remote_url(location: str) -> bool:
    return location.lower().startswith(REMOTE_SCHEMES)


class Row:
    """Read-only row with access by position or by column name (like ``sqlite3.Row``)."""

    __slots__ = ("_values", "_index")

    def __init__(self, values: tuple, index: dict[str, int]):
        self._values = values
        self._index = index

    def __getitem__(self, key: int | str | slice) -> Any:
        if isinstance(key, str):
            return self._values[self._index[key]]
        return self._values[key]

    def keys(self) -> list[str]:
        return list(self._index)

    def __iter__(self) -> Iterator[Any]:
        return iter(self._values)

    def __len__(self) -> int:
        return len(self._values)

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Row):
            return self._values == other._values and self._index == other._index
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self._values)

    def __repr__(self) -> str:
        return f"Row({dict(zip(self._index, self._values))!r})"


class RemoteCursor:
    def __init__(self, raw: Any):
        self._raw = raw
        description = raw.description or ()
        self.description = description
        self._index = {d[0]: i for i, d in enumerate(description)}

    @property
    def lastrowid(self) -> int | None:
        return self._raw.lastrowid

    @property
    def rowcount(self) -> int:
        return self._raw.rowcount

    def _row(self, values: tuple | None) -> Row | None:
        return None if values is None else Row(tuple(values), self._index)

    def fetchone(self) -> Row | None:
        return self._row(_call(self._raw.fetchone))

    def fetchall(self) -> list[Row]:
        return [Row(tuple(v), self._index) for v in _call(self._raw.fetchall)]

    def __iter__(self) -> Iterator[Row]:
        return iter(self.fetchall())


# ':name' outside quotes. Quoted strings/identifiers are matched first and kept as they are.
_TOKENS = re.compile(r"""('(?:[^']|'')*'|"(?:[^"]|"")*"|`[^`]*`|\[[^\]]*\])|(?<!:):([A-Za-z_]\w*)""")


def to_positional(sql: str, params: Sequence | Mapping | None) -> tuple[str, tuple]:
    """Rewrite ``:name`` parameters as ``?N`` (libsql takes only positional values)."""
    if params is None:
        return sql, ()
    if not isinstance(params, Mapping):
        return sql, tuple(params)
    order: dict[str, int] = {}

    def replace(m: re.Match) -> str:
        if m.group(1) is not None:
            return m.group(1)
        name = m.group(2)
        if name not in order:
            order[name] = len(order) + 1
        return f"?{order[name]}"

    rewritten = _TOKENS.sub(replace, sql)
    try:
        values = tuple(params[name] for name in order)
    except KeyError as exc:
        raise sqlite3.ProgrammingError(f"Parâmetro sem valor: {exc.args[0]}") from exc
    return rewritten, values


class DatabaseUnavailable(ValidationError, sqlite3.OperationalError):
    """Turso could not be reached or refused the access (no internet, bad token...)."""


def _translate(exc: Exception) -> sqlite3.Error:
    message = str(exc)
    lower = message.lower()
    # libsql: "Hrana: `http error: ...`" (network), "Hrana: `api error: status=..." (HTTP
    # status from Turso), "Hrana: `stream error: ...SQLite error..." (the SQL itself).
    if "http error" in lower:
        return DatabaseUnavailable(f"Sem conexão com o Turso (verifique a internet). Detalhe: {message}")
    if "api error" in lower:
        if "jwt" in lower or "401" in lower or "403" in lower:
            return DatabaseUnavailable(
                "O Turso recusou o token (inválido ou expirado). Gere outro e rode "
                f"'giano turso configurar'. Detalhe: {message}"
            )
        return DatabaseUnavailable(f"O Turso recusou a conexão. Detalhe: {message}")
    if "constraint failed" in lower:
        return sqlite3.IntegrityError(message)
    return sqlite3.OperationalError(message)


def _call(fn, *args):
    try:
        return fn(*args)
    except sqlite3.Error:
        raise
    except Exception as exc:  # libsql raises ValueError (SQL) or its own errors (network)
        raise _translate(exc) from exc


class RemoteConnection:
    """The subset of ``sqlite3.Connection`` used by :class:`giano.db.Database`."""

    def __init__(self, url: str, auth_token: str):
        import libsql

        self._raw = _call(
            lambda: libsql.connect(url, auth_token=auth_token, isolation_level=None)
        )

    def execute(self, sql: str, params: Sequence | Mapping | None = ()) -> RemoteCursor:
        sql, values = to_positional(sql, params)
        return RemoteCursor(_call(self._raw.execute, sql, values))

    def executescript(self, script: str) -> None:
        _call(self._raw.executescript, script)

    @property
    def in_transaction(self) -> bool:
        return bool(self._raw.in_transaction)

    def close(self) -> None:
        try:
            self._raw.close()
        except Exception:  # pragma: no cover - closing a dead remote stream
            pass
