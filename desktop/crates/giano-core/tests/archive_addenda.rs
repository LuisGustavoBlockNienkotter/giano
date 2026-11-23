//! Archiving (versioned, read-only while archived) and addenda (MERC-1.1).

mod common;

use std::collections::BTreeSet;

use common::*;
use giano_core::db::migrations::{V1, V2};
use giano_core::db::Database;
use giano_core::domain::{ItemData, ItemStatus, ItemType, ItemView, Priority};
use giano_core::params;
use giano_core::services::exports::HtmlOptions;
use giano_core::services::items::Archived;
use giano_core::services::search::HitKind;
use giano_core::Error;

fn codes(views: &[ItemView]) -> Vec<String> {
    views.iter().map(ItemView::code).collect()
}

fn three(g: &giano_core::Giano, pid: i64) -> (ItemView, ItemView, ItemView) {
    let a = g.items().create(pid, &make("Ativo").tags(&["ux"]).body("corpo A"), None).unwrap();
    let b = g
        .items()
        .create(
            pid,
            &make("Feito há tempos")
                .priority(Priority::High)
                .difficulty(4)
                .tags(&["velho", "erp"])
                .status(ItemStatus::Done)
                .body("## Detalhe\n\ntexto antigo"),
            None,
        )
        .unwrap();
    let c = g.items().create(pid, &make("Outro"), None).unwrap();
    (a, b, c)
}

// ---------------------------------------------------------------- archive

#[test]
fn archive_creates_revision_and_keeps_content() {
    let g = env();
    let p = project(&g);
    let (_, b, _) = three(&g, p.id);
    let before = g.items().latest_revision(p.id).unwrap();
    let archived = g.items().archive(b.item_id).unwrap();
    assert_eq!(archived.rev_start, before + 1);
    assert_eq!(archived.data, ItemData { archived: true, ..b.data.clone() });
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "MERC-2: arquivado");
    assert_eq!(codes(&g.items().list_items(p.id, None, Archived::No).unwrap()), ["MERC-1", "MERC-3"]);
    assert_eq!(codes(&g.items().list_items(p.id, None, Archived::Only).unwrap()), ["MERC-2"]);
    assert_eq!(g.items().list_items(p.id, None, Archived::Any).unwrap().len(), 3);
    assert!(codes(&g.items().list_items(p.id, Some(before), Archived::No).unwrap()).contains(&"MERC-2".into()));
}

#[test]
fn unarchive_restores_and_double_operations_are_refused() {
    let g = env();
    let p = project(&g);
    let (a, b, _) = three(&g, p.id);
    g.items().archive(b.item_id).unwrap();
    let rev = g.items().latest_revision(p.id).unwrap();
    assert!(err_msg(g.items().archive(b.item_id)).contains("já está arquivado"));
    assert!(err_msg(g.items().unarchive(a.item_id)).contains("não está arquivado"));
    assert_eq!(g.items().latest_revision(p.id).unwrap(), rev);
    let restored = g.items().unarchive(b.item_id).unwrap();
    assert_eq!(restored.data, b.data);
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "MERC-2: restaurado do arquivo");
}

#[test]
fn archive_many_one_revision_and_restore_many() {
    let g = env();
    let p = project(&g);
    let (a, b, c) = three(&g, p.id);
    let before = g.items().latest_revision(p.id).unwrap();
    let mut seen = Vec::new();
    let mut progress = |d: usize, t: usize| seen.push((d, t));
    let changed = g
        .items()
        .set_archived_many(&[a.item_id, c.item_id, a.item_id], true, Some(&mut progress))
        .unwrap();
    assert_eq!(codes(&changed), ["MERC-1", "MERC-3"]);
    assert_eq!(changed.iter().map(|v| v.rev_start).collect::<Vec<_>>(), [before + 1, before + 1]);
    assert_eq!(seen, [(0, 2), (1, 2), (2, 2)]);
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "2 itens arquivados");
    assert_eq!(codes(&g.items().list_items(p.id, None, Archived::No).unwrap()), ["MERC-2"]);
    assert_eq!(codes(&g.items().list_items(p.id, None, Archived::Only).unwrap()), ["MERC-1", "MERC-3"]);
    // Already archived items are skipped; nothing to change: no revision.
    assert!(g.items().set_archived_many(&[a.item_id, c.item_id], true, None).unwrap().is_empty());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), before + 1);
    // Mixed: only the one that changes; a single change keeps its own description.
    let changed = g.items().set_archived_many(&[a.item_id, b.item_id], true, None).unwrap();
    assert_eq!(codes(&changed), ["MERC-2"]);
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "MERC-2: arquivado");
    let restored = g.items().set_archived_many(&[a.item_id, b.item_id, c.item_id], false, None).unwrap();
    assert_eq!(codes(&restored), ["MERC-1", "MERC-2", "MERC-3"]);
    assert_eq!(g.items().revisions(p.id).unwrap()[0].summary, "3 itens restaurados do arquivo");
    assert_eq!(restored[1].data, b.data);
    assert!(g.items().list_items(p.id, None, Archived::Only).unwrap().is_empty());
}

