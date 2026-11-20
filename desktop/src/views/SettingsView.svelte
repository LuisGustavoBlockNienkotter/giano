<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, RotateCcw, Cloud, HardDrive, Upload, Unplug, Sparkles, Save } from "@lucide/svelte";
  import { api, errorMessage } from "../lib/api";
  import { store } from "../lib/state.svelte";
  import type { SettingsView } from "../lib/types";

  let settings = $state<SettingsView | null>(null);
  let exportDir = $state("");
  let prompt = $state("");
  let tursoUrl = $state("");
  let tursoToken = $state("");
  let canPush = $state(false);

  let promptDirty = $derived(!!settings && prompt !== settings.claude_prompt);
  let isDefaultPrompt = $derived(!!settings && prompt.trim() === settings.default_claude_prompt.trim());

  async function load() {
    try {
      settings = await api.getSettings();
      exportDir = settings.export_dir;
      prompt = settings.claude_prompt;
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  onMount(() => {
    load();
    return store.onRefresh(load);
  });

  async function chooseFolder() {
    const dir = await openDialog({ directory: true, defaultPath: exportDir, title: "Pasta de exportação" });
    if (typeof dir === "string") {
      exportDir = dir;
      await saveExportDir();
    }
  }

  async function saveExportDir(value: string | null = exportDir) {
    try {
      await api.setExportDir(value);
      await load();
      store.toast("Pasta de exportação salva.");
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function savePrompt(value: string | null = prompt) {
    try {
      await api.setClaudePrompt(value);
      await load();
      store.toast("Template do pedido de análise salvo.");
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function connect() {
    const r = await store.withBusy("Conectando ao Turso…", () => api.tursoConnect(tursoUrl, tursoToken));
    if (!r) return;
    store.status = r.status;
    canPush = r.can_push_local;
    tursoToken = "";
    store.toast(`Conectado ao Turso (${r.projects} projeto(s)).`);
    await store.loadProjects();
    await load();
  }

  async function disconnect() {
    const ok = await store.confirm(
      "Remover a URL e o token do Turso deste computador? O Giano volta a usar o banco local. Os dados no Turso continuam lá.",
      { title: "Desconectar do Turso", confirmLabel: "Desconectar" },
    );
    if (!ok) return;
    try {
      store.status = await api.tursoDisconnect();
      canPush = false;
      await store.loadStatus();
      await load();
      store.toast("Usando o banco local.");
    } catch (e) {
      store.toast(errorMessage(e), "error");
    }
  }

  async function push() {
    const ok = await store.confirm(
      `Copiar todos os projetos, itens, revisões e anotações do banco local (${store.status?.local_path}) para o Turso, que está vazio?`,
      { title: "Enviar o banco local", confirmLabel: "Enviar" },
    );
    if (!ok) return;
    const msg = await store.withBusy("Enviando para o Turso…", () => api.tursoPushLocal());
    if (msg) {
      canPush = false;
      store.toast(msg);
      await store.loadProjects();
    }
  }

  async function demo() {
    const ok = await store.confirm("Criar os projetos de exemplo LOJA e RH neste banco?", {
      title: "Dados de exemplo",
      confirmLabel: "Criar",
    });
    if (!ok) return;
    const codes = await store.withBusy("Gerando dados de exemplo…", () => api.createDemo());
    if (codes) {
      store.toast(`Projetos de exemplo criados: ${codes.join(", ")}.`);
      await store.loadProjects();
    }
  }
</script>

<div class="h-full overflow-y-auto">
  <div class="mx-auto max-w-3xl space-y-6 px-8 py-8">
    <h1 class="font-display text-xl font-semibold">Configurações</h1>

    <section class="card p-5">
      <h2 class="text-[14px] font-semibold">Pasta de exportação</h2>
      <p class="mt-0.5 text-xs text-zinc-500">
        Onde ficam os HTML e os arquivos para o Claude.
        {store.status?.remote ? "Com o Turso, vale só para este computador." : ""}
      </p>
      <div class="mt-3 flex gap-2">
        <input class="input" bind:value={exportDir} onkeydown={(e) => e.key === "Enter" && saveExportDir()} />
        <button class="btn-secondary" onclick={chooseFolder}><FolderOpen size={14} /> Escolher</button>
        <button class="btn-ghost" onclick={() => saveExportDir(null)} title="Voltar ao padrão (Documentos\Giano)"><RotateCcw size={14} /></button>
      </div>
    </section>

    <section class="card p-5">
      <div class="flex items-start gap-2">
        <div class="flex-1">
          <h2 class="text-[14px] font-semibold">Pedido de análise (exportação para o Claude)</h2>
          <p class="mt-0.5 text-xs text-zinc-500">
            Última seção do arquivo. <code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">{"{sigla}"}</code> e
            <code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">{"{nome}"}</code> viram a sigla e o nome do projeto. Compartilhado por todos os computadores.
          </p>
        </div>
        {#if isDefaultPrompt}<span class="chip bg-zinc-100 text-zinc-500 dark:bg-zinc-800">padrão</span>{/if}
      </div>
      <textarea class="input mt-3 min-h-[260px] font-mono text-[12.5px]" bind:value={prompt}></textarea>
      <div class="mt-3 flex justify-end gap-2">
        <button class="btn-ghost" onclick={() => savePrompt(null)} disabled={isDefaultPrompt}><RotateCcw size={14} /> Restaurar o padrão</button>
        <button class="btn-primary" onclick={() => savePrompt()} disabled={!promptDirty}><Save size={14} /> Salvar</button>
      </div>
    </section>

    <section class="card p-5">
      <h2 class="text-[14px] font-semibold">Banco de dados</h2>
      <div class="mt-3 flex items-center gap-3 rounded-lg bg-zinc-50 px-3.5 py-3 dark:bg-zinc-800/50">
        {#if store.status?.remote}
          <Cloud size={18} class="text-sky-500" />
          <div class="min-w-0 flex-1">
            <p class="text-[13px] font-medium">Turso (compartilhado entre computadores)</p>
            <p class="truncate text-xs text-zinc-500 select-text">{store.status.location} · configurado em {store.status.turso_source}</p>
          </div>
          <button class="btn-secondary" onclick={disconnect}><Unplug size={14} /> Desconectar</button>
        {:else}
          <HardDrive size={18} class="text-zinc-500" />
          <div class="min-w-0 flex-1">
            <p class="text-[13px] font-medium">Arquivo local</p>
            <p class="truncate text-xs text-zinc-500 select-text">{store.status?.location}</p>
          </div>
        {/if}
      </div>
      {#if store.status?.turso_configured && !store.status?.remote}
        <p class="mt-2 text-xs text-amber-700 dark:text-amber-400">
          O Turso está configurado neste computador, mas esta sessão usa o arquivo local.
          <button class="underline" onclick={async () => { store.status = await api.reconnect(); await store.loadProjects(); }}>Reconectar</button>
        </p>
      {/if}
      {#if canPush}
        <div class="mt-3 flex items-center gap-3 rounded-lg border border-sky-200 bg-sky-50 px-3.5 py-3 text-[13px] dark:border-sky-500/20 dark:bg-sky-500/10">
          <span class="flex-1">O banco no Turso está vazio. Levar as listas deste computador para lá?</span>
          <button class="btn-primary" onclick={push}><Upload size={14} /> Enviar banco local</button>
        </div>
      {/if}
      {#if !store.status?.remote}
        <div class="mt-4 space-y-2">
          <p class="text-xs text-zinc-500">
            Para usar as mesmas listas em vários computadores, conecte um banco Turso (crie com
            <code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">turso db create giano</code> e gere o token com
            <code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">turso db tokens create giano</code>).
          </p>
          <div class="grid grid-cols-[1fr_1fr_auto] gap-2">
            <input class="input" placeholder="libsql://giano-sua-org.turso.io" bind:value={tursoUrl} />
            <input class="input" type="password" placeholder="Token" bind:value={tursoToken} autocomplete="off" />
            <button class="btn-primary" onclick={connect} disabled={!tursoUrl.trim() || !tursoToken.trim()}><Cloud size={14} /> Conectar</button>
          </div>
          <p class="text-[11px] text-zinc-400">A URL e o token ficam em {store.status?.config_path} (o mesmo arquivo da TUI).</p>
        </div>
      {/if}
    </section>

    <section class="card flex items-center gap-4 p-5">
      <div class="flex-1">
        <h2 class="text-[14px] font-semibold">Dados de exemplo</h2>
        <p class="mt-0.5 text-xs text-zinc-500">Projetos LOJA e RH com histórico de revisões, adendo, item arquivado e anotações.</p>
      </div>
      <button class="btn-secondary" onclick={demo}><Sparkles size={14} /> Criar</button>
    </section>

    <p class="text-center text-[11px] text-zinc-400">Giano {store.status?.version} · núcleo em Rust, mesmo banco da TUI</p>
  </div>
</div>
