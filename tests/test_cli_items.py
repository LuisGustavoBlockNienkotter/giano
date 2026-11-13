"""Non-interactive item commands: giano itens / giano item novo|ver|editar|arquivar|restaurar."""

from __future__ import annotations

import pytest
from typer.testing import CliRunner

from giano.cli import app
from giano.domain import ItemStatus, ItemType, Priority, ValidationError, parse_choice
from tests.conftest import make


def cli(g, *args, input=None):
    return CliRunner().invoke(app, ["--db", g.db_path, *args], input=input)


@pytest.mark.parametrize(
    "text, expected",
    [
        ("em-analise", ItemStatus.ANALYSIS),
        ("Em análise", ItemStatus.ANALYSIS),
        ("analysis", ItemStatus.ANALYSIS),
        ("in_progress", ItemStatus.IN_PROGRESS),
        ("EM DESENVOLVIMENTO", ItemStatus.IN_PROGRESS),
        ("concluido", ItemStatus.DONE),
    ],
)
def test_parse_choice_accepts_values_and_labels(text, expected):
    assert parse_choice(ItemStatus, text) is expected


def test_parse_choice_lists_options_on_error():
    with pytest.raises(ValidationError, match="Melhoria, Discussão, Ajuste"):
        parse_choice(ItemType, "bug")


def test_item_novo_with_body_from_stdin(g, project):
    res = cli(g, "item", "novo", "Integração com ERP", "-p", "merc", "-t", "ajuste",
              "--prioridade", "crítica", "-d", "4", "-s", "aprovado", "--tags", "erp, api",
              "-c", "-", input="## Contexto\n\nAcentuação: ção.\n")
    assert res.exit_code == 0, res.output
    assert "MERC-1 criado (Rev. 1): Integração com ERP" in res.output
    v = g.items.get_by_code("MERC-1")
    assert v.data == make("Integração com ERP", type=ItemType.ADJUSTMENT,
                          priority=Priority.CRITICAL, difficulty=4, status=ItemStatus.APPROVED,
                          tags=("api", "erp"), body="## Contexto\n\nAcentuação: ção.")


def test_item_novo_defaults_and_errors(g, project):
    res = cli(g, "item", "novo", "Simples", "-p", "MERC")
    assert res.exit_code == 0, res.output
    assert g.items.get_by_code("MERC-1").data == make("Simples")

    res = cli(g, "item", "novo", "X", "-p", "NADA")
    assert res.exit_code == 1 and "NADA" in res.output
    res = cli(g, "item", "novo", "X", "-p", "MERC", "--prioridade", "urgente")
    assert res.exit_code == 1 and "Crítica, Alta, Média, Baixa" in res.output
    assert g.items.latest_revision(project.id) == 1


def test_itens_lists_normal_list_or_archive(g, project):
    g.items.create(project.id, make("Ativo", tags=("ux",)))
    b = g.items.create(project.id, make("Velho", status=ItemStatus.DONE))
    g.items.archive(b.item_id)

    res = cli(g, "itens", "-p", "MERC")
    assert res.exit_code == 0, res.output
    assert "MERC-1 · Ativo · Melhoria · Média · dif. 3/5 · Aberto · tags: ux" in res.output
    assert "Velho" not in res.output
    res = cli(g, "itens", "-p", "MERC", "--arquivados")
    assert "MERC-2 · Velho" in res.output and "Ativo" not in res.output


def test_item_ver(g, project):
    g.items.create(project.id, make("Ver isto", body="Detalhes"))
    res = cli(g, "item", "ver", "merc-1")
    assert res.exit_code == 0, res.output
    assert "# MERC-1 — Ver isto" in res.output and "Detalhes" in res.output
    assert cli(g, "item", "ver", "MERC-9").exit_code == 1


def test_item_editar_changes_only_given_fields(g, project):
    g.items.create(project.id, make("Original", tags=("a",), body="corpo"))
    res = cli(g, "item", "editar", "MERC-1", "-s", "em-desenvolvimento", "--prioridade", "alta")
    assert res.exit_code == 0, res.output
    assert "Rev. 2 — MERC-1: prioridade Média → Alta; status Aberto → Em desenvolvimento" in res.output
    v = g.items.get_by_code("MERC-1")
    assert (v.data.title, v.data.tags, v.data.body) == ("Original", ("a",), "corpo")

    res = cli(g, "item", "editar", "MERC-1", "--prioridade", "alta")
    assert "nenhuma alteração" in res.output
    assert cli(g, "item", "editar", "MERC-1").exit_code == 1
    assert g.items.latest_revision(project.id) == 2


def test_item_arquivar_e_restaurar(g, project):
    g.items.create(project.id, make("A"))
    res = cli(g, "item", "arquivar", "MERC-1")
    assert res.exit_code == 0 and "MERC-1 arquivado (Rev. 2)" in res.output
    res = cli(g, "item", "editar", "MERC-1", "--titulo", "B")
    assert res.exit_code == 1 and "arquivado" in res.output
    res = cli(g, "item", "restaurar", "MERC-1")
    assert res.exit_code == 0 and "MERC-1 restaurado à lista (Rev. 3)" in res.output
