//! Property tests — M1.md §5 D6, built in S7.
//!
//! > **Recommendation:** run the 24 h libFuzzer soak on Linux in CI, and run
//! > `proptest` on all three platforms in the normal test job. The corpus
//! > already contains the awkward inputs to seed from. Panic-freedom is a
//! > property of the code, not the platform — but the *platform-specific* risk
//! > here is UTF-8 boundary slicing, so make sure the proptest generator
//! > produces multi-byte and lone-surrogate-adjacent input.
//!
//! # What this file is for, and what `fuzz/` is for
//!
//! They check the same properties over different input distributions, and the
//! division is D6's:
//!
//! | | here | `fuzz/` |
//! |---|---|---|
//! | Runs | every `cargo test`, three platforms | nightly, Linux only |
//! | Input | *shaped* — fragments that reach handlers | coverage-guided bytes |
//! | Budget | seconds | 24 hours |
//! | Finds | a property broken on ordinary markdown | a property broken on input nobody would write |
//!
//! The reason both exist is that a coverage-guided fuzzer starting from bytes
//! spends a long time discovering that `**` opens something, while a generator
//! that only emits `**`-shaped strings will never find the input that breaks
//! the trim. Neither distribution subsumes the other.
//!
//! # The properties
//!
//! All of them are restatements of RUST-REWRITE-PLAN.md §3's three rules and
//! §9's M1 exit gate, over generated rather than collected input:
//!
//! 1. **Nothing panics.** The exit gate, and the reason `panic = "abort"` in
//!    the release profile makes it non-negotiable — a panic in the shipped
//!    build is a crash, not an exception. Every property below implies this,
//!    because a panic fails the test whatever else it was checking.
//! 2. **`generator(tokenize(s)) == s`.** §3 rule 1 as an equality.
//! 3. **The tokens tile the input**, per level, with every child span inside
//!    its parent's (M1.md §4 C3).
//! 4. **Every span lands on `char` boundaries.** This is the one D6 calls
//!    platform-specific, and it is the reason the generator below is what it
//!    is rather than `".*"`.
//! 5. **The three consumers are total too** — `tokens_to_plain_text`, the
//!    `highlights` post-pass and `marker_state`, over arbitrary offsets
//!    including ones that are not `char` boundaries.
//!
//! # Why `Span::of` panicking is not a finding
//!
//! [`mt_inline::Span::of`] panics by design on an out-of-bounds or
//! non-boundary span, and `Span::get` is the total version. A property that
//! built spans *by hand* and called `of` would find that instantly and it would
//! not be a bug — it would be the documented contract. So nothing here
//! constructs a span: every span under test comes out of `tokenize`, and what
//! is asserted is that **the tokenizer never produces one `get` rejects**.
//! Generate inputs, not tokens.
//!
//! # Shrinking, and the one place it does not work
//!
//! proptest shrinks a failing case by `catch_unwind`, which needs unwinding.
//! `cargo test` uses the `test` profile (inheriting `dev`), so shrinking works;
//! `cargo test --release` inherits `panic = "abort"` and a failure there aborts
//! with the unshrunk input. That is a fair trade — the release run exists for
//! `round_trip.rs`'s large-corpus gate, where there is nothing to shrink.

use std::collections::HashSet;

