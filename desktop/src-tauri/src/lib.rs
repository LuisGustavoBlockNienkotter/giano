//! Giano desktop (Tauri 2). A thin layer: every command calls `giano-core` on a blocking
//! thread and returns plain data; the interface lives in `../src` (Svelte).

mod commands;

use std::sync::Mutex;

use giano_core::config::{resolve_db_target, DbTarget};
use giano_core::{system_clock, Giano};

/// The open database. `None` when it could not be opened (e.g. Turso without internet): the
/// interface shows `error` and offers to retry or to use the local file.
pub struct AppState {
    pub giano: Mutex<Option<Giano>>,
    pub error: Mutex<Option<String>>,
}

impl AppState {
    fn open(target: Option<DbTarget>) -> (Option<Giano>, Option<String>) {
        let target = match target.map(Ok).unwrap_or_else(resolve_db_target) {
            Ok(t) => t,
            Err(e) => return (None, Some(e.to_string())),
        };
        match Giano::open(Some(&target), system_clock()) {
            Ok(g) => (Some(g), None),
            Err(e) => (None, Some(format!("Não foi possível abrir {}: {e}", target.location))),
        }
    }

    /// Replace the open database (`None` = the configured one).
    pub fn reopen(&self, target: Option<DbTarget>) {
        let (g, err) = AppState::open(target);
        *self.giano.lock().unwrap_or_else(|e| e.into_inner()) = g;
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = err;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (giano, error) = AppState::open(None);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { giano: Mutex::new(giano), error: Mutex::new(error) })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::reconnect,
            commands::use_local_database,
            commands::meta,
            commands::list_projects,
            commands::create_project,
            commands::update_project,
            commands::load_project,
            commands::get_item,
            commands::item_history,
            commands::item_markdown,
            commands::all_tags,
            commands::create_item,
            commands::create_addendum,
            commands::update_item,
            commands::delete_item,
            commands::archive_item,
            commands::unarchive_item,
            commands::set_status_many,
            commands::set_control_date,
            commands::list_revisions,
            commands::open_revision,
            commands::close_revision,
            commands::diff,
            commands::list_notes,
            commands::get_note,
            commands::create_note,
            commands::save_note,
            commands::set_note_status,
            commands::delete_note,
            commands::note_links,
            commands::create_item_from_note,
            commands::search,
            commands::render_markdown,
            commands::highlight_css,
            commands::export_html,
            commands::export_claude,
            commands::get_settings,
            commands::set_export_dir,
            commands::set_claude_prompt,
            commands::turso_connect,
            commands::turso_disconnect,
            commands::turso_push_local,
            commands::create_demo,
            commands::open_path,
            commands::reveal_path,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Giano");
}
