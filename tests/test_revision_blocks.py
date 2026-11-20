"""A revision with several changes: open revisions, batches, net change per item."""

from __future__ import annotations

import sqlite3

import pytest
from typer.testing import CliRunner

from giano.cli import app
from giano.db import Database
from giano.db.migrations import V1, V2, V3
from giano.domain import ItemStatus, Priority, ValidationError
from tests.conftest import make


def cli(g, *args):
    return CliRunner().invoke(app, ["--db", g.db_path, *args])


def _versions(g, item_id):
    return [(v.rev_start, v.rev_end, v.data.title) for v in g.items.history(item_id)]


@pytest.fixture
def two(g, project):
    a = g.items.create(project.id, make("A"))  # rev 1
    b = g.items.create(project.id, make("B"))  # rev 2
    return a, b


# ---------------------------------------------------------------- open revision


def test_open_revision_collects_every_change_into_one(g, project, two):
    a, b = two
    opened = g.items.open_revision(project.id, "Reunião com o cliente")
    assert (opened.number, opened.is_open, opened.changes) == (3, True, ())
    c = g.items.create(project.id, make("C"))
    g.items.set_status(a.item_id, ItemStatus.APPROVED)
    g.items.set_priority(a.item_id, Priority.HIGH)  # same item again: one net change
    g.items.update(c.item_id, make("C2"))  # created in this revision: still "criado"
    g.items.delete(b.item_id)
    assert g.items.latest_revision(project.id) == 3
    rev = g.items.get_open_revision(project.id)
    assert [ch.summary for ch in rev.changes] == [
        "MERC-3 criado: C2",
        "MERC-1: prioridade Média → Alta; status Aberto → Aprovado",
        "MERC-2 excluído: B",
    ]
    # One version per item per revision; the list before the block is untouched.
    assert _versions(g, a.item_id) == [(1, 3, "A"), (3, None, "A")]
    assert _versions(g, c.item_id) == [(3, None, "C2")]
    assert [v.code for v in g.items.list_items(project.id, 2)] == ["MERC-1", "MERC-2"]
    assert [v.code for v in g.items.list_items(project.id)] == ["MERC-1", "MERC-3"]
    d = g.items.diff(project.id, 2, 3)
    assert ([v.code for v in d.added], [v.code for v in d.removed], [c.code for c in d.changed]) == (
        ["MERC-3"], ["MERC-2"], ["MERC-1"])

    closed = g.items.close_revision(project.id)
    assert (closed.number, closed.is_open, closed.summary) == (3, False, "Reunião com o cliente")
    assert closed.created_at > opened.created_at  # dated when closed
    assert g.items.get_open_revision(project.id) is None
    # After closing, a change is a revision of its own again.
    v = g.items.set_status(a.item_id, ItemStatus.DONE)
    assert v.rev_start == 4
    assert _versions(g, a.item_id) == [(1, 3, "A"), (3, 4, "A"), (4, None, "A")]


def test_going_back_leaves_the_revision(g, project, two):
    a, _ = two
    g.items.open_revision(project.id)
    g.items.set_status(a.item_id, ItemStatus.APPROVED)
    back = g.items.set_status(a.item_id, ItemStatus.OPEN)
    assert back.rev_start == 1 and back.rev_end is None
    assert _versions(g, a.item_id) == [(1, None, "A")]
    assert g.items.get_open_revision(project.id).changes == ()
    # Archive and restore inside the block: no net change either.
    g.items.archive(a.item_id)
    g.items.unarchive(a.item_id)
    assert g.items.get_open_revision(project.id).changes == ()
    # Closing an empty revision discards it.
    assert g.items.close_revision(project.id) is None
    assert g.items.latest_revision(project.id) == 2
    assert g.items.create(project.id, make("C")).rev_start == 3


def test_create_and_delete_in_the_same_revision(g, project, two):
    g.items.open_revision(project.id)
    c = g.items.create(project.id, make("C"))
    assert g.items.delete(c.item_id) == 3
    assert g.items.get(c.item_id) is None
    assert g.items.history(c.item_id) == []
    assert g.items.get_open_revision(project.id).changes == ()
    assert g.items.create(project.id, make("D")).code == "MERC-4"  # numbers never reused


def test_edit_then_delete_reports_the_old_title(g, project, two):
    a, _ = two
    g.items.open_revision(project.id)
    g.items.update(a.item_id, make("A editado"))
    g.items.delete(a.item_id)
    rev = g.items.close_revision(project.id, "Limpeza")
    assert [c.summary for c in rev.changes] == ["MERC-1 excluído: A"]
    assert rev.summary == "Limpeza"
    assert _versions(g, a.item_id) == [(1, 3, "A")]


def test_addendum_and_summary_defaults(g, project, two):
    a, _ = two
    g.items.open_revision(project.id)
    g.items.create_addendum(a.item_id, make("Ajuste"))
    rev = g.items.close_revision(project.id)
    assert rev.summary == "MERC-1.1 criado (adendo de MERC-1): Ajuste"  # only change
    g.items.open_revision(project.id, "   ")
    g.items.set_status(a.item_id, ItemStatus.DONE)
    g.items.set_status(two[1].item_id, ItemStatus.DONE)
    assert g.items.get_open_revision(project.id).title == "Em aberto (2 alterações)"
    assert g.items.close_revision(project.id).summary == "2 itens alterados"