use mt_inline::{
    Cursor, Highlight, Span, Token, TokenizerOptions, generator, marker_state, tokenize, tokenizer,
    tokens_to_plain_text,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;

// ---------------------------------------------------------------------------
// The generators
// ---------------------------------------------------------------------------

/// Characters chosen because a byte-oriented port gets them wrong.
///
/// D6 asks for "multi-byte and lone-surrogate-adjacent input". Rust's `str`
/// cannot hold a lone surrogate — that is the point of `char` — so what
/// "surrogate-adjacent" means here is the three places the surrogate block
/// makes JavaScript and Rust disagree about what one character is:
///
/// - **U+D7FF and U+E000**, the scalar values either side of the block. A
///   transcription that decoded muya's `\uD800`-`\uDFFF` escapes by hand can be
///   off by one at exactly these two, and `emphasis.rs`'s non-BMP punctuation
///   table is 40 such escapes.
/// - **astral characters**, which are *one* `char` here and *two* UTF-16 code
///   units there. Every one of them is a place where a UTF-16 offset and a byte
///   offset diverge by more than the usual amount, which is D1's whole subject
///   and the differential harness's whole conversion problem.
/// - **grapheme clusters that are several characters** — ZWJ sequences,
///   variation selectors, skin-tone modifiers, combining marks. `bench/corpus/
///   emoji.md` exists for these; the tokenizer must not split one, and a rule
///   that advanced by byte rather than by `char` would.
const AWKWARD: &[char] = &[
    // Either side of the surrogate block.
    '\u{d7ff}',
    '\u{e000}',
    // Astral: the first, the last, an emoji, a digit, a musical symbol.
    '\u{10000}',
    '\u{10ffff}',
    '😀',
    '\u{1d7f9}',
    '𝄞',
    // Grapheme-cluster machinery.
    '\u{200d}',  // ZWJ
    '\u{fe0f}',  // VARIATION SELECTOR-16
    '\u{1f3ff}', // EMOJI MODIFIER FITZPATRICK TYPE-6
    '\u{301}',   // COMBINING ACUTE ACCENT
    // M1.md §4 C2's whitespace disagreements, both directions.
    '\u{85}',
    '\u{feff}',
    '\u{2028}',
    '\u{2029}',
    '\u{a0}',
    '\u{3000}',
    // Two- and three-byte characters from the corpus's own scripts.
    'é',
    'я',
    'α',
    '中',
    '한',
    'ع',
    '\u{5d0}',
    // Replacement character and the BMP's last two scalars.
    '\u{fffd}',
    '\u{fffe}',
    '\u{ffff}',
];

/// Fragments that reach a handler.
///
/// A generator over `char` alone reaches `try_strong_em` roughly never: it has
/// to produce two `*`s, then content, then two more. These are the markers of
/// all sixteen handlers plus the few multi-character openers, so a random
/// sequence of them is a syntactically dense input rather than a syntactically
/// empty one.
const FRAGMENTS: &[&str] = &[
    // Emphasis and chunks.
    "**",
    "__",
    "*",
    "_",
    "~~",
    "~",
    "^",
    "`",
    "``",
    "$",
    "$$",
    // Links, images, references, footnotes.
    //
    // The bracket pieces alone are not enough, and
    // `the_generator_reaches_every_handler_that_default_options_can_reach`
    // is what said so: assembling `[`, text, `]`, `(`, url, `)` **in that
    // order** out of a uniform pick happens too rarely to matter, so 20,000
    // samples produced not one `link` or `image`. The composites below make
    // the two handlers reachable; the pieces stay, so the generator still
    // explores the malformed neighbourhood around them.
    "[",
    "]",
    "(",
    ")",
    "![",
    "][",
    "[^",
    "]: ",
    "<",
    ">",
    "](",
    "[a](b)",
    "![a](b)",
    "[a](/u \"t\")",
    "![alt](/s \"t\")",
    "[a][b]",
    "[a]: /u \"t\"",
    // Escapes and entities.
    "\\",
    "&amp;",
    "&lt;",
    "&NotACharacter;",
    "&#38;",
    // The tail header, which unlike the begin rules can appear anywhere.
    " #",
    " ###",
    "***",
    "---",
    // Autolinks, both forms and all three kinds. The angle-bracket form is a
    // composite for the same reason the link forms are: `<`, a scheme, a host
    // and `>` in that order is not something a uniform pick assembles.
    "https://",
    "http://",
    "www.",
    "@",
    ".com",
    "example",
    ":",
    "<https://example.com>",
    "<user@example.com>",
    "<http://x.y/a%20b>",
    // HTML.
    "<div>",
    "</div>",
    "<span id=\"i\">",
    "</span>",
    "<!--",
    "-->",
    "<br>",
    "<td>",
    "<table>",
    // Whitespace and breaks.
    " ",
    "  ",
    "\n",
    "  \n",
    "\t",
    // Ordinary text.
    "a",
    "z",
    "0",
    "9",
    "-",
    "+",
    "!",
    "?",
    "'",
    "\"",
    "smile",
    "100",
];

/// Openers that only match at **offset 0** of a top-level tokenize.
///
/// `consumeBeginRules` (`lexer.ts:60`) runs once, before the loop, and only
/// when `state.pos == 0`. So a generator that picks pieces uniformly reaches
/// these five rules at roughly `1/|FRAGMENTS|` — which is how
/// `the_generator_reaches_every_handler_that_default_options_can_reach` came to
/// report `multiple_math` and `reference_definition` missing from 4,000
/// samples. Giving offset 0 its own distribution is not a thumb on the scale;
/// it is the shape of the rule.
const BEGIN_FRAGMENTS: &[&str] = &[
    "# ",
    "## ",
    "###### ",
    "***",
    "---",
    "___",
    "```",
    "````rust title=\"x\"",
    "$$",
    "[label]: /url \"title\"",
    "  [a]: <href> 'title'  ",
    "[a\\\\]: /u (t)",
];

/// Fragment-assembled input: dense in constructs, and salted with [`AWKWARD`].
///
/// The `1..=3` repeat on a fragment is what produces the marker runs that
/// `isLengthEven` and the flanking rules turn on — `***`, `\\\`, ` ``` ` — which
/// a uniform pick would produce only by accident.
fn markdownish() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        // Weighted toward fragments: the point is to reach handlers.
        8 => (0usize..FRAGMENTS.len(), 1usize..=3usize)
            .prop_map(|(i, n)| FRAGMENTS[i].repeat(n)),
        3 => (0usize..AWKWARD.len()).prop_map(|i| AWKWARD[i].to_string()),
        1 => any::<char>().prop_map(|c| c.to_string()),
    ];
    let body = proptest::collection::vec(piece, 0..24).prop_map(|pieces| pieces.concat());
    let begin = prop_oneof![
        // Most inputs are not a heading. A quarter of them are one of these,
        // which is enough to exercise five rules that would otherwise be
        // reached by accident.
        3 => Just(String::new()),
        1 => (0usize..BEGIN_FRAGMENTS.len()).prop_map(|i| BEGIN_FRAGMENTS[i].to_string()),
    ];
    (begin, body).prop_map(|(begin, body)| format!("{begin}{body}"))
}

