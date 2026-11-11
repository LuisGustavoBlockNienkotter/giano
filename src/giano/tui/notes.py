"""Quick notes: list + editor with autosave, and the global quick-capture modal."""

from __future__ import annotations

from datetime import date, datetime

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.message import Message
from textual.screen import ModalScreen, Screen
from textual.timer import Timer
from textual.widgets import DataTable, Footer, Input, Select, Static, TextArea

from giano.domain import Note, NoteStatus, ValidationError
from giano.services.notes import parse_date
from giano.tui.dialogs import ChoiceDialog, ConfirmDialog
from giano.tui.item_editor import ItemDraft, ItemEditorScreen
from giano.tui.widgets import note_status_select, plain, select_value

AUTOSAVE_DELAY = 0.6  # seconds after the last keystroke


class NoteEditor(Vertical):
    """Edits one note and saves it automatically while typing.

    With ``note=None`` the note is created on the first real content.
    """

    class Saved(Message):
        def __init__(self, note: Note):
            super().__init__()
            self.note = note

    def __init__(self, note: Note | None = None, project_id: int | None = None, **kwargs):
        super().__init__(**kwargs)
        self.note = note
        self.default_project_id = project_id
        self._timer: Timer | None = None
        self._saved_snapshot: tuple | None = None
        self._latest: tuple | None = None  # last known editor content (survives unmount)

    def compose(self) -> ComposeResult:
        n = self.note
        with Horizontal(classes="note-fields"):
            yield Input(n.note_date if n else date.today().isoformat(), id="note-date",
                        placeholder="AAAA-MM-DD", classes="date-input")
            yield Input(n.name if n else "", id="note-name", placeholder="Nome (ex.: reunião com o comercial)")
        with Horizontal(classes="note-fields"):
            yield Select(self._project_options(), prompt="(sem projeto)", allow_blank=True,
                         value=self._initial_project(), id="note-project")
            yield note_status_select(n.status if n else NoteStatus.PENDING, id="note-status")
        yield TextArea.code_editor(
            n.body if n else "", language="markdown", soft_wrap=True, show_line_numbers=False,
            tab_behavior="focus", id="note-body",
            placeholder="Escreva livremente em Markdown. Tudo é salvo automaticamente.",
        )
        with Horizontal(classes="note-status-bar"):
            yield Static("", id="note-save-state", markup=False)
            yield Static("", id="note-links", markup=False)

    def _project_options(self) -> list[tuple[str, int]]:
        g = self.app.giano
        projects = g.projects.list(include_archived=False)
        current = self.note.project_id if self.note else None
        if current and all(p.id != current for p in projects):
            p = g.projects.get(current)
            if p:
                projects.append(p)
        return [(f"{p.code} — {p.name}", p.id) for p in projects]

    def _initial_project(self):
        pid = self.note.project_id if self.note else self.default_project_id
        valid = {v for _, v in self._project_options()}
        return pid if pid in valid else Select.NULL

    def on_mount(self) -> None:
        self._saved_snapshot = self._latest = self.snapshot()
        self._refresh_status()

    # -- state ----------------------------------------------------------------

    def snapshot(self) -> tuple:
        return (
            self.query_one("#note-date", Input).value.strip(),
            self.query_one("#note-name", Input).value,
            select_value(self.query_one("#note-project", Select)),
            select_value(self.query_one("#note-status", Select)) or NoteStatus.PENDING,
            self.query_one("#note-body", TextArea).text,
        )

    @property
    def dirty(self) -> bool:
        return self.snapshot() != self._saved_snapshot

    # -- autosave -------------------------------------------------------------

    @on(Input.Changed)
    @on(Select.Changed)
    @on(TextArea.Changed)
    def _changed(self, event) -> None:
        event.stop()
        if self._saved_snapshot is None:
            return
        self._latest = self.snapshot()
        if self._latest == self._saved_snapshot:
            return
        self.query_one("#note-save-state", Static).update("✎ alterações pendentes…")
        if self._timer is not None:
            self._timer.stop()
        self._timer = self.set_timer(AUTOSAVE_DELAY, self.save_now)

    def _persist(self, snap: tuple) -> tuple[Note | None, bool]:
        """Write a snapshot to the database. Returns (note, date_ok)."""
        note_date, name, project_id, status, body = snap
        try:
            note_date = parse_date(note_date)
            date_ok = True
        except ValidationError:
            note_date = self.note.note_date if self.note else date.today().isoformat()
            date_ok = False
        g = self.app.giano
        if self.note is None:
            if not body.strip() and not name.strip():
                return None, date_ok  # nothing worth saving yet
            self.note = g.notes.create(name, project_id, body, note_date)
        self.note = g.notes.save(
            self.note.id, name=name, note_date=note_date, project_id=project_id,
            body=body, status=status,
        )
        self._saved_snapshot = snap
        return self.note, date_ok

    def save_now(self) -> Note | None:
        """Persist the editor content if it changed. Safe to call any time."""
        if self._timer is not None:
            self._timer.stop()
            self._timer = None
        if self._saved_snapshot is None:
            return self.note
        snap = self.snapshot() if self.is_mounted else self._latest
        if snap is None or snap == self._saved_snapshot:
            return self.note
        note, date_ok = self._persist(snap)
        if note is None or not self.is_mounted:
            return note
        if not snap[1].strip():
            # The service filled in a default name: show it without re-triggering a save.
            name_input = self.query_one("#note-name", Input)
            with name_input.prevent(Input.Changed):
                name_input.value = note.name
            self._saved_snapshot = self._latest = self.snapshot()
        state = self.query_one("#note-save-state", Static)
        if date_ok:
            state.update(f"✓ salvo às {datetime.now().strftime('%H:%M:%S')}")
        else:
            state.update("⚠ data inválida (use AAAA-MM-DD); o restante foi salvo")
        self.post_message(self.Saved(note))
        return note

    def _refresh_status(self) -> None:
        if self.note is None:
            self.query_one("#note-save-state", Static).update("nova anotação — salva ao digitar")
            self.query_one("#note-links", Static).update("")
            return
        self.query_one("#note-save-state", Static).update(
            f"✓ salvo · atualizada em {self.note.updated_at.replace('T', ' ')}"
        )
        self.refresh_links()

    def refresh_links(self) -> None:
        if self.note is None:
            return
        links = self.app.giano.notes.linked_items(self.note.id)
        self.query_one("#note-links", Static).update(
            f"Itens criados a partir desta anotação: {', '.join(links)}" if links else ""
        )

    def on_unmount(self) -> None:
        # Last-chance save (closing the screen, quitting the app…), from the
        # last known content since child widgets may already be gone.
        if self._timer is not None:
            self._timer.stop()
            self._timer = None
        if self._saved_snapshot is not None and self._latest not in (None, self._saved_snapshot):
            try:
                self._persist(self._latest)
            except Exception:  # pragma: no cover - never crash the app over this
                pass


