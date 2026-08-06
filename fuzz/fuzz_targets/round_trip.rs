//! **The round trip settles**: `serialize ∘ parse` stops moving the bytes after
//! a small, measured number of applications — coverage-guided, from bytes.
//!
//! RUST-REWRITE-PLAN.md §11.3's *"Round-trip"* row and `docs/M2.md` §6's S7 row.
//! `crates/mt-md/tests/round_trip_properties.rs` is this target's structure-aware
//! sibling and states the same claim with the same bound over proptest's
//! generators; neither subsumes the other, for the reason `mt-inline`'s
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
//! would therefore report failures on inputs working exactly as designed, and
//! would keep reporting them until somebody deleted the property or diverged
//! from the engine this milestone is porting.
//!
//! **What is asserted instead is that the bytes settle.** With `m0 = s` and
//! `m(k+1) = serialize(parse(mk))`, the claim is `m(SETTLES_BY) ==
//! m(SETTLES_BY+1)` — `m4 == m5` — since *"settles after k applications"* is
//! exactly *"the k+1st changes nothing"*. That is the property an editor needs — repeated
//! open/save must not drift and must not oscillate — it is strictly weaker than
//! S4's clause where that clause fails, and strictly implied by it where it
//! holds.
//!
//! # Four, and the number is measured rather than chosen
//!
//! Over `round_trip.rs`'s fixed 1346 the *second* application is enough:
//! `m2 == m3` holds **1346 of 1346**, which is what
//! `the_round_trip_converges_over_every_input` asserts with no exception list.
//! Over **generated** documents it is not enough. Measured at S7 over 466,909
//! generated documents at both option sets:
//!
//! | Settles after | Documents |
//! |---|---:|
//! | 1 application | 458,272 |
//! | 2 | 8,317 |
//! | 3 | 316 |
//! | 4 | 4 |
//! | more than 4, or never | **0** |
//!
//! So [`SETTLES_BY`] is **4**, and this target carries the same constant and the
//! same three exclusions as the property test rather than a second, weaker
//! claim of its own. Two claims about one behaviour that differ by a fuzz
//! target's convenience are one claim and one hole.
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

/// How many applications of `serialize ∘ parse` the bytes are allowed to keep
/// moving for.
///
/// **Four, measured** — the table in this file's header, where 4 of 466,909
/// generated documents need exactly this many and none needs more. It is
/// therefore the observed maximum rather than a comfortable round number, and
/// that is deliberate: a document that needs five passes to stop moving is a
/// document a user can watch change under them on the fourth save, so raising
/// this is not a free way to make a failure go away — it has to be written down
/// and argued, here and in
/// `crates/mt-md/tests/round_trip_properties.rs::SETTLES_BY`, which must agree.
///
/// The families for which no constant works are excluded **by name** in
/// [`the_settling_claim_does_not_apply`] rather than by inflating this.
const SETTLES_BY: usize = 4;

/// The three shapes the settling claim is not made about, asked of **every**
/// element of the sequence and not only of the source.
///
/// Asking only about the source is wrong twice over, and the property test found
/// both by running rather than by reasoning: a soak run proved the serializer
/// can emit the upstream panic's shape from a source that is neither, and
/// `"> - [a]:x\n\t- [a]:x"` — which has no empty list item in it — serializes
/// into the oscillator [`the_round_trip_is_known_to_oscillate`] names. A
/// sequence is a sequence of documents, and each of them is as much an input to
/// `parse` as the first one was.
fn the_settling_claim_does_not_apply(m: &str) -> bool {
    known_panics::is_the_known_upstream_panic(m)
        || the_settling_time_is_known_to_be_unbounded(m)
        || the_round_trip_is_known_to_oscillate(m)
}

/// **The one family with no constant settling time at all** — a code fence whose
/// info string contains a backtick.
///
/// `n` fences of `` ~~~a` `` followed by `` ~~~` ~ `` settle after exactly
/// **n + 2** applications, for every `n` from 1 to 12; the property test's
/// `the_settling_time_grows_with_the_number_of_backtick_info_strings` pins six
/// of them. So the family has no constant bound, no [`SETTLES_BY`] can cover it,
/// and excluding it is not a way of avoiding an inconvenient number — it is the
/// only statement about it that is true.
///
/// The mechanism is muya's, and it is `round_trip.rs`'s `FIXED_POINT_EXCEPTIONS`
/// **mechanism 2**: the serializer always emits backticks whatever the source
/// used, CommonMark forbids a backtick inside a backtick fence's info string, so
/// the emitted fence is closed by its own info string; `_codeFenceLength` then
/// grows the opening fence past the longest all-backtick line in the body, and
/// the next pass has a longer line to grow past. Driven directly against the
/// reference engine, muya produces byte-identical output for the first three
/// passes and then **keeps growing where this crate stops** — a *serializer*
/// divergence, which §8's risk table records as the class this repository has no
/// harness for, and which S7 reports rather than decides.
///
/// It over-approximates in one direction: such a line inside *another* fence is
/// content rather than an opener, and this predicate cannot tell without being a
/// second block parser.
fn the_settling_time_is_known_to_be_unbounded(src: &str) -> bool {
    src.lines().any(|line| {
        let line = line.trim_start_matches(' ');
        let fence = line.trim_start_matches(['`', '~']);
        (line.starts_with("```") || line.starts_with("~~~")) && fence.contains('`')
    })
}

