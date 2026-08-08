//! **The M2 S7 gate, first half**: the parse/serialize path over *generated*
//! markdown, and the invariants that are true of every string rather than of a
//! corpus.
//!
//! `crates/mt-md/tests/round_trip.rs` — S4's gate — says in its own module doc
//! that **"generated inputs are S7's, not this file's"**, and §11.3's
//! *"Round-trip"* row and §6's S7 row are where they were deferred to. This is
//! that file. `fuzz/fuzz_targets/` is its coverage-guided sibling and carries
//! the same claims from a different input distribution, for the reason
//! `mt-inline`'s `tests/properties.rs` gives at length: a fuzzer starting from
//! bytes spends a long time discovering that `>` opens a block quote, and a
//! generator that only emits well-formed blocks will never find the input that
//! breaks the stripper.
//!
//! # The round trip is **not** a fixed point on every string, so this file does
//! not assert that it is
//!
//! S4's first clause is `parse(serialize(parse(s))) == parse(s)`, and it holds
//! over all 22 whole documents with **22 enumerated exceptions** among the 1324
//! spec examples — `round_trip.rs`'s `FIXED_POINT_EXCEPTIONS`. Those are not
//! port defects: for every one of them the port's output is byte-identical to
//! `ExportMarkdown.generate`'s and muya's own round trip is not a fixed point on
//! them either. A generated test that asserted the fixed point would therefore
//! fail on inputs that are working exactly as designed, and would keep failing
//! until someone deleted the property or diverged from the engine this milestone
//! is porting.
//!
//! **What this file asserts instead is that the round trip settles.** With
//! `m0 = s` and `m(k+1) = serialize(parse(mk))`, the claim is that the bytes
//! stop moving after a small, **measured** number of applications. That is the
//! property an editor actually needs — repeated open/save must not drift and
//! must not oscillate — it is strictly weaker than S4's clause where that
//! clause fails, and it is strictly implied by it where it holds.
//!
//! **The number is different at the two denominators, and finding that out is
//! most of what this file did.**
//!
//! Over `round_trip.rs`'s fixed 1346 inputs at their own option sets, the
//! *second* application is enough, and
//! [`the_round_trip_converges_over_every_input`] is that measurement:
//!
//! | Claim | Count |
//! |---|---|
//! | `m1 == m2` — *one* round trip is already stable | **1334 of 1346** |
//! | `m2 == m3` — *two* round trips are always stable | **1346 of 1346** |
//!
//! So that claim needs no exception list, and the twelve inputs still moving
//! after one pass are enumerated as [`SECOND_PASS_MOVERS`] — a ratchet in
//! `round_trip.rs`'s four-row shape, because "one round trip is nearly always
//! enough" is exactly the kind of sentence §8's risk table exists to make
//! unwriteable.
//!
//! Over **generated** documents the second application is not enough: 316 in
//! 466,909 settle only on the third and four need a fourth. **And that
//! distribution is not a bound** — S7's soak drove a byte fuzzer at the same
//! claim and found one input settling at 8 and one at 6, neither of them
//! reachable by these strategies. So the generated property is stated at
//! [`SETTLES_BY`] `= 8`, which is that measured maximum; the distribution is on
//! [`the_round_trip_settles`] and the argument for the number is on
//! [`SETTLES_BY`] itself.
//!
//! # What this file found, which is more than it was built to assert
//!
//! Five things, and the first two are §10's *"Owed by S6"* rather than this
//! file's. **The third is repaired**, which is why it alone reads in the past
//! tense — it was this crate's own and S7 took it. Each of the other four is
//! guarded **by name** with a ratchet test that asserts the behaviour it was
//! guarded for, so none of them can be fixed without the guard failing and
//! asking to be deleted.
//!
//! 1. **`pulldown-cmark` 0.13.4 panics on `"> - [a]: /x\n\t"`**, before any of
//!    this crate's code runs. §10 defers the decision to M4.
//!    [`is_the_known_upstream_panic`] is the pre-check that keeps this file —
//!    and the nightly soak that runs it — from being permanently red on it. It
//!    is **not a fix**. It is also **wider than §10 records**: the block quote
//!    is not required, and any whitespace character CommonMark does not count
//!    as blank will do, so `"- [a]:x\n\u{b}"` panics where §10's table says the
//!    quote is needed. That widens a table rather than contradicting one — all
//!    of §10's rows are still exactly as recorded — and the guard's docs carry
//!    the measurement.
//! 2. **A child's source range can escape its parent's**, on
//!    `-\t- [a]: /x[xter\n`. `tests/source_ranges.rs` asserts nesting **over the
//!    corpus**, which contains no such document, and
//!    `block::tests::the_ranges_are_not_injective_and_do_not_always_nest` is the
//!    input that says the invariant is about the corpus rather than about every
//!    string. **So nothing here asserts that source ranges nest**, and a reader
//!    who "restores" that assertion will get a failure that is correct.
//!    `fuzz/fuzz_targets/tokenize.rs` *does* assert nesting for `mt-inline`
//!    tokens; that is right there and wrong here, and the difference is this
//!    paragraph.
//! 3. **`mt_md::parse` panicked on `"- a\n\u{2028}"`, and S7 repaired it.**
//!    `block.rs`'s `apply_prefix` dedented a whitespace-only line inside a list
//!    item with a clamp that was bounds-safe and **not
//!    character-boundary-safe**, so a whitespace character wider than the
//!    item's remaining indent put the cut inside it. That is §11.3's own gate
//!    row failing rather than an inherited defect, which is why this one was
//!    fixed rather than reported: a guard here would have been this file
//!    excusing the crate from the property it exists to assert. The answer the
//!    dedent now gives was chosen by measurement against
//!    `nextLine.slice(indent)` — see
//!    `block::tests::a_wide_whitespace_continuation_line_dedents_to_nothing`,
//!    which carries it — and
//!    [`parse_does_not_panic_on_a_wide_whitespace_line_inside_a_list_item`] is
//!    what keeps the class from coming back.
//! 4. **The round trip has no constant settling time** when a code fence's info
//!    string holds a backtick: `n` such fences take `n + 2` applications.
//!    `FIXED_POINT_EXCEPTIONS` mechanism 2 in a document with more than one of
//!    them — and the corpus has `commonmark#146`, which is that shape with
//!    exactly one and which settles, so no fixture could ever have shown it.
//!    Guarded by [`the_settling_time_is_known_to_be_unbounded`], pinned by
//!    [`the_settling_time_grows_with_the_number_of_backtick_info_strings`].
//! 5. **The round trip oscillates** with period two on `"> - [a]: /x\n>   * "`,
//!    flipping between a tight and a loose rendering forever — **and muya
//!    settles on one of the two phases on its first pass**, measured directly
//!    against the reference engine. This is the one exclusion in this file that
//!    is a **port defect**, it is the sharpest form of the harm the settling
//!    claim exists to prevent, and it is reported rather than repaired because
//!    looseness is settled by `cargo xtask blocks --require-ts` over 1344
//!    inputs and not by a guess.
//!    Guarded by [`the_round_trip_is_known_to_oscillate`], pinned by
//!    [`the_round_trip_oscillates_between_a_tight_and_a_loose_rendering`].
//!
//! Items 4 and 5 are excluded from the **settling claim alone**
//! ([`the_settling_claim_does_not_apply`]); those documents are still parsed,
//! serialized and checked for everything else. Item 1 is excluded from every
//! claim, because a panic leaves nothing to check. Item 3 is excluded from
//! nothing, which is what a repair buys.
//!
//! # What is asserted, and it is only what is universally true
//!
//! | # | Claim | Test |
//! |---|---|---|
//! | 1 | Nothing panics — `parse`, `serialize`, `dump_state`, `render_to_static_html` at both `sanitize` values | [`parse_and_its_consumers_are_total_over_generated_documents`] |
//! | 2 | Every source range is inside `0..src.len()` and both ends are `char` boundaries | [`every_source_range_is_in_bounds_and_on_a_char_boundary`] |
//! | 3 | Every live node carries a `SourceMap` entry, and the map holds nothing else | [`every_live_node_has_a_source_range`] |
//! | 4 | The round trip settles within [`SETTLES_BY`] applications | [`the_round_trip_settles_within_the_measured_bound`] |
//! | 5 | `Incremental::new(s, o)` is `parse(s, o)` | [`the_incremental_entry_point_agrees_with_a_full_parse_before_any_edit`] |
//!
//! Claim 1 is the gate itself — §11.3: *"Malformed input must never panic —
//! `panic = "abort"` makes a panic a crash."*
//!
//! **Claim 3 is wider here than `source_ranges.rs` states it**, and the brief
//! asked whether that generalisation was safe. It is: `source_ranges.rs`
//! asserts one entry per live node over the corpus, and the same holds over
//! every generated document at both option sets, footnote and re-lex nodes
//! included — those two mechanisms make the map non-*injective* (§8's risk row,
//! `SourceMap::is_exact`) but never leave a node without an entry. So the claim
//! is stated for every live node rather than narrowed to leaves.
//!
//! **Not** asserted, each for a named reason: range nesting (above), range
//! injectivity (S5 made the map non-injective on purpose — footnote nodes share
//! the definition's range), the fixed point (above), and byte identity (S4's 348
//! enumerated exceptions).
//!
//! # Why `every_input` is copied out of `round_trip.rs` rather than shared
//!
//! Because an integration test is its own binary and `round_trip.rs`'s
//! `every_input` is private to it — and because this repository deliberately
//! keeps generators and corpus readers per-file rather than building a shared
//! test crate. The denominator is pinned to **1346** in the same breath the
//! original pins it, so a corpus that silently shrinks fails here too rather
//! than passing over less.

use std::path::{Path, PathBuf};

use mt_doc::{Document, NodeId};
use mt_md::reparse::Incremental;
use mt_md::{Options, normalize_source, parse, serialize};
use proptest::prelude::*;
use proptest::strategy::ValueTree;

/// The two option sets every claim is made at.
///
/// §11.3's row is about the parse/serialize path and both of these are on it:
/// `SPEC` is what the 1324 spec examples are driven with and `MUYA_DEFAULT` is
/// what the 22 whole documents are, and the two differ in `math`,
/// `front_matter` and `gitlab_compatibility` — three flags that change which
/// *block kinds exist*, not merely how they render. A property proved at one of
/// them says nothing about the other.
///
/// `footnote` is `false` in both, which is muya's own default; the option is
/// exercised by `tests/reparse_properties.rs`, whose third property turns it on.
const OPTION_SETS: [(&str, Options); 2] = [
    ("SPEC", Options::SPEC),
    ("MUYA_DEFAULT", Options::MUYA_DEFAULT),
];

// ---------------------------------------------------------------------------
// The one panic `parse` is not total on, guarded and not fixed
// ---------------------------------------------------------------------------

/// The one place a claim below is not made about every string.
///
/// **One shape since S7, and it was two.** The second was this crate's own —
/// `block.rs`'s dedent cut a whitespace-only continuation line at a byte that
/// need not be a `char` boundary — and S7 **repaired** it rather than guarding
/// it, because §11.3's gate row is the property this file exists to assert and a
/// guard would have been the file excusing the crate from it. What is left is
/// `pulldown-cmark`'s, whose decision §10 owes to M4.
///
/// It stays a function of its own rather than becoming a call to
/// [`is_the_known_upstream_panic`] at each of the six sites, because the
/// sentence it is named for — *these are the only strings a claim below is not
/// made about* — is the one a reader needs a single place to check, and the day
/// a second shape lands it should not have to touch six of them.
/// [`the_only_panics_the_generators_reach_are_the_ones_the_guards_name`] is what
/// keeps that second shape from hiding behind this one.
fn is_a_known_panic(src: &str) -> bool {
    is_the_known_upstream_panic(src)
}

