//! The divergence register (docs/M1.md §5 D3).
//!
//! # Why this exists
//!
//! M1's verification strategy is agreement with the TypeScript engine, and
//! D3 decides that muya's bugs are **fixed** in the port rather than
//! reproduced bug-for-bug. Every fix is therefore a deliberate disagreement —
//! and without somewhere to record it, *a fixed bug and a botched port look
//! identical in the differential harness*. The fixes are only safe if each one
//! is registered before it is made.
//!
//! `spec/divergences.json` is that register. Four rules, which are what make
//! fixing-and-registering stronger than bug-for-bug rather than weaker:
//!
//! 1. A differential disagreement on a **registered** input is expected. A
//!    disagreement on **any other** input is a failure, exactly as before.
//! 2. Every entry names concrete inputs, and those inputs become Rust tests
//!    asserting the **fixed** behaviour.
//! 3. An entry with **no failing differential case is stale** — the fix is
//!    either unimplemented or the divergence was imaginary. This runner says
//!    so.
//! 4. `upstream` holds the marktext issue URL once filed.
//!
//! Rule 1's second half belongs to `diff.rs` rather than here: when the
//! differential harness gains a token-stream mode it consults
//! [`registered_inputs`] to decide which disagreements to tolerate. Everything
//! else lives in this file.
//!
//! # Shape, deliberately
//!
//! This is the same shape as `spec/expected-failures.json` and the conformance
//! ratchet that reads it, on purpose: M2's ratchet needs the identical
//! mechanism, and one register serving both harnesses is one thing to keep
//! honest. The verdict that corresponds to the ratchet's `UnexpectedPass` — a
//! listed entry that no longer holds — is [`Verdict::Stale`], and like
//! `UnexpectedPass` it fails CI, so the register is forced back down instead
//! of accumulating entries that quietly widen what the harness tolerates.
//!
//! # Status: skipped-but-present
//!
//! No comparison can be made yet, so [`disagrees`] returns `None` for every
//! input and every entry reports [`Verdict::Skipped`]. The runner exits 0 with
//! a loud summary.
//!
//! At S0 that was because neither side could produce a token stream. S1
//! changed half of it: `mt_inline::tokenizer` is implemented, and the nine
//! plain handlers are live. What is still missing is the other half —
//! `diff.rs` compares **block state**, not token streams, so there is no
//! TypeScript token stream to compare a Rust one against. Both registered
//! entries are S2 and S4 behaviours in any case, so nothing is being deferred
//! that could be checked today.
//!
//! That is not the same as "not wired up". Running today already checks that
//! the register parses, that every entry is well-formed and uniquely
//! identified, that no two entries claim the same input, and — via the unit
//! tests below — that a stale entry is actually reported rather than silently
//! tolerated. The things that rot quietly are caught from day one.
//!
//! ## Flipping it on
//!
//! There is nothing to flip. Point [`disagrees`] at a token-stream comparator
//! and the same run starts enforcing;
//! `tests::every_entry_is_skipped_until_the_harness_compares_token_streams`
//! fails on that day and tells whoever hits it to delete it.
//!
//! S2 and S3 did the comparison by hand instead — muya's `tokenizer` loaded
//! directly and its token streams diffed field by field against the port's,
//! over 207 inputs and then 319 — and between them they widened
//! `emoji-nested-boundary` from two registered inputs to seven and then to
//! twelve. That is rule 1 working (an unregistered disagreement is a failure,
//! so a class wider than its entry must widen the entry) and it is also the
//! argument for building the comparator: a hand-run finds this once, a harness
//! finds it every time.
//!
//! S3's widening makes the argument sharper than S2's did. The five inputs it
//! added are the **opposite direction** of the same bug — muya keeping an
//! emoji the port suppresses, rather than losing one it keeps — and one of
//! them, `*a:smile:*`, was reachable at S2 and missed, because every input the
//! S2 run tried put a space before the `:` and so only ever probed the losing
//! direction. A register is only as wide as the inputs someone thought to try.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One registered intentional difference from muya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// Unique, kebab-case.
    pub id: String,
    /// `file:line` and the function, e.g. `lexer.ts:206 tryChunks`.
    pub site: String,
    /// What muya does.
    pub muya: String,
    /// What the port does instead.
    pub ours: String,
    /// Concrete inputs on which the two engines disagree. Non-empty, and
    /// unique across the whole register.
    pub inputs: Vec<String>,
    /// Optional prose about the entry itself rather than about the behaviour —
    /// in practice, where a widened [`inputs`](Self::inputs) list came from.
    ///
    /// Added at M1 S2, when diffing token streams found five more inputs in
    /// `emoji-nested-boundary`'s class than the entry named. Rule 1 makes an
    /// unregistered disagreement a failure, so a class wider than its entry has
    /// to widen the entry — and a reviewer looking at seven inputs where there
    /// were two deserves to be told that in the register rather than in a
    /// commit message they would have to go and find.
    ///
    /// Not read by the runner. It is data for people.
    pub note: Option<String>,
    /// The marktext issue URL, once filed.
    pub upstream: Option<String>,
}

