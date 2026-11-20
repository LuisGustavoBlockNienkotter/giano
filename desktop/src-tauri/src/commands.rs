//! Tauri commands. Arguments arrive in camelCase from JavaScript (`projectId`) and are
//! snake_case here. Errors reach the interface as `{ kind, message }`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use giano_core::config::{self, DbTarget, DEFAULT_CLAUDE_PROMPT};
use giano_core::db::notes::NoteFilter;
use giano_core::domain::{
    Choice, ItemData, ItemStatus, ItemType, ItemView, Note, NoteStatus, Priority, Project, Revision, RevisionDiff,
};
use giano_core::services::exports::{ClaudeOptions, HtmlOptions};
use giano_core::services::formatting::{diff_markdown, item_preview};
use giano_core::services::items::Archived;
use giano_core::services::markdown;
use giano_core::services::notes::NoteInput;
use giano_core::services::search::SearchHit;
use giano_core::{system_clock, Giano};

use crate::AppState;

// ---------------------------------------------------------------------------
// Errors and the blocking runner
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CmdError {
    /// `validation` (refused, message for the user), `unavailable` (Turso offline),
    /// `conflict` (changed elsewhere), `closed` (no database), `internal`.
    kind: &'static str,
    message: String,
}

impl CmdError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        CmdError { kind, message: message.into() }
    }
}

impl From<giano_core::Error> for CmdError {
    fn from(e: giano_core::Error) -> Self {
        let kind = match &e {
            giano_core::Error::Validation(_) => "validation",
            giano_core::Error::Unavailable(_) => "unavailable",
            _ => "internal",
        };
        CmdError::new(kind, e.to_string())
    }
}

type CmdResult<T> = Result<T, CmdError>;

/// Run `f` with the open database on a blocking thread (SQLite and Turso calls block).
async fn run<T, F>(app: &AppHandle, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Giano) -> CmdResult<T> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let guard = state.giano.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(g) => f(g),
            None => Err(CmdError::new("closed", "Nenhum banco de dados aberto.")),
        }
    })
    .await
    .map_err(|e| CmdError::new("internal", e.to_string()))?
}

/// Like `run`, for the commands that change which database is open.
async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| CmdError::new("internal", e.to_string()))?
}

#[derive(Clone, Serialize)]
struct ProgressEvent {
    task: String,
    done: usize,
    total: usize,
}

fn emit_progress(app: &AppHandle, task: &str, done: usize, total: usize) {
    let _ = app.emit("progress", ProgressEvent { task: task.into(), done, total });
}

// ---------------------------------------------------------------------------
// Database / app status
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AppStatus {
    open: bool,
    error: Option<String>,
    location: Option<String>,
    remote: bool,
    /// Turso configured on this computer (even if it could not be reached).
    turso_configured: bool,
    turso_url: Option<String>,
    turso_source: String,
    local_path: String,
    config_path: String,
    version: &'static str,
}

fn status(state: &AppState) -> AppStatus {
    let guard = state.giano.lock().unwrap_or_else(|e| e.into_inner());
    let turso = config::turso_target().ok().flatten();
    AppStatus {
        open: guard.is_some(),
        error: state.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        location: guard.as_ref().map(|g| g.db_location().to_string()),
        remote: guard.as_ref().is_some_and(Giano::is_remote),
        turso_configured: turso.is_some(),
        turso_url: turso.map(|t| t.location),
        turso_source: config::turso_source(),
        local_path: config::default_db_path().display().to_string(),
        config_path: config::config_path().display().to_string(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
pub async fn app_status(app: AppHandle) -> CmdResult<AppStatus> {
    blocking(move || Ok(status(&app.state::<AppState>()))).await
}

/// Open the configured database again (after a network failure, for instance).
#[tauri::command]
pub async fn reconnect(app: AppHandle) -> CmdResult<AppStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.reopen(None);
        Ok(status(&state))
    })
    .await
}

/// Use the local file of this computer for this session (Turso stays configured).
#[tauri::command]
pub async fn use_local_database(app: AppHandle) -> CmdResult<AppStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.reopen(Some(DbTarget::local(config::default_db_path())));
        Ok(status(&state))
    })
    .await
}

