//! Claude Code CLI in print mode (`claude -p`), run inside a repository with read-only tools.
//! It only answers: the prompt goes in through stdin and the answer comes back as JSON.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::Value as Json;

use crate::config;
use crate::error::{Error, Result};
use crate::process;

const TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_TURNS: &str = "40";

/// Tools Claude may use: reading files and the read-only git commands. Everything else is
/// denied without asking (`dontAsk`); editing tools are removed. On Windows without Git Bash
/// Claude Code uses PowerShell, hence the PowerShell rules.
const ALLOWED_TOOLS: &str = "Read,Grep,Glob,\
Bash(git log *),Bash(git show *),Bash(git diff *),\
PowerShell(git log *),PowerShell(git show *),PowerShell(git diff *)";
const DISALLOWED_TOOLS: &str = "Edit,Write,NotebookEdit,WebFetch,WebSearch";

/// Where Claude Code is: `claude_path` in config.json, then the PATH, then the default folders
/// of the installers.
pub fn find() -> Option<PathBuf> {
    if let Ok(Some(p)) = config::config_value(config::CLAUDE_PATH) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(p) = process::find_on_path("claude") {
        return Some(p);
    }
    let mut candidates = Vec::new();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local").join("bin").join(if cfg!(windows) { "claude.exe" } else { "claude" }));
        candidates.push(home.join(".claude").join("local").join(if cfg!(windows) { "claude.exe" } else { "claude" }));
    }
    if cfg!(windows) {
        if let Some(roaming) = dirs::config_dir() {
            candidates.push(roaming.join("npm").join("claude.cmd"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub fn not_found_message() -> String {
    "O Claude Code não foi encontrado neste computador. Instale-o (https://claude.com/claude-code), \
     faça login rodando `claude` uma vez no terminal e tente de novo. Se ele estiver numa pasta fora \
     do PATH, informe o caminho em Configurações."
        .to_string()
}

pub struct Answer {
    /// The JSON object Claude answered with.
    pub json: Json,
    pub cost_usd: Option<f64>,
}

/// Run Claude Code in `cwd` with `prompt` (sent through stdin) and get the JSON object it
/// answers with. `schema` asks the CLI for a validated answer (`--json-schema`); without
/// support for it, the JSON is read from the text of the answer.
pub fn run(exe: &Path, cwd: &Path, prompt: &str, schema: &Json, cancel: Option<&AtomicBool>) -> Result<Answer> {
    let schema_text = schema.to_string();
    let attempt = |with_schema: bool| -> Result<process::Output> {
        let mut cmd = process::command(exe);
        cmd.current_dir(cwd)
            .arg("-p")
            .arg("Siga as instruções recebidas pela entrada padrão e responda somente com o JSON pedido.")
            .args(["--output-format", "json", "--permission-mode", "dontAsk", "--max-turns", MAX_TURNS])
            .args(["--allowedTools", ALLOWED_TOOLS, "--disallowedTools", DISALLOWED_TOOLS]);
        if with_schema {
            cmd.args(["--json-schema", &schema_text]);
        }
        process::run(cmd, Some(prompt.as_bytes()), cancel, TIMEOUT)
    };
    let mut out = match attempt(true) {
        // A .cmd launcher cannot always receive the schema as an argument.
        Err(Error::Validation(m)) if m.starts_with("Não foi possível executar") => attempt(false)?,
        other => other?,
    };
    if !out.success && mentions_unknown_schema_flag(&out) {
        out = attempt(false)?;
    }
    parse_output(&out)
}

fn mentions_unknown_schema_flag(out: &process::Output) -> bool {
    let text = format!("{}{}", out.stderr, out.stdout).to_lowercase();
    text.contains("json-schema") && (text.contains("unknown") || text.contains("unrecognized"))
}

fn parse_output(out: &process::Output) -> Result<Answer> {
    let envelope: Option<Json> = serde_json::from_str(out.stdout.trim()).ok().or_else(|| {
        // Some versions print other lines before the result: take the last JSON line.
        out.stdout.lines().rev().find_map(|l| serde_json::from_str::<Json>(l.trim()).ok())
    });
    let Some(envelope) = envelope.filter(Json::is_object) else {
        let detail = if out.stderr.trim().is_empty() { out.stdout.trim() } else { out.stderr.trim() };
        return Err(Error::validation(format!(
            "O Claude Code não respondeu como esperado{}:\n{}",
            out.code.map(|c| format!(" (código {c})")).unwrap_or_default(),
            truncate(detail, 1200)
        )));
    };
    let result_text = envelope.get("result").and_then(Json::as_str).unwrap_or("").to_string();
    let is_error = envelope.get("is_error").and_then(Json::as_bool).unwrap_or(false);
    if is_error || !out.success {
        let detail = if result_text.trim().is_empty() { out.stderr.trim().to_string() } else { result_text };
        return Err(Error::validation(format!("O Claude Code devolveu um erro:\n{}", truncate(&detail, 1200))));
    }
    let cost_usd = envelope.get("total_cost_usd").and_then(Json::as_f64);
    let json = match envelope.get("structured_output").filter(|v| v.is_object()) {
        Some(v) => v.clone(),
        None => extract_json(&result_text).ok_or_else(|| {
            Error::validation(format!(
                "A resposta do Claude não trouxe o JSON pedido:\n{}",
                truncate(&result_text, 1200)
            ))
        })?,
    };
    Ok(Answer { json, cost_usd })
}

/// The JSON object inside a text answer: a ```json block, or from the first `{` to the last `}`.
pub fn extract_json(text: &str) -> Option<Json> {
    if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        if let Some(end) = rest.find("```") {
            if let Ok(v) = serde_json::from_str::<Json>(rest[..end].trim()) {
                return v.is_object().then_some(v);
            }
        }
    }
    let (start, end) = (text.find('{')?, text.rfind('}')?);
    if end <= start {
        return None;
    }
    serde_json::from_str::<Json>(&text[start..=end]).ok().filter(Json::is_object)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut t: String = text.chars().take(max).collect();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stdout: &str) -> process::Output {
        process::Output { success: true, code: Some(0), stdout: stdout.into(), stderr: String::new() }
    }

    #[test]
    fn json_from_text() {
        assert_eq!(extract_json("ok\n```json\n{\"a\": 1}\n```\n").unwrap()["a"], 1);
        assert_eq!(extract_json("Resposta: {\"a\": {\"b\": 2}} fim").unwrap()["a"]["b"], 2);
        assert!(extract_json("sem json").is_none());
    }

    #[test]
    fn envelope() {
        let a = parse_output(&output(r#"{"type":"result","is_error":false,"result":"{\"resumo\":\"x\"}","total_cost_usd":0.5}"#)).unwrap();
        assert_eq!(a.json["resumo"], "x");
        assert_eq!(a.cost_usd, Some(0.5));
        let a = parse_output(&output(r#"{"result":"","structured_output":{"resumo":"y"}}"#)).unwrap();
        assert_eq!(a.json["resumo"], "y");
        let e = parse_output(&output(r#"{"is_error":true,"result":"Invalid API key"}"#)).err().unwrap();
        assert!(e.to_string().contains("Invalid API key"));
        assert!(parse_output(&output("not json")).is_err());
    }
}
