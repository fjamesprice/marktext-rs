//! The differential-test harness (RUST-REWRITE-PLAN.md §11.2).
//!
//! ```text
//! corpus file ──┬──► node harness → @muyajs/core → state JSON ──┐
//!               │                                                ├──► assert equal
//!               └──► mt-md (via mt-cli --dump-state) ────────────┘
//! ```
//!
//! # Why this is the highest-value test asset the project has
//!
//! It exists only because §0 keeps the state shape identical between the two
//! engines: muya's `TState` union and `mt_doc::Block` map 1:1, so both sides
//! can emit the same JSON and be compared structurally.
//!
//! What that buys is a change of kind, not of degree. "Did I port the 898-line
//! lexer correctly?" is a judgement call — you read the diff, you squint, you
//! decide. Run over the whole corpus on every commit, it becomes a boolean.
//! §13 R1 ("lexer port diverges subtly; markdown renders *almost* right")
//! names this as the mitigation, and it is why §14 step 3 puts building this
//! **before** writing any of `mt-inline`.
//!
//! Extend it to serialized markdown and exported HTML as those land.
//!
//! # M0 status
//!
//! The TypeScript side works today. The Rust side (`mt-cli --dump-state`)
//! exits 3 = UNIMPLEMENTED, which this runner reports as SKIPPED rather than
//! FAILED, so CI is green and honest. When `mt-md` lands the exit code becomes
//! 0 and this runner starts comparing — with no change to this file.
//!
//! # Skip semantics, and why they are not a loophole
//!
//! Two things can be missing, and they are reported distinctly:
//!
//! - **Rust engine unimplemented** (`mt-cli` exits 3). Expected at M0. Every
//!   file is SKIPPED.
//! - **TypeScript engine unavailable** (no marktext clone, or its deps are not
//!   installed; the dumper exits 3). Also SKIPPED — but pass `--require-ts` to
//!   make it a hard failure instead. CI passes that flag on the one job that
//!   is configured with a marktext checkout, so a silently-missing reference
//!   engine cannot quietly turn the whole harness into a no-op.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Per-file outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Both engines produced state and it matched.
    Pass,
    /// Both engines produced state and it differed. Carries the JSON path of
    /// the first disagreement.
    Fail { at: String, ts: String, rs: String },
    /// One side is not available. Not a failure.
    Skip { reason: String },
    /// A side errored in a way that is not "not implemented".
    Error { message: String },
}

impl Outcome {
    pub fn is_failure(&self) -> bool {
        matches!(self, Outcome::Fail { .. } | Outcome::Error { .. })
    }

    fn tag(&self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail { .. } => "FAIL",
            Outcome::Skip { .. } => "SKIP",
            Outcome::Error { .. } => "ERR ",
        }
    }
}

/// One file's result.
#[derive(Debug, Clone)]
pub struct FileResult {
    pub path: PathBuf,
    pub outcome: Outcome,
}

/// Locate the first JSON path at which two values disagree.
///
/// Returns `None` when they are equal. The path is dotted with bracketed
/// indices — `[3].children[0].meta.lang` — so a report line points at the
/// exact block and field rather than dumping two multi-megabyte trees.
pub fn first_difference(
    ts: &serde_json::Value,
    rs: &serde_json::Value,
) -> Option<(String, String, String)> {
    fn walk(
        ts: &serde_json::Value,
        rs: &serde_json::Value,
        path: &mut String,
    ) -> Option<(String, String, String)> {
        use serde_json::Value;
        match (ts, rs) {
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    return Some((
                        format!("{path}.length"),
                        a.len().to_string(),
                        b.len().to_string(),
                    ));
                }
                for (i, (x, y)) in a.iter().zip(b).enumerate() {
                    let mark = path.len();
                    path.push_str(&format!("[{i}]"));
                    if let Some(found) = walk(x, y, path) {
                        return Some(found);
                    }
                    path.truncate(mark);
                }
                None
            }
            (Value::Object(a), Value::Object(b)) => {
                // Keys are compared as a set first, so a missing or extra
                // field is reported as such rather than as a value mismatch.
                for key in a.keys() {
                    if !b.contains_key(key) {
                        return Some((format!("{path}.{key}"), "present".into(), "missing".into()));
                    }
                }
                for key in b.keys() {
                    if !a.contains_key(key) {
                        return Some((format!("{path}.{key}"), "missing".into(), "present".into()));
                    }
                }
                for (key, x) in a {
                    let mark = path.len();
                    path.push('.');
                    path.push_str(key);
                    if let Some(found) = walk(x, &b[key], path) {
                        return Some(found);
                    }
                    path.truncate(mark);
                }
                None
            }
            (x, y) if x == y => None,
            (x, y) => Some((
                if path.is_empty() {
                    "<root>".into()
                } else {
                    path.clone()
                },
                truncate(&x.to_string()),
                truncate(&y.to_string()),
            )),
        }
    }

    let mut path = String::new();
    walk(ts, rs, &mut path)
}

