"""Quick notes service. Notes are outside the versioned list: no revisions."""

from __future__ import annotations

from collections.abc import Callable
from datetime import date, datetime

from giano.db import Database
from giano.db import notes as repo
from giano.domain import ItemData, ItemView, Note, NoteStatus, ValidationError, normalize_body
from giano.services.items import ItemService


def parse_date(value: str | date | None) -> str:
    """Validate a ``AAAA-MM-DD`` date. Accepts ``DD/MM/AAAA`` too."""
    if value is None or value == "":
        return date.today().isoformat()
    if isinstance(value, date):
        return value.isoformat()
    value = value.strip()
    for fmt in ("%Y-%m-%d", "%d/%m/%Y"):
        try:
            return datetime.strptime(value, fmt).date().isoformat()
        except ValueError:
            continue
    raise ValidationError(f"Data inválida: '{value}'. Use AAAA-MM-DD.")


class NoteService:
    def __init__(
        self, db: Database, items: ItemService, clock: Callable[[], datetime] = datetime.now
    ):
        self.db = db
        self.items = items
        self.clock = clock

    def _now(self) -> str:
        return self.clock().isoformat(timespec="seconds")

    def default_name(self) -> str:
        return f"Anotação {self.clock().strftime('%d/%m %H:%M')}"

    def create(
        self,
        name: str = "",
        project_id: int | None = None,
        body: str = "",
        note_date: str | date | None = None,
    ) -> Note:
        note_date = parse_date(note_date) if note_date else self.clock().date().isoformat()
        name = " ".join(name.split()) or self.default_name()
        with self.db.transaction():
            nid = repo.insert(self.db, note_date, name, project_id, body, self._now())
        return repo.get(self.db, nid)  # type: ignore[return-value]

    def save(
        self,
        note_id: int,
        *,
        name: str,
        note_date: str,
        project_id: int | None,
        body: str,
        status: NoteStatus,
    ) -> Note:
        """Autosave entry point: overwrite the note with the editor content.

        Body whitespace is preserved exactly while typing.
        """
        note_date = parse_date(note_date)
        name = " ".join(name.split()) or self.default_name()
        with self.db.transaction():
            current = repo.get(self.db, note_id)
            if current is None:
                raise ValidationError("Anotação não encontrada.")
            if (
                current.name == name
                and current.note_date == note_date
                and current.project_id == project_id
                and current.body == body
                and current.status == status
            ):
                return current
            repo.update(self.db, note_id, note_date, name, project_id, body, status, self._now())
        return repo.get(self.db, note_id)  # type: ignore[return-value]

    def set_status(self, note_id: int, status: NoteStatus) -> None:
        with self.db.transaction():
            repo.set_status(self.db, note_id, status, self._now())

    def delete(self, note_id: int) -> None:
        with self.db.transaction():
            repo.delete(self.db, note_id)

    def get(self, note_id: int) -> Note | None:
        return repo.get(self.db, note_id)

    def list(
        self,
        project_id: int | None = None,
        statuses: list[NoteStatus] | None = None,
        since: str | None = None,
        include_unassigned: bool = False,
        oldest_first: bool = False,
    ) -> list[Note]:
        return repo.list_notes(
            self.db, project_id, statuses, parse_date(since) if since else None,
            include_unassigned, oldest_first,
        )

    def linked_items(self, note_id: int) -> list[str]:
        return [f"{code}-{n}" for code, n in repo.linked_items(self.db, note_id)]

    def create_item(self, note_id: int, project_id: int, data: ItemData) -> ItemView:
        """Create an improvement item from a note; the item keeps the link."""
        note = self.get(note_id)
        if note is None:
            raise ValidationError("Anotação não encontrada.")
        return self.items.create(project_id, data, source_note_id=note_id)

    @staticmethod
    def clean_body(body: str) -> str:
        return normalize_body(body)
