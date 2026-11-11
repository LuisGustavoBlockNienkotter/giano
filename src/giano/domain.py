"""Domain model: enums, value objects and pure comparison logic.

Identifiers are stored in English in the database; labels shown to the user are
in Portuguese (``.label``).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import StrEnum


class ItemType(StrEnum):
    IMPROVEMENT = "improvement"
    DISCUSSION = "discussion"
    ADJUSTMENT = "adjustment"

    @property
    def label(self) -> str:
        return {
            ItemType.IMPROVEMENT: "Melhoria",
            ItemType.DISCUSSION: "Discussão",
            ItemType.ADJUSTMENT: "Ajuste",
        }[self]

    @property
    def plural(self) -> str:
        return {
            ItemType.IMPROVEMENT: "Melhorias",
            ItemType.DISCUSSION: "Discussões",
            ItemType.ADJUSTMENT: "Ajustes",
        }[self]


class Priority(StrEnum):
    CRITICAL = "critical"
    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"

    @property
    def label(self) -> str:
        return {
            Priority.CRITICAL: "Crítica",
            Priority.HIGH: "Alta",
            Priority.MEDIUM: "Média",
            Priority.LOW: "Baixa",
        }[self]

    @property
    def rank(self) -> int:
        """Lower rank = more urgent (used for sorting)."""
        return list(Priority).index(self)


class ItemStatus(StrEnum):
    OPEN = "open"
    ANALYSIS = "analysis"
    APPROVED = "approved"
    IN_PROGRESS = "in_progress"
    DONE = "done"
    DISCARDED = "discarded"

    @property
    def label(self) -> str:
        return {
            ItemStatus.OPEN: "Aberto",
            ItemStatus.ANALYSIS: "Em análise",
            ItemStatus.APPROVED: "Aprovado",
            ItemStatus.IN_PROGRESS: "Em desenvolvimento",
            ItemStatus.DONE: "Concluído",
            ItemStatus.DISCARDED: "Descartado",
        }[self]

    @property
    def rank(self) -> int:
        return list(ItemStatus).index(self)


class NoteStatus(StrEnum):
    PENDING = "pending"
    PROCESSED = "processed"
    ARCHIVED = "archived"

    @property
    def label(self) -> str:
        return {
            NoteStatus.PENDING: "Pendente",
            NoteStatus.PROCESSED: "Processada",
            NoteStatus.ARCHIVED: "Arquivada",
        }[self]


DIFFICULTY_RANGE = range(1, 6)

_CODE_RE = re.compile(r"^[A-Z][A-Z0-9]{0,9}$")


class ValidationError(ValueError):
    """Raised when user-provided data is invalid. Message is user-facing (pt-BR)."""


def normalize_code(code: str) -> str:
    code = code.strip().upper()
    if not _CODE_RE.match(code):
        raise ValidationError(
            "Sigla inválida: use de 1 a 10 letras/números, começando por letra (ex.: MERC, P2K)."
        )
    return code


def normalize_tag(tag: str) -> str:
    return re.sub(r"\s+", "-", tag.strip().lower())


def normalize_tags(tags) -> tuple[str, ...]:
    """Normalize, de-duplicate and sort tags. Accepts an iterable or a comma-separated string."""
    if isinstance(tags, str):
        tags = tags.split(",")
    return tuple(sorted({t for t in (normalize_tag(x) for x in tags) if t}))


def normalize_body(body: str) -> str:
    """Trailing whitespace is not a real change: strip it (and normalize newlines)."""
    return body.replace("\r\n", "\n").rstrip()


@dataclass(frozen=True)
class ItemData:
    """The versioned content of an item."""

    title: str
    type: ItemType = ItemType.IMPROVEMENT
    priority: Priority = Priority.MEDIUM
    difficulty: int = 3
    status: ItemStatus = ItemStatus.OPEN
    tags: tuple[str, ...] = ()
    body: str = ""

    def normalized(self) -> ItemData:
        title = " ".join(self.title.split())
        if not title:
            raise ValidationError("O título é obrigatório.")
        try:
            difficulty = int(self.difficulty)
        except (TypeError, ValueError):
            raise ValidationError("Dificuldade deve ser um número de 1 a 5.") from None
        if difficulty not in DIFFICULTY_RANGE:
            raise ValidationError("Dificuldade deve ser um número de 1 a 5.")
        return ItemData(
            title=title,
            type=ItemType(self.type),
            priority=Priority(self.priority),
            difficulty=difficulty,
            status=ItemStatus(self.status),
            tags=normalize_tags(self.tags),
            body=normalize_body(self.body),
        )


@dataclass(frozen=True)
class Project:
    id: int
    code: str
    name: str
    description: str
    active: bool
    created_at: str


@dataclass(frozen=True)
class Revision:
    project_id: int
    number: int
    created_at: str
    summary: str


@dataclass(frozen=True)
class ItemView:
    """An item as it is at a given revision."""

    item_id: int
    project_id: int
    project_code: str
    number: int
    version_id: int
    rev_start: int
    rev_end: int | None
    data: ItemData
    source_note_id: int | None = None

    @property
    def code(self) -> str:
        return f"{self.project_code}-{self.number}"


@dataclass(frozen=True)
class Note:
    id: int
    note_date: str
    name: str
    project_id: int | None
    body: str
    status: NoteStatus
    created_at: str
    updated_at: str
    project_code: str | None = None


# ---------------------------------------------------------------------------
# Comparison (shared by revision summaries and diffs)
# ---------------------------------------------------------------------------

FIELD_LABELS = {
    "title": "título",
    "type": "tipo",
    "priority": "prioridade",
    "difficulty": "dificuldade",
    "status": "status",
    "tags": "tags",
    "body": "corpo",
}


@dataclass(frozen=True)
class FieldChange:
    field: str
    old: object
    new: object

    @property
    def label(self) -> str:
        return FIELD_LABELS[self.field]

    def describe(self) -> str:
        """Short human description, e.g. ``prioridade Alta → Média``."""
        if self.field == "body":
            return "corpo alterado"
        if self.field == "title":
            return f'título "{_shorten(self.old)}" → "{_shorten(self.new)}"'
        if self.field == "tags":
            old, new = set(self.old), set(self.new)
            parts = [f"+{t}" for t in sorted(new - old)] + [f"−{t}" for t in sorted(old - new)]
            return "tags " + " ".join(parts)
        return f"{self.label} {display_value(self.field, self.old)} → {display_value(self.field, self.new)}"


def display_value(field_name: str, value: object) -> str:
    if value is None:
        return ""
    if field_name in ("type", "priority", "status"):
        return value.label  # type: ignore[union-attr]
    if field_name == "tags":
        return ", ".join(value) if value else "—"  # type: ignore[arg-type]
    if field_name == "difficulty":
        return f"{value}/5"
    return str(value)


def _shorten(text: object, limit: int = 40) -> str:
    s = str(text)
    return s if len(s) <= limit else s[: limit - 1] + "…"


def compare_items(old: ItemData, new: ItemData) -> list[FieldChange]:
    changes: list[FieldChange] = []
    for name in ("title", "type", "priority", "difficulty", "status", "tags", "body"):
        a, b = getattr(old, name), getattr(new, name)
        if a != b:
            changes.append(FieldChange(name, a, b))
    return changes


@dataclass
class ItemChange:
    old: ItemView
    new: ItemView
    changes: list[FieldChange] = field(default_factory=list)

    @property
    def code(self) -> str:
        return self.new.code


@dataclass
class RevisionDiff:
    project: Project
    rev_from: int
    rev_to: int
    added: list[ItemView] = field(default_factory=list)
    removed: list[ItemView] = field(default_factory=list)
    changed: list[ItemChange] = field(default_factory=list)

    @property
    def is_empty(self) -> bool:
        return not (self.added or self.removed or self.changed)
