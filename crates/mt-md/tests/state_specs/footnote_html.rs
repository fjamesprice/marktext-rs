//! `footnoteHtml.spec.ts` — 6 cases.
//!
//! PR-8c: footnote backrefs and numbering for `renderToStaticHTML`. The marked
//! footnote extension lands the block definition as
//! `<div class="footnote-block" data-identifier="…">`; this pins the
//! post-processed shape —
//! `<sup class="footnote-ref"><a href="#fn-N" id="fnref-N">N</a></sup>` at each
//! inline reference, plus a final `<section class="footnotes"><ol>` with
//! `<a class="footnote-backref">` arrows on each `<li id="fn-N">`.
//!
//! Numbering follows pandoc / GFM: **by first-occurrence order of the inline
//! references**, not by where the definitions appear in the source. That is
//! the assertion a naive implementation gets wrong, and it is case 2.

#![allow(unused_imports)]
use crate::*;

/// `{ footnote: true }` — the spec's `PROFILE`.
const PROFILE: Options = Options {
    footnote: true,
    ..NO_EXT
};

/// The text between the first `<li id="fn-N">` and its matching `</li>`, or
/// the whole footnotes section when nesting makes that ambiguous.
fn footnotes_section(html: &str) -> Option<&str> {
    let start = html.find("<section class=\"footnotes\">")?;
    let end = html.rfind("</section>")? + "</section>".len();
    Some(&html[start..end])
}

spec_cases! { "footnote_html",

fn emits_a_footnotes_section_with_an_li_for_a_single_ref_and_def_pair() {
    let out = html("foo[^1]\n\n[^1]: bar", super::PROFILE, true);

    assert!(super::footnotes_section(&out).is_some(), "{out:?}");
    assert!(out.contains("<li id=\"fn-1\">"), "{out:?}");
    assert!(
        out.contains("<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup>"),
        "{out:?}"
    );
    assert!(out.contains("<a href=\"#fnref-1\" class=\"footnote-backref\">"), "{out:?}");
    assert!(out.contains("bar"), "{out:?}");
    // The raw `[^1]` must not survive — it was rewritten to the sup link.
    assert!(!out.contains("<p>foo[^1]</p>"), "{out:?}");
    // And the intermediate wrapper is hoisted into the section, not left
    // dangling in the body.
    assert!(!out.contains("<div class=\"footnote-block\""), "{out:?}");
}

/// `[^second]` appears first inline → `fn-1`; `[^first]` appears second →
/// `fn-2`, whatever order the definitions are written in.
fn numbers_inline_references_in_source_order() {
    let md = "A[^second] B[^first].\n\n[^first]: first definition\n\n[^second]: second definition";
    let out = html(md, super::PROFILE, true);

    assert!(out.contains("A<sup class=\"footnote-ref\"><a href=\"#fn-1\""), "{out:?}");
    assert!(out.contains("B<sup class=\"footnote-ref\"><a href=\"#fn-2\""), "{out:?}");

    let fn1 = out.find("<li id=\"fn-1\">").expect("fn-1");
    let fn2 = out.find("<li id=\"fn-2\">").expect("fn-2");
    assert!(fn1 < fn2, "the section lists items in numeric order: {out:?}");
    assert!(
        out[fn1..fn2].contains("second definition"),
        "fn-1 carries the body of [^second]: {out:?}"
    );
    assert!(out[fn2..].contains("first definition"), "{out:?}");
}

fn leaves_an_orphan_inline_reference_as_plain_text() {
    let out = html("foo[^missing] bar", super::PROFILE, true);
    assert!(
        !out.contains("<section class=\"footnotes\">"),
        "no section — there is nothing to list: {out:?}"
    );
    assert!(out.contains("[^missing]"), "{out:?}");
    assert!(!out.contains("<sup class=\"footnote-ref\""), "{out:?}");
}

fn points_every_repeated_inline_reference_to_the_same_target() {
    let md = "First [^x]. Again [^x]. Once more [^x].\n\n[^x]: shared body";
    let out = html(md, super::PROFILE, true);

    assert_eq!(
        out.matches("<sup class=\"footnote-ref\">").count(),
        3,
        "three inline refs: {out:?}"
    );
    assert_eq!(
        out.matches("href=\"#fn-1\"").count(),
        3,
        "all three point at fn-1: {out:?}"
    );
    assert_eq!(
        out.matches("<li id=\"fn-").count(),
        1,
        "only one entry in the list: {out:?}"
    );
    assert!(out.contains("<li id=\"fn-1\">"), "{out:?}");
}

/// A naive `<li id="fn-1">…</li>` regex bails at the first inner `</li>` of
/// the nested list, so the assertion is over the whole section slice.
fn preserves_a_footnote_definition_containing_a_nested_bullet_list() {
    let md = "text[^n]\n\n[^n]: intro\n\n    - item a\n    - item b\n";
    let out = html(md, super::PROFILE, true);

    let section = super::footnotes_section(&out).expect("a footnotes section");
    assert!(section.contains("<li id=\"fn-1\">"), "{section:?}");
    assert!(section.contains("intro"), "{section:?}");
    assert!(section.contains("<ul>"), "{section:?}");
    assert!(section.contains("<li>item a</li>"), "{section:?}");
    assert!(section.contains("<li>item b</li>"), "{section:?}");
}

/// `[^code-only]` inside a fence is content, not a reference.
fn does_not_transform_a_literal_reference_inside_a_fenced_code_block() {
    let out = html("```\n[^code-only]\n```\n", super::PROFILE, true);
    assert!(out.contains("[^code-only]"), "{out:?}");
    assert!(!out.contains("<sup class=\"footnote-ref\""), "{out:?}");
    assert!(!out.contains("<section class=\"footnotes\">"), "{out:?}");
}

}
