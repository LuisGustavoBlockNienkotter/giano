"""Sample data for ``giano demo``."""

from __future__ import annotations

from dataclasses import replace
from datetime import date, timedelta

from giano.domain import ItemData, ItemStatus, ItemType, NoteStatus, Priority, ValidationError
from giano.services import Giano

I, D, A = ItemType.IMPROVEMENT, ItemType.DISCUSSION, ItemType.ADJUSTMENT
CRIT, HIGH, MED, LOW = Priority.CRITICAL, Priority.HIGH, Priority.MEDIUM, Priority.LOW

LOJA_DESC = """\
Loja virtual B2C de materiais de construção.

- **Backend**: Python/Django, PostgreSQL
- **Frontend**: Next.js
- **Integrações**: gateway de pagamento, ERP (pedidos e estoque), transportadoras

Público principal: pequenos construtores e consumidores finais. Pico de acesso às segundas-feiras.
"""

RH_DESC = """\
Portal interno de RH: férias, holerites, ponto eletrônico e avaliação de desempenho.

Usado por ~800 colaboradores. Autenticação via SSO corporativo.
"""

LOJA_ITEMS = [
    ItemData("Checkout em uma página", I, HIGH, 4, ItemStatus.APPROVED, ("checkout", "ux"),
             "Unificar endereço, frete e pagamento numa só tela.\n\n"
             "## Critérios de aceite\n\n- Sem recarregar a página\n- Frete recalculado ao trocar o CEP\n"
             "- Manter o carrinho se o pagamento falhar"),
    ItemData("Cálculo de frete por volume", A, CRIT, 3, ItemStatus.IN_PROGRESS, ("frete", "integração"),
             "Hoje o frete considera só o peso. Itens volumosos (telhas, tubos) ficam com frete subestimado.\n\n"
             "```python\ndef peso_cubado(alt, larg, comp, fator=300):\n    return alt * larg * comp * fator\n```\n\n"
             "Usar `max(peso_real, peso_cubado)`."),
    ItemData("Avise-me quando chegar", I, MED, 2, ItemStatus.OPEN, ("estoque", "ux"),
             "Permitir que o cliente deixe o e-mail em produtos sem estoque."),
    ItemData("Política de devolução de itens cortados sob medida", D, MED, 1, ItemStatus.ANALYSIS,
             ("jurídico",), "Definir com o jurídico se itens cortados sob medida podem ser devolvidos."),
    ItemData("Timeout na sincronização de estoque com o ERP", A, HIGH, 3, ItemStatus.OPEN,
             ("integração", "erp"),
             "A rotina noturna falha quando o ERP demora mais de 30 s.\n\n"
             "| Dia | Duração | Resultado |\n|---|---|---|\n| seg | 42 s | falhou |\n| ter | 18 s | ok |"),
    ItemData("Busca com tolerância a erros de digitação", I, MED, 4, ItemStatus.OPEN, ("busca",),
             "Ex.: \"cimento\" deve achar \"cimneto\"."),
    ItemData("Banner rotativo na home", I, LOW, 1, ItemStatus.OPEN, ("marketing",),
             "Pedido do marketing para campanhas sazonais."),
]

RH_ITEMS = [
    ItemData("Solicitação de férias pelo celular", I, HIGH, 3, ItemStatus.OPEN, ("mobile", "férias"),
             "Layout responsivo para a tela de férias."),
    ItemData("Holerite em PDF com assinatura digital", I, MED, 4, ItemStatus.ANALYSIS, ("holerite",),
             "Avaliar certificado ICP-Brasil."),
    ItemData("Arredondamento de horas no banco de horas", A, CRIT, 2, ItemStatus.APPROVED, ("ponto",),
             "Minutos residuais estão sendo truncados em vez de arredondados."),
    ItemData("Ciclo de avaliação semestral ou anual?", D, LOW, 1, ItemStatus.OPEN, ("avaliação",),
             "Discutir com a diretoria."),
]


def create_demo(g: Giano) -> list[str]:
    """Create the demo projects. Returns the codes created. Fails if they already exist."""
    for code in ("LOJA", "RH"):
        if g.projects.get_by_code(code):
            raise ValidationError(
                f"O projeto de exemplo {code} já existe neste banco. "
                "Use outro banco (GIANO_DB / --db) para gerar a demonstração novamente."
            )

    loja = g.projects.create("LOJA", "Loja Virtual", LOJA_DESC)
    rh = g.projects.create("RH", "Portal de RH", RH_DESC)

    loja_items = [g.items.create(loja.id, d) for d in LOJA_ITEMS]
    rh_items = [g.items.create(rh.id, d) for d in RH_ITEMS]

    # Some history on LOJA: priority change, status changes, tags, an edit and a deletion.
    g.items.set_priority(loja_items[2].item_id, HIGH)
    g.items.set_status(loja_items[0].item_id, ItemStatus.IN_PROGRESS)
    g.items.set_tags(loja_items[4].item_id, ("integração", "erp", "noturno"))
    v = g.items.get(loja_items[5].item_id)
    g.items.update(
        v.item_id,
        replace(v.data, title="Busca tolerante a erros de digitação e sinônimos",
                body=v.data.body + "\n\nIncluir sinônimos: \"argamassa\" ≈ \"massa\"."),
    )
    g.items.delete(loja_items[6].item_id)
    g.items.set_status(loja_items[1].item_id, ItemStatus.DONE)
    g.items.set_status(rh_items[2].item_id, ItemStatus.IN_PROGRESS)

    today = date.today()
    n1 = g.notes.create(
        "Reunião semanal com o comercial", loja.id,
        "- Clientes reclamam do frete de telhas: muito caro em algumas regiões\n"
        "- Pedir parcelamento em 12x sem juros acima de R$ 2.000\n"
        "- Avise-me: comercial quer também por WhatsApp\n",
        today - timedelta(days=6),
    )
    g.notes.create(
        "Alinhamento com o time do ERP", loja.id,
        "O ERP vai ganhar um endpoint de estoque incremental em dezembro.\n"
        "Com isso a sincronização noturna pode virar a cada 15 minutos.\n\n"
        "**Atenção**: o endpoint antigo será desligado em março.",
        today - timedelta(days=2),
    )
    g.notes.create(
        "Ideias soltas", loja.id,
        "- Programa de fidelidade para construtores\n- Lista de compras compartilhada por obra",
        today,
    )
    n_rh = g.notes.create(
        "Comitê de RH", rh.id,
        "Diretoria decidiu: avaliação **anual**, com check-in semestral informal.",
        today - timedelta(days=3),
    )
    g.notes.create("Pendências diversas", None, "Ver orçamento de licenças do SSO.", today)

    # An item created from a note keeps the link to its origin.
    g.notes.create_item(
        n1.id, loja.id,
        ItemData("Parcelamento em 12x sem juros acima de R$ 2.000", I, MED, 2, ItemStatus.OPEN,
                 ("pagamento", "comercial"), "Pedido do comercial na reunião semanal."),
    )
    g.notes.set_status(n_rh.id, NoteStatus.PROCESSED)
    return ["LOJA", "RH"]
