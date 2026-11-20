<script lang="ts" module>
  // Kept while switching views.
  const filter = $state<{ status: "pending" | "processed" | "archived" | "all"; allProjects: boolean }>({
    status: "pending",
    allProjects: false,
  });
</script>

<script lang="ts">
  // Quick notes for meetings: autosave while typing (nothing is lost), not versioned.
  import { onMount } from "svelte";
  import { Plus, Trash2, ListPlus, Sparkles, Eye, PenLine, Check, CloudOff, LoaderCircle, Link2 } from "@lucide/svelte";
  import Markdown from "../lib/components/Markdown.svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import { relativeDay, formatDate } from "../lib/format";
  import type { Note, NoteInput, NoteStatusValue } from "../lib/types";

  const STATUS_DOT: Record<NoteStatusValue, string> = {
    pending: "bg-amber-400",
    processed: "bg-emerald-500",
    archived: "bg-zinc-300 dark:bg-zinc-600",
  };

  let notes = $state<Note[]>([]);
  let current = $state<Note | null>(null);
  let form = $state<NoteInput | null>(null);
  let links = $state<string[]>([]);
  let preview = $state(false);
  let saveState = $state<"saved" | "saving" | "error" | "idle">("idle");
  let savedAt = $state("");
  let lastSaved = "";
  let timer: ReturnType<typeof setTimeout> | undefined;
  let bodyArea: HTMLTextAreaElement | undefined = $state();
  let initialized = $state(false);

  async function loadList() {
    const statuses: NoteStatusValue[] = filter.status === "all" ? [] : [filter.status];
    const projectId = filter.allProjects ? null : store.projectId;
    try {
      notes = await api.listNotes(projectId, statuses, !filter.allProjects);
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function open(note: Note) {
    await flush();
    current = note;
    form = { name: note.name, note_date: note.note_date, project_id: note.project_id, body: note.body, status: note.status };
    lastSaved = JSON.stringify(form);
    saveState = "idle";
    preview = false;
    links = await api.noteLinks(note.id).catch(() => []);
  }

  async function openById(id: number) {
    const note = await api.getNote(id).catch(() => null);
    if (!note) return;
    if (filter.status !== "all" && note.status !== filter.status) filter.status = "all";
    if (!filter.allProjects && note.project_id != null && note.project_id !== store.projectId) filter.allProjects = true;
    await loadList();
    await open(note);
    setTimeout(() => bodyArea?.focus(), 30);
  }

  onMount(() => {
    const stop = store.onRefresh(async () => {
      await loadList();
      if (current) links = await api.noteLinks(current.id).catch(() => []);
    });
    (async () => {
      const pending = store.pendingNoteId;
      store.pendingNoteId = null;
      if (pending != null) {
        await openById(pending);
      } else {
        await loadList();
        if (notes.length && !current) await open(notes[0]);
      }
      initialized = true;
    })();
    return () => {
      stop();
      flush();
    };
  });

  // Quick note created while this view is open (Ctrl+N).
  $effect(() => {
    const id = store.pendingNoteId;
    if (initialized && id != null && current?.id !== id) {
      store.pendingNoteId = null;
      openById(id);
    }
  });

  // Changing the filter or the project reloads the list.
  $effect(() => {
    filter.status;
    filter.allProjects;
    store.projectId;
    loadList();
  });

  // Autosave: a short pause after typing.
  $effect(() => {
    if (!form) return;
    const snapshot = JSON.stringify(form);
    if (snapshot === lastSaved) return;
    clearTimeout(timer);
    timer = setTimeout(() => save(), 700);
  });

  async function save(): Promise<boolean> {
    clearTimeout(timer);
    if (!form || !current) return true;
    const snapshot = JSON.stringify(form);
    if (snapshot === lastSaved) return true;
    const id = current.id;
    const input = $state.snapshot(form) as NoteInput;
    saveState = "saving";
    try {
      const saved = await api.saveNote(id, input);
      lastSaved = snapshot;
      saveState = "saved";
      savedAt = new Date().toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
      const i = notes.findIndex((n) => n.id === id);
      if (i >= 0) notes[i] = saved;
      if (current?.id === id) current = saved;
      return true;
    } catch (e) {
      // Never drop what was typed: keep the text and try again.
      saveState = "error";
      store.toast(`Anotação não salva: ${errorMessage(e)}`, "error");
      timer = setTimeout(() => save(), 5000);
      return false;
    }
  }

  async function flush() {
    if (form && current && JSON.stringify(form) !== lastSaved) await save();
  }

  async function newNote() {
    await flush();
    try {
      const note = await api.createNote("", filter.allProjects ? null : store.projectId);
      if (filter.status !== "pending" && filter.status !== "all") filter.status = "pending";
      await loadList();
      await open(note);
      setTimeout(() => bodyArea?.focus(), 30);
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function remove() {
    if (!current) return;
    const ok = await store.confirm(`Excluir a anotação "${current.name}"? Não é possível desfazer.`, {
      title: "Excluir anotação",
      confirmLabel: "Excluir",
      danger: true,
    });
    if (!ok) return;
    try {
      clearTimeout(timer);
      await api.deleteNote(current.id);
      const idx = notes.findIndex((n) => n.id === current!.id);
      current = null;
      form = null;
      await loadList();
      const next = notes[Math.min(idx, notes.length - 1)];
      if (next) await open(next);
      store.toast("Anotação excluída.");
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function createItem() {
    if (!current || !form) return;
    await flush();
    const projectId = form.project_id ?? store.projectId;
    if (projectId == null) {
      store.toast("Escolha o projeto da anotação antes de criar um item.", "info");
      return;
    }
    const project = store.projects.find((p) => p.id === projectId);
    if (project && !project.active) {
      store.toast(`O projeto ${project.code} está arquivado.`, "info");
      return;
    }
    // The selected text becomes the item: first line = title, the rest = body.
    let title = "";
    let body = "";
    if (bodyArea && !preview) {
      const selected = bodyArea.value.slice(bodyArea.selectionStart, bodyArea.selectionEnd).trim();
      if (selected) {
        const [first, ...rest] = selected.split("\n");
        title = first.replace(/^[-*#\s]+/, "").trim().slice(0, 120);
        body = rest.join("\n").trim();
      }
    }
    store.editor = { mode: "new", projectId, sourceNoteId: current.id, initial: { title, body } };
  }

  function keydown(e: KeyboardEvent) {
    if (store.overlayOpen) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (mod && e.key.toLowerCase() === "o") {
      e.preventDefault();
      createItem();
    } else if (mod && e.key.toLowerCase() === "t") {
      e.preventDefault();
      preview = !preview;
    }
  }

  const statusTabs: { value: typeof filter.status; label: string }[] = [
    { value: "pending", label: "Pendentes" },
    { value: "processed", label: "Processadas" },
    { value: "archived", label: "Arquivadas" },
    { value: "all", label: "Todas" },
  ];

  function firstLine(body: string): string {
    return body.split("\n").map((l) => l.replace(/^[#>\-*\s]+/, "").trim()).find(Boolean) ?? "";
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="flex h-full min-h-0">
  <!-- list -->
  <section class="flex w-80 shrink-0 flex-col border-r border-zinc-200/80 dark:border-zinc-800">
    <header class="flex items-center gap-2 px-4 pt-4 pb-2">
      <h1 class="flex-1 font-display text-[17px] font-semibold">Anotações</h1>
      <button class="btn-primary h-7 px-2.5" onclick={newNote} title="Nova anotação (Ctrl+N)"><Plus size={14} /> Nova</button>
    </header>
    <div class="px-4 pb-2">
      <div class="flex rounded-lg bg-zinc-200/60 p-0.5 dark:bg-zinc-800">
        {#each statusTabs as t (t.value)}
          <button
            class="h-6 flex-1 rounded-md text-[12px] font-medium {filter.status === t.value ? 'bg-white shadow-xs dark:bg-zinc-700' : 'text-zinc-500'}"
            onclick={() => (filter.status = t.value)}>{t.label}</button
          >
        {/each}
      </div>
      <label class="mt-2 flex items-center gap-1.5 text-[12px] text-zinc-500">
        <input type="checkbox" class="accent-accent-600" bind:checked={filter.allProjects} />
        Todos os projetos {#if !filter.allProjects && store.project}<span class="text-zinc-400">(senão: {store.project.code} e sem projeto)</span>{/if}
      </label>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
      {#each notes as n (n.id)}
        <button
          class="mb-0.5 block w-full rounded-lg px-3 py-2 text-left {current?.id === n.id
            ? 'bg-white shadow-xs ring-1 ring-zinc-200 dark:bg-zinc-800 dark:ring-zinc-700'
            : 'hover:bg-zinc-200/50 dark:hover:bg-zinc-900'}"
          onclick={() => open(n)}
        >
          <div class="flex items-center gap-2">
            <span class="h-1.5 w-1.5 shrink-0 rounded-full {STATUS_DOT[n.status]}"></span>
            <span class="min-w-0 flex-1 truncate text-[13px] font-medium">{current?.id === n.id && form ? form.name : n.name}</span>
            <span class="shrink-0 text-[11px] text-zinc-400">{relativeDay(n.note_date)}</span>
          </div>
          <div class="mt-0.5 flex items-center gap-1.5 pl-3.5 text-[12px] text-zinc-500">
            {#if n.project_code}<span class="code text-[10.5px]">{n.project_code}</span>{/if}
            <span class="truncate">{firstLine(current?.id === n.id && form ? form.body : n.body) || "Vazia"}</span>
          </div>
        </button>
      {:else}
        <p class="px-3 py-10 text-center text-[13px] text-zinc-400">Nenhuma anotação aqui.<br />Crie uma com <span class="kbd">Ctrl N</span>.</p>
      {/each}
    </div>
  </section>

  <!-- editor -->
  <section class="flex min-w-0 flex-1 flex-col">
    {#if current && form}
      <div class="border-b border-zinc-200/80 px-6 pt-4 pb-3 dark:border-zinc-800">
        <div class="flex items-center gap-2">
          <input
            class="min-w-0 flex-1 bg-transparent font-display text-xl font-semibold outline-none placeholder:text-zinc-300"
            bind:value={form.name}
            placeholder="Nome da reunião"
          />
          <span class="flex items-center gap-1 text-[12px] text-zinc-400">
            {#if saveState === "saving"}
              <LoaderCircle size={13} class="animate-spin" /> Salvando…
            {:else if saveState === "error"}
              <CloudOff size={13} class="text-red-500" /> <span class="text-red-500">Não salvo — tentando de novo</span>
            {:else if saveState === "saved"}
              <Check size={13} class="text-emerald-500" /> Salvo às {savedAt}
            {:else}
              Salvamento automático
            {/if}
          </span>
        </div>
        <div class="mt-3 flex flex-wrap items-center gap-2">
          <input type="date" class="input h-7 w-36" bind:value={form.note_date} />
          <select class="input h-7 w-auto" bind:value={form.project_id}>
            <option value={null}>Sem projeto</option>
            {#each store.projects as p (p.id)}<option value={p.id}>{p.code} — {p.name}</option>{/each}
          </select>
          <select class="input h-7 w-auto" bind:value={form.status}>
            {#each store.meta?.note_statuses ?? [] as s (s.value)}<option value={s.value}>{s.label}</option>{/each}
          </select>
          <span class="flex-1"></span>
          <button class="btn-ghost h-7 px-2" onclick={() => (preview = !preview)} title="Escrever / visualizar (Ctrl+T)">
            {#if preview}<PenLine size={13} /> Escrever{:else}<Eye size={13} /> Visualizar{/if}
          </button>
          <button class="btn-secondary h-7 px-2.5" onclick={createItem} title="Criar um item a partir desta anotação; o texto selecionado vira título e corpo (Ctrl+O)">
            <ListPlus size={13} /> Criar item
          </button>
          <button class="btn-secondary h-7 px-2.5" onclick={async () => { await flush(); store.exportClaude = { noteIds: [current!.id] }; }} title="Exportar esta anotação para o Claude">
            <Sparkles size={13} /> Claude
          </button>
          <button class="btn-ghost h-7 px-2 hover:text-red-600" onclick={remove} title="Excluir anotação"><Trash2 size={13} /></button>
        </div>
        {#if links.length}
          <div class="mt-2 flex items-center gap-1.5 text-[12px] text-zinc-500">
            <Link2 size={12} /> Itens criados desta anotação:
            {#each links as code (code)}<span class="code rounded bg-zinc-100 px-1.5 dark:bg-zinc-800">{code}</span>{/each}
          </div>
        {/if}
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto">
        {#if preview}
          <div class="px-8 py-6"><Markdown source={form.body} empty="Anotação vazia." /></div>
        {:else}
          <textarea
            bind:this={bodyArea}
            class="block h-full w-full resize-none bg-transparent px-8 py-6 font-mono text-[13.5px] leading-relaxed outline-none placeholder:text-zinc-400"
            bind:value={form.body}
            spellcheck="true"
            placeholder="Escreva livremente em Markdown. Tudo é salvo enquanto você digita."
          ></textarea>
        {/if}
      </div>
      <div class="border-t border-zinc-200/80 px-6 py-1.5 text-[11px] text-zinc-400 dark:border-zinc-800">
        Criada em {formatDate(current.created_at)} · atualizada em {formatDate(current.updated_at)}
      </div>
    {:else}
      <div class="flex h-full flex-col items-center justify-center gap-3 text-[13px] text-zinc-400">
        <p>Selecione ou crie uma anotação.</p>
        <button class="btn-secondary" onclick={newNote}><Plus size={14} /> Nova anotação</button>
      </div>
    {/if}
  </section>
</div>
