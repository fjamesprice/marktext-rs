//! A Rust port of `normalizeHtml` from `spec/runner.ts`.
//!
//! # Why normalise at all
//!
//! The reference implementation (cmark) emits compact HTML. muya emits
//! equivalent but slightly different HTML: extra spaces around self-closing
//! `<br/>`, attribute ordering, optional trailing newlines. Those differences
//! are not compliance failures and must not masquerade as them. The bar the
//! upstream runner sets is "is the engine within shouting distance of the
//! spec", and this is where that judgement is encoded.
//!
//! # Why a hand-written scanner
//!
//! The TypeScript original is four regex passes. This port is hand-written
//! rather than using the `regex` crate for the same reason nothing else here
//! has dependencies at M0 (§8/§12: deps land with the milestone that needs
//! them), and because the passes are simple enough — tag scanning, attribute
//! splitting, whitespace collapsing — that a scanner is not meaningfully
//! harder than the regexes it replaces.
//!
//! # Fidelity
//!
//! Each pass below documents the exact JavaScript regex it replaces, and the
//! tests at the bottom cover every behaviour called out in `runner.ts`'s own
//! doc comment. Two limitations are inherited deliberately rather than fixed,
//! because fixing them here would make the two runners disagree:
//!
//! - An attribute value containing `>` breaks tag scanning. The JS `[^>]*?`
//!   has the same blind spot.
//! - Tag names are not lower-cased by the attribute pass (only by the
//!   void-tag pass, which rewrites the name from a fixed list).
//!
//! ~~**Verification owed at M2.**~~ — **paid at M2 S5.** Nothing here could be
//! checked end-to-end until `mt_md::render_to_static_html` produced output;
//! when it did, the cheapest high-confidence check was the differential this
//! paragraph named. It is [`crate::normalize`]: the same HTML through this
//! function and through `normalizeHtml` in `spec/runner.ts`, over all 1,324
//! rendered fixtures **and** the 1,324 expected ones, because the conformance
//! comparison normalises both sides. **2,648 of 2,648 agree.**
//!
//! Which makes the two limitations above a *checked* claim rather than a
//! stated one — and makes "fixing" either of them a way to break the gate.

/// Void elements whose self-closing form is normalised. Same list, same order
/// as `voidTags` in `runner.ts`.
const VOID_TAGS: [&str; 5] = ["br", "hr", "img", "wbr", "input"];

/// Canonicalise an HTML fragment so trivially-different but semantically-equal
/// outputs compare equal.
pub fn normalize_html(html: &str) -> String {
    let out = normalize_void_tags(html);
    let out = sort_attributes(&out);
    let out = collapse_inter_tag_whitespace(&out);
    let out = strip_whitespace_after_void_tags(&out);
    trim_outer_newlines(&out)
}

