//! Full-text search across current items and notes (SQLite FTS5).

use serde::Serialize;

use crate::db::search as repo;
use crate::domain::{ItemStatus, NoteStatus};
use crate::error::Result;
use crate::services::Giano;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HitKind {
    Item,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub kind: HitKind,
    /// Item id or note id.
    pub id: i64,
    pub project_id: Option<i64>,
    /// `MERC-12` or `2026-11-10 · MERC`.
    pub label: String,
    pub title: String,
    /// User-facing label.
    pub status: String,
    pub archived: bool,
    pub snippet: String,
}

fn flat(s: Option<String>) -> String {
    s.unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct SearchService<'g> {
    pub(crate) g: &'g Giano,
}

impl<'g> SearchService<'g> {
    pub fn search(&self, text: &str, project_id: Option<i64>, limit: i64) -> Result<Vec<SearchHit>> {
        let mut hits = Vec::new();
        for r in repo::search_items(&self.g.db, text, project_id, limit)? {
            let status: String = r.get("status");
            let archived: bool = r.get("archived");
            let label = ItemStatus::from_value(&status).map_or(status.clone(), |s| s.label().to_string());
            hits.push(SearchHit {
                kind: HitKind::Item,
                id: r.get("item_id"),
                project_id: Some(r.get("project_id")),
                label: r.get("code"),
                title: r.get("title"),
                status: if archived { format!("{label} (arquivado)") } else { label },
                archived,
                snippet: flat(r.get("snippet")),
            });
        }
        for r in repo::search_notes(&self.g.db, text, project_id, limit)? {
            let date: String = r.get("note_date");
            let code: Option<String> = r.get("project_code");
            let status: String = r.get("status");
            hits.push(SearchHit {
                kind: HitKind::Note,
                id: r.get("note_id"),
                project_id: None,
                label: match code {
                    Some(c) => format!("{date} · {c}"),
                    None => date,
                },
                title: r.get("name"),
                status: NoteStatus::from_value(&status).map_or(status.clone(), |s| s.label().to_string()),
                archived: false,
                snippet: flat(r.get("snippet")),
            });
        }
        Ok(hits)
    }
}
