"""Creation date and control date of items (the control date is not versioned)."""

from __future__ import annotations

import sqlite3

import pytest
from typer.testing import CliRunner

from giano.cli import app
from giano.db import Database
from giano.db.migrations import V1, V2, V3, V4
from giano.domain import ItemStatus, ValidationError
from tests.conftest import make


def cli(g, *args):
    return CliRunner().invoke(app, ["--db", g.db_path, *args])


@pytest.fixture
def three(g, project):
    a = g.items.create(project.id, make("A"))  # rev 1
    b = g.items.create(project.id, make("B"))  # rev 2
    c = g.items.create(project.id, make("C", status=ItemStatus.DONE))  # rev 3
    return a, b, c


def _control(g, item_id, rev=None):
    return g.items.get(item_id, rev).control_date


def test_created_at_is_the_item_creation(g, project, clock):
    a = g.items.create(project.id, make("A"))
    g.items.set_status(a.item_id, ItemStatus.APPROVED)
    addendum = g.items.create_addendum(a.item_id, make("A.1"))
    current = g.items.get(a.item_id)
    assert current.created_at == a.created_at
    assert a.created_at.startswith("2026-03-10T09:")
    assert addendum.created_at > a.created_at
    assert current.control_date is None


def test_set_control_date_is_not_a_revision(g, project, three):
    a, b, _ = three
    g.items.set_control_date([a.item_id, b.item_id], "2026-04-01")
    assert g.items.latest_revision(project.id) == 3
    # The same at every revision (not versioned).
    assert _control(g, a.item_id) == _control(g, a.item_id, 1) == "2026-04-01"
    g.items.set_control_date([a.item_id], None)
    assert _control(g, a.item_id) is None and _control(g, b.item_id) == "2026-04-01"
    # Changing the content keeps the control date.
    g.items.set_status(b.item_id, ItemStatus.APPROVED)
    assert _control(g, b.item_id) == "2026-04-01"


def test_set_control_date_validation(g, project, three):
    a = three[0]
    with pytest.raises(ValidationError, match="inválida"):
        g.items.set_control_date([a.item_id], "31/02/2026")
    g.projects.set_active(project.id, False)
    with pytest.raises(ValidationError, match="somente leitura"):
        g.items.set_control_date([a.item_id], "2026-04-01")


def test_html_export_sets_today_on_the_exported_items(g, project, three):
    a, b, c = three
    archived = g.items.create(project.id, make("D"))
    g.items.archive(archived.item_id)
    rev = g.items.latest_revision(project.id)
    r = g.exports.export_html("MERC", numbers={1, 3}, control_date=True)
    assert r.control_dated == 2
    assert "data de controle 10/03/2026 em 2 itens" in r.summary
    assert _control(g, a.item_id) == _control(g, c.item_id) == "2026-03-10"
    assert _control(g, b.item_id) is None and _control(g, archived.item_id) is None
    assert g.items.latest_revision(project.id) == rev  # no revision


def test_html_export_control_date_off_by_default_and_old_revisions(g, project, three):
    a, b, c = three
    r = g.exports.export_html("MERC")
    assert r.control_dated == 0 and "data de controle" not in r.summary
    assert _control(g, a.item_id) is None
    # An old revision may be exported (and dated): only its items get the date.
    g.exports.export_html("MERC", rev=2, control_date=True)
    assert [_control(g, v.item_id) for v in three] == ["2026-03-10", "2026-03-10", None]


def test_html_export_control_date_with_mark_in_progress(g, project, three):
    a, b, c = three
    r = g.exports.export_html("MERC", mark_in_progress=True, control_date=True)
    assert r.marked == ("MERC-1", "MERC-2") and r.control_dated == 3
    assert g.items.latest_revision(project.id) == 4  # one revision, for the status changes
    assert g.items.get(a.item_id).control_date == "2026-03-10"


def test_html_export_control_date_rolls_back_with_the_file(g, project, three, tmp_path):
    blocker = tmp_path / "blocked"
    blocker.write_text("not a folder")
    with pytest.raises(OSError):
        g.exports.export_html("MERC", out_dir=blocker, control_date=True)
    assert _control(g, three[0].item_id) is None


def test_html_export_skips_control_date_for_archived_project(g, project, three):
    g.projects.set_active(project.id, False)
    r = g.exports.export_html("MERC", control_date=True)
    assert r.control_dated == 0 and _control(g, three[0].item_id) is None


def test_cli_export_html_sets_control_date_by_default(g, project, three):
    res = cli(g, "export", "html", "-p", "MERC", "--sem-data-controle")
    assert res.exit_code == 0, res.output
    assert _control(g, three[0].item_id) is None
    res = cli(g, "export", "html", "-p", "MERC", "-i", "2")
    assert res.exit_code == 0, res.output
    # Rich wraps at 80 columns and the temp path length varies (longer on CI).
    assert "data de controle" in " ".join(res.output.split())
    assert _control(g, three[1].item_id) is not None and _control(g, three[0].item_id) is None


def test_cli_item_controle_and_ver(g, project, three):
    res = cli(g, "item", "controle", "MERC-1", "05/04/2026")
    assert res.exit_code == 0, res.output
    assert "MERC-1: data de controle 05/04/2026." in res.output
    res = cli(g, "item", "ver", "MERC-1")
    assert "Criado em 10/03/2026" in res.output and "Data de controle: 05/04/2026" in res.output
    res = cli(g, "item", "controle", "MERC-1", "--limpar")
    assert res.exit_code == 0 and _control(g, three[0].item_id) is None
    res = cli(g, "item", "controle", "MERC-1", "ontem")
    assert res.exit_code == 1 and "inválida" in res.output


def test_v4_database_gets_empty_control_dates(tmp_path):
    path = tmp_path / "v4.db"
    conn = sqlite3.connect(path)
    conn.executescript(V1 + V2 + V3 + V4 + """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version VALUES (1), (2), (3), (4);
        INSERT INTO projects (id, code, name, created_at) VALUES (1, 'MERC', 'M', '2026-01-01');
        INSERT INTO items (id, project_id, number, created_at) VALUES (7, 1, 1, '2026-01-02T10:00:00');
        INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
            VALUES (7, 1, 'A', 'improvement', 'low', 1, 'open');
    """)
    conn.close()
    db = Database(path)
    try:
        from giano.db import items as repo

        v = repo.get_current(db, 7)
        assert (v.created_at, v.control_date) == ("2026-01-02T10:00:00", None)
    finally:
        db.close()
