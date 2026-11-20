<script lang="ts">
  // Open a revision (every change to the project goes into it, from any screen or computer,
  // until it is closed: a block of changes = one revision) or close the open one.
  import { onMount, untrack } from "svelte";
  import Modal from "../lib/components/Modal.svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import { plural } from "../lib/format";

  let { mode }: { mode: "open" | "close" } = $props();

  const pv = untrack(() => store.pv)!;
  const open = untrack(() => (pv.revision?.is_open ? pv.revision : null));
  let summary = $state(untrack(() => (mode === "close" ? (open?.summary ?? "") : "")));
  let saving = $state(false);
  let input: HTMLInputElement | undefined = $state();

  onMount(() => input?.focus());

  function close() {
    store.revisionDialog = null;
  }

  async function submit() {
    if (saving) return;
    saving = true;
    try {
      if (mode === "open") {
        const r = await api.openRevision(pv.project.id, summary);
        store.toast(`Rev. ${r.number} aberta: as próximas alterações entram nela.`);
      } else {
        const r = await api.closeRevision(pv.project.id, summary);
        store.toast(
          r ? `Rev. ${r.number} fechada (${plural(r.changes.length, "item alterado", "itens alterados")}).`
            : `Rev. ${open?.number ?? ""} descartada: nenhuma alteração.`,
          r ? "success" : "info",
        );
      }
      close();
      await store.loadProject();
    } catch (e) {
      store.toast(errorMessage(e), "error");
      await store.loadProject(); // e.g. opened/closed on another computer meanwhile
    } finally {
      saving = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Enter" || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s")) {
      e.preventDefault();
      submit();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div onkeydown={keydown}>
  <Modal
    title={mode === "open" ? `Abrir a Rev. ${pv.latest_rev + 1}` : `Fechar a Rev. ${open?.number ?? ""}`}
    subtitle={mode === "open"
      ? `${pv.project.code} · todas as alterações seguintes no projeto (criar, editar, status, excluir…), desta tela ou de outro computador, entram nesta revisão até você fechá-la`
      : `${pv.project.code} · a revisão passa a ter a data de agora; alterações seguintes geram novas revisões`}
    onclose={close}
  >
    <div class="space-y-4 px-5 py-4">
      <div>
        <label class="field-label" for="rev-summary">Resumo {mode === "open" ? "(opcional; pode ficar para o fechamento)" : "(opcional)"}</label>
        <input
          id="rev-summary"
          bind:this={input}
          class="input"
          bind:value={summary}
          placeholder={mode === "open" ? "Ex.: Reunião com o cliente 20/11" : "Sem resumo: a única alteração, ou a contagem de itens"}
        />
      </div>
      {#if mode === "close" && open}
        {#if open.changes.length}
          <div>
            <p class="field-label">{plural(open.changes.length, "item alterado", "itens alterados")}</p>
            <ul class="max-h-64 space-y-1 overflow-y-auto rounded-md border border-zinc-200 px-3 py-2 text-[12.5px] text-zinc-600 select-text dark:border-zinc-800 dark:text-zinc-300">
              {#each open.changes as c (c.item_id)}<li>• {c.summary}</li>{/each}
            </ul>
          </div>
        {:else}
          <p class="text-[13px] text-zinc-500">Nenhuma alteração: a revisão será descartada.</p>
        {/if}
      {/if}
    </div>
    {#snippet footer()}
      <button class="btn-secondary" onclick={close}>Cancelar</button>
      <button class="btn-primary" onclick={submit} disabled={saving}>
        {#if mode === "open"}Abrir revisão{:else if open?.changes.length}Fechar revisão{:else}Descartar revisão{/if}
      </button>
    {/snippet}
  </Modal>
</div>