/// The keys an entry may have. Anything else is a typo that would otherwise be
/// silently ignored — including a misspelled `inputs`, which would empty an
/// entry without emptying the JSON.
const ENTRY_KEYS: [&str; 7] = ["id", "site", "muya", "ours", "inputs", "note", "upstream"];

/// What the runner decided about one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// At least one registered input disagrees. The fix is real, and the
    /// register is doing its job.
    Confirmed,
    /// Every registered input **agrees**. Rule 3: the fix is either
    /// unimplemented or the divergence was imaginary. **Fails CI** — an entry
    /// that no longer holds is an entry that tolerates a future regression.
    Stale,
    /// The two engines cannot be compared on these inputs yet.
    Skipped,
}

impl Verdict {
    /// Decide an entry's verdict from its inputs' outcomes.
    ///
    /// `Some(true)` — the engines disagree on that input.
    /// `Some(false)` — they agree.
    /// `None` — the comparison could not be made.
    ///
    /// A single confirmed disagreement settles the entry, whatever the rest
    /// did. Otherwise an undetermined input blocks the staleness call: "no
    /// disagreement found" only means stale if every input was actually
    /// checked.
    pub fn decide(outcomes: &[Option<bool>]) -> Verdict {
        if outcomes.contains(&Some(true)) {
            Verdict::Confirmed
        } else if outcomes.is_empty() || outcomes.iter().any(Option::is_none) {
            Verdict::Skipped
        } else {
            Verdict::Stale
        }
    }

    pub fn is_ci_failure(self) -> bool {
        matches!(self, Verdict::Stale)
    }
}

/// What one entry did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryReport {
    pub id: String,
    pub verdict: Verdict,
    /// Inputs on which the engines disagreed, as expected.
    pub disagreeing: Vec<String>,
    /// Inputs on which they agreed. Under [`Verdict::Stale`] these are the
    /// evidence: the fix is not reaching them.
    pub agreeing: Vec<String>,
    /// Inputs that could not be compared.
    pub undetermined: Vec<String>,
}

/// What the whole register did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub entries: Vec<EntryReport>,
    /// Malformed-register findings: a duplicate id, an empty `inputs`, an
    /// unknown key. Always a failure, whether or not the engines can be
    /// compared.
    pub shape_problems: Vec<String>,
}

