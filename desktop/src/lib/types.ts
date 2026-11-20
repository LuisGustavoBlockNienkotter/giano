// Mirrors of what the Rust commands return (giano-core serialization).

export type ItemTypeValue = "improvement" | "discussion" | "adjustment";
export type PriorityValue = "critical" | "high" | "medium" | "low";
export type StatusValue = "open" | "analysis" | "approved" | "in_progress" | "done" | "discarded";
export type NoteStatusValue = "pending" | "processed" | "archived";

export interface ItemData {
  title: string;
  type: ItemTypeValue;
  priority: PriorityValue;
  difficulty: number;
  status: StatusValue;
  tags: string[];
  body: string;
  archived: boolean;
}

export interface ItemView {
  item_id: number;
  project_id: number;
  project_code: string;
  number: number;
  addendum: number;
  version_id: number;
  rev_start: number;
  rev_end: number | null;
  source_note_id: number | null;
  data: ItemData;
  code: string;
  key: [number, number];
  is_addendum: boolean;
  parent_code: string | null;
}

export interface Project {
  id: number;
  code: string;
  name: string;
  description: string;
  active: boolean;
  created_at: string;
}

export interface Revision {
  project_id: number;
  number: number;
  created_at: string;
  summary: string;
  item_id: number | null;
}

export interface ProjectView {
  project: Project;
  latest_rev: number;
  rev: number;
  revision: Revision | null;
  items: ItemView[];
}

export interface FieldChange {
  field: string;
  label: string;
  old: string;
  new: string;
  description: string;
}

export interface ItemChange {
  old: ItemView;
  new: ItemView;
  changes: FieldChange[];
}

export interface RevisionDiff {
  project: Project;
  rev_from: number;
  rev_to: number;
  added: ItemView[];
  removed: ItemView[];
  changed: ItemChange[];
}

export interface DiffView {
  diff: RevisionDiff;
  markdown: string;
}

export interface Note {
  id: number;
  note_date: string;
  name: string;
  project_id: number | null;
  body: string;
  status: NoteStatusValue;
  created_at: string;
  updated_at: string;
  project_code: string | null;
}

export interface NoteInput {
  name: string;
  note_date: string;
  project_id: number | null;
  body: string;
  status: NoteStatusValue;
}

export interface SearchHit {
  kind: "item" | "note";
  id: number;
  project_id: number | null;
  label: string;
  title: string;
  status: string;
  archived: boolean;
  snippet: string;
}

export interface ChoiceMeta {
  value: string;
  label: string;
  plural: string | null;
}

export interface Meta {
  item_types: ChoiceMeta[];
  priorities: ChoiceMeta[];
  statuses: ChoiceMeta[];
  note_statuses: ChoiceMeta[];
}

export interface AppStatus {
  open: boolean;
  error: string | null;
  location: string | null;
  remote: boolean;
  turso_configured: boolean;
  turso_url: string | null;
  turso_source: string;
  local_path: string;
  config_path: string;
  version: string;
}

export interface UpdateResult {
  item: ItemView | null;
  summary: string | null;
}

export interface ExportOutcome {
  path: string;
  summary: string;
  marked: string[];
  content: string | null;
}

export interface SettingsView {
  export_dir: string;
  claude_prompt: string;
  default_claude_prompt: string;
}

export interface TursoConnected {
  status: AppStatus;
  projects: number;
  can_push_local: boolean;
}

export interface ProgressEvent {
  task: string;
  done: number;
  total: number;
}

export interface CmdError {
  kind: "validation" | "unavailable" | "conflict" | "closed" | "internal";
  message: string;
}
