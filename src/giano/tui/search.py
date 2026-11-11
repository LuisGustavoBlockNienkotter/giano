"""Full-text search over items and notes."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.screen import Screen
from textual.widgets import Checkbox, DataTable, Footer, Input, Static

from giano.services.search import SearchHit
from giano.tui.widgets import plain


class SearchScreen(Screen[SearchHit | None]):
    BINDINGS = [
        Binding("escape", "close", "Voltar"),
        Binding("down", "focus_results", "Resultados", show=False),
    ]

    def __init__(self, project_id: int | None = None, project_code: str | None = None):
        super().__init__()
        self.project_id = project_id
        self.project_code = project_code
        self.hits: list[SearchHit] = []

    def compose(self) -> ComposeResult:
        yield Static("Buscar em itens (lista atual) e anotações", classes="topbar")
        yield Input(placeholder="Digite para buscar…  (enter: ir para os resultados)", id="query")
        label = f"Somente no projeto {self.project_code}" if self.project_code else "Somente no projeto atual"
        yield Checkbox(label, False, id="only-project", disabled=self.project_id is None)
        yield DataTable(id="results", cursor_type="row", zebra_stripes=True)
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("Tipo", "Ref.", "Título", "Status", "Trecho")
        self.query_one("#query", Input).focus()

    def on_input_changed(self, _: Input.Changed) -> None:
        self._run()

    def on_checkbox_changed(self, _: Checkbox.Changed) -> None:
        self._run()

    def on_input_submitted(self, _: Input.Submitted) -> None:
        self.action_focus_results()

    def refresh_data(self) -> None:
        self._run()

    def _run(self) -> None:
        text = self.query_one("#query", Input).value
        only = self.query_one("#only-project", Checkbox).value
        self.hits = self.app.giano.search.search(text, self.project_id if only else None)
        table = self.query_one(DataTable)
        table.clear()
        for i, h in enumerate(self.hits):
            kind = "Item" if h.kind == "item" else "Anotação"
            table.add_row(kind, plain(h.label), plain(h.title), h.status, plain(h.snippet), key=str(i))

    def action_focus_results(self) -> None:
        table = self.query_one(DataTable)
        if table.row_count:
            table.focus()

    def on_data_table_row_selected(self, event: DataTable.RowSelected) -> None:
        self.dismiss(self.hits[int(event.row_key.value)])

    def action_close(self) -> None:
        self.dismiss(None)
