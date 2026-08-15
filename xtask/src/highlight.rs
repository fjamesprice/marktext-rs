//! The highlight-span differential harness (docs/M3.md §5 D14/D15, §6 S3).
//!
//! ```text
//! bench/corpus/*.md ──► mt_md::parse ──► every Block::CodeBlock
//!                                            │
//!                          {lang, code} ─────┼──► node → Prism.tokenize → spans ──┐
//!                                            │                                     ├──► compare
//!                                            └──► mt_highlight ────────────────────┘
//! ```
//!
//! # What this is, and what it is not
//!
//! The third differential in this crate. [`crate::diff`] compares block state,
//! [`crate::tokens`] compares muya's inline token stream, and this one compares
//! the **highlight spans of a fenced code block** — the artifact D15 routes into
//! `mt-layout` as `StyleRun`s. The three share [`crate::diff::first_difference`],
//! the Node-invocation shape and the engine-unavailable convention, and nothing
//! else.
//!
//! # Why it exists, and why it is written before the thing it judges
//!
//! M3 §7's finding is that for the first time in this project *"does it match
//! MarkText?" cannot be answered by running MarkText*. S3 is the one exception
//! §6 names: the reference for syntax highlighting is Prism's own tokenization,
//! Prism is a Node module in the reference clone, and the harness shape that
//! runs it already exists in two working instances. So S1's and S2's *"check the
//! emitted thing against the reference before the artifact freezes it"* stops
//! being a manual cross-check and becomes the gate itself, on **every fence**.
//!
//! §6's "S3 is open" record fixes the order that follows from that: **harness →
//! interpreter → differential green → seam into `mt-layout` → regenerate goldens
//! → read them**, in separate commits. *"Building the harness before the
//! interpreter is the whole point: an interpreter written first is an
//! interpreter whose first check is the thing it was written to satisfy."*
//!
//! **This module was the first of those commits, and it reported zero
//! coverage** — [`PORTED`] was empty, `mt-highlight` was a stub, and every fence
//! came back `NOT-PORTED`. The interpreter commit that followed took it to
//! **16/297**: 2,504 fences compared, 2,504 agreeing, 0 disagreeing. §6 names
//! the hazard that a green line like that creates: *"a differential that runs is
//! indistinguishable in a summary from a differential that was skipped, and both
//! look like a passing stage. S3's record states what was compared, how many
//! cases, and against what — or it has not discharged the gate."* So the report
//! below prints, on every run:
//!
//! - how many fences were collected and how many **distinct** `(lang, code)`
//!   inputs they are;
//! - how many the reference engine tokenized, how many it has no grammar for,
//!   and how many spans it produced in total;
//! - how many this side compared, and coverage as **N/297**.
//!
//! # Two numbers this stage inherits, one of which does not survive measurement
//!
//! §6 hands S3 two figures and says neither may be quietly restated: the gate
//! runs *"over every fence in the corpus"*, which is **2,512** fences, of which
//! *"only ~54 are distinct"* and **97.8 %** are generated `rust` filler.
//!
//! This runner does **both** halves and refuses to pick one. It sends every
//! fence, because narrowing a gate on the grounds that it is slow is the move
//! this project's method exists to prevent — and it prints the distinct-input
//! count on the same line, because *"2,512 fences agree" claims coverage that 54
//! distinct inputs did not buy*. Every number in its output is measured here
//! rather than transcribed, and two of them come out differently:
//!
//! | §6 | Measured | Why |
//! |---|---:|---|
//! | 2,512 fences | **2,507** code blocks | The five ```` ```mermaid ````-style fences of `block-kinds.md` parse as `Block::Diagram`, not `Block::CodeBlock`. They are counted separately and named in the output rather than dropped — see [`Corpus::diagrams`] — and D11 puts them in `mt-layout`. 2,507 + 5 = 2,512 exactly. |
//! | ~54 distinct | **2,448** distinct | §6's figure is the count of **hand-written** fences (`50-code-fences.md`'s 51, plus three elsewhere), not of distinct inputs. `corpus.rs`'s generator varies an identifier and a number per fence, so 2,399 of the 2,458 `rust` fences are distinct *by bytes* while all of them are one shape. |
//!
//! The second correction does not rescue the caveat, it relocates it: **the
//! diversity §6 was worried about really is absent**, and distinct-by-bytes is
//! simply the wrong instrument for saying so. So the report prints the busiest
//! language's share **and** its distinct-body count on one line, which is the
//! measured form of the claim §6 was making.
//!
//! # The language of a fence has exactly one source
//!
//! [`mt_doc::Block::highlight_language`] — the **first word** of the info
//! string, `None` for a bare fence, `None` for every non-`CodeBlock` variant.
//! That is `mt-highlight`'s stated contract (*"Never take the whole info string
//! as a language name"*) and this harness may not have a second opinion: a
//! harness that resolved languages its own way would report disagreements that
//! belong to itself.
//!
//! **It is deliberately not [`mt_layout::code_language`]**, which is a wider
//! question — D11's mapping, which also names `latex` for a math block, `html`
//! for an HTML block and a diagram kind for a diagram. Those are not fences,
//! they carry no info string, and D11 puts that mapping in `mt-layout` on
//! purpose. Widening this harness to them would be a change to what S3's gate
//! covers, and it is not this stage's to make quietly.
//!
//! # What the reference is
//!
//! `tools/diff/dump-prism-tokens.mjs`, whose header carries the parts that
//! cost a measurement: the full 297-language load (D14 consequence 2 — a
//! grammar's content depends on the load set, so the load set is part of the
//! contract), MarkText's alias table, the two places muya patches Prism, and
//! the flattening rule that turns Prism's nested `Token` tree into the
//! non-overlapping ascending run list D15 requires. None of that is repeated
//! here; that file is the one place it lives.
//!
//! # Where the seam for the interpreter is
//!
//! [`rust_spans`], and it was the only place: the interpreter commit changed
//! that one function and added one manifest edge, and nothing else here changed
//! shape.
//!
//! [`PORTED`] is the ratchet §5's `mt-highlight` header asks for — *"let
//! `cargo xtask highlight` count coverage as a number that only goes up"* — and
//! it is a plain list rather than a generated file so that adding a language is
//! a reviewable one-line diff. **It now drives the port as well as the
//! measurement**: `cargo xtask grammars` generates `mt-highlight`'s tables from
//! this same list, so a name cannot be added here without the grammar it names
//! being emitted, and cannot be emitted without this harness comparing it. The
//! two halves of "ported" are one list, which is the only arrangement under
//! which the number means anything.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use mt_doc::{Block, Document, NodeId};
use serde_json::{Value, json};

