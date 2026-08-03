//! The whole-tokenizer benchmark — M1.md §5 D5, pulled forward to S3.
//!
//! # Why this exists, and why now rather than at S7
//!
//! D5 records `lowerPriority` as O(offset × |rules|) regex executions and asks
//! for a benchmark *"before M1 closes, so M3 is not surprised"*. S2 found that
//! the number to watch is bigger than `lowerPriority`:
//! `round_trip.rs`'s two ignored corpus files went from **~21 s at S1 to
//! ~134 s at S2** in a debug profile — 6.4× for four handlers — because
//! `strong`, `em`, `del` and `inline_math` each carry a lazy `[\s\S]*?` that
//! scans to the end of the level before failing, at every unmatched position.
//!
//! S3 adds three more lazy-quantified rules (`link`, `image`,
//! `reference_link`). So the ask was widened and moved: a **whole-tokenizer**
//! benchmark, existing from S3. By S7 the number would be too large to bisect
//! — one would know the tokenizer had become slow without knowing which stage
//! did it.
//!
//! # What it is not
//!
//! Not a statistical harness. There is no `criterion`, deliberately: the
//! workspace's dependency policy is that nothing lands before the milestone
//! that needs it, and what is needed here is *a number per stage that can be
//! compared with the previous stage's*, not a confidence interval. A wall
//! clock over a fixed corpus gives that. If M3 wants distributions, it can add
//! `criterion` then, against the same inputs.
//!
//! # Running it
//!
//! ```sh
//! cargo bench -p mt-inline                 # release; the meaningful numbers
//! cargo bench -p mt-inline -- --quick      # skip the two multi-megabyte files
//! ```
//!
//! `cargo bench` builds with the `bench` profile, which inherits `release`, so
//! the debug tiling assertion is compiled out. The header line says which
//! profile produced the numbers, because a debug figure and a release figure
//! differ by more than an order of magnitude and a table without that label is
//! worse than no table.
//!
//! `test = false` in the manifest keeps `cargo test --workspace` from running
//! a benchmark as a test; `cargo clippy --all-targets` still builds it.
//!
//! # Measured — S3, S4, S5, S6
//!
//! Development machine (Windows 11, x86-64). Add a row per stage; the point of
//! the table is that a 6× jump like S2's shows up as a jump rather than as a
//! test that got slow.
//!
//! | Stage | Handlers | corpus total, release | corpus total, debug | `lowerPriority` set, release |
//! |---|---:|---:|---:|---:|
//! | S3 | 13 of 16 | **6.1 s** (1.03 MiB/s) | 71.6 s (0.09 MiB/s) | 200 ms |
//! | S4 | 14 of 16 | **14.9 s** (0.42 MiB/s) | 243.4 s (0.03 MiB/s) | 472 ms |
//! | S5 | 16 of 16 | **16.3 s** (0.38 MiB/s) | 265.6 s (0.02 MiB/s) | 488 ms |
//! | S6 | 16 of 16 + 3 post-passes | **16.3 s** (0.38 MiB/s) | 260.1 s (0.02 MiB/s) | 501 ms |
//!
//! S5's release rise over S4 is 1.09× as the table reads, but that comparison
//! is across sessions and this is a wall clock. The trustworthy number is an
//! **A/B on the same binary**, with the two new handlers short-circuited
//! behind a one-time flag: **13.9 s stubbed against 16.6 s live, 1.20×**. The
//! table row is a separate clean run; 16.3 against 16.6 is this benchmark's
//! run-to-run noise (~1.5%), and the stubbed 13.9 against S4's recorded 14.9
//! is ordinary session variation — which is the reason the A/B was done at all
//! rather than trusting the cross-stage subtraction.
//!
//! # What S6 changed — nothing, and that was the prediction
//!
//! **Predicted before running: no measurable change on any row.** S6 adds no
//! rule, so it touches none of the model's three terms. The only thing
//! `tokenize()` gains is `if !options.highlights.is_empty()` in `tokenizer()`,
//! once per call, and every call in this file uses the default options — so the
//! post-pass never runs at all.
//!
//! S5 asked S6 to do an A/B rather than a cross-stage subtraction, because by
//! S5 the stage delta had shrunk to the size of session drift. Done, and the
//! form is different from S5's: there is no flag to stub, so it is **two
//! binaries alternated in one session** — the S6 tree against a `git worktree`
//! at `f9fea3e`.
//!
//! | Section | S5 r1 | S6 r1 | S5 r2 | S6 r2 | S5 mean | S6 mean | |
//! |---|---:|---:|---:|---:|---:|---:|---|
//! | `bench/corpus/` | 16.23 s | 16.20 s | 16.55 s | 16.37 s | 16.39 s | 16.29 s | 0.99× |
//! | `lowerPriority` | 490 ms | 497 ms | 475 ms | 505 ms | 483 ms | 501 ms | 1.04× |
//! | autolink | 53.3 ms | 53.1 ms | 52.5 ms | 53.5 ms | 52.9 ms | 53.3 ms | 1.01× |
//!
//! The corpus is nominally *faster* and the `lowerPriority` set nominally
//! slower, both by less than the within-binary spread (2% and 3%), and the
//! signs disagreeing is the tell. There is nothing to attribute.
//!
//! Debug agrees: 265.6 s → **260.1 s**, 2% down and in the same direction as
//! release, which is what session variation looks like when nothing changed.
//! The debug/release ratio is 260.1 / 16.3 = **16.0×**, the same as S4's and
//! S5's. That ratio has moved exactly once in the milestone — at S4, which
//! added a container and so charged `check_tiling` and an extra nesting level
//! twice. S6 adds neither, and it did not move.
//!
//! ## Where the post-pass does cost something
//!
//! The `s6_inputs` section below measures it, and it needed 1024 highlights to
//! become visible at all. Release:
//!
//! | Input | 1 | 8 | 64 | 1024 highlights |
//! |---|---:|---:|---:|---:|
//! | `10kb.md`, 338 tokens (27.6 ms to tokenize) | — | — | — | +0.17 ms |
//! | marker-dense prose, 2601 tokens (57.4 ms) | — | — | — | +2.20 ms |
//!
//! The dashes are not zeroes — they are **below this benchmark's noise floor**,
//! which is the result rather than a failure of the method. At 1024 the delta
//! resolves to about **0.5–0.8 ns per `union` call**: two integer comparisons, a
//! `max` and a `min`. A realistic search is tens of matches in a block, so the
//! post-pass is unmeasurable against the tokenize it follows, and M1.md §5 D7's
//! guard earns its place for the memory reason it was decided on rather than
//! for time.
//!
//! In debug the same rows are noisier still — the 64-highlight delta comes back
//! *negative* against a 424 ms baseline — so read the release figures. Running
//! this section in a debug profile is worth it only to confirm that.
//!
//! The other two post-passes are measurable directly, over an already-tokenized
//! tree: `tokensToPlainText` is **0.005 ms** for `10kb.md` (338 tokens) and
//! **0.026 ms** for 2601 tokens, and `marker_state` over every top-level token
//! is under a microsecond. Both are ~5000× cheaper than the tokenize that
//! produced their input — worth remembering when M3 asks whether the table of
//! contents should cache anything. It should not.
//!
//! Three things worth carrying forward:
//!
//! - **Throughput was flat at ~1 MiB/s across four orders of magnitude of
//!   input at S3, ~0.42 MiB/s at S4 and ~0.38 MiB/s at S5.** `empty.md`
//!   through `5mb.md` land within a few percent of it, so on *prose* the
//!   tokenizer is linear in practice despite the quadratic-in-the-worst-case
//!   rules. The corpus is one leaf block per file, which is already the
//!   pessimistic framing — a real document is split into blocks first, and
//!   each block is tokenized separately.
//! - **`[` × 4000 is the outlier, and it is the canary.** 4 KiB of unclosed
//!   brackets costs 412 ms at S4 (172 ms at S3) where 4 KiB of prose costs
//!   ~10 ms, because every position runs `link`, `reference_link` and the four
//!   lazy S2 rules to the end of the level before failing. It is in the set
//!   deliberately. If a later stage makes the prose rows worse, the cause will
//!   be visible here first, magnified. **S5 is the first stage it did not
//!   catch** — 412 ms before and after — and that is information rather than a
//!   failure: see "What S5 changed" below for why one canary is not enough.
//! - **Debug is ~12× release at S3, ~16× at S4, and ~16× at S5 — unchanged.**
//!   Both profiles are recorded because `round_trip.rs` tracks the debug
//!   figure — that is what a developer waits for — and this file tracks the
//!   release one, which is what M3 will care about. The ratio widened at S4
//!   because `check_tiling` and the extra nesting level `html_tag` introduces
//!   exist only in debug, so a stage that adds a *container* costs that profile
//!   twice. **S5 holding the ratio flat is the confirmation of that
//!   explanation**: neither autolink handler tokenizes children, so there is no
//!   second charge, and 265.6/16.3 lands on the same 16× S4 did. A ratio is
//!   worth recording precisely because it is the number that moves when the
//!   *shape* of the work changes rather than its amount.
//!
//! # What S3 changed, and it is the opposite of what S2 predicted
//!
//! S2 measured the two ignored corpus files at ~134 s and reasoned that S3–S5
//! would add three more lazy-quantified rules and push them "past ten
//! minutes". **S3 halved them instead: ~68 s.**
//!
//! The reasoning was sound and the model was wrong. Cost is not
//! *rules × characters*; it is *rules × **unmatched** characters*, because a
//! rule only scans to the end of the level when it **fails**. `1mb.md`
//! contains 1586 links, and at S2 each one was ~37 positions at which six
//! lazy rules each scanned the rest of a very long paragraph. At S3 one
//! `try_link` call consumes the whole construct, so those positions stop
//! existing.
//!
//! # What S4 changed — the prediction held, and the *shape* is new
//!
//! S3 predicted that S4's `html_tag` and S5's autolinks "match rarely in this
//! corpus, so they should add cost rather than remove it". **It held: 6.1 s →
//! 14.9 s, a 2.4× rise.** That is the first stage to make the number worse.
//!
//! What is worth carrying forward is not the factor but its *uniformity*.
//! Every row moved by almost exactly the same 2.4×: prose 1.03 → 0.42 MiB/s,
//! the `[` × 4000 canary 172 → 412 ms, the `lowerPriority` set 200 → 472 ms.
//! S2's and S3's changes were **shape-dependent** — they moved the rows with
//! long levels and left the short ones alone, because a lazy `[\s\S]*?` costs
//! in proportion to how far it has to scan. S4's is **flat**, and flat means a
//! fixed cost per *attempt* rather than per character scanned.
//!
//! That is `html_tag` being the most expensive rule in the table to merely
//! *fail*. Its `\3` backreference and its `(?i)` flag put it on
//! `fancy-regex`'s backtracking path rather than the delegated
//! non-backtracking one, so every unmatched position now pays a VM setup even
//! though both alternatives reject on the first character. Before S4 the
//! handler returned `false` without running the rule at all; the rule's
//! presence in `lowerPriority`'s two sets was not the same cost, because those
//! only run *inside* a candidate emphasis or link span.
//!
//! **The obvious remedy is deliberately not taken here.** Both alternatives of
//! `html_tag` begin with a literal `<`, so skipping the `exec` when the next
//! byte is not `<` would be semantics-preserving and would recover most of the
//! 2.4×. It is not done at S4 because §3 sequences it that way — port
//! faithfully, then hand-optimize with the suite as the guard — and because a
//! first-byte pre-check is a change to *how rules are run*, which belongs with
//! the rest of the rule-table work at M3 rather than inside a handler stage.
//! Recorded here so M3 finds it as a measurement rather than a hunch.
//!
//! # What S5 changed — a third cost shape, and a prediction that missed
//!
//! S5 predicted, before measuring, a **1.4–1.8× rise, flat across rows**, on
//! S4's two-term model: `auto_link` has no backreference and no `(?i)` so it
//! should delegate to the non-backtracking engine and cost nothing;
//! `auto_link_extension` has a lookahead so it should pay `html_tag`'s flat
//! per-attempt VM setup.
//!
//! Measured: **1.20×, and not flat.** Half the prediction held —
//! `auto_link` is free. The other half was the wrong mechanism entirely, and
//! the `autolink_inputs` section below exists because the corpus rows could
//! not show which:
//!
//! | Input | stubbed | live | |
//! |---|---:|---:|---|
//! | one 4000-character word run, no `@` | 9.9 ms | **22.5 ms** | 2.3× |
//! | the same 4000 characters as 800 short words | 9.7 ms | 11.0 ms | 1.15× |
//! | the same again, broken by a non-local-part character | 9.8 ms | 10.9 ms | 1.11× |
//! | 160 bare URLs that match | 9.7 ms | **0.9 ms** | **0.09×** |
//! | 240 bare emails that match | 10.0 ms | **1.3 ms** | **0.13×** |
//! | a 2000-character trimmable tail | 5.0 ms | 7.0 ms | 1.44× |
//! | `[` × 4000 (the S4 canary) | 418.8 ms | 412.4 ms | 1.00× |
//!
//! - **The cost is the email alternative's local part.**
//!   `[\w.!#$%&'*+/=?^`{|}~-]+@` contains every letter, digit and underscore,
//!   so at every position inside a word the engine scans to the end of the word
//!   run looking for an `@` that is not there. Rows one and two are the *same
//!   4000 bytes* and differ by 2×, entirely in how long the runs are. So the
//!   cost model is now **three-termed**: *(how far a **lazy** quantifier scans
//!   when it fails)* — S2/S3; *(a flat VM setup per attempt)* — S4; and *(how
//!   far a **greedy** quantifier scans before the character that must follow it
//!   turns out to be absent)* — S5.
//! - **One canary is not enough.** `[` × 4000 did not move at all, because not
//!   one of its characters is in that class. It is still the right canary for
//!   the lazy-quantifier shape; it is blind to this one. That is why rows one
//!   to three below are the *same bytes* rearranged — a shape difference a
//!   single input cannot express.
//! - **A matching autolink pays for itself an order of magnitude over.** Bare
//!   URLs went 9.7 ms → 0.9 ms, an **11× speedup**, because one match consumes
//!   twenty-five characters that would otherwise each have been a position at
//!   which every rule ran and failed. S3 found this at 2×; here it is 11×, and
//!   it is why the corpus rose only 1.20× despite row one's 2.3×.
//! - **The trim is quadratic in the extent** — the paren rule recounts
//!   `raw[0..end]` on every pass while `end` moves by one. muya is quadratic
//!   there too, so it is faithfulness rather than a regression, but note that
//!   it is hand-written Rust and therefore the one part of S5 that
//!   `rules::BACKTRACK_LIMIT` does **not** bound. S7's fuzzer should know.
//!
//! **A second M3 candidate, recorded and not taken.** `tryAutoLinkExtension`
//! runs `exec` *before* its `state.top` guard, which is muya's order and is
//! unobservable — so hoisting the guard would skip the rule in every nested
//! level for nothing. Like the first-byte pre-check above it is a change to
//! *when a rule is run*, so it belongs with the rule-table work at M3.
//!
//! **S6 implements neither and adds no third**, because it adds no rule. Both
//! remain M3's, and both remain measurements rather than hunches: the
//! first-byte pre-check is worth most of S4's 2.4×, and the `state.top` hoist
//! is worth whatever fraction of `auto_link_extension`'s cost falls in nested
//! levels.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use mt_inline::{
    Highlight, Span, TokenizerOptions, marker_state, tokenize, tokenizer, tokens_to_plain_text,
};

