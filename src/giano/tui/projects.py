"""Project management: list, create, edit, archive."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen, Screen
from textual.widgets import Button, DataTable, Footer, Input, Label, Static, Switch, TextArea

from giano.domain import Project, ValidationError
from giano.tui.widgets import plain


class ProjectEditor(ModalScreen[Project | None]):
    BINDINGS = [
        Binding("ctrl+s", "save", "Salvar", priority=True),
        Binding("escape", "cancel", "Cancelar"),
    ]

    def __init__(self, project: Project | None = None):
        super().__init__()
        self.project = project

    def compose(self) -> ComposeResult:
        p = self.project
        with Vertical(classes="dialog wide"):
            yield Label(f"Editar projeto {p.code}" if p else "Novo projeto", classes="dialog-title", markup=False)
            with Horizontal(classes="row"):
                with Vertical(classes="code-col"):
                    yield Label("Sigla")
                    yield Input(p.code if p else "", id="code", placeholder="ex.: MERC", max_length=10)
                with Vertical():
                    yield Label("Nome")
                    yield Input(p.name if p else "", id="name", placeholder="Nome do sistema")
            with Horizontal(classes="row switch-row"):
                yield Switch(p.active if p else True, id="active")
                yield Label("Ativo (desligado = arquivado)")
            yield Label("Descrição em Markdown (contexto do sistema; vai para a exportação do Claude)")
            yield TextArea.code_editor(
                p.description if p else "", language="markdown", soft_wrap=True,
                show_line_numbers=False, tab_behavior="focus", id="description",
            )
            with Horizontal(classes="buttons"):
                yield Button("Salvar", variant="primary", id="save")
                yield Button("Cancelar", id="cancel")
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        self.query_one("#code" if self.project is None else "#name", Input).focus()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "save":
            self.action_save()
        else:
            self.dismiss(None)

    def action_save(self) -> None:
        svc = self.app.giano.projects
        code = self.query_one("#code", Input).value
        name = self.query_one("#name", Input).value
        desc = self.query_one("#description", TextArea).text
        active = self.query_one("#active", Switch).value
        try:
            if self.project is None:
                project = svc.create(code, name, desc, active)
            else:
                project = svc.update(self.project.id, code, name, desc, active)
        except ValidationError as exc:
            self.notify(str(exc), severity="error")
            return
        self.dismiss(project)

    def action_cancel(self) -> None:
        self.dismiss(None)


class ProjectsScreen(Screen[int | None]):
    """Returns the id of the project to open (or None)."""

    BINDINGS = [
        Binding("enter", "open", "Abrir", show=True, priority=False),
        Binding("n", "new", "Novo projeto"),
        Binding("e", "edit", "Editar"),
        Binding("a", "toggle_archive", "Arquivar/reativar"),
        Binding("escape", "close", "Voltar"),
    ]

    def compose(self) -> ComposeResult:
        yield Static("Projetos", classes="topbar")
        table = DataTable(id="projects", cursor_type="row", zebra_stripes=True)
        yield table
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        table = self.query_one(DataTable)
        table.add_columns("Sigla", "Nome", "Situação", "Rev.", "Itens")
        self.reload()
        table.focus()

    def refresh_data(self) -> None:
        self.reload()

    def reload(self, select_id: int | None = None) -> None:
        g = self.app.giano
        table = self.query_one(DataTable)
        previous = self._selected_id()
        table.clear()
        for p in g.projects.list():
            rev = g.items.latest_revision(p.id)
            table.add_row(
                plain(p.code), plain(p.name), "ativo" if p.active else "arquivado", str(rev),
                str(len(g.items.list_items(p.id, rev, archived=False))), key=str(p.id),
            )
        target = select_id or previous
        if target is not None:
            try:
                table.move_cursor(row=table.get_row_index(str(target)))
            except Exception:
                pass

    def _selected_id(self) -> int | None:
        table = self.query_one(DataTable)
        if table.row_count == 0:
            return None
        key, _ = table.coordinate_to_cell_key(table.cursor_coordinate)
        return int(key.value)

    def on_data_table_row_selected(self, event: DataTable.RowSelected) -> None:
        self.dismiss(int(event.row_key.value))

    def action_open(self) -> None:
        pid = self._selected_id()
        if pid is not None:
            self.dismiss(pid)

    def action_new(self) -> None:
        self.app.push_screen(ProjectEditor(), lambda p: p and self.reload(p.id))

    def action_edit(self) -> None:
        pid = self._selected_id()
        if pid is None:
            return
        self.app.push_screen(
            ProjectEditor(self.app.giano.projects.get(pid)), lambda p: p and self.reload(p.id)
        )

    def action_toggle_archive(self) -> None:
        pid = self._selected_id()
        if pid is None:
            return
        p = self.app.giano.projects.get(pid)
        p = self.app.giano.projects.set_active(pid, not p.active)
        self.notify(f"{p.code} {'reativado' if p.active else 'arquivado'}.")
        self.reload(pid)

    def action_close(self) -> None:
        self.dismiss(None)
