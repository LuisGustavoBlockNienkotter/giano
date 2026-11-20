//! Domain model: enums, value objects and pure comparison logic.
//!
//! Identifiers are stored in English in the database; labels shown to the user are in
//! Portuguese (`.label()`). Same rules as `src/giano/domain.py`.
//!
//! One file per entity; everything is re-exported here, so callers keep using
//! `crate::domain::X`.

mod choice;
mod field;
mod field_change;
mod item_change;
mod item_data;
mod item_key;
mod item_status;
mod item_type;
mod item_view;
mod note;
mod note_status;
mod priority;
mod project;
mod ranges;
mod revision;
mod revision_diff;
mod text;

pub use choice::{parse_choice, Choice};
pub use field::{display_value, Field, FieldValue};
pub use field_change::{compare_items, FieldChange};
pub use item_change::ItemChange;
pub use item_data::{ItemData, DIFFICULTY_MAX, DIFFICULTY_MIN};
pub use item_key::{format_item_key, item_code, parse_item_keys, ItemKey};
pub use item_status::ItemStatus;
pub use item_type::ItemType;
pub use item_view::ItemView;
pub use note::Note;
pub use note_status::NoteStatus;
pub use priority::Priority;
pub use project::Project;
pub use ranges::{format_ranges, parse_ranges};
pub use revision::Revision;
pub use revision_diff::RevisionDiff;
pub use text::{normalize_body, normalize_code, normalize_tag, normalize_tags, split_tags};
