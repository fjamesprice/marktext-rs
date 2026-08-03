//! `trimAutoLinkExtent` — **the one thing `BACKTRACK_LIMIT` does not bound.**
//!
//! M1.md §4 C1 says the fuzzing gate should be pointed first at the
//! ReDoS-class patterns `rules.ts` disables its lint for, and `backtracking.rs`
//! is that target. This one is pointed somewhere the limit cannot reach at all,
//! and S5 recorded it for whoever built this:
//!
//! > The trim itself is quadratic in the extent, because the paren rule
//! > recounts `raw[0..end]` on every pass and `end` moves by one. 1.44× on a
//! > 2000-character tail. muya is quadratic there too, so this is faithfulness
//! > rather than a regression, but it is the one part of S5 that is **not**
//! > bounded by the backtrack limit — it is hand-written Rust, not a regex —
//! > and S7's fuzzer should know that.
//!
//! `rules::BACKTRACK_LIMIT` caps a *regex* at 1,000,000 steps per attempt and
//! degrades to "the rule did not match". Nothing caps a hand-written loop. So
//! the failure this target is looking for is not only a panic:
//!
//! - **A hang.** `trim_auto_link_extent` returning 0 would make the handler
//!   consume nothing while reporting `true`, which is an infinite loop in muya
//!   and would be one here without the guard at `lexer.rs`'s `raw.is_empty()`.
//!   That guard now carries a `debug_assert!`, so this profile turns the hang
//!   into a crash libFuzzer can report — which is the only way a hang gets a
//!   reproducer instead of a timeout.
//! - **A slow unit.** libFuzzer reports any input taking longer than
//!   `-report_slow_units` seconds. Quadratic-in-the-extent means a long enough
//!   trailing run of `)`/`.`/`&` finds the knee, and whatever it finds belongs
//!   in `benches/tokenizer.rs` as a row.
//!
//! # Why the input is shaped
//!
//! `try_auto_link_extension` is gated on `state.top` and on an allow-list of
//! preceding characters, and the rule only fires after `www.`, `http://`,
//! `https://` or an email local part. A general fuzzer reaches the trim by
//! accident; this one starts every input there, so the whole budget is spent
//! on the extent rather than on finding the opener.
//!
//! The four shapes cover the trim's four interleaving rules — the `<` cut, the
//! trailing-punctuation set, the unbalanced `)`, and the `&entity;` jump —
//! which is the interleaving S5 had to sweep 42,130 inputs to trust.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_inline::{generator, tokenize};

/// The openers that reach `trim_auto_link_extent`.
///
/// The email alternative is here even though the trim is skipped for it: that
/// skip is muya's `if (!email)` guard, the third of the milestone's four
/// proved-unreachable branches, and `lexer.rs` now `debug_assert!`s that the
/// trim would have been the identity anyway. Feeding emails is how that proof
/// gets attacked.
const OPENERS: [&str; 5] = [
    "https://example.com/",
    "http://x.y/",
    "www.example.com/",
    "user@example.com",
    "a.b_c@d.example",
];

fuzz_target!(|src: &str| {
    for opener in OPENERS {
        let mut input = String::with_capacity(opener.len() + src.len() + 2);
        input.push_str(opener);
        input.push_str(src);
        let tokens = tokenize(&input);
        assert_eq!(generator(&input, &tokens), input);

        // The same extent behind a nested level and after a boundary
        // character, because `state.top` and the preceding-character
        // allow-list are what decide whether the trim runs at all — and a
        // change to either would otherwise make this target silently stop
        // reaching the function it exists for.
        let nested = format!("**{input}**");
        let nested_tokens = tokenize(&nested);
        assert_eq!(generator(&nested, &nested_tokens), nested);

        let after_boundary = format!("({input}");
        let boundary_tokens = tokenize(&after_boundary);
        assert_eq!(generator(&after_boundary, &boundary_tokens), after_boundary);
    }
});
