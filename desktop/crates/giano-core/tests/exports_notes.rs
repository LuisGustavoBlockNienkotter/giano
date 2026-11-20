//! Claude/HTML exports, item selection, notes, search, projects and settings.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use giano_core::config::DEFAULT_CLAUDE_PROMPT;
use giano_core::db::notes::NoteFilter;
use giano_core::domain::{ItemStatus, ItemType, ItemView, NoteStatus, Priority};
use giano_core::services::exports::{ClaudeOptions, HtmlOptions};
use giano_core::services::items::Archived;
use giano_core::services::notes::NoteInput;
use giano_core::services::search::HitKind;
use giano_core::Giano;

fn populated(g: &Giano, pid: i64) -> (ItemView, ItemView) {
    let a = g
        .items()
        .create(
            pid,
            &make("Checkout rápido")
                .priority(Priority::High)
                .difficulty(4)
                .tags(&["ux", "checkout"])
                .body("## Detalhes\n\n```python\nprint('oi')\n```"),
            None,
        )
        .unwrap();
    let b = g
        .items()
        .create(pid, &make("Bug no frete").kind(ItemType::Adjustment).priority(Priority::Critical), None)
        .unwrap();
    g.items().create(pid, &make("Política <b>de</b> devolução").kind(ItemType::Discussion), None).unwrap();
    g.items().set_status(b.item_id, ItemStatus::Done).unwrap();
    let notes = g.notes();
    notes.create("Reunião 1", Some(pid), "# Pauta\nfrete caro", Some("2026-03-01")).unwrap();
    let n2 = notes.create("Reunião 2", Some(pid), "parcelamento", Some("2026-03-05")).unwrap();
    notes.create("Sem projeto", None, "solta", Some("2026-03-06")).unwrap();
    notes.create("Antiga", Some(pid), "velha", Some("2026-01-01")).unwrap();
    notes.set_status(n2.id, NoteStatus::Processed).unwrap();
    (a, b)
}

fn sections(md: &str) -> Vec<&str> {
    md.lines().filter_map(|l| l.strip_prefix("## ")).collect()
}

#[test]
fn claude_export_structure() {
    let g = env();
    let p = project(&g);
    populated(&g, p.id);
    let r = g.exports().export_claude("merc", &ClaudeOptions::default()).unwrap();
    let md = &r.content;
    assert_eq!(r.path.file_name().unwrap(), "giano_MERC_claude_2026-03-10.md");
    assert_eq!(&std::fs::read_to_string(&r.path).unwrap(), md);
    assert_eq!(
        sections(md),
        ["1. Contexto do projeto", "2. Lista atual de melhorias (Rev. 4)", "3. Anotações (pendentes)", "4. Pedido de análise"]
    );
    assert!(md.contains("Sistema de **gestão**."));
    assert!(md.find("### MERC-2 — Bug no frete").unwrap() < md.find("### MERC-1 — Checkout rápido").unwrap());
    assert!(md.contains("Tipo: Melhoria · Prioridade: Alta · Dificuldade: 4/5 · Status: Aberto · Tags: checkout, ux"));
    assert!(md.contains("##### Detalhes") && md.contains("print('oi')"));
    assert!(md.find("01/01/2026 — Antiga").unwrap() < md.find("01/03/2026 — Reunião 1").unwrap());
    assert!(!md.contains("Reunião 2") && !md.contains("Sem projeto"));
    assert!(md.contains("#### Pauta"));
    assert!(md.to_lowercase().contains("relacione"));
}

#[test]
fn claude_export_options_and_prompt() {
    let g = env();
    let p = project(&g);
    populated(&g, p.id);
    let ex = g.exports();
    let md = ex.export_claude("MERC", &ClaudeOptions { all_notes: true, ..Default::default() }).unwrap().content;
    assert!(md.contains("Reunião 2") && md.contains("(todas)"));
    let md = ex.export_claude("MERC", &ClaudeOptions { since: Some("2026-02-01".into()), ..Default::default() }).unwrap().content;
    assert!(!md.contains("Antiga") && md.contains("Reunião 1") && md.contains("desde 01/02/2026"));
    let md = ex.export_claude("MERC", &ClaudeOptions { include_unassigned: true, ..Default::default() }).unwrap().content;
    assert!(md.contains("Sem projeto (sem projeto)"));
    g.settings().set_claude_prompt(Some("Analise o {sigla} ({nome}) por favor.")).unwrap();
    let md = ex.export_claude("MERC", &ClaudeOptions::default()).unwrap().content;
    assert!(md.trim_end().ends_with("Analise o MERC (Mercurius) por favor."));
    assert!(ex.export_claude("NOPE", &ClaudeOptions::default()).is_err());
    let out = g.dir.path().join("x").join("saida.md");
    let r = ex.export_claude("MERC", &ClaudeOptions { output: Some(out.clone()), ..Default::default() }).unwrap();
    assert_eq!(r.path, out);
    assert!(out.exists());
}

