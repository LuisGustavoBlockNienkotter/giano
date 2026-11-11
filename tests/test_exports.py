"""Claude (Markdown) and HTML exports."""

from __future__ import annotations

import re

import pytest

from giano.domain import ItemStatus, ItemType, NoteStatus, Priority, ValidationError
from giano.services.markdown import demote_headings, to_html
from tests.conftest import make


@pytest.fixture
def populated(g, project):
    a = g.items.create(project.id, make(
        "Checkout rápido", type=ItemType.IMPROVEMENT, priority=Priority.HIGH, difficulty=4,
        tags=("ux", "checkout"), body="## Detalhes\n\n```python\nprint('oi')\n```"))
    b = g.items.create(project.id, make("Bug no frete", type=ItemType.ADJUSTMENT, priority=Priority.CRITICAL))
    g.items.create(project.id, make("Política <b>de</b> devolução", type=ItemType.DISCUSSION))
    g.items.set_status(b.item_id, ItemStatus.DONE)
    g.notes.create("Reunião 1", project.id, "# Pauta\nfrete caro", "2026-03-01")
    n2 = g.notes.create("Reunião 2", project.id, "parcelamento", "2026-03-05")
    g.notes.create("Sem projeto", None, "solta", "2026-03-06")
    g.notes.create("Antiga", project.id, "velha", "2026-01-01")
    g.notes.set_status(n2.id, NoteStatus.PROCESSED)
    return a, b


# ---------------------------------------------------------------- Claude


def test_claude_export_structure(g, project, populated):
    r = g.exports.export_claude("merc")
    md = r.content
    assert r.path.name == "giano_MERC_claude_2026-03-10.md"
    assert r.path.read_text(encoding="utf-8") == md
    sections = re.findall(r"^## (.+)$", md, flags=re.M)
    assert sections == [
        "1. Contexto do projeto",
        "2. Lista atual de melhorias (Rev. 4)",
        "3. Anotações (pendentes)",
        "4. Pedido de análise",
    ]
    assert "Sistema de **gestão**." in md
    # compact item format, sorted by priority
    assert md.index("### MERC-2 — Bug no frete") < md.index("### MERC-1 — Checkout rápido")
    assert ("Tipo: Melhoria · Prioridade: Alta · Dificuldade: 4/5 · Status: Aberto · "
            "Tags: checkout, ux") in md
    # body headings are demoted below the item heading; code fences untouched
    assert "##### Detalhes" in md and "print('oi')" in md
    # pending notes only, chronological, with date and name
    assert md.index("01/01/2026 — Antiga") < md.index("01/03/2026 — Reunião 1")
    assert "Reunião 2" not in md and "Sem projeto" not in md
    assert "#### Pauta" in md
    assert "relacione" in md.lower()


def test_claude_export_options(g, project, populated):
    md = g.exports.export_claude("MERC", all_notes=True).content
    assert "Reunião 2" in md and "(todas)" in md
    md = g.exports.export_claude("MERC", since="2026-02-01").content
    assert "Antiga" not in md and "Reunião 1" in md
    md = g.exports.export_claude("MERC", include_unassigned=True).content
    assert "Sem projeto (sem projeto)" in md


def test_claude_prompt_template_is_configurable(g, project, populated):
    g.settings.claude_prompt = "Analise o {sigla} ({nome}) por favor."
    md = g.exports.export_claude("MERC").content
    assert md.rstrip().endswith("Analise o MERC (Mercurius) por favor.")


def test_claude_export_unknown_project(g):
    with pytest.raises(ValidationError):
        g.exports.export_claude("NOPE")


def test_claude_export_custom_output(g, project, populated, tmp_path):
    out = tmp_path / "x" / "saida.md"
    r = g.exports.export_claude("MERC", output=out)
    assert r.path == out and out.exists()


# ---------------------------------------------------------------- HTML


def test_html_export_latest(g, project, populated):
    r = g.exports.export_html("MERC")
    html = r.content
    assert re.fullmatch(r"giano_MERC_rev4_\d{4}-\d{2}-\d{2}\.html", r.path.name)
    assert r.path.read_text(encoding="utf-8") == html
    # self-contained: inline CSS, no external resources
    assert "<style>" in html
    assert not re.search(r'<(link|script)\b', html)
    assert not re.search(r'(src|href)="https?://', html)
    # header, toc, grouping, labels
    assert "Mercurius" in html and "Revisão <strong>4</strong>" in html
    assert 'href="#merc-1"' in html and 'id="merc-1"' in html
    assert html.index('id="tipo-improvement"') < html.index('id="tipo-discussion"') < html.index('id="tipo-adjustment"')
    assert "Prioridade: Crítica" in html and "Dificuldade: 4/5" in html and "#checkout" in html
    # markdown rendered + pygments highlighting
    assert "<h2>Detalhes</h2>" in html
    assert 'class="highlight"' in html and '<span class="nb">print</span>' in html
    # raw HTML in user content is escaped
    assert "Política &lt;b&gt;de&lt;/b&gt; devolução" in html
    # changes section (rev 3 -> 4)
    assert "Alterações nesta revisão" in html
    assert "status: Aberto → <strong>Concluído</strong>" in html


def test_html_export_old_revision(g, project, populated):
    r = g.exports.export_html("MERC", rev=2)
    assert "giano_MERC_rev2_2026-03-10.html" == r.path.name
    assert "Checkout rápido" in r.content and "Bug no frete" in r.content
    assert "Política" not in r.content
    assert "(de 4)" in r.content


def test_html_export_without_changes_and_compare(g, project, populated):
    html = g.exports.export_html("MERC", changes=False).content
    assert "Alterações nesta revisão" not in html
    html = g.exports.export_html("MERC", compare_to=0).content
    assert "Adicionados (3)" in html
    with pytest.raises(ValidationError):
        g.exports.export_html("MERC", rev=2, compare_to=3)


def test_html_export_empty_project(g, project):
    r = g.exports.export_html("MERC")
    assert "Nenhum item nesta revisão" in r.content
    assert r.path.name.startswith("giano_MERC_rev0_")


def test_html_export_invalid_revision(g, project, populated):
    with pytest.raises(ValidationError):
        g.exports.export_html("MERC", rev=10)


# ---------------------------------------------------------------- helpers


def test_demote_headings_skips_code():
    src = "# A\ntexto\n```\n# comentário\n```\n###### F"
    assert demote_headings(src, 2) == "### A\ntexto\n```\n# comentário\n```\n###### F"


def test_to_html_unknown_language():
    html = to_html("```nãoexiste\nx < y\n```")
    assert "x &lt; y" in html
