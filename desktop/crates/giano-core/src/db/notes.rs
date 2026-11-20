//! Repository: quick notes (not versioned).

use crate::db::{Database, Row, Value};
use crate::domain::{Note, NoteStatus};
use crate::error::Result;
use crate::params;

const SELECT: &str = "
SELECT n.*, p.code AS project_code
FROM notes n LEFT JOIN projects p ON p.id = n.project_id
";

fn row(r: &Row) -> Note {
    let status: String = r.get("status");
    Note {
        id: r.get("id"),
        note_date: r.get("note_date"),
        name: r.get("name"),
        project_id: r.get("project_id"),
        body: r.get("body"),
        status: NoteStatus::from_value(&status).unwrap_or(NoteStatus::Pending),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        project_code: r.get("project_code"),
    }
}

pub fn insert(
    db: &Database,
    note_date: &str,
    name: &str,
    project_id: Option<i64>,
    body: &str,
    now: &str,
) -> Result<i64> {
    Ok(db
        .execute(
            "INSERT INTO notes (note_date, name, project_id, body, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'pending', ?, ?)",
            params![note_date, name, project_id, body, now, now],
        )?
        .last_insert_rowid)
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    db: &Database,
    note_id: i64,
    note_date: &str,
    name: &str,
    project_id: Option<i64>,
    body: &str,
    status: NoteStatus,
    now: &str,
) -> Result<()> {
    db.execute(
        "UPDATE notes SET note_date = ?, name = ?, project_id = ?, body = ?, status = ?, updated_at = ?
         WHERE id = ?",
        params![note_date, name, project_id, body, status.value(), now, note_id],
    )?;
    Ok(())
}

pub fn set_status(db: &Database, note_id: i64, status: NoteStatus, now: &str) -> Result<()> {
    db.execute(
        "UPDATE notes SET status = ?, updated_at = ? WHERE id = ?",
        params![status.value(), now, note_id],
    )?;
    Ok(())
}

pub fn delete(db: &Database, note_id: i64) -> Result<()> {
    db.execute("DELETE FROM notes WHERE id = ?", params![note_id])?;
    Ok(())
}

pub fn get(db: &Database, note_id: i64) -> Result<Option<Note>> {
    let sql = format!("{SELECT} WHERE n.id = ?");
    Ok(db.query_one(&sql, params![note_id])?.as_ref().map(row))
}

#[derive(Debug, Clone, Default)]
pub struct NoteFilter {
    pub project_id: Option<i64>,
    pub statuses: Vec<NoteStatus>,
    pub since: Option<String>,
    pub include_unassigned: bool,
    pub oldest_first: bool,
}

pub fn list_notes(db: &Database, f: &NoteFilter) -> Result<Vec<Note>> {
    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();
    if let Some(pid) = f.project_id {
        wheres.push(if f.include_unassigned {
            "(n.project_id = ? OR n.project_id IS NULL)".into()
        } else {
            "n.project_id = ?".into()
        });
        params.push(pid.into());
    }
    if !f.statuses.is_empty() {
        let marks = vec!["?"; f.statuses.len()].join(", ");
        wheres.push(format!("n.status IN ({marks})"));
        params.extend(f.statuses.iter().map(|s| Value::from(s.value())));
    }
    if let Some(since) = &f.since {
        wheres.push("n.note_date >= ?".into());
        params.push(since.into());
    }
    let mut sql = SELECT.to_string();
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    let order = if f.oldest_first { "ASC" } else { "DESC" };
    sql.push_str(&format!(" ORDER BY n.note_date {order}, n.created_at {order}, n.id {order}"));
    Ok(db.query(&sql, &params)?.iter().map(row).collect())
}

/// (project code, item number, addendum) of items created from this note.
pub fn linked_items(db: &Database, note_id: i64) -> Result<Vec<(String, i64, i64)>> {
    let rows = db.query(
        "SELECT p.code, i.number, i.addendum FROM items i JOIN projects p ON p.id = i.project_id
         WHERE i.source_note_id = ? ORDER BY p.code, i.number, i.addendum",
        params![note_id],
    )?;
    Ok(rows.iter().map(|r| (r.at(0), r.at(1), r.at(2))).collect())
}