#[test]
fn archive_many_is_all_or_nothing() {
    let g = env();
    let p = project(&g);
    let (a, b, _) = three(&g, p.id);
    let rev = g.items().latest_revision(p.id).unwrap();
    assert!(g.items().set_archived_many(&[a.item_id, 999], true, None).is_err());
    assert!(!g.items().get(a.item_id, None).unwrap().unwrap().data.archived);
    assert_eq!(g.items().latest_revision(p.id).unwrap(), rev);
    g.projects().set_active(p.id, false).unwrap();
    assert!(err_msg(g.items().set_archived_many(&[a.item_id, b.item_id], true, None)).contains("Projeto arquivado"));
    assert_eq!(g.items().latest_revision(p.id).unwrap(), rev);
}

#[test]
fn archive_many_goes_into_the_open_revision() {
    let g = env();
    let p = project(&g);
    let (a, b, _) = three(&g, p.id);
    let open = g.items().open_revision(p.id, "Limpeza").unwrap();
    g.items().set_archived_many(&[a.item_id, b.item_id], true, None).unwrap();
    assert_eq!(g.items().latest_revision(p.id).unwrap(), open.number);
    // Restoring in the same open revision cancels the change: the items leave the revision.
    g.items().set_archived_many(&[a.item_id], false, None).unwrap();
    let rev = g.items().revisions(p.id).unwrap().remove(0);
    let lines: Vec<&str> = rev.changes.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(lines, ["MERC-2: arquivado"]);
}

#[test]
fn archived_item_is_read_only_and_edit_never_archives() {
    let g = env();
    let p = project(&g);
    let (a, b, _) = three(&g, p.id);
    g.items().archive(b.item_id).unwrap();
    let rev = g.items().latest_revision(p.id).unwrap();
    assert!(err_msg(g.items().update(b.item_id, &ItemData { title: "Mudou".into(), ..b.data.clone() })).contains("arquivado"));
    assert!(err_msg(g.items().set_status(b.item_id, ItemStatus::Open)).contains("arquivado"));
    assert_eq!(g.items().latest_revision(p.id).unwrap(), rev);
    assert!(g.items().update(a.item_id, &ItemData { archived: true, ..a.data.clone() }).unwrap().is_none());
    assert!(!g.items().get(a.item_id, None).unwrap().unwrap().data.archived);
    g.items().delete(b.item_id).unwrap(); // archived items can be deleted
    assert!(g.items().list_items(p.id, None, Archived::Only).unwrap().is_empty());
    g.projects().set_active(p.id, false).unwrap();
    assert!(err_msg(g.items().archive(a.item_id)).contains("Projeto arquivado"));
}

#[test]
fn diff_shows_archiving() {
    let g = env();
    let p = project(&g);
    let (_, b, _) = three(&g, p.id);
    let base = g.items().latest_revision(p.id).unwrap();
    g.items().archive(b.item_id).unwrap();
    let rev = base + 1;
    let d = g.items().diff(p.id, base, rev, true).unwrap();
    assert_eq!(d.changed.iter().map(|c| c.code()).collect::<Vec<_>>(), ["MERC-2"]);
    assert_eq!(d.changed[0].changes[0].describe(), "arquivado");
    let d = g.items().diff(p.id, base, rev, false).unwrap();
    assert_eq!(codes(&d.removed), ["MERC-2"]);
    assert!(d.changed.is_empty());
    g.items().unarchive(b.item_id).unwrap();
    assert_eq!(codes(&g.items().diff(p.id, rev, rev + 1, false).unwrap().added), ["MERC-2"]);
}

