//! Repository link and "Atualizar e verificar" (a real git repository in a temporary folder;
//! Claude Code is replaced by a script that prints a fixed answer).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use common::*;
use giano_core::config;
use giano_core::domain::{ItemStatus, TouchRelation};
use giano_core::services::repo::{self, Analysis};
use serde_json::json;

/// config.json is process-wide (GIANO_CONFIG_DIR): the tests of this file take turns.
fn config_lock() -> std::sync::MutexGuard<'static, ()> {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let dir = DIR.get_or_init(|| tempfile::tempdir().unwrap());
    std::env::set_var("GIANO_CONFIG_DIR", dir.path());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Teste", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) -> String {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

fn new_repo(root: &Path) -> PathBuf {
    let dir = root.join("repo");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["remote", "add", "origin", "https://user:secret@github.com/csg/mercurius.git"]);
    dir
}

/// A fake `claude` that prints `answer` as the structured output of a run.
fn fake_claude(root: &Path, answer: &serde_json::Value) -> PathBuf {
    let envelope = json!({"type": "result", "is_error": false, "result": "", "structured_output": answer, "total_cost_usd": 0.12});
    let json_file = root.join("answer.json");
    std::fs::write(&json_file, envelope.to_string()).unwrap();
    if cfg!(windows) {
        let script = root.join("claude.cmd");
        std::fs::write(&script, format!("@type \"{}\"\r\n", json_file.display())).unwrap();
        script
    } else {
        let script = root.join("claude");
        std::fs::write(&script, format!("#!/bin/sh\ncat > /dev/null\ncat '{}'\n", json_file.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        script
    }
}

#[test]
fn parse_codes() {
    assert_eq!(repo::parse_code("MERC-33.1", "MERC"), Some((33, 1)));
    assert_eq!(repo::parse_code(" merc-7 ", "MERC"), Some((7, 0)));
    assert_eq!(repo::parse_code("`#12`", "MERC"), Some((12, 0)));
    assert_eq!(repo::parse_code("P2K-3", "MERC"), None);
    assert_eq!(repo::parse_code("item três", "MERC"), None);
}

#[test]
fn link_stores_folder_here_and_remote_without_credentials() {
    let _l = config_lock();
    let g = env();
    let p = g.projects().create("LNK", "Link", "").unwrap();
    let dir = new_repo(g.dir.path());
    commit(&dir, "a.txt", "a", "início");
    // A subfolder links the root of its repository.
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let info = g.repo().link(p.id, &dir.join("sub").to_string_lossy()).unwrap();
    let git_info = info.git.unwrap();
    assert_eq!(git_info.branch, "main");
    assert_eq!(info.remote, "https://github.com/csg/mercurius.git");
    assert_eq!(info.web_url.as_deref(), Some("https://github.com/csg/mercurius"));
    let stored = config::repo_path("LNK").unwrap().unwrap();
    assert_eq!(std::fs::canonicalize(stored).unwrap(), std::fs::canonicalize(&dir).unwrap());
    assert_eq!(g.projects().get(p.id).unwrap().unwrap().repo_remote, "https://github.com/csg/mercurius.git");

    assert_eq!(g.repo().existing_files(p.id, &["a.txt".into(), "x.txt".into(), "../a.txt".into()]).unwrap(), ["a.txt"]);
    assert!(g.repo().resolve_file(p.id, "../../etc/passwd").is_err());

    config::rename_repo_path("LNK", "LNK2").unwrap();
    assert!(config::repo_path("LNK").unwrap().is_none());
    assert!(config::repo_path("LNK2").unwrap().is_some());
    config::set_repo_path("LNK2", None).unwrap();
    // Unlinking is per computer: the remote stays in the database.
    assert_eq!(g.projects().get(p.id).unwrap().unwrap().repo_remote, "https://github.com/csg/mercurius.git");
}

#[test]
fn check_finds_touched_items_and_remembers_the_range() {
    let _l = config_lock();
    let g = env();
    let p = g.projects().create("CHK", "Check", "Contexto.").unwrap();
    let a = g.items().create(p.id, &make("Login com SSO"), None).unwrap();
    let b = g.items().create(p.id, &make("Relatório de vendas"), None).unwrap();
    let c = g.items().create(p.id, &make("Tela antiga").status(ItemStatus::Discarded), None).unwrap();
    let dir = new_repo(g.dir.path());
    let first = commit(&dir, "src/login.rs", "fn login() {}", "Login: SSO");

    let answer = json!({
        "resumo": "Implementado o login com SSO (CHK-1).",
        "itens": [
            {"codigo": "CHK-1", "relacao": "resolve", "justificativa": "Login SSO feito.", "commits": [first.clone(), "HEAD"], "arquivos": ["./src/login.rs"]},
            {"codigo": "CHK-99", "relacao": "avanca", "justificativa": "Não existe."},
            {"codigo": "CHK-3", "relacao": "avanca", "justificativa": "Descartado: não entra."}
        ],
        "sem_item": ["Ajuste no README"]
    });
    let script = fake_claude(g.dir.path(), &answer);
    config::save_config(&[(config::CLAUDE_PATH, Some(&script.to_string_lossy()))]).unwrap();
    g.repo().link(p.id, &dir.to_string_lossy()).unwrap();

    let plan = g.repo().prepare(p.id).unwrap();
    assert_eq!(plan.items.iter().map(|v| v.item_id).collect::<Vec<_>>(), [a.item_id, b.item_id]);
    let mut steps = Vec::new();
    let analysis = repo::analyze(&plan, false, None, &mut |s| steps.push(s.to_string())).unwrap();
    let Analysis::Done { from, to, branch, summary, result } = analysis else { panic!("expected a result") };
    assert_eq!((from, to.as_str(), branch.as_str()), (None, first.as_str(), "main"));
    assert_eq!(summary, "Implementado o login com SSO (CHK-1).");
    assert_eq!(result.commits.len(), 1);
    assert_eq!(result.items.len(), 1);
    let t = &result.items[0];
    assert_eq!((t.item_id, t.relation, t.code.as_str()), (a.item_id, TouchRelation::Resolves, "CHK-1"));
    assert_eq!(t.files, ["src/login.rs"]);
    assert_eq!(t.commits, [result.commits[0].short.clone()]);
    assert_eq!(result.unmatched, ["Ajuste no README"]);
    assert_eq!(result.cost_usd, Some(0.12));
    assert!(steps.iter().any(|s| s.contains("analisando 1 commit")));

    let check = g.repo().save(p.id, None, &to, &branch, &summary, &result).unwrap();
    assert_eq!(check.commit_count, 1);
    assert_eq!(check.result, result);
    assert!(check.revision_summary().starts_with("Pull "));
    assert_eq!(g.repo().checks(p.id, 10).unwrap().len(), 1);
    // Nothing changed on the items.
    assert_eq!(g.items().latest_revision(p.id).unwrap(), 3);

    // No commit since the check.
    let plan = g.repo().prepare(p.id).unwrap();
    assert!(matches!(repo::analyze(&plan, false, None, &mut |_| {}).unwrap(), Analysis::NothingNew { .. }));

    // A new commit: the range starts at the last commit verified; the message cites CHK-2.
    let second = commit(&dir, "src/report.rs", "fn report() {}", "Relatório (CHK-2)");
    let plan = g.repo().prepare(p.id).unwrap();
    let Analysis::Done { from, to, result, .. } = repo::analyze(&plan, false, None, &mut |_| {}).unwrap() else {
        panic!("expected a result")
    };
    assert_eq!((from.as_deref(), to.as_str()), (Some(first.as_str()), second.as_str()));
    assert_eq!(result.commits.len(), 1);
    let cited = result.items.iter().find(|t| t.item_id == b.item_id).unwrap();
    assert!(cited.cited);
    assert_eq!(cited.relation, TouchRelation::Cited);
    let _ = c;
    config::save_config(&[(config::CLAUDE_PATH, None)]).unwrap();
}

#[test]
fn pull_failure_is_reported() {
    let _l = config_lock();
    let g = env();
    let p = g.projects().create("PUL", "Pull", "").unwrap();
    let dir = new_repo(g.dir.path());
    commit(&dir, "a.txt", "a", "início");
    g.repo().link(p.id, &dir.to_string_lossy()).unwrap();
    let plan = g.repo().prepare(p.id).unwrap();
    // No upstream (and an unreachable remote): the pull is refused with git's message.
    let err = repo::analyze(&plan, true, None, &mut |_| {}).err().unwrap();
    assert!(err.to_string().starts_with("O git pull não foi feito"), "{err}");
    config::set_repo_path("PUL", None).unwrap();
}

#[test]
fn not_linked() {
    let _l = config_lock();
    let g = env();
    let p = g.projects().create("NOL", "Sem repo", "").unwrap();
    let info = g.repo().info(p.id).unwrap();
    assert!(info.path.is_none() && info.git.is_none());
    assert!(g.repo().prepare(p.id).unwrap_err().to_string().contains("não está vinculado"));
}
