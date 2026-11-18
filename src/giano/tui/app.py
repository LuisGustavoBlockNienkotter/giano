"""Textual application entry point."""

from __future__ import annotations

from textual import events
from textual.app import App
from textual.binding import Binding

from giano.domain import ValidationError
from giano.services import Giano
from giano.tui.dialogs import ProgressDialog
from giano.tui.main_screen import MainScreen
from giano.tui.notes import NoteEditor, QuickNoteScreen


class GianoApp(App):
    TITLE = "Giano"
    CSS_PATH = "giano.tcss"
    ENABLE_COMMAND_PALETTE = True
    BINDINGS = [
        Binding("ctrl+n", "quick_note", "Nota", priority=True),
        Binding("f5", "refresh_data", "Atualizar", priority=True),
    ]

    def __init__(self, giano: Giano, quick_note_id: int | None = None):
        super().__init__()
        self.giano = giano
        self.quick_note_id = quick_note_id
        self.current_project_id: int | None = None

    def on_mount(self) -> None:
        self.theme = "textual-dark"
        if self.quick_note_id is not None:
            # `giano nota`: open just the quick note editor, exit when it closes.
            note = self.giano.notes.get(self.quick_note_id)
            self.push_screen(QuickNoteScreen(note), lambda _: self.exit())
        else:
            self.push_screen(MainScreen())

    def check_action(self, action: str, parameters) -> bool | None:
        # A worker thread is using the database (ProgressDialog on top): F5, quick note,
        # the command palette… would use the same connection mid-transaction. Wait.
        if isinstance(self.screen, ProgressDialog) and action != "quit":
            return False
        return super().check_action(action, parameters)

    async def on_event(self, event: events.Event) -> None:
        if isinstance(event, events.MouseDown) and self._hits_removed_widget(event):
            # Textual bug: a click that lands while a Markdown is re-rendering (e.g. the
            # main screen's preview reloading right after a dialog closes) can hit a block
            # that was already removed but is still in the screen's map, and Textual's text
            # selection crashes on it ('NoneType' object has no attribute 'region'). The
            # screen is changing under the mouse anyway: drop the click.
            return
        await super().on_event(event)

    def _hits_removed_widget(self, event: events.MouseDown) -> bool:
        try:
            widget, _ = self.screen.get_widget_and_offset_at(event.x, event.y)
        except Exception:
            return False
        return widget is not None and widget is not self.screen and not widget.is_attached

    def action_quick_note(self) -> None:
        if isinstance(self.screen, QuickNoteScreen):
            return
        self.push_screen(QuickNoteScreen(project_id=self.current_project_id))

    def notify(self, message: str, *, title: str = "", severity="information",
               timeout: float | None = None, markup: bool = True) -> None:
        # Messages often contain user text (titles, names): never parse it as markup.
        super().notify(message, title=title, severity=severity, timeout=timeout, markup=False)

    def action_refresh_data(self) -> None:
        """Reload data on every open screen (changes made by another Giano, the CLI,
        a script…). Screens opt in by defining ``refresh_data() -> str | None``; a
        returned string is a warning for the user (e.g. an item edited elsewhere)."""
        warnings = []
        for screen in reversed(self.screen_stack):
            refresh = getattr(screen, "refresh_data", None)
            if refresh is not None and screen.is_mounted:
                try:
                    message = refresh()
                except ValidationError as exc:  # e.g. no connection to Turso
                    self.notify(str(exc), severity="error", timeout=15)
                    return
                if message:
                    warnings.append(message)
        for message in warnings:
            self.notify(message, severity="warning", timeout=10)
        if not warnings:
            self.notify("Dados atualizados.", timeout=2)

    def save_all_notes(self) -> None:
        """Flush every open note editor (used before quitting)."""
        for screen in self.screen_stack:
            for editor in screen.query(NoteEditor):
                editor.save_now()

    async def action_quit(self) -> None:
        self.save_all_notes()
        self.exit()

    def exit(self, *args, **kwargs) -> None:  # type: ignore[override]
        try:
            self.save_all_notes()
        except Exception:  # pragma: no cover
            pass
        super().exit(*args, **kwargs)
