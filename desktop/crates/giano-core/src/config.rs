//! Locations, the per-computer configuration file and defaults (same files as the Python
//! version, so the TUI and the desktop app share the database and the configuration).
//!
//! Where the data lives (first match wins):
//!
//! 1. `GIANO_DB`: a local SQLite file (tests, demos, offline copies).
//! 2. `GIANO_TURSO_URL` + `GIANO_TURSO_TOKEN`: a Turso database.
//! 3. `turso_url` + `turso_token` in `config.json`.
//! 4. The local file in the user's data directory.
//!
//! `config.json` lives in the user's config directory (`GIANO_CONFIG_DIR` overrides it) and
//! holds only what belongs to this computer: the Turso connection, the export folder, the
//! local folder of each project's repository (`repo.<SIGLA>`), the editor that opens its files
//! and where Claude Code is.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const APP_NAME: &str = "giano";
pub const CONFIG_FILE: &str = "config.json";
pub const TURSO_URL: &str = "turso_url";
pub const TURSO_TOKEN: &str = "turso_token";
pub const EXPORT_DIR: &str = "export_dir";
/// `repo.MERC`: local folder of the repository of project MERC on this computer.
pub const REPO_PREFIX: &str = "repo.";
/// Program that opens files of a repository (`code`, `zed`...; empty = automatic).
pub const EDITOR: &str = "editor";
/// Full path of the Claude Code executable (empty = found on the PATH).
pub const CLAUDE_PATH: &str = "claude_path";

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.is_empty())
}

fn expand_user(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~") {
        if rest.is_empty() || rest.starts_with(['/', '\\']) {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    PathBuf::from(path)
}

/// Python's `platformdirs.user_data_dir("giano", appauthor=False)`.
fn user_data_dir() -> PathBuf {
    let base = if cfg!(windows) { dirs::data_local_dir() } else { dirs::data_dir() };
    base.unwrap_or_else(|| PathBuf::from(".")).join(APP_NAME)
}

/// Python's `platformdirs.user_config_dir("giano", appauthor=False)`.
fn user_config_dir() -> PathBuf {
    let base = if cfg!(windows) { dirs::data_local_dir() } else { dirs::config_dir() };
    base.unwrap_or_else(|| PathBuf::from(".")).join(APP_NAME)
}

/// `GIANO_DB` overrides the default location (the user's data directory).
pub fn default_db_path() -> PathBuf {
    match non_empty_env("GIANO_DB") {
        Some(p) => expand_user(&p),
        None => user_data_dir().join("giano.db"),
    }
}

pub fn default_export_dir() -> PathBuf {
    dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("Giano")
}

// ---------------------------------------------------------------------------
// Per-computer configuration file
// ---------------------------------------------------------------------------

pub fn config_path() -> PathBuf {
    let base = match non_empty_env("GIANO_CONFIG_DIR") {
        Some(dir) => expand_user(&dir),
        None => user_config_dir(),
    };
    base.join(CONFIG_FILE)
}

pub type Config = BTreeMap<String, String>;

pub fn load_config() -> Result<Config> {
    let path = config_path();
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::new()),
        Err(e) => {
            return Err(Error::Validation(format!(
                "Não foi possível ler a configuração {}: {e}",
                path.display()
            )))
        }
    };
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| {
            Error::Validation(format!("Não foi possível ler a configuração {}: {e}", path.display()))
        })?;
    let mut cfg = Config::new();
    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            if let serde_json::Value::String(s) = v {
                cfg.insert(k, s);
            }
        }
    }
    Ok(cfg)
}

/// Update keys of `config.json` (`None` removes the key).
pub fn save_config(changes: &[(&str, Option<&str>)]) -> Result<PathBuf> {
    let mut data = load_config()?;
    for (key, value) in changes {
        match value {
            Some(v) => data.insert((*key).to_string(), (*v).to_string()),
            None => data.remove(*key),
        };
    }
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut text = serde_json::to_string_pretty(&data).map_err(|e| Error::Database(e.to_string()))?;
    text.push('\n');
    fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)); // holds the token
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}

/// The local folder of a project's repository on this computer.
pub fn repo_path(project_code: &str) -> Result<Option<PathBuf>> {
    let key = format!("{REPO_PREFIX}{}", project_code.to_uppercase());
    Ok(load_config()?.get(&key).filter(|p| !p.trim().is_empty()).map(|p| expand_user(p.trim())))
}

