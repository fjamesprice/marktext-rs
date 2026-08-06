//! **The one string class every `mt-md` target here declines, and it is not this
//! repository's bug.**
//!
//! `docs/M2.md` §10, *"Owed by S6"*, item 1, and `docs/upstream-issues.md`:
//! **`pulldown-cmark` 0.13.4 panics** on `Option::unwrap()` of `None` inside
//! `OffsetIter::next` (`parse.rs:2199`), reached before any of this
//! repository's code runs. `mt_md::parse`'s own doc comment names it as the one
//! exception to its totality, and
//! `mt_md::block::tests::pulldown_cmark_panics_on_a_definition_in_a_quoted_list_item_before_a_tab_line`
//! is `#[should_panic]` so that the day upstream fixes it, the build says so.
//!
//! # This is not a fix, and S7 is not the stage that decides
//!
//! §10 defers the decision — file upstream, pin a patched fork, or pre-scan in
//! `mt_md::block` — to **M4**, the first milestone with a user who can type it.
//! What S7 owes is that `.github/workflows/soak.yml` is not permanently red on a
//! deferred upstream defect, and this file is the whole of that: a **pre**-check
//! that makes a target return early, not a rescue.
//!
//! Catching it is not open. §12's release profile is `panic = "abort"`, and
//! `libfuzzer-sys` installs a panic hook that aborts the process regardless — so
//! a `catch_unwind` of the kind `crates/mt-md/tests/reparse_properties.rs` uses
//! exists only in a test binary and cannot be borrowed here.
//!
//! # Why this file is a `mod` and not a `[[bin]]`
//!
//! `cargo-fuzz` treats every `[[bin]]` of `fuzz/Cargo.toml` as a fuzz target and
//! would try to run this one. It is declared instead as
//! `#[path = "known_panics.rs"] mod known_panics;` by each target that calls
//! `mt_md::parse`, which is `parse`, `round_trip` and `reparse` — all three.
//!
//! # Why its coverage check is a startup assertion and not a `#[cfg(test)]`
//!
//! **Because neither of the two obvious placements can run.** Every `[[bin]]`
//! here carries `test = false`, so a `#[cfg(test)] mod tests` in a fuzz target's
//! module is dead code by construction. A `[[test]]` target over this same file
//! does not work either, and it was tried: `cargo test --test …` builds *every*
//! `[[bin]]` of the package so that an integration test can exec them, and these
//! bins are `#![no_main]` — they link only under the flags `cargo fuzz` passes,
//! and a plain `cargo test` fails at `LNK1561: entry point must be defined`.
//!
//! So [`the_guard_still_names_the_recorded_shapes`] runs **once per process**,
//! behind a `std::sync::Once` inside the predicate itself, which means it runs
//! at the start of every soak shard on the platform the gate is met on. That is
//! strictly more often than a test nobody can invoke, and the placement is
//! inside the predicate rather than at three call sites so that a fourth target
//! cannot acquire the guard without it.
//!
//! The **authoritative** ratchet is still next door:
//! `crates/mt-md/tests/round_trip_properties.rs`'s
//! `the_upstream_panic_guard_covers_the_recorded_shape_and_the_wider_class`,
//! which `cargo test --workspace` runs on three platforms on every push. This
//! one checks *this* copy, where it lives.
//!
//! # Why the predicate exists twice at all
//!
//! `fuzz/` is its own cargo workspace — deliberately, so the nightly requirement
//! stays out of `rust-toolchain.toml` — so a module cannot be shared across the
//! boundary without dragging `fuzz/` into `cargo test --workspace` and the
//! nightly toolchain in front of every build on three platforms. A copy is the
//! price of that split, and it is the same price this repository already pays
//! for per-file test helpers by convention.