/// A bullet or ordered marker followed by a space, a tab **or the end of the
/// line**, after any indentation — the opener `Prefix::Item` is built from.
///
/// The end-of-line arm is not decoration: `*` alone is an *empty* list item in
/// CommonMark, and a first version of this predicate that required whitespace
/// after the marker let `"*\n\ta\n\u{2028}"` through to the panic S7 has since
/// repaired. It is still load-bearing for the guard that remains —
/// `"-\n  [a]:x\n\u{b}"`, in
/// [`the_upstream_panic_guard_covers_the_recorded_shape_and_the_wider_class`],
/// opens its item with a bare `-`. `"**"` and `"--"` still say no, which is
/// what keeps thematic breaks and setext underlines out.
fn opens_a_list_item(line: &str) -> bool {
    let rest = line.trim_start_matches([' ', '\t', '>']);
    let after_marker = if let Some(after) = rest.strip_prefix(['-', '*', '+']) {
        after
    } else {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return false;
        }
        match rest[digits..].strip_prefix(['.', ')']) {
            Some(after) => after,
            None => return false,
        }
    };
    after_marker.is_empty() || after_marker.starts_with([' ', '\t'])
}

/// **`pulldown-cmark` 0.13.4 panics on `"- [a]:x\n\u{b}"`. This is not a fix,
/// and the recorded ingredient list was too narrow.**
///
/// `docs/M2.md` §10, *"Owed by S6"*, item 1, and `docs/upstream-issues.md`'s
/// second half. `Option::unwrap()` on `None` inside `OffsetIter::next`
/// (`parse.rs:2199`), reached before any of this crate's code runs.
///
/// # What §10 records, and what this file measured
///
/// §10 and `docs/upstream-issues.md` give **four** ingredients — a block quote,
/// a list item, a link reference definition, and a following whitespace-only
/// line ending in a tab — with `"> - [a]: /x\n\t"` as the reproducer and
/// `"- [a]: /x\n\t"` in the `ok` column. **Every word of that is still true**,
/// and `block::tests::the_neighbours_of_the_upstream_panic_parse_normally`
/// still passes. It is also not the whole shape, and this file's soak found the
/// rest within a minute:
///
/// | Trailing line, after `"- [a]:x"` | Result |
/// |---|---|
/// | `"\t"` U+0009 | ok — §10's row, and why the quote is in its reproducer |
/// | `"\u{b}"` VERTICAL TAB | **panic** |
/// | `"\u{c}"` FORM FEED | **panic** |
/// | `"\u{2028}"` LINE SEPARATOR | **panic** |
/// | `"\u{3000}"` IDEOGRAPHIC SPACE | **panic** |
/// | `"\r"`, `" "`, `"\u{85}"`, `"\u{a0}"` | ok |
///
/// So the real ingredient is **a line that is whitespace to Unicode and not
/// blank to CommonMark** — CommonMark's blank line is spaces and tabs only, so
/// every other whitespace character reaches the same unwrap. A tab gets there
/// too, but needs the extra container level `"> - "` provides, which is why
/// §10's reproducer has a block quote and why nobody noticed the wider class.
/// Two more rows the same probe turned up: `"-\n  [a]:x\n\u{b}"` panics, so the
/// definition may be on a *continuation* line of the item; and
/// `"- [a]:x\nz\n\u{b}"` does not, so a content line in between prevents it.
///
/// **This does not contradict anything in the repository** — it widens a table.
/// The `#[should_panic]` reproducer and its three `ok` neighbours are all still
/// exactly as recorded. Widening §10 and `docs/upstream-issues.md` is a
/// documentation change S7 reports rather than makes.
///
/// # Still not a fix, and still M4's decision
///
/// **§10 defers it to M4** — file upstream, pin a patched fork, or pre-scan in
/// `mt_md::block` — because M4 is the first milestone with a user who can type
/// it. S7 is not that stage. What S7 must do is stop `soak.yml` from being
/// permanently red on a deferred upstream defect, and this is the whole of
/// that: a **pre**-check, not a rescue. Catching the panic is not open — §12's
/// release profile is `panic = "abort"`, which is also why this cannot be a
/// `catch_unwind` the way `tests/reparse_properties.rs`'s [`quietly`] is, since
/// the soak runs `--release`.
///
/// # How narrow it is, and where it over-approximates
///
/// Three ingredients anywhere in the document: a line that opens a list item, a
/// line shaped like a link reference definition, and a line that is non-empty,
/// entirely whitespace, and not entirely spaces. All three, or it does not fire
/// — a list alone, a definition alone or a tab alone is not enough, and a blank
/// line of plain spaces is not either.
///
/// It **over-approximates on order and on the trailing character**: it does not
/// require the blank line to follow the definition (`"- [a]:x\nz\n\u{b}"` is
/// skipped although it parses), and it treats a tab like the wider characters
/// even without a block quote (`"- [a]: /x\n\t"`, §10's own `ok` row). Both are
/// deliberate. Guessing at the exact boundary of someone else's bug is how a
/// guard becomes wrong in the expensive direction, and
/// [`the_only_panics_the_generators_reach_are_the_ones_the_guards_name`]
/// measures what the looseness costs rather than leaving it as an adjective.
///
/// # Why this predicate exists twice
///
/// `fuzz/` is its own cargo workspace — deliberately, so the nightly
/// requirement stays out of `rust-toolchain.toml` — so a module cannot be
/// shared across the boundary without dragging `fuzz/` into
/// `cargo test --workspace`. The fuzz targets carry their own copy in
/// `fuzz/fuzz_targets/known_panics.rs`, which is also where this repository
/// already puts per-file test helpers by convention.
fn is_the_known_upstream_panic(src: &str) -> bool {
    let mut a_list_item = false;
    let mut a_definition = false;
    let mut a_blank_line_after_it = false;
    for line in src.split('\n') {
        a_list_item |= opens_a_list_item(line);
        // The order matters here, unlike in the stripper guard S7 deleted: the
        // unwrap is about what `OffsetIter` finds *after* the definition, and
        // every panicking row measured has the whitespace line following it.
        // The list item may be anywhere, since it is the container the
        // definition sits in and therefore opens at or before it.
        if a_definition {
            a_blank_line_after_it |= !line.is_empty()
                && line.chars().all(char::is_whitespace)
                && line.chars().any(|c| c != ' ');
        }
        a_definition |= is_a_reference_definition(line);
    }
    a_list_item && a_definition && a_blank_line_after_it
}

/// Everything left of a line's content once every block-quote marker and list
/// marker has been taken off, plus whether any **list** marker was among them.
///
/// One stripper for the three predicates that need one, because they need the
/// same one: a definition and an empty item are both "what is left after the
/// containers", and two hand-rolled copies would be the "two scanners" mistake
/// §5 D3 spends a section on. It is not `mt_md::block`'s `strip_lines` and does
/// not try to be — it never sees the tree, so it cannot know which markers are
/// real. Over-approximating is what a guard wants.
fn strip_container_prefixes(line: &str) -> (&str, bool) {
    let mut rest = line.trim_start_matches([' ', '\t']);
    let mut a_list_marker = false;
    loop {
        let before = rest.len();
        if let Some(after) = rest.strip_prefix('>') {
            rest = after.trim_start_matches([' ', '\t']);
        } else if let Some(after) = rest.strip_prefix(['-', '*', '+'])
            && (after.is_empty() || after.starts_with([' ', '\t']))
        {
            rest = after.trim_start_matches([' ', '\t']);
            a_list_marker = true;
        } else {
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            if digits > 0
                && let Some(after) = rest[digits..].strip_prefix(['.', ')'])
                && (after.is_empty() || after.starts_with([' ', '\t']))
            {
                rest = after.trim_start_matches([' ', '\t']);
                a_list_marker = true;
            }
        }
        // Every arm above removes at least one byte, so this terminates.
        if rest.len() == before {
            return (rest, a_list_marker);
        }
    }
}

/// A list marker with nothing after it — muya's synthetic empty item, and one
/// of the three ingredients of [`the_round_trip_is_known_to_oscillate`].
fn is_an_empty_list_item(line: &str) -> bool {
    let (rest, a_list_marker) = strip_container_prefixes(line);
    a_list_marker && rest.trim().is_empty()
}

/// `[label]:` after **any number** of container prefixes.
///
/// The destination is not part of the shape — `"- [a]:x"` reproduces as well as
/// `"- [a]: /url"` — and the markers are optional and repeatable because two
/// measured reproducers need that: `"-\n  [a]:x\n\u{b}"` puts the definition on
/// the item's *second* line, and `"-\t- [a]:r\n\u{b}"` puts it two list levels
/// deep. The second escaped a version of this that stripped one marker, once,
/// in 352,572 samples of a random-seeded hunt — which is the argument for
/// stripping in a loop rather than by hand-counting the levels a reproducer
/// happened to have.
fn is_a_reference_definition(line: &str) -> bool {
    let (rest, _) = strip_container_prefixes(line);
    let Some(body) = rest.strip_prefix('[') else {
        return false;
    };
    body.find(']')
        .is_some_and(|close| body[close + 1..].starts_with(':'))
}

// ---------------------------------------------------------------------------
// The generators — private to this file, per the repository's convention
// ---------------------------------------------------------------------------

/// Characters chosen because a byte-oriented port gets them wrong.
///
/// The same argument `mt-inline`'s `tests/properties.rs` makes for its own copy,
/// one layer up: astral characters are one `char` here and two UTF-16 code units
/// in the reference engine, the two scalars either side of the surrogate block
/// are where a hand-decoded escape table is off by one, and a grapheme cluster
/// that is several characters is what a rule advancing by byte would split.
///
/// It matters more at this layer than the shared spirit suggests, because
/// **block parsing measures columns**: `strip_lines` takes leading whitespace
/// back by column count and a tab is four of them, so a non-ASCII character in
/// a marker's padding is the exact neighbourhood of §10's second known non-bug.
const AWKWARD: &[char] = &[
    // Either side of the surrogate block.
    '\u{d7ff}',
    '\u{e000}',
    // Astral: the first, the last, an emoji, a musical symbol.
    '\u{10000}',
    '\u{10ffff}',
    '😀',
    '𝄞',
    // Grapheme-cluster machinery.
    '\u{200d}',  // ZWJ
    '\u{fe0f}',  // VARIATION SELECTOR-16
    '\u{1f3ff}', // EMOJI MODIFIER FITZPATRICK TYPE-6
    '\u{301}',   // COMBINING ACUTE ACCENT
    // Whitespace the two engines disagree about, both directions.
    '\u{85}',
    '\u{feff}',
    '\u{2028}',
    '\u{2029}',
    '\u{a0}',
    '\u{3000}',
    // Two- and three-byte characters from the corpus's own scripts.
    'é',
    'я',
    '中',
    'ع',
    // Replacement character and the BMP's last two scalars.
    '\u{fffd}',
    '\u{fffe}',
    '\u{ffff}',
];

