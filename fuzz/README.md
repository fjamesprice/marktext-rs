# `fuzz/` — the M1 and M2 exit gates' soak

RUST-REWRITE-PLAN.md §9's M1 exit gate: *"Fuzzer runs 24 h without a panic."*
§11.3: *"`cargo-fuzz` on `mt-inline` and the HTML-paste path"* and *"`cargo-fuzz`
on the parse/serialize path. Malformed input must never panic — `panic = "abort"`
makes a panic a crash."*

`docs/M1.md` §5 D6 decides where it runs:

> `cargo-fuzz`/libFuzzer support on Windows MSVC is thin. **Recommendation:**
> run the 24 h libFuzzer soak on Linux in CI, and run `proptest` on all three
> platforms in the normal test job.

`docs/M2.md` §6's S7 row is the second half: *"round-trip property tests and
`cargo-fuzz` on the parse/serialize path (§11.3); wire both into `soak.yml`
beside `mt-inline`'s three targets."*

**The package is `mt-fuzz`.** It was `mt-inline-fuzz` until M2 S7, when it stopped
being about one crate: it now holds six targets across `mt-inline`, `mt-md` and
`mt-doc`. `publish = false`, so nothing depends on the name and the rename cost
this paragraph.

## The six targets

| Target | Pointed at | Why it is separate |
|---|---|---|
| `tokenize` | `mt-inline`, everything | The M1 gate itself: any UTF-8 string, every invariant. |
| `autolink` | `trim_auto_link_extent` | **The one thing `BACKTRACK_LIMIT` does not bound** — hand-written Rust, quadratic in the extent (M1 S5). A hang here has no reproducer unless something asserts. |
| `backtracking` | the seven ReDoS-class rules | M1.md §4 C1: *"it is what the M1 fuzzing gate should be pointed at first."* Every input opens a quantifier that must scan to the end of the level before it can fail. |
| `parse` | `mt_md::parse` and every consumer | The M2 gate itself. Totality at both option sets, every `SourceMap` range in bounds and on a `char` boundary, one entry per live node, and `Incremental::new == parse`. It is the expensive one — a case is two option sets × (parse, serialize, `dump_state`, two `render_to_static_html` walks and an `Incremental`) — which is why it is not folded into `round_trip`. |
| `round_trip` | `serialize ∘ parse`, iterated | A *different question* from `parse`'s, and one nothing else asks: does the document stop moving. See below — it needs its own corpus because the inputs that make bytes settle slowly are not the inputs that make a parse interesting. |
| `reparse` | `Incremental::edit` sequences | The only target with **structured** input: a document and a list of `(at, remove, insert)` edits, derived through `arbitrary`. A `&str` target cannot reach S6's region reparse at all, because there is no edit in a string. |

`autolink` and `backtracking` prepend their own openers, so the fuzzer spends its
budget on the construct rather than on finding the opener. That is the whole
difference between them and `tokenize`, and it is worth three targets because a
general fuzzer reaches `trim_auto_link_extent` by accident.

`fuzz_targets/known_panics.rs` is **not** a target — see "The two known
non-bugs" below.

## Two seed sets, one per layer

`cargo xtask fuzz-seed` writes `fuzz/corpus/<target>/`, and since M2 S7 the seed
set is chosen per target:

| Targets | Seeds | Count |
|---|---|---:|
| `tokenize`, `autolink`, `backtracking` | `xtask::tokens::sweep_inputs` at `Breadth::Default` — every corpus **line**, the spec examples, the C2 probes, the per-type probes | 4,237 |
| `parse`, `round_trip`, `reparse` | whole **documents**: `bench/corpus/`, the round-trip fixtures, the spec examples | 712 |

A line is the closest analogue of the leaf block `mt_inline::tokenize` is handed,
which is why the first set is line-shaped and why M2 S7 did not change it. It is
the wrong shape for the second: **a block-structure target seeded with single
lines starts from a frontier with no block structure in it**, and would spend its
first hours discovering that a `>` on one line and text on the next make a block
quote.

712 rather than `round_trip.rs`'s 1346 because `1mb.md` and `5mb.md` are excluded
(`-max_len=65536` — a 5 MB seed is one libFuzzer truncates and never mutates
productively) and because the corpora are deduplicated by content: GFM 0.29
restates most of CommonMark 0.29's examples verbatim, so the 1324 spec examples
are **693 distinct documents**. `xtask/src/fuzz.rs` carries the arithmetic.

## Exceeding the backtrack limit is not a bug

`mt_inline::rules::BACKTRACK_LIMIT` is 1,000,000 steps **per rule attempt** and
exceeding it means *the rule did not match*, never a panic — M1.md §4 C1 decided
that, and the first of its three reasons is that the exit gate is panic-freedom
and a resource limit that panics **is** the panic the gate forbids.

So an input that reaches the limit is a `spec/divergences.json` entry like any
other deliberate difference, not something to "fix". What the targets assert is
that the tokenizer stays **total**: a rule that gives up leaves its characters to
accumulate as text, so `generator(tokenize(s)) == s` holds either way.

## The round trip is not a fixed point, so no target asserts that it is

