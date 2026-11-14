"""Addenda: MERC-1.1 is an item of its own that keeps the number of its original item."""

from __future__ import annotations

import asyncio
import sqlite3
from dataclasses import replace

import pytest
from textual.widgets import DataTable, Input, Markdown

from giano.db import Database
from giano.db.migrations import V1, V2
from giano.domain import ItemStatus, ItemType, Priority, ValidationError, parse_item_keys
from giano.tui.app import GianoApp
from giano.tui.item_editor import ItemEditorScreen
from giano.tui.main_screen import MainScreen
from tests.conftest import make
from tests.test_cli_items import cli

SIZE = (160, 48)


@pytest.fixture
def original(g, project):
    return g.items.create(
        project.id,
        make("Cálculo de frete", priority=Priority.HIGH, difficulty=4, tags=("frete",),
             body="Texto **original** completo."),
    )


def adj(title="Ajuste do frete", **kw):
    kw.setdefault("type", ItemType.ADJUSTMENT)
    kw.setdefault("body", "Só o ajuste.")
    return make(title, **kw)


# ---------------------------------------------------------------- service rules


def test_create_addendum_keeps_number_and_original(g, project, original):
    rev_before = g.items.latest_revision(project.id)
    a = g.items.create_addendum(original.item_id, adj())
    assert a.code == "MERC-1.1" and a.key == (1, 1) and a.is_addendum
    assert a.parent_code == "MERC-1"
    assert a.item_id != original.item_id
    assert a.rev_start == rev_before + 1
    assert g.items.revision(project.id, a.rev_start).summary == (
        "MERC-1.1 criado (adendo de MERC-1): Ajuste do frete"
    )
    # The original is untouched (same version, same text).
    assert g.items.get(original.item_id).version_id == original.version_id
    assert g.items.parent(a).item_id == original.item_id
    assert [x.code for x in g.items.addenda(original)] == ["MERC-1.1"]
    # Normal numbering is not affected by addenda.
    assert g.items.create(project.id, make("Outro")).code == "MERC-2"


def test_addendum_of_addendum_goes_to_the_original(g, original):
    a1 = g.items.create_addendum(original.item_id, adj("primeiro"))
    a2 = g.items.create_addendum(a1.item_id, adj("segundo"))
    assert a2.code == "MERC-1.2"
    assert [x.code for x in g.items.addenda(original)] == ["MERC-1.1", "MERC-1.2"]


def test_list_order_and_lookup_by_code(g, project, original):
    g.items.create(project.id, make("Dois"))
    g.items.create_addendum(original.item_id, adj())
    assert [v.code for v in g.items.list_items(project.id)] == ["MERC-1", "MERC-1.1", "MERC-2"]
    assert g.items.get_by_code("merc-1.1").data.title == "Ajuste do frete"
    assert g.items.get_by_code("MERC-1").data.title == "Cálculo de frete"
    assert g.items.get_by_code("MERC-1.9") is None
    assert g.items.get_by_code("MERC-1.x") is None


def test_addendum_is_managed_independently(g, project, original):
    a = g.items.create_addendum(original.item_id, adj())
    g.items.set_status(original.item_id, ItemStatus.DONE)
    g.items.archive(original.item_id)
    current = {v.code: v for v in g.items.list_items(project.id, archived=False)}
    assert list(current) == ["MERC-1.1"]
    assert current["MERC-1.1"].data.status == ItemStatus.OPEN
    # Editing the addendum never touches the original, and no-op saves create no revision.
    rev = g.items.latest_revision(project.id)
    assert g.items.update(a.item_id, a.data) is None
    assert g.items.latest_revision(project.id) == rev
    new = g.items.set_status(a.item_id, ItemStatus.IN_PROGRESS)
    assert g.items.revision(project.id, new.rev_start).summary == (
        "MERC-1.1: status Aberto → Em desenvolvimento"
    )
    # An archived original can still receive addenda.
    assert g.items.create_addendum(original.item_id, adj("mais um")).code == "MERC-1.2"


def test_old_revisions_do_not_show_later_addenda(g, project, original):
    rev = g.items.latest_revision(project.id)
    a = g.items.create_addendum(original.item_id, adj())
    assert [v.code for v in g.items.list_items(project.id, rev)] == ["MERC-1"]
    assert g.items.addenda(original, rev) == []
    diff = g.items.diff(project.id, rev, a.rev_start)
    assert [v.code for v in diff.added] == ["MERC-1.1"]


