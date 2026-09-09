"""Convert plain user-authored text to the HTML Plane's editor stores.

The Plane API stores HTML verbatim and does not parse markdown or auto-link
URLs — only the web editor does — so text written via the CLI must arrive
already converted. Shared by comment and document write commands.
"""

from __future__ import annotations

import html
import re

_URL_RE = re.compile(r"(?<![\"'>=])(https?://[A-Za-z0-9\-._~:/?#\[\]@!$&()*+,;=%]+)")
# Trailing punctuation that belongs to the sentence, not the URL.
_URL_TRAIL = ".,;:!?)]}。，；：！？）】、"


def _linkify(text: str) -> str:
    """Turn bare http(s) URLs into anchors.

    URLs already inside an href attribute are left alone (the regex excludes
    matches preceded by a quote or angle bracket from an attribute context).
    """

    def _sub(m: re.Match[str]) -> str:
        url = m.group(1).rstrip(_URL_TRAIL)
        trail = m.group(1)[len(url) :]
        return f'<a href="{url}">{url}</a>{trail}'

    return _URL_RE.sub(_sub, text)


def _inline_code(text: str) -> str:
    """Convert `inline code` to a code tag with HTML-escaped content."""
    return re.sub(
        r"`([^`\n]+)`",
        lambda m: f"<code>{html.escape(m.group(1))}</code>",
        text,
    )


def _extract_code_blocks(text: str) -> tuple[str, list[str]]:
    """Pull fenced code blocks out of the text, replacing each with a token.

    Returns the text with \x00N\x00 placeholders and the list of HTML
    fragments (pre-wrapped escaped code) to restore at the end. Extracting
    first keeps their content out of linkify and the newline-to-br pass.
    """
    blocks: list[str] = []

    def _hold(match: re.Match[str]) -> str:
        code = html.escape(match.group(1).strip("\n"))
        blocks.append(f"<pre><code>{code}</code></pre>")
        return f"\x00{len(blocks) - 1}\x00"

    text = re.sub(
        r"```[ \t]*\w*[ \t]*\n(.*?)```",
        _hold,
        text,
        flags=re.DOTALL,
    )
    return text, blocks


_HEADING_RE = re.compile(r"^(#{1,6})[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$")
_UL_ITEM_RE = re.compile(r"^[-*+][ \t]+(.*)$")
_OL_ITEM_RE = re.compile(r"^\d{1,9}[.)][ \t]+(.*)$")
_CODE_SPAN_RE = re.compile(r"`([^`\n]+)`")
_BOLD_RE = re.compile(r"(\*\*|__)(.+?)\1")
_EM_STAR_RE = re.compile(r"(?<!\*)\*([^*\n]+)\*(?!\*)")
_EM_UNDER_RE = re.compile(r"(?<![\w_])_([^_\n]+)_(?![\w_])")
_TOKEN_ONLY_RE = re.compile(r"(?:\x00\d+\x00[ \t]*)+")


def _md_inline(text: str) -> str:
    """Apply the inline markdown subset to one block of user text.

    Everything is HTML-escaped first, so user text (e.g. a script tag written
    literally) renders as-is and only the generated tags pass through
    unescaped. Code spans are recognized on the escaped text, so their
    content is escaped exactly once.
    """
    text = html.escape(text)
    text = _CODE_SPAN_RE.sub(r"<code>\1</code>", text)
    text = _BOLD_RE.sub(r"<strong>\2</strong>", text)
    text = _EM_STAR_RE.sub(r"<em>\1</em>", text)
    text = _EM_UNDER_RE.sub(r"<em>\1</em>", text)
    return _linkify(text)


def _md_list(lines: list[str]) -> str:
    """Render consecutive list-item lines as ul/ol. A non-item line joins the
    previous item (lazy continuation); a bullet-style change starts a new list."""
    out: list[str] = []
    tag: str | None = None
    items: list[str] = []

    def flush() -> None:
        nonlocal tag, items
        if tag is not None:
            lis = "".join(f"<li>{_md_inline(item)}</li>" for item in items)
            out.append(f"<{tag}>{lis}</{tag}>")
            tag = None
            items = []

    for line in lines:
        stripped = line.strip()
        match = _UL_ITEM_RE.match(stripped) or _OL_ITEM_RE.match(stripped)
        if match is None:
            if items and stripped:
                items[-1] += " " + stripped
            continue
        kind = "ul" if stripped[:1] in "-*+" else "ol"
        if kind != tag:
            flush()
            tag = kind
        items.append(match.group(1))
    flush()
    return "".join(out)


def md_to_html(md: str) -> str:
    """Convert a markdown subset to the HTML Plane's editor stores.

    Supported: ATX headings (one to six hash marks), unordered lists (dash,
    star, or plus bullets), ordered lists (number-dot), paragraphs separated
    by blank lines, inline code spans, fenced code blocks (pre-wrapped code),
    bold/italic, and bare http(s) URLs (linkified). All user text is
    HTML-escaped before markup is applied — only the generated tags are
    unescaped.
    """
    text, blocks = _extract_code_blocks(md.strip())
    parts: list[str] = []
    for chunk in re.split(r"\n\s*\n", text):
        chunk = chunk.strip()
        if not chunk:
            continue
        lines = chunk.split("\n")
        first = lines[0].strip()
        if _UL_ITEM_RE.match(first) or _OL_ITEM_RE.match(first):
            parts.append(_md_list(lines))
            continue
        heading = _HEADING_RE.match(first) if len(lines) == 1 else None
        if heading:
            level = len(heading.group(1))
            parts.append(f"<h{level}>{_md_inline(heading.group(2))}</h{level}>")
            continue
        converted = _md_inline(chunk).replace(chr(10), "<br/>")
        if _TOKEN_ONLY_RE.fullmatch(chunk):
            # A chunk that is only a code block keeps its pre wrapper.
            parts.append(converted)
        else:
            parts.append(f"<p>{converted}</p>")
    result = "".join(parts)
    for i, fragment in enumerate(blocks):
        result = result.replace(f"\x00{i}\x00", fragment)
    return result


def body_to_html(body: str) -> str:
    """Convert plain text to HTML paragraphs.

    Blank lines separate paragraphs; a single newline becomes a br tag —
    the editor collapses whitespace inside a paragraph, so unconverted
    newlines would render as one long line. Backticks become code tags and
    fenced blocks become pre-wrapped code (the editor stores HTML, it does
    not parse markdown).
    """
    text, blocks = _extract_code_blocks(body.strip())
    parts: list[str] = []
    for p in re.split(r"\n\s*\n", text):
        p = p.strip()
        if not p:
            continue
        converted = _linkify(_inline_code(p)).replace(chr(10), "<br/>")
        if re.fullmatch(r"(?:\x00\d+\x00[ \t]*)+", p):
            # A paragraph that is only a code block keeps its pre wrapper.
            parts.append(converted)
        else:
            parts.append(f"<p>{converted}</p>")
    result = "".join(parts)
    for i, fragment in enumerate(blocks):
        result = result.replace(f"\x00{i}\x00", fragment)
    return result
