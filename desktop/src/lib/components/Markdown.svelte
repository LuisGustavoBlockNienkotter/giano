<script lang="ts">
  // Markdown rendered by the Rust side (same renderer as the HTML export: raw HTML escaped).
  import { api } from "../api";

  let { source = "", empty = "Sem descrição.", class: klass = "" }: { source?: string; empty?: string; class?: string } =
    $props();

  let html = $state("");
  let token = 0;

  $effect(() => {
    const text = source;
    const mine = ++token;
    if (!text.trim()) {
      html = "";
      return;
    }
    api.renderMarkdown(text).then((h) => {
      if (mine === token) html = h;
    });
  });
</script>

{#if source.trim()}
  <div class="md prose prose-sm prose-zinc dark:prose-invert max-w-none {klass}">
    {@html html}
  </div>
{:else}
  <p class="text-sm text-zinc-400 italic dark:text-zinc-500">{empty}</p>
{/if}
