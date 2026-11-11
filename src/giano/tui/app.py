"""Textual application entry point."""

from __future__ import annotations

from textual.app import App
from textual.binding import Binding

from giano.services import Giano
from giano.tui.main_screen import MainScreen
from giano.tui.notes import NoteEditor, QuickNoteScreen


class GianoApp(App):
    TITLE = "Giano"
    CSS_PATH = "giano.tcss"
    ENABLE_COMMAND_PALETTE = True
    BINDINGS = [
        Binding("ctrl+n", "quick_note", "Nota rápida", priority=True),
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

    def action_quick_note(self) -> None:
        if isinstance(self.screen, QuickNoteScreen):
            return
        self.push_screen(QuickNoteScreen(project_id=self.current_project_id))

    def notify(self, message: str, *, title: str = "", severity="information",
               timeout: float | None = None, markup: bool = True) -> None:
        # Messages often contain user text (titles, names): never parse it as markup.
        super().notify(message, title=title, severity=severity, timeout=timeout, markup=False)

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
