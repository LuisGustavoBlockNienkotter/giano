<script lang="ts" module>
  import type { PriorityValue, StatusValue } from "../types";

  export const PRIORITY_CLASS: Record<PriorityValue, string> = {
    critical: "bg-red-600 text-white dark:bg-red-500/90",
    high: "bg-orange-100 text-orange-800 dark:bg-orange-500/15 dark:text-orange-300",
    medium: "bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300",
    low: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  };

  export const PRIORITY_DOT: Record<PriorityValue, string> = {
    critical: "bg-red-500",
    high: "bg-orange-500",
    medium: "bg-amber-400",
    low: "bg-zinc-400",
  };

  export const STATUS_CLASS: Record<StatusValue, string> = {
    open: "bg-zinc-100 text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300",
    analysis: "bg-sky-100 text-sky-800 dark:bg-sky-500/15 dark:text-sky-300",
    approved: "bg-blue-100 text-blue-800 dark:bg-blue-500/15 dark:text-blue-300",
    in_progress: "bg-violet-100 text-violet-800 dark:bg-violet-500/20 dark:text-violet-300",
    done: "bg-emerald-100 text-emerald-800 dark:bg-emerald-500/15 dark:text-emerald-300",
    discarded: "bg-zinc-100 text-zinc-400 line-through dark:bg-zinc-800/60 dark:text-zinc-500",
  };

  export const STATUS_DOT: Record<StatusValue, string> = {
    open: "bg-zinc-400",
    analysis: "bg-sky-500",
    approved: "bg-blue-500",
    in_progress: "bg-violet-500",
    done: "bg-emerald-500",
    discarded: "bg-zinc-300 dark:bg-zinc-600",
  };
</script>

<script lang="ts">
  import { store } from "../state.svelte";

  let {
    priority = undefined,
    status = undefined,
    prefix = false,
  }: { priority?: PriorityValue; status?: StatusValue; prefix?: boolean } = $props();
</script>

{#if priority}
  <span class="chip {PRIORITY_CLASS[priority]}">{prefix ? "Prioridade " : ""}{store.priorityLabel(priority)}</span>
{/if}
{#if status}
  <span class="chip {STATUS_CLASS[status]}">
    <span class="h-1.5 w-1.5 rounded-full {STATUS_DOT[status]}"></span>
    {store.statusLabel(status)}
  </span>
{/if}
