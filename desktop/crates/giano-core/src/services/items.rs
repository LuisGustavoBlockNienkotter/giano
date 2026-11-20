//! Item service: the revision rules of a project's list.
//!
//! Every change to a project's list (create, edit, delete, status/priority/tags) goes into a
//! revision, inside a single transaction, using append-only temporal versions:
//!
//! * the current version is closed with `rev_end = N`;
//! * the new content is inserted with `rev_start = N`;
//! * deleting only closes the current version.
//!
//! A revision may hold several changes (a block of changes = one revision):
//!
//! * an OPEN revision (`open_revision` ... `close_revision`) collects every change made to
//!   the project, from any screen or computer, until it is closed;
//! * a batch (`items.batch(summary, || ...)`) puts the changes of one operation that touches
//!   several items (status of several items, export marking items) into one revision;
//! * any other change creates a revision of its own.
//!
//! A revision that collects changes keeps only the NET change of each item: changing an item
//! again rewrites the version created by that revision (the only case where a version is
//! deleted: it was never part of a finished revision), and an item that goes back to what it
//! was leaves the revision. `revision_changes` has one line per item changed.
//!
//! Archiving is a versioned change too: the new version is a copy of the current content with
//! `archived = true` (restoring does the opposite). Archived items keep everything they had,
//! but they are read-only until restored and stay out of the normal list and the exports.
//!
//! Saving without a real change does not create a revision.
//!
//! Addenda: an addendum (`MERC-33.1`) is an item of its own, created from an existing item to
//! record a follow-up without touching the original. It keeps the original's number, has its
//! own content, status, revisions and archive state. The original cannot be deleted while it
//! has addenda.

use std::collections::{BTreeMap, HashSet};
use std::sync::MutexGuard;

use crate::db::items as repo;
use crate::db::projects as projects_repo;
use crate::domain::{
    compare_items, FieldChange, ItemChange, ItemData, ItemStatus, ItemView, Priority, Project, Revision,
    RevisionDiff,
};
use crate::error::{Error, Result};
use crate::services::Giano;

/// Which items of a revision: every item, only the normal list, only the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archived {
    Any,
    No,
    Only,
}

impl Archived {
    fn keeps(self, view: &ItemView) -> bool {
        match self {
            Archived::Any => true,
            Archived::No => !view.data.archived,
            Archived::Only => view.data.archived,
        }
    }
}

/// `progress(done, total)`; on Turso every change is several round trips.
pub type Progress<'p> = Option<&'p mut dyn FnMut(usize, usize)>;

pub struct ItemService<'g> {
    pub(crate) g: &'g Giano,
}

/// The revision that receives a change.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target {
    project_id: i64,
    number: i64,
    /// `None`: the row is written together with the first change.
    id: Option<i64>,
    /// An open revision or a batch: several changes, net per item.
    collecting: bool,
    is_open: bool,
}

/// State of an `ItemService::batch` in progress (kept in `Giano::batch`).
#[derive(Debug, Default)]
pub(crate) struct BatchState {
    summary: String,
    targets: Vec<Target>,
}

/// The summary of a revision: the given text, or its only change, or a count.
fn revision_summary(text: &str, lines: &[String]) -> String {
    if !text.is_empty() {
        text.to_string()
    } else if lines.len() == 1 {
        lines[0].clone()
    } else {
        format!("{} itens alterados", lines.len())
    }
}

fn created_line(view: &ItemView, title: &str) -> String {
    match view.parent_code() {
        Some(parent) => format!("{} criado (adendo de {parent}): {title}", view.code()),
        None => format!("{} criado: {title}", view.code()),
    }
}

fn change_line(view: &ItemView, changes: &[FieldChange]) -> String {
    let described: Vec<String> = changes.iter().map(|c| c.describe()).collect();
    format!("{}: {}", view.code(), described.join("; "))
}

fn not_found() -> Error {
    Error::validation("Item não encontrado ou excluído.")
}

fn archived_read_only(view: &ItemView) -> Error {
    Error::validation(format!("{} está arquivado: restaure-o para a lista antes de editar.", view.code()))
}

