"""Command line interface (Typer). Without a subcommand, opens the TUI."""

from __future__ import annotations

import locale
import sys
from dataclasses import replace
from pathlib import Path
from typing import Annotated, Optional

import typer
from rich.console import Console
from rich.markdown import Markdown
from rich.markup import escape
from rich.table import Table
from rich.text import Text

from giano.domain import (
    ItemData,
    ItemStatus,
    ItemType,
    ItemView,
    Priority,
    ValidationError,
    parse_choice,
    parse_ranges,
)
from giano.services import Giano
from giano.services import clipboard
from giano.services.formatting import diff_markdown, item_preview
from giano.services.notes import parse_date

app = typer.Typer(
    help="Giano — gerenciador de melhorias de sistemas. Sem subcomando, abre a interface (TUI).",
    no_args_is_help=False,
    add_completion=False,
    rich_markup_mode="rich",
)
export_app = typer.Typer(help="Exporta a lista de um projeto (Claude ou HTML).", no_args_is_help=True)
app.add_typer(export_app, name="export")
item_app = typer.Typer(
    help="Cria, mostra, edita e arquiva itens (sem abrir a interface).", no_args_is_help=True
)
app.add_typer(item_app, name="item")

console = Console()
err = Console(stderr=True)


def _giano(ctx: typer.Context) -> Giano:
    obj = ctx.find_root().obj
    if obj.get("giano") is None:
        obj["giano"] = Giano.open(obj.get("db"))
    return obj["giano"]


def _fail(message: str) -> None:
    err.print(f"[bold red]Erro:[/] {escape(message)}")
    raise typer.Exit(1)


@app.callback(invoke_without_command=True)
def root(
    ctx: typer.Context,
    db: Annotated[
        Optional[Path],
        typer.Option("--db", help="Arquivo do banco (padrão: variável GIANO_DB ou pasta de dados do usuário)."),
    ] = None,
) -> None:
    ctx.obj = {"db": db, "giano": None}
    if ctx.invoked_subcommand is None:
        from giano.tui.app import GianoApp

        GianoApp(_giano(ctx)).run()


@app.command()
def nota(
    ctx: typer.Context,
    nome: Annotated[str, typer.Argument(help="Nome da anotação (ex.: nome da reunião).")],
    projeto: Annotated[Optional[str], typer.Option("--projeto", "-p", help="Sigla do projeto.")] = None,
    data: Annotated[Optional[str], typer.Option("--data", "-d", help="Data AAAA-MM-DD (padrão: hoje).")] = None,
    texto: Annotated[
        Optional[str], typer.Option("--texto", "-t", help="Corpo da anotação; não abre o editor.")
    ] = None,
    sem_editor: Annotated[
        bool, typer.Option("--sem-editor", help="Apenas cria a anotação, sem abrir o editor.")
    ] = False,
) -> None:
    """Cria uma anotação rápida e abre o editor (com salvamento automático)."""
    g = _giano(ctx)
    try:
        project_id = g.projects.require(projeto).id if projeto else None
        body = texto or ""
        if texto is None and not sys.stdin.isatty():
            body = sys.stdin.read()
            sem_editor = True
        note = g.notes.create(nome, project_id, body, parse_date(data) if data else None)
    except ValidationError as exc:
        _fail(str(exc))
        return
    if texto is not None or sem_editor:
        console.print(f"Anotação [bold]{escape(note.name)}[/] criada ({note.note_date}).")
        return
    from giano.tui.app import GianoApp

    GianoApp(g, quick_note_id=note.id).run()
    console.print(f"Anotação [bold]{escape(note.name)}[/] salva.")


@export_app.command("claude")
def export_claude(
    ctx: typer.Context,
    projeto: Annotated[str, typer.Option("--projeto", "-p", help="Sigla do projeto.")],
    desde: Annotated[
        Optional[str], typer.Option("--desde", help="Somente anotações a partir desta data (AAAA-MM-DD).")
    ] = None,
    todas: Annotated[bool, typer.Option("--todas", help="Inclui todas as anotações, não só as pendentes.")] = False,
    sem_projeto: Annotated[
        bool, typer.Option("--sem-projeto", help="Inclui também anotações sem projeto.")
    ] = False,
    saida: Annotated[Optional[Path], typer.Option("--saida", "-o", help="Arquivo ou pasta de saída.")] = None,
    copiar: Annotated[
        bool, typer.Option("--copiar/--sem-copiar", help="Copia o conteúdo para a área de transferência.")
    ] = True,
) -> None:
    """Gera um .md para analisar as anotações com o Claude e copia para a área de transferência."""
    g = _giano(ctx)
    try:
        result = g.exports.export_claude(projeto, desde, todas, sem_projeto, saida)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(result.summary, markup=False)
    if copiar:
        if clipboard.copy(result.content):
            console.print("Conteúdo copiado para a área de transferência.")
        else:
            err.print("[yellow]Não foi possível copiar para a área de transferência.[/]")


