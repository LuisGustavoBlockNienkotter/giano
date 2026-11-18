"""Service layer: business rules (revisions, notes, exports).

``Giano`` bundles the services over one database; the CLI and the TUI only
talk to it, never to SQL.
"""

from __future__ import annotations

from collections.abc import Callable
from datetime import datetime
from pathlib import Path

from giano.config import DbTarget, resolve_db_target
from giano.db import Database, transfer
from giano.domain import ValidationError
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
        cls,
        path: str | Path | DbTarget | None = None,
        clock: Callable[[], datetime] = datetime.now,
    ) -> Giano:
        """Open the database: ``path`` (file or :class:`DbTarget`) or the configured one
        (``GIANO_DB``, Turso, the local default; see :mod:`giano.config`)."""
        target = resolve_db_target(path)
        return cls(Database(target.location, target.auth_token), clock)

    @property
    def db_path(self) -> str:
        """File path or Turso URL (never the token)."""
        return self.db.path

    @property
    def remote(self) -> bool:
        return self.db.remote

    def copy_to(self, other: Giano) -> dict[str, int]:
        """Copy all data of this database into ``other``, which must be empty
        (e.g. the local file into a new Turso database). Returns rows per table."""
        if not transfer.is_empty(other.db):
            raise ValidationError(
                f"O banco de destino ({other.db_path}) já tem dados; a cópia só é feita "
                "para um banco vazio."
            )
        # The export folder is a setting of each computer, not of the shared database.
        skip = ("export_dir",) if other.remote else ()
        return transfer.copy_all(self.db, other.db, skip_settings=skip)

    def close(self) -> None:
        self.db.close()


__all__ = ["Giano"]