impl<'g> ItemService<'g> {
    fn db(&self) -> &crate::db::Database {
        &self.g.db
    }

    fn project(&self, project_id: i64) -> Result<Project> {
        projects_repo::get(self.db(), project_id)?.ok_or_else(|| Error::validation("Projeto não encontrado."))
    }

    // -- revisions ----------------------------------------------------------

    pub fn latest_revision(&self, project_id: i64) -> Result<i64> {
        repo::latest_revision_number(self.db(), project_id)
    }

    /// All revisions of a project, newest first.
    pub fn revisions(&self, project_id: i64) -> Result<Vec<Revision>> {
        repo::list_revisions(self.db(), project_id)
    }

    pub fn revision(&self, project_id: i64, number: i64) -> Result<Option<Revision>> {
        repo::get_revision(self.db(), project_id, number)
    }

    /// The open revision of a project (always its latest), if any.
    pub fn get_open_revision(&self, project_id: i64) -> Result<Option<Revision>> {
        repo::get_open_revision(self.db(), project_id)
    }

    /// Open a revision: every change to the project goes into it (one revision for a whole
    /// block of changes) until `close_revision`.
    pub fn open_revision(&self, project_id: i64, summary: &str) -> Result<Revision> {
        self.db().transaction(|| {
            if !self.project(project_id)?.active {
                return Err(Error::validation("Projeto arquivado: somente leitura."));
            }
            let last = repo::last_revision(self.db(), project_id)?;
            if let Some(last) = last.filter(|l| l.is_open) {
                return Err(Error::validation(format!(
                    "A Rev. {} já está aberta: feche-a antes de abrir outra.",
                    last.number
                )));
            }
            let number = last.map_or(0, |l| l.number) + 1;
            repo::insert_revision(self.db(), project_id, number, &self.g.now_iso(), summary.trim(), true)?;
            repo::get_revision(self.db(), project_id, number)?.ok_or_else(not_found)
        })
    }

    /// Close the open revision (its date becomes the closing time). `summary` replaces the
    /// one given when opening; without any, the summary is the only change or a count. A
    /// revision that ended up with no change is removed (returns `None`).
    pub fn close_revision(&self, project_id: i64, summary: Option<&str>) -> Result<Option<Revision>> {
        self.db().transaction(|| {
            let last = repo::last_revision(self.db(), project_id)?
                .filter(|l| l.is_open)
                .ok_or_else(|| Error::validation("Não há revisão aberta neste projeto."))?;
            let rev = repo::get_revision(self.db(), project_id, last.number)?.ok_or_else(not_found)?;
            let lines: Vec<String> = rev.changes.iter().map(|c| c.summary.clone()).collect();
            if lines.is_empty() {
                repo::delete_revision(self.db(), last.id)?;
                return Ok(None);
            }
            let text = summary.map_or(rev.summary.clone(), |s| s.trim().to_string());
            let now = self.g.now_iso();
            repo::update_revision(self.db(), last.id, &revision_summary(&text, &lines), false, Some(&now))?;
            repo::get_revision(self.db(), project_id, last.number)
        })
    }

    /// Put every change made inside `f` into ONE revision per project, in one transaction (all
    /// or nothing). With an open revision the changes go into it. `summary` (also settable
    /// with `set_batch_summary`) is used when more than one item changes; a single change
    /// keeps its own description. Nested calls join the outer batch.
    pub fn batch<T>(&self, summary: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.batch_state().is_some() {
            return f();
        }
        self.db().transaction(|| {
            *self.batch_state() = Some(BatchState { summary: summary.to_string(), targets: Vec::new() });
            let result = f();
            let state = self.batch_state().take().unwrap_or_default();
            let value = result?;
            for t in state.targets.iter().filter(|t| !t.is_open) {
                let Some(id) = t.id else { continue };
                let lines = repo::change_lines(self.db(), id)?;
                if lines.is_empty() {
                    repo::delete_revision(self.db(), id)?;
                } else {
                    let text = if lines.len() > 1 { state.summary.as_str() } else { "" };
                    repo::update_revision(self.db(), id, &revision_summary(text, &lines), false, None)?;
                }
            }
            Ok(value)
        })
    }