#[test]
fn html_export_latest() {
    let g = env();
    let p = project(&g);
    populated(&g, p.id);
    let r = g.exports().export_html("MERC", &HtmlOptions::default(), None).unwrap();
    let html = &r.content;
    assert_eq!(r.path.file_name().unwrap(), "giano_MERC_rev4_2026-03-10.html");
    assert_eq!(&std::fs::read_to_string(&r.path).unwrap(), html);
    // self-contained: inline CSS, no external resources
    assert!(html.contains("<style>"));
    assert!(!html.contains("<link") && !html.contains("<script"));
    assert!(!html.contains("src=\"http") && !html.contains("href=\"http"));
    assert!(html.contains("Mercurius") && html.contains("Revisão <strong>4</strong>"));
    assert!(html.contains("href=\"#merc-1\"") && html.contains("id=\"merc-1\""));
    let pos = |s: &str| html.find(s).unwrap();
    assert!(pos("id=\"tipo-improvement\"") < pos("id=\"tipo-discussion\""));
    assert!(pos("id=\"tipo-discussion\"") < pos("id=\"tipo-adjustment\""));
    assert!(html.contains("Prioridade: Crítica") && html.contains("Dificuldade: 4/5") && html.contains("#checkout"));
    assert!(html.contains("<h2>Detalhes</h2>"));
    assert!(html.contains("class=\"highlight\"") && html.contains("class=\"hl-"));
    assert!(html.contains("Política &lt;b&gt;de&lt;/b&gt; devolução"), "título escapado");
    assert!(html.contains("Alterações nesta revisão"));
    assert!(html.contains("status: Aberto → <strong>Concluído</strong>"));
    assert!(html.contains("10/03/2026"));
}

#[test]
fn html_export_revisions_and_compare() {
    let g = env();
    let p = project(&g);
    populated(&g, p.id);
    let ex = g.exports();
    let r = ex.export_html("MERC", &HtmlOptions { rev: Some(2), ..Default::default() }, None).unwrap();
    assert_eq!(r.path.file_name().unwrap(), "giano_MERC_rev2_2026-03-10.html");
    assert!(r.content.contains("Checkout rápido") && r.content.contains("Bug no frete"));
    assert!(!r.content.contains("Política") && r.content.contains("(de 4)"));
    let html = ex.export_html("MERC", &HtmlOptions { changes: false, ..Default::default() }, None).unwrap().content;
    assert!(!html.contains("Alterações nesta revisão"));
    let html = ex.export_html("MERC", &HtmlOptions { compare_to: Some(0), ..Default::default() }, None).unwrap().content;
    assert!(html.contains("Adicionados (3)"));
    assert!(ex.export_html("MERC", &HtmlOptions { rev: Some(2), compare_to: Some(3), ..Default::default() }, None).is_err());
    assert!(ex.export_html("MERC", &HtmlOptions { rev: Some(10), ..Default::default() }, None).is_err());
}

#[test]
fn html_export_empty_project() {
    let g = env();
    project(&g);
    let r = g.exports().export_html("MERC", &HtmlOptions::default(), None).unwrap();
    assert!(r.content.contains("Nenhum item nesta revisão"));
    assert!(r.path.file_name().unwrap().to_string_lossy().starts_with("giano_MERC_rev0_"));
}

// ---------------------------------------------------------------- selection

fn five(g: &Giano, pid: i64) -> Vec<ItemView> {
    let vs: Vec<ItemView> =
        (1..=5).map(|n| g.items().create(pid, &make(&format!("Item {n}")).difficulty(n), None).unwrap()).collect();
    g.items().set_priority(vs[1].item_id, Priority::High).unwrap(); // rev 6: MERC-2
    g.items().set_priority(vs[3].item_id, Priority::Low).unwrap(); // rev 7: MERC-4
    vs
}