def test_open_revision_rules(g, project, two):
    with pytest.raises(ValidationError, match="Não há revisão aberta"):
        g.items.close_revision(project.id)
    g.items.open_revision(project.id)
    with pytest.raises(ValidationError, match="Rev. 3 já está aberta"):
        g.items.open_revision(project.id)
    other = g.projects.create("RH", "RH")
    assert g.items.open_revision(other.id).number == 1  # one open revision per project
    g.items.close_revision(other.id)
    g.projects.set_active(other.id, False)
    with pytest.raises(ValidationError, match="Projeto arquivado"):
        g.items.open_revision(other.id)


def test_batch_goes_into_the_open_revision(g, project, two):
    a, b = two
    g.items.open_revision(project.id)
    g.items.update(a.item_id, make("A2"))
    g.items.set_status_many([a.item_id, b.item_id], ItemStatus.APPROVED)
    rev = g.items.get_open_revision(project.id)
    assert rev.number == 3 and rev.summary == ""  # the batch does not close it
    assert [c.summary for c in rev.changes] == [
        'MERC-1: título "A" → "A2"; status Aberto → Aprovado',
        "MERC-2: status Aberto → Aprovado",
    ]


def test_batch_is_all_or_nothing(g, project, two):
    a, b = two
    with pytest.raises(ValidationError):
        with g.items.batch("x"):
            g.items.update(a.item_id, make("A2"))
            g.items.delete(999)
    assert g.items.latest_revision(project.id) == 2
    assert _versions(g, a.item_id) == [(1, None, "A")]
    with g.items.batch("Dois itens"):
        g.items.update(a.item_id, make("A2"))
        g.items.update(b.item_id, make("B2"))
        g.items.update(a.item_id, make("A"))  # back: leaves the batch
    rev = g.items.revision(project.id, 3)
    assert rev.summary == "MERC-2: título \"B\" → \"B2\"" and len(rev.changes) == 1
    with g.items.batch("nada"):
        g.items.update(a.item_id, make("A3"))
        g.items.update(a.item_id, make("A"))
    assert g.items.latest_revision(project.id) == 3  # empty batch: no revision


# ---------------------------------------------------------------- exports


def test_html_export_lists_the_changes_of_a_block(g, project, two):
    a, b = two
    g.items.open_revision(project.id, "Reunião")
    g.items.update(a.item_id, make("A2"))
    g.items.update(b.item_id, make("B2"))
    g.items.close_revision(project.id)
    html, rev, _ = g.exports.html(project, None, True, None, None)
    assert rev == 3
    assert "<td>Reunião<ul><li>MERC-1: título" in html and "<li>MERC-2: título" in html
    # A selection says nothing about the items left out.
    html, _, _ = g.exports.html(project, None, True, None, {1})
    assert "Reunião" not in html and "MERC-2" not in html
    assert "<td>MERC-1: título" in html and "<ul><li>MERC-1" not in html


# ---------------------------------------------------------------- CLI


def test_cli_open_and_close_revision(g, project, two):
    res = cli(g, "revisao", "abrir", "-p", "merc", "--resumo", "Reunião")
    assert res.exit_code == 0, res.output
    assert "Rev. 3 aberta." in res.output
    res = cli(g, "item", "editar", "MERC-1", "-s", "aprovado")
    assert "Rev. 3 (aberta) — MERC-1: status Aberto → Aprovado" in res.output
    res = cli(g, "item", "novo", "C", "-p", "MERC")
    assert "MERC-3 criado (Rev. 3): C" in res.output
    res = cli(g, "item", "editar", "MERC-1", "-s", "aberto")
    assert "MERC-1: voltou ao que era; fora da Rev. 3 (aberta)." in res.output
    res = cli(g, "revisao", "ver", "-p", "MERC")
    assert "Rev. 3 (aberta)" in res.output and "• MERC-3 criado: C" in res.output
    res = cli(g, "revisao", "fechar", "-p", "MERC")
    assert res.exit_code == 0, res.output
    assert "Rev. 3 fechada (1 item alterado): Reunião" in res.output
    res = cli(g, "revisao", "fechar", "-p", "MERC")
    assert res.exit_code == 1 and "Não há revisão aberta" in res.output
    cli(g, "revisao", "abrir", "-p", "MERC")
    res = cli(g, "revisao", "fechar", "-p", "MERC")
    assert "Rev. 4 descartada: nenhuma alteração." in res.output


# ---------------------------------------------------------------- migration


def test_v3_database_gets_one_change_line_per_revision(tmp_path):
    path = tmp_path / "v3.db"
    conn = sqlite3.connect(path)
    conn.executescript(V1 + V2 + V3 + """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version VALUES (1), (2), (3);
        INSERT INTO projects (id, code, name, created_at) VALUES (1, 'MERC', 'M', '2026-01-01');
        INSERT INTO items (id, project_id, number, created_at) VALUES (7, 1, 1, '2026-01-01');
        INSERT INTO revisions (project_id, number, created_at, summary, item_id)
            VALUES (1, 1, '2026-01-01', 'MERC-1 criado: A', 7);
        INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
            VALUES (7, 1, 'A', 'improvement', 'low', 1, 'open');
    """)
    conn.close()
    db = Database(path)
    try:
        from giano.db import items as repo

        rev = repo.get_revision(db, 1, 1)
        assert rev.is_open is False
        assert [(c.item_id, c.summary) for c in rev.changes] == [(7, "MERC-1 criado: A")]
    finally:
        db.close()
