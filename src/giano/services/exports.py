"""Exports: Markdown for Claude and self-contained HTML for sharing."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import date, datetime
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

from giano.domain import (
    ItemType,
    ItemView,
    NoteStatus,
    Project,
    ValidationError,
    display_value,
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
    return (v.data.priority.rank, v.number)


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
        items = sorted(self.items.list_items(project.id, rev), key=sort_key)
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
        for v in items:
            out += [f"### {v.code} — {v.data.title}", item_meta_line(v)]
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
    ) -> tuple[str, int, str]:
        """Render the HTML document. Returns (html, revision, revision date ISO)."""
        latest = self.items.latest_revision(project.id)
        rev = latest if rev is None else rev
        if not 0 <= rev <= latest:
            raise ValidationError(f"Revisão {rev} não existe (última: {latest}).")
        revision = self.items.revision(project.id, rev)
        rev_date = revision.created_at if revision else project.created_at
        items = self.items.list_items(project.id, rev)

        groups = []
        for t in ItemType:
            members = sorted((v for v in items if v.data.type == t), key=sort_key)
            if members:
                groups.append(
                    {
                        "type": t,
                        "items": [
                            {"v": v, "anchor": v.code.lower(), "body_html": to_html(v.data.body)}
                            for v in members
                        ],
                    }
                )

        diff = None
        if changes and rev > 0:
            base = rev - 1 if compare_to is None else compare_to
            if not 0 <= base < rev:
                raise ValidationError("A revisão de comparação deve ser anterior à exportada.")
            diff = self.items.diff(project.id, base, rev)
            revs = [r for r in self.items.revisions(project.id) if base < r.number <= rev]
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
    ) -> ExportResult:
        project = self.projects.require(project_code)
        html, rev, rev_date = self.html(project, rev, changes, compare_to)
        day = rev_date[:10]
        folder = Path(out_dir) if out_dir else self.settings.export_dir
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / f"giano_{project.code}_rev{rev}_{day}.html"
        path.write_text(html, encoding="utf-8")
        return ExportResult(path, html, f"Rev. {rev} exportada para {path}")
