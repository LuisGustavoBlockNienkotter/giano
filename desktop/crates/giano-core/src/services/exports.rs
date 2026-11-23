//! Exports: Markdown for Claude and self-contained HTML for sharing.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;
use serde_json::{json, Value as Json};

use crate::db::notes::NoteFilter;
use crate::domain::{item_code, Field, ItemKey, ItemStatus, ItemType, ItemView, NoteStatus, Project};
use crate::error::{Error, Result};
use crate::services::items::{Archived, Progress};
use crate::services::markdown::{demote_headings, escape_html, highlight_css, to_html};
use crate::services::notes::parse_date;
use crate::services::Giano;

const TEMPLATE: &str = include_str!("../../templates/export.html.j2");

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub path: PathBuf,
    pub content: String,
    /// User-facing one-liner.
    pub summary: String,
    /// Codes moved to "Em desenvolvimento" by the export.
    pub marked: Vec<String>,
    /// Items whose control date the export set to today.
    pub control_dated: usize,
}

/// Statuses that "mark as in development" moves forward; done/discarded items and the ones
/// already in development are left alone.
pub const MARKABLE: [ItemStatus; 3] = [ItemStatus::Open, ItemStatus::Analysis, ItemStatus::Approved];

fn sort_key(v: &ItemView) -> (usize, ItemKey) {
    (v.data.priority.rank(), v.key())
}

/// `2026-11-11T10:30:00` → `11/11/2026 10:30`; `2026-11-11` → `11/11/2026`.
pub fn format_date(iso: &str) -> String {
    if iso.contains('T') {
        for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
            if let Ok(dt) = NaiveDateTime::parse_from_str(iso, fmt) {
                return dt.format("%d/%m/%Y %H:%M").to_string();
            }
        }
        return iso.to_string();
    }
    NaiveDate::parse_from_str(iso, "%Y-%m-%d").map_or_else(|_| iso.to_string(), |d| d.format("%d/%m/%Y").to_string())
}

pub fn item_meta_line(v: &ItemView) -> String {
    let d = &v.data;
    let tags = if d.tags.is_empty() { "—".to_string() } else { d.tags.join(", ") };
    format!(
        "Tipo: {} · Prioridade: {} · Dificuldade: {}/5 · Status: {} · Tags: {tags}",
        d.kind.label(),
        d.priority.label(),
        d.difficulty,
        d.status.label()
    )
}

/// Options of the Claude export.
#[derive(Debug, Clone, Default)]
pub struct ClaudeOptions {
    /// Only notes from this date on (`AAAA-MM-DD`).
    pub since: Option<String>,
    /// All notes, not only the pending ones.
    pub all_notes: bool,
    /// Also notes without a project.
    pub include_unassigned: bool,
    /// Exactly these notes (overrides the three options above).
    pub note_ids: Option<Vec<i64>>,
    /// A file, or a folder (ending with a separator or existing); default: the export folder.
    pub output: Option<PathBuf>,
}

/// Options of the HTML export.
#[derive(Debug, Clone)]
pub struct HtmlOptions {
    /// Default: the latest revision.
    pub rev: Option<i64>,
    /// Include the section "Alterações nesta revisão".
    pub changes: bool,
    /// Base revision of the changes section (default: the previous one).
    pub compare_to: Option<i64>,
    /// Only these items (`(33, 0)` = MERC-33, `(33, 1)` = MERC-33.1); `None` = all.
    pub keys: Option<BTreeSet<ItemKey>>,
    /// Default: the export folder.
    pub out_dir: Option<PathBuf>,
    /// Move the exported items with status Aberto/Em análise/Aprovado to "Em desenvolvimento".
    pub mark_in_progress: bool,
    /// Set today as the control date of the exported items (not versioned: no revision).
    pub control_date: bool,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        HtmlOptions {
            rev: None,
            changes: true,
            compare_to: None,
            keys: None,
            out_dir: None,
            mark_in_progress: false,
            control_date: false,
        }
    }
}

/// A rendered HTML document.
pub struct HtmlDocument {
    pub html: String,
    pub rev: i64,
    /// ISO date/time of the revision.
    pub rev_date: String,
}

pub struct Exporter<'g> {
    pub(crate) g: &'g Giano,
}

