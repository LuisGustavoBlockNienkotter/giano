<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Modal from "../lib/components/Modal.svelte";
  import Markdown from "../lib/components/Markdown.svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import type { Project } from "../lib/types";

  let { project }: { project: Project | null } = $props();

  // The form starts from the project as it was when the dialog opened.
  const initial = untrack(() => project);
  let code = $state(initial?.code ?? "");
  let name = $state(initial?.name ?? "");
  let description = $state(initial?.description ?? "");
  let active = $state(initial?.active ?? true);
  let preview = $state(false);
  let saving = $state(false);
  let codeInput: HTMLInputElement | undefined = $state();

  onMount(() => codeInput?.focus());

  async function save() {
    saving = true;
    try {
      const p = initial
        ? await api.updateProject(initial.id, code, name, description, active)
        : await api.createProject(code, name, description);
      store.projectDialog = null;
      store.toast(initial ? `Projeto ${p.code} salvo.` : `Projeto ${p.code} criado.`);
      await store.loadProjects();
      await store.selectProject(p.id);
      if (!initial) store.view = "items";
    } catch (e) {
      store.toast(errorMessage(e), "error");
    } finally {
      saving = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div onkeydown={keydown}>
  <Modal
    title={project ? `Projeto ${project.code}` : "Novo projeto"}
    subtitle="A descrição em Markdown é o contexto do sistema: aparece no HTML e na exportação para o Claude."
    width="max-w-2xl"
    onclose={() => (store.projectDialog = null)}
  >
    <div class="space-y-4 px-5 py-4">
      <div class="grid grid-cols-[140px_1fr] gap-3">
        <div>
          <label class="field-label" for="pcode">Sigla</label>
          <input id="pcode" bind:this={codeInput} class="input font-mono uppercase" bind:value={code} placeholder="MERC" maxlength="10" />
        </div>
        <div>
          <label class="field-label" for="pname">Nome</label>
          <input id="pname" class="input" bind:value={name} placeholder="Mercurius" />
        </div>
      </div>
      {#if project && project.code !== code.trim().toUpperCase() && code.trim()}
        <p class="text-xs text-amber-700 dark:text-amber-400">
          Os identificadores dos itens passam a usar a nova sigla ({code.trim().toUpperCase()}-1, …).
        </p>
      {/if}
      <div>
        <div class="mb-1 flex items-center">
          <span class="field-label mb-0 flex-1">Descrição (Markdown)</span>
          <button class="text-xs text-accent-600 hover:underline" onclick={() => (preview = !preview)}>
            {preview ? "Escrever" : "Visualizar"}
          </button>
        </div>
        {#if preview}
          <div class="min-h-[220px] rounded-md border border-zinc-200 px-4 py-3 dark:border-zinc-800">
            <Markdown source={description} />
          </div>
        {:else}
          <textarea class="input min-h-[220px] font-mono text-[13px]" bind:value={description} placeholder="Stack, integrações, público, decisões importantes…"></textarea>
        {/if}
      </div>
      {#if project}
        <label class="flex items-center gap-2 text-[13px]">
          <input type="checkbox" class="accent-accent-600" bind:checked={active} />
          Projeto ativo <span class="text-xs text-zinc-400">(arquivado = somente leitura, fica no fim da lista)</span>
        </label>
      {/if}
    </div>
    {#snippet footer()}
      <button class="btn-secondary" onclick={() => (store.projectDialog = null)}>Cancelar</button>
      <button class="btn-primary" onclick={save} disabled={saving || !code.trim() || !name.trim()}>
        {project ? "Salvar" : "Criar projeto"}
      </button>
    {/snippet}
  </Modal>
</div>