/// **Lines**, not inline fragments — which is the whole reason this table is not
/// `mt-inline`'s.
///
/// `markdownish()` next door is a table of inline markers assembled into one
/// line, and a document made of those would be a paragraph however long it got:
/// it would exercise the block layer only through the one rule that says
/// "anything else is a paragraph". These are the openers of every block rule
/// `mt_md::block` maps, so a random sequence of them joined with newlines is a
/// document with block structure in it rather than a document with markers in
/// it.
///
/// **Tabs are over-represented on purpose.** Both of §10's known non-bugs are
/// tab-driven — the upstream panic needs a whitespace-only line ending in a tab,
/// and the range that escapes its parent needs a tab inside a list item's marker
/// padding — which is the evidence that tab handling is the thin part of this
/// layer, and evidence is a better reason to weight a generator than symmetry
/// is.
const BLOCK_FRAGMENTS: &[&str] = &[
    // ATX headings, closed and unclosed, at the edges of the level range.
    "# h",
    "## h",
    "###### h",
    "####### h",
    "#h",
    "# h #",
    "#",
    "### ",
    // Setext underlines — only meaningful under a paragraph, which is why they
    // are pieces rather than a composite.
    "===",
    "=",
    "---",
    "--",
    // Fences, backtick and tilde, with and without info strings. The `mermaid`
    // one is the only way `Block::Diagram` is reachable.
    "```",
    "```js",
    "````rust title=\"x\"",
    "~~~",
    "~~~js",
    "~~~ aa ``` ~~~",
    "```mermaid",
    "```vega-lite",
    "``",
    // Block quotes at several depths, including the lazy and tab-prefixed forms.
    "> a",
    ">> a",
    "> > a",
    ">",
    ">>",
    "   > a",
    ">\t- a",
    "> ",
    // All five list markers, plus the bare and wide forms.
    "- a",
    "* a",
    "+ a",
    "1. a",
    "1) a",
    "20. a",
    "-",
    "*",
    "+",
    "1.",
    "  - a",
    "     - a",
    // Task-list markers, including the malformed one.
    "- [ ] a",
    "- [x] a",
    "* [X] a",
    "- [] a",
    "+ [ ]",
    // Tables: rows, the alignment row in all four spellings, and ragged rows.
    "| a | b |",
    "| --- | --- |",
    "| :-- | --: |",
    "| :-: | - |",
    "|a|b|",
    "| a |",
    "a | b",
    "| a | b | c |",
    // HTML blocks — the seven CommonMark kinds are not all reachable from a
    // table this size, but the four that decide a *block* boundary are.
    "<div>",
    "</div>",
    "<!-- c -->",
    "<table>",
    "</table>",
    "<br/>",
    "<?php",
    // Math, which is a block kind only at MUYA_DEFAULT. The closed composite is
    // there for the reason `mt-inline`'s link composites are: `$$`, a body and
    // `$$` **in that order** out of a uniform pick over ninety pieces happens
    // too rarely to make `math-block` reachable, and
    // `the_generator_produces_every_block_kind_the_options_can_build` is what
    // said so rather than anybody guessing.
    "$$",
    "$$\nx^2\n$$",
    "$$x^2$$",
    "x^2",
    // Front-matter fences. Only the leading one is a front matter (see
    // `BEGIN_FRAGMENTS`); the rest are thematic breaks or paragraphs, which is
    // the interesting half.
    "+++",
    ";;;",
    // Definitions: link reference and footnote.
    "[a]: /x",
    "[a]: /x \"t\"",
    "[]: /x",
    "[a]:",
    "[^a]: note",
    "[^a]",
    // Thematic breaks in all three characters and the spaced forms.
    "***",
    "___",
    "- - -",
    "* * *",
    // Indentation — tabs as well as spaces, and the two shapes §10's known
    // non-bugs are made of.
    "\ta",
    "    a",
    "  a",
    " \t- a",
    "-\t- a",
    "-\t- [a]: /x[xter",
    "\t- [a]: /x",
    "> - [a]: /x",
    "  \t",
    "\t",
    // Blank lines and ordinary text, so the paragraph and lazy-continuation
    // paths are reached as often as the marker paths.
    "",
    " ",
    "alpha bravo",
    "a",
    "continued",
    "  continued",
    "0",
];

/// Openers that mean something only on the **first line** of a document.
///
/// Front matter is the reason this exists, and it is the same argument
/// `mt-inline`'s `BEGIN_FRAGMENTS` makes for `consumeBeginRules`: a `---` drawn
/// uniformly from a table of ninety pieces lands at offset 0 about one time in
/// ninety, so a generator without this distribution would leave
/// `Block::Frontmatter` effectively unreachable and every property about it
/// vacuous.
const BEGIN_FRAGMENTS: &[&str] = &[
    "---\ntitle: x\n---",
    "---\ntitle: x\n---\n",
    "---",
    "+++\ntitle = \"x\"\n+++",
    "---\n---",
];

/// Block-structured markdown: lines from [`BLOCK_FRAGMENTS`], salted with
/// [`AWKWARD`], joined with newlines.
///
/// `0..24` pieces because a document of 24 lines is enough to contain a list
/// holding a quote holding a fence — the nesting depth every container rule in
/// `mt_md::block` needs to be reached at all — while staying small enough that
/// 192 cases parsed six times at two option sets is seconds rather than minutes.
fn markdownish_document() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        // Weighted toward whole lines: the point is to reach block rules.
        8 => (0usize..BLOCK_FRAGMENTS.len(), 1usize..=2usize)
            .prop_map(|(i, n)| BLOCK_FRAGMENTS[i].repeat(n)),
        // An awkward character alone **is** a line, because the pieces are
        // joined with newlines — and a line that is one wide space is a real
        // and interesting shape, since it is what found the dedent panic S7
        // repaired. It is one of three shapes rather than always that one so
        // that the same characters are also exercised *adjacent to text*, which
        // is a different code path and which keeps the remaining guard's skip
        // rate down to something a measurement can defend.
        2 => (0usize..AWKWARD.len(), 0usize..3usize).prop_map(|(i, shape)| match shape {
            0 => AWKWARD[i].to_string(),
            1 => format!("a{}", AWKWARD[i]),
            _ => format!("{}a", AWKWARD[i]),
        }),
        1 => any::<char>().prop_map(|c| c.to_string()),
    ];
    let body = proptest::collection::vec(piece, 0..24).prop_map(|pieces| pieces.join("\n"));
    let begin = prop_oneof![
        // Most documents have no front matter. A fifth of them do, which is
        // enough to exercise a rule that would otherwise be reached by accident.
        4 => Just(String::new()),
        1 => (0usize..BEGIN_FRAGMENTS.len())
            .prop_map(|i| format!("{}\n", BEGIN_FRAGMENTS[i])),
    ];
    (begin, body).prop_map(|(begin, body)| format!("{begin}{body}"))
}

/// Unstructured `char` soup, weighted toward [`AWKWARD`].
///
/// The same role it has next door: [`markdownish_document`] can only produce
/// what its table contains, and a property that only ever sees well-formed
/// blocks is not testing a *parser*. Newlines are drawn explicitly rather than
/// left to `any::<char>()`, because without them the soup is one very long
/// paragraph and the block layer never runs.
fn unicode_soup() -> impl Strategy<Value = String> {
    let character = prop_oneof![
        3 => (0usize..AWKWARD.len()).prop_map(|i| AWKWARD[i]),
        2 => any::<char>(),
        1 => prop::sample::select(&['\n', '\t', ' ', '>', '-', '#', '`', '|', '[', ']'][..]),
    ];
    proptest::collection::vec(character, 0..64).prop_map(|chars| chars.into_iter().collect())
}

/// The three distributions in one strategy, so that every claim below is made
/// over all three rather than over whichever one its test happened to name.
///
/// `any::<String>()` is the honest control and it is weighted lowest for the
/// reason `mt-inline`'s file gives: it is the distribution `fuzz/` starts from
/// and the one that will find a boundary bug, but on its own it produces a
/// document with no block structure in it roughly always.
fn a_document() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => markdownish_document(),
        3 => unicode_soup(),
        1 => any::<String>(),
    ]
}

// ---------------------------------------------------------------------------
// The five claims, one function each
// ---------------------------------------------------------------------------

/// Apply `claim` at both option sets, skipping the two shapes [`is_a_known_panic`]
/// names.
///
/// The guard is here rather than in each claim so that there is exactly one
/// place a reader has to check to know that nothing else is being skipped.
fn at_both_option_sets(
    src: &str,
    claim: impl Fn(&str, Options, &str) -> Result<(), TestCaseError>,
) -> Result<(), TestCaseError> {
    if is_a_known_panic(src) {
        return Ok(());
    }
    for (label, options) in OPTION_SETS {
        claim(src, options, label)?;
    }
    Ok(())
}

/// **D3 claim 1 — the gate itself.** §11.3: *"Malformed input must never panic
/// — `panic = "abort"` makes a panic a crash."*
///
/// Every public entry point of the crate, called and — with one exception —
/// **not** asserted about, deliberately. `dump_state`'s JSON is compared against
/// the reference engine by `cargo xtask blocks` over 1344 inputs and
/// `render_to_static_html`'s output by `cargo xtask conformance` over 1324;
/// neither has an equality against `src` that could be checked here, and a
/// property test that encodes a guess fails on a correct input. What all of them
/// *do* have to be is total, and calling them is the whole check.
///
/// The exception is `render_to_static_html`'s `sanitize` parameter, which is
/// driven at **both** values because the sanitiser is a second walk of a second
/// tree (`ammonia`) and §11.3's row is about the whole path, not the parser.
fn nothing_panics(src: &str, options: Options, label: &str) -> Result<(), TestCaseError> {
    let parsed = parse(src, options);
    let markdown = serialize(&parsed.document, options);
    let _ = mt_md::dump_state(src, options);
    let _ = mt_md::render_to_static_html(src, options, false);
    let _ = mt_md::render_to_static_html(src, options, true);

    // And once more over the serializer's own output, which is the string a
    // save-then-open actually hands back to the parser and the one input class
    // no fixture in this repository contains.
    //
    // **The guard has to be re-asked here**, and finding that out cost a soak
    // run: [`is_a_known_panic`] is checked on the *source* by
    // [`at_both_option_sets`], and the serializer can produce one of the two
    // known shapes from a source that is neither. §10's item is about `parse`,
    // not about what a user typed, so a derived string is exactly as much a
    // caller of it.
    if !is_a_known_panic(&markdown) {
        let _ = parse(&markdown, options);
    }
    prop_assert!(
        markdown.is_empty() || markdown.ends_with('\n'),
        "{label}: serialize produced {markdown:?}, which a file layer cannot append to"
    );
    Ok(())
}

/// **D3 claim 2** — every range is one `&src[range]` accepts.
///
/// `str::is_char_boundary` on both ends and `end <= src.len()`, which together
/// are exactly what a `SourceMap::get`-then-slice needs and what M4's caret
/// placement will do on every click. `source_ranges.rs` makes the same claim
/// over the corpus; this makes it over strings nobody wrote.
///
/// **The slice is taken only where [`SourceMap::is_exact`] says the range is the
/// node's own extent**, which is §8's risk row — *"a range that is a coarse
/// stand-in is sliced with as though it were exact"* — obeyed rather than
/// restated. The bounds and boundary checks are made for *every* entry, coarse
/// included, because a coarse range is still a range and a caller that only
/// locates a block with it still indexes the string.
///
/// **Nesting is not asserted.** §10's second known non-bug, and this file's
/// header says why at length.
fn ranges_are_sliceable(src: &str, options: Options, label: &str) -> Result<(), TestCaseError> {
    let parsed = parse(src, options);
    for (id, range) in parsed.source_map.iter() {
        prop_assert!(
            range.start <= range.end && range.end <= src.len(),
            "{label}: {range:?} is not inside 0..{} for node {id:?}",
            src.len()
        );
        prop_assert!(
            src.is_char_boundary(range.start) && src.is_char_boundary(range.end),
            "{label}: {range:?} is not on a character boundary"
        );
        if parsed.source_map.is_exact(id) {
            let _ = &src[range];
        }
    }
    Ok(())
}

/// **D3 claim 3** — one entry per live node, no more and no fewer.
///
/// `source_ranges.rs::the_map_has_exactly_one_entry_per_live_node` makes this
/// claim over the corpus and `every_node_has_a_range_and_the_ranges_nest` makes
/// its per-node half; the brief for this stage asked whether it was safe to
/// generalise from the corpus to every string, and the answer measured here is
/// **yes** — including under the two mechanisms that make the map
/// non-injective, which give two nodes the *same* range but never give a node
/// no range.
///
/// Stated for every live node rather than only for leaves, because that is what
/// holds and because a consumer walking the tree does not know which nodes are
/// leaves before it asks.
fn the_map_covers_the_live_tree(
    src: &str,
    options: Options,
    label: &str,
) -> Result<(), TestCaseError> {
    let parsed = parse(src, options);
    let ids = tree_ids(&parsed.document);
    prop_assert!(
        parsed.source_map.get(parsed.document.root()).is_none(),
        "{label}: the root has no block and must have no range"
    );
    for id in &ids {
        prop_assert!(
            parsed.source_map.get(*id).is_some(),
            "{label}: a live node carries no range"
        );
    }
    prop_assert_eq!(
        parsed.source_map.len(),
        ids.len(),
        "{}: the map holds an entry that is not a live node, or misses one",
        label
    );
    Ok(())
}