    /// Change the summary of the batch in progress (no-op outside a batch).
    pub fn set_batch_summary(&self, summary: &str) {
        if let Some(state) = self.batch_state().as_mut() {
            state.summary = summary.to_string();
        }
    }

    fn batch_state(&self) -> MutexGuard<'_, Option<BatchState>> {
        self.g.batch.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The revision a change of `project_id` goes into (one query).
    fn target(&self, project_id: i64) -> Result<Target> {
        let known = self
            .batch_state()
            .as_ref()
            .and_then(|b| b.targets.iter().find(|t| t.project_id == project_id).copied());
        if let Some(t) = known {
            return Ok(t);
        }
        let last = repo::last_revision(self.db(), project_id)?;
        let mut batch = self.batch_state();
        let target = match last {
            Some(l) if l.is_open => {
                Target { project_id, number: l.number, id: Some(l.id), collecting: true, is_open: true }
            }
            _ => Target {
                project_id,
                number: last.map_or(0, |l| l.number) + 1,
                id: None,
                collecting: batch.is_some(),
                is_open: false,
            },
        };
        if let Some(b) = batch.as_mut() {
            b.targets.push(target);
        }
        Ok(target)
    }

    /// Record the net change of an item in `target` (`None`: no net change).
    fn record(&self, mut target: Target, item_id: i64, line: Option<&str>) -> Result<()> {
        let id = match target.id {
            Some(id) => id,
            None => {
                let Some(line) = line else { return Ok(()) };
                let summary = if target.collecting { "" } else { line };
                let id = repo::insert_revision(
                    self.db(),
                    target.project_id,
                    target.number,
                    &self.g.now_iso(),
                    summary,
                    false,
                )?;
                target.id = Some(id);
                if let Some(b) = self.batch_state().as_mut() {
                    if let Some(t) = b.targets.iter_mut().find(|t| t.project_id == target.project_id) {
                        *t = target;
                    }
                }
                id
            }
        };
        match line {
            None => repo::delete_change(self.db(), id, item_id),
            Some(line) => repo::set_change(self.db(), id, item_id, line),
        }
    }

    // -- queries ------------------------------------------------------------

    /// The list at revision `rev` (default: latest).
    pub fn list_items(&self, project_id: i64, rev: Option<i64>, archived: Archived) -> Result<Vec<ItemView>> {
        let rev = match rev {
            Some(r) => r,
            None => self.latest_revision(project_id)?,
        };
        let mut items = repo::list_at_revision(self.db(), project_id, rev)?;
        items.retain(|v| archived.keeps(v));
        Ok(items)
    }

    pub fn get(&self, item_id: i64, rev: Option<i64>) -> Result<Option<ItemView>> {
        match rev {
            None => repo::get_current(self.db(), item_id),
            Some(rev) => repo::get_at_revision(self.db(), item_id, rev),
        }
    }

    /// Find a current item by its identifier, e.g. `MERC-12` or `MERC-12.1`.
    pub fn get_by_code(&self, code: &str) -> Result<Option<ItemView>> {
        let code = code.trim();
        let Some((prefix, reference)) = code.rsplit_once('-') else { return Ok(None) };
        let (number, addendum) = reference.split_once('.').unwrap_or((reference, ""));
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if prefix.is_empty() || !digits(number) || (!addendum.is_empty() && !digits(addendum)) {
            return Ok(None);
        }
        let (Ok(number), Ok(addendum)) =
            (number.parse::<i64>(), if addendum.is_empty() { Ok(0) } else { addendum.parse::<i64>() })
        else {
            return Ok(None);
        };
        let Some(project) = projects_repo::get_by_code(self.db(), prefix)? else { return Ok(None) };
        repo::get_current_by_number(self.db(), project.id, number, addendum)
    }

