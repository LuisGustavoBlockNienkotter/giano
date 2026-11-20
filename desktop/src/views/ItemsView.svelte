<script lang="ts" module>
  import type { ItemView } from "../lib/types";

  type SortColumn = "id" | "title" | "type" | "priority" | "difficulty" | "status" | "tags";

  // Kept while switching views (the TUI keeps its filters too).
  const filters = $state({
    text: "",
    type: "",
    status: "",
    priority: "",
    tag: "",
    difficulty: "",
    hideClosed: true,
    // The archive is mostly done/discarded items: don't hide them there by default.
    archiveHideClosed: false,
  });
  const sort = $state<{ column: SortColumn; reverse: boolean }>({ column: "priority", reverse: false });
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    Plus,
    FileCode2,
    Sparkles,
    Search,
    X,
    ChevronDown,
    ChevronUp,
    CornerDownRight,
    History,
    ListChecks,
    Archive,
    FolderOpen,
  } from "@lucide/svelte";
  import { store } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import { parseRanges, CLOSED, plural } from "../lib/format";
  import { newItem, editItem, newAddendum, deleteItem, toggleArchive, setStatus } from "../lib/actions";
  import Badges, { PRIORITY_DOT, STATUS_DOT } from "../lib/components/Badges.svelte";
  import Menu from "../lib/components/Menu.svelte";
  import ItemDetail from "./ItemDetail.svelte";
  import type { StatusValue } from "../lib/types";

  const PRIORITY_RANK = { critical: 0, high: 1, medium: 2, low: 3 } as const;
  const STATUS_RANK = { open: 0, analysis: 1, approved: 2, in_progress: 3, done: 4, discarded: 5 } as const;

  let marked = new SvelteSet<number>();
  let anchorId: number | null = null;
  let filterInput: HTMLInputElement | undefined = $state();
  let statusMenu: ReturnType<typeof Menu> | undefined = $state();
  let tableBody: HTMLTableSectionElement | undefined = $state();

  let pv = $derived(store.pv);
  let all = $derived(pv?.items ?? []);
  let list = $derived(all.filter((v) => v.data.archived === store.showArchive));
  let archivedCount = $derived(all.filter((v) => v.data.archived).length);
  let difficulty = $derived(filters.difficulty.trim() ? parseRanges(filters.difficulty) : null);
  let difficultyInvalid = $derived(!!filters.difficulty.trim() && !difficulty);
  let hideClosed = $derived(store.showArchive ? filters.archiveHideClosed : filters.hideClosed);
  let projectTags = $derived([...new Set(list.flatMap((v) => v.data.tags))].sort());

  function matches(v: ItemView): boolean {
    const d = v.data;
    if (filters.type && d.type !== filters.type) return false;
    if (filters.status && d.status !== filters.status) return false;
    if (filters.priority && d.priority !== filters.priority) return false;
    if (filters.tag && !d.tags.includes(filters.tag)) return false;
    if (difficulty && !difficulty.has(d.difficulty)) return false;
    if (hideClosed && !filters.status && CLOSED.includes(d.status)) return false;
    const q = filters.text.trim().toLowerCase();
    if (q) {
      const hay = `${v.code} ${d.title} ${d.tags.join(" ")}`.toLowerCase();
      if (!q.split(/\s+/).every((w) => hay.includes(w))) return false;
    }
    return true;
  }

  function keyOf(v: ItemView): (string | number)[] {
    const d = v.data;
    switch (sort.column) {
      case "title":
        return [d.title.toLowerCase()];
      case "type":
        return [store.typeLabel(d.type), v.number, v.addendum];
      case "priority":
        return [PRIORITY_RANK[d.priority], v.number, v.addendum];
      case "difficulty":
        return [d.difficulty, v.number, v.addendum];
      case "status":
        return [STATUS_RANK[d.status], v.number, v.addendum];
      case "tags":
        return [d.tags.join(", "), v.number, v.addendum];
      default:
        return [v.number, v.addendum];
    }
  }

  function cmp(a: ItemView, b: ItemView): number {
    const ka = keyOf(a);
    const kb = keyOf(b);
    for (let i = 0; i < ka.length; i++) {
      if (ka[i] < kb[i]) return sort.reverse ? 1 : -1;
      if (ka[i] > kb[i]) return sort.reverse ? -1 : 1;
    }
    return 0;
  }

  let visible = $derived(list.filter(matches).sort(cmp));
  let selected = $derived(all.find((v) => v.item_id === store.selectedItemId) ?? null);
  let markedItems = $derived(visible.filter((v) => marked.has(v.item_id)));
  let filtersActive = $derived(
    !!(filters.text || filters.type || filters.status || filters.priority || filters.tag || filters.difficulty),
  );

  // Keep a valid selection: the first visible item when the selected one is gone.
  $effect(() => {
    if (!visible.length) return;
    if (store.selectedItemId == null || !all.some((v) => v.item_id === store.selectedItemId)) {
      store.selectedItemId = visible[0].item_id;
    }
  });

  // A different list (project, revision, archive): the multi-selection does not carry over.
  let context = $derived(`${pv?.project.id}:${pv?.rev}:${store.showArchive}`);
  $effect(() => {
    context;
    marked.clear();
  });

  function setSort(column: SortColumn) {
    if (sort.column === column) sort.reverse = !sort.reverse;
    else {
      sort.column = column;
      sort.reverse = false;
    }
  }

  function clearFilters() {
    Object.assign(filters, { text: "", type: "", status: "", priority: "", tag: "", difficulty: "" });
  }

  async function select(id: number, scroll = true) {
    store.selectedItemId = id;
    if (scroll) {
      await tick();
      tableBody?.querySelector(`[data-id="${id}"]`)?.scrollIntoView({ block: "nearest" });
    }
  }

  function clickRow(e: MouseEvent, v: ItemView) {
    if (e.ctrlKey || e.metaKey) {
      toggleMark(v.item_id);
      anchorId = v.item_id;
    } else if (e.shiftKey && anchorId != null) {
      const ids = visible.map((x) => x.item_id);
      const [a, b] = [ids.indexOf(anchorId), ids.indexOf(v.item_id)].sort((x, y) => x - y);
      if (a >= 0) for (const id of ids.slice(a, b + 1)) marked.add(id);
    } else {
      anchorId = v.item_id;
    }
    select(v.item_id, false);
  }

  function toggleMark(id: number) {
    if (marked.has(id)) marked.delete(id);
    else marked.add(id);
  }

  function move(delta: number, extend = false) {
    if (!visible.length) return;
    const idx = visible.findIndex((v) => v.item_id === store.selectedItemId);
    const next = visible[Math.max(0, Math.min(visible.length - 1, (idx < 0 ? 0 : idx) + delta))];
    if (extend) {
      if (selected) marked.add(selected.item_id);
      marked.add(next.item_id);
    }
    select(next.item_id);
  }

  /** The items an action applies to: the marked ones, or the selected one. */
  function targets(): ItemView[] {
    if (markedItems.length) return markedItems;
    return selected ? [selected] : [];
  }

  async function applyStatus(value: string) {
    const items = targets();
    if (!items.length) return;
    if (await setStatus(items, value as StatusValue)) marked.clear();
  }

  function toggleArchiveView() {
    store.showArchive = !store.showArchive;
    store.selectedItemId = null;
  }

  function keydown(e: KeyboardEvent) {
    if (store.overlayOpen || e.altKey) return;
    const el = e.target as HTMLElement;
    const typing =
      (el.tagName === "INPUT" && !["checkbox", "radio", "button"].includes((el as HTMLInputElement).type)) ||
      el.tagName === "TEXTAREA" ||
      el.tagName === "SELECT" ||
      el.isContentEditable;
    if (typing) {
      if (e.key === "Escape") el.blur();
      if (e.key === "ArrowDown" && el === filterInput) {
        el.blur();
        move(0);
      }
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (mod && key === "a") {
      e.preventDefault();
      for (const v of visible) marked.add(v.item_id);
      return;
    }
    if (mod) return;
    const handlers: Record<string, () => void> = {
      ArrowDown: () => move(1, e.shiftKey),
      ArrowUp: () => move(-1, e.shiftKey),
      PageDown: () => move(15),
      PageUp: () => move(-15),
      Home: () => move(-1e6),
      End: () => move(1e6),
      " ": () => selected && toggleMark(selected.item_id),
      Escape: () => marked.clear(),
      Enter: () => selected && editItem(selected),
      e: () => selected && editItem(selected),
      n: () => newItem(),
      d: () => selected && newAddendum(selected),
      Delete: () => selected && deleteItem(selected),
      z: () => selected && toggleArchive(selected),
      s: () => statusMenu?.show(),
      v: () => toggleArchiveView(),
      f: () => filterInput?.focus(),
      "/": () => (store.searchOpen = true),
      r: () => (store.view = "revisions"),
      l: () => store.showRevision(null),
      x: () => (store.exportHtml = true),
      c: () => (store.exportClaude = { noteIds: null }),
      a: () => (store.view = "notes"),
    };
    const h = handlers[key];
    if (h) {
      e.preventDefault();
      h();
    }
  }

  const columns: { key: SortColumn; label: string; class: string }[] = [
    { key: "id", label: "ID", class: "w-[88px]" },
    { key: "title", label: "Título", class: "" },
    { key: "type", label: "Tipo", class: "w-[84px]" },
    { key: "priority", label: "Prioridade", class: "w-[92px]" },
    { key: "difficulty", label: "Dif.", class: "w-[48px]" },
    { key: "status", label: "Status", class: "w-[144px]" },
  ];

  let statusItems = $derived(
    (store.meta?.statuses ?? []).map((s, i) => ({
      value: s.value,
      label: s.label,
      dot: STATUS_DOT[s.value as StatusValue],
      hint: String(i + 1),
      current: targets().length === 1 && targets()[0].data.status === s.value,
    })),
  );
  let statusHeading = $derived.by(() => {
    const t = targets();
    return t.length === 1 ? `Mudar status de ${t[0].code}` : `Mudar status de ${t.length} itens`;
  });
</script>

<svelte:window onkeydown={keydown} />

{#if !pv}
  <div class="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
    {#if store.projects.length === 0}
      <div class="flex h-12 w-12 items-center justify-center rounded-2xl bg-accent-100 text-accent-600 dark:bg-accent-500/15">
        <FolderOpen size={22} />
      </div>
      <div>
        <h1 class="font-display text-lg font-semibold">Bem-vindo ao Giano</h1>
        <p class="mt-1 max-w-sm text-[13px] text-zinc-500">
          Crie o primeiro projeto para começar a lista de melhorias, ou gere dados de exemplo para conhecer o app.
        </p>
      </div>
      <div class="flex gap-2">
        <button class="btn-primary" onclick={() => (store.projectDialog = { project: null })}><Plus size={14} /> Novo projeto</button>
        <button
          class="btn-secondary"
          onclick={() =>
            store.withBusy("Gerando dados de exemplo…", async () => {
              await api.createDemo();
              await store.loadProjects();
              store.toast("Projetos de exemplo LOJA e RH criados.");
            })}><Sparkles size={14} /> Dados de exemplo</button
        >
      </div>
    {:else}
      <p class="text-sm text-zinc-400">Selecione um projeto.</p>
    {/if}
  </div>
{:else}
  <div class="relative flex h-full min-h-0 flex-col">
    {#if pv.rev < pv.latest_rev}
      <div class="flex items-center gap-3 border-b border-amber-200 bg-amber-50 px-5 py-2 text-[13px] text-amber-900 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-200">
        <History size={15} />
        <span class="flex-1"><strong>Rev. {pv.rev} de {pv.latest_rev}</strong> — somente leitura{pv.revision ? `: ${pv.revision.summary}` : ""}</span>
        <button class="btn h-7 bg-amber-100 px-2.5 hover:bg-amber-200 dark:bg-amber-500/20 dark:hover:bg-amber-500/30" onclick={() => store.showRevision(null)}>
          Voltar à última <span class="kbd">L</span>
        </button>
      </div>
    {/if}

    <!-- toolbar -->
    <div class="flex flex-wrap items-center gap-2 border-b border-zinc-200/80 px-5 py-2 dark:border-zinc-800">
      <div class="flex rounded-lg bg-zinc-200/60 p-0.5 dark:bg-zinc-800">
        <button
          class="h-7 rounded-md px-3 text-[13px] font-medium {!store.showArchive ? 'bg-white shadow-xs dark:bg-zinc-700' : 'text-zinc-500'}"
          onclick={() => store.showArchive && toggleArchiveView()}>Lista</button
        >
        <button
          class="flex h-7 items-center gap-1.5 rounded-md px-3 text-[13px] font-medium {store.showArchive ? 'bg-white shadow-xs dark:bg-zinc-700' : 'text-zinc-500'}"
          onclick={() => !store.showArchive && toggleArchiveView()}
          title="Alternar lista / arquivo (V)"><Archive size={13} /> Arquivo {#if archivedCount}<span class="text-[11px] text-zinc-400">{archivedCount}</span>{/if}</button
        >
      </div>

      <div class="relative w-52">
        <Search size={14} class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-zinc-400" />
        <input bind:this={filterInput} class="input h-7 pl-8" placeholder="Filtrar (F)" bind:value={filters.text} />
      </div>

      <select class="input h-7 w-auto" bind:value={filters.type} title="Tipo">
        <option value="">Tipo</option>
        {#each store.meta?.item_types ?? [] as t (t.value)}<option value={t.value}>{t.label}</option>{/each}
      </select>
      <select class="input h-7 w-auto" bind:value={filters.priority} title="Prioridade">
        <option value="">Prioridade</option>
        {#each store.meta?.priorities ?? [] as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
      </select>
      <select class="input h-7 w-auto" bind:value={filters.status} title="Status">
        <option value="">Status</option>
        {#each store.meta?.statuses ?? [] as s (s.value)}<option value={s.value}>{s.label}</option>{/each}
      </select>
      <select class="input h-7 w-auto max-w-36" bind:value={filters.tag} title="Tag">
        <option value="">Tag</option>
        {#each projectTags as t (t)}<option value={t}>#{t}</option>{/each}
      </select>
      <input
        class="input h-7 w-20 {difficultyInvalid ? 'border-red-400 focus:border-red-500 focus:ring-red-500/15' : ''}"
        placeholder="Dif. 1-5"
        title="Dificuldade: ex. 1,3-5"
        bind:value={filters.difficulty}
      />
      <label class="flex h-7 items-center gap-1.5 px-1 text-[12.5px] text-zinc-600 dark:text-zinc-400">
        {#if store.showArchive}
          <input type="checkbox" class="accent-accent-600" bind:checked={filters.archiveHideClosed} />
        {:else}
          <input type="checkbox" class="accent-accent-600" bind:checked={filters.hideClosed} />
        {/if}
        Ocultar concluídos
      </label>
      {#if filtersActive}
        <button class="btn-ghost h-7 px-2" onclick={clearFilters}><X size={13} /> Limpar</button>
      {/if}
      <span class="ml-auto text-xs text-zinc-400">{visible.length} de {list.length}</span>
    </div>

    <div class="flex min-h-0 flex-1">
      <!-- table -->
      <div class="flex min-w-0 flex-1 flex-col">
        {#if markedItems.length}
          <div class="flex items-center gap-3 border-b border-accent-200 bg-accent-50 px-5 py-1.5 text-[13px] dark:border-accent-500/20 dark:bg-accent-500/10">
            <ListChecks size={15} class="text-accent-600" />
            <span class="font-medium">{plural(markedItems.length, "item selecionado", "itens selecionados")}</span>
            <span class="text-xs text-zinc-500">Ctrl+clique, Shift+clique, Espaço, Ctrl+A</span>
            <span class="flex-1"></span>
            <button class="btn-ghost h-7 px-2" onclick={() => marked.clear()}>Limpar <span class="kbd">Esc</span></button>
          </div>
        {/if}
        <div class="min-h-0 flex-1 overflow-auto pb-16">
          <table class="w-full min-w-[600px] table-fixed border-separate border-spacing-0 text-[13px]">
            <thead class="sticky top-0 z-10 bg-zinc-50/95 backdrop-blur dark:bg-zinc-950/95">
              <tr>
                <th class="w-9 border-b border-zinc-200 pl-4 dark:border-zinc-800">
                  <input
                    type="checkbox"
                    class="accent-accent-600"
                    checked={visible.length > 0 && markedItems.length === visible.length}
                    onchange={(e) => {
                      if ((e.target as HTMLInputElement).checked) for (const v of visible) marked.add(v.item_id);
                      else marked.clear();
                    }}
                  />
                </th>
                {#each columns as c (c.key)}
                  <th class="{c.class} border-b border-zinc-200 px-2 py-2 text-left text-[11.5px] font-semibold text-zinc-500 dark:border-zinc-800">
                    <button class="inline-flex items-center gap-0.5 hover:text-zinc-900 dark:hover:text-zinc-100" onclick={() => setSort(c.key)}>
                      {c.label}
                      {#if sort.column === c.key}
                        {#if sort.reverse}<ChevronUp size={12} />{:else}<ChevronDown size={12} />{/if}
                      {/if}
                    </button>
                  </th>
                {/each}
              </tr>
            </thead>
            <tbody bind:this={tableBody}>
              {#each visible as v (v.item_id)}
                {@const isSel = v.item_id === store.selectedItemId}
                {@const isMarked = marked.has(v.item_id)}
                <tr
                  data-id={v.item_id}
                  class="group cursor-default {isSel
                    ? 'bg-accent-50 dark:bg-accent-500/10'
                    : isMarked
                      ? 'bg-accent-50/50 dark:bg-accent-500/5'
                      : 'hover:bg-zinc-100/70 dark:hover:bg-zinc-900'}"
                  onclick={(e) => clickRow(e, v)}
                  ondblclick={() => editItem(v)}
                >
                  <td class="border-b border-zinc-100 pl-4 dark:border-zinc-900 {isSel ? 'shadow-[inset_2px_0_0] shadow-accent-500' : ''}">
                    <input
                      type="checkbox"
                      class="accent-accent-600"
                      checked={isMarked}
                      onclick={(e) => e.stopPropagation()}
                      onchange={() => toggleMark(v.item_id)}
                    />
                  </td>
                  <td class="border-b border-zinc-100 px-2 py-2 dark:border-zinc-900">
                    <span class="code inline-flex items-center gap-1 whitespace-nowrap {v.is_addendum ? 'pl-2' : ''}">
                      {#if v.is_addendum}<CornerDownRight size={11} class="text-orange-500" />{/if}{v.code}
                    </span>
                  </td>
                  <td class="border-b border-zinc-100 px-2 py-1.5 dark:border-zinc-900" title={v.data.title}>
                    <div class="truncate font-medium {v.data.status === 'discarded' ? 'text-zinc-400 line-through' : ''}">{v.data.title}</div>
                    {#if v.data.tags.length}
                      <div class="truncate text-[11.5px] text-zinc-400">{v.data.tags.map((t) => `#${t}`).join(" ")}</div>
                    {/if}
                  </td>
                  <td class="truncate border-b border-zinc-100 px-2 py-2 text-zinc-500 dark:border-zinc-900">{store.typeLabel(v.data.type)}</td>
                  <td class="border-b border-zinc-100 px-2 py-2 dark:border-zinc-900">
                    <span class="inline-flex items-center gap-1.5 text-zinc-700 dark:text-zinc-300">
                      <span class="h-2 w-2 rounded-full {PRIORITY_DOT[v.data.priority]}"></span>{store.priorityLabel(v.data.priority)}
                    </span>
                  </td>
                  <td class="border-b border-zinc-100 px-2 py-2 dark:border-zinc-900" title="Dificuldade {v.data.difficulty}/5">
                    <span class="flex items-end gap-[2px]">
                      {#each [1, 2, 3, 4, 5] as n (n)}
                        <span class="w-[3px] rounded-sm {n <= v.data.difficulty ? 'bg-zinc-500 dark:bg-zinc-400' : 'bg-zinc-200 dark:bg-zinc-700'}" style="height: {4 + n * 2}px"></span>
                      {/each}
                    </span>
                  </td>
                  <td class="border-b border-zinc-100 px-2 py-2 dark:border-zinc-900"><Badges status={v.data.status} /></td>
                </tr>
              {:else}
                <tr>
                  <td colspan="7" class="px-5 py-16 text-center text-[13px] text-zinc-400">
                    {#if list.length}
                      Nenhum item com esses filtros.
                      <button class="text-accent-600 hover:underline" onclick={clearFilters}>Limpar filtros</button>
                    {:else if store.showArchive}
                      Nenhum item arquivado.
                    {:else}
                      A lista está vazia. Crie o primeiro item com <span class="kbd">N</span>.
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="flex items-center gap-3 border-t border-zinc-200/80 px-5 py-1.5 text-[11.5px] text-zinc-400 dark:border-zinc-800">
          <Menu bind:this={statusMenu} items={statusItems} onselect={applyStatus} centered heading={statusHeading} class="btn-ghost h-6 px-2 text-[11.5px]" disabled={store.readOnly || !targets().length} title="Mudar status (S)">
            {#snippet trigger()}Status{markedItems.length ? ` (${markedItems.length})` : ""} <span class="kbd">S</span>{/snippet}
          </Menu>
          <span><span class="kbd">N</span> novo</span>
          <span><span class="kbd">E</span> editar</span>
          <span><span class="kbd">D</span> adendo</span>
          <span><span class="kbd">Z</span> arquivar</span>
          <span><span class="kbd">V</span> arquivo</span>
          <span><span class="kbd">R</span> revisões</span>
          <span><span class="kbd">F5</span> atualizar</span>
        </div>
      </div>

      <!-- detail -->
      <section class="w-[40%] max-w-[620px] min-w-[340px] border-l border-zinc-200/80 bg-white dark:border-zinc-800 dark:bg-zinc-900/40">
        {#if selected}
          {#key selected.item_id}
            <ItemDetail item={selected} />
          {/key}
        {:else}
          <div class="flex h-full items-center justify-center text-[13px] text-zinc-400">Nenhum item selecionado.</div>
        {/if}
      </section>
    </div>

    <!-- floating actions -->
    <div class="pointer-events-none absolute right-5 bottom-11 z-20 flex items-center gap-2.5">
      <button
        class="fab pointer-events-auto"
        onclick={() => (store.exportClaude = { noteIds: null })}
        title="Exportar para o Claude (C)"
        aria-label="Exportar para o Claude"
      >
        <Sparkles size={17} />
      </button>
      <button class="fab pointer-events-auto" onclick={() => (store.exportHtml = true)} title="Exportar HTML (X)" aria-label="Exportar HTML">
        <FileCode2 size={17} />
      </button>
      <button
        class="fab-primary pointer-events-auto"
        onclick={newItem}
        disabled={store.readOnly || store.showArchive}
        title="Novo item (N)"
        aria-label="Novo item"
      >
        <Plus size={22} />
      </button>
    </div>
  </div>
{/if}
