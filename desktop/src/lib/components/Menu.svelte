<script lang="ts">
  // A button with a popover list of options.
  import { tick, type Snippet } from "svelte";

  let {
    items,
    onselect,
    trigger,
    align = "left",
    disabled = false,
    title = "",
    class: klass = "btn-secondary",
  }: {
    items: { value: string; label: string; dot?: string; current?: boolean; hint?: string }[];
    onselect: (value: string) => void;
    trigger: Snippet;
    align?: "left" | "right";
    disabled?: boolean;
    title?: string;
    class?: string;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;
  let active = $state(0);
  let list: HTMLDivElement | undefined = $state();

  function toggle() {
    open = !open;
    active = Math.max(0, items.findIndex((i) => i.current));
    if (open) tick().then(() => list?.focus());
  }

  function pick(value: string) {
    open = false;
    onselect(value);
  }

  function keydown(e: KeyboardEvent) {
    if (!open) return;
    if (["Escape", "ArrowDown", "ArrowUp", "Enter"].includes(e.key) || /^[1-9]$/.test(e.key)) e.stopPropagation();
    if (e.key === "Escape") {
      e.stopPropagation();
      open = false;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      active = (active + 1) % items.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = (active - 1 + items.length) % items.length;
    } else if (/^[1-9]$/.test(e.key) && Number(e.key) <= items.length) {
      e.preventDefault();
      pick(items[Number(e.key) - 1].value);
    } else if (e.key === "Enter") {
      e.preventDefault();
      pick(items[active].value);
    }
  }

  export function show() {
    if (!disabled) toggle();
  }
</script>

<svelte:window
  onmousedown={(e) => {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }}
/>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="relative inline-block" bind:this={root} onkeydown={keydown}>
  <button class={klass} {disabled} {title} onclick={toggle} aria-haspopup="listbox" aria-expanded={open}>
    {@render trigger()}
  </button>
  {#if open}
    <div
      class="card absolute top-full z-30 mt-1 min-w-48 overflow-hidden py-1 shadow-xl {align === 'right'
        ? 'right-0'
        : 'left-0'}"
      role="listbox"
      tabindex="-1"
      bind:this={list}
    >
      {#each items as it, i (it.value)}
        <button
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] {i === active
            ? 'bg-zinc-100 dark:bg-zinc-800'
            : ''} {it.current ? 'font-semibold' : ''}"
          onmouseenter={() => (active = i)}
          onclick={() => pick(it.value)}
          role="option"
          aria-selected={it.current ?? false}
        >
          {#if it.dot}<span class="h-2 w-2 rounded-full {it.dot}"></span>{/if}
          <span class="flex-1">{it.label}</span>
          {#if it.hint}<span class="kbd">{it.hint}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>
