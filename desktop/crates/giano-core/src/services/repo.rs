//! Repository of a project and "Atualizar e verificar": `git pull`, then Claude Code reads
//! the new commits and says which items of the list they touched.
//!
//! The local folder is per computer (`config::repo_path`); the remote URL is shared
//! (`projects.repo_remote`). A check never changes items: it is stored in `repo_checks` and
//! the user changes the status of the items from it.
//!
//! The slow part (`analyze`: pull + Claude, minutes) needs no database, so the app does not
//! hold the database while it runs: `prepare` (database) → `analyze` → `save` (database).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value as Json};

use crate::claude_cli;
use crate::config;
use crate::db::{projects as projects_repo, repo_checks as repo};
use crate::domain::{
    CheckResult, CommitInfo, ItemKey, ItemStatus, ItemView, Project, RepoCheck, TouchRelation, TouchedItem,
};
use crate::error::{Error, Result};
use crate::git::{self, GitInfo};
use crate::process;
use crate::services::items::Archived;
use crate::services::Giano;

/// Commits analyzed at most (the most recent ones).
const MAX_COMMITS: usize = 50;
/// Without a previous check (or with history rewritten): the most recent commits.
const FIRST_COMMITS: usize = 20;
const BODY_CHARS: usize = 500;

/// What the interface shows about the repository of a project on this computer.
#[derive(Debug, Clone, Serialize)]
pub struct RepoInfo {
    pub project_id: i64,
    /// Remote URL stored in the database (shared; "" = none).
    pub remote: String,
    /// Page of the remote on GitHub/GitLab/Bitbucket (links to commits and files).
    pub web_url: Option<String>,
    /// Local folder on this computer (None = not linked here).
    pub path: Option<String>,
    pub path_exists: bool,
    /// None = the folder is not a git repository (or does not exist).
    pub git: Option<GitInfo>,
    /// Claude Code executable found on this computer.
    pub claude: Option<String>,
}

/// Everything `analyze` needs, read from the database.
#[derive(Debug, Clone)]
pub struct CheckPlan {
    pub project: Project,
    pub path: PathBuf,
    /// The last commit verified and when.
    pub last: Option<(String, String)>,
    /// Items Claude may cite: not archived, not discarded.
    pub items: Vec<ItemView>,
}

#[derive(Debug, Clone)]
pub enum Analysis {
    /// No commit since the last check. `pull` = output of the pull, when there was one.
    NothingNew { pull: Option<String>, since: Option<String> },
    Done { from: Option<String>, to: String, branch: String, summary: String, result: CheckResult },
}

pub struct RepoService<'g> {
    pub(crate) g: &'g Giano,
}

impl<'g> RepoService<'g> {
    fn project(&self, project_id: i64) -> Result<Project> {
        projects_repo::get(&self.g.db, project_id)?.ok_or_else(|| Error::validation("Projeto não encontrado."))
    }

    /// The local folder of the project on this computer (error when it is not linked).
    fn require_path(&self, project: &Project) -> Result<PathBuf> {
        let path = config::repo_path(&project.code)?.ok_or_else(|| {
            Error::validation(format!("O projeto {} não está vinculado a um repositório neste computador.", project.code))
        })?;
        if !path.is_dir() {
            return Err(Error::validation(format!("A pasta do repositório não existe mais: {}", path.display())));
        }
        Ok(path)
    }

    pub fn info(&self, project_id: i64) -> Result<RepoInfo> {
        let project = self.project(project_id)?;
        let path = config::repo_path(&project.code)?;
        let path_exists = path.as_ref().is_some_and(|p| p.is_dir());
        let git = match &path {
            Some(p) if path_exists => git::info(p)?,
            _ => None,
        };
        let remote = git.as_ref().and_then(|g| g.remote.clone()).unwrap_or_else(|| project.repo_remote.clone());
        Ok(RepoInfo {
            project_id,
            web_url: git::web_url(&remote),
            remote,
            path: path.map(|p| p.display().to_string()),
            path_exists,
            git,
            claude: claude_cli::find().map(|p| p.display().to_string()),
        })
    }

