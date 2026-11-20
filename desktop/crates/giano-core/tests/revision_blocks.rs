//! A revision with several changes: open revisions, batches, net change per item
//! (port of tests/test_revision_blocks.py).

mod common;

use common::*;
use giano_core::db::migrations::{V1, V2, V3};
use giano_core::db::{items as repo, Database};
use giano_core::domain::{ItemStatus, ItemView, Priority};
use giano_core::services::exports::HtmlOptions;
use giano_core::Giano;

fn versions(g: &Giano, item_id: i64) -> Vec<(i64, Option<i64>, String)> {
    g.items().history(item_id).unwrap().into_iter().map(|v| (v.rev_start, v.rev_end, v.data.title)).collect()
}

fn v(start: i64, end: Option<i64>, title: &str) -> (i64, Option<i64>, String) {
    (start, end, title.to_string())
}

fn lines(g: &Giano, pid: i64, number: i64) -> Vec<String> {
    g.items().revision(pid, number).unwrap().unwrap().changes.into_iter().map(|c| c.summary).collect()
}

fn codes(views: &[ItemView]) -> Vec<String> {
    views.iter().map(ItemView::code).collect()
}

fn keys(numbers: &[i64]) -> Option<std::collections::BTreeSet<(i64, i64)>> {
    Some(numbers.iter().map(|n| (*n, 0)).collect())
}

fn two(g: &Giano, pid: i64) -> (ItemView, ItemView) {
    let a = g.items().create(pid, &make("A"), None).unwrap(); // rev 1
    let b = g.items().create(pid, &make("B"), None).unwrap(); // rev 2
    (a, b)
}

#[test]
fn open_revision_collects_every_change_into_one() {
    let g = env();
    let p = project(&g);
    let (a, b) = two(&g, p.id);
    let items = g.items();
    let opened = items.open_revision(p.id, "Reunião com o cliente").unwrap();
    assert_eq!((opened.number, opened.is_open, opened.changes.len()), (3, true, 0));
    let c = items.create(p.id, &make("C"), None).unwrap();
    items.set_status(a.item_id, ItemStatus::Approved).unwrap();
    items.set_priority(a.item_id, Priority::High).unwrap(); // same item again: one net change
    items.update(c.item_id, &make("C2")).unwrap(); // created in this revision: still "criado"
    items.delete(b.item_id).unwrap();
    assert_eq!(items.latest_revision(p.id).unwrap(), 3);
    assert_eq!(
        lines(&g, p.id, 3),
        ["MERC-3 criado: C2", "MERC-1: prioridade Média → Alta; status Aberto → Aprovado", "MERC-2 excluído: B"]
    );
    assert_eq!(versions(&g, a.item_id), [v(1, Some(3), "A"), v(3, None, "A")]);
    assert_eq!(versions(&g, c.item_id), [v(3, None, "C2")]);
    use giano_core::services::items::Archived;
    assert_eq!(codes(&items.list_items(p.id, Some(2), Archived::Any).unwrap()), ["MERC-1", "MERC-2"]);
    assert_eq!(codes(&items.list_items(p.id, None, Archived::Any).unwrap()), ["MERC-1", "MERC-3"]);
    let d = items.diff(p.id, 2, 3, true).unwrap();
    assert_eq!((codes(&d.added), codes(&d.removed), d.changed.len()), (vec!["MERC-3".to_string()], vec!["MERC-2".to_string()], 1));

    let closed = items.close_revision(p.id, None).unwrap().unwrap();
    assert_eq!((closed.number, closed.is_open, closed.summary.as_str()), (3, false, "Reunião com o cliente"));
    assert!(closed.created_at > opened.created_at); // dated when closed
    assert!(items.get_open_revision(p.id).unwrap().is_none());
    let after = items.set_status(a.item_id, ItemStatus::Done).unwrap().unwrap();
    assert_eq!(after.rev_start, 4);
    assert_eq!(versions(&g, a.item_id), [v(1, Some(3), "A"), v(3, Some(4), "A"), v(4, None, "A")]);
}