/// Exit code the dumper uses for "the reference engine is not available".
/// Same convention as its two siblings; see [`crate::diff`].
const EXIT_UNIMPLEMENTED: i32 = 3;

/// Prism's language count, and the denominator of the coverage ratchet.
///
/// **297, not the 298 keys in `components.json`**: the 298th is `meta`, which
/// is not a language but the path template every other entry is expanded
/// against. Of the 297, D14 measured that **292** register a grammar under
/// their own id; the other five are modifier packs. The denominator stays 297
/// rather than 292 because that is the number `mt-highlight`'s header, §5 D3 and
/// §4 C11 are all written in terms of, and a ratchet whose denominator moves is
/// not a ratchet. The dumper reports both counts in its envelope and this runner
/// checks them, so the day the reference clone's prismjs changes, the run says
/// so instead of a constant lying.
const PRISM_LANGUAGES: usize = 297;

/// The Prism languages `mt-highlight` has a ported grammar for.
///
/// **This list is the ratchet, and it drives two things rather than one.**
/// `cargo xtask grammars` generates `crates/mt-highlight/src/generated.rs` from
/// exactly these names (plus everything they reach through `inside`), and this
/// harness compares exactly these names against Prism. So a language is here if
/// and only if the differential agreed with Prism on **every corpus fence in
/// it**, span for span — and adding a name without regenerating produces a
/// missing grammar, which [`rust_spans`] turns into a panic rather than into a
/// quiet `NotPorted`.
///
/// **These are resolved ids, not fence words.** The corpus writes ```` ```ts
/// ````, ```` ```js ```` , ```` ```sh ```` and ```` ```html ````; MarkText's
/// `transformAliasToOrigin` answers `typescript`, `javascript`, `bash` and
/// `markup`, and coverage is a question about the grammar, not about the word.
///
/// The order is D3's *"port languages in usage order"*, which for this corpus
/// is the sixteen languages it actually contains — there is no seventeenth to
/// choose between.
pub(crate) const PORTED: &[&str] = &[
    "rust",
    "javascript",
    "typescript",
    "python",
    "go",
    "c",
    "cpp",
    "java",
    "bash",
    "yaml",
    "json",
    "toml",
    "markup",
    "css",
    "sql",
    "diff",
];

