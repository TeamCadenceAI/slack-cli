//! Convert standard Markdown into Slack **mrkdwn**.
//!
//! Slack's `chat.postMessage` `text` field is parsed as *mrkdwn*, which is not
//! Markdown. Agents (and humans) naturally emit standard Markdown, so common
//! syntax like `**bold**` or `[text](url)` renders as literal characters. This
//! module rewrites the Markdown constructs Slack understands into their mrkdwn
//! equivalents:
//!
//! | Markdown | mrkdwn |
//! | --- | --- |
//! | `**bold**`, `__bold__` | `*bold*` |
//! | `*italic*`, `_italic_` | `_italic_` |
//! | `~~strike~~` | `~strike~` |
//! | `# Heading` | `*Heading*` (bold line) |
//! | `[text](url)` | `<url\|text>` |
//! | `![alt](url)` | `<url\|alt>` |
//! | `- item`, `* item`, `+ item` | `• item` |
//! | `1. item`, `1) item` | `1. item` (normalized) |
//!
//! Regions that are already mrkdwn — inline code `` `…` ``, fenced code blocks
//! ```` ```…``` ````, and existing angle-bracket spans `<@U…>` / `<url|text>`
//! — are passed through untouched so nothing gets double-converted.
//!
//! The conversion is a pragmatic, dependency-free scanner rather than a full
//! CommonMark parser; it targets the constructs that actually differ between
//! Markdown and Slack mrkdwn.

/// Convert a standard-Markdown string into Slack mrkdwn.
///
/// See the [module docs](self) for the exact mapping. Input that is already
/// valid mrkdwn is generally left unchanged.
pub fn markdown_to_mrkdwn(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_fence = false;
    let mut fence_marker: &str = "";

    let mut first = true;
    for line in input.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;

        let trimmed = line.trim_start();

        // Toggle fenced code blocks on ``` or ~~~ (kept verbatim, including the
        // fence lines themselves).
        if !in_fence && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            in_fence = true;
            fence_marker = if trimmed.starts_with("```") {
                "```"
            } else {
                "~~~"
            };
            out.push_str(line);
            continue;
        } else if in_fence {
            out.push_str(line);
            if trimmed.starts_with(fence_marker) {
                in_fence = false;
            }
            continue;
        }

        out.push_str(&convert_block_line(line));
    }

    out
}

/// Apply block-level (line-leading) transforms, then inline transforms to the
/// line's content.
fn convert_block_line(line: &str) -> String {
    // Measure leading whitespace so we can preserve indentation.
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let rest_trimmed = rest.trim_end();

    // ATX heading: `#`..`######` followed by a space -> bold line.
    if let Some(after) = strip_heading(rest_trimmed) {
        return format!("{indent}*{}*", convert_inline(after));
    }

    // Blockquote: `>` optionally followed by a space. mrkdwn supports `>`.
    if let Some(after) = rest.strip_prefix("> ").or_else(|| rest.strip_prefix('>')) {
        return format!("{indent}> {}", convert_inline(after));
    }

    // Bullet list: `-`, `*`, or `+` followed by whitespace -> `• `.
    if let Some(after) = strip_bullet(rest) {
        return format!("{indent}• {}", convert_inline(after));
    }

    // Ordered list: `<n>.` or `<n>)` followed by whitespace -> `<n>. `.
    if let Some((n, after)) = strip_ordered(rest) {
        return format!("{indent}{n}. {}", convert_inline(after));
    }

    format!("{indent}{}", convert_inline(rest))
}

/// If `s` is an ATX heading (`#`..`######` + space), return the heading text
/// with any trailing `#` run removed.
fn strip_heading(s: &str) -> Option<&str> {
    let hashes = s.len() - s.trim_start_matches('#').len();
    if (1..=6).contains(&hashes) {
        let after = &s[hashes..];
        // Require a space after the hashes to be a heading (not `#tag`).
        if let Some(text) = after.strip_prefix(' ') {
            return Some(text.trim_end().trim_end_matches('#').trim_end());
        }
    }
    None
}

/// If `s` begins with a bullet marker (`-`/`*`/`+` + whitespace), return the
/// item content.
fn strip_bullet(s: &str) -> Option<&str> {
    let mut chars = s.chars();
    match chars.next() {
        Some('-') | Some('*') | Some('+') => {}
        _ => return None,
    }
    let after = &s[1..];
    // Must be followed by at least one space/tab to be a list item.
    if after.starts_with([' ', '\t']) {
        Some(after.trim_start())
    } else {
        None
    }
}

