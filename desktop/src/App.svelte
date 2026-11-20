<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Database, RefreshCw, HardDrive } from "@lucide/svelte";
  import { api, errorMessage } from "./lib/api";
  import { store } from "./lib/state.svelte";
  import Sidebar from "./views/Sidebar.svelte";
  import ItemsView from "./views/ItemsView.svelte";
  import NotesView from "./views/NotesView.svelte";
  import RevisionsView from "./views/RevisionsView.svelte";
  import SettingsView from "./views/SettingsView.svelte";
  import ItemEditor from "./views/ItemEditor.svelte";
  import ProjectDialog from "./views/ProjectDialog.svelte";
  import RevisionDialog from "./views/RevisionDialog.svelte";
  import ExportHtmlDialog from "./views/ExportHtmlDialog.svelte";
  import ExportClaudeDialog from "./views/ExportClaudeDialog.svelte";
  import SearchPalette from "./views/SearchPalette.svelte";
  import Overlays from "./lib/components/Overlays.svelte";

  let ready = $state(false);
  let retrying = $state(false);

  onMount(() => {
    store.init().finally(() => (ready = true));
    // Back to the window: reload (changes made by the TUI or by another computer).
    let lastFocus = Date.now();
    const unlisten = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused && Date.now() - lastFocus > 4000 && store.status?.open && !store.busy) store.refresh(true);
      if (!focused) lastFocus = Date.now();
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  async function quickNote() {
    try {
      const note = await api.createNote("", store.projectId, "");
      store.pendingNoteId = note.id;
      store.view = "notes";
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  function keydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === "F5") {
      e.preventDefault();
      if (!store.busy) store.refresh();
      return;
    }
    if (store.overlayOpen) return;
    if (mod && e.key.toLowerCase() === "k") {
      e.preventDefault();
      store.searchOpen = true;
    } else if (mod && e.key.toLowerCase() === "n" && store.status?.open) {
      e.preventDefault();
      quickNote();
    }
  }

  async function retry() {
    retrying = true;
    try {
      store.status = await api.reconnect();
      if (store.status.open) await store.loadProjects();
    } finally {
      retrying = false;
    }
  }

  async function useLocal() {
    store.status = await api.useLocalDatabase();
    if (store.status.open) await store.loadProjects();
  }
</script>

<svelte:window onkeydown={keydown} />

{#if !ready}
  <div class="flex h-full items-center justify-center text-sm text-zinc-400">Abrindo o Giano…</div>
{:else if !store.status?.open}
  <div class="flex h-full items-center justify-center p-8">
    <div class="card max-w-lg p-6">
      <div class="flex items-center gap-2.5">
        <Database size={20} class="text-red-500" />
        <h1 class="font-display text-base font-semibold">Não foi possível abrir o banco de dados</h1>
      </div>
      <p class="mt-3 text-[13px] leading-relaxed text-zinc-600 select-text dark:text-zinc-300">
        {store.status?.error ?? "Erro desconhecido."}
      </p>
      <div class="mt-5 flex gap-2">
        <button class="btn-primary" onclick={retry} disabled={retrying}>
          <RefreshCw size={14} class={retrying ? "animate-spin" : ""} /> Tentar de novo
        </button>
        {#if store.status?.remote || store.status?.turso_configured}
          <button class="btn-secondary" onclick={useLocal}>
            <HardDrive size={14} /> Usar o banco local nesta sessão
          </button>
        {/if}
      </div>
    </div>
  </div>
{:else}
  <div class="flex h-full">
    <Sidebar />
    <main class="flex min-w-0 flex-1 flex-col">
      {#if store.view === "settings"}
        <SettingsView />
      {:else if store.view === "notes"}
        <NotesView />
      {:else if store.view === "revisions"}
        <RevisionsView />
      {:else}
        <ItemsView />
      {/if}
    </main>
  </div>
{/if}

{#if store.editor}<ItemEditor request={store.editor} />{/if}
{#if store.projectDialog}<ProjectDialog project={store.projectDialog?.project ?? null} />{/if}
{#if store.revisionDialog && store.pv}<RevisionDialog mode={store.revisionDialog} />{/if}
{#if store.exportHtml}<ExportHtmlDialog />{/if}
{#if store.exportClaude}<ExportClaudeDialog noteIds={store.exportClaude?.noteIds ?? null} />{/if}
{#if store.searchOpen}<SearchPalette />{/if}
<Overlays />