// ---------------------------------------------------------------------------
// The inputs
// ---------------------------------------------------------------------------

/// One fenced code block from the corpus.
///
/// `lang` is the empty string for a fence with no info string. It is kept as an
/// input rather than filtered out: a bare fence is a fence, the gate says *every
/// fence*, and the reference reports it as `noGrammar` — which is the same
/// answer MarkText gives it, and a filtered input is one nobody ever checks.
pub struct Fence {
    /// `50-code-fences.md#12 ts`, for `--only` and for failure lines.
    pub label: String,
    /// [`mt_doc::Block::highlight_language`], or `""`.
    pub lang: String,
    /// The fence's text, exactly as `mt-layout` would shape it: D15 notes that a
    /// code block's visible-text map is `VisibleTextMap::identity(text.len())`,
    /// so these byte offsets are already offsets into the shaped text and no
    /// conversion happens at that seam.
    pub code: String,
}

/// What [`collect`] found: the fences, and what it deliberately did not take.
pub struct Corpus {
    pub fences: Vec<Fence>,
    /// Fence-shaped blocks the parser classified as **diagrams**, by kind.
    ///
    /// These are the ```` ```mermaid ````-style fences of `block-kinds.md`.
    /// They are counted and named rather than silently dropped, because a raw
    /// count of ```` ``` ```` runs over `bench/corpus/` is larger than the
    /// number of code blocks by exactly this many, and a reader comparing the
    /// two numbers deserves the reason instead of a discrepancy. D11 routes
    /// them through `mt_doc::DiagramKind` inside `mt-layout`, they carry no
    /// info string once parsed, and `Block::highlight_language` answers `None`
    /// for every one of them — so they are not this harness's to send.
    pub diagrams: BTreeMap<&'static str, usize>,
}

/// Every `Block::CodeBlock` in the corpus, in file order then document order.
///
/// The corpus file list and the per-input parse options are [`crate::layout`]'s,
/// reused rather than re-derived: two harnesses reading `bench/corpus/` through
/// two different definitions of which files count and how they parse is a
/// difference that would eventually be mistaken for a finding.
fn collect(repo_root: &Path) -> Result<Corpus, String> {
    let mut fences = Vec::new();
    let mut diagrams: BTreeMap<&'static str, usize> = BTreeMap::new();
    for path in crate::layout::inputs(repo_root)? {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        // Same CRLF normalisation the layout harness applies, and for the same
        // reason: a checkout that rewrote line endings is not a finding, and
        // offsets measured against one checkout must mean the same thing in
        // another.
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
            .replace("\r\n", "\n");
        let (options, _label) = crate::layout::parse_options(&name);
        let parsed = mt_md::parse(&text, options);

        let mut ordinal = 0usize;
        visit(&parsed.document, parsed.document.root(), &mut |block| {
            if let Block::Diagram { kind, .. } = block {
                *diagrams.entry(kind.info_lang()).or_default() += 1;
                return;
            }
            if !matches!(block, Block::CodeBlock { .. }) {
                return;
            }
            let lang = block.highlight_language().unwrap_or_default().to_string();
            let code = block
                .text()
                .map(|t| t.to_str().into_owned())
                .unwrap_or_default();
            let shown = if lang.is_empty() { "«none»" } else { &lang };
            fences.push(Fence {
                label: format!("{name}#{ordinal} {shown}"),
                lang,
                code,
            });
            ordinal += 1;
        });
    }
    Ok(Corpus { fences, diagrams })
}

