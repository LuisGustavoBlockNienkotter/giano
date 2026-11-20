"""TUI smoke tests with Textual's pilot (no real terminal needed)."""

from __future__ import annotations

import asyncio

from textual.widgets import DataTable, Input, TextArea

from giano.domain import ItemStatus
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
            # Default filter hides Concluído/Descartado (the demo has one done item).
            items = g.items.list_items(screen.project.id)
            open_items = [v for v in items if v.data.status not in (ItemStatus.DONE, ItemStatus.DISCARDED)]
            assert len(open_items) < len(items)
            assert table.row_count == len(open_items)
            assert "sem concluídos/descartados" in str(screen.query_one("#filterbar").render())
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
            await app.workers.wait_for_complete()  # the change runs in a worker
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


def test_hide_closed_is_default_and_search_still_reaches_closed_items(g, project):
    from tests.conftest import make
    from giano.tui.dialogs import FilterDialog
    from textual.widgets import Checkbox

    g.items.create(project.id, make("Aberto"))
    done = g.items.create(project.id, make("Entregue zebra", status=ItemStatus.DONE))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            table = main.query_one("#items", DataTable)
            assert table.row_count == 1
            # the dialog opens with the box checked; "Limpar" keeps the default
            await pilot.press("f")
            await pilot.pause()
            assert isinstance(app.screen, FilterDialog)
            assert app.screen.query_one("#f-closed", Checkbox).value is True
            await pilot.click("#clear")
            await pilot.pause()
            assert table.row_count == 1
            # search jumps to a done item: it must be visible and selected
            await pilot.press("/")
            await pilot.pause()
            await pilot.press(*"zebra")
            await pilot.pause()
            await pilot.press("enter", "enter")
            await pilot.pause()
            assert main.selected_item().item_id == done.item_id
            assert table.row_count == 2
            assert not main.filters.hide_closed

    run(scenario())


# ---------------------------------------------------------------- changes in background


def _status_scenario(g, project, monkeypatch, set_status):
    """Press s, choose "Em desenvolvimento"; ``set_status`` replaces the service call."""
    import threading

    from tests.conftest import make
    from giano.tui.dialogs import ChoiceDialog, ProgressDialog

    v = g.items.create(project.id, make("Item A"))
    gate = threading.Event()
    real = g.items.set_status

    def slow(item_id, status):
        assert gate.wait(5)
        return set_status(real, item_id, status)

    monkeypatch.setattr(g.items, "set_status", slow)
    seen = {}

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            reloads = []
            monkeypatch.setattr(main, "reload_projects", lambda *a, **k: reloads.append(1))
            await pilot.press("s")
            await pilot.pause()
            assert isinstance(app.screen, ChoiceDialog)
            app.screen.dismiss(ItemStatus.IN_PROGRESS)
            for _ in range(3):
                await pilot.pause()
            # Loading while the change runs; the main screen does not touch the database.
            assert isinstance(app.screen, ProgressDialog)
            assert "status → Em desenvolvimento" in str(app.screen.query_one(".dialog-title").render())
            assert app.check_action("refresh_data", ()) is False
            assert reloads == []
            gate.set()
            await app.workers.wait_for_complete()
            for _ in range(5):
                await pilot.pause()
            assert app.screen is main
            seen["reloads_after"] = len(reloads)
            seen["rows"] = [str(c) for c in main.query_one("#items", DataTable).get_row_at(0)]
            seen["notes"] = [(n.title, n.message, n.severity) for n in app._notifications]

    run(scenario())
    return v, seen


def test_tui_quick_status_shows_loading_then_ok(g, project, monkeypatch):
    v, seen = _status_scenario(g, project, monkeypatch, lambda real, i, s: real(i, s))
    assert g.items.get(v.item_id).data.status == ItemStatus.IN_PROGRESS
    assert "Em desenvolvimento" in seen["rows"]  # table refreshed with the worker's data
    assert seen["reloads_after"] == 0  # the worker already re-read the list
    assert ("OK", "✓ MERC-1: status Aberto → Em desenvolvimento (Rev. 2).", "information") \
        in seen["notes"]


def test_tui_quick_status_refused_shows_error(g, project, monkeypatch):
    from giano.domain import ValidationError

    def refuse(real, item_id, status):
        raise ValidationError("Sem conexão com o banco.")

    v, seen = _status_scenario(g, project, monkeypatch, refuse)
    assert g.items.get(v.item_id).data.status == ItemStatus.OPEN
    assert ("Operação não concluída", "Sem conexão com o banco.", "error") in seen["notes"]
    assert seen["reloads_after"] == 1  # refused: the list is re-read as it is


