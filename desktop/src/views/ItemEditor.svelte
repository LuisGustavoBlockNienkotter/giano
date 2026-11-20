<script lang="ts">
  // Item editor: new item, edit, addendum, or read-only view. Ctrl+S saves, Esc closes
  // (asking before dropping unsaved changes). If the item changed elsewhere since the editor
  // opened (TUI, another computer), saving asks before overwriting.
  import { onMount, untrack } from "svelte";
  import { Eye, PenLine, Columns2, CornerDownRight, ChevronRight } from "@lucide/svelte";
  import Modal from "../lib/components/Modal.svelte";
  import Markdown from "../lib/components/Markdown.svelte";
  import TagInput from "../lib/components/TagInput.svelte";
  import Badges from "../lib/components/Badges.svelte";
  import { api, errorMessage, isConflict } from "../lib/api";
  import { store, emptyItem, type EditorRequest } from "../lib/state.svelte";
  import type { ItemData, ItemView } from "../lib/types";

  let { request }: { request: EditorRequest } = $props();
  // The request of this editor; the prop turns null while the editor closes.
  const req: EditorRequest = untrack(() => request);

  function initial(): ItemData {
    switch (req.mode) {
      case "new":
        return { ...emptyItem(), ...req.initial };
      case "edit":
      case "view":
        return $state.snapshot(req.item.data) as ItemData;
      case "addendum": {
        // Starts as an adjustment with the priority, difficulty and tags of the original.
        const o = req.original ?? req.base;
        return { ...emptyItem(), type: "adjustment", priority: o.data.priority, difficulty: o.data.difficulty, tags: [...o.data.tags] };
      }
    }
  }

  const MODES = [
    ["write", PenLine, "Escrever"],
    ["split", Columns2, "Lado a lado"],
    ["preview", Eye, "Visualizar"],
  ] as const;

  const start = initial();
  let data = $state<ItemData>(structuredClone(start));
  let mode = $state<"write" | "split" | "preview">("write");
  let saving = $state(false);
  let showOriginal = $state(false);
  let titleInput: HTMLInputElement | undefined = $state();

  let readOnly = $derived(req.mode === "view");
  let original: ItemView | null = $derived(
    req.mode === "addendum"
      ? req.original
      : req.mode === "edit" || req.mode === "view"
        ? req.item.is_addendum
          ? (store.pv?.items.find((v) => v.number === req.item.number && v.addendum === 0) ?? null)
          : null
        : null,
  );
  let dirty = $derived(JSON.stringify(data) !== JSON.stringify(start));

  let title = $derived.by(() => {
    switch (req.mode) {
      case "new":
        return req.sourceNoteId ? "Novo item a partir da anotação" : "Novo item";
      case "edit":
        return `Editar ${req.item.code}`;
      case "view":
        return `${req.item.code} (somente leitura)`;
      case "addendum": {
        const o = req.original ?? req.base;
        return `Novo adendo de ${o.is_addendum ? o.parent_code : o.code}`;
      }
    }
  });

  let subtitle = $derived(
    req.mode === "addendum"
      ? "Um item próprio, com o número do original: o texto do original continua intacto."
      : req.mode === "view"
        ? "Itens arquivados e revisões antigas não podem ser editados."
        : "",
  );

  onMount(() => {
    if (!readOnly) titleInput?.focus();
    else mode = "preview";
  });

  async function close() {
    if (dirty && !readOnly) {
      const ok = await store.confirm("Descartar as alterações não salvas?", {
        title: "Fechar o editor",
        confirmLabel: "Descartar",
        danger: true,
      });
      if (!ok) return;
    }
    store.editor = null;
  }

  async function save(force = false) {
    if (readOnly || saving) return;
    saving = true;
    const payload = $state.snapshot(data) as ItemData;
    try {
      let saved: ItemView | null = null;
      if (req.mode === "new") {
        saved = req.sourceNoteId
          ? await api.createItemFromNote(req.sourceNoteId, req.projectId, payload)
          : await api.createItem(req.projectId, payload);
        store.toast(`${saved.code} criado (Rev. ${saved.rev_start}).`);
      } else if (req.mode === "addendum") {
        saved = await api.createAddendum(req.base.item_id, payload);
        store.toast(`${saved.code} criado (Rev. ${saved.rev_start}).`);
      } else if (req.mode === "edit") {
        const r = await api.updateItem(req.item.item_id, payload, req.item.version_id, force);
        saved = r.item;
        store.toast(r.summary ? `Rev. ${r.item?.rev_start} — ${r.summary}` : "Nenhuma alteração; nenhuma revisão criada.", r.item ? "success" : "info");
      }
      store.editor = null;
      if (req.mode === "new" && req.sourceNoteId) {
        // Created from the notes screen: stay there, refresh what it shows.
        await store.refresh(true);
      } else {
        await store.loadProject();
        if (saved) {
          store.showArchive = saved.data.archived;
          store.selectedItemId = saved.item_id;
        }
      }
    } catch (e) {
      if (isConflict(e)) {
        const ok = await store.confirm(`${errorMessage(e)}\n\nSalvar mesmo assim, por cima da outra alteração?`, {
          title: "Item alterado em outro lugar",
          confirmLabel: "Sobrescrever",
          danger: true,
        });
        saving = false;
        if (ok) await save(true);
        return;
      }
      store.toast(errorMessage(e), "error");
    } finally {
      saving = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (mod && e.key.toLowerCase() === "t") {
      e.preventDefault();
      mode = mode === "preview" ? "write" : "preview";
    } else if (mod && e.key.toLowerCase() === "o" && original) {
      e.preventDefault();
      showOriginal = !showOriginal;
    }
  }

  function onBodyKeydown(e: KeyboardEvent) {
    // Tab inserts two spaces (lists, code) instead of leaving the editor.
    if (e.key === "Tab" && !e.shiftKey) {
      e.preventDefault();
      const ta = e.target as HTMLTextAreaElement;
      const { selectionStart: s, selectionEnd: end } = ta;
      data.body = data.body.slice(0, s) + "  " + data.body.slice(end);
      requestAnimationFrame(() => ta.setSelectionRange(s + 2, s + 2));
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div onkeydown={keydown}>
  <Modal {title} {subtitle} width="max-w-5xl" onclose={close}>
    {#snippet header()}
      <div class="flex rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800">
        {#each MODES as [m, Icon, label] (m)}
          <button
            class="flex h-6 items-center gap-1 rounded-md px-2 text-xs {mode === m ? 'bg-white shadow-xs dark:bg-zinc-700' : 'text-zinc-500'}"
            onclick={() => (mode = m)}
            title="{label} (Ctrl+T alterna)"><Icon size={12} /> {label}</button
          >
        {/each}
      </div>
    {/snippet}

    <div class="space-y-4 px-5 py-4">
      {#if original}
        <div class="rounded-lg border-l-3 border-orange-400 bg-orange-50/70 text-[13px] dark:bg-orange-500/10">
          <button class="flex w-full items-center gap-2 px-3.5 py-2 text-left" onclick={() => (showOriginal = !showOriginal)}>
            <CornerDownRight size={14} class="text-orange-500" />
            <span class="font-medium text-orange-800 dark:text-orange-300">Adendo de</span>
            <span class="code">{original.code}</span>
            <span class="min-w-0 flex-1 truncate text-zinc-700 dark:text-zinc-300">{original.data.title}</span>
            <Badges status={original.data.status} />
            <span class="kbd">Ctrl O</span>
            <ChevronRight size={14} class="transition-transform {showOriginal ? 'rotate-90' : ''}" />
          </button>
          {#if showOriginal}
            <div class="max-h-64 overflow-y-auto border-t border-orange-200/70 px-4 py-3 dark:border-orange-500/20">
              <Markdown source={original.data.body} />
            </div>
          {/if}
        </div>
      {/if}

      <div>
        <label class="field-label" for="title">Título</label>
        <input
          id="title"
          bind:this={titleInput}
          class="input h-9 text-[14px] font-medium"
          bind:value={data.title}
          disabled={readOnly}
          placeholder="Ex.: Checkout em uma página"
        />
      </div>

      <div class="grid grid-cols-[1fr_1fr_auto_1.3fr] gap-3">
        <div>
          <label class="field-label" for="type">Tipo</label>
          <select id="type" class="input" bind:value={data.type} disabled={readOnly}>
            {#each store.meta?.item_types ?? [] as t (t.value)}<option value={t.value}>{t.label}</option>{/each}
          </select>
        </div>
        <div>
          <label class="field-label" for="priority">Prioridade</label>
          <select id="priority" class="input" bind:value={data.priority} disabled={readOnly}>
            {#each store.meta?.priorities ?? [] as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
          </select>
        </div>
        <div>
          <span class="field-label">Dificuldade</span>
          <div class="flex h-8 rounded-md border border-zinc-200 bg-white p-0.5 shadow-xs dark:border-zinc-700 dark:bg-zinc-900">
            {#each [1, 2, 3, 4, 5] as n (n)}
              <button
                class="w-7 rounded text-[13px] font-medium {data.difficulty === n
                  ? 'bg-accent-600 text-white'
                  : 'text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800'}"
                disabled={readOnly}
                onclick={() => (data.difficulty = n)}>{n}</button
              >
            {/each}
          </div>
        </div>
        <div>
          <label class="field-label" for="status">Status</label>
          <select id="status" class="input" bind:value={data.status} disabled={readOnly}>
            {#each store.meta?.statuses ?? [] as s (s.value)}<option value={s.value}>{s.label}</option>{/each}
          </select>
        </div>
      </div>

      <div>
        <span class="field-label">Tags</span>
        <TagInput bind:tags={data.tags} suggestions={store.tags} disabled={readOnly} />
      </div>

      <div>
        <span class="field-label">Descrição (Markdown)</span>
        <div class="grid gap-3 {mode === 'split' ? 'grid-cols-2' : 'grid-cols-1'}">
          {#if mode !== "preview"}
            <textarea
              class="input min-h-[340px] resize-y font-mono text-[13px]"
              bind:value={data.body}
              disabled={readOnly}
              onkeydown={onBodyKeydown}
              spellcheck="true"
              placeholder="Contexto, critérios de aceite, exemplos… Markdown com tabelas e blocos de código."
            ></textarea>
          {/if}
          {#if mode !== "write"}
            <div class="min-h-[340px] overflow-y-auto rounded-md border border-zinc-200 px-4 py-3 dark:border-zinc-800 {mode === 'split' ? 'max-h-[52vh]' : ''}">
              <Markdown source={data.body} />
            </div>
          {/if}
        </div>
      </div>
    </div>

    {#snippet footer()}
      <span class="mr-auto text-xs text-zinc-400">
        {#if readOnly}Somente leitura{:else if dirty}Alterações não salvas{:else}Sem alterações{/if}
      </span>
      <button class="btn-secondary" onclick={close}>{readOnly ? "Fechar" : "Cancelar"}</button>
      {#if !readOnly}
        <button class="btn-primary" onclick={() => save()} disabled={saving}>
          {saving ? "Salvando…" : "Salvar"} <span class="kbd border-white/20 bg-white/15 text-white/80">Ctrl S</span>
        </button>
      {/if}
    {/snippet}
  </Modal>
</div>
