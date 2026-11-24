//! Giano core: domain, database (local SQLite file or Turso) and services.
//!
//! This is the Rust port of the Python backend (`src/giano/domain.py`, `db/`, `services/`).
//! It reads and writes the same database and the same `config.json`, so the Python TUI and
//! the desktop app can be used side by side. No UI code lives here.

pub mod claude_cli;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod git;
pub mod process;
pub mod services;

pub use error::{Error, Result};
pub use services::{system_clock, Clock, Giano};
