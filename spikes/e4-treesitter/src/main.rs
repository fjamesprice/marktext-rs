//! E4 — `tree-sitter` arm of the D3 bake-off (M3 S0, docs/M3.md §5 D3, C11).
//!
//! Uses `tree-sitter-highlight`, not bare `Parser::parse` (that is what
//! `spikes/e2-treesitter-*` measured, for binary size only) — parsing a
//! document is not highlighting it, and `HighlightEvent::{Source,
//! HighlightStart, HighlightEnd}` is the actual output C11 credits with
//! being "a direct fit" for the byte-range contract.
//!
//! ```text
//! cold        <file>         time to first highlight: setup vs. first fence
//! throughput  <file> <reps>  steady-state lines/s and MB/s, best-of-<reps>
//! coverage                   which corpus languages this grammar set covers
//! ```
use std::time::Instant;

use tree_sitter_highlight::{Highlighter, HighlightConfiguration};

/// The capture names `tree-sitter-highlight` recognizes internally
/// (`STANDARD_CAPTURE_NAMES` in its own `src/highlight.rs`) — that constant is
/// private, so this is a copy of the same list, not a guess: every language's
/// `HIGHLIGHTS_QUERY` in this crate's dependency set writes captures like
/// `@function`, `@keyword`, `@string`, `@number` that only fire if the name
/// (or its dotted prefix) is on this list.
const RECOGNIZED_CAPTURES: &[&str] = &[
    "attribute", "boolean", "carriage-return", "comment", "comment.documentation",
    "constant", "constant.builtin", "constructor", "constructor.builtin", "embedded",
    "error", "escape", "function", "function.builtin", "keyword", "markup",
    "markup.bold", "markup.heading", "markup.italic", "markup.link", "markup.link.url",
    "markup.list", "markup.list.checked", "markup.list.numbered", "markup.list.unchecked",
    "markup.list.unnumbered", "markup.quote", "markup.raw", "markup.raw.block",
    "markup.raw.inline", "markup.strikethrough", "module", "number", "operator",
    "property", "property.builtin", "punctuation", "punctuation.bracket",
    "punctuation.delimiter", "punctuation.special", "string", "string.escape",
    "string.regexp", "string.special", "string.special.symbol", "tag", "type",
    "type.builtin", "variable", "variable.builtin", "variable.member", "variable.parameter",
];

/// Build exactly one `HighlightConfiguration`, for the lazy-load case: a real
/// previewer only needs the grammar for a language it has actually seen, the
/// way Prism only imports the language a fence names. Aliases the corpus's
/// info-string tokens (`js`, `ts`, `sh`) to their grammar crate's own name.
fn build_config_for(alias: &str) -> Option<HighlightConfiguration> {
    macro_rules! cfg {
        ($lang:expr, $highlights:expr, $injections:expr) => {{
            let mut c = HighlightConfiguration::new($lang, alias, $highlights, $injections, "")
                .unwrap_or_else(|e| panic!("{alias}: bad query: {e}"));
            c.configure(RECOGNIZED_CAPTURES);
            c
        }};
    }
    Some(match alias {
        "rust" => cfg!(
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY
        ),
        "js" => cfg!(
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY
        ),
        "ts" => cfg!(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ""
        ),
        "python" => cfg!(
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
            ""
        ),
        "c" => cfg!(
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
            ""
        ),
        "cpp" => cfg!(
            tree_sitter_cpp::LANGUAGE.into(),
            tree_sitter_cpp::HIGHLIGHT_QUERY,
            ""
        ),
        "java" => cfg!(
            tree_sitter_java::LANGUAGE.into(),
            tree_sitter_java::HIGHLIGHTS_QUERY,
            ""
        ),
        "go" => cfg!(
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
            ""
        ),
        "sh" => cfg!(
            tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::HIGHLIGHT_QUERY,
            ""
        ),
        "yaml" => cfg!(
            tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            ""
        ),
        "json" => cfg!(
            tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
            ""
        ),
        "toml" => cfg!(
            tree_sitter_toml_ng::LANGUAGE.into(),
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            ""
        ),
        "html" => cfg!(
            tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY
        ),
        "css" => cfg!(
            tree_sitter_css::LANGUAGE.into(),
            tree_sitter_css::HIGHLIGHTS_QUERY,
            ""
        ),
        _ => return None,
    })
}

