<script lang="ts">
  // Full-text search (FTS5) over the current items and the notes, of every project.
  import { onMount } from "svelte";
  import { Search, ListTodo, NotebookPen, LoaderCircle } from "@lucide/svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import type { SearchHit } from "../lib/types";

  let text = $state("");
  let onlyProject = $state(false);
  let hits = $state<SearchHit[]>([]);
  let active = $state(0);
  let loading = $state(false);
  let input: HTMLInputElement | undefined = $state();
  let token = 0;

  onMount(() => input?.focus());

  $effect(() => {
    const q = text.trim();
    const pid = onlyProject ? store.projectId : null;
    const mine = ++token;
    if (!q) {
      hits = [];
      loading = false;
      return;
    }
    loading = true;
    const t = setTimeout(async () => {
      try {
        const r = await api.search(q, pid);
        if (mine === token) {
          hits = r;
          active = 0;
        }
      } catch (e) {
        if (mine === token) store.toast(errorMessage(e), "error");
      } finally {
        if (mine === token) loading = false;
      }
    }, 120);
    return () => clearTimeout(t);
  });

  function close() {
    store.searchOpen = false;
  }

  async function openHit(h: SearchHit) {
    close();
    if (h.kind === "item") {
      if (h.project_id != null && h.project_id !== store.projectId) await store.selectProject(h.project_id);
      if (store.viewRev != null) await store.showRevision(null);
      store.view = "items";
      store.showArchive = h.archived;
      store.selectedItemId = h.id;
    } else {
      store.pendingNoteId = h.id;
      store.view = "notes";
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      active = Math.min(hits.length - 1, active + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = Math.max(0, active - 1);
    } else if (e.key === "Enter" && hits[active]) {
      e.preventDefault();
      openHit(hits[active]);
    }
  }

  /** FTS snippets mark the matches with [ and ]. */
  function parts(snippet: string): { text: string; hit: boolean }[] {
    return snippet.split(/(\[[^\]]*\])/).filter(Boolean).map((p) =>
      p.startsWith("[") && p.endsWith("]") ? { text: p.slice(1, -1), hit: true } : { text: p, hit: false },
    );
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex items-start justify-center bg-zinc-950/40 px-4 pt-[12vh] backdrop-blur-[2px] dark:bg-black/60"
  onmousedown={(e) => e.target === e.currentTarget && close()}
  onkeydown={keydown}
>
  <div class="card w-full max-w-2xl overflow-hidden shadow-2xl ring-1 ring-black/5">
    <div class="flex items-center gap-3 border-b border-zinc-100 px-4 dark:border-zinc-800">
      {#if loading}<LoaderCircle size={18} class="animate-spin text-zinc-400" />{:else}<Search size={18} class="text-zinc-400" />{/if}
      <input
        bind:this={input}
        bind:value={text}
        class="h-12 flex-1 bg-transparent text-[15px] outline-none placeholder:text-zinc-400"
        placeholder="Buscar em itens e anotações…"
      />
      <label class="flex items-center gap-1.5 text-[12px] whitespace-nowrap text-zinc-500">
        <input type="checkbox" class="accent-accent-600" bind:checked={onlyProject} disabled={!store.project} />
        Só {store.project?.code ?? "o projeto"}
      </label>
    </div>
    <div class="max-h-[56vh] overflow-y-auto py-1">
      {#each hits as h, i (h.kind + h.id)}
        <button
          class="flex w-full items-start gap-3 px-4 py-2.5 text-left {i === active ? 'bg-accent-50 dark:bg-accent-500/10' : ''}"
          onmouseenter={() => (active = i)}
          onclick={() => openHit(h)}
        >
          {#if h.kind === "item"}
            <ListTodo size={16} class="mt-0.5 shrink-0 text-accent-500" />
          {:else}
            <NotebookPen size={16} class="mt-0.5 shrink-0 text-amber-500" />
          {/if}
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-2">
              <span class="code shrink-0">{h.label}</span>
              <span class="truncate text-[13px] font-medium">{h.title}</span>
              <span class="ml-auto shrink-0 text-[11px] text-zinc-400">{h.status}</span>
            </span>
            {#if h.snippet}
              <span class="mt-0.5 line-clamp-2 block text-[12.5px] text-zinc-500">
                {#each parts(h.snippet) as p, j (j)}{#if p.hit}<mark class="rounded bg-amber-200/70 px-0.5 text-inherit dark:bg-amber-400/30">{p.text}</mark>{:else}{p.text}{/if}{/each}
              </span>
            {/if}
          </span>
        </button>
      {:else}
        <p class="px-4 py-8 text-center text-[13px] text-zinc-400">
          {text.trim() ? (loading ? "Buscando…" : "Nada encontrado.") : "Digite para buscar. Acentos são ignorados; palavras parciais funcionam."}
        </p>
      {/each}
    </div>
    <div class="flex gap-4 border-t border-zinc-100 px-4 py-2 text-[11px] text-zinc-400 dark:border-zinc-800">
      <span><span class="kbd">↑</span> <span class="kbd">↓</span> navegar</span>
      <span><span class="kbd">Enter</span> abrir</span>
      <span><span class="kbd">Esc</span> fechar</span>
    </div>
  </div>
</div>
