"""Archiving items: versioned, read-only while archived, out of the list and the exports."""

from __future__ import annotations

import asyncio
import sqlite3
from dataclasses import replace

import pytest
from textual.widgets import DataTable

from giano.db import Database
from giano.db.migrations import V1
from giano.domain import ItemStatus, Priority, ValidationError
from giano.tui.app import GianoApp
from giano.tui.main_screen import MainScreen
from giano.tui.search import SearchScreen
from tests.conftest import make

SIZE = (160, 48)


@pytest.fixture
def three(g, project):
    a = g.items.create(project.id, make("Ativo", tags=("ux",), body="corpo A"))
    b = g.items.create(
        project.id,
        make("Feito há tempos", priority=Priority.HIGH, difficulty=4, tags=("velho", "erp"),
             status=ItemStatus.DONE, body="## Detalhe\n\ntexto antigo"),
    )
    c = g.items.create(project.id, make("Outro"))
    return a, b, c


def codes(views):
    return [v.code for v in views]


# ---------------------------------------------------------------- service


def test_archive_creates_revision_and_keeps_content(g, project, three):
    _, b, _ = three
    before = g.items.latest_revision(project.id)
    archived = g.items.archive(b.item_id)
    assert archived.rev_start == before + 1
    assert archived.data == replace(b.data, archived=True)  # everything else identical
    assert g.items.revisions(project.id)[0].summary == "MERC-2: arquivado"

    assert codes(g.items.list_items(project.id, archived=False)) == ["MERC-1", "MERC-3"]
    assert codes(g.items.list_items(project.id, archived=True)) == ["MERC-2"]
    assert len(g.items.list_items(project.id)) == 3
    # The previous revision still shows it in the normal list.
    assert "MERC-2" in codes(g.items.list_items(project.id, before, archived=False))


