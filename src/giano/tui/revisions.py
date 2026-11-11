"""Revision history and diff between revisions."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import VerticalScroll
from textual.screen import Screen
from textual.widgets import DataTable, Footer, Markdown, Static

from giano.domain import Project
from giano.services.exports import format_date
from giano.services.formatting import diff_markdown
from giano.tui.widgets import plain


class DiffScreen(Screen[None]):
    BINDINGS = [Binding("escape,q", "close", "Voltar")]

    def __init__(self, project: Project, rev_from: int, rev_to: int):
        super().__init__()
        self.project, self.rev_from, self.rev_to = project, rev_from, rev_to

    def compose(self) -> ComposeResult:
        yield Static(
            f"{self.project.code} — diferenças entre Rev. {self.rev_from} e Rev. {self.rev_to}",
            classes="topbar",
            markup=False,
        )
        diff = self.app.giano.items.diff(self.project.id, self.rev_from, self.rev_to)
        with VerticalScroll(id="diff-scroll"):
            yield Markdown(diff_markdown(diff))
        yield Footer(compact=True)

    def on_mount(self) -> None:
        self.query_one("#diff-scroll").focus()

    def action_close(self) -> None:
        self.dismiss(None)


class RevisionsScreen(Screen[int | None]):
    """Lists revisions. Returns the revision number to display (or None)."""

    BINDINGS = [
        Binding("enter", "view", "Ver revisão"),
        Binding("d", "diff", "Diferenças"),
        Binding("m", "mark", "Marcar base"),
        Binding("l", "latest", "Última"),
        Binding("escape", "close", "Voltar"),
    ]

    def __init__(self, project: Project, shown_rev: int | None = None):
        super().__init__()
        self.project = project
        self.shown_rev = shown_rev
        self.marked: int | None = None

    def compose(self) -> ComposeResult:
        yield Static(id="rev-title", classes="topbar", markup=False)
        yield DataTable(id="revisions", cursor_type="row", zebra_stripes=True)
        yield Static(
            "enter: ver a lista nessa revisão (somente leitura) · d: diferenças entre a revisão "
            "marcada (m) — ou a anterior — e a selecionada",
            classes="hint",
        )
        yield Footer(compact=True)

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_column("Rev.", key="rev")
        table.add_column("Data", key="date")
        table.add_column("Resumo", key="summary")
        self._fill()
        if self.shown_rev is not None:
            try:
                table.move_cursor(row=table.get_row_index(str(self.shown_rev)))
            except Exception:
                pass
        table.focus()

    def _fill(self) -> None:
        table = self.query_one(DataTable)
        table.clear()
        revs = self.app.giano.items.revisions(self.project.id)
        latest = revs[0].number if revs else 0
        title = f"{self.project.code} — {len(revs)} revisões (última: Rev. {latest})"
        if self.marked is not None:
            title += f" · base marcada: Rev. {self.marked}"
        self.query_one("#rev-title", Static).update(title)
        for r in revs:
            mark = "▶ " if r.number == self.marked else ""
            table.add_row(f"{mark}{r.number}", format_date(r.created_at), plain(r.summary), key=str(r.number))
        if not revs:
            table.add_row("0", "", "(nenhuma revisão — lista vazia)", key="0")

    def _selected(self) -> int | None:
        table = self.query_one(DataTable)
        if table.row_count == 0:
            return None
        key, _ = table.coordinate_to_cell_key(table.cursor_coordinate)
        return int(key.value)

    def on_data_table_row_selected(self, event: DataTable.RowSelected) -> None:
        self.dismiss(int(event.row_key.value))

    def action_view(self) -> None:
        rev = self._selected()
        if rev is not None:
            self.dismiss(rev)

    def action_mark(self) -> None:
        rev = self._selected()
        self.marked = None if rev == self.marked else rev
        row = self.query_one(DataTable).cursor_row
        self._fill()
        self.query_one(DataTable).move_cursor(row=row)

    def action_diff(self) -> None:
        rev = self._selected()
        if rev is None:
            return
        base = self.marked if self.marked is not None else max(rev - 1, 0)
        a, b = sorted((base, rev))
        if a == b:
            self.notify("Selecione uma revisão diferente da base marcada.")
            return
        self.app.push_screen(DiffScreen(self.project, a, b))

    def action_latest(self) -> None:
        self.dismiss(self.app.giano.items.latest_revision(self.project.id))

    def action_close(self) -> None:
        self.dismiss(None)
