//! `ItemView`: an item as it is at a given revision.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use super::item_key::{item_code, ItemKey};
use super::ItemData;

/// An item as it is at a given revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemView {
    pub item_id: i64,
    pub project_id: i64,
    pub project_code: String,
    pub number: i64,
    pub version_id: i64,
    pub rev_start: i64,
    pub rev_end: Option<i64>,
    pub data: ItemData,
    pub source_note_id: Option<i64>,
    /// 0 = a normal item; N > 0 = the N-th addendum of item `number` (MERC-33.1).
    pub addendum: i64,
    /// When the item (or addendum) was created, ISO date/time.
    pub created_at: String,
    /// The user's control date (ISO `YYYY-MM-DD`), not versioned: the same at every revision.
    /// Set by the HTML export (today) or by hand.
    pub control_date: Option<String>,
}

impl ItemView {
    /// `(number, addendum)`: identifies the item in its project and sorts addenda right
    /// after their original item.
    pub fn key(&self) -> ItemKey {
        (self.number, self.addendum)
    }

    pub fn code(&self) -> String {
        item_code(&self.project_code, self.key())
    }

    pub fn is_addendum(&self) -> bool {
        self.addendum > 0
    }

    /// Code of the original item of an addendum (`MERC-33` for `MERC-33.1`).
    pub fn parent_code(&self) -> Option<String> {
        self.is_addendum().then(|| format!("{}-{}", self.project_code, self.number))
    }
}

impl Serialize for ItemView {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("ItemView", 16)?;
        st.serialize_field("item_id", &self.item_id)?;
        st.serialize_field("project_id", &self.project_id)?;
        st.serialize_field("project_code", &self.project_code)?;
        st.serialize_field("number", &self.number)?;
        st.serialize_field("addendum", &self.addendum)?;
        st.serialize_field("version_id", &self.version_id)?;
        st.serialize_field("rev_start", &self.rev_start)?;
        st.serialize_field("rev_end", &self.rev_end)?;
        st.serialize_field("source_note_id", &self.source_note_id)?;
        st.serialize_field("created_at", &self.created_at)?;
        st.serialize_field("control_date", &self.control_date)?;
        st.serialize_field("data", &self.data)?;
        st.serialize_field("code", &self.code())?;
        st.serialize_field("key", &self.key())?;
        st.serialize_field("is_addendum", &self.is_addendum())?;
        st.serialize_field("parent_code", &self.parent_code())?;
        st.end()
    }
}
