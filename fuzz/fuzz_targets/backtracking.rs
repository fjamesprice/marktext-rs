//! The ReDoS-class patterns — M1.md §4 C1's *"what the M1 fuzzing gate should
//! be pointed at first"*.
//!
//! > `fancy-regex` is a backtracking engine, so the ReDoS-class patterns
//! > `rules.ts` disables lint rules for (`no-super-linear-backtracking` appears
//! > **7 times**) are genuinely super-linear here too. Input is the user's own
//! > block text, not network data, so this is a latency question and not a
//! > security one — but it is what the M1 fuzzing gate should be pointed at
//! > first. `fancy-regex` has a backtrack limit; set it explicitly and decide
//! > what exceeding it means.
//!
//! # What "failure" means here, and what it does **not**
//!
//! `rules::BACKTRACK_LIMIT` is 1,000,000 steps **per rule attempt**, and
//! exceeding it means *the rule did not match* — never a panic. C1 decided that
//! deliberately, and the first of its three reasons is that the exit gate is
//! panic-freedom and a resource limit that panics **is** the panic the gate
//! forbids.
//!
//! So an input that reaches the limit is **not a bug**, and nobody should
//! "fix" it. C1 says what it is:
//!
//! > Nothing in `bench/corpus/` or `spec/fixtures/` comes close. If one ever
//! > does, it is a `spec/divergences.json` entry like any other deliberate
//! > difference.
//!
//! The tokenizer stays total either way — a rule that gives up leaves its
//! characters to accumulate as text, which is what a reader sees for any
//! construct that fails to parse, and the tiling invariant is untouched. That
//! is what this target asserts.
//!
//! What *would* be a failure: a panic, a broken round trip, or a **slow unit**.
//! The last is the interesting one and it is why this target exists separately
//! from `tokenize.rs` — libFuzzer reports any input exceeding
//! `-report_slow_units`, and a target whose every input is already
//! quantifier-shaped finds the knee far faster than one starting from bytes.
//!
//! # The three cost shapes this is aimed at
//!
//! S2–S5 built a three-term model of what a rule costs, each term measured by
//! the stage that hit it (`benches/tokenizer.rs`):
//!
//! 1. **How far a lazy quantifier scans when it fails** (S2, S3) — `strong`,
//!    `em`, `del`, `inline_math`, `link`, `reference_link` all carry a
//!    `[\s\S]*?` that runs to the end of the level before giving up.
//! 2. **A flat VM setup per attempt** (S4) — `html_tag`'s `\3` and `(?i)` keep
//!    it on the backtracking path even to reject on the first character.
//! 3. **How far a greedy quantifier scans before the character that must
//!    follow it is absent** (S5) — `auto_link_extension`'s
//!    `[\w.!#$%&'*+/=?^`{|}~-]+@`.
//!
//! The prefixes below open each of them. A fourth shape showing up as a slow
//! unit that none of the three explains is the most valuable thing this target
//! can produce, and it belongs in `benches/tokenizer.rs` as a row.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_inline::{generator, tokenize};

/// Openers for the seven patterns `rules.ts` silences the lint for, plus the
/// two containers that nest.
///
/// Every one of these puts the engine into a quantifier that has to scan to the
/// end of the level before it can fail, so whatever the fuzzer appends is paid
/// for at full price.
const PREFIXES: [&str; 12] = [
    "**",   // strong — lazy `[\s\S]*?`, shape 1
    "__",   //   …the other marker
    "*",    // em
    "~~",   // del
    "$",    // inline_math — lazy, plus `\\.`
    "`",    // inline_code
    "[",    // link — lazy, and lowerPriority over every interior position
    "![",   // image
    "<div", // html_tag — shape 2, the flat per-attempt cost
    "<",    //   …and the bare opener, which auto_link also claims
    "www.", // auto_link_extension — shape 3, the greedy local part
    ":",    // emoji, whose word-boundary read is D3 site 1
];

fuzz_target!(|src: &str| {
    for prefix in PREFIXES {
        let mut input = String::with_capacity(prefix.len() + src.len());
        input.push_str(prefix);
        input.push_str(src);

        // The whole assertion. Exceeding `BACKTRACK_LIMIT` is a *no-match*, and
        // a no-match leaves the characters to accumulate as text — so the round
        // trip holds whether or not the limit was reached, and that is exactly
        // the property C1's decision was made to preserve. If this ever fails,
        // the limit stopped degrading gracefully.
        let tokens = tokenize(&input);
        assert_eq!(
            generator(&input, &tokens),
            input,
            "a rule giving up must leave its characters as text, not lose them"
        );
    }
});
