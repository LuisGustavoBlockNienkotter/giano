<script lang="ts">
  // Result of an "Atualizar e verificar": Claude's summary and the items the commits touched.
  // Every item code opens the status menu. The status changes made here go into ONE revision:
  // opened on the first change (unless the project already has an open revision, which is
  // then used and left open) and closed as "Pull abc..def" when this dialog closes.
  import { untrack } from "svelte";
  import { GitCommitVertical, FileCode, ExternalLink, CircleHelp } from "@lucide/svelte";
  import Modal from "../lib/components/Modal.svelte";
  import Markdown from "../lib/components/Markdown.svelte";
  import Menu from "../lib/components/Menu.svelte";
  import Badges, { STATUS_DOT } from "../lib/components/Badges.svelte";
  import { api, errorMessage } from "../lib/api";
  import { setStatus } from "../lib/actions";
  import { store } from "../lib/state.svelte";
  import { formatDate } from "../lib/format";
  import type { ItemView, RepoCheck, StatusValue, TouchRelation } from "../lib/types";

  let { check }: { check: RepoCheck } = $props();
  const c = untrack(() => check);

  const short = (s: string) => s.slice(0, 7);
  const range = c.from_commit ? `${short(c.from_commit)}..${short(c.to_commit)}` : short(c.to_commit);
  const revisionSummary = `Pull ${range}`;

  const RELATION: Record<TouchRelation, { label: string; class: string; suggest: StatusValue | null }> = {
    resolves: { label: "Resolve", class: "bg-emerald-100 text-emerald-800 dark:bg-emerald-500/15 dark:text-emerald-300", suggest: "done" },
    advances: { label: "Avança", class: "bg-violet-100 text-violet-800 dark:bg-violet-500/15 dark:text-violet-300", suggest: "in_progress" },
    related: { label: "Relacionado", class: "bg-zinc-100 text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300", suggest: null },
    cited: { label: "Citado no commit", class: "bg-sky-100 text-sky-800 dark:bg-sky-500/15 dark:text-sky-300", suggest: null },
  };

  /** This dialog opened the revision (and closes it). */
  let openedRevision = $state<number | null>(null);
  let target = $state<ItemView | null>(null);
  let suggested = $state<StatusValue | null>(null);
  let menu: ReturnType<typeof Menu> | undefined = $state();
  let showCommits = $state(false);

  let webUrl = $derived(store.repoInfo?.project_id === c.project_id ? store.repoInfo.web_url : null);

  /** The item as it is now (the list may have changed since the check). */
  function live(itemId: number): ItemView | null {
    return store.pv?.project.id === c.project_id ? (store.pv.items.find((v) => v.item_id === itemId) ?? null) : null;
  }

  function openStatus(itemId: number | null, code: string, relation: TouchRelation | null = null) {
    const item = itemId != null ? live(itemId) : (store.pv?.items.find((v) => v.code === code) ?? null);
    if (!item) {
      store.toast(`${code} não está na lista atual.`, "info");
      return;
    }
    target = item;
    const rel = relation ?? c.result.items.find((t) => t.item_id === item.item_id)?.relation ?? null;
    suggested = rel ? RELATION[rel].suggest : null;
    queueMicrotask(() => menu?.show());
  }

  let statusItems = $derived(
    (store.meta?.statuses ?? []).map((s) => ({
      value: s.value,
      label: s.label,
      dot: STATUS_DOT[s.value as StatusValue],
      current: target?.data.status === s.value,
      hint: s.value === suggested && target?.data.status !== s.value ? "sugerido" : undefined,
    })),
  );

  async function applyStatus(value: string) {
    const item = target;
    if (!item || item.data.status === value) return;
    if (openedRevision == null && !(store.pv?.rev === store.pv?.latest_rev && store.pv?.revision?.is_open)) {
      try {
        const rev = await api.openRevision(c.project_id, revisionSummary);
        openedRevision = rev.number;
      } catch (e) {
        // Opened meanwhile (another window or computer): the change goes into it.
        if (!/já está aberta/.test(errorMessage(e))) {
          store.toast(errorMessage(e), "error");
          return;
        }
      }
    }
    await setStatus([item], value as StatusValue);
  }

  async function close() {
    store.checkDialog = null;
    if (openedRevision == null) return;
    try {
      const rev = await api.closeRevision(c.project_id, revisionSummary);
      store.toast(rev ? `Rev. ${rev.number} fechada: ${revisionSummary}.` : "Nenhum status mudou: a revisão aberta foi descartada.", rev ? "success" : "info");
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
    await store.loadProject();
  }

  function openFile(path: string) {
    api.openRepoFile(c.project_id, path, null).catch((e) => store.toast(errorMessage(e), "error"));
  }

  function openCommit(sha: string) {
    if (webUrl) api.openPath(`${webUrl}/commit/${sha}`).catch((e) => store.toast(errorMessage(e), "error"));
  }

  const subtitle = [
    `Branch ${c.branch || "?"}`,
    `${c.commit_count} commit${c.commit_count === 1 ? "" : "s"}${c.result.truncated ? " (os mais recentes)" : ""}`,
    formatDate(c.created_at),
    c.result.cost_usd != null ? `US$ ${c.result.cost_usd.toFixed(2)}` : "",
  ]
    .filter(Boolean)
    .join(" · ");
</script>

<Modal title="Verificação {range}" {subtitle} width="max-w-3xl" onclose={close}>
  <div class="space-y-5 px-5 py-4">
    {#if openedRevision != null}
      <div class="flex items-center gap-2 rounded-md border border-sky-200 bg-sky-50 px-3 py-2 text-[12.5px] text-sky-900 dark:border-sky-500/20 dark:bg-sky-500/10 dark:text-sky-200">
        <GitCommitVertical size={14} />
        As mudanças de status estão na Rev. {openedRevision}, que será fechada como "{revisionSummary}" ao fechar esta janela.
      </div>
    {/if}

    <section>
      <h3 class="mb-1.5 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">Resumo</h3>
      <Markdown source={c.summary} empty="O Claude não escreveu um resumo." projectId={c.project_id} onitem={(code) => openStatus(null, code)} />
    </section>

    <section>
      <h3 class="mb-1.5 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">
        Itens tocados ({c.result.items.length})
      </h3>
      {#if c.result.items.length}
        <ul class="divide-y divide-zinc-100 rounded-lg border border-zinc-200 dark:divide-zinc-800 dark:border-zinc-800">
          {#each c.result.items as t (t.item_id)}
            {@const item = live(t.item_id)}
            <li class="px-3 py-2.5">
              <div class="flex items-center gap-2">
                <button
                  class="rounded bg-accent-50 px-1.5 py-0.5 font-mono text-[12px] font-semibold text-accent-700 ring-1 ring-accent-200 hover:bg-accent-100 dark:bg-accent-500/15 dark:text-accent-300 dark:ring-accent-500/30"
                  title="Mudar o status"
                  onclick={() => openStatus(t.item_id, t.code, t.relation)}>{item?.code ?? t.code}</button
                >
                <span class="chip {RELATION[t.relation].class}">{RELATION[t.relation].label}</span>
                {#if t.cited && t.relation !== "cited"}<span class="chip bg-sky-100 text-sky-800 dark:bg-sky-500/15 dark:text-sky-300">citado no commit</span>{/if}
                <span class="min-w-0 flex-1 truncate text-[13px] font-medium">{item?.data.title ?? t.title}</span>
                {#if item}<Badges status={item.data.status} />{:else}<span class="text-xs text-zinc-400">fora da lista</span>{/if}
              </div>
              {#if t.reason}<p class="mt-1 text-[12.5px] text-zinc-600 dark:text-zinc-300">{t.reason}</p>{/if}
              {#if t.files.length || t.commits.length}
                <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                  {#each t.files as f (f)}
                    <button class="flex items-center gap-1 rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-[11px] text-zinc-600 hover:text-accent-700 dark:bg-zinc-800 dark:text-zinc-300" title="Abrir no editor" onclick={() => openFile(f)}>
                      <FileCode size={11} />{f}
                    </button>
                  {/each}
                  {#each t.commits as sha (sha)}
                    <button class="font-mono text-[11px] text-zinc-400 {webUrl ? 'hover:text-accent-600 hover:underline' : 'cursor-default'}" title={webUrl ? "Abrir o commit" : sha} onclick={() => openCommit(sha)}>{sha}</button>
                  {/each}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="text-[13px] text-zinc-500">Nenhum item da lista foi tocado por estes commits.</p>
      {/if}
    </section>

    {#if c.result.unmatched.length}
      <section>
        <h3 class="mb-1.5 flex items-center gap-1.5 text-[11px] font-semibold tracking-wider text-zinc-400 uppercase">
          Sem item na lista <span title="Mudanças que não correspondem a nenhum item: crie um item se quiser acompanhá-las."><CircleHelp size={12} /></span>
        </h3>
        <ul class="list-disc space-y-0.5 pl-5 text-[13px] text-zinc-600 dark:text-zinc-300">
          {#each c.result.unmatched as u, i (i)}<li>{u}</li>{/each}
        </ul>
      </section>
    {/if}

    <section>
      <button class="text-[11px] font-semibold tracking-wider text-zinc-400 uppercase hover:text-zinc-600" onclick={() => (showCommits = !showCommits)}>
        {showCommits ? "▾" : "▸"} Commits ({c.result.commits.length})
      </button>
      {#if showCommits}
        <ul class="mt-1.5 space-y-1 text-[12.5px]">
          {#each c.result.commits as cm (cm.sha)}
            <li class="flex items-baseline gap-2">
              <button class="font-mono text-[11.5px] text-zinc-400 {webUrl ? 'hover:text-accent-600 hover:underline' : 'cursor-default'}" onclick={() => openCommit(cm.short)}>{cm.short}</button>
              <span class="min-w-0 flex-1">{cm.subject}</span>
              <span class="shrink-0 text-[11px] text-zinc-400">{cm.author} · {formatDate(cm.date)}</span>
              {#if webUrl}<ExternalLink size={11} class="shrink-0 text-zinc-300" />{/if}
            </li>
          {/each}
        </ul>
        {#if c.result.pull}
          <pre class="mt-2 rounded bg-zinc-50 p-2 text-[11px] whitespace-pre-wrap text-zinc-500 dark:bg-zinc-900">{c.result.pull}</pre>
        {/if}
      {/if}
    </section>
  </div>
  {#snippet footer()}
    <span class="flex-1 text-[11.5px] text-zinc-400">O Claude não altera itens: clique num código para mudar o status.</span>
    <button class="btn-primary" onclick={close}>Fechar</button>
  {/snippet}
</Modal>

<Menu bind:this={menu} items={statusItems} onselect={applyStatus} centered heading={target ? `Mudar status de ${target.code}` : ""} class="hidden" disabled={store.readOnly}>
  {#snippet trigger()}Status{/snippet}
</Menu>
