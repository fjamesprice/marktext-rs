//! `cargo xtask normalize` — the `normalizeHtml` differential.
//!
//! # The item this discharges
//!
//! M0 decision 12 and M1 §10 both owe this to M2, and docs/M2.md §10 names it
//! as **S5's** rather than as a standing item: when
//! `mt_md::render_to_static_html` returns `Ok`, feed the same HTML through
//! [`crate::html::normalize_html`] and through `spec/runner.ts`'s
//! `normalizeHtml`, and assert agreement over every rendered fixture.
//!
//! `xtask/src/html.rs` has said the same thing in its own header since M0:
//! *"Nothing here can be checked end-to-end until
//! `mt_md::render_to_static_html` produces output. When it does, the cheapest
//! high-confidence check is differential."* Until S5 the Rust normaliser was
//! tested only against hand-written cases, and a hand-written case can only
//! find the bugs someone thought of.
//!
//! # What is checked, and what is emphatically not
//!
//! **Agreement, not correctness.** `normalizeHtml` has two inherited
//! limitations and the Rust port reproduces both **deliberately**:
//!
//! - an attribute value containing `>` breaks tag scanning (the JS `[^>]*?`
//!   has the same blind spot);
//! - the attribute pass does not lower-case tag names (only the void-tag pass
//!   does, and only because it rewrites the name from a fixed list).
//!
//! Fixing either would make the two runners disagree, which is the one thing
//! this check exists to prevent. A "fix" here makes the gate unsatisfiable.
//!
//! # The denominator
//!
//! Every rendered fixture, and the expected HTML beside it: 1,324 examples ×
//! 2. The second half is not padding — the conformance comparison normalises
//! **both** sides, so a disagreement on cmark's output would corrupt a verdict
//! exactly as readily as one on the port's, and cmark's HTML is the half that
//! contains the constructs the port never emits.
//!
//! # Why this is not folded into `cargo xtask conformance`
//!
//! Because `conformance` needs neither Node nor the marktext clone, and that
//! is load-bearing: it is the one harness that runs on a bare CI runner with
//! the committed fixtures and nothing else. This one needs Node and `tsx` —
//! though notably **not** the marktext clone, because `spec/runner.ts` lives
//! in this repository — so it is its own step with its own `--require-ts`,
//! beside the three that already have one.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::conformance::{Suite, load_examples};
use crate::html::normalize_html;

/// `cargo xtask normalize`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut require_ts = false;
    for arg in args {
        match arg.as_str() {
            "--require-ts" => require_ts = true,
            other => return Err(format!("unrecognised argument: {other}")),
        }
    }

    println!("normalizeHtml differential — docs/M2.md §10, owed since M0");

    let spec_dir = repo_root.join("spec");
    let mut labels: Vec<String> = Vec::new();
    let mut inputs: Vec<String> = Vec::new();

    for suite in Suite::ALL {
        for example in load_examples(&spec_dir, suite)? {
            // The port's own output, which is the half M0's owed item names.
            let rendered =
                mt_md::render_to_static_html(&example.markdown, mt_md::Options::SPEC, false)
                    .map_err(|e| format!("{suite} #{}: {e}", example.number))?;
            labels.push(format!("{} #{} rendered", suite.key(), example.number));
            inputs.push(rendered);
            // And cmark's, which the same comparison normalises.
            labels.push(format!("{} #{} expected", suite.key(), example.number));
            inputs.push(example.html.clone());
        }
    }

    println!(
        "  inputs            {} ({} rendered, {} expected)",
        inputs.len(),
        inputs.len() / 2,
        inputs.len() / 2
    );

    let typescript = match normalize_with_typescript(repo_root, &inputs)? {
        Ok(values) => values,
        Err(reason) => {
            if require_ts {
                return Err(format!(
                    "the TypeScript normaliser is unavailable and --require-ts was passed: {reason}"
                ));
            }
            println!("  SKIPPED           {reason}");
            println!("  note              --require-ts makes this a failure instead.");
            return Ok(0);
        }
    };

    if typescript.len() != inputs.len() {
        return Err(format!(
            "normalize-ts-html.mjs returned {} values for {} inputs",
            typescript.len(),
            inputs.len()
        ));
    }

    let mut disagreements = Vec::new();
    for ((label, input), ts) in labels.iter().zip(&inputs).zip(&typescript) {
        let rust = normalize_html(input);
        if &rust != ts {
            disagreements.push((label.clone(), input.clone(), rust, ts.clone()));
        }
    }

    if disagreements.is_empty() {
        println!(
            "  {}/{} agree — the Rust port reproduces runner.ts's normaliser, \
             inherited limitations included",
            inputs.len(),
            inputs.len()
        );
        return Ok(0);
    }

    println!(
        "  FAIL              {} of {} disagree",
        disagreements.len(),
        inputs.len()
    );
    for (label, input, rust, ts) in disagreements.iter().take(20) {
        println!("    {label}");
        println!("      input       {input:?}");
        println!("      rust        {rust:?}");
        println!("      typescript  {ts:?}");
    }
    if disagreements.len() > 20 {
        println!("    … and {} more", disagreements.len() - 20);
    }
    println!(
        "  note              the gate is that the two AGREE. xtask/src/html.rs\n\
         \x20                   reproduces two runner.ts bugs on purpose; do not\n\
         \x20                   'fix' either of them to close a disagreement."
    );
    Ok(1)
}

/// Run every input through `spec/runner.ts`'s `normalizeHtml`, in one process.
///
/// The outer `Result` is "did the harness itself work"; the inner one is "was
/// Node available". Same split as `diff.rs`'s, and for the same reason.
fn normalize_with_typescript(
    repo_root: &Path,
    inputs: &[String],
) -> Result<Result<Vec<String>, String>, String> {
    let script = repo_root
        .join("tools")
        .join("diff")
        .join("normalize-ts-html.mjs");
    let dir = std::env::temp_dir();
    let inputs_path: PathBuf = dir.join(format!("mt-normalize-in-{}.json", std::process::id()));
    let out_path: PathBuf = dir.join(format!("mt-normalize-out-{}.json", std::process::id()));
    std::fs::write(
        &inputs_path,
        serde_json::to_vec(inputs).map_err(|e| format!("cannot encode inputs: {e}"))?,
    )
    .map_err(|e| format!("cannot write {}: {e}", inputs_path.display()))?;

    let output = Command::new("node")
        .current_dir(repo_root)
        .arg("--import")
        .arg("tsx")
        .arg(&script)
        .arg("--inputs")
        .arg(&inputs_path)
        .arg("--out")
        .arg(&out_path)
        .output();

    let result = (|| {
        let output = match output {
            Ok(o) => o,
            Err(e) => {
                return Ok(Err(format!(
                    "cannot run `node`: {e}. Install Node 20.19+ and run `pnpm install` in this repo."
                )));
            }
        };
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            // A missing `tsx` looks exactly like this and is the common case
            // on a machine that has Node but has not run `pnpm install`.
            return Ok(Err(format!(
                "normalize-ts-html.mjs exited {}: {stderr}",
                output.status.code().unwrap_or(-1)
            )));
        }
        let text = std::fs::read_to_string(&out_path)
            .map_err(|e| format!("cannot read {}: {e}", out_path.display()))?;
        let values: Vec<String> = serde_json::from_str(&text)
            .map_err(|e| format!("normalize-ts-html.mjs produced invalid JSON: {e}"))?;
        Ok(Ok(values))
    })();

    let _ = std::fs::remove_file(&inputs_path);
    let _ = std::fs::remove_file(&out_path);
    result
}
