//! E4 — `syntect` arm of the D3 bake-off (M3 S0, docs/M3.md §5 D3, C11).
//!
//! Four subcommands, one per axis §5 asks for on this engine. Each is its own
//! process invocation (mirrors E3's rule: a process-wide peak, or here a
//! process-wide "did the loader already warm a cache", must not leak between
//! scenarios), so `results/e4-highlight.md` prints the exact command lines
//! rather than one binary doing everything in one run.
//!
//! ```text
//! cold        <file>         time to first highlight: setup vs. first fence
//! throughput  <file> <reps>  steady-state lines/s and MB/s, best-of-<reps>
//! adapter     [<file>]       prices C11's "needs an adapter" claim, honestly
//! coverage                   what SyntaxSet::load_defaults_newlines() ships
//! ```
use std::ops::Range;
use std::time::Instant;

use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Highlighter, HighlightState, RangedHighlightIterator, Style, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};
use syntect::util::LinesWithEndings;

const THEME: &str = "base16-ocean.dark";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["cold", file] => mode_cold(file),
        ["throughput", file, reps] => mode_throughput(file, reps.parse().expect("reps: usize")),
        ["adapter"] => mode_adapter(None),
        ["adapter", file] => mode_adapter(Some(file)),
        ["coverage"] => mode_coverage(),
        _ => {
            eprintln!(
                "usage: e4-syntect <cold FILE | throughput FILE REPS | adapter [FILE] | coverage>"
            );
            std::process::exit(1);
        }
    }
}

/// Resolve a fence's language to a syntax, falling back to plain text the way
/// a highlighter must for a language it does not ship — recorded as a miss,
/// not a panic.
fn resolve<'a>(ps: &'a SyntaxSet, lang: Option<&str>) -> (&'a syntect::parsing::SyntaxReference, bool) {
    match lang.and_then(|l| ps.find_syntax_by_token(l)) {
        Some(s) => (s, true),
        None => (ps.find_syntax_plain_text(), false),
    }
}

// ---------------------------------------------------------------------------
// cold — time to first highlight
// ---------------------------------------------------------------------------

/// Everything before this line is CRT/loader startup this process cannot
/// observe from inside `main`; `results/e4-highlight.md` states that caveat
/// once rather than repeating it per engine. File read + `mt_md::parse` are
/// *not* timed here — they are `mt-md`'s cost, not `mt-highlight`'s, and the
/// language contract (crates/mt-highlight/src/lib.rs) is text + language in,
/// spans out.
fn mode_cold(file: &str) {
    let markdown = e4_common::read_file(file);
    let fences = e4_common::code_fences(&markdown);
    let first = fences
        .iter()
        .find(|f| !f.text.trim().is_empty())
        .expect("corpus has at least one non-empty fence");

    let t_setup = Instant::now();
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let setup = t_setup.elapsed();

    let (syntax, matched) = resolve(&ps, first.lang.as_deref());

    let t_fence = Instant::now();
    let mut h = HighlightLines::new(syntax, &ts.themes[THEME]);
    let mut first_span_at = None;
    let mut spans = 0usize;
    for line in LinesWithEndings::from(&first.text) {
        let ranges = h.highlight_line(line, &ps).expect("highlight_line");
        if first_span_at.is_none() && !ranges.is_empty() {
            first_span_at = Some(t_fence.elapsed());
        }
        spans += ranges.len();
    }
    let fence_total = t_fence.elapsed();

    println!(
        "[syntect cold] setup={setup:?} lang={:?} matched_syntax={matched} \
         first_span_at={first_span_at:?} fence_total={fence_total:?} spans={spans} \
         total={:?}",
        first.lang,
        setup + fence_total
    );
}

// ---------------------------------------------------------------------------
// throughput — steady state on the code-dense file
// ---------------------------------------------------------------------------

fn mode_throughput(file: &str, reps: usize) {
    let markdown = e4_common::read_file(file);
    let fences = e4_common::code_fences(&markdown);
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes[THEME];

    let total_bytes: usize = fences.iter().map(|f| f.text.len()).sum();
    let total_lines: usize = fences.iter().map(|f| f.text.lines().count()).sum();

    let run = |misses: &mut usize| {
        let mut spans = 0usize;
        for f in &fences {
            let (syntax, matched) = resolve(&ps, f.lang.as_deref());
            if !matched {
                *misses += 1;
            }
            let mut h = HighlightLines::new(syntax, theme);
            for line in LinesWithEndings::from(&f.text) {
                spans += h.highlight_line(line, &ps).expect("highlight_line").len();
            }
        }
        spans
    };

    // Warm-up: not timed, discarded. syntect's onig regexes compile lazily on
    // first use per syntax, so this pass is what pays that cost, not rep 0.
    let mut misses = 0usize;
    let _ = run(&mut misses);

    let mut durations = Vec::with_capacity(reps);
    for _ in 0..reps {
        let mut m = 0usize;
        let t = Instant::now();
        let spans = run(&mut m);
        durations.push((t.elapsed(), spans));
    }

    let best = durations.iter().map(|(d, _)| *d).min().unwrap();
    let worst = durations.iter().map(|(d, _)| *d).max().unwrap();
    let lines_per_sec = total_lines as f64 / best.as_secs_f64();
    let mb_per_sec = (total_bytes as f64 / 1_000_000.0) / best.as_secs_f64();

    println!(
        "[syntect throughput] file={file} fences={} total_lines={total_lines} \
         total_bytes={total_bytes} lang_misses={misses} reps={reps} best={best:?} worst={worst:?} \
         lines_per_sec={lines_per_sec:.0} mb_per_sec={mb_per_sec:.3}",
        fences.len()
    );
    for (i, (d, spans)) in durations.iter().enumerate() {
        println!("  rep{i}: {d:?} spans={spans}");
    }
}

