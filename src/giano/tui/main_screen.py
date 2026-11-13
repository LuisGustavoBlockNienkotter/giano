"""Main screen: projects sidebar, item table, Markdown preview."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import Screen
from textual.widgets import DataTable, Footer, Label, Markdown, OptionList, Static
from textual.widgets.option_list import Option

from giano.domain import ItemStatus, ItemView, Project, ValidationError
from giano.services.exports import format_date
from giano.services.formatting import item_preview
from giano.tui.dialogs import (
    ChoiceDialog,
    ClaudeExportDialog,
    ConfirmDialog,
    Filters,
    FilterDialog,
    HtmlExportDialog,
    SettingsScreen,
)
from giano.tui.item_editor import ItemEditorScreen
from giano.tui.notes import NotesScreen
from giano.tui.projects import ProjectEditor, ProjectsScreen
from giano.tui.revisions import RevisionsScreen
from giano.tui.search import SearchScreen
from giano.tui.widgets import plain

COLUMNS = [
    ("id", "ID"),
    ("title", "Título"),
    ("type", "Tipo"),
    ("priority", "Prioridade"),
    ("difficulty", "Dif."),
    ("status", "Status"),
    ("tags", "Tags"),
]

SORT_KEYS = {
    "id": lambda v: v.number,
    "title": lambda v: v.data.title.lower(),
    "type": lambda v: v.data.type.label,
    "priority": lambda v: (v.data.priority.rank, v.number),
    "difficulty": lambda v: (v.data.difficulty, v.number),
    "status": lambda v: (v.data.status.rank, v.number),
    "tags": lambda v: ", ".join(v.data.tags),
}

# The archive is mostly done/discarded items: don't hide them there by default.
ARCHIVE_FILTERS = Filters(hide_closed=False)

PRIORITY_STYLE = {
    "critical": "bold red",
    "high": "dark_orange",
    "medium": "yellow",
    "low": "dim",
}

EDIT_ACTIONS = {"new_item", "edit_item", "delete_item", "quick_status", "archive_item", "unarchive_item"}
# Actions that make no sense on an archived item (it is read-only until restored).
NOT_IN_ARCHIVE = {"new_item", "edit_item", "quick_status", "archive_item", "export_html"}


class MainScreen(Screen):
    BINDINGS = [
        Binding("n", "new_item", "Novo"),
        Binding("e", "edit_item", "Editar"),
        Binding("delete", "delete_item", "Excluir"),
        Binding("s", "quick_status", "Status"),
        Binding("z", "archive_item", "Arquivar"),
        Binding("z", "unarchive_item", "Restaurar"),
        Binding("v", "toggle_archive_view", "Arquivo"),
        Binding("/", "search", "Buscar", key_display="/"),
        Binding("f", "filters", "Filtros"),
        Binding("o", "sort", "Ordenar", show=False),
        Binding("r", "revisions", "Revisões"),
        Binding("l", "latest", "Última"),
        Binding("x", "export_html", "HTML"),
        Binding("c", "export_claude", "Claude"),
        Binding("a", "notes", "Anotações"),
        Binding("p", "projects", "Projetos"),
        Binding("g", "settings", "Config", show=False),
        Binding("q", "quit", "Sair"),
    ]

    def __init__(self):
        super().__init__()
        self.projects: list[Project] = []
        self.project: Project | None = None
        self.view_rev: int | None = None  # None = latest
        self.latest_rev = 0
        self.items: list[ItemView] = []
        self.filters = Filters()
        self.show_archived = False  # False = normal list, True = the archive
        self.archive_filters = ARCHIVE_FILTERS
        self.sort_column = "priority"
        self.sort_reverse = False

    # -- layout ---------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield Static(id="topbar", classes="topbar", markup=False)
        with Horizontal(id="main-body"):
            with Vertical(id="sidebar"):
                yield Label("Projetos", classes="section-title")
                yield OptionList(id="project-list")
            with Vertical(id="content"):
                yield Static(id="filterbar", markup=False)
                yield DataTable(id="items", cursor_type="row", zebra_stripes=True)
                with VerticalScroll(id="preview-scroll"):
                    yield Markdown(id="preview")
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        table = self.query_one("#items", DataTable)
        for key, label in COLUMNS:
            table.add_column(label, key=key)
        self.reload_projects()
        table.focus()

    def on_screen_resume(self) -> None:
        # Coming back from another screen: data may have changed.
        if self.is_mounted:
            self.reload_projects(keep_rev=True)

    def refresh_data(self) -> None:
        """F5: reload projects and items, keeping project, revision, filters and selection."""
        self.reload_projects(keep_rev=True)

    # -- data -----------------------------------------------------------------

    @property
    def read_only(self) -> bool:
        return self.view_rev is not None and self.view_rev < self.latest_rev

    def reload_projects(self, select_id: int | None = None, keep_rev: bool = False) -> None:
        g = self.app.giano
        self.projects = g.projects.list(include_archived=False)
        if self.project and all(p.id != self.project.id for p in self.projects):
            fresh = g.projects.get(self.project.id)
            if fresh:  # archived but currently open: keep it visible
                self.projects.append(fresh)
        target = select_id or (self.project.id if self.project else None) or self.app.current_project_id
        ol = self.query_one("#project-list", OptionList)
        with ol.prevent(OptionList.OptionHighlighted):
            ol.clear_options()
            ol.add_options(
                Option(f"{p.code:<6} {p.name}" + ("" if p.active else " (arq.)"), id=str(p.id))
                for p in self.projects
            )
        if not self.projects:
            self.set_project(None)
            return
        index = next((i for i, p in enumerate(self.projects) if p.id == target), 0)
        with ol.prevent(OptionList.OptionHighlighted):
            ol.highlighted = index
        self.set_project(self.projects[index], keep_rev=keep_rev)

    def set_project(self, project: Project | None, keep_rev: bool = False) -> None:
        changed = (project.id if project else None) != (self.project.id if self.project else None)
        if changed:
            self.view_rev = None
            self.filters = Filters()
            self.show_archived = False
            self.archive_filters = ARCHIVE_FILTERS
        elif not keep_rev:
            self.view_rev = None
        self.project = project
        self.app.current_project_id = project.id if project else None
        self.reload_items()

    def reload_items(self, select_item_id: int | None = None) -> None:
        g = self.app.giano
        table = self.query_one("#items", DataTable)
        if self.project is None:
            self.items, self.latest_rev = [], 0
        else:
            self.latest_rev = g.items.latest_revision(self.project.id)
            if self.view_rev is not None and self.view_rev >= self.latest_rev:
                self.view_rev = None
            self.items = g.items.list_items(self.project.id, self.view_rev, self.show_archived)
        if select_item_id is None:
            current = self.selected_item()
            select_item_id = current.item_id if current else None
        visible = sorted(
            (v for v in self.items if self.current_filters.matches(v)),
            key=SORT_KEYS[self.sort_column],
            reverse=self.sort_reverse,
        )
        with table.prevent(DataTable.RowHighlighted):
            table.clear()
            for v in visible:
                d = v.data
                closed = d.status in (ItemStatus.DONE, ItemStatus.DISCARDED)
                table.add_row(
                    plain(v.code),
                    plain(d.title, "dim" if closed else ""),
                    plain(d.type.label),
                    plain(d.priority.label, PRIORITY_STYLE[d.priority.value]),
                    plain(d.difficulty),
                    plain(d.status.label, "dim" if closed else ""),
                    plain(", ".join(d.tags)),
                    key=str(v.item_id),
                )
        if select_item_id is not None and any(v.item_id == select_item_id for v in visible):
            table.move_cursor(row=table.get_row_index(str(select_item_id)))
        self._update_bars(len(visible))
        self.update_preview()
        self.refresh_bindings()

    def _update_bars(self, shown: int) -> None:
        top = self.query_one("#topbar", Static)
        if self.project is None:
            top.update("Giano — nenhum projeto. Tecle p para criar um projeto.")
            top.set_class(False, "readonly")
        else:
            p = self.project
            if self.read_only:
                rev = self.app.giano.items.revision(p.id, self.view_rev)
                when = f" · {format_date(rev.created_at)}" if rev else ""
                text = (f"Giano │ {p.code} — {p.name} │ Rev. {self.view_rev} de {self.latest_rev} "
                        f"— somente leitura{when}   (l: voltar à última)")
            else:
                text = f"Giano │ {p.code} — {p.name} │ Rev. {self.latest_rev} (última)"
            if not p.active:
                text += " │ projeto arquivado"
            if self.show_archived:
                text += " │ ARQUIVO (v: voltar à lista)"
            top.update(text)
            top.set_class(self.read_only, "readonly")
            top.set_class(self.show_archived and not self.read_only, "archive")
        arrow = "↓" if self.sort_reverse else "↑"
        sort_label = dict(COLUMNS)[self.sort_column]
        noun = "itens arquivados" if self.show_archived else "itens"
        bar = f"{shown} de {len(self.items)} {noun} · ordem: {sort_label} {arrow} (o ou clique no cabeçalho)"
        if self.current_filters.active:
            bar += f" · filtros: {self.current_filters.describe()} (f para alterar)"
        self.query_one("#filterbar", Static).update(bar)

    @property
    def current_filters(self) -> Filters:
        """The normal list and the archive keep their own filters."""
        return self.archive_filters if self.show_archived else self.filters

    @current_filters.setter
    def current_filters(self, value: Filters) -> None:
        if self.show_archived:
            self.archive_filters = value
        else:
            self.filters = value

    def selected_item(self) -> ItemView | None:
        table = self.query_one("#items", DataTable)
        if table.row_count == 0:
            return None
        try:
            key, _ = table.coordinate_to_cell_key(table.cursor_coordinate)
        except Exception:
            return None
        item_id = int(key.value)
        return next((v for v in self.items if v.item_id == item_id), None)

    def update_preview(self) -> None:
        v = self.selected_item()
        md = self.query_one("#preview", Markdown)
        if v is None:
            md.update("_Nenhum item selecionado._" if self.project else
                      "# Bem-vindo ao Giano\n\nTecle **p** para criar o primeiro projeto, "
                      "ou rode `giano demo` para gerar dados de exemplo.")
            return
        note = self.app.giano.notes.get(v.source_note_id) if v.source_note_id else None
        md.update(item_preview(v, note))

    # -- events ---------------------------------------------------------------

    def on_option_list_option_highlighted(self, event: OptionList.OptionHighlighted) -> None:
        pid = int(event.option.id)
        if self.project is None or pid != self.project.id:
            self.set_project(next(p for p in self.projects if p.id == pid))

    def on_option_list_option_selected(self, _: OptionList.OptionSelected) -> None:
        self.query_one("#items", DataTable).focus()

    def on_data_table_row_highlighted(self, _: DataTable.RowHighlighted) -> None:
        self.update_preview()

    def on_data_table_row_selected(self, _: DataTable.RowSelected) -> None:
        if not self.read_only and not self.show_archived:
            self.action_edit_item()

    def on_data_table_header_selected(self, event: DataTable.HeaderSelected) -> None:
        key = event.column_key.value
        if key == self.sort_column:
            self.sort_reverse = not self.sort_reverse
        else:
            self.sort_column, self.sort_reverse = key, False
        self.reload_items()

    # -- bindings state -------------------------------------------------------

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        if action == "archive_item" and self.show_archived:
            return False  # hidden: "z" means Restaurar in the archive
        if action == "unarchive_item" and not self.show_archived:
            return False
        if action in NOT_IN_ARCHIVE and self.show_archived:
            return None  # shown but disabled
        if action in EDIT_ACTIONS:
            if self.project is None or self.read_only or not self.project.active:
                return None  # shown but disabled
            if action != "new_item" and self.selected_item() is None:
                return None
        if action == "latest":
            return self.read_only  # only shown while viewing an old revision
        if action in ("filters", "sort", "revisions", "export_html", "export_claude",
                      "toggle_archive_view"):
            return True if self.project is not None else None
        return True

    # -- actions --------------------------------------------------------------

    def _guard_editable(self) -> bool:
        if self.project is None:
            self.notify("Crie ou selecione um projeto (p).", severity="warning")
            return False
        if self.read_only:
            self.notify("Revisão antiga: somente leitura. Tecle l para voltar à última.",
                        severity="warning")
            return False
        if not self.project.active:
            self.notify("Projeto arquivado: somente leitura.", severity="warning")
            return False
        return True

    def _guard_not_archive(self) -> bool:
        if self.show_archived:
            self.notify("Item arquivado: somente leitura. Tecle z para restaurá-lo à lista.",
                        severity="warning")
            return False
        return True

    def action_new_item(self) -> None:
        if self._guard_not_archive() and self._guard_editable():
            self.app.push_screen(ItemEditorScreen(self.project), self._after_edit)

    def action_edit_item(self) -> None:
        v = self.selected_item()
        if v and self._guard_not_archive() and self._guard_editable():
            self.app.push_screen(ItemEditorScreen(self.project, v), self._after_edit)

    def _after_edit(self, view: ItemView | None) -> None:
        if view is not None:
            self.reload_items(select_item_id=view.item_id)

    def action_delete_item(self) -> None:
        v = self.selected_item()
        if v is None or not self._guard_editable():
            return

        def answer(ok: bool | None) -> None:
            if ok:
                rev = self.app.giano.items.delete(v.item_id)
                self.notify(f"{v.code} excluído (Rev. {rev}).")
                self.reload_items()

        self.app.push_screen(
            ConfirmDialog(f"Excluir {v.code} — {v.data.title}?\n\n"
                          "Será criada uma nova revisão; o item continua visível nas revisões anteriores."),
            answer,
        )

    def action_archive_item(self) -> None:
        v = self.selected_item()
        if v is None or not self._guard_not_archive() or not self._guard_editable():
            return
        try:
            new = self.app.giano.items.archive(v.item_id)
        except ValidationError as exc:
            self.notify(str(exc), severity="error")
            return
        self.notify(f"{v.code} arquivado (Rev. {new.rev_start}). Tecle v para ver o arquivo.")
        self.reload_items()

    def action_unarchive_item(self) -> None:
        v = self.selected_item()
        if v is None or not self._guard_editable():
            return
        try:
            new = self.app.giano.items.unarchive(v.item_id)
        except ValidationError as exc:
            self.notify(str(exc), severity="error")
            return
        self.notify(f"{v.code} restaurado à lista (Rev. {new.rev_start}).")
        self.reload_items()

    def action_toggle_archive_view(self) -> None:
        if self.project is None:
            return
        self.show_archived = not self.show_archived
        self.reload_items()
        if self.show_archived and not self.items:
            self.notify("Nenhum item arquivado nesta revisão.")

    def action_quick_status(self) -> None:
        v = self.selected_item()
        if v is None or not self._guard_not_archive() or not self._guard_editable():
            return

        def apply(status) -> None:
            if status is None:
                return
            new = self.app.giano.items.set_status(v.item_id, status)
            if new:
                self.notify(f"{v.code}: status {v.data.status.label} → {status.label} (Rev. {new.rev_start}).")
            self.reload_items(select_item_id=v.item_id)

        self.app.push_screen(
            ChoiceDialog(f"Status de {v.code}", [(s.label, s) for s in ItemStatus], v.data.status),
            apply,
        )

    def action_search(self) -> None:
        p = self.project
        self.app.push_screen(
            SearchScreen(p.id if p else None, p.code if p else None), self._after_search
        )

    def _after_search(self, hit) -> None:
        if hit is None:
            return
        if hit.kind == "item":
            self.view_rev = None
            if self.project is None or hit.project_id != self.project.id:
                project = self.app.giano.projects.get(hit.project_id)
                if project and not project.active:
                    self.project = project  # keep an archived project visible
                self.reload_projects(select_id=hit.project_id)
            # Default filters, but never hide the item the user is jumping to.
            found = self.app.giano.items.get(hit.id)
            closed = found is not None and found.data.status in (ItemStatus.DONE, ItemStatus.DISCARDED)
            self.show_archived = found is not None and found.data.archived
            self.current_filters = (
                ARCHIVE_FILTERS if self.show_archived else Filters(hide_closed=not closed)
            )
            self.reload_items(select_item_id=hit.id)
            self.query_one("#items", DataTable).focus()
        else:
            self.app.push_screen(NotesScreen(None, select_note_id=hit.id))

    def action_filters(self) -> None:
        if self.project is None:
            return
        tags = sorted({t for v in self.items for t in v.data.tags})

        def apply(f: Filters | None) -> None:
            if f is not None:
                self.current_filters = f
                self.reload_items()

        cleared = ARCHIVE_FILTERS if self.show_archived else Filters()
        self.app.push_screen(FilterDialog(self.current_filters, tags, cleared), apply)

    def action_sort(self) -> None:
        keys = [k for k, _ in COLUMNS]
        if self.sort_reverse:
            self.sort_column = keys[(keys.index(self.sort_column) + 1) % len(keys)]
            self.sort_reverse = False
        else:
            self.sort_reverse = True
        self.reload_items()

    def action_revisions(self) -> None:
        if self.project is None:
            return

        def show(rev: int | None) -> None:
            if rev is None:
                return
            self.view_rev = None if rev >= self.latest_rev else rev
            self.reload_items()
            if self.read_only:
                self.notify(f"Exibindo a Rev. {rev} (somente leitura). Tecle l para voltar à última.")

        self.app.push_screen(RevisionsScreen(self.project, self.view_rev), show)

    def action_latest(self) -> None:
        self.view_rev = None
        self.reload_items()

    def action_export_html(self) -> None:
        if self.project is None or not self._guard_not_archive():
            return
        if not self.items:
            self.notify("Nenhum item nesta revisão para exportar.", severity="warning")
            return
        project, rev = self.project, self.view_rev
        shown_rev = rev if rev is not None else self.latest_rev
        ordered = sorted(self.items, key=SORT_KEYS[self.sort_column], reverse=self.sort_reverse)
        filtered = (
            {v.number for v in self.items if self.filters.matches(v)} if self.filters.active else None
        )

        def run(opts) -> None:
            if opts is None:
                return
            try:
                result = self.app.giano.exports.export_html(
                    project.code, rev, changes=opts.changes, numbers=opts.numbers
                )
            except (ValidationError, OSError) as exc:
                self.notify(str(exc), severity="error", title="Falha na exportação")
                return
            n = len(opts.numbers) if opts.numbers is not None else len(self.items)
            self.notify(str(result.path), title=f"HTML exportado (Rev. {shown_rev}, {n} itens)",
                        timeout=8)

        self.app.push_screen(
            HtmlExportDialog(
                f"Exportar HTML — {project.code}, Rev. {shown_rev}: escolha os itens",
                ordered, filtered,
            ),
            run,
        )

    def action_export_claude(self) -> None:
        if self.project is None:
            return
        project = self.project

        def run(opts) -> None:
            if opts is None:
                return
            try:
                result = self.app.giano.exports.export_claude(
                    project.code, opts.since, opts.all_notes, opts.include_unassigned
                )
            except (ValidationError, OSError) as exc:
                self.notify(str(exc), severity="error", title="Falha na exportação")
                return
            self.app.copy_to_clipboard(result.content)
            self.notify(f"{result.path}\nConteúdo copiado para a área de transferência.",
                        title="Exportado para o Claude", timeout=8)

        self.app.push_screen(ClaudeExportDialog(f"{project.code} — {project.name}"), run)

    def action_notes(self) -> None:
        self.app.push_screen(NotesScreen(self.project.id if self.project else None))

    def action_projects(self) -> None:
        if not self.app.giano.projects.list():
            self.app.push_screen(ProjectEditor(), lambda p: p and self.reload_projects(select_id=p.id))
            return

        def opened(pid: int | None) -> None:
            if pid is not None:
                p = self.app.giano.projects.get(pid)
                if p and not p.active:
                    self.project = p  # allow browsing an archived project
                self.reload_projects(select_id=pid)

        self.app.push_screen(ProjectsScreen(), opened)

    def action_settings(self) -> None:
        self.app.push_screen(SettingsScreen())

    def action_quit(self) -> None:
        self.app.exit()