/// Depth-first over the block tree, parents before children.
///
/// A fence is not necessarily a top-level block — `block-kinds.md` has them
/// inside containers — so this walks rather than iterating the root's children.
fn visit(document: &Document, node: NodeId, f: &mut impl FnMut(&Block)) {
    if let Some(block) = document.block(node) {
        f(block);
    }
    for child in document.children(node) {
        visit(document, *child, f);
    }
}

// ---------------------------------------------------------------------------
// The reference side
// ---------------------------------------------------------------------------

/// The envelope `dump-prism-tokens.mjs` returns, reduced to what is read here.
struct Reference {
    /// `prismjs 1.30.0 (components/index.js, full load)`.
    engine: String,
    /// How many languages the dumper loaded, and how many registered a grammar.
    languages: usize,
    grammars: usize,
    /// The two muya fork patches, echoed so a run records which reference it
    /// measured against rather than which one it assumed.
    patches: Vec<String>,
    /// One entry per input, in order.
    results: Vec<Value>,
}

/// Ask Prism for every fence's spans in one process.
///
/// The outer `Result` is "did the harness itself work"; `Ok(Err(reason))` is
/// "the reference engine was not available", which is a skip rather than a
/// failure unless `--require-ts`. Same split, and the same reason for it, as
/// [`crate::diff`] and [`crate::tokens`].
fn dump_prism(repo_root: &Path, fences: &[Fence]) -> Result<Result<Reference, String>, String> {
    // Under `target/` rather than a system temp directory, and deliberately not
    // deleted: a failed run leaves the exact inputs and the exact reference
    // output on disk beside the build, which is what makes a disagreement
    // reproducible by hand.
    let dir = repo_root.join("target").join("xtask");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let inputs_path = dir.join("highlight-inputs.json");
    let out_path = dir.join("highlight-dump-prism.json");

    let payload = Value::Array(
        fences
            .iter()
            .map(|f| json!({"label": f.label, "lang": f.lang, "code": f.code}))
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
        .join("dump-prism-tokens.mjs");
    // No `--import tsx`, unlike the two siblings: nothing the dumper loads is
    // TypeScript. Bare `node` from PATH, never `npx`, never a shell, and the
    // working directory is the repository root so a relative path in a
    // diagnostic means the same thing on every platform.
    let output = match Command::new("node")
        .current_dir(repo_root)
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
        return Err(format!("dump-prism-tokens.mjs exited {code}:\n{stderr}"));
    }

    let text = std::fs::read_to_string(&out_path)
        .map_err(|e| format!("cannot read {}: {e}", out_path.display()))?;
    let envelope: Value = serde_json::from_str(&text)
        .map_err(|e| format!("dump-prism-tokens.mjs produced invalid JSON: {e}"))?;

    let results = envelope
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| "dump-prism-tokens.mjs envelope has no `results` array".to_string())?;
    // The arity check is not optional. A dumper that silently dropped an input
    // would shift every subsequent comparison by one and report a wall of
    // disagreements that mean nothing.
    if results.len() != fences.len() {
        return Err(format!(
            "dump-prism-tokens.mjs returned {} results for {} inputs",
            results.len(),
            fences.len()
        ));
    }

    let count = |key: &str| -> Result<usize, String> {
        envelope
            .get("prism")
            .and_then(|p| p.get(key))
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .ok_or_else(|| format!("dump-prism-tokens.mjs envelope has no `prism.{key}`"))
    };

    Ok(Ok(Reference {
        engine: envelope
            .get("engine")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        languages: count("languages")?,
        grammars: count("grammars")?,
        patches: envelope
            .get("patches")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        results: results.clone(),
    }))
}

// ---------------------------------------------------------------------------
// The port side — the seam, and nothing behind it yet
// ---------------------------------------------------------------------------

