//! Markdown renderings of an item and of a diff (copy as Markdown, CLI-style output).

use crate::domain::{display_value, Field, ItemView, Note, RevisionDiff};
use crate::services::markdown::demote_headings;

/// `original`: the original item of an addendum; `addenda`: the addenda of an item.
pub fn item_preview(v: &ItemView, source_note: Option<&Note>, original: Option<&ItemView>, addenda: &[ItemView]) -> String {
    let d = &v.data;
    let tags = if d.tags.is_empty() {
        "—".to_string()
    } else {
        d.tags.iter().map(|t| format!("`#{t}`")).collect::<Vec<_>>().join(" ")
    };
    let mut lines = vec![
        format!("# {} — {}", v.code(), d.title),
        String::new(),
        format!(
            "**{}** · Prioridade **{}** · Dificuldade **{}/5** · Status **{}**",
            d.kind.label(),
            d.priority.label(),
            d.difficulty,
            d.status.label()
        ),
        String::new(),
        format!("Tags: {tags}"),
    ];
    if let Some(parent) = v.parent_code() {
        let mut reference = format!("**{parent}**");
        if let Some(o) = original {
            let archived = if o.data.archived { ", arquivado" } else { "" };
            reference.push_str(&format!(" — {} ({}{archived})", o.data.title, o.data.status.label()));
        }
        lines.extend([String::new(), format!("> **Adendo** de {reference}")]);
    }
    if !addenda.is_empty() {
        let parts: Vec<String> = addenda
            .iter()
            .map(|a| {
                let archived = if a.data.archived { ", arquivado" } else { "" };
                format!("**{}** {} ({}{archived})", a.code(), a.data.title, a.data.status.label())
            })
            .collect();
        lines.extend([String::new(), format!("Adendos: {}", parts.join(" · "))]);
    }
    if d.archived {
        lines.extend([String::new(), "> **Arquivado** — fora da lista normal e das exportações.".into()]);
    }
    if let Some(n) = source_note {
        lines.extend([String::new(), format!("_Origem: anotação de {} — {}_", n.note_date, n.name)]);
    }
    lines.extend([String::new(), "---".into(), String::new()]);
    lines.push(if d.body.is_empty() { "_Sem descrição._".into() } else { demote_headings(&d.body, 1) });
    lines.join("\n")
}

pub fn diff_markdown(diff: &RevisionDiff) -> String {
    let p = &diff.project;
    let mut out = vec![format!("# {}: Rev. {} → Rev. {}", p.code, diff.rev_from, diff.rev_to), String::new()];
    if diff.is_empty() {
        out.push("_Nenhuma diferença entre as revisões._".into());
        return out.join("\n");
    }
    out.push(format!(
        "**{}** adicionados · **{}** removidos · **{}** alterados",
        diff.added.len(),
        diff.removed.len(),
        diff.changed.len()
    ));
    if !diff.added.is_empty() {
        out.extend([String::new(), format!("## Adicionados ({})", diff.added.len()), String::new()]);
        for v in &diff.added {
            out.push(format!(
                "- **{}** {} — {}, {}, {}",
                v.code(),
                v.data.title,
                v.data.kind.label(),
                v.data.priority.label(),
                v.data.status.label()
            ));
        }
    }
    if !diff.removed.is_empty() {
        out.extend([String::new(), format!("## Removidos ({})", diff.removed.len()), String::new()]);
        for v in &diff.removed {
            out.push(format!("- ~~**{}** {}~~", v.code(), v.data.title));
        }
    }
    if !diff.changed.is_empty() {
        out.extend([String::new(), format!("## Alterados ({})", diff.changed.len()), String::new()]);
        for c in &diff.changed {
            out.push(format!("- **{}** {}", c.code(), c.new.data.title));
            for f in &c.changes {
                if f.field == Field::Body {
                    out.push("    - corpo alterado".into());
                } else {
                    out.push(format!(
                        "    - {}: {} → **{}**",
                        f.label(),
                        display_value(&f.old),
                        display_value(&f.new)
                    ));
                }
            }
        }
    }
    out.join("\n")
}
