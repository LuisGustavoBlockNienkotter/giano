"""Turso support: the libsql adapter, configuration and copying a local database."""

from __future__ import annotations

import json
import sqlite3

import pytest
from typer.testing import CliRunner

from giano import config
from giano.cli import app
from giano.config import DbTarget, resolve_db_target
from giano.db.remote import Row, to_positional
from giano.domain import ValidationError
from giano.services import Giano

from .conftest import make

# ---------------------------------------------------------------------------
# Adapter
# ---------------------------------------------------------------------------


def test_named_parameters_become_positional():
    sql, values = to_positional(
        "SELECT * FROM t WHERE a = :x AND b = :y AND c = :x AND d = ':x' AND e = \"x:y\"",
        {"x": 1, "y": 2, "unused": 3},
    )
    assert sql == "SELECT * FROM t WHERE a = ?1 AND b = ?2 AND c = ?1 AND d = ':x' AND e = \"x:y\""
    assert values == (1, 2)


def test_positional_parameters_pass_through():
    assert to_positional("SELECT ?", [5]) == ("SELECT ?", (5,))
    assert to_positional("SELECT 1", None) == ("SELECT 1", ())


def test_missing_named_parameter():
    with pytest.raises(sqlite3.ProgrammingError):
        to_positional("SELECT :a", {"b": 1})


def test_row_by_name_and_position():
    r = Row((1, "x"), {"id": 0, "name": 1})
    assert r["name"] == "x" and r[0] == 1 and tuple(r) == (1, "x") and r.keys() == ["id", "name"]


def test_backend_is_really_switched(g, backend):
    assert g.remote is (backend == "libsql")
    assert type(g.db.conn).__name__ == ("RemoteConnection" if backend == "libsql" else "Connection")


def test_constraint_errors_are_integrity_errors(g, project):
    with pytest.raises(sqlite3.IntegrityError):
        g.db.execute(
            "INSERT INTO projects (code, name, created_at) VALUES ('MERC', 'x', '2026-01-01')"
        )


def test_failed_transaction_rolls_back(g, project):
    with pytest.raises(RuntimeError):
        with g.db.transaction():
            g.items.create(project.id, make("Some"))
            raise RuntimeError("boom")
    assert g.items.latest_revision(project.id) == 0
    g.items.create(project.id, make("Fica"))  # the connection is usable afterwards
    assert [v.data.title for v in g.items.list_items(project.id, 1)] == ["Fica"]


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------


def test_default_is_the_local_file():
    target = resolve_db_target()
    assert not target.remote and target.location == str(config.default_db_path())


def test_turso_from_config_file():
    config.save_config(turso_url="giano-org.turso.io", turso_token="tok")
    assert resolve_db_target() == DbTarget("libsql://giano-org.turso.io", "tok")
    assert resolve_db_target().remote


def test_environment_overrides_config(monkeypatch, tmp_path):
    config.save_config(turso_url="libsql://a.turso.io", turso_token="a")
    monkeypatch.setenv("GIANO_TURSO_URL", "libsql://b.turso.io")
    monkeypatch.setenv("GIANO_TURSO_TOKEN", "b")
    assert resolve_db_target() == DbTarget("libsql://b.turso.io", "b")
    monkeypatch.setenv("GIANO_DB", str(tmp_path / "x.db"))  # an explicit local file wins
    assert resolve_db_target() == DbTarget(str(tmp_path / "x.db"))
    assert resolve_db_target(tmp_path / "y.db") == DbTarget(str(tmp_path / "y.db"))


def test_save_config_updates_and_removes_keys():
    config.save_config(turso_url="libsql://a", turso_token="t", export_dir="/x")
    config.save_config(turso_url=None, turso_token=None)
    assert json.loads(config.config_path().read_text(encoding="utf-8")) == {"export_dir": "/x"}


def test_export_dir_belongs_to_the_computer_with_turso(g, backend, tmp_path):
    g.settings.export_dir = str(tmp_path / "aqui")
    in_db = g.db.query_one("SELECT value FROM settings WHERE key = 'export_dir'")
    if backend == "libsql":
        assert in_db is None
        assert config.load_config()["export_dir"] == str(tmp_path / "aqui")
    else:
        assert in_db[0] == str(tmp_path / "aqui")
    assert g.settings.export_dir == tmp_path / "aqui"


def test_cli_turso_status_and_disconnect():
    runner = CliRunner()
    res = runner.invoke(app, ["turso", "status"])
    assert res.exit_code == 0 and "banco local" in res.output
    config.save_config(turso_url="libsql://a.turso.io", turso_token="t", export_dir="/x")
    res = runner.invoke(app, ["turso", "desconectar"])
    assert res.exit_code == 0
    assert config.load_config() == {"export_dir": "/x"}


def test_cli_reports_unreachable_turso():
    config.save_config(turso_url="http://127.0.0.1:9", turso_token="t")
    res = CliRunner().invoke(app, ["projetos"])
    assert res.exit_code == 1
    assert "Não foi possível abrir http://127.0.0.1:9" in res.output


# ---------------------------------------------------------------------------
# Copying the local database to Turso
# ---------------------------------------------------------------------------


