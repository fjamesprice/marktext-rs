//! The `sanitize: true` arm of [`crate::render_to_static_html`] — `ammonia`
//! configured against muya's `EXPORT_DOMPURIFY_CONFIG`.
//!
//! # Why this is a configuration rather than a port
//!
//! Everything else in this crate is a transcription, because the reference
//! engine's behaviour *is* the specification. A sanitizer is the one place
//! where that reasoning inverts: DOMPurify's allowlist is a security artefact
//! maintained against live browser bypasses, and hand-transcribing it into
//! Rust would produce a snapshot that stops tracking the thing it was copied
//! from. §4's table names `ammonia` for exactly that reason, and this module
//! is the mapping between the two — not a reimplementation of either.
//!
//! # The four places the two cannot be made identical
//!
//! Measured against the running engine (jsdom + DOMPurify, which is the
//! environment `renderToStaticHTML.spec.ts` declares), and **not registered**:
//! `spec/divergences.json`'s rule 3 makes an entry stale unless a differential
//! case disagrees, and no harness in this repository compares sanitized HTML —
//! the conformance runner passes `sanitize: false` on purpose. M2.md §10
//! carries all four with this note.
//!
//! | | DOMPurify | here | direction |
//! |---|---|---|---|
//! | `style="…"` | kept, with the declaration list sanitized | **dropped** | more conservative |
//! | `<svg>` and its children | kept (`USE_PROFILES.svg`) | **dropped** | more conservative |
//! | an unlisted tag with safe content | unwrapped, content kept | unwrapped, content kept | same |
//! | a URL in a disallowed scheme | the attribute is dropped, the element stays | the attribute is dropped, the element stays | same |
//!
//! The two divergences both drop output rather than admit it, which is the
//! only direction a sanitizer may differ in without becoming a vulnerability.
//! Both are owed to `mt-export` at M6, which is the milestone that exports
//! styled documents and rendered diagrams and therefore the one that needs
//! them.
//!
//! # What is reproduced deliberately
//!
//! - **`data-*` attributes are dropped except `data-align`** —
//!   `ALLOW_DATA_ATTR: false` plus `ADD_ATTR: ['data-align']`. That is why
//!   [`crate::footnotes::transform_footnotes`] has to run *before* this pass:
//!   the `data-identifier` marker it reads would be gone by the time it looked.
//! - **`contenteditable` is dropped** — `FORBID_ATTR`.
//! - **`id` and `class` survive**, which the footnote section and the code
//!   fence's `language-…` class both depend on and which `ammonia`'s defaults
//!   do not allow.
//! - **No `rel="noopener noreferrer"` is added.** `ammonia` adds one by
//!   default; DOMPurify does not, and `footnoteHtml.spec.ts` asserts the exact
//!   attribute list of the backref anchor.
//! - **Comments are stripped**, which both do.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Sanitize rendered HTML for the `sanitize: true` path.
#[must_use]
pub fn clean(html: &str) -> String {
    BUILDER.clean(html).to_string()
}

/// Built once: `ammonia::Builder` owns several hash sets and rebuilding them
/// per call would put the allocation on the render path.
static BUILDER: LazyLock<ammonia::Builder<'static>> = LazyLock::new(build);

/// Tags `ammonia`'s default set does not carry and this renderer emits or
/// passes through.
///
/// `input` is the task-list checkbox, `section` and `ol`/`li` are the footnote
/// section, and the rest are `USE_PROFILES.html` members a markdown document
/// can legitimately contain as raw HTML.
const EXTRA_TAGS: [&str; 7] = [
    "input", "section", "main", "picture", "source", "label", "legend",
];

/// `EXPORT_DOMPURIFY_CONFIG.ALLOWED_URI_REGEXP`'s scheme list, verbatim.
///
/// `file` is there because of marktext #1997 — exporting images on Windows —
/// and is the one entry that would look like a mistake without the issue
/// number beside it.
const URL_SCHEMES: [&str; 10] = [
    "http", "https", "ftp", "ftps", "mailto", "tel", "callto", "cid", "xmpp", "file",
];

/// Attributes allowed on every tag: DOMPurify's `USE_PROFILES.html` essentials
/// plus `ADD_ATTR`'s one entry.
const GENERIC_ATTRIBUTES: [&str; 7] =
    ["class", "id", "title", "lang", "dir", "align", "data-align"];