/// Inputs aimed at [`mt_inline`]'s known hot spot rather than at real prose.
///
/// `lowerPriority` walks **every position** inside a candidate emphasis or
/// link span and runs **every rule in its set** at each one, so its cost grows
/// with the square of the span length. These are the shapes that make it do
/// that: a long emphasis span, a long link, and the two marktext fixes
/// (`ignoreIndex` #1071 and the backslash-parity skip #3778) that add work
/// inside the scan.
///
/// Each is repeated to a fixed size so the numbers are comparable between
/// stages even as the handler set grows.
fn lower_priority_inputs() -> Vec<(String, String)> {
    vec![
        (
            "one long strong span".to_string(),
            format!("**{}**", "word ".repeat(400)),
        ),
        ("many short strong spans".to_string(), "**a** ".repeat(400)),
        (
            "#1071 — code spans inside emphasis".to_string(),
            "**`word`**, ".repeat(200),
        ),
        (
            "#3778 — escaped dollars inside emphasis".to_string(),
            r"It costs **\$20** to **\$30** online. ".repeat(100),
        ),
        (
            "one long link anchor".to_string(),
            format!("[{}](https://example.com)", "word ".repeat(400)),
        ),
        (
            "many links on one line".to_string(),
            "[a](https://example.com/path) ".repeat(200),
        ),
        (
            "links with parenthesised destinations".to_string(),
            "[a](path/to/(file).html) and (parens) ".repeat(100),
        ),
        (
            "unclosed brackets — the backtracking shape".to_string(),
            "[".repeat(4000),
        ),
        (
            "unclosed link destination".to_string(),
            format!("[{}](", "a".repeat(4000)),
        ),
    ]
}

