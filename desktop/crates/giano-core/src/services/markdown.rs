//! Markdown helpers: HTML rendering (with syntect highlighting) and heading demotion.
//!
//! Raw HTML in the source is escaped, never passed through: the output is safe to share and
//! to show in the app.

use std::sync::OnceLock;

use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use syntect::highlighting::ThemeSet;
use syntect::html::{css_for_theme_with_class_style, ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Highlighted tokens get classes like `hl-keyword` (styled by [`highlight_css`]).
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

fn highlight(code: &str, lang: &str) -> String {
    let lang = lang.split_whitespace().next().unwrap_or("");
    let ss = syntaxes();
    let syntax = if lang.is_empty() { None } else { ss.find_syntax_by_token(lang) };
    let inner = match syntax {
        Some(syntax) => {
            let mut gen = ClassedHTMLGenerator::new_with_class_style(syntax, ss, CLASS_STYLE);
            let ok = LinesWithEndings::from(code)
                .all(|line| gen.parse_html_for_line_which_includes_newline(line).is_ok());
            if ok {
                gen.finalize()
            } else {
                escape_html(code)
            }
        }
        None => escape_html(code),
    };
    format!("<pre class=\"highlight\"><code>{inner}</code></pre>\n")
}

/// Render Markdown to HTML (CommonMark + tables + strikethrough).
pub fn to_html(text: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut events: Vec<Event> = Vec::new();
    let mut code: Option<(String, String)> = None; // (language, text) of the open code block
    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang, text)) = code.take() {
                    events.push(Event::Html(highlight(&text, &lang).into()));
                }
            }
            Event::Text(t) if code.is_some() => {
                if let Some((_, text)) = code.as_mut() {
                    text.push_str(&t);
                }
            }
            // Raw HTML is shown as text (escaped by the renderer).
            Event::Html(h) | Event::InlineHtml(h) => events.push(Event::Text(h)),
            other => events.push(other),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

/// CSS for the highlighted code (light theme), scoped to `selector`.
pub fn highlight_css() -> String {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        let theme = &themes.themes["InspiredGitHub"];
        css_for_theme_with_class_style(theme, CLASS_STYLE).unwrap_or_default()
    })
    .clone()
}

/// CSS for the highlighted code in a dark theme.
pub fn highlight_css_dark() -> String {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        let theme = &themes.themes["base16-ocean.dark"];
        css_for_theme_with_class_style(theme, CLASS_STYLE).unwrap_or_default()
    })
    .clone()
}

/// Push ATX headings down `levels` levels (max 6), ignoring fenced code. Used when embedding
/// a Markdown body under a heading of the export.
pub fn demote_headings(text: &str, levels: usize) -> String {
    static FENCE_RE: OnceLock<Regex> = OnceLock::new();
    static HEADING_RE: OnceLock<Regex> = OnceLock::new();
    let fence_re = FENCE_RE.get_or_init(|| Regex::new(r"^\s{0,3}(`{3,}|~{3,})").expect("regex"));
    let heading_re = HEADING_RE.get_or_init(|| Regex::new(r"^(\s{0,3})(#{1,6})(\s|$)").expect("regex"));
    let mut out = Vec::new();
    let mut fence: Option<char> = None;
    for line in text.lines() {
        if let Some(m) = fence_re.captures(line) {
            let marker = m[1].chars().next().unwrap_or('`');
            match fence {
                None => fence = Some(marker),
                Some(f) if f == marker => fence = None,
                _ => {}
            }
            out.push(line.to_string());
            continue;
        }
        if fence.is_none() {
            if let Some(h) = heading_re.captures(line) {
                let hashes = h.get(2).expect("group");
                let n = (hashes.as_str().len() + levels).min(6);
                out.push(format!("{}{}{}", &h[1], "#".repeat(n), &line[hashes.end()..]));
                continue;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demote_skips_code() {
        let src = "# A\ntexto\n```\n# comentário\n```\n###### F";
        assert_eq!(demote_headings(src, 2), "### A\ntexto\n```\n# comentário\n```\n###### F");
    }

    #[test]
    fn unknown_language_is_escaped() {
        let html = to_html("```nãoexiste\nx < y\n```");
        assert!(html.contains("x &lt; y"), "{html}");
        assert!(html.contains("<pre class=\"highlight\"><code>"));
    }

    #[test]
    fn raw_html_is_escaped() {
        let html = to_html("Política <b>de</b> devolução\n\n<script>alert(1)</script>");
        assert!(!html.contains("<b>") && !html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;b&gt;de&lt;/b&gt;"), "{html}");
    }

    #[test]
    fn python_is_highlighted() {
        let html = to_html("```python\nprint('oi')\n```");
        assert!(html.contains("class=\"hl-"), "{html}");
        assert!(html.contains("print"));
        assert!(!highlight_css().is_empty());
    }

    #[test]
    fn tables_and_headings() {
        let html = to_html("## Detalhes\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n~~velho~~");
        assert!(html.contains("<h2>Detalhes</h2>"));
        assert!(html.contains("<table>") && html.contains("<del>velho</del>"), "{html}");
    }
}
