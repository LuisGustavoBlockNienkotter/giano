<script lang="ts">
  // Revisions of the project's list: open the list at any revision (read-only) and compare
  // two revisions (added, removed, changed items, with the fields that changed).
  import { onMount } from "svelte";
  import { Eye, GitCompareArrows, Flag, Copy, Plus, Minus, PencilLine } from "@lucide/svelte";
  import Badges from "../lib/components/Badges.svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import { formatDate } from "../lib/format";
  import type { DiffView, Revision } from "../lib/types";

  let revisions = $state<Revision[]>([]);
  let selected = $state<number | null>(null);
  /** Base of the comparison; null = the revision before the selected one. */
  let base = $state<number | null>(null);
  let diff = $state<DiffView | null>(null);
  let listEl: HTMLDivElement | undefined = $state();

  let latest = $derived(revisions[0]?.number ?? 0);
  let from = $derived(selected == null ? null : (base ?? Math.max(0, selected - 1)));

  async function load() {
    if (store.projectId == null) return;
    try {
      revisions = await api.listRevisions(store.projectId);
      if (selected == null || !revisions.some((r) => r.number === selected)) {
        selected = store.pv && store.pv.rev < store.pv.latest_rev ? store.pv.rev : (revisions[0]?.number ?? null);
      }
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  onMount(() => {
    load();
    return store.onRefresh(load);
  });

  $effect(() => {
    store.projectId;
    base = null;
    selected = null;
    load();
  });

  $effect(() => {
    const pid = store.projectId;
    const a = from;
    const b = selected;
    if (pid == null || a == null || b == null) {
      diff = null;
      return;
    }
    const lo = Math.min(a, b);
    const hi = Math.max(a, b);
    api.diff(pid, lo, hi).then(
      (d) => (diff = d),
      (e) => store.toast(errorMessage(e), "error"),
    );
  });

  function move(delta: number) {
    const idx = revisions.findIndex((r) => r.number === selected);
    const next = revisions[Math.max(0, Math.min(revisions.length - 1, idx + delta))];
    if (next) {
      selected = next.number;
      listEl?.querySelector(`[data-rev="${next.number}"]`)?.scrollIntoView({ block: "nearest" });
    }
  }

  function keydown(e: KeyboardEvent) {
    if (store.overlayOpen || (e.target as HTMLElement).tagName === "INPUT") return;
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    const handlers: Record<string, () => void> = {
      ArrowDown: () => move(1),
      ArrowUp: () => move(-1),
      Enter: () => selected != null && store.showRevision(selected === latest ? null : selected),
      m: () => (base = selected),
      l: () => store.showRevision(null).then(() => (store.view = "items")),
      Escape: () => (store.view = "items"),
    };
    if (handlers[k]) {
      e.preventDefault();
      handlers[k]();
    }
  }

  async function copyDiff() {
    if (!diff) return;
    await navigator.clipboard.writeText(diff.markdown);
    store.toast("Diferenças copiadas como Markdown.");
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="flex h-full min-h-0">
  <section class="flex w-[420px] shrink-0 flex-col border-r border-zinc-200/80 dark:border-zinc-800">
    <header class="px-5 pt-4 pb-3">
      <h1 class="font-display text-[17px] font-semibold">Revisões</h1>
      <p class="mt-0.5 text-xs text-zinc-500">
        Cada alteração gera uma revisão; com uma revisão aberta, o bloco todo vira uma só.
        <span class="kbd">Enter</span> abre a lista nela,
        <span class="kbd">M</span> marca a base da comparação.
      </p>
    </header>
    <div bind:this={listEl} class="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
      {#each revisions as r (r.number)}
        <button
          data-rev={r.number}
          class="mb-0.5 flex w-full gap-3 rounded-lg px-3 py-2 text-left {selected === r.number
            ? 'bg-white shadow-xs ring-1 ring-zinc-200 dark:bg-zinc-800 dark:ring-zinc-700'
            : 'hover:bg-zinc-200/50 dark:hover:bg-zinc-900'}"
          onclick={() => (selected = r.number)}
          ondblclick={() => store.showRevision(r.number === latest ? null : r.number)}
        >
          <span class="w-10 shrink-0 pt-px font-mono text-[12px] font-semibold text-zinc-500">{r.number}</span>
          <span class="min-w-0 flex-1">
            <span class="block text-[13px] leading-snug select-text">{r.title}</span>
            {#if r.changes.length > 1}
              <ul class="mt-1 space-y-0.5 text-[12px] leading-snug text-zinc-500 select-text dark:text-zinc-400">
                {#each r.changes as c (c.item_id)}<li class="truncate" title={c.summary}>• {c.summary}</li>{/each}
              </ul>
            {/if}
            <span class="mt-0.5 flex items-center gap-2 text-[11px] text-zinc-400">
              {formatDate(r.created_at)}
              {#if r.is_open}<span class="chip h-4 bg-sky-100 px-1.5 text-[10px] text-sky-800 dark:bg-sky-500/20 dark:text-sky-300">em aberto</span>{/if}
              {#if base === r.number}<span class="chip h-4 bg-accent-100 px-1.5 text-[10px] text-accent-700 dark:bg-accent-500/20 dark:text-accent-300">base</span>{/if}
              {#if store.pv && store.pv.rev === r.number && store.pv.rev < store.pv.latest_rev}<span class="chip h-4 bg-amber-100 px-1.5 text-[10px] text-amber-800">aberta</span>{/if}
            </span>
          </span>
        </button>
      {:else}
        <p class="px-3 py-10 text-center text-[13px] text-zinc-400">Nenhuma revisão ainda: a lista está vazia.</p>
      {/each}
    </div>
  </section>

  <section class="flex min-w-0 flex-1 flex-col">
    {#if selected != null && from != null}
      <header class="flex flex-wrap items-center gap-2 border-b border-zinc-200/80 px-6 py-3 dark:border-zinc-800">
        <GitCompareArrows size={16} class="text-zinc-400" />
        <h2 class="font-display text-[15px] font-semibold">Rev. {Math.min(from, selected)} → Rev. {Math.max(from, selected)}</h2>
        {#if base != null}
          <button class="btn-ghost h-7 px-2 text-xs" onclick={() => (base = null)}>Comparar com a anterior</button>
        {/if}
        <span class="flex-1"></span>
        <button class="btn-ghost h-7 px-2.5" onclick={copyDiff} disabled={!diff}><Copy size={13} /> Copiar</button>
        <button class="btn-secondary h-7 px-2.5" onclick={() => (base = selected)} title="Marcar como base da comparação (M)">
          <Flag size={13} /> Marcar base
        </button>
        <button class="btn-primary h-7 px-2.5" onclick={() => store.showRevision(selected === latest ? null : selected)}>
          <Eye size={13} /> Ver lista na Rev. {selected}
        </button>
      </header>
      <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        {#if diff}
          {@const d = diff.diff}
          {#if !d.added.length && !d.removed.length && !d.changed.length}
            <p class="text-[13px] text-zinc-400">Nenhuma diferença entre as revisões.</p>
          {:else}
            <div class="mb-5 flex gap-2 text-[12.5px]">
              <span class="chip bg-emerald-100 text-emerald-800 dark:bg-emerald-500/15 dark:text-emerald-300">{d.added.length} adicionados</span>
              <span class="chip bg-red-100 text-red-800 dark:bg-red-500/15 dark:text-red-300">{d.removed.length} removidos</span>
              <span class="chip bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300">{d.changed.length} alterados</span>
            </div>
          {/if}
          {#if d.added.length}
            <h3 class="mb-2 flex items-center gap-1.5 text-[12px] font-semibold tracking-wider text-emerald-700 uppercase dark:text-emerald-400"><Plus size={13} /> Adicionados</h3>
            <div class="card mb-6 divide-y divide-zinc-100 dark:divide-zinc-800">
              {#each d.added as v (v.item_id)}
                <div class="flex items-center gap-3 px-4 py-2.5 text-[13px]">
                  <span class="code w-20">{v.code}</span>
                  <span class="min-w-0 flex-1 truncate font-medium">{v.data.title}</span>
                  <span class="text-xs text-zinc-500">{store.typeLabel(v.data.type)}</span>
                  <Badges priority={v.data.priority} />
                  <Badges status={v.data.status} />
                </div>
              {/each}
            </div>
          {/if}
          {#if d.removed.length}
            <h3 class="mb-2 flex items-center gap-1.5 text-[12px] font-semibold tracking-wider text-red-700 uppercase dark:text-red-400"><Minus size={13} /> Removidos</h3>
            <div class="card mb-6 divide-y divide-zinc-100 dark:divide-zinc-800">
              {#each d.removed as v (v.item_id)}
                <div class="flex items-center gap-3 px-4 py-2.5 text-[13px] text-zinc-500">
                  <span class="code w-20">{v.code}</span>
                  <span class="min-w-0 flex-1 truncate line-through">{v.data.title}</span>
                </div>
              {/each}
            </div>
          {/if}
          {#if d.changed.length}
            <h3 class="mb-2 flex items-center gap-1.5 text-[12px] font-semibold tracking-wider text-amber-700 uppercase dark:text-amber-400"><PencilLine size={13} /> Alterados</h3>
            <div class="card divide-y divide-zinc-100 dark:divide-zinc-800">
              {#each d.changed as c (c.new.item_id)}
                <div class="px-4 py-3 text-[13px]">
                  <div class="flex items-center gap-3">
                    <span class="code w-20">{c.new.code}</span>
                    <span class="min-w-0 flex-1 truncate font-medium">{c.new.data.title}</span>
                  </div>
                  <ul class="mt-1.5 ml-[92px] space-y-0.5 text-[12.5px] text-zinc-600 dark:text-zinc-400">
                    {#each c.changes as f (f.field)}
                      <li>
                        {#if f.field === "body"}
                          corpo alterado
                        {:else if f.field === "archived"}
                          {f.description}
                        {:else}
                          <span class="text-zinc-400">{f.label}:</span>
                          <span class="line-through decoration-zinc-300">{f.old}</span> →
                          <strong class="text-zinc-900 dark:text-zinc-100">{f.new}</strong>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    {:else}
      <div class="flex h-full items-center justify-center text-[13px] text-zinc-400">Selecione uma revisão.</div>
    {/if}
  </section>
</div>
