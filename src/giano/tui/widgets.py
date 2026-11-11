"""Shared TUI widgets and helpers."""

from __future__ import annotations

from collections.abc import Callable

from rich.text import Text
from textual.suggester import Suggester
from textual.widgets import Select

from giano.domain import DIFFICULTY_RANGE, ItemStatus, ItemType, NoteStatus, Priority


class TagSuggester(Suggester):
    """Autocompletes the last tag of a comma-separated list with existing tags."""

    def __init__(self, tags_source: Callable[[], list[str]]):
        super().__init__(use_cache=False, case_sensitive=False)
        self._tags_source = tags_source

    async def get_suggestion(self, value: str) -> str | None:
        head, sep, last = value.rpartition(",")
        prefix = last.lstrip().lower()
        if not prefix:
            return None
        used = {t.strip().lower() for t in head.split(",")}
        for tag in self._tags_source():
            if tag.lower().startswith(prefix) and tag.lower() != prefix and tag not in used:
                lead = last[: len(last) - len(last.lstrip())]
                return f"{head}{sep}{lead}{tag}"
        return None


def type_select(value: ItemType | None = None, *, blank: bool = False, id: str | None = None) -> Select:
    return Select(
        [(t.label, t) for t in ItemType],
        prompt="Tipo",
        allow_blank=blank,
        value=value if value is not None else Select.NULL,
        id=id,
    )


def priority_select(value: Priority | None = None, *, blank: bool = False, id: str | None = None) -> Select:
    return Select(
        [(p.label, p) for p in Priority],
        prompt="Prioridade",
        allow_blank=blank,
        value=value if value is not None else Select.NULL,
        id=id,
    )


def status_select(value: ItemStatus | None = None, *, blank: bool = False, id: str | None = None) -> Select:
    return Select(
        [(s.label, s) for s in ItemStatus],
        prompt="Status",
        allow_blank=blank,
        value=value if value is not None else Select.NULL,
        id=id,
    )


def difficulty_select(value: int | None = None, *, id: str | None = None) -> Select:
    return Select(
        [(f"Dificuldade {d}", d) for d in DIFFICULTY_RANGE],
        prompt="Dificuldade",
        allow_blank=False,
        value=value if value is not None else 3,
        id=id,
    )


def note_status_select(value: NoteStatus | None = None, *, blank: bool = False, id: str | None = None) -> Select:
    return Select(
        [(s.label, s) for s in NoteStatus],
        prompt="Status",
        allow_blank=blank,
        value=value if value is not None else (Select.NULL if blank else NoteStatus.PENDING),
        id=id,
    )


def select_value(select: Select):
    """The value of a Select, or None when blank."""
    v = select.value
    return None if v is Select.NULL else v


def plain(value: object, style: str = "") -> Text:
    """Cell content that is never parsed as markup (user text may contain '[')."""
    return Text(str(value), style=style)