fn keys(numbers: &[i64]) -> Option<BTreeSet<(i64, i64)>> {
    Some(numbers.iter().map(|n| (*n, 0)).collect())
}

#[test]
fn html_subset_only_has_selected_items() {
    let g = env();
    let p = project(&g);
    five(&g, p.id);
    let opts = HtmlOptions { keys: keys(&[1, 2]), compare_to: Some(5), ..Default::default() };
    let html = g.exports().export_html("MERC", &opts, None).unwrap().content;
    assert!(html.contains("Item 1") && html.contains("Item 2"));
    for n in 3..=5 {
        assert!(!html.contains(&format!("Item {n}")) && !html.contains(&format!("merc-{n}")));
    }
    assert!(html.contains("2 itens (seleção de 5)"));
    assert!(html.split("id=\"alteracoes\"").nth(1).unwrap().contains("MERC-2"));
    assert!(!html.contains("prioridade Média → Baixa"));
    assert!(!html.contains("<td>7</td>"));
    let all = g.exports().export_html("MERC", &HtmlOptions::default(), None).unwrap().content;
    assert!((1..=5).all(|n| all.contains(&format!("Item {n}"))) && !all.contains("seleção de"));
}

#[test]
fn html_subset_errors() {
    let g = env();
    let p = project(&g);
    let vs = five(&g, p.id);
    let ex = g.exports();
    let msg = err_msg(ex.export_html("MERC", &HtmlOptions { keys: keys(&[1, 9]), ..Default::default() }, None));
    assert!(msg.contains("MERC-9"));
    assert!(ex.export_html("MERC", &HtmlOptions { keys: keys(&[]), ..Default::default() }, None).is_err());
    g.items().delete(vs[0].item_id).unwrap();
    assert!(err_msg(ex.export_html("MERC", &HtmlOptions { keys: keys(&[1]), ..Default::default() }, None)).contains("MERC-1"));
    let opts = HtmlOptions { rev: Some(5), keys: keys(&[1]), ..Default::default() };
    assert!(ex.export_html("MERC", &opts, None).unwrap().content.contains("Item 1"));
}

fn statuses(g: &Giano, pid: i64) -> BTreeMap<i64, ItemStatus> {
    g.items().list_items(pid, None, Archived::Any).unwrap().into_iter().map(|v| (v.number, v.data.status)).collect()
}

#[test]
fn html_mark_in_progress() {
    let g = env();
    let p = project(&g);
    let vs = five(&g, p.id);
    g.items().set_status(vs[3].item_id, ItemStatus::Done).unwrap(); // rev 8
    g.items().set_status(vs[4].item_id, ItemStatus::InProgress).unwrap(); // rev 9
    g.items().set_status(vs[1].item_id, ItemStatus::Approved).unwrap(); // rev 10
    let mut calls = Vec::new();
    let mut progress = |d: usize, t: usize| calls.push((d, t));
    let opts = HtmlOptions { keys: keys(&[1, 2, 4, 5]), mark_in_progress: true, ..Default::default() };
    let r = g.exports().export_html("MERC", &opts, Some(&mut progress)).unwrap();
    assert_eq!(r.marked, ["MERC-1", "MERC-2"]);
    assert_eq!(calls, [(0, 2), (1, 2), (2, 2)]);
    assert!(r.summary.contains("2 itens marcados como Em desenvolvimento (MERC-1, MERC-2)"));
    assert!(r.path.file_name().unwrap().to_string_lossy().starts_with("giano_MERC_rev10_"));
    use ItemStatus::*;
    assert_eq!(statuses(&g, p.id), [(1, InProgress), (2, InProgress), (3, Open), (4, Done), (5, InProgress)].into());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 12);
    let opts = HtmlOptions { keys: keys(&[1, 2]), mark_in_progress: true, ..Default::default() };
    assert!(g.exports().export_html("MERC", &opts, None).unwrap().marked.is_empty());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 12);
}