/// If `s` begins with an ordered-list marker (`<n>.` or `<n>)` + whitespace),
/// return `(n, content)`.
fn strip_ordered(s: &str) -> Option<(u64, &str)> {
    let digits_len = s.len() - s.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits_len == 0 {
        return None;
    }
    let n: u64 = s[..digits_len].parse().ok()?;
    let after_digits = &s[digits_len..];
    let after_marker = after_digits
        .strip_prefix('.')
        .or_else(|| after_digits.strip_prefix(')'))?;
    if after_marker.starts_with([' ', '\t']) {
        Some((n, after_marker.trim_start()))
    } else {
        None
    }
}

/// Convert inline Markdown spans within a single line (no newlines) into
/// mrkdwn. Protects inline code and existing angle-bracket spans.
fn convert_inline(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;

    while i < b.len() {
        match b[i] {
            // Inline code span: copy verbatim through the closing backtick.
            b'`' => {
                if let Some(close) = find_byte(b, i + 1, b'`') {
                    out.push_str(&s[i..=close]);
                    i = close + 1;
                } else {
                    out.push('`');
                    i += 1;
                }
            }
            // Existing angle-bracket span (<@U…>, <#C…>, <!here>, <url|text>):
            // copy verbatim so we never double-encode links/mentions. Only do
            // so when the content actually looks like a mrkdwn span; otherwise
            // (e.g. `x < 5 and **bold** > 2`) emit `<` literally and keep
            // converting the text after it.
            b'<' => match find_byte(b, i + 1, b'>') {
                Some(close) if is_mrkdwn_span(&s[i + 1..close]) => {
                    out.push_str(&s[i..=close]);
                    i = close + 1;
                }
                _ => {
                    out.push('<');
                    i += 1;
                }
            },
            // Image: ![alt](url) -> <url|alt>
            b'!' if i + 1 < b.len() && b[i + 1] == b'[' => {
                if let Some((text, url, next)) = parse_link(b, s, i + 1) {
                    push_link(&mut out, &url, &text);
                    i = next;
                } else {
                    out.push('!');
                    i += 1;
                }
            }
            // Link: [text](url) -> <url|text>
            b'[' => {
                if let Some((text, url, next)) = parse_link(b, s, i) {
                    push_link(&mut out, &url, &text);
                    i = next;
                } else {
                    out.push('[');
                    i += 1;
                }
            }
            // Bold/italic with asterisks.
            b'*' => {
                if b.get(i + 1) == Some(&b'*') {
                    // **bold** -> *bold*
                    if let Some(close) = find_seq(b, i + 2, b"**") {
                        let inner = convert_inline(&s[i + 2..close]);
                        out.push('*');
                        out.push_str(&inner);
                        out.push('*');
                        i = close + 2;
                        continue;
                    }
                    // Unmatched `**`: emit both asterisks literally so the
                    // second one isn't re-parsed as an italic opener.
                    out.push_str("**");
                    i += 2;
                } else if let Some(close) = find_emphasis_close(b, i + 1) {
                    // *italic* -> _italic_
                    let inner = convert_inline(&s[i + 1..close]);
                    out.push('_');
                    out.push_str(&inner);
                    out.push('_');
                    i = close + 1;
                } else {
                    out.push('*');
                    i += 1;
                }
            }
            // Bold/italic with underscores.
            b'_' => {
                if b.get(i + 1) == Some(&b'_') {
                    // __bold__ -> *bold*
                    if let Some(close) = find_seq(b, i + 2, b"__") {
                        let inner = convert_inline(&s[i + 2..close]);
                        out.push('*');
                        out.push_str(&inner);
                        out.push('*');
                        i = close + 2;
                        continue;
                    }
                    out.push('_');
                    i += 1;
                } else {
                    // _italic_ is already mrkdwn; copy through the closing `_`.
                    if let Some(close) = find_byte(b, i + 1, b'_') {
                        out.push_str(&s[i..=close]);
                        i = close + 1;
                    } else {
                        out.push('_');
                        i += 1;
                    }
                }
            }
            // Strikethrough: ~~strike~~ -> ~strike~ (single ~ already mrkdwn).
            b'~' => {
                if b.get(i + 1) == Some(&b'~') {
                    if let Some(close) = find_seq(b, i + 2, b"~~") {
                        let inner = convert_inline(&s[i + 2..close]);
                        out.push('~');
                        out.push_str(&inner);
                        out.push('~');
                        i = close + 2;
                        continue;
                    }
                    out.push('~');
                    i += 1;
                } else if let Some(close) = find_byte(b, i + 1, b'~') {
                    out.push_str(&s[i..=close]);
                    i = close + 1;
                } else {
                    out.push('~');
                    i += 1;
                }
            }
            // Any other byte (including UTF-8 continuation bytes >= 0x80): copy
            // the full UTF-8 char verbatim.
            _ => {
                let ch_len = utf8_len(b[i]);
                let end = (i + ch_len).min(b.len());
                out.push_str(&s[i..end]);
                i = end;
            }
        }
    }

    out
}

