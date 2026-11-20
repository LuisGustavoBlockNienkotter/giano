//! Repository: key/value settings.

use crate::db::Database;
use crate::error::Result;
use crate::params;

pub fn get(db: &Database, key: &str) -> Result<Option<String>> {
    Ok(db.query_one("SELECT value FROM settings WHERE key = ?", params![key])?.map(|r| r.at(0)))
}

pub fn set(db: &Database, key: &str, value: &str) -> Result<()> {
    db.execute(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete(db: &Database, key: &str) -> Result<()> {
    db.execute("DELETE FROM settings WHERE key = ?", params![key])?;
    Ok(())
}
