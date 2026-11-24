<script lang="ts">
  // Repository of the project on this computer: link a folder, "Atualizar e verificar"
  // (git pull + Claude) and the history of checks (each opens its result).
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    GitBranch,
    FolderOpen,
    RefreshCw,
    ScanSearch,
    Unlink,
    LoaderCircle,
    CircleAlert,
    ExternalLink,
    FolderGit2,
    Square,
  } from "@lucide/svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import { formatDate } from "../lib/format";
  import type { RepoCheck } from "../lib/types";

  let linking = $state(false);

  onMount(() => {
    store.loadRepo();
    return store.onRefresh(() => store.loadRepo());
  });

  let project = $derived(store.pv?.project ?? null);
  let info = $derived(store.repoInfo?.project_id === store.projectId ? store.repoInfo : null);
  let running = $derived(!!store.repoRun && store.repoRun.projectId === store.projectId);
  let otherRunning = $derived(!!store.repoRun && store.repoRun.projectId !== store.projectId);

  async function chooseFolder() {
    if (!project) return;
    const dir = await openDialog({ directory: true, defaultPath: info?.path ?? undefined, title: `Repositório de ${project.code}` });
    if (typeof dir !== "string") return;
    linking = true;
    try {
      store.repoInfo = await api.linkRepo(project.id, dir);
      store.toast(store.repoInfo.git ? `Repositório vinculado (${store.repoInfo.git.branch}).` : "Pasta vinculada (não é um repositório git).");
      await store.loadProjects();
    } catch (e) {
      store.toast(errorMessage(e), "error");
    } finally {
      linking = false;
    }
  }

  async function unlink() {
    if (!project) return;
    const ok = await store.confirm(
      `Desvincular a pasta ${info?.path} do projeto ${project.code} neste computador? Os arquivos não são tocados e o histórico de verificações continua.`,
      { title: "Desvincular repositório", confirmLabel: "Desvincular" },
    );
    if (!ok) return;
    try {
      await api.unlinkRepo(project.id);
      await store.loadRepo();
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function openCheck(c: RepoCheck) {
    if (store.viewRev != null) await store.showRevision(null);
    store.view = "repo";
    store.checkDialog = c;
  }

  const short = (s: string | null) => (s ? s.slice(0, 7) : "");
  const range = (c: RepoCheck) => (c.from_commit ? `${short(c.from_commit)}..${short(c.to_commit)}` : `até ${short(c.to_commit)}`);
  const firstLine = (md: string) =>
    md
      .split("\n")
      .map((l) => l.replace(/^[#>*\-\s]+/, "").replace(/[*_`]/g, "").trim())
      .find((l) => l) ?? "";
</script>

<div class="h-full overflow-y-auto">
  <div class="mx-auto max-w-3xl space-y-6 px-8 py-8">
    <div class="flex items-center gap-2.5">
      <FolderGit2 size={20} class="text-zinc-400" />
      <h1 class="font-display flex-1 text-xl font-semibold">Repositório {project ? `de ${project.code}` : ""}</h1>
    </div>

    {#if !info}
      <p class="text-sm text-zinc-400">Carregando…</p>
    {:else if !info.path}
      <section class="card p-5">
        <h2 class="text-[14px] font-semibold">Vincular um repositório local</h2>
        <p class="mt-1 text-[13px] text-zinc-500">
          Com a pasta do código do projeto vinculada, os itens podem citar arquivos dela
          (<code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">src/arquivo.rs:42</code> ou
          <code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">[texto](repo:src/arquivo.rs#L42)</code>), e, num repositório git,
          "Atualizar e verificar" faz o pull e pede ao Claude o resumo do que chegou e os itens tocados.
        </p>
        {#if info.remote}
          <p class="mt-2 text-[13px] text-sky-700 dark:text-sky-300">
            Este projeto usa o repositório <span class="font-mono select-text">{info.remote}</span>: aponte a pasta dele neste computador.
          </p>
        {/if}
        <p class="mt-1 text-xs text-zinc-400">A pasta vale só para este computador; o endereço do remote fica no banco, para os outros.</p>
        <button class="btn-primary mt-4" onclick={chooseFolder} disabled={linking || !project?.active}>
          <FolderOpen size={14} /> Escolher a pasta…
        </button>
      </section>
    {:else}
      <section class="card p-5">
        <div class="flex items-start gap-3">
          <div class="min-w-0 flex-1 space-y-1">
            <p class="truncate font-mono text-[13px] select-text" title={info.path}>{info.path}</p>
            {#if !info.path_exists}
              <p class="flex items-center gap-1.5 text-[13px] text-red-600"><CircleAlert size={14} /> A pasta não existe mais neste computador.</p>
            {:else if info.git}
              <p class="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-zinc-500">
                <span class="flex items-center gap-1"><GitBranch size={13} /> {info.git.branch}</span>
                <span class="font-mono">{short(info.git.head)}</span>
                {#if info.git.upstream}<span>↔ {info.git.upstream}</span>{:else}<span class="text-amber-600">sem branch remoto (o pull não funciona)</span>{/if}
                {#if info.remote}
                  {#if info.web_url}
                    <button class="flex items-center gap-1 hover:text-accent-600 hover:underline" onclick={() => api.openPath(info.web_url!)}>{info.remote} <ExternalLink size={11} /></button>
                  {:else}
                    <span class="font-mono select-text">{info.remote}</span>
                  {/if}
                {/if}
              </p>
              {#if info.git.dirty > 0}
                <p class="text-[12.5px] text-amber-700 dark:text-amber-400">
                  {info.git.dirty} arquivo{info.git.dirty === 1 ? "" : "s"} com mudanças não commitadas: o pull pode ser recusado.
                </p>
              {/if}
            {:else}
              <p class="text-[12.5px] text-zinc-500">A pasta não é um repositório git: os itens podem citar arquivos dela, mas não há verificação.</p>
            {/if}
          </div>
          <button class="btn-icon h-8 w-8" title="Abrir a pasta" onclick={() => info?.path && api.openPath(info.path)}><FolderOpen size={15} /></button>
          <button class="btn-icon h-8 w-8" title="Trocar a pasta" onclick={chooseFolder} disabled={linking || !project?.active}><RefreshCw size={15} /></button>
          <button class="btn-icon h-8 w-8" title="Desvincular neste computador" onclick={unlink}><Unlink size={15} /></button>
        </div>

        {#if info.git && info.path_exists}
          <div class="mt-4 border-t border-zinc-100 pt-4 dark:border-zinc-800">
            {#if !info.claude}
              <p class="mb-3 flex items-start gap-1.5 text-[12.5px] text-amber-700 dark:text-amber-400">
                <CircleAlert size={14} class="mt-0.5 shrink-0" />
                <span>
                  O Claude Code não foi encontrado neste computador. Instale-o, faça login rodando <code>claude</code> uma vez no terminal,
                  ou informe o caminho em <button class="underline" onclick={() => (store.view = "settings")}>Configurações</button>.
                </span>
              </p>
            {/if}
            {#if running}
              <div class="flex items-center gap-3 rounded-lg bg-accent-50 px-3.5 py-3 text-[13px] text-accent-900 dark:bg-accent-500/10 dark:text-accent-200">
                <LoaderCircle size={16} class="animate-spin" />
                <span class="flex-1">{store.repoRun?.message}</span>
                <button class="btn-secondary h-7" onclick={() => api.cancelRepoCheck()}><Square size={12} /> Cancelar</button>
              </div>
              <p class="mt-2 text-[11.5px] text-zinc-400">A análise pode levar alguns minutos. Dá para continuar usando o Giano enquanto isso.</p>
            {:else}
              <div class="flex flex-wrap items-center gap-2">
                <button class="btn-primary" onclick={() => store.runCheck(true)} disabled={otherRunning || !project?.active}>
                  <RefreshCw size={14} /> Atualizar e verificar
                </button>
                <button class="btn-secondary" onclick={() => store.runCheck(false)} disabled={otherRunning || !project?.active} title="Analisa os commits que já estão na pasta, sem git pull">
                  <ScanSearch size={14} /> Verificar sem atualizar
                </button>
                <span class="text-[11.5px] text-zinc-400">
                  {store.repoChecks.length ? `Analisa os commits desde ${short(store.repoChecks[0].to_commit)}.` : "Primeira verificação: os commits mais recentes."}
                  O Claude só lê: nenhum item é alterado.
                </span>
              </div>
            {/if}
          </div>
        {/if}
      </section>
    {/if}

    {#if store.repoChecks.length}
      <section>
        <h2 class="mb-2 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">Verificações</h2>
        <ul class="card divide-y divide-zinc-100 overflow-hidden dark:divide-zinc-800">
          {#each store.repoChecks as c (c.id)}
            <li>
              <button class="flex w-full items-start gap-3 px-4 py-3 text-left hover:bg-zinc-50 dark:hover:bg-zinc-800/50" onclick={() => openCheck(c)}>
                <div class="w-32 shrink-0 text-[12px] text-zinc-500">
                  <div>{formatDate(c.created_at)}</div>
                  <div class="font-mono text-[11px] text-zinc-400">{range(c)}</div>
                </div>
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px]">{firstLine(c.summary) || "Sem resumo."}</div>
                  <div class="mt-0.5 text-[11.5px] text-zinc-400">
                    {c.commit_count} commit{c.commit_count === 1 ? "" : "s"} · {c.result.items.length === 1 ? "1 item tocado" : `${c.result.items.length} itens tocados`}{c.result.items.length ? `: ${c.result.items.map((t) => t.code).join(", ")}` : ""}
                  </div>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</div>