fn render_template(ctx: Json) -> Result<String> {
    let mut env = minijinja::Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    // Escape every value except the ones marked `| safe` (our own HTML).
    env.set_formatter(|out, _state, value| {
        let text = if value.is_undefined() || value.is_none() {
            String::new()
        } else if value.is_safe() {
            value.to_string()
        } else if let Some(s) = value.as_str() {
            escape_html(s)
        } else {
            escape_html(&value.to_string())
        };
        out.write_str(&text)
            .map_err(|_| minijinja::Error::new(minijinja::ErrorKind::WriteFailure, "falha ao gerar o HTML"))
    });
    env.add_template("export.html", TEMPLATE)?;
    Ok(env.get_template("export.html")?.render(ctx)?)
}

impl<'g> Exporter<'g> {
    // ------------------------------------------------------------------
    // Claude
    // ------------------------------------------------------------------

    /// Build the Markdown document. Returns (content, number of notes).
    pub fn claude_markdown(&self, project: &Project, opts: &ClaudeOptions) -> Result<(String, usize)> {
        let items_svc = self.g.items();
        let rev = items_svc.latest_revision(project.id)?;
        let everything = items_svc.list_items(project.id, Some(rev), Archived::Any)?;
        let mut items: Vec<&ItemView> = everything.iter().filter(|v| !v.data.archived).collect();
        items.sort_by_key(|v| sort_key(v));
        let notes_svc = self.g.notes();
        let notes = match &opts.note_ids {
            Some(ids) => {
                let mut notes = Vec::new();
                for id in ids {
                    if let Some(n) = notes_svc.get(*id)? {
                        notes.push(n);
                    }
                }
                notes.sort_by(|a, b| (&a.note_date, &a.created_at, a.id).cmp(&(&b.note_date, &b.created_at, b.id)));
                notes
            }
            None => notes_svc.list(&NoteFilter {
                project_id: Some(project.id),
                statuses: if opts.all_notes { vec![] } else { vec![NoteStatus::Pending] },
                since: opts.since.clone(),
                include_unassigned: opts.include_unassigned,
                oldest_first: true,
            })?,
        };
        let now = self.g.now();
        let description = demote_headings(&project.description, 2);
        let mut out: Vec<String> = vec![
            format!("# {} — {}: exportação para análise", project.code, project.name),
            String::new(),
            format!(
                "Gerado pelo Giano em {} · lista na Rev. {rev} · {} itens · {} anotações",
                now.format("%d/%m/%Y %H:%M"),
                items.len(),
                notes.len()
            ),
            String::new(),
            "## 1. Contexto do projeto".into(),
            String::new(),
            if description.is_empty() { "_(sem descrição)_".into() } else { description },
            String::new(),
            format!("## 2. Lista atual de melhorias (Rev. {rev})"),
            String::new(),
        ];
        if items.is_empty() {
            out.extend(["_(lista vazia)_".into(), String::new()]);
        }
        let by_key: BTreeMap<ItemKey, &ItemView> = everything.iter().map(|v| (v.key(), v)).collect();
        for v in &items {
            out.push(format!("### {} — {}", v.code(), v.data.title));
            out.push(item_meta_line(v));
            if let Some(parent) = v.parent_code() {
                let reference = match by_key.get(&(v.number, 0)) {
                    Some(o) => format!("{parent} — {} ({})", o.data.title, o.data.status.label()),
                    None => parent,
                };
                out.push(format!("Adendo de {reference}"));
            }
            if !v.data.body.is_empty() {
                out.extend([String::new(), demote_headings(&v.data.body, 3)]);
            }
            out.push(String::new());
        }

        let which = if opts.note_ids.is_some() {
            "selecionadas".to_string()
        } else {
            let mut which = if opts.all_notes { "todas" } else { "pendentes" }.to_string();
            if let Some(since) = opts.since.as_deref().filter(|s| !s.trim().is_empty()) {
                which.push_str(&format!(", desde {}", format_date(&parse_date(since)?)));
            }
            which
        };
        out.extend([format!("## 3. Anotações ({which})"), String::new()]);
        if notes.is_empty() {
            out.extend(["_(nenhuma anotação)_".into(), String::new()]);
        }
        for n in &notes {
            let extra = if n.project_id == Some(project.id) { "" } else { " (sem projeto)" };
            out.extend([format!("### {} — {}{extra}", format_date(&n.note_date), n.name), String::new()]);
            let body = demote_headings(&n.body, 3);
            out.extend([if body.is_empty() { "_(vazia)_".into() } else { body }, String::new()]);
        }

        let prompt = self.g.settings().claude_prompt()?;
        let prompt = prompt.replace("{sigla}", &project.code).replace("{nome}", &project.name);
        out.extend(["## 4. Pedido de análise".into(), String::new(), prompt.trim().to_string(), String::new()]);
        Ok((out.join("\n"), notes.len()))
    }