fn truncate(s: &str) -> String {
    const MAX: usize = 160;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let head: String = s.chars().take(MAX).collect();
    format!("{head}… ({} chars)", s.chars().count())
}

/// Exit code `mt-cli` and `dump-ts-state.mjs` both use for "not implemented /
/// not available". Distinct from 1 so this runner can skip rather than fail.
const EXIT_UNIMPLEMENTED: i32 = 3;

/// Find the `mt-cli` binary.
///
/// Looks next to this xtask executable first — under `cargo run -p xtask` that
/// is `target/<profile>/`, so a `cargo build --workspace` puts it exactly
/// there. Falls back to the conventional debug and release paths.
fn find_mt_cli(repo_root: &Path, explicit: Option<&Path>) -> Result<PathBuf, String> {
    let exe = if cfg!(windows) {
        "mt-cli.exe"
    } else {
        "mt-cli"
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        candidates.push(p.to_path_buf());
    }
    if let Ok(current) = std::env::current_exe()
        && let Some(dir) = current.parent()
    {
        candidates.push(dir.join(exe));
    }
    candidates.push(repo_root.join("target").join("debug").join(exe));
    candidates.push(repo_root.join("target").join("release").join(exe));

    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "could not find the mt-cli binary. Tried:\n{}\nRun: cargo build -p mt-cli",
                candidates
                    .iter()
                    .map(|p| format!("  - {}", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
}

/// Collect the corpus.
///
/// Two sources, both markdown, both committed:
///   - `bench/corpus/` — the §14 step 4 corpus, which is also the performance
///     regression suite.
///   - `spec/fixtures/marktext-round-trip/` — the eleven fixtures backported
///     from marktext's own `markdown-basic` tests. Small, hand-written, and
///     dense in exactly the constructs that break round-trips.
fn collect_corpus(repo_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for dir in [
        repo_root.join("bench").join("corpus"),
        repo_root
            .join("spec")
            .join("fixtures")
            .join("marktext-round-trip"),
    ] {
        collect_markdown(&dir, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            // `bench/corpus/README.md` documents the corpus; it is not part of
            // it. Comparing it would still be valid, but it would quietly make
            // the harness's own documentation a test input whose churn shows
            // up as harness churn.
            let is_readme = path
                .file_stem()
                .is_some_and(|s| s.eq_ignore_ascii_case("README"));
            if !is_readme {
                out.push(path);
            }
        }
    }
    Ok(())
}

/// One file's state as the TypeScript engine sees it: either the state tree,
/// or the message from an engine that threw on this input.
type TsState = Result<serde_json::Value, String>;

/// The whole TypeScript-side run: either every file's state, or the reason the
/// engine could not be consulted at all.
type TsRun = Result<Vec<(PathBuf, TsState)>, String>;

/// Ask the Node harness for every file's state in one process, because tsx
/// startup dominates the cost for small files.
///
/// The outer `Result` is "did the harness itself work"; the inner one is "was
/// the reference engine available". They are different failures with different
/// consequences, which is why they are not collapsed.
fn dump_ts_states(repo_root: &Path, files: &[PathBuf]) -> Result<TsRun, String> {
    let script = repo_root
        .join("tools")
        .join("diff")
        .join("dump-ts-state.mjs");
    let mut cmd = Command::new("node");
    cmd.current_dir(repo_root)
        .arg("--import")
        .arg("tsx")
        .arg(&script);
    for f in files {
        cmd.arg(f);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            return Ok(Err(format!(
                "cannot run `node`: {e}. Install Node 20.19+ and run `pnpm install` in this repo."
            )));
        }
    };

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if code == EXIT_UNIMPLEMENTED {
        return Ok(Err(stderr));
    }
    if code != 0 {
        return Err(format!("dump-ts-state.mjs exited {code}:\n{stderr}"));
    }

    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("dump-ts-state.mjs produced invalid JSON: {e}"))?;
    let entries = envelope
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "dump-ts-state.mjs envelope has no `files` array".to_string())?;

    let mut results = Vec::with_capacity(entries.len());
    for entry in entries {
        let path = PathBuf::from(
            entry
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        match entry.get("error").and_then(|v| v.as_str()) {
            Some(err) => results.push((path, Err(err.to_string()))),
            None => {
                let state = entry
                    .get("state")
                    .cloned()
                    .ok_or_else(|| "envelope entry has neither `state` nor `error`".to_string())?;
                results.push((path, Ok(state)));
            }
        }
    }
    Ok(Ok(results))
}

