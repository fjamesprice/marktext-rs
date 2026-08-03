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
//! # Scope — and S7 ran the deferred half
//!
//! The M1 **exit** gate is "tiling holds on all corpus files". Every
//! `cargo test` runs the files up to [`DEBUG_PROFILE_SIZE_LIMIT`]; the two
//! larger generated ones are behind `#[ignore]`, with the reasoning at
//! [`the_large_corpus_files_also_tile`]. Nothing is silently skipped — the
//! covered run prints what it covered and the ignored test names what it does
//! not.
//!
//! **S7 ran the ignored half, in release, and the gate is met over all eleven
//! corpus files:** 22.4 s for this whole file, `1mb.md` and `5mb.md` included,
//! alongside the 1324 spec fixtures and the eleven round-trip fixtures. The
//! `--release` matters for a reason other than speed — `check_tiling` is
//! compiled out there, so the property goes through the public API alone.
//! `.github/workflows/soak.yml` runs it nightly on Windows and Linux so it
//! stays run rather than having been run once.
//!
//! The gate row said *"all **22** corpus files"*. It is **eleven**; the 22 is
//! `cargo xtask diff`'s file count, which collects from `bench/corpus/` **and**
//! `spec/fixtures/marktext-round-trip/`, both of which happen to hold exactly
//! eleven. See M1.md's "Correction to S7's gate row".
//!
//! # S6 widened it to `spec/fixtures/`, and the deferral's reason had expired
//!
//! This header used to record that `spec/fixtures/` was deliberately left out:
//!
//! > §7 puts it under "free smoke test from S2 onward" and it gates M2, not
//! > M1; adding it before the handlers exist would only assert that 1324
//! > fixtures tokenize to one text token each.
//!
//! That was true while handlers were missing and stopped being true at S5,
//! when the sixteenth landed. So S6's gate row widens the round-trip property
//! to the 1324 CommonMark and GFM examples and to the eleven
//! `spec/fixtures/marktext-round-trip/` files.
//!
//! **What that is and is not.** §7 calls it a *smoke test*, and the wording
//! matters: these fixtures are whole multi-line markdown **documents**, and
//! `mt-inline` tokenizes one leaf block's text. Feeding a document to it is not
//! parsing the document — a fenced code block's contents are inline-tokenized
//! like anything else, a list marker is just text, and nothing here claims
//! otherwise. What it does check is exactly what §3 rule 1 asks for on the
//! widest input set the repository has: **every byte is accounted for by some
//! token, no span lands off a `char` boundary, and nothing panics.** That is a
//! tiling and panic-freedom check over 1335 real-world documents, and it is
//! worth having under that description rather than a grander one.

