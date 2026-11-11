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
    assert f.active and f.describe() == "dificuldade=1,3-4"
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
            assert sorted(sl.selected) == [1, 2, 3, 4, 5]  # all selected by default
            sl.deselect(3)
            sl.deselect(5)
            await pilot.pause()
            assert "3 de 5" in str(dialog.query_one("#export-count").render())
            await pilot.press("ctrl+s")
            await pilot.pause()
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
            assert sorted(dialog.query_one(SelectionList).selected) == [1, 2, 3, 4, 5]
            await pilot.click("#filtered")
            await pilot.pause()
            assert sorted(dialog.query_one(SelectionList).selected) == [2, 3]
            await pilot.press("ctrl+u")
            await pilot.pause()
            await pilot.press("ctrl+s")  # nothing selected: refuses
            await pilot.pause()
            assert app.screen is dialog
            await pilot.press("escape")

    asyncio.run(scenario())
    assert not list((tmp_path / "exports").glob("*.html"))
