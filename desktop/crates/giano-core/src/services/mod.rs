//! Service layer: business rules (revisions, notes, exports).
//!
//! [`Giano`] bundles the services over one database; the UI only talks to it, never to SQL:
//! `g.items().create(...)`, `g.notes().save(...)`, `g.exports().export_html(...)`.

pub mod demo;
pub mod exports;
pub mod formatting;
pub mod items;
pub mod markdown;
pub mod notes;
pub mod projects;
pub mod search;
pub mod settings;

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::NaiveDateTime;

use crate::config::{resolve_db_target, DbTarget};
use crate::db::{transfer, Database};
use crate::error::{Error, Result};

/// Source of "now" (local time). Tests use a fake one that advances one minute per call.
pub type Clock = Arc<dyn Fn() -> NaiveDateTime + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| chrono::Local::now().naive_local())
}

/// `2026-03-10T09:00:00` (Python's `isoformat(timespec="seconds")`).
pub fn iso(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub struct Giano {
    pub(crate) db: Database,
    pub(crate) clock: Clock,
}

impl Giano {
    pub fn new(db: Database, clock: Clock) -> Self {
        Giano { db, clock }
    }

    /// Open a database (`target`) or the configured one (`GIANO_DB`, Turso, the local
    /// default; see `config`).
    pub fn open(target: Option<&DbTarget>, clock: Clock) -> Result<Giano> {
        let target = match target {
            Some(t) => t.clone(),
            None => resolve_db_target()?,
        };
        let db = Database::open(&target.location, target.auth_token.as_deref())?;
        Ok(Giano::new(db, clock))
    }

    pub(crate) fn now(&self) -> NaiveDateTime {
        (self.clock)()
    }

    pub(crate) fn now_iso(&self) -> String {
        iso(self.now())
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    /// File path or Turso URL (never the token).
    pub fn db_location(&self) -> &str {
        self.db.location()
    }

    pub fn is_remote(&self) -> bool {
        self.db.is_remote()
    }

    pub fn projects(&self) -> projects::ProjectService<'_> {
        projects::ProjectService { g: self }
    }

    pub fn items(&self) -> items::ItemService<'_> {
        items::ItemService { g: self }
    }

    pub fn notes(&self) -> notes::NoteService<'_> {
        notes::NoteService { g: self }
    }

    pub fn settings(&self) -> settings::SettingsService<'_> {
        settings::SettingsService { g: self }
    }

    pub fn search(&self) -> search::SearchService<'_> {
        search::SearchService { g: self }
    }

    pub fn exports(&self) -> exports::Exporter<'_> {
        exports::Exporter { g: self }
    }

    /// Copy all data of this database into `other`, which must be empty (e.g. the local file
    /// into a new Turso database). Returns rows per table.
    pub fn copy_to(&self, other: &Giano) -> Result<BTreeMap<String, usize>> {
        if !transfer::is_empty(&other.db)? {
            return Err(Error::validation(format!(
                "O banco de destino ({}) já tem dados; a cópia só é feita para um banco vazio.",
                other.db_location()
            )));
        }
        // The export folder is a setting of each computer, not of the shared database.
        let skip: &[&str] = if other.is_remote() { &[settings::EXPORT_DIR] } else { &[] };
        transfer::copy_all(&self.db, &other.db, skip)
    }
}
