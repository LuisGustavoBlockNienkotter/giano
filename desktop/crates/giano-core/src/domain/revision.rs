//! `Revision`: one change to a project (numbered sequentially per project).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Revision {
    pub project_id: i64,
    pub number: i64,
    pub created_at: String,
    pub summary: String,
    /// The item changed by this revision.
    pub item_id: Option<i64>,
}