/// Inputs aimed at what S5's two rules cost, which turned out not to be what
/// S4's cost.
///
/// `auto_link_extension`'s email alternative opens
/// `[\w.!#$%&'*+/=?^`{|}~-]+@`, and that class contains **every letter, digit
/// and underscore**. So at every position inside a word the engine scans
/// forward to the end of the word run looking for an `@` that is not there.
/// The cost is therefore proportional to how far a *greedy* quantifier gets
/// before the character that must follow it turns out to be absent — a third
/// shape, alongside S2/S3's lazy-quantifier scan and S4's flat per-attempt VM
/// setup.
///
/// The first two rows are the same bytes arranged to make that visible: one
/// long unbroken word run against the same characters broken into short words.
/// The last two are the rules matching rather than failing, and the trim's own
/// quadratic case.
fn autolink_inputs() -> Vec<(String, String)> {
    vec![
        (
            "one 4000-character word run — no `@`".to_string(),
            "a".repeat(4000),
        ),
        (
            "the same characters as 800 short words".to_string(),
            "aaaa ".repeat(800),
        ),
        (
            "the same again, broken by a non-local-part character".to_string(),
            "aaaa(".repeat(800),
        ),
        (
            "bare URLs that match".to_string(),
            "https://example.com/path ".repeat(160),
        ),
        (
            "bare emails that match".to_string(),
            "user@example.com ".repeat(240),
        ),
        (
            "a trimmable tail — the paren rule recounts on every pass".to_string(),
            format!("https://example.com/{}", ")".repeat(2000)),
        ),
    ]
}

