//! The **block-structure** differential harness (docs/M2.md §6 S1 and S2).
//!
//! ```text
//! input ──┬──► node → @muyajs/core MarkdownToState → TState[] ──┐
//!         │                                                      ├──► compare
//!         └──► mt_md::block::parse_blocks → mt_md::state ────────┘
//! ```
//!
//! # What this is, and what it is not
//!
//! [`diff.rs`](crate::diff) is RUST-REWRITE-PLAN.md §11.2's harness: 22 whole
//! **documents**, driven through `mt-cli --dump-state`, gated on
//! `mt_md::parse` returning `Ok` and therefore skipped until S3.
//! [`tokens.rs`](crate::tokens) is M1's, one layer *down*, and compares the
//! token stream of one leaf block.
//!
//! This one is between them. It compares the **block tree** — names and `meta`
//! at S1, names, `meta` and leaf text at S2 — over §4 C1's 1344 inputs, and it
//! exists because S1 and S2 need a gate before `parse` is allowed to answer.
//! D6 stages the three ratchets by entry point and the first of them wakes at
//! S3; a stage with no gate until then would be two stages of unmeasured work.
//!
//! # It is not new plumbing, and M2.md §6 says so twice
//!
//! `tools/diff/dump-ts-state.mjs` already ran `MarkdownToState` over N inputs
//! in one process; S1 added `--inputs` so the N can be spec examples rather
//! than files, which is thirty lines. [`crate::diff::first_difference`]
//! already reports a JSON path like `[3].children[0].meta.lang`.
//! [`crate::divergences`] already loads the register and filters it by layer.
//! `xtask/src/tokens.rs` is the worked example of the comparison shape. **The
//! new code here is the input set and the reporting, not a harness** — M1's S5
//! deferred a comparator four times on the belief that it meant answering
//! questions that had already been answered.
//!
//! # Why it calls `mt_md` in-process rather than `mt-cli`
//!
//! `mt_md::parse` must keep returning `Err(Unimplemented)` until S3, so the
//! block mapping needs a surface that is not `parse`. Three were possible: a
//! second `mt-cli` flag, a temporary entry point on the crate, or a direct
//! call. It is a direct call to [`mt_md::block::parse_blocks`], because:
//!
//! - `xtask` already depends on `mt-md` and on `mt-inline`, and `tokens.rs`
//!   already compares in-process for the same reason — a second process per
//!   input is 1344 process spawns to answer a question that needs none.
//! - An `mt-cli` flag would be a **surface to delete at S3**, and D6's whole
//!   point is that S3 is the commit where the entry points change. A function
//!   `parse` will call is not.
//! - `parse_blocks` is not scaffolding: at S3, `parse` is this plus leaf text
//!   and the label-map pass. S3 folds the harness in by pointing `dump_state`
//!   at the same tree, and this runner keeps working unchanged.
//!
//! # What S1 compares, said out loud rather than quietly excluded
//!
//! Every leaf `parse_blocks` builds carries the **empty string**: §4 C2
//! measured that a leaf's text is a per-kind reconstruction and D2's rules are
//! S2's. So at S1 the `text` field is projected out of **both** sides before
//! they are compared, and the runner prints that it did. `--with-text` turns
//! the projection off, which is how S2 will run it; today it reports the
//! 4,419-leaf gap C2 measured rather than pretending there is none.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Exit code the dumper uses for "the reference engine is not available".
/// Same convention as `dump-ts-state.mjs` and `dump-ts-tokens.mjs`.
const EXIT_UNIMPLEMENTED: i32 = 3;

/// Which option set an input is parsed with — `mt_md::Options`' two constants,
/// named rather than spelled out so the two engines cannot be driven with
/// settings that differ in a field nobody compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSet {
    /// `Options::MUYA_DEFAULT` — math and front matter on.
    Muya,
    /// `Options::SPEC` — every muya extension off, matching
    /// `commonmark.spec.ts` and `gfm.spec.ts`.
    Spec,
}

