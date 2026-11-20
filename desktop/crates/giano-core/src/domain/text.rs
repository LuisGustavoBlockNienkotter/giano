//! Text normalization: project codes, tags and Markdown bodies.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::error::{Error, Result};

/// A regex compiled once and kept in `cell`.
pub(super) fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

pub fn normalize_code(code: &str) -> Result<String> {
    static CODE_RE: OnceLock<Regex> = OnceLock::new();
    let code = code.trim().to_uppercase();
    if !re(&CODE_RE, r"^[A-Z][A-Z0-9]{0,9}$").is_match(&code) {
        return Err(Error::validation(
            "Sigla inválida: use de 1 a 10 letras/números, começando por letra (ex.: MERC, P2K).",
        ));
    }
    Ok(code)
}

pub fn normalize_tag(tag: &str) -> String {
    static WS_RE: OnceLock<Regex> = OnceLock::new();
    re(&WS_RE, r"\s+").replace_all(&tag.trim().to_lowercase(), "-").into_owned()
}

/// Normalize, de-duplicate and sort tags.
pub fn normalize_tags<S: AsRef<str>>(tags: &[S]) -> Vec<String> {
    let set: BTreeSet<String> =
        tags.iter().map(|t| normalize_tag(t.as_ref())).filter(|t| !t.is_empty()).collect();
    set.into_iter().collect()
}

/// Tags typed as one comma-separated string.
pub fn split_tags(text: &str) -> Vec<String> {
    normalize_tags(&text.split(',').collect::<Vec<_>>())
}

/// Trailing whitespace is not a real change: strip it (and normalize newlines).
pub fn normalize_body(body: &str) -> String {
    body.replace("\r\n", "\n").trim_end().to_string()
}

pub(super) fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_tags() {
        assert_eq!(normalize_code(" p2k ").unwrap(), "P2K");
        assert!(normalize_code("1ABC").is_err());
        assert!(normalize_code("ABCDEFGHIJK").is_err());
        assert_eq!(split_tags(" UX , api,ux, Nova Tag,"), vec!["api", "nova-tag", "ux"]);
    }
}