@export_app.command("html")
def export_html(
    ctx: typer.Context,
    projeto: Annotated[str, typer.Option("--projeto", "-p", help="Sigla do projeto.")],
    rev: Annotated[Optional[int], typer.Option("--rev", "-r", help="Revisão (padrão: a última).")] = None,
    saida: Annotated[Optional[Path], typer.Option("--saida", "-o", help="Pasta de saída.")] = None,
    alteracoes: Annotated[
        bool, typer.Option("--alteracoes/--sem-alteracoes", help="Inclui a seção 'Alterações nesta revisão'.")
    ] = True,
    comparar_com: Annotated[
        Optional[int],
        typer.Option("--comparar-com", help="Revisão base da seção de alterações (padrão: a anterior)."),
    ] = None,
    itens: Annotated[
        Optional[str],
        typer.Option("--itens", "-i", help="Números dos itens a incluir, ex.: 1,3-5 (padrão: todos)."),
    ] = None,
) -> None:
    """Gera um HTML autocontido da lista de um projeto numa revisão."""
    g = _giano(ctx)
    try:
        numbers = parse_ranges(itens, lo=1) if itens else None
        result = g.exports.export_html(projeto, rev, saida, alteracoes, comparar_com, numbers)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(result.summary, markup=False)


@app.command()
def demo(ctx: typer.Context) -> None:
    """Gera dados de exemplo (projetos LOJA e RH, com revisões e anotações)."""
    from giano.services.demo import create_demo

    g = _giano(ctx)
    try:
        codes = create_demo(g)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(f"Dados de exemplo criados em {g.db_path}: projetos {', '.join(codes)}.")
    console.print("Abra a interface com: [bold]giano[/]")


@app.command()
def projetos(ctx: typer.Context) -> None:
    """Lista os projetos."""
    g = _giano(ctx)
    table = Table("Sigla", "Nome", "Rev.", "Itens", "Arquivados", "Situação")
    for p in g.projects.list():
        rev = g.items.latest_revision(p.id)
        items = g.items.list_items(p.id, rev)
        archived = sum(v.data.archived for v in items)
        table.add_row(p.code, Text(p.name), str(rev), str(len(items) - archived), str(archived),
                      "ativo" if p.active else "arquivado")
    console.print(table)


@app.command()
def revisoes(
    ctx: typer.Context,
    projeto: Annotated[str, typer.Option("--projeto", "-p", help="Sigla do projeto.")],
) -> None:
    """Lista as revisões de um projeto."""
    g = _giano(ctx)
    try:
        p = g.projects.require(projeto)
    except ValidationError as exc:
        _fail(str(exc))
        return
    table = Table("Rev.", "Data", "Resumo")
    for r in g.items.revisions(p.id):
        table.add_row(str(r.number), r.created_at.replace("T", " "), Text(r.summary))
    console.print(table)


@app.command()
def diff(
    ctx: typer.Context,
    projeto: Annotated[str, typer.Option("--projeto", "-p", help="Sigla do projeto.")],
    de: Annotated[int, typer.Argument(help="Revisão inicial.")],
    ate: Annotated[Optional[int], typer.Argument(help="Revisão final (padrão: a última).")] = None,
) -> None:
    """Mostra as diferenças entre duas revisões."""
    g = _giano(ctx)
    try:
        p = g.projects.require(projeto)
        result = g.items.diff(p.id, de, g.items.latest_revision(p.id) if ate is None else ate)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(Markdown(diff_markdown(result)))


