//! `listMarkerOption.spec.ts` — 6 cases, transcribed as their expressible half.
//!
//! # What the original asserts, and what is transcribable here
//!
//! Each case boots a `Muya` instance with a `bulletListMarker` /
//! `orderListDelimiter` option, places the caret, runs
//! `muya.updateParagraph('ul-bullet')`, waits for the rAF state flush, and
//! then asserts two things: that the created list's `meta` carries the
//! configured marker, and that `getMarkdown()` emits it.
//!
//! The first clause is `muya.ts`'s command layer and `replaceBlockByLabel` —
//! M4's. The second is `stateToMarkdown` reading `meta.marker` /
//! `meta.delimiter` and emitting it verbatim, which is `mt_md::serialize` and
//! is M2's.
//!
//! So these transcribe the serializer half, exactly as M1 transcribed
//! `autoLinkEncoding.spec.ts`'s tokenizer half and owed the renderer half
//! forward. **Owed to M4:** that `updateParagraph` creates a list whose `meta`
//! carries the boot option. Recorded in M2.md §10 rather than left implicit.
//!
//! Each case therefore builds the list `updateParagraph` would have built —
//! one item, text `item` — and asserts the emitted marker.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "list_marker_option",

fn default_bullet_list_uses_a_dash_marker() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        false,
        vec![list_item(vec![para("item")])],
    )]);
    let md = serialize(&doc, NO_EXT);
    let first = md.lines().find(|l| !l.trim().is_empty()).expect("a line");
    assert!(first.starts_with("- "), "{md:?}");
}

fn a_star_bullet_list_marker_emits_star_markers() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Star,
        false,
        vec![list_item(vec![para("item")])],
    )]);
    let md = serialize(&doc, NO_EXT);
    let first = md.lines().find(|l| !l.trim().is_empty()).expect("a line");
    assert!(first.starts_with("* "), "{md:?}");
    assert!(!first.starts_with("- "), "{md:?}");
}

fn a_plus_bullet_list_marker_emits_plus_markers() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Plus,
        false,
        vec![list_item(vec![para("item")])],
    )]);
    let md = serialize(&doc, NO_EXT);
    let first = md.lines().find(|l| !l.trim().is_empty()).expect("a line");
    assert!(first.starts_with("+ "), "{md:?}");
}

fn default_ordered_list_uses_the_period_delimiter() {
    let doc = doc_of(vec![order_list(
        1,
        OrderDelim::Period,
        false,
        vec![list_item(vec![para("item")])],
    )]);
    let md = serialize(&doc, NO_EXT);
    let first = md.lines().find(|l| !l.trim().is_empty()).expect("a line");
    assert!(first.starts_with("1. "), "{md:?}");
}

fn a_paren_order_list_delimiter_emits_paren() {
    let doc = doc_of(vec![order_list(
        1,
        OrderDelim::Paren,
        false,
        vec![list_item(vec![para("item")])],
    )]);
    let md = serialize(&doc, NO_EXT);
    let first = md.lines().find(|l| !l.trim().is_empty()).expect("a line");
    assert!(first.starts_with("1) "), "{md:?}");
    assert!(!first.starts_with("1. "), "{md:?}");
}

/// The original distinguishes the `ol-order` and `ol-bullet` command labels;
/// both create an `order-list` and both read the same option, so at the
/// serializer the two are one claim — the delimiter survives a round trip as
/// well as a fresh serialize.
fn the_paren_delimiter_carries_to_the_ol_bullet_command_label_too() {
    let doc = doc_of(vec![order_list(
        1,
        OrderDelim::Paren,
        false,
        vec![list_item(vec![para("item")])],
    )]);
    let md = serialize(&doc, NO_EXT);
    assert!(md.starts_with("1) "), "{md:?}");

    let reparsed = parse(&md, NO_EXT);
    let list = top(&reparsed)[0];
    assert_eq!(
        meta(&reparsed, list),
        BlockMeta::OrderList { start: 1, delimiter: OrderDelim::Paren, loose: false }
    );
    assert_eq!(serialize(&reparsed, NO_EXT), md);
}

}
