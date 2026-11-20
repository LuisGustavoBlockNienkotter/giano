//! Repository: FTS5 full-text search over current items and notes.

use std::sync::OnceLock;

use regex::Regex;

use crate::db::{Database, Row, Value};
use crate::error::Result;

/// Turn free user text into a safe FTS5 query (AND of prefix terms).
pub fn fts_query(text: &str) -> Option<String> {
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN_RE.get_or_init(|| Regex::new(r"\w+").expect("valid regex"));
    let tokens: Vec<String> = re.find_iter(text).map(|m| format!("\"{}\"*", m.as_str())).collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

pub fn search_items(db: &Database, text: &str, project_id: Option<i64>, limit: i64) -> Result<Vec<Row>> {
    let Some(q) = fts_query(text) else { return Ok(Vec::new()) };
    let mut sql = String::from(
        "SELECT i.id AS item_id, p.id AS project_id,
                p.code || '-' || i.number
                    || CASE WHEN i.addendum > 0 THEN '.' || i.addendum ELSE '' END AS code,
                v.title, v.status, v.archived,
                snippet(item_fts, -1, '[', ']', '…', 12) AS snippet
         FROM item_fts
         JOIN item_versions v ON v.id = item_fts.rowid AND v.rev_end IS NULL
         JOIN items i ON i.id = v.item_id
         JOIN projects p ON p.id = i.project_id
         WHERE item_fts MATCH ?",
    );
    let mut params: Vec<Value> = vec![q.into()];
    if let Some(pid) = project_id {
        sql.push_str(" AND p.id = ?");
        params.push(pid.into());
    }
    sql.push_str(" ORDER BY bm25(item_fts) LIMIT ?");
    params.push(limit.into());
    db.query(&sql, &params)
}

pub fn search_notes(db: &Database, text: &str, project_id: Option<i64>, limit: i64) -> Result<Vec<Row>> {
    let Some(q) = fts_query(text) else { return Ok(Vec::new()) };
    let mut sql = String::from(
        "SELECT n.id AS note_id, n.note_date, n.name, n.status, p.code AS project_code,
                snippet(note_fts, -1, '[', ']', '…', 12) AS snippet
         FROM note_fts
         JOIN notes n ON n.id = note_fts.rowid
         LEFT JOIN projects p ON p.id = n.project_id
         WHERE note_fts MATCH ?",
    );
    let mut params: Vec<Value> = vec![q.into()];
    if let Some(pid) = project_id {
        sql.push_str(" AND n.project_id = ?");
        params.push(pid.into());
    }
    sql.push_str(" ORDER BY bm25(note_fts) LIMIT ?");
    params.push(limit.into());
    db.query(&sql, &params)
}