/// **The round trip oscillates with period two on `"> - [a]: /x\n>   * "`, and
/// unlike everything else this target skips, muya does not.**
///
/// This is the one exclusion here that is a **port defect** rather than an
/// inherited or reproduced behaviour, and it should be read as a finding rather
/// than as a guard:
///
/// ```text
/// m1 = "> - [a]: /x\n>\n>   *\n"  loose
/// m2 = "> - [a]: /x\n>   * \n"    tight
/// m3 = m1, m4 = m2, forever
/// ```
///
/// Driven directly against the reference engine, muya reaches `m2` on its first
/// pass and **stays there**. So the two engines do not merely settle
/// differently and the port does not merely take longer: the port never stops. A
/// user saving this document twice gets two different files and saving it a
/// third time gets the first one back, which is the same class of harm §11.3's
/// *"Data safety"* row exists for and the sharpest statement of why this target
/// is worth running.
///
/// S7 reports it rather than repairing it because looseness is decided by
/// `_insertLineBreak` and by what `loose` is read back from — the mechanism
/// `FIXED_POINT_EXCEPTIONS` mechanism 6 and §10's *"Owed by S4"* nine already
/// turn on — and it is settled in this repository by `cargo xtask blocks
/// --require-ts` against the running engine over 1344 inputs, which is the
/// harness a fuzzing stage does not run. Changing looseness on a guess is how
/// the 1344 stops being 1344.
/// `the_round_trip_oscillates_between_a_tight_and_a_loose_rendering` next door
/// is the ratchet: the day the item stops being read back as loose, it fails,
/// and this predicate comes out with it.
///
/// Three ingredients: a block-quote marker, a reference-definition line, and an
/// empty list item. `"> - a\n>   * "` settles on the first pass (no definition),
/// `"- a\n  * "` settles on the first pass (no quote), and `"> - [a]:x\n>   - b"`
/// settles on the first pass (no empty item). Over-approximating: it does not
/// check that the three are nested in one another, because doing so would mean
/// parsing.
fn the_round_trip_is_known_to_oscillate(src: &str) -> bool {
    let mut a_quote = false;
    let mut a_definition = false;
    let mut an_empty_item = false;
    for line in src.split('\n') {
        a_quote |= line.trim_start_matches([' ', '\t']).starts_with('>');
        a_definition |= known_panics::is_a_reference_definition(line);
        an_empty_item |= is_an_empty_list_item(line);
    }
    a_quote && a_definition && an_empty_item
}

/// A list marker with nothing after it — muya's synthetic empty item, and one of
/// the three ingredients of [`the_round_trip_is_known_to_oscillate`].
fn is_an_empty_list_item(line: &str) -> bool {
    let (rest, a_list_marker) = known_panics::strip_container_prefixes(line);
    a_list_marker && rest.trim().is_empty()
}

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
        let mut sequence: Vec<String> = Vec::with_capacity(SETTLES_BY + 1);
        let mut current = src.to_string();
        for _ in 0..=SETTLES_BY {
            if the_settling_claim_does_not_apply(&current) {
                // Not a failure and not a silent skip: the three families are
                // named, each carries a ratchet test next door, and two of them
                // are findings this stage reports rather than behaviours it
                // tolerates.
                break;
            }
            current = serialize(&parse(&current, options).document, options);

            // Totality of the whole path, asserted where the strings are least
            // like anything a human wrote: `serialize`'s own output is what a
            // save-then-open hands back to the parser, and it is the one input
            // class no fixture in this repository contains.
            assert!(
                current.is_empty() || current.ends_with('\n'),
                "{label}: serialize produced {:?}, which a file layer cannot append to, \
                 from {src:?}",
                truncate(&current)
            );
            sequence.push(current.clone());
        }

        if sequence.len() > SETTLES_BY {
            let shown: Vec<String> = sequence
                .iter()
                .enumerate()
                .map(|(i, m)| format!("\n  m{}: {:?}", i + 1, truncate(m)))
                .collect();
            assert_eq!(
                sequence[SETTLES_BY - 1],
                sequence[SETTLES_BY],
                "{label}: the round trip had not settled after {SETTLES_BY} applications \
                 of serialize∘parse on {src:?}{}",
                shown.concat()
            );
        }
    }
});