/// How many times an S6 post-pass row is repeated. These are cheap, so the
/// count is high enough that the timer resolution is not the measurement.
const S6_RUNS: u32 = 20;

/// One caret, planted a third of the way in, for the `marker_state` row.
const CARET: mt_inline::Cursor = mt_inline::Cursor::collapsed(1024);

/// Inputs for the S6 section: the three post-passes cost per **token**, not per
/// character, so what matters is a realistic token density rather than a
/// pathological rule shape. Prose from the corpus is the realistic one; the
/// marker-heavy row is what a document full of inline syntax looks like.
fn s6_inputs() -> Vec<(String, String)> {
    let mut out = vec![(
        "marker-dense prose".to_string(),
        "A **bold** and *em* run with `code`, a [link](https://example.com) and an \
         entity &amp; too. "
            .repeat(200),
    )];
    let path = corpus_dir().join("10kb.md");
    if let Ok(text) = std::fs::read_to_string(&path) {
        out.insert(0, ("10kb.md".to_string(), text));
    }
    out
}

/// A run of `count` highlights spread across `src`, on `char` boundaries.
fn highlights(src: &str, count: usize) -> Vec<Highlight> {
    let step = (src.len() / (count + 1)).max(1);
    (0..count)
        .map(|i| {
            let start = src
                .char_indices()
                .map(|(at, _)| at)
                .find(|at| *at >= (i + 1) * step)
                .unwrap_or(0);
            let end = src
                .char_indices()
                .map(|(at, _)| at)
                .find(|at| *at >= start + 32)
                .unwrap_or(src.len());
            Highlight {
                span: Span::new(start, end),
                active: (i == 0).then_some(true),
            }
        })
        .collect()
}

