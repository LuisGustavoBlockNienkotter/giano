//! `Note`: a free-text note (not versioned), optionally tied to a project.

use serde::Serialize;

use super::NoteStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub id: i64,
    pub note_date: String,
    pub name: String,
    pub project_id: Option<i64>,
    pub body: String,
    pub status: NoteStatus,
    pub created_at: String,
    pub updated_at: String,
    pub project_code: Option<String>,
}
