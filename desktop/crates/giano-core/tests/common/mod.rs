#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use chrono::{Duration, NaiveDate, NaiveDateTime};
use giano_core::config::DbTarget;
use giano_core::domain::{ItemData, ItemStatus, ItemType, Priority, Project};
use giano_core::{Clock, Giano};
use tempfile::TempDir;

/// Deterministic clock: each call advances one minute, starting 2026-03-10 09:00.
pub fn fake_clock() -> Clock {
    let start: NaiveDateTime = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap().and_hms_opt(9, 0, 0).unwrap();
    let now = Arc::new(Mutex::new(start));
    Arc::new(move || {
        let mut n = now.lock().unwrap();
        let current = *n;
        *n += Duration::minutes(1);
        current
    })
}

pub struct Env {
    pub dir: TempDir,
    pub g: Giano,
}

impl std::ops::Deref for Env {
    type Target = Giano;
    fn deref(&self) -> &Giano {
        &self.g
    }
}

/// A fresh database in a temporary folder, exporting to `<tmp>/exports`.
pub fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let g = Giano::open(Some(&DbTarget::local(dir.path().join("test.db"))), fake_clock()).unwrap();
    g.settings().set_export_dir(Some(&dir.path().join("exports").to_string_lossy())).unwrap();
    Env { dir, g }
}

pub fn project(g: &Giano) -> Project {
    g.projects().create("MERC", "Mercurius", "Sistema de **gestão**.").unwrap()
}

pub fn make(title: &str) -> ItemData {
    ItemData::new(title)
}

/// Builder-style helpers for test data.
pub trait With {
    fn kind(self, t: ItemType) -> Self;
    fn priority(self, p: Priority) -> Self;
    fn difficulty(self, d: i64) -> Self;
    fn status(self, s: ItemStatus) -> Self;
    fn tags(self, t: &[&str]) -> Self;
    fn body(self, b: &str) -> Self;
}

impl With for ItemData {
    fn kind(mut self, t: ItemType) -> Self {
        self.kind = t;
        self
    }
    fn priority(mut self, p: Priority) -> Self {
        self.priority = p;
        self
    }
    fn difficulty(mut self, d: i64) -> Self {
        self.difficulty = d;
        self
    }
    fn status(mut self, s: ItemStatus) -> Self {
        self.status = s;
        self
    }
    fn tags(mut self, t: &[&str]) -> Self {
        self.tags = t.iter().map(|s| s.to_string()).collect();
        self
    }
    fn body(mut self, b: &str) -> Self {
        self.body = b.to_string();
        self
    }
}

pub fn err_msg<T: std::fmt::Debug>(r: giano_core::Result<T>) -> String {
    match r {
        Ok(v) => panic!("esperava erro, veio {v:?}"),
        Err(e) => e.to_string(),
    }
}
