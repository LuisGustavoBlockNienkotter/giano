<script lang="ts">
  // Self-contained HTML of the list at the revision shown, with item selection and the
  // option to mark the exported items as "Em desenvolvimento" (latest revision only).
  import { SvelteSet } from "svelte/reactivity";
  import { FileCode2, ExternalLink, FolderOpen, Filter } from "@lucide/svelte";
  import Modal from "../lib/components/Modal.svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import { compareKeys, CLOSED } from "../lib/format";
  import type { ExportOutcome, ItemView } from "../lib/types";

  const pv = store.pv!;
  const items: ItemView[] = pv.items.filter((v) => !v.data.archived).sort(compareKeys);
  const isLatest = pv.rev === pv.latest_rev;
  const MARKABLE = ["open", "analysis", "approved"];

  let selected = new SvelteSet<number>(items.map((v) => v.item_id));
  let changes = $state(true);
  let compareTo = $state<number | null>(null);
  let markInProgress = $state(false);
  let result = $state<ExportOutcome | null>(null);

  let all = $derived(selected.size === items.length);
  let toMark = $derived(items.filter((v) => selected.has(v.item_id) && MARKABLE.includes(v.data.status)));

  function selectOpen() {
    selected.clear();
    for (const v of items) if (!CLOSED.includes(v.data.status)) selected.add(v.item_id);
  }

  async function run() {
    const keys = all ? null : items.filter((v) => selected.has(v.item_id)).map((v) => v.key);
    const outcome = await store.withBusy(markInProgress ? "Exportando e marcando os itens…" : "Gerando o HTML…", () =>
      api.exportHtml({
        projectCode: pv.project.code,
        rev: pv.rev,
        changes,
        compareTo: changes ? compareTo : null,
        keys,
        markInProgress: isLatest && markInProgress,
      }),
    );
    if (!outcome) return;
    result = outcome;
    if (outcome.marked.length) await store.loadProject();
  }

  async function openFile() {
    if (!result) return;
    await api.openPath(result.path).catch((e) => store.toast(errorMessage(e), "error"));
  }

  function keydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s" && !result) {
      e.preventDefault();
      run();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div onkeydown={keydown}>
  <Modal
    title="Exportar HTML"
    subtitle="{pv.project.code} · Rev. {pv.rev}{isLatest ? '' : ` de ${pv.latest_rev}`} · arquivo autocontido para enviar a outras pessoas"
    width="max-w-2xl"
    onclose={() => (store.exportHtml = false)}
  >
    {#if result}
      <div class="space-y-4 px-5 py-6">
        <div class="flex items-start gap-3">
          <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-emerald-100 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300">
            <FileCode2 size={18} />
          </div>
          <div class="min-w-0">
            <p class="text-[13px] font-medium">Arquivo gerado</p>
            <p class="mt-0.5 text-[12.5px] break-all text-zinc-500 select-text">{result.path}</p>
            {#if result.marked.length}
              <p class="mt-2 text-[12.5px] text-violet-700 dark:text-violet-300">
                {result.marked.length} itens marcados como Em desenvolvimento: {result.marked.join(", ")}
              </p>
            {/if}
          </div>
        </div>
      </div>
    {:else}
      <div class="space-y-5 px-5 py-4">
        <div>
          <div class="mb-1.5 flex items-center gap-2">
            <span class="field-label mb-0 flex-1">Itens ({selected.size} de {items.length})</span>
            <button class="text-xs text-accent-600 hover:underline" onclick={() => items.forEach((v) => selected.add(v.item_id))}>Todos</button>
            <button class="text-xs text-accent-600 hover:underline" onclick={() => selected.clear()}>Nenhum</button>
            <button class="flex items-center gap-1 text-xs text-accent-600 hover:underline" onclick={selectOpen}><Filter size={11} /> Sem concluídos/descartados</button>
          </div>
          <div class="max-h-64 overflow-y-auto rounded-lg border border-zinc-200 dark:border-zinc-800">
            {#each items as v (v.item_id)}
              <label class="flex items-center gap-2.5 border-b border-zinc-100 px-3 py-1.5 text-[13px] last:border-0 hover:bg-zinc-50 dark:border-zinc-800/70 dark:hover:bg-zinc-800/50">
                <input
                  type="checkbox"
                  class="accent-accent-600"
                  checked={selected.has(v.item_id)}
                  onchange={(e) => ((e.target as HTMLInputElement).checked ? selected.add(v.item_id) : selected.delete(v.item_id))}
                />
                <span class="code w-20 shrink-0">{v.code}</span>
                <span class="min-w-0 flex-1 truncate">{v.data.title}</span>
                <span class="text-[11.5px] text-zinc-400">{store.statusLabel(v.data.status)}</span>
              </label>
            {:else}
              <p class="px-3 py-6 text-center text-[13px] text-zinc-400">Nenhum item nesta revisão.</p>
            {/each}
          </div>
        </div>

        <div class="space-y-2">
          <label class="flex items-center gap-2 text-[13px]">
            <input type="checkbox" class="accent-accent-600" bind:checked={changes} disabled={pv.rev === 0} />
            Incluir a seção "Alterações nesta revisão"
          </label>
          {#if changes && pv.rev > 0}
            <div class="ml-6 flex items-center gap-2 text-[12.5px] text-zinc-600 dark:text-zinc-400">
              Comparar com a
              <select class="input h-7 w-auto" bind:value={compareTo}>
                <option value={null}>revisão anterior (Rev. {pv.rev - 1})</option>
                {#each Array.from({ length: pv.rev }, (_, i) => pv.rev - 1 - i) as r (r)}
                  <option value={r}>Rev. {r}</option>
                {/each}
              </select>
            </div>
          {/if}
          {#if isLatest}
            <label class="flex items-start gap-2 text-[13px]">
              <input type="checkbox" class="mt-0.5 accent-accent-600" bind:checked={markInProgress} />
              <span>
                Marcar os itens exportados como <strong>Em desenvolvimento</strong>
                <span class="block text-xs text-zinc-500">
                  Só Aberto, Em análise e Aprovado mudam ({toMark.length} {toMark.length === 1 ? "item" : "itens"}; uma revisão por item). O arquivo mostra a lista
                  como estava antes. Se o arquivo não puder ser gravado, nada muda.
                </span>
              </span>
            </label>
          {/if}
        </div>
      </div>
    {/if}

    {#snippet footer()}
      {#if result}
        <button class="btn-ghost mr-auto" onclick={() => api.revealPath(result!.path)}><FolderOpen size={14} /> Mostrar na pasta</button>
        <button class="btn-secondary" onclick={() => (store.exportHtml = false)}>Fechar</button>
        <button class="btn-primary" onclick={openFile}><ExternalLink size={14} /> Abrir no navegador</button>
      {:else}
        <button class="btn-secondary" onclick={() => (store.exportHtml = false)}>Cancelar</button>
        <button class="btn-primary" onclick={run} disabled={selected.size === 0}>
          <FileCode2 size={14} /> Exportar <span class="kbd border-white/20 bg-white/15 text-white/80">Ctrl S</span>
        </button>
      {/if}
    {/snippet}
  </Modal>
</div>