use std::path::{Path, PathBuf};

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
/// suite is green enough to guard a hand-written scanner. So the cost of this
/// file grows with every handler a stage implements, and the doc asks for a
/// **re-measurement at each stage** rather than a number anyone trusts.
///
/// Measured on the development machine, in a debug profile:
///
/// | Stage | Handlers live | The corpus test | `1mb.md` + `5mb.md` |
/// |---|---:|---:|---:|
/// | S1 | 9 of 16 | ~1.3 s | ~21 s |
/// | S2 | 13 of 16 | ~4.2 s | **~134 s** |
/// | S3 | 13 of 16 (+4 rules) | ~3.2 s | **~68 s** |
/// | S4 | 14 of 16 | ~4.3 s | **~229 s** |
/// | S5 | **16 of 16** | ~5.7 s | **~245 s** |
/// | S6 | 16 of 16 | ~5.7 s | **~248 s** |
/// | S7 | 16 of 16 (+ 5 `debug_assert!`s) | — | — (see below) |
///
/// **S7's row is empty on purpose, and that is a note about the column rather
/// than about S7.** The measurement taken this stage is a single
/// `--include-ignored` run of the whole file: **324.4 s**. That is not the sum
/// of the two columns and must not be read as one — libtest runs the six tests
/// in parallel, so the large-file test contends with the other five for cores,
/// where the recorded S1–S6 figures come from two *separate* invocations. A
/// number in a different shape in the same column is worse than no number.
///
/// What replaced it is a measurement this table was never able to make.
/// `benches/tokenizer.rs` now carries a debug **A/B** — the S7 tree against a
/// `git worktree` at `d258349`, alternated — and it puts the whole-corpus debug
/// figure at 257.7 s against 261.1 s, **1.013×**, with the `lowerPriority`
/// section (which reaches none of the five new assertions) at exactly 1.000×.
/// That is the attributable number, and it is what S5's caution asked for.
///
/// The S6 row is the point of having the table at all by now: a stage that adds
/// no rule and no nesting level should not move a debug profile, and it did
/// not. What S6 *does* add to this file is two more tests —
/// [`every_spec_fixture_tiles_and_round_trips`] at ~0.51 s and
/// [`every_marktext_round_trip_fixture_tiles_and_round_trips`] at ~0.20 s — so
/// the whole file is ~6.4 s where the corpus test alone is ~5.7 s. Those are
/// listed separately rather than folded into the column, because folding them
/// in would have made a stage that changed nothing look like a 12% regression.
///
/// S2's four handlers cost **6.4×**, which is more than the S1 note's
/// "plausibly a minute or more by S5" allowed for — S2 alone passed that. The
/// jump was not mysterious: `tryStrongEm` and `tryChunks` run six regexes at
/// every unmatched position, four of which (`strong`, `em`, `del`,
/// `inline_math`) carry a lazy `[\s\S]*?` that scans to the end of the level
/// before failing, and `250kb.md` is one very long level. That is the O(n²)
/// shape M1.md §5 D5 flags for `lowerPriority`, arriving early and from the
/// rule table instead.
///
/// # S2's prediction for S3 was wrong, and the model behind it was wrong
///
/// S2 reasoned that S3–S5 add three more lazy-quantified rules (`link`,
/// `image`, `reference_link`) and that extrapolating the same factor puts the
/// ignored pair past ten minutes. **S3 halved it instead: ~134 s → ~68 s.**
///
/// The correction is worth more than the number. Cost is not
/// *rules × characters*, it is *rules × **unmatched** characters*, because a
/// lazy rule only scans to the end of the level when it **fails**. `1mb.md`
/// contains 1586 links; at S2 each was ~37 consecutive positions at which six
/// lazy rules each scanned the rest of a very long paragraph, and at S3 a
/// single `try_link` consumes the construct and those positions stop existing.
/// A handler that matches often can pay for itself several times over.
///
/// S4's `html_tag` and S5's autolinks match *rarely* in this corpus, so they
/// should add cost rather than remove it — but the factor to expect is not
/// S2's.
///
/// # S4: the prediction held, and the cost has a different shape
///
/// **~68 s → ~229 s**, a 3.4× rise, and the first stage to make this number
/// worse. In release the same change is 2.4× (see `benches/tokenizer.rs`); the
/// gap is `check_tiling` and the extra nesting level `html_tag` introduces,
/// both of which only exist in a debug profile.
///
/// The shape is what to carry forward. S2's and S3's changes were
/// *shape-dependent* — they moved the rows with long levels and left short
/// ones alone, because a lazy `[\s\S]*?` costs in proportion to how far it
/// scans. S4's is **flat**: every measured row moved by about the same factor,
/// which means a fixed cost per rule *attempt* rather than per character
/// scanned. `html_tag` carries a `\3` backreference and an `(?i)` flag, so it
/// runs on `fancy-regex`'s backtracking path even to fail on the first
/// character, and before S4 the handler returned `false` without running it at
/// all.
///
/// The one thing S2 asked for that survives unchanged is the benchmark:
/// `benches/tokenizer.rs` exists as of S3, it covers the whole tokenizer
/// rather than `lowerPriority` alone, and it carries the release numbers, the
/// per-file breakdown and the remedy this comment only summarises.
///
/// # S5: a third cost shape, and a caution about this whole table
///
/// **~229 s → ~245 s** for the ignored pair, and ~4.3 s → ~5.7 s for the
/// covered run. Release moved 1.20× over the same change.
///
/// **Read the two rows as different numbers, not as one factor**, because they
/// disagree: 1.07× for the ignored pair against 1.33× for the covered run.
/// Both are cross-session wall clocks, and by S5 the stage-over-stage delta has
/// shrunk to about the size of the session-to-session drift — S5's release A/B
/// measured the stubbed build at 13.9 s where S4 had recorded 14.9 s for the
/// same behaviour, a 7% gap from nothing but the machine. `benches/tokenizer.rs`
/// carries the only clean attribution S5 has, an A/B on one binary, and **S6
/// should do the same here** rather than subtracting two sessions.
///
/// What the table is still good for is catching a *jump* — S2's 6.4× was
/// unmistakable at any precision. It is no longer good for a 10% claim.
///
/// The cost S5 adds is a scan *inside* a regex:
/// `auto_link_extension`'s email alternative opens
/// `[\w.!#$%&'*+/=?^`{|}~-]+@`, a class containing every letter, digit and
/// underscore, so at every position inside a word the engine runs to the end
/// of the word looking for an `@` that is not there. That is a third shape for
/// the model — S2/S3 measured how far a *lazy* quantifier scans when it fails,
/// S4 measured a flat per-attempt VM setup, and this is how far a **greedy**
/// one scans before the character that must follow it turns out to be absent.
/// Note that it adds **no nesting level** — neither autolink handler tokenizes
/// children — so unlike S4 this is not a change the debug profile should be
/// expected to pay for twice.
///
/// The covered run is what the limit is protecting, and it is still seconds
/// rather than minutes. 256 KiB is chosen to keep `250kb.md` in: it is the
/// largest file whose content differs *structurally* from the two above it.
///
/// # S6: the fourth shape is not one, and the S5 caution was the useful part
///
/// S5 predicted that *"S6 adds a post-pass over every token rather than a rule,
/// so it should move these numbers in a way neither of the three shapes
/// predicts"*. **It moved them by nothing: ~5.7 s and ~248 s, against ~5.7 s
/// and ~245 s.** The reason is that the post-pass does not run here — the
/// tokenizer skips it entirely when the highlight list is empty
/// (`lexer.ts:890`, M1.md §5 D7), and this file never passes one. So there is
/// no fourth cost shape to add to the model; there is a guard that works.
///
/// What S5 asked for that *was* worth doing is the A/B rather than the
/// subtraction, and `benches/tokenizer.rs` has it: the S6 tree against a
/// worktree at S5's commit, alternated in one session, with the difference
/// smaller than the within-binary spread in both directions. Where the
/// post-pass *does* cost something — 1024 highlights, ~0.5–0.8 ns per `union`
/// — is a section of that file too.
///
/// **Re-measured at S7** — and the answer is that this table has reached the
/// end of what it can tell anyone. S5 already said it was "no longer good for a
/// 10% claim"; S7 adds that the two columns cannot be filled by the run the
/// exit gate actually wants (`--include-ignored`, in release, over everything),
/// because that run has a different parallelism shape. The table is still good
/// for what S2's 6.4× was: **catching a jump**. For anything smaller, go to
/// `benches/tokenizer.rs`, which measures two binaries in one session.
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

    // Nine of eleven: `1mb.md` and `5mb.md` are the two over the limit. The
    // gate row's "22" was `cargo xtask diff`'s file count — that harness
    // collects from `spec/fixtures/marktext-round-trip/` as well, and both
    // directories hold exactly eleven. See this file's header.
    assert!(
        covered >= 9,
        "the corpus lost files: only {covered} checked, of the nine under the limit"
    );
    // Not silent: `deps.rs`'s own comment about crude checks applies here too
    // — a bounded run that does not say what it bounded reads as full
    // coverage.
    println!("tiling: {covered} corpus files checked, deferred to --ignored: {deferred:?}");
}

