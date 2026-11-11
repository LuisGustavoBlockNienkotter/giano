"""Markdown helpers: HTML rendering (with Pygments) and heading demotion."""

from __future__ import annotations

import re
from functools import lru_cache
from html import escape

from markdown_it import MarkdownIt
from pygments import highlight
from pygments.formatters import HtmlFormatter
from pygments.lexers import get_lexer_by_name
from pygments.util import ClassNotFound

_FORMATTER = HtmlFormatter(nowrap=True)


def _highlight(code: str, lang: str, _attrs) -> str:
    try:
        lexer = get_lexer_by_name(lang.strip()) if lang and lang.strip() else None
    except ClassNotFound:
        lexer = None
    inner = highlight(code, lexer, _FORMATTER) if lexer else escape(code)
    return f'<pre class="highlight"><code>{inner}</code></pre>\n'


@lru_cache(maxsize=1)
def _md() -> MarkdownIt:
    md = MarkdownIt("commonmark", {"html": False, "linkify": False, "highlight": _highlight})
    md.enable(["table", "strikethrough"])
    return md


def to_html(text: str) -> str:
    """Render Markdown to HTML. Raw HTML in the source is escaped (safe to share)."""
    return _md().render(text or "")


def pygments_css(selector: str = ".highlight") -> str:
    return HtmlFormatter(style="friendly").get_style_defs(selector)


_FENCE_RE = re.compile(r"^\s{0,3}(`{3,}|~{3,})")
_HEADING_RE = re.compile(r"^(\s{0,3})(#{1,6})(\s|$)")


def demote_headings(text: str, levels: int) -> str:
    """Push ATX headings down ``levels`` levels (max 6), ignoring fenced code.

    Used when embedding a Markdown body under a heading of the export.
    """
    out, fence = [], None
    for line in (text or "").splitlines():
        m = _FENCE_RE.match(line)
        if m:
            marker = m.group(1)[0]
            if fence is None:
                fence = marker
            elif fence == marker:
                fence = None
            out.append(line)
            continue
        if fence is None:
            h = _HEADING_RE.match(line)
            if h:
                n = min(6, len(h.group(2)) + levels)
                line = h.group(1) + "#" * n + line[h.end(2):]
        out.append(line)
    return "\n".join(out)
