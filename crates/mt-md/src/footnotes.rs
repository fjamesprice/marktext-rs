//! `packages/muya/src/state/transformFootnotes.ts`, 100 lines, transcribed.
//!
//! The footnote extension emits `<div class="footnote-block" data-identifier="…">`
//! for each definition and leaves every inline `[^id]` as source. This pass
//! turns that into the GFM / pandoc shape: a numbered
//! `<sup class="footnote-ref">` at each reference and a final
//! `<section class="footnotes">` holding one `<li id="fn-N">` per referenced
//! definition, each with a backref arrow.
//!
//! # Why it is a pass over HTML rather than over the tree
//!
//! Because the numbering is by **first-occurrence order of the inline
//! references**, not by definition order, and the inline references are only
//! known once the body has been rendered. muya makes the same choice for the
//! same reason, and `footnoteHtml.spec.ts`'s case 2 is the assertion a
//! definition-ordered implementation fails.
//!
//! It must run **before** sanitization, because `data-identifier` is a `data-*`
//! attribute and the export sanitizer config strips those. [`crate::render_to_static_html`]
//! sequences the two.
//!
//! # Four behaviours that are not the obvious reading, all transcribed
//!
//! - **First definition wins** for a repeated identifier, matching the way
//!   pandoc and GFM treat repeated link labels.
//! - **An orphan reference stays plain text** and an orphan *definition* is
//!   dropped, so `foo[^missing]` emits no section at all.
//! - **`[^id]` inside `<code>` or `<pre>` is content**, not a reference. Those
//!   regions are masked before the scan and restored after it.
//! - **The backref goes inside the last `</p>`**, so the arrow sits beside the
//!   last word rather than after the paragraph.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// muya's `CODE_PLACEHOLDER_PREFIX`. The token cannot appear in rendered
/// output, which is what makes the restore step unambiguous.
const CODE_PLACEHOLDER_PREFIX: &str = "_MUYA_FN_GUARD_";

/// The post-processing pass. Returns `html` unchanged when it holds no
/// footnote definitions, exactly as muya's early return does.
#[must_use]
pub fn transform_footnotes(html: &str) -> String {
    // 1. Lift every footnote-block out of the body, keyed by identifier.
    let mut definitions: Vec<(String, String)> = Vec::new();
    let body = lift_definitions(html, &mut definitions);
    if definitions.is_empty() {
        return html.to_string();
    }

    // 2. Stash code spans and blocks so step 3 only scans live prose.
    let mut code_slots: Vec<&str> = Vec::new();
    let body = mask_code(&body, &mut code_slots);

    // 3. Number the inline references in source order.
    let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
    let mut order: Vec<(String, usize)> = Vec::new();
    let body = number_references(&body, &definitions, &mut numbers, &mut order);

    // 4. Restore the protected code regions.
    let body = restore_code(&body, &code_slots);

    if order.is_empty() {
        return body;
    }

    // 5. Build the section in numeric order. `order` is already in it.
    let mut items = Vec::with_capacity(order.len());
    for (id, n) in &order {
        let inner = definitions
            .iter()
            .find(|(key, _)| key == id)
            .map_or("", |(_, html)| html.as_str());
        items.push(format!(
            "<li id=\"fn-{n}\">{}</li>",
            append_backref(inner, *n)
        ));
    }

    let mut out = body.trim_end().to_string();
    let _ = write!(
        out,
        "\n\n<section class=\"footnotes\">\n<ol>\n{}\n</ol>\n</section>\n",
        items.join("\n")
    );
    out
}

