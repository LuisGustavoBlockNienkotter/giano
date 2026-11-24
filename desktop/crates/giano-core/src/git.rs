//! The git commands the repository link needs (the `git` of the computer, with the user's
//! credentials). Read-only except `pull --ff-only`.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::Serialize;

use crate::domain::CommitInfo;
use crate::error::{Error, Result};
use crate::process;

const QUICK: Duration = Duration::from_secs(60);
const PULL: Duration = Duration::from_secs(5 * 60);

/// What the interface shows about a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitInfo {
    pub root: String,
    pub branch: String,
    pub head: String,
    /// `origin` (or the first remote), without credentials.
    pub remote: Option<String>,
    /// Upstream of the branch (`origin/main`): needed by the pull.
    pub upstream: Option<String>,
    /// Files changed and not committed.
    pub dirty: usize,
}

fn git_cmd(dir: &Path, args: &[&str]) -> std::process::Command {
    let mut cmd = process::command("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=off", "-c", "i18n.logOutputEncoding=utf-8", "-c", "color.ui=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

fn git_output(dir: &Path, args: &[&str], cancel: Option<&AtomicBool>, timeout: Duration) -> Result<process::Output> {
    process::run(git_cmd(dir, args), None, cancel, timeout).map_err(|e| match e {
        Error::Validation(m) if m.starts_with("Não foi possível executar") => {
            Error::validation("O git não foi encontrado neste computador (instale o Git for Windows).")
        }
        other => other,
    })
}

/// `git args` in `dir`: stdout, or the error of git as a user-facing message.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git_output(dir, args, None, QUICK)?;
    if !out.success {
        return Err(Error::validation(format!("git {}: {}", args.join(" "), first_lines(&out.stderr, &out.stdout))));
    }
    Ok(out.stdout)
}

fn first_lines(stderr: &str, stdout: &str) -> String {
    let text = if stderr.trim().is_empty() { stdout } else { stderr };
    text.lines().filter(|l| !l.trim().is_empty()).take(6).collect::<Vec<_>>().join("\n")
}

/// The repository that contains `dir`; `None` when it is not inside one.
pub fn info(dir: &Path) -> Result<Option<GitInfo>> {
    let out = git_output(dir, &["rev-parse", "--show-toplevel"], None, QUICK)?;
    if !out.success {
        return Ok(None);
    }
    let root = out.stdout.trim().to_string();
    let head = git(dir, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let upstream = git(dir, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let remote = remote_url(dir);
    let dirty = git(dir, &["status", "--porcelain"]).map(|s| s.lines().count()).unwrap_or(0);
    Ok(Some(GitInfo { root, branch, head, remote, upstream, dirty }))
}

fn remote_url(dir: &Path) -> Option<String> {
    let name = git(dir, &["remote"]).ok().and_then(|s| {
        let names: Vec<String> = s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        names.iter().find(|n| *n == "origin").or(names.first()).cloned()
    })?;
    git(dir, &["remote", "get-url", &name]).ok().map(|u| sanitize_remote(u.trim())).filter(|u| !u.is_empty())
}

pub fn head(dir: &Path) -> Result<String> {
    Ok(git(dir, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// `git pull --ff-only`: never merges nor overwrites local changes. Returns git's output.
pub fn pull(dir: &Path, cancel: Option<&AtomicBool>) -> Result<String> {
    let out = git_output(dir, &["pull", "--ff-only"], cancel, PULL)?;
    if !out.success {
        return Err(Error::validation(format!(
            "O git pull não foi feito:\n{}",
            first_lines(&out.stderr, &out.stdout)
        )));
    }
    let text = format!("{}\n{}", out.stdout.trim(), out.stderr.trim());
    Ok(text.trim().to_string())
}

/// `a` is `b` or one of its ancestors (false also when `a` does not exist here).
pub fn is_ancestor(dir: &Path, a: &str, b: &str) -> bool {
    git_output(dir, &["merge-base", "--is-ancestor", a, b], None, QUICK).is_ok_and(|o| o.success)
}

/// Commits of `base..head` (all of `head` without a base), newest first, at most `limit`.
/// The flag says the range had more.
pub fn log(dir: &Path, base: Option<&str>, head: &str, limit: usize) -> Result<(Vec<CommitInfo>, bool)> {
    let range = match base {
        Some(b) => format!("{b}..{head}"),
        None => head.to_string(),
    };
    let max = format!("--max-count={}", limit + 1);
    let text = git(dir, &["log", &max, "--format=%H%x1f%h%x1f%an%x1f%aI%x1f%s%x1f%b%x1e", &range])?;
    let mut commits: Vec<CommitInfo> = text
        .split('\u{1e}')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.trim_start_matches(['\n', '\r']).split('\u{1f}').collect();
            (f.len() >= 6 && !f[0].trim().is_empty()).then(|| CommitInfo {
                sha: f[0].trim().to_string(),
                short: f[1].trim().to_string(),
                author: f[2].trim().to_string(),
                date: f[3].trim().to_string(),
                subject: f[4].trim().to_string(),
                body: f[5].trim().to_string(),
            })
        })
        .collect();
    let truncated = commits.len() > limit;
    commits.truncate(limit);
    Ok((commits, truncated))
}

/// `git diff --stat` of the range (files and lines changed), at most `max_lines` lines.
pub fn diff_stat(dir: &Path, base: &str, head: &str, max_lines: usize) -> Result<String> {
    let text = git(dir, &["diff", "--stat=160", &format!("{base}..{head}")])?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return Ok(text.trim_end().to_string());
    }
    let mut out = lines[..max_lines - 1].join("\n");
    out.push_str(&format!("\n… ({} linhas omitidas)\n{}", lines.len() - max_lines, lines[lines.len() - 1]));
    Ok(out)
}

/// A remote URL without credentials (`https://user:token@host/x` → `https://host/x`): it is
/// stored in the shared database.
pub fn sanitize_remote(url: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.-]*://)[^/@]*@").expect("regex"));
    re.replace(url.trim(), "$1").into_owned()
}

/// The web page of a remote on GitHub, GitLab or Bitbucket (`git@github.com:o/r.git` →
/// `https://github.com/o/r`); `None` for other hosts.
pub fn web_url(remote: &str) -> Option<String> {
    static SSH: OnceLock<Regex> = OnceLock::new();
    static HTTP: OnceLock<Regex> = OnceLock::new();
    let ssh = SSH.get_or_init(|| Regex::new(r"^(?:ssh://)?git@([^:/]+)[:/](.+?)(?:\.git)?/?$").expect("regex"));
    let http = HTTP.get_or_init(|| Regex::new(r"^https?://(?:[^@/]*@)?([^/]+)/(.+?)(?:\.git)?/?$").expect("regex"));
    let remote = remote.trim();
    let caps = ssh.captures(remote).or_else(|| http.captures(remote))?;
    let host = caps[1].to_lowercase();
    if !["github.com", "gitlab.com", "bitbucket.org"].contains(&host.as_str()) {
        return None;
    }
    Some(format!("https://{host}/{}", &caps[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes() {
        assert_eq!(sanitize_remote("https://user:tok@github.com/o/r.git"), "https://github.com/o/r.git");
        assert_eq!(sanitize_remote("git@github.com:o/r.git"), "git@github.com:o/r.git");
        assert_eq!(web_url("git@github.com:o/r.git").as_deref(), Some("https://github.com/o/r"));
        assert_eq!(web_url("https://github.com/o/r").as_deref(), Some("https://github.com/o/r"));
        assert_eq!(web_url("https://x@gitlab.com/g/s/r.git/").as_deref(), Some("https://gitlab.com/g/s/r"));
        assert_eq!(web_url("https://git.example.com/o/r.git"), None);
        assert_eq!(web_url("C:/repos/r"), None);
    }
}
