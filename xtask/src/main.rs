//! `xtask` — build, test-harness, and packaging automation.
//!
//! Run as `cargo xtask <command>` (see `.cargo/config.toml` for the alias).
//! Nothing here ships; this crate exists so that CI on three platforms runs
//! the same code a developer runs locally, without a shell script per OS.
//!
//! Commands:
//!
//! | Command | What it does | Plan reference |
//! |---|---|---|
//! | `conformance` | CommonMark + GFM ratchet | §11.1 |
//! | `blocks` | Block-tree differential over 1344 inputs | M2 §6 S1 |
//! | `diff` | Differential test against the TypeScript engine | §11.2 |
//! | `divergences` | The register of intentional differences from muya | M1 §5 D3 |
//! | `normalize` | `normalizeHtml` vs `spec/runner.ts`'s | M2 §10, owed since M0 |
//! | `corpus` | Generate `bench/corpus/` | §14 step 4 |
//! | `layout` | Textual layout goldens for every corpus file × theme | M3 §5 D10 |
//! | `grammars` | Regenerate `mt-highlight`'s grammar tables from a loaded Prism | M3 §5 D14 |
//! | `highlight` | Highlight-span differential against Prism, over every corpus fence | M3 §5 D14/D15 |
//! | `fuzz-seed` | Write `fuzz/corpus/` from the sweep's inputs | M1 §5 D6 |
//! | `deps` | Enforce the dependency-direction constraints | §1 |
//! | `ci` | All of the above, in order | §9 M0 exit gate |

mod blocks;
mod conformance;
mod corpus;
mod deps;
mod diff;
mod divergences;
mod fuzz;
mod grammars;
mod highlight;
mod html;
mod layout;
mod normalize;
mod tokens;

use std::path::{Path, PathBuf};

/// The workspace root.
///
/// Derived from this crate's manifest directory at compile time rather than
/// from the current working directory, so `cargo xtask` behaves the same
/// whichever subdirectory it is invoked from.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ always has a parent")
        .to_path_buf()
}

const USAGE: &str = "\
cargo xtask <COMMAND> [ARGS...]