/// Pass 1 — `<br />` / `<br/>` / `<br>` / `<BR   >` all become `<br />`.
///
/// JS: ``new RegExp(`<${tag}((?:\\s+[^>]*?)?)\\s*/?\\s*>`, 'gi')`` per tag,
/// replaced with `<tag attrs />` (attrs trimmed) or `<tag />`.
fn normalize_void_tags(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    'outer: while i < bytes.len() {
        if bytes[i] == b'<' {
            for tag in VOID_TAGS {
                let after_name = i + 1 + tag.len();
                if after_name > bytes.len() || !eq_ignore_ascii_case(&bytes[i + 1..after_name], tag)
                {
                    continue;
                }
                // `[^>]*?` and `\s*` cannot cross a `>`, so the match ends at
                // the next `>` or not at all.
                let Some(close) = find_byte(bytes, after_name, b'>') else {
                    continue;
                };
                let raw = &input[after_name..close];
                // `raw` must split as group 1 `(?:\s+[^>]*?)?` followed by the
                // tail `\s*/?\s*`. Group 1 is either absent, or present and
                // therefore starting with whitespace — so `raw` is acceptable
                // when it is empty (`<br>`), starts with whitespace
                // (`<br foo>`, `<br />`), or is the bare tail (`<br/>`).
                // Anything else is a different tag: `<brx>` is not `<br>`.
                let matches_tag = raw.is_empty()
                    || raw.starts_with(is_html_space)
                    || raw.trim_matches(is_html_space) == "/";
                if !matches_tag {
                    continue;
                }
                // The tail `\s*/?\s*` is what the lazy `[^>]*?` gives back.
                let attrs = strip_self_closing_tail(raw).trim();
                if attrs.is_empty() {
                    out.push('<');
                    out.push_str(tag);
                    out.push_str(" />");
                } else {
                    out.push('<');
                    out.push_str(tag);
                    out.push(' ');
                    out.push_str(attrs);
                    out.push_str(" />");
                }
                i = close + 1;
                continue 'outer;
            }
        }
        // Not a void tag: copy one character (not one byte — the input is
        // UTF-8 and slicing mid-codepoint would panic).
        let ch = input[i..]
            .chars()
            .next()
            .expect("index is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Remove the `\s*/?\s*` the lazy `[^>]*?` in pass 1 gives back to the tail.
fn strip_self_closing_tail(raw: &str) -> &str {
    let trimmed = raw.trim_end_matches(is_html_space);
    match trimmed.strip_suffix('/') {
        Some(rest) => rest,
        None => trimmed,
    }
}

/// Pass 2 — sort tag attributes alphabetically, so `<a href="x" title="y">`
/// and `<a title="y" href="x">` compare equal.
///
/// JS: `/<([a-z][\w-]*)[ \t\n\r]+([a-z_:][^>]*?)(\/?)>/gi`.
///
/// The attrs group is anchored to start with a valid attribute-name character.
/// That is what excludes the `/` of `<br />` — which would otherwise be
/// captured as attrs and dropped by the attribute tokenizer, undoing pass 1.
fn sort_attributes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            let ch = input[i..]
                .chars()
                .next()
                .expect("index is on a char boundary");
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        match match_open_tag(input, i) {
            Some((name, attrs_src, self_close, end)) => {
                let attrs = parse_attributes(attrs_src);
                let rebuilt = attrs
                    .iter()
                    .map(|(k, v)| {
                        if v.is_empty() {
                            (*k).to_string()
                        } else {
                            format!("{k}={v}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let sc = if self_close { " /" } else { "" };
                if rebuilt.is_empty() {
                    out.push_str(&format!("<{name}{sc}>"));
                } else {
                    out.push_str(&format!("<{name} {rebuilt}{sc}>"));
                }
                i = end;
            }
            None => {
                out.push('<');
                i += 1;
            }
        }
    }
    out
}

/// Match `<name WS attrs /?>` at `start`. Returns
/// `(name, attrs, self_closing, index_past_the_closing_angle)`.
fn match_open_tag(input: &str, start: usize) -> Option<(&str, &str, bool, usize)> {
    let bytes = input.as_bytes();
    debug_assert_eq!(bytes[start], b'<');

    // name: `[a-z][\w-]*` with the `i` flag.
    let name_start = start + 1;
    let first = *bytes.get(name_start)?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut j = name_start + 1;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
    {
        j += 1;
    }
    let name = &input[name_start..j];

    // `[ \t\n\r]+`
    let ws_start = j;
    while j < bytes.len() && is_html_space_byte(bytes[j]) {
        j += 1;
    }
    if j == ws_start {
        return None;
    }

    // attrs must start with `[a-z_:]` (case-insensitive).
    let attrs_start = j;
    let first_attr = *bytes.get(attrs_start)?;
    if !(first_attr.is_ascii_alphabetic() || first_attr == b'_' || first_attr == b':') {
        return None;
    }

    // `[^>]*?(\/?)>` — the lazy body means the optional `/` is consumed by
    // group 3 whenever one sits immediately before the `>`.
    let close = find_byte(bytes, attrs_start, b'>')?;
    let (attrs_end, self_close) = if bytes[close - 1] == b'/' && close > attrs_start {
        (close - 1, true)
    } else {
        (close, false)
    };

    Some((name, &input[attrs_start..attrs_end], self_close, close + 1))
}

/// Split an attribute list into `(name, raw_value_including_quotes)` pairs,
/// sorted by name.
///
/// JS: `/([a-z_:][\w:.-]*)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s"'>`]+))?/gi` in an
/// `exec` loop — which skips over anything that does not match, rather than
/// failing.
fn parse_attributes(src: &str) -> Vec<(&str, &str)> {
    let bytes = src.as_bytes();
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if !(b.is_ascii_alphabetic() || b == b'_' || b == b':') {
            i += 1;
            continue;
        }
        let name_start = i;
        i += 1;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.' | b'-') {
                i += 1;
            } else {
                break;
            }
        }
        let name = &src[name_start..i];

        // Optional `\s*=\s*value`.
        let mut probe = i;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe < bytes.len() && bytes[probe] == b'=' {
            probe += 1;
            while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
                probe += 1;
            }
            if let Some((value, next)) = read_attribute_value(src, probe) {
                pairs.push((name, value));
                i = next;
                continue;
            }
        }
        pairs.push((name, ""));
    }

    // JS sorts by name only, with a stable sort. `sort_by_key` in Rust is
    // stable too, so attributes repeating the same name keep source order in
    // both. Sorting by the *name* alone rather than by the pair is the whole
    // point — comparing `(name, value)` would reorder repeats.
    pairs.sort_by_key(|(name, _)| *name);
    pairs
}