class QuickNoteScreen(ModalScreen[Note | None]):
    """Global quick capture (ctrl+n) and ``giano nota``."""

    BINDINGS = [
        Binding("escape", "close", "Fechar (já está salvo)"),
        Binding("ctrl+s", "close", "Salvar e fechar", priority=True),
    ]

    def __init__(self, note: Note | None = None, project_id: int | None = None):
        super().__init__()
        self.note = note
        self.project_id = project_id

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog quick-note"):
            yield Static("Anotação rápida — salvamento automático", classes="dialog-title")
            yield NoteEditor(self.note, self.project_id, id="quick-editor")
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        target = "#note-body" if self.note else "#note-name"
        self.query_one(target).focus()

    def action_close(self) -> None:
        editor = self.query_one(NoteEditor)
        note = editor.save_now()
        if note:
            self.notify(f"Anotação \"{note.name}\" salva.")
        self.dismiss(note)


class NotesScreen(Screen[None]):
    BINDINGS = [
        Binding("escape", "back", "Voltar"),
        Binding("ctrl+s", "save", "Salvar agora", priority=True),
        Binding("ctrl+o", "create_item", "Criar item", priority=True),
        Binding("n", "new", "Nova"),
        Binding("s", "status", "Status"),
        Binding("v", "cycle_view", "Filtrar status"),
        Binding("delete", "delete", "Excluir"),
    ]

    VIEWS = [
        ("pendentes", [NoteStatus.PENDING]),
        ("todas", None),
        ("processadas", [NoteStatus.PROCESSED]),
        ("arquivadas", [NoteStatus.ARCHIVED]),
    ]

    def __init__(self, project_id: int | None = None, select_note_id: int | None = None):
        super().__init__()
        self.project_id = project_id
        self.select_note_id = select_note_id
        self.view_index = 1 if select_note_id else 0
        self.notes: list[Note] = []
        self.only_project = project_id is not None

    def compose(self) -> ComposeResult:
        yield Static(id="notes-title", classes="topbar", markup=False)
        with Horizontal(id="notes-body"):
            with Vertical(id="notes-left"):
                yield DataTable(id="notes-list", cursor_type="row", zebra_stripes=True)
                yield Static(
                    "n nova · s status · v filtrar · del excluir · ctrl+o criar item "
                    "(usa o texto selecionado) · enter editar",
                    classes="hint",
                )
            yield Vertical(id="notes-editor-box")
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        table = self.query_one("#notes-list", DataTable)
        table.add_column("Data", key="date")
        table.add_column("Nome", key="name")
        table.add_column("Proj.", key="project")
        table.add_column("Status", key="status")
        self.reload(self.select_note_id)
        table.focus()

    # -- list -----------------------------------------------------------------

    def _project_code(self) -> str | None:
        if self.project_id is None:
            return None
        p = self.app.giano.projects.get(self.project_id)
        return p.code if p else None

    def reload(self, select_id: int | None = None) -> None:
        label, statuses = self.VIEWS[self.view_index]
        code = self._project_code()
        pid = self.project_id if self.only_project else None
        self.notes = self.app.giano.notes.list(pid, statuses, include_unassigned=pid is not None)
        scope = f"{code} + sem projeto" if pid is not None else "todos os projetos"
        self.query_one("#notes-title", Static).update(
            f"Anotações — {label} · {scope} · {len(self.notes)}"
        )
        table = self.query_one("#notes-list", DataTable)
        with table.prevent(DataTable.RowHighlighted):
            table.clear()
            for n in self.notes:
                table.add_row(n.note_date, plain(n.name), n.project_code or "—", n.status.label, key=str(n.id))
        target = select_id if select_id is not None else (self.notes[0].id if self.notes else None)
        if target is not None and any(n.id == target for n in self.notes):
            table.move_cursor(row=table.get_row_index(str(target)))
            self._open(target)
        elif not self.notes:
            self._open(None)

    def refresh_data(self) -> str | None:
        """F5: reload the list. The open note keeps unsaved typing (it is saved first);
        if it was not touched here, it shows what was saved elsewhere."""
        editor = self._current_editor()
        if editor is None or editor.note is None:
            self.reload()
            return None
        note_id, warning, reopen = editor.note.id, None, False
        if editor.dirty:
            try:
                editor.save_now()
            except ValidationError:
                # Deleted elsewhere while being typed here: keep the text as a new note.
                editor.note = None
                note = editor.save_now()
                note_id = note.id if note else None
                warning = "A anotação aberta foi excluída fora daqui; o texto foi salvo como uma nova anotação."
        else:
            fresh = self.app.giano.notes.get(note_id)
            if fresh is None:
                note_id = None
            elif fresh != editor.note:
                reopen = True
        self.reload(note_id)
        if reopen:
            self._open(note_id, force=True)
        return warning

    def _current_editor(self) -> NoteEditor | None:
        editors = self.query(NoteEditor)
        return editors.first() if editors else None

    def _open(self, note_id: int | None, force: bool = False) -> None:
        editor = self._current_editor()
        if not force and editor is not None and editor.note is not None and editor.note.id == note_id:
            return
        note = self.app.giano.notes.get(note_id) if note_id is not None else None
        if note is None and note_id is not None:
            return
        if editor is not None:
            editor.save_now()
            editor._saved_snapshot = None  # saved; do not save again while being removed
        box = self.query_one("#notes-editor-box", Vertical)
        box.remove_children()
        if note is None:
            box.mount(Static("Nenhuma anotação. Tecle n para criar.", classes="hint"))
        else:
            box.mount(NoteEditor(note))

    def on_data_table_row_highlighted(self, event: DataTable.RowHighlighted) -> None:
        if event.row_key is not None and event.row_key.value is not None:
            self._open(int(event.row_key.value))

    def on_data_table_row_selected(self, event: DataTable.RowSelected) -> None:
        self._open(int(event.row_key.value))
        self.query_one("#note-body", TextArea).focus()

    def on_note_editor_saved(self, event: NoteEditor.Saved) -> None:
        n = event.note
        table = self.query_one("#notes-list", DataTable)
        try:
            table.get_row_index(str(n.id))
        except Exception:
            return
        table.update_cell(str(n.id), "date", n.note_date)
        table.update_cell(str(n.id), "name", plain(n.name))
        table.update_cell(str(n.id), "project", n.project_code or "—")
        table.update_cell(str(n.id), "status", n.status.label)

    # -- actions --------------------------------------------------------------

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        # Letter shortcuts only make sense when the list (not the editor) has focus.
        if action in ("new", "status", "cycle_view", "delete"):
            focused = self.focused
            if focused is not None and focused.id != "notes-list":
                return False
        return True

    def on_descendant_focus(self) -> None:
        self.refresh_bindings()

    def action_back(self) -> None:
        focused = self.focused
        if focused is not None and focused.id != "notes-list":
            self.query_one("#notes-list", DataTable).focus()
            return
        editor = self._current_editor()
        if editor:
            editor.save_now()
        self.dismiss(None)

    def action_save(self) -> None:
        editor = self._current_editor()
        if editor:
            editor.save_now()
            self.notify("Anotação salva.")

    def action_new(self) -> None:
        g = self.app.giano
        pid = self.project_id
        note = g.notes.create("", pid, "")
        if self.view_index not in (0, 1):
            self.view_index = 0
        self.reload(note.id)
        self.query_one("#note-name", Input).focus()

    def action_cycle_view(self) -> None:
        self.view_index = (self.view_index + 1) % len(self.VIEWS)
        self.reload()

    def _selected_note(self) -> Note | None:
        editor = self._current_editor()
        if editor and editor.note:
            editor.save_now()
            return editor.note
        return None

    def action_status(self) -> None:
        note = self._selected_note()
        if note is None:
            return

        def apply(status) -> None:
            editor = self._current_editor()
            if status is None or editor is None:
                return
            editor.query_one("#note-status", Select).value = status
            editor.save_now()

        self.app.push_screen(
            ChoiceDialog("Status da anotação", [(s.label, s) for s in NoteStatus], note.status), apply
        )

    def action_delete(self) -> None:
        note = self._selected_note()
        if note is None:
            return

        def answer(ok: bool | None) -> None:
            if ok:
                editor = self._current_editor()
                if editor:
                    editor._saved_snapshot = None  # don't resave a deleted note
                    editor.remove()
                self.app.giano.notes.delete(note.id)
                self.notify("Anotação excluída.")
                self.reload()

        self.app.push_screen(
            ConfirmDialog(f"Excluir a anotação \"{note.name}\" ({note.note_date})?\n"
                          "Itens criados a partir dela são mantidos."),
            answer,
        )

    def action_create_item(self) -> None:
        note = self._selected_note()
        if note is None:
            self.notify("Selecione uma anotação.")
            return
        g = self.app.giano
        pid = note.project_id or self.project_id or self.app.current_project_id
        project = g.projects.get(pid) if pid else None
        if project is None:
            self.notify("Defina o projeto da anotação antes de criar um item.", severity="warning")
            return
        if not project.active:
            self.notify(f"O projeto {project.code} está arquivado.", severity="warning")
            return
        selected = self.query_one("#note-body", TextArea).selected_text.strip()
        title, body = "", ""
        if selected:
            first, _, rest = selected.partition("\n")
            title = first.lstrip("-*# ").strip()[:120]
            body = rest.strip() if rest.strip() else ""
        draft = ItemDraft(
            title=title, body=body, source_note_id=note.id,
            source_label=f"{note.note_date} — {note.name}",
        )

        def done(view) -> None:
            if view is not None:
                self.notify(f"{view.code} criado a partir da anotação.")
                editor = self._current_editor()
                if editor:
                    editor.refresh_links()

        self.app.push_screen(ItemEditorScreen(project, draft=draft), done)
