//! Revision rules: append-only versions, reconstruction, no-op edits, deletes, diffs.

mod common;

use common::*;
use giano_core::db::migrations::MIGRATIONS;
use giano_core::db::Database;
use giano_core::domain::{ItemData, ItemStatus, ItemType, ItemView, Priority};
use giano_core::params;
use giano_core::services::formatting::diff_markdown;
use giano_core::services::items::Archived;
use giano_core::Error;

fn titles(views: &[ItemView]) -> Vec<String> {
    views.iter().map(|v| v.data.title.clone()).collect()
}

fn codes(views: &[ItemView]) -> Vec<String> {
    views.iter().map(ItemView::code).collect()
}

#[test]
fn new_project_starts_at_revision_zero() {
    let g = env();
    let p = project(&g);
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 0);
    assert!(g.items().list_items(p.id, None, Archived::Any).unwrap().is_empty());
    assert!(g.items().revisions(p.id).unwrap().is_empty());
}

#[test]
fn create_assigns_sequential_ids_and_revisions() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("Primeiro"), None).unwrap();
    let b = g.items().create(p.id, &make("Segundo"), None).unwrap();
    assert_eq!((a.code(), b.code()), ("MERC-1".into(), "MERC-2".into()));
    assert_eq!((a.rev_start, b.rev_start), (1, 2));
    assert_eq!(a.rev_end, None);
    let revs = g.items().revisions(p.id).unwrap();
    assert_eq!(revs.iter().map(|r| r.number).collect::<Vec<_>>(), vec![2, 1]);
    assert_eq!(revs[1].summary, "MERC-1 criado: Primeiro");
    assert_eq!(revs[1].created_at, "2026-03-10T09:02:00");
}

#[test]
fn numbering_is_per_project() {
    let g = env();
    let p = project(&g);
    let other = g.projects().create("P2K", "P2000", "").unwrap();
    g.items().create(p.id, &make("A"), None).unwrap();
    g.items().create(p.id, &make("B"), None).unwrap();
    let x = g.items().create(other.id, &make("X"), None).unwrap();
    assert_eq!(x.code(), "P2K-1");
    assert_eq!(g.items().latest_revision(other.id).unwrap(), 1);
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 2);
}

#[test]
fn update_closes_version_and_inserts_new() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("Item").priority(Priority::High), None).unwrap();
    let new = g.items().update(a.item_id, &a.data.clone().priority(Priority::Medium)).unwrap().unwrap();
    assert_eq!((new.rev_start, new.rev_end), (2, None));
    let history = g.items().history(a.item_id).unwrap();
    assert_eq!(history.iter().map(|h| (h.rev_start, h.rev_end)).collect::<Vec<_>>(), vec![(1, Some(2)), (2, None)]);
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "MERC-1: prioridade Alta → Média");
}

#[test]
fn save_without_change_creates_no_revision() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("Item").tags(&["b", "a"]).body("texto"), None).unwrap();
    // Same content, different formatting: tag order/case, trailing whitespace, CRLF.
    let same = ItemData { title: "  Item ".into(), ..a.data.clone() }.tags(&["A", "b"]).body("texto  \r\n\n");
    assert!(g.items().update(a.item_id, &same).unwrap().is_none());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 1);
    assert_eq!(g.items().history(a.item_id).unwrap().len(), 1);
}

#[test]
fn list_reconstruction_at_any_revision() {
    let g = env();
    let p = project(&g);
    let items = g.items();
    let a = items.create(p.id, &make("A"), None).unwrap(); // 1
    let b = items.create(p.id, &make("B"), None).unwrap(); // 2
    items.update(a.item_id, &ItemData { title: "A2".into(), ..a.data.clone() }).unwrap(); // 3
    items.delete(b.item_id).unwrap(); // 4
    let c = items.create(p.id, &make("C"), None).unwrap(); // 5
    items.set_status(c.item_id, ItemStatus::Done).unwrap(); // 6

    let expected: [(i64, &[&str]); 7] =
        [(0, &[]), (1, &["A"]), (2, &["A", "B"]), (3, &["A2", "B"]), (4, &["A2"]), (5, &["A2", "C"]), (6, &["A2", "C"])];
    for (rev, names) in expected {
        assert_eq!(titles(&items.list_items(p.id, Some(rev), Archived::Any).unwrap()), names, "rev {rev}");
    }
    assert_eq!(items.list_items(p.id, Some(5), Archived::Any).unwrap()[1].data.status, ItemStatus::Open);
    assert_eq!(items.list_items(p.id, Some(6), Archived::Any).unwrap()[1].data.status, ItemStatus::Done);
    assert_eq!(titles(&items.list_items(p.id, None, Archived::Any).unwrap()), ["A2", "C"]);
}

