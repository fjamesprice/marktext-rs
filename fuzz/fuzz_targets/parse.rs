//! **The M2 gate itself, coverage-guided**: any UTF-8 string through
//! `mt_md::parse` and every consumer of what it returns.
//!
//! RUST-REWRITE-PLAN.md §11.3 asks for `cargo-fuzz` *"on the parse/serialize
//! path"* and states the clause this target exists for in one sentence:
//!
//! > Malformed input must never panic — `panic = "abort"` makes a panic a crash.
//!
//! `docs/M2.md` §6's S7 row is where that falls due. This is the general target
//! of the three, the way `tokenize.rs` is the general target of `mt-inline`'s.
//!
//! # What a failure here means
//!
//! Four things, and `panic = "abort"` in §12's shipped release profile is why
//! they are all the same severity — in the application a panic is a crash, not
//! an exception:
//!
//! 1. **A panic**, from `parse`, `serialize`, `dump_state` or
//!    `render_to_static_html` at either `sanitize` value. The gate, directly.
//! 2. **A range that cannot be sliced.** M4 places a caret by slicing a
//!    `SourceMap` range on every click; a range off a `char` boundary or past
//!    the end is a panic in M4 with no reproducer.
//! 3. **A node with no range, or a range with no node.** S6's region reparse and
//!    §10's per-leaf reverse map both walk the tree and ask the map, and both
//!    assume the answer exists.
//! 4. **`Incremental::new` disagreeing with `parse`.** Everything
//!    `tests/reparse_properties.rs` proves about *edited* documents is proved
//!    about a starting state this claim is what pins.
//!
//! # `&str`, not `&[u8]`
//!
//! `libfuzzer-sys` hands a target `&str` by rejecting non-UTF-8 inputs, and that
//! is the right contract: `mt_md::parse` takes `&str`, so invalid UTF-8 is not a
//! reachable state — `mt_md::normalize_source` and the file layer above it deal
//! in `String`. Fuzzing bytes would spend the budget rediscovering UTF-8 rather
//! than the parser. `tokenize.rs` gives the same reason one layer down.
//!
//! What that costs is that libFuzzer's mutations are byte-level and most byte
//! mutations of a multi-byte character produce invalid UTF-8, which is rejected
//! and wasted. The seed corpus is what compensates: `cargo xtask fuzz-seed`
//! writes **whole documents** into this target's directory rather than the
//! line-shaped inputs the three `mt-inline` targets get, because a block-layer
//! target seeded with single lines starts from a frontier with no block
//! structure in it.
//!
//! # What is **not** asserted, and it is not an oversight
//!
//! **Source ranges are not asserted to nest.** `docs/M2.md` §10's second known
//! non-bug: on `"-\t- [a]: /x[xter\n"` — a tab inside a list item's marker
//! padding — the innermost paragraph's range is `1..13` inside an item at
//! `2..17`, because the stripped line's text is not a slice of the source and
//! the scanner's range is computed in stripped space.
//! `crates/mt-md/tests/source_ranges.rs` asserts nesting **over the corpus**,
//! which contains no such document, and
//! `mt_md::block::tests::the_ranges_are_not_injective_and_do_not_always_nest`
//! is the input that says the invariant is about the corpus rather than about
//! every string.
//!
//! **`fuzz_targets/tokenize.rs` does assert nesting**, for `mt-inline` tokens,
//! and that is correct there: a token tree is built by slicing the input and
//! `check_spans` is the shape of the claim. Asserting it here would fail on a
//! string §10 already names. The difference is written down so that the next
//! reader does not "restore" it.
//!
//! Nor is **injectivity**: S5 made the map deliberately non-injective, since
//! footnote nodes share the definition's range. `SourceMap::is_exact` is how a
//! caller sees the difference, and §8's risk row — *"a range that is a coarse
//! stand-in is sliced with as though it were exact"* — is why the slice below is
//! taken only where `is_exact` says it may be.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_doc::{Document, NodeId};
use mt_md::reparse::Incremental;
use mt_md::{Options, parse};

#[path = "known_panics.rs"]
mod known_panics;