    pub fn export_claude(&self, project_code: &str, opts: &ClaudeOptions) -> Result<ExportResult> {
        let project = self.g.projects().require(project_code)?;
        let (content, n_notes) = self.claude_markdown(&project, opts)?;
        let name = format!("giano_{}_claude_{}.md", project.code, self.g.now().date().format("%Y-%m-%d"));
        let path = match &opts.output {
            Some(out) if !(out.is_dir() || out.to_string_lossy().ends_with(['/', '\\'])) => out.clone(),
            Some(folder) => folder.join(&name),
            None => self.g.settings().export_dir()?.join(&name),
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &content)?;
        let summary = format!("{n_notes} anotações exportadas para {}", path.display());
        Ok(ExportResult { path, content, summary, marked: vec![], control_dated: 0 })
    }

    // ------------------------------------------------------------------
    // HTML
    // ------------------------------------------------------------------

    /// Render the HTML document.
    ///
    /// `opts.keys` limits the document to those items; `None` means all items of the
    /// revision. Archived items never go into the document (not even in the changes section).
    /// An addendum always shows a reference to its original item, with the original's text
    /// (collapsed) when the original is not archived.
    pub fn html(&self, project: &Project, opts: &HtmlOptions) -> Result<HtmlDocument> {
        let items_svc = self.g.items();
        let keys = opts.keys.as_ref();
        let latest = items_svc.latest_revision(project.id)?;
        let rev = opts.rev.unwrap_or(latest);
        if !(0..=latest).contains(&rev) {
            return Err(Error::validation(format!("Revisão {rev} não existe (última: {latest}).")));
        }
        let rev_date = match items_svc.revision(project.id, rev)? {
            Some(r) => r.created_at,
            None => project.created_at.clone(),
        };
        let everything = items_svc.list_items(project.id, Some(rev), Archived::Any)?;
        let archived_ids: HashSet<i64> = everything.iter().filter(|v| v.data.archived).map(|v| v.item_id).collect();
        let mut items: Vec<&ItemView> = everything.iter().filter(|v| !v.data.archived).collect();
        let total_in_rev = items.len();
        if let Some(keys) = keys {
            let codes = |ks: Vec<ItemKey>| ks.into_iter().map(|k| item_code(&project.code, k)).collect::<Vec<_>>().join(", ");
            let archived: Vec<ItemKey> =
                everything.iter().filter(|v| v.data.archived && keys.contains(&v.key())).map(ItemView::key).collect();
            if !archived.is_empty() {
                return Err(Error::validation(format!(
                    "Itens arquivados na Rev. {rev} não são exportados: {}.",
                    codes(archived)
                )));
            }
            let present: BTreeSet<ItemKey> = items.iter().map(|v| v.key()).collect();
            let missing: Vec<ItemKey> = keys.difference(&present).copied().collect();
            if !missing.is_empty() {
                return Err(Error::validation(format!("Itens que não existem na Rev. {rev}: {}.", codes(missing))));
            }
            items.retain(|v| keys.contains(&v.key()));
            if items.is_empty() {
                return Err(Error::validation("Selecione ao menos um item para exportar."));
            }
        }
        let selected_ids: HashSet<i64> = items.iter().map(|v| v.item_id).collect();
        let by_key: BTreeMap<ItemKey, &ItemView> = everything.iter().map(|v| (v.key(), v)).collect();
        let in_doc: BTreeSet<ItemKey> = items.iter().map(|v| v.key()).collect();
        let anchor = |v: &ItemView| v.code().to_lowercase();

        let entry = |v: &ItemView| -> Json {
            let d = &v.data;
            let mut e = json!({
                "anchor": anchor(v),
                "code": v.code(),
                "title": d.title,
                "priority": d.priority.value(),
                "priority_label": d.priority.label(),
                "difficulty": d.difficulty,
                "status": d.status.value(),
                "status_label": d.status.label(),
                "tags": d.tags,
                "has_body": !d.body.is_empty(),
                "body_html": to_html(&d.body),
                "is_addendum": v.is_addendum(),
                "original_code": v.parent_code(),
                "original": Json::Null,
                "addenda": [],
            });
            if v.is_addendum() {
                if let Some(o) = by_key.get(&(v.number, 0)) {
                    e["original"] = json!({
                        "anchor": if in_doc.contains(&o.key()) { Json::from(anchor(o)) } else { Json::Null },
                        "title": o.data.title,
                        "status_label": o.data.status.label(),
                        "archived": o.data.archived,
                        "body_html": if o.data.archived || o.data.body.is_empty() { Json::Null } else { Json::from(to_html(&o.data.body)) },
                    });
                }
            } else {
                e["addenda"] = items
                    .iter()
                    .filter(|a| a.number == v.number && a.is_addendum())
                    .map(|a| {
                        json!({
                            "anchor": if in_doc.contains(&a.key()) { Json::from(anchor(a)) } else { Json::Null },
                            "code": a.code(),
                            "title": a.data.title,
                            "status_label": a.data.status.label(),
                        })
                    })
                    .collect();
            }
            e
        };

        let mut groups = Vec::new();
        for t in [ItemType::Improvement, ItemType::Discussion, ItemType::Adjustment] {
            let mut members: Vec<&ItemView> = items.iter().copied().filter(|v| v.data.kind == t).collect();
            if members.is_empty() {
                continue;
            }
            members.sort_by_key(|v| sort_key(v));
            groups.push(json!({
                "type": t.value(),
                "plural": t.plural(),
                "entries": members.iter().map(|v| entry(v)).collect::<Vec<_>>(),
            }));
        }

        let mut diff_json = Json::Null;
        let mut diff_revisions = Vec::new();
        if opts.changes && rev > 0 {
            let base = opts.compare_to.unwrap_or(rev - 1);
            if !(0..rev).contains(&base) {
                return Err(Error::validation("A revisão de comparação deve ser anterior à exportada."));
            }
            let mut diff = items_svc.diff(project.id, base, rev, false)?;
            let mut revs: Vec<_> = items_svc
                .revisions(project.id)?
                .into_iter()
                .filter(|r| base < r.number && r.number <= rev)
                .collect();
            // Items archived at this revision are left out entirely.
            diff.removed.retain(|v| !archived_ids.contains(&v.item_id));
            if keys.is_some() {
                // Only the selected items: nothing about the others goes into the file.
                diff.added.retain(|v| selected_ids.contains(&v.item_id));
                diff.changed.retain(|c| selected_ids.contains(&c.new.item_id));
                diff.removed.clear();
            }
            let shown = |id: i64| !archived_ids.contains(&id) && (keys.is_none() || selected_ids.contains(&id));
            revs.sort_by_key(|r| r.number);
            for r in &revs {
                let lines: Vec<&str> =
                    r.changes.iter().filter(|c| shown(c.item_id)).map(|c| c.summary.as_str()).collect();
                if lines.is_empty() {
                    continue;
                }
                let summary = if lines.len() == r.changes.len() {
                    r.summary.clone()
                } else if lines.len() == 1 {
                    // Part of a block: say nothing about the items left out.
                    lines[0].to_string()
                } else {
                    format!("{} itens alterados", lines.len())
                };
                let lines: Vec<&str> = if lines.len() > 1 { lines } else { Vec::new() };
                diff_revisions.push(json!({
                    "number": r.number,
                    "date": format_date(&r.created_at),
                    "summary": summary,
                    "lines": lines,
                }));
            }
            diff_json = json!({
                "rev_from": diff.rev_from,
                "rev_to": diff.rev_to,
                "is_empty": diff.is_empty(),
                "added": diff.added.iter().map(|v| json!({"code": v.code(), "anchor": anchor(v), "title": v.data.title})).collect::<Vec<_>>(),
                "removed": diff.removed.iter().map(|v| json!({"code": v.code(), "title": v.data.title})).collect::<Vec<_>>(),
                "changed": diff.changed.iter().map(|c| json!({
                    "code": c.code(),
                    "anchor": anchor(&c.new),
                    "title": c.new.data.title,
                    "changes": c.changes.iter().map(|f| json!({
                        "field": if f.field == Field::Body { "body" } else { "other" },
                        "label": f.label(),
                        "old": crate::domain::display_value(&f.old),
                        "new": crate::domain::display_value(&f.new),
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            });
        }

        let html = render_template(json!({
            "project": {"code": project.code, "name": project.name, "description": project.description},
            "description_html": to_html(&project.description),
            "rev": rev,
            "latest": latest,
            "rev_date": format_date(&rev_date),
            "generated": self.g.now().format("%d/%m/%Y %H:%M").to_string(),
            "groups": groups,
            "total": items.len(),
            "total_in_rev": total_in_rev,
            "diff": diff_json,
            "diff_revisions": diff_revisions,
            "highlight_css": highlight_css(),
        }))?;
        Ok(HtmlDocument { html, rev, rev_date })
    }

    /// Write the HTML file. With `mark_in_progress` the exported items whose status is
    /// Aberto/Em análise/Aprovado move to "Em desenvolvimento" (one revision for all, after the
    /// document is rendered: the file shows the list as it was exported). Only for the latest
    /// revision; the status changes and the file are all or nothing.
    ///
    /// `progress(done, total)` is called before the first status change and after each one.
    ///
    /// With `control_date` the exported items get today as their control date (not versioned:
    /// no revision, so any revision may be exported; skipped for an archived project, which is
    /// read-only). Also all or nothing with the file.
    pub fn export_html(&self, project_code: &str, opts: &HtmlOptions, mut progress: Progress<'_>) -> Result<ExportResult> {
        let project = self.g.projects().require(project_code)?;
        let items_svc = self.g.items();
        let latest = items_svc.latest_revision(project.id)?;
        if opts.mark_in_progress && opts.rev.is_some_and(|r| r != latest) {
            return Err(Error::validation(format!(
                "Só é possível marcar os itens como \"Em desenvolvimento\" ao exportar a última revisão (Rev. {latest})."
            )));
        }
        let doc = self.html(&project, opts)?;
        let day: String = doc.rev_date.chars().take(10).collect();
        let folder: PathBuf = match &opts.out_dir {
            Some(d) => d.clone(),
            None => self.g.settings().export_dir()?,
        };
        let path = folder.join(format!("giano_{}_rev{}_{day}.html", project.code, doc.rev));
        let mut marked: Vec<String> = Vec::new();
        let control_date = opts.control_date && project.active;
        let today = self.g.now().date().format("%Y-%m-%d").to_string();
        let mut exported: Vec<ItemView> = Vec::new();
        items_svc.batch("Exportação: itens → Em desenvolvimento", || {
            if opts.mark_in_progress || control_date {
                exported = items_svc
                    .list_items(project.id, Some(doc.rev), Archived::No)?
                    .into_iter()
                    .filter(|v| opts.keys.as_ref().map_or(true, |k| k.contains(&v.key())))
                    .collect();
            }
            if control_date {
                let ids: Vec<i64> = exported.iter().map(|v| v.item_id).collect();
                crate::db::items::set_control_date(&self.g.db, &ids, Some(&today))?;
            }
            if opts.mark_in_progress {
                let todo: Vec<&ItemView> = exported.iter().filter(|v| MARKABLE.contains(&v.data.status)).collect();
                if let Some(p) = progress.as_mut() {
                    p(0, todo.len());
                }
                for v in todo.iter().copied() {
                    let data = crate::domain::ItemData { status: ItemStatus::InProgress, ..v.data.clone() };
                    items_svc.update(v.item_id, &data)?;
                    marked.push(v.code());
                    if let Some(p) = progress.as_mut() {
                        p(marked.len(), todo.len());
                    }
                }
            }
            // Written inside the transaction: if the file fails, no status changes.
            write_file(&folder, &path, &doc.html)
        })?;
        let mut summary = format!("Rev. {} exportada para {}", doc.rev, path.display());
        if opts.mark_in_progress {
            let _ = write!(summary, "; {} itens marcados como Em desenvolvimento", marked.len());
            if !marked.is_empty() {
                let _ = write!(summary, " ({})", marked.join(", "));
            }
        }
        if control_date {
            let _ = write!(summary, "; data de controle {} em {} itens", format_date(&today), exported.len());
        }
        let control_dated = if control_date { exported.len() } else { 0 };
        Ok(ExportResult { path, content: doc.html, summary, marked, control_dated })
    }
}

fn write_file(folder: &Path, path: &Path, content: &str) -> Result<()> {
    fs::create_dir_all(folder).map_err(|e| {
        Error::validation(format!("Não foi possível criar a pasta {}: {e}", folder.display()))
    })?;
    fs::write(path, content)
        .map_err(|e| Error::validation(format!("Não foi possível gravar {}: {e}", path.display())))
}