`crates/mt-md/tests/round_trip.rs` — M2 S4's gate — asserts
`parse(serialize(parse(s))) == parse(s)` over a fixed 1346-input corpus with
**22 enumerated exceptions**, and `serialize(parse(s)) == s` with **348**. Those
are not port defects: for every fixed-point exception the port's output is
byte-identical to `ExportMarkdown.generate`'s and **muya's own round trip is not
a fixed point on them either**. A fuzz target asserting the fixed point would
report failures on inputs working exactly as designed, and would keep reporting
them until somebody deleted the property or diverged from the engine M2 is
porting.

**What `round_trip` asserts instead is that the bytes settle.** With `m0 = s` and
`m(k+1) = serialize(parse(mk))`, the claim is `m4 == m5` — *"settles after four
applications"* is exactly *"the fifth changes nothing"*. That is the property an
editor actually needs, since repeated open/save must not drift and must not
oscillate.

**Four is measured, not chosen.** Over the fixed 1346, `m2 == m3` holds 1346 of
1346. Over 466,909 *generated* documents it does not: 458,272 settle after one
application, 8,317 after two, 316 after three, **4 after four**, and none after
more. `SETTLES_BY` is that observed maximum, in this target and in
`crates/mt-md/tests/round_trip_properties.rs`, and the two must agree — two
claims about one behaviour that differ by a fuzz target's convenience are one
claim and one hole. Raising it is not a free way to make a failure go away: a
document that needs five passes to stop moving is a document a user can watch
change under them on the fourth save.

Three families are excluded **by name**, each pinned next door by a ratchet test
asserting the behaviour it was excluded for:

1. the upstream panic below;
2. a code fence whose info string holds a backtick — `n` of them need exactly
   `n + 2` applications, so the family has **no** constant bound and excluding it
   is the only true statement about it;
3. `"> - [a]: /x\n>   * "`, which **oscillates with period two** — tight ↔ loose
   forever, while muya settles on its first pass. That one is a port defect and
   should be read as a finding rather than as a guard; M2 S7 reports it because
   confirming a repair needs a serializer differential runner against
   `ExportMarkdown` that this repository does not yet have.

## The two known non-bugs — one repaired, one guarded

`docs/M2.md` §10, *"Owed by S6"*, records two inputs on which the parse path does
not behave as the gate's plain reading would want. Neither is fixed here, and the
reasons differ.

**(a) `pulldown-cmark` 0.13.4 panics.** `Option::unwrap()` on `None` inside
`OffsetIter::next` (`parse.rs:2199`), reached before any of this repository's
code runs, on a document holding a list item, a link reference definition, and a
following line that is whitespace-only and not all spaces — `"> - [a]: /x\n\t"`
is §10's reproducer and `"- [a]:x\n\u{b}"` is the wider class S7 measured. 0.13.4
is the latest published version, the release profile is `panic = "abort"`, and
`libfuzzer-sys` installs a panic hook that aborts, so it can be neither upgraded
away nor caught.

**§10 defers the decision to M4** — file upstream, pin a fork, or pre-scan — and
M2 S7 is not that stage. What S7 owes is that the nightly soak is not permanently
red on a deferred upstream defect, so `fuzz_targets/known_panics.rs` holds a
single predicate that every `mt-md` target calls first and returns early on.
**This is not a fix.** It is a `mod` and not a `[[bin]]` so cargo-fuzz does not
treat it as a target, and it is narrow — it skips **89 of 1,600** generated
documents, measured. A check in the same file asserts that it still fires on ten
recorded reproducers and still declines nine near-misses, so the day upstream
fixes `parse.rs:2199` the guard is discoverably dead. That check runs **once per
process**, at the start of every soak shard, rather than as a `#[test]`: a
`#[cfg(test)]` module inside a `test = false` fuzz target is dead code, and a
`[[test]]` target cannot be run either because `cargo test` builds every
`#![no_main]` bin in the package and those link only under cargo-fuzz's flags.
The authoritative ratchet remains
`crates/mt-md/tests/round_trip_properties.rs`'s
`the_upstream_panic_guard_covers_the_recorded_shape_and_the_wider_class`, which
`cargo test --workspace` runs on three platforms on every push.

**(b) A child's source range can escape its parent's**, on
`"-\t- [a]: /x[xter\n"`. A tab inside a list item's marker padding is the one
case where a stripped line's text is not a slice of the source, so the scanner's
range is computed in stripped space and lands one byte to the left of its item.
`crates/mt-md/tests/source_ranges.rs` asserts nesting **over the corpus**, which
contains no such document.

**Therefore no `mt-md` target asserts that source ranges nest.** `tokenize.rs`
*does* assert it for `mt-inline` tokens, and that is correct there — a token tree
is built by slicing the input. The difference is stated in the module docs of
both `parse.rs` and `reparse.rs` so that the next reader does not "restore" it.

**There is no third guard.** §10's other panic was this port's own —
`block.rs`'s dedent cut a whitespace-only continuation line at a byte that need
not be a `char` boundary — and **M2 S7 repaired it** rather than guarding it,
because §11.3's gate row is the property these targets exist to assert and a
guard would have been the fuzzer excusing the crate from it.