@pytest.fixture
def source(tmp_path, clock):
    src = Giano.open(tmp_path / "local.db", clock=clock)
    p = src.projects.create("MERC", "Mercurius", "Contexto")
    one = src.items.create(p.id, make("Relatório de produção", tags=("ui", "pdf"), body="Gerar **PDF**"))
    src.items.update(one.item_id, make("Relatório de produção", tags=("pdf",), body="Gerar PDF e XLS"))
    src.items.create_addendum(one.item_id, make("Ajuste do relatório"))
    src.notes.create("Reunião", p.id, "Falamos de exportação")
    src.settings.claude_prompt = "Analise {sigla}"
    src.settings.export_dir = str(tmp_path / "exports-local")
    yield src
    src.close()


def test_copy_to_an_empty_database(source, g, backend):
    counts = source.copy_to(g)
    assert counts["projects"] == 1 and counts["revisions"] == 3 and counts["notes"] == 1
    p = g.projects.require("MERC")
    assert g.items.latest_revision(p.id) == 3
    assert [v.code for v in g.items.list_items(p.id, 3)] == ["MERC-1", "MERC-1.1"]
    assert g.items.list_items(p.id, 1)[0].data.tags == ("pdf", "ui")  # history kept
    assert g.settings.claude_prompt == "Analise {sigla}"
    assert [(h.kind, h.label) for h in g.search.search("xls")] == [("item", "MERC-1")]
    # note_fts is filled by the triggers
    assert [h.kind for h in g.search.search("exportacao")] == ["note"]
    # The export folder is per computer: not carried into a shared (Turso) database.
    assert (g.settings.export_dir == source.settings.export_dir) is (backend == "sqlite")
    # The copy keeps working: new items and revisions continue the sequences.
    created = g.items.create(p.id, make("Novo"))
    assert created.code == "MERC-2" and g.items.latest_revision(p.id) == 4


def test_copy_refuses_a_database_with_data(source, g, project):
    with pytest.raises(ValidationError, match="já tem dados"):
        source.copy_to(g)


def test_network_errors_are_user_facing():
    from giano.db.remote import DatabaseUnavailable, _translate

    offline = _translate(ValueError("Hrana: `http error: `error trying to connect: tcp connect error``"))
    assert isinstance(offline, DatabaseUnavailable) and isinstance(offline, ValidationError)
    assert "Sem conexão com o Turso" in str(offline)
    token = _translate(ValueError('Hrana: `api error: `status=400 Bad Request, body={"error":"JWT error: InvalidToken"}``'))
    assert "token" in str(token)
    sql = _translate(ValueError('Hrana: `stream error: `Error { message: "SQLite error: no such table: x" }``'))
    assert type(sql) is sqlite3.OperationalError
    dup = _translate(ValueError("SQLite error: UNIQUE constraint failed: projects.code"))
    assert type(dup) is sqlite3.IntegrityError


def test_tui_survives_a_dropped_connection(g, backend, project, monkeypatch):
    """Without internet, saving or F5 warns; the app and what was typed stay."""
    import asyncio

    from textual.widgets import Input

    from giano.tui.app import GianoApp
    from giano.tui.item_editor import ItemEditorScreen

    if backend != "libsql":
        pytest.skip("network failures only exist with the Turso driver")

    raw = g.db.conn._raw

    class Offline:
        def __getattr__(self, name):
            return getattr(raw, name)

        def execute(self, *_a):
            raise ValueError("Hrana: `http error: `error trying to connect: dns error``")

    async def scenario():
        app = GianoApp(g)
        async with app.run_test(size=(160, 48)) as pilot:
            await pilot.pause()
            await pilot.press("n")
            await pilot.pause()
            editor = app.screen
            assert isinstance(editor, ItemEditorScreen)
            editor.query_one("#title", Input).value = "Texto que não pode se perder"
            monkeypatch.setattr(g.db.conn, "_raw", Offline())
            await pilot.press("ctrl+s")
            await pilot.pause()
            await pilot.press("f5")
            await pilot.pause()
            assert app.is_running and app.screen is editor
            assert editor.query_one("#title", Input).value == "Texto que não pode se perder"
            assert sum("Sem conexão com o Turso" in n.message for n in app._notifications) >= 2
            monkeypatch.setattr(g.db.conn, "_raw", raw)
            await pilot.press("ctrl+s")  # back online: saving works
            await pilot.pause()
            assert [v.data.title for v in g.items.list_items(project.id)] == [
                "Texto que não pode se perder"
            ]

    asyncio.run(scenario())


def test_copy_keeps_repository_link_and_checks(source, g):
    """V6 (desktop app): the remote URL of a project and its checks go to the copy too."""
    p = source.projects.require("MERC")
    with source.db.transaction():
        source.db.execute(
            "UPDATE projects SET repo_remote = 'https://github.com/csg/merc.git' WHERE id = ?", (p.id,)
        )
        source.db.execute(
            "INSERT INTO repo_checks (project_id, from_commit, to_commit, branch, commit_count,"
            " summary, result, created_at) VALUES (?, NULL, 'abc', 'main', 1, 'Resumo', '{}', 'x')",
            (p.id,),
        )
    counts = source.copy_to(g)
    assert counts["repo_checks"] == 1
    row = g.db.query_one("SELECT repo_remote FROM projects WHERE code = 'MERC'")
    assert row["repo_remote"] == "https://github.com/csg/merc.git"