def test_original_with_addenda_cannot_be_deleted(g, project, original):
    a = g.items.create_addendum(original.item_id, adj())
    with pytest.raises(ValidationError, match="MERC-1 tem adendos \\(MERC-1.1\\)"):
        g.items.delete(original.item_id)
    g.items.archive(a.item_id)  # archived addenda still count
    with pytest.raises(ValidationError, match="tem adendos"):
        g.items.delete(original.item_id)
    g.items.delete(a.item_id)
    g.items.delete(original.item_id)
    assert g.items.list_items(project.id) == []
    # The number of a deleted addendum is never reused.
    with pytest.raises(ValidationError):
        g.items.create_addendum(original.item_id, adj())


def test_addendum_validation(g, project, original):
    with pytest.raises(ValidationError, match="título"):
        g.items.create_addendum(original.item_id, adj("  "))
    with pytest.raises(ValidationError, match="não encontrado"):
        g.items.create_addendum(9999, adj())
    g.projects.set_active(project.id, False)
    with pytest.raises(ValidationError, match="Projeto arquivado"):
        g.items.create_addendum(original.item_id, adj())


def test_search_finds_addendum_by_code(g, project, original):
    g.items.create_addendum(original.item_id, adj(body="cubagem dos tubos"))
    hits = [h for h in g.search.search("cubagem", project.id) if h.kind == "item"]
    assert [h.label for h in hits] == ["MERC-1.1"]


# ---------------------------------------------------------------- selection parsing


@pytest.mark.parametrize(
    "text, expected",
    [
        ("33.1", {(33, 1)}),
        ("1,3-4,33.1", {(1, 0), (3, 0), (4, 0), (33, 1)}),
        (" 2 ; 2.2 ", {(2, 0), (2, 2)}),
    ],
)
def test_parse_item_keys(text, expected):
    assert parse_item_keys(text) == expected


@pytest.mark.parametrize("text", ["", "1.0", "0.1", "1.a", "x", "1-"])
def test_parse_item_keys_invalid(text):
    with pytest.raises(ValidationError):
        parse_item_keys(text)


# ---------------------------------------------------------------- exports


def test_html_with_only_the_addendum_references_the_original(g, project, original):
    a = g.items.create_addendum(original.item_id, adj(body="Converter **cm** em m."))
    html = g.exports.export_html("MERC", numbers={(1, 1)}).content
    assert 'id="merc-1.1"' in html
    assert 'id="merc-1"' not in html  # the original is not an item of this document
    assert "Adendo" in html and "Cálculo de frete" in html
    assert "<strong>original</strong>" in html  # original text, in a <details>
    assert "Ver o item original (MERC-1)" in html
    assert "Converter <strong>cm</strong> em m." in html
    # The selection tells about the addendum only.
    assert "1 item (seleção de 2)" in html
    assert a.code in html


def test_html_with_both_links_original_and_addendum(g, project, original):
    g.items.create_addendum(original.item_id, adj())
    html = g.exports.export_html("MERC").content
    assert 'id="merc-1"' in html and 'id="merc-1.1"' in html
    assert '<a href="#merc-1">MERC-1</a>' in html  # addendum → original
    assert '<a href="#merc-1.1">MERC-1.1</a>' in html  # original → its addenda
    assert "Ver o item original" not in html  # already in the document


def test_html_archived_original_is_referenced_without_its_text(g, project, original):
    g.items.create_addendum(original.item_id, adj())
    g.items.archive(original.item_id)
    html = g.exports.export_html("MERC").content
    assert 'id="merc-1"' not in html
    assert "arquivado" in html and "Cálculo de frete" in html
    assert "<strong>original</strong>" not in html
    with pytest.raises(ValidationError, match="arquivados"):
        g.exports.export_html("MERC", numbers={1})


def test_claude_export_marks_addenda(g, project, original):
    g.items.create_addendum(original.item_id, adj())
    md, _ = g.exports.claude_markdown(project)
    assert "### MERC-1.1 — Ajuste do frete" in md
    assert "Adendo de MERC-1 — Cálculo de frete (Aberto)" in md


# ---------------------------------------------------------------- migration


