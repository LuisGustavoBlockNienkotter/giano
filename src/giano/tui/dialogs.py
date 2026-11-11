"""Modal dialogs: confirmation, filters, quick status, Claude export, settings."""

from __future__ import annotations

from dataclasses import dataclass

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, Footer, Input, Label, OptionList, Select, Static, TextArea
from textual.widgets.option_list import Option

from giano.domain import ItemStatus, ItemType, Priority, ValidationError
from giano.services.notes import parse_date
from giano.tui.widgets import priority_select, select_value, status_select, type_select


class ConfirmDialog(ModalScreen[bool]):
    BINDINGS = [
        Binding("escape", "answer(False)", "Cancelar"),
        Binding("y,s", "answer(True)", "Sim", show=False),
    ]

    def __init__(self, message: str, confirm_label: str = "Excluir"):
        super().__init__()
        self.message = message
        self.confirm_label = confirm_label

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Static(self.message, classes="dialog-message", markup=False)
            with Horizontal(classes="buttons"):
                yield Button(self.confirm_label, variant="error", id="yes")
                yield Button("Cancelar", id="no")

    def on_mount(self) -> None:
        self.query_one("#no", Button).focus()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        self.dismiss(event.button.id == "yes")

    def action_answer(self, value: bool) -> None:
        self.dismiss(value)


@dataclass(frozen=True)
class Filters:
    type: ItemType | None = None
    status: ItemStatus | None = None
    priority: Priority | None = None
    tag: str | None = None
    hide_closed: bool = False

    @property
    def active(self) -> bool:
        return any((self.type, self.status, self.priority, self.tag, self.hide_closed))

    def describe(self) -> str:
        parts = []
        if self.type:
            parts.append(f"tipo={self.type.label}")
        if self.status:
            parts.append(f"status={self.status.label}")
        if self.priority:
            parts.append(f"prioridade={self.priority.label}")
        if self.tag:
            parts.append(f"tag={self.tag}")
        if self.hide_closed:
            parts.append("sem concluídos/descartados")
        return ", ".join(parts)

    def matches(self, view) -> bool:
        d = view.data
        if self.type and d.type != self.type:
            return False
        if self.status and d.status != self.status:
            return False
        if self.priority and d.priority != self.priority:
            return False
        if self.tag and self.tag not in d.tags:
            return False
        if self.hide_closed and d.status in (ItemStatus.DONE, ItemStatus.DISCARDED):
            return False
        return True


class FilterDialog(ModalScreen[Filters | None]):
    BINDINGS = [
        Binding("escape", "cancel", "Cancelar"),
        Binding("ctrl+s", "apply", "Aplicar", priority=True),
    ]

    def __init__(self, current: Filters, tags: list[str]):
        super().__init__()
        self.current = current
        self.tags = tags

    def compose(self) -> ComposeResult:
        c = self.current
        with Vertical(classes="dialog"):
            yield Label("Filtros", classes="dialog-title")
            yield type_select(c.type, blank=True, id="f-type")
            yield status_select(c.status, blank=True, id="f-status")
            yield priority_select(c.priority, blank=True, id="f-priority")
            yield Select(
                [(f"#{t}", t) for t in self.tags],
                prompt="Tag",
                allow_blank=True,
                value=c.tag if c.tag in self.tags else Select.NULL,
                id="f-tag",
            )
            yield Checkbox("Ocultar concluídos e descartados", c.hide_closed, id="f-closed")
            with Horizontal(classes="buttons"):
                yield Button("Aplicar", variant="primary", id="apply")
                yield Button("Limpar", id="clear")
                yield Button("Cancelar", id="cancel")
        yield Footer(compact=True)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "apply":
            self.action_apply()
        elif event.button.id == "clear":
            self.dismiss(Filters())
        else:
            self.dismiss(None)

    def action_apply(self) -> None:
        self.dismiss(
            Filters(
                type=select_value(self.query_one("#f-type", Select)),
                status=select_value(self.query_one("#f-status", Select)),
                priority=select_value(self.query_one("#f-priority", Select)),
                tag=select_value(self.query_one("#f-tag", Select)),
                hide_closed=self.query_one("#f-closed", Checkbox).value,
            )
        )

    def action_cancel(self) -> None:
        self.dismiss(None)