/// The one shape a target here does not hand to `mt_md::parse`.
///
/// # What §10 records, and what S7 measured
///
/// §10 and `docs/upstream-issues.md` give **four** ingredients — a block quote,
/// a list item, a link reference definition, and a following whitespace-only
/// line ending in a tab — with `"> - [a]: /x\n\t"` as the reproducer. Every word
/// of that is still true. It is also **not the whole shape**, and S7's soak
/// found the rest within a minute:
///
/// | Trailing line, after `"- [a]:x"` | Result |
/// |---|---|
/// | `"\t"` U+0009 | ok — §10's row, and why the quote is in its reproducer |
/// | `"\u{b}"`, `"\u{c}"`, `"\u{2028}"`, `"\u{3000}"` | **panic** |
/// | `"\r"`, `" "`, `"\u{85}"`, `"\u{a0}"` | ok |
///
/// The real ingredient is **a line that is whitespace to Unicode and not blank
/// to CommonMark** — CommonMark's blank line is spaces and tabs only, so every
/// other whitespace character reaches the same unwrap. A tab gets there too but
/// needs the extra container level `"> - "` supplies, which is why §10's
/// reproducer has a block quote and why the wider class went unnoticed.
///
/// So this is the **wider** predicate, not §10's four-ingredient sentence.
/// Porting the narrow one would leave the soak red on `"- [a]:x\n\u{b}"`, which
/// is a string a fuzzer reaches in seconds.
///
/// # How narrow it is, and where it over-approximates
///
/// Three ingredients anywhere in the document: a line that opens a list item, a
/// line shaped like a link reference definition, and a line that is non-empty,
/// entirely whitespace, and not entirely spaces. All three, or it does not fire
/// — a list alone, a definition alone or a tab alone is not enough, and a blank
/// line of plain spaces is not either.
///
/// **It skips 89 of 1,600 generated documents**, measured by
/// `crates/mt-md/tests/round_trip_properties.rs`'s
/// `the_only_panics_the_generators_reach_are_the_ones_the_guards_name`, which
/// also asserts that nothing *else* panics on the other 1,511. That is the
/// number that makes "narrow" a measurement rather than an adjective, and the
/// reason it is not lower is deliberate: it over-approximates on order
/// (`"- [a]:x\nz\n\u{b}"` is skipped although it parses) and on the trailing
/// character (a tab counts even without a block quote, §10's own `ok` row).
/// Guessing at the exact boundary of someone else's bug is how a guard becomes
/// wrong in the expensive direction — a guard that skips broadly is a guard that
/// hides the next real bug.
pub(crate) fn is_the_known_upstream_panic(src: &str) -> bool {
    // Once per process — see the module doc for why this is not a `#[test]`.
    // On the fast path it is one relaxed atomic load, which at the four hundred
    // executions a second this target sustains is not measurable.
    static CHECKED: std::sync::Once = std::sync::Once::new();
    CHECKED.call_once(the_guard_still_names_the_recorded_shapes);
    predicate(src)
}

/// The predicate itself, without the once-per-process check around it, so that
/// [`the_guard_still_names_the_recorded_shapes`] can ask it from inside the
/// `Once` that is calling it.
fn predicate(src: &str) -> bool {
    let mut a_list_item = false;
    let mut a_definition = false;
    let mut a_blank_line_after_it = false;
    for line in src.split('\n') {
        a_list_item |= opens_a_list_item(line);
        // The order matters: the unwrap is about what `OffsetIter` finds *after*
        // the definition, and every panicking row measured has the whitespace
        // line following it. The list item may be anywhere, since it is the
        // container the definition sits in and therefore opens at or before it.
        if a_definition {
            a_blank_line_after_it |= !line.is_empty()
                && line.chars().all(char::is_whitespace)
                && line.chars().any(|c| c != ' ');
        }
        a_definition |= is_a_reference_definition(line);
    }
    a_list_item && a_definition && a_blank_line_after_it
}