    /// Link the project to a folder on this computer. A git repository also records its
    /// remote URL (without credentials) in the database, for the other computers.
    pub fn link(&self, project_id: i64, path: &str) -> Result<RepoInfo> {
        let project = self.project(project_id)?;
        let path = path.trim();
        if path.is_empty() {
            return Err(Error::validation("Informe a pasta do repositório."));
        }
        let dir = PathBuf::from(path);
        if !dir.is_dir() {
            return Err(Error::validation(format!("Pasta não encontrada: {path}")));
        }
        let info = git::info(&dir)?;
        let folder = match &info {
            Some(g) => PathBuf::from(&g.root),
            None => dir,
        };
        let folder = folder.display().to_string().replace('/', std::path::MAIN_SEPARATOR_STR);
        config::set_repo_path(&project.code, Some(&folder))?;
        if let Some(remote) = info.and_then(|g| g.remote) {
            if remote != project.repo_remote {
                projects_repo::set_repo_remote(&self.g.db, project_id, &remote)?;
            }
        }
        self.info(project_id)
    }

    /// Unlink on this computer only (the remote URL stays for the other computers).
    pub fn unlink(&self, project_id: i64) -> Result<()> {
        let project = self.project(project_id)?;
        config::set_repo_path(&project.code, None)
    }

    pub fn checks(&self, project_id: i64, limit: i64) -> Result<Vec<RepoCheck>> {
        repo::list(&self.g.db, project_id, limit)
    }

    pub fn check(&self, check_id: i64) -> Result<Option<RepoCheck>> {
        repo::get(&self.g.db, check_id)
    }

    /// Read what a check needs (quick; the database is free again before `analyze`).
    pub fn prepare(&self, project_id: i64) -> Result<CheckPlan> {
        let project = self.project(project_id)?;
        if !project.active {
            return Err(Error::validation("Projeto arquivado: somente leitura."));
        }
        let path = self.require_path(&project)?;
        let items = self
            .g
            .items()
            .list_items(project_id, None, Archived::No)?
            .into_iter()
            .filter(|v| v.data.status != ItemStatus::Discarded)
            .collect();
        Ok(CheckPlan { last: repo::last_commit(&self.g.db, project_id)?, project, path, items })
    }

    /// Store a finished analysis.
    pub fn save(
        &self,
        project_id: i64,
        from: Option<&str>,
        to: &str,
        branch: &str,
        summary: &str,
        result: &CheckResult,
    ) -> Result<RepoCheck> {
        let db = &self.g.db;
        let count = result.commits.len() as i64;
        let id = db.transaction(|| repo::insert(db, project_id, from, to, branch, count, summary, result, &self.g.now_iso()))?;
        repo::get(db, id)?.ok_or_else(|| Error::validation("Verificação não encontrada."))
    }

    /// A file of the repository (`src/a.rs`), refused when it is outside the folder.
    pub fn resolve_file(&self, project_id: i64, relative: &str) -> Result<PathBuf> {
        let project = self.project(project_id)?;
        let root = self.require_path(&project)?;
        let file = inside(&root, relative)
            .ok_or_else(|| Error::validation(format!("Caminho fora do repositório: {relative}")))?;
        if !file.exists() {
            return Err(Error::validation(format!("Arquivo não encontrado no repositório: {relative}")));
        }
        Ok(file)
    }

    /// The paths (relative to the repository) that exist in it; empty when not linked.
    pub fn existing_files(&self, project_id: i64, paths: &[String]) -> Result<Vec<String>> {
        let project = self.project(project_id)?;
        let Some(root) = config::repo_path(&project.code)?.filter(|p| p.is_dir()) else { return Ok(Vec::new()) };
        Ok(paths.iter().filter(|p| inside(&root, p).is_some_and(|f| f.exists())).cloned().collect())
    }
}

/// `relative` joined to `root`, if it stays inside it (no absolute paths, no `..`).
fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let rel = Path::new(relative.trim().trim_start_matches("./"));
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        return None;
    }
    Some(root.join(rel))
}

// ---------------------------------------------------------------------------
// The analysis (no database)
// ---------------------------------------------------------------------------

