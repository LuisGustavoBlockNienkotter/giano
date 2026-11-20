//! `RevisionDiff`: what was added, removed and changed between two revisions.

use serde::Serialize;

use super::{ItemChange, ItemView, Project};

#[derive(Debug, Clone, Serialize)]
pub struct RevisionDiff {
    pub project: Project,
    pub rev_from: i64,
    pub rev_to: i64,
    pub added: Vec<ItemView>,
    pub removed: Vec<ItemView>,
    pub changed: Vec<ItemChange>,
}

impl RevisionDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}