// ---------------------------------------------------------------------------
// adapter — prices C11's "needs an adapter" claim
// ---------------------------------------------------------------------------

/// C11 / M3.md §5 D3: "its output is `Vec<(Style, &str)>`, so `mt-highlight`'s
/// byte-range contract needs an adapter." This recovers byte ranges from that
/// shape by pointer arithmetic: every `&str` piece syntect returns is
/// documented (`highlighting/highlighter.rs`: "the concatenation of the
/// strings in each token will make the original string") to be a contiguous,
/// in-order subslice of the input `line`, so its offset from `line`'s own
/// start is exactly its byte range's start.
///
/// 7 lines. That is the whole cost of the claim, if you enter through
/// `easy::HighlightLines`. See `mode_adapter` below for the finding that
/// makes this function moot for a real caller.
fn adapt_to_byte_ranges<'a>(
    line: &'a str,
    pieces: &[(Style, &'a str)],
) -> Vec<(Style, Range<usize>)> {
    let base = line.as_ptr() as usize;
    pieces
        .iter()
        .map(|(style, piece)| {
            let start = piece.as_ptr() as usize - base;
            (*style, start..start + piece.len())
        })
        .collect()
}

fn mode_adapter(file: Option<&str>) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes[THEME];

    let sample_owned;
    let sample: &str = match file {
        Some(f) => {
            sample_owned = e4_common::read_file(f);
            &sample_owned
        }
        None => "pub struct Wow { hi: u64 }\nfn blah(x: i32) -> i32 { x * 2 }\n",
    };
    let syntax = ps.find_syntax_by_extension("rs").expect("rust syntax");

    // Path 1: the easy API C11 describes, `Vec<(Style, &str)>`, then this
    // spike's pointer-arithmetic adapter — timed separately from the highlight
    // call itself so "what the adapter costs" is not confounded with parsing.
    let mut h = HighlightLines::new(syntax, theme);
    let mut adapter_ns_total = 0u128;
    let mut easy_ranges_total = 0usize;
    let mut checked_contiguous = true;
    for line in LinesWithEndings::from(sample) {
        let pieces = h.highlight_line(line, &ps).expect("highlight_line");
        let t = Instant::now();
        let ranges = adapt_to_byte_ranges(line, &pieces);
        adapter_ns_total += t.elapsed().as_nanos();
        // Self-check: ranges are contiguous and cover the whole line, which is
        // the invariant the pointer-arithmetic trick depends on.
        let mut cursor = 0usize;
        for (_, r) in &ranges {
            if r.start != cursor {
                checked_contiguous = false;
            }
            cursor = r.end;
        }
        if cursor != line.len() {
            checked_contiguous = false;
        }
        easy_ranges_total += ranges.len();
    }

    // Path 2: syntect's own `RangedHighlightIterator` — public API, one layer
    // below `easy::HighlightLines` — which yields `(Style, &str, Range<usize>)`
    // directly. No adapter, no pointer arithmetic, nothing to price.
    let mut state = ParseState::new(syntax);
    let highlighter = Highlighter::new(theme);
    let mut highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
    let mut ranged_total = 0usize;
    let t_ranged = Instant::now();
    for line in LinesWithEndings::from(sample) {
        let ops = state.parse_line(line, &ps).expect("parse_line");
        let iter = RangedHighlightIterator::new(&mut highlight_state, &ops[..], line, &highlighter);
        let regions: Vec<(Style, &str, Range<usize>)> = iter.collect();
        ranged_total += regions.len();
    }
    let ranged_elapsed = t_ranged.elapsed();

    println!(
        "[syntect adapter] easy_api: pieces_converted={easy_ranges_total} \
         adapter_time_total={adapter_ns_total}ns \
         adapter_time_per_piece={:.1}ns contiguous_and_covers_line={checked_contiguous}",
        adapter_ns_total as f64 / easy_ranges_total.max(1) as f64
    );
    println!(
        "[syntect adapter] ranged_iterator (no adapter, public API): \
         regions={ranged_total} total_time={ranged_elapsed:?} — \
         RangedHighlightIterator<Item = (Style, &str, Range<usize>)> is already the contract"
    );
}

// ---------------------------------------------------------------------------
// coverage — what the default bundle actually ships
// ---------------------------------------------------------------------------

fn mode_coverage() {
    let ps = SyntaxSet::load_defaults_newlines();
    println!("[syntect coverage] syntaxes={}", ps.syntaxes().len());
    let mut names: Vec<&str> = ps.syntaxes().iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    for name in names {
        println!("  {name}");
    }
    let mut exts: Vec<&str> = ps
        .syntaxes()
        .iter()
        .flat_map(|s| s.file_extensions.iter().map(String::as_str))
        .collect();
    exts.sort_unstable();
    exts.dedup();
    println!("[syntect coverage] distinct file extensions/tokens={}", exts.len());
    for e in exts {
        println!("  .{e}");
    }
}
