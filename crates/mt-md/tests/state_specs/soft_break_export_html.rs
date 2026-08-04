//! `softBreakExportHtml.spec.ts` — 3 cases.
//!
//! #3676. A soft line break (Shift+Enter, serialized as a bare `\n` inside a
//! block) shows as a line break in the editor — `.mu-content` is `pre-wrap` —
//! but was lost on export, because marked renders a soft break as a space.
//! Rather than emit a non-standard `<br>` (CommonMark reserves that for hard
//! breaks), the export keeps the conformant `\n` and renders it with
//! `white-space: pre-wrap`.
//!
//! The pre-wrap rendering itself is a CSS concern verified in a real browser;
//! what is asserted here is that the HTML stays conformant.
//!
//! The TypeScript original calls `getHighlightHtml` directly. That is
//! `renderToStaticHTML`'s internals — §4 C3: `renderToStaticHTML` *is*
//! `getHighlightHtml(markdown, …)` plus a sanitize pass — so these transcribe
//! against `render_to_static_html` with `sanitize: false`, which is the flag
//! that skips the pass.

#![allow(unused_imports)]
use crate::*;

/// The spec's `OPTS`: `{ math: false, superSubScript: false, footnote: false,
/// frontMatter: false }`.
const OPTS: Options = NO_EXT;

/// `<br>` or `<br/>` or `<br />`.
fn has_a_br(html: &str) -> bool {
    html.contains("<br>") || html.contains("<br/>") || html.contains("<br />")
}

spec_cases! { "soft_break_export_html",

fn keeps_a_paragraph_soft_break_as_a_newline_never_a_br() {
    let out = html("line one\nline two", super::OPTS, false);
    assert!(out.contains("<p>line one\nline two</p>"), "{out:?}");
    assert!(!super::has_a_br(&out), "{out:?}");
}

fn keeps_a_soft_break_inside_a_tight_list_item_never_a_br() {
    let out = html("- line A\n  line B", super::OPTS, false);
    assert!(out.contains("<li>line A\nline B</li>"), "{out:?}");
    assert!(!super::has_a_br(&out), "{out:?}");
}

/// Sanity: the change only touches *soft* breaks. Two trailing spaces is a
/// hard break and stays a `<br>`.
fn leaves_a_real_hard_break_as_a_br() {
    let out = html("line one  \nline two", super::OPTS, false);
    assert!(super::has_a_br(&out), "{out:?}");
}

}
