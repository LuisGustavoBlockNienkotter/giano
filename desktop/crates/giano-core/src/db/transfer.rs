//! Copy every row of one Giano database into another, empty one (e.g. local file -> Turso).
//!
//! Both databases are already migrated to the same schema (`Database::open` migrates). Rows
//! keep their ids, so revisions, versions and tags stay linked. `note_fts` is filled by the
//! `notes` triggers; `item_fts` is copied (it is written by the repository).

use std::collections::BTreeMap;

use crate::db::{Database, Value};
use crate::error::Result;
use crate::params;

/// Parents before children (foreign keys).
pub const TABLES: [&str; 9] =
    ["projects", "notes", "items", "revisions", "revision_changes", "item_versions", "tags", "item_version_tags", "settings"];

/// Stay below SQLite's oldest limit (999 bound values per statement).
const MAX_PARAMS: usize = 900;

/// No user data (settings alone don't count: they are overwritten by the copy).
pub fn is_empty(db: &Database) -> Result<bool> {
    for t in TABLES.iter().filter(|t| **t != "settings") {
        if db.query_one(&format!("SELECT 1 FROM {t} LIMIT 1"), params![])?.is_some() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn columns(db: &Database, table: &str) -> Result<Vec<String>> {
    Ok(db.query(&format!("PRAGMA table_info({table})"), params![])?.iter().map(|r| r.get("name")).collect())
}

/// Multi-row INSERTs: one round trip per chunk instead of per row (matters on Turso).
fn insert_rows(dst: &Database, table: &str, columns: &[String], rows: &[Vec<Value>], verb: &str) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let per_chunk = (MAX_PARAMS / columns.len()).max(1);
    let cols = columns.join(", ");
    let one = format!("({})", vec!["?"; columns.len()].join(", "));
    for chunk in rows.chunks(per_chunk) {
        let values: Vec<Value> = chunk.iter().flatten().cloned().collect();
        let placeholders = vec![one.as_str(); chunk.len()].join(", ");
        dst.execute(&format!("{verb} INTO {table} ({cols}) VALUES {placeholders}"), &values)?;
    }
    Ok(())
}

/// Copy all rows from `src` to `dst` in one transaction. Returns rows per table.
pub fn copy_all(src: &Database, dst: &Database, skip_settings: &[&str]) -> Result<BTreeMap<String, usize>> {
    dst.transaction(|| {
        let mut counts = BTreeMap::new();
        for table in TABLES {
            let cols = columns(src, table)?;
            let mut rows: Vec<Vec<Value>> = src
                .query(&format!("SELECT {} FROM {table}", cols.join(", ")), params![])?
                .into_iter()
                .map(|r| r.into_values())
                .collect();
            if table == "settings" {
                let key_idx = cols.iter().position(|c| c == "key").unwrap_or(0);
                rows.retain(|r| match &r[key_idx] {
                    Value::Text(k) => !skip_settings.contains(&k.as_str()),
                    _ => true,
                });
            }
            let verb = if table == "settings" { "INSERT OR REPLACE" } else { "INSERT" };
            insert_rows(dst, table, &cols, &rows, verb)?;
            counts.insert(table.to_string(), rows.len());
        }
        let fts: Vec<Vec<Value>> = src
            .query("SELECT rowid, title, body, tags FROM item_fts", params![])?
            .into_iter()
            .map(|r| r.into_values())
            .collect();
        let fts_cols = ["rowid", "title", "body", "tags"].map(String::from);
        insert_rows(dst, "item_fts", &fts_cols, &fts, "INSERT")?;
        Ok(counts)
    })
}
