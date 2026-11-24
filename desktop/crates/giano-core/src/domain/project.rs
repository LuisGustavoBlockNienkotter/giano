//! `Project`: a list of items with its own code (`MERC`) and revision history.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
    pub active: bool,
    pub created_at: String,
    /// The git remote of the project's repository (shared, without credentials; "" = none).
    /// The local folder is per computer (`config::repo_path`).
    pub repo_remote: String,
}