/// **D3 claim 4 — the round trip settles.** `m4 == m5`, this file's central
/// claim, and the number of applications is measured rather than assumed.
///
/// # Why the *fourth* application and not the second
///
/// Over `round_trip.rs`'s fixed 1346 the second is enough — `m2 == m3` holds
/// 1346 of 1346, and [`the_round_trip_converges_over_every_input`] asserts
/// exactly that with no exception list. **Over generated documents it is not.**
/// Measured over this file's three strategies at both option sets from a random
/// seed, searching up to sixteen applications and skipping
/// [`the_settling_claim_does_not_apply`] at every step:
///
/// | Settles after | Documents |
/// |---|---:|
/// | 1 application | 458,272 |
/// | 2 | 8,317 |
/// | 3 | 316 |
/// | 4 | 4 |
/// | more than 4, or never | **0** |
/// | | **466,909** checked, 67,379 more guarded |
///
/// The three hundred and sixteen are a blank line that the port inserts one
/// pass later than the first serialization does, and the shape that produces
/// them is a setext-shaped underline inside a list item —
/// `"1. a\n--\n0\n\t- a"` is the smallest, and `">a\n \t- a\n--\n--"` the
/// smallest that needs all four. Asserting the second application would fail on
/// every one of them, and none is oscillating: they are slower than the fixed
/// corpus led S4 to expect, which is the whole thing a generator is for.
///
/// So the honest statement of *"repeated open/save must not drift or
/// oscillate"* is a **bounded** settling time with the bound measured, and
/// [`SETTLES_BY`] is that bound. **The table above is this generator's and not
/// the language's**, which is the mistake that put the bound at 4 — see
/// [`SETTLES_BY`] for the two soak inputs above it and for the re-measure. The
/// four families for which no bound is right are named by
/// [`the_settling_claim_does_not_apply`], and three of them are findings this
/// file reports rather than behaviours it tolerates.
fn the_round_trip_settles(src: &str, options: Options, label: &str) -> Result<(), TestCaseError> {
    let mut sequence = Vec::with_capacity(SETTLES_BY + 1);
    let mut current = src.to_string();
    for _ in 0..=SETTLES_BY {
        if the_settling_claim_does_not_apply(&current) {
            return Ok(());
        }
        current = serialize(&parse(&current, options).document, options);
        sequence.push(current.clone());
    }
    let shown: Vec<String> = sequence
        .iter()
        .enumerate()
        .map(|(i, m)| format!("\n  m{}: {:?}", i + 1, truncate(m)))
        .collect();
    prop_assert_eq!(
        &sequence[SETTLES_BY - 1],
        &sequence[SETTLES_BY],
        "{}: the round trip had not settled after {} applications{}",
        label,
        SETTLES_BY,
        shown.concat()
    );
    Ok(())
}

/// How many applications of `serialize ∘ parse` the bytes are allowed to keep
/// moving for.
///
/// **Eight, measured — and it was 4, which was false.** The number is a named
/// constant precisely so that raising it has to be written down and argued, so
/// here is the argument.
///
/// # Why 4 was wrong
///
/// 4 was the observed maximum over [`the_round_trip_settles`]'s 466,909
/// generated documents, and an observed maximum over a generator that does not
/// cover the space is not a bound. S7's nightly soak drove the byte fuzzer at
/// the same claim and found two inputs above it inside one run:
///
/// | Soak input | bytes | settles after | mechanism |
/// |---|---:|---:|---|
/// | `crash-8e6638ae…` | 267 | **8** | over-indented child of a list item |
/// | `crash-d5e4b027…` | 163 | **6** | backslash run in a fence info string |
///
/// Both **settle**; neither oscillates, and none of the three exclusion
/// predicates fired on either input or on any of its first twenty
/// intermediates. So this was simply below the true maximum, and 8 is that
/// maximum re-measured here.
///
/// # The two laws behind the number, because a maximum without a law is the
/// mistake that produced 4
///
/// * **An over-indented child of a list item sheds four columns per pass**, so
///   its settling time is *linear in the indentation* and no constant covers
///   it. That family is excluded **by name** —
///   [`the_settling_time_is_known_to_grow_with_a_list_items_stray_indent`] —
///   and its threshold is derived from this constant rather than guessed.
///   `crash-8e6638ae…` carries 32 columns and is excluded by it.
/// * **A backslash run in a fence info string halves per pass**, so its
///   settling time is ⌊log₂ n⌋ + 1 in the run's length: 8 of 8 measured, `n` = 1
///   to 128. 8 therefore covers runs up to 255 backslashes, and
///   `crash-d5e4b027…`'s 37 settle at 6 with two passes to spare. This is a
///   *logarithmic* law rather than an unbounded one, which is why it is covered
///   by the constant instead of excluded by name — but it is bounded only by
///   the document's size, so a fence info string with 256 backslashes is where
///   8 stops being true. That is a **finding, not a guard**: both laws are
///   serializer defects (data the user loses or watches move on every save,
///   §11.3's *"Data safety"* row), and repairing them is blocked on the
///   `ExportMarkdown` differential runner §10 has declined to build. Neither is
///   repaired here; this constant only stops asserting something false.
///
/// # And the sentence above came true one soak later, which is why this
/// constant is now stated **here only**
///
/// The very next nightly run falsified 8 from bytes, with a **305-byte** input
/// — ` ```o ` and 300 backslashes, needing ⌊log₂ 300⌋ + 1 = 9. The response was
/// not to raise the constant again: a coverage-guided fuzzer over
/// `-max_len=65536` can always make the run longer, so against *bytes* any
/// constant is a statement about `max_len` rather than about the engine, and
/// the exclusion list would grow every run into an enumeration of engine
/// defects the property had been made blind to.
///
/// So `fuzz/fuzz_targets/round_trip.rs` **no longer asserts settling at all**
/// — it asserts totality under repetition, which is §11.3's actual clause —
/// and the settling claim lives here alone, where the generators emit shapes
/// this repository enumerates and the three laws above are the whole space.
/// The asymmetry is deliberate and is the opposite of the usual rule that two
/// harnesses should carry one claim: a claim that is true of a generator and
/// false of a fuzzer is **two** claims, and writing it once in each place
/// would have been the hole. §10's *"Owed by S7"* carries the day it goes
/// back: when the serializer defects are repaired the bound becomes real and
/// small, and the byte fuzzer should assert it again.
const SETTLES_BY: usize = 8;

/// **The first of the two families with no constant settling time at all** — a
/// code fence whose info string contains a backtick. S7 characterised the
/// second, and it is
/// [`the_settling_time_is_known_to_grow_with_a_list_items_stray_indent`].
///
/// # The measurement that makes this a guard rather than a fudge
///
/// `n` fences of `` ~~~a` `` followed by `` ~~~` ~ `` settle after exactly
/// **n + 2** applications, for every `n` from 1 to 12
/// ([`the_settling_time_grows_with_the_number_of_backtick_info_strings`] pins
/// six of them). So the family has no constant bound, no [`SETTLES_BY`] can
/// cover it, and excluding it is not a way of avoiding an inconvenient number —
/// it is the only statement about it that is true.
///
/// # The mechanism, which is muya's
///
/// This is `round_trip.rs`'s `FIXED_POINT_EXCEPTIONS` **mechanism 2**, whose
/// doc comment already names it as *"the only entry here that loses content
/// rather than moving it, and the only one still unstable on the third pass"*.
/// The serializer always emits backticks whatever the source used, and
/// CommonMark forbids a backtick inside a backtick fence's info string, so the
/// emitted fence is closed by its own info string; `_codeFenceLength` then
/// grows the opening fence past the longest all-backtick line in the body, and
/// the next pass has a longer line to grow past.
///
/// Driven directly against the reference engine — `MarkdownToState.generate`
/// then `new ExportMarkdown({listIndentation: 1}).generate(state)`, out of the
/// clone `cargo xtask diff` uses — muya produces **byte-identical output for
/// the first three passes** on both `` "~~~ aa ``` ~~~\n~~~ aa ``` ~~~" `` and
/// `` "~~~a`\n~~~` ~" ``. On the first it settles where this crate settles; on
/// the second **it keeps growing where this crate stops**, 29 → 31 → 33 → 35
/// bytes against a fixed point. That difference is a *serializer* divergence,
/// it is the exact thing §8's risk table records as unregisterable — *"rule 3
/// needs a failing differential case and no harness in this repository compares
/// serialized markdown"* — and S7 reports it rather than deciding it.
///
/// # Why the corpus does not contain it
///
/// It does, once: `commonmark-spec-0.31.json#146` **is** `` ~~~ aa ``` ~~~ ``.
/// One fence settles on its second pass, which is why
/// [`the_round_trip_converges_over_every_input`] needs no exception list at all
/// over the fixed 1346. It takes more than one for the growth to feed itself,
/// and no fixture has more than one. That is the whole distance between S4's
/// denominator and this one, and the clearest example in this file of what a
/// generator buys.
///
/// # What it costs, and where
///
/// 3,256 of 46,691 generated documents — high, because [`BLOCK_FRAGMENTS`]
/// carries `` "~~~ aa ``` ~~~" `` on purpose so the family is reachable at all.
/// It is affordable because this guard is checked **inside
/// [`the_round_trip_settles`] and nowhere else**: those 3,256 documents are
/// still parsed, still serialized, still checked for totality, ranges, map
/// completeness and the incremental identity. Only the settling claim declines
/// them, and only because that claim is false for them.
///
/// # How narrow it is
///
/// A line that opens a fence — three or more backticks or tildes, after any
/// leading spaces — whose remaining info string holds a backtick. It is an
/// over-approximation in one direction: such a line inside *another* fence is
/// content rather than an opener, and this predicate cannot tell without being
/// a second block parser.
/// The four shapes the settling claim is not made about, asked of **every**
/// element of the sequence and not only of the source.
///
/// Asking only about the source is wrong twice over, and both were found by
/// running rather than by reasoning: a soak run proved the serializer can emit
/// [`is_a_known_panic`]'s shape from a source that is neither, and
/// `">- [a]:x\n\t- [a]:x"` — which has no empty list item in it — serializes
/// into the oscillator that [`the_round_trip_is_known_to_oscillate`] names. A
/// sequence is a sequence of documents, and each of them is as much an input to
/// `parse` as the first one was.
fn the_settling_claim_does_not_apply(m: &str) -> bool {
    is_a_known_panic(m)
        || the_settling_time_is_known_to_be_unbounded(m)
        || the_settling_time_is_known_to_grow_with_a_list_items_stray_indent(m)
        || the_round_trip_is_known_to_oscillate(m)
}

/// **The second family with no constant settling time** — a list item whose
/// content is indented past its own column. S7's soak found it; nothing in this
/// repository had characterised it before.
///
/// # The law, measured
///
/// The serializer re-emits four columns fewer than `apply_prefix` consumed, so
/// each application peels one level and the settling time is **linear in the
/// indentation**:
///
/// ```text
/// format!("- a\n{}```o\n", " ".repeat(4 * n))   settles after exactly n
/// n        | 1 | 2 | 4 | 8 | 16 | 32
/// settles  | 1 | 2 | 4 | 8 | 16 | 32     6 of 6, and the tab spelling matches
/// ```
///
/// The tab is incidental — `"\t".repeat(n)` gives the identical table — so the
/// quantity is the indentation in **columns**, which is what this counts. The
/// list container is required: replacing the `- ` with `p ` makes the same
/// document settle on the first pass.
///
/// # Why a threshold, and why *this* threshold
///
/// A predicate that named the family without a magnitude would skip every
/// nested list in the corpus, which is most of the property. Instead the
/// threshold is **derived from [`SETTLES_BY`]**: over a 4,590-document sweep of
/// five markers × nine bodies × five container wraps × seventeen indents × two
/// option sets, the worst settling time among documents kept by a threshold of
/// `t` columns was exactly `t / 4 + 2`, monotone in `t`, for every `t` from 4 to
/// 32. At 24 columns that worst case is 8 — this file's constant, reached
/// exactly and not exceeded. So the two numbers are one statement: *beyond 24
/// columns the measured law says no bound of 8 can hold.*
///
/// **It skips nothing the fixed corpus contains**: 0 of 1,324 spec examples and
/// 0 of 11 round-trip fixtures are excluded by it. The cost of the exclusion is
/// therefore paid entirely by generated and fuzzed documents, which is where the
/// family lives.
///
/// It over-approximates in one direction, as its three siblings do: an indented
/// line inside a fenced code block is content rather than an over-indented
/// child, and this predicate cannot tell without being a second block parser.
///
/// Pinned by
/// [`the_settling_time_grows_with_a_list_items_indentation`].
fn the_settling_time_is_known_to_grow_with_a_list_items_stray_indent(src: &str) -> bool {
    /// A tab advances to the next multiple of four, which is the unit the law
    /// above is stated in. `>` is a container marker rather than indentation, so
    /// it advances the column without being counted as a run — a quoted list is
    /// the same family one level in.
    fn stray_indent_columns(src: &str) -> usize {
        src.split('\n')
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (mut column, mut widest) = (0usize, 0usize);
                for byte in line.bytes() {
                    match byte {
                        b' ' => {
                            column += 1;
                            widest = widest.max(column);
                        }
                        b'\t' => {
                            column += 4 - column % 4;
                            widest = widest.max(column);
                        }
                        b'>' => column += 1,
                        _ => break,
                    }
                }
                widest
            })
            .max()
            .unwrap_or(0)
    }

    /// ` {0,3}` then `[*+-]` or `\d{1,9}[.)]` — `marked`'s `bullet`, loosened to
    /// a scan for the same reason [`is_a_reference_definition`] is.
    fn opens_a_list_item(line: &str) -> bool {
        let rest = line.trim_start_matches([' ', '\t', '>']);
        let bytes = rest.as_bytes();
        if matches!(bytes.first(), Some(b'-' | b'*' | b'+'))
            && matches!(bytes.get(1), Some(b' ' | b'\t') | None)
        {
            return true;
        }
        let digits = rest.bytes().take(9).take_while(u8::is_ascii_digit).count();
        digits > 0 && matches!(bytes.get(digits), Some(b'.' | b')'))
    }

    // `worst = columns / 4 + 2`, so `worst <= SETTLES_BY` exactly while
    // `columns <= 4 * (SETTLES_BY - 2)` — 24. Written as the inequality rather
    // than as `24` so that the threshold cannot drift away from the constant it
    // was derived from.
    stray_indent_columns(src) > 4 * (SETTLES_BY - 2) && src.split('\n').any(opens_a_list_item)
}

