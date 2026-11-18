"""Difficulty range filter ("1,3-5") and item selection in the HTML export."""

from __future__ import annotations

import asyncio

import pytest
from textual.widgets import DataTable, Input, SelectionList

from giano.domain import ItemStatus, Priority, ValidationError, format_ranges, parse_ranges
from giano.tui.app import GianoApp
from giano.tui.dialogs import Filters, FilterDialog, HtmlExportDialog
from giano.tui.main_screen import MainScreen
from tests.conftest import make

SIZE = (160, 48)


# ---------------------------------------------------------------- ranges


@pytest.mark.parametrize(
    "text, expected",
    [
        ("1,3-5", {1, 3, 4, 5}),
        (" 2 ", {2}),
        ("1-1", {1}),
        ("5-3", {3, 4, 5}),
        ("1, 2 - 3 ,5", {1, 2, 3, 5}),
        ("1;4", {1, 4}),
        ("3,3,2-3", {2, 3}),
    ],
)
def test_parse_ranges(text, expected):
    assert parse_ranges(text) == expected


@pytest.mark.parametrize("text", ["", " , ", "a", "1-", "-2", "1-2-3", "1.5", "0", "6", "2-7"])
def test_parse_ranges_invalid(text):
    with pytest.raises(ValidationError):
        parse_ranges(text, 1, 5)


def test_format_ranges_roundtrip():
    assert format_ranges({1, 3, 4, 5}) == "1,3-5"
    assert format_ranges({2}) == "2"
    assert format_ranges({1, 2, 4, 6, 7, 8}) == "1-2,4,6-8"
    for text in ("1,3-5", "2-4", "1,5"):
        assert format_ranges(parse_ranges(text)) == text


def test_filters_difficulty(g, project):
    views = [g.items.create(project.id, make(f"d{d}", difficulty=d)) for d in range(1, 6)]
    f = Filters(difficulty=parse_ranges("1,3-4"))
    assert [v.data.difficulty for v in views if f.matches(v)] == [1, 3, 4]
    assert f.active and f.describe() == "dificuldade=1,3-4, sem concluídos/descartados"
    both = Filters(difficulty=frozenset({4, 5}), status=ItemStatus.OPEN)
    assert [v.data.difficulty for v in views if both.matches(v)] == [4, 5]


# ---------------------------------------------------------------- HTML subset


@pytest.fixture
def five(g, project):
    vs = [g.items.create(project.id, make(f"Item {n}", difficulty=n)) for n in range(1, 6)]  # rev 1-5
    g.items.set_priority(vs[1].item_id, Priority.HIGH)   # rev 6: MERC-2
    g.items.set_priority(vs[3].item_id, Priority.LOW)    # rev 7: MERC-4
    return vs


def test_html_subset_only_has_selected_items(g, project, five):
    html = g.exports.export_html("MERC", numbers={1, 2}, compare_to=5).content
    assert "Item 1" in html and "Item 2" in html
    for n in (3, 4, 5):
        assert f"Item {n}" not in html and f"merc-{n}" not in html
    assert "2 itens (seleção de 5)" in html
    # the changes section only talks about selected items
    assert "MERC-2" in html.split('id="alteracoes"')[1]
    assert "prioridade Média → Baixa" not in html  # MERC-4's revision summary
    assert "<td>7</td>" not in html


def test_html_all_items_by_default(g, project, five):
    html = g.exports.export_html("MERC").content
    assert all(f"Item {n}" in html for n in range(1, 6))
    assert "seleção de" not in html


def test_html_subset_errors(g, project, five):
    with pytest.raises(ValidationError, match="MERC-9"):
        g.exports.export_html("MERC", numbers={1, 9})
    with pytest.raises(ValidationError):
        g.exports.export_html("MERC", numbers=set())
    g.items.delete(five[0].item_id)
    with pytest.raises(ValidationError, match="MERC-1"):
        g.exports.export_html("MERC", numbers={1})
    assert "Item 1" in g.exports.export_html("MERC", rev=5, numbers={1}).content


def test_cli_itens_option(g, project, five, tmp_path, monkeypatch):
    from typer.testing import CliRunner

    from giano.cli import app

    out = tmp_path / "cli"
    res = CliRunner().invoke(app, ["--db", g.db_path, "export", "html", "-p", "MERC",
                                   "--itens", "2-3", "-o", str(out)])
    assert res.exit_code == 0, res.output
    html = next(out.glob("*.html")).read_text(encoding="utf-8")
    assert "Item 2" in html and "Item 3" in html and "Item 1" not in html
    res = CliRunner().invoke(app, ["--db", g.db_path, "export", "html", "-p", "MERC", "--itens", "x"])
    assert res.exit_code == 1


# ---------------------------------------------------------------- TUI


