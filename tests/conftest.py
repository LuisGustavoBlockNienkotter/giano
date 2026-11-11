from __future__ import annotations

from datetime import datetime, timedelta

import pytest

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


@pytest.fixture
def g(tmp_path, clock) -> Giano:
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