/// The two option sets every claim is made at.
///
/// `SPEC` is what the 1324 spec examples are driven with and `MUYA_DEFAULT` is
/// what the whole documents are; the two differ in `math`, `front_matter` and
/// `gitlab_compatibility` — three flags that change which *block kinds exist*,
/// not merely how they render. A property proved at one of them says nothing
/// about the other. `crates/mt-md/tests/round_trip_properties.rs` carries the
/// same pair for the same reason.
const OPTION_SETS: [(&str, Options); 2] = [
    ("SPEC", Options::SPEC),
    ("MUYA_DEFAULT", Options::MUYA_DEFAULT),
];

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

fuzz_target!(|src: &str| {
    // The one class §10 defers to M4. Checked once, before anything, because
    // every call below reaches `pulldown-cmark`'s `OffsetIter`.
    if known_panics::is_the_known_upstream_panic(src) {
        return;
    }

    for (label, options) in OPTION_SETS {
        let parsed = parse(src, options);

        // --- claim 1: totality, over every public entry point ---------------
        //
        // Called and — deliberately — **not** asserted about. `dump_state`'s
        // JSON is compared against the reference engine by `cargo xtask blocks`
        // over 1344 inputs and `render_to_static_html`'s output by `cargo xtask
        // conformance` over 1324; neither has an equality against `src` that
        // could be checked here, and an assertion that encodes a guess fails on
        // a correct input. What all of them do have to be is total, and calling
        // them is the whole check.
        //
        // `sanitize` is driven at **both** values because the sanitiser is a
        // second walk of a second tree (`ammonia`) and §11.3's row is about the
        // whole path, not about the parser.
        let _ = mt_md::serialize(&parsed.document, options);
        let _ = mt_md::dump_state(src, options);
        let _ = mt_md::render_to_static_html(src, options, false);
        let _ = mt_md::render_to_static_html(src, options, true);

        // --- claim 2: every range is one `&src[range]` accepts --------------
        for (id, range) in parsed.source_map.iter() {
            assert!(
                range.start <= range.end && range.end <= src.len(),
                "{label}: {range:?} is not inside 0..{} for node {id:?} of {src:?}",
                src.len()
            );
            assert!(
                src.is_char_boundary(range.start) && src.is_char_boundary(range.end),
                "{label}: {range:?} is not on a character boundary in {src:?}"
            );
            // Only where the range is the node's own extent — §8's risk row.
            if parsed.source_map.is_exact(id) {
                let _ = &src[range];
            }
        }

        // --- claim 3: one entry per live node, no more and no fewer ---------
        let ids = tree_ids(&parsed.document);
        assert!(
            parsed.source_map.get(parsed.document.root()).is_none(),
            "{label}: the root has no block and must have no range, on {src:?}"
        );
        for id in &ids {
            assert!(
                parsed.source_map.get(*id).is_some(),
                "{label}: a live node carries no range, on {src:?}"
            );
        }
        assert_eq!(
            parsed.source_map.len(),
            ids.len(),
            "{label}: the map holds an entry that is not a live node, or misses one, on {src:?}"
        );

        // --- claim 5: the zero-edit identity --------------------------------
        //
        // Cheap, and it pins the two entry points together. `reparse.rs`
        // compares an *edited* `Incremental` against a full parse, which says
        // nothing about the state it started from: if `new` ever stops being
        // `parse` — a pre-pass, a normalisation, a cached anything — every one
        // of S6's properties would still pass while measuring a different
        // document from the one the caller asked for.
        let incremental = Incremental::new(src, options);
        assert_eq!(
            mt_md::state::to_state_json(incremental.document()),
            mt_md::state::to_state_json(&parsed.document),
            "{label}: Incremental::new produced a different tree from parse, on {src:?}"
        );
        assert!(
            incremental.source_map() == &parsed.source_map,
            "{label}: Incremental::new produced a different source map from parse, on {src:?}"
        );
        assert!(
            incremental.labels() == &parsed.labels,
            "{label}: Incremental::new produced a different label map from parse, on {src:?}"
        );
        assert_eq!(
            incremental.source(),
            src,
            "{label}: Incremental::new did not keep the source it was given"
        );
    }
});
