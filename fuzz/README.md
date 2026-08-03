# `fuzz/` — the M1 exit gate's soak

RUST-REWRITE-PLAN.md §9's M1 exit gate: *"Fuzzer runs 24 h without a panic."*
§11.3: *"`cargo-fuzz` on `mt-inline` and the HTML-paste path. Malformed input
must never panic — `panic = "abort"` makes a panic a crash."*

docs/M1.md §5 D6 decides where it runs:

> `cargo-fuzz`/libFuzzer support on Windows MSVC is thin. **Recommendation:**
> run the 24 h libFuzzer soak on Linux in CI, and run `proptest` on all three
> platforms in the normal test job.

## The three targets

| Target | Pointed at | Why it is separate |
|---|---|---|
| `tokenize` | everything | The gate itself: any UTF-8 string, every invariant. |
| `autolink` | `trim_auto_link_extent` | **The one thing `BACKTRACK_LIMIT` does not bound** — hand-written Rust, quadratic in the extent (S5). A hang here has no reproducer unless something asserts. |
| `backtracking` | the seven ReDoS-class rules | §4 C1: *"it is what the M1 fuzzing gate should be pointed at first."* Every input opens a quantifier that must scan to the end of the level before it can fail. |

`autolink` and `backtracking` prepend their own openers, so the fuzzer spends
its budget on the construct rather than on finding the opener. That is the whole
difference between them and `tokenize`, and it is worth three targets because a
general fuzzer reaches `trim_auto_link_extent` by accident.

## Exceeding the backtrack limit is not a bug

`rules::BACKTRACK_LIMIT` is 1,000,000 steps **per rule attempt** and exceeding
it means *the rule did not match*, never a panic — §4 C1 decided that, and the
first of its three reasons is that the exit gate is panic-freedom and a resource
limit that panics **is** the panic the gate forbids.

So an input that reaches the limit is a `spec/divergences.json` entry like any
other deliberate difference, not something to "fix". What the targets assert is
that the tokenizer stays **total**: a rule that gives up leaves its characters to
accumulate as text, so `generator(tokenize(s)) == s` holds either way.

## Assertions are the point

`Cargo.toml` sets `debug-assertions = true` in the release profile. M1.md §6
records four branches this crate proves unreachable and reproduces anyway, each
with the proof as a test:

| # | Branch | Stage |
|---|---|---|
| 1 | `validateEmphasize`'s rule-16 guard (`SHORTER && !CLOSE`) | S2 |
| 2 | `correctUrl` receiving a non-empty group 5 | S3 |
| 3 | `tryAutoLinkExtension`'s `if (!email)` | S5 |
| 4 | `tokensToPlainText`'s `html_tag` `else if (token.content)` | S6 |

S7 turned each proof into a `debug_assert!` at the site, so **a fuzzer reaching
one means a proof is wrong** and says so with a reproducer. A profile with
assertions compiled out would run faster and check strictly less — and "less"
here is exactly the four claims nobody has been able to break by hand. A fifth
guards `trim_auto_link_extent` returning 0, whose consequence is a hang rather
than a wrong token.

## Running it

Needs a **nightly** toolchain: `cargo fuzz` uses `-Zsanitizer` and
`-Cpasses=sancov-module`. This directory is its own workspace so that nightly
requirement stays out of the repository's `rust-toolchain.toml`, which pins
stable.

```sh
rustup toolchain install nightly
cargo install cargo-fuzz

# Seed from the differential sweep's own inputs — real markdown, already known
# to agree with muya, and covering the C2 traps and all 26 token types.
cargo xtask fuzz-seed

# From the REPOSITORY ROOT, not from this directory: cargo-fuzz locates the
# fuzz crate as a `fuzz/` subdirectory of wherever it is invoked, so running it
# in here makes it look for `fuzz/fuzz/` and report no targets.
cargo +nightly fuzz run tokenize -- -max_total_time=3600 -report_slow_units=5
```

`-report_slow_units` is what makes a soak produce a *benchmark* row rather than
only a pass/fail: whatever it reports slowest belongs in
`crates/mt-inline/benches/tokenizer.rs`, tagged with which of the three cost
terms it exercises — how far a lazy quantifier scans when it fails, a flat VM
setup per attempt, or how far a greedy quantifier scans before the character
that must follow it is absent — or as a fourth.

### On a machine without nightly

`crates/mt-inline/tests/properties.rs` carries a `soak` test that runs the same
properties over the same generators on **any** platform and any toolchain:

```sh
MT_SOAK_SECONDS=3600 RUSTFLAGS="-C debug-assertions=yes" \
  cargo test -p mt-inline --release --test properties -- --ignored soak --nocapture
```

It is coverage-**blind** where libFuzzer is coverage-guided, and
structure-**aware** where libFuzzer starts from bytes. Neither subsumes the
other. It exists because §9 says Windows ships first, and a gate met only on
Linux is a gate met on the platform whose failures block least.

`-C debug-assertions=yes` is not optional — see above.

## After changing `mt-inline`'s public API

`cargo test --workspace` does **not** compile these targets: `fuzz/` is a
separate workspace, which is the price of keeping nightly out of the main build.
So a rename in `mt-inline` breaks them silently. Run:

```sh
cargo +nightly fuzz build     # from the repository root
```

`.github/workflows/soak.yml` does this on every dispatch, before the matrix, so
a broken target fails in one minute rather than four hours in.

## The 24 hours, and GitHub's 6-hour job limit

A GitHub-hosted runner kills a job at 6 hours, so "24 h" cannot be one job.
`soak.yml` runs a **3 targets × 2 shards** matrix at 4 hours each: 24 CPU-hours,
4 hours of wall clock, every shard under the limit with room to spare. The
shards differ only in their libFuzzer `-seed`, and they share a cached corpus,
so two shards of one target explore different mutation sequences from the same
frontier rather than repeating each other.