impl OptionSet {
    fn wire(self) -> &'static str {
        match self {
            OptionSet::Muya => "muya",
            OptionSet::Spec => "spec",
        }
    }

    fn options(self) -> mt_md::Options {
        match self {
            OptionSet::Muya => mt_md::Options::MUYA_DEFAULT,
            OptionSet::Spec => mt_md::Options::SPEC,
        }
    }
}

/// One comparison input, with where it came from so a failure names it.
#[derive(Debug, Clone)]
pub struct Input {
    pub source: String,
    pub options: OptionSet,
    /// `commonmark#42`, `gfm#317`, `marktext-round-trip/common/Links.md`.
    pub label: String,
}

/// §4 C1's comparison set: 1344 inputs from four sources.
///
/// | Source | Options | Count |
/// |---|---|---:|
/// | CommonMark 0.31 fixtures | `SPEC` | 652 |
/// | GFM 0.29-gfm fixtures | `SPEC` | 672 |
/// | `spec/fixtures/marktext-round-trip/` | `MUYA_DEFAULT` | 11 |
/// | `bench/corpus/`, the files ≤ 300 KB | `MUYA_DEFAULT` | 9 |
///
/// The two large generated corpus files are excluded by size and **said so**
/// rather than dropped: `1mb.md` and `5mb.md` are 1 MB and 5 MB of generated
/// markdown, and running the reference engine over them in this loop costs
/// minutes to re-measure a shape the nine smaller files already cover. They
/// are `cargo xtask diff`'s at S3, which runs whole documents.
pub const CORPUS_SIZE_LIMIT: u64 = 300 * 1024;

