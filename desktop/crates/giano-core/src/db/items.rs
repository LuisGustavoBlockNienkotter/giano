//! Repository: items, append-only item versions, tags and revisions.
//!
//! This module only knows SQL. The revision rules (when to create a revision, summaries,
//! diffs) live in `services::items`.

use crate::db::{Database, Row, Value};
use crate::domain::{ItemData, ItemStatus, ItemType, ItemView, Priority, Revision, RevisionChange};
use crate::error::{Error, Result};
use crate::params;

// ---------------------------------------------------------------------------
// Revisions
// ---------------------------------------------------------------------------

pub fn latest_revision_number(db: &Database, project_id: i64) -> Result<i64> {
    let r = db.query_one("SELECT MAX(number) FROM revisions WHERE project_id = ?", params![project_id])?;
    Ok(r.and_then(|r| r.at::<Option<i64>>(0)).unwrap_or(0))
}

/// The latest revision of a project (an open revision is always the latest), one query.
#[derive(Debug, Clone, Copy)]
pub struct LastRevision {
    pub id: i64,
    pub number: i64,
    pub is_open: bool,
}

pub fn last_revision(db: &Database, project_id: i64) -> Result<Option<LastRevision>> {
    let r = db.query_one(
        "SELECT id, number, is_open FROM revisions WHERE project_id = ? ORDER BY number DESC LIMIT 1",
        params![project_id],
    )?;
    Ok(r.map(|r| LastRevision { id: r.get("id"), number: r.get("number"), is_open: r.get("is_open") }))
}

pub fn insert_revision(
    db: &Database,
    project_id: i64,
    number: i64,
    now: &str,
    summary: &str,
    is_open: bool,
) -> Result<i64> {
    Ok(db
        .execute(
            "INSERT INTO revisions (project_id, number, created_at, summary, is_open) VALUES (?, ?, ?, ?, ?)",
            params![project_id, number, now, summary, is_open],
        )?
        .last_insert_rowid)
}

pub fn update_revision(
    db: &Database,
    revision_id: i64,
    summary: &str,
    is_open: bool,
    created_at: Option<&str>,
) -> Result<()> {
    match created_at {
        None => db.execute(
            "UPDATE revisions SET summary = ?, is_open = ? WHERE id = ?",
            params![summary, is_open, revision_id],
        )?,
        Some(at) => db.execute(
            "UPDATE revisions SET summary = ?, is_open = ?, created_at = ? WHERE id = ?",
            params![summary, is_open, at, revision_id],
        )?,
    };
    Ok(())
}

/// Only for a revision that ended up with no change (no version points to it).
pub fn delete_revision(db: &Database, revision_id: i64) -> Result<()> {
    db.execute("DELETE FROM revision_changes WHERE revision_id = ?", params![revision_id])?;
    db.execute("DELETE FROM revisions WHERE id = ?", params![revision_id])?;
    Ok(())
}

/// Record (or replace) the net change of an item in a revision.
pub fn set_change(db: &Database, revision_id: i64, item_id: i64, summary: &str) -> Result<()> {
    db.execute(
        "INSERT INTO revision_changes (revision_id, item_id, summary) VALUES (?, ?, ?)
         ON CONFLICT (revision_id, item_id) DO UPDATE SET summary = excluded.summary",
        params![revision_id, item_id, summary],
    )?;
    Ok(())
}

pub fn delete_change(db: &Database, revision_id: i64, item_id: i64) -> Result<()> {
    db.execute(
        "DELETE FROM revision_changes WHERE revision_id = ? AND item_id = ?",
        params![revision_id, item_id],
    )?;
    Ok(())
}

pub fn change_lines(db: &Database, revision_id: i64) -> Result<Vec<String>> {
    let rows = db.query(
        "SELECT summary FROM revision_changes WHERE revision_id = ? ORDER BY id",
        params![revision_id],
    )?;
    Ok(rows.iter().map(|r| r.at(0)).collect())
}