COMMANDS:
    conformance [OPTIONS]
                         Run the CommonMark + GFM conformance ratchet (§11.1).
        --suite <NAME>     commonmark | gfm. Default: both.
        --sections         Print the per-section pass rates that
                           spec/conformance.md is written from.
        --update           Rewrite spec/expected-failures.json from the
                           measured results. ONE-WAY DOOR: see
                           spec/README.md step 1 and docs/M2.md §5 D5.
    blocks [OPTIONS]     Compare the full TState tree — names, meta and leaf
                         text — against MarkdownToState over §4 C1's 1344
                         inputs (docs/M2.md §6 S1 and S2).
        --require-ts       Fail instead of skipping when the TypeScript engine
                           is unavailable.
        --no-text          Compare names and meta only. S1's mode, kept
                           because it is the fastest way to tell a structure
                           regression from a leaf-text one; it is no longer
                           the default, because leaf text landed at S2.
        --only <SUBSTR>    Only inputs whose label contains SUBSTR.
        --verbose          Print every disagreement rather than the first 20.
    diff [OPTIONS]       Run the differential test against @muyajs/core (§11.2).
        --require-ts       Fail instead of skipping when the TypeScript engine
                           is unavailable.
        --mt-cli <PATH>    Use a specific mt-cli binary.
        <FILE...>          Compare only these files.
    divergences [OPTIONS]
                         Check spec/divergences.json, the register of
                         intentional differences from muya (M1 §5 D3), by
                         comparing token streams against the TypeScript engine.
        --require-ts       Fail instead of skipping when the TypeScript engine
                           is unavailable.
        --no-sweep         Check the register's own inputs only, skipping
                           rule 1's sweep. For a fast local loop; CI runs both.
        --full-sweep       Add 1mb.md's and 5mb.md's lines to the sweep:
                           ~40k inputs instead of ~5.6k. The nightly soak.
    normalize [OPTIONS]  Compare xtask/src/html.rs's normalize_html against
                         spec/runner.ts's over every rendered fixture
                         (docs/M2.md §10, owed since M0 decision 12).
        --require-ts       Fail instead of skipping when Node or tsx is
                           unavailable.
    corpus [--check]     Generate bench/corpus/ (§14 step 4); --check verifies
                         the committed files match the generator.
    layout [OPTIONS]     Lay every bench/corpus/ file out at both shipped
                         themes and compare the serialized display list against
                         bench/layout-goldens/, on exact equality
                         (docs/M3.md §5 D10). Verifies the face files against
                         faces.toml's SHA-256s and hard-fails on any codepoint
                         that resolves to tofu before comparing anything.
        --update           Rewrite the goldens from the measured output. The
                           SOLE writer, never implied. A golden update is a
                           reviewable event on the same footing as a parley pin
                           move: see bench/layout-goldens/README.md.
        --only <SUBSTR>    Only corpus files whose name contains SUBSTR.
        --verbose          Print every differing line rather than the first 20,
                           and the per-block-kind census of each input.
        --glyphs           Dump the serialization with one line per glyph to
                           stdout. Debugging only: writes nothing, compares
                           nothing, and requires --only.
        --measure          Print the would-be full serialization size of every
                           input at both themes. This is the measurement the
                           three digest goldens were cut from.
    grammars [OPTIONS]   Regenerate crates/mt-highlight/src/generated.rs from a
                         fully-loaded Prism in the marktext clone (docs/M3.md
                         §5 D14). Emits the ported languages of
                         xtask/src/highlight.rs's PORTED plus everything they
                         reach through `inside`, translating every JS regex to
                         fancy-regex syntax and compiling it before writing.
        --check            Verify the committed file matches a fresh generation
                           instead of rewriting it. What `ci` runs.
        --require-ts       Fail instead of skipping when Prism is unavailable.
    highlight [OPTIONS]  Compare the highlight spans of every bench/corpus/
                         fenced code block against Prism's own tokenization,
                         run from the marktext clone (docs/M3.md §5 D14/D15).
                         Prints the distinct-input count beside the fence count
                         and coverage as N/297, because a fence total is not a
                         coverage number.
        --require-ts       Fail instead of skipping when Prism is unavailable.
        --only <SUBSTR>    Only fences whose label contains SUBSTR.
        --verbose          Print every disagreement rather than the first 20.
    fuzz-seed [--check]  Write fuzz/corpus/<target>/ from the differential
                         sweep's inputs (M1 §5 D6). Not part of `ci`.
    deps                 Enforce the §1 dependency-direction constraints.
    ci                   deps, corpus --check, layout, grammars --check,
                         highlight, divergences, conformance, blocks,
                         normalize, diff — in order.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repo_root();

    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        std::process::exit(1);
    };
    let rest = &args[1..];

    let result = match command {
        "conformance" => conformance::main(&root, rest),
        "blocks" => blocks::main(&root, rest),
        "diff" => diff::main(&root, rest),
        "divergences" => divergences::main(&root, rest),
        "normalize" => normalize::main(&root, rest),
        "corpus" => corpus::main(&root, rest),
        "layout" => layout::main(&root, rest),
        "grammars" => grammars::main(&root, rest),
        "highlight" => highlight::main(&root, rest),
        "fuzz-seed" => fuzz::main(&root, rest),
        "deps" => deps::main(&root),
        "ci" => ci(&root, rest),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(0)
        }
        other => Err(format!("unknown command: {other}\n\n{USAGE}")),
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(message) => {
            eprintln!("xtask: {message}");
            std::process::exit(1);
        }
    }
}

