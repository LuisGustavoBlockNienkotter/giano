"""Exports: Markdown for Claude and self-contained HTML for sharing."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import date, datetime
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

from giano.domain import (
    ItemKey,
    ItemType,
    ItemView,
    NoteStatus,
    Project,
    ValidationError,
    display_value,
    item_code,
)
from giano.services.items import ItemService
from giano.services.markdown import demote_headings, pygments_css, to_html
from giano.services.notes import NoteService, parse_date
from giano.services.projects import ProjectService
from giano.services.settings import SettingsService

TEMPLATES = Path(__file__).resolve().parent.parent / "templates"


@dataclass(frozen=True)
class ExportResult:
    path: Path
    content: str
    summary: str  # user-facing one-liner


def sort_key(v: ItemView):
    return (v.data.priority.rank, v.key)


def selection_keys(selection) -> frozenset[ItemKey] | None:
    """Normalize an item selection: ints are normal items (``33`` = MERC-33),
    ``(number, addendum)`` tuples pick addenda too (``(33, 1)`` = MERC-33.1)."""
    if selection is None:
        return None
    return frozenset((n, 0) if isinstance(n, int) else (int(n[0]), int(n[1])) for n in selection)


def format_date(iso: str) -> str:
    """``2026-11-11T10:30:00`` → ``11/11/2026 10:30``; ``2026-11-11`` → ``11/11/2026``."""
    try:
        if "T" in iso:
            return datetime.fromisoformat(iso).strftime("%d/%m/%Y %H:%M")
        return date.fromisoformat(iso).strftime("%d/%m/%Y")
    except ValueError:
        return iso


def item_meta_line(v: ItemView) -> str:
    d = v.data
    tags = ", ".join(d.tags) if d.tags else "—"
    return (
        f"Tipo: {d.type.label} · Prioridade: {d.priority.label} · "
        f"Dificuldade: {d.difficulty}/5 · Status: {d.status.label} · Tags: {tags}"
    )


class Exporter:
    def __init__(
        self,
        projects: ProjectService,
        items: ItemService,
        notes: NoteService,
        settings: SettingsService,
        clock: Callable[[], datetime] = datetime.now,
    ):
        self.projects = projects
        self.items = items
        self.notes = notes
        self.settings = settings
        self.clock = clock
        self._env = Environment(
            loader=FileSystemLoader(TEMPLATES),
            autoescape=select_autoescape(["html", "j2"]),
            trim_blocks=True,
            lstrip_blocks=True,
        )

    # ------------------------------------------------------------------
    # Claude
    # ------------------------------------------------------------------

    def claude_markdown(
        self,
        project: Project,
        since: str | None = None,
        all_notes: bool = False,
        include_unassigned: bool = False,
        note_ids: list[int] | None = None,
    ) -> tuple[str, int]:
        """Build the Markdown document. Returns (content, number of notes)."""
        rev = self.items.latest_revision(project.id)
        items = sorted(self.items.list_items(project.id, rev, archived=False), key=sort_key)
        if note_ids is not None:
            notes = [n for n in (self.notes.get(i) for i in note_ids) if n]
            notes.sort(key=lambda n: (n.note_date, n.created_at, n.id))
        else:
            statuses = None if all_notes else [NoteStatus.PENDING]
            notes = self.notes.list(
                project.id, statuses, since, include_unassigned, oldest_first=True
            )
        now = self.clock()

        out: list[str] = [
            f"# {project.code} — {project.name}: exportação para análise",
            "",
            f"Gerado pelo Giano em {now.strftime('%d/%m/%Y %H:%M')} · "
            f"lista na Rev. {rev} · {len(items)} itens · {len(notes)} anotações",
            "",
            "## 1. Contexto do projeto",
            "",
            demote_headings(project.description, 2) or "_(sem descrição)_",
            "",
            f"## 2. Lista atual de melhorias (Rev. {rev})",
            "",
        ]
        if not items:
            out += ["_(lista vazia)_", ""]
        by_key = {v.key: v for v in self.items.list_items(project.id, rev)}
        for v in items:
            out += [f"### {v.code} — {v.data.title}", item_meta_line(v)]
            if v.is_addendum:
                original = by_key.get((v.number, 0))
                ref = f"{v.parent_code} — {original.data.title} ({original.data.status.label})"                     if original else f"{v.parent_code}"
                out.append(f"Adendo de {ref}")
            if v.data.body:
                out += ["", demote_headings(v.data.body, 3)]
            out.append("")

        if note_ids is not None:
            which = "selecionadas"
        else:
            which = "todas" if all_notes else "pendentes"
            if since:
                which += f", desde {format_date(parse_date(since))}"
        out += [f"## 3. Anotações ({which})", ""]
        if not notes:
            out += ["_(nenhuma anotação)_", ""]
        for n in notes:
            extra = "" if n.project_id == project.id else " (sem projeto)"
            out += [f"### {format_date(n.note_date)} — {n.name}{extra}", ""]
            out += [demote_headings(n.body, 3) or "_(vazia)_", ""]

        prompt = (
            self.settings.claude_prompt.replace("{sigla}", project.code)
            .replace("{nome}", project.name)
            .strip()
        )
        out += ["## 4. Pedido de análise", "", prompt, ""]
        return "\n".join(out), len(notes)

    def export_claude(
        self,
        project_code: str,
        since: str | None = None,
        all_notes: bool = False,
        include_unassigned: bool = False,
        output: Path | None = None,
        note_ids: list[int] | None = None,
    ) -> ExportResult:
        project = self.projects.require(project_code)
        content, n_notes = self.claude_markdown(
            project, since, all_notes, include_unassigned, note_ids
        )
        if output is None or output.is_dir() or str(output).endswith(("/", "\\")):
            folder = output or self.settings.export_dir
            name = f"giano_{project.code}_claude_{self.clock().date().isoformat()}.md"
            output = Path(folder) / name
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(content, encoding="utf-8")
        return ExportResult(output, content, f"{n_notes} anotações exportadas para {output}")

    # ------------------------------------------------------------------
    # HTML
    # ------------------------------------------------------------------

    def html(
        self,
        project: Project,
        rev: int | None = None,
        changes: bool = True,
        compare_to: int | None = None,
        numbers=None,
    ) -> tuple[str, int, str]:
        """Render the HTML document. Returns (html, revision, revision date ISO).

        ``numbers`` limits the document to those items (e.g. {1, 3} = MERC-1 and MERC-3;
        ``(33, 1)`` = the addendum MERC-33.1, see :func:`selection_keys`); ``None`` means
        all items of the revision. Archived items never go into the document (not even in
        the changes section). An addendum always shows a reference to its original item,
        with the original's text (collapsed) when the original is not archived.
        """
        keys = selection_keys(numbers)
        latest = self.items.latest_revision(project.id)
        rev = latest if rev is None else rev
        if not 0 <= rev <= latest:
            raise ValidationError(f"Revisão {rev} não existe (última: {latest}).")
        revision = self.items.revision(project.id, rev)
        rev_date = revision.created_at if revision else project.created_at
        everything = self.items.list_items(project.id, rev)
        archived_ids = {v.item_id for v in everything if v.data.archived}
        items = [v for v in everything if not v.data.archived]
        total_in_rev = len(items)
        if keys is not None:
            archived = sorted(v.key for v in everything if v.data.archived and v.key in keys)
            if archived:
                codes = ", ".join(item_code(project.code, k) for k in archived)
                raise ValidationError(f"Itens arquivados na Rev. {rev} não são exportados: {codes}.")
            missing = sorted(set(keys) - {v.key for v in items})
            if missing:
                codes = ", ".join(item_code(project.code, k) for k in missing)
                raise ValidationError(f"Itens que não existem na Rev. {rev}: {codes}.")
            items = [v for v in items if v.key in keys]
            if not items:
                raise ValidationError("Selecione ao menos um item para exportar.")
        selected_ids = {v.item_id for v in items}
        by_key = {v.key: v for v in everything}
        in_doc = {v.key for v in items}

        def entry(v: ItemView) -> dict:
            e = {"v": v, "anchor": v.code.lower(), "body_html": to_html(v.data.body)}
            if v.is_addendum:
                original = by_key.get((v.number, 0))
                e["original"] = original and {
                    "v": original,
                    "anchor": original.code.lower() if original.key in in_doc else None,
                    "body_html": None if original.data.archived else to_html(original.data.body),
                }
                e["original_code"] = v.parent_code
            else:
                e["addenda"] = [
                    {"v": a, "anchor": a.code.lower() if a.key in in_doc else None}
                    for a in items if a.number == v.number and a.is_addendum
                ]
            return e

        groups = []
        for t in ItemType:
            members = sorted((v for v in items if v.data.type == t), key=sort_key)
            if members:
                groups.append({"type": t, "items": [entry(v) for v in members]})

        diff = None
        if changes and rev > 0:
            base = rev - 1 if compare_to is None else compare_to
            if not 0 <= base < rev:
                raise ValidationError("A revisão de comparação deve ser anterior à exportada.")
            diff = self.items.diff(project.id, base, rev, include_archived=False)
            revs = [r for r in self.items.revisions(project.id) if base < r.number <= rev]
            # Items archived at this revision are left out entirely.
            diff.removed = [v for v in diff.removed if v.item_id not in archived_ids]
            revs = [r for r in revs if r.item_id not in archived_ids]
            if keys is not None:
                # Only the selected items: nothing about the others goes into the file.
                diff.added = [v for v in diff.added if v.item_id in selected_ids]
                diff.changed = [c for c in diff.changed if c.new.item_id in selected_ids]
                diff.removed = []
                revs = [r for r in revs if r.item_id in selected_ids]
            diff_revisions = sorted(revs, key=lambda r: r.number)
        else:
            diff_revisions = []

        template = self._env.get_template("export.html.j2")
        html = template.render(
            project=project,
            description_html=to_html(project.description),
            rev=rev,
            latest=latest,
            rev_date=format_date(rev_date),
            generated=self.clock().strftime("%d/%m/%Y %H:%M"),
            groups=groups,
            total=len(items),
            total_in_rev=total_in_rev,
            diff=diff,
            diff_revisions=diff_revisions,
            display_value=display_value,
            format_date=format_date,
            pygments_css=pygments_css(),
        )
        return html, rev, rev_date

    def export_html(
        self,
        project_code: str,
        rev: int | None = None,
        out_dir: Path | None = None,
        changes: bool = True,
        compare_to: int | None = None,
        numbers=None,
    ) -> ExportResult:
        project = self.projects.require(project_code)
        html, rev, rev_date = self.html(project, rev, changes, compare_to, numbers)
        day = rev_date[:10]
        folder = Path(out_dir) if out_dir else self.settings.export_dir
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"giano_{project.code}_rev{rev}_{day}.html"
        path.write_text(html, encoding="utf-8")
        return ExportResult(path, html, f"Rev. {rev} exportada para {path}")