def test_v2_database_is_rebuilt_with_addenda(tmp_path):
    path = tmp_path / "v2.db"
    conn = sqlite3.connect(path)
    conn.executescript(V1 + V2 + """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version VALUES (1), (2);
        INSERT INTO projects (id, code, name, created_at) VALUES (1, 'MERC', 'M', '2026-01-01');
        INSERT INTO notes (id, note_date, name, created_at, updated_at)
            VALUES (1, '2026-01-01', 'n', '2026-01-01', '2026-01-01');
        INSERT INTO items (id, project_id, number, source_note_id, created_at)
            VALUES (7, 1, 3, 1, '2026-01-01');
        INSERT INTO revisions (project_id, number, created_at, summary, item_id)
            VALUES (1, 1, '2026-01-01', 'x', 7);
        INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
            VALUES (7, 1, 'Antigo', 'improvement', 'low', 1, 'done');
    """)
    conn.close()
    db = Database(path)
    try:
        row = db.query_one("SELECT id, number, addendum, source_note_id FROM items")
        assert tuple(row) == (7, 3, 0, 1)
        assert db.query_one("PRAGMA foreign_keys")[0] == 1
        assert db.query("PRAGMA foreign_key_check") == []
        # Foreign keys still point at the rebuilt table.
        with pytest.raises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty,"
                " status) VALUES (99, 1, 't', 'improvement', 'low', 1, 'open')"
            )
        with pytest.raises(sqlite3.IntegrityError):  # (project, number, addendum) is unique
            db.execute("INSERT INTO items (project_id, number, addendum, created_at)"
                       " VALUES (1, 3, 0, 'x')")
        db.execute("INSERT INTO items (project_id, number, addendum, created_at)"
                   " VALUES (1, 3, 1, 'x')")
    finally:
        db.close()


# ---------------------------------------------------------------- CLI


def test_cli_adendo_inherits_from_original(g, project, original):
    res = cli(g, "item", "adendo", "MERC-1", "Ajuste pós-teste", "-c", "-", input="Corrigir **x**.\n")
    assert res.exit_code == 0, res.output
    assert "MERC-1.1 criado" in res.output and "adendo de MERC-1" in res.output
    a = g.items.get_by_code("MERC-1.1")
    assert a.data.type == ItemType.ADJUSTMENT
    assert a.data.priority == Priority.HIGH and a.data.difficulty == 4
    assert a.data.tags == ("frete",)
    assert a.data.body == "Corrigir **x**."

    res = cli(g, "item", "ver", "MERC-1.1")
    assert "Adendo** de **MERC-1** — Cálculo de frete" in res.output
    res = cli(g, "item", "ver", "MERC-1")
    assert "Adendos: **MERC-1.1** Ajuste pós-teste (Aberto)" in res.output
    res = cli(g, "itens", "-p", "MERC")
    assert "MERC-1.1 · Ajuste pós-teste" in res.output and "adendo de MERC-1" in res.output

    res = cli(g, "item", "editar", "MERC-1.1", "--status", "concluido")
    assert res.exit_code == 0 and "MERC-1.1: status Aberto → Concluído" in res.output

    res = cli(g, "export", "html", "-p", "MERC", "--itens", "1.1")
    assert res.exit_code == 0, res.output
    res = cli(g, "item", "adendo", "MERC-9", "x")
    assert res.exit_code == 1 and "não encontrado" in res.output


# ---------------------------------------------------------------- TUI


def run(coro):
    return asyncio.run(coro)


def test_tui_new_addendum(g, project, original):
    g.items.set_status(original.item_id, ItemStatus.DONE)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            screen = app.screen
            assert isinstance(screen, MainScreen)
            # The done original is hidden by the default filter: show it to pick it.
            screen.filters = replace(screen.filters, hide_closed=False)
            screen.reload_items(select_item_id=original.item_id)
            await pilot.pause()
            await pilot.press("d")
            await pilot.pause()
            editor = app.screen
            assert isinstance(editor, ItemEditorScreen)
            assert editor.query_one("#title", Input).value == ""
            await pilot.press("ctrl+o")  # the original, read-only
            await pilot.pause()
            assert editor.query_one("#body-switcher").current == "body-preview"
            await pilot.press("ctrl+t")
            await pilot.pause()
            editor.query_one("#title", Input).value = "Ajuste vindo do teste"
            await pilot.press("ctrl+s")
            await pilot.pause()
            assert isinstance(app.screen, MainScreen)
            a = g.items.get_by_code("MERC-1.1")
            assert a is not None and a.data.type == ItemType.ADJUSTMENT
            assert a.data.priority == Priority.HIGH and a.data.tags == ("frete",)
            assert screen.selected_item().code == "MERC-1.1"
            # Back to the default filter: the addendum stays, the done original is hidden.
            screen.filters = replace(screen.filters, hide_closed=True)
            screen.reload_items()
            table = screen.query_one("#items", DataTable)
            assert [str(table.get_row_at(i)[0]) for i in range(table.row_count)] == ["MERC-1.1"]
            preview = screen.query_one("#preview", Markdown)
            assert "Adendo" in preview.source and "Cálculo de frete" in preview.source

    run(scenario())


def test_tui_delete_original_with_addenda_shows_error(g, project, original):
    g.items.create_addendum(original.item_id, adj())

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            screen = app.screen
            screen.reload_items(select_item_id=original.item_id)
            await pilot.pause()
            await pilot.press("delete")
            await pilot.pause()
            await pilot.press("y")
            await pilot.pause()
            assert g.items.get(original.item_id) is not None

    run(scenario())
