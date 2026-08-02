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
//! | `diff` | Differential test against the TypeScript engine | §11.2 |
//! | `divergences` | The register of intentional differences from muya | M1 §5 D3 |
//! | `corpus` | Generate `bench/corpus/` | §14 step 4 |
//! | `deps` | Enforce the dependency-direction constraints | §1 |
//! | `ci` | All of the above, in order | §9 M0 exit gate |

mod conformance;
mod corpus;
mod deps;
mod diff;
mod divergences;
mod html;

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
    conformance          Run the CommonMark + GFM conformance ratchet (§11.1).
    diff [OPTIONS]       Run the differential test against @muyajs/core (§11.2).
        --require-ts       Fail instead of skipping when the TypeScript engine
                           is unavailable.
        --mt-cli <PATH>    Use a specific mt-cli binary.
        <FILE...>          Compare only these files.
    divergences          Check spec/divergences.json, the register of
                         intentional differences from muya (M1 §5 D3).
    corpus [--check]     Generate bench/corpus/ (§14 step 4); --check verifies
                         the committed files match the generator.
    deps                 Enforce the §1 dependency-direction constraints.
    ci                   deps, corpus --check, divergences, conformance, diff —
                         in order.
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
        "conformance" => conformance::main(&root),
        "diff" => diff::main(&root, rest),
        "divergences" => divergences::main(&root),
        "corpus" => corpus::main(&root, rest),
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
    let steps: Vec<Step<'_>> = vec![
        ("deps", Box::new(|| deps::main(root))),
        (
            "corpus --check",
            Box::new(|| corpus::main(root, &["--check".to_string()])),
        ),
        // Before the two harnesses that will consult it: a malformed register
        // is a register that silently widens what `diff` tolerates, so it
        // should be reported before `diff`'s own output, not after.
        ("divergences", Box::new(|| divergences::main(root))),
        ("conformance", Box::new(|| conformance::main(root))),
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
