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
//! # Status: enforcing, as of M1 S7
//!
//! From S0 to S6 this runner reported [`Verdict::Skipped`] for every entry and
//! exited 0, because there was no TypeScript token stream to compare a Rust one
//! against — `diff.rs` compares **block state**. Rule 3 could therefore not
//! fire for any entry, so the register could not detect its own staleness:
//! revert a fix and the runner still printed `SKIPPED` and passed.
//!
//! S7 closed it. [`crate::tokens`] is the comparator — a Node dumper beside
//! `dump-ts-state.mjs` that loads muya's `tokenizer` with a happy-dom
//! `DOMParser` registered, and a Rust-side serializer that puts an
//! `mt_inline::Token` into muya's wire shape — and this runner now does two
//! things with it:
//!
//! 1. **The negative control.** Every registered input goes through the
//!    comparator, and an entry whose inputs all agree is [`Verdict::Stale`] and
//!    fails CI. That is rule 3, mechanically.
//! 2. **Rule 1's other half.** A sweep over `bench/corpus/`, the 1324
//!    CommonMark and GFM examples and the `marktext-round-trip` fixtures, on
//!    which **any** disagreement is a failure. Without it the register only
//!    ever proves that its own inputs still disagree, which is necessary and
//!    nothing like sufficient.
//!
//! When the reference engine is absent the run still reports `SKIPPED` and
//! exits 0, exactly as `diff.rs` does — and `--require-ts` turns that into a
//! hard failure. CI passes it, so a broken checkout cannot quietly return this
//! runner to the state S7 spent a stage getting it out of.
//!
//! # Why five stages left this undone, and what it cost
//!
//! Kept because the *shape* of the mistake is the finding, and because the same
//! shape is available to any future stage that decides a hand-check will do.
//!
//! S2 and S3 did the comparison by hand — muya's `tokenizer` loaded directly
//! and its token streams diffed field by field against the port's, over 207
//! inputs and then 319 — and between them they widened `emoji-nested-boundary`
//! from two registered inputs to seven and then to twelve. That is rule 1
//! working (an unregistered disagreement is a failure, so a class wider than
//! its entry must widen the entry) and it is also the argument for building the
//! comparator: a hand-run finds this once, a harness finds it every time.
//!
//! S3's widening makes the argument sharper than S2's did. The five inputs it
//! added are the **opposite direction** of the same bug — muya keeping an
//! emoji the port suppresses, rather than losing one it keeps — and one of
//! them, `*a:smile:*`, was reachable at S2 and missed, because every input the
//! S2 run tried put a space before the `:` and so only ever probed the losing
//! direction. A register is only as wide as the inputs someone thought to try.
//!
//! S4 did it a third time, to a *different* entry:
//! `disallowed-html-tag-substring-match` went from two inputs to seven, and
//! the ones it was missing are the ones that matter — `<noscript>` is a
//! standard HTML element that muya rejects for containing `script`, which is
//! harder to dismiss than the invented `<scripty>` the entry opened with.
//!
//! **S5 did it a fourth time, and that one is the argument by itself.** S4
//! probed `emoji-nested-boundary` at its own new container, `html_tag`, with
//! two inputs, found both agreeing, and wrote down that the entry did not need
//! to widen. One of the two agrees for a reason that does not generalise —
//! muya's misread index runs off the end of a short child string, and
//! `lexer.ts:203`'s `prevChar &&` treats `undefined` as a boundary — and 96 of
//! 228 swept inputs disagree, in both directions. A hand-run does not just
//! find a class once instead of every time; it can find *half* of one and read
//! as a clean result.
//!
//! S6 built the fifth and found the gap had a second dimension: its comparator
//! agreed on all 5586 inputs *and was structurally unable to see one of the
//! three entries*, because that divergence lives in `attrs` and a text-level
//! comparison never reads `attrs`.
//!
//! # What S5 and S6 left for S7, and where each note landed
//!
//! S5 gave three reasons for deferring, and **all three were already decided
//! and built** — `diff.rs` had been invoking Node with `tsx` for the whole
//! corpus since M0, degrading to a skip without a marktext clone and
//! hard-failing under `--require-ts`, and CI had been checking the engine out
//! at `MARKTEXT_REF` and running `cargo xtask ci --require-ts` since the same
//! commit. So the deferral's stated cost was a second dumper beside the first,
//! not new plumbing. Recorded here rather than quietly dropped, because the
//! sentence deterred four stages.
//!
//! The part of the handoff that *was* worth having is the list of things each
//! stage lost a run to. Every one of them is implemented in [`crate::tokens`]
//! and restated beside the code that satisfies it:
//!
//! - **Compare hex of UTF-8 bytes, not strings** (S5).
//! - **Convert muya's UTF-16 offsets by walking code points** (S5), not by
//!   dividing or by `length`. An astral character is one code point, two UTF-16
//!   units and four bytes.
//! - **Do not let the transport normalise** (S5). Four of S5's first-run
//!   disagreements were `TextDecoder`'s default BOM-stripping rather than the
//!   tokenizer. A harness bug and a port bug look identical in the output.
//! - **Normalise `undefined` versus `''` per field**, not per type (S3),
//!   following muya's own `|| ''` sites. S3 lost a run to this; `token.rs`
//!   lists the four fields that differ.
//! - **Sweep, do not sample** (S3, S4, S5).
//! - **Run a negative control every time** (S6). A comparator that has never
//!   produced a disagreement has not been shown to be able to.
//! - **Compare fields, not rendered text** (S6), or
//!   `html-tag-attrs-from-a-foster-parented-element` is invisible.
//! - **`highlights` is a per-field absent/empty normalisation** (S6). muya
//!   creates the key on the first push; the port gives every token an empty
//!   `Vec`. Treat the two as different and every token in every document
//!   reports a disagreement.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Which comparator an entry belongs to.
///
/// **Added at M2 S0, per docs/M2.md §5 D4.** M2 produces disagreements at a
/// second layer — anywhere `marked` and CommonMark differ about *blocks* and
/// the port follows CommonMark — and `spec/README.md`'s rule is *"three
/// registers, one shape, on purpose"*. So the block divergences go in this
/// register rather than in a fourth one, and each runner runs the entries that
/// are its own.
///
/// **The filtering is not cosmetic.** An entry is [`Verdict::Stale`] when
/// every one of its inputs *agrees*, and a comparator that cannot see a
/// divergence reports agreement. So an inline entry run through
/// `cargo xtask diff` — which compares block state, in which a leaf's text is
/// unparsed inline source (§4 C4) — would be `Stale` on every input and fail
/// the build for a reason that has nothing to do with the port. M1 S6 hit
/// exactly this shape: its text-level comparator agreed on all 5,586 inputs
/// and was *structurally unable* to see the `attrs`-level entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// Token streams — `cargo xtask divergences`, via [`crate::tokens`].
    Inline,
    /// Block state — `cargo xtask diff`, via [`crate::diff`].
    Block,
}