@app.command()
def config(
    ctx: typer.Context,
    pasta_exportacao: Annotated[
        Optional[str],
        typer.Option("--pasta-exportacao", help="Define a pasta de exportação ('' volta ao padrão)."),
    ] = None,
) -> None:
    """Mostra (ou altera) as configurações."""
    g = _giano(ctx)
    if pasta_exportacao is not None:
        g.settings.export_dir = pasta_exportacao
    console.print(f"Banco de dados:      {g.db_path}")
    console.print(f"Pasta de exportação: {g.settings.export_dir}")
    console.print("Template do pedido de análise (edite na TUI, tecla g):")
    console.print(g.settings.claude_prompt, markup=False)


# ---------------------------------------------------------------------------
# Items (non-interactive: usable from scripts and by Claude)
# ---------------------------------------------------------------------------

TIPO_HELP = "Tipo: melhoria, discussao ou ajuste."
PRIORIDADE_HELP = "Prioridade: critica, alta, media ou baixa."
STATUS_HELP = "Status: aberto, em-analise, aprovado, em-desenvolvimento, concluido ou descartado."
CORPO_HELP = "Corpo em Markdown; use '-' para ler da entrada padrão (stdin)."


def _read_body(value: str | None) -> str | None:
    if value != "-":
        return value
    raw = sys.stdin.buffer.read()
    try:
        return raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        return raw.decode(locale.getpreferredencoding(False), errors="replace")


def _item(g: Giano, code: str) -> ItemView:
    view = g.items.get_by_code(code)
    if view is None:
        raise ValidationError(f"Item não encontrado: {code} (use SIGLA-NÚMERO, ex.: MERC-12).")
    return view


def _item_line(v: ItemView) -> str:
    d = v.data
    tags = ", ".join(d.tags) or "—"
    return (f"{v.code} · {d.title} · {d.type.label} · {d.priority.label} · "
            f"dif. {d.difficulty}/5 · {d.status.label} · tags: {tags}")


@app.command()
def itens(
    ctx: typer.Context,
    projeto: Annotated[str, typer.Option("--projeto", "-p", help="Sigla do projeto.")],
    arquivados: Annotated[
        bool, typer.Option("--arquivados", help="Mostra o arquivo em vez da lista normal.")
    ] = False,
    rev: Annotated[Optional[int], typer.Option("--rev", "-r", help="Revisão (padrão: a última).")] = None,
) -> None:
    """Lista os itens de um projeto (uma linha por item)."""
    g = _giano(ctx)
    try:
        p = g.projects.require(projeto)
        views = g.items.list_items(p.id, rev, archived=arquivados)
    except ValidationError as exc:
        _fail(str(exc))
        return
    if not views:
        console.print("Nenhum item arquivado." if arquivados else "Nenhum item.")
    for v in views:
        console.print(_item_line(v), markup=False, highlight=False, soft_wrap=True)


@item_app.command("novo")
def item_novo(
    ctx: typer.Context,
    titulo: Annotated[str, typer.Argument(help="Título do item.")],
    projeto: Annotated[str, typer.Option("--projeto", "-p", help="Sigla do projeto.")],
    tipo: Annotated[str, typer.Option("--tipo", "-t", help=TIPO_HELP)] = "melhoria",
    prioridade: Annotated[str, typer.Option("--prioridade", help=PRIORIDADE_HELP)] = "media",
    dificuldade: Annotated[int, typer.Option("--dificuldade", "-d", help="Dificuldade de 1 a 5.")] = 3,
    status: Annotated[str, typer.Option("--status", "-s", help=STATUS_HELP)] = "aberto",
    tags: Annotated[str, typer.Option("--tags", help="Tags separadas por vírgula.")] = "",
    corpo: Annotated[Optional[str], typer.Option("--corpo", "-c", help=CORPO_HELP)] = None,
) -> None:
    """Cria um item na lista de um projeto (gera uma nova revisão)."""
    g = _giano(ctx)
    try:
        p = g.projects.require(projeto)
        data = ItemData(
            title=titulo,
            type=parse_choice(ItemType, tipo),
            priority=parse_choice(Priority, prioridade),
            difficulty=dificuldade,
            status=parse_choice(ItemStatus, status),
            tags=tags,
            body=_read_body(corpo) or "",
        )
        v = g.items.create(p.id, data)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(f"{v.code} criado (Rev. {v.rev_start}): {v.data.title}", markup=False, highlight=False)