/// **The round trip oscillates with period two on `"> - [a]: /x\n>   * "`, and
/// unlike everything else this file skips, muya does not.**
///
/// This is the one exclusion in this file that is a **port defect** rather than
/// an inherited or reproduced behaviour, and it should be read as a finding
/// rather than as a guard.
///
/// # The cycle
///
/// ```text
/// m1 = "> - [a]: /x\n>\n>   *\n"  loose
/// m2 = "> - [a]: /x\n>   * \n"    tight
/// m3 = m1, m4 = m2, forever
/// ```
///
/// The serializer alternates between the tight and the loose rendering of a
/// block quote holding a list item that contains a reference definition and an
/// empty nested item. `FIXED_POINT_EXCEPTIONS`' mechanism 6 — *"a blank line
/// before a non-paragraph block inside a tight item makes the list loose"* — is
/// the neighbourhood, but a **2-cycle** is a different and worse thing than the
/// one-way canonicalisation that mechanism describes, and no input in the fixed
/// 1346 shows one.
///
/// # muya settles immediately, on one of the two phases
///
/// Driven directly against the reference engine, `"> - [a]: /x\n>   * "` gives
/// `"> - [a]: /x\n>   * \n"` on its **first** pass and **stays there** —
/// nineteen bytes, unchanged, for as long as it is asked. That string is `m2`
/// above: the port reaches muya's fixed point on every even pass and leaves it
/// again on every odd one. So the two engines do not merely settle differently,
/// and the port does not merely take longer; the port never stops.
///
/// A user of the port saving this document twice gets two different files, and
/// saving it a third time gets the first one back. §11.3's *"Data safety"* row
/// is about a different mechanism, but this is the same class of harm — and it
/// is why this file's central claim is worth stating at all.
///
/// # Why S7 reports it instead of fixing it
///
/// Looseness is decided by `_insertLineBreak` and by what `loose` is read back
/// from, which is precisely the mechanism `FIXED_POINT_EXCEPTIONS` mechanism 6
/// and §10's *"Owed by S4"* nine already turn on, and it is settled in this
/// repository by `cargo xtask blocks --require-ts` against the running engine
/// over 1344 inputs — the harness a property-test stage does not run. Changing
/// looseness on a guess is how the 1344 stops being 1344. The evidence is here;
/// the decision is not S7's.
///
/// # How narrow it is
///
/// Three ingredients: a block-quote marker, a reference-definition line, and an
/// empty list item. `"> - a\n>   * "` settles on the first pass (no
/// definition), `"- a\n  * "` settles on the first pass (no quote), and
/// `"> - [a]:x\n>   - b"` settles on the first pass (no empty item). Over-
/// approximating: it does not check that the three are nested in one another,
/// because doing so would mean parsing.
fn the_round_trip_is_known_to_oscillate(src: &str) -> bool {
    let mut a_quote = false;
    let mut a_definition = false;
    let mut an_empty_item = false;
    for line in src.split('\n') {
        a_quote |= line.trim_start_matches([' ', '\t']).starts_with('>');
        a_definition |= is_a_reference_definition(line);
        an_empty_item |= is_an_empty_list_item(line);
    }
    a_quote && a_definition && an_empty_item
}

fn the_settling_time_is_known_to_be_unbounded(src: &str) -> bool {
    src.lines().any(|line| {
        let line = line.trim_start_matches(' ');
        let fence = line.trim_start_matches(['`', '~']);
        (line.starts_with("```") || line.starts_with("~~~")) && fence.contains('`')
    })
}

/// **D3 claim 5 — the zero-edit identity.** `Incremental::new(s, o)` is
/// `parse(s, o)`.
///
/// Cheap, and it pins the two entry points together: `tests/reparse_properties.rs`
/// compares an *edited* `Incremental` against a full parse, which says nothing
/// about the state it started from. If `new` ever stops being `parse` — a
/// pre-pass, a normalisation, a cached anything — every one of S6's properties
/// would still pass while measuring a different document from the one the caller
/// asked for.
///
/// The comparison is `dump_state`'s JSON, the whole `SourceMap`, and the label
/// map: the same three things `agrees_with_a_full_parse` compares next door,
/// except that the map can be compared **exactly** here rather than leaf by
/// leaf, because with no edit applied there is no region boundary for the two
/// parses to disagree about.
fn the_incremental_entry_point_agrees(
    src: &str,
    options: Options,
    label: &str,
) -> Result<(), TestCaseError> {
    let full = parse(src, options);
    let incremental = Incremental::new(src, options);
    prop_assert_eq!(
        mt_md::state::to_state_json(incremental.document()),
        mt_md::state::to_state_json(&full.document),
        "{}: Incremental::new produced a different tree from parse",
        label
    );
    prop_assert!(
        incremental.source_map() == &full.source_map,
        "{label}: Incremental::new produced a different source map from parse"
    );
    prop_assert!(
        incremental.labels() == &full.labels,
        "{label}: Incremental::new produced a different label map from parse"
    );
    prop_assert_eq!(
        incremental.source(),
        src,
        "{}: Incremental::new did not keep the source it was given",
        label
    );
    Ok(())
}

/// Everything above, for one input — what the soak drives and what a fuzz
/// target would.
fn check(src: &str) -> Result<(), TestCaseError> {
    at_both_option_sets(src, nothing_panics)?;
    at_both_option_sets(src, ranges_are_sliceable)?;
    at_both_option_sets(src, the_map_covers_the_live_tree)?;
    at_both_option_sets(src, the_round_trip_settles)?;
    at_both_option_sets(src, the_incremental_entry_point_agrees)?;
    Ok(())
}

/// §11.3's limit: no tree a parse builds is deeper than
/// [`mt_md::MAX_NESTING_DEPTH`].
///
/// The claim is about `parse`, so it is checked on the tree rather than on the
/// walk that built it — a limit enforced in one of the two halves of
/// `block.rs`'s clamp and not the other would still pass a test that only ran
/// the reproducer.
fn the_tree_is_inside_the_limit(
    src: &str,
    options: Options,
    label: &str,
) -> Result<(), TestCaseError> {
    let deepest = deepest_node(&parse(src, options).document);
    prop_assert!(
        deepest <= mt_md::MAX_NESTING_DEPTH,
        "{label}: {deepest} deep\n  src: {:?}",
        truncate(src)
    );
    Ok(())
}

/// The depth of the deepest node, counting a top-level block as 1.
///
/// Iterative for the reason everything on this path is: the tree it is asked
/// about is the one a recursive walk could not survive.
fn deepest_node(doc: &Document) -> usize {
    let mut deepest = 0;
    let mut stack: Vec<(NodeId, usize)> =
        doc.children(doc.root()).iter().map(|id| (*id, 1)).collect();
    while let Some((id, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        stack.extend(doc.children(id).iter().map(|child| (*child, depth + 1)));
    }
    deepest
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

// ---------------------------------------------------------------------------
// The properties
// ---------------------------------------------------------------------------

proptest! {
    // 192 to match `tests/reparse_properties.rs`, which is what CI can afford on
    // three platforms: each case here parses the same input six times per option
    // set, so the per-case cost is nearer that file's than `mt-inline`'s 512.
    #![proptest_config(ProptestConfig { cases: 192, ..ProptestConfig::default() })]

    /// D3 claim 1, at both option sets.
    #[test]
    fn parse_and_its_consumers_are_total_over_generated_documents(src in a_document()) {
        at_both_option_sets(&src, nothing_panics)?;
    }

    /// D3 claim 2, at both option sets.
    #[test]
    fn every_source_range_is_in_bounds_and_on_a_char_boundary(src in a_document()) {
        at_both_option_sets(&src, ranges_are_sliceable)?;
    }

    /// D3 claim 3, at both option sets.
    #[test]
    fn every_live_node_has_a_source_range(src in a_document()) {
        at_both_option_sets(&src, the_map_covers_the_live_tree)?;
    }

    /// D3 claim 4, at both option sets — the round trip settles within
    /// [`SETTLES_BY`] applications, which is what this file exists for.
    #[test]
    fn the_round_trip_settles_within_the_measured_bound(src in a_document()) {
        at_both_option_sets(&src, the_round_trip_settles)?;
    }

    /// D3 claim 5, at both option sets.
    #[test]
    fn the_incremental_entry_point_agrees_with_a_full_parse_before_any_edit(src in a_document()) {
        at_both_option_sets(&src, the_incremental_entry_point_agrees)?;
    }

    /// §11.3's limit, as a property rather than as a promise.
    ///
    /// It is stated over the same three distributions as the five above rather
    /// than only over the reproducer, because the reservation the limit uses is
    /// **per container kind** ([`mt_md::MAX_NESTING_DEPTH`]) and a generated
    /// document mixes them.
    ///
    /// It is an addition rather than a change: nothing above needed a new
    /// exception for the limit, and the closing assertion of
    /// [`the_generator_produces_every_block_kind_the_options_can_build`] is the
    /// measurement that says why — the deepest tree these strategies reach over
    /// 4,000 samples is not within a factor of four of the limit, so
    /// [`SETTLES_BY`] and the four claims beside it are testing exactly what
    /// they were testing before.
    #[test]
    fn no_generated_document_parses_deeper_than_the_limit(src in a_document()) {
        at_both_option_sets(&src, the_tree_is_inside_the_limit)?;
    }
}

// ---------------------------------------------------------------------------
// The generators' own negative controls
// ---------------------------------------------------------------------------

/// **A generator that quietly degenerates to paragraphs makes every property
/// above vacuous**, and nothing in a green run would say so.
///
/// `mt-inline`'s `the_generator_reaches_every_handler_that_default_options_can_reach`
/// is the same test one layer down, and its lesson is the reason this one
/// exists: 4,000 samples of a plausible-looking fragment table produced not one
/// `link` token, because assembling `[`, text, `]`, `(`, url, `)` in that order
/// out of a uniform pick happens too rarely to matter. The block-layer version
/// of that failure is a table full of markers that never produces a *nested*
/// container, and a property proved only over flat documents is a property about
/// paragraphs.
///
/// Deterministic rather than random — a fixed RNG seed — so a change that
/// narrows the distribution fails here every time rather than one run in twenty.
///
/// Two of `mt_doc::Block`'s nineteen names are not asserted and neither is this
/// generator's fault: `footnote` needs `options.footnote`, which is `false` in
/// **both** option sets because it is `false` in muya's own defaults
/// (`tests/reparse_properties.rs` is what drives it on), and `diagram` needs a
/// fence whose info string is one of four names — it is in the table, and it is
/// asserted, because unlike `footnote` nothing about the options prevents it.
#[test]
fn the_generator_produces_every_block_kind_the_options_can_build() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );

    let mut seen: std::collections::HashSet<&'static str> = std::collections::HashSet::new();
    let mut deepest = 0usize;
    let strategy = markdownish_document();
    for _ in 0..4_000 {
        let src = strategy
            .new_tree(&mut runner)
            .expect("the strategy produces a value")
            .current();
        if is_a_known_panic(&src) {
            continue;
        }
        for (_, options) in OPTION_SETS {
            let parsed = parse(&src, options);
            collect_names(
                &parsed.document,
                parsed.document.root(),
                0,
                &mut seen,
                &mut deepest,
            );
        }
    }

    for wanted in [
        "paragraph",
        "atx-heading",
        "setext-heading",
        "thematic-break",
        "code-block",
        "html-block",
        "math-block",
        "frontmatter",
        "diagram",
        "block-quote",
        "bullet-list",
        "order-list",
        "list-item",
        "task-list",
        "task-list-item",
        "table",
        "table.row",
        "table.cell",
    ] {
        assert!(
            seen.contains(wanted),
            "the generator never produced a `{wanted}` block, so every property above \
             was vacuous for it. Seen: {seen:?}"
        );
    }

    // A flat document exercises no container rule. Four levels is a list holding
    // an item holding a quote holding a paragraph, which is the shallowest
    // nesting that puts a container inside a container.
    assert!(
        deepest >= 4,
        "the deepest tree the generator produced was {deepest} levels — nothing here \
         exercised a container inside a container"
    );

    // **And the other end of the same measurement, added at S7.** The nesting
    // limit (`mt_md::MAX_NESTING_DEPTH`) is the one thing that could have made
    // a claim above quietly stop applying to part of its own distribution, and
    // it does not: the deepest tree 4,000 samples reach is not close to it, so
    // no property here needed an exception and none was added. A generator that
    // grew deep enough to clamp would fail this and say so, which is the point
    // of measuring rather than asserting `<= MAX_NESTING_DEPTH`.
    assert!(
        deepest * 4 < mt_md::MAX_NESTING_DEPTH,
        "the generator now reaches {deepest} levels, which is within a factor of \
         four of the nesting limit ({}) — the claims above may be clamping",
        mt_md::MAX_NESTING_DEPTH
    );
}