#[test]
fn search_marks_archived_hits() {
    let g = env();
    let p = project(&g);
    let (_, b, _) = three(&g, p.id);
    g.items().archive(b.item_id).unwrap();
    let hits = g.search().search("antigo", None, 100).unwrap();
    assert_eq!(
        hits.iter().map(|h| (h.label.as_str(), h.status.as_str())).collect::<Vec<_>>(),
        [("MERC-2", "Concluído (arquivado)")]
    );
}

#[test]
fn exports_leave_archived_items_out() {
    let g = env();
    let p = project(&g);
    let (_, b, _) = three(&g, p.id);
    g.items().archive(b.item_id).unwrap();
    let html = g.exports().html(&p, &HtmlOptions::default()).unwrap().html;
    assert!(!html.contains("Feito há tempos") && !html.contains("MERC-2"));
    assert!(html.contains("Ativo") && html.contains("Outro"));
    let keys: BTreeSet<_> = [(1, 0), (2, 0)].into();
    let opts = HtmlOptions { keys: Some(keys), ..Default::default() };
    let msg = err_msg(g.exports().html(&p, &opts).map(|d| d.rev));
    assert!(msg.contains("arquivados") && msg.contains("MERC-2"), "{msg}");

    let (md, _) = g.exports().claude_markdown(&p, &Default::default()).unwrap();
    assert!(!md.contains("Feito há tempos"));
    assert!(md.contains("2 itens"));

    g.items().unarchive(b.item_id).unwrap();
    let html = g.exports().html(&p, &HtmlOptions::default()).unwrap().html;
    assert!(html.contains("Feito há tempos") && html.contains("Adicionados (1)"));
}

#[test]
fn existing_v1_database_gets_archived_column_and_addenda() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "{V1}{V2}
             CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version VALUES (1), (2);
             INSERT INTO projects (id, code, name, created_at) VALUES (1, 'MERC', 'M', '2026-01-01');
             INSERT INTO notes (id, note_date, name, created_at, updated_at)
                 VALUES (1, '2026-01-01', 'n', '2026-01-01', '2026-01-01');
             INSERT INTO items (id, project_id, number, source_note_id, created_at)
                 VALUES (7, 1, 3, 1, '2026-01-01');
             INSERT INTO revisions (project_id, number, created_at, summary, item_id)
                 VALUES (1, 1, '2026-01-01', 'x', 7);
             INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
                 VALUES (7, 1, 'Antigo', 'improvement', 'low', 1, 'done');"
        ))
        .unwrap();
    }
    let db = Database::open_local(&path).unwrap();
    let row = db.query_one("SELECT id, number, addendum, source_note_id FROM items", params![]).unwrap().unwrap();
    assert_eq!(
        (row.at::<i64>(0), row.at::<i64>(1), row.at::<i64>(2), row.at::<i64>(3)),
        (7, 3, 0, 1)
    );
    let fk: i64 = db.query_one("PRAGMA foreign_keys", params![]).unwrap().unwrap().at(0);
    assert_eq!(fk, 1);
    assert!(db.query("PRAGMA foreign_key_check", params![]).unwrap().is_empty());
    let archived: i64 = db.query_one("SELECT archived FROM item_versions", params![]).unwrap().unwrap().at(0);
    assert_eq!(archived, 0);
    // Foreign keys still point at the rebuilt table; (project, number, addendum) is unique.
    assert!(matches!(
        db.execute(
            "INSERT INTO item_versions (item_id, rev_start, title, type, priority, difficulty, status)
             VALUES (99, 1, 't', 'improvement', 'low', 1, 'open')",
            params![]
        ),
        Err(Error::Integrity(_))
    ));
    assert!(matches!(
        db.execute("INSERT INTO items (project_id, number, addendum, created_at) VALUES (1, 3, 0, 'x')", params![]),
        Err(Error::Integrity(_))
    ));
    db.execute("INSERT INTO items (project_id, number, addendum, created_at) VALUES (1, 3, 1, 'x')", params![])
        .unwrap();
    assert!(matches!(db.execute("UPDATE item_versions SET archived = 2", params![]), Err(Error::Integrity(_))));
}