/// The port's spans for one fence, in the dumper's wire shape.
///
/// **This function is the whole seam, and it was the only thing S3's second
/// commit had to change.** It returns `None` when `lang` is not in [`PORTED`];
/// the caller turns that into [`Verdict::NotPorted`] and counts it against
/// coverage rather than treating it as agreement.
///
/// The conversion is the wire shape `dump-prism-tokens.mjs` documents: a JSON
/// array of `{start, end, type}`, byte offsets into `code`, ascending,
/// non-overlapping, one entry per emitted run. An empty result stays `[]` here
/// and [`decide`] normalises the dumper's *absent* `spans` to the same value,
/// because absent and empty are one state and both sides must spell it the same
/// way.
///
/// The name check comes **first**, and the `expect` behind it is the ratchet's
/// teeth: a name added to `PORTED` without a regeneration has no grammar in
/// `mt-highlight`, and this panics rather than reporting agreement, because a
/// coverage number that can be raised by editing a list is not a measurement.
fn rust_spans(lang: &str, code: &str) -> Option<Value> {
    if !PORTED.contains(&lang) {
        return None;
    }
    let spans = mt_highlight::highlight(lang, code).unwrap_or_else(|| {
        panic!(
            "`{lang}` is listed in PORTED but `mt-highlight` has no grammar for it; \
             run `cargo xtask grammars`"
        )
    });
    Some(Value::Array(
        spans
            .iter()
            .map(|s| json!({"start": s.start, "end": s.end, "type": s.class}))
            .collect(),
    ))
}

// ---------------------------------------------------------------------------
// Verdicts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Both engines produced the same spans, boundary for boundary and class
    /// for class.
    Agree,
    /// They differ. Carries the JSON path of the first disagreement, so a
    /// failure names the span and the field rather than dumping two lists.
    Disagree { at: String, ts: String, rs: String },
    /// Prism threw on this input. A finding, not a harness bug — recorded per
    /// input so one bad fence does not blind the rest of a run.
    EngineThrew(String),
    /// Prism has no grammar under this name either. **A coverage fact about the
    /// corpus, not about the port**: `flowchart` and `sequence` are MarkText
    /// diagram kinds with no Prism grammar at all, and nothing will ever
    /// highlight them.
    NoGrammar,
    /// Prism has a grammar and `mt-highlight` does not. The ratchet's own
    /// verdict, and today the only one that fires.
    NotPorted,
}

impl Verdict {
    fn tag(&self) -> &'static str {
        match self {
            Verdict::Agree => "PASS",
            Verdict::Disagree { .. } => "FAIL",
            Verdict::EngineThrew(_) => "ERR ",
            Verdict::NoGrammar => "NOGR",
            Verdict::NotPorted => "PORT",
        }
    }
}

