//! Sample data (`giano demo` in the Python version; the "dados de exemplo" button here).

use chrono::Duration;

use crate::domain::{ItemData, ItemStatus, ItemType, NoteStatus, Priority};
use crate::error::{Error, Result};
use crate::services::Giano;

const LOJA_DESC: &str = "\
Loja virtual B2C de materiais de construção.

- **Backend**: Python/Django, PostgreSQL
- **Frontend**: Next.js
- **Integrações**: gateway de pagamento, ERP (pedidos e estoque), transportadoras

Público principal: pequenos construtores e consumidores finais. Pico de acesso às segundas-feiras.
";

const RH_DESC: &str = "\
Portal interno de RH: férias, holerites, ponto eletrônico e avaliação de desempenho.

Usado por ~800 colaboradores. Autenticação via SSO corporativo.
";

fn item(title: &str, kind: ItemType, priority: Priority, difficulty: i64, status: ItemStatus, tags: &[&str], body: &str) -> ItemData {
    ItemData {
        title: title.into(),
        kind,
        priority,
        difficulty,
        status,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        body: body.into(),
        archived: false,
    }
}

/// Create the demo projects. Returns the codes created. Fails if they already exist.
pub fn create_demo(g: &Giano) -> Result<Vec<String>> {
    use ItemStatus as S;
    use ItemType::{Adjustment as A, Discussion as D, Improvement as I};
    use Priority::{Critical as CRIT, High as HIGH, Low as LOW, Medium as MED};

    for code in ["LOJA", "RH"] {
        if g.projects().get_by_code(code)?.is_some() {
            return Err(Error::validation(format!(
                "O projeto de exemplo {code} já existe neste banco. Use outro banco (GIANO_DB) para \
                 gerar a demonstração novamente."
            )));
        }
    }
    let loja = g.projects().create("LOJA", "Loja Virtual", LOJA_DESC)?;
    let rh = g.projects().create("RH", "Portal de RH", RH_DESC)?;

    let loja_data = [
        item("Checkout em uma página", I, HIGH, 4, S::Approved, &["checkout", "ux"],
             "Unificar endereço, frete e pagamento numa só tela.\n\n## Critérios de aceite\n\n- Sem recarregar a página\n- Frete recalculado ao trocar o CEP\n- Manter o carrinho se o pagamento falhar"),
        item("Cálculo de frete por volume", A, CRIT, 3, S::InProgress, &["frete", "integração"],
             "Hoje o frete considera só o peso. Itens volumosos (telhas, tubos) ficam com frete subestimado.\n\n```python\ndef peso_cubado(alt, larg, comp, fator=300):\n    return alt * larg * comp * fator\n```\n\nUsar `max(peso_real, peso_cubado)`."),
        item("Avise-me quando chegar", I, MED, 2, S::Open, &["estoque", "ux"],
             "Permitir que o cliente deixe o e-mail em produtos sem estoque."),
        item("Política de devolução de itens cortados sob medida", D, MED, 1, S::Analysis, &["jurídico"],
             "Definir com o jurídico se itens cortados sob medida podem ser devolvidos."),
        item("Timeout na sincronização de estoque com o ERP", A, HIGH, 3, S::Open, &["integração", "erp"],
             "A rotina noturna falha quando o ERP demora mais de 30 s.\n\n| Dia | Duração | Resultado |\n|---|---|---|\n| seg | 42 s | falhou |\n| ter | 18 s | ok |"),
        item("Busca com tolerância a erros de digitação", I, MED, 4, S::Open, &["busca"],
             "Ex.: \"cimento\" deve achar \"cimneto\"."),
        item("Banner rotativo na home", I, LOW, 1, S::Open, &["marketing"],
             "Pedido do marketing para campanhas sazonais."),
    ];
    let rh_data = [
        item("Solicitação de férias pelo celular", I, HIGH, 3, S::Open, &["mobile", "férias"],
             "Layout responsivo para a tela de férias."),
        item("Holerite em PDF com assinatura digital", I, MED, 4, S::Analysis, &["holerite"],
             "Avaliar certificado ICP-Brasil."),
        item("Arredondamento de horas no banco de horas", A, CRIT, 2, S::Approved, &["ponto"],
             "Minutos residuais estão sendo truncados em vez de arredondados."),
        item("Ciclo de avaliação semestral ou anual?", D, LOW, 1, S::Open, &["avaliação"],
             "Discutir com a diretoria."),
    ];

    let items = g.items();
    let mut loja_items = Vec::new();
    for d in &loja_data {
        loja_items.push(items.create(loja.id, d, None)?);
    }
    let mut rh_items = Vec::new();
    for d in &rh_data {
        rh_items.push(items.create(rh.id, d, None)?);
    }

    // Some history on LOJA: priority change, status changes, tags, an edit and a deletion.
    items.set_priority(loja_items[2].item_id, HIGH)?;
    items.set_status(loja_items[0].item_id, S::InProgress)?;
    items.set_tags(loja_items[4].item_id, &["integração", "erp", "noturno"])?;
    if let Some(v) = items.get(loja_items[5].item_id, None)? {
        let data = ItemData {
            title: "Busca tolerante a erros de digitação e sinônimos".into(),
            body: format!("{}\n\nIncluir sinônimos: \"argamassa\" ≈ \"massa\".", v.data.body),
            ..v.data.clone()
        };
        items.update(v.item_id, &data)?;
    }
    items.delete(loja_items[6].item_id)?;
    items.set_status(loja_items[1].item_id, S::Done)?;
    // The freight item came back from testing with an adjustment: an addendum (LOJA-2.1).
    items.create_addendum(
        loja_items[1].item_id,
        &item("Frete por volume: tubos com cubagem errada", A, HIGH, 2, S::Open, &["frete"],
              "Na homologação, tubos de 6 m saíram com frete menor que o esperado: a cubagem usa o comprimento em **cm** e o cadastro está em **m**.\n\n- Converter as medidas antes de `peso_cubado`\n- Testar com tubo de 6 m e telha colonial"),
        None,
    )?;
    // An old decision, discarded and archived: out of the list, still kept.
    items.set_status(loja_items[3].item_id, S::Discarded)?;
    items.archive(loja_items[3].item_id)?;
    items.set_status(rh_items[2].item_id, S::InProgress)?;

    let today = chrono::Local::now().date_naive();
    let day = |offset: i64| (today - Duration::days(offset)).format("%Y-%m-%d").to_string();
    let notes = g.notes();
    let n1 = notes.create(
        "Reunião semanal com o comercial",
        Some(loja.id),
        "- Clientes reclamam do frete de telhas: muito caro em algumas regiões\n- Pedir parcelamento em 12x sem juros acima de R$ 2.000\n- Avise-me: comercial quer também por WhatsApp\n",
        Some(&day(6)),
    )?;
    notes.create(
        "Alinhamento com o time do ERP",
        Some(loja.id),
        "O ERP vai ganhar um endpoint de estoque incremental em dezembro.\nCom isso a sincronização noturna pode virar a cada 15 minutos.\n\n**Atenção**: o endpoint antigo será desligado em março.",
        Some(&day(2)),
    )?;
    notes.create(
        "Ideias soltas",
        Some(loja.id),
        "- Programa de fidelidade para construtores\n- Lista de compras compartilhada por obra",
        Some(&day(0)),
    )?;
    let n_rh = notes.create(
        "Comitê de RH",
        Some(rh.id),
        "Diretoria decidiu: avaliação **anual**, com check-in semestral informal.",
        Some(&day(3)),
    )?;
    notes.create("Pendências diversas", None, "Ver orçamento de licenças do SSO.", Some(&day(0)))?;

    // An item created from a note keeps the link to its origin.
    notes.create_item(
        n1.id,
        loja.id,
        &item("Parcelamento em 12x sem juros acima de R$ 2.000", I, MED, 2, S::Open, &["pagamento", "comercial"],
              "Pedido do comercial na reunião semanal."),
    )?;
    notes.set_status(n_rh.id, NoteStatus::Processed)?;
    Ok(vec!["LOJA".into(), "RH".into()])
}