    /// The original item of an addendum, at revision `rev` (default: latest).
    pub fn parent(&self, view: &ItemView, rev: Option<i64>) -> Result<Option<ItemView>> {
        if !view.is_addendum() {
            return Ok(None);
        }
        match rev {
            None => repo::get_current_by_number(self.db(), view.project_id, view.number, 0),
            Some(rev) => repo::get_by_number_at_revision(self.db(), view.project_id, view.number, 0, rev),
        }
    }

    /// The addenda of an item (archived ones included), at revision `rev`.
    pub fn addenda(&self, view: &ItemView, rev: Option<i64>) -> Result<Vec<ItemView>> {
        let rev = match rev {
            Some(r) => r,
            None => self.latest_revision(view.project_id)?,
        };
        repo::addenda_at_revision(self.db(), view.project_id, view.number, rev)
    }

    pub fn history(&self, item_id: i64) -> Result<Vec<ItemView>> {
        repo::item_history(self.db(), item_id)
    }

    pub fn all_tags(&self) -> Result<Vec<String>> {
        repo::all_tags(self.db())
    }

    // -- changes ------------------------------------------------------------

    pub fn create(&self, project_id: i64, data: &ItemData, source_note_id: Option<i64>) -> Result<ItemView> {
        let data = data.normalized()?;
        self.db().transaction(|| {
            let project = self.project(project_id)?;
            if !project.active {
                return Err(Error::validation("Projeto arquivado: não é possível criar itens."));
            }
            let target = self.target(project_id)?;
            let number = repo::next_item_number(self.db(), project_id)?;
            let item_id =
                repo::insert_item(self.db(), project_id, number, source_note_id, &self.g.now_iso(), 0)?;
            repo::insert_version(self.db(), item_id, target.number, &data)?;
            let line = format!("{}-{number} criado: {}", project.code, data.title);
            self.record(target, item_id, Some(&line))?;
            repo::get_current(self.db(), item_id)?.ok_or_else(not_found)
        })
    }

    /// Create an addendum of an item (`MERC-33` → `MERC-33.1`), one new revision. Asking for
    /// an addendum of an addendum attaches it to the same original (`MERC-33.1` → `MERC-33.2`).
    pub fn create_addendum(&self, item_id: i64, data: &ItemData, source_note_id: Option<i64>) -> Result<ItemView> {
        let data = data.normalized()?;
        self.db().transaction(|| {
            let base = repo::get_current(self.db(), item_id)?.ok_or_else(not_found)?;
            let project = self.project(base.project_id)?;
            if !project.active {
                return Err(Error::validation("Projeto arquivado: não é possível criar adendos."));
            }
            let original = if base.is_addendum() {
                repo::get_current_by_number(self.db(), base.project_id, base.number, 0)?
            } else {
                Some(base.clone())
            };
            original.ok_or_else(|| {
                Error::validation(format!(
                    "O item original {} foi excluído.",
                    base.parent_code().unwrap_or_default()
                ))
            })?;
            let target = self.target(base.project_id)?;
            let addendum = repo::next_addendum_number(self.db(), base.project_id, base.number)?;
            let new_id = repo::insert_item(
                self.db(),
                base.project_id,
                base.number,
                source_note_id,
                &self.g.now_iso(),
                addendum,
            )?;
            repo::insert_version(self.db(), new_id, target.number, &data)?;
            let new = repo::get_current(self.db(), new_id)?.ok_or_else(not_found)?;
            self.record(target, new_id, Some(&created_line(&new, &data.title)))?;
            Ok(new)
        })
    }

    /// Save new content. Returns the new view, or `None` if nothing changed.
    pub fn update(&self, item_id: i64, data: &ItemData) -> Result<Option<ItemView>> {
        let mut data = data.normalized()?;
        self.db().transaction(|| {
            let current = repo::get_current(self.db(), item_id)?.ok_or_else(not_found)?;
            if current.data.archived {
                return Err(archived_read_only(&current));
            }
            // Archiving has its own operation; an edit never archives or restores.
            data.archived = false;
            self.save(&current, &data)
        })
    }

