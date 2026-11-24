// Global UI state (Svelte 5 runes). Components read and mutate `store` directly.

import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "./api";
import { formatDate } from "./format";
import type {
  AppStatus,
  ItemData,
  ItemView,
  Meta,
  ProgressEvent,
  Project,
  ProjectView,
  RepoCheck,
  RepoInfo,
  PriorityValue,
  StatusValue,
  ItemTypeValue,
  NoteStatusValue,
} from "./types";

export type View = "items" | "notes" | "revisions" | "repo" | "settings";

export type EditorRequest =
  | { mode: "new"; projectId: number; sourceNoteId?: number | null; initial?: Partial<ItemData> }
  | { mode: "edit"; item: ItemView }
  | { mode: "addendum"; base: ItemView; original: ItemView | null }
  | { mode: "view"; item: ItemView };

export interface Toast {
  id: number;
  message: string;
  kind: "info" | "success" | "error";
}

export interface DialogRequest {
  id: number;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string | null;
  danger: boolean;
  resolve: (ok: boolean) => void;
}

export interface Busy {
  title: string;
  task: string;
  done: number;
  total: number;
}

let seq = 0;

class Store {
  status = $state<AppStatus | null>(null);
  meta = $state<Meta | null>(null);
  projects = $state<Project[]>([]);
  projectId = $state<number | null>(null);
  view = $state<View>("items");

  /** The loaded project (items at `viewRev`, archived included). */
  pv = $state<ProjectView | null>(null);
  /** null = latest revision. */
  viewRev = $state<number | null>(null);
  showArchive = $state(false);
  selectedItemId = $state<number | null>(null);
  tags = $state<string[]>([]);

  editor = $state<EditorRequest | null>(null);
  projectDialog = $state<{ project: Project | null } | null>(null);
  exportHtml = $state(false);
  /** Open a revision (collects a block of changes) or close the open one. */
  revisionDialog = $state<"open" | "close" | null>(null);
  exportClaude = $state<{ noteIds: number[] | null } | null>(null);
  searchOpen = $state(false);
  /** Note to open when the notes view shows up (search result, quick note). */
  pendingNoteId = $state<number | null>(null);

  /** Repository of the selected project on this computer (loaded by the repository view). */
  repoInfo = $state<RepoInfo | null>(null);
  repoChecks = $state<RepoCheck[]>([]);
  /** The "Atualizar e verificar" running (it keeps running while other screens are used). */
  repoRun = $state<{ projectId: number; message: string } | null>(null);
  /** The check whose result is open (item links change status from it). */
  checkDialog = $state<RepoCheck | null>(null);

  toasts = $state<Toast[]>([]);
  dialogs = $state<DialogRequest[]>([]);
  busy = $state<Busy | null>(null);

  /** Extra refresh callbacks registered by views (notes list...). */
  private refreshers = new Set<() => Promise<void> | void>();

  get project(): Project | null {
    return this.projects.find((p) => p.id === this.projectId) ?? null;
  }

  get readOnly(): boolean {
    return !!this.pv && (this.pv.rev !== this.pv.latest_rev || !this.pv.project.active);
  }

  // -- labels ---------------------------------------------------------------

  private find(list: keyof Meta, value: string) {
    return this.meta?.[list].find((c) => c.value === value);
  }
  typeLabel = (v: ItemTypeValue) => this.find("item_types", v)?.label ?? v;
  typePlural = (v: ItemTypeValue) => this.find("item_types", v)?.plural ?? v;
  priorityLabel = (v: PriorityValue) => this.find("priorities", v)?.label ?? v;
  statusLabel = (v: StatusValue) => this.find("statuses", v)?.label ?? v;
  noteStatusLabel = (v: NoteStatusValue) => this.find("note_statuses", v)?.label ?? v;

  // -- loading --------------------------------------------------------------

  async init() {
    listen<{ project_id: number; message: string }>("repo_check", (e) => {
      if (this.repoRun?.projectId === e.payload.project_id) this.repoRun = { ...this.repoRun, message: e.payload.message };
    });
    listen<ProgressEvent>("progress", (e) => {
      if (this.busy) {
        this.busy = { ...this.busy, task: e.payload.task, done: e.payload.done, total: e.payload.total };
      }
    });
    this.meta = await api.meta();
    await this.loadStatus();
  }

  async loadStatus() {
    this.status = await api.appStatus();
    if (this.status.open) {
      await this.loadProjects();
    } else {
      this.projects = [];
      this.pv = null;
    }
  }

  async loadProjects() {
    this.projects = await api.listProjects();
    if (!this.projects.some((p) => p.id === this.projectId)) {
      const saved = Number(localStorage.getItem("giano.project") ?? "0");
      const first = this.projects.find((p) => p.id === saved) ?? this.projects.find((p) => p.active) ?? this.projects[0];
      this.projectId = first?.id ?? null;
      this.viewRev = null;
    }
    await this.loadProject();
  }

  async selectProject(id: number) {
    if (id === this.projectId) return;
    this.projectId = id;
    this.viewRev = null;
    this.showArchive = false;
    this.selectedItemId = null;
    this.repoInfo = null;
    this.repoChecks = [];
    try {
      localStorage.setItem("giano.project", String(id));
    } catch {
      /* private storage: not important */
    }
    await this.loadProject();
  }

