"""Service layer: business rules (revisions, notes, exports).

``Giano`` bundles the services over one database; the CLI and the TUI only
talk to it, never to SQL.
"""

from __future__ import annotations

from collections.abc import Callable
from datetime import datetime
from pathlib import Path

from giano.config import default_db_path
from giano.db import Database
from giano.services.exports import Exporter
from giano.services.items import ItemService
from giano.services.notes import NoteService
from giano.services.projects import ProjectService
from giano.services.search import SearchService
from giano.services.settings import SettingsService


class Giano:
    def __init__(self, db: Database, clock: Callable[[], datetime] = datetime.now):
        self.db = db
        self.projects = ProjectService(db, clock)
        self.items = ItemService(db, clock)
        self.notes = NoteService(db, self.items, clock)
        self.settings = SettingsService(db)
        self.search = SearchService(db)
        self.exports = Exporter(self.projects, self.items, self.notes, self.settings, clock)

    @classmethod
    def open(
        cls, path: str | Path | None = None, clock: Callable[[], datetime] = datetime.now
    ) -> Giano:
        return cls(Database(path or default_db_path()), clock)

    @property
    def db_path(self) -> str:
        return self.db.path

    def close(self) -> None:
        self.db.close()


__all__ = ["Giano"]
