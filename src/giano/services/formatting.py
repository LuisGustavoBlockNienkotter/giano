"""Markdown renderings used by the TUI and CLI (previews, diffs)."""

from __future__ import annotations

from giano.domain import ItemView, Note, RevisionDiff, display_value
from giano.services.markdown import demote_headings


def item_preview(v: ItemView, source_note: Note | None = None) -> str:
    d = v.data
    tags = " ".join(f"`#{t}`" for t in d.tags) or "—"
    lines = [
        f"# {v.code} — {d.title}",
        "",
        f"**{d.type.label}** · Prioridade **{d.priority.label}** · "
        f"Dificuldade **{d.difficulty}/5** · Status **{d.status.label}**",
        "",
        f"Tags: {tags}",
    ]
    if source_note:
        lines += ["", f"_Origem: anotação de {source_note.note_date} — {source_note.name}_"]
    lines += ["", "---", "", demote_headings(d.body, 1) if d.body else "_Sem descrição._"]
    return "\n".join(lines)


def diff_markdown(diff: RevisionDiff) -> str:
    p = diff.project
    out = [f"# {p.code}: Rev. {diff.rev_from} → Rev. {diff.rev_to}", ""]
    if diff.is_empty:
        out.append("_Nenhuma diferença entre as revisões._")
        return "\n".join(out)
    out.append(
        f"**{len(diff.added)}** adicionados · **{len(diff.removed)}** removidos · "
        f"**{len(diff.changed)}** alterados"
    )
    if diff.added:
        out += ["", f"## Adicionados ({len(diff.added)})", ""]
        out += [f"- **{v.code}** {v.data.title} — {v.data.type.label}, "
                f"{v.data.priority.label}, {v.data.status.label}" for v in diff.added]
    if diff.removed:
        out += ["", f"## Removidos ({len(diff.removed)})", ""]
        out += [f"- ~~**{v.code}** {v.data.title}~~" for v in diff.removed]
    if diff.changed:
        out += ["", f"## Alterados ({len(diff.changed)})", ""]
        for c in diff.changed:
            out.append(f"- **{c.code}** {c.new.data.title}")
            for f in c.changes:
                if f.field == "body":
                    out.append("    - corpo alterado")
                else:
                    out.append(
                        f"    - {f.label}: {display_value(f.field, f.old)} → "
                        f"**{display_value(f.field, f.new)}**"
                    )
    return "\n".join(out)