/// The other two corpus files — `1mb.md` and `5mb.md`, and whatever else grows
/// past the limit.
///
/// `#[ignore]` for the reason given at [`DEBUG_PROFILE_SIZE_LIMIT`], and
/// because the M1 exit gate that needs them (S7: *"tiling holds on all corpus
/// files"*) runs alongside the 24-hour soak rather than on every
/// `cargo test`. **S7 ran it, in release, and it passes**; `soak.yml`'s
/// `release-invariants` job keeps running it. Run it with:
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

/// `spec/fixtures/`, resolved the same way as [`corpus_dir`].
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("spec")
        .join("fixtures")
}

/// The `markdown` field of every example in a CommonMark/GFM spec fixture.
///
/// The same two files `xtask/src/conformance.rs` reads for the M2 ratchet, in
/// the same shape: an array of `{markdown, html, section, number}`.
fn spec_examples(file: &str) -> Vec<(String, String)> {
    let path = fixtures_dir().join(file);
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
                .unwrap_or_else(|| panic!("{}: example {number} has no `markdown`", file))
                .to_string();
            (format!("{file}#{number}"), markdown)
        })
        .collect()
}

/// Every `.md` under `spec/fixtures/marktext-round-trip/`, as `(name, text)`.
fn round_trip_fixtures() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let name = path
                    .strip_prefix(dir.parent().unwrap_or(dir))
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                out.push((name, text));
            }
        }
    }

    let mut files = Vec::new();
    walk(&fixtures_dir().join("marktext-round-trip"), &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// **The S6 gate, first half.** `generator(tokenize(s)) == s` for every
/// CommonMark and GFM example.
///
/// Deferred since S1 on the ground that it would only assert that 1324
/// fixtures tokenize to one text token each. Sixteen handlers later that is no
/// longer what it asserts — see this file's header for what it does and does
/// not claim.
#[test]
fn every_spec_fixture_tiles_and_round_trips() {
    let mut checked = 0;
    for file in ["commonmark-spec-0.31.json", "gfm-spec-0.29-gfm.json"] {
        for (name, markdown) in spec_examples(file) {
            check(&name, &markdown);
            checked += 1;
        }
    }
    assert!(
        checked >= 1324,
        "the fixture suites lost examples: only {checked} checked"
    );
    println!("tiling: {checked} CommonMark + GFM examples checked");
}

/// **The S6 gate, second half.** The eleven `marktext-round-trip/` fixtures —
/// whole files rather than single examples, which is the multi-line shape the
/// spec examples mostly are not.
#[test]
fn every_marktext_round_trip_fixture_tiles_and_round_trips() {
    let files = round_trip_fixtures();
    assert_eq!(
        files.len(),
        11,
        "spec/fixtures/marktext-round-trip/ holds eleven files; found {:?}",
        files.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    for (name, text) in &files {
        check(name, text);
    }
    println!(
        "tiling: {} marktext round-trip fixtures checked",
        files.len()
    );
}

/// The corpus is committed, so a file silently disappearing would quietly
/// shrink the gate. Named files, not a count.
///
/// **All eleven**, as of S7. The list used to hold nine — it omitted `1mb.md`
/// and `5mb.md`, the two this file runs only under `--ignored`, which is
/// exactly backwards: a file nobody runs on every commit is the one whose
/// disappearance would go unnoticed longest. The count is also the correction
/// to S7's gate row, which said 22; see this file's header.
#[test]
fn the_corpus_still_contains_the_files_this_gate_names() {
    let names: Vec<String> = corpus_files().into_iter().map(|(name, _)| name).collect();
    let expected = [
        "empty.md",
        "10kb.md",
        "cjk.md",
        "emoji.md",
        "rtl.md",
        "50-code-fences.md",
        "100-inline-math.md",
        "20-tables.md",
        "250kb.md",
        "1mb.md",
        "5mb.md",
    ];
    for name in expected {
        assert!(
            names.iter().any(|found| found == name),
            "bench/corpus/{name} is missing; found {names:?}"
        );
    }
    assert_eq!(
        names.len(),
        expected.len(),
        "bench/corpus/ holds eleven .md files plus a README; found {names:?}. A new one is \
         welcome — add it here, and check whether the sweep in xtask/src/tokens.rs should \
         treat it like the two large prose files."
    );
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