/// Time `f` over an already-tokenized tree, so the row is the post-pass alone
/// rather than the post-pass plus another tokenize.
fn time_over<F: Fn(&[mt_inline::Token])>(tokens: &[mt_inline::Token], f: F) -> Duration {
    f(tokens);
    let start = Instant::now();
    for _ in 0..S6_RUNS {
        f(tokens);
    }
    start.elapsed() / S6_RUNS
}

/// [`time`], for a call that needs non-default options.
fn time_tokenizer(src: &str, options: &TokenizerOptions) -> Duration {
    let _ = tokenizer(src, options);
    let start = Instant::now();
    for _ in 0..S6_RUNS {
        std::hint::black_box(tokenizer(src, options));
    }
    start.elapsed() / S6_RUNS
}

fn count_tokens(tokens: &[mt_inline::Token]) -> usize {
    tokens
        .iter()
        .map(|token| 1 + token.children().map_or(0, count_tokens))
        .sum()
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bench")
        .join("corpus")
}

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
    files.sort_by_key(|(_, text)| text.len());
    files
}

/// Tokenize `src` enough times to be worth timing, and return the per-run mean.
///
/// muya tokenizes one **leaf block's** text, not a document, so a corpus file
/// here is one very long block. That is not what an editor does — and it is
/// exactly the shape the O(n²) rules are worst on, which is why it is the
/// input that catches a stage making things worse.
fn time(src: &str) -> Duration {
    // Enough iterations to clear timer noise on the small files, one on the
    // large ones. The threshold is arbitrary; the reported figure is per run
    // either way.
    //
    // Note what this costs in a **debug** profile, because it is not obvious
    // from the reported numbers and it dominates the wall clock: the small
    // inputs include the pathological rows, and `[` × 4000 at ~12 s per run in
    // debug is four minutes of the run all by itself. The reported per-run
    // figure is unaffected — this is a note about how long `cargo bench
    // --profile dev` takes, not about what it measures. If that becomes
    // tiresome, lower `runs` for the pathological set rather than dropping it.
    let runs = if src.len() < 64 * 1024 { 20 } else { 1 };
    // One untimed run so a lazily compiled rule table is not charged to the
    // first file measured.
    let _ = tokenize(src);
    let start = Instant::now();
    for _ in 0..runs {
        let tokens = tokenize(src);
        std::hint::black_box(&tokens);
    }
    start.elapsed() / runs
}