/// Parse a Markdown link/image body starting at `open` (the `[`). Returns the
/// (converted link text, url, index just past the closing `)`).
fn parse_link(b: &[u8], s: &str, open: usize) -> Option<(String, String, usize)> {
    debug_assert_eq!(b[open], b'[');
    let text_close = find_byte(b, open + 1, b']')?;
    // The `]` must be immediately followed by `(`.
    if b.get(text_close + 1) != Some(&b'(') {
        return None;
    }
    // Find the `)` that matches the opening `(`, allowing balanced parens
    // inside the URL (e.g. `…/wiki/Topic_(disambiguation)`).
    let url_close = find_matching_paren(b, text_close + 1)?;

    let text = convert_inline(&s[open + 1..text_close]);
    let raw_url = s[text_close + 2..url_close].trim();

    // Strip an optional `"title"` after the URL: [t](url "title").
    let url = match raw_url.split_once(char::is_whitespace) {
        Some((u, _title)) => u.trim(),
        None => raw_url,
    };

    if url.is_empty() {
        return None;
    }

    Some((text, url.to_string(), url_close + 1))
}

/// Append a Slack link span `<url|text>` (or `<url>` when text matches url or
/// is empty).
fn push_link(out: &mut String, url: &str, text: &str) {
    if text.is_empty() || text == url {
        out.push('<');
        out.push_str(url);
        out.push('>');
    } else {
        out.push('<');
        out.push_str(url);
        out.push('|');
        out.push_str(text);
        out.push('>');
    }
}

/// Find the next single byte `needle` at or after `from`.
fn find_byte(b: &[u8], from: usize, needle: u8) -> Option<usize> {
    (from..b.len()).find(|&i| b[i] == needle)
}

