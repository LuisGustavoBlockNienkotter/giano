// Item actions shared by the keyboard shortcuts, the table and the detail panel.

import { api, errorMessage } from "./api";
import { store } from "./state.svelte";
import type { ItemView, StatusValue } from "./types";

function readOnlyReason(item: ItemView | null): string | null {
  if (!store.pv) return "Nenhum projeto aberto.";
  if (store.pv.rev !== store.pv.latest_rev)
    return `Rev. ${store.pv.rev} de ${store.pv.latest_rev}: somente leitura. Volte à última revisão para editar.`;
  if (!store.pv.project.active) return "Projeto arquivado: somente leitura.";
  if (item?.data.archived) return `${item.code} está arquivado: restaure-o para a lista antes de editar.`;
  return null;
}

function refuse(item: ItemView | null): boolean {
  const reason = readOnlyReason(item);
  if (reason) store.toast(reason, "info");
  return !!reason;
}

export function newItem() {
  if (refuse(null) || !store.pv) return;
  store.editor = { mode: "new", projectId: store.pv.project.id };
}

export function editItem(item: ItemView) {
  if (readOnlyReason(item)) {
    store.editor = { mode: "view", item };
    return;
  }
  store.editor = { mode: "edit", item };
}

export function newAddendum(item: ItemView) {
  if (!store.pv) return;
  if (store.pv.rev !== store.pv.latest_rev || !store.pv.project.active) {
    refuse(null);
    return;
  }
  const original = item.is_addendum
    ? (store.pv.items.find((v) => v.number === item.number && v.addendum === 0) ?? null)
    : item;
  store.editor = { mode: "addendum", base: item, original };
}

/** After a change: reload the list and keep `selectId` selected. */
async function reload(selectId: number | null) {
  await store.loadProject();
  if (selectId != null) store.selectedItemId = selectId;
}

export async function deleteItem(item: ItemView) {
  if (refuse(null)) return;
  const ok = await store.confirm(
    `Excluir ${item.code} — ${item.data.title}?\n\nA exclusão gera uma nova revisão; o item continua visível nas revisões anteriores.`,
    { title: "Excluir item", confirmLabel: "Excluir", danger: true },
  );
  if (!ok) return;
  try {
    const rev = await api.deleteItem(item.item_id);
    store.toast(`${item.code} excluído (Rev. ${rev}).`);
    await reload(null);
  } catch (e) {
    store.toast(errorMessage(e), "error");
  }
}

export async function toggleArchive(item: ItemView) {
  if (refuse(null)) return;
  try {
    if (item.data.archived) {
      const v = await api.unarchiveItem(item.item_id);
      store.toast(`${v.code} restaurado à lista (Rev. ${v.rev_start}).`);
    } else {
      const v = await api.archiveItem(item.item_id);
      store.toast(`${v.code} arquivado (Rev. ${v.rev_start}).`);
    }
    await reload(null);
  } catch (e) {
    store.toast(errorMessage(e), "error");
  }
}

/** One revision for all the items that change (or into the open revision); all or nothing. */
export async function setStatus(items: ItemView[], status: StatusValue): Promise<boolean> {
  if (refuse(items.find((i) => i.data.archived) ?? null)) return false;
  const todo = items.filter((i) => i.data.status !== status);
  if (!todo.length) {
    store.toast(`Já ${items.length === 1 ? "está" : "estão"} como ${store.statusLabel(status)}.`, "info");
    return false;
  }
  const label = store.statusLabel(status);
  const run = () => api.setStatusMany(todo.map((i) => i.item_id), status);
  const changed = todo.length > 1 ? await store.withBusy(`Mudando o status para ${label}…`, run) : await run().catch((e) => {
    store.toast(errorMessage(e), "error");
    return null;
  });
  if (!changed) return false;
  store.toast(
    changed.length === 1
      ? `${changed[0].code}: status → ${label} (Rev. ${changed[0].rev_start}).`
      : `${changed.length} itens → ${label} (Rev. ${changed[0].rev_start}).`,
  );
  await reload(items.length === 1 ? items[0].item_id : store.selectedItemId);
  return true;
}

/** Set (`YYYY-MM-DD`) or clear (null) the control date. Not versioned: no revision, so the
 * views are patched in place instead of reloading the list. */
export async function setControlDate(item: ItemView, day: string | null): Promise<boolean> {
  if (!store.pv?.project.active) {
    store.toast("Projeto arquivado: somente leitura.", "info");
    return false;
  }
  try {
    const views = await api.setControlDate([item.item_id], day);
    for (const v of views) {
      const local = store.pv?.items.find((x) => x.item_id === v.item_id);
      if (local) local.control_date = v.control_date;
    }
    return true;
  } catch (e) {
    store.toast(errorMessage(e), "error");
    return false;
  }
}

export async function copyItemMarkdown(item: ItemView) {
  try {
    const md = await api.itemMarkdown(item.item_id, store.pv?.rev === store.pv?.latest_rev ? null : (store.pv?.rev ?? null));
    await navigator.clipboard.writeText(md);
    store.toast(`${item.code} copiado como Markdown.`);
  } catch (e) {
    store.toast(errorMessage(e), "error");
  }
}

export { readOnlyReason };
