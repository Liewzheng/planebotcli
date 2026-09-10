//! Plain-text → HTML conversion for comment and document bodies.
//!
//! The Plane API stores HTML verbatim and does not parse markdown or auto-link
//! URLs — only the web editor does — so text written via the CLI must arrive
//! already converted. Ported from `src/planecli/utils/body_html.py`.

use regex::Regex;

const URL_TRAIL: &str = ".,;:!?)]}。，；：！？）】、";

fn url_re() -> Regex {
    Regex::new(r"(https?://[A-Za-z0-9\-._~:/?#\[\]@!$&()*+,;=%]+)").unwrap()
}

/// Turn bare http(s) URLs into anchors. URLs already inside an href attribute
/// or a tag (preceded by a quote, angle bracket, or equals sign) are left
/// alone — the Rust `regex` crate has no look-behind, so the guard is applied
/// per match in code.
pub fn linkify(text: &str) -> String {
    let re = url_re();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in re.captures_iter(text) {
        let m = caps.get(1).expect("url group always present");
        let inside_attr = m
            .start()
            .checked_sub(1)
            .and_then(|i| text[..=i].chars().next_back())
            .is_some_and(|c| matches!(c, '"' | '\'' | '>' | '='));
        if inside_attr {
            continue;
        }
        out.push_str(&text[last..m.start()]);
        let full = m.as_str();
        let url: String = full
            .trim_end_matches(|c: char| URL_TRAIL.contains(c))
            .to_string();
        let trail = &full[url.len()..];
        out.push_str(&format!("<a href=\"{url}\">{url}</a>{trail}"));
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

/// Convert `` `inline code` `` to a `<code>` tag with HTML-escaped content.
pub fn inline_code(text: &str) -> String {
    let re = Regex::new(r"`([^`\n]+)`").unwrap();
    re.replace_all(text, |caps: &regex::Captures<'_>| {
        format!("<code>{}</code>", html_escape::encode_text(&caps[1]))
    })
    .to_string()
}

/// Pull fenced code blocks out of the text, replacing each with a placeholder
/// token and returning the list of pre-wrapped HTML fragments to restore.
/// Extracting first keeps block content out of linkify and the br pass.
fn extract_code_blocks(text: &str) -> (String, Vec<String>) {
    let re = Regex::new(r"(?s)```[ \t]*\w*[ \t]*\n(.*?)```").unwrap();
    let mut blocks: Vec<String> = Vec::new();
    let held = re.replace_all(text, |caps: &regex::Captures<'_>| {
        let code = html_escape::encode_text(caps[1].trim_end_matches('\n'));
        blocks.push(format!("<pre><code>{code}</code></pre>"));
        format!("\u{0}{}\u{0}", blocks.len() - 1)
    });
    (held.to_string(), blocks)
}

/// Convert plain text to HTML paragraphs: blank lines separate paragraphs, a
/// single newline becomes a `<br/>`, backticks become code tags, fenced blocks
/// become pre-wrapped code, and bare URLs become anchors.
pub fn body_to_html(body: &str) -> String {
    let (text, blocks) = extract_code_blocks(body.trim());
    let token_only = Regex::new(r"^\u{0}\d+\u{0}[ \t]*$").unwrap();
    let mut parts: Vec<String> = Vec::new();
    for raw in text.split("\n\n") {
        let para = raw.trim();
        if para.is_empty() {
            continue;
        }
        let converted = linkify(&inline_code(para)).replace('\n', "<br/>");
        if token_only.is_match(para) {
            parts.push(converted);
        } else {
            parts.push(format!("<p>{converted}</p>"));
        }
    }
    let mut result = parts.concat();
    for (i, fragment) in blocks.iter().enumerate() {
        result = result.replace(&format!("\u{0}{i}\u{0}"), fragment);
    }
    result
}

/// Apply the inline markdown subset to one block of user text.
///
/// The text is HTML-escaped first (quotes included, like Python
/// `html.escape`), so user text renders as-is and only the generated tags pass
/// through unescaped. Code spans are recognized on the escaped text, so their
/// content is escaped exactly once; markup inside code spans is further
/// processed, mirroring the Python pass order.
fn md_inline(text: &str) -> String {
    let escaped = html_escape::encode_quoted_attribute(text);
    let code = Regex::new(r"`([^`\n]+)`").unwrap();
    let with_code = code.replace_all(&escaped, "<code>$1</code>");
    let bold = Regex::new(r"\*\*(.+?)\*\*|__(.+?)__").unwrap();
    let with_bold = bold.replace_all(&with_code, |caps: &regex::Captures<'_>| {
        let content = caps.get(1).or_else(|| caps.get(2)).expect("branch matched");
        format!("<strong>{}</strong>", content.as_str())
    });
    let with_em_star = em_star(&with_bold);
    let with_em_under = em_under(&with_em_star);
    linkify(&with_em_under)
}

