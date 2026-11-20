//! Print the projects and items of a database (checks compatibility with the Python TUI).
//!
//! cargo run -p giano-core --example inspect -- <arquivo.db> [criar SIGLA "título"]

use giano_core::config::DbTarget;
use giano_core::domain::ItemData;
use giano_core::services::items::Archived;
use giano_core::{system_clock, Giano};

fn main() -> giano_core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("uso: inspect <arquivo.db> [criar SIGLA título]");
    let g = Giano::open(Some(&DbTarget::local(path)), system_clock())?;
    if args.get(2).map(String::as_str) == Some("criar") {
        let p = g.projects().require(&args[3])?;
        let v = g.items().create(p.id, &ItemData::new(args[4].clone()), None)?;
        let a = g.items().create_addendum(v.item_id, &ItemData::new("Adendo pelo Rust"), None)?;
        println!("criado {} e {} (Rev. {})", v.code(), a.code(), a.rev_start);
        return Ok(());
    }
    for p in g.projects().list(true)? {
        let rev = g.items().latest_revision(p.id)?;
        println!("== {} — {} (Rev. {rev})", p.code, p.name);
        for v in g.items().list_items(p.id, None, Archived::Any)? {
            let d = &v.data;
            println!(
                "{} · {} · {} · {} · dif. {}/5 · {} · tags: {}{}",
                v.code(),
                d.title,
                d.kind.label(),
                d.priority.label(),
                d.difficulty,
                d.status.label(),
                if d.tags.is_empty() { "—".into() } else { d.tags.join(", ") },
                if d.archived { " · ARQUIVADO" } else { "" }
            );
        }
        for r in g.items().revisions(p.id)?.iter().take(3) {
            println!("   Rev. {} {}", r.number, r.summary);
        }
    }
    let hits = g.search().search("frete", None, 5)?;
    println!("busca 'frete': {}", hits.iter().map(|h| h.label.clone()).collect::<Vec<_>>().join(", "));
    Ok(())
}