pub fn collect_inputs(repo_root: &Path) -> Result<Vec<Input>, String> {
    let spec_dir = repo_root.join("spec");
    let mut inputs = Vec::new();

    for (suite, fixture) in [
        ("commonmark", "fixtures/commonmark-spec-0.31.json"),
        ("gfm", "fixtures/gfm-spec-0.29-gfm.json"),
    ] {
        let path = spec_dir.join(fixture);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let examples: Vec<Value> =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        for example in examples {
            let number = example
                .get("number")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{}: an example has no `number`", path.display()))?;
            let markdown = example
                .get("markdown")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{}: example {number} has no `markdown`", path.display()))?;
            inputs.push(Input {
                source: markdown.to_string(),
                options: OptionSet::Spec,
                label: format!("{suite}#{number}"),
            });
        }
    }

    let mut documents = Vec::new();
    collect_markdown(
        &spec_dir.join("fixtures").join("marktext-round-trip"),
        &mut documents,
    )?;
    collect_markdown(&repo_root.join("bench").join("corpus"), &mut documents)?;
    documents.sort();
    for path in documents {
        let size = std::fs::metadata(&path)
            .map_err(|e| format!("cannot stat {}: {e}", path.display()))?
            .len();
        if size > CORPUS_SIZE_LIMIT {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        inputs.push(Input {
            // muya's file layer strips a BOM and normalises CRLF before the
            // parser sees a document; `dump-ts-state.mjs` does the same on its
            // side, so this is the two engines seeing one string.
            source: source.trim_start_matches('\u{feff}').replace("\r\n", "\n"),
            options: OptionSet::Muya,
            label: relative(repo_root, &path),
        });
    }

    Ok(inputs)
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?
    {
        let path = entry
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_markdown(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            // `bench/corpus/README.md` documents the corpus; it is not part of
            // it. Same exclusion, and the same reason, as `diff.rs`.
            let readme = path
                .file_stem()
                .is_some_and(|s| s.eq_ignore_ascii_case("README"));
            if !readme {
                out.push(path);
            }
        }
    }
    Ok(())
}

fn relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

// ---------------------------------------------------------------------------
// The two sides
// ---------------------------------------------------------------------------

fn scratch(repo_root: &Path) -> Result<PathBuf, String> {
    let dir = repo_root.join("target").join("xtask");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Ask muya for every input's state in one process.
///
/// `Ok(Err(reason))` is "the reference engine was not available" — a skip
/// unless `--require-ts`. Same split, and the same reason for it, as
/// [`crate::diff`] and [`crate::tokens`].
fn dump_ts_states(
    repo_root: &Path,
    inputs: &[Input],
) -> Result<Result<Vec<Value>, String>, String> {
    let dir = scratch(repo_root)?;
    let inputs_path = dir.join("block-inputs.json");
    let out_path = dir.join("block-dump-ts.json");

    let payload = Value::Array(
        inputs
            .iter()
            .map(
                |input| serde_json::json!({ "src": input.source, "options": input.options.wire() }),
            )
            .collect(),
    );
    std::fs::write(
        &inputs_path,
        serde_json::to_vec(&payload).map_err(|e| format!("cannot serialize inputs: {e}"))?,
    )
    .map_err(|e| format!("cannot write {}: {e}", inputs_path.display()))?;

    let script = repo_root
        .join("tools")
        .join("diff")
        .join("dump-ts-state.mjs");
    let output = match Command::new("node")
        .current_dir(repo_root)
        .arg("--import")
        .arg("tsx")
        .arg(&script)
        .arg("--inputs")
        .arg(&inputs_path)
        .arg("--out")
        .arg(&out_path)
        .output()
    {
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

    let text = std::fs::read_to_string(&out_path)
        .map_err(|e| format!("cannot read {}: {e}", out_path.display()))?;
    let envelope: Value = serde_json::from_str(&text)
        .map_err(|e| format!("dump-ts-state.mjs produced invalid JSON: {e}"))?;
    let results = envelope
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| "dump-ts-state.mjs envelope has no `results` array".to_string())?;
    if results.len() != inputs.len() {
        return Err(format!(
            "dump-ts-state.mjs returned {} results for {} inputs",
            results.len(),
            inputs.len()
        ));
    }
    Ok(Ok(results.clone()))
}

/// The Rust side: the block tree as muya's `TState[]`.
fn rust_state(input: &Input) -> Value {
    mt_md::state::to_state(&mt_md::block::parse_blocks(
        &input.source,
        input.options.options(),
    ))
}

/// Remove every leaf's `text` from a state tree.
///
/// S1 builds leaves with the empty string, so leaf text is not part of the
/// comparison **yet**. Projecting it out of both sides is the honest form of
/// that: the alternative — comparing `""` against muya's real text — would
/// report 4,419 disagreements that say nothing about block structure and would
/// bury the ones that do.
///
/// S2 deletes the call, not the function: `--with-text` is how the same runner
/// reports the leaf-text gap today.
fn strip_text(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(strip_text),
        Value::Object(object) => {
            object.remove("text");
            if let Some(children) = object.get_mut("children") {
                strip_text(children);
            }
        }
        _ => {}
    }
}

/// How many leaves a tree has, and how many of them differ — the S2 number,
/// reported at S1 so the gap has a size rather than a promise.
fn leaf_text_gap(ts: &Value, rs: &Value) -> (usize, usize) {
    fn walk(ts: &Value, rs: &Value, leaves: &mut usize, differing: &mut usize) {
        match (ts, rs) {
            (Value::Array(a), Value::Array(b)) => {
                for (x, y) in a.iter().zip(b) {
                    walk(x, y, leaves, differing);
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                if let (Some(x), Some(y)) = (a.get("text"), b.get("text")) {
                    *leaves += 1;
                    if x != y {
                        *differing += 1;
                    }
                }
                if let (Some(x), Some(y)) = (a.get("children"), b.get("children")) {
                    walk(x, y, leaves, differing);
                }
            }
            _ => {}
        }
    }
    let (mut leaves, mut differing) = (0, 0);
    walk(ts, rs, &mut leaves, &mut differing);
    (leaves, differing)
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Agree,
    /// Disagreed on a **registered** block divergence. Rule 1: expected.
    Registered,
    /// Disagreed at a `name`. Never tolerable unregistered — S1's gate.
    NameDisagreement {
        at: String,
        ts: String,
        rs: String,
    },
    /// Disagreed at a `meta` field.
    MetaDisagreement {
        at: String,
        ts: String,
        rs: String,
    },
    /// Disagreed at a leaf's `text`. Only reachable under `--with-text`; this
    /// is S2's gate and S1 reports the count rather than the field.
    TextDisagreement {
        at: String,
        ts: String,
        rs: String,
    },
    /// muya threw. A finding recorded against its input, not a harness bug.
    EngineThrew(String),
}

impl Outcome {
    fn tag(&self) -> &'static str {
        match self {
            Outcome::Agree => "OK  ",
            Outcome::Registered => "DIVG",
            Outcome::NameDisagreement { .. } => "NAME",
            Outcome::MetaDisagreement { .. } => "META",
            Outcome::TextDisagreement { .. } => "TEXT",
            Outcome::EngineThrew(_) => "THRW",
        }
    }
}

/// One input's outcome: agreement, a registered divergence, or a
/// disagreement classified by which gate it belongs to.
///
/// **Extracted so the register's block layer is testable.** The layer was
/// wired at S0 and S1 registered nothing in it, so on every real run this
/// function's `Registered` arm is dead code — and *"a check that cannot fail
/// is not a check"* applies to a tolerance just as much as to an assertion. A
/// tolerance nobody has seen fire is a tolerance that might be spelled wrong.
/// `tests::a_registered_block_divergence_is_tolerated_and_an_unregistered_one_is_not`
/// drives both branches.
fn decide(
    ts: &Value,
    rs: &Value,
    source: &str,
    tolerated: &std::collections::BTreeSet<&str>,
) -> Outcome {
    match crate::diff::first_difference(ts, rs) {
        None => Outcome::Agree,
        // Rule 1: a disagreement on a registered input is expected; a
        // disagreement on anything else is a failure.
        Some(_) if tolerated.contains(source) => Outcome::Registered,
        Some((at, ts, rs)) => classify(&at)(at.clone(), ts, rs),
    }
}

/// Which gate a JSON path from `first_difference` belongs to.
///
/// Three classes, because S1's gate and S2's are stated over different ones:
///
/// - **`.text`** is S2's, and is the only class `--with-text` can add.
/// - **`.meta.…`** is a metadata field. S1's gate tolerates a *registered* one
///   and nothing else.
/// - **everything else** is a name: `[0].name` says the wrong block was built,
///   and `.length` says the wrong number of them was. Those are the same
///   claim, and the classification is deliberately conservative in the
///   direction of calling something a name — S1's gate is that no name
///   disagreement survives unregistered, so a misclassification must not be
///   able to hide one.
fn classify(path: &str) -> fn(String, String, String) -> Outcome {
    if path.ends_with(".text") {
        |at, ts, rs| Outcome::TextDisagreement { at, ts, rs }
    } else if path.contains(".meta.") {
        |at, ts, rs| Outcome::MetaDisagreement { at, ts, rs }
    } else {
        |at, ts, rs| Outcome::NameDisagreement { at, ts, rs }
    }
}

pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut require_ts = false;
    let mut with_text = false;
    let mut verbose = false;
    let mut only: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--require-ts" => require_ts = true,
            "--with-text" => with_text = true,
            "--verbose" => verbose = true,
            "--only" => {
                i += 1;
                only = Some(args.get(i).ok_or("--only requires a substring")?.clone());
            }
            other => return Err(format!("unrecognised argument: {other}")),
        }
        i += 1;
    }

    println!("block-structure differential harness — docs/M2.md §6 S1");

    let mut inputs = collect_inputs(repo_root)?;
    if let Some(filter) = &only {
        inputs.retain(|input| input.label.contains(filter.as_str()));
    }
    let spec_count = inputs
        .iter()
        .filter(|i| i.options == OptionSet::Spec)
        .count();
    println!(
        "inputs: {} ({spec_count} spec examples at SPEC, {} documents at MUYA_DEFAULT)",
        inputs.len(),
        inputs.len() - spec_count
    );
    if with_text {
        println!("comparing names, meta AND leaf text (--with-text)");
    } else {
        println!(
            "comparing names and meta. **Leaf text is excluded**: S1 builds every leaf with\n\
             the empty string and §4 C2's reconstruction is S2's. Run --with-text to see the\n\
             gap; it is not hidden, it is not yet closed."
        );
    }

    // The register's block layer (M2.md §5 D4). Loaded unconditionally so a
    // malformed register fails this runner too, whichever layer is broken.
    let register = crate::divergences::load(&repo_root.join("spec"))?;
    let problems = crate::divergences::validate(&register);
    let block_entries =
        crate::divergences::entries_for(&register, crate::divergences::Layer::Block);
    let tolerated =
        crate::divergences::registered_inputs_for(&register, crate::divergences::Layer::Block);
    println!(
        "register: {} block entr{}, {} tolerated input(s)",
        block_entries.len(),
        if block_entries.len() == 1 { "y" } else { "ies" },
        tolerated.len()
    );
    for problem in &problems {
        println!("  FAIL  register is malformed: {problem}");
    }
    println!();

    if inputs.is_empty() {
        println!("nothing to compare");
        return Ok(if problems.is_empty() { 0 } else { 1 });
    }

    let ts_states = match dump_ts_states(repo_root, &inputs)? {
        Ok(states) => states,
        Err(reason) => {
            if require_ts {
                return Err(format!(
                    "the TypeScript reference engine is unavailable and --require-ts was given:\n{reason}"
                ));
            }
            println!("TypeScript engine unavailable — nothing was compared.");
            for line in reason.lines() {
                println!("  {line}");
            }
            println!("Pass --require-ts to make that a failure instead of a skip; CI does.");
            return Ok(if problems.is_empty() { 0 } else { 1 });
        }
    };

    let mut outcomes = Vec::with_capacity(inputs.len());
    let (mut leaves, mut differing_leaves) = (0usize, 0usize);

    for (input, result) in inputs.iter().zip(&ts_states) {
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            outcomes.push(Outcome::EngineThrew(error.to_string()));
            continue;
        }
        let Some(ts) = result.get("state") else {
            return Err("a result has neither `state` nor `error`".to_string());
        };
        let mut ts = ts.clone();
        let mut rs = rust_state(input);

        let (found, differed) = leaf_text_gap(&ts, &rs);
        leaves += found;
        differing_leaves += differed;

        if !with_text {
            strip_text(&mut ts);
            strip_text(&mut rs);
        }

        outcomes.push(decide(&ts, &rs, &input.source, &tolerated));
    }

    let mut names_disagreeing = Vec::new();
    let mut metas_disagreeing = Vec::new();
    let mut texts_disagreeing = Vec::new();
    let mut threw = Vec::new();
    let mut registered = 0;
    for (input, outcome) in inputs.iter().zip(&outcomes) {
        match outcome {
            Outcome::Agree => {}
            Outcome::Registered => registered += 1,
            Outcome::NameDisagreement { .. } => names_disagreeing.push((input, outcome)),
            Outcome::MetaDisagreement { .. } => metas_disagreeing.push((input, outcome)),
            Outcome::TextDisagreement { .. } => texts_disagreeing.push((input, outcome)),
            Outcome::EngineThrew(_) => threw.push((input, outcome)),
        }
    }

    let report = |title: &str, rows: &[(&Input, &Outcome)], limit: usize| {
        if rows.is_empty() {
            return;
        }
        println!("{title} ({}):", rows.len());
        for (input, outcome) in rows.iter().take(limit) {
            println!("  {}  {}", outcome.tag(), input.label);
            match outcome {
                Outcome::NameDisagreement { at, ts, rs }
                | Outcome::MetaDisagreement { at, ts, rs }
                | Outcome::TextDisagreement { at, ts, rs } => {
                    println!("        at {at}");
                    println!("        typescript: {ts}");
                    println!("        rust:       {rs}");
                }
                Outcome::EngineThrew(message) => println!("        {message}"),
                _ => {}
            }
        }
        if rows.len() > limit {
            // No silent caps: say how many were not printed.
            println!("  … {} more not printed (--verbose)", rows.len() - limit);
        }
        println!();
    };

    let limit = if verbose { usize::MAX } else { 20 };
    report("name disagreements", &names_disagreeing, limit);
    report("meta disagreements", &metas_disagreeing, limit);
    report(
        "leaf-text disagreements — S2's gate",
        &texts_disagreeing,
        limit,
    );
    report("the reference engine threw", &threw, limit);

    let agreeing = outcomes.iter().filter(|o| **o == Outcome::Agree).count();
    println!(
        "  {agreeing}/{} agree, {registered} registered, {} name, {} meta, {} leaf-text \
         disagreement(s), {} engine throw(s)",
        inputs.len(),
        names_disagreeing.len(),
        metas_disagreeing.len(),
        texts_disagreeing.len(),
        threw.len()
    );
    // A run of 1344 agreements over 1344 empty trees would print the same line
    // as a run over 1344 real ones, so the size of what was compared is
    // printed beside it. `leaves` is counted on the **TypeScript** side, which
    // makes it a statement about the reference engine's output rather than
    // about the port's — the cheapest form of the negative control M1 S6 asks
    // every harness for.
    println!(
        "  {leaves} leaves compared across {} inputs; {differing_leaves} differ in text, \
         which is {}",
        inputs.len(),
        if with_text {
            "in the comparison above"
        } else {
            "S2's gate and not compared here"
        }
    );

    // §4 C1's measurement of an *unmapped* `pulldown-cmark`, kept as a floor.
    // A stage that lowers it has done something wrong and nothing else would
    // say so (M2.md §6, "On the three numbers to beat").
    const C1_FLOOR: usize = 1167;
    if inputs.len() == 1344 {
        let names_agreeing = inputs.len() - names_disagreeing.len();
        println!(
            "  names: {names_agreeing}/1344 — §4 C1 measured {C1_FLOOR}/1344 before any mapping"
        );
        if names_agreeing < C1_FLOOR {
            println!(
                "  FAIL  below C1's floor of {C1_FLOOR}. That number is what an *unmapped*\n\
                 \x20       pulldown-cmark scored, so a mapping layer under it has broken\n\
                 \x20       something the mapping layer was not supposed to touch."
            );
        }
    }

    // **The strict reading of S1's gate, which the measurement made
    // affordable.** §6's S1 row says *"every residual disagreement is either
    // registered or a `meta` field, and no *name* disagreement survives
    // unregistered"* — which tolerates an unregistered `meta` disagreement.
    // The brief's own statement of the same gate does not: *"every residual
    // disagreement is either a registered block divergence or nothing"*. S1
    // measured **zero** of either, so the stricter reading costs nothing to
    // adopt and is what is enforced here; the looser one would leave a class
    // that can grow without failing anything.
    //
    // Leaf text is a failure only under `--with-text`, which is S2's gate.
    let failed = !names_disagreeing.is_empty()
        || !metas_disagreeing.is_empty()
        || !threw.is_empty()
        || !problems.is_empty()
        || (with_text && !texts_disagreeing.is_empty())
        || (inputs.len() == 1344 && inputs.len() - names_disagreeing.len() < C1_FLOOR);
    Ok(i32::from(failed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// §4 C1's input set, at the size C1 measured. If a fixture is added or a
    /// corpus file crosses the size limit, the gate stops being comparable
    /// with the 1167/1344 this stage is measured against.
    #[test]
    fn the_input_set_is_c1s_1344() {
        let inputs = collect_inputs(&crate::repo_root()).expect("collect");
        assert_eq!(inputs.len(), 1344, "§4 C1's comparison set is 1344 inputs");
        let spec = inputs
            .iter()
            .filter(|i| i.options == OptionSet::Spec)
            .count();
        assert_eq!(spec, 1324, "652 CommonMark + 672 GFM at SPEC");
        assert_eq!(inputs.len() - spec, 20, "11 round-trip fixtures + 9 corpus");
    }

    /// The two generated megabyte corpus files are the ones excluded, and no
    /// others.
    #[test]
    fn only_the_two_large_corpus_files_are_left_out() {
        let inputs = collect_inputs(&crate::repo_root()).expect("collect");
        let labels: Vec<&str> = inputs.iter().map(|i| i.label.as_str()).collect();
        assert!(!labels.iter().any(|l| l.ends_with("1mb.md")));
        assert!(!labels.iter().any(|l| l.ends_with("5mb.md")));
        assert!(labels.iter().any(|l| l.ends_with("250kb.md")));
        assert!(labels.iter().any(|l| l.ends_with("empty.md")));
    }

    #[test]
    fn stripping_text_leaves_names_meta_and_children() {
        let mut value = json!([{
            "name": "block-quote",
            "children": [{ "name": "paragraph", "text": "q" }],
        }]);
        strip_text(&mut value);
        assert_eq!(
            value,
            json!([{ "name": "block-quote", "children": [{ "name": "paragraph" }] }])
        );
    }

    /// The classification S1's gate rests on: a `name` disagreement is never
    /// tolerable unregistered, and `meta` and `text` are separate counts.
    #[test]
    fn paths_are_classified_into_the_three_gates() {
        let tag = |path: &str| {
            classify(path)(String::new(), String::new(), String::new())
                .tag()
                .trim()
                .to_string()
        };
        assert_eq!(tag("[0].name"), "NAME");
        assert_eq!(tag(".length"), "NAME");
        assert_eq!(tag("[0].children.length"), "NAME");
        assert_eq!(tag("[0].meta.lang"), "META");
        assert_eq!(tag("[3].children[0].meta.fenceLength"), "META");
        assert_eq!(tag("[0].text"), "TEXT");
        assert_eq!(tag("[0].children[1].text"), "TEXT");
    }

    /// **The negative control for the register's block layer.** S1 registered
    /// no block divergence, so nothing on a real run exercises the `Registered`
    /// arm; without this, "0 registered" and "the tolerance is broken" print
    /// the same line. Both branches are driven here, on the same disagreement.
    #[test]
    fn a_registered_block_divergence_is_tolerated_and_an_unregistered_one_is_not() {
        let ts = json!([{ "name": "paragraph" }]);
        let rs = json!([{ "name": "html-block" }]);
        let registered: std::collections::BTreeSet<&str> = ["<img a>\n"].into_iter().collect();
        let none = std::collections::BTreeSet::new();

        assert_eq!(
            decide(&ts, &rs, "<img a>\n", &registered),
            Outcome::Registered
        );
        assert!(matches!(
            decide(&ts, &rs, "<img a>\n", &none),
            Outcome::NameDisagreement { .. }
        ));
        // And a *registered* input that agrees is still an agreement — the
        // tolerance widens what a disagreement means, not what a run reports.
        assert_eq!(
            decide(&ts, &ts.clone(), "<img a>\n", &registered),
            Outcome::Agree
        );
    }

    /// A tree that differs only in leaf text agrees once the text is
    /// projected out — which is what makes S1's exclusion a projection rather
    /// than a blind spot.
    #[test]
    fn the_leaf_text_gap_is_counted_even_when_text_is_not_compared() {
        let ts = json!([{ "name": "paragraph", "text": "hello" }]);
        let rs = json!([{ "name": "paragraph", "text": "" }]);
        assert_eq!(leaf_text_gap(&ts, &rs), (1, 1));
        let (mut a, mut b) = (ts, rs);
        strip_text(&mut a);
        strip_text(&mut b);
        assert_eq!(crate::diff::first_difference(&a, &b), None);
    }
}
