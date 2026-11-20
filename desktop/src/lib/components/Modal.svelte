<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";

  let {
    title,
    subtitle = "",
    width = "max-w-lg",
    onclose,
    children,
    footer,
    header,
  }: {
    title: string;
    subtitle?: string;
    width?: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
    header?: Snippet;
  } = $props();

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex items-start justify-center overflow-y-auto bg-zinc-950/40 px-4 pt-[8vh] pb-8 backdrop-blur-[2px] dark:bg-black/60"
  onkeydown={keydown}
  onmousedown={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    class="card flex max-h-[84vh] w-full {width} flex-col overflow-hidden shadow-2xl ring-1 ring-black/5"
    role="dialog"
    aria-modal="true"
    aria-label={title}
  >
    <div class="flex items-start gap-3 border-b border-zinc-100 px-5 py-3.5 dark:border-zinc-800">
      <div class="min-w-0 flex-1">
        <h2 class="font-display text-[15px] font-semibold">{title}</h2>
        {#if subtitle}<p class="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">{subtitle}</p>{/if}
      </div>
      {@render header?.()}
      <button class="btn-icon -mr-2 h-7 w-7" onclick={onclose} title="Fechar (Esc)"><X size={16} /></button>
    </div>
    <div class="min-h-0 flex-1 overflow-y-auto">
      {@render children()}
    </div>
    {#if footer}
      <div
        class="flex items-center justify-end gap-2 border-t border-zinc-100 bg-zinc-50/60 px-5 py-3 dark:border-zinc-800 dark:bg-zinc-900/60"
      >
        {@render footer()}
      </div>
    {/if}
  </div>
</div>
