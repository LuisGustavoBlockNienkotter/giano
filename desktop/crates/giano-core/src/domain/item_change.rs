//! `ItemChange`: one item that changed between two revisions.

use serde::Serialize;

use super::{FieldChange, ItemView};

#[derive(Debug, Clone, Serialize)]
pub struct ItemChange {
    pub old: ItemView,
    pub new: ItemView,
    pub changes: Vec<FieldChange>,
}

impl ItemChange {
    pub fn code(&self) -> String {
        self.new.code()
    }
}
