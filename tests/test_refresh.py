"""Global F5 refresh: changes made outside the open app (another Giano, the CLI, scripts)."""

from __future__ import annotations

import asyncio
from dataclasses import replace

import pytest
from textual.widgets import DataTable, Input, TextArea

from giano.domain import ItemStatus, Priority
from giano.services import Giano
from giano.tui.app import GianoApp
from giano.tui.dialogs import ConfirmDialog, FilterDialog
from giano.tui.item_editor import ItemEditorScreen
from giano.tui.main_screen import MainScreen
from giano.tui.notes import NotesScreen
from tests.conftest import make

SIZE = (160, 48)


@pytest.fixture
def other(g):
    """A second, independent connection to the same database file."""
    o = Giano.open(g.db_path)
    yield o
    o.close()


def run(coro):
    asyncio.run(coro)


def test_f5_reloads_main_list(g, project, other):
    a = g.items.create(project.id, make("A"))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            table = main.query_one("#items", DataTable)
            assert table.row_count == 1
            other.items.create(project.id, make("Criado fora"))
            other.items.set_priority(a.item_id, Priority.CRITICAL)
            other.projects.create("P2K", "P2000")
            await pilot.pause()
            assert table.row_count == 1  # nothing changes by itself
            await pilot.press("f5")
            await pilot.pause()
            assert table.row_count == 2
            assert main.latest_rev == 3 and "Rev. 3 (última)" in str(main.query_one("#topbar").render())
            assert main.selected_item().data.priority == Priority.CRITICAL  # selection kept
            assert main.query_one("#project-list").option_count == 2

    run(scenario())


def test_f5_keeps_old_revision_and_filters(g, project, other):
    g.items.create(project.id, make("A"))
    g.items.create(project.id, make("B", difficulty=5))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            main.view_rev = 1
            main.reload_items()
            other.items.create(project.id, make("C"))
            await pilot.press("f5")
            await pilot.pause()
            assert main.read_only and main.view_rev == 1 and main.latest_rev == 3
            assert "Rev. 1 de 3" in str(main.query_one("#topbar").render())

    run(scenario())


def test_f5_from_a_dialog_refreshes_the_screen_below(g, project, other):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            await pilot.press("f")
            await pilot.pause()
            assert isinstance(app.screen, FilterDialog)
            other.items.create(project.id, make("Fora"))
            await pilot.press("f5")
            await pilot.pause()
            assert isinstance(app.screen, FilterDialog)  # dialog stays open
            assert main.query_one("#items", DataTable).row_count == 1

    run(scenario())


def test_f5_notes_untouched_editor_shows_external_text(g, project, other):
    n = g.notes.create("Reunião", project.id, "versão 1")

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("a")
            await pilot.pause()
            screen = app.screen
            assert isinstance(screen, NotesScreen)
            other.notes.save(n.id, name="Reunião", note_date=n.note_date, project_id=project.id,
                             body="versão de fora", status=n.status)
            other.notes.create("Outra", project.id, "nova")
            await pilot.press("f5")
            await pilot.pause()
            assert screen.query_one("#notes-list", DataTable).row_count == 2
            assert screen.query_one("#note-body", TextArea).text == "versão de fora"

    run(scenario())
    assert g.notes.get(n.id).body == "versão de fora"


def test_f5_notes_unsaved_typing_is_never_lost(g, project, other):
    n = g.notes.create("Reunião", project.id, "início")

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("a")
            await pilot.pause()
            screen = app.screen
            screen.query_one("#note-body", TextArea).load_text("digitado aqui")
            await pilot.pause(0.1)  # before the autosave delay
            other.notes.save(n.id, name="Reunião", note_date=n.note_date, project_id=project.id,
                             body="mudado fora", status=n.status)
            await pilot.press("f5")
            await pilot.pause()
            assert screen.query_one("#note-body", TextArea).text == "digitado aqui"

    run(scenario())
    assert g.notes.get(n.id).body == "digitado aqui"


def test_f5_item_editor_reloads_when_untouched(g, project, other):
    a = g.items.create(project.id, make("Original"))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("e")
            await pilot.pause()
            editor = app.screen
            assert isinstance(editor, ItemEditorScreen)
            other.items.update(a.item_id, replace(a.data, title="Título de fora", status=ItemStatus.APPROVED))
            await pilot.press("f5")
            await pilot.pause()
            assert editor.query_one("#title", Input).value == "Título de fora"
            await pilot.press("ctrl+s")  # nothing changed here: no confirmation, no revision
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)

    run(scenario())
    assert g.items.latest_revision(project.id) == 2


def test_item_editor_asks_before_overwriting_external_change(g, project, other):
    a = g.items.create(project.id, make("Original"))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("e")
            await pilot.pause()
            editor = app.screen
            editor.query_one("#title", Input).value = "Editado aqui"
            other.items.update(a.item_id, replace(a.data, title="Editado fora"))
            await pilot.press("f5")  # warns, keeps the edit
            await pilot.pause()
            assert editor.query_one("#title", Input).value == "Editado aqui"
            await pilot.press("ctrl+s")
            await pilot.pause()
            assert isinstance(app.screen, ConfirmDialog)
            await pilot.press("escape")  # don't overwrite
            await pilot.pause()
            assert app.screen is editor
            assert g.items.get(a.item_id).data.title == "Editado fora"
            await pilot.press("ctrl+s")
            await pilot.pause()
            await pilot.click("#yes")
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)

    run(scenario())
    assert g.items.get(a.item_id).data.title == "Editado aqui"
    assert [v.data.title for v in g.items.history(a.item_id)] == ["Original", "Editado fora", "Editado aqui"]


def test_f5_item_editor_in_view_mode_rerenders(g, project, other):
    """Reading an item in view mode (ctrl+t): F5 must update the rendered text too."""
    from textual.widgets import Markdown

    a = g.items.create(project.id, make("Original", body="texto antigo"))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("e")
            await pilot.pause()
            editor = app.screen
            await pilot.press("ctrl+t")  # view mode
            await pilot.pause()
            md = editor.query_one("#body-markdown", Markdown)
            assert md.source == "texto antigo"
            other.items.update(a.item_id, replace(a.data, body="texto novo de fora"))
            await pilot.press("f5")
            await pilot.pause()
            assert editor.query_one("#body", TextArea).text == "texto novo de fora"
            assert md.source == "texto novo de fora"

    run(scenario())
