//! The tiling invariant and the round-trip property, over `bench/corpus/`.
//!
//! RUST-REWRITE-PLAN.md §3 rule 1:
//!
//! > **Ranges are byte offsets, and they must tile the input exactly.** Assert
//! > in debug builds that concatenating every token's `raw` reproduces the
//! > source byte-for-byte. This single invariant catches most porting errors
//! > immediately.
//!
//! The tokenizer asserts this internally on every level it produces
//! (`lexer::check_tiling`, under `cfg!(debug_assertions)`). This file is the
//! same property stated from the outside, over real files, and it exists for
//! three reasons the internal assertion cannot cover:
//!
//! 1. **It is the S1 gate.** M1.md §6: *"Tiling invariant holds on plain-text
//!    and line-break corpus inputs."*
//! 2. **It is S6's round-trip property, pulled forward.** M1.md §6 says to
//!    build `generator(tokenize(s)) == s` as soon as S1 lands rather than
//!    waiting, *"because it catches tiling errors in every stage after it"*.
//!    Every handler S2–S5 adds is covered by this file the moment it is
//!    written, without anyone remembering to add a test.
//! 3. **It survives the internal assertion being wrong.** An invariant checked
//!    only by the code that maintains it is checked by its own author. This
//!    goes through the public API — [`mt_inline::generator`] and `Token::raw`
//!    — so a bug in `check_tiling` does not hide a bug in the tokenizer.
//!
//! # Why an integration test and not a unit test
//!
//! It reads files, and §1 says `mt-inline` does no I/O — machine-checked by
//! `cargo xtask deps`, which scans `src/` for `std::fs`. `tests/` is outside
//! that scan, which is exactly the escape hatch `deps.rs` names: *"if a test
//! genuinely needs a fixture, move it to an integration test"*. The crate
//! stays pure; the fixture-reading lives here.
//!
//! # Scope, and what is deferred to S7
//!
//! The M1 **exit** gate is "tiling holds on all corpus files". This runs every
//! file up to [`DEBUG_PROFILE_SIZE_LIMIT`]; the three larger generated files
//! are behind `#[ignore]`, with the reasoning at
//! [`the_large_corpus_files_also_tile`]. Nothing is silently skipped — the
//! covered run prints what it covered and the ignored test names what it does
//! not.

use std::path::PathBuf;

use mt_inline::{Token, generator, tokenize};

/// `bench/corpus/`, resolved from this crate's manifest directory rather than
/// the working directory, so the test behaves the same however `cargo test` is
/// invoked.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bench")
        .join("corpus")
}