/// Build one `HighlightConfiguration` per grammar this spike links — the
/// eager-registration case, all 14 up front. Used by `throughput` (which
/// exercises every language in the corpus) and by `cold`'s `--eager` arm.
fn build_configs() -> Vec<(&'static str, HighlightConfiguration)> {
    const ALIASES: &[&str] = &[
        "rust", "js", "ts", "python", "c", "cpp", "java", "go", "sh", "yaml", "json", "toml",
        "html", "css",
    ];
    ALIASES
        .iter()
        .map(|&alias| (alias, build_config_for(alias).expect(alias)))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["cold", file] => mode_cold(file, false),
        ["cold", file, "--eager"] => mode_cold(file, true),
        ["throughput", file, reps] => mode_throughput(file, reps.parse().expect("reps: usize")),
        ["coverage"] => mode_coverage(),
        _ => {
            eprintln!(
                "usage: e4-treesitter <cold FILE [--eager] | throughput FILE REPS | coverage>"
            );
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// cold — time to first highlight
// ---------------------------------------------------------------------------

/// Same caveat as `e4-syntect`'s `mode_cold`: this times from `main` entry,
/// not real process start, and excludes file read + `mt_md::parse` (that is
/// `mt-md`'s cost, not `mt-highlight`'s).
///
/// `eager=true` builds all 14 linked grammars' `HighlightConfiguration`s
/// before highlighting anything — the worst case, and the only case syntect's
/// single whole-bundle load has an equivalent to. `eager=false` (the
/// default) builds only the first fence's own grammar, matching a lazy
/// design where a language's `Query` is compiled the first time it is seen —
/// the fairer number for "time to first highlight" specifically, and the one
/// a real embedding would actually choose (§5 D3 already reads tree-sitter's
/// per-grammar static linkage as a reason on-demand loading is a real
/// subsystem, not a flag — the same applies to configuring the query).
fn mode_cold(file: &str, eager: bool) {
    let markdown = e4_common::read_file(file);
    let fences = e4_common::code_fences(&markdown);
    let first = fences
        .iter()
        .find(|f| !f.text.trim().is_empty())
        .expect("corpus has at least one non-empty fence");
    let lang = first.lang.as_deref().unwrap_or("");

    let t_setup = Instant::now();
    let (config, config_count) = if eager {
        let configs = build_configs();
        let found = configs.into_iter().find(|(name, _)| *name == lang).map(|(_, c)| c);
        (found, 14)
    } else {
        (build_config_for(lang), 1)
    };
    let setup = t_setup.elapsed();

    let t_fence = Instant::now();
    let mut first_event_at = None;
    let mut events = 0usize;
    let matched = config.is_some();
    if let Some(config) = &config {
        let mut highlighter = Highlighter::new();
        let iter = highlighter
            .highlight(config, first.text.as_bytes(), None, |_| None)
            .expect("highlight");
        for ev in iter {
            let _ = ev.expect("highlight event");
            if first_event_at.is_none() {
                first_event_at = Some(t_fence.elapsed());
            }
            events += 1;
        }
    }
    let fence_total = t_fence.elapsed();

    println!(
        "[tree-sitter cold] eager={eager} grammars_configured={config_count} setup={setup:?} \
         lang={:?} matched_grammar={matched} first_event_at={first_event_at:?} \
         fence_total={fence_total:?} events={events} total={:?}",
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
    let configs = build_configs();

    let total_bytes: usize = fences.iter().map(|f| f.text.len()).sum();
    let total_lines: usize = fences.iter().map(|f| f.text.lines().count()).sum();

    let lookup = |lang: Option<&str>| {
        configs
            .iter()
            .find(|(name, _)| Some(*name) == lang)
            .map(|(_, c)| c)
    };

    let run = |misses: &mut usize| {
        let mut highlighter = Highlighter::new();
        let mut events = 0usize;
        for f in &fences {
            match lookup(f.lang.as_deref()) {
                Some(config) => {
                    let iter = highlighter
                        .highlight(config, f.text.as_bytes(), None, |_| None)
                        .expect("highlight");
                    for ev in iter {
                        let _ = ev.expect("highlight event");
                        events += 1;
                    }
                }
                None => *misses += 1,
            }
        }
        events
    };

    // Warm-up: not timed.
    let mut misses = 0usize;
    let _ = run(&mut misses);

    let mut durations = Vec::with_capacity(reps);
    for _ in 0..reps {
        let mut m = 0usize;
        let t = Instant::now();
        let events = run(&mut m);
        durations.push((t.elapsed(), events));
    }

    let best = durations.iter().map(|(d, _)| *d).min().unwrap();
    let worst = durations.iter().map(|(d, _)| *d).max().unwrap();
    let lines_per_sec = total_lines as f64 / best.as_secs_f64();
    let mb_per_sec = (total_bytes as f64 / 1_000_000.0) / best.as_secs_f64();

    println!(
        "[tree-sitter throughput] file={file} fences={} total_lines={total_lines} \
         total_bytes={total_bytes} lang_misses={misses} reps={reps} best={best:?} worst={worst:?} \
         lines_per_sec={lines_per_sec:.0} mb_per_sec={mb_per_sec:.3}",
        fences.len()
    );
    for (i, (d, events)) in durations.iter().enumerate() {
        println!("  rep{i}: {d:?} events={events}");
    }
}

// ---------------------------------------------------------------------------
// coverage — which languages this linked grammar set covers
// ---------------------------------------------------------------------------

fn mode_coverage() {
    let configs = build_configs();
    println!("[tree-sitter coverage] linked_grammars={}", configs.len());
    for (name, _) in &configs {
        println!("  {name}");
    }
}
