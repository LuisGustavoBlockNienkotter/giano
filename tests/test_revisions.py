"""Revision rules: append-only versions, reconstruction, no-op edits, deletes."""

from __future__ import annotations

from dataclasses import replace

import pytest

from giano.db import Database
from giano.domain import ItemStatus, ItemType, Priority, ValidationError
from tests.conftest import make


def titles(views):
    return [v.data.title for v in views]


def test_new_project_starts_at_revision_zero(g, project):
    assert g.items.latest_revision(project.id) == 0
    assert g.items.list_items(project.id) == []
    assert g.items.revisions(project.id) == []


def test_create_assigns_sequential_ids_and_revisions(g, project):
    a = g.items.create(project.id, make("Primeiro"))
    b = g.items.create(project.id, make("Segundo"))
    assert (a.code, b.code) == ("MERC-1", "MERC-2")
    assert (a.rev_start, b.rev_start) == (1, 2)
    assert a.rev_end is None
    revs = g.items.revisions(project.id)
    assert [r.number for r in revs] == [2, 1]
    assert revs[1].summary == "MERC-1 criado: Primeiro"


def test_numbering_is_per_project(g, project):
    other = g.projects.create("P2K", "P2000")
    g.items.create(project.id, make("A"))
    g.items.create(project.id, make("B"))
    x = g.items.create(other.id, make("X"))
    assert x.code == "P2K-1"
    assert g.items.latest_revision(other.id) == 1
    assert g.items.latest_revision(project.id) == 2


def test_update_closes_version_and_inserts_new(g, project):
    a = g.items.create(project.id, make("Item", priority=Priority.HIGH))
    new = g.items.update(a.item_id, replace(a.data, priority=Priority.MEDIUM))
    assert new.rev_start == 2 and new.rev_end is None
    history = g.items.history(a.item_id)
    assert [(h.rev_start, h.rev_end) for h in history] == [(1, 2), (2, None)]
    assert g.items.revisions(project.id)[0].summary == "MERC-1: prioridade Alta → Média"


def test_save_without_change_creates_no_revision(g, project):
    a = g.items.create(project.id, make("Item", tags=("b", "a"), body="texto"))
    # Same content, different formatting: tag order/case, trailing whitespace, CRLF.
    same = replace(a.data, title="  Item ", tags=("A", "b"), body="texto  \r\n\n")
    assert g.items.update(a.item_id, same) is None
    assert g.items.latest_revision(project.id) == 1
    assert len(g.items.history(a.item_id)) == 1


def test_list_reconstruction_at_any_revision(g, project):
    a = g.items.create(project.id, make("A"))                      # rev 1
    b = g.items.create(project.id, make("B"))                      # rev 2
    g.items.update(a.item_id, replace(a.data, title="A2"))         # rev 3
    g.items.delete(b.item_id)                                      # rev 4
    c = g.items.create(project.id, make("C"))                      # rev 5
    g.items.set_status(c.item_id, ItemStatus.DONE)                 # rev 6

    expected = {
        0: [],
        1: ["A"],
        2: ["A", "B"],
        3: ["A2", "B"],
        4: ["A2"],
        5: ["A2", "C"],
        6: ["A2", "C"],
    }
    for rev, names in expected.items():
        assert titles(g.items.list_items(project.id, rev)) == names, rev
    assert g.items.list_items(project.id, 5)[1].data.status == ItemStatus.OPEN
    assert g.items.list_items(project.id, 6)[1].data.status == ItemStatus.DONE
    assert titles(g.items.list_items(project.id)) == ["A2", "C"]


def test_delete_only_closes_current_version(g, project):
    a = g.items.create(project.id, make("A"))
    rev = g.items.delete(a.item_id)
    assert rev == 2
    history = g.items.history(a.item_id)
    assert [(h.rev_start, h.rev_end) for h in history] == [(1, 2)]
    assert g.items.get(a.item_id) is None
    assert g.items.get(a.item_id, rev=1).data.title == "A"
    assert g.items.revisions(project.id)[0].summary == "MERC-1 excluído: A"
    with pytest.raises(ValidationError):
        g.items.delete(a.item_id)
    with pytest.raises(ValidationError):
        g.items.update(a.item_id, make("ressuscitar"))


def test_deleted_number_is_never_reused(g, project):
    a = g.items.create(project.id, make("A"))
    g.items.delete(a.item_id)
    b = g.items.create(project.id, make("B"))
    assert b.code == "MERC-2"


def test_tags_belong_to_versions(g, project):
    a = g.items.create(project.id, make("A", tags=("ux",)))
    g.items.set_tags(a.item_id, ("ux", "api"))
    g.items.set_tags(a.item_id, "api")
    assert g.items.list_items(project.id, 1)[0].data.tags == ("ux",)
    assert g.items.list_items(project.id, 2)[0].data.tags == ("api", "ux")
    assert g.items.list_items(project.id, 3)[0].data.tags == ("api",)
    summaries = [r.summary for r in g.items.revisions(project.id)]
    assert summaries[1] == "MERC-1: tags +api"
    assert summaries[0] == "MERC-1: tags −ux"
    assert g.items.all_tags() == ["api"]


def test_multi_field_summary(g, project):
    a = g.items.create(project.id, make("A"))
    g.items.update(
        a.item_id,
        replace(a.data, status=ItemStatus.APPROVED, difficulty=5, body="novo", type=ItemType.ADJUSTMENT),
    )
    s = g.items.revisions(project.id)[0].summary
    assert s == "MERC-1: tipo Melhoria → Ajuste; dificuldade 3/5 → 5/5; status Aberto → Aprovado; corpo alterado"


def test_change_is_atomic(g, project, monkeypatch):
    a = g.items.create(project.id, make("A"))
    from giano.db import items as repo

    def boom(*args, **kwargs):
        raise RuntimeError("falha simulada")

    monkeypatch.setattr(repo, "insert_version", boom)
    with pytest.raises(RuntimeError):
        g.items.update(a.item_id, replace(a.data, title="B"))
    # Nothing half-written: no new revision, the version is still current.
    assert g.items.latest_revision(project.id) == 1
    assert g.items.get(a.item_id).data.title == "A"
    assert g.items.get(a.item_id).rev_end is None


def test_validation(g, project):
    with pytest.raises(ValidationError):
        g.items.create(project.id, make("   "))
    with pytest.raises(ValidationError):
        g.items.create(project.id, make("x", difficulty=6))
    assert g.items.latest_revision(project.id) == 0


def test_archived_project_rejects_new_items(g, project):
    g.projects.set_active(project.id, False)
    with pytest.raises(ValidationError):
        g.items.create(project.id, make("A"))


def test_get_by_code(g, project):
    a = g.items.create(project.id, make("A"))
    assert g.items.get_by_code("merc-1").item_id == a.item_id
    assert g.items.get_by_code("MERC-99") is None
    assert g.items.get_by_code("bogus") is None


def test_migrations_are_idempotent(tmp_path):
    path = tmp_path / "m.db"
    db = Database(path)
    db.close()
    db = Database(path)
    assert db.query_one("SELECT MAX(version) FROM schema_version")[0] >= 1
    assert db.query_one("SELECT COUNT(*) FROM schema_version")[0] == 1
    db.close()
