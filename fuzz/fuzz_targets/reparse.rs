//! **S6's gate, driven from bytes**: an `Incremental` under a sequence of edits
//! equals a full parse of the string those edits produce.
//!
//! `docs/M2.md` §6's S6 row asks for the region reparse to equal a full reparse
//! *"over generated edit sequences"*, and §11.3's row puts `cargo-fuzz` on the
//! same path. `crates/mt-md/tests/reparse_properties.rs` is the structure-aware
//! half of that and this is the coverage-guided half. The difference is not
//! decoration:
//!
//! - `reparse_properties.rs` draws its edits from a **table** of §4.1's own
//!   triggers (`INSERTS`) at positions uniform in `0..4096`, over a fixed set of
//!   shapes and fixtures. It samples the space somebody thought of.
//! - This draws the source **and** every inserted string from the fuzzer, so the
//!   insertion alphabet is unbounded, and libFuzzer keeps the byte sequences
//!   that reach new branches. It searches the space nobody thought of.
//!
//! Neither subsumes the other, and this file adds no second proptest to
//! `reparse_properties.rs` for exactly that reason — §6's S7 row asks for the
//! fuzz target, not for a wider generator next door.
//!
//! # What "equal" means, and it is `reparse_properties.rs`'s definition
//!
//! `mt_md::dump_state`'s JSON — names, `meta` and leaf text, which is what
//! `cargo xtask diff` and `cargo xtask blocks` compare — **plus** the leaf
//! source ranges in tree order, **plus** the label map's size. The ranges are
//! the half a tree comparison would miss and `SourceMap`'s own docs promise
//! them: *"S6's region reparse produces new ones for the region rather than
//! patching these"*. See [`leaf_ranges`] for the three narrowings that
//! comparison needs and why each one is `pulldown-cmark`'s trailing-whitespace
//! inconsistency rather than a hole.
//!
//! What is **not** here is `reparse_properties.rs`'s `ReparsePath` arithmetic —
//! that a leaf-text edit reparses exactly one block and renumbers nothing. That
//! is a claim about the *fast path being taken*, it is asserted there over
//! fixtures where the path is reachable by construction, and duplicating it
//! here would make one change fail in two places for one reason.
//!
//! # Structured input, unlike the other two `mt-md` targets
//!
//! `parse.rs` and `round_trip.rs` take `&str` because `mt_md::parse` does. This
//! target's input is a *session* — a document and a list of edits — so it takes
//! a derived [`Arbitrary`] struct instead and lets `libfuzzer-sys` cut the byte
//! string into one. `arbitrary` is a dependency of `fuzz/` alone, which the root
//! `Cargo.toml` anticipates by name beside `libfuzzer-sys`.
//!
//! Offsets are resolved onto `char` boundaries before being applied, the way
//! `reparse_properties.rs`'s `resolve` does: `Incremental::edit` takes byte
//! offsets into a `&str`, so an offset off a boundary is a **caller** bug and
//! finding it here would be the target reporting its own mistake.

#![no_main]

use std::ops::Range;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mt_doc::{Document, NodeId};
use mt_md::reparse::Incremental;
use mt_md::{Options, SourceMap};

#[path = "known_panics.rs"]
mod known_panics;

/// A document and the edits made to it.
///
/// `String` and not `&'a str` deliberately: a borrowed field would make this
/// type carry a lifetime, and `fuzz_target!`'s expansion is clearer without one.
/// The allocation is nothing beside the full parse each edit is checked against.
#[derive(Debug, Arbitrary)]
struct Session {
    /// Which option set to drive — see [`option_set`].
    options: u8,
    source: String,
    edits: Vec<Edit>,
}

/// One edit, in `Incremental::edit`'s own `(at, remove, insert)` shape.
///
/// `u32`/`u16` rather than `usize` so that a single edit costs six bytes of the
/// fuzzer's budget instead of sixteen; both are reduced modulo the source length
/// by [`resolve`], so the narrower types lose no reachable position.
#[derive(Debug, Arbitrary)]
struct Edit {
    at: u32,
    remove: u16,
    insert: String,
}

/// How many edits of a session are applied.
///
/// Each one costs a full parse of the whole document to check, so an unbounded
/// `Vec` from a 64 KB input would spend the entire budget on one session.
/// `reparse_properties.rs` uses 1..6 for the same reason; this is looser because
/// a fuzzer that finds a bug on the ninth edit should be able to.
const MAX_EDITS: usize = 16;

/// The three option sets, chosen by the fuzzer rather than swept.
///
/// Sweeping all three per session would triple the cost of every case; letting
/// the fuzzer pick means the byte that selects one is part of the corpus and a
/// session that only fails at `MUYA_DEFAULT` is a reproducer that keeps failing.
///
/// The third is the one `reparse_properties.rs` calls *"the option no harness in
/// the repository drives"* — footnotes on, whose segmentation forces the whole
/// document rather than a region.
fn option_set(which: u8) -> (&'static str, Options) {
    match which % 3 {
        0 => ("SPEC", Options::SPEC),
        1 => ("MUYA_DEFAULT", Options::MUYA_DEFAULT),
        _ => (
            "MUYA_DEFAULT+footnote",
            Options {
                footnote: true,
                ..Options::MUYA_DEFAULT
            },
        ),
    }
}