    fn save(&self, current: &ItemView, data: &ItemData) -> Result<Option<ItemView>> {
        let changes = compare_items(&current.data, data);
        if changes.is_empty() {
            return Ok(None);
        }
        let target = self.target(current.project_id)?;
        let line = if current.rev_start == target.number {
            // Already changed in this revision (open revision or batch): keep only the net
            // change, rewriting the version this revision created.
            let before = repo::get_at_revision(self.db(), current.item_id, target.number - 1)?;
            repo::delete_version(self.db(), current.version_id)?;
            if let Some(before) = before.as_ref().filter(|b| compare_items(&b.data, data).is_empty()) {
                // Back to what it was: the item leaves the revision.
                repo::reopen_version(self.db(), before.version_id)?;
                self.record(target, current.item_id, None)?;
                return repo::get_current(self.db(), current.item_id);
            }
            repo::insert_version(self.db(), current.item_id, target.number, data)?;
            match &before {
                None => created_line(current, &data.title), // created in this revision
                Some(before) => change_line(current, &compare_items(&before.data, data)),
            }
        } else {
            repo::close_version(self.db(), current.version_id, target.number)?;
            repo::insert_version(self.db(), current.item_id, target.number, data)?;
            change_line(current, &changes)
        };
        self.record(target, current.item_id, Some(&line))?;
        repo::get_current(self.db(), current.item_id)
    }

    /// Move an item to the archive, keeping all its content (new revision).
    pub fn archive(&self, item_id: i64) -> Result<ItemView> {
        self.set_archived(item_id, true)
    }

    /// Bring an archived item back to the normal list (new revision).
    pub fn unarchive(&self, item_id: i64) -> Result<ItemView> {
        self.set_archived(item_id, false)
    }

    fn set_archived(&self, item_id: i64, archived: bool) -> Result<ItemView> {
        self.db().transaction(|| {
            let current = repo::get_current(self.db(), item_id)?.ok_or_else(not_found)?;
            if current.data.archived == archived {
                let state = if archived { "já está arquivado" } else { "não está arquivado" };
                return Err(Error::validation(format!("{} {state}.", current.code())));
            }
            if !self.project(current.project_id)?.active {
                return Err(Error::validation("Projeto arquivado: somente leitura."));
            }
            let data = ItemData { archived, ..current.data.clone() };
            self.save(&current, &data)?.ok_or_else(not_found)
        })
    }

    fn require(&self, item_id: i64) -> Result<ItemView> {
        repo::get_current(self.db(), item_id)?.ok_or_else(not_found)
    }

    pub fn set_status(&self, item_id: i64, status: ItemStatus) -> Result<Option<ItemView>> {
        let current = self.require(item_id)?;
        self.update(item_id, &ItemData { status, ..current.data })
    }

    pub fn set_priority(&self, item_id: i64, priority: Priority) -> Result<Option<ItemView>> {
        let current = self.require(item_id)?;
        self.update(item_id, &ItemData { priority, ..current.data })
    }

    pub fn set_tags<S: AsRef<str>>(&self, item_id: i64, tags: &[S]) -> Result<Option<ItemView>> {
        let current = self.require(item_id)?;
        let tags = tags.iter().map(|t| t.as_ref().to_string()).collect();
        self.update(item_id, &ItemData { tags, ..current.data })
    }

    /// Change the status of several items in ONE revision (per project; with an open revision,
    /// into it), skipping items already in `status`, all in one transaction: either every
    /// item changes or none does. Returns the new views, in the order of `item_ids`.
    ///
    /// `progress(done, total)` is called before the first change and after each one (`total`
    /// = items that change).
    pub fn set_status_many(
        &self,
        item_ids: &[i64],
        status: ItemStatus,
        mut progress: Progress<'_>,
    ) -> Result<Vec<ItemView>> {
        self.batch("", || {
            let mut seen = HashSet::new();
            let mut todo = Vec::new();
            for &item_id in item_ids {
                if !seen.insert(item_id) {
                    continue;
                }
                let current = self.require(item_id)?;
                if current.data.archived {
                    return Err(archived_read_only(&current));
                }
                if current.data.status != status {
                    todo.push(current);
                }
            }
            self.set_batch_summary(&format!("{} itens → {}", todo.len(), status.label()));
            if let Some(p) = progress.as_mut() {
                p(0, todo.len());
            }
            let mut changed = Vec::new();
            for current in &todo {
                // save() (not update) reuses the view just loaded: one query less per item.
                let data = ItemData { status, ..current.data.clone() }.normalized()?;
                if let Some(new) = self.save(current, &data)? {
                    changed.push(new);
                }
                if let Some(p) = progress.as_mut() {
                    p(changed.len(), todo.len());
                }
            }
            Ok(changed)
        })
    }

