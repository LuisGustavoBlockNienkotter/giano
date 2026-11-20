//! `ItemData`: the versioned content of an item.

use serde::{Deserialize, Serialize};

use super::text::{collapse_spaces, normalize_body, normalize_tags};
use super::{ItemStatus, ItemType, Priority};
use crate::error::{Error, Result};

pub const DIFFICULTY_MIN: i64 = 1;
pub const DIFFICULTY_MAX: i64 = 5;

/// The versioned content of an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemData {
    pub title: String,
    #[serde(rename = "type")]
    pub kind: ItemType,
    pub priority: Priority,
    pub difficulty: i64,
    pub status: ItemStatus,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub body: String,
    /// Archived items keep all their content but leave the normal list and the exports;
    /// archiving/restoring is a versioned change like any other.
    #[serde(default)]
    pub archived: bool,
}

impl ItemData {
    pub fn new(title: impl Into<String>) -> Self {
        ItemData {
            title: title.into(),
            kind: ItemType::Improvement,
            priority: Priority::Medium,
            difficulty: 3,
            status: ItemStatus::Open,
            tags: Vec::new(),
            body: String::new(),
            archived: false,
        }
    }

    pub fn normalized(&self) -> Result<ItemData> {
        let title = collapse_spaces(&self.title);
        if title.is_empty() {
            return Err(Error::validation("O título é obrigatório."));
        }
        if !(DIFFICULTY_MIN..=DIFFICULTY_MAX).contains(&self.difficulty) {
            return Err(Error::validation("Dificuldade deve ser um número de 1 a 5."));
        }
        Ok(ItemData {
            title,
            kind: self.kind,
            priority: self.priority,
            difficulty: self.difficulty,
            status: self.status,
            tags: normalize_tags(&self.tags),
            body: normalize_body(&self.body),
            archived: self.archived,
        })
    }
}
