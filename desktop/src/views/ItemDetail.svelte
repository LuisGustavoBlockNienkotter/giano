<script lang="ts">
  import {
    Pencil,
    GitBranchPlus,
    Archive,
    ArchiveRestore,
    Trash2,
    Copy,
    CornerDownRight,
    NotebookPen,
    Eye,
    CalendarPlus,
    CalendarCheck,
  } from "@lucide/svelte";
  import Markdown from "../lib/components/Markdown.svelte";
  import Badges from "../lib/components/Badges.svelte";
  import { store } from "../lib/state.svelte";
  import { editItem, newAddendum, toggleArchive, deleteItem, copyItemMarkdown, readOnlyReason, setControlDate } from "../lib/actions";
  import { formatDate, todayIso } from "../lib/format";
  import type { ItemView } from "../lib/types";

  let { item }: { item: ItemView } = $props();

  let all = $derived(store.pv?.items ?? []);
  let original = $derived(item.is_addendum ? all.find((v) => v.number === item.number && v.addendum === 0) : undefined);
  let addenda = $derived(item.is_addendum ? [] : all.filter((v) => v.number === item.number && v.addendum > 0));
  let editable = $derived(!readOnlyReason(item));
  let listEditable = $derived(!readOnlyReason(null));
  // The control date is not versioned: it can be changed at any revision, only an archived
  // project is read-only.
  let controlEditable = $derived(!!store.pv?.project.active);

  function go(v: ItemView) {
    store.showArchive = v.data.archived;
    store.selectedItemId = v.item_id;
  }

  function openNote(id: number) {
    store.pendingNoteId = id;
    store.view = "notes";
  }
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-zinc-200/80 px-6 pt-5 pb-4 dark:border-zinc-800">
    <div class="flex items-center gap-2">
      <span class="code text-[12.5px]">{item.code}</span>
      {#if item.is_addendum}
        <span class="chip bg-orange-100 text-orange-800 dark:bg-orange-500/15 dark:text-orange-300">Adendo</span>
      {/if}
      {#if item.data.archived}
        <span class="chip bg-zinc-200 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">Arquivado</span>
      {/if}
      <span class="flex-1"></span>
      <span class="text-[11px] text-zinc-400" title="Revisão em que esta versão foi criada">desde a Rev. {item.rev_start}</span>
    </div>
    <h2 class="mt-1.5 font-display text-xl leading-snug font-semibold select-text">{item.data.title}</h2>
    <div class="mt-3 flex flex-wrap items-center gap-1.5">
      <span class="chip bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">{store.typeLabel(item.data.type)}</span>
      <Badges priority={item.data.priority} prefix />
      <span class="chip bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">Dificuldade {item.data.difficulty}/5</span>
      <Badges status={item.data.status} />
      {#each item.data.tags as t (t)}
        <span class="chip text-zinc-500 ring-1 ring-zinc-200 ring-inset dark:text-zinc-400 dark:ring-zinc-700">#{t}</span>
      {/each}
    </div>

    <div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[12px] text-zinc-500">
      <span class="inline-flex items-center gap-1.5" title="Data de criação do item">
        <CalendarPlus size={13} class="text-zinc-400" /> Criado em {formatDate(item.created_at)}
      </span>
      <span class="inline-flex items-center gap-1.5" title="Data de controle: gravada como hoje ao exportar o HTML, ou definida aqui (não gera revisão)">
        <CalendarCheck size={13} class="text-zinc-400" /> Controle
        {#if controlEditable}
          <input
            type="date"
            class="input h-6 w-auto px-1.5 text-[12px]"
            value={item.control_date ?? ""}
            onchange={(e) => setControlDate(item, (e.target as HTMLInputElement).value || null)}
          />
          {#if item.control_date !== todayIso()}
            <button class="text-accent-600 hover:underline dark:text-accent-400" onclick={() => setControlDate(item, todayIso())}>hoje</button>
          {/if}
        {:else}
          <span class="text-zinc-700 dark:text-zinc-300">{item.control_date ? formatDate(item.control_date) : "—"}</span>
        {/if}
      </span>
    </div>

    <div class="mt-4 flex flex-wrap gap-1.5">
      {#if editable}
        <button class="btn-primary h-7 px-2.5" onclick={() => editItem(item)}><Pencil size={13} /> Editar <span class="kbd ml-0.5 border-white/20 bg-white/15 text-white/80">E</span></button>
      {:else}
        <button class="btn-secondary h-7 px-2.5" onclick={() => editItem(item)}><Eye size={13} /> Ver</button>
      {/if}
      {#if listEditable}
        <button class="btn-secondary h-7 px-2.5" onclick={() => newAddendum(item)} title="Criar adendo (D)">
          <GitBranchPlus size={13} /> Adendo
        </button>
        <button class="btn-secondary h-7 px-2.5" onclick={() => toggleArchive([item])} title="Arquivar / restaurar (Z)">
          {#if item.data.archived}<ArchiveRestore size={13} /> Restaurar{:else}<Archive size={13} /> Arquivar{/if}
        </button>
      {/if}
      <button class="btn-ghost h-7 px-2.5" onclick={() => copyItemMarkdown(item)} title="Copiar como Markdown">
        <Copy size={13} /> Copiar
      </button>
      {#if listEditable}
        <button class="btn-ghost h-7 px-2.5 hover:text-red-600" onclick={() => deleteItem(item)} title="Excluir (Del)">
          <Trash2 size={13} />
        </button>
      {/if}
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-6 pt-5 pb-24">
    {#if item.is_addendum}
      <div class="mb-5 rounded-lg border-l-3 border-orange-400 bg-orange-50/70 px-3.5 py-2.5 text-[13px] dark:bg-orange-500/10">
        <span class="font-medium text-orange-800 dark:text-orange-300">Adendo de</span>
        {#if original}
          <button class="code ml-1 text-[12.5px] text-accent-600 hover:underline dark:text-accent-400" onclick={() => go(original!)}>
            {original.code}
          </button>
          <span class="text-zinc-700 dark:text-zinc-300">— {original.data.title}</span>
          <span class="text-zinc-500">({store.statusLabel(original.data.status)}{original.data.archived ? ", arquivado" : ""})</span>
        {:else}
          <span class="code ml-1">{item.parent_code}</span> <span class="text-zinc-500">(excluído)</span>
        {/if}
      </div>
    {/if}

    {#if addenda.length}
      <div class="mb-5 rounded-lg border border-dashed border-zinc-300 px-3.5 py-2.5 dark:border-zinc-700">
        <div class="mb-1 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">Adendos</div>
        {#each addenda as a (a.item_id)}
          <button class="flex w-full items-center gap-2 rounded px-1 py-1 text-left text-[13px] hover:bg-zinc-100 dark:hover:bg-zinc-800" onclick={() => go(a)}>
            <CornerDownRight size={13} class="text-zinc-400" />
            <span class="code">{a.code}</span>
            <span class="min-w-0 flex-1 truncate">{a.data.title}</span>
            <Badges status={a.data.status} />
            {#if a.data.archived}<span class="text-[11px] text-zinc-400">arquivado</span>{/if}
          </button>
        {/each}
      </div>
    {/if}

    {#if item.data.archived}
      <p class="mb-5 rounded-lg bg-zinc-100 px-3.5 py-2.5 text-[13px] text-zinc-600 dark:bg-zinc-800/70 dark:text-zinc-400">
        <strong>Arquivado</strong> — fora da lista normal e das exportações. Restaure para editar.
      </p>
    {/if}

    {#if item.source_note_id}
      <button
        class="mb-5 flex items-center gap-1.5 text-[12.5px] text-zinc-500 hover:text-accent-600"
        onclick={() => openNote(item.source_note_id!)}
      >
        <NotebookPen size={13} /> Criado a partir de uma anotação
      </button>
    {/if}

    <Markdown source={item.data.body} />
  </div>
</div>