/// Emulate Python's `(?<!\*)\*([^*\n]+)\*(?!\*)` — the `regex` crate has no
/// look-around, so matches of the bare pattern are guard-checked per match.
fn em_star(text: &str) -> String {
    let re = Regex::new(r"\*([^*\n]+)\*").unwrap();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in re.captures_iter(text) {
        let m = caps.get(0).expect("group 0 always present");
        let open_ok = text[..m.start()]
            .chars()
            .next_back()
            .is_none_or(|c| c != '*');
        let close_ok = text[m.end()..].chars().next().is_none_or(|c| c != '*');
        if !(open_ok && close_ok) {
            continue;
        }
        out.push_str(&text[last..m.start()]);
        out.push_str("<em>");
        out.push_str(&caps[1]);
        out.push_str("</em>");
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

/// Emulate Python's `(?<![\w_])_([^_\n]+)_(?![\w_])` (word-boundary aware).
fn em_under(text: &str) -> String {
    let re = Regex::new(r"_([^_\n]+)_").unwrap();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in re.captures_iter(text) {
        let m = caps.get(0).expect("group 0 always present");
        let open_ok = text[..m.start()]
            .chars()
            .next_back()
            .is_none_or(|c| !md_word_char(c));
        let close_ok = text[m.end()..]
            .chars()
            .next()
            .is_none_or(|c| !md_word_char(c));
        if !(open_ok && close_ok) {
            continue;
        }
        out.push_str(&text[last..m.start()]);
        out.push_str("<em>");
        out.push_str(&caps[1]);
        out.push_str("</em>");
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

/// `\w` for the em-underscore boundary guard (letters, digits, underscore).
fn md_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Render consecutive list-item lines as ul/ol (Python `_md_list`): a
/// non-item line joins the previous item (lazy continuation) and a
/// bullet-style change starts a new list.
fn md_list(lines: &[&str]) -> String {
    let ul = Regex::new(r"^[-*+][ \t]+(.*)$").unwrap();
    let ol = Regex::new(r"^\d{1,9}[.)][ \t]+(.*)$").unwrap();
    let mut out: Vec<String> = Vec::new();
    let mut tag: Option<&str> = None;
    let mut items: Vec<String> = Vec::new();
    for line in lines {
        let stripped = line.trim();
        let matched = ul
            .captures(stripped)
            .or_else(|| ol.captures(stripped))
            .map(|c| c.get(1).expect("content group").as_str());
        match matched {
            None => {
                if !items.is_empty() && !stripped.is_empty() {
                    let last = items.last_mut().expect("non-empty");
                    last.push(' ');
                    last.push_str(stripped);
                }
            }
            Some(content) => {
                let kind = if stripped.starts_with(['-', '*', '+']) {
                    "ul"
                } else {
                    "ol"
                };
                if kind != tag.unwrap_or_default() {
                    flush(&mut out, &mut tag, &mut items);
                    tag = Some(kind);
                }
                items.push(content.to_string());
            }
        }
    }
    flush(&mut out, &mut tag, &mut items);
    out.concat()
}

fn flush(out: &mut Vec<String>, tag: &mut Option<&str>, items: &mut Vec<String>) {
    if let Some(tag) = tag.take() {
        let lis = items
            .iter()
            .map(|item| format!("<li>{}</li>", md_inline(item)))
            .collect::<String>();
        out.push(format!("<{tag}>{lis}</{tag}>"));
        items.clear();
    }
}

/// Convert a markdown subset to the HTML Plane's editor stores.
///
/// Blocks are recognized line by line, so a heading may be followed by a list
/// (or a paragraph by a table) without an intervening blank line:
///
/// * ATX headings (one to six hash marks, closing hashes stripped)
/// * unordered lists (dash, star, or plus bullets) and ordered lists
///   (number-dot or number-paren)
/// * fenced code blocks (backtick or tilde fences, pre-wrapped code)
/// * blockquotes (`>`), horizontal rules, and GitHub-style tables
/// * paragraphs, where a single newline becomes `<br/>`
///
/// Inline: code spans, bold/italic, and bare http(s) URLs (linkified). All
/// user text is HTML-escaped before markup is applied — only the generated
/// tags pass through unescaped.
pub fn md_to_html(md: &str) -> String {
    let lines: Vec<&str> = md.trim().lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        // Fenced code block.
        if let Some(fence) = md_fence(trimmed) {
            let mut code: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !md_fence_close(lines[i].trim_start(), fence) {
                code.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // consume the closing fence
            }
            let joined = code.join("\n");
            let escaped = html_escape::encode_quoted_attribute(joined.trim_matches('\n'));
            out.push_str(&format!("<pre><code>{escaped}</code></pre>"));
            continue;
        }
        // Horizontal rule.
        if md_hrule(trimmed) {
            out.push_str("<hr/>");
            i += 1;
            continue;
        }
        // ATX heading (closing hashes stripped).
        if let Some((level, content)) = md_heading(trimmed) {
            out.push_str(&format!("<h{level}>{}</h{level}>", md_inline(&content)));
            i += 1;
            continue;
        }
        // Blockquote: consecutive `>` lines become one quoted paragraph.
        if trimmed.starts_with('>') {
            let mut quoted: Vec<String> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let body = lines[i].trim_start().strip_prefix('>').unwrap_or("");
                quoted.push(body.strip_prefix(' ').unwrap_or(body).to_string());
                i += 1;
            }
            let inner = md_inline(&quoted.join("\n")).replace('\n', "<br/>");
            out.push_str(&format!("<blockquote><p>{inner}</p></blockquote>"));
            continue;
        }
        // GitHub-style table: a header row followed by a `|---|` separator.
        if trimmed.starts_with('|') && i + 1 < lines.len() && md_table_sep(lines[i + 1].trim()) {
            let header = md_table_cells(trimmed);
            let mut rows = String::from("<thead><tr>");
            for cell in &header {
                rows.push_str(&format!("<th>{}</th>", md_inline(cell)));
            }
            rows.push_str("</tr></thead><tbody>");
            i += 2;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let cells = md_table_cells(lines[i].trim());
                rows.push_str("<tr>");
                for cell in &cells {
                    rows.push_str(&format!("<td>{}</td>", md_inline(cell)));
                }
                rows.push_str("</tr>");
                i += 1;
            }
            rows.push_str("</tbody>");
            out.push_str(&format!("<table>{rows}</table>"));
            continue;
        }
        // List: consecutive item lines (indented lines continue the block).
        if md_list_item(trimmed) {
            let mut block: Vec<&str> = Vec::new();
            while i < lines.len() {
                let line = lines[i];
                let lt = line.trim();
                if lt.is_empty() || (!md_list_item(lt) && !line.starts_with([' ', '\t'])) {
                    break;
                }
                block.push(line);
                i += 1;
            }
            out.push_str(&md_list(&block));
            continue;
        }
        // Paragraph: run of lines up to the next blank line or block start.
        let mut para: Vec<&str> = Vec::new();
        while i < lines.len() {
            let line = lines[i];
            let lt = line.trim();
            let starts_table =
                lt.starts_with('|') && i + 1 < lines.len() && md_table_sep(lines[i + 1].trim());
            if lt.is_empty()
                || md_fence(lt).is_some()
                || md_hrule(lt)
                || md_heading(lt).is_some()
                || lt.starts_with('>')
                || md_list_item(lt)
                || starts_table
            {
                break;
            }
            para.push(line);
            i += 1;
        }
        let inner = md_inline(&para.join("\n")).replace('\n', "<br/>");
        out.push_str(&format!("<p>{inner}</p>"));
    }
    out
}

/// Fence character of an opening code fence (backtick or tilde, three or more).
fn md_fence(line: &str) -> Option<char> {
    let c = line.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    (line.chars().take_while(|&x| x == c).count() >= 3).then_some(c)
}

/// Whether a line closes a fence opened with `fence` (only fence chars/spaces).
fn md_fence_close(line: &str, fence: char) -> bool {
    let count = line.chars().take_while(|&x| x == fence).count();
    count >= 3 && line.chars().all(|x| x == fence || x == ' ' || x == '\t')
}

/// A thematic break: three or more `-`, `*`, or `_` (spaces allowed).
fn md_hrule(line: &str) -> bool {
    let t = line.trim();
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, '-' | '*' | '_') {
        return false;
    }
    t.chars().filter(|&c| c != ' ' && c != '\t').count() >= 3
        && t.chars().all(|c| c == first || c == ' ' || c == '\t')
}

