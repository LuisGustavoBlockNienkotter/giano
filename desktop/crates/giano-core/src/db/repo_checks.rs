//! Repository: repo_checks (one row per "Atualizar e verificar").

use crate::db::{Database, Row};
use crate::domain::{CheckResult, RepoCheck};
use crate::error::Result;
use crate::params;

fn row(r: &Row) -> RepoCheck {
    let result: String = r.get("result");
    RepoCheck {
        id: r.get("id"),
        project_id: r.get("project_id"),
        from_commit: r.get("from_commit"),
        to_commit: r.get("to_commit"),
        branch: r.get("branch"),
        commit_count: r.get("commit_count"),
        summary: r.get("summary"),
        // Written by this app only; an unreadable result shows as empty.
        result: serde_json::from_str::<CheckResult>(&result).unwrap_or_default(),
        created_at: r.get("created_at"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn insert(
    db: &Database,
    project_id: i64,
    from_commit: Option<&str>,
    to_commit: &str,
    branch: &str,
    commit_count: i64,
    summary: &str,
    result: &CheckResult,
    now: &str,
) -> Result<i64> {
    let json = serde_json::to_string(result).map_err(|e| crate::Error::Database(e.to_string()))?;
    Ok(db
        .execute(
            "INSERT INTO repo_checks (project_id, from_commit, to_commit, branch, commit_count, summary, result, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![project_id, from_commit, to_commit, branch, commit_count, summary, json, now],
        )?
        .last_insert_rowid)
}

pub fn get(db: &Database, id: i64) -> Result<Option<RepoCheck>> {
    Ok(db.query_one("SELECT * FROM repo_checks WHERE id = ?", params![id])?.as_ref().map(row))
}

/// Most recent first.
pub fn list(db: &Database, project_id: i64, limit: i64) -> Result<Vec<RepoCheck>> {
    Ok(db
        .query("SELECT * FROM repo_checks WHERE project_id = ? ORDER BY id DESC LIMIT ?", params![project_id, limit])?
        .iter()
        .map(row)
        .collect())
}

/// The last commit verified in a project (start of the next range).
pub fn last_commit(db: &Database, project_id: i64) -> Result<Option<(String, String)>> {
    Ok(db
        .query_one(
            "SELECT to_commit, created_at FROM repo_checks WHERE project_id = ? ORDER BY id DESC LIMIT 1",
            params![project_id],
        )?
        .map(|r| (r.get("to_commit"), r.get("created_at"))))
}