def test_unarchive_restores_to_the_list(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    restored = g.items.unarchive(b.item_id)
    assert restored.data == b.data
    assert g.items.revisions(project.id)[0].summary == "MERC-2: restaurado do arquivo"
    assert codes(g.items.list_items(project.id, archived=False)) == ["MERC-1", "MERC-2", "MERC-3"]
    assert g.items.list_items(project.id, archived=True) == []


def test_archive_twice_or_unarchive_active_is_refused(g, project, three):
    a, b, _ = three
    g.items.archive(b.item_id)
    rev = g.items.latest_revision(project.id)
    with pytest.raises(ValidationError, match="já está arquivado"):
        g.items.archive(b.item_id)
    with pytest.raises(ValidationError, match="não está arquivado"):
        g.items.unarchive(a.item_id)
    assert g.items.latest_revision(project.id) == rev


def test_archived_item_is_read_only(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    rev = g.items.latest_revision(project.id)
    with pytest.raises(ValidationError, match="arquivado"):
        g.items.update(b.item_id, replace(b.data, title="Mudou"))
    with pytest.raises(ValidationError, match="arquivado"):
        g.items.set_status(b.item_id, ItemStatus.OPEN)
    assert g.items.latest_revision(project.id) == rev


def test_edit_never_archives(g, project, three):
    a, _, _ = three
    assert g.items.update(a.item_id, replace(a.data, archived=True)) is None
    assert not g.items.get(a.item_id).data.archived


def test_archived_item_can_be_deleted(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    g.items.delete(b.item_id)
    assert g.items.list_items(project.id, archived=True) == []


def test_archive_refused_in_archived_project(g, project, three):
    a, _, _ = three
    g.projects.set_active(project.id, False)
    with pytest.raises(ValidationError, match="Projeto arquivado"):
        g.items.archive(a.item_id)


def test_diff_shows_archiving(g, project, three):
    _, b, _ = three
    base = g.items.latest_revision(project.id)
    g.items.archive(b.item_id)
    rev = g.items.latest_revision(project.id)

    d = g.items.diff(project.id, base, rev)
    assert [c.code for c in d.changed] == ["MERC-2"]
    assert [(f.field, f.old, f.new) for f in d.changed[0].changes] == [("archived", False, True)]

    d = g.items.diff(project.id, base, rev, include_archived=False)
    assert codes(d.removed) == ["MERC-2"] and not d.changed
    g.items.unarchive(b.item_id)
    d = g.items.diff(project.id, rev, rev + 1, include_archived=False)
    assert codes(d.added) == ["MERC-2"]


def test_search_marks_archived_hits(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    hits = g.search.search("antigo")
    assert [(h.label, h.status) for h in hits] == [("MERC-2", "Concluído (arquivado)")]


# ---------------------------------------------------------------- exports


def test_html_leaves_archived_items_out(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    html, _, _ = g.exports.html(project)  # changes section on: archiving is the last revision
    assert "Feito há tempos" not in html and "MERC-2" not in html
    assert "Ativo" in html and "Outro" in html
    with pytest.raises(ValidationError, match="arquivados.*MERC-2"):
        g.exports.html(project, numbers={1, 2})


def test_html_shows_restored_item_as_added(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    g.items.unarchive(b.item_id)
    html, _, _ = g.exports.html(project)
    assert "Feito há tempos" in html
    assert "Adicionados (1)" in html


def test_claude_export_leaves_archived_items_out(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)
    content, _ = g.exports.claude_markdown(project)
    assert "Feito há tempos" not in content
    assert "2 itens" in content


# ---------------------------------------------------------------- migration


def test_existing_database_gets_archived_column(tmp_path):
    path = tmp_path / "v1.db"
    conn = sqlite3.connect(path)
    conn.executescript(V1 + """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version VALUES (1);
        INSERT INTO projects (id, code, name, created_at) VALUES (1, 'MERC', 'M', '2026-01-01');
        INSERT INTO items (id, project_id, number, created_at) VALUES (1, 1, 1, '2026-01-01');
        INSERT INTO revisions (project_id, number, created_at, summary) VALUES (1, 1, '2026-01-01', 'x');
        INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
            VALUES (1, 1, 'Antigo', 'improvement', 'low', 1, 'done');
    """)
    conn.close()
    db = Database(path)
    try:
        assert db.query_one("SELECT archived FROM item_versions")[0] == 0
        with pytest.raises(sqlite3.IntegrityError):
            db.execute("UPDATE item_versions SET archived = 2")
    finally:
        db.close()


# ---------------------------------------------------------------- TUI


def test_tui_archive_and_restore(g, project, three):
    _, b, _ = three

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            assert isinstance(main, MainScreen)
            table = main.query_one("#items", DataTable)
            main.filters = replace(main.filters, hide_closed=False)
            main.reload_items(select_item_id=b.item_id)
            assert table.row_count == 3
            await pilot.press("z")  # archive the selected item
            await pilot.pause()
            assert g.items.get(b.item_id).data.archived
            assert table.row_count == 2

            await pilot.press("v")  # the archive (done items are not hidden there)
            await pilot.pause()
            assert main.show_archived and table.row_count == 1
            assert "ARQUIVO" in str(main.query_one("#topbar").render())
            assert "1 de 1 itens arquivados" in str(main.query_one("#filterbar").render())
            await pilot.press("e")  # read-only: no editor
            await pilot.press("enter")
            await pilot.pause()
            assert app.screen is main
            await pilot.press("z")  # restore
            await pilot.pause()
            assert not g.items.get(b.item_id).data.archived
            assert table.row_count == 0

            await pilot.press("v")
            await pilot.pause()
            assert not main.show_archived and table.row_count == 3

    asyncio.run(scenario())
    summaries = [r.summary for r in g.items.revisions(project.id)][:2]
    assert summaries == ["MERC-2: restaurado do arquivo", "MERC-2: arquivado"]


def test_tui_search_jumps_into_the_archive(g, project, three):
    _, b, _ = three
    g.items.archive(b.item_id)

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            main = app.screen
            await pilot.press("/")
            await pilot.pause()
            assert isinstance(app.screen, SearchScreen)
            await pilot.press(*"antigo")
            await pilot.press("enter", "enter")
            await pilot.pause()
            assert app.screen is main
            assert main.show_archived
            assert main.selected_item().item_id == b.item_id

    asyncio.run(scenario())