/// Parse an ATX heading into its level and inline content. Closing hashes are
/// stripped only when preceded by whitespace.
fn md_heading(line: &str) -> Option<(usize, String)> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let mut content = rest.trim().to_string();
    if content.ends_with('#') {
        let stripped = content.trim_end_matches('#');
        if stripped.ends_with([' ', '\t']) {
            content = stripped.trim_end().to_string();
        }
    }
    Some((hashes, content))
}

/// Whether a line starts a list item (unordered or ordered).
fn md_list_item(line: &str) -> bool {
    md_ul_content(line).is_some() || md_ol_content(line).is_some()
}

fn md_ul_content(line: &str) -> Option<&str> {
    ['-', '*', '+'].iter().find_map(|&marker| {
        line.strip_prefix(marker)
            .filter(|r| r.starts_with(' ') || r.starts_with('\t'))
    })
}

fn md_ol_content(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    ['.', ')'].iter().find_map(|&sep| {
        line[digits..]
            .strip_prefix(sep)
            .filter(|r| r.starts_with(' ') || r.starts_with('\t'))
    })
}

/// Whether a line is a table separator (`|---|:--:|`, a single dash per cell is
/// enough).
fn md_table_sep(line: &str) -> bool {
    line.contains('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// Split a table row into trimmed cells (outer pipes ignored).
fn md_table_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

/// Strip HTML tags from a fragment, leaving text (no entity decoding) —
/// matches the Python `description_stripped` derivation.
pub fn strip_html_tags(input: &str) -> String {
    let tag = Regex::new(r"<[^>]+>").unwrap();
    tag.replace_all(input, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_paragraphs_and_br() {
        let html = body_to_html("line one\nline two\n\nsecond para");
        assert_eq!(html, "<p>line one<br/>line two</p><p>second para</p>");
    }

    #[test]
    fn converts_inline_code_with_escaping() {
        let html = body_to_html("use `x < y` here");
        assert_eq!(html, "<p>use <code>x &lt; y</code> here</p>");
    }

    #[test]
    fn linkifies_bare_urls() {
        let html = body_to_html("see https://example.com/a now");
        assert_eq!(
            html,
            "<p>see <a href=\"https://example.com/a\">https://example.com/a</a> now</p>"
        );
    }

    #[test]
    fn keeps_trailing_punctuation_out_of_link() {
        let html = body_to_html("see https://example.com/a.");
        assert!(html.contains("</a>."));
    }

    #[test]
    fn fences_block_are_pre_wrapped() {
        let html = body_to_html("before\n\n```\nif a < b\n```\n\nafter");
        assert!(html.contains("<pre><code>if a &lt; b</code></pre>"));
        assert!(html.starts_with("<p>before</p>"));
        assert!(html.ends_with("<p>after</p>"));
    }

    #[test]
    fn url_inside_code_not_linkified() {
        let html = body_to_html("`https://example.com`");
        assert!(!html.contains("<a href"));
        assert!(html.contains("<code>https://example.com</code>"));
    }

    #[test]
    fn md_escapes_user_text_in_heading() {
        assert_eq!(
            md_to_html("# <script>alert(1)</script>"),
            "<h1>&lt;script&gt;alert(1)&lt;/script&gt;</h1>"
        );
    }

    #[test]
    fn md_escapes_user_text_in_paragraph() {
        assert_eq!(
            md_to_html("a <b href=\"x\">y</b>"),
            "<p>a &lt;b href=&quot;x&quot;&gt;y&lt;/b&gt;</p>"
        );
    }

    #[test]
    fn md_headings_all_levels() {
        assert_eq!(md_to_html("# one"), "<h1>one</h1>");
        assert_eq!(md_to_html("###### six"), "<h6>six</h6>");
    }

    #[test]
    fn md_heading_strips_closing_hashes() {
        assert_eq!(md_to_html("## Title ##"), "<h2>Title</h2>");
    }

    #[test]
    fn md_plain_hash_line_is_a_paragraph() {
        assert_eq!(md_to_html("#nospace"), "<p>#nospace</p>");
    }

    #[test]
    fn md_blank_lines_split_paragraphs() {
        assert_eq!(md_to_html("one\n\n\ntwo"), "<p>one</p><p>two</p>");
    }

    #[test]
    fn md_converts_single_newline_to_br() {
        assert_eq!(md_to_html("a\nb\n\nc"), "<p>a<br/>b</p><p>c</p>");
    }

    #[test]
    fn md_unordered_list_bullets() {
        assert_eq!(md_to_html("- a\n- b"), "<ul><li>a</li><li>b</li></ul>");
        assert_eq!(md_to_html("* a\n+ b"), "<ul><li>a</li><li>b</li></ul>");
    }

    #[test]
    fn md_ordered_list() {
        assert_eq!(
            md_to_html("1. first\n2. second"),
            "<ol><li>first</li><li>second</li></ol>"
        );
    }

    #[test]
    fn md_bullet_change_starts_new_list() {
        assert_eq!(
            md_to_html("- a\n\n1. b"),
            "<ul><li>a</li></ul><ol><li>b</li></ol>"
        );
    }

    #[test]
    fn md_list_item_content_gets_inline_markup() {
        assert_eq!(
            md_to_html("- `x` and https://a/1"),
            "<ul><li><code>x</code> and <a href=\"https://a/1\">https://a/1</a></li></ul>"
        );
    }

    #[test]
    fn md_fenced_block_becomes_pre_code() {
        assert_eq!(
            md_to_html("before\n\n```py\nprint('<x>')\n```\n\nafter"),
            "<p>before</p><pre><code>print(&#x27;&lt;x&gt;&#x27;)</code></pre><p>after</p>"
        );
    }

    #[test]
    fn md_inline_code_is_escaped_and_not_linkified() {
        assert_eq!(
            md_to_html("use `https://x.io <b>` here"),
            "<p>use <code>https://x.io &lt;b&gt;</code> here</p>"
        );
    }

    #[test]
    fn md_linkifies_bare_url() {
        assert_eq!(
            md_to_html("see https://example.com/a, ok"),
            "<p>see <a href=\"https://example.com/a\">https://example.com/a</a>, ok</p>"
        );
    }

    #[test]
    fn md_bold_and_italic() {
        assert_eq!(
            md_to_html("**bold** and *italic*"),
            "<p><strong>bold</strong> and <em>italic</em></p>"
        );
        assert_eq!(
            md_to_html("__bold__ and _italic_"),
            "<p><strong>bold</strong> and <em>italic</em></p>"
        );
    }

    #[test]
    fn md_underscore_inside_word_is_not_italic() {
        assert_eq!(md_to_html("snake_case_name"), "<p>snake_case_name</p>");
    }

    #[test]
    fn md_bold_content_keeps_inner_markup_literal() {
        // A single bold pass: inner `__`/`**` is not re-processed as bold.
        assert_eq!(md_to_html("**__a__**"), "<p><strong>__a__</strong></p>");
        assert_eq!(md_to_html("__a**b__"), "<p><strong>a**b</strong></p>");
    }

    #[test]
    fn md_heading_then_list_without_blank_line() {
        assert_eq!(
            md_to_html("## Lists\n- a\n- b"),
            "<h2>Lists</h2><ul><li>a</li><li>b</li></ul>"
        );
    }

    #[test]
    fn md_paragraph_then_heading_without_blank_line() {
        assert_eq!(md_to_html("intro\n# Title"), "<p>intro</p><h1>Title</h1>");
    }

    #[test]
    fn md_table() {
        assert_eq!(
            md_to_html("| a | b |\n|---|---|\n| 1 | 2 |"),
            "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>"
        );
    }

    #[test]
    fn md_table_after_paragraph() {
        assert_eq!(
            md_to_html("text\n\n| h |\n| - |\n| v |"),
            "<p>text</p><table><thead><tr><th>h</th></tr></thead><tbody><tr><td>v</td></tr></tbody></table>"
        );
    }

    #[test]
    fn md_blockquote() {
        assert_eq!(
            md_to_html("> quoted\n> more"),
            "<blockquote><p>quoted<br/>more</p></blockquote>"
        );
    }

    #[test]
    fn md_horizontal_rule() {
        assert_eq!(md_to_html("a\n\n---\n\nb"), "<p>a</p><hr/><p>b</p>");
    }

    #[test]
    fn md_tilde_fence() {
        assert_eq!(
            md_to_html("~~~\nif a < b\n~~~"),
            "<pre><code>if a &lt; b</code></pre>"
        );
    }

    #[test]
    fn md_pipe_line_without_separator_is_paragraph() {
        assert_eq!(md_to_html("| just | text |"), "<p>| just | text |</p>");
    }
}
