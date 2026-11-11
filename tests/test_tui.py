"""TUI smoke tests with Textual's pilot (no real terminal needed)."""

from __future__ import annotations

import asyncio

from textual.widgets import DataTable, Input, TextArea

from giano.services.demo import create_demo
from giano.tui.app import GianoApp
from giano.tui.item_editor import ItemEditorScreen
from giano.tui.main_screen import MainScreen
from giano.tui.notes import NotesScreen, QuickNoteScreen
from giano.tui.revisions import DiffScreen, RevisionsScreen
from giano.tui.search import SearchScreen

SIZE = (160, 48)


def run(coro):
    return asyncio.run(coro)


def test_main_screen_shows_latest_revision(g):
    create_demo(g)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            screen = app.screen
            assert isinstance(screen, MainScreen)
            assert screen.project.code == "LOJA"
            assert screen.view_rev is None and not screen.read_only
            table = screen.query_one("#items", DataTable)
            assert table.row_count == len(g.items.list_items(screen.project.id))
            assert "(última)" in str(screen.query_one("#topbar").render())

    run(scenario())


def test_create_item_via_editor(g, project):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("n")
            await pilot.pause()
            assert isinstance(app.screen, ItemEditorScreen)
            app.screen.query_one("#title", Input).value = "Item pela TUI"
            app.screen.query_one("#tags", Input).value = "tui, teste"
            app.screen.query_one("#body", TextArea).load_text("Corpo **md**")
            await pilot.press("ctrl+t")  # preview
            await pilot.pause()
            await pilot.press("ctrl+s")
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)
            assert app.screen.query_one("#items", DataTable).row_count == 1

    run(scenario())
    items = g.items.list_items(project.id)
    assert [(v.code, v.data.title, v.data.tags) for v in items] == [
        ("MERC-1", "Item pela TUI", ("teste", "tui"))
    ]
    assert g.items.latest_revision(project.id) == 1


def test_edit_without_change_creates_no_revision(g, project):
    from tests.conftest import make

    g.items.create(project.id, make("A", body="x"))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("e")
            await pilot.pause()
            assert isinstance(app.screen, ItemEditorScreen)
            await pilot.press("ctrl+s")
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)

    run(scenario())
    assert g.items.latest_revision(project.id) == 1


def test_old_revision_is_read_only_and_delete_confirms(g):
    create_demo(g)
    loja = g.projects.get_by_code("LOJA")
    latest = g.items.latest_revision(loja.id)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            await pilot.press("r")
            await pilot.pause()
            assert isinstance(app.screen, RevisionsScreen)
            await pilot.press("d")  # diff selected (latest) vs previous
            await pilot.pause()
            assert isinstance(app.screen, DiffScreen)
            await pilot.press("escape")
            await pilot.pause()
            # go to revision 3 and open it
            table = app.screen.query_one(DataTable)
            table.move_cursor(row=table.get_row_index("3"))
            await pilot.press("enter")
            await pilot.pause()
            assert app.screen is main
            assert main.read_only and main.view_rev == 3
            assert f"Rev. 3 de {latest} — somente leitura" in str(main.query_one("#topbar").render())
            assert main.query_one("#items", DataTable).row_count == 3
            await pilot.press("n")  # disabled in read-only mode
            await pilot.pause()
            assert app.screen is main
            await pilot.press("l")  # back to latest
            await pilot.pause()
            assert not main.read_only
            await pilot.press("delete")
            await pilot.pause()
            await pilot.press("escape")  # cancel confirmation
            await pilot.pause()
            assert g.items.latest_revision(loja.id) == latest
            await pilot.press("delete")
            await pilot.pause()
            await pilot.click("#yes")
            await pilot.pause()

    run(scenario())
    assert g.items.latest_revision(loja.id) == latest + 1


def test_quick_note_autosaves(g, project):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("ctrl+n")
            await pilot.pause()
            assert isinstance(app.screen, QuickNoteScreen)
            await pilot.press(*"Daily")
            app.screen.query_one("#note-body", TextArea).focus()
            await pilot.press(*"ideia boa")
            await pilot.pause(1.0)  # autosave delay
            # saved while still typing, before closing
            notes = g.notes.list()
            assert [(n.name, n.body, n.project_id) for n in notes] == [("Daily", "ideia boa", project.id)]
            await pilot.press(*" mais")
            await pilot.press("escape")
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)

    run(scenario())
    assert g.notes.list()[0].body == "ideia boa mais"


def test_notes_screen_and_item_from_note(g, project):
    note = g.notes.create("Reunião", project.id, "Criar relatório\ndetalhes do relatório")

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("a")
            await pilot.pause()
            assert isinstance(app.screen, NotesScreen)
            body = app.screen.query_one("#note-body", TextArea)
            body.select_all()
            await pilot.press("ctrl+o")
            await pilot.pause()
            assert isinstance(app.screen, ItemEditorScreen)
            assert app.screen.query_one("#title", Input).value == "Criar relatório"
            await pilot.press("ctrl+s")
            await pilot.pause()
            assert isinstance(app.screen, NotesScreen)
            await pilot.press("escape")
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)

    run(scenario())
    item = g.items.list_items(project.id)[0]
    assert item.source_note_id == note.id
    assert item.data.body == "detalhes do relatório"


def test_search_jumps_to_item(g):
    create_demo(g)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("/")
            await pilot.pause()
            assert isinstance(app.screen, SearchScreen)
            await pilot.press(*"holerite")
            await pilot.pause()
            await pilot.press("enter")  # focus results
            await pilot.press("enter")  # open first
            await pilot.pause()
            main = app.screen
            assert isinstance(main, MainScreen)
            assert main.project.code == "RH"
            assert main.selected_item().data.title.startswith("Holerite")

    run(scenario())


def test_cli_note_mode(g, project):
    note = g.notes.create("Via CLI", project.id)

    async def scenario():
        app = GianoApp(g, quick_note_id=note.id)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            assert isinstance(app.screen, QuickNoteScreen)
            await pilot.press(*"texto")
            await pilot.press("ctrl+s")
            await pilot.pause()

    run(scenario())
    assert g.notes.get(note.id).body == "texto"


def test_user_text_with_markup_characters(g):
    from tests.conftest import make

    p = g.projects.create("X", "Sistema [bold]x[/] [link=a]", "desc [red]")
    g.items.create(p.id, make("Título [b] com [/] colchetes", tags=("[tag]",)))
    g.notes.create("Reunião [teste]", p.id, "corpo [x]")

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            assert "Sistema [bold]x[/]" in str(app.screen.query_one("#topbar").render())
            await pilot.press("s")
            await pilot.pause()
            await pilot.press("escape")
            await pilot.press("delete")
            await pilot.pause()
            await pilot.press("escape")
            await pilot.press("a")
            await pilot.pause()
            assert isinstance(app.screen, NotesScreen)
            await pilot.press("escape")
            await pilot.press("r")
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()

    run(scenario())