/// Ask `mt-cli --dump-state` for one file's state.
fn dump_rs_state(mt_cli: &Path, file: &Path) -> Result<Option<serde_json::Value>, String> {
    let output = Command::new(mt_cli)
        .arg("--dump-state")
        .arg(file)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", mt_cli.display()))?;

    let code = output.status.code().unwrap_or(-1);
    if code == EXIT_UNIMPLEMENTED {
        return Ok(None);
    }
    if code != 0 {
        return Err(format!(
            "mt-cli exited {code}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map(Some)
        .map_err(|e| format!("mt-cli produced invalid JSON: {e}"))
}

/// `cargo xtask diff`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut require_ts = false;
    let mut explicit_cli: Option<PathBuf> = None;
    let mut explicit_files: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--require-ts" => require_ts = true,
            "--mt-cli" => {
                i += 1;
                explicit_cli = Some(PathBuf::from(
                    args.get(i).ok_or("--mt-cli requires a path")?,
                ));
            }
            other if other.starts_with("--") => {
                return Err(format!("unrecognised argument: {other}"));
            }
            other => explicit_files.push(PathBuf::from(other)),
        }
        i += 1;
    }

    let files = if explicit_files.is_empty() {
        collect_corpus(repo_root)?
    } else {
        explicit_files
    };

    println!("differential harness — RUST-REWRITE-PLAN.md §11.2");
    println!("corpus: {} file(s)", files.len());
    println!();

    if files.is_empty() {
        println!("nothing to compare");
        return Ok(0);
    }

    let mt_cli = find_mt_cli(repo_root, explicit_cli.as_deref())?;

    let ts_states = match dump_ts_states(repo_root, &files)? {
        Ok(states) => Some(states),
        Err(reason) => {
            if require_ts {
                return Err(format!(
                    "the TypeScript reference engine is unavailable and --require-ts was given:\n{reason}"
                ));
            }
            println!("TypeScript engine unavailable — every file will be SKIPPED.");
            for line in reason.lines() {
                println!("  {line}");
            }
            println!();
            None
        }
    };

    let mut results: Vec<FileResult> = Vec::with_capacity(files.len());
    for (index, path) in files.iter().enumerate() {
        let outcome = match &ts_states {
            None => Outcome::Skip {
                reason: "TypeScript engine unavailable".into(),
            },
            Some(states) => {
                let ts = states.get(index).map(|(_, s)| s);
                match ts {
                    None => Outcome::Error {
                        message: "dump-ts-state.mjs returned fewer files than requested".into(),
                    },
                    Some(Err(e)) => Outcome::Error {
                        message: format!("@muyajs/core threw: {e}"),
                    },
                    Some(Ok(ts_state)) => match dump_rs_state(&mt_cli, path) {
                        Err(e) => Outcome::Error { message: e },
                        Ok(None) => Outcome::Skip {
                            reason: "mt-md is unimplemented (M2)".into(),
                        },
                        Ok(Some(rs_state)) => match first_difference(ts_state, &rs_state) {
                            None => Outcome::Pass,
                            Some((at, ts, rs)) => Outcome::Fail { at, ts, rs },
                        },
                    },
                }
            }
        };
        results.push(FileResult {
            path: path.clone(),
            outcome,
        });
    }

    let rel = |p: &Path| -> String {
        p.strip_prefix(repo_root)
            .unwrap_or(p)
            .display()
            .to_string()
            .replace('\\', "/")
    };

    for result in &results {
        println!("  {}  {}", result.outcome.tag(), rel(&result.path));
        match &result.outcome {
            Outcome::Fail { at, ts, rs } => {
                println!("        first difference at {at}");
                println!("        typescript: {ts}");
                println!("        rust:       {rs}");
            }
            Outcome::Error { message } => println!("        {message}"),
            _ => {}
        }
    }

    let passed = results
        .iter()
        .filter(|r| r.outcome == Outcome::Pass)
        .count();
    let failed = results.iter().filter(|r| r.outcome.is_failure()).count();
    let skipped = results.len() - passed - failed;

    println!();
    println!("  {passed} passed, {failed} failed, {skipped} skipped");

    if skipped == results.len() && !results.is_empty() {
        println!();
        // Say which side was missing. "Everything skipped" for the wrong
        // reason is how a harness quietly stops testing anything.
        if ts_states.is_none() {
            println!(
                "Everything skipped because the TypeScript reference engine was not\n\
                 available — nothing was compared. Pass --require-ts to make that a\n\
                 failure instead of a skip; CI does."
            );
        } else {
            println!(
                "Everything skipped. That is the expected M0 state: mt_md::dump_state is a\n\
                 stub until M2. The harness itself is running — when the Rust side starts\n\
                 answering, correctness becomes a boolean with no change to this harness."
            );
        }
    }

    Ok(if failed > 0 { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_trees_have_no_difference() {
        let a = json!([{ "name": "paragraph", "text": "hi" }]);
        assert_eq!(first_difference(&a, &a.clone()), None);
    }

    /// Key order must not matter: both dumpers canonicalise, but a comparator
    /// that depended on order would turn a serialisation detail into a
    /// correctness signal.
    #[test]
    fn key_order_does_not_matter() {
        let a = json!({ "name": "atx-heading", "text": "# x", "meta": { "level": 1 } });
        let b = json!({ "meta": { "level": 1 }, "text": "# x", "name": "atx-heading" });
        assert_eq!(first_difference(&a, &b), None);
    }

    #[test]
    fn array_order_does_matter() {
        let a = json!([{ "name": "paragraph" }, { "name": "table" }]);
        let b = json!([{ "name": "table" }, { "name": "paragraph" }]);
        assert!(first_difference(&a, &b).is_some());
    }

    #[test]
    fn reports_the_path_of_a_scalar_mismatch() {
        let a = json!([{ "name": "atx-heading", "meta": { "level": 1 } }]);
        let b = json!([{ "name": "atx-heading", "meta": { "level": 2 } }]);
        let (at, ts, rs) = first_difference(&a, &b).expect("differ");
        assert_eq!(at, "[0].meta.level");
        assert_eq!(ts, "1");
        assert_eq!(rs, "2");
    }

    #[test]
    fn reports_a_length_mismatch_rather_than_walking_into_it() {
        let a = json!([{ "name": "paragraph" }, { "name": "paragraph" }]);
        let b = json!([{ "name": "paragraph" }]);
        let (at, ts, rs) = first_difference(&a, &b).expect("differ");
        assert_eq!(at, ".length");
        assert_eq!(ts, "2");
        assert_eq!(rs, "1");
    }

    #[test]
    fn reports_a_missing_field_as_missing() {
        let a = json!({ "meta": { "fenceLength": 4 } });
        let b = json!({ "meta": {} });
        let (at, ts, rs) = first_difference(&a, &b).expect("differ");
        assert_eq!(at, ".meta.fenceLength");
        assert_eq!(ts, "present");
        assert_eq!(rs, "missing");
    }

    #[test]
    fn reports_an_extra_field_as_extra() {
        let a = json!({ "meta": {} });
        let b = json!({ "meta": { "fenceLength": 4 } });
        let (at, _, rs) = first_difference(&a, &b).expect("differ");
        assert_eq!(at, ".meta.fenceLength");
        assert_eq!(rs, "present");
    }

    /// The exact class of bug this harness exists to catch: a code fence whose
    /// info string was "helpfully" reduced to its first word.
    #[test]
    fn catches_a_truncated_code_fence_info_string() {
        let ts = json!([{
            "name": "code-block",
            "meta": { "type": "fenced", "lang": "js title=\"x\"" },
            "text": "let a = 1",
        }]);
        let rs = json!([{
            "name": "code-block",
            "meta": { "type": "fenced", "lang": "js" },
            "text": "let a = 1",
        }]);
        let (at, ts_val, rs_val) = first_difference(&ts, &rs).expect("differ");
        assert_eq!(at, "[0].meta.lang");
        assert!(ts_val.contains("title"));
        assert!(!rs_val.contains("title"));
    }

    /// The other one: a reference definition promoted to its own block type
    /// instead of round-tripping as a paragraph.
    #[test]
    fn catches_a_reference_definition_that_grew_its_own_block_type() {
        let ts = json!([{ "name": "paragraph", "text": "[a]: /url \"t\"" }]);
        let rs = json!([{ "name": "link-reference-definition", "text": "[a]: /url \"t\"" }]);
        let (at, ..) = first_difference(&ts, &rs).expect("differ");
        assert_eq!(at, "[0].name");
    }

    #[test]
    fn long_values_are_truncated_in_the_report() {
        let long = "x".repeat(1000);
        let a = json!({ "text": long });
        let b = json!({ "text": "y" });
        let (_, ts, _) = first_difference(&a, &b).expect("differ");
        assert!(
            ts.contains("chars)"),
            "expected a truncation marker, got {ts}"
        );
        assert!(ts.chars().count() < 300);
    }

    #[test]
    fn outcome_failure_classification() {
        assert!(!Outcome::Pass.is_failure());
        assert!(
            !Outcome::Skip {
                reason: String::new()
            }
            .is_failure()
        );
        assert!(
            Outcome::Fail {
                at: String::new(),
                ts: String::new(),
                rs: String::new()
            }
            .is_failure()
        );
        assert!(
            Outcome::Error {
                message: String::new()
            }
            .is_failure()
        );
    }

    /// The corpus must not be empty, or the harness is a very elaborate no-op.
    #[test]
    fn the_corpus_is_not_empty() {
        let files = collect_corpus(&crate::repo_root()).expect("collect");
        assert!(
            files.len() >= 11,
            "expected at least the §14 step 4 corpus, found {}",
            files.len()
        );
    }
}
