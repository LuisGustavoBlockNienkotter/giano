<script lang="ts">
  // One Markdown file for a conversation with Claude: project context, the current list,
  // the selected notes and the analysis request. Saved to a file and copied.
  import { untrack } from "svelte";
  import { Sparkles, ClipboardCheck, FolderOpen } from "@lucide/svelte";
  import Modal from "../lib/components/Modal.svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import type { ExportOutcome } from "../lib/types";

  let { noteIds: noteIdsProp }: { noteIds: number[] | null } = $props();
  const noteIds = untrack(() => noteIdsProp);

  const project = store.project!;
  let allNotes = $state(false);
  let since = $state("");
  let includeUnassigned = $state(false);
  let result = $state<ExportOutcome | null>(null);
  let copied = $state(false);

  async function run() {
    const outcome = await store.withBusy("Gerando a exportação…", () =>
      api.exportClaude({
        projectCode: project.code,
        since: noteIds ? null : since || null,
        allNotes: noteIds ? false : allNotes,
        includeUnassigned: noteIds ? false : includeUnassigned,
        noteIds,
      }),
    );
    if (!outcome) return;
    result = outcome;
    try {
      await navigator.clipboard.writeText(outcome.content ?? "");
      copied = true;
    } catch (e) {
      store.toast(`Não foi possível copiar: ${errorMessage(e)}`, "error");
    }
  }
</script>

<Modal
  title="Exportar para o Claude"
  subtitle="{project.code} · contexto, lista atual, anotações e o pedido de análise (editável nas configurações)"
  onclose={() => (store.exportClaude = null)}
>
  {#if result}
    <div class="px-5 py-6">
      <div class="flex items-start gap-3">
        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent-100 text-accent-700 dark:bg-accent-500/15 dark:text-accent-300">
          <ClipboardCheck size={18} />
        </div>
        <div class="min-w-0">
          <p class="text-[13px] font-medium">{copied ? "Copiado para a área de transferência" : "Arquivo gerado"}</p>
          <p class="mt-0.5 text-[12.5px] text-zinc-500 select-text">{result.summary}</p>
          <p class="mt-2 text-[12.5px] text-zinc-500">Cole numa conversa com o Claude.</p>
        </div>
      </div>
    </div>
  {:else}
    <div class="space-y-3 px-5 py-4 text-[13px]">
      {#if noteIds}
        <p>Exporta {noteIds.length === 1 ? "a anotação selecionada" : `${noteIds.length} anotações selecionadas`} junto com a lista atual de {project.code}.</p>
      {:else}
        <label class="flex items-center gap-2">
          <input type="checkbox" class="accent-accent-600" bind:checked={allNotes} />
          Todas as anotações <span class="text-xs text-zinc-400">(padrão: só as pendentes)</span>
        </label>
        <label class="flex items-center gap-2">
          <input type="checkbox" class="accent-accent-600" bind:checked={includeUnassigned} />
          Incluir anotações sem projeto
        </label>
        <div class="flex items-center gap-2">
          <span class="text-zinc-600 dark:text-zinc-400">Desde</span>
          <input type="date" class="input h-7 w-40" bind:value={since} />
          {#if since}<button class="text-xs text-accent-600 hover:underline" onclick={() => (since = "")}>limpar</button>{/if}
        </div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if result}
      <button class="btn-ghost mr-auto" onclick={() => api.revealPath(result!.path)}><FolderOpen size={14} /> Mostrar na pasta</button>
      <button class="btn-primary" onclick={() => (store.exportClaude = null)}>Fechar</button>
    {:else}
      <button class="btn-secondary" onclick={() => (store.exportClaude = null)}>Cancelar</button>
      <button class="btn-primary" onclick={run}><Sparkles size={14} /> Gerar e copiar</button>
    {/if}
  {/snippet}
</Modal>