const REVISION_SELECT: &str = "
SELECT r.project_id, r.number, r.created_at, r.summary, r.is_open,
       c.item_id AS change_item, c.summary AS change_summary
FROM revisions r LEFT JOIN revision_changes c ON c.revision_id = r.id";

/// Group the rows of revisions LEFT JOIN changes (ordered by revision, then change).
fn revisions(rows: &[Row]) -> Vec<Revision> {
    struct Acc {
        project_id: i64,
        number: i64,
        created_at: String,
        summary: String,
        is_open: bool,
        changes: Vec<RevisionChange>,
    }
    let mut out: Vec<Acc> = Vec::new();
    for r in rows {
        let number: i64 = r.get("number");
        if out.last().map_or(true, |a| a.number != number) {
            out.push(Acc {
                project_id: r.get("project_id"),
                number,
                created_at: r.get("created_at"),
                summary: r.get("summary"),
                is_open: r.get("is_open"),
                changes: Vec::new(),
            });
        }
        if let Some(item_id) = r.get::<Option<i64>>("change_item") {
            let acc = out.last_mut().expect("pushed above");
            acc.changes.push(RevisionChange { item_id, summary: r.get("change_summary") });
        }
    }
    out.into_iter()
        .map(|a| Revision::new(a.project_id, a.number, a.created_at, a.summary, a.is_open, a.changes))
        .collect()
}

/// All revisions of a project with their changes, newest first. One query.
pub fn list_revisions(db: &Database, project_id: i64) -> Result<Vec<Revision>> {
    let rows = db.query(
        &format!("{REVISION_SELECT} WHERE r.project_id = ? ORDER BY r.number DESC, c.id"),
        params![project_id],
    )?;
    Ok(revisions(&rows))
}

pub fn get_revision(db: &Database, project_id: i64, number: i64) -> Result<Option<Revision>> {
    let rows = db.query(
        &format!("{REVISION_SELECT} WHERE r.project_id = ? AND r.number = ? ORDER BY c.id"),
        params![project_id, number],
    )?;
    Ok(revisions(&rows).into_iter().next())
}

pub fn get_open_revision(db: &Database, project_id: i64) -> Result<Option<Revision>> {
    match last_revision(db, project_id)? {
        Some(last) if last.is_open => get_revision(db, project_id, last.number),
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Items and versions
// ---------------------------------------------------------------------------

pub fn next_item_number(db: &Database, project_id: i64) -> Result<i64> {
    let r = db.query_one("SELECT MAX(number) FROM items WHERE project_id = ?", params![project_id])?;
    Ok(r.and_then(|r| r.at::<Option<i64>>(0)).unwrap_or(0) + 1)
}

pub fn next_addendum_number(db: &Database, project_id: i64, number: i64) -> Result<i64> {
    let r = db.query_one(
        "SELECT MAX(addendum) FROM items WHERE project_id = ? AND number = ?",
        params![project_id, number],
    )?;
    Ok(r.and_then(|r| r.at::<Option<i64>>(0)).unwrap_or(0) + 1)
}

pub fn insert_item(
    db: &Database,
    project_id: i64,
    number: i64,
    source_note_id: Option<i64>,
    now: &str,
    addendum: i64,
) -> Result<i64> {
    Ok(db
        .execute(
            "INSERT INTO items (project_id, number, addendum, source_note_id, created_at)
             VALUES (?, ?, ?, ?, ?)",
            params![project_id, number, addendum, source_note_id, now],
        )?
        .last_insert_rowid)
}

fn tag_id(db: &Database, name: &str) -> Result<i64> {
    if let Some(r) = db.query_one("SELECT id FROM tags WHERE name = ?", params![name])? {
        return Ok(r.at(0));
    }
    Ok(db.execute("INSERT INTO tags (name) VALUES (?)", params![name])?.last_insert_rowid)
}

pub fn insert_version(db: &Database, item_id: i64, rev_start: i64, data: &ItemData) -> Result<i64> {
    let version_id = db
        .execute(
            "INSERT INTO item_versions
                 (item_id, rev_start, rev_end, title, type, priority, difficulty, status, body, archived)
             VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item_id,
                rev_start,
                data.title.as_str(),
                data.kind.value(),
                data.priority.value(),
                data.difficulty,
                data.status.value(),
                data.body.as_str(),
                data.archived
            ],
        )?
        .last_insert_rowid;
    for tag in &data.tags {
        let tid = tag_id(db, tag)?;
        db.execute(
            "INSERT INTO item_version_tags (version_id, tag_id) VALUES (?, ?)",
            params![version_id, tid],
        )?;
    }
    db.execute(
        "INSERT INTO item_fts (rowid, title, body, tags) VALUES (?, ?, ?, ?)",
        params![version_id, data.title.as_str(), data.body.as_str(), data.tags.join(" ")],
    )?;
    Ok(version_id)
}

/// Set (or clear, `None`) the control date of these items. Not versioned. One statement for all
/// of them (each statement is a round trip on Turso).
pub fn set_control_date(db: &Database, item_ids: &[i64], day: Option<&str>) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let marks = vec!["?"; item_ids.len()].join(", ");
    let mut values: Vec<Value> = vec![Value::from(day)];
    values.extend(item_ids.iter().map(|&id| Value::from(id)));
    db.execute(&format!("UPDATE items SET control_date = ? WHERE id IN ({marks})"), &values)?;
    Ok(())
}