fn tree_ids(doc: &Document) -> Vec<NodeId> {
    fn walk(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
        for child in doc.children(id) {
            out.push(*child);
            walk(doc, *child, out);
        }
    }
    let mut out = Vec::new();
    walk(doc, doc.root(), &mut out);
    out
}

/// Every leaf's range, in tree order, as the **content** it names rather than as
/// two numbers.
///
/// Three narrowings, all measured by S6, and all of them the same phenomenon S4
/// named as *"the trap nobody named"*: **`pulldown-cmark`'s ranges are not
/// consistent about the newline and the whitespace that end a block.** A region
/// has an end and a document does not, so the two parses see the same block with
/// a different amount of trailing nothing after it.
///
/// - **Containers are excluded.** The blank line before the next block is inside
///   a list mid-document and not at end of input, so a list's range can end two
///   bytes earlier. Its children's do not.
/// - **Zero-width leaves are excluded** — the synthetic empty paragraph an empty
///   block quote or list item gets (muya #1735), whose range is
///   `container.end..container.end` and therefore inherits the same bytes.
/// - **The end is trimmed.** A leaf's range can end one byte later after a
///   leaf-text edit, on the trailing newline, because the fast path moves the
///   range by the edit's delta and `pulldown-cmark` re-decides whether that
///   newline is inside the block.
///
/// What is compared is therefore the leaf's start and the content its range
/// names with trailing whitespace off — the whole of what a caret mapping or
/// §10's per-leaf reverse map can read out of it, since the text itself is
/// already asserted identical by the tree comparison.
///
/// **Nesting is not asserted, here or anywhere in this directory that touches
/// `mt-md`.** §10's second known non-bug: on `"-\t- [a]: /x[xter\n"` a child's
/// range escapes its parent's, because a tab inside a list item's marker padding
/// is the one case where a stripped line's text is not a slice of the source.
/// `fuzz_targets/tokenize.rs` **does** assert nesting, for `mt-inline` tokens,
/// and that is correct there — the difference is stated so that the next reader
/// does not "restore" it here.
fn leaf_ranges<'a>(doc: &Document, map: &SourceMap, src: &'a str) -> Vec<(usize, &'a str)> {
    tree_ids(doc)
        .into_iter()
        .filter(|id| doc.block(*id).is_some_and(|b| b.is_leaf()))
        .filter_map(|id| map.get(id))
        .filter(|range: &Range<usize>| range.start < range.end)
        .map(|range| (range.start, src[range].trim_end()))
        .collect()
}

/// Snap a generated `(at, remove)` onto `char` boundaries of `source`.
fn resolve(source: &str, edit: &Edit) -> (usize, usize) {
    if source.is_empty() {
        return (0, 0);
    }
    let mut at = (edit.at as usize) % (source.len() + 1);
    while at < source.len() && !source.is_char_boundary(at) {
        at += 1;
    }
    let mut end = (at + edit.remove as usize).min(source.len());
    while end < source.len() && !source.is_char_boundary(end) {
        end += 1;
    }
    (at, end - at)
}

/// The whole of the claim, for one document after one edit.
fn agrees_with_a_full_parse(incremental: &Incremental, options: Options, what: &str) {
    let full = mt_md::parse(incremental.source(), options);

    assert_eq!(
        mt_md::state::to_state_json(incremental.document()),
        mt_md::state::to_state_json(&full.document),
        "{what}: the trees disagree"
    );
    assert_eq!(
        leaf_ranges(
            incremental.document(),
            incremental.source_map(),
            incremental.source()
        ),
        leaf_ranges(&full.document, &full.source_map, incremental.source()),
        "{what}: the leaf source ranges disagree"
    );
    assert_eq!(
        incremental.source_map().len(),
        tree_ids(incremental.document()).len(),
        "{what}: the map holds an entry that is not a live node, or misses one"
    );
    assert_eq!(
        incremental.labels().len(),
        full.labels.len(),
        "{what}: the label maps disagree"
    );
}

fuzz_target!(|session: Session| {
    let (label, options) = option_set(session.options);

    // §10's deferred upstream panic, on the *source*. `known_panics.rs` says at
    // length why this is a pre-check and not a `catch_unwind`.
    if known_panics::is_the_known_upstream_panic(&session.source) {
        return;
    }

    let mut doc = Incremental::new(&session.source, options);
    let mut history = String::new();
    for (step, edit) in session.edits.iter().take(MAX_EDITS).enumerate() {
        let (at, remove) = resolve(doc.source(), edit);

        // The **edited** string, guarded before it is applied — the easy one to
        // miss. `Incremental::edit` parses a region of it and
        // `agrees_with_a_full_parse` parses the whole of it, so a sequence can
        // walk into §10's shape from a source that is not it. Stopping the
        // sequence rather than skipping the step keeps the remaining edits'
        // offsets meaningful.
        let mut edited = doc.source().to_string();
        edited.replace_range(at..at + remove, &edit.insert);
        if known_panics::is_the_known_upstream_panic(&edited) {
            return;
        }

        history.push_str(&format!(
            "\n  [{step}] {at}..{} → {:?}",
            at + remove,
            edit.insert
        ));
        doc.edit(at, remove, &edit.insert);
        agrees_with_a_full_parse(
            &doc,
            options,
            &format!("{label}: {:?}{history}", session.source),
        );
    }
});