def test_tui_quick_status_same_status_does_nothing(g, project):
    from tests.conftest import make
    from giano.tui.dialogs import ChoiceDialog

    g.items.create(project.id, make("Item A"))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("s")
            await pilot.pause()
            assert isinstance(app.screen, ChoiceDialog)
            app.screen.dismiss(ItemStatus.OPEN)
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)  # no loading for nothing

    run(scenario())
    assert g.items.latest_revision(project.id) == 1


# ---------------------------------------------------------------- multi-selection


def _codes(main):
    return [v.code for v in main.selected_items()]


def test_tui_multi_selection_keyboard_and_batch_status(g, project):
    from tests.conftest import make
    from giano.tui.dialogs import ChoiceDialog

    for title in "ABCDE":
        g.items.create(project.id, make(title))
    g.items.set_status(g.items.get_by_code("MERC-5").item_id, ItemStatus.APPROVED)  # rev 6

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            main.sort_column = "id"
            main.reload_items()
            table = main.query_one("#items", DataTable)
            table.move_cursor(row=1)
            await pilot.press("shift+down", "shift+down")  # MERC-2..4
            await pilot.pause()
            assert _codes(main) == ["MERC-2", "MERC-3", "MERC-4"]
            assert str(table.get_row_at(1)[0]) == "●" and str(table.get_row_at(0)[0]) == " "
            assert "3 selecionados" in str(main.query_one("#filterbar").render())
            await pilot.press("shift+up")  # range shrinks back toward the anchor
            await pilot.pause()
            assert _codes(main) == ["MERC-2", "MERC-3"]
            table.move_cursor(row=4)  # a plain move keeps the selection
            await pilot.press("space")
            await pilot.pause()
            assert _codes(main) == ["MERC-2", "MERC-3", "MERC-5"]
            await pilot.press("s")
            await pilot.pause()
            assert isinstance(app.screen, ChoiceDialog)
            assert "3 itens selecionados" in str(app.screen.query_one(".dialog-title").render())
            app.screen.dismiss(ItemStatus.APPROVED)
            await pilot.pause()
            await app.workers.wait_for_complete()
            for _ in range(5):
                await pilot.pause()
            assert app.screen is main
            assert _codes(main) == []  # the batch consumed the selection
            notes = [n.message for n in app._notifications]
            assert "✓ 2 itens: status → Aprovado (Rev. 7): MERC-2, MERC-3." in notes
            assert str(table.get_row_at(1)[0]) == " "
            await pilot.press("ctrl+a")
            await pilot.pause()
            assert len(_codes(main)) == 5
            await pilot.press("escape")
            await pilot.pause()
            assert _codes(main) == []

    run(scenario())
    statuses = {v.code: v.data.status for v in g.items.list_items(project.id)}
    assert statuses == {"MERC-1": ItemStatus.OPEN, "MERC-2": ItemStatus.APPROVED,
                        "MERC-3": ItemStatus.APPROVED, "MERC-4": ItemStatus.OPEN,
                        "MERC-5": ItemStatus.APPROVED}
    assert g.items.latest_revision(project.id) == 7  # the batch made one revision


def test_tui_multi_selection_with_ctrl_and_shift_click(g, project):
    from tests.conftest import make

    for title in "ABCDE":
        g.items.create(project.id, make(title))

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            main.sort_column = "id"
            main.reload_items()
            # Row i is at y = i + 1 (the header is the first line).
            await pilot.click("#items", offset=(10, 1), control=True)  # MERC-1
            await pilot.click("#items", offset=(10, 3), control=True)  # MERC-3
            await pilot.pause()
            assert _codes(main) == ["MERC-1", "MERC-3"]
            assert app.screen is main  # ctrl+click never opens the editor
            await pilot.click("#items", offset=(10, 5), shift=True)  # range from MERC-3
            await pilot.pause()
            assert _codes(main) == ["MERC-3", "MERC-4", "MERC-5"]
            await pilot.click("#items", offset=(10, 1), shift=True, control=True)  # adds 1..3
            await pilot.pause()
            assert len(_codes(main)) == 5
            await pilot.click("#items", offset=(10, 4), control=True)  # toggles MERC-4 off
            await pilot.pause()
            assert _codes(main) == ["MERC-1", "MERC-2", "MERC-3", "MERC-5"]
            await pilot.click("#items", offset=(10, 2))  # plain click: cursor only
            await pilot.pause()
            assert len(_codes(main)) == 4 and app.screen is main
            await pilot.press("v")  # another list (the archive): selection cleared
            await pilot.pause()
            await pilot.press("v")
            await pilot.pause()
            assert _codes(main) == []

    run(scenario())
    assert g.items.latest_revision(project.id) == 5