pub fn close_version(db: &Database, version_id: i64, rev_end: i64) -> Result<()> {
    let res = db.execute(
        "UPDATE item_versions SET rev_end = ? WHERE id = ? AND rev_end IS NULL",
        params![rev_end, version_id],
    )?;
    if res.rows_affected != 1 {
        return Err(Error::Database(format!("Versão {version_id} não está vigente")));
    }
    Ok(())
}

/// Make a closed version current again (its successor, created in a revision that is still
/// collecting changes, was undone).
pub fn reopen_version(db: &Database, version_id: i64) -> Result<()> {
    db.execute("UPDATE item_versions SET rev_end = NULL WHERE id = ?", params![version_id])?;
    Ok(())
}

/// Remove a version created by a revision that is still collecting changes (an open revision
/// or a batch), when the same item changes again in that revision. Versions of finished
/// revisions are never deleted.
pub fn delete_version(db: &Database, version_id: i64) -> Result<()> {
    db.execute("DELETE FROM item_version_tags WHERE version_id = ?", params![version_id])?;
    db.execute("DELETE FROM item_fts WHERE rowid = ?", params![version_id])?;
    db.execute("DELETE FROM item_versions WHERE id = ?", params![version_id])?;
    Ok(())
}

const SELECT: &str = "
SELECT i.id AS item_id, i.project_id, p.code AS project_code, i.number, i.addendum,
       i.source_note_id, i.created_at, i.control_date,
       v.id AS version_id, v.rev_start, v.rev_end, v.title, v.type, v.priority,
       v.difficulty, v.status, v.body, v.archived,
       (SELECT group_concat(name, char(31)) FROM (
            SELECT t.name FROM item_version_tags vt JOIN tags t ON t.id = vt.tag_id
            WHERE vt.version_id = v.id ORDER BY t.name)) AS tags
FROM items i
JOIN projects p ON p.id = i.project_id
JOIN item_versions v ON v.item_id = i.id
";

/// `?1` = the revision.
const AT_REVISION: &str = "v.rev_start <= ?1 AND (v.rev_end IS NULL OR v.rev_end > ?1)";