fn row(name: &str, bytes: usize, elapsed: Duration) {
    let secs = elapsed.as_secs_f64();
    let throughput = if secs > 0.0 {
        format!("{:>9.2} MiB/s", bytes as f64 / secs / (1024.0 * 1024.0))
    } else {
        "        —".to_string()
    };
    println!(
        "  {name:<44} {bytes:>9} B {:>11.3} ms {throughput}",
        secs * 1000.0
    );
}

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    // `cargo bench` passes libtest flags even to a harnessless target.
    let profile = if cfg!(debug_assertions) {
        "debug (debug_assertions ON — check_tiling is running)"
    } else {
        "release (debug_assertions off)"
    };

    println!("mt-inline whole-tokenizer benchmark — M1.md §5 D5");
    println!("profile: {profile}");
    println!("stage:   M1 S6 — 16 of 16 handlers, plus the three post-passes\n");

    println!("bench/corpus/ — one file is one leaf block:");
    let mut total_bytes = 0usize;
    let mut total = Duration::ZERO;
    let mut skipped = Vec::new();
    for (name, src) in corpus_files() {
        if quick && src.len() > 256 * 1024 {
            skipped.push(name);
            continue;
        }
        let elapsed = time(&src);
        row(&name, src.len(), elapsed);
        total_bytes += src.len();
        total += elapsed;
    }
    row("TOTAL", total_bytes, total);
    if !skipped.is_empty() {
        println!("  (--quick skipped: {skipped:?})");
    }

    println!("\nlowerPriority-shaped inputs — M1.md §5 D5's original ask:");
    let mut total = Duration::ZERO;
    let mut total_bytes = 0usize;
    for (name, src) in lower_priority_inputs() {
        let elapsed = time(&src);
        row(&name, src.len(), elapsed);
        total_bytes += src.len();
        total += elapsed;
    }
    row("TOTAL", total_bytes, total);

    println!("\nautolink-shaped inputs — what S5 costs, and where:");
    let mut total = Duration::ZERO;
    let mut total_bytes = 0usize;
    for (name, src) in autolink_inputs() {
        let elapsed = time(&src);
        row(&name, src.len(), elapsed);
        total_bytes += src.len();
        total += elapsed;
    }
    row("TOTAL", total_bytes, total);

    println!("\nS6's three post-passes, against the tokenize they follow:");
    for (name, src) in s6_inputs() {
        let tokens = tokenize(&src);
        let token_count = count_tokens(&tokens);
        println!("  {name} — {} B, {token_count} tokens", src.len());

        row(
            "    tokensToPlainText",
            src.len(),
            time_over(&tokens, |tokens| {
                std::hint::black_box(tokens_to_plain_text(&src, tokens));
            }),
        );
        row(
            "    marker_state, every token",
            src.len(),
            time_over(&tokens, |tokens| {
                for token in tokens {
                    std::hint::black_box(marker_state(token, Some(CARET)));
                }
            }),
        );

        // The post-pass is not separately callable — muya has no such entry
        // point and neither does the port — so it is measured as the difference
        // between two `tokenizer` calls that differ only in the highlight list.
        // At realistic counts that difference is **below this benchmark's own
        // noise**, which is the finding rather than a failure of the method;
        // the last row exists to show where it does become visible.
        let baseline = time_tokenizer(&src, &TokenizerOptions::muya_default());
        row(
            "    tokenize, no highlights (baseline)",
            src.len(),
            baseline,
        );
        for count in [1usize, 8, 64, 1024] {
            let options = TokenizerOptions::muya_default().with_highlights(highlights(&src, count));
            let with = time_tokenizer(&src, &options);
            let delta = with.saturating_sub(baseline);
            println!(
                "    + post-pass, {count:>4} highlights          {:>11.3} ms   \
                 delta {:>9.3} ms   {:>10.1} ns/union",
                with.as_secs_f64() * 1000.0,
                delta.as_secs_f64() * 1000.0,
                delta.as_secs_f64() * 1e9 / (token_count * count) as f64,
            );
        }
    }
}