/// Everything CI runs that `cargo test` does not.
///
/// Ordered cheapest-and-most-diagnostic first: an architectural violation or a
/// hand-edited corpus should be reported before spending time on the harnesses
/// that depend on them.
fn ci(root: &Path, rest: &[String]) -> Result<i32, String> {
    // The steps are thunks, not pre-computed results: an array literal of
    // `(label, deps::main(root))` evaluates every step while the array is
    // built, so all the output appears before any of the headers.
    type Step<'a> = (&'a str, Box<dyn Fn() -> Result<i32, String> + 'a>);
    // `divergences` takes `--require-ts` and nothing else `diff` takes, so it
    // gets the flag rather than the whole argument list — passing `rest`
    // through would make `cargo xtask ci --mt-cli <path>` fail inside the
    // register runner for a reason that has nothing to do with the register.
    let engine_flags: Vec<String> = rest
        .iter()
        .filter(|a| a.as_str() == "--require-ts")
        .cloned()
        .collect();

    let steps: Vec<Step<'_>> = vec![
        ("deps", Box::new(|| deps::main(root))),
        (
            "corpus --check",
            Box::new(|| corpus::main(root, &["--check".to_string()])),
        ),
        // Immediately after `corpus --check`, and that adjacency is the point:
        // `bench/layout-goldens/` is generated from `bench/corpus/`, and the
        // committed CJK face is a subset derived from the same corpus, so a
        // hand-edited corpus file should be reported by the generator check
        // before this step spends a minute discovering it as tofu. It is also
        // the slowest step in the chain by a wide margin — see `layout.rs`'s
        // module doc on what the dev profile costs on `5mb.md` — so putting it
        // early means the expensive step is the second thing that reports,
        // rather than the thing everyone waits for at the end.
        ("layout", Box::new(|| layout::main(root, &[]))),
        // Immediately after `layout`, and before the three engine-backed
        // harnesses, for two reasons that pull the same way.
        //
        // It reads `bench/corpus/` through the *same* file list and the same
        // parse options the layout goldens were generated from — literally
        // `layout::inputs` and `layout::parse_options` — so it inherits that
        // adjacency to `corpus --check` rather than re-earning it, and a
        // hand-edited corpus is still reported by the generator check before
        // either step spends time on it.
        //
        // The second reason is why a step that compares **nothing** earns a
        // slot at all. M3 §6 names the hazard S3 is exposed to: *"a
        // differential that runs is indistinguishable in a summary from a
        // differential that was skipped, and both look like a passing stage."*
        // While `mt-highlight` is a stub this step's entire output is the
        // negative control — it proves Node ran, Prism loaded all 297 grammars
        // and spans came back for every fence — and running it on every commit
        // is what stops the harness from silently rotting in the window between
        // being built and being used.
        // Immediately before `highlight`, for the reason `corpus --check` sits
        // before `layout`: `crates/mt-highlight/src/generated.rs` is generated
        // from the reference clone's prismjs, and a tree whose committed
        // grammars do not match a fresh generation should say so *here* rather
        // than as a wall of span disagreements in the step below. A prismjs bump
        // in the clone shows up as this step failing, which names the cause.
        (
            "grammars --check",
            Box::new(|| {
                let mut args = vec!["--check".to_string()];
                args.extend(engine_flags.iter().cloned());
                grammars::main(root, &args)
            }),
        ),
        (
            "highlight",
            Box::new(|| highlight::main(root, &engine_flags)),
        ),
        // Before the two harnesses that will consult it: a malformed register
        // is a register that silently widens what `diff` tolerates, so it
        // should be reported before `diff`'s own output, not after. As of M1 S7
        // this step is also the token-stream differential itself — it compares
        // both engines over the register's inputs and over a sweep of the whole
        // corpus, so it is no longer only a lint on a JSON file.
        (
            "divergences",
            Box::new(|| divergences::main(root, &engine_flags)),
        ),
        ("conformance", Box::new(|| conformance::main(root, &[]))),
        // Before `diff`, and for the reason D6 gives: this one compares the
        // block tree over 1344 inputs and `diff` compares 22 whole documents
        // through an entry point that is still `Err(Unimplemented)`. Until S3
        // wakes `parse`, `blocks` is the only step here that measures the
        // parser at all, so it should report before the step that skips.
        ("blocks", Box::new(|| blocks::main(root, &engine_flags))),
        // After `conformance`, because it renders the same 1,324 examples and
        // a renderer that is broken should be reported by the ratchet rather
        // than by the normaliser it feeds.
        (
            "normalize",
            Box::new(|| normalize::main(root, &engine_flags)),
        ),
        ("diff", Box::new(|| diff::main(root, rest))),
    ];

    let mut worst = 0;
    for (label, run) in steps {
        println!("── xtask {label} ──────────────────────────────────────────────");
        let code = run()?;
        if code != 0 {
            eprintln!("xtask ci: `{label}` failed with exit code {code}");
            worst = worst.max(code);
        }
        println!();
    }
    Ok(worst)
}