// ---------------------------------------------------------------- addenda

fn original(g: &giano_core::Giano, pid: i64) -> ItemView {
    g.items()
        .create(
            pid,
            &make("Cálculo de frete").priority(Priority::High).difficulty(4).tags(&["frete"]).body("Texto **original** completo."),
            None,
        )
        .unwrap()
}

fn adj(title: &str) -> ItemData {
    make(title).kind(ItemType::Adjustment).body("Só o ajuste.")
}

#[test]
fn create_addendum_keeps_number_and_original() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    let rev_before = g.items().latest_revision(p.id).unwrap();
    let a = g.items().create_addendum(o.item_id, &adj("Ajuste do frete"), None).unwrap();
    assert_eq!(a.code(), "MERC-1.1");
    assert_eq!(a.key(), (1, 1));
    assert_eq!(a.parent_code().as_deref(), Some("MERC-1"));
    assert_ne!(a.item_id, o.item_id);
    assert_eq!(a.rev_start, rev_before + 1);
    assert_eq!(
        g.items().revision(p.id, a.rev_start).unwrap().unwrap().summary,
        "MERC-1.1 criado (adendo de MERC-1): Ajuste do frete"
    );
    assert_eq!(g.items().get(o.item_id, None).unwrap().unwrap().version_id, o.version_id);
    assert_eq!(g.items().parent(&a, None).unwrap().unwrap().item_id, o.item_id);
    assert_eq!(codes(&g.items().addenda(&o, None).unwrap()), ["MERC-1.1"]);
    assert_eq!(g.items().create(p.id, &make("Outro"), None).unwrap().code(), "MERC-2");
    // An addendum of an addendum goes to the original.
    let a2 = g.items().create_addendum(a.item_id, &adj("segundo"), None).unwrap();
    assert_eq!(a2.code(), "MERC-1.2");
    assert_eq!(codes(&g.items().list_items(p.id, None, Archived::Any).unwrap()), ["MERC-1", "MERC-1.1", "MERC-1.2", "MERC-2"]);
    assert_eq!(g.items().get_by_code("merc-1.1").unwrap().unwrap().data.title, "Ajuste do frete");
    assert!(g.items().get_by_code("MERC-1.9").unwrap().is_none());
    assert!(g.items().get_by_code("MERC-1.x").unwrap().is_none());
}

#[test]
fn addendum_is_managed_independently() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    let a = g.items().create_addendum(o.item_id, &adj("Ajuste do frete"), None).unwrap();
    g.items().set_status(o.item_id, ItemStatus::Done).unwrap();
    g.items().archive(o.item_id).unwrap();
    let current = g.items().list_items(p.id, None, Archived::No).unwrap();
    assert_eq!(codes(&current), ["MERC-1.1"]);
    assert_eq!(current[0].data.status, ItemStatus::Open);
    let rev = g.items().latest_revision(p.id).unwrap();
    assert!(g.items().update(a.item_id, &a.data).unwrap().is_none());
    assert_eq!(g.items().latest_revision(p.id).unwrap(), rev);
    let new = g.items().set_status(a.item_id, ItemStatus::InProgress).unwrap().unwrap();
    assert_eq!(
        g.items().revision(p.id, new.rev_start).unwrap().unwrap().summary,
        "MERC-1.1: status Aberto → Em desenvolvimento"
    );
    assert_eq!(g.items().create_addendum(o.item_id, &adj("mais um"), None).unwrap().code(), "MERC-1.2");
}

#[test]
fn old_revisions_do_not_show_later_addenda() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    let rev = g.items().latest_revision(p.id).unwrap();
    let a = g.items().create_addendum(o.item_id, &adj("Ajuste"), None).unwrap();
    assert_eq!(codes(&g.items().list_items(p.id, Some(rev), Archived::Any).unwrap()), ["MERC-1"]);
    assert!(g.items().addenda(&o, Some(rev)).unwrap().is_empty());
    assert_eq!(codes(&g.items().diff(p.id, rev, a.rev_start, true).unwrap().added), ["MERC-1.1"]);
}