/// Read `"…"` | `'…'` | an unquoted run, returning the raw slice *including*
/// its quotes — the JS rebuilds `k=v` with `v` as captured, quotes and all.
fn read_attribute_value(src: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = src.as_bytes();
    let first = *bytes.get(start)?;
    if first == b'"' || first == b'\'' {
        let end = find_byte(bytes, start + 1, first)?;
        return Some((&src[start..=end], end + 1));
    }
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() || matches!(c, b'"' | b'\'' | b'>' | b'`') {
            break;
        }
        i += 1;
    }
    if i == start {
        None
    } else {
        Some((&src[start..i], i))
    }
}

/// Pass 3 — `>WS<` becomes `><`.
///
/// JS: `/>[ \t\n\r]+</g`. Crucially `>foo\n<` does **not** match, because
/// `foo` is not whitespace: content between tags is preserved, which is what
/// fenced-code-block examples depend on.
fn collapse_inter_tag_whitespace(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'>' {
            let mut j = i + 1;
            while j < bytes.len() && is_html_space_byte(bytes[j]) {
                j += 1;
            }
            if j > i + 1 && j < bytes.len() && bytes[j] == b'<' {
                out.push_str("><");
                i = j + 1;
                continue;
            }
        }
        let ch = input[i..]
            .chars()
            .next()
            .expect("index is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Pass 4 — drop whitespace immediately after a bare self-closed void tag.
///
/// JS: `/(<(?:br|hr|wbr|img|input)\s+\/>)[ \t\n\r]+/gi`. This is what makes
/// cmark's `<br>\nfoo` and muya's `<br>foo` compare equal: after pass 1 both
/// read `<br />`, and the following whitespace run goes here.
///
/// Note the `\s+\/>`: only the *attribute-less* form matches, because a tag
/// with attributes has them where the `\s+` would be.
fn strip_whitespace_after_void_tags(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    'outer: while i < bytes.len() {
        if bytes[i] == b'<' {
            for tag in VOID_TAGS {
                let after_name = i + 1 + tag.len();
                if after_name > bytes.len() || !eq_ignore_ascii_case(&bytes[i + 1..after_name], tag)
                {
                    continue;
                }
                let mut j = after_name;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j == after_name || !bytes[j..].starts_with(b"/>") {
                    continue;
                }
                let tag_end = j + 2;
                out.push_str(&input[i..tag_end]);
                let mut k = tag_end;
                while k < bytes.len() && is_html_space_byte(bytes[k]) {
                    k += 1;
                }
                i = k;
                continue 'outer;
            }
        }
        let ch = input[i..]
            .chars()
            .next()
            .expect("index is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Pass 5 — strip leading and trailing newlines.
///
/// JS: `/^\n+|\n+$/g`. Internal blank-line runs are deliberately preserved:
/// code blocks can contain semantically significant blank lines.
fn trim_outer_newlines(input: &str) -> String {
    input
        .trim_start_matches('\n')
        .trim_end_matches('\n')
        .to_string()
}

// --- small helpers ---------------------------------------------------------

fn is_html_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn is_html_space_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn eq_ignore_ascii_case(bytes: &[u8], s: &str) -> bool {
    bytes.eq_ignore_ascii_case(s.as_bytes())
}

fn find_byte(bytes: &[u8], from: usize, needle: u8) -> Option<usize> {
    bytes[from..]
        .iter()
        .position(|&b| b == needle)
        .map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn void_tags_normalise_to_one_form() {
        for input in ["<br>", "<br/>", "<br />", "<br   />", "<BR>", "<br\n/>"] {
            assert_eq!(normalize_html(input), "<br />", "input was {input:?}");
        }
    }

    #[test]
    fn void_tag_attributes_survive_normalisation() {
        assert_eq!(
            normalize_html(r#"<img src="x.png"/>"#),
            r#"<img src="x.png" />"#
        );
    }

    #[test]
    fn a_tag_that_merely_starts_with_a_void_tag_name_is_untouched() {
        assert_eq!(normalize_html("<brx>"), "<brx>");
        assert_eq!(normalize_html("<hrefish>"), "<hrefish>");
    }

    #[test]
    fn attributes_sort_alphabetically() {
        assert_eq!(
            normalize_html(r#"<a href="x" title="y">z</a>"#),
            normalize_html(r#"<a title="y" href="x">z</a>"#)
        );
        assert_eq!(
            normalize_html(r#"<a title="y" href="x">z</a>"#),
            r#"<a href="x" title="y">z</a>"#
        );
    }

    #[test]
    fn valueless_attributes_are_kept() {
        assert_eq!(
            normalize_html(r#"<input disabled type="checkbox">"#),
            r#"<input disabled type="checkbox" />"#
        );
    }

    #[test]
    fn single_quoted_and_unquoted_values_keep_their_form() {
        assert_eq!(normalize_html("<a href='x'>y</a>"), "<a href='x'>y</a>");
        assert_eq!(normalize_html("<a href=x>y</a>"), "<a href=x>y</a>");
    }

    #[test]
    fn whitespace_between_tags_collapses() {
        assert_eq!(
            normalize_html("<ul>\n  <li>a</li>\n</ul>"),
            "<ul><li>a</li></ul>"
        );
    }

    /// The load-bearing negative: content between tags is NOT whitespace, so
    /// it survives. Fenced-code-block spec examples depend on this.
    #[test]
    fn content_between_tags_is_preserved() {
        assert_eq!(
            normalize_html("<pre><code>foo\n</code></pre>"),
            "<pre><code>foo\n</code></pre>"
        );
        assert_eq!(normalize_html("<p>foo\nbar</p>"), "<p>foo\nbar</p>");
    }

    /// cmark emits `<br>\nfoo`; muya emits `<br>foo`. They render identically.
    #[test]
    fn whitespace_after_a_bare_void_tag_is_dropped() {
        assert_eq!(
            normalize_html("<p>a<br>\nfoo</p>"),
            normalize_html("<p>a<br>foo</p>")
        );
        assert_eq!(normalize_html("<p>a<br>\nfoo</p>"), "<p>a<br />foo</p>");
    }

    /// ...but only for the attribute-less form, matching `\s+\/>` in the JS.
    #[test]
    fn whitespace_after_a_void_tag_with_attributes_is_kept() {
        assert_eq!(
            normalize_html("<img src=\"x\">\nfoo"),
            "<img src=\"x\" />\nfoo"
        );
    }

    #[test]
    fn outer_newlines_are_trimmed_but_inner_blank_lines_are_not() {
        assert_eq!(normalize_html("\n\n<p>a</p>\n\n"), "<p>a</p>");
        assert_eq!(
            normalize_html("<pre><code>a\n\n\nb\n</code></pre>"),
            "<pre><code>a\n\n\nb\n</code></pre>"
        );
    }

    #[test]
    fn non_ascii_content_is_not_corrupted() {
        // The scanners index by byte; anything that slices mid-codepoint
        // panics rather than corrupting, so this is a real guard.
        let input = "<p>日本語 — الْعَرَبِيَّة — 👨‍👩‍👧‍👦</p>";
        assert_eq!(normalize_html(input), input);
    }

    #[test]
    fn a_lone_angle_bracket_is_left_alone() {
        assert_eq!(normalize_html("<p>5 < 6</p>"), "<p>5 < 6</p>");
        assert_eq!(normalize_html("a < b"), "a < b");
    }

    #[test]
    fn closing_tags_are_untouched() {
        assert_eq!(normalize_html("</a>"), "</a>");
    }

    #[test]
    fn is_idempotent() {
        for input in [
            "<ul>\n <li>a</li>\n</ul>",
            r#"<a title="y" href="x">z</a>"#,
            "<p>a<br>\nfoo</p>",
            "\n<p>x</p>\n",
        ] {
            let once = normalize_html(input);
            assert_eq!(normalize_html(&once), once, "input was {input:?}");
        }
    }
}
