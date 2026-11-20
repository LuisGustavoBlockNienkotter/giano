//! `Revision`: a numbered state of a project's list. One revision may change several items
//! (a block of changes); `changes` has one line per item, in the order they were first changed.

use serde::Serialize;

/// The net change of one item in a revision, e.g. `MERC-3: status Aberto → Concluído`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevisionChange {
    pub item_id: i64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Revision {
    pub project_id: i64,
    pub number: i64,
    pub created_at: String,
    pub summary: String,
    /// The latest revision, still collecting every change of the project until closed.
    pub is_open: bool,
    pub changes: Vec<RevisionChange>,
    /// The summary to show (an open revision may not have one yet). Filled by `Revision::new`.
    pub title: String,
}

impl Revision {
    pub fn new(
        project_id: i64,
        number: i64,
        created_at: String,
        summary: String,
        is_open: bool,
        changes: Vec<RevisionChange>,
    ) -> Self {
        let title = if !summary.is_empty() {
            summary.clone()
        } else if is_open {
            let n = changes.len();
            format!("Em aberto ({n} {})", if n == 1 { "alteração" } else { "alterações" })
        } else {
            String::new()
        };
        Revision { project_id, number, created_at, summary, is_open, changes, title }
    }

    pub fn item_ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.changes.iter().map(|c| c.item_id)
    }
}