fn build() -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::default();

    let tags: HashSet<&str> = builder.clone_tags().into_iter().chain(EXTRA_TAGS).collect();
    builder.tags(tags);

    let generic: HashSet<&str> = GENERIC_ATTRIBUTES.into_iter().collect();
    builder.generic_attributes(generic);

    // `<input disabled="" type="checkbox">` is what `Renderer.checkbox` emits
    // and what `renderToStaticHTML` keeps; without these three a task list
    // exports as an unmarked bullet list.
    let mut per_tag: HashMap<&str, HashSet<&str>> = builder.clone_tag_attributes();
    per_tag
        .entry("input")
        .or_default()
        .extend(["type", "checked", "disabled"]);
    builder.tag_attributes(per_tag);

    builder.url_schemes(URL_SCHEMES.into_iter().collect());
    // DOMPurify adds nothing to an anchor; `ammonia` adds
    // `rel="noopener noreferrer"` unless told not to.
    builder.link_rel(None);
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three assertions `renderToStaticHTML.spec.ts` makes about this arm.
    #[test]
    fn scripts_and_event_handlers_do_not_survive() {
        assert!(!clean("<script>alert(1)</script>").contains("script"));
        assert!(!clean("<script>alert(1)</script>").contains("alert"));
        let out = clean(r#"<a href="x" onclick="alert(1)">x</a>"#);
        assert!(!out.contains("onclick"), "{out}");
        assert!(!out.contains("alert(1)"), "{out}");
        assert!(out.contains("<a"), "the anchor itself stays: {out}");
    }

    /// `javascript:` is not in `ALLOWED_URI_REGEXP`, and DOMPurify drops the
    /// attribute rather than the element. Measured, then reproduced.
    #[test]
    fn a_javascript_url_loses_its_href_and_keeps_its_anchor() {
        let out = clean(r#"<a href="javascript:alert(1)">j</a>"#);
        assert!(!out.contains("javascript"), "{out}");
        assert!(out.contains(">j</a>"), "{out}");
    }

    /// #1997: `file:` is in the allowlist so Windows image exports survive.
    #[test]
    fn the_schemes_are_export_dompurify_configs() {
        assert!(clean(r#"<a href="file:///c/x.png">f</a>"#).contains("file:///c/x.png"));
        assert!(clean(r##"<a href="#fn-1">g</a>"##).contains("href=\"#fn-1\""));
        assert!(clean(r#"<a href="mailto:a@b.c">m</a>"#).contains("mailto:a@b.c"));
    }

    /// Everything the renderer emits has to survive its own sanitizer, and
    /// `ammonia`'s defaults drop three of these.
    #[test]
    fn the_renderers_own_output_survives() {
        let out = clean("<pre><code class=\"language-js\">x\n</code></pre>");
        assert!(out.contains("class=\"language-js\""), "{out}");

        let out =
            clean("<ul>\n<li><input checked=\"\" disabled=\"\" type=\"checkbox\"> a</li>\n</ul>\n");
        assert!(out.contains("type=\"checkbox\""), "{out}");
        assert!(out.contains("checked=\"\""), "{out}");
        assert!(out.contains("disabled=\"\""), "{out}");

        let out = clean(
            "<section class=\"footnotes\">\n<ol>\n<li id=\"fn-1\"><p>b \
             <a href=\"#fnref-1\" class=\"footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n",
        );
        assert!(out.contains("<section class=\"footnotes\">"), "{out}");
        assert!(out.contains("<li id=\"fn-1\">"), "{out}");
        assert!(
            out.contains("<a href=\"#fnref-1\" class=\"footnote-backref\">"),
            "no rel= is added, and the attribute order survives: {out}"
        );

        let out =
            clean("<table>\n<thead>\n<tr>\n<th align=\"left\">a</th>\n</tr>\n</thead>\n</table>\n");
        assert!(out.contains("align=\"left\""), "{out}");
    }

    /// `ALLOW_DATA_ATTR: false` with `ADD_ATTR: ['data-align']`, measured from
    /// the running engine rather than read off the config object.
    #[test]
    fn data_attributes_are_dropped_except_data_align() {
        let out = clean(r#"<p data-align="left" data-style="-" data-identifier="1">t</p>"#);
        assert!(out.contains("data-align=\"left\""), "{out}");
        assert!(!out.contains("data-style"), "{out}");
        assert!(
            !out.contains("data-identifier"),
            "this is why transform_footnotes runs first: {out}"
        );
    }

    #[test]
    fn contenteditable_and_comments_are_dropped() {
        assert!(!clean(r#"<p contenteditable="true">t</p>"#).contains("contenteditable"));
        assert_eq!(clean("<!-- a comment -->"), "");
    }

    /// The two documented divergences, asserted so that "more conservative
    /// than DOMPurify" is a checked claim rather than a sentence.
    #[test]
    fn style_and_svg_are_dropped_where_dompurify_keeps_them() {
        let out = clean(r#"<p style="color:red">t</p>"#);
        assert!(!out.contains("style"), "{out}");
        assert!(out.contains("<p>t</p>"), "{out}");

        let out = clean("<svg><circle r=\"1\"></circle></svg>");
        assert!(!out.contains("<svg"), "{out}");
    }
}
