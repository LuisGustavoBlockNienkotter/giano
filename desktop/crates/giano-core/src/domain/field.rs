//! `Field` / `FieldValue`: the comparable fields of an item and how they are shown.

use serde::Serialize;

use super::{ItemStatus, ItemType, Priority};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Title,
    Type,
    Priority,
    Difficulty,
    Status,
    Tags,
    Body,
    Archived,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Title => "título",
            Field::Type => "tipo",
            Field::Priority => "prioridade",
            Field::Difficulty => "dificuldade",
            Field::Status => "status",
            Field::Tags => "tags",
            Field::Body => "corpo",
            Field::Archived => "arquivado",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Type(ItemType),
    Priority(Priority),
    Difficulty(i64),
    Status(ItemStatus),
    Tags(Vec<String>),
    Bool(bool),
}

/// How a value is shown to the user (`Alta`, `3/5`, `ux, api`, `sim`).
pub fn display_value(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(s) => s.clone(),
        FieldValue::Type(t) => t.label().into(),
        FieldValue::Priority(p) => p.label().into(),
        FieldValue::Status(s) => s.label().into(),
        FieldValue::Difficulty(d) => format!("{d}/5"),
        FieldValue::Tags(t) if t.is_empty() => "—".into(),
        FieldValue::Tags(t) => t.join(", "),
        FieldValue::Bool(b) => (if *b { "sim" } else { "não" }).into(),
    }
}