fn collect_names(
    doc: &Document,
    id: NodeId,
    depth: usize,
    seen: &mut std::collections::HashSet<&'static str>,
    deepest: &mut usize,
) {
    *deepest = (*deepest).max(depth);
    for child in doc.children(id) {
        if let Some(block) = doc.block(*child) {
            seen.insert(block.name());
        }
        collect_names(doc, *child, depth + 1, seen, deepest);
    }
}

/// [`is_the_known_upstream_panic`] covers `docs/upstream-issues.md`'s
/// reproducer **and** the wider class this file measured.
///
/// Three groups, and the second and third are the load-bearing ones.
///
/// The `false` group says the guard is about three ingredients *together*
/// rather than about lists, or definitions, or unusual whitespace — a guard
/// that skips broadly is a guard that hides the next real bug. The
/// over-approximated group is stated rather than asserted: those inputs have
/// all three ingredients and parse anyway, and pinning them would pin an
/// upstream boundary this file deliberately does not claim to know.
///
/// It is also what makes the guard becoming dead discoverable — the day
/// upstream fixes `parse.rs:2199`,
/// `block::tests::pulldown_cmark_panics_on_a_definition_in_a_quoted_list_item_before_a_tab_line`
/// fails, and this test is the second place a reader will land.
#[test]
fn the_upstream_panic_guard_covers_the_recorded_shape_and_the_wider_class() {
    for src in [
        // §10's own reproducer and its indented variant.
        "> - [a]: /x\n\t",
        "> - [a]: /x\n \t",
        // The class the soak found: no block quote, and a whitespace character
        // CommonMark does not count as blank.
        "- [a]:x\n\u{b}",
        "* [a]:x\n\u{c}",
        "1. [a]:x\n\u{2028}",
        "- [a]:x\n\u{3000}",
        // The definition on the item's continuation line, and two levels deep —
        // the second escaped a one-marker version of `is_a_reference_definition`
        // in 352,572 samples.
        "-\n  [a]:x\n\u{b}",
        "-\t- [a]:r\n\u{b}",
    ] {
        assert!(
            is_the_known_upstream_panic(src),
            "{src:?} panics in pulldown-cmark and the guard missed it"
        );
    }

    for src in [
        "> [a]: /x\n\t",     // no list item
        "[a]:x\n\u{b}",      // no list item
        "> - x\n\t",         // no reference definition
        "- x\n\u{b}",        // no reference definition
        "> - [a]: /x\n\tb",  // the trailing line has content
        "> - [a]: /x\n",     // no whitespace-only line at all
        "> - [a]: /x\n    ", // whitespace, but all of it spaces
        "",
        "\t",
    ] {
        assert!(
            !is_the_known_upstream_panic(src),
            "{src:?} parses normally and the guard skipped it anyway — a guard that \
             skips broadly is a guard that hides the next real bug"
        );
    }

    // Over-approximated on purpose; see the guard's docs. Not asserted either
    // way, listed so that a reader who tries them is not surprised.
    for src in ["- [a]: /x\n\t", "- [a]:x\nz\n\u{b}"] {
        assert!(is_the_known_upstream_panic(src), "{src:?}");
    }
}

/// **The class that used to panic, parsing.** `"- a\n\u{2028}"` is the minimal
/// input — eleven bytes, a bullet list item and one LINE SEPARATOR — and this
/// test was `#[should_panic]` until S7 made `block.rs`'s dedent cut on a `char`
/// boundary.
///
/// It is the same ratchet turned around. A `#[should_panic]` guarded a defect
/// nobody had decided how to repair; this guards the repair — and it guards the
/// **class** rather than the one input, because the reproducer is minimal and a
/// minimal reproducer is the easiest thing to accidentally special-case.
///
/// Nine characters, every one `str::trim` calls whitespace and CommonMark does
/// not count as blank, across eight documents at two option sets: **144
/// parses**. `block::tests::a_wide_whitespace_continuation_line_dedents_to_nothing`
/// asserts what the dedent *returns* and carries the measurement that chose it;
/// what is asserted here is §11.3's own words and nothing more — the call
/// returns — because this file's claim is totality, and duplicating the text
/// assertion would make one text change fail in two places for one reason.
#[test]
fn parse_does_not_panic_on_a_wide_whitespace_line_inside_a_list_item() {
    assert!(
        !parse("- a\n\u{2028}", Options::SPEC).source_map.is_empty(),
        "the reproducer parsed to nothing"
    );

    for line in [
        "\u{b}", "\u{c}", "\u{85}", "\u{a0}", "\u{1680}", "\u{2028}", "\u{2029}", "\u{202f}",
        "\u{3000}",
    ] {
        for src in [
            format!("- a\n{line}"),
            // Another bullet, an ordered marker, a task marker and a marker
            // padded wide enough to move the cut — the item's indent is what
            // decides where the cut lands, and it is not a property of `line`.
            format!("* a\n{line}"),
            format!("1. a\n{line}"),
            format!("- [ ] a\n{line}"),
            format!("-     a\n{line}"),
            // The cut inside the *second* character rather than the first.
            format!("- a\n {line}"),
            format!("- a\n{line}{line}"),
            // The wide line may come **before** the marker, because the
            // stripper walks back to the previous source line — the row that
            // cost a sweep when the deleted guard was written the other way up.
            format!("{line}\n- a"),
        ] {
            for (label, options) in OPTION_SETS {
                assert!(
                    !parse(&src, options).source_map.is_empty(),
                    "{label}: {src:?} parsed to nothing"
                );
            }
        }
    }
}

/// **The ratchet on the oscillation**: it is still a 2-cycle, one of its two
/// phases is still muya's fixed point, and the guard still names it.
///
/// [`the_round_trip_is_known_to_oscillate`] excludes an input from this file's
/// central claim on the grounds that the behaviour is a **defect**, so the
/// exclusion has to assert the defect or it is a place a fix could land without
/// anybody noticing. The day the item stops being read back as loose, this test
/// fails, the guard can be deleted, and the twelve lines above it about muya
/// can come out with it.
///
/// The muya half is a literal rather than a live comparison for the reason
/// `round_trip.rs` gives about itself: *"the round-trip property is a claim
/// about the Rust engine alone"*, and `cargo test --workspace` runs on three
/// platforms with no Node process. The value was measured against the running
/// engine — see the guard — and what is pinned here is that the port's even
/// passes still land on it, which is what the disagreement is made of.
#[test]
fn the_round_trip_oscillates_between_a_tight_and_a_loose_rendering() {
    /// What `MarkdownToState.generate` then `ExportMarkdown.generate` produce
    /// for the input below on the first pass, and on every pass after it.
    const MUYAS_FIXED_POINT: &str = "> - [a]: /x\n>   * \n";

    let src = "> - [a]: /x\n>   * ";
    assert!(
        the_round_trip_is_known_to_oscillate(src),
        "the guard does not name its own reproducer"
    );

    let mut sequence = vec![src.to_string()];
    for _ in 0..6 {
        let last = sequence.last().expect("non-empty").clone();
        sequence.push(serialize(
            &parse(&last, Options::SPEC).document,
            Options::SPEC,
        ));
    }

    assert_eq!(
        sequence[2], MUYAS_FIXED_POINT,
        "the port's even passes no longer land on muya's fixed point. That is a bigger \
         change than the oscillation and it wants measuring against the engine again, \
         not editing here:\n{sequence:#?}"
    );
    assert_ne!(
        sequence[1], sequence[2],
        "the oscillation is gone. Delete `the_round_trip_is_known_to_oscillate` and stop \
         skipping it — this is the outcome to want:\n{sequence:#?}"
    );
    for phases in sequence[1..].windows(3) {
        assert_eq!(
            phases[0], phases[2],
            "it is no longer a 2-cycle, which is a different behaviour from the one the \
             guard was written for:\n{sequence:#?}"
        );
    }
}

/// **The ratchet on the family with no constant settling time**: `n` fences
/// take `n + 2` applications, exactly, so no [`SETTLES_BY`] can ever cover it.
///
/// [`the_settling_time_is_known_to_be_unbounded`] excludes an input from this
/// file's central claim, so the exclusion has to be a claim of its own or it is
/// a hole — and "this input is awkward" is not a claim, whereas "the settling
/// time is a linear function of the input and here are its two coefficients"
/// is. The day `_codeFenceLength` or the tilde-to-backtick canonicalisation
/// changes, this fails with the new number, and the guard can be deleted rather
/// than quietly outliving the behaviour it was written for.
///
/// Measured to `n = 12` and asserted to `n = 6`, which is where the documents
/// stop being small enough to be worth parsing seven times in an every-commit
/// test.
#[test]
fn the_settling_time_grows_with_the_number_of_backtick_info_strings() {
    for n in 1..=6usize {
        let src = format!("{}~~~` ~", "~~~a`\n".repeat(n));
        assert!(
            the_settling_time_is_known_to_be_unbounded(&src),
            "the guard does not name its own reproducer at n = {n}"
        );

        let mut sequence = vec![src.clone()];
        for _ in 0..(n + 4) {
            let last = sequence.last().expect("non-empty").clone();
            sequence.push(serialize(
                &parse(&last, Options::SPEC).document,
                Options::SPEC,
            ));
        }
        let settled_at = (1..sequence.len() - 1)
            .find(|&i| sequence[i] == sequence[i + 1])
            .unwrap_or_else(|| {
                panic!(
                    "n = {n}: still moving after {} applications",
                    sequence.len() - 1
                )
            });

        assert_eq!(
            settled_at,
            n + 2,
            "n = {n}: the settling time is no longer n + 2. If the family now settles in a \
             bounded number of passes, delete `the_settling_time_is_known_to_be_unbounded` \
             and stop skipping it:\n{sequence:#?}"
        );
    }
}