/// Unstructured `char` soup, weighted toward [`AWKWARD`].
///
/// The other half of the distribution: `markdownish` can only produce what its
/// fragment table contains, and a property that only ever sees well-formed
/// markers is not testing a *lexer*.
fn unicode_soup() -> impl Strategy<Value = String> {
    let character = prop_oneof![
        2 => (0usize..AWKWARD.len()).prop_map(|i| AWKWARD[i]),
        1 => any::<char>(),
    ];
    proptest::collection::vec(character, 0..48).prop_map(|chars| chars.into_iter().collect())
}

// ---------------------------------------------------------------------------
// The property bodies
// ---------------------------------------------------------------------------

/// Every token span is one `Span::get` accepts.
///
/// `get` is the total version of `of`: `None` means out of bounds **or** off a
/// `char` boundary, which is D6's platform-specific risk stated as a check.
fn spans_are_on_char_boundaries(src: &str, tokens: &[Token]) -> Result<(), TestCaseError> {
    for token in tokens {
        prop_assert!(
            token.raw.get(src).is_some(),
            "a `{}` token's raw {:?} is out of bounds or off a char boundary",
            token.type_str(),
            token.raw
        );
        prop_assert!(token.range.get(src).is_some());
        if let Some(children) = token.children() {
            spans_are_on_char_boundaries(src, children)?;
        }
    }
    Ok(())
}

/// §3 rule 1 per level, plus M1.md §4 C3's cross-level half.
///
/// The same three claims `round_trip.rs::assert_tiles` makes over real files,
/// restated so a failure is a proptest counterexample rather than a panic in a
/// helper: the tokens of a level are contiguous and cover it exactly, each
/// child sits inside its parent's range, and one token's children tile their
/// own extent.
fn tiles(tokens: &[Token], start: usize, end: usize) -> Result<(), TestCaseError> {
    let mut cursor = start;
    for token in tokens {
        prop_assert_eq!(
            token.raw.start,
            cursor,
            "a `{}` token starts at {} but the previous one ended at {}",
            token.type_str(),
            token.raw.start,
            cursor
        );
        prop_assert_eq!(token.range, token.raw);
        if let Some(children) = token.children() {
            for child in children {
                prop_assert!(
                    token.range.contains_span(child.range),
                    "a `{}` child escapes its `{}` parent",
                    child.type_str(),
                    token.type_str()
                );
            }
            if let (Some(first), Some(last)) = (children.first(), children.last()) {
                tiles(children, first.raw.start, last.raw.end)?;
            }
        }
        cursor = token.raw.end;
    }
    prop_assert_eq!(
        cursor,
        end,
        "the tokens stop at {} but the input ends at {}",
        cursor,
        end
    );
    Ok(())
}

