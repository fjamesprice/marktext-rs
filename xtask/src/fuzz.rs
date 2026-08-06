//! `cargo xtask fuzz-seed` — write `fuzz/corpus/<target>/`.
//!
//! # Why a seed corpus is not optional here
//!
//! Every target in `fuzz/fuzz_targets/` takes `&str` (or, for `reparse`, a
//! struct holding them), and `libfuzzer-sys` implements that by **rejecting**
//! any mutation that is not valid UTF-8. Most byte-level mutations of a
//! multi-byte character are not, so a run starting from nothing spends a large
//! part of its budget rediscovering UTF-8 rather than the code under test — and
//! the platform-specific risk M1.md §5 D6 names is precisely UTF-8 boundary
//! slicing, which needs multi-byte input to reach at all.
//!
//! # Why *this* seed corpus
//!
//! It is the differential sweep's input set, unchanged:
//! [`crate::tokens::sweep_inputs`] at [`Breadth::Default`]. Three properties
//! follow from reusing it rather than writing a second one.
//!
//! - **It is real markdown.** Every corpus file and every line of one, the 1324
//!   CommonMark and GFM examples, the eleven round-trip fixtures. libFuzzer's
//!   mutations are far more productive starting from a valid construct than
//!   from random bytes.
//! - **It is already known to agree with muya.** Every seed is an input the
//!   token-stream harness compares on every commit, so the fuzzer starts from a
//!   frontier that is *verified* rather than merely reachable, and anything it
//!   finds is genuinely new.
//! - **It covers the C2 traps and all 26 token types**, because
//!   `sweep_inputs` includes the character-class probes and the per-type
//!   probes. A seed corpus assembled by hand would have neither.
//!
//! The targets share one corpus directory each rather than one between them:
//! `autolink` and `backtracking` prepend their own openers, so a seed that is
//! useful to one is a wasted mutation for the other.
//!
//! # Why the seed set is **per target** since M2 S7
//!
//! `sweep_inputs` is line-shaped: its bulk is every *line* of every corpus file,
//! because `mt_inline::tokenize` tokenizes a **leaf block's** text and a line is
//! the closest analogue of one. That is exactly right for the three `mt-inline`
//! targets and exactly wrong for the three `mt-md` ones — a block-structure
//! target seeded with single lines starts from a frontier with **no block
//! structure in it**, and would spend its first hours discovering that a `>` on
//! one line and text on the next make a block quote.
//!
//! So `parse`, `round_trip` and `reparse` are seeded with [`document_inputs`]:
//! whole documents, and nothing but. Same corpora, different granularity — the
//! three properties above hold of it for the same reasons, and the fourth is
//! that a document is what those three targets are *about*.
//!
//! **`1mb.md` and `5mb.md` are excluded**, and that is a decision rather than an
//! omission: `.github/workflows/soak.yml` passes `-max_len=65536`, so a 5 MB
//! seed is a seed libFuzzer will read, truncate and never mutate productively,
//! while costing a 5 MB file in every shard's restored cache. `250kb.md` is kept
//! — it is over the limit too and libFuzzer will truncate it, but 64 KB of the
//! long-prose shape is still the long-prose shape, and it is the only corpus
//! file that has it. Both large files remain fully checked elsewhere:
//! `crates/mt-md/tests/round_trip.rs` round-trips them whole on every commit and
//! `cargo xtask diff` runs the block engine over both.
//!
//! # Naming
//!
//! Files are `<sha-free hash>` — content-addressed by a small non-cryptographic
//! hash so that regenerating is idempotent and a diff of the directory shows
//! what actually changed. libFuzzer does not care about names; a human reading
//! `git status` does.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::tokens::{Breadth, sweep_inputs};

/// Which of the two seed sets a target takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seeds {
    /// [`crate::tokens::sweep_inputs`] — line-shaped, for the `mt-inline`
    /// targets. **Unchanged since M1 S7**: the three targets that had it keep
    /// exactly what they had, so an S7 change to the seeding cannot be mistaken
    /// for a change in what the M1 gate has been running against all along.
    InlineSweep,
    /// [`document_inputs`] — whole documents, for the `mt-md` targets.
    WholeDocuments,
}

/// The six targets in `fuzz/fuzz_targets/`, and what each is seeded from.
///
/// `known_panics.rs` is not here and is not a target — see `fuzz/Cargo.toml`.
const TARGETS: [(&str, Seeds); 6] = [
    ("tokenize", Seeds::InlineSweep),
    ("autolink", Seeds::InlineSweep),
    ("backtracking", Seeds::InlineSweep),
    ("parse", Seeds::WholeDocuments),
    ("round_trip", Seeds::WholeDocuments),
    ("reparse", Seeds::WholeDocuments),
];

/// The two files whose whole-document seed [`document_inputs`] omits, and the
/// README that is documentation rather than corpus.
const NOT_SEEDED: [&str; 3] = ["1mb.md", "5mb.md", "README.md"];