class ChoiceDialog(ModalScreen[object]):
    """Pick one value from a short list (e.g. quick status change)."""

    BINDINGS = [Binding("escape", "cancel", "Cancelar")]

    def __init__(self, title: str, choices: list[tuple[str, object]], current: object = None):
        super().__init__()
        self.title_text = title
        self.choices = choices
        self.current = current

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label(self.title_text, classes="dialog-title", markup=False)
            yield OptionList(
                *[Option(("● " if v == self.current else "  ") + label, id=str(i))
                  for i, (label, v) in enumerate(self.choices)],
                id="choices",
            )

    def on_mount(self) -> None:
        ol = self.query_one(OptionList)
        for i, (_, v) in enumerate(self.choices):
            if v == self.current:
                ol.highlighted = i
        ol.focus()

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        self.dismiss(self.choices[int(event.option.id)][1])

    def action_cancel(self) -> None:
        self.dismiss(None)


@dataclass(frozen=True)
class ClaudeExportOptions:
    since: str | None
    all_notes: bool
    include_unassigned: bool


class ClaudeExportDialog(ModalScreen[ClaudeExportOptions | None]):
    BINDINGS = [
        Binding("escape", "cancel", "Cancelar"),
        Binding("ctrl+s", "export", "Exportar", priority=True),
    ]

    def __init__(self, project_label: str):
        super().__init__()
        self.project_label = project_label

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label(f"Exportar para o Claude — {self.project_label}", classes="dialog-title", markup=False)
            yield Static(
                "Gera um arquivo .md com o contexto do projeto, a lista atual e as anotações, "
                "e copia o conteúdo para a área de transferência.",
                classes="dialog-message",
            )
            yield Input(placeholder="Desde (AAAA-MM-DD), opcional", id="since")
            yield Checkbox("Todas as anotações (não só as pendentes)", id="all")
            yield Checkbox("Incluir anotações sem projeto", id="unassigned")
            with Horizontal(classes="buttons"):
                yield Button("Exportar", variant="primary", id="export")
                yield Button("Cancelar", id="cancel")
        yield Footer(compact=True)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "export":
            self.action_export()
        else:
            self.dismiss(None)

    def on_input_submitted(self, _: Input.Submitted) -> None:
        self.action_export()

    def action_export(self) -> None:
        since = self.query_one("#since", Input).value.strip() or None
        if since:
            try:
                since = parse_date(since)
            except ValidationError as exc:
                self.notify(str(exc), severity="error")
                return
        self.dismiss(
            ClaudeExportOptions(
                since,
                self.query_one("#all", Checkbox).value,
                self.query_one("#unassigned", Checkbox).value,
            )
        )

    def action_cancel(self) -> None:
        self.dismiss(None)


class SettingsScreen(ModalScreen[None]):
    BINDINGS = [
        Binding("escape", "cancel", "Cancelar"),
        Binding("ctrl+s", "save", "Salvar", priority=True),
    ]

    def compose(self) -> ComposeResult:
        s = self.app.giano.settings
        with Vertical(classes="dialog wide"):
            yield Label("Configurações", classes="dialog-title")
            yield Label("Pasta de exportação (vazio = padrão)")
            yield Input(str(s.export_dir), id="export-dir")
            yield Label("Template do pedido de análise (exportação para o Claude). "
                        "Use {sigla} e {nome} para o projeto.")
            yield TextArea(s.claude_prompt, id="prompt", soft_wrap=True)
            with Horizontal(classes="buttons"):
                yield Button("Salvar", variant="primary", id="save")
                yield Button("Restaurar template padrão", id="reset")
                yield Button("Cancelar", id="cancel")
            yield Static(f"Banco de dados: {self.app.giano.db_path}", classes="muted", markup=False)
        yield Footer(compact=True)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "save":
            self.action_save()
        elif event.button.id == "reset":
            from giano.config import DEFAULT_CLAUDE_PROMPT

            self.query_one("#prompt", TextArea).load_text(DEFAULT_CLAUDE_PROMPT)
        else:
            self.dismiss(None)

    def action_save(self) -> None:
        from giano.config import default_export_dir

        s = self.app.giano.settings
        folder = self.query_one("#export-dir", Input).value.strip()
        s.export_dir = "" if folder == str(default_export_dir()) else folder
        s.claude_prompt = self.query_one("#prompt", TextArea).text
        self.notify("Configurações salvas.")
        self.dismiss(None)

    def action_cancel(self) -> None:
        self.dismiss(None)
