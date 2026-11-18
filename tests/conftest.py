from __future__ import annotations

from datetime import datetime, timedelta

import pytest

from giano.db import connection
from giano.domain import ItemData, ItemStatus, ItemType, Priority
from giano.services import Giano


class FakeClock:
    """Deterministic clock: each call advances one minute."""

    def __init__(self, start: datetime = datetime(2026, 3, 10, 9, 0, 0)):
        self.now = start

    def __call__(self) -> datetime:
        current = self.now
        self.now += timedelta(minutes=1)
        return current


@pytest.fixture
def clock() -> FakeClock:
    return FakeClock()


@pytest.fixture(autouse=True)
def isolated_config(tmp_path, monkeypatch):
    """Never read or write the real per-computer config (Turso URL/token, export folder)."""
    monkeypatch.setenv("GIANO_CONFIG_DIR", str(tmp_path / "config"))
    for var in ("GIANO_TURSO_URL", "GIANO_TURSO_TOKEN", "GIANO_DB"):
        monkeypatch.delenv(var, raising=False)


@pytest.fixture(params=["sqlite", "libsql"])
def backend(request, monkeypatch) -> str:
    """Run against Python's sqlite3 and against the Turso driver (libsql, on a local file:
    same adapter as a libsql:// URL, without the network)."""
    if request.param == "libsql":
        monkeypatch.setattr(connection, "is_remote_url", lambda location: True)
    return request.param


@pytest.fixture
def g(tmp_path, clock, backend) -> Giano:
    giano = Giano.open(tmp_path / "test.db", clock=clock)
    giano.settings.export_dir = str(tmp_path / "exports")
    yield giano
    giano.close()


@pytest.fixture
def project(g):
    return g.projects.create("MERC", "Mercurius", "Sistema de **gestão**.")


def make(title: str, **kw) -> ItemData:
    defaults = dict(
        type=ItemType.IMPROVEMENT,
        priority=Priority.MEDIUM,
        difficulty=3,
        status=ItemStatus.OPEN,
        tags=(),
        body="",
    )
    defaults.update(kw)
    return ItemData(title=title, **defaults)
