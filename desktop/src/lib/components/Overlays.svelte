<script lang="ts">
  // Confirm dialogs, toasts and the progress overlay (driven by `store`).
  import { CircleCheck, CircleAlert, Info, LoaderCircle } from "@lucide/svelte";
  import { store } from "../state.svelte";

  let confirmButtons: Record<number, HTMLButtonElement> = $state({});

  $effect(() => {
    const top = store.dialogs.at(-1);
    if (top) queueMicrotask(() => confirmButtons[top.id]?.focus());
  });
</script>

{#each store.dialogs as d (d.id)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/40 px-4 backdrop-blur-[2px] dark:bg-black/60"
    onkeydown={(e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        store.closeDialog(d.id, false);
      }
    }}
  >
    <div class="card w-full max-w-md p-5 shadow-2xl" role="alertdialog" aria-modal="true" aria-label={d.title}>
      <h2 class="font-display text-[15px] font-semibold">{d.title}</h2>
      <p class="mt-2 text-[13px] leading-relaxed whitespace-pre-line text-zinc-600 select-text dark:text-zinc-300">
        {d.message}
      </p>
      <div class="mt-5 flex justify-end gap-2">
        {#if d.cancelLabel}
          <button class="btn-secondary" onclick={() => store.closeDialog(d.id, false)}>{d.cancelLabel}</button>
        {/if}
        <button
          bind:this={confirmButtons[d.id]}
          class={d.danger ? "btn-danger" : "btn-primary"}
          onclick={() => store.closeDialog(d.id, true)}>{d.confirmLabel}</button
        >
      </div>
    </div>
  </div>
{/each}

{#if store.busy}
  <div class="fixed inset-0 z-[60] flex items-center justify-center bg-zinc-950/30 backdrop-blur-[1px] dark:bg-black/50">
    <div class="card w-80 p-5 shadow-2xl">
      <div class="flex items-center gap-2.5">
        <LoaderCircle size={18} class="animate-spin text-accent-600" />
        <span class="text-[13px] font-medium">{store.busy.title}</span>
      </div>
      {#if store.busy.total > 0}
        <div class="mt-4 h-1.5 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800">
          <div
            class="h-full rounded-full bg-accent-500 transition-[width] duration-200"
            style="width: {Math.round((store.busy.done / store.busy.total) * 100)}%"
          ></div>
        </div>
        <p class="mt-2 text-xs text-zinc-500">{store.busy.done} de {store.busy.total}</p>
      {/if}
    </div>
  </div>
{/if}

<div class="pointer-events-none fixed right-4 bottom-4 z-[70] flex w-96 flex-col items-end gap-2">
  {#each store.toasts as t (t.id)}
    <div
      class="card pointer-events-auto flex max-w-full items-start gap-2.5 px-3.5 py-2.5 text-[13px] shadow-lg select-text"
    >
      {#if t.kind === "error"}
        <CircleAlert size={16} class="mt-0.5 shrink-0 text-red-500" />
      {:else if t.kind === "success"}
        <CircleCheck size={16} class="mt-0.5 shrink-0 text-emerald-500" />
      {:else}
        <Info size={16} class="mt-0.5 shrink-0 text-accent-500" />
      {/if}
      <span class="min-w-0 break-words">{t.message}</span>
    </div>
  {/each}
</div>