/// Everything the exit gate asks of one input.
fn check(src: &str) -> Result<(), TestCaseError> {
    let tokens = tokenize(src);
    prop_assert_eq!(
        generator(src, &tokens),
        src,
        "generator(tokenize(src)) != src"
    );
    spans_are_on_char_boundaries(src, &tokens)?;
    tiles(&tokens, 0, src.len())?;

    // The consumers, called and not asserted about — deliberately, and it is
    // worth saying why rather than reaching for an assertion that looks like
    // one.
    //
    // `tokens_to_plain_text` drops markers and resolves entities, so it has no
    // equality against `src` to check; `generator_rebuilding_wrappers` rebuilds
    // a wrapper from its children rather than echoing a stale `raw` (muya
    // #2063), so `== src` is not its contract either. What both *do* have to be
    // is **total**: each is a walk that slices spans of its own, and each is a
    // place a wrong span becomes a panic. That is the exit gate, and calling
    // them is the whole check.
    //
    // (An assertion like `plain.len() <= src.len()` would be a plausible-
    // looking third property. It is not asserted because it has not been
    // proved, and a property test that encodes a guess fails on a correct
    // input.)
    let _ = tokens_to_plain_text(src, &tokens);
    let _ = mt_inline::generator_rebuilding_wrappers(src, &tokens);
    Ok(())
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

proptest! {
    // 512 rather than the default 256 because these cases are microseconds
    // each; the whole file is under two seconds in a debug profile, which is
    // what keeps it in the every-commit job on three platforms rather than
    // becoming something that gets `#[ignore]`d.
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// The exit gate over syntactically dense input.
    #[test]
    fn markdownish_input_tiles_and_round_trips(src in markdownish()) {
        check(&src)?;
    }

    /// The exit gate over `char` soup — where the boundary bugs live.
    #[test]
    fn unicode_soup_tiles_and_round_trips(src in unicode_soup()) {
        check(&src)?;
    }

    /// And over genuinely unstructured input, which is the distribution
    /// `fuzz/` starts from and the one neither generator above can reach.
    #[test]
    fn arbitrary_strings_tile_and_round_trip(src in ".*") {
        check(&src)?;
    }

    /// The `highlights` post-pass (M1.md §5 D7) over arbitrary ranges,
    /// including inverted and out-of-bounds ones.
    ///
    /// `union` is pure arithmetic on two ranges, so nothing here can be off a
    /// `char` boundary — but the post-pass *walks the tree*, and a highlight
    /// list is the only thing that makes it run at all (`lexer.ts:890`). What
    /// is checked is that adding highlights changes no span: the post-pass
    /// annotates, it does not re-tokenize.
    #[test]
    fn the_highlight_post_pass_changes_no_span(
        src in markdownish(),
        ranges in proptest::collection::vec((0usize..64, 0usize..64, any::<Option<bool>>()), 0..6),
    ) {
        let highlights: Vec<Highlight> = ranges
            .into_iter()
            .map(|(a, b, active)| Highlight { span: Span::new(a, b), active })
            .collect();
        let plain = tokenize(&src);
        let lit = tokenizer(
            &src,
            &TokenizerOptions::muya_default().with_highlights(highlights),
        );
        let round_tripped = generator(&src, &lit);
        prop_assert_eq!(round_tripped.as_str(), src.as_str());
        tiles(&lit, 0, src.len())?;
        prop_assert_eq!(
            plain.iter().map(|t| t.raw).collect::<Vec<_>>(),
            lit.iter().map(|t| t.raw).collect::<Vec<_>>(),
            "the post-pass moved a token"
        );
    }

    /// §3.1's marker-reveal predicate (M1.md §5 D8) over arbitrary cursors.
    ///
    /// `Cursor` takes `usize` offsets and does not require them to be `char`
    /// boundaries or in bounds — the predicate is pure comparison, so it must
    /// be total for any pair. A renderer will pass it a caret from a different
    /// document sooner or later.
    #[test]
    fn marker_state_is_total_over_any_cursor(
        src in markdownish(),
        anchor in 0usize..128,
        focus in 0usize..128,
    ) {
        let tokens = tokenize(&src);
        for token in &tokens {
            let _ = marker_state(token, None);
            let _ = marker_state(token, Some(Cursor { anchor, focus }));
            let _ = marker_state(token, Some(Cursor { anchor: focus, focus: anchor }));
            let _ = marker_state(token, Some(Cursor::collapsed(anchor)));
        }
    }
}

// ---------------------------------------------------------------------------
// The soak
// ---------------------------------------------------------------------------

/// The M1 exit gate's *"fuzzer runs 24 h without a panic"*, on **this**
/// platform.
///
/// # Why this exists beside `fuzz/`
///
/// D6 puts the 24-hour libFuzzer soak on Linux, because `cargo-fuzz` on Windows
/// MSVC is thin — it needs a nightly toolchain, `-Zsanitizer` and a bundled C++
/// libFuzzer, and `rust-toolchain.toml` pins stable for good reasons. That is
/// the right call and `fuzz/` implements it.
///
/// It leaves a hole, though, and §9 is where it matters: **Windows ships
/// first.** A gate met only on Linux is a gate met on the platform whose
/// failures block least. So this is the same properties, the same generators,
/// driven for a wall-clock duration on whatever platform runs it — coverage-
/// blind where libFuzzer is coverage-guided, and structure-*aware* where
/// libFuzzer starts from bytes. Neither subsumes the other; running both is
/// cheaper than arguing about which is better.
///
/// # Running it
///
/// `#[ignore]` because it runs until told to stop, and a default `cargo test`
/// must stay in seconds.
///
/// ```sh
/// # one hour, with the four unreachable-branch assertions live
/// MT_SOAK_SECONDS=3600 RUSTFLAGS="-C debug-assertions=yes" \
///   cargo test -p mt-inline --release --test properties -- --ignored soak --nocapture
/// ```
///
/// **`-C debug-assertions=yes` is not optional in a release soak.** M1.md §6
/// records four branches this crate proves unreachable and reproduces anyway;
/// S7 turned each proof into a `debug_assert!` so that a soak can falsify it.
/// A plain `--release` run compiles all four out and checks strictly less than
/// a debug run does — it would be faster and worth less. `fuzz/Cargo.toml`
/// sets the same flag in its profile for the same reason.
///
/// Without `MT_SOAK_SECONDS` it runs for 60 seconds, so that `-- --ignored`
/// with no configuration still does something finite and useful.
///
/// # It also reports its slowest inputs, and getting that right took two tries
///
/// libFuzzer has `-report_slow_units`; this is the same idea, and it is what
/// makes a soak produce a *benchmark row* rather than only a pass/fail. S5's
/// lesson was that one canary is not enough — `[` × 4000 was blind to the
/// greedy-quantifier cost S5 itself introduced — and a search that *maximises*
/// time per byte is the general version of that.
///
/// The first version timed one call per input and ranked by nanoseconds per
/// byte, with a 10-byte floor. Over 14.7 million cases it confidently reported
/// eight "slowest" inputs of 10 to 32 bytes at 250,000–939,000 ns/B. Every one
/// of them worked out to **10–14 ms of wall clock regardless of length** —
/// a fixed cost, which is a scheduler quantum and not anything the tokenizer
/// did. At ~2.5 µs/byte a 15-byte input is ~37 µs of work; the number reported
/// was 380× that.
///
/// So the ranking pass is only a candidate filter now, and what is printed is
/// the **minimum over [`RETIMES`] re-runs** of inputs of at least
/// [`MIN_TIMED_BYTES`]. Minimum rather than mean, because preemption and page
/// faults can only make a measurement longer: the smallest observation is the
/// least contaminated one. A genuinely slow input stays slow across all of
/// them; a hiccup collapses.
///
/// Worth writing down rather than quietly fixing, because the first version
/// would have put a scheduler artefact into `benches/tokenizer.rs` as a cost
/// row — and a benchmark row nobody can reproduce is worse than a missing one.
#[test]
#[ignore = "runs for MT_SOAK_SECONDS (default 60); the M1 exit gate runs it long"]
fn soak() {
    use std::time::{Duration, Instant};

    use proptest::test_runner::{Config, TestRunner};

    let seconds: u64 = std::env::var("MT_SOAK_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let deadline = Instant::now() + Duration::from_secs(seconds);

    // A *random* seed, not the deterministic one the two generator tests below
    // use: a soak that replays the same sequence every night is a very long way
    // to run one test. `Config::default()` seeds randomly, and a failure prints
    // the offending input itself — which is what reproducibility needs here,
    // since there is nothing to shrink from a single failing case.
    let mut runner = TestRunner::new(Config::default());

    let strategies: [BoxedStrategy<String>; 3] = [
        markdownish().boxed(),
        unicode_soup().boxed(),
        any::<String>().boxed(),
    ];

    let mut cases = 0u64;
    let mut bytes = 0u64;
    let mut longest = String::new();
    // The soak's other output. libFuzzer has `-report_slow_units`, and it is
    // what turns a soak into a *benchmark* row rather than only a pass/fail:
    // an input that is slow per byte is one the three-term cost model
    // (`benches/tokenizer.rs`) either explains or does not, and the second case
    // is a finding. This is that, for the platforms libFuzzer does not run on.
    //
    // Per **byte**, not per input, or the answer is always "the longest one".
    let mut candidates: Vec<(f64, String)> = Vec::new();
    while Instant::now() < deadline {
        for _ in 0..64 {
            for strategy in &strategies {
                let src = strategy
                    .new_tree(&mut runner)
                    .expect("the strategy produces a value")
                    .current();
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

    // **Re-time the finalists, and take the minimum of many runs.**
    //
    // The first pass is one `Instant::now()` pair around one call, and at these
    // input sizes that measures the *operating system*. The first version of
    // this reported eight "slowest" inputs of 10-32 bytes at 250,000 to 939,000
    // ns/B — which is 10-14 ms of wall clock each, a fixed cost independent of
    // length, i.e. a scheduler quantum rather than anything the tokenizer did.
    // At ~2.5 µs/byte a 15-byte input should take ~37 µs; the reported figure
    // was 380× that. Every entry in that list was a preemption.
    //
    // So the ranking above is only a *candidate filter*, and the number
    // reported is the **minimum over `RETIMES` runs**. Minimum, not mean:
    // preemption and page faults can only ever make a measurement longer, so
    // the smallest observation is the one least contaminated by them. An input
    // that is genuinely slow stays slow across all of them; a hiccup collapses.
    let mut slowest: Vec<(f64, String)> = candidates
        .into_iter()
        .map(|(_, src)| {
            let mut best = f64::INFINITY;
            for _ in 0..RETIMES {
                let started = Instant::now();
                let _ = check(&src);
                let ns = started.elapsed().as_nanos() as f64;
                best = best.min(ns);
            }
            (best / src.len() as f64, src)
        })
        .collect();
    slowest.sort_by(|a, b| b.0.total_cmp(&a.0));
    slowest.truncate(8);

    println!(
        "soak: {cases} cases, {bytes} bytes, longest input {} bytes, over {seconds}s",
        longest.len()
    );
    println!("slowest per byte (min of {RETIMES} re-runs, inputs >= {MIN_TIMED_BYTES} B):");
    for (per_byte, src) in &slowest {
        let shown: String = src.chars().take(90).collect();
        println!("  {per_byte:9.1} ns/B  {:5} B  {shown:?}", src.len());
    }
    assert!(cases > 0, "the soak ran no cases");
}

/// Below this, a single-call timing is dominated by the clock and the
/// scheduler rather than by the tokenizer. At ~2.5 µs/byte in release, 64 bytes
/// is ~160 µs of work, which is enough to survive re-timing.
const MIN_TIMED_BYTES: usize = 64;

/// How many times to re-run a slow-input candidate before believing its number.
const RETIMES: u32 = 32;

// ---------------------------------------------------------------------------
// The generator's own negative control
// ---------------------------------------------------------------------------

/// **A generator that quietly degenerates to ASCII makes every property above
/// vacuous**, and nothing in a green run would say so.
///
/// This is S6's lesson applied to proptest: 5586 agreements meant nothing until
/// the register's own inputs were shown to disagree through the same code path.
/// Here the equivalent is showing that the distribution actually contains what
/// D6 asked for — multi-byte characters, astral characters, and the two scalars
/// either side of the surrogate block — before believing that a green run says
/// anything about UTF-8 boundary slicing.
///
/// Deterministic rather than random: it drives the strategies through a fixed
/// RNG seed, so a change that narrows the distribution fails here every time
/// rather than one run in twenty.
#[test]
fn the_generator_produces_multi_byte_and_surrogate_adjacent_input() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );

    let mut characters: HashSet<char> = HashSet::new();
    let mut multi_byte = 0usize;
    let mut astral = 0usize;
    let mut samples = 0usize;

    let strategies: [BoxedStrategy<String>; 2] = [markdownish().boxed(), unicode_soup().boxed()];
    for strategy in &strategies {
        for _ in 0..2000 {
            let value = strategy
                .new_tree(&mut runner)
                .expect("the strategy produces a value")
                .current();
            samples += 1;
            for character in value.chars() {
                characters.insert(character);
                if character.len_utf8() > 1 {
                    multi_byte += 1;
                }
                if character as u32 > 0xFFFF {
                    astral += 1;
                }
            }
        }
    }

    assert!(
        multi_byte > samples,
        "the distribution is ASCII soup: {multi_byte} multi-byte characters in {samples} samples"
    );
    assert!(
        astral > 0,
        "no astral character was ever generated; UTF-16 offsets and byte offsets never diverge in this distribution"
    );
    for wanted in ['\u{d7ff}', '\u{e000}'] {
        assert!(
            characters.contains(&wanted),
            "U+{:04X} — a scalar adjacent to the surrogate block — is never generated",
            wanted as u32
        );
    }
    for wanted in ['\u{200d}', '\u{fe0f}', '\u{feff}', '\u{85}'] {
        assert!(
            characters.contains(&wanted),
            "U+{:04X} is never generated, so a grapheme or whitespace trap goes unprobed",
            wanted as u32
        );
    }
}

/// The fragment table has to reach handlers, or `markdownish` is an expensive
/// way to generate text tokens.
///
/// The same argument as above one level up: a distribution that never produces
/// a `strong` is not testing `try_strong_em`, however many cases it runs.
#[test]
fn the_generator_reaches_every_handler_that_default_options_can_reach() {
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let strategy = markdownish();

    fn collect(tokens: &[Token], seen: &mut HashSet<&'static str>) {
        for token in tokens {
            seen.insert(token.type_str());
            if let Some(children) = token.children() {
                collect(children, seen);
            }
        }
    }

    // 4,000 rather than a round 20,000: this test is a check on the
    // *distribution*, not a search, and a construct that needs more than a few
    // thousand samples to appear once is not being exercised by a 512-case
    // property run either. Keeping it small is what keeps this file inside the
    // every-commit budget on three platforms.
    let mut seen: HashSet<&'static str> = HashSet::new();
    for _ in 0..4_000 {
        let value = strategy
            .new_tree(&mut runner)
            .expect("the strategy produces a value")
            .current();
        collect(&tokenize(&value), &mut seen);
    }

    // Three of the 26 types are unreachable under muya's default options and
    // are not this generator's fault: `reference_link` and `reference_image`
    // are gated on a populated label map, `footnote_identifier` on
    // `options.footnote`, which defaults to false. `tokenize` uses the
    // defaults, so they cannot appear. The differential harness covers those
    // three — see `xtask/src/tokens.rs::token_type_probes`.
    for wanted in [
        "auto_link",
        "auto_link_extension",
        "backlash",
        "code_fence",
        "del",
        "em",
        "emoji",
        "hard_line_break",
        "header",
        "hr",
        "html_escape",
        "html_tag",
        "image",
        "inline_code",
        "inline_math",
        "link",
        "multiple_math",
        "reference_definition",
        "soft_line_break",
        "strong",
        "super_sub_script",
        "tail_header",
        "text",
    ] {
        assert!(
            seen.contains(wanted),
            "the generator never produced a `{wanted}` token, so the properties never \
             exercised its handler. Seen: {seen:?}"
        );
    }
}