## Assertions are the point

`Cargo.toml` sets `debug-assertions = true` and `overflow-checks = true` in the
release profile. M1.md §6 records four branches `mt-inline` proves unreachable
and reproduces anyway, each with the proof as a test:

| # | Branch | Stage |
|---|---|---|
| 1 | `validateEmphasize`'s rule-16 guard (`SHORTER && !CLOSE`) | M1 S2 |
| 2 | `correctUrl` receiving a non-empty group 5 | M1 S3 |
| 3 | `tryAutoLinkExtension`'s `if (!email)` | M1 S5 |
| 4 | `tokensToPlainText`'s `html_tag` `else if (token.content)` | M1 S6 |

M1 S7 turned each proof into a `debug_assert!` at the site, so **a fuzzer
reaching one means a proof is wrong** and says so with a reproducer. A profile
with assertions compiled out would run faster and check strictly less — and
"less" here is exactly the four claims nobody has been able to break by hand. A
fifth guards `trim_auto_link_extent` returning 0, whose consequence is a hang
rather than a wrong token.

`overflow-checks` earns its place at the `mt-md` layer for a reason of its own:
the block layer's container-prefix stripper is column arithmetic on `usize`, and
an underflow there is a range that wraps to something enormous — a wrong answer
with no crash — unless the check is on.

## Running it

Needs a **nightly** toolchain: `cargo fuzz` uses `-Zsanitizer` and
`-Cpasses=sancov-module`. This directory is its own workspace so that nightly
requirement stays out of the repository's `rust-toolchain.toml`, which pins
stable.

```sh
rustup toolchain install nightly
cargo install cargo-fuzz

# Seed both sets — the sweep's lines for the three inline targets, whole
# documents for the three mt-md ones.
cargo xtask fuzz-seed

# From the REPOSITORY ROOT, not from this directory: cargo-fuzz locates the
# fuzz crate as a `fuzz/` subdirectory of wherever it is invoked, so running it
# in here makes it look for `fuzz/fuzz/` and report no targets.
cargo +nightly fuzz run tokenize   -- -max_total_time=3600 -report_slow_units=5
cargo +nightly fuzz run round_trip -- -max_total_time=3600 -max_len=65536
```

`-report_slow_units` is what makes a soak produce a *benchmark* row rather than
only a pass/fail: whatever it reports slowest belongs in
`crates/mt-inline/benches/tokenizer.rs`, tagged with which of the three cost
terms it exercises — how far a lazy quantifier scans when it fails, a flat VM
setup per attempt, or how far a greedy quantifier scans before the character that
must follow it is absent — or as a fourth. Expect it to fire on `parse` more
often than on the inline targets, for the reason its row in the table gives; a
slow unit there is a benchmark candidate, not a hang.

### On a machine without nightly

`crates/mt-inline/tests/properties.rs` and
`crates/mt-md/tests/round_trip_properties.rs` each carry a `soak` test that runs
the same properties over the same generators on **any** platform and any
toolchain:

```sh
MT_SOAK_SECONDS=3600 RUSTFLAGS="-C debug-assertions=yes" \
  cargo test -p mt-inline --release --test properties -- --ignored soak --nocapture

MT_SOAK_SECONDS=3600 RUSTFLAGS="-C debug-assertions=yes" \
  cargo test -p mt-md --release --test round_trip_properties -- --ignored soak --nocapture
```

They are coverage-**blind** where libFuzzer is coverage-guided, and
structure-**aware** where libFuzzer starts from bytes. Neither subsumes the
other. They exist because §9 says Windows ships first, and a gate met only on
Linux is a gate met on the platform whose failures block least.

`-C debug-assertions=yes` is not optional — see above.

## After changing a public API

`cargo test --workspace` does **not** compile these targets: `fuzz/` is a
separate workspace, which is the price of keeping nightly out of the main build.
So a rename in `mt-inline`, `mt-md` or `mt-doc` breaks them silently. Run:

```sh
cargo +nightly fuzz build     # from the repository root
```

`.github/workflows/soak.yml` does this on every dispatch, before the matrix, so a
broken target fails in one minute rather than four hours in. Its cache key hashes
all three crates for the same reason: a key naming one of them would restore a
stale `fuzz/target` and skip the very build the job exists to run.

## The 48 CPU-hours, and GitHub's 6-hour job limit

A GitHub-hosted runner kills a job at 6 hours, so "24 h" cannot be one job.
`soak.yml` runs a **6 targets × 2 shards** matrix at 4 hours each: **48
CPU-hours, 4 hours of wall clock**, every shard under the limit with room to
spare. The shards differ only in their libFuzzer `-seed`, and they share a cached
corpus, so two shards of one target explore different mutation sequences from the
same frontier rather than repeating each other.

It was 3 × 2 × 4 = 24 CPU-hours until M2 S7. Each target keeps its own **8
CPU-hours**, which is the point of widening the matrix rather than shortening the
runs: §9's M1 gate says *"fuzzer runs 24 h without a panic"* about `mt-inline`,
and halving `tokenize` to pay for `parse` would meet M2's gate by weakening M1's.