#[test]
fn original_with_addenda_cannot_be_deleted() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    let a = g.items().create_addendum(o.item_id, &adj("Ajuste"), None).unwrap();
    assert!(err_msg(g.items().delete(o.item_id)).contains("MERC-1 tem adendos (MERC-1.1)"));
    g.items().archive(a.item_id).unwrap(); // archived addenda still count
    assert!(err_msg(g.items().delete(o.item_id)).contains("tem adendos"));
    g.items().delete(a.item_id).unwrap();
    g.items().delete(o.item_id).unwrap();
    assert!(g.items().list_items(p.id, None, Archived::Any).unwrap().is_empty());
    assert!(g.items().create_addendum(o.item_id, &adj("x"), None).is_err());
}

#[test]
fn addendum_validation_and_search() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    assert!(err_msg(g.items().create_addendum(o.item_id, &adj("  "), None)).contains("título"));
    assert!(err_msg(g.items().create_addendum(9999, &adj("x"), None)).contains("não encontrado"));
    g.items().create_addendum(o.item_id, &adj("Ajuste").body("cubagem dos tubos"), None).unwrap();
    let hits: Vec<String> = g
        .search()
        .search("cubagem", Some(p.id), 100)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == HitKind::Item)
        .map(|h| h.label)
        .collect();
    assert_eq!(hits, ["MERC-1.1"]);
    g.projects().set_active(p.id, false).unwrap();
    assert!(err_msg(g.items().create_addendum(o.item_id, &adj("x"), None)).contains("Projeto arquivado"));
}

#[test]
fn html_with_only_the_addendum_references_the_original() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    g.items().create_addendum(o.item_id, &adj("Ajuste").body("Converter **cm** em m."), None).unwrap();
    let opts = HtmlOptions { keys: Some([(1, 1)].into()), ..Default::default() };
    let html = g.exports().export_html("MERC", &opts, None).unwrap().content;
    assert!(html.contains("id=\"merc-1.1\""));
    assert!(!html.contains("id=\"merc-1\""));
    assert!(html.contains("Adendo") && html.contains("Cálculo de frete"));
    assert!(html.contains("<strong>original</strong>"));
    assert!(html.contains("Ver o item original (MERC-1)"));
    assert!(html.contains("Converter <strong>cm</strong> em m."));
    assert!(html.contains("1 item (seleção de 2)"));
}

#[test]
fn html_with_both_links_original_and_addendum() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    g.items().create_addendum(o.item_id, &adj("Ajuste"), None).unwrap();
    let html = g.exports().export_html("MERC", &HtmlOptions::default(), None).unwrap().content;
    assert!(html.contains("id=\"merc-1\"") && html.contains("id=\"merc-1.1\""));
    assert!(html.contains("<a href=\"#merc-1\">MERC-1</a>"));
    assert!(html.contains("<a href=\"#merc-1.1\">MERC-1.1</a>"));
    assert!(!html.contains("Ver o item original"));
}

#[test]
fn html_archived_original_is_referenced_without_its_text() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    g.items().create_addendum(o.item_id, &adj("Ajuste"), None).unwrap();
    g.items().archive(o.item_id).unwrap();
    let html = g.exports().export_html("MERC", &HtmlOptions::default(), None).unwrap().content;
    assert!(!html.contains("id=\"merc-1\""));
    assert!(html.contains("arquivado") && html.contains("Cálculo de frete"));
    assert!(!html.contains("<strong>original</strong>"));
    let opts = HtmlOptions { keys: Some([(1, 0)].into()), ..Default::default() };
    assert!(err_msg(g.exports().export_html("MERC", &opts, None)).contains("arquivados"));
}

#[test]
fn claude_export_marks_addenda() {
    let g = env();
    let p = project(&g);
    let o = original(&g, p.id);
    g.items().create_addendum(o.item_id, &adj("Ajuste do frete"), None).unwrap();
    let (md, _) = g.exports().claude_markdown(&p, &Default::default()).unwrap();
    assert!(md.contains("### MERC-1.1 — Ajuste do frete"));
    assert!(md.contains("Adendo de MERC-1 — Cálculo de frete (Aberto)"));
}