#[test]
fn delete_only_closes_current_version() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    assert_eq!(g.items().delete(a.item_id).unwrap(), 2);
    let history = g.items().history(a.item_id).unwrap();
    assert_eq!(history.iter().map(|h| (h.rev_start, h.rev_end)).collect::<Vec<_>>(), vec![(1, Some(2))]);
    assert!(g.items().get(a.item_id, None).unwrap().is_none());
    assert_eq!(g.items().get(a.item_id, Some(1)).unwrap().unwrap().data.title, "A");
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "MERC-1 excluído: A");
    assert!(matches!(g.items().delete(a.item_id), Err(Error::Validation(_))));
    assert!(matches!(g.items().update(a.item_id, &make("ressuscitar")), Err(Error::Validation(_))));
}

#[test]
fn deleted_number_is_never_reused() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    g.items().delete(a.item_id).unwrap();
    assert_eq!(g.items().create(p.id, &make("B"), None).unwrap().code(), "MERC-2");
}

#[test]
fn tags_belong_to_versions() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A").tags(&["ux"]), None).unwrap();
    g.items().set_tags(a.item_id, &["ux", "api"]).unwrap();
    g.items().set_tags(a.item_id, &["api"]).unwrap();
    let at = |rev| g.items().list_items(p.id, Some(rev), Archived::Any).unwrap()[0].data.tags.clone();
    assert_eq!(at(1), ["ux"]);
    assert_eq!(at(2), ["api", "ux"]);
    assert_eq!(at(3), ["api"]);
    let summaries: Vec<String> = g.items().revisions(p.id).unwrap().into_iter().map(|r| r.summary).collect();
    assert_eq!(summaries[1], "MERC-1: tags +api");
    assert_eq!(summaries[0], "MERC-1: tags −ux");
    assert_eq!(g.items().all_tags().unwrap(), ["api"]);
}

#[test]
fn multi_field_summary() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    let data = a.data.clone().status(ItemStatus::Approved).difficulty(5).body("novo").kind(ItemType::Adjustment);
    g.items().update(a.item_id, &data).unwrap();
    assert_eq!(
        g.items().revisions(p.id).unwrap()[0].summary,
        "MERC-1: tipo Melhoria → Ajuste; dificuldade 3/5 → 5/5; status Aberto → Aprovado; corpo alterado"
    );
}

#[test]
fn transaction_is_atomic() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    let result: giano_core::Result<()> = g.db().transaction(|| {
        g.db().execute(
            "INSERT INTO revisions (project_id, number, created_at, summary) VALUES (?, 2, 'x', 'x')",
            params![p.id],
        )?;
        Err(Error::validation("falha simulada"))
    });
    assert!(result.is_err());
    // Nothing half-written: no new revision, the version is still current.
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 1);
    assert_eq!(g.items().get(a.item_id, None).unwrap().unwrap().rev_end, None);
    // Nested transactions join the outer one.
    let result: giano_core::Result<()> = g.db().transaction(|| {
        g.items().set_status(a.item_id, ItemStatus::Done)?;
        Err(Error::validation("falha depois"))
    });
    assert!(result.is_err());
    assert_eq!(g.items().get(a.item_id, None).unwrap().unwrap().data.status, ItemStatus::Open);
}

#[test]
fn validation() {
    let g = env();
    let p = project(&g);
    assert!(err_msg(g.items().create(p.id, &make("   "), None)).contains("título"));
    assert!(err_msg(g.items().create(p.id, &make("x").difficulty(6), None)).contains("Dificuldade"));
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 0);
}

#[test]
fn archived_project_rejects_new_items() {
    let g = env();
    let p = project(&g);
    g.projects().set_active(p.id, false).unwrap();
    assert!(err_msg(g.items().create(p.id, &make("A"), None)).contains("Projeto arquivado"));
}

#[test]
fn get_by_code() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    assert_eq!(g.items().get_by_code("merc-1").unwrap().unwrap().item_id, a.item_id);
    assert!(g.items().get_by_code("MERC-99").unwrap().is_none());
    assert!(g.items().get_by_code("bogus").unwrap().is_none());
}

#[test]
fn migrations_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("m.db");
    drop(Database::open_local(&path).unwrap());
    let db = Database::open_local(&path).unwrap();
    let count: i64 = db.query_one("SELECT COUNT(*) FROM schema_version", params![]).unwrap().unwrap().at(0);
    assert_eq!(count, MIGRATIONS.len() as i64);
}

