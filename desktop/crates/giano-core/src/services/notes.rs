//! Quick notes service. Notes are outside the versioned list: no revisions.

use chrono::NaiveDate;

use crate::db::notes::{self as repo, NoteFilter};
use crate::domain::{item_code, normalize_body, ItemData, ItemView, Note, NoteStatus};
use crate::error::{Error, Result};
use crate::services::Giano;

/// Validate a `AAAA-MM-DD` date. Accepts `DD/MM/AAAA` too.
pub fn parse_date(value: &str) -> Result<String> {
    let value = value.trim();
    for fmt in ["%Y-%m-%d", "%d/%m/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(value, fmt) {
            return Ok(d.format("%Y-%m-%d").to_string());
        }
    }
    Err(Error::validation(format!("Data inválida: '{value}'. Use AAAA-MM-DD.")))
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct NoteService<'g> {
    pub(crate) g: &'g Giano,
}

/// What the editor sends on every autosave.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NoteInput {
    pub name: String,
    pub note_date: String,
    pub project_id: Option<i64>,
    pub body: String,
    pub status: NoteStatus,
}

impl<'g> NoteService<'g> {
    pub fn default_name(&self) -> String {
        format!("Anotação {}", self.g.now().format("%d/%m %H:%M"))
    }

    fn today(&self) -> String {
        self.g.now().date().format("%Y-%m-%d").to_string()
    }

    pub fn create(&self, name: &str, project_id: Option<i64>, body: &str, note_date: Option<&str>) -> Result<Note> {
        let note_date = match note_date.filter(|d| !d.trim().is_empty()) {
            Some(d) => parse_date(d)?,
            None => self.today(),
        };
        let name = match collapse(name) {
            n if n.is_empty() => self.default_name(),
            n => n,
        };
        let db = &self.g.db;
        let id = db.transaction(|| repo::insert(db, &note_date, &name, project_id, body, &self.g.now_iso()))?;
        repo::get(db, id)?.ok_or_else(|| Error::validation("Anotação não encontrada."))
    }

    /// Autosave entry point: overwrite the note with the editor content. Body whitespace is
    /// preserved exactly while typing. Saving the same content changes nothing.
    pub fn save(&self, note_id: i64, input: &NoteInput) -> Result<Note> {
        let note_date = parse_date(&input.note_date)?;
        let name = match collapse(&input.name) {
            n if n.is_empty() => self.default_name(),
            n => n,
        };
        let db = &self.g.db;
        db.transaction(|| {
            let current = repo::get(db, note_id)?.ok_or_else(|| Error::validation("Anotação não encontrada."))?;
            if current.name == name
                && current.note_date == note_date
                && current.project_id == input.project_id
                && current.body == input.body
                && current.status == input.status
            {
                return Ok(());
            }
            repo::update(db, note_id, &note_date, &name, input.project_id, &input.body, input.status, &self.g.now_iso())
        })?;
        repo::get(db, note_id)?.ok_or_else(|| Error::validation("Anotação não encontrada."))
    }

    pub fn set_status(&self, note_id: i64, status: NoteStatus) -> Result<()> {
        let db = &self.g.db;
        db.transaction(|| repo::set_status(db, note_id, status, &self.g.now_iso()))
    }

    pub fn delete(&self, note_id: i64) -> Result<()> {
        let db = &self.g.db;
        db.transaction(|| repo::delete(db, note_id))
    }

    pub fn get(&self, note_id: i64) -> Result<Option<Note>> {
        repo::get(&self.g.db, note_id)
    }

    /// `since` accepts `AAAA-MM-DD` or `DD/MM/AAAA`.
    pub fn list(&self, filter: &NoteFilter) -> Result<Vec<Note>> {
        let mut f = filter.clone();
        if let Some(since) = f.since.as_deref().filter(|s| !s.trim().is_empty()) {
            f.since = Some(parse_date(since)?);
        } else {
            f.since = None;
        }
        repo::list_notes(&self.g.db, &f)
    }

    /// Codes of the items created from this note (`MERC-1`, `MERC-1.1`).
    pub fn linked_items(&self, note_id: i64) -> Result<Vec<String>> {
        Ok(repo::linked_items(&self.g.db, note_id)?
            .into_iter()
            .map(|(code, n, a)| item_code(&code, (n, a)))
            .collect())
    }

    /// Create an improvement item from a note; the item keeps the link.
    pub fn create_item(&self, note_id: i64, project_id: i64, data: &ItemData) -> Result<ItemView> {
        if self.get(note_id)?.is_none() {
            return Err(Error::validation("Anotação não encontrada."));
        }
        self.g.items().create(project_id, data, Some(note_id))
    }

    pub fn clean_body(body: &str) -> String {
        normalize_body(body)
    }
}
