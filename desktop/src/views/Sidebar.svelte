<script lang="ts">
  import {
    ListTodo,
    NotebookPen,
    History,
    Settings,
    Search,
    Plus,
    Cloud,
    HardDrive,
    ChevronRight,
    Archive,
    Pencil,
    FolderGit2,
  } from "@lucide/svelte";
  import { store, type View } from "../lib/state.svelte";

  let showArchived = $state(false);
  let active = $derived(store.projects.filter((p) => p.active));
  let archived = $derived(store.projects.filter((p) => !p.active));

  const nav: { view: View; label: string; icon: typeof ListTodo }[] = [
    { view: "items", label: "Itens", icon: ListTodo },
    { view: "notes", label: "Anotações", icon: NotebookPen },
    { view: "revisions", label: "Revisões", icon: History },
    { view: "repo", label: "Repositório", icon: FolderGit2 },
  ];

  function dbName(): string {
    const loc = store.status?.location ?? "";
    if (store.status?.remote) return loc.replace(/^[a-z]+:\/\//i, "").split(".")[0];
    return loc.split(/[\\/]/).pop() ?? loc;
  }
</script>

<aside
  class="flex w-60 shrink-0 flex-col border-r border-zinc-200/80 bg-zinc-100/70 dark:border-zinc-800 dark:bg-zinc-900/60"
>
  <div class="px-3 pt-3 pb-2">
    <button
      class="flex h-8 w-full items-center gap-2 rounded-md border border-zinc-200 bg-white px-2.5 text-[13px] text-zinc-400 shadow-xs hover:border-zinc-300 dark:border-zinc-700/80 dark:bg-zinc-800/60 dark:hover:border-zinc-600"
      onclick={() => (store.searchOpen = true)}
    >
      <Search size={14} />
      <span class="flex-1 text-left">Buscar</span>
      <span class="kbd">Ctrl K</span>
    </button>
  </div>

  <div class="flex items-center justify-between px-4 pt-3 pb-1">
    <span class="text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">Projetos</span>
    <button class="btn-icon h-6 w-6" title="Novo projeto" onclick={() => (store.projectDialog = { project: null })}>
      <Plus size={14} />
    </button>
  </div>

  <nav class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
    {#each active as p (p.id)}
      {@const selected = p.id === store.projectId && store.view !== "settings"}
      <div class="group">
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="clickable flex h-8 items-center gap-2 rounded-md px-2 text-[13px] {selected
            ? 'bg-white font-medium text-zinc-900 shadow-xs ring-1 ring-zinc-200/80 dark:bg-zinc-800 dark:text-white dark:ring-zinc-700'
            : 'text-zinc-700 hover:bg-zinc-200/60 dark:text-zinc-300 dark:hover:bg-zinc-800/70'}"
          onclick={() => {
            store.selectProject(p.id);
            if (store.view === "settings") store.view = "items";
          }}
        >
          <span
            class="flex h-5 min-w-9 items-center justify-center rounded bg-zinc-200/80 px-1 font-mono text-[10px] font-semibold text-zinc-600 dark:bg-zinc-700/70 dark:text-zinc-300"
            >{p.code}</span
          >
          <span class="flex min-w-0 flex-1 items-baseline gap-1">
            <span class="truncate">{p.name}</span>
            {#if store.pv?.project.id === p.id}
              <span class="shrink-0 font-normal whitespace-nowrap text-zinc-400 dark:text-zinc-500"
                >- rev. {store.pv.rev}</span
              >
            {/if}
          </span>
          <button
            class="hidden h-6 w-6 items-center justify-center rounded text-zinc-400 group-hover:flex hover:bg-zinc-200 hover:text-zinc-700 dark:hover:bg-zinc-700 dark:hover:text-zinc-200"
            title="Editar projeto"
            onclick={(e) => {
              e.stopPropagation();
              store.projectDialog = { project: p };
            }}><Pencil size={12} /></button
          >
        </div>
        {#if selected}
          <div class="mt-0.5 mb-1.5 ml-3 border-l border-zinc-200 pl-2 dark:border-zinc-700/70">
            {#each nav as n (n.view)}
              {@const Icon = n.icon}
              <button
                class="flex h-7 w-full items-center gap-2 rounded-md px-2 text-[13px] {store.view === n.view
                  ? 'text-accent-700 dark:text-accent-300'
                  : 'text-zinc-500 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-100'}"
                onclick={() => (store.view = n.view)}
              >
                <Icon size={14} />
                {n.label}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      <p class="px-2 py-3 text-xs text-zinc-400">Nenhum projeto ativo.</p>
    {/each}

    {#if archived.length}
      <button
        class="mt-2 flex h-7 w-full items-center gap-1.5 px-2 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase hover:text-zinc-600"
        onclick={() => (showArchived = !showArchived)}
      >
        <ChevronRight size={12} class="transition-transform {showArchived ? 'rotate-90' : ''}" />
        <Archive size={12} /> Arquivados ({archived.length})
      </button>
      {#if showArchived}
        {#each archived as p (p.id)}
          <button
            class="flex h-7 w-full items-center gap-2 rounded-md px-2 text-[13px] text-zinc-400 hover:bg-zinc-200/60 dark:hover:bg-zinc-800/70 {p.id ===
            store.projectId
              ? 'bg-zinc-200/60 dark:bg-zinc-800'
              : ''}"
            onclick={() => {
              store.selectProject(p.id);
              store.view = "items";
            }}
            ondblclick={() => (store.projectDialog = { project: p })}
          >
            <span class="font-mono text-[10px]">{p.code}</span>
            <span class="truncate">{p.name}</span>
          </button>
        {/each}
      {/if}
    {/if}
  </nav>

  <div class="border-t border-zinc-200/80 p-2 dark:border-zinc-800">
    <button
      class="flex h-8 w-full items-center gap-2 rounded-md px-2 text-[13px] {store.view === 'settings'
        ? 'bg-white font-medium shadow-xs ring-1 ring-zinc-200/80 dark:bg-zinc-800 dark:ring-zinc-700'
        : 'text-zinc-600 hover:bg-zinc-200/60 dark:text-zinc-400 dark:hover:bg-zinc-800/70'}"
      onclick={() => (store.view = "settings")}
    >
      <Settings size={14} /> Configurações
    </button>
    <div class="mt-1 flex h-6 items-center gap-1.5 px-2 text-[11px] whitespace-nowrap text-zinc-400" title={store.status?.location ?? ""}>
      {#if store.status?.remote}
        <Cloud size={12} class="shrink-0 text-sky-500" />
        <span class="shrink-0">Turso ·</span>
      {:else}
        <HardDrive size={12} class="shrink-0" />
        <span class="shrink-0">Local ·</span>
      {/if}
      <span class="min-w-0 truncate">{dbName()}</span>
    </div>
  </div>
</aside>
