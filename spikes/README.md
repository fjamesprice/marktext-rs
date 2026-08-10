# `spikes/` — M3 S0's measurement stage, as running code

`docs/M3.md` §6 S0: *"**Measure, then decide.** No product code."* Its gate is
*"every one of D1, D2, D3, D7, D9 taken **with a number attached**"* — and a
number with no program behind it is an estimate with better formatting. This
directory is where the programs live.

Everything here is **evidence, not product**. Nothing in `crates/` depends on
any of it, nothing here ships, and every crate is `publish = false`.

## Why it is outside the root workspace

Two reasons, and the first is mechanical.

`cargo xtask deps` asserts that `crates/` holds exactly the 14 crates
RUST-REWRITE-PLAN.md §1 names — a 15th is a hard failure, not a style note.
`docs/M3.md` §5 D6 records that constraint as the thing that decides where
spikes live. A spike is by definition a crate that should not survive, so it
must not be able to enter that count.

The second reason is the lockfile. M3 shops for `parley`, which brings
`fontique`, `harfrust`, `skrifa`, `read-fonts` and `peniko` — a 0.x graph that
E1 deliberately holds at **two revisions at once** (an exact `=0.11.0` from
crates.io and a pinned git `main`) in order to compare them. Putting that in the
root `Cargo.lock` would place a churning tree in front of every `cargo build` on
three platforms, and would make *"which parley?"* a workspace-wide answer at
exactly the moment the project needs it to be a per-experiment one.

`fuzz/` carries its own `[workspace]` for the same *shape* of reason — a
nightly-only toolchain that must not reach the shipped lockfile. This follows
that precedent rather than inventing one.

**The cost is the same cost `fuzz/` pays**: `cargo test --workspace` at the root
does not build any of this, so a change to `mt-md` or `mt-doc` can break the two
spikes that path-depend on them without CI noticing. Those two are E3 and E4,
they are finished, and nothing re-runs them automatically.

## What each experiment took, and where its answer is

| Crates | Experiment | Result | Decision it took |
|---|---|---|---|
| `e1-parley-{0-11,main}`, `e1-fonts-{0-11,main}` | Inline box inside an RTL run inside an LTR paragraph, on both parley revisions | `results/e1-bidi-inline-box.md` | **M3-R1** (answered yes), **D2**, part of **D7** |
| `e2-*` (11 crates + `e2-common`) | `dist`-profile builds and `cargo bloat`, one per backend/highlighter | `results/e2-binary-size.md` | **§12.2's table**, **D1** |
| `e3-layout-{0-11,main}` | One `parley::Layout` per block over `5mb.md` | `results/e3-layout-scale.md` | **D9**, confirms **C5** |
| `e4-{syntect,treesitter,common}` | Three-way highlighter bake-off | `results/e4-highlight.md` | **D3** |

The Electron baseline (E5) has no crate here — it measures the *other*
implementation — and lands at [`../bench/BASELINE.md`](../bench/BASELINE.md).

## Running them

Each results file prints the exact command line that produced its numbers.
Two things about this workspace will produce wrong answers if ignored, and both
are recorded in `Cargo.toml` beside the thing that causes them:

- **Build the `e2` and `e1-fonts` members one at a time.** Cargo unifies
  features across everything built in one invocation, so `cargo build
  --workspace` hands a `default-features = false` member a `parley` that still
  has `system` enabled, via a sibling that needs it. The "no filesystem I/O"
  claim then measures nothing.
- **Sizes come from `--profile dist`; `cargo bloat` comes from `dist-bloat`.**
  `dist` sets `strip = "symbols"`, and `cargo bloat` cannot attribute what
  `strip` removed. `dist-bloat` is `dist` with stripping off and debug info on,
  for attribution only — **never quote a file size from it.**

## What this directory is not

It is not a benchmark harness. §6 S6 says *"Criterion arrives here if it
arrives at all"*, and when it does it will live with the crate it measures, run
in CI, and guard against regression. These spikes answer questions once, on one
machine, on a stated date, and then stop being true — which is why every results
file states its hardware and pins its dependencies, and why none of them is
wired into CI.

**When a spike's question is settled and its decision is written into
`docs/M3.md`, the crate has done its job.** Deleting one is not a loss of
coverage; the results file is the artifact worth keeping.
