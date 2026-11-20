//! Item service: the revision rules of a project's list.
//!
//! Every change to a project's list (create, edit, delete, status/priority/tags) produces
//! exactly one new revision, inside a single transaction, using append-only temporal
//! versions:
//!
//! * the current version is closed with `rev_end = N + 1`;
//! * the new content is inserted with `rev_start = N + 1`;
//! * deleting only closes the current version.
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

use crate::db::items as repo;
use crate::db::projects as projects_repo;
use crate::domain::{
    compare_items, ItemChange, ItemData, ItemStatus, ItemView, Priority, Project, Revision,
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

    fn new_revision(&self, project_id: i64, summary: &str, item_id: Option<i64>) -> Result<i64> {
        let number = repo::latest_revision_number(self.db(), project_id)? + 1;
        repo::insert_revision(self.db(), project_id, number, &self.g.now_iso(), summary, item_id)?;
        Ok(number)
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
            let number = repo::next_item_number(self.db(), project_id)?;
            let item_id =
                repo::insert_item(self.db(), project_id, number, source_note_id, &self.g.now_iso(), 0)?;
            let code = format!("{}-{number}", project.code);
            let rev = self.new_revision(project_id, &format!("{code} criado: {}", data.title), Some(item_id))?;
            repo::insert_version(self.db(), item_id, rev, &data)?;
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
            let original = original.ok_or_else(|| {
                Error::validation(format!(
                    "O item original {} foi excluído.",
                    base.parent_code().unwrap_or_default()
                ))
            })?;
            let addendum = repo::next_addendum_number(self.db(), base.project_id, base.number)?;
            let new_id = repo::insert_item(
                self.db(),
                base.project_id,
                base.number,
                source_note_id,
                &self.g.now_iso(),
                addendum,
            )?;
            let code = format!("{}.{addendum}", original.code());
            let summary = format!("{code} criado (adendo de {}): {}", original.code(), data.title);
            let rev = self.new_revision(base.project_id, &summary, Some(new_id))?;
            repo::insert_version(self.db(), new_id, rev, &data)?;
            repo::get_current(self.db(), new_id)?.ok_or_else(not_found)
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
        let described: Vec<String> = changes.iter().map(|c| c.describe()).collect();
        let summary = format!("{}: {}", current.code(), described.join("; "));
        let rev = self.new_revision(current.project_id, &summary, Some(current.item_id))?;
        repo::close_version(self.db(), current.version_id, rev)?;
        repo::insert_version(self.db(), current.item_id, rev, data)?;
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

    /// Change the status of several items: one revision per item that really changes (items
    /// already in `status` are skipped), all in one transaction, so either every item changes
    /// or none does. Returns the new views, in the order of `item_ids`.
    ///
    /// `progress(done, total)` is called before the first change and after each one (`total`
    /// = items that change).
    pub fn set_status_many(
        &self,
        item_ids: &[i64],
        status: ItemStatus,
        mut progress: Progress<'_>,
    ) -> Result<Vec<ItemView>> {
        self.db().transaction(|| {
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

    /// Delete an item (closes its current version). Returns the new revision number.
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
            let summary = format!("{} excluído: {}", current.code(), current.data.title);
            let rev = self.new_revision(current.project_id, &summary, Some(item_id))?;
            repo::close_version(self.db(), current.version_id, rev)?;
            Ok(rev)
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