def test_tui_difficulty_filter(g, project, five):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("f")
            await pilot.pause()
            assert isinstance(app.screen, FilterDialog)
            box = app.screen.query_one("#f-difficulty", Input)
            box.value = "9"
            await pilot.press("ctrl+s")  # invalid: stays open
            await pilot.pause()
            assert isinstance(app.screen, FilterDialog)
            box.value = "1,4-5"
            await pilot.press("ctrl+s")
            await pilot.pause()
            main = app.screen
            assert isinstance(main, MainScreen)
            assert main.query_one("#items", DataTable).row_count == 3
            assert "dificuldade=1,4-5" in str(main.query_one("#filterbar").render())
            await pilot.press("f")
            await pilot.pause()
            assert app.screen.query_one("#f-difficulty", Input).value == "1,4-5"

    asyncio.run(scenario())


def test_tui_html_export_selection(g, project, five, tmp_path):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("x")
            await pilot.pause()
            dialog = app.screen
            assert isinstance(dialog, HtmlExportDialog)
            sl = dialog.query_one(SelectionList)
            assert sorted(sl.selected) == [(n, 0) for n in range(1, 6)]  # all selected by default
            sl.deselect((3, 0))
            sl.deselect((5, 0))
            await pilot.pause()
            assert "3 de 5" in str(dialog.query_one("#export-count").render())
            await pilot.press("ctrl+s")
            await _wait_export(app, pilot)
            assert isinstance(app.screen, MainScreen)

    asyncio.run(scenario())
    files = list((tmp_path / "exports").glob("giano_MERC_rev7_*.html"))
    assert len(files) == 1
    html = files[0].read_text(encoding="utf-8")
    assert "Item 1" in html and "Item 4" in html
    assert "Item 3" not in html and "Item 5" not in html


def test_tui_html_export_only_filtered_button(g, project, five, tmp_path):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            main.filters = Filters(difficulty=frozenset({2, 3}))
            main.reload_items()
            await pilot.press("x")
            await pilot.pause()
            dialog = app.screen
            assert sorted(dialog.query_one(SelectionList).selected) == [(n, 0) for n in range(1, 6)]
            await pilot.click("#filtered")
            await pilot.pause()
            assert sorted(dialog.query_one(SelectionList).selected) == [(2, 0), (3, 0)]
            await pilot.press("ctrl+u")
            await pilot.pause()
            await pilot.press("ctrl+s")  # nothing selected: refuses
            await pilot.pause()
            assert app.screen is dialog
            await pilot.press("escape")

    asyncio.run(scenario())
    assert not list((tmp_path / "exports").glob("*.html"))


# ---------------------------------------------------------------- mark "Em desenvolvimento"


async def _wait_export(app, pilot):
    """The TUI exports in a worker thread behind a ProgressDialog."""
    await app.workers.wait_for_complete()
    for _ in range(5):
        await pilot.pause()


def _statuses(g, project):
    return {v.number: v.data.status for v in g.items.list_items(project.id)}


def test_html_mark_in_progress(g, project, five):
    g.items.set_status(five[3].item_id, ItemStatus.DONE)          # rev 8: MERC-4
    g.items.set_status(five[4].item_id, ItemStatus.IN_PROGRESS)   # rev 9: MERC-5
    g.items.set_status(five[1].item_id, ItemStatus.APPROVED)      # rev 10: MERC-2
    r = g.exports.export_html("MERC", numbers={1, 2, 4, 5}, mark_in_progress=True)
    assert r.marked == ("MERC-1", "MERC-2")
    assert "2 itens marcados como Em desenvolvimento (MERC-1, MERC-2)" in r.summary
    assert r.path.name.startswith("giano_MERC_rev10_")
    assert "Em desenvolvimento" not in r.content.split("MERC-2")[1].split("MERC-4")[0]
    assert _statuses(g, project) == {
        1: ItemStatus.IN_PROGRESS, 2: ItemStatus.IN_PROGRESS, 3: ItemStatus.OPEN,
        4: ItemStatus.DONE, 5: ItemStatus.IN_PROGRESS,
    }
    assert g.items.latest_revision(project.id) == 12  # one revision per marked item
    # nothing left to mark: no new revision
    again = g.exports.export_html("MERC", numbers={1, 2}, mark_in_progress=True)
    assert again.marked == () and g.items.latest_revision(project.id) == 12


def test_html_mark_in_progress_all_items(g, project, five):
    r = g.exports.export_html("MERC", mark_in_progress=True)
    assert r.marked == tuple(f"MERC-{n}" for n in range(1, 6))
    assert set(_statuses(g, project).values()) == {ItemStatus.IN_PROGRESS}


def test_html_mark_in_progress_only_latest_revision(g, project, five, tmp_path):
    with pytest.raises(ValidationError, match="última revisão"):
        g.exports.export_html("MERC", rev=5, out_dir=tmp_path / "old", mark_in_progress=True)
    assert not (tmp_path / "old").exists()
    assert g.exports.export_html("MERC", rev=7, mark_in_progress=True).marked  # 7 = latest


def test_html_mark_in_progress_rolls_back_if_file_fails(g, project, five, tmp_path):
    blocker = tmp_path / "not-a-folder"
    blocker.write_text("x", encoding="utf-8")
    with pytest.raises(OSError):
        g.exports.export_html("MERC", out_dir=blocker, mark_in_progress=True)
    assert g.items.latest_revision(project.id) == 7
    assert set(_statuses(g, project).values()) == {ItemStatus.OPEN}