/// **The ratchet on the second family with no constant settling time**: a list
/// item's over-indented child sheds four columns per application, so the
/// settling time is the indentation divided by four and no [`SETTLES_BY`] can
/// ever cover it.
///
/// [`the_settling_time_is_known_to_grow_with_a_list_items_stray_indent`]
/// excludes an input from this file's central claim, so the exclusion has to be
/// a claim of its own or it is a hole — the same argument
/// [`the_settling_time_grows_with_the_number_of_backtick_info_strings`] makes
/// next door, and the same shape. "This input is awkward" is not a claim; "the
/// settling time is the indentation in columns divided by four, and here is the
/// table" is.
///
/// The day the serializer emits the same number of indent columns
/// `block::apply_prefix` consumes, this fails with the new number and the guard
/// can be deleted rather than quietly outliving the behaviour it was written
/// for. That repair is a **serializer** change, which this repository settles
/// against `ExportMarkdown` and not against a guess — §10's owed differential
/// runner — so S7 reports the law rather than closing it.
///
/// Measured to `n = 32` and asserted to `n = 8`, which is where the documents
/// stop being small enough to be worth parsing nine times in an every-commit
/// test.
#[test]
fn the_settling_time_grows_with_a_list_items_indentation() {
    for n in 1..=8usize {
        // Spaces rather than tabs: the two spellings give the identical table,
        // so the quantity is columns, and spaces say so without a tab stop.
        let src = format!("- a\n{}```o\n", " ".repeat(4 * n));

        let mut sequence = vec![src.clone()];
        for _ in 0..(n + 4) {
            let last = sequence.last().expect("non-empty").clone();
            sequence.push(serialize(
                &parse(&last, Options::SPEC).document,
                Options::SPEC,
            ));
        }
        let settled_at = (1..sequence.len() - 1)
            .find(|&i| sequence[i] == sequence[i + 1])
            .unwrap_or_else(|| {
                panic!(
                    "n = {n}: still moving after {} applications",
                    sequence.len() - 1
                )
            });

        assert_eq!(
            settled_at, n,
            "n = {n}: the settling time is no longer the indentation over four. If the family \
             now settles in a bounded number of passes, delete \
             `the_settling_time_is_known_to_grow_with_a_list_items_stray_indent` and stop \
             skipping it:\n{sequence:#?}"
        );
    }

    // The guard names its own reproducer only past the threshold, and the
    // threshold is the point at which `SETTLES_BY` stops covering the law — so
    // these two assertions are the derivation, run.
    let under = format!("- a\n{}```o\n", " ".repeat(4 * (SETTLES_BY - 2)));
    let over = format!("- a\n{}```o\n", " ".repeat(4 * (SETTLES_BY - 2) + 1));
    assert!(
        !the_settling_time_is_known_to_grow_with_a_list_items_stray_indent(&under),
        "24 columns settles inside SETTLES_BY and must stay in the claim"
    );
    assert!(
        the_settling_time_is_known_to_grow_with_a_list_items_stray_indent(&over),
        "25 columns cannot settle inside SETTLES_BY and must leave it"
    );
    // And the container is required: the same indentation with no list opener
    // above it is not this family and is not skipped.
    assert!(
        !the_settling_time_is_known_to_grow_with_a_list_items_stray_indent(&format!(
            "p a\n{}```o\n",
            " ".repeat(64)
        )),
        "no list opener, no shed"
    );

    // The soak input this whole family was characterised from: 32 columns of
    // stray indent, excluded, and it settles at 8 — which is where
    // [`SETTLES_BY`] came from.
    assert!(
        the_settling_time_is_known_to_grow_with_a_list_items_stray_indent(
            "* *\n\t\t\t\t\t\t\t```o\n"
        )
    );
}

/// The guard is narrow enough to be worth having, and wide enough to be the
/// only thing skipped.
///
/// Two claims in one sweep, because they are two halves of the same question
/// and separating them would mean generating 8,000 documents to answer it:
///
/// 1. **It skips little.** **89 of 1,600**, which is what makes "narrow" a
///    measurement rather than an adjective. Two guards skipped **119** of the
///    same 1,600 — 89 upstream and 30 for the stripper panic S7 repaired — so
///    the repair bought 30 documents back into every claim below. (The `114`,
///    split `82`/`32`, that this paragraph carried before does not reproduce at
///    the committed generator; it predates the widening of
///    [`is_the_known_upstream_panic`]. Re-measured rather than re-copied.) It is
///    not lower because the guard deliberately over-approximates rather than
///    reproduce an upstream boundary it does not know, and because this claim is
///    only about *panics*: an input a panic guard skips is skipped by every
///    claim, whereas the far larger settling guard is checked inside one alone.
///    [`BLOCK_FRAGMENTS`] deliberately contains `"> - [a]: /x"`, `"\t"` and
///    `"- a"`, so the shape is *reachable* — a skip rate of zero would mean the
///    generators never get near it and the guard is untested.
/// 2. **Nothing else panics.** Every input the guard does *not* skip is parsed
///    at both option sets under a swapped panic hook, and a panic that escapes
///    it fails this test with the input printed. That is the ratchet in the
///    useful direction: a second shape lands here, with a reproducer, rather
///    than in a red nightly soak without one. Over 60,000 samples at the stage
///    that wrote this, the two named shapes were the only two; S7's repair took
///    one of them out of the guards and left it in the checked population, so
///    the wide-whitespace lines [`AWKWARD`] emits are now *asserted about*
///    rather than skipped.
///
/// `catch_unwind` works here because a test target is built to unwind whatever
/// the profile says. It is **not** why the guard is a pre-check: a `fuzz/`
/// target or a shipped binary is `panic = "abort"` (§12) and has no such
/// luxury, so a guard that ran after the fact would protect only this file.
#[test]
fn the_only_panics_the_generators_reach_are_the_ones_the_guards_name() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );

    let strategies: [BoxedStrategy<String>; 2] =
        [markdownish_document().boxed(), unicode_soup().boxed()];
    let mut samples = 0usize;
    let mut upstream = 0usize;
    for strategy in &strategies {
        // 800 rather than a rounder number: each sample drives `check`, which
        // is eleven parses at each of two option sets, and this file's budget
        // is the every-commit job on three platforms. A sweep that has to be
        // `#[ignore]`d is a sweep that runs when somebody remembers.
        for _ in 0..800 {
            let src = strategy
                .new_tree(&mut runner)
                .expect("the strategy produces a value")
                .current();
            samples += 1;
            if is_the_known_upstream_panic(&src) {
                upstream += 1;
                continue;
            }
            // `check` and not `parse`, deliberately: the round trip re-parses
            // strings the *serializer* produced, and a soak run is what proved
            // those can be the known shape when the source is not. A sweep that
            // only drove `parse(src)` would have gone on reporting a clean run
            // while the nightly job aborted.
            assert!(
                quietly(|| {
                    let _ = check(&src);
                })
                .is_ok(),
                "the parse/serialize path panicked on {src:?}, which \
                 `is_the_known_upstream_panic` does not name. If it is a second shape of \
                 §10's upstream defect, widen that guard and say so; otherwise it is a \
                 panic in this crate and it is a finding — §11.3's gate row, which S7 \
                 repaired once already."
            );
        }
    }

    println!("the guard skipped {upstream} of {samples} generated documents");
    // One in twelve rather than the one in six two guards were held to: with
    // the stripper shape repaired and back in the checked population, the same
    // fraction would have been a bar the survivor could triple without firing.
    assert!(
        upstream * 12 < samples,
        "the guard skipped {upstream} of {samples} — it was 89 of 1,600 when S7 left it \
         alone, and that is no longer narrow. A guard that skips broadly is a guard that \
         hides the next real bug"
    );
}

/// Run something that may panic, without printing a backtrace for it — the same
/// helper `tests/reparse_properties.rs` carries, and per this repository's
/// convention a copy rather than a shared module.
fn quietly<T>(f: impl FnOnce() -> T) -> std::thread::Result<T> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(previous);
    out
}

// ---------------------------------------------------------------------------
// The measurement, over `round_trip.rs`'s own 1346 inputs
// ---------------------------------------------------------------------------

/// One input: a name for the failure message, its source, and the options both
/// engines would be driven with. `round_trip.rs`'s, copied for the reason this
/// file's header gives.
struct Input {
    name: String,
    src: String,
    options: Options,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// `bench/corpus/`, all eleven files, at `MUYA_DEFAULT`.
fn corpus_inputs() -> Vec<Input> {
    let dir = repo_root().join("bench").join("corpus");
    let mut inputs: Vec<Input> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter(|path| path.file_name().is_some_and(|name| name != "README.md"))
        .map(|path| {
            let name = format!(
                "bench/corpus/{}",
                path.file_name().expect("a file").to_string_lossy()
            );
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            Input {
                name,
                src: normalize_source(&text),
                options: Options::MUYA_DEFAULT,
            }
        })
        .collect();
    inputs.sort_by(|a, b| a.name.cmp(&b.name));
    inputs
}

/// `spec/fixtures/marktext-round-trip/`, all eleven, at `MUYA_DEFAULT`.
fn fixture_inputs() -> Vec<Input> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<Input>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let name = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                out.push(Input {
                    name,
                    src: normalize_source(&text),
                    options: Options::MUYA_DEFAULT,
                });
            }
        }
    }

    let root = repo_root().join("spec").join("fixtures");
    let mut inputs = Vec::new();
    walk(&root.join("marktext-round-trip"), &root, &mut inputs);
    inputs.sort_by(|a, b| a.name.cmp(&b.name));
    inputs
}

/// The `markdown` field of every example in a CommonMark/GFM spec fixture, at
/// `SPEC`.
fn spec_inputs(file: &str) -> Vec<Input> {
    let path = repo_root().join("spec").join("fixtures").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let examples: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    examples
        .as_array()
        .unwrap_or_else(|| panic!("{}: not a JSON array", path.display()))
        .iter()
        .map(|example| {
            let number = example["number"].as_u64().unwrap_or_default();
            let markdown = example["markdown"]
                .as_str()
                .unwrap_or_else(|| panic!("{file}: example {number} has no `markdown`"))
                .to_string();
            Input {
                name: format!("{file}#{number}"),
                src: markdown,
                options: Options::SPEC,
            }
        })
        .collect()
}

/// All 1346, in the order `round_trip.rs` names them.
fn every_input() -> Vec<Input> {
    let mut inputs = corpus_inputs();
    inputs.extend(fixture_inputs());
    inputs.extend(spec_inputs("commonmark-spec-0.31.json"));
    inputs.extend(spec_inputs("gfm-spec-0.29-gfm.json"));
    inputs
}

/// **The measurement that chose `MAX_NESTING_DEPTH`, re-taken on every run.**
///
/// §11.3's limit is only defensible if it is far above anything anyone writes,
/// and this is the denominator that says so: **the deepest of all 1346 inputs
/// is 11**, in `spec/fixtures/marktext-round-trip/common/Lists.md`, and the
/// distribution has a long flat tail — 1,102 of them are one level deep. So
/// [`mt_md::MAX_NESTING_DEPTH`] is 11.6× the deepest markdown this project
/// contains, **no test in this repository can reach it**, and the clamp is
/// provably behaviour-preserving over everything `cargo xtask blocks`,
/// `cargo xtask diff` and `cargo xtask conformance` measure.
///
/// The assertion is a **factor**, not the number 11: an input set that grew a
/// genuinely deeper document should not fail, and one that grew a document
/// within a factor of four of the limit should, because at that point the
/// harnesses above are measuring a clamped tree and nothing else would say so.
#[test]
fn the_deepest_input_this_repository_owns_is_an_order_of_magnitude_below_the_limit() {
    let inputs = every_input();
    assert_eq!(inputs.len(), 1346, "the same denominator as the gates");

    let (deepest, name) = inputs
        .iter()
        .map(|input| {
            (
                deepest_node(&parse(&input.src, input.options).document),
                input.name.clone(),
            )
        })
        .max()
        .expect("1346 inputs");

    assert!(
        deepest * 4 < mt_md::MAX_NESTING_DEPTH,
        "the deepest input is now {deepest} ({name}), within a factor of four of \
         the limit ({}) — the gates are measuring a clamped tree",
        mt_md::MAX_NESTING_DEPTH
    );
}