/// Pull (optional), find the commits since the last check and ask Claude which items they
/// touched. `progress` receives a pt-BR line for each step.
pub fn analyze(plan: &CheckPlan, pull: bool, cancel: Option<&AtomicBool>, progress: &mut dyn FnMut(&str)) -> Result<Analysis> {
    let dir = &plan.path;
    let before = git::head(dir)?;
    let pull_out = if pull {
        progress("Atualizando o repositório (git pull)…");
        Some(git::pull(dir, cancel)?)
    } else {
        None
    };
    let info = git::info(dir)?.ok_or_else(|| Error::validation("A pasta vinculada não é um repositório git."))?;
    let head = info.head.clone();
    let base = match &plan.last {
        Some((sha, _)) if *sha == head => {
            return Ok(Analysis::NothingNew { pull: pull_out, since: plan.last.as_ref().map(|l| l.1.clone()) })
        }
        Some((sha, _)) if git::is_ancestor(dir, sha, &head) => Some(sha.clone()),
        _ if before != head => Some(before.clone()),
        _ => None,
    };
    let limit = if base.is_some() { MAX_COMMITS } else { FIRST_COMMITS };
    let (commits, truncated) = git::log(dir, base.as_deref(), &head, limit)?;
    if commits.is_empty() {
        return Ok(Analysis::NothingNew { pull: pull_out, since: plan.last.as_ref().map(|l| l.1.clone()) });
    }
    let stat = match &base {
        Some(b) => git::diff_stat(dir, b, &head, 120).unwrap_or_default(),
        None => commits
            .last()
            .and_then(|oldest| git::diff_stat(dir, &format!("{}~1", oldest.sha), &head, 120).ok())
            .unwrap_or_default(),
    };
    let exe = claude_cli::find().ok_or_else(|| Error::validation(claude_cli::not_found_message()))?;
    progress(&format!(
        "O Claude está analisando {} commit{}…",
        commits.len(),
        if commits.len() == 1 { "" } else { "s" }
    ));
    let prompt = build_prompt(plan, &info, base.as_deref(), &commits, truncated, &stat);
    let answer = claude_cli::run(&exe, Path::new(&info.root), &prompt, &answer_schema(), cancel)?;
    let (summary, items, unmatched) = interpret(&answer.json, &plan.items, &plan.project.code, &commits);
    Ok(Analysis::Done {
        from: base,
        to: head,
        branch: info.branch,
        summary,
        result: CheckResult { commits, truncated, items, unmatched, pull: pull_out, cost_usd: answer.cost_usd },
    })
}

fn cut(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut t: String = text.chars().take(max).collect();
    t.push_str(" […]");
    t
}