    /// Delete an item (closes its current version). Returns the revision number.
    pub fn delete(&self, item_id: i64) -> Result<i64> {
        self.db().transaction(|| {
            let current = repo::get_current(self.db(), item_id)?
                .ok_or_else(|| Error::validation("Item não encontrado ou já excluído."))?;
            if !current.is_addendum() {
                let rev_now = repo::latest_revision_number(self.db(), current.project_id)?;
                let addenda = repo::addenda_at_revision(self.db(), current.project_id, current.number, rev_now)?;
                if !addenda.is_empty() {
                    let codes: Vec<String> = addenda.iter().map(ItemView::code).collect();
                    return Err(Error::validation(format!(
                        "{} tem adendos ({}): exclua-os antes, ou arquive o item em vez de excluir.",
                        current.code(),
                        codes.join(", ")
                    )));
                }
            }
            let target = self.target(current.project_id)?;
            let line = if current.rev_start == target.number {
                // Changed (or created) in this revision: drop that version; the one it replaced
                // is already closed by this revision.
                let before = repo::get_at_revision(self.db(), item_id, target.number - 1)?;
                repo::delete_version(self.db(), current.version_id)?;
                before.map(|b| format!("{} excluído: {}", current.code(), b.data.title))
            } else {
                repo::close_version(self.db(), current.version_id, target.number)?;
                Some(format!("{} excluído: {}", current.code(), current.data.title))
            };
            self.record(target, item_id, line.as_deref())?;
            Ok(target.number)
        })
    }

    // -- diff ---------------------------------------------------------------

    /// Differences going from revision `rev_from` to `rev_to`.
    ///
    /// With `include_archived = false` archived items count as absent: archiving shows up as
    /// a removal and restoring as an addition (used by the HTML export).
    pub fn diff(&self, project_id: i64, rev_from: i64, rev_to: i64, include_archived: bool) -> Result<RevisionDiff> {
        let project = self.project(project_id)?;
        let latest = self.latest_revision(project_id)?;
        for r in [rev_from, rev_to] {
            if !(0..=latest).contains(&r) {
                return Err(Error::validation(format!("Revisão {r} não existe (última: {latest}).")));
            }
        }
        let only = if include_archived { Archived::Any } else { Archived::No };
        let before: BTreeMap<i64, ItemView> = self
            .list_items(project_id, Some(rev_from), only)?
            .into_iter()
            .map(|v| (v.item_id, v))
            .collect();
        let after = self.list_items(project_id, Some(rev_to), only)?;
        let after_ids: HashSet<i64> = after.iter().map(|v| v.item_id).collect();
        let mut result =
            RevisionDiff { project, rev_from, rev_to, added: vec![], removed: vec![], changed: vec![] };
        for view in after {
            match before.get(&view.item_id) {
                None => result.added.push(view),
                Some(old) if old.version_id != view.version_id => {
                    let changes = compare_items(&old.data, &view.data);
                    if !changes.is_empty() {
                        // changed and changed back = no difference
                        result.changed.push(ItemChange { old: old.clone(), new: view, changes });
                    }
                }
                Some(_) => {}
            }
        }
        result.removed = before.into_values().filter(|v| !after_ids.contains(&v.item_id)).collect();
        result.added.sort_by_key(ItemView::key);
        result.removed.sort_by_key(ItemView::key);
        result.changed.sort_by_key(|c| c.new.key());
        Ok(result)
    }
}