impl Layer {
    /// The wire spelling, which is what `divergences.json` carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Inline => "inline",
            Layer::Block => "block",
        }
    }

    fn parse(s: &str) -> Option<Layer> {
        match s {
            "inline" => Some(Layer::Inline),
            "block" => Some(Layer::Block),
            _ => None,
        }
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One registered intentional difference from muya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// Unique, kebab-case.
    pub id: String,
    /// Which comparator owns this entry. See [`Layer`].
    pub layer: Layer,
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
const ENTRY_KEYS: [&str; 8] = [
    "id", "layer", "site", "muya", "ours", "inputs", "note", "upstream",
];

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

            // Required, not defaulted. A missing `layer` would silently put
            // the entry in whichever layer the default named, and the runner
            // that does not own it would never see it — which is the shape of
            // a register entry that tolerates a disagreement nobody checks.
            let layer_str = string("layer")?;
            let layer = Layer::parse(&layer_str).ok_or_else(|| {
                format!("entry {i}: \"layer\" must be \"inline\" or \"block\", got {layer_str:?}")
            })?;

            Ok(Divergence {
                id: string("id")?,
                layer,
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

/// Every input the register tolerates a disagreement on, at every layer.
///
/// Rule 1's second half. Prefer [`registered_inputs_for`]: a runner should
/// tolerate the disagreements of **its own** layer, because an input
/// registered at the other layer is one its comparator has no reason to
/// disagree on and every reason to be checked against.
pub fn registered_inputs(entries: &[Divergence]) -> BTreeSet<&str> {
    entries
        .iter()
        .flat_map(|e| e.inputs.iter().map(String::as_str))
        .collect()
}

/// The entries belonging to one comparator.
///
/// M2.md §5 D4. See [`Layer`] for why running an entry through the wrong
/// comparator is a false `Stale` rather than a harmless no-op.
pub fn entries_for(entries: &[Divergence], layer: Layer) -> Vec<&Divergence> {
    entries.iter().filter(|e| e.layer == layer).collect()
}

/// Every input one layer's entries tolerate a disagreement on.
pub fn registered_inputs_for(entries: &[Divergence], layer: Layer) -> BTreeSet<&str> {
    entries
        .iter()
        .filter(|e| e.layer == layer)
        .flat_map(|e| e.inputs.iter().map(String::as_str))
        .collect()
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

/// The register's verdicts, given a comparison already made.
///
/// Split from [`run_with`] only so that `main` can run **one** Node process for
/// the register and the sweep together — the engine costs about a second to
/// start and nothing per input after that, so two invocations would double the
/// fixed cost to separate two things that are one run.
pub fn run(entries: &[Divergence], outcomes: &BTreeMap<String, Option<bool>>) -> Report {
    run_with(entries, |input| outcomes.get(input).copied().flatten())
}

/// [`run`], with the engine comparison injected — which is what makes the
/// staleness logic testable without a marktext clone, and what let the decision
/// table below be written and asserted five stages before a comparison existed.
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

/// How many unregistered disagreements to print before summarising.
///
/// A bounded report that does not say what it bounded reads as full coverage,
/// so the count is always printed even when the list is truncated.
const REPORTED_FAILURES: usize = 12;

/// `cargo xtask divergences [--require-ts] [--no-sweep] [--full-sweep]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut require_ts = false;
    let mut sweep = true;
    let mut breadth = crate::tokens::Breadth::Default;
    for arg in args {
        match arg.as_str() {
            // Same flag and the same meaning as `cargo xtask diff`: without a
            // marktext clone this runner skips, and CI passes this so that a
            // broken checkout is a failure rather than a silent no-op. That is
            // the one failure mode that would return the register to its
            // pre-S7 state without anyone noticing.
            "--require-ts" => require_ts = true,
            // The register alone, for a fast local loop. Not what CI runs:
            // without the sweep, rule 1's other half is unchecked.
            "--no-sweep" => sweep = false,
            // Adds the lines of `1mb.md` and `5mb.md`: 4,264 inputs becomes
            // 41,009, and 22 s of Node becomes 120 s. The nightly soak workflow
            // runs this; every-commit CI does not. See `tokens::sweep_inputs`
            // for why that cap is defensible rather than merely convenient.
            "--full-sweep" => breadth = crate::tokens::Breadth::Full,
            other => return Err(format!("unrecognised argument: {other}")),
        }
    }

    let spec_dir: PathBuf = repo_root.join("spec");
    let all_entries = load(&spec_dir)?;

    // Shape problems are a property of the whole register, so they are
    // computed over every entry and reported here even when the failing entry
    // belongs to the other layer — otherwise a malformed block entry would be
    // invisible until S3 wakes `cargo xtask diff`.
    let register_problems = validate(&all_entries);

    // M2.md §5 D4: this runner compares **token streams**, so it runs the
    // inline entries. A block entry run here would agree on every input and
    // report STALE, which is a failure about the comparator rather than about
    // the port.
    let entries: Vec<Divergence> = entries_for(&all_entries, Layer::Inline)
        .into_iter()
        .cloned()
        .collect();
    let block_entries = entries_for(&all_entries, Layer::Block).len();

    // The register's own inputs first, then everything else. Both go through
    // one Node process: the engine costs about a second to start and almost
    // nothing per input, so splitting them would double the fixed cost to
    // separate two halves of one run.
    //
    // Registered inputs are always tokenized with muya's **defaults**, which is
    // why `emoji-nested-boundary`'s note records that `[a:smile:][ref]` and its
    // siblings are not listed: they need a populated label map to reach
    // `tryReferenceLink` at all. Option-carrying probes live in the sweep.
    let registered: Vec<crate::tokens::Input> = entries
        .iter()
        .flat_map(|e| {
            e.inputs
                .iter()
                .map(|i| crate::tokens::Input::new(i.clone()))
        })
        .collect();
    let tolerated = registered_inputs(&entries);
    let swept: Vec<crate::tokens::Input> = if sweep {
        crate::tokens::sweep_inputs(repo_root, breadth)?
            .into_iter()
            .filter(|input| !tolerated.contains(input.src.as_str()))
            .collect()
    } else {
        Vec::new()
    };

    let mut all = registered.clone();
    all.extend(swept.iter().cloned());

    let comparison = crate::tokens::compare(repo_root, &all)?;
    let mut outcomes: BTreeMap<String, Option<bool>> = BTreeMap::new();
    let mut engine_errors: Vec<(String, String)> = Vec::new();
    let mut unregistered_failures: Vec<(String, String, String, String)> = Vec::new();
    let mut swept_agreements = 0usize;

    match &comparison {
        None => {
            let reason = crate::tokens::unavailable_reason(repo_root)
                .unwrap_or_else(|| "the TypeScript engine is unavailable".to_string());
            if require_ts {
                return Err(format!(
                    "the TypeScript reference engine is unavailable and --require-ts was given:\n\
                     {reason}"
                ));
            }
            for input in &all {
                outcomes.insert(input.src.clone(), None);
            }
        }
        Some(verdicts) => {
            for (input, verdict) in all.iter().zip(verdicts) {
                let registered_here = tolerated.contains(input.src.as_str());
                match verdict {
                    crate::tokens::InputVerdict::Agree => {
                        outcomes.insert(input.src.clone(), Some(false));
                        if !registered_here {
                            swept_agreements += 1;
                        }
                    }
                    crate::tokens::InputVerdict::Disagree { at, ts, rs } => {
                        outcomes.insert(input.src.clone(), Some(true));
                        if !registered_here {
                            // Rule 1: a disagreement on an unregistered input
                            // is a failure, exactly as before the harness
                            // existed. Either the port is wrong, or the class
                            // is wider than its entry and the entry must widen
                            // (which is what happened four times in S2–S5).
                            unregistered_failures.push((
                                input.src.clone(),
                                at.clone(),
                                ts.clone(),
                                rs.clone(),
                            ));
                        }
                    }
                    crate::tokens::InputVerdict::EngineThrew(message) => {
                        outcomes.insert(input.src.clone(), None);
                        engine_errors.push((input.src.clone(), message.clone()));
                    }
                }
            }
        }
    }

    let mut report = run(&entries, &outcomes);
    // `run` validated the inline slice; replace that with the whole
    // register's problems, so a malformed entry cannot hide behind its layer.
    report.shape_problems = register_problems;

    println!("divergence register — docs/M1.md §5 D3, docs/M2.md §5 D4");
    println!("register: {}", spec_dir.join("divergences.json").display());
    println!(
        "  layer             inline — this runner compares token streams; the block entries\n\
         \x20                   are `cargo xtask diff`'s and are not run here"
    );
    println!("  entries           {} inline", entries.len());
    if block_entries > 0 {
        println!(
            "                    {block_entries} block, skipped here — see `cargo xtask diff`"
        );
    }
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

    // --- rule 1's other half ------------------------------------------------
    //
    // The register is a list of tolerated disagreements. On its own, confirming
    // every entry proves only that the tolerated ones still hold; what makes it
    // a claim about the port is the set of inputs on which the two engines must
    // agree *exactly*.
    if sweep && comparison.is_some() {
        println!(
            "  sweep             {} inputs beyond the register — rule 1: a disagreement on any\n\
             \x20                   of these is a failure, not a divergence",
            swept.len()
        );
        // No silent caps: a bounded run that does not say what it bounded reads
        // as full coverage.
        if breadth == crate::tokens::Breadth::Default {
            println!(
                "            bounded    1mb.md's and 5mb.md's lines are excluded here; \
                 --full-sweep\n\
                 \x20                      adds them (40,982 swept, ~120 s). The nightly soak \
                 runs it."
            );
        }
        println!("            agree      {swept_agreements}");
        println!("            disagree   {}", unregistered_failures.len());
        if !engine_errors.is_empty() {
            println!("            muya threw {}", engine_errors.len());
        }
        println!();
    }

    for (input, at, ts, rs) in unregistered_failures.iter().take(REPORTED_FAILURES) {
        println!("  FAIL  unregistered disagreement on {input:?}");
        println!("        first difference at {at}");
        println!("        typescript: {ts}");
        println!("        rust:       {rs}");
    }
    if unregistered_failures.len() > REPORTED_FAILURES {
        println!(
            "  … and {} more (strings are hex of their UTF-8 bytes; see xtask/src/tokens.rs)",
            unregistered_failures.len() - REPORTED_FAILURES
        );
    }
    if !unregistered_failures.is_empty() {
        println!();
        println!(
            "  Rule 1: a disagreement on a registered input is expected; a disagreement on\n\
             \x20 anything else is a failure. Either the port is wrong, or a registered class\n\
             \x20 is wider than its entry and rule 2 says widen it with concrete inputs that\n\
             \x20 become tests. S2, S3, S4 and S5 each hit the second case."
        );
        println!();
    }

    for (input, message) in engine_errors.iter().take(REPORTED_FAILURES) {
        println!("  ERR   muya threw on {input:?}: {message}");
    }
    if !engine_errors.is_empty() {
        println!();
    }

    if report.everything_skipped() {
        println!(
            "All entries skipped: the TypeScript reference engine was not available, so\n\
             nothing was compared. Set MARKTEXT_DIR or pass --marktext to the dumper, and\n\
             pass --require-ts to make a missing engine a failure instead of a skip; CI\n\
             does. See xtask/src/tokens.rs."
        );
    }

    let failed =
        report.is_ci_failure() || !unregistered_failures.is_empty() || !engine_errors.is_empty();
    Ok(i32::from(failed))
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
            layer: Layer::Inline,
            site: "lexer.ts:1 someHandler".to_string(),
            muya: "does the wrong thing".to_string(),
            ours: "does the right thing".to_string(),
            inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
            note: None,
            upstream: None,
        }
    }

    // --- the layer field, added at M2 S0 (docs/M2.md §5 D4) ----------------

    fn with_layer(layer: &str) -> String {
        format!(
            r#"{{"divergences":[{{"id":"x","layer":{layer},"site":"a:1","muya":"m",
                "ours":"o","inputs":["a"],"upstream":null}}]}}"#
        )
    }

    fn parse_str(json: &str) -> Result<Vec<Divergence>, String> {
        let value: serde_json::Value = serde_json::from_str(json).expect("valid json");
        parse(&value)
    }

    #[test]
    fn layer_parses_both_spellings() {
        assert_eq!(
            parse_str(&with_layer(r#""inline""#)).expect("inline")[0].layer,
            Layer::Inline
        );
        assert_eq!(
            parse_str(&with_layer(r#""block""#)).expect("block")[0].layer,
            Layer::Block
        );
    }

    /// Required rather than defaulted. A defaulted `layer` would silently file
    /// an entry under whichever layer the default named, and the runner that
    /// does not own it would never run it — a tolerated disagreement nobody
    /// checks, which is the state rule 3 exists to prevent.
    #[test]
    fn a_missing_layer_is_a_parse_error() {
        let json = r#"{"divergences":[{"id":"x","site":"a:1","muya":"m","ours":"o",
            "inputs":["a"],"upstream":null}]}"#;
        let err = parse_str(json).expect_err("layer is required");
        assert!(err.contains("layer"), "{err}");
    }

    #[test]
    fn an_unknown_layer_is_a_parse_error() {
        let err = parse_str(&with_layer(r#""html""#)).expect_err("only two layers exist");
        assert!(err.contains("inline") && err.contains("block"), "{err}");
    }

    /// The other half of the same guard: an entry may not carry a key the
    /// runner does not read, because a misspelled `layer` would otherwise be
    /// silently dropped — the same reason `ENTRY_KEYS` exists at all.
    #[test]
    fn a_misspelled_layer_key_is_a_parse_error() {
        let json = r#"{"divergences":[{"id":"x","layers":"inline","site":"a:1","muya":"m",
            "ours":"o","inputs":["a"],"upstream":null}]}"#;
        let err = parse_str(json).expect_err("unknown key");
        assert!(err.contains("layers"), "{err}");
    }

    /// The filtering itself: each runner sees only its own entries, and only
    /// its own tolerated inputs.
    #[test]
    fn each_layer_gets_its_own_entries_and_inputs() {
        let mut block = entry("a-block-thing", &["> [foo]: /a"]);
        block.layer = Layer::Block;
        let entries = vec![entry("an-inline-thing", &["**a :smile:**"]), block];

        let inline = entries_for(&entries, Layer::Inline);
        assert_eq!(inline.len(), 1);
        assert_eq!(inline[0].id, "an-inline-thing");
        assert_eq!(
            registered_inputs_for(&entries, Layer::Inline)
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["**a :smile:**"]
        );

        let block = entries_for(&entries, Layer::Block);
        assert_eq!(block.len(), 1);
        assert_eq!(block[0].id, "a-block-thing");
        assert_eq!(
            registered_inputs_for(&entries, Layer::Block)
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["> [foo]: /a"]
        );

        // And `registered_inputs` still spans both, for anything that needs
        // the whole register.
        assert_eq!(registered_inputs(&entries).len(), 2);
    }

    /// **Why the filtering is not cosmetic**, as a test rather than as prose.
    /// Running an entry through a comparator that cannot see its divergence
    /// makes every input agree, which is `Stale`, which fails CI — for a
    /// reason that is about the harness rather than about the port. M1 S6 hit
    /// this shape with a text-level comparator and an `attrs`-level entry.
    #[test]
    fn an_entry_run_by_the_wrong_comparator_would_report_stale() {
        let mut block = entry("a-block-thing", &["> [foo]: /a"]);
        block.layer = Layer::Block;
        let entries = vec![entry("an-inline-thing", &["**a :smile:**"]), block];

        // A token-stream comparator: it disagrees on the inline input and has
        // no opinion about the block one, so it reports agreement there.
        let compare = |input: &str| Some(input == "**a :smile:**");

        let unfiltered = run_with(&entries, compare);
        assert_eq!(
            unfiltered.stale(),
            vec!["a-block-thing"],
            "unfiltered, the block entry is falsely stale"
        );
        assert!(unfiltered.is_ci_failure());

        let mine: Vec<Divergence> = entries_for(&entries, Layer::Inline)
            .into_iter()
            .cloned()
            .collect();
        let filtered = run_with(&mine, compare);
        assert!(filtered.stale().is_empty());
        assert!(!filtered.is_ci_failure());
    }

    /// The committed register, as data: three entries, all inline, and the
    /// block layer empty until S1 produces one. If that changes without this
    /// test changing, the change was not deliberate.
    #[test]
    fn the_committed_register_is_three_inline_entries_and_no_block_entries() {
        let entries = load(&spec_dir()).expect("the register parses");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries_for(&entries, Layer::Inline).len(), 3);
        assert_eq!(
            entries_for(&entries, Layer::Block).len(),
            0,
            "S1 is the stage that registers the first block divergence (§4 C1)"
        );
        assert!(validate(&entries).is_empty(), "{:?}", validate(&entries));
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
                r#"{{"divergences":[{{"id":"x","layer":"inline","site":"a:1","muya":"m","ours":"o",
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
                {"id":"a","layer":"inline","site":"a:1","muya":"m","ours":"o","inputs":["1"]},
                {"id":"b","layer":"inline","site":"a:1","muya":"m","ours":"o","inputs":["2"],"upstream":null},
                {"id":"c","layer":"block","site":"a:1","muya":"m","ours":"o","inputs":["3"],
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
    ///
    /// The third entry is **not** one of D3's sites and that is deliberate:
    /// `html-tag-attrs-from-a-foster-parented-element` came out of deciding
    /// D2 at S4, where porting `getAttributes` without a DOM left exactly one
    /// tree-construction mechanism unmodelled. D3's list was never meant to be
    /// closed — it is the list of *muya bugs known when M1 opened* — so a
    /// fourth entry from a later stage is the register working, not the plan
    /// being violated. What must not happen is an entry appearing with no
    /// decision behind it, which is why this asserts the id and M1.md §5 names
    /// each one.
    #[test]
    fn the_register_holds_d3s_two_entries_and_d2s() {
        let entries = load(&spec_dir()).expect("load");
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                // docs/M1.md §5 D3, sites 1 and 2
                "emoji-nested-boundary",
                "disallowed-html-tag-substring-match",
                // docs/M1.md §5 D2, decided at S4
                "html-tag-attrs-from-a-foster-parented-element"
            ],
            "every entry needs a decision in docs/M1.md §5 behind it"
        );
    }

    /// Rule 2 again, against the real register: the registered inputs are what
    /// become Rust tests asserting the fixed behaviour, so they have to be the
    /// inputs someone actually ran against the engine.
    ///
    /// The four D3 named are still here; the five `emoji-nested-boundary`
    /// gained at S2 came from diffing token streams over 207 inputs, the five
    /// more at S3 from 319, and S4's eleven — five widening
    /// `disallowed-html-tag-substring-match` and four opening
    /// `html-tag-attrs-from-a-foster-parented-element` — from 4172. Every one
    /// of them has a test in `crates/mt-inline/src/lexer.rs`. **Grow this
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
            // S4 widened the disallowed-tag class from two inputs to seven:
            // the unanchored test reaches real HTML elements, is
            // case-insensitive on the containing name, and hits hyphenated
            // custom-element names GFM explicitly allows.
            "<noscript>",
            "<SubTitle>x</SubTitle>",
            "<subscript>x</subscript>",
            "<iframe-x>x</iframe-x>",
            "<xstyle>x</xstyle>",
            // …and opened D2's entry: the one tree-construction mechanism the
            // DOM-free `getAttributes` does not model.
            "<table><span id=\"i\">y</span></table>",
            "<table class=\"c\"><span id=\"i\">y</span></table>",
            "<table class=\"c\"><img src=\"s\"></table>",
            "<table class=\"c\">a <code>c</code> b</table>",
            // S5 widened `emoji-nested-boundary` a third time, at `html_tag`
            // — and this one is a correction: S4 probed the class with two
            // inputs, found both agreeing, and recorded that it needed no
            // entry. Only one of the two agrees, and for a reason that does
            // not generalise. Both directions occur here too.
            "<div>:smile:</div>",
            "<b>x :smile:</b>",
            "<em>:100:</em>",
            "<strong>x:smile:</strong>",
        ] {
            assert!(inputs.contains(expected), "{expected:?} is not registered");
        }
        assert_eq!(inputs.len(), 27);
    }

    /// The register is now enforced by a comparison against a running engine,
    /// which `cargo test` cannot make: the marktext clone is not a build
    /// dependency and CI checks it out as a separate step. So what this asserts
    /// is the wiring — that an undetermined outcome still blocks the staleness
    /// call, over the **real** register rather than a synthetic one.
    ///
    /// This replaces `every_entry_is_skipped_until_the_harness_compares_token_
    /// streams`, which was designed to fail the day the comparison worked and
    /// to tell whoever hit it to delete it. It did, and this is that deletion.
    #[test]
    fn an_engine_that_cannot_be_reached_skips_rather_than_reporting_stale() {
        let entries = load(&spec_dir()).expect("load");
        let outcomes: BTreeMap<String, Option<bool>> = registered_inputs(&entries)
            .into_iter()
            .map(|input| (input.to_string(), None))
            .collect();
        let report = run(&entries, &outcomes);
        assert!(report.everything_skipped());
        assert!(
            !report.is_ci_failure(),
            "a machine with no marktext clone must not fail the build; --require-ts is \
             what makes it one, and CI passes it"
        );
    }

    /// The negative control, as a decision-table case: this is the shape a real
    /// run produces, and it must be green rather than merely not-red.
    #[test]
    fn a_register_whose_inputs_all_disagree_is_confirmed_and_green() {
        let entries = load(&spec_dir()).expect("load");
        let outcomes: BTreeMap<String, Option<bool>> = registered_inputs(&entries)
            .into_iter()
            .map(|input| (input.to_string(), Some(true)))
            .collect();
        let report = run(&entries, &outcomes);
        assert!(!report.everything_skipped());
        assert!(report.stale().is_empty());
        assert!(!report.is_ci_failure());
        assert!(
            report
                .entries
                .iter()
                .all(|e| e.verdict == Verdict::Confirmed)
        );
    }

    /// Rule 3 against the real register: revert any one fix and that entry's
    /// inputs start agreeing, which must fail the build. This is the property
    /// that could not be checked at all from S0 to S6.
    #[test]
    fn reverting_a_fix_makes_its_entry_stale() {
        let entries = load(&spec_dir()).expect("load");
        for reverted in &entries {
            let outcomes: BTreeMap<String, Option<bool>> = entries
                .iter()
                .flat_map(|e| {
                    let agrees = e.id == reverted.id;
                    e.inputs.iter().map(move |i| (i.clone(), Some(!agrees)))
                })
                .collect();
            let report = run(&entries, &outcomes);
            assert_eq!(report.stale(), vec![reverted.id.as_str()]);
            assert!(report.is_ci_failure());
        }
    }
}