/// `FOOTNOTE_DEF_RE` — `/<div class="footnote-block" data-identifier="([^"]*)">([\s\S]*?)<\/div>\s*/g`.
///
/// The lazy body means the match ends at the **first** `</div>`, so a
/// definition containing a raw `<div>` loses its tail. That is muya's
/// behaviour and it is reproduced rather than repaired: the alternative is a
/// nesting-aware scan that would disagree with the reference engine on an
/// input neither renders sensibly.
fn lift_definitions(html: &str, definitions: &mut Vec<(String, String)>) -> String {
    const OPEN: &str = "<div class=\"footnote-block\" data-identifier=\"";
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(found) = html[at..].find(OPEN).map(|i| at + i) {
        let id_start = found + OPEN.len();
        let Some(id_end) = html[id_start..].find('"').map(|i| id_start + i) else {
            break;
        };
        if !html[id_end + 1..].starts_with('>') {
            break;
        }
        let inner_start = id_end + 2;
        let Some(inner_end) = html[inner_start..].find("</div>").map(|i| inner_start + i) else {
            break;
        };
        let id = html[id_start..id_end].to_string();
        let inner = html[inner_start..inner_end].to_string();
        // First definition wins, matching pandoc / GFM on repeated labels.
        if !definitions.iter().any(|(key, _)| *key == id) {
            definitions.push((id, inner));
        }
        out.push_str(&html[at..found]);
        // The trailing `\s*` of the regex.
        let after = inner_end + "</div>".len();
        at = after + html[after..].len() - html[after..].trim_start().len();
    }
    out.push_str(&html[at..]);
    out
}

/// `CODE_GUARD_RE` — `/<(code|pre)\b[^>]*>[\s\S]*?<\/\1>/g`.
fn mask_code<'a>(html: &'a str, slots: &mut Vec<&'a str>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while at < html.len() {
        let Some(found) = next_code_open(html, at) else {
            break;
        };
        let (start, tag, body_at) = found;
        let close = format!("</{tag}>");
        let Some(end) = html[body_at..].find(&close).map(|i| body_at + i) else {
            break;
        };
        let end = end + close.len();
        out.push_str(&html[at..start]);
        let _ = write!(out, "{CODE_PLACEHOLDER_PREFIX}{}_", slots.len());
        slots.push(&html[start..end]);
        at = end;
    }
    out.push_str(&html[at..]);
    out
}

/// The next `<code…>` or `<pre…>` at or after `at`: `(tag_start, name, body_start)`.
fn next_code_open(html: &str, at: usize) -> Option<(usize, &'static str, usize)> {
    let mut i = at;
    while let Some(found) = html[i..].find('<').map(|x| x + i) {
        for tag in ["code", "pre"] {
            let after = found + 1 + tag.len();
            // **`is_char_boundary` and not merely `<= html.len()`.** `found` is
            // the index of a `<`, and the three or four bytes after it are only
            // a tag name if they *are* three or four characters — an accented
            // letter within four bytes of a `<` makes `after` land inside one,
            // and this slice then aborts the process. S7's soak reached it from
            // `render_to_static_html("é[^a]\n\n[^a]: n\n", …)`, which is an
            // ordinary document and not a hostile one; §11.3's *"malformed input
            // must never panic"* is about this and the bounds check alone was
            // not it. `is_char_boundary` is false past the end too, so it
            // subsumes the length check rather than sitting beside it. The two
            // slices below reuse `after` and were unreachable only because this
            // one aborted first.
            if html.is_char_boundary(after)
                && html[found + 1..after].eq_ignore_ascii_case(tag)
                // `\b` — the name may not run on into another word.
                && html[after..]
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_ascii_alphanumeric())
                && let Some(close) = html[after..].find('>').map(|x| x + after)
            {
                return Some((found, tag, close + 1));
            }
        }
        i = found + 1;
    }
    None
}

/// `FOOTNOTE_REF_RE` — `/\[\^([^[\]\s]+)\]/g`. Note the class is *wider* than
/// the block rule's: it admits `^`, so `[^^x]` is a reference to `^x`.
fn number_references(
    html: &str,
    definitions: &[(String, String)],
    numbers: &mut BTreeMap<String, usize>,
    order: &mut Vec<(String, usize)>,
) -> String {
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(found) = html[at..].find("[^").map(|i| at + i) {
        out.push_str(&html[at..found]);
        let id_start = found + 2;
        let id_len = html[id_start..]
            .find(|c: char| c == '[' || c == ']' || c.is_whitespace())
            .unwrap_or(0);
        let id = &html[id_start..id_start + id_len];
        if id.is_empty() || !html[id_start + id_len..].starts_with(']') {
            out.push_str("[^");
            at = id_start;
            continue;
        }
        let end = id_start + id_len + 1;
        if definitions.iter().any(|(key, _)| key == id) {
            let next = numbers.len() + 1;
            let n = *numbers.entry(id.to_string()).or_insert_with(|| {
                order.push((id.to_string(), next));
                next
            });
            let _ = write!(
                out,
                "<sup class=\"footnote-ref\"><a href=\"#fn-{n}\" id=\"fnref-{n}\">{n}</a></sup>"
            );
        } else {
            out.push_str(&html[found..end]);
        }
        at = end;
    }
    out.push_str(&html[at..]);
    out
}