#[test]
fn going_back_leaves_the_revision() {
    let g = env();
    let p = project(&g);
    let (a, _) = two(&g, p.id);
    let items = g.items();
    items.open_revision(p.id, "").unwrap();
    items.set_status(a.item_id, ItemStatus::Approved).unwrap();
    let back = items.set_status(a.item_id, ItemStatus::Open).unwrap().unwrap();
    assert_eq!((back.rev_start, back.rev_end), (1, None));
    assert_eq!(versions(&g, a.item_id), [v(1, None, "A")]);
    assert!(items.get_open_revision(p.id).unwrap().unwrap().changes.is_empty());
    items.archive(a.item_id).unwrap();
    items.unarchive(a.item_id).unwrap();
    assert!(items.get_open_revision(p.id).unwrap().unwrap().changes.is_empty());
    assert!(items.close_revision(p.id, None).unwrap().is_none()); // empty: discarded
    assert_eq!(items.latest_revision(p.id).unwrap(), 2);
    assert_eq!(items.create(p.id, &make("C"), None).unwrap().rev_start, 3);
}

#[test]
fn create_and_delete_in_the_same_revision() {
    let g = env();
    let p = project(&g);
    two(&g, p.id);
    let items = g.items();
    items.open_revision(p.id, "").unwrap();
    let c = items.create(p.id, &make("C"), None).unwrap();
    assert_eq!(items.delete(c.item_id).unwrap(), 3);
    assert!(items.get(c.item_id, None).unwrap().is_none());
    assert!(items.history(c.item_id).unwrap().is_empty());
    assert!(items.get_open_revision(p.id).unwrap().unwrap().changes.is_empty());
    assert_eq!(items.create(p.id, &make("D"), None).unwrap().code(), "MERC-4"); // never reused
}

#[test]
fn edit_then_delete_reports_the_old_title() {
    let g = env();
    let p = project(&g);
    let (a, _) = two(&g, p.id);
    let items = g.items();
    items.open_revision(p.id, "").unwrap();
    items.update(a.item_id, &make("A editado")).unwrap();
    items.delete(a.item_id).unwrap();
    let rev = items.close_revision(p.id, Some("Limpeza")).unwrap().unwrap();
    assert_eq!(rev.summary, "Limpeza");
    assert_eq!(lines(&g, p.id, 3), ["MERC-1 excluído: A"]);
    assert_eq!(versions(&g, a.item_id), [v(1, Some(3), "A")]);
}

#[test]
fn addendum_and_summary_defaults() {
    let g = env();
    let p = project(&g);
    let (a, b) = two(&g, p.id);
    let items = g.items();
    items.open_revision(p.id, "").unwrap();
    items.create_addendum(a.item_id, &make("Ajuste"), None).unwrap();
    let rev = items.close_revision(p.id, None).unwrap().unwrap();
    assert_eq!(rev.summary, "MERC-1.1 criado (adendo de MERC-1): Ajuste");
    items.open_revision(p.id, "   ").unwrap();
    items.set_status(a.item_id, ItemStatus::Done).unwrap();
    items.set_status(b.item_id, ItemStatus::Done).unwrap();
    assert_eq!(items.get_open_revision(p.id).unwrap().unwrap().title, "Em aberto (2 alterações)");
    assert_eq!(items.close_revision(p.id, None).unwrap().unwrap().summary, "2 itens alterados");
}

#[test]
fn open_revision_rules() {
    let g = env();
    let p = project(&g);
    two(&g, p.id);
    let items = g.items();
    assert!(err_msg(items.close_revision(p.id, None)).contains("Não há revisão aberta"));
    items.open_revision(p.id, "").unwrap();
    assert!(err_msg(items.open_revision(p.id, "")).contains("Rev. 3 já está aberta"));
    let other = g.projects().create("RH", "RH", "").unwrap();
    assert_eq!(items.open_revision(other.id, "").unwrap().number, 1); // one per project
    items.close_revision(other.id, None).unwrap();
    g.projects().set_active(other.id, false).unwrap();
    assert!(err_msg(items.open_revision(other.id, "")).contains("Projeto arquivado"));
}