/// A bullet or ordered marker followed by a space, a tab **or the end of the
/// line**, after any indentation — the opener `mt_md::block`'s `Prefix::Item` is
/// built from.
///
/// The end-of-line arm is not decoration: `*` alone is an *empty* list item in
/// CommonMark, and `"-\n  [a]:x\n\u{b}"` — one of the recorded reproducers below
/// — opens its item with a bare `-`. `"**"` and `"--"` still say no, which is
/// what keeps thematic breaks and setext underlines out.
pub(crate) fn opens_a_list_item(line: &str) -> bool {
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

/// Everything left of a line's content once every block-quote marker and list
/// marker has been taken off, plus whether any **list** marker was among them.
///
/// It is not `mt_md::block`'s `strip_lines` and does not try to be — it never
/// sees the tree, so it cannot know which markers are real. Over-approximating
/// is what a guard wants. `fuzz_targets/round_trip.rs` reuses it for the two
/// families whose settling time is excluded, for the same reason: they are
/// "what is left after the containers" questions and two hand-rolled copies
/// would be the "two scanners" mistake `docs/M2.md` §5 D3 spends a section on.
pub(crate) fn strip_container_prefixes(line: &str) -> (&str, bool) {
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

/// `[label]:` after **any number** of container prefixes.
///
/// The destination is not part of the shape — `"- [a]:x"` reproduces as well as
/// `"- [a]: /url"` — and the markers are optional and repeatable because two
/// measured reproducers need that: `"-\n  [a]:x\n\u{b}"` puts the definition on
/// the item's *second* line, and `"-\t- [a]:r\n\u{b}"` puts it two list levels
/// deep. The second escaped a version of this that stripped one marker, once, in
/// 352,572 samples of a random-seeded hunt — which is the argument for stripping
/// in a loop rather than hand-counting the levels a reproducer happened to have.
pub(crate) fn is_a_reference_definition(line: &str) -> bool {
    let (rest, _) = strip_container_prefixes(line);
    let Some(body) = rest.strip_prefix('[') else {
        return false;
    };
    body.find(']')
        .is_some_and(|close| body[close + 1..].starts_with(':'))
}

/// **Both directions, so that a guard which becomes dead is discoverable.**
///
/// Run once per process from [`is_the_known_upstream_panic`]; the module doc
/// says why it is not a `#[test]` and where the authoritative ratchet lives.
///
/// The day upstream fixes `parse.rs:2199`, `mt_md::block`'s `#[should_panic]`
/// reproducer fails and this is the second place a reader lands: every string in
/// the first list stops panicking, this guard stops earning its keep, and it and
/// the `mod known_panics;` lines in three targets come out together.
///
/// The second list is the half that matters more day to day. **A guard is only
/// as good as what it declines to skip**, and each of these differs from a
/// reproducer in exactly one ingredient.
///
/// The bodies call [`predicate`] rather than [`is_the_known_upstream_panic`],
/// which would re-enter the `Once` that called this and deadlock.
fn the_guard_still_names_the_recorded_shapes() {
    for src in [
        // §10's own reproducer and its indented variant.
        "> - [a]: /x\n\t",
        "> - [a]: /x\n \t",
        // The class S7's soak found: no block quote, and a whitespace character
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
        // Over-approximated on purpose; see the guard's docs. `"- [a]: /x\n\t"`
        // is §10's own `ok` row and is skipped anyway, and `"- [a]:x\nz\n\u{b}"`
        // parses because a content line intervenes.
        "- [a]: /x\n\t",
        "- [a]:x\nz\n\u{b}",
    ] {
        assert!(
            predicate(src),
            "the upstream-panic guard no longer names {src:?}. If `pulldown-cmark` has \
             fixed parse.rs:2199, delete the guard and the `mod known_panics;` lines with \
             it — see docs/M2.md §10 'Owed by S6' item 1"
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
            !predicate(src),
            "the upstream-panic guard now skips {src:?}, which parses normally — a guard \
             that skips broadly is a guard that hides the next real bug"
        );
    }
}