fn restore_code(html: &str, slots: &[&str]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(found) = html[at..].find(CODE_PLACEHOLDER_PREFIX).map(|i| at + i) {
        let digits_at = found + CODE_PLACEHOLDER_PREFIX.len();
        let digits_len = html[digits_at..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(0);
        if digits_len == 0 || !html[digits_at + digits_len..].starts_with('_') {
            out.push_str(&html[at..digits_at]);
            at = digits_at;
            continue;
        }
        out.push_str(&html[at..found]);
        let index: usize = html[digits_at..digits_at + digits_len]
            .parse()
            .expect("a run of ASCII digits");
        out.push_str(slots.get(index).copied().unwrap_or_default());
        at = digits_at + digits_len + 1;
    }
    out.push_str(&html[at..]);
    out
}

/// `appendBackref` — inside the last `</p>` when there is one, after the block
/// otherwise.
fn append_backref(definition: &str, n: usize) -> String {
    let backref = format!(" <a href=\"#fnref-{n}\" class=\"footnote-backref\">↩</a>");
    match definition.rfind("</p>") {
        Some(at) => format!("{}{backref}{}", &definition[..at], &definition[at..]),
        None => format!("{definition}{backref}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_with_no_definitions_is_returned_unchanged() {
        let input = "<p>foo[^1]</p>\n";
        assert_eq!(transform_footnotes(input), input);
    }

    /// The shape `footnoteHtml.spec.ts` case 1 asserts, end to end.
    #[test]
    fn a_single_pair_becomes_a_sup_and_a_section() {
        let input = "<p>foo[^1]</p>\n<div class=\"footnote-block\" data-identifier=\"1\"><p>bar</p>\n</div>\n";
        let out = transform_footnotes(input);
        assert!(
            out.contains(
                "<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup>"
            ),
            "{out}"
        );
        assert!(out.contains("<section class=\"footnotes\">"), "{out}");
        assert!(out.contains("<li id=\"fn-1\">"), "{out}");
        assert!(
            out.contains("bar <a href=\"#fnref-1\" class=\"footnote-backref\">↩</a></p>"),
            "the backref goes inside the last </p>: {out}"
        );
        assert!(!out.contains("footnote-block"), "{out}");
    }

    /// Numbering is by inline order, not definition order — the assertion a
    /// naive implementation gets wrong.
    #[test]
    fn numbering_follows_the_first_inline_occurrence() {
        let input = "<p>A[^second] B[^first].</p>\n\
             <div class=\"footnote-block\" data-identifier=\"first\"><p>fd</p>\n</div>\n\
             <div class=\"footnote-block\" data-identifier=\"second\"><p>sd</p>\n</div>\n";
        let out = transform_footnotes(input);
        assert!(
            out.contains("A<sup class=\"footnote-ref\"><a href=\"#fn-1\""),
            "{out}"
        );
        assert!(
            out.contains("B<sup class=\"footnote-ref\"><a href=\"#fn-2\""),
            "{out}"
        );
        let one = out.find("<li id=\"fn-1\">").expect("fn-1");
        let two = out.find("<li id=\"fn-2\">").expect("fn-2");
        assert!(one < two);
        assert!(out[one..two].contains("sd"), "{out}");
    }

    #[test]
    fn an_orphan_reference_stays_plain_text_and_emits_no_section() {
        let input = "<p>foo[^missing] bar</p>\n\
             <div class=\"footnote-block\" data-identifier=\"other\"><p>x</p>\n</div>\n";
        let out = transform_footnotes(input);
        assert!(out.contains("[^missing]"), "{out}");
        assert!(!out.contains("<section class=\"footnotes\">"), "{out}");
        assert!(
            !out.contains("footnote-block"),
            "the block is still lifted: {out}"
        );
    }

    #[test]
    fn a_reference_inside_code_is_content() {
        let input = "<pre><code>[^x]\n</code></pre>\n\
             <div class=\"footnote-block\" data-identifier=\"x\"><p>d</p>\n</div>\n";
        let out = transform_footnotes(input);
        assert!(out.contains("<pre><code>[^x]\n</code></pre>"), "{out}");
        assert!(!out.contains("footnote-ref"), "{out}");
    }

    #[test]
    fn repeated_references_share_one_number_and_one_item() {
        let input = "<p>a[^x] b[^x] c[^x]</p>\n\
             <div class=\"footnote-block\" data-identifier=\"x\"><p>d</p>\n</div>\n";
        let out = transform_footnotes(input);
        assert_eq!(
            out.matches("<sup class=\"footnote-ref\">").count(),
            3,
            "{out}"
        );
        assert_eq!(out.matches("href=\"#fn-1\"").count(), 3, "{out}");
        assert_eq!(out.matches("<li id=\"fn-").count(), 1, "{out}");
    }

    /// First definition wins, as pandoc and GFM do for repeated link labels.
    #[test]
    fn the_first_definition_wins_for_a_repeated_identifier() {
        let input = "<p>a[^x]</p>\n\
             <div class=\"footnote-block\" data-identifier=\"x\"><p>first</p>\n</div>\n\
             <div class=\"footnote-block\" data-identifier=\"x\"><p>second</p>\n</div>\n";
        let out = transform_footnotes(input);
        assert!(out.contains("first"), "{out}");
        assert!(!out.contains("second"), "{out}");
    }

    /// A definition that ends in a list gets the backref tacked on after it,
    /// because there is no `</p>` to put it inside.
    #[test]
    fn a_definition_ending_in_a_list_gets_the_backref_after_the_block() {
        let input = "<p>a[^x]</p>\n\
             <div class=\"footnote-block\" data-identifier=\"x\"><ul>\n<li>i</li>\n</ul>\n</div>\n";
        let out = transform_footnotes(input);
        assert!(
            out.contains("</ul>\n <a href=\"#fnref-1\" class=\"footnote-backref\">↩</a></li>"),
            "{out}"
        );
    }

    /// **The most alarming of S7's five panics, and the only one not in
    /// `block.rs`.** `next_code_open` reads the three or four bytes after a `<`
    /// to see whether they spell `code` or `pre`, bounds-checking `after`
    /// against `html.len()` and never against a character boundary. An accented
    /// letter within four bytes of a `<` in the *rendered* HTML puts the cut
    /// inside it and aborts the process — on the public
    /// `render_to_static_html` path, from a document containing nothing more
    /// exotic than an `é`. §11.3: *"malformed input must never panic"*, and this
    /// input is not even malformed.
    ///
    /// The two slices after it reuse the same `after` and were unreachable only
    /// because this one aborted first, so the guard covers all three.
    #[test]
    fn a_tag_name_is_never_read_across_a_character_boundary() {
        let mut options = crate::Options::MUYA_DEFAULT;
        options.footnote = true;
        let out = crate::render_to_static_html("é[^a]\n\n[^a]: n\n", options, false)
            .expect("static html is implemented for this document");
        assert!(out.contains("<sup class=\"footnote-ref\""), "{out}");
        assert!(out.contains("<section class=\"footnotes\">"), "{out}");

        // The unit underneath, at every offset a multi-byte character can sit
        // at relative to the `<`: `code` and `pre` are 4 and 3 bytes, so a two-,
        // three- or four-byte character starting 1..=4 bytes after the `<` is
        // the whole reachable set.
        for pad in ["", "a", "aa", "aaa"] {
            for wide in ["é", "\u{3000}", "\u{1f600}"] {
                assert_eq!(
                    next_code_open(&format!("<{pad}{wide}x"), 0),
                    None,
                    "pad {pad:?} wide {wide:?}"
                );
            }
        }
        // And it still finds the tags it is for.
        assert_eq!(next_code_open("<code>x", 0), Some((0, "code", 6)));
        assert_eq!(next_code_open("é<pre>x", 0), Some((2, "pre", 7)));
    }
}