def test_cli_em_desenvolvimento(g, project, five, tmp_path):
    from typer.testing import CliRunner

    from giano.cli import app

    res = CliRunner().invoke(app, ["--db", g.db_path, "export", "html", "-p", "MERC",
                                   "--itens", "2-3", "-o", str(tmp_path / "cli"),
                                   "--em-desenvolvimento"])
    assert res.exit_code == 0, res.output
    assert "MERC-2, MERC-3" in res.output
    s = _statuses(g, project)
    assert s[2] == s[3] == ItemStatus.IN_PROGRESS and s[1] == ItemStatus.OPEN


def test_tui_html_export_mark_in_progress(g, project, five, tmp_path, monkeypatch):
    from textual.widgets import Checkbox

    from giano.tui.dialogs import ProgressDialog

    seen = []
    original = ProgressDialog.set_progress

    def spy(self, done, total, what="itens"):
        seen.append((done, total))
        original(self, done, total, what)

    monkeypatch.setattr(ProgressDialog, "set_progress", spy)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await pilot.press("x")
            await pilot.pause()
            dialog = app.screen
            assert dialog.query_one("#export-mark", Checkbox).value is False  # off by default
            sl = dialog.query_one(SelectionList)
            sl.deselect((3, 0))
            dialog.query_one("#export-mark", Checkbox).value = True
            await pilot.press("ctrl+s")
            await pilot.pause()
            assert isinstance(app.screen, ProgressDialog) or seen  # progress shown meanwhile
            await _wait_export(app, pilot)
            assert isinstance(app.screen, MainScreen)
            assert app.screen.latest_rev == 11  # list reloaded after the status changes

    asyncio.run(scenario())
    assert seen == [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4)]
    assert len(list((tmp_path / "exports").glob("giano_MERC_rev7_*.html"))) == 1
    s = _statuses(g, project)
    assert s[3] == ItemStatus.OPEN
    assert all(s[n] == ItemStatus.IN_PROGRESS for n in (1, 2, 4, 5))


def test_tui_html_export_old_revision_has_no_mark_option(g, project, five):
    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            main.view_rev = 5
            main.reload_items()
            await pilot.press("x")
            await pilot.pause()
            assert isinstance(app.screen, HtmlExportDialog)
            assert not app.screen.query("#export-mark")
            await pilot.press("escape")

    asyncio.run(scenario())


def test_tui_cancel_export_keeps_preview_and_survives_stale_clicks(g, project, five):
    """Cancelling the HTML export used to crash Textual: the preview re-rendered on screen
    resume and a click in that window hit a removed Markdown block."""
    from textual import events
    from textual.widgets import Markdown

    def mouse(cls, x, y):
        return cls(None, x, y, 0, 0, 1, False, False, False, screen_x=x, screen_y=y)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            md = main.query_one("#preview", Markdown)
            blocks = list(md.children)
            await pilot.press("x")
            await pilot.pause()
            await pilot.click("#cancel")
            await pilot.pause()
            assert app.screen is main
            assert list(md.children) == blocks  # same text: not rebuilt
            r = md.region
            for ticks in range(15):  # clicks at every moment of a re-render
                md.update(f"# Outro {ticks}\n\nParágrafo {ticks}.\n\nMais um.")
                for _ in range(ticks):
                    await asyncio.sleep(0)
                for y in range(r.y, r.bottom):
                    await app.on_event(mouse(events.MouseDown, r.x + 3, y))
                    await app.on_event(mouse(events.MouseUp, r.x + 3, y))
                await pilot.pause()

    asyncio.run(scenario())


def test_html_mark_in_progress_reports_progress(g, project, five):
    g.items.set_status(five[0].item_id, ItemStatus.DONE)
    calls = []
    g.exports.export_html("MERC", mark_in_progress=True, progress=lambda d, t: calls.append((d, t)))
    assert calls == [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4)]
    calls.clear()
    g.exports.export_html("MERC", mark_in_progress=True, progress=lambda d, t: calls.append((d, t)))
    assert calls == [(0, 0)]  # nothing left to mark
    calls.clear()
    g.exports.export_html("MERC", progress=lambda d, t: calls.append((d, t)))
    assert calls == []  # not marking: no progress steps


def test_tui_progress_dialog_blocks_app_actions(g, project, five):
    from giano.tui.dialogs import ProgressDialog

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            dialog = ProgressDialog("Exportando", "Gerando o HTML…")
            await app.push_screen(dialog)
            await pilot.pause()
            assert app.check_action("refresh_data", ()) is False
            assert app.check_action("quick_note", ()) is False
            await pilot.press("ctrl+n", "escape")  # neither opens a note nor closes it
            await pilot.pause()
            assert app.screen is dialog
            dialog.set_progress(2, 5, "itens")
            assert "2 de 5 itens · faltam 3" in str(dialog.query_one("#progress-text").render())
            dialog.dismiss()
            await pilot.pause()
            assert app.check_action("refresh_data", ()) is not False

    asyncio.run(scenario())
