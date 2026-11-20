//! Repository: projects.

use crate::db::{Database, Row};
use crate::domain::Project;
use crate::error::Result;
use crate::params;

fn row(r: &Row) -> Project {
    Project {
        id: r.get("id"),
        code: r.get("code"),
        name: r.get("name"),
        description: r.get("description"),
        active: r.get("active"),
        created_at: r.get("created_at"),
    }
}

pub fn insert(db: &Database, code: &str, name: &str, description: &str, active: bool, now: &str) -> Result<i64> {
    Ok(db
        .execute(
            "INSERT INTO projects (code, name, description, active, created_at) VALUES (?, ?, ?, ?, ?)",
            params![code, name, description, active, now],
        )?
        .last_insert_rowid)
}

pub fn update(db: &Database, id: i64, code: &str, name: &str, description: &str, active: bool) -> Result<()> {
    db.execute(
        "UPDATE projects SET code = ?, name = ?, description = ?, active = ? WHERE id = ?",
        params![code, name, description, active, id],
    )?;
    Ok(())
}

pub fn get(db: &Database, id: i64) -> Result<Option<Project>> {
    Ok(db.query_one("SELECT * FROM projects WHERE id = ?", params![id])?.as_ref().map(row))
}

pub fn get_by_code(db: &Database, code: &str) -> Result<Option<Project>> {
    Ok(db
        .query_one("SELECT * FROM projects WHERE code = ? COLLATE NOCASE", params![code])?
        .as_ref()
        .map(row))
}

pub fn list_all(db: &Database, include_archived: bool) -> Result<Vec<Project>> {
    let mut sql = String::from("SELECT * FROM projects");
    if !include_archived {
        sql.push_str(" WHERE active = 1");
    }
    sql.push_str(" ORDER BY active DESC, code");
    Ok(db.query(&sql, params![])?.iter().map(row).collect())
}