/// Given the index of an opening `(`, return the index of its matching `)`,
/// honoring nested/balanced parentheses. Returns `None` if unbalanced.
fn find_matching_paren(b: &[u8], open_paren: usize) -> Option<usize> {
    debug_assert_eq!(b[open_paren], b'(');
    let mut depth = 0usize;
    let mut i = open_paren;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Find the closing `*` of a single-asterisk italic span opened at `open`
/// (the index just past the opening `*`), applying simplified CommonMark
/// "flanking" rules: the opener must not be followed by whitespace, and the
/// closing `*` must not be preceded by whitespace and must not itself be part
/// of a `**` pair. This prevents `a * b * c` from being read as emphasis.
fn find_emphasis_close(b: &[u8], open: usize) -> Option<usize> {
    // A left-flanking `*` cannot be immediately followed by whitespace.
    if b.get(open).map_or(true, |&c| c.is_ascii_whitespace()) {
        return None;
    }
    let mut i = open;
    while i < b.len() {
        if b[i] == b'*' {
            // Skip `**` (bold) delimiters — not a single-emphasis closer.
            if b.get(i + 1) == Some(&b'*') {
                i += 2;
                continue;
            }
            // A right-flanking `*` cannot be immediately preceded by whitespace.
            if i > open && !b[i - 1].is_ascii_whitespace() {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Return `true` if the text between a `<` and `>` looks like an existing Slack
/// mrkdwn span that must be preserved verbatim: a user/channel mention
/// (`@U…`/`#C…`), a special mention (`!here`/`!channel`/`!subteam…`), or a
/// link with a URL scheme (`http:`, `https:`, `mailto:`, `tel:`), optionally
/// with a `|label`. Anything else (e.g. `5 and **bold** > 2`) is treated as
/// literal text so conversion can continue inside it.
fn is_mrkdwn_span(inner: &str) -> bool {
    if inner.is_empty() {
        return false;
    }
    let first = inner.as_bytes()[0];
    if matches!(first, b'@' | b'#' | b'!') {
        return true;
    }
    // Link form: <scheme:...> possibly with a |label. Check the part before `|`.
    let url = inner.split('|').next().unwrap_or(inner);
    let schemes = ["http://", "https://", "mailto:", "tel:"];
    schemes.iter().any(|s| url.starts_with(s))
}

/// Find the next occurrence of the byte sequence `seq` at or after `from`.
fn find_seq(b: &[u8], from: usize, seq: &[u8]) -> Option<usize> {
    if seq.is_empty() || b.len() < seq.len() {
        return None;
    }
    (from..=b.len() - seq.len()).find(|&i| &b[i..i + seq.len()] == seq)
}

/// UTF-8 byte length implied by a leading byte.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1, // continuation/invalid byte: consume one to make progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_double_asterisk() {
        assert_eq!(markdown_to_mrkdwn("**bold**"), "*bold*");
    }

    #[test]
    fn bold_double_underscore() {
        assert_eq!(markdown_to_mrkdwn("__bold__"), "*bold*");
    }

    #[test]
    fn italic_asterisk_becomes_underscore() {
        assert_eq!(markdown_to_mrkdwn("*italic*"), "_italic_");
    }

    #[test]
    fn italic_underscore_unchanged() {
        assert_eq!(markdown_to_mrkdwn("_italic_"), "_italic_");
    }

    #[test]
    fn strikethrough() {
        assert_eq!(markdown_to_mrkdwn("~~gone~~"), "~gone~");
    }

    #[test]
    fn link_becomes_angle_span() {
        assert_eq!(
            markdown_to_mrkdwn("[a link](https://example.com)"),
            "<https://example.com|a link>"
        );
    }

    #[test]
    fn image_becomes_angle_span() {
        assert_eq!(
            markdown_to_mrkdwn("![alt text](https://example.com/x.png)"),
            "<https://example.com/x.png|alt text>"
        );
    }

    #[test]
    fn link_with_title_strips_title() {
        assert_eq!(
            markdown_to_mrkdwn("[t](https://e.com \"the title\")"),
            "<https://e.com|t>"
        );
    }

    #[test]
    fn bare_link_text_equal_url_collapses() {
        assert_eq!(
            markdown_to_mrkdwn("[https://e.com](https://e.com)"),
            "<https://e.com>"
        );
    }

    #[test]
    fn issue_example() {
        // The exact example from issue #1.
        assert_eq!(
            markdown_to_mrkdwn("**bold** and [a link](https://example.com)"),
            "*bold* and <https://example.com|a link>"
        );
    }

    #[test]
    fn heading_becomes_bold_line() {
        assert_eq!(markdown_to_mrkdwn("# Heading"), "*Heading*");
        assert_eq!(markdown_to_mrkdwn("### Sub heading ###"), "*Sub heading*");
    }

    #[test]
    fn hashtag_is_not_a_heading() {
        assert_eq!(markdown_to_mrkdwn("#notaheading"), "#notaheading");
    }

    #[test]
    fn bullets_become_bullet_glyph() {
        let input = "- one\n* two\n+ three";
        let expected = "• one\n• two\n• three";
        assert_eq!(markdown_to_mrkdwn(input), expected);
    }

    #[test]
    fn ordered_list_keeps_numbers() {
        let input = "1. first\n2) second\n3. third";
        let expected = "1. first\n2. second\n3. third";
        assert_eq!(markdown_to_mrkdwn(input), expected);
    }

    #[test]
    fn indented_list_preserves_indent() {
        assert_eq!(markdown_to_mrkdwn("  - nested"), "  • nested");
    }

    #[test]
    fn bullet_with_inline_formatting() {
        assert_eq!(
            markdown_to_mrkdwn("- **bold** item [x](https://e.com)"),
            "• *bold* item <https://e.com|x>"
        );
    }

    #[test]
    fn inline_code_is_protected() {
        // Markdown-ish characters inside `code` must not be converted.
        assert_eq!(
            markdown_to_mrkdwn("use `**not bold**` here"),
            "use `**not bold**` here"
        );
    }

    #[test]
    fn fenced_code_block_is_verbatim() {
        let input = "```\n**not bold**\n[no](link)\n```";
        assert_eq!(markdown_to_mrkdwn(input), input);
    }

    #[test]
    fn existing_mrkdwn_link_untouched() {
        assert_eq!(
            markdown_to_mrkdwn("see <https://e.com|here>"),
            "see <https://e.com|here>"
        );
    }

    #[test]
    fn existing_mention_untouched() {
        assert_eq!(
            markdown_to_mrkdwn("cc <@U090BKEQXMH> <@U094XFRB77C>"),
            "cc <@U090BKEQXMH> <@U094XFRB77C>"
        );
    }

    #[test]
    fn link_text_with_formatting() {
        assert_eq!(
            markdown_to_mrkdwn("[**bold** link](https://e.com)"),
            "<https://e.com|*bold* link>"
        );
    }

    #[test]
    fn unmatched_markers_are_literal() {
        assert_eq!(markdown_to_mrkdwn("a * b"), "a * b");
        assert_eq!(markdown_to_mrkdwn("2 * 3 = 6"), "2 * 3 = 6");
    }

    #[test]
    fn spaced_asterisks_are_not_italic() {
        // Multiple space-flanked asterisks must stay literal (CommonMark
        // flanking rules); regression test for the `a * b * c` bug.
        assert_eq!(markdown_to_mrkdwn("a * b * c"), "a * b * c");
        assert_eq!(markdown_to_mrkdwn("1 * 2 * 3 * 4"), "1 * 2 * 3 * 4");
        // But a genuine *italic* (no inner-flanking whitespace) still converts.
        assert_eq!(markdown_to_mrkdwn("an *italic* word"), "an _italic_ word");
        assert_eq!(markdown_to_mrkdwn("*two words*"), "_two words_");
    }

    #[test]
    fn angle_brackets_only_preserved_for_real_spans() {
        // Non-span angle brackets (comparisons) must not swallow conversion.
        assert_eq!(
            markdown_to_mrkdwn("x < 5 and **bold** > 2"),
            "x < 5 and *bold* > 2"
        );
        assert_eq!(
            markdown_to_mrkdwn("if a < b and c > d"),
            "if a < b and c > d"
        );
        // Genuine mrkdwn spans are still preserved verbatim.
        assert_eq!(
            markdown_to_mrkdwn("see <https://e.com|here> and <@U123>"),
            "see <https://e.com|here> and <@U123>"
        );
        assert_eq!(markdown_to_mrkdwn("ping <!here> now"), "ping <!here> now");
    }

    #[test]
    fn link_url_with_parentheses() {
        // Balanced parens inside the URL must not truncate the link.
        assert_eq!(
            markdown_to_mrkdwn("[Topic](https://en.wikipedia.org/wiki/Topic_(disambiguation))"),
            "<https://en.wikipedia.org/wiki/Topic_(disambiguation)|Topic>"
        );
    }

    #[test]
    fn unmatched_double_asterisk_does_not_italicize() {
        // `**foo* bar`: the unmatched `**` stays literal; the lone `*` after
        // `foo` is space-flanked on its right-hand search so no italic forms.
        assert_eq!(markdown_to_mrkdwn("**foo* bar"), "**foo* bar");
    }

    #[test]
    fn unicode_is_preserved() {
        assert_eq!(
            markdown_to_mrkdwn("**café** ☕ [x](https://e.com)"),
            "*café* ☕ <https://e.com|x>"
        );
    }

    #[test]
    fn multiline_document_matches_slack_composer_style() {
        // Mirrors the structure of a real Slack post: bold header, prose, an
        // ordered list, and a single-item bullet list.
        let input = "\
# Update

**Why:** rotation matters.

1. **Codex**: use it.
2. **Claude**: same.

- Antigravity: safe.";
        let expected = "\
*Update*

*Why:* rotation matters.

1. *Codex*: use it.
2. *Claude*: same.

• Antigravity: safe.";
        assert_eq!(markdown_to_mrkdwn(input), expected);
    }
}