/// FNV-1a, 64-bit. Four lines rather than a dependency, for the same reason
/// `corpus.rs` hand-rolls its xorshift.
fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Whole documents: `bench/corpus/`, the round-trip fixtures, and the 1324
/// CommonMark and GFM examples.
///
/// **Not their lines.** That is the whole difference from
/// [`crate::tokens::sweep_inputs`] and the reason this function exists; the
/// module doc has the argument.
///
/// # The denominator, and why it is not 1346
///
/// It starts as the one `crates/mt-md/tests/round_trip.rs` states its gate over
/// — 11 corpus files + 11 fixtures + 1324 spec examples = **1346** — less
/// [`NOT_SEEDED`]'s two large files and `bench/corpus/empty.md` (libFuzzer
/// starts from the empty input regardless, so seeding it is a file that buys
/// nothing), which leaves **1343** candidates. Deduplicated by content it is
/// **712**: 19 whole files, plus **693 distinct spec examples of the 1324**,
/// because GFM 0.29 restates most of CommonMark 0.29's examples verbatim and
/// `round_trip.rs` counts them twice on purpose — the same bytes at two
/// *names*, driven as two tests. A fuzzer has no use for the second copy.
fn document_inputs(repo_root: &Path) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();

    // bench/corpus/, whole.
    let corpus = repo_root.join("bench").join("corpus");
    let mut corpus_files: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .map_err(|e| format!("cannot read {}: {e}", corpus.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "md"))
        .filter(|path| {
            !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| NOT_SEEDED.contains(&n))
        })
        .collect();
    corpus_files.sort();

    // spec/fixtures/marktext-round-trip/, whole — real documents with front
    // matter, tables and nested containers, which is what the spec examples are
    // deliberately not.
    let fixtures = repo_root.join("spec").join("fixtures");
    let mut fixture_files = Vec::new();
    collect_markdown(&fixtures.join("marktext-round-trip"), &mut fixture_files)?;
    fixture_files.sort();

    for path in corpus_files.into_iter().chain(fixture_files) {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if !text.is_empty() {
            out.push(text);
        }
    }

    // The spec examples. Each is a whole document in its own right — that is
    // what a CommonMark example *is* — so they belong in this set as well as in
    // the line-shaped one.
    for file in ["commonmark-spec-0.31.json", "gfm-spec-0.29-gfm.json"] {
        let path = fixtures.join(file);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let examples: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        for example in examples
            .as_array()
            .ok_or_else(|| format!("{}: not a JSON array", path.display()))?
        {
            if let Some(markdown) = example.get("markdown").and_then(Value::as_str)
                && !markdown.is_empty()
            {
                out.push(markdown.to_string());
            }
        }
    }

    // Deduplicate: a round-trip fixture and a spec example are occasionally the
    // same bytes, and two seeds with the same content are one seed and one
    // wasted file. Content-addressed naming would collapse them on disk anyway;
    // deduplicating here is what makes the printed count honest.
    let mut seen = std::collections::HashSet::new();
    out.retain(|src| seen.insert(src.clone()));
    Ok(out)
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
            out.push(path);
        }
    }
    Ok(())
}

/// `cargo xtask fuzz-seed [--check]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let check = args.iter().any(|a| a == "--check");
    for arg in args {
        if arg != "--check" {
            return Err(format!("unrecognised argument: {arg}"));
        }
    }

    let inline: Vec<String> = sweep_inputs(repo_root, Breadth::Default)?
        .into_iter()
        .map(|input| input.src)
        .collect();
    let documents = document_inputs(repo_root)?;
    // The register's inputs are deliberately **not** seeded, in either set.
    // They are the inputs on which the two engines are known to disagree; the
    // fuzzer is looking for panics, lost bytes and a round trip that will not
    // settle, which is a different question, and seeding them would suggest
    // otherwise to the next reader.
    println!("fuzz seed corpus");
    println!(
        "  {:<16} {:>5} inputs — tokenize, autolink, backtracking",
        "inline sweep",
        inline.len()
    );
    println!(
        "  {:<16} {:>5} inputs — parse, round_trip, reparse",
        "whole documents",
        documents.len()
    );

    let mut written = 0usize;
    let mut stale = Vec::new();
    for (target, seeds) in TARGETS {
        let inputs = match seeds {
            Seeds::InlineSweep => &inline,
            Seeds::WholeDocuments => &documents,
        };
        let dir = repo_root.join("fuzz").join("corpus").join(target);
        if !check {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        for src in inputs {
            let name = format!("{:016x}", hash(src.as_bytes()));
            let path = dir.join(&name);
            let current = std::fs::read(&path).ok();
            if current.as_deref() == Some(src.as_bytes()) {
                continue;
            }
            if check {
                stale.push(format!("{target}/{name}"));
                continue;
            }
            let mut file = std::fs::File::create(&path)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            file.write_all(src.as_bytes())
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            written += 1;
        }
        println!("  {target:<16} {}", dir.display());
    }

    if check {
        if stale.is_empty() {
            println!("  up to date");
            return Ok(0);
        }
        println!("  {} seed(s) missing or stale, e.g.:", stale.len());
        for name in stale.iter().take(5) {
            println!("    {name}");
        }
        println!("  Run: cargo xtask fuzz-seed");
        return Ok(1);
    }

    println!("  {written} file(s) written");
    Ok(0)
}