#[test]
fn html_mark_in_progress_rules() {
    let g = env();
    let p = project(&g);
    five(&g, p.id);
    let old = g.dir.path().join("old");
    let opts = HtmlOptions { rev: Some(5), out_dir: Some(old.clone()), mark_in_progress: true, ..Default::default() };
    assert!(err_msg(g.exports().export_html("MERC", &opts, None)).contains("última revisão"));
    assert!(!old.exists());
    // The file cannot be written (a file where the folder should be): nothing changes.
    let blocker = g.dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let opts = HtmlOptions { out_dir: Some(blocker), mark_in_progress: true, ..Default::default() };
    assert!(g.exports().export_html("MERC", &opts, None).is_err());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 7);
    let opts = HtmlOptions { rev: Some(7), mark_in_progress: true, ..Default::default() };
    assert_eq!(g.exports().export_html("MERC", &opts, None).unwrap().marked.len(), 5);
}

// ---------------------------------------------------------------- notes & search

fn input(name: &str, date: &str, pid: Option<i64>, body: &str, status: NoteStatus) -> NoteInput {
    NoteInput { name: name.into(), note_date: date.into(), project_id: pid, body: body.into(), status }
}

#[test]
fn notes_do_not_create_revisions() {
    let g = env();
    let p = project(&g);
    let n = g.notes().create("Reunião", Some(p.id), "corpo", None).unwrap();
    g.notes().save(n.id, &input("Reunião 2", "2026-03-01", Some(p.id), "outro", NoteStatus::Processed)).unwrap();
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 0);
    let n2 = g.notes().get(n.id).unwrap().unwrap();
    assert_eq!(
        (n2.name.as_str(), n2.note_date.as_str(), n2.body.as_str(), n2.status),
        ("Reunião 2", "2026-03-01", "outro", NoteStatus::Processed)
    );
}

#[test]
fn note_defaults_and_dates() {
    let g = env();
    let n = g.notes().create("", None, "", None).unwrap();
    assert_eq!(n.note_date, "2026-03-10");
    assert!(n.name.starts_with("Anotação "));
    assert_eq!(n.status, NoteStatus::Pending);
    assert_eq!(n.project_id, None);
    assert_eq!(g.notes().create("x", None, "", Some("05/02/2026")).unwrap().note_date, "2026-02-05");
    assert!(g.notes().create("x", None, "", Some("ontem")).is_err());
}

#[test]
fn item_from_note_keeps_link() {
    let g = env();
    let p = project(&g);
    let n = g.notes().create("Reunião", Some(p.id), "pedido", None).unwrap();
    let item = g.notes().create_item(n.id, p.id, &make("Novo")).unwrap();
    assert_eq!(item.source_note_id, Some(n.id));
    assert_eq!(g.notes().linked_items(n.id).unwrap(), ["MERC-1"]);
    g.items().update(item.item_id, &make("Novo 2")).unwrap();
    assert_eq!(g.items().get(item.item_id, None).unwrap().unwrap().source_note_id, Some(n.id));
    g.notes().delete(n.id).unwrap();
    assert_eq!(g.items().get(item.item_id, None).unwrap().unwrap().source_note_id, None);
}

#[test]
fn list_notes_filters() {
    let g = env();
    let p = project(&g);
    let other = g.projects().create("P2K", "P2000", "").unwrap();
    let notes = g.notes();
    notes.create("a", Some(p.id), "", Some("2026-01-01")).unwrap();
    let b = notes.create("b", Some(p.id), "", Some("2026-02-01")).unwrap();
    notes.create("c", Some(other.id), "", Some("2026-02-02")).unwrap();
    notes.create("d", None, "", Some("2026-02-03")).unwrap();
    notes.set_status(b.id, NoteStatus::Archived).unwrap();
    let names = |f: NoteFilter| notes.list(&f).unwrap().into_iter().map(|n| n.name).collect::<Vec<_>>();
    let base = NoteFilter { project_id: Some(p.id), ..Default::default() };
    assert_eq!(names(NoteFilter { oldest_first: true, ..base.clone() }), ["a", "b"]);
    assert_eq!(names(NoteFilter { statuses: vec![NoteStatus::Pending], ..base.clone() }), ["a"]);
    assert_eq!(names(NoteFilter { since: Some("2026-01-15".into()), ..base.clone() }), ["b"]);
    assert_eq!(names(NoteFilter { include_unassigned: true, oldest_first: true, ..base }), ["a", "b", "d"]);
}

