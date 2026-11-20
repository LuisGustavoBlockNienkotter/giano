//! `FieldChange` and `compare_items` (shared by revision summaries and diffs).

use std::collections::BTreeSet;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use super::field::{display_value, Field, FieldValue};
use super::ItemData;

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(limit - 1).collect();
        s.push('…');
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: Field,
    pub old: FieldValue,
    pub new: FieldValue,
}

impl FieldChange {
    pub fn label(&self) -> &'static str {
        self.field.label()
    }

    /// Short human description, e.g. `prioridade Alta → Média`.
    pub fn describe(&self) -> String {
        match (&self.field, &self.old, &self.new) {
            (Field::Body, _, _) => "corpo alterado".into(),
            (Field::Archived, _, FieldValue::Bool(true)) => "arquivado".into(),
            (Field::Archived, _, _) => "restaurado do arquivo".into(),
            (Field::Title, FieldValue::Text(a), FieldValue::Text(b)) => {
                format!("título \"{}\" → \"{}\"", shorten(a, 40), shorten(b, 40))
            }
            (Field::Tags, FieldValue::Tags(old), FieldValue::Tags(new)) => {
                let old: BTreeSet<&String> = old.iter().collect();
                let new: BTreeSet<&String> = new.iter().collect();
                let mut parts: Vec<String> = new.difference(&old).map(|t| format!("+{t}")).collect();
                parts.extend(old.difference(&new).map(|t| format!("−{t}")));
                format!("tags {}", parts.join(" "))
            }
            _ => format!("{} {} → {}", self.label(), display_value(&self.old), display_value(&self.new)),
        }
    }
}

impl Serialize for FieldChange {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("FieldChange", 5)?;
        st.serialize_field("field", &self.field)?;
        st.serialize_field("label", self.label())?;
        st.serialize_field("old", &display_value(&self.old))?;
        st.serialize_field("new", &display_value(&self.new))?;
        st.serialize_field("description", &self.describe())?;
        st.end()
    }
}

pub fn compare_items(old: &ItemData, new: &ItemData) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    let mut push = |field, a: FieldValue, b: FieldValue| {
        if a != b {
            changes.push(FieldChange { field, old: a, new: b });
        }
    };
    push(Field::Title, FieldValue::Text(old.title.clone()), FieldValue::Text(new.title.clone()));
    push(Field::Type, FieldValue::Type(old.kind), FieldValue::Type(new.kind));
    push(Field::Priority, FieldValue::Priority(old.priority), FieldValue::Priority(new.priority));
    push(Field::Difficulty, FieldValue::Difficulty(old.difficulty), FieldValue::Difficulty(new.difficulty));
    push(Field::Status, FieldValue::Status(old.status), FieldValue::Status(new.status));
    push(Field::Tags, FieldValue::Tags(old.tags.clone()), FieldValue::Tags(new.tags.clone()));
    push(Field::Body, FieldValue::Text(old.body.clone()), FieldValue::Text(new.body.clone()));
    push(Field::Archived, FieldValue::Bool(old.archived), FieldValue::Bool(new.archived));
    changes
}