/// **D1's measurement, and the reason the convergence property has no exception
/// list.**
///
/// `m2 == m3` over the same 1346 inputs S4's gate is stated over, at the same
/// option sets, with the denominator asserted so that a corpus which silently
/// shrinks fails rather than passing over less.
///
/// | Claim | Measured |
/// |---|---|
/// | `m1 == m2` | **1334 of 1346** — [`SECOND_PASS_MOVERS`] is the other twelve |
/// | `m2 == m3` | **1346 of 1346**, no exceptions at any denominator here |
///
/// The second row is what makes the generated property above assertable without
/// a list. The first is worth having anyway: *"one round trip is enough"* is a
/// claim someone will make, and this is the twelve inputs on which it is false.
#[test]
fn the_round_trip_converges_over_every_input() {
    let inputs = every_input();
    assert_eq!(
        inputs.len(),
        1346,
        "11 corpus files + 11 round-trip fixtures + 1324 spec examples"
    );

    let mut still_moving = Vec::new();
    let mut regressions = Vec::new();
    let mut delistable = Vec::new();
    for input in &inputs {
        let m1 = serialize(&parse(&input.src, input.options).document, input.options);
        let m2 = serialize(&parse(&m1, input.options).document, input.options);
        let m3 = serialize(&parse(&m2, input.options).document, input.options);

        if m2 != m3 {
            still_moving.push(format!(
                "  {}\n    m2: {:?}\n    m3: {:?}",
                input.name,
                truncate(&m2),
                truncate(&m3)
            ));
        }

        let settled = m1 == m2;
        let listed = SECOND_PASS_MOVERS.contains(&input.name.as_str());
        match (listed, settled) {
            (true, false) | (false, true) => {}
            (true, true) => delistable.push(input.name.clone()),
            (false, false) => regressions.push(format!(
                "  {}\n    m1: {:?}\n    m2: {:?}",
                input.name,
                truncate(&m1),
                truncate(&m2)
            )),
        }
    }

    assert!(
        still_moving.is_empty(),
        "serialize(parse(m1)) != serialize(parse(m2)) for {} input(s).\n\
         This is S7's central claim and it has no exception list because the measurement \
         did not force one — 1346 of 1346 at the stage that took it. A failure here means \
         the serializer has started to oscillate, which is a document drifting on every \
         save, and the generated property in this file will be failing too.\n{}",
        still_moving.len(),
        still_moving.join("\n")
    );
    assert!(
        regressions.is_empty(),
        "serialize(parse(s)) is not already stable for {} input(s) that are NOT in \
         SECOND_PASS_MOVERS.\nOne round trip used to be enough for all but twelve. Either \
         the serializer has lost ground or the list needs an argued addition in the same \
         commit.\n{}",
        regressions.len(),
        regressions.join("\n")
    );
    assert!(
        delistable.is_empty(),
        "{} input(s) are listed in SECOND_PASS_MOVERS but now settle on the first pass.\n\
         Delete them from the list — this is the ratchet, and a floor that does not fall \
         when the code improves has stopped being one:\n  {}",
        delistable.len(),
        delistable.join("\n  ")
    );

    println!(
        "convergence: m1 == m2 for {} of {} inputs, m2 == m3 for {} of {} — \
         the second application is a fixed point with no exceptions",
        inputs.len() - SECOND_PASS_MOVERS.len(),
        inputs.len(),
        inputs.len(),
        inputs.len()
    );
}

/// **The twelve inputs on which one round trip is not yet stable**, each listed
/// twice because CommonMark and GFM ship the same example under different
/// numbers — so six examples.
///
/// A ratchet in `round_trip.rs`'s four-row shape: a listed input that starts
/// settling on the first pass fails the build and must be delisted, and an
/// unlisted one that stops settling fails it too.
///
/// # What they are, and the relationship to `round_trip.rs`'s list
///
/// **All twelve are among `FIXED_POINT_EXCEPTIONS`' twenty-two**, and the
/// converse is false: five of that file's eleven examples change the *tree* on
/// reparse without changing the *bytes* again, so they are fixed-point
/// exceptions whose serialized output was already stable. That asymmetry is the
/// clearest statement of what the two claims measure — S4's is about the tree
/// surviving a save, this one is about the bytes settling — and it is why
/// neither list can be derived from the other.
///
/// By `FIXED_POINT_EXCEPTIONS`' own numbering of the six mechanisms, the movers
/// are 2 (`#146`, a backtick fence cannot carry backticks in its info string),
/// 3 (`#238`, a lazy continuation becomes an explicit container line), 5
/// (`#313`, varying item indents collapse to the marker width) and 6 (`#300`,
/// `#320`, `#321`, a blank line before a block inside a tight item makes the
/// list loose). Mechanisms 1 and 4 are the ones whose bytes were already
/// canonical after one pass.
///
/// **Every one of them settles on the second**, which is the whole of D1's
/// argument and the reason the property above needs no list of its own.
const SECOND_PASS_MOVERS: &[&str] = &[
    // 2 — a backtick fence cannot carry backticks in its info string
    "commonmark-spec-0.31.json#146",
    "gfm-spec-0.29-gfm.json#116",
    // 3 — a lazy continuation becomes an explicit container line
    "commonmark-spec-0.31.json#238",
    "gfm-spec-0.29-gfm.json#216",
    // 5 — varying item indents collapse to the marker width
    "commonmark-spec-0.31.json#313",
    "gfm-spec-0.29-gfm.json#293",
    // 6 — a blank line before a block inside a tight item makes the list loose
    "commonmark-spec-0.31.json#300",
    "gfm-spec-0.29-gfm.json#278",
    "commonmark-spec-0.31.json#320",
    "gfm-spec-0.29-gfm.json#300",
    "commonmark-spec-0.31.json#321",
    "gfm-spec-0.29-gfm.json#301",
];

/// Every name in [`SECOND_PASS_MOVERS`] names a real input.
///
/// A stale entry silently weakens the ratchet by one input — the same failure
/// `spec/README.md` records for `expected-failures.json`, where a listed number
/// that names no example stops guarding anything.
#[test]
fn every_second_pass_mover_names_a_real_input() {
    let names: Vec<String> = every_input().into_iter().map(|i| i.name).collect();
    let unknown: Vec<&&str> = SECOND_PASS_MOVERS
        .iter()
        .filter(|e| !names.iter().any(|n| n == *e))
        .collect();
    assert!(
        unknown.is_empty(),
        "SECOND_PASS_MOVERS names inputs that do not exist: {unknown:?}"
    );

    let mut sorted: Vec<&&str> = SECOND_PASS_MOVERS.iter().collect();
    sorted.sort_unstable();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "duplicate entry in SECOND_PASS_MOVERS"
    );
}

// ---------------------------------------------------------------------------
// The soak
// ---------------------------------------------------------------------------

/// The M2 exit gate's soak arm for the **block** layer, on whatever platform
/// runs it.
///
/// # Why this exists beside `fuzz/`
///
/// The same reason `mt-inline`'s `tests/properties.rs::soak` does, and the
/// argument has not changed: `cargo-fuzz` needs a nightly toolchain,
/// `-Zsanitizer` and a bundled C++ libFuzzer, `rust-toolchain.toml` pins stable
/// for good reasons, and so `soak.yml`'s libFuzzer jobs run on Linux only.
/// **Windows ships first**, and a gate met only on Linux is a gate met on the
/// platform whose failures block least. So this is the same properties and the
/// same generators, driven for a wall-clock duration wherever it is run —
/// coverage-blind where libFuzzer is coverage-guided, and structure-*aware*
/// where libFuzzer starts from bytes. Neither subsumes the other.
///
/// # Running it
///
/// `#[ignore]` because it runs until told to stop, and a default `cargo test`
/// must stay in seconds.
///
/// ```sh
/// # one hour, with debug assertions live
/// MT_SOAK_SECONDS=3600 RUSTFLAGS="-C debug-assertions=yes" \
///   cargo test -p mt-md --release --test round_trip_properties -- --ignored soak --nocapture
/// ```
///
/// **`-C debug-assertions=yes` is not optional in a release soak**, for the
/// reason the file next door states in full: a plain `--release` run compiles
/// out every `debug_assert!` and checks strictly less than a debug run does.
/// `fuzz/Cargo.toml` sets the same flag in its profile for the same reason.
///
/// Without `MT_SOAK_SECONDS` it runs for 60 seconds, so that a bare
/// `-- --ignored` still does something finite and useful.
///
/// # It reports its slowest inputs, and that is deliberate
///
/// libFuzzer has `-report_slow_units`; this is the same idea, and it is what
/// makes a soak produce a **benchmark row** rather than only a pass/fail. The
/// method is `mt-inline`'s and so is its lesson: a single timing at these input
/// sizes measures the *scheduler*, so the ranking pass is only a candidate
/// filter and what is printed is the **minimum over [`RETIMES`] re-runs** of
/// inputs of at least [`MIN_TIMED_BYTES`]. Minimum rather than mean, because
/// preemption and page faults can only make a measurement longer — a genuinely
/// slow input stays slow across all of them and a hiccup collapses.
///
/// The block layer's cost model is not the tokenizer's, which is why this is
/// worth running here as well: `strip_lines` allocates a `String` per source
/// line (M2.md §6's S3 note), so a document whose containers are deep pays per
/// line per level, and that is the shape a per-byte ranking finds.
#[test]
#[ignore = "runs for MT_SOAK_SECONDS (default 60); the M2 exit gate runs it long"]
fn soak() {
    use std::time::{Duration, Instant};

    use proptest::test_runner::{Config, TestRunner};

    let seconds: u64 = std::env::var("MT_SOAK_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let deadline = Instant::now() + Duration::from_secs(seconds);

    // A *random* seed, not a deterministic one: a soak that replays the same
    // sequence every night is a very long way of running one test.
    // `Config::default()` seeds randomly, and a failure prints the offending
    // input itself — which is what reproducibility needs here, since there is
    // nothing to shrink from a single failing case.
    let mut runner = TestRunner::new(Config::default());

    let strategies: [BoxedStrategy<String>; 3] = [
        markdownish_document().boxed(),
        unicode_soup().boxed(),
        any::<String>().boxed(),
    ];

    let mut cases = 0u64;
    let mut bytes = 0u64;
    let mut skipped = 0u64;
    let mut longest = String::new();
    // Per **byte**, not per input, or the answer is always "the longest one".
    let mut candidates: Vec<(f64, String)> = Vec::new();
    while Instant::now() < deadline {
        for _ in 0..16 {
            for strategy in &strategies {
                let src = strategy
                    .new_tree(&mut runner)
                    .expect("the strategy produces a value")
                    .current();
                if is_a_known_panic(&src) {
                    skipped += 1;
                    continue;
                }
                let started = Instant::now();
                check(&src).unwrap_or_else(|e| {
                    panic!("soak failure on {src:?}\n{e}");
                });
                let per_byte = started.elapsed().as_nanos() as f64 / (src.len().max(1)) as f64;
                cases += 1;
                bytes += src.len() as u64;
                if src.len() > longest.len() {
                    longest = src.clone();
                }
                if src.len() >= MIN_TIMED_BYTES {
                    candidates.push((per_byte, src));
                    if candidates.len() > 4096 {
                        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
                        candidates.truncate(64);
                    }
                }
            }
        }
    }

    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    candidates.truncate(64);

    // Re-time the finalists and take the minimum of many runs — see the doc
    // comment, and `mt-inline`'s, for the eight scheduler artefacts that
    // method exists to keep out of a benchmark table.
    let mut slowest: Vec<(f64, String)> = candidates
        .into_iter()
        .map(|(_, src)| {
            let mut best = f64::INFINITY;
            for _ in 0..RETIMES {
                let started = Instant::now();
                let _ = check(&src);
                best = best.min(started.elapsed().as_nanos() as f64);
            }
            (best / src.len() as f64, src)
        })
        .collect();
    slowest.sort_by(|a, b| b.0.total_cmp(&a.0));
    slowest.truncate(8);

    println!(
        "soak: {cases} cases, {bytes} bytes, longest input {} bytes, over {seconds}s \
         ({skipped} skipped as one of the two known panics)",
        longest.len()
    );
    println!("slowest per byte (min of {RETIMES} re-runs, inputs >= {MIN_TIMED_BYTES} B):");
    for (per_byte, src) in &slowest {
        let shown: String = src.chars().take(90).collect();
        println!("  {per_byte:9.1} ns/B  {:5} B  {shown:?}", src.len());
    }
    assert!(cases > 0, "the soak ran no cases");
}

/// Below this, a single-call timing is dominated by the clock and the scheduler
/// rather than by the parser. Higher than `mt-inline`'s 64 because a block parse
/// is several passes over the document and a short input is mostly fixed cost.
const MIN_TIMED_BYTES: usize = 128;

/// How many times to re-run a slow-input candidate before believing its number.
/// Fewer than `mt-inline`'s 32 because one `check` here is eleven parses rather
/// than one tokenize.
const RETIMES: u32 = 8;
