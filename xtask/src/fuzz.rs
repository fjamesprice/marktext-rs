//! `cargo xtask fuzz-seed` — write `fuzz/corpus/<target>/`.
//!
//! # Why a seed corpus is not optional here
//!
//! `fuzz_targets/tokenize.rs` takes `&str`, and `libfuzzer-sys` implements that
//! by **rejecting** any mutation that is not valid UTF-8. Most byte-level
//! mutations of a multi-byte character are not, so a run starting from nothing
//! spends a large part of its budget rediscovering UTF-8 rather than the
//! tokenizer — and the platform-specific risk D6 names is precisely UTF-8
//! boundary slicing, which needs multi-byte input to reach at all.
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
//! The three targets share one corpus directory each rather than one between
//! them: `autolink` and `backtracking` prepend their own openers, so a seed
//! that is useful to one is a wasted mutation for the other.
//!
//! # Naming
//!
//! Files are `<index>-<sha-free hash>` — content-addressed by a small
//! non-cryptographic hash so that regenerating is idempotent and a diff of the
//! directory shows what actually changed. libFuzzer does not care about names;
//! a human reading `git status` does.

use std::io::Write as _;
use std::path::Path;

use crate::tokens::{Breadth, sweep_inputs};

/// The three targets in `fuzz/fuzz_targets/`.
const TARGETS: [&str; 3] = ["tokenize", "autolink", "backtracking"];

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

/// `cargo xtask fuzz-seed [--check]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let check = args.iter().any(|a| a == "--check");
    for arg in args {
        if arg != "--check" {
            return Err(format!("unrecognised argument: {arg}"));
        }
    }

    let inputs = sweep_inputs(repo_root, Breadth::Default)?;
    // The register's inputs are deliberately **not** seeded. They are the
    // inputs on which the two engines are known to disagree; the fuzzer is
    // looking for panics and lost bytes, which is a different question, and
    // seeding them would suggest otherwise to the next reader.
    println!("fuzz seed corpus — {} inputs per target", inputs.len());

    let mut written = 0usize;
    let mut stale = Vec::new();
    for target in TARGETS {
        let dir = repo_root.join("fuzz").join("corpus").join(target);
        if !check {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        for input in &inputs {
            let name = format!("{:016x}", hash(input.src.as_bytes()));
            let path = dir.join(&name);
            let current = std::fs::read(&path).ok();
            if current.as_deref() == Some(input.src.as_bytes()) {
                continue;
            }
            if check {
                stale.push(format!("{target}/{name}"));
                continue;
            }
            let mut file = std::fs::File::create(&path)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            file.write_all(input.src.as_bytes())
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            written += 1;
        }
        println!("  {target:<14} {}", dir.display());
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