fn view(r: &Row) -> ItemView {
    let tags: Option<String> = r.get("tags");
    let mut tags: Vec<String> = match tags {
        Some(t) if !t.is_empty() => t.split('\u{1f}').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    tags.sort();
    let kind: String = r.get("type");
    let priority: String = r.get("priority");
    let status: String = r.get("status");
    ItemView {
        item_id: r.get("item_id"),
        project_id: r.get("project_id"),
        project_code: r.get("project_code"),
        number: r.get("number"),
        version_id: r.get("version_id"),
        rev_start: r.get("rev_start"),
        rev_end: r.get("rev_end"),
        source_note_id: r.get("source_note_id"),
        addendum: r.get("addendum"),
        created_at: r.get("created_at"),
        control_date: r.get("control_date"),
        data: ItemData {
            title: r.get("title"),
            kind: ItemType::from_value(&kind).unwrap_or(ItemType::Improvement),
            priority: Priority::from_value(&priority).unwrap_or(Priority::Medium),
            difficulty: r.get("difficulty"),
            status: ItemStatus::from_value(&status).unwrap_or(ItemStatus::Open),
            tags,
            body: r.get("body"),
            archived: r.get("archived"),
        },
    }
}

fn views(rows: Vec<Row>) -> Vec<ItemView> {
    rows.iter().map(view).collect()
}

pub fn list_at_revision(db: &Database, project_id: i64, rev: i64) -> Result<Vec<ItemView>> {
    let sql = format!("{SELECT} WHERE i.project_id = ?2 AND {AT_REVISION} ORDER BY i.number, i.addendum");
    Ok(views(db.query(&sql, params![rev, project_id])?))
}

pub fn get_current(db: &Database, item_id: i64) -> Result<Option<ItemView>> {
    let sql = format!("{SELECT} WHERE i.id = ? AND v.rev_end IS NULL");
    Ok(db.query_one(&sql, params![item_id])?.as_ref().map(view))
}

pub fn get_at_revision(db: &Database, item_id: i64, rev: i64) -> Result<Option<ItemView>> {
    let sql = format!("{SELECT} WHERE i.id = ?2 AND {AT_REVISION}");
    Ok(db.query_one(&sql, params![rev, item_id])?.as_ref().map(view))
}

pub fn get_current_by_number(
    db: &Database,
    project_id: i64,
    number: i64,
    addendum: i64,
) -> Result<Option<ItemView>> {
    let sql = format!(
        "{SELECT} WHERE i.project_id = ? AND i.number = ? AND i.addendum = ? AND v.rev_end IS NULL"
    );
    Ok(db.query_one(&sql, params![project_id, number, addendum])?.as_ref().map(view))
}

pub fn get_by_number_at_revision(
    db: &Database,
    project_id: i64,
    number: i64,
    addendum: i64,
    rev: i64,
) -> Result<Option<ItemView>> {
    let sql = format!(
        "{SELECT} WHERE i.project_id = ?2 AND i.number = ?3 AND i.addendum = ?4 AND {AT_REVISION}"
    );
    Ok(db.query_one(&sql, params![rev, project_id, number, addendum])?.as_ref().map(view))
}

/// The addenda of item `number` (`number.1`, `number.2`…) at revision `rev`.
pub fn addenda_at_revision(db: &Database, project_id: i64, number: i64, rev: i64) -> Result<Vec<ItemView>> {
    let sql = format!(
        "{SELECT} WHERE i.project_id = ?2 AND i.number = ?3 AND i.addendum > 0 AND {AT_REVISION}
         ORDER BY i.addendum"
    );
    Ok(views(db.query(&sql, params![rev, project_id, number])?))
}

pub fn item_history(db: &Database, item_id: i64) -> Result<Vec<ItemView>> {
    let sql = format!("{SELECT} WHERE i.id = ? ORDER BY v.rev_start");
    Ok(views(db.query(&sql, params![item_id])?))
}

/// Tags used by at least one current version (for autocomplete).
pub fn all_tags(db: &Database) -> Result<Vec<String>> {
    let rows = db.query(
        "SELECT DISTINCT t.name FROM tags t
         JOIN item_version_tags vt ON vt.tag_id = t.id
         JOIN item_versions v ON v.id = vt.version_id
         WHERE v.rev_end IS NULL
         ORDER BY t.name",
        params![],
    )?;
    Ok(rows.iter().map(|r| r.at(0)).collect())
}
