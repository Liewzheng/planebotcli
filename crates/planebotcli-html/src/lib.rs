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
}
