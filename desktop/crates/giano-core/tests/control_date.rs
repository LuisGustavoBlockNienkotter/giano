//! Creation date and control date of items (port of tests/test_control_date.py).

mod common;

use std::collections::BTreeSet;

use common::*;
use giano_core::domain::{ItemStatus, ItemView};
use giano_core::services::exports::HtmlOptions;
use giano_core::Giano;

fn keys(numbers: &[i64]) -> Option<BTreeSet<(i64, i64)>> {
    Some(numbers.iter().map(|n| (*n, 0)).collect())
}

fn three(g: &Giano, pid: i64) -> Vec<ItemView> {
    vec![
        g.items().create(pid, &make("A"), None).unwrap(),
        g.items().create(pid, &make("B"), None).unwrap(),
        g.items().create(pid, &make("C").status(ItemStatus::Done), None).unwrap(),
    ]
}

fn control(g: &Giano, item_id: i64, rev: Option<i64>) -> Option<String> {
    g.items().get(item_id, rev).unwrap().unwrap().control_date
}

#[test]
fn created_at_is_the_item_creation() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    g.items().set_status(a.item_id, ItemStatus::Approved).unwrap();
    let addendum = g.items().create_addendum(a.item_id, &make("A.1"), None).unwrap();
    let current = g.items().get(a.item_id, None).unwrap().unwrap();
    assert_eq!(current.created_at, a.created_at);
    assert!(a.created_at.starts_with("2026-03-10T09:"));
    assert!(addendum.created_at > a.created_at);
    assert_eq!(current.control_date, None);
    let json = serde_json::to_value(&current).unwrap();
    assert_eq!(json["created_at"], a.created_at.as_str());
    assert!(json["control_date"].is_null());
}

#[test]
fn set_control_date_is_not_a_revision() {
    let g = env();
    let p = project(&g);
    let vs = three(&g, p.id);
    let (a, b) = (vs[0].item_id, vs[1].item_id);
    let views = g.items().set_control_date(&[a, b], Some("2026-04-01")).unwrap();
    assert_eq!(views[1].control_date.as_deref(), Some("2026-04-01"));
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 3);
    // The same at every revision (not versioned).
    assert_eq!(control(&g, a, None).as_deref(), Some("2026-04-01"));
    assert_eq!(control(&g, a, Some(1)).as_deref(), Some("2026-04-01"));
    g.items().set_control_date(&[a], None).unwrap();
    assert_eq!(control(&g, a, None), None);
    // Changing the content keeps the control date.
    g.items().set_status(b, ItemStatus::Approved).unwrap();
    assert_eq!(control(&g, b, None).as_deref(), Some("2026-04-01"));
}

#[test]
fn set_control_date_validation() {
    let g = env();
    let p = project(&g);
    let a = three(&g, p.id)[0].item_id;
    assert!(err_msg(g.items().set_control_date(&[a], Some("2026-02-31"))).contains("inválida"));
    g.projects().set_active(p.id, false).unwrap();
    assert!(err_msg(g.items().set_control_date(&[a], Some("2026-04-01"))).contains("somente leitura"));
}

#[test]
fn html_export_sets_today_on_the_exported_items() {
    let g = env();
    let p = project(&g);
    let vs = three(&g, p.id);
    let archived = g.items().create(p.id, &make("D"), None).unwrap();
    g.items().archive(archived.item_id).unwrap();
    let rev = g.items().latest_revision(p.id).unwrap();
    let opts = HtmlOptions { keys: keys(&[1, 3]), control_date: true, ..Default::default() };
    let r = g.exports().export_html("MERC", &opts, None).unwrap();
    assert_eq!(r.control_dated, 2);
    assert!(r.summary.contains("data de controle 10/03/2026 em 2 itens"), "{}", r.summary);
    let dates: Vec<Option<String>> = vs.iter().map(|v| control(&g, v.item_id, None)).collect();
    let today = Some("2026-03-10".to_string());
    assert_eq!(dates, [today.clone(), None, today]);
    assert_eq!(control(&g, archived.item_id, None), None);
    assert_eq!(g.items().latest_revision(p.id).unwrap(), rev); // no revision
}

#[test]
fn html_export_control_date_off_by_default_and_old_revisions() {
    let g = env();
    let p = project(&g);
    let vs = three(&g, p.id);
    let r = g.exports().export_html("MERC", &HtmlOptions::default(), None).unwrap();
    assert_eq!(r.control_dated, 0);
    assert!(!r.summary.contains("data de controle"));
    assert_eq!(control(&g, vs[0].item_id, None), None);
    // An old revision may be exported (and dated): only its items get the date.
    let opts = HtmlOptions { rev: Some(2), control_date: true, ..Default::default() };
    g.exports().export_html("MERC", &opts, None).unwrap();
    let dates: Vec<Option<String>> = vs.iter().map(|v| control(&g, v.item_id, None)).collect();
    let today = Some("2026-03-10".to_string());
    assert_eq!(dates, [today.clone(), today, None]);
}

#[test]
fn html_export_control_date_with_mark_in_progress_and_rollback() {
    let g = env();
    let p = project(&g);
    let vs = three(&g, p.id);
    // The file cannot be written: no date either.
    let blocker = g.dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let opts = HtmlOptions { out_dir: Some(blocker), control_date: true, ..Default::default() };
    assert!(g.exports().export_html("MERC", &opts, None).is_err());
    assert_eq!(control(&g, vs[0].item_id, None), None);

    let opts = HtmlOptions { mark_in_progress: true, control_date: true, ..Default::default() };
    let r = g.exports().export_html("MERC", &opts, None).unwrap();
    assert_eq!(r.marked, ["MERC-1", "MERC-2"]);
    assert_eq!(r.control_dated, 3);
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 4); // one revision, for the statuses
    assert_eq!(control(&g, vs[0].item_id, None).as_deref(), Some("2026-03-10"));
}

#[test]
fn html_export_skips_control_date_for_archived_project() {
    let g = env();
    let p = project(&g);
    let vs = three(&g, p.id);
    g.projects().set_active(p.id, false).unwrap();
    let opts = HtmlOptions { control_date: true, ..Default::default() };
    assert_eq!(g.exports().export_html("MERC", &opts, None).unwrap().control_dated, 0);
    assert_eq!(control(&g, vs[0].item_id, None), None);
}