/// The request to Claude (pt-BR).
pub fn build_prompt(
    plan: &CheckPlan,
    info: &GitInfo,
    base: Option<&str>,
    commits: &[CommitInfo],
    truncated: bool,
    stat: &str,
) -> String {
    let p = &plan.project;
    let code = &p.code;
    let head = &info.head[..info.head.len().min(7)];
    let mut s = String::new();
    s.push_str(&format!(
        "Você é o verificador do Giano, um gerenciador de listas de melhorias. O projeto {code} ({}) \
         está ligado a este repositório git. Chegaram commits novos: quero saber o que foi feito e quais \
         itens da lista do projeto foram tocados por eles.\n\n",
        p.name
    ));
    s.push_str("## Regras\n");
    s.push_str("- NÃO altere nada: não edite arquivos, não rode comandos que mudem o repositório e não use o comando `giano`. Apenas leia.\n");
    match base {
        Some(b) => s.push_str(&format!(
            "- Para ver as mudanças use `git show <commit>` e `git diff {}..{head}`, e leia os arquivos quando precisar.\n",
            &b[..b.len().min(7)]
        )),
        None => s.push_str("- Para ver as mudanças use `git show <commit>` e leia os arquivos quando precisar.\n"),
    }
    s.push_str(&format!(
        "- Considere só os itens da lista abaixo e cite-os pelo código exato (ex.: {code}-12 ou {code}-12.1).\n\
         - relacao: \"resolve\" (as mudanças parecem concluir o item), \"avanca\" (fazem parte do item, mas falta algo) ou \"relacionado\" (mexem no assunto do item sem fazê-lo).\n\
         - Inclua um item só com evidência concreta nas mudanças; na dúvida, não inclua. Na justificativa, diga em uma ou duas frases o que foi feito para o item.\n\
         - \"sem_item\": mudanças relevantes que não correspondem a nenhum item da lista (frases curtas).\n\
         - \"resumo\": em português, em Markdown, curto (até ~15 linhas), do que foi feito no intervalo; cite os itens pelo código.\n\n"
    ));
    if !p.description.trim().is_empty() {
        s.push_str(&format!("## Contexto do projeto\n{}\n\n", cut(&p.description, 3000)));
    }
    s.push_str("## Intervalo\n");
    match base {
        Some(b) => s.push_str(&format!(
            "Branch {}: {}..{head} ({} commits).\n",
            info.branch,
            &b[..b.len().min(7)],
            commits.len()
        )),
        None => s.push_str(&format!(
            "Branch {}: primeira verificação, os {} commits mais recentes até {head}.\n",
            info.branch,
            commits.len()
        )),
    }
    if truncated {
        s.push_str("O intervalo tem mais commits: só os mais recentes estão listados abaixo.\n");
    }
    s.push_str("\n## Commits (mais recentes primeiro)\n");
    for c in commits {
        s.push_str(&format!("- {} {} {}: {}\n", c.short, &c.date[..c.date.len().min(10)], c.author, c.subject));
        if !c.body.trim().is_empty() {
            for line in cut(&c.body, BODY_CHARS).lines() {
                s.push_str(&format!("  {line}\n"));
            }
        }
    }
    if !stat.trim().is_empty() {
        s.push_str(&format!("\n## Arquivos alterados (git diff --stat)\n```\n{stat}\n```\n"));
    }
    s.push_str(&format!("\n## Itens da lista ({})\n", plan.items.len()));
    for v in &plan.items {
        s.push_str(&format!(
            "\n### {} — {}\nStatus: {} · Tipo: {}\n",
            v.code(),
            v.data.title,
            v.data.status.label(),
            v.data.kind.label()
        ));
        if !v.data.body.trim().is_empty() {
            s.push_str(&cut(&v.data.body, BODY_CHARS));
            s.push('\n');
        }
    }
    s.push_str(
        "\n## Resposta\nResponda SOMENTE com um objeto JSON neste formato (sem texto antes ou depois):\n\
         {\"resumo\": \"...\", \"itens\": [{\"codigo\": \"...\", \"relacao\": \"resolve|avanca|relacionado\", \
         \"justificativa\": \"...\", \"commits\": [\"abc1234\"], \"arquivos\": [\"caminho/do/arquivo\"]}], \"sem_item\": [\"...\"]}\n",
    );
    s
}

/// JSON schema of the answer (`claude --json-schema`).
pub fn answer_schema() -> Json {
    json!({
        "type": "object",
        "properties": {
            "resumo": { "type": "string" },
            "itens": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "codigo": { "type": "string" },
                        "relacao": { "type": "string", "enum": ["resolve", "avanca", "relacionado"] },
                        "justificativa": { "type": "string" },
                        "commits": { "type": "array", "items": { "type": "string" } },
                        "arquivos": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["codigo", "relacao", "justificativa"]
                }
            },
            "sem_item": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["resumo", "itens", "sem_item"]
    })
}

/// `MERC-33.1`, `merc-33`, `33.1`, `#33` → `(33, 1)`; another project's code → None.
pub fn parse_code(text: &str, project_code: &str) -> Option<ItemKey> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(?:([A-Za-z0-9_]+)-)?#?([0-9]+)(?:\.([0-9]+))?$").expect("regex"));
    let text = text.trim().trim_matches(|c: char| matches!(c, '`' | '[' | ']' | '*' | '"'));
    let caps = re.captures(text)?;
    if let Some(prefix) = caps.get(1) {
        if !prefix.as_str().eq_ignore_ascii_case(project_code) {
            return None;
        }
    }
    let number = caps[2].parse().ok()?;
    let addendum = caps.get(3).map_or(Some(0), |m| m.as_str().parse().ok())?;
    Some((number, addendum))
}

