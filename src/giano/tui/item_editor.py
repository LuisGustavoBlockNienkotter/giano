"""Item editor: structured fields + Markdown body with edit/preview toggle."""

from __future__ import annotations

from dataclasses import dataclass

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import Screen
from textual.widgets import ContentSwitcher, Footer, Input, Label, Markdown, Select, Static, TextArea

from giano.domain import ItemData, ItemType, ItemView, Project, ValidationError, compare_items
from giano.services.formatting import item_preview
from giano.tui.dialogs import ConfirmDialog
from giano.tui.widgets import (
    TagSuggester,
    difficulty_select,
    priority_select,
    select_value,
    status_select,
    type_select,
)


@dataclass(frozen=True)
class ItemDraft:
    """Prefill for a new item (e.g. from a note)."""

    title: str = ""
    body: str = ""
    source_note_id: int | None = None
    source_label: str | None = None


class ItemEditorScreen(Screen[ItemView | None]):
    """Returns the saved item view, or None if cancelled / nothing changed."""

    BINDINGS = [
        Binding("ctrl+s", "save", "Salvar", priority=True),
        Binding("ctrl+t", "toggle_preview", "Editar/visualizar", priority=True),
        Binding("ctrl+o", "show_original", "Ver original", priority=True),
        Binding("escape", "cancel", "Cancelar"),
    ]

    def __init__(
        self,
        project: Project,
        item: ItemView | None = None,
        draft: ItemDraft | None = None,
        addendum_of: ItemView | None = None,
    ):
        """``addendum_of``: create an addendum of this item instead of a new item."""
        super().__init__()
        self.project = project
        self.item = item
        self.draft = draft or ItemDraft()
        self.addendum_of = addendum_of

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        if action == "show_original":
            return self.addendum_of is not None or (self.item is not None and self.item.is_addendum)
        return True

    def _original(self) -> ItemView | None:
        """The original item when creating or editing an addendum (latest revision)."""
        items = self.app.giano.items
        if self.addendum_of is not None:
            base = items.get(self.addendum_of.item_id) or self.addendum_of
            return items.parent(base) if base.is_addendum else base
        if self.item is not None and self.item.is_addendum:
            return items.parent(self.item)
        return None

    def compose(self) -> ComposeResult:
        if self.item:
            d = self.item.data
        elif self.addendum_of:
            # An addendum is usually an adjustment of the original: start from its
            # priority, difficulty and tags; the text is new.
            o = self.addendum_of.data
            d = ItemData(title=self.draft.title, body=self.draft.body, type=ItemType.ADJUSTMENT,
                         priority=o.priority, difficulty=o.difficulty, tags=o.tags)
        else:
            d = ItemData(title=self.draft.title, body=self.draft.body)
        if self.item:
            heading = f"Editar {self.item.code} — {self.project.name}"
            if self.item.is_addendum:
                heading += f"  (adendo de {self.item.parent_code}; ctrl+o mostra o original)"
        elif self.addendum_of:
            root = self.addendum_of.parent_code or self.addendum_of.code
            heading = (f"Novo adendo de {root} — {self.addendum_of.data.title}"
                       "  (ctrl+o mostra o original)")
        else:
            heading = f"Novo item — {self.project.code} · {self.project.name}"
            if self.draft.source_label:
                heading += f"  (a partir da anotação: {self.draft.source_label})"
        yield Static(heading, id="editor-title", classes="topbar", markup=False)
        with Vertical(id="editor-form"):
            yield Label("Título")
            yield Input(d.title, id="title", placeholder="Título do item")
            with Horizontal(id="editor-selects"):
                yield type_select(d.type, id="type")
                yield priority_select(d.priority, id="priority")
                yield difficulty_select(d.difficulty, id="difficulty")
                yield status_select(d.status, id="status")
            yield Label("Tags (separadas por vírgula; → completa)")
            yield Input(
                ", ".join(d.tags),
                id="tags",
                placeholder="ex.: ux, integração",
                suggester=TagSuggester(self.app.giano.items.all_tags),
            )
            yield Label("Descrição (Markdown) — ctrl+t alterna editar/visualizar", id="body-label")
            with ContentSwitcher(initial="body", id="body-switcher"):
                yield TextArea.code_editor(
                    d.body, language="markdown", soft_wrap=True, show_line_numbers=False,
                    tab_behavior="focus", id="body", theme="monokai",
                )
                with VerticalScroll(id="body-preview"):
                    yield Markdown(id="body-markdown")
        yield Footer(compact=True, show_command_palette=False)

    def on_mount(self) -> None:
        self.query_one("#title", Input).focus()
        self._initial = self._read(validate=False)

    def _read(self, validate: bool = True) -> ItemData:
        data = ItemData(
            title=self.query_one("#title", Input).value,
            type=select_value(self.query_one("#type", Select)),
            priority=select_value(self.query_one("#priority", Select)),
            difficulty=select_value(self.query_one("#difficulty", Select)),
            status=select_value(self.query_one("#status", Select)),
            tags=self.query_one("#tags", Input).value,
            body=self.query_one("#body", TextArea).text,
        )
        return data.normalized() if validate else data

    def _dirty(self) -> bool:
        try:
            current = self._read()
            initial = self._initial.normalized()
        except (ValidationError, ValueError):
            return True
        return bool(compare_items(initial, current))

    async def action_toggle_preview(self) -> None:
        switcher = self.query_one(ContentSwitcher)
        if switcher.current == "body":
            await self._render_preview()
            switcher.current = "body-preview"
            self.query_one("#body-preview").focus()
        else:
            switcher.current = "body"
            self.query_one("#body", TextArea).focus()

    async def action_show_original(self) -> None:
        original = self._original()
        if original is None:
            self.notify("Este item não é um adendo.", severity="warning")
            return
        note = self.app.giano.notes.get(original.source_note_id) if original.source_note_id else None
        await self.query_one("#body-markdown", Markdown).update(
            "> **Item original** (somente leitura) — ctrl+t volta ao texto do adendo\n\n"
            + item_preview(original, note)
        )
        self.query_one(ContentSwitcher).current = "body-preview"
        self.query_one("#body-preview").focus()

    async def _render_preview(self) -> None:
        await self.query_one("#body-markdown", Markdown).update(
            self.query_one("#body", TextArea).text or "_Sem descrição._"
        )

    # -- changes made outside this editor (another Giano, the CLI, a script) --

    def _load(self, view: ItemView) -> None:
        """Show ``view`` in the form (used when nothing was edited here)."""
        d = view.data
        self.item = view
        self.query_one("#title", Input).value = d.title
        self.query_one("#type", Select).value = d.type
        self.query_one("#priority", Select).value = d.priority
        self.query_one("#difficulty", Select).value = d.difficulty
        self.query_one("#status", Select).value = d.status
        self.query_one("#tags", Input).value = ", ".join(d.tags)
        self.query_one("#body", TextArea).load_text(d.body)
        self._initial = self._read(validate=False)
        if self.query_one(ContentSwitcher).current == "body-preview":
            # In view mode the rendered Markdown must follow the new text too.
            self.call_later(self._render_preview)

    def refresh_data(self) -> str | None:
        """F5: if the item changed elsewhere, reload it when the form is untouched,
        or warn when there are unsaved edits here (they are never discarded)."""
        if self.item is None:
            return None
        current = self.app.giano.items.get(self.item.item_id)
        if current is None:
            return f"{self.item.code} foi excluído fora daqui: não será possível salvá-lo."
        if current.version_id == self.item.version_id:
            return None
        if not self._dirty():
            self._load(current)
            return None
        return (f"{self.item.code} foi alterado fora daqui (Rev. {current.rev_start}). "
                "Suas alterações não salvas foram mantidas; ao salvar, você confirma a substituição.")

    def action_save(self, overwrite: bool = False) -> None:
        items = self.app.giano.items
        if self.item is not None and not overwrite:
            current = items.get(self.item.item_id)
            if current is not None and current.version_id != self.item.version_id:

                def answer(ok: bool | None) -> None:
                    if ok:
                        self.action_save(overwrite=True)

                self.app.push_screen(
                    ConfirmDialog(
                        f"{self.item.code} foi alterado fora deste editor depois que você o abriu "
                        f"(agora na Rev. {current.rev_start}).\n\n"
                        "Salvar mesmo assim? O conteúdo deste editor substitui o de fora "
                        "(numa nova revisão; a versão de fora continua no histórico).",
                        "Salvar mesmo assim",
                    ),
                    answer,
                )
                return
        try:
            data = self._read()
            if self.item is None and self.addendum_of is not None:
                view = items.create_addendum(
                    self.addendum_of.item_id, data, source_note_id=self.draft.source_note_id
                )
                self.notify(f"{view.code} criado (adendo de {view.parent_code}).")
            elif self.item is None:
                view = items.create(self.project.id, data, source_note_id=self.draft.source_note_id)
                self.notify(f"{view.code} criado.")
            else:
                view = items.update(self.item.item_id, data)
                if view is None:
                    self.notify("Nenhuma alteração: nenhuma revisão criada.")
                else:
                    self.notify(f"{view.code} salvo (Rev. {view.rev_start}).")
        except (ValidationError, ValueError) as exc:
            self.notify(str(exc), severity="error", title="Não foi possível salvar")
            return
        self.dismiss(view)

    def action_cancel(self) -> None:
        if not self._dirty():
            self.dismiss(None)
            return

        def answer(discard: bool | None) -> None:
            if discard:
                self.dismiss(None)

        self.app.push_screen(
            ConfirmDialog("Descartar as alterações não salvas?", "Descartar"), answer
        )