/// Every `.md` in the corpus, as `(name, contents)`, sorted by name so a
/// failure reports the same file first on every platform.
fn corpus_files() -> Vec<(String, String)> {
    let dir = corpus_dir();
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter(|path| path.file_name().is_some_and(|name| name != "README.md"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("a file")
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            (name, text)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Above this, a corpus file runs only in [`the_large_corpus_files_also_tile`].
///
/// The tokenizer runs every unmatched rule at every unmatched character —
/// what a faithful port of `lexer.ts` does, and what §3 says to keep until the
/// suite is green enough to guard a hand-written scanner. Measured on the
/// development machine at S1, in a debug profile: **~1 s for `250kb.md`,
/// ~21 s for `1mb.md` and `5mb.md` together.**
///
/// 21 s is not fatal on its own. What makes it the wrong default is that it
/// only grows: S1 runs five of the sixteen handlers, and each of S2–S5 adds
/// more regexes to try at every unmatched position. The same two files are
/// plausibly a minute or more by S5, on every `cargo test` on three platforms,
/// for input that is `250kb.md`'s generated prose made longer — scale, not new
/// structure. **Re-measure at each stage** rather than trusting these numbers.
///
/// 256 KiB is chosen to keep `250kb.md` in: it is the largest file whose
/// content differs *structurally* from the two above it.
const DEBUG_PROFILE_SIZE_LIMIT: usize = 256 * 1024;

/// Assert that a token list tiles `[start, end)` of `src`, recursively.
///
/// The public-API restatement of `lexer::check_tiling`, and it is deliberately
/// weaker in one place. M1.md §4 C3 says the invariant holds *per level*: a
/// level's tokens reproduce that level's input, and a child level's
/// concatenation reproduces its parent's **capture group**. From outside the
/// crate the capture group is not observable — the token does not carry it —
/// so what is checked here is what an outside observer can check:
///
/// - the tokens of a level are contiguous and cover it exactly;
/// - each child span sits inside its parent's `range`;
/// - the children of one token are themselves contiguous.
///
/// That is enough to catch a gap, an overlap or an escaped child, which is
/// every tiling failure mode the internal assertion catches too.
fn assert_tiles(src: &str, tokens: &[Token], start: usize, end: usize, what: &str) {
    let mut cursor = start;

    for token in tokens {
        assert_eq!(
            token.raw.start,
            cursor,
            "{what}: a `{}` token starts at {} but the previous one ended at {cursor}",
            token.type_str(),
            token.raw.start
        );
        assert_eq!(
            token.range,
            token.raw,
            "{what}: a `{}` token's range is not its raw",
            token.type_str()
        );
        assert!(
            token.raw.get(src).is_some(),
            "{what}: a `{}` token's span {:?} is out of bounds or off a `char` boundary",
            token.type_str(),
            token.raw
        );

        if let Some(children) = token.children() {
            for child in children {
                assert!(
                    token.range.contains_span(child.range),
                    "{what}: a `{}` child escapes its `{}` parent",
                    child.type_str(),
                    token.type_str()
                );
            }
            if let (Some(first), Some(last)) = (children.first(), children.last()) {
                assert_tiles(src, children, first.raw.start, last.raw.end, what);
            }
        }

        cursor = token.raw.end;
    }

    assert_eq!(
        cursor, end,
        "{what}: the tokens stop at {cursor} but the input ends at {end}"
    );
}

/// One file, both statements of the property.
fn check(name: &str, src: &str) {
    let tokens = tokenize(src);
    assert_eq!(
        generator(src, &tokens),
        src,
        "{name}: generator(tokenize(src)) != src"
    );
    assert_tiles(src, &tokens, 0, src.len(), name);
}

/// **The S1 gate.**
///
/// M1.md §6 words it as "plain-text and line-break corpus inputs", which is
/// what S1's handlers actually reach — but the property is not rule-specific,
/// so there is no reason to narrow the input set to match the implemented
/// rules. Everything else in these files tokenizes as text today and as its
/// own token from S2 onward, and this assertion is unchanged either way. That
/// is the point of building it now.
#[test]
fn every_corpus_file_tiles_and_round_trips() {
    let mut covered = 0;
    let mut deferred = Vec::new();

    for (name, src) in corpus_files() {
        if src.len() > DEBUG_PROFILE_SIZE_LIMIT {
            deferred.push(name);
            continue;
        }
        check(&name, &src);
        covered += 1;
    }

    assert!(
        covered >= 8,
        "the corpus lost files: only {covered} checked"
    );
    // Not silent: `deps.rs`'s own comment about crude checks applies here too
    // — a bounded run that does not say what it bounded reads as full
    // coverage.
    println!("tiling: {covered} corpus files checked, deferred to --ignored: {deferred:?}");
}

/// The other three corpus files — `250kb.md`'s two larger siblings and
/// whatever else grows past the limit.
///
/// `#[ignore]` for the reason given at [`DEBUG_PROFILE_SIZE_LIMIT`], and
/// because the M1 exit gate that needs them (S7: *"tiling holds on all corpus
/// files"*) runs alongside the 24-hour soak rather than on every
/// `cargo test`. Run it with:
///
/// ```sh
/// cargo test -p mt-inline --release --test round_trip -- --ignored
/// ```
///
/// The `--release` matters: `check_tiling` is compiled out there, so this
/// checks the property through the public API alone, which is the honest
/// version of it.
#[test]
#[ignore = "minutes in a debug profile; the M1 exit gate (S7) runs it in release"]
fn the_large_corpus_files_also_tile() {
    for (name, src) in corpus_files() {
        if src.len() <= DEBUG_PROFILE_SIZE_LIMIT {
            continue;
        }
        check(&name, &src);
    }
}

/// The corpus is committed, so a file silently disappearing would quietly
/// shrink the gate. Named files, not a count.
#[test]
fn the_corpus_still_contains_the_files_this_gate_names() {
    let names: Vec<String> = corpus_files().into_iter().map(|(name, _)| name).collect();
    for expected in [
        "empty.md",
        "10kb.md",
        "cjk.md",
        "emoji.md",
        "rtl.md",
        "50-code-fences.md",
        "100-inline-math.md",
        "20-tables.md",
        "250kb.md",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "bench/corpus/{expected} is missing; found {names:?}"
        );
    }
}

/// The two corpus files M1.md §6 S1 singles out, checked one property at a
/// time so a failure says which half broke.
///
/// `cjk.md` carries the CJK flanking cases and a BOM-adjacent unspaced line;
/// `emoji.md` carries ZWJ sequences, skin-tone modifiers and regional-
/// indicator flags. Both are the reason D1 chose byte offsets and the reason
/// M1.md §4 C2 insists the whitespace class be written out. If the tokenizer
/// ever advances by byte instead of by `char`, this is where it shows.
#[test]
fn the_multibyte_corpus_files_never_split_a_character() {
    for name in ["cjk.md", "emoji.md", "rtl.md"] {
        let path = corpus_dir().join(name);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        for token in tokenize(&src) {
            assert!(
                token.raw.get(&src).is_some(),
                "{name}: a `{}` token's span {:?} is not on `char` boundaries",
                token.type_str(),
                token.raw
            );
        }
        check(name, &src);
    }
}
