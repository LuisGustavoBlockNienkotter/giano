// Typed wrappers around the Tauri commands (src-tauri/src/commands.rs).
// Tauri converts camelCase arguments to the snake_case Rust parameters.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  CmdError,
  DiffView,
  ExportOutcome,
  ItemData,
  ItemView,
  Meta,
  Note,
  NoteInput,
  NoteStatusValue,
  Project,
  ProjectView,
  Revision,
  SearchHit,
  SettingsView,
  StatusValue,
  TursoConnected,
  UpdateResult,
} from "./types";

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as CmdError).message);
  return String(e);
}

export function isConflict(e: unknown): boolean {
  return !!e && typeof e === "object" && (e as CmdError).kind === "conflict";
}

export const api = {
  appStatus: () => invoke<AppStatus>("app_status"),
  reconnect: () => invoke<AppStatus>("reconnect"),
  useLocalDatabase: () => invoke<AppStatus>("use_local_database"),
  meta: () => invoke<Meta>("meta"),

  listProjects: () => invoke<Project[]>("list_projects"),
  createProject: (code: string, name: string, description: string) =>
    invoke<Project>("create_project", { code, name, description }),
  updateProject: (id: number, code: string, name: string, description: string, active: boolean) =>
    invoke<Project>("update_project", { id, code, name, description, active }),
  loadProject: (projectId: number, rev: number | null) => invoke<ProjectView>("load_project", { projectId, rev }),

  getItem: (itemId: number, rev: number | null = null) => invoke<ItemView | null>("get_item", { itemId, rev }),
  itemHistory: (itemId: number) => invoke<ItemView[]>("item_history", { itemId }),
  itemMarkdown: (itemId: number, rev: number | null) => invoke<string>("item_markdown", { itemId, rev }),
  allTags: () => invoke<string[]>("all_tags"),
  createItem: (projectId: number, data: ItemData, sourceNoteId: number | null = null) =>
    invoke<ItemView>("create_item", { projectId, data, sourceNoteId }),
  createAddendum: (itemId: number, data: ItemData) => invoke<ItemView>("create_addendum", { itemId, data }),
  updateItem: (itemId: number, data: ItemData, expectedVersionId: number | null, force = false) =>
    invoke<UpdateResult>("update_item", { itemId, data, expectedVersionId, force }),
  deleteItem: (itemId: number) => invoke<number>("delete_item", { itemId }),
  archiveItem: (itemId: number) => invoke<ItemView>("archive_item", { itemId }),
  unarchiveItem: (itemId: number) => invoke<ItemView>("unarchive_item", { itemId }),
  /** One revision for all the items that change, all or nothing. */
  setArchivedMany: (itemIds: number[], archived: boolean) =>
    invoke<ItemView[]>("set_archived_many", { itemIds, archived }),
  /** Not versioned: no revision. `day` = `YYYY-MM-DD`, or null to clear. */
  setControlDate: (itemIds: number[], day: string | null) => invoke<ItemView[]>("set_control_date", { itemIds, day }),
  setStatusMany: (itemIds: number[], status: StatusValue) =>
    invoke<ItemView[]>("set_status_many", { itemIds, status }),

  listRevisions: (projectId: number) => invoke<Revision[]>("list_revisions", { projectId }),
  openRevision: (projectId: number, summary: string) => invoke<Revision>("open_revision", { projectId, summary }),
  closeRevision: (projectId: number, summary: string | null) =>
    invoke<Revision | null>("close_revision", { projectId, summary }),
  diff: (projectId: number, revFrom: number, revTo: number) =>
    invoke<DiffView>("diff", { projectId, revFrom, revTo }),

  listNotes: (projectId: number | null, statuses: NoteStatusValue[], includeUnassigned: boolean) =>
    invoke<Note[]>("list_notes", { projectId, statuses, includeUnassigned }),
  getNote: (noteId: number) => invoke<Note | null>("get_note", { noteId }),
  createNote: (name: string, projectId: number | null, body = "", noteDate: string | null = null) =>
    invoke<Note>("create_note", { name, projectId, body, noteDate }),
  saveNote: (noteId: number, input: NoteInput) => invoke<Note>("save_note", { noteId, input }),
  setNoteStatus: (noteId: number, status: NoteStatusValue) => invoke<void>("set_note_status", { noteId, status }),
  deleteNote: (noteId: number) => invoke<void>("delete_note", { noteId }),
  noteLinks: (noteId: number) => invoke<string[]>("note_links", { noteId }),
  createItemFromNote: (noteId: number, projectId: number, data: ItemData) =>
    invoke<ItemView>("create_item_from_note", { noteId, projectId, data }),

  search: (text: string, projectId: number | null) => invoke<SearchHit[]>("search", { text, projectId }),
  renderMarkdown: (text: string) => invoke<string>("render_markdown", { text }),
  highlightCss: () => invoke<{ light: string; dark: string }>("highlight_css"),

  exportHtml: (args: {
    projectCode: string;
    rev: number | null;
    changes: boolean;
    compareTo: number | null;
    keys: [number, number][] | null;
    markInProgress: boolean;
    controlDate: boolean;
  }) => invoke<ExportOutcome>("export_html", args),
  exportClaude: (args: {
    projectCode: string;
    since: string | null;
    allNotes: boolean;
    includeUnassigned: boolean;
    noteIds: number[] | null;
  }) => invoke<ExportOutcome>("export_claude", args),

  getSettings: () => invoke<SettingsView>("get_settings"),
  setExportDir: (value: string | null) => invoke<void>("set_export_dir", { value }),
  setClaudePrompt: (value: string | null) => invoke<void>("set_claude_prompt", { value }),
  tursoConnect: (url: string, token: string) => invoke<TursoConnected>("turso_connect", { url, token }),
  tursoDisconnect: () => invoke<AppStatus>("turso_disconnect"),
  tursoPushLocal: () => invoke<string>("turso_push_local"),
  createDemo: () => invoke<string[]>("create_demo"),

  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
};