/// One reference result against one fence.
fn decide(fence: &Fence, result: &Value) -> Result<Verdict, String> {
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return Ok(Verdict::EngineThrew(error.to_string()));
    }
    if result.get("noGrammar").and_then(Value::as_bool) == Some(true) {
        return Ok(Verdict::NoGrammar);
    }
    // Absent and empty are one state on both sides — the rule the token
    // harness applies to `highlights`. A fence that Prism tokenized to nothing
    // has no `spans` key, and the port must spell that the same way.
    let ts = result.get("spans").cloned().unwrap_or_else(|| json!([]));
    // `resolved` rather than `fence.lang`: the reference resolves `ts` to
    // `typescript` through MarkText's own alias table, and the port is ported
    // *per grammar*, so coverage is a question about the resolved id.
    let resolved = result
        .get("resolved")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "{}: a result has neither `resolved` nor `error`",
                fence.label
            )
        })?;
    let Some(rs) = rust_spans(resolved, &fence.code) else {
        return Ok(Verdict::NotPorted);
    };
    Ok(match crate::diff::first_difference(&ts, &rs) {
        None => Verdict::Agree,
        Some((at, ts, rs)) => Verdict::Disagree { at, ts, rs },
    })
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `cargo xtask highlight [--require-ts] [--verbose] [--only SUBSTR]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut require_ts = false;
    let mut verbose = false;
    let mut only: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--require-ts" => require_ts = true,
            "--verbose" => verbose = true,
            "--only" => {
                i += 1;
                only = Some(args.get(i).ok_or("--only requires a substring")?.clone());
            }
            other => return Err(format!("unrecognised argument: {other}")),
        }
        i += 1;
    }

    println!("highlight-span differential harness — docs/M3.md §5 D14/D15, §6 S3");

    let Corpus {
        mut fences,
        diagrams,
    } = collect(repo_root)?;
    let total_fences = fences.len();
    if let Some(filter) = &only {
        fences.retain(|f| f.label.contains(filter.as_str()));
        if fences.is_empty() {
            return Err(format!("--only {filter:?} matched no fence"));
        }
    }

    // The distinct count, printed beside the total for §6's reason: *"2,512
    // fences agree" claims coverage that 54 distinct inputs did not buy*. Every
    // fence is still sent — the gate is written over all of them and narrowing
    // it because it is slow is the move this project's method exists to
    // prevent.
    let distinct: BTreeSet<(&str, &str)> = fences
        .iter()
        .map(|f| (f.lang.as_str(), f.code.as_str()))
        .collect();
    // The same question asked of the language alone, which is what the coverage
    // ratchet is denominated in, and of each language's distinct bodies, which
    // is what §6's caveat is actually about.
    let mut per_language: BTreeMap<&str, (usize, BTreeSet<&str>)> = BTreeMap::new();
    for fence in &fences {
        let entry = per_language
            .entry(if fence.lang.is_empty() {
                "«none»"
            } else {
                fence.lang.as_str()
            })
            .or_default();
        entry.0 += 1;
        entry.1.insert(fence.code.as_str());
    }
    // §6's *"97.8 % are generated `rust` filler"*, measured rather than
    // restated: the biggest single language, as a share of the whole.
    let (busiest, busiest_count, busiest_distinct) = per_language
        .iter()
        .max_by_key(|(_, (n, _))| *n)
        .map(|(l, (n, d))| (*l, *n, d.len()))
        .unwrap_or(("«none»", 0, 0));

    println!(
        "fences  {} code block(s){}; {} distinct (language, code) input(s)",
        fences.len(),
        if fences.len() == total_fences {
            String::new()
        } else {
            format!(" of {total_fences} (--only)")
        },
        distinct.len()
    );
    if !fences.is_empty() {
        // Both halves of the same sentence, and the second half is why the
        // first is printed at all. §6 says the distinct count must appear
        // beside the total *because* "2,512 fences agree" claims coverage a
        // handful of distinct inputs did not buy. Distinct-**by-bytes** does
        // not settle that on its own: the corpus generator varies an
        // identifier and a number per fence, so nearly every filler fence is
        // distinct while all of them are one shape. Printing the busiest
        // language's share and its distinct-body count together is the
        // measured form of the caveat.
        println!(
            "        busiest language `{busiest}`: {busiest_count} fence(s) ({:.1} %), \
             {busiest_distinct} distinct bodies",
            100.0 * busiest_count as f64 / fences.len() as f64
        );
        println!(
            "        a fence count is not a coverage number, and distinct-by-bytes is not \
             distinct-by-shape"
        );
    }
    println!(
        "        {} distinct language(s): {}",
        per_language.len(),
        per_language
            .iter()
            .map(|(l, (n, _))| format!("{l}({n})"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    // Only on an unfiltered run. The census is corpus-wide — diagrams carry no
    // label to match `--only` against, because they are not inputs — and a
    // corpus-wide number printed under a filtered heading reads as though the
    // filter had matched it.
    if only.is_none() && !diagrams.is_empty() {
        // The delta between a raw count of fence markers in `bench/corpus/` and
        // the number above, named so that nobody has to rediscover it by
        // subtraction. See `Corpus::diagrams`.
        println!(
            "        {} further fence(s) parse as diagrams, which D11 maps in `mt-layout` and \
             `Block::highlight_language` answers `None` for: {}",
            diagrams.values().sum::<usize>(),
            diagrams
                .iter()
                .map(|(l, n)| format!("{l}({n})"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    if fences.is_empty() {
        println!("nothing to compare");
        return Ok(0);
    }

    let reference = match dump_prism(repo_root, &fences)? {
        Ok(reference) => reference,
        Err(reason) => {
            if require_ts {
                return Err(format!(
                    "the TypeScript reference engine is unavailable and --require-ts was given:\n{reason}"
                ));
            }
            println!("Prism is unavailable — nothing was compared.");
            for line in reason.lines() {
                println!("  {line}");
            }
            println!("Pass --require-ts to make that a failure instead of a skip; CI does.");
            return Ok(0);
        }
    };

    println!("engine  {}", reference.engine);
    println!(
        "        {} language(s) loaded, {} of them register a grammar (D14)",
        reference.languages, reference.grammars
    );
    for patch in &reference.patches {
        println!("        patched {patch}");
    }
    // D14's denominator, checked rather than assumed. A prismjs bump in the
    // reference clone that changes the language count changes what `N/297`
    // means, and the run should say so on the line where it happens.
    if reference.languages != PRISM_LANGUAGES {
        println!(
            "  WARN  the reference loaded {} languages but this runner's coverage \
             denominator is {PRISM_LANGUAGES}; docs/M3.md §5 D14 was written against 297",
            reference.languages
        );
    }

    let mut verdicts = Vec::with_capacity(fences.len());
    for (fence, result) in fences.iter().zip(&reference.results) {
        verdicts.push(decide(fence, result)?);
    }

    // The negative control every harness in this crate owes (M1 S6): a run of
    // "0 disagreements" over 2,512 inputs the reference silently produced
    // nothing for would print the same summary as a real one. This counts what
    // the *reference* emitted, so it is a statement about the engine's output
    // rather than about the port's.
    let mut spans_dumped = 0usize;
    let mut tokenized = 0usize;
    for result in &reference.results {
        if result.get("resolved").is_some() && result.get("noGrammar").is_none() {
            tokenized += 1;
            spans_dumped += result
                .get("spans")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
        }
    }

    let mut disagreeing = Vec::new();
    let mut threw = Vec::new();
    let mut no_grammar: BTreeMap<&str, usize> = BTreeMap::new();
    let mut not_ported: BTreeMap<&str, usize> = BTreeMap::new();
    let mut agreeing = 0usize;
    for (fence, verdict) in fences.iter().zip(&verdicts) {
        let key = if fence.lang.is_empty() {
            "«none»"
        } else {
            fence.lang.as_str()
        };
        match verdict {
            Verdict::Agree => agreeing += 1,
            Verdict::Disagree { .. } => disagreeing.push((fence, verdict)),
            Verdict::EngineThrew(_) => threw.push((fence, verdict)),
            Verdict::NoGrammar => *no_grammar.entry(key).or_default() += 1,
            Verdict::NotPorted => *not_ported.entry(key).or_default() += 1,
        }
    }

    let report = |title: &str, rows: &[(&Fence, &Verdict)], limit: usize| {
        if rows.is_empty() {
            return;
        }
        println!("\n{title} ({}):", rows.len());
        for (fence, verdict) in rows.iter().take(limit) {
            println!("  {}  {}", verdict.tag(), fence.label);
            match verdict {
                Verdict::Disagree { at, ts, rs } => {
                    println!("        at {at}");
                    println!("        prism: {ts}");
                    println!("        rust:  {rs}");
                }
                Verdict::EngineThrew(message) => println!("        {message}"),
                _ => {}
            }
        }
        if rows.len() > limit {
            // No silent caps: say how many were not printed.
            println!("  … {} more not printed (--verbose)", rows.len() - limit);
        }
    };

    let limit = if verbose { usize::MAX } else { 20 };
    report("span disagreements", &disagreeing, limit);
    report("the reference engine threw", &threw, limit);

    let census = |m: &BTreeMap<&str, usize>| {
        m.iter()
            .map(|(l, n)| format!("{l}({n})"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    println!();
    println!(
        "prism   {tokenized}/{} fence(s) tokenized, {spans_dumped} span(s) dumped",
        fences.len()
    );
    if !no_grammar.is_empty() {
        println!(
            "        {} fence(s) have no Prism grammar at all: {}",
            no_grammar.values().sum::<usize>(),
            census(&no_grammar)
        );
    }
    println!(
        "compare {agreeing} agree, {} disagree, {} engine throw(s)",
        disagreeing.len(),
        threw.len()
    );
    if !not_ported.is_empty() {
        println!(
            "        {} fence(s) not compared — the port has no grammar: {}",
            not_ported.values().sum::<usize>(),
            census(&not_ported)
        );
    }
    // The line §6 says the stage's record stands or falls on. It names the
    // denominator, the distinct-input count and the fence count together, so
    // that none of the three can be read as either of the others.
    println!(
        "coverage {}/{PRISM_LANGUAGES} Prism language(s) ported; {agreeing} of {} fence(s) \
         and {} distinct input(s) compared",
        PORTED.len(),
        fences.len(),
        distinct.len()
    );
    if PORTED.is_empty() {
        println!(
            "        `mt-highlight` is a stub and ports nothing yet, so no fence was compared. \
             S3's order of work is harness → interpreter → differential green (docs/M3.md §6), \
             and this is the harness."
        );
    }

    // The floor that stops this harness from degrading into a green no-op while
    // its port side is empty. `blocks.rs` carries the same idea as
    // `LEAVES_FLOOR`, and §6 names the failure mode by hand: *"a differential
    // that runs is indistinguishable in a summary from a differential that was
    // skipped."* With `PORTED` empty, **every** line above except this one would
    // read identically if Prism had quietly stopped emitting.
    //
    // Stated as an invariant rather than as a measured number, because the
    // measured number moves with the corpus and this one cannot: a run that
    // tokenized anything at all in a real language must have produced spans. It
    // is checked only on an unfiltered run — `--only` can legitimately select a
    // single fence that tokenizes to nothing at all.
    if only.is_none() && tokenized > 0 && spans_dumped == 0 {
        return Err(format!(
            "the reference tokenized {tokenized} fence(s) and produced no spans at all; \
             that is a broken reference, not a clean run"
        ));
    }

    // A disagreement or an engine throw fails; an unported language does not,
    // because the ratchet's whole shape is that coverage rises over stages.
    Ok(i32::from(!disagreeing.is_empty() || !threw.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus must actually contain fences, and they must carry languages.
    ///
    /// A collector that silently returned nothing would make every run of this
    /// harness report a clean sweep of zero inputs — the exact failure mode M1
    /// S5 established is real and that §6 warns S3 about by name.
    #[test]
    fn the_corpus_has_fences_in_more_than_one_language() {
        let root = crate::repo_root();
        let fences = collect(&root).expect("corpus fences").fences;
        assert!(
            fences.len() > 1000,
            "the corpus should carry thousands of fences, found {}",
            fences.len()
        );
        let languages: BTreeSet<&str> = fences.iter().map(|f| f.lang.as_str()).collect();
        assert!(
            languages.len() >= 16,
            "the corpus should carry at least 16 fence languages, found {}: {languages:?}",
            languages.len()
        );
    }

    /// Every fence's language is the **first word** of the info string.
    ///
    /// `50-code-fences.md` carries eight fences whose info string is
    /// `ts title="example-36.txt" {.numberLines}` and friends. If this harness
    /// ever fed the whole info string to Prism, every one of them would report
    /// `noGrammar` and the coverage number would quietly drop.
    #[test]
    fn a_fence_language_is_never_a_whole_info_string() {
        let root = crate::repo_root();
        for fence in collect(&root).expect("corpus fences").fences {
            assert!(
                !fence.lang.contains(char::is_whitespace),
                "{}: language {:?} is not a single word",
                fence.label,
                fence.lang
            );
        }
    }

    /// The ratchet cannot be raised by editing a list.
    ///
    /// [`PORTED`] is the coverage numerator and [`rust_spans`] is the thing that
    /// has to be true for it to mean anything. While the list is empty they are
    /// trivially consistent; this test exists so that the day a name is added
    /// without an interpreter, the failure is here rather than in a report that
    /// says `1/297` and compared nothing.
    #[test]
    fn ported_languages_have_an_interpreter() {
        for lang in PORTED {
            assert!(
                rust_spans(lang, "").is_some(),
                "{lang} is in PORTED but rust_spans has nothing behind it"
            );
        }
    }
}