fn strings(v: Option<&Json>) -> Vec<String> {
    v.and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

/// The short hash of a commit of the range Claude cited (None for anything else, e.g. `HEAD`).
fn short_sha(given: &str, commits: &[CommitInfo]) -> Option<String> {
    let g = given.trim().to_lowercase();
    commits
        .iter()
        .find(|c| g.len() >= 4 && (c.sha.starts_with(&g) || g.starts_with(&c.short)))
        .map(|c| c.short.clone())
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// Claude's answer → (summary, touched items, changes without an item). Unknown codes are
/// dropped; commits that cite an item's code in their message mark it `cited` (and add it
/// when Claude did not).
pub fn interpret(
    answer: &Json,
    items: &[ItemView],
    project_code: &str,
    commits: &[CommitInfo],
) -> (String, Vec<TouchedItem>, Vec<String>) {
    let by_key: BTreeMap<ItemKey, &ItemView> = items.iter().map(|v| (v.key(), v)).collect();
    let mut touched: BTreeMap<ItemKey, TouchedItem> = BTreeMap::new();
    let new_touch = |v: &ItemView, relation: TouchRelation, reason: String| TouchedItem {
        item_id: v.item_id,
        number: v.number,
        addendum: v.addendum,
        code: v.code(),
        title: v.data.title.clone(),
        relation,
        reason,
        commits: Vec::new(),
        files: Vec::new(),
        cited: false,
    };
    for entry in answer.get("itens").and_then(Json::as_array).into_iter().flatten() {
        let Some(key) = entry.get("codigo").and_then(Json::as_str).and_then(|c| parse_code(c, project_code)) else {
            continue;
        };
        let Some(view) = by_key.get(&key) else { continue };
        let relation = entry
            .get("relacao")
            .and_then(Json::as_str)
            .and_then(TouchRelation::parse_answer)
            .unwrap_or(TouchRelation::Related);
        let reason = entry.get("justificativa").and_then(Json::as_str).unwrap_or("").trim().to_string();
        let t = touched.entry(key).or_insert_with(|| new_touch(view, relation, String::new()));
        t.relation = t.relation.min(relation);
        if !reason.is_empty() {
            t.reason = if t.reason.is_empty() { reason } else { format!("{} {}", t.reason, reason) };
        }
        for c in strings(entry.get("commits")) {
            if let Some(sha) = short_sha(&c, commits) {
                push_unique(&mut t.commits, sha);
            }
        }
        for f in strings(entry.get("arquivos")) {
            push_unique(&mut t.files, f.replace('\\', "/").trim_start_matches("./").to_string());
        }
    }
    // Commit messages that cite the items' codes.
    let cite = Regex::new(&format!(r"(?i)\b{}-([0-9]+)(?:\.([0-9]+))?\b", regex::escape(project_code))).ok();
    for c in commits {
        let text = format!("{}\n{}", c.subject, c.body);
        for caps in cite.iter().flat_map(|re| re.captures_iter(&text)) {
            let number: i64 = caps[1].parse().unwrap_or(0);
            let addendum: i64 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let Some(view) = by_key.get(&(number, addendum)) else { continue };
            let t = touched.entry((number, addendum)).or_insert_with(|| {
                new_touch(view, TouchRelation::Cited, format!("Citado na mensagem do commit {}.", c.short))
            });
            t.cited = true;
            push_unique(&mut t.commits, c.short.clone());
        }
    }
    let mut list: Vec<TouchedItem> = touched.into_values().collect();
    list.sort_by_key(|t| (t.relation, t.number, t.addendum));
    let summary = answer.get("resumo").and_then(Json::as_str).unwrap_or("").trim().to_string();
    (summary, list, strings(answer.get("sem_item")))
}

// ---------------------------------------------------------------------------
// Opening files of the repository
// ---------------------------------------------------------------------------

/// Open `file` (at `line`) in the editor of config.json (`editor`), else VS Code or Zed if
/// installed. `false` = no editor found (the caller opens it with the default program).
pub fn open_in_editor(file: &Path, line: Option<u32>) -> Result<bool> {
    let configured = config::config_value(config::EDITOR)?;
    let exe = match configured {
        Some(e) => {
            let p = PathBuf::from(&e);
            if p.is_file() {
                Some(p)
            } else {
                process::find_on_path(&e)
            }
        }
        None => process::find_on_path("code").or_else(|| process::find_on_path("zed")),
    };
    let Some(exe) = exe else { return Ok(false) };
    let name = exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let target = match line {
        Some(l) => format!("{}:{l}", file.display()),
        None => file.display().to_string(),
    };
    let mut cmd = process::command(&exe);
    if name == "code" || name == "code-insiders" || name == "cursor" || name == "codium" {
        cmd.arg("-g");
    }
    cmd.arg(target);
    cmd.spawn().map_err(|e| Error::validation(format!("Não foi possível abrir o editor {}: {e}", exe.display())))?;
    Ok(true)
}
