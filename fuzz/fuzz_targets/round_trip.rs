//! **`serialize ∘ parse` is total under repetition** — the serializer's own
//! output, fed back to the parser, coverage-guided from bytes.
//!
//! RUST-REWRITE-PLAN.md §11.3's *"Round-trip"* row and `docs/M2.md` §6's S7 row.
//! `crates/mt-md/tests/round_trip_properties.rs` is this target's structure-aware
//! sibling; neither subsumes the other, for the reason `mt-inline`'s
//! `tests/properties.rs` gives at length — a fuzzer starting from bytes spends a
//! long time discovering that `>` opens a block quote, and a generator that only
//! emits well-formed blocks will never find the input that breaks the stripper.
//!
//! # The fixed point is **not** the property, and asserting it would be wrong
//!
//! `crates/mt-md/tests/round_trip.rs` — S4's gate — asserts
//! `parse(serialize(parse(s))) == parse(s)` over a fixed 1346-input corpus with
//! **22 enumerated exceptions** (`FIXED_POINT_EXCEPTIONS`), and
//! `serialize(parse(s)) == s` with **348** (`IDENTITY_EXCEPTIONS`). Those are
//! not port defects: for every fixed-point exception the port's output is
//! byte-identical to `ExportMarkdown.generate`'s and muya's own round trip is
//! not a fixed point on them either. A fuzz target asserting the fixed point
//! would therefore report failures on inputs working exactly as designed.
//!
//! # And **settling in a constant number of applications is not assertable from
//! bytes either** — this target is what proved that, twice
//!
//! *"The bytes settle"* — with `m0 = s` and `m(k+1) = serialize(parse(mk))`,
//! that `m(k) == m(k+1)` for some fixed `k` — is the property an editor needs:
//! repeated open/save must not drift and must not oscillate. It is asserted, at
//! `k = 8` with four families excluded by name, in
//! [the property test](../../crates/mt-md/tests/round_trip_properties.rs), whose
//! generators emit shapes this repository enumerates.
//!
//! **It is not asserted here, and the reason is structural rather than a matter
//! of picking a larger constant.** Three families have settling times that
//! *grow with the size of the input*:
//!
//! | Family | Settling time |
//! |---|---|
//! | A backtick inside a code fence's info string | `n + 2` for `n` fences |
//! | A list item whose content is indented past its own column | linear in the indentation |
//! | A backslash run in a fence info string | `floor(log2 n) + 1` |
//!
//! A coverage-guided fuzzer over `-max_len=65536` can always make the input
//! bigger, so **any constant here is a statement about the fuzzer's `max_len`
//! rather than about the engine.** The history is the argument: S7 measured 4
//! over 466,909 generated documents (458,272 settled at 1; 8,317 at 2; 316 at 3;
//! 4 at 4; none above) and one nightly soak falsified it; the constant was
//! raised to 8 with the linear family excluded by name, and the next soak
//! falsified *that* — a **305-byte** input, ` ```o ` followed by 300
//! backslashes, needs `floor(log2 300) + 1 = 9`. Excluding families by name
//! works against a generator whose shapes are enumerated. Against a fuzzer it is
//! a list that grows every run, and a list of engine defects the property has
//! been made blind to is not a property.
//!
//! **All three families are serializer defects** — data the user watches move on
//! every save — and repairing them is blocked on the `ExportMarkdown`
//! differential runner that `docs/M2.md` §10 has now declined to build four
//! times. When they are repaired the bound becomes real, small and worth
//! asserting from bytes; **the assertion belongs back here on that day and not
//! before.** §10's *"Owed by S7"* carries that.
//!
//! # What is asserted instead
//!
//! Totality under repetition, which is §11.3's actual clause and which the
//! settling assertion was never needed for: `parse` and `serialize` are driven
//! [`APPLICATIONS`] times over, each pass fed the previous pass's output. That
//! output is the input class **no fixture in this repository contains** — it is
//! what a save-then-open hands back to the parser — and it is reached here
//! without any claim about when the sequence stops moving.
//!
//! # `&str`, not `&[u8]`
//!
//! `mt_md::parse` takes `&str`, so invalid UTF-8 is not a reachable state and
//! fuzzing bytes would spend the budget rediscovering UTF-8 rather than the
//! serializer. `fuzz_targets/parse.rs` and `fuzz_targets/tokenize.rs` say the
//! same at more length. The seed corpus is whole documents — see
//! `xtask/src/fuzz.rs`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mt_md::{Options, parse, serialize};

#[path = "known_panics.rs"]
mod known_panics;

/// The two option sets the claim is made at — `parse.rs` says why both.
const OPTION_SETS: [(&str, Options); 2] = [
    ("SPEC", Options::SPEC),
    ("MUYA_DEFAULT", Options::MUYA_DEFAULT),
];

/// How many times `serialize ∘ parse` is driven over its own output.
///
/// **Not a settling bound** — the header says at length why no constant one is
/// assertable from bytes while three serializer families' settling times grow
/// with the input. This is a coverage knob: each extra pass feeds the parser a
/// string further from anything a human wrote, and the returns fall off fast
/// because the sequence has usually stopped moving by the second. Four keeps a
/// slow path from costing an execution-per-second it cannot repay.
const APPLICATIONS: usize = 4;

fn truncate(s: &str) -> String {
    const LIMIT: usize = 200;
    if s.len() <= LIMIT {
        return s.to_string();
    }
    let mut end = LIMIT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes)", &s[..end], s.len())
}

fuzz_target!(|src: &str| {
    for (label, options) in OPTION_SETS {
        let mut current = src.to_string();
        for pass in 1..=APPLICATIONS {
            // The one thing still skipped, and it is not a settling family: the
            // upstream `pulldown-cmark` panic, which `docs/M2.md` §10 defers to
            // M4 and which no choice of assertion here can survive. Asked of
            // every element of the sequence rather than only of the source,
            // because a soak run proved `serialize` can emit its shape from a
            // source that is not that shape.
            if known_panics::is_the_known_upstream_panic(&current) {
                break;
            }
            current = serialize(&parse(&current, options).document, options);

            // Totality of the whole path, asserted where the strings are least
            // like anything a human wrote: `serialize`'s own output is what a
            // save-then-open hands back to the parser, and it is the one input
            // class no fixture in this repository contains.
            //
            // A file layer appends to this. `mt-fs` will write it and `mt-cli`
            // already reads its own output back, so a final byte that is not a
            // newline is a document that grows a joined line every save.
            assert!(
                current.is_empty() || current.ends_with('\n'),
                "{label}: pass {pass} of {APPLICATIONS}: serialize produced {:?}, \
                 which a file layer cannot append to, from {src:?}",
                truncate(&current)
            );
        }
    }
});
