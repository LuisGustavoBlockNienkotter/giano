//! User settings.
//!
//! The Claude prompt is stored in the database (shared by every computer). The export folder
//! belongs to the computer: with a Turso database it is kept in the local `config.json`.

use std::path::PathBuf;

use crate::config::{self, default_export_dir, DEFAULT_CLAUDE_PROMPT};
use crate::db::settings as repo;
use crate::error::Result;
use crate::services::Giano;

pub const EXPORT_DIR: &str = "export_dir";
pub const CLAUDE_PROMPT: &str = "claude_prompt";

pub struct SettingsService<'g> {
    pub(crate) g: &'g Giano,
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => dirs::home_dir()
            .map(|h| h.join(rest.trim_start_matches(['/', '\\'])))
            .unwrap_or_else(|| PathBuf::from(path)),
        _ => PathBuf::from(path),
    }
}

impl<'g> SettingsService<'g> {
    pub fn export_dir(&self) -> Result<PathBuf> {
        let value = if self.g.is_remote() {
            config::load_config()?.get(config::EXPORT_DIR).cloned()
        } else {
            repo::get(&self.g.db, EXPORT_DIR)?
        };
        Ok(match value.filter(|v| !v.is_empty()) {
            Some(v) => expand(&v),
            None => default_export_dir(),
        })
    }

    /// `None` or `""` goes back to the default folder.
    pub fn set_export_dir(&self, value: Option<&str>) -> Result<()> {
        let value = value.map(str::trim).filter(|v| !v.is_empty());
        if self.g.is_remote() {
            config::save_config(&[(config::EXPORT_DIR, value)])?;
            return Ok(());
        }
        let db = &self.g.db;
        db.transaction(|| match value {
            None => repo::delete(db, EXPORT_DIR),
            Some(v) => repo::set(db, EXPORT_DIR, v),
        })
    }

    pub fn claude_prompt(&self) -> Result<String> {
        Ok(repo::get(&self.g.db, CLAUDE_PROMPT)?
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_CLAUDE_PROMPT.to_string()))
    }

    /// Empty (or the default text) goes back to the default.
    pub fn set_claude_prompt(&self, value: Option<&str>) -> Result<()> {
        let db = &self.g.db;
        db.transaction(|| match value {
            Some(v) if !v.trim().is_empty() && v.trim() != DEFAULT_CLAUDE_PROMPT.trim() => {
                repo::set(db, CLAUDE_PROMPT, v)
            }
            _ => repo::delete(db, CLAUDE_PROMPT),
        })
    }
}