#[test]
fn set_status_many_one_revision_for_the_block() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    let b = g.items().create(p.id, &make("B").status(ItemStatus::InProgress), None).unwrap();
    let c = g.items().create(p.id, &make("C"), None).unwrap();
    let mut seen = Vec::new();
    let mut progress = |d: usize, t: usize| seen.push((d, t));
    let changed = g
        .items()
        .set_status_many(&[a.item_id, b.item_id, c.item_id, a.item_id], ItemStatus::InProgress, Some(&mut progress))
        .unwrap();
    assert_eq!(codes(&changed), ["MERC-1", "MERC-3"]); // B already was; A once
    assert_eq!(changed.iter().map(|v| v.rev_start).collect::<Vec<_>>(), [4, 4]);
    assert_eq!(seen, [(0, 2), (1, 2), (2, 2)]);
    let rev = g.items().revisions(p.id).unwrap().remove(0);
    assert_eq!((rev.number, rev.summary.as_str()), (4, "2 itens → Em desenvolvimento"));
    let lines: Vec<&str> = rev.changes.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(lines, ["MERC-1: status Aberto → Em desenvolvimento", "MERC-3: status Aberto → Em desenvolvimento"]);
    // Nothing to change: no revision.
    assert!(g.items().set_status_many(&[a.item_id, b.item_id], ItemStatus::InProgress, None).unwrap().is_empty());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 4);
    // A single item changing keeps its own description.
    g.items().set_status_many(&[a.item_id, c.item_id], ItemStatus::Done, None).unwrap();
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "2 itens → Concluído");
    g.items().set_status_many(&[a.item_id, b.item_id], ItemStatus::Done, None).unwrap();
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "MERC-2: status Em desenvolvimento → Concluído");
}

#[test]
fn set_status_many_is_all_or_nothing() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    let b = g.items().create(p.id, &make("B"), None).unwrap();
    g.items().archive(b.item_id).unwrap(); // rev 3
    assert!(err_msg(g.items().set_status_many(&[a.item_id, b.item_id], ItemStatus::Done, None))
        .contains("MERC-2 está arquivado"));
    assert!(g.items().set_status_many(&[a.item_id, 999], ItemStatus::Done, None).is_err());
    assert_eq!(g.items().get(a.item_id, None).unwrap().unwrap().data.status, ItemStatus::Open);
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 3);
}

// ---------------------------------------------------------------- diff

fn history(g: &giano_core::Giano, pid: i64) {
    let items = g.items();
    let a = items.create(pid, &make("A").priority(Priority::High), None).unwrap(); // 1
    let b = items.create(pid, &make("B"), None).unwrap(); // 2
    let c = items.create(pid, &make("C"), None).unwrap(); // 3
    items.set_priority(a.item_id, Priority::Low).unwrap(); // 4
    items.delete(b.item_id).unwrap(); // 5
    items.create(pid, &make("D"), None).unwrap(); // 6
    items.update(c.item_id, &ItemData { title: "C2".into(), ..c.data.clone() }.tags(&["x"])).unwrap(); // 7
}

#[test]
fn diff_added_removed_changed() {
    let g = env();
    let p = project(&g);
    history(&g, p.id);
    let diff = g.items().diff(p.id, 3, 7, true).unwrap();
    assert_eq!(codes(&diff.added), ["MERC-4"]);
    assert_eq!(codes(&diff.removed), ["MERC-2"]);
    assert_eq!(diff.changed.iter().map(|c| c.code()).collect::<Vec<_>>(), ["MERC-1", "MERC-3"]);
    let fields: Vec<Vec<&str>> = diff.changed.iter().map(|c| c.changes.iter().map(|f| f.label()).collect()).collect();
    assert_eq!(fields, vec![vec!["prioridade"], vec!["título", "tags"]]);

    let md = diff_markdown(&diff);
    assert!(md.contains("## Adicionados (1)"));
    assert!(md.contains("~~**MERC-2** B~~"));
    assert!(md.contains("prioridade: Alta → **Baixa**"));
    assert!(md.contains("tags: — → **x**"));
}

#[test]
fn diff_from_zero_and_same_revision() {
    let g = env();
    let p = project(&g);
    history(&g, p.id);
    let diff = g.items().diff(p.id, 0, 7, true).unwrap();
    assert_eq!(codes(&diff.added), ["MERC-1", "MERC-3", "MERC-4"]);
    assert!(diff.removed.is_empty() && diff.changed.is_empty());
    assert!(g.items().diff(p.id, 5, 5, true).unwrap().is_empty());
    assert!(g.items().diff(p.id, 0, 99, true).is_err());
}

#[test]
fn change_and_revert_is_not_a_difference() {
    let g = env();
    let p = project(&g);
    let a = g.items().create(p.id, &make("A"), None).unwrap();
    g.items().set_status(a.item_id, ItemStatus::Approved).unwrap();
    g.items().set_status(a.item_id, ItemStatus::Open).unwrap();
    assert!(g.items().diff(p.id, 1, 3, true).unwrap().is_empty());
    assert!(!g.items().diff(p.id, 1, 2, true).unwrap().is_empty());
}
