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
  /** When the item (or addendum) was created, ISO date/time. */
  created_at: string;
  /** The user's control date (`YYYY-MM-DD`), not versioned: the same at every revision. */
  control_date: string | null;
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
  /** git remote of the project's repository (shared; "" = none). */
  repo_remote: string;
}

/** The net change of one item in a revision. */
export interface RevisionChange {
  item_id: number;
  summary: string;
}

/** One revision may change several items (a block of changes). */
export interface Revision {
  project_id: number;
  number: number;
  created_at: string;
  summary: string;
  /** The latest revision, still collecting every change until it is closed. */
  is_open: boolean;
  changes: RevisionChange[];
  /** The summary to show (an open revision may not have one yet). */
  title: string;
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
  /** Items whose control date the HTML export set to today. */
  control_dated: number;
  content: string | null;
}

export interface SettingsView {
  export_dir: string;
  claude_prompt: string;
  default_claude_prompt: string;
  /** Per computer: program that opens files of a repository ("" = automatic). */
  editor: string;
  /** Per computer: full path of Claude Code ("" = automatic). */
  claude_path: string;
  claude_found: string | null;
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

export interface GitInfo {
  root: string;
  branch: string;
  head: string;
  remote: string | null;
  upstream: string | null;
  /** Files changed and not committed. */
  dirty: number;
}

/** The repository of a project on this computer. */
export interface RepoInfo {
  project_id: number;
  remote: string;
  web_url: string | null;
  /** Local folder (null = not linked on this computer). */
  path: string | null;
  path_exists: boolean;
  git: GitInfo | null;
  /** Claude Code executable found. */
  claude: string | null;
}

export interface CommitInfo {
  sha: string;
  short: string;
  author: string;
  date: string;
  subject: string;
  body: string;
}

export type TouchRelation = "resolves" | "advances" | "related" | "cited";

export interface TouchedItem {
  item_id: number;
  number: number;
  addendum: number;
  code: string;
  title: string;
  relation: TouchRelation;
  reason: string;
  commits: string[];
  files: string[];
  cited: boolean;
}

export interface CheckResult {
  commits: CommitInfo[];
  truncated: boolean;
  items: TouchedItem[];
  unmatched: string[];
  pull: string | null;
  cost_usd: number | null;
}

/** One "Atualizar e verificar". */
export interface RepoCheck {
  id: number;
  project_id: number;
  from_commit: string | null;
  to_commit: string;
  branch: string;
  commit_count: number;
  summary: string;
  result: CheckResult;
  created_at: string;
}

export interface CheckOutcome {
  /** null = nothing new since the last check. */
  check: RepoCheck | null;
  pull: string | null;
  since: string | null;
}