#[test]
fn batch_goes_into_the_open_revision() {
    let g = env();
    let p = project(&g);
    let (a, b) = two(&g, p.id);
    let items = g.items();
    items.open_revision(p.id, "").unwrap();
    items.update(a.item_id, &make("A2")).unwrap();
    items.set_status_many(&[a.item_id, b.item_id], ItemStatus::Approved, None).unwrap();
    let rev = items.get_open_revision(p.id).unwrap().unwrap();
    assert_eq!((rev.number, rev.summary.as_str()), (3, "")); // the batch does not close it
    assert_eq!(
        lines(&g, p.id, 3),
        ["MERC-1: título \"A\" → \"A2\"; status Aberto → Aprovado", "MERC-2: status Aberto → Aprovado"]
    );
}

#[test]
fn batch_is_all_or_nothing() {
    let g = env();
    let p = project(&g);
    let (a, b) = two(&g, p.id);
    let items = g.items();
    let failed = items.batch("x", || {
        items.update(a.item_id, &make("A2"))?;
        items.delete(999)
    });
    assert!(failed.is_err());
    assert_eq!(items.latest_revision(p.id).unwrap(), 2);
    assert_eq!(versions(&g, a.item_id), [v(1, None, "A")]);
    items
        .batch("Dois itens", || {
            items.update(a.item_id, &make("A2"))?;
            items.update(b.item_id, &make("B2"))?;
            items.update(a.item_id, &make("A")) // back: leaves the batch
        })
        .unwrap();
    let rev = items.revision(p.id, 3).unwrap().unwrap();
    assert_eq!((rev.summary.as_str(), rev.changes.len()), ("MERC-2: título \"B\" → \"B2\"", 1));
    items
        .batch("nada", || {
            items.update(a.item_id, &make("A3"))?;
            items.update(a.item_id, &make("A"))
        })
        .unwrap();
    assert_eq!(items.latest_revision(p.id).unwrap(), 3); // empty batch: no revision
}

#[test]
fn html_export_lists_the_changes_of_a_block() {
    let g = env();
    let p = project(&g);
    let (a, b) = two(&g, p.id);
    let items = g.items();
    items.open_revision(p.id, "Reunião").unwrap();
    items.update(a.item_id, &make("A2")).unwrap();
    items.update(b.item_id, &make("B2")).unwrap();
    items.close_revision(p.id, None).unwrap();
    let doc = g.exports().html(&p, &HtmlOptions { changes: true, ..Default::default() }).unwrap();
    assert_eq!(doc.rev, 3);
    assert!(doc.html.contains("<td>Reunião<ul><li>MERC-1: título"), "{}", doc.html);
    assert!(doc.html.contains("<li>MERC-2: título"));
    let only_one = HtmlOptions { changes: true, keys: keys(&[1]), ..Default::default() };
    let doc = g.exports().html(&p, &only_one).unwrap();
    assert!(!doc.html.contains("Reunião") && !doc.html.contains("MERC-2"));
    assert!(doc.html.contains("<td>MERC-1: título") && !doc.html.contains("<ul><li>MERC-1"));
}

#[test]
fn v3_database_gets_one_change_line_per_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v3.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "{V1}{V2}{V3}
            CREATE TABLE schema_version (version INTEGER NOT NULL);
            INSERT INTO schema_version VALUES (1), (2), (3);
            INSERT INTO projects (id, code, name, created_at) VALUES (1, 'MERC', 'M', '2026-01-01');
            INSERT INTO items (id, project_id, number, created_at) VALUES (7, 1, 1, '2026-01-01');
            INSERT INTO revisions (project_id, number, created_at, summary, item_id)
                VALUES (1, 1, '2026-01-01', 'MERC-1 criado: A', 7);
            INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
                VALUES (7, 1, 'A', 'improvement', 'low', 1, 'open');"
        ))
        .unwrap();
    }
    let db = Database::open(&path.to_string_lossy(), None).unwrap();
    let rev = repo::get_revision(&db, 1, 1).unwrap().unwrap();
    assert!(!rev.is_open);
    assert_eq!(rev.changes.iter().map(|c| (c.item_id, c.summary.as_str())).collect::<Vec<_>>(), [(7, "MERC-1 criado: A")]);
}
