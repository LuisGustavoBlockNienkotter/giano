"""Shared TUI widgets and helpers."""

from __future__ import annotations

from collections.abc import Callable, Iterable
from typing import ClassVar

from rich.style import Style
from rich.text import Text
from textual import events
from textual.binding import Binding, BindingType
from textual.message import Message
from textual.suggester import Suggester
from textual.widgets import DataTable, Select

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


MARK_COLUMN = "mark"
MARK = "●"


class ItemTable(DataTable):
    """Row-cursor DataTable whose rows can be selected (marked) several at a time:

    * Ctrl+click or Espaço: add/remove the row;
    * Shift+click or Shift+↑/↓: select the rows from the anchor (the last row marked
      with Ctrl/Espaço, or the cursor) to the target; Ctrl+Shift+click adds the range;
    * Ctrl+A: select every row; Esc: clear the selection.

    A plain click or arrow only moves the cursor and keeps the selection. Selected rows
    show :data:`MARK` in the :data:`MARK_COLUMN` column (the owner adds it, first) and the
    ``itemtable--selected`` background. Row keys must be strings; :attr:`selected` holds
    them. Every change posts :class:`SelectionChanged`.

    Some terminals keep Shift+click for their own text selection (Windows Terminal does):
    there Shift+↑/↓ does the same."""

    COMPONENT_CLASSES: ClassVar[set[str]] = DataTable.COMPONENT_CLASSES | {"itemtable--selected"}

    DEFAULT_CSS = """
    ItemTable > .itemtable--selected {
        background: $accent 35%;
    }
    """

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("space", "toggle_row", "Marcar", show=False),
        Binding("shift+up", "extend(-1)", "Seleção ↑", show=False),
        Binding("shift+down", "extend(1)", "Seleção ↓", show=False),
        Binding("ctrl+a", "select_all", "Selecionar tudo", show=False),
        Binding("escape", "clear_selection", "Limpar seleção", show=False),
    ]

    class SelectionChanged(Message):
        def __init__(self, table: ItemTable) -> None:
            super().__init__()
            self.table = table

        @property
        def control(self) -> ItemTable:
            return self.table

    def __init__(self, **kwargs) -> None:
        super().__init__(cursor_type="row", **kwargs)
        self.selected: set[str] = set()
        self._anchor: str | None = None

    # -- queries --------------------------------------------------------------

    def _row_keys(self) -> list[str]:
        return [row.key.value for row in self.ordered_rows]

    def _cursor_key(self) -> str | None:
        keys = self._row_keys()
        return keys[self.cursor_row] if 0 <= self.cursor_row < len(keys) else None

    def selected_in_order(self) -> list[str]:
        """The selected keys in the order the rows are shown."""
        return [k for k in self._row_keys() if k in self.selected]

    # -- changes --------------------------------------------------------------

    def set_selection(self, keys: Iterable[str], anchor: str | None = None) -> None:
        """Replace the selection (keys that are not rows are ignored)."""
        rows = set(self._row_keys())
        new = {k for k in keys if k in rows}
        self._anchor = anchor if new else None
        if new == self.selected:
            return
        changed = new ^ self.selected
        self.selected = new
        if MARK_COLUMN in self.columns:
            for key in changed:
                # update_cell also invalidates the render caches: the row style follows.
                self.update_cell(key, MARK_COLUMN, mark_cell(key in new))
        self.refresh()
        self.post_message(self.SelectionChanged(self))

    def clear_selection(self) -> None:
        self.set_selection(())

    def keep_existing(self) -> None:
        """After the rows were rebuilt: drop selected keys that are no longer rows
        (no message: the owner is refreshing anyway)."""
        rows = set(self._row_keys())
        self.selected &= rows
        if not self.selected or self._anchor not in rows:
            self._anchor = None

    def _toggle(self, key: str) -> None:
        self.set_selection(self.selected ^ {key}, anchor=key)

    def _select_range(self, target: str, add: bool) -> None:
        keys = self._row_keys()
        anchor = self._anchor if self._anchor in keys else self._cursor_key()
        if anchor is None:
            return
        a, b = sorted((keys.index(anchor), keys.index(target)))
        span = set(keys[a : b + 1])
        self.set_selection((self.selected | span) if add else span, anchor=anchor)

    # -- actions and events ---------------------------------------------------

    def action_toggle_row(self) -> None:
        key = self._cursor_key()
        if key is not None:
            self._toggle(key)

    def action_extend(self, delta: int) -> None:
        start = self._cursor_key()
        if start is None:
            return
        if self._anchor is None or self._anchor not in self._row_keys():
            self._anchor = start
        self.move_cursor(row=max(0, min(self.row_count - 1, self.cursor_row + delta)))
        self._select_range(self._cursor_key(), add=False)  # type: ignore[arg-type]

    def action_select_all(self) -> None:
        self.set_selection(self._row_keys(), anchor=self._cursor_key())

    def action_clear_selection(self) -> None:
        self.clear_selection()

    async def _on_click(self, event: events.Click) -> None:
        """Ctrl/Shift+click select rows; any other click is DataTable's (moves the cursor,
        a second click opens the row)."""
        if not (event.ctrl or event.shift or event.meta):
            return  # DataTable._on_click runs next (MRO)
        meta = event.style.meta
        row = meta.get("row")
        if row is None or row < 0 or row >= self.row_count or meta.get("out_of_bounds"):
            return  # header or empty area: let DataTable handle it
        event.prevent_default()  # no DataTable._on_click: it would open the row
        event.stop()
        key = self._row_keys()[row]
        if event.shift:
            if self._anchor is None or self._anchor not in self._row_keys():
                self._anchor = self._cursor_key()
            self.move_cursor(row=row)
            self._select_range(key, add=event.ctrl or event.meta)
        else:
            self.move_cursor(row=row)
            self._toggle(key)

    def _get_row_style(self, row_index: int, base_style: Style) -> Style:
        style = super()._get_row_style(row_index, base_style)
        if self.selected and 0 <= row_index < self.row_count:
            if self.ordered_rows[row_index].key.value in self.selected:
                style += self.get_component_styles("itemtable--selected").rich_style
        return style


def mark_cell(selected: bool) -> Text:
    return Text(MARK if selected else " ", style="bold")
