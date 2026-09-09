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

/// Extract fenced code blocks with full HTML escaping (md flavor).
///
/// `body_to_html`'s shared helper escapes only `& < >`; Python's markdown
/// converter escapes quotes as well (`html.escape`), so the md path extracts
/// its own blocks to stay byte-identical to `body_html.py::md_to_html`.
fn md_extract_code_blocks(text: &str) -> (String, Vec<String>) {
    let re = Regex::new(r"(?s)```[ \t]*\w*[ \t]*\n(.*?)```").unwrap();
    let mut blocks: Vec<String> = Vec::new();
    let held = re.replace_all(text, |caps: &regex::Captures<'_>| {
        let code = html_escape::encode_quoted_attribute(caps[1].trim_matches('\n'));
        blocks.push(format!("<pre><code>{code}</code></pre>"));
        format!("\u{0}{}\u{0}", blocks.len() - 1)
    });
    (held.to_string(), blocks)
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
/// Supported: ATX headings (one to six hash marks, closing hashes stripped),
/// unordered lists (dash, star, or plus bullets) and ordered lists
/// (number-dot or number-paren), lists and paragraphs separated by blank
/// lines, inline code spans, fenced code blocks (pre-wrapped code),
/// bold/italic, and bare http(s) URLs (linkified). All user text is
/// HTML-escaped before markup is applied — only the generated tags pass
/// through unescaped. Mirrors `body_html.py::md_to_html`.
pub fn md_to_html(md: &str) -> String {
    let (text, blocks) = md_extract_code_blocks(md.trim());
    let split = Regex::new(r"\n\s*\n").unwrap();
    let ul = Regex::new(r"^[-*+][ \t]+").unwrap();
    let ol = Regex::new(r"^\d{1,9}[.)][ \t]+").unwrap();
    let heading = Regex::new(r"^(#{1,6})[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$").unwrap();
    let token_only = Regex::new(r"^(?:\u{0}\d+\u{0}[ \t]*)+$").unwrap();
    let mut parts: Vec<String> = Vec::new();
    for chunk in split.split(&text) {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let lines: Vec<&str> = chunk.split('\n').collect();
        let first = lines[0].trim();
        if ul.is_match(first) || ol.is_match(first) {
            parts.push(md_list(&lines));
            continue;
        }
        let heading = (lines.len() == 1)
            .then(|| heading.captures(first))
            .flatten();
        if let Some(caps) = heading {
            let level = caps.get(1).expect("level group").as_str().len();
            let content = md_inline(caps.get(2).expect("content group").as_str());
            parts.push(format!("<h{level}>{content}</h{level}>"));
            continue;
        }
        let converted = md_inline(chunk).replace('\n', "<br/>");
        if token_only.is_match(chunk) {
            // A chunk that is only a code block keeps its pre wrapper.
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
}
