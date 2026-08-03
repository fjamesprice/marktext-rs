//! The general target: any UTF-8 string, every invariant.
//!
//! RUST-REWRITE-PLAN.md §11.3 asks for `cargo-fuzz` on `mt-inline`, and §9's M1
//! exit gate is *"fuzzer runs 24 h without a panic"*. This is that target.
//!
//! # What a failure here means
//!
//! Four distinct things, and the `panic = "abort"` in the shipped release
//! profile is why they are all the same severity — a panic in the application
//! is a crash, not an exception:
//!
//! 1. **A panic.** The gate, directly.
//! 2. **A broken round trip.** `generator(tokenize(s)) != s` means a handler
//!    lost or duplicated a byte, which in the application is data loss.
//! 3. **A tiling failure.** §3 rule 1 — a gap, an overlap, or a child escaping
//!    its parent.
//! 4. **A `debug_assert!` from inside the crate.** Four of those are the
//!    milestone's proved-unreachable branches (M1.md §6), and this profile
//!    turns assertions on precisely so that a fuzzer can falsify a proof
//!    instead of agreeing with it.
//!
//! # `&str`, not `&[u8]`
//!
//! `libfuzzer-sys` will hand a target `&str` by rejecting non-UTF-8 inputs, and
//! that is the right contract here: `mt_inline::tokenize` takes `&str`, so
//! feeding it invalid UTF-8 is not a reachable state — the block layer above it
//! deals in `String`. Fuzzing bytes would spend the budget rediscovering UTF-8
//! rather than the tokenizer.
//!
//! The one thing that costs is that libFuzzer's mutations are byte-level and
//! most byte mutations of a multi-byte character produce invalid UTF-8, which
//! is rejected and wasted. The seed corpus is what compensates: `cargo xtask
//! fuzz-seed` writes the same 4,237 inputs the differential sweep uses, which
//! are real markdown and include `bench/corpus/cjk.md`, `emoji.md` and
//! `rtl.md`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_inline::{Token, generator, marker_state, tokenize, tokens_to_plain_text};

/// Every token span is one `Span::get` accepts, recursively.
///
/// `Span::of` panics by design on an out-of-bounds or non-`char`-boundary span
/// and `Span::get` is the total version, so this is the honest statement of the
/// invariant: **the tokenizer never produces a span `get` rejects.** A target
/// that built spans by hand and called `of` would find a panic instantly and it
/// would not be a bug.
fn check_spans(src: &str, tokens: &[Token]) {
    for token in tokens {
        assert!(
            token.raw.get(src).is_some(),
            "a `{}` token's raw {:?} is out of bounds or off a char boundary",
            token.type_str(),
            token.raw
        );
        assert_eq!(token.range, token.raw);
        if let Some(children) = token.children() {
            for child in children {
                assert!(
                    token.range.contains_span(child.range),
                    "a `{}` child escapes its `{}` parent",
                    child.type_str(),
                    token.type_str()
                );
            }
            check_spans(src, children);
        }
    }
}

/// The tokens of one level are contiguous and cover it exactly.
fn check_tiling(tokens: &[Token], start: usize, end: usize) {
    let mut cursor = start;
    for token in tokens {
        assert_eq!(
            token.raw.start,
            cursor,
            "a gap or overlap before a `{}`",
            token.type_str()
        );
        if let Some(children) = token.children()
            && let (Some(first), Some(last)) = (children.first(), children.last())
        {
            check_tiling(children, first.raw.start, last.raw.end);
        }
        cursor = token.raw.end;
    }
    assert_eq!(cursor, end, "the tokens stop short of the input");
}

fuzz_target!(|src: &str| {
    let tokens = tokenize(src);
    assert_eq!(generator(src, &tokens), src, "generator(tokenize(src)) != src");
    check_spans(src, &tokens);
    check_tiling(&tokens, 0, src.len());

    // The three consumers, each a walk that slices spans of its own.
    let _ = tokens_to_plain_text(src, &tokens);
    let _ = mt_inline::generator_rebuilding_wrappers(src, &tokens);
    for token in &tokens {
        let _ = marker_state(token, None);
    }
});