#[derive(Serialize)]
pub struct ChoiceMeta {
    value: &'static str,
    label: &'static str,
    plural: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Meta {
    item_types: Vec<ChoiceMeta>,
    priorities: Vec<ChoiceMeta>,
    statuses: Vec<ChoiceMeta>,
    note_statuses: Vec<ChoiceMeta>,
}

fn choices<T: Choice>() -> Vec<ChoiceMeta> {
    T::ALL.iter().map(|c| ChoiceMeta { value: c.value(), label: c.label(), plural: None }).collect()
}

/// Enum values and their Portuguese labels (one source of truth: the core).
#[tauri::command]
pub fn meta() -> Meta {
    Meta {
        item_types: ItemType::ALL
            .iter()
            .map(|t| ChoiceMeta { value: t.value(), label: t.label(), plural: Some(t.plural()) })
            .collect(),
        priorities: choices::<Priority>(),
        statuses: choices::<ItemStatus>(),
        note_statuses: choices::<NoteStatus>(),
    }
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_projects(app: AppHandle) -> CmdResult<Vec<Project>> {
    run(&app, |g| Ok(g.projects().list(true)?)).await
}

#[tauri::command]
pub async fn create_project(app: AppHandle, code: String, name: String, description: String) -> CmdResult<Project> {
    run(&app, move |g| Ok(g.projects().create(&code, &name, &description)?)).await
}

#[tauri::command]
pub async fn update_project(
    app: AppHandle,
    id: i64,
    code: String,
    name: String,
    description: String,
    active: bool,
) -> CmdResult<Project> {
    run(&app, move |g| Ok(g.projects().update(id, &code, &name, &description, active)?)).await
}

#[derive(Serialize)]
pub struct ProjectView {
    project: Project,
    latest_rev: i64,
    rev: i64,
    revision: Option<Revision>,
    /// Every item at `rev`, archived ones included (the interface splits list/archive and
    /// finds originals/addenda here, without more round trips).
    items: Vec<ItemView>,
}

#[tauri::command]
pub async fn load_project(app: AppHandle, project_id: i64, rev: Option<i64>) -> CmdResult<ProjectView> {
    run(&app, move |g| {
        let project = g
            .projects()
            .get(project_id)?
            .ok_or_else(|| CmdError::new("validation", "Projeto não encontrado."))?;
        let items = g.items();
        let latest_rev = items.latest_revision(project_id)?;
        let rev = rev.filter(|r| (0..=latest_rev).contains(r)).unwrap_or(latest_rev);
        Ok(ProjectView {
            items: items.list_items(project_id, Some(rev), Archived::Any)?,
            revision: items.revision(project_id, rev)?,
            project,
            latest_rev,
            rev,
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_item(app: AppHandle, item_id: i64, rev: Option<i64>) -> CmdResult<Option<ItemView>> {
    run(&app, move |g| Ok(g.items().get(item_id, rev)?)).await
}

#[tauri::command]
pub async fn item_history(app: AppHandle, item_id: i64) -> CmdResult<Vec<ItemView>> {
    run(&app, move |g| Ok(g.items().history(item_id)?)).await
}

/// The item as Markdown (to copy), with its original/addenda and source note.
#[tauri::command]
pub async fn item_markdown(app: AppHandle, item_id: i64, rev: Option<i64>) -> CmdResult<String> {
    run(&app, move |g| {
        let items = g.items();
        let v = items.get(item_id, rev)?.ok_or_else(|| CmdError::new("validation", "Item não encontrado."))?;
        let note = match v.source_note_id {
            Some(id) => g.notes().get(id)?,
            None => None,
        };
        let original = items.parent(&v, rev)?;
        let addenda = if v.is_addendum() { vec![] } else { items.addenda(&v, rev)? };
        Ok(item_preview(&v, note.as_ref(), original.as_ref(), &addenda))
    })
    .await
}

#[tauri::command]
pub async fn all_tags(app: AppHandle) -> CmdResult<Vec<String>> {
    run(&app, |g| Ok(g.items().all_tags()?)).await
}

#[tauri::command]
pub async fn create_item(
    app: AppHandle,
    project_id: i64,
    data: ItemData,
    source_note_id: Option<i64>,
) -> CmdResult<ItemView> {
    run(&app, move |g| Ok(g.items().create(project_id, &data, source_note_id)?)).await
}

#[tauri::command]
pub async fn create_addendum(app: AppHandle, item_id: i64, data: ItemData) -> CmdResult<ItemView> {
    run(&app, move |g| Ok(g.items().create_addendum(item_id, &data, None)?)).await
}

#[derive(Serialize)]
pub struct UpdateResult {
    /// `None`: nothing changed, no revision was created.
    item: Option<ItemView>,
    /// What happened, ready to show: `Rev. N — MERC-3: status Aberto → Aprovado`.
    summary: Option<String>,
}

/// `Rev. N — <the change of this item in revision N>` (N may be the open revision).
fn change_message(g: &Giano, view: &ItemView) -> giano_core::Result<String> {
    let items = g.items();
    if let Some(open) = items.get_open_revision(view.project_id)? {
        if view.rev_start != open.number {
            // Changed back to what it was before the open revision: it left the revision.
            return Ok(format!("{}: voltou ao que era; fora da Rev. {} (em aberto).", view.code(), open.number));
        }
    }
    Ok(match items.revision(view.project_id, view.rev_start)? {
        Some(rev) => {
            let line = rev.changes.iter().find(|c| c.item_id == view.item_id).map_or(rev.title.as_str(), |c| c.summary.as_str());
            let state = if rev.is_open { " (em aberto)" } else { "" };
            format!("Rev. {}{state} — {line}", rev.number)
        }
        None => format!("Rev. {}", view.rev_start),
    })
}

/// Save an item. `expected_version_id` is the version the editor opened: if the item changed
/// elsewhere since then (the TUI, another computer), the save is refused with `conflict`
/// unless `force` is set.
#[tauri::command]
pub async fn update_item(
    app: AppHandle,
    item_id: i64,
    data: ItemData,
    expected_version_id: Option<i64>,
    force: bool,
) -> CmdResult<UpdateResult> {
    run(&app, move |g| {
        let items = g.items();
        if let (Some(expected), false) = (expected_version_id, force) {
            let current = items.get(item_id, None)?.ok_or_else(|| {
                CmdError::new("validation", "Item não encontrado ou excluído em outro lugar.")
            })?;
            if current.version_id != expected {
                return Err(CmdError::new(
                    "conflict",
                    format!(
                        "{} foi alterado em outro lugar (Rev. {}) depois que você abriu o editor.",
                        current.code(),
                        current.rev_start
                    ),
                ));
            }
        }
        let item = items.update(item_id, &data)?;
        let summary = match &item {
            Some(v) => Some(change_message(g, v)?),
            None => None,
        };
        Ok(UpdateResult { item, summary })
    })
    .await
}

#[tauri::command]
pub async fn delete_item(app: AppHandle, item_id: i64) -> CmdResult<i64> {
    run(&app, move |g| Ok(g.items().delete(item_id)?)).await
}

#[tauri::command]
pub async fn archive_item(app: AppHandle, item_id: i64) -> CmdResult<ItemView> {
    run(&app, move |g| Ok(g.items().archive(item_id)?)).await
}

#[tauri::command]
pub async fn unarchive_item(app: AppHandle, item_id: i64) -> CmdResult<ItemView> {
    run(&app, move |g| Ok(g.items().unarchive(item_id)?)).await
}

/// One revision for all the items that really change, all or nothing. Emits `progress` events.
#[tauri::command]
pub async fn set_status_many(app: AppHandle, item_ids: Vec<i64>, status: ItemStatus) -> CmdResult<Vec<ItemView>> {
    let events = app.clone();
    run(&app, move |g| {
        let mut progress = |done: usize, total: usize| emit_progress(&events, "status", done, total);
        Ok(g.items().set_status_many(&item_ids, status, Some(&mut progress))?)
    })
    .await
}

// ---------------------------------------------------------------------------
// Revisions
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_revisions(app: AppHandle, project_id: i64) -> CmdResult<Vec<Revision>> {
    run(&app, move |g| Ok(g.items().revisions(project_id)?)).await
}

/// Open a revision: every change to the project goes into it until `close_revision`.
#[tauri::command]
pub async fn open_revision(app: AppHandle, project_id: i64, summary: String) -> CmdResult<Revision> {
    run(&app, move |g| Ok(g.items().open_revision(project_id, &summary)?)).await
}

/// Close the open revision; `None` = it had no change and was discarded.
#[tauri::command]
pub async fn close_revision(app: AppHandle, project_id: i64, summary: Option<String>) -> CmdResult<Option<Revision>> {
    run(&app, move |g| Ok(g.items().close_revision(project_id, summary.as_deref())?)).await
}

#[derive(Serialize)]
pub struct DiffView {
    diff: RevisionDiff,
    markdown: String,
}

#[tauri::command]
pub async fn diff(app: AppHandle, project_id: i64, rev_from: i64, rev_to: i64) -> CmdResult<DiffView> {
    run(&app, move |g| {
        let diff = g.items().diff(project_id, rev_from, rev_to, true)?;
        let markdown = diff_markdown(&diff);
        Ok(DiffView { diff, markdown })
    })
    .await
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_notes(
    app: AppHandle,
    project_id: Option<i64>,
    statuses: Vec<NoteStatus>,
    include_unassigned: bool,
) -> CmdResult<Vec<Note>> {
    run(&app, move |g| {
        Ok(g.notes().list(&NoteFilter { project_id, statuses, include_unassigned, ..Default::default() })?)
    })
    .await
}

#[tauri::command]
pub async fn get_note(app: AppHandle, note_id: i64) -> CmdResult<Option<Note>> {
    run(&app, move |g| Ok(g.notes().get(note_id)?)).await
}

#[tauri::command]
pub async fn create_note(
    app: AppHandle,
    name: String,
    project_id: Option<i64>,
    body: String,
    note_date: Option<String>,
) -> CmdResult<Note> {
    run(&app, move |g| Ok(g.notes().create(&name, project_id, &body, note_date.as_deref())?)).await
}

/// Autosave.
#[tauri::command]
pub async fn save_note(app: AppHandle, note_id: i64, input: NoteInput) -> CmdResult<Note> {
    run(&app, move |g| Ok(g.notes().save(note_id, &input)?)).await
}

#[tauri::command]
pub async fn set_note_status(app: AppHandle, note_id: i64, status: NoteStatus) -> CmdResult<()> {
    run(&app, move |g| Ok(g.notes().set_status(note_id, status)?)).await
}

#[tauri::command]
pub async fn delete_note(app: AppHandle, note_id: i64) -> CmdResult<()> {
    run(&app, move |g| Ok(g.notes().delete(note_id)?)).await
}

#[tauri::command]
pub async fn note_links(app: AppHandle, note_id: i64) -> CmdResult<Vec<String>> {
    run(&app, move |g| Ok(g.notes().linked_items(note_id)?)).await
}

#[tauri::command]
pub async fn create_item_from_note(app: AppHandle, note_id: i64, project_id: i64, data: ItemData) -> CmdResult<ItemView> {
    run(&app, move |g| Ok(g.notes().create_item(note_id, project_id, &data)?)).await
}

// ---------------------------------------------------------------------------
// Search and Markdown
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn search(app: AppHandle, text: String, project_id: Option<i64>) -> CmdResult<Vec<SearchHit>> {
    run(&app, move |g| Ok(g.search().search(&text, project_id, 50)?)).await
}

/// Same renderer as the HTML export (raw HTML in the text is escaped).
#[tauri::command]
pub fn render_markdown(text: String) -> String {
    markdown::to_html(&text)
}

#[derive(Serialize)]
pub struct HighlightCss {
    light: String,
    dark: String,
}

#[tauri::command]
pub fn highlight_css() -> HighlightCss {
    HighlightCss { light: markdown::highlight_css(), dark: markdown::highlight_css_dark() }
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ExportOutcome {
    path: PathBuf,
    summary: String,
    marked: Vec<String>,
    /// The Markdown (Claude export only), to copy to the clipboard.
    content: Option<String>,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn export_html(
    app: AppHandle,
    project_code: String,
    rev: Option<i64>,
    changes: bool,
    compare_to: Option<i64>,
    keys: Option<Vec<(i64, i64)>>,
    mark_in_progress: bool,
) -> CmdResult<ExportOutcome> {
    let events = app.clone();
    run(&app, move |g| {
        let opts = HtmlOptions {
            rev,
            changes,
            compare_to,
            keys: keys.map(|k| k.into_iter().collect::<BTreeSet<_>>()),
            out_dir: None,
            mark_in_progress,
        };
        let mut progress = |done: usize, total: usize| emit_progress(&events, "export", done, total);
        let r = g.exports().export_html(&project_code, &opts, Some(&mut progress))?;
        Ok(ExportOutcome { path: r.path, summary: r.summary, marked: r.marked, content: None })
    })
    .await
}

#[tauri::command]
pub async fn export_claude(
    app: AppHandle,
    project_code: String,
    since: Option<String>,
    all_notes: bool,
    include_unassigned: bool,
    note_ids: Option<Vec<i64>>,
) -> CmdResult<ExportOutcome> {
    run(&app, move |g| {
        let opts = ClaudeOptions { since, all_notes, include_unassigned, note_ids, output: None };
        let r = g.exports().export_claude(&project_code, &opts)?;
        Ok(ExportOutcome { path: r.path, summary: r.summary, marked: vec![], content: Some(r.content) })
    })
    .await
}

// ---------------------------------------------------------------------------
// Settings and Turso
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SettingsView {
    export_dir: String,
    claude_prompt: String,
    default_claude_prompt: &'static str,
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> CmdResult<SettingsView> {
    run(&app, |g| {
        let s = g.settings();
        Ok(SettingsView {
            export_dir: s.export_dir()?.display().to_string(),
            claude_prompt: s.claude_prompt()?,
            default_claude_prompt: DEFAULT_CLAUDE_PROMPT,
        })
    })
    .await
}

#[tauri::command]
pub async fn set_export_dir(app: AppHandle, value: Option<String>) -> CmdResult<()> {
    run(&app, move |g| Ok(g.settings().set_export_dir(value.as_deref())?)).await
}

#[tauri::command]
pub async fn set_claude_prompt(app: AppHandle, value: Option<String>) -> CmdResult<()> {
    run(&app, move |g| Ok(g.settings().set_claude_prompt(value.as_deref())?)).await
}

#[derive(Serialize)]
pub struct TursoConnected {
    status: AppStatus,
    projects: usize,
    /// The Turso database is empty and this computer has a local file with data.
    can_push_local: bool,
}

/// Test the connection, save the URL and token in `config.json` and switch to Turso.
#[tauri::command]
pub async fn turso_connect(app: AppHandle, url: String, token: String) -> CmdResult<TursoConnected> {
    blocking(move || {
        let target = DbTarget::remote(&url, &token);
        if !target.is_remote() {
            return Err(CmdError::new("validation", "A URL deve começar com libsql:// (ou https://)."));
        }
        let g = Giano::open(Some(&target), system_clock())?;
        let projects = g.projects().list(true)?.len();
        let auth = target.auth_token.clone().unwrap_or_default();
        config::save_config(&[
            (config::TURSO_URL, Some(target.location.as_str())),
            (config::TURSO_TOKEN, Some(auth.as_str())),
        ])?;
        let local = config::default_db_path();
        let can_push_local = projects == 0 && local.exists();
        let state = app.state::<AppState>();
        *state.giano.lock().unwrap_or_else(|e| e.into_inner()) = Some(g);
        *state.error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(TursoConnected { status: status(&state), projects, can_push_local })
    })
    .await
}

/// Remove the URL and token from this computer and go back to the local file.
#[tauri::command]
pub async fn turso_disconnect(app: AppHandle) -> CmdResult<AppStatus> {
    blocking(move || {
        config::save_config(&[(config::TURSO_URL, None), (config::TURSO_TOKEN, None)])?;
        let state = app.state::<AppState>();
        state.reopen(None);
        Ok(status(&state))
    })
    .await
}

/// Copy the local file of this computer into the open Turso database (which must be empty).
#[tauri::command]
pub async fn turso_push_local(app: AppHandle) -> CmdResult<String> {
    run(&app, |g| {
        if !g.is_remote() {
            return Err(CmdError::new("validation", "O banco aberto não é o Turso."));
        }
        let source = config::default_db_path();
        if !source.exists() {
            return Err(CmdError::new("validation", format!("Arquivo não encontrado: {}", source.display())));
        }
        let local = Giano::open(Some(&DbTarget::local(&source)), system_clock())?;
        let counts = local.copy_to(g)?;
        let n = |t: &str| counts.get(t).copied().unwrap_or(0);
        Ok(format!(
            "Enviado para o Turso: {} projeto(s), {} item(ns), {} revisão(ões), {} anotação(ões).",
            n("projects"),
            n("items"),
            n("revisions"),
            n("notes")
        ))
    })
    .await
}

#[tauri::command]
pub async fn create_demo(app: AppHandle) -> CmdResult<Vec<String>> {
    run(&app, |g| Ok(giano_core::services::demo::create_demo(g)?)).await
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener().open_path(path, None::<&str>).map_err(|e| CmdError::new("validation", e.to_string()))
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(path).map_err(|e| CmdError::new("validation", e.to_string()))
}