impl Report {
    pub fn stale(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.verdict == Verdict::Stale)
            .map(|e| e.id.as_str())
            .collect()
    }

    pub fn everything_skipped(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|e| e.verdict == Verdict::Skipped)
    }

    pub fn is_ci_failure(&self) -> bool {
        !self.shape_problems.is_empty() || self.entries.iter().any(|e| e.verdict.is_ci_failure())
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load and parse `spec/divergences.json`.
///
/// Returns `Err` only when the file cannot be read or is not the expected JSON
/// shape. Per-entry problems that still parse — a duplicate id, an empty
/// `inputs` — are [`validate`]'s job, so that the runner can report all of
/// them at once instead of one per run.
pub fn load(spec_dir: &Path) -> Result<Vec<Divergence>, String> {
    let path = spec_dir.join("divergences.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&value).map_err(|e| format!("{}: {e}", path.display()))
}

/// The parse half of [`load`], split out so it can be unit-tested against
/// synthetic registers without touching the filesystem.
pub fn parse(value: &serde_json::Value) -> Result<Vec<Divergence>, String> {
    let list = value
        .get("divergences")
        .ok_or("missing key \"divergences\"")?
        .as_array()
        .ok_or("\"divergences\" is not an array")?;

    list.iter()
        .enumerate()
        .map(|(i, entry)| {
            let object = entry
                .as_object()
                .ok_or_else(|| format!("entry {i} is not an object"))?;

            for key in object.keys() {
                if !ENTRY_KEYS.contains(&key.as_str()) {
                    return Err(format!(
                        "entry {i} has unknown key {key:?}; expected one of {ENTRY_KEYS:?}"
                    ));
                }
            }

            let string = |name: &str| -> Result<String, String> {
                object
                    .get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| format!("entry {i} has no string {name:?}"))
            };

            let inputs = object
                .get("inputs")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("entry {i} has no \"inputs\" array"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| format!("entry {i}: {v} is not an input string"))
                })
                .collect::<Result<Vec<String>, String>>()?;

            let upstream = match object.get("upstream") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(other) => {
                    return Err(format!(
                        "entry {i}: \"upstream\" must be null or a URL string, got {other}"
                    ));
                }
            };

            let note = match object.get("note") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(other) => {
                    return Err(format!(
                        "entry {i}: \"note\" must be null or a string, got {other}"
                    ));
                }
            };

            Ok(Divergence {
                id: string("id")?,
                site: string("site")?,
                muya: string("muya")?,
                ours: string("ours")?,
                inputs,
                note,
                upstream,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shape validation
// ---------------------------------------------------------------------------

fn is_kebab_case(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Everything wrong with the register that does not depend on running either
/// engine. Empty means the register is well-formed.
pub fn validate(entries: &[Divergence]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
    let mut seen_inputs: Vec<(&str, &str)> = Vec::new();

    if entries.is_empty() {
        problems.push(
            "the register is empty. D3 names two sites that must be registered before their \
             fixes are made; an empty register means one was deleted, not that none exist."
                .to_string(),
        );
    }

    for entry in entries {
        let id = entry.id.as_str();

        if !is_kebab_case(id) {
            problems.push(format!("{id:?} is not a kebab-case id"));
        }
        if !seen_ids.insert(id) {
            problems.push(format!("duplicate id {id:?}"));
        }

        // `file:line symbol`. Checked loosely — line numbers drift and a
        // strict check would fail for a reason nobody cares about — but a
        // `site` with no file reference at all is not a site.
        if !entry.site.contains(':') {
            problems.push(format!(
                "{id}: \"site\" {:?} names no file:line",
                entry.site
            ));
        }
        for (field, text) in [("muya", &entry.muya), ("ours", &entry.ours)] {
            if text.trim().is_empty() {
                problems.push(format!("{id}: {field:?} is empty"));
            }
        }

        // Rule 2. An entry with no inputs can never be confirmed and can never
        // be shown stale — it is a claim with no evidence attached, which is
        // exactly what the register exists to prevent.
        if entry.inputs.is_empty() {
            problems.push(format!(
                "{id}: \"inputs\" is empty. Rule 2: every entry names concrete inputs, and \
                 those inputs become Rust tests asserting the fixed behaviour."
            ));
        }
        for input in &entry.inputs {
            if input.is_empty() {
                problems.push(format!("{id}: an input is the empty string"));
                continue;
            }
            // Globally unique, not just per-entry: a shared input makes the
            // attribution of a disagreement ambiguous, so neither entry could
            // be shown stale.
            if let Some((other, _)) = seen_inputs.iter().find(|(_, i)| i == input) {
                problems.push(format!(
                    "{id}: input {input:?} is already registered by {other:?}"
                ));
            } else {
                seen_inputs.push((id, input));
            }
        }

        if let Some(url) = &entry.upstream
            && !url.starts_with("https://")
        {
            problems.push(format!("{id}: \"upstream\" {url:?} is not an https URL"));
        }
    }

    problems
}

/// Every input the register tolerates a disagreement on.
///
/// Rule 1's second half: `diff.rs` consults this to decide which differential
/// disagreements are expected. A disagreement on anything not in this set is
/// still a failure.
pub fn registered_inputs(entries: &[Divergence]) -> BTreeSet<&str> {
    entries
        .iter()
        .flat_map(|e| e.inputs.iter().map(String::as_str))
        .collect()
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

/// Whether the two engines disagree on `input`.
///
/// `None` means the comparison could not be made, which is still the whole
/// story: `diff.rs` compares block state rather than token streams, so there
/// is no TypeScript side to compare against. `mt_inline::tokenizer` itself has
/// worked since S1.
///
/// When the harness grows a token-stream mode, this is the one call site to
/// point at it. Everything else in this file already works.
fn disagrees(input: &str) -> Option<bool> {
    let _ = input;
    None
}

pub fn run(entries: &[Divergence]) -> Report {
    run_with(entries, disagrees)
}

/// [`run`], with the engine comparison injected — which is what makes the
/// staleness logic testable before either engine can produce a token stream.
pub fn run_with(entries: &[Divergence], compare: impl Fn(&str) -> Option<bool>) -> Report {
    let mut report = Report {
        shape_problems: validate(entries),
        ..Default::default()
    };

    for entry in entries {
        let outcomes: Vec<Option<bool>> = entry.inputs.iter().map(|i| compare(i)).collect();
        let mut entry_report = EntryReport {
            id: entry.id.clone(),
            verdict: Verdict::decide(&outcomes),
            disagreeing: Vec::new(),
            agreeing: Vec::new(),
            undetermined: Vec::new(),
        };
        for (input, outcome) in entry.inputs.iter().zip(&outcomes) {
            match outcome {
                Some(true) => entry_report.disagreeing.push(input.clone()),
                Some(false) => entry_report.agreeing.push(input.clone()),
                None => entry_report.undetermined.push(input.clone()),
            }
        }
        report.entries.push(entry_report);
    }

    report
}

/// `cargo xtask divergences`.
pub fn main(repo_root: &Path) -> Result<i32, String> {
    let spec_dir: PathBuf = repo_root.join("spec");
    let entries = load(&spec_dir)?;
    let report = run(&entries);

    println!("divergence register — docs/M1.md §5 D3");
    println!("register: {}", spec_dir.join("divergences.json").display());
    println!("  entries           {}", entries.len());
    // Rule 1, stated in the output so the register's scope is never inferred:
    // these are the only inputs on which a differential disagreement is
    // tolerated. `diff.rs` reads the same set from `registered_inputs`.
    println!(
        "  tolerated inputs  {} — a disagreement on anything else is still a failure",
        registered_inputs(&entries).len()
    );
    println!();

    for (entry, result) in entries.iter().zip(&report.entries) {
        let tag = match result.verdict {
            Verdict::Confirmed => "ok     ",
            Verdict::Stale => "FAIL   ",
            Verdict::Skipped => "SKIPPED",
        };
        println!("  {tag}  {}", entry.id);
        println!("            site     {}", entry.site);
        println!(
            "            inputs   {} registered ({} disagree, {} agree, {} undetermined)",
            entry.inputs.len(),
            result.disagreeing.len(),
            result.agreeing.len(),
            result.undetermined.len()
        );
        match entry.upstream.as_deref() {
            Some(url) => println!("            upstream {url}"),
            // Rule 4 is an instruction, not a formality: the two entries D3
            // names are both real upstream bugs, and an unfiled one is a fix
            // this project carries alone forever.
            None => println!("            upstream NOT FILED — rule 4 says file it"),
        }
        if result.verdict == Verdict::Stale {
            println!(
                "            STALE: the engines agree on every registered input. Either the \
                 fix is\n            unimplemented, or the divergence was imaginary. Implement \
                 it or delete\n            the entry — a stale entry silently widens what the \
                 differential\n            harness tolerates.\n            agreeing: {:?}",
                result.agreeing
            );
        }
        println!();
    }

    if !report.shape_problems.is_empty() {
        println!("  register is malformed:");
        for problem in &report.shape_problems {
            println!("    FAIL  {problem}");
        }
        println!();
    }

    let stale = report.stale();
    if !stale.is_empty() {
        println!(
            "  FAIL  {} stale entr{}: {stale:?}\n\
             \x20       Implement the fix or delete the entry. Rule 3 — an entry with no \
             failing\n\
             \x20       differential case is either an unimplemented fix or an imaginary \
             divergence,\n\
             \x20       and either way it widens what the harness tolerates for nothing.",
            stale.len(),
            if stale.len() == 1 { "y" } else { "ies" }
        );
        println!();
    }

    if report.everything_skipped() {
        println!(
            "All entries skipped: the differential harness compares block state rather than\n\
             token streams, so there is no TypeScript token stream to compare against.\n\
             mt_inline::tokenizer itself has worked since M1 S1. The register is parsed,\n\
             validated and wired — it begins enforcing on the first run where a comparison\n\
             can be made. See xtask/src/divergences.rs."
        );
    }

    Ok(if report.is_ci_failure() { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_dir() -> PathBuf {
        crate::repo_root().join("spec")
    }

    fn entry(id: &str, inputs: &[&str]) -> Divergence {
        Divergence {
            id: id.to_string(),
            site: "lexer.ts:1 someHandler".to_string(),
            muya: "does the wrong thing".to_string(),
            ours: "does the right thing".to_string(),
            inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
            note: None,
            upstream: None,
        }
    }

    /// `note` is optional and, unlike `upstream`, may not be a number or an
    /// array either — it is parsed the same way so a typo cannot become a
    /// silently-dropped field.
    #[test]
    fn note_may_be_absent_null_or_a_string() {
        let parse_note = |json: &str| {
            let value: serde_json::Value = serde_json::from_str(json).expect("valid json");
            parse(&value).map(|entries| entries[0].note.clone())
        };
        let with = |note: &str| {
            format!(
                r#"{{"divergences":[{{"id":"x","site":"a:1","muya":"m","ours":"o",
                    "inputs":["a"],{note}"upstream":null}}]}}"#
            )
        };
        assert_eq!(parse_note(&with("")).expect("absent"), None);
        assert_eq!(parse_note(&with(r#""note":null,"#)).expect("null"), None);
        assert_eq!(
            parse_note(&with(r#""note":"why",  "#)).expect("string"),
            Some("why".to_string())
        );
        assert!(
            parse_note(&with(r#""note":7,"#)).is_err(),
            "a number is not prose"
        );
    }

    // --- the decision table, against synthetic outcomes --------------------

    #[test]
    fn a_registered_input_that_disagrees_confirms_the_entry() {
        assert_eq!(Verdict::decide(&[Some(true)]), Verdict::Confirmed);
        assert!(!Verdict::Confirmed.is_ci_failure());
    }

    /// Rule 3, and the half that makes the register stronger than bug-for-bug:
    /// an entry whose inputs all agree is a claim with no evidence.
    #[test]
    fn an_entry_whose_inputs_all_agree_is_stale_and_fails_ci() {
        assert_eq!(Verdict::decide(&[Some(false), Some(false)]), Verdict::Stale);
        assert!(Verdict::Stale.is_ci_failure());
    }

    #[test]
    fn one_disagreement_is_enough_even_when_other_inputs_agree() {
        assert_eq!(
            Verdict::decide(&[Some(false), Some(true)]),
            Verdict::Confirmed
        );
    }

    /// "No disagreement found" only means stale if every input was actually
    /// checked. An undetermined input must not be read as agreement.
    #[test]
    fn an_undetermined_input_blocks_the_staleness_call() {
        assert_eq!(Verdict::decide(&[Some(false), None]), Verdict::Skipped);
        assert_eq!(Verdict::decide(&[None]), Verdict::Skipped);
        assert!(!Verdict::Skipped.is_ci_failure());
    }

    /// ...but a confirmed disagreement still settles it.
    #[test]
    fn a_disagreement_settles_an_entry_with_undetermined_inputs() {
        assert_eq!(Verdict::decide(&[None, Some(true)]), Verdict::Confirmed);
    }

    // --- a stale entry is reported (the S0 gate) ---------------------------

    /// The S0 gate: *register parses; a stale entry is reported*. Driven
    /// through the real runner over a synthetic register, so it exercises the
    /// reporting path and not just [`Verdict::decide`].
    #[test]
    fn a_stale_entry_is_reported_by_the_runner() {
        let entries = vec![entry("fixed-and-real", &["a"]), entry("imaginary", &["b"])];
        let report = run_with(&entries, |input| Some(input == "a"));

        assert_eq!(report.entries[0].verdict, Verdict::Confirmed);
        assert_eq!(report.entries[1].verdict, Verdict::Stale);
        assert_eq!(report.stale(), vec!["imaginary"]);
        assert!(report.is_ci_failure());

        // The report names the evidence, so the failure says which inputs
        // stopped disagreeing rather than only that something did.
        assert_eq!(report.entries[1].agreeing, vec!["b".to_string()]);
        assert!(report.entries[0].agreeing.is_empty());
    }

    #[test]
    fn a_fully_confirmed_register_is_green() {
        let entries = vec![entry("one", &["a"]), entry("two", &["b", "c"])];
        let report = run_with(&entries, |_| Some(true));
        assert!(report.stale().is_empty());
        assert!(!report.is_ci_failure());
        assert!(!report.everything_skipped());
    }

    // --- shape validation --------------------------------------------------

    #[test]
    fn a_duplicate_id_is_a_shape_problem() {
        let problems = validate(&[entry("same", &["a"]), entry("same", &["b"])]);
        assert!(
            problems.iter().any(|p| p.contains("duplicate id")),
            "{problems:?}"
        );
    }

    /// Rule 2. An entry with no inputs can be neither confirmed nor shown
    /// stale, so it would sit in the register forever asserting nothing.
    #[test]
    fn an_entry_with_no_inputs_is_a_shape_problem() {
        let problems = validate(&[entry("no-evidence", &[])]);
        assert!(
            problems.iter().any(|p| p.contains("\"inputs\" is empty")),
            "{problems:?}"
        );
    }

    /// Two entries claiming the same input makes the attribution of a
    /// disagreement ambiguous, so neither could ever be shown stale.
    #[test]
    fn the_same_input_registered_twice_is_a_shape_problem() {
        let problems = validate(&[entry("one", &["dup"]), entry("two", &["dup"])]);
        assert!(
            problems.iter().any(|p| p.contains("already registered")),
            "{problems:?}"
        );
    }

    #[test]
    fn an_id_that_is_not_kebab_case_is_a_shape_problem() {
        for bad in ["Emoji_Boundary", "emoji--boundary", "-lead", "trail-", ""] {
            let problems = validate(&[entry(bad, &["a"])]);
            assert!(
                problems.iter().any(|p| p.contains("kebab-case")),
                "{bad:?} should be rejected: {problems:?}"
            );
        }
        assert!(is_kebab_case("emoji-nested-boundary"));
        assert!(is_kebab_case("gfm-6-11"));
    }

    #[test]
    fn a_non_url_upstream_is_a_shape_problem() {
        let mut e = entry("one", &["a"]);
        e.upstream = Some("#4307".to_string());
        assert!(
            validate(&[e]).iter().any(|p| p.contains("https URL")),
            "an issue number is not a URL"
        );
    }

    // --- parsing -----------------------------------------------------------

    /// A misspelled key would otherwise be ignored silently — and a
    /// misspelled `inputs` would empty an entry without emptying the JSON.
    #[test]
    fn an_unknown_key_is_rejected_rather_than_ignored() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"divergences":[{"id":"x","site":"a:1","muya":"m","ours":"o",
                "inpts":["a"],"upstream":null}]}"#,
        )
        .expect("valid json");
        let error = parse(&value).expect_err("unknown key");
        assert!(error.contains("inpts"), "{error}");
    }

    #[test]
    fn upstream_may_be_absent_null_or_a_url() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"divergences":[
                {"id":"a","site":"a:1","muya":"m","ours":"o","inputs":["1"]},
                {"id":"b","site":"a:1","muya":"m","ours":"o","inputs":["2"],"upstream":null},
                {"id":"c","site":"a:1","muya":"m","ours":"o","inputs":["3"],
                 "upstream":"https://github.com/marktext/marktext/issues/1"}]}"#,
        )
        .expect("valid json");
        let entries = parse(&value).expect("parses");
        assert_eq!(entries[0].upstream, None);
        assert_eq!(entries[1].upstream, None);
        assert_eq!(
            entries[2].upstream.as_deref(),
            Some("https://github.com/marktext/marktext/issues/1")
        );
    }

    // --- the real register -------------------------------------------------

    #[test]
    fn the_register_parses_and_is_well_formed() {
        let entries = load(&spec_dir()).expect("load spec/divergences.json");
        let problems = validate(&entries);
        assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    }

    /// D3's table names three sites and marks two of them "Register? Yes" —
    /// the third (`pending` doubling, `lexer.ts:135`) is inert and diverges
    /// from nothing. If a fix is ever made without an entry, this is what
    /// notices.
    #[test]
    fn the_register_holds_the_two_entries_d3_names() {
        let entries = load(&spec_dir()).expect("load");
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "emoji-nested-boundary",
                "disallowed-html-tag-substring-match"
            ],
            "docs/M1.md §5 D3 registers exactly these two sites"
        );
    }

    /// Rule 2 again, against the real register: the registered inputs are what
    /// become Rust tests asserting the fixed behaviour, so they have to be the
    /// inputs someone actually ran against the engine.
    ///
    /// The four D3 named are still here; the five `emoji-nested-boundary`
    /// gained at S2 came from diffing token streams over 207 inputs, and every
    /// one of them has a test in `crates/mt-inline/src/lexer.rs`. **Grow this
    /// number only alongside those tests** — a registered input with no test is
    /// a tolerated disagreement nobody is asserting anything about, which is
    /// the failure mode rule 2 exists to prevent.
    #[test]
    fn the_registered_inputs_are_the_ones_someone_verified() {
        let entries = load(&spec_dir()).expect("load");
        let inputs = registered_inputs(&entries);
        for expected in [
            // D3, at S0
            "**a :smile:**",
            "**:smile:**",
            "<subtitle>",
            "<scripty>",
            // the same divergence, found wider at S2 — muya LOSES an emoji
            "__a :smile:__",
            "~~a :smile:~~",
            "**:100:**",
            "**a :smile: b**",
            "**abcdefgh :smile:**",
            // …and wider again at S3, in the opposite direction: at a rule
            // whose base is 1 muya reads the `:` itself, so the word-boundary
            // guard never fires and it KEEPS an emoji the port suppresses.
            "[a:smile:](u)",
            "[12:00-14:00](u)",
            "[x:100:](u)",
            "*a:smile:*",
            "[[a:smile:]](u)",
        ] {
            assert!(inputs.contains(expected), "{expected:?} is not registered");
        }
        assert_eq!(inputs.len(), 14);
    }

    /// Asserted so that the day it changes is the day someone deliberately
    /// wires the token-stream comparison in.
    #[test]
    fn every_entry_is_skipped_until_the_harness_compares_token_streams() {
        let entries = load(&spec_dir()).expect("load");
        let report = run(&entries);
        assert!(
            report.everything_skipped(),
            "the divergence runner can now compare token streams. Good — that means the \
             register starts enforcing. Delete this test."
        );
        assert!(!report.is_ci_failure());
    }
}
