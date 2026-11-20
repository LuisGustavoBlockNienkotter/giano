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
}
