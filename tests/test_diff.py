"""Diff between revisions."""

from __future__ import annotations

from dataclasses import replace

import pytest

from giano.domain import ItemStatus, Priority, ValidationError
from giano.services.formatting import diff_markdown
from tests.conftest import make


@pytest.fixture
def history(g, project):
    a = g.items.create(project.id, make("A", priority=Priority.HIGH))   # 1
    b = g.items.create(project.id, make("B"))                           # 2
    c = g.items.create(project.id, make("C"))                           # 3
    g.items.set_priority(a.item_id, Priority.LOW)                       # 4
    g.items.delete(b.item_id)                                           # 5
    d = g.items.create(project.id, make("D"))                           # 6
    g.items.update(c.item_id, replace(c.data, title="C2", tags=("x",))) # 7
    return a, b, c, d


def test_added_removed_changed(g, project, history):
    a, b, c, d = history
    diff = g.items.diff(project.id, 3, 7)
    assert [v.code for v in diff.added] == ["MERC-4"]
    assert [v.code for v in diff.removed] == ["MERC-2"]
    assert [ch.code for ch in diff.changed] == ["MERC-1", "MERC-3"]
    fields = {ch.code: [f.field for f in ch.changes] for ch in diff.changed}
    assert fields == {"MERC-1": ["priority"], "MERC-3": ["title", "tags"]}
    prio = diff.changed[0].changes[0]
    assert (prio.old, prio.new) == (Priority.HIGH, Priority.LOW)


def test_diff_from_zero_is_everything_added(g, project, history):
    diff = g.items.diff(project.id, 0, 7)
    assert [v.code for v in diff.added] == ["MERC-1", "MERC-3", "MERC-4"]
    assert diff.removed == [] and diff.changed == []


def test_same_revision_is_empty(g, project, history):
    assert g.items.diff(project.id, 5, 5).is_empty


def test_change_and_revert_is_not_a_difference(g, project):
    a = g.items.create(project.id, make("A"))                         # 1
    g.items.set_status(a.item_id, ItemStatus.APPROVED)                # 2
    g.items.set_status(a.item_id, ItemStatus.OPEN)                    # 3
    assert g.items.diff(project.id, 1, 3).is_empty
    assert not g.items.diff(project.id, 1, 2).is_empty


def test_invalid_revision(g, project, history):
    with pytest.raises(ValidationError):
        g.items.diff(project.id, 0, 99)


def test_diff_markdown(g, project, history):
    md = diff_markdown(g.items.diff(project.id, 3, 7))
    assert "## Adicionados (1)" in md
    assert "~~**MERC-2** B~~" in md
    assert "prioridade: Alta → **Baixa**" in md
    assert "tags: — → **x**" in md