#[test]
fn search_items_only_current_versions() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("Relatório de vendas").body("exportar planilha").tags(&["financeiro"]), None).unwrap();
    g.items().update(a.item_id, &a.data.clone().body("exportar CSV")).unwrap();
    let s = |q: &str| g.search().search(q, None, 100).unwrap();
    assert!(s("planilha").is_empty());
    assert_eq!(s("csv").iter().map(|h| (h.kind.clone(), h.label.clone())).collect::<Vec<_>>(), [(HitKind::Item, "MERC-1".to_string())]);
    assert_eq!(s("relatorio")[0].label, "MERC-1"); // accents ignored
    assert_eq!(s("financ")[0].label, "MERC-1"); // prefix + tags
    g.items().delete(a.item_id).unwrap();
    assert!(s("csv").is_empty());
    for q in ["\"", "AND OR NOT", "a*b", "(", "título: x", ""] {
        s(q); // must not fail
    }
}

#[test]
fn search_notes() {
    let g = env();
    let p = project(&g);
    let n = g.notes().create("Reunião comercial", Some(p.id), "frete de telhas", None).unwrap();
    let s = |q: &str| g.search().search(q, None, 100).unwrap();
    assert_eq!(s("telha").iter().map(|h| h.id).collect::<Vec<_>>(), [n.id]);
    g.notes().save(n.id, &input(&n.name, &n.note_date, Some(p.id), "parcelamento", n.status)).unwrap();
    assert!(s("telha").is_empty());
    assert_eq!(s("parcel")[0].kind, HitKind::Note);
    g.notes().delete(n.id).unwrap();
    assert!(s("parcel").is_empty());
}

#[test]
fn projects_and_settings() {
    let g = env();
    let p = project(&g);
    assert!(g.projects().create("merc", "Duplicado", "").is_err());
    assert!(g.projects().create("1ABC", "Sigla inválida", "").is_err());
    let p = g.projects().update(p.id, "mrc", "Mercurius", "", true).unwrap();
    assert_eq!(p.code, "MRC");
    assert_eq!(g.items().create(p.id, &make("A"), None).unwrap().code(), "MRC-1");
    g.projects().set_active(p.id, false).unwrap();
    assert!(g.projects().list(false).unwrap().is_empty());
    assert_eq!(g.projects().list(true).unwrap().into_iter().map(|x| x.code).collect::<Vec<_>>(), ["MRC"]);

    let st = g.settings();
    assert_eq!(st.claude_prompt().unwrap(), DEFAULT_CLAUDE_PROMPT);
    st.set_claude_prompt(Some("Analise {sigla}.")).unwrap();
    assert_eq!(st.claude_prompt().unwrap(), "Analise {sigla}.");
    st.set_claude_prompt(Some("")).unwrap();
    assert_eq!(st.claude_prompt().unwrap(), DEFAULT_CLAUDE_PROMPT);
    let x = g.dir.path().join("x");
    st.set_export_dir(Some(&x.to_string_lossy())).unwrap();
    assert_eq!(st.export_dir().unwrap(), x);
}

#[test]
fn demo_and_copy_to_another_database() {
    let g = env();
    let codes = giano_core::services::demo::create_demo(&g).unwrap();
    assert_eq!(codes, ["LOJA", "RH"]);
    assert!(giano_core::services::demo::create_demo(&g).is_err());
    let loja = g.projects().require("LOJA").unwrap();
    let list = g.items().list_items(loja.id, None, Archived::No).unwrap();
    assert!(list.iter().any(|v| v.code() == "LOJA-2.1"));

    let dir = tempfile::tempdir().unwrap();
    let other = Giano::open(Some(&giano_core::config::DbTarget::local(dir.path().join("copy.db"))), fake_clock()).unwrap();
    let counts = g.copy_to(&other).unwrap();
    assert_eq!(counts["projects"], 2);
    let loja2 = other.projects().require("LOJA").unwrap();
    assert_eq!(other.items().list_items(loja2.id, None, Archived::Any).unwrap(), g.items().list_items(loja.id, None, Archived::Any).unwrap());
    assert_eq!(other.search().search("cubagem", None, 10).unwrap().len(), g.search().search("cubagem", None, 10).unwrap().len());
    assert!(g.copy_to(&other).is_err()); // the destination is not empty anymore
}
