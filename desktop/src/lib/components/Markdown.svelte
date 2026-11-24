<script lang="ts">
  // Markdown rendered by the Rust side (same renderer as the HTML export: raw HTML escaped).
  //
  // Files of the project's repository become links that open them in the editor:
  // `[texto](repo:src/a.rs#L42)` and inline code with a path that exists in it (`src/a.rs:42`).
  // With `onitem`, item codes of the project (`MERC-12`, `MERC-12.1`) become buttons.
  import { api, errorMessage } from "../api";
  import { store } from "../state.svelte";

  let {
    source = "",
    empty = "Sem descrição.",
    class: klass = "",
    projectId = undefined,
    onitem = undefined,
  }: {
    source?: string;
    empty?: string;
    class?: string;
    /** Project whose repository the file links point to (default: the selected one). */
    projectId?: number | null;
    /** Item codes become buttons that call this with the code (`MERC-12.1`). */
    onitem?: (code: string) => void;
  } = $props();

  let html = $state("");
  let token = 0;

  let project = $derived(projectId === undefined ? store.projectId : projectId);
  let projectCode = $derived(store.projects.find((p) => p.id === project)?.code ?? null);

  const PATH_RE = /^(?![a-z][a-z0-9+.-]*:\/\/)([\w@.\-~/\\]*[\w@\-~][\w@.\-~/\\]*)(?::(\d+))?$/i;
  const escapeAttr = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
  const unescape = (s: string) =>
    s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#x27;/g, "'").replace(/&amp;/g, "&");

  /** `src/a.rs:42` → path and line; only things that look like a file (a slash or an extension). */
  function pathOf(text: string): { path: string; line: number | null } | null {
    const m = PATH_RE.exec(text.trim());
    if (!m || !/[/\\]|\.[a-z0-9]{1,8}$/i.test(m[1]) || /^\d+(\.\d+)*$/.test(m[1])) return null;
    return { path: m[1].replace(/\\/g, "/"), line: m[2] ? Number(m[2]) : null };
  }

  /** `repo:src/a.rs#L42` / `repo:src/a.rs:42` → path and line. */
  function repoHref(href: string): { path: string; line: number | null } | null {
    const m = /^repo:([^#]+?)(?::(\d+)|#L(\d+))?$/i.exec(unescape(href));
    return m ? { path: decodeURIComponent(m[1]), line: m[2] || m[3] ? Number(m[2] || m[3]) : null } : null;
  }

  async function enhance(raw: string): Promise<string> {
    // Inline code (not code blocks) that may be a file of the repository.
    const candidates = new Map<string, string>();
    raw.replace(/(<pre[\s\S]*?<\/pre>)|<code>([^<]*)<\/code>/g, (all, pre, code) => {
      if (!pre && code) {
        const p = pathOf(unescape(code));
        if (p) candidates.set(code, p.path);
      }
      return all;
    });
    let existing = new Set<string>();
    if (project != null && candidates.size) {
      try {
        existing = new Set(await api.repoFilesExist(project, [...new Set(candidates.values())]));
      } catch {
        /* not linked or offline: plain code */
      }
    }
    let out = raw.replace(/(<pre[\s\S]*?<\/pre>)|<code>([^<]*)<\/code>/g, (all, pre, code) => {
      if (pre) return all;
      const p = pathOf(unescape(code));
      if (!p || !existing.has(p.path)) return all;
      return `<a href="#" class="repo-link" data-path="${escapeAttr(p.path)}" data-line="${p.line ?? ""}" title="Abrir no editor">${all}</a>`;
    });
    out = out.replace(/<a href="(repo:[^"]*)"/g, (all, href) => {
      const p = repoHref(href);
      return p
        ? `<a href="#" class="repo-link" data-path="${escapeAttr(p.path)}" data-line="${p.line ?? ""}" title="Abrir no editor: ${escapeAttr(p.path)}"`
        : all;
    });
    if (onitem && projectCode) {
      const code = projectCode.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const re = new RegExp(`\\b(${code}-\\d+(?:\\.\\d+)?)\\b`, "gi");
      let depth = 0; // inside <a> or <pre>: no buttons
      out = out
        .split(/(<[^>]+>)/)
        .map((part) => {
          if (part.startsWith("<")) {
            if (/^<(a|pre)[\s>]/i.test(part)) depth++;
            else if (/^<\/(a|pre)>/i.test(part)) depth--;
            return part;
          }
          if (depth > 0) return part;
          return part.replace(re, (c) => `<button type="button" class="item-link" data-code="${c.toUpperCase()}">${c.toUpperCase()}</button>`);
        })
        .join("");
    }
    return out;
  }

  $effect(() => {
    const text = source;
    void project;
    void projectCode;
    void onitem;
    const mine = ++token;
    if (!text.trim()) {
      html = "";
      return;
    }
    api
      .renderMarkdown(text)
      .then(enhance)
      .then((h) => {
        if (mine === token) html = h;
      });
  });

  function click(e: MouseEvent) {
    const target = e.target as HTMLElement;
    const item = target.closest<HTMLElement>("button.item-link");
    if (item && onitem) {
      e.preventDefault();
      onitem(item.dataset.code ?? "");
      return;
    }
    const link = target.closest<HTMLAnchorElement>("a.repo-link");
    if (link && project != null) {
      e.preventDefault();
      const line = link.dataset.line ? Number(link.dataset.line) : null;
      api.openRepoFile(project, link.dataset.path ?? "", line).catch((err) => store.toast(errorMessage(err), "error"));
    }
  }
</script>

{#if source.trim()}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="md prose prose-sm prose-zinc dark:prose-invert max-w-none {klass}" onclick={click}>
    {@html html}
  </div>
{:else}
  <p class="text-sm text-zinc-400 italic dark:text-zinc-500">{empty}</p>
{/if}