  async loadProject() {
    if (this.projectId == null) {
      this.pv = null;
      return;
    }
    try {
      const [pv, tags] = await Promise.all([api.loadProject(this.projectId, this.viewRev), api.allTags()]);
      this.pv = pv;
      this.tags = tags;
      if (this.viewRev != null && pv.rev === pv.latest_rev) this.viewRev = null;
    } catch (e) {
      this.toast(errorMessage(e), "error");
    }
  }

  async showRevision(rev: number | null) {
    this.viewRev = rev;
    this.view = "items";
    await this.loadProject();
  }

  // -- repository ------------------------------------------------------------

  async loadRepo() {
    const id = this.projectId;
    if (id == null) return;
    try {
      const [info, checks] = await Promise.all([api.repoInfo(id), api.listRepoChecks(id)]);
      if (id !== this.projectId) return;
      this.repoInfo = info;
      this.repoChecks = checks;
    } catch (e) {
      this.toast(errorMessage(e), "error");
    }
  }

  /** "Atualizar e verificar" (`pull`) or "Verificar sem atualizar". A refused pull offers to
   * verify without it. The result opens in the check dialog. */
  async runCheck(pull: boolean): Promise<void> {
    const id = this.projectId;
    if (id == null || this.repoRun) return;
    this.repoRun = { projectId: id, message: pull ? "Atualizando o repositório (git pull)…" : "Lendo os commits…" };
    try {
      const out = await api.runRepoCheck(id, pull);
      if (out.check) {
        if (this.projectId !== id) await this.selectProject(id);
        this.viewRev = null;
        await this.loadProject();
        this.checkDialog = out.check;
        const n = out.check.result.items.length;
        this.toast(`Verificação concluída: ${n === 1 ? "1 item tocado" : `${n} itens tocados`}.`);
      } else {
        const since = out.since ? ` desde a verificação de ${formatDate(out.since)}` : "";
        this.toast(`Nenhum commit novo${since}.`, "info");
      }
    } catch (e) {
      const message = errorMessage(e);
      if (pull && message.startsWith("O git pull não foi feito")) {
        this.repoRun = null;
        const ok = await this.confirm(message, {
          title: "Pull não feito",
          confirmLabel: "Verificar sem atualizar",
        });
        if (ok) return this.runCheck(false);
      } else if (message !== "Cancelado.") {
        this.toast(message, "error");
      }
    } finally {
      this.repoRun = null;
      if (this.projectId === id) await this.loadRepo();
    }
  }

  onRefresh(fn: () => Promise<void> | void) {
    this.refreshers.add(fn);
    return () => this.refreshers.delete(fn);
  }

  /** F5 / window focus: reload everything shown (changes made by the TUI or another computer). */
  async refresh(quiet = false) {
    try {
      const status = await api.appStatus();
      this.status = status;
      if (!status.open) return;
      await this.loadProjects();
      for (const fn of this.refreshers) await fn();
      if (!quiet) this.toast("Dados atualizados.", "info");
    } catch (e) {
      if (!quiet) this.toast(errorMessage(e), "error");
    }
  }

  // -- feedback -------------------------------------------------------------

  toast(message: string, kind: Toast["kind"] = "success") {
    const id = ++seq;
    this.toasts = [...this.toasts, { id, message, kind }];
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === "error" ? 7000 : 3500);
  }

  confirm(
    message: string,
    opts: { title?: string; confirmLabel?: string; cancelLabel?: string | null; danger?: boolean } = {},
  ): Promise<boolean> {
    return new Promise((resolve) => {
      this.dialogs = [
        ...this.dialogs,
        {
          id: ++seq,
          title: opts.title ?? "Confirmar",
          message,
          confirmLabel: opts.confirmLabel ?? "Confirmar",
          cancelLabel: opts.cancelLabel === undefined ? "Cancelar" : opts.cancelLabel,
          danger: opts.danger ?? false,
          resolve,
        },
      ];
    });
  }

  closeDialog(id: number, ok: boolean) {
    const d = this.dialogs.find((x) => x.id === id);
    this.dialogs = this.dialogs.filter((x) => x.id !== id);
    d?.resolve(ok);
  }

  /** Run a long operation behind a progress overlay; returns its result or null on error. */
  async withBusy<T>(title: string, fn: () => Promise<T>): Promise<T | null> {
    this.busy = { title, task: "", done: 0, total: 0 };
    try {
      return await fn();
    } catch (e) {
      this.toast(errorMessage(e), "error");
      return null;
    } finally {
      this.busy = null;
    }
  }

  /** Any overlay open: global shortcuts stay quiet. */
  get overlayOpen(): boolean {
    return (
      !!this.editor ||
      !!this.projectDialog ||
      this.exportHtml ||
      !!this.revisionDialog ||
      !!this.exportClaude ||
      !!this.checkDialog ||
      this.searchOpen ||
      this.dialogs.length > 0 ||
      !!this.busy
    );
  }
}

export const store = new Store();

export function emptyItem(): ItemData {
  return {
    title: "",
    type: "improvement",
    priority: "medium",
    difficulty: 3,
    status: "open",
    tags: [],
    body: "",
    archived: false,
  };
}