@item_app.command("ver")
def item_ver(
    ctx: typer.Context,
    codigo: Annotated[str, typer.Argument(help="Identificador do item, ex.: MERC-12.")],
) -> None:
    """Mostra um item (Markdown)."""
    g = _giano(ctx)
    try:
        v = _item(g, codigo)
    except ValidationError as exc:
        _fail(str(exc))
        return
    note = g.notes.get(v.source_note_id) if v.source_note_id else None
    console.print(item_preview(v, note), markup=False, highlight=False, soft_wrap=True)


@item_app.command("editar")
def item_editar(
    ctx: typer.Context,
    codigo: Annotated[str, typer.Argument(help="Identificador do item, ex.: MERC-12.")],
    titulo: Annotated[Optional[str], typer.Option("--titulo", help="Novo título.")] = None,
    tipo: Annotated[Optional[str], typer.Option("--tipo", "-t", help=TIPO_HELP)] = None,
    prioridade: Annotated[Optional[str], typer.Option("--prioridade", help=PRIORIDADE_HELP)] = None,
    dificuldade: Annotated[Optional[int], typer.Option("--dificuldade", "-d", help="1 a 5.")] = None,
    status: Annotated[Optional[str], typer.Option("--status", "-s", help=STATUS_HELP)] = None,
    tags: Annotated[
        Optional[str], typer.Option("--tags", help="Substitui as tags (separadas por vírgula).")
    ] = None,
    corpo: Annotated[Optional[str], typer.Option("--corpo", "-c", help="Substitui o corpo. " + CORPO_HELP)] = None,
) -> None:
    """Altera campos de um item; só os informados mudam (gera uma nova revisão)."""
    g = _giano(ctx)
    try:
        v = _item(g, codigo)
        changes: dict = {}
        if titulo is not None:
            changes["title"] = titulo
        if tipo is not None:
            changes["type"] = parse_choice(ItemType, tipo)
        if prioridade is not None:
            changes["priority"] = parse_choice(Priority, prioridade)
        if dificuldade is not None:
            changes["difficulty"] = dificuldade
        if status is not None:
            changes["status"] = parse_choice(ItemStatus, status)
        if tags is not None:
            changes["tags"] = tags
        if corpo is not None:
            changes["body"] = _read_body(corpo) or ""
        if not changes:
            raise ValidationError("Informe ao menos um campo para alterar (veja --help).")
        new = g.items.update(v.item_id, replace(v.data, **changes))
    except ValidationError as exc:
        _fail(str(exc))
        return
    if new is None:
        console.print(f"{v.code}: nenhuma alteração; nenhuma revisão criada.", markup=False)
    else:
        summary = g.items.revision(new.project_id, new.rev_start).summary
        console.print(f"Rev. {new.rev_start} — {summary}", markup=False, highlight=False)


@item_app.command("arquivar")
def item_arquivar(
    ctx: typer.Context,
    codigo: Annotated[str, typer.Argument(help="Identificador do item, ex.: MERC-12.")],
) -> None:
    """Arquiva um item: sai da lista normal e das exportações, sem perder nada."""
    g = _giano(ctx)
    try:
        v = g.items.archive(_item(g, codigo).item_id)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(f"{v.code} arquivado (Rev. {v.rev_start}).", markup=False, highlight=False)


@item_app.command("restaurar")
def item_restaurar(
    ctx: typer.Context,
    codigo: Annotated[str, typer.Argument(help="Identificador do item, ex.: MERC-12.")],
) -> None:
    """Devolve um item arquivado para a lista normal."""
    g = _giano(ctx)
    try:
        v = g.items.unarchive(_item(g, codigo).item_id)
    except ValidationError as exc:
        _fail(str(exc))
        return
    console.print(f"{v.code} restaurado à lista (Rev. {v.rev_start}).", markup=False, highlight=False)


def _utf8_when_redirected() -> None:
    """Piped/redirected output (scripts, Claude) defaults to the ANSI code page on
    Windows, which garbles accents and cannot encode characters like "→". A real
    terminal is left alone."""
    for stream in (sys.stdout, sys.stderr):
        try:
            if not stream.isatty():
                stream.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError, OSError):
            pass


def main() -> None:
    _utf8_when_redirected()
    app()


if __name__ == "__main__":
    main()
