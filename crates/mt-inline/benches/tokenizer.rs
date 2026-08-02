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
//! # Measured — S3
//!
//! Development machine (Windows 11, x86-64). Add a row per stage; the point of
//! the table is that a 6× jump like S2's shows up as a jump rather than as a
//! test that got slow.
//!
//! | Stage | Handlers | corpus total, release | corpus total, debug | `lowerPriority` set, release |
//! |---|---:|---:|---:|---:|
//! | S3 | 13 of 16 | **6.1 s** (1.03 MiB/s) | 71.6 s (0.09 MiB/s) | 200 ms |
//!
//! Three things worth carrying forward:
//!
//! - **Throughput is flat at ~1 MiB/s across four orders of magnitude of
//!   input.** `empty.md` through `5mb.md` all land within a few percent of it,
//!   so on *prose* the tokenizer is linear in practice despite the
//!   quadratic-in-the-worst-case rules. The corpus is one leaf block per file,
//!   which is already the pessimistic framing — a real document is split into
//!   blocks first, and each block is tokenized separately.
//! - **`[` × 4000 is the outlier, and it is the canary.** 4 KiB of unclosed
//!   brackets costs 172 ms where 4 KiB of prose costs ~4 ms: **fifty times
//!   slower per byte**, because every position runs `link`, `reference_link`
//!   and the four lazy S2 rules to the end of the level before failing. It is
//!   in the set deliberately. If a later stage makes the prose rows worse, the
//!   cause will be visible here first, magnified.
//! - **Debug is ~12× release.** Both are recorded because `round_trip.rs`
//!   tracks the debug figure — that is what a developer waits for — and this
//!   file tracks the release one, which is what M3 will care about.
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
//! So a handler that matches often can pay for itself several times over, and
//! the stages left are the ones that match *rarely* in this corpus — S4's
//! `html_tag` and S5's autolinks. Expect them to add cost rather than remove
//! it, but not by the factor S2's extrapolation implied.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use mt_inline::tokenize;

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
    println!("stage:   M1 S3 — 13 of 16 handlers, 16 of 16 rules reachable\n");

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
}