/// Link (`Some`) or unlink (`None`) the repository folder of a project on this computer.
pub fn set_repo_path(project_code: &str, path: Option<&str>) -> Result<()> {
    let key = format!("{REPO_PREFIX}{}", project_code.to_uppercase());
    save_config(&[(key.as_str(), path)])?;
    Ok(())
}

/// A project changed its code: its repository folder follows it.
pub fn rename_repo_path(old_code: &str, new_code: &str) -> Result<()> {
    if old_code.eq_ignore_ascii_case(new_code) {
        return Ok(());
    }
    if let Some(path) = repo_path(old_code)? {
        let path = path.to_string_lossy().into_owned();
        let old_key = format!("{REPO_PREFIX}{}", old_code.to_uppercase());
        let new_key = format!("{REPO_PREFIX}{}", new_code.to_uppercase());
        save_config(&[(old_key.as_str(), None), (new_key.as_str(), Some(path.as_str()))])?;
    }
    Ok(())
}

pub fn config_value(key: &str) -> Result<Option<String>> {
    Ok(load_config()?.get(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
}

// ---------------------------------------------------------------------------
// Database location
// ---------------------------------------------------------------------------

const REMOTE_SCHEMES: [&str; 5] = ["libsql://", "https://", "http://", "wss://", "ws://"];

pub fn is_remote_url(location: &str) -> bool {
    let lower = location.to_lowercase();
    REMOTE_SCHEMES.iter().any(|s| lower.starts_with(s))
}

/// A local SQLite file or a Turso URL (with its token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTarget {
    pub location: String,
    pub auth_token: Option<String>,
}

impl DbTarget {
    pub fn local(path: impl AsRef<Path>) -> Self {
        DbTarget { location: path.as_ref().to_string_lossy().into_owned(), auth_token: None }
    }

    pub fn remote(url: &str, token: &str) -> Self {
        DbTarget { location: normalize_turso_url(url), auth_token: Some(token.trim().to_string()) }
    }

    pub fn is_remote(&self) -> bool {
        is_remote_url(&self.location)
    }
}

pub fn normalize_turso_url(url: &str) -> String {
    let url = url.trim();
    if !url.is_empty() && !url.contains("://") {
        format!("libsql://{url}")
    } else {
        url.to_string()
    }
}

/// The configured Turso database (environment first, then `config.json`).
pub fn turso_target() -> Result<Option<DbTarget>> {
    if let Some(url) = non_empty_env("GIANO_TURSO_URL") {
        let token = env::var("GIANO_TURSO_TOKEN").unwrap_or_default();
        return Ok(Some(DbTarget::remote(&url, &token)));
    }
    let cfg = load_config()?;
    Ok(cfg.get(TURSO_URL).filter(|u| !u.is_empty()).map(|url| {
        DbTarget::remote(url, cfg.get(TURSO_TOKEN).map(String::as_str).unwrap_or(""))
    }))
}

/// Where Turso settings come from, for the settings screen.
pub fn turso_source() -> String {
    if non_empty_env("GIANO_TURSO_URL").is_some() {
        "variáveis GIANO_TURSO_URL/GIANO_TURSO_TOKEN".into()
    } else {
        config_path().display().to_string()
    }
}

pub fn resolve_db_target() -> Result<DbTarget> {
    if non_empty_env("GIANO_DB").is_some() {
        return Ok(DbTarget::local(default_db_path()));
    }
    Ok(turso_target()?.unwrap_or_else(|| DbTarget::local(default_db_path())))
}

pub const DEFAULT_CLAUDE_PROMPT: &str = "\
Analise as anotações acima à luz do contexto e da lista atual de melhorias do projeto.

1. Relacione cada anotação aos itens existentes que ela afeta (cite os identificadores, ex.: {sigla}-12).
2. Aponte duplicidades: anotações que repetem itens já existentes ou que se repetem entre si.
3. Aponte conflitos: pontos das anotações que contradizem itens existentes, decisões ou o contexto do sistema.
4. Sugira atualizações para itens existentes (status, prioridade, descrição) quando as anotações justificarem.
5. Proponha novos itens para o que ainda não está na lista. Para cada um, informe: título, tipo
   (Melhoria / Discussão / Ajuste), prioridade sugerida (Crítica / Alta / Média / Baixa),
   dificuldade sugerida (1 a 5), tags e uma descrição curta em Markdown.

Responda em português, de forma objetiva, organizada nessas seções.
";
