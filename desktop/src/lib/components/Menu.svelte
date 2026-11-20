<script lang="ts">
  // A button with a list of options. By default the list drops down under the button;
  // with `centered` it opens centered on the screen over a backdrop (moved to <body>, so it
  // never changes the layout of the bar the button sits in).
  import { tick, type Snippet } from "svelte";

  let {
    items,
    onselect,
    trigger,
    align = "left",
    disabled = false,
    title = "",
    centered = false,
    heading = "",
    class: klass = "btn-secondary",
  }: {
    items: { value: string; label: string; dot?: string; current?: boolean; hint?: string }[];
    onselect: (value: string) => void;
    trigger: Snippet;
    align?: "left" | "right";
    disabled?: boolean;
    title?: string;
    centered?: boolean;
    heading?: string;
    class?: string;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;
  let active = $state(0);
  let list: HTMLDivElement | undefined = $state();
  let returnFocus: HTMLElement | null = null;

  function toggle() {
    if (open) return close();
    returnFocus = document.activeElement as HTMLElement | null;
    open = true;
    active = Math.max(0, items.findIndex((i) => i.current));
    tick().then(() => list?.focus());
  }

  function close() {
    open = false;
    if (centered) returnFocus?.focus?.();
  }

  function pick(value: string) {
    close();
    onselect(value);
  }

  function keydown(e: KeyboardEvent) {
    if (!open) return;
    // The popover is modal: no key reaches the screen behind it.
    if (centered || ["Escape", "ArrowDown", "ArrowUp", "Enter"].includes(e.key) || /^[1-9]$/.test(e.key))
      e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      close();
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

  // Moves the popover to <body>: a `fixed` element inside a transformed or clipped
  // ancestor would otherwise be positioned and clipped by it.
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  export function show() {
    if (!disabled) toggle();
  }
</script>

<svelte:window
  onmousedown={(e) => {
    if (open && !centered && root && !root.contains(e.target as Node)) close();
  }}
/>

{#snippet options()}
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
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="relative inline-block" bind:this={root} onkeydown={keydown}>
  <button class={klass} {disabled} {title} onclick={toggle} aria-haspopup="listbox" aria-expanded={open}>
    {@render trigger()}
  </button>
  {#if open && !centered}
    <div
      class="card absolute top-full z-30 mt-1 min-w-48 overflow-hidden py-1 shadow-xl {align === 'right'
        ? 'right-0'
        : 'left-0'}"
      role="listbox"
      tabindex="-1"
      bind:this={list}
    >
      {@render options()}
    </div>
  {/if}
</div>

{#if open && centered}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="fixed inset-0 z-50 flex items-start justify-center bg-zinc-950/30 px-4 pt-[18vh] backdrop-blur-[1px] dark:bg-black/50"
    onmousedown={(e) => {
      if (e.target === e.currentTarget) close();
    }}
    onkeydown={keydown}
  >
    <div
      class="card w-full max-w-xs overflow-hidden py-1 shadow-2xl"
      role="listbox"
      tabindex="-1"
      aria-label={heading || title}
      bind:this={list}
    >
      {#if heading}
        <div class="border-b border-zinc-100 px-3 pt-1.5 pb-2 text-[11.5px] font-medium text-zinc-500 dark:border-zinc-800 dark:text-zinc-400">
          {heading}
        </div>
      {/if}
      {@render options()}
    </div>
  </div>
{/if}
