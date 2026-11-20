<script lang="ts">
  // Tags as chips, with autocomplete from the tags already used.
  import { X } from "@lucide/svelte";

  let {
    tags = $bindable([]),
    suggestions = [],
    disabled = false,
  }: { tags: string[]; suggestions?: string[]; disabled?: boolean } = $props();

  let text = $state("");
  let focused = $state(false);
  let active = $state(0);
  let input: HTMLInputElement;

  const norm = (t: string) => t.trim().toLowerCase().replace(/\s+/g, "-");

  let matches = $derived.by(() => {
    const q = norm(text);
    if (!q) return [];
    return suggestions.filter((s) => s.includes(q) && !tags.includes(s)).slice(0, 8);
  });

  function add(raw: string) {
    const parts = raw.split(",").map(norm).filter(Boolean);
    const next = [...tags];
    for (const p of parts) if (!next.includes(p)) next.push(p);
    tags = next;
    text = "";
    active = 0;
  }

  function remove(t: string) {
    tags = tags.filter((x) => x !== t);
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === "," || e.key === "Tab") {
      if (matches.length && (e.key === "Enter" || e.key === "Tab") && text.trim()) {
        e.preventDefault();
        add(matches[active] ?? text);
      } else if (text.trim()) {
        e.preventDefault();
        add(text);
      }
    } else if (e.key === "Backspace" && !text && tags.length) {
      tags = tags.slice(0, -1);
    } else if (e.key === "ArrowDown" && matches.length) {
      e.preventDefault();
      active = (active + 1) % matches.length;
    } else if (e.key === "ArrowUp" && matches.length) {
      e.preventDefault();
      active = (active - 1 + matches.length) % matches.length;
    }
  }
</script>

<div class="relative">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="input flex h-auto min-h-8 flex-wrap items-center gap-1 py-1 {focused
      ? 'border-accent-500 ring-3 ring-accent-500/15'
      : ''}"
    onclick={() => input?.focus()}
  >
    {#each tags as t (t)}
      <span class="chip bg-accent-50 text-accent-700 dark:bg-accent-500/15 dark:text-accent-300">
        #{t}
        {#if !disabled}
          <button class="-mr-1 rounded-full p-0.5 hover:bg-accent-100 dark:hover:bg-accent-500/20" onclick={() => remove(t)} tabindex="-1">
            <X size={11} />
          </button>
        {/if}
      </span>
    {/each}
    <input
      bind:this={input}
      bind:value={text}
      {disabled}
      class="min-w-24 flex-1 bg-transparent text-[13px] outline-none"
      placeholder={tags.length ? "" : "ux, api… (Enter ou vírgula)"}
      onkeydown={keydown}
      onfocus={() => (focused = true)}
      onblur={() => {
        focused = false;
        if (text.trim()) add(text);
      }}
    />
  </div>
  {#if focused && matches.length}
    <div class="card absolute top-full right-0 left-0 z-10 mt-1 overflow-hidden py-1 shadow-lg">
      {#each matches as m, i (m)}
        <button
          class="block w-full px-3 py-1.5 text-left text-[13px] {i === active
            ? 'bg-accent-50 text-accent-700 dark:bg-accent-500/15 dark:text-accent-300'
            : 'hover:bg-zinc-50 dark:hover:bg-zinc-800'}"
          onmousedown={(e) => {
            e.preventDefault();
            add(m);
          }}>#{m}</button
        >
      {/each}
    </div>
  {/if}
</div>
