"""Notes (not versioned), item creation from notes, FTS5 search, projects."""

from __future__ import annotations

from dataclasses import replace

import pytest

from giano.domain import NoteStatus, ValidationError
from tests.conftest import make


def test_notes_do_not_create_revisions(g, project):
    n = g.notes.create("Reunião", project.id, "corpo")
    g.notes.save(n.id, name="Reunião 2", note_date="2026-03-01", project_id=project.id,
                 body="outro", status=NoteStatus.PROCESSED)
    assert g.items.latest_revision(project.id) == 0
    n2 = g.notes.get(n.id)
    assert (n2.name, n2.note_date, n2.body, n2.status) == (
        "Reunião 2", "2026-03-01", "outro", NoteStatus.PROCESSED)


def test_note_defaults(g, clock):
    n = g.notes.create()
    assert n.note_date == "2026-03-10"
    assert n.name.startswith("Anotação ")
    assert n.status == NoteStatus.PENDING
    assert n.project_id is None


def test_note_date_validation(g):
    assert g.notes.create("x", note_date="05/02/2026").note_date == "2026-02-05"
    with pytest.raises(ValidationError):
        g.notes.create("x", note_date="ontem")


def test_item_from_note_keeps_link(g, project):
    n = g.notes.create("Reunião", project.id, "pedido")
    item = g.notes.create_item(n.id, project.id, make("Novo"))
    assert item.source_note_id == n.id
    assert g.notes.linked_items(n.id) == ["MERC-1"]
    # The link survives edits (it is on the item identity, not the version).
    g.items.update(item.item_id, replace(item.data, title="Novo 2"))
    assert g.items.get(item.item_id).source_note_id == n.id
    # Deleting the note keeps the item.
    g.notes.delete(n.id)
    assert g.items.get(item.item_id).source_note_id is None


def test_list_notes_filters(g, project):
    other = g.projects.create("P2K", "P2000")
    g.notes.create("a", project.id, note_date="2026-01-01")
    b = g.notes.create("b", project.id, note_date="2026-02-01")
    g.notes.create("c", other.id, note_date="2026-02-02")
    g.notes.create("d", None, note_date="2026-02-03")
    g.notes.set_status(b.id, NoteStatus.ARCHIVED)
    names = lambda ns: [n.name for n in ns]  # noqa: E731
    assert names(g.notes.list(project.id, oldest_first=True)) == ["a", "b"]
    assert names(g.notes.list(project.id, [NoteStatus.PENDING])) == ["a"]
    assert names(g.notes.list(project.id, since="2026-01-15")) == ["b"]
    assert names(g.notes.list(project.id, include_unassigned=True, oldest_first=True)) == ["a", "b", "d"]


def test_search_items_only_current_versions(g, project):
    a = g.items.create(project.id, make("Relatório de vendas", body="exportar planilha", tags=("financeiro",)))
    g.items.update(a.item_id, replace(a.data, body="exportar CSV"))
    hits = g.search.search("planilha")
    assert hits == []  # old version is not searchable
    hits = g.search.search("csv")
    assert [(h.kind, h.label) for h in hits] == [("item", "MERC-1")]
    assert g.search.search("relatorio")[0].label == "MERC-1"  # accents ignored
    assert g.search.search("financ")[0].label == "MERC-1"     # prefix + tags
    g.items.delete(a.item_id)
    assert g.search.search("csv") == []


def test_search_notes(g, project):
    n = g.notes.create("Reunião comercial", project.id, "frete de telhas")
    assert [h.id for h in g.search.search("telha")] == [n.id]
    g.notes.save(n.id, name=n.name, note_date=n.note_date, project_id=project.id,
                 body="parcelamento", status=n.status)
    assert g.search.search("telha") == []
    assert g.search.search("parcel")[0].kind == "note"
    g.notes.delete(n.id)
    assert g.search.search("parcel") == []


def test_search_is_safe_with_fts_syntax(g, project):
    g.items.create(project.id, make("abc"))
    for q in ['"', "AND OR NOT", "a*b", "(", "título: x", ""]:
        g.search.search(q)  # must not raise


def test_projects(g, project):
    with pytest.raises(ValidationError):
        g.projects.create("merc", "Duplicado")
    with pytest.raises(ValidationError):
        g.projects.create("1ABC", "Sigla inválida")
    p = g.projects.update(project.id, "mrc", "Mercurius", "", True)
    assert p.code == "MRC"
    item = g.items.create(p.id, make("A"))
    assert item.code == "MRC-1"
    g.projects.set_active(p.id, False)
    assert [x.code for x in g.projects.list(include_archived=False)] == []
    assert [x.code for x in g.projects.list()] == ["MRC"]


def test_settings(g, tmp_path):
    from giano.config import DEFAULT_CLAUDE_PROMPT

    assert g.settings.claude_prompt == DEFAULT_CLAUDE_PROMPT
    g.settings.claude_prompt = "Analise {sigla}."
    assert g.settings.claude_prompt == "Analise {sigla}."
    g.settings.claude_prompt = ""
    assert g.settings.claude_prompt == DEFAULT_CLAUDE_PROMPT
    g.settings.export_dir = str(tmp_path / "x")
    assert g.settings.export_dir == tmp_path / "x"
