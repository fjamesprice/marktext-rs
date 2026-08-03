# marktext-rs

A native Rust reimplementation of [MarkText](https://github.com/marktext/marktext).

The build plan is [`docs/RUST-REWRITE-PLAN.md`](docs/RUST-REWRITE-PLAN.md), and
it is authoritative. This README describes only what exists.

> **Status: M1, stage 7 of 7 — one exit-gate clause outstanding.** Workspace,
> CI, conformance suite, differential-test harness, and benchmark corpus, from
> M0. `mt-inline` — the inline tokenizer — is a complete port of muya's
> `lexer.ts`: all sixteen handlers, all 49 of muya's inline specs passing, and
> as of S7 a **token-stream differential against the running TypeScript engine
> on every commit**. Nothing consumes it yet, and the rest of the workspace is
> still stubs. There is no parser, no layout, no window; **you cannot run the
> application.** The one thing M1 still owes is a green 24-hour fuzz soak: the
> targets and the nightly workflow are in, and the run has not happened.
> Wiring the tokenizer into `mt-md` is M2.

---

## Quick start

```sh
cargo test --workspace                        # unit tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

cargo xtask ci                                # everything CI runs beyond the above
```

`cargo xtask ci` runs, in order:

| Step | What | Plan |
|---|---|---|
| `deps` | Dependency-direction guard | §1 |
| `corpus --check` | `bench/corpus/` matches its generator | §14.4 |
| `divergences` | **Token-stream** differential vs. the TypeScript engine, plus the register of intentional differences | M1 §5 D3 |
| `conformance` | CommonMark + GFM ratchet | §11.1 |
| `diff` | **Block-state** differential vs. the TypeScript engine | §11.2 |

Two differentials, one per layer: `divergences` compares `mt-inline`'s token
stream and is live today; `diff` compares `mt-md`'s block state and skips until
M2 lands the Rust side. Neither subsumes the other — block state carries no
inline tokens and a token stream has no blocks.

The nightly [`soak`](.github/workflows/soak.yml) workflow runs what does not fit
in a per-commit budget: 24 CPU-hours of libFuzzer, the divergence register's
full 41,009-input sweep, an hour of property testing on all three platforms, and
tiling over `1mb.md` and `5mb.md` in release.

Both differentials need a marktext clone. It is found via
`--marktext <DIR>`, `$MARKTEXT_DIR`, or a sibling `../marktext`, and needs only
muya's own dependencies:

```sh
cd ../marktext && pnpm install --filter @muyajs/core --ignore-scripts
```

Without it, the harness skips rather than fails. See
[`tools/diff/README.md`](tools/diff/README.md).

---

## Layout

```
crates/
  mt-doc/         model, buffer, edits, undo          [no I/O, no UI]
  mt-inline/      inline tokenizer (port of lexer.ts) [pure]
  mt-md/          markdown ⇄ Document                 [pure]
  mt-layout/      block flow + parley inline layout   [no GPU]
  mt-render/      display list → pixels
  mt-highlight/   tree-sitter code fences
  mt-math/        TeX layout
  mt-diagram/     native diagram subset
  mt-export/      HTML + PDF
  mt-fs/          io, encodings, watch, atomic save
  mt-search/      in-doc find/replace + project grep
  mt-ui/          widgets, floats, panes, a11y tree
  mt-app/         winit shell, menus, prefs, session
  mt-cli/         argv, headless convert
spec/             CommonMark + GFM fixtures (copied verbatim)
bench/corpus/     performance + differential corpus
fuzz/             cargo-fuzz targets  [its own workspace — needs nightly]
tools/            Node-side harness tooling
xtask/            build/test-harness automation
```

Each crate's `lib.rs` opens with its contract and its dependency constraints.
Those are not decoration: **`mt-doc`, `mt-inline`, `mt-md`, and `mt-layout`
must compile and test with no windowing, no GPU, and no I/O**, and must never
depend on `mt-ui` or `mt-app`. That property is what makes the whole of §11
runnable in CI on three platforms, and `cargo xtask deps` enforces it — Cargo
catches a dependency *cycle*, but not a one-way edge.

---

## What is real, and what is a stub

**Real, and running:**

- The workspace, on stable Rust 1.85+ (edition 2024, resolver 3).
- `mt_doc::Block` and `mt_doc::Edit` as concrete types, with the four
  round-trip constraints from §2 carried as code comments and a test asserting
  the 1:1 mapping onto muya's `TState` union.
- **`mt-inline`, the whole tokenizer** — M1 S6. The token types, the 26-rule
  table, the tokenizer loop with muya's ordered handler list, `generator`, and
  all of `utils.ts` — emphasis validation, link/destination parsing, and
  `getAttributes` reimplemented **without a DOM**, which is where the HTML5
  named-reference table in `entities.rs` comes from. **All sixteen handlers are
  implemented** and **all 49 transcribed muya specs pass**: the `PENDING` list
  in `crates/mt-inline/tests/inline_renderer_specs.rs` is empty. Nested
  tokenization runs, so ``[**a** `b`](c)`` and `<div>**a**</div>` are trees.
  `cargo bench -p mt-inline` is the per-stage cost number.

  S6 added the three things that *read* a finished token tree rather than
  producing one: `tokens_to_plain_text` (the reader-facing text a heading slug
  comes from), the `highlights` intersection post-pass, and `marker_state` —
  the rule that reveals a token's markers when the caret is on it, which is what
  makes MarkText feel like MarkText. The round-trip property
  `generator(tokenize(s)) == s` now runs over `bench/corpus/`, the 1,324
  CommonMark and GFM examples and the eleven `marktext-round-trip` fixtures.

  S7 closed the verification rather than adding behaviour. `cargo xtask
  divergences` is now a **live token-stream differential**: it loads muya's
  tokenizer with happy-dom, serializes an `mt_inline::Token` into the same wire
  shape, and compares field by field — the register's 27 inputs as a negative
  control (27 of 27 disagree) and 4,264 more on which any disagreement is a
  failure, 41,009 in the nightly run. Property tests run on all three platforms,
  tiling runs over every corpus file in release, and the four branches this
  crate proves unreachable are `debug_assert!`s so a fuzzer attacks the proofs
  instead of agreeing with them.

  **What M1 still owes is one clause: 24 hours of libFuzzer, green.**
  `fuzz/` and `.github/workflows/soak.yml` carry it — 3 targets × 2 shards × 4 h
  on Linux, because a GitHub-hosted job is killed at six hours — and it has not
  run yet.
- The conformance ratchet, over 1,324 real fixtures.
- The TypeScript half of the differential harness, over the 22-file corpus
  (eleven `bench/corpus/` files plus eleven `marktext-round-trip` fixtures —
  see docs/M1.md's correction to S7's gate row, which is where that 22 got
  misread as `bench/corpus/`'s own count).
- The dependency-direction guard.
- `bench/corpus/`, deterministic and `--check`able.
- CI on Windows, macOS and Linux.

**Stubs:**

- Every other crate body. `mt_md`'s entry points return `Unimplemented`;
  `mt-cli --dump-state` exits 3.
- Method bodies in `mt-doc` are `todo!()`, except `Block::name()` and
  `Block::is_leaf()`, which are the 1:1 mapping itself and are therefore
  implemented and tested now.

Both harnesses report **skipped, not failed**, while the Rust side is
unimplemented — and they start enforcing the first time it returns `Ok`, with
no flag to remember and no code to change.

---

## Third-party dependencies

Two:

- `serde_json`, in `xtask` only, for reading fixtures and diffing state. Dev
  tooling, not shipped.
- `fancy-regex`, in `mt-inline` — the first dependency of a shipped crate,
  added at M1 S1. §3 asks for `regex`; 16 of `rules.ts`'s 26 patterns need a
  backreference or lookaround, which `regex` does not have by design
  (docs/M1.md §4 C1). `fancy-regex` wraps it and adds exactly those. Pure: no
  I/O, no windowing, no GPU, so `mt-inline` stays headless.

The dependency tables in §8 and §12 of the plan (wgpu, vello, parley, swash,
tree-sitter, pulldown-cmark, winit, accesskit, …) are the plan of record, not
something to front-load. Each lands with the milestone that needs it. `xtask
deps` additionally fails if any of them appears in one of the four headless
crates.

Node-side: `tsx`, to load muya's TypeScript.

---

## Documents

| | |
|---|---|
| [`docs/RUST-REWRITE-PLAN.md`](docs/RUST-REWRITE-PLAN.md) | The build plan. Authoritative. |
| [`docs/NATIVE-REWRITE-PLAN.md`](docs/NATIVE-REWRITE-PLAN.md) | Rationale record: alternatives considered, why Tauri was rejected, the incremental fallback. Superseded on the stack question. |
| [`docs/M0.md`](docs/M0.md) | What M0 delivered, and the decisions taken that the plan did not cover. |
| [`docs/M1.md`](docs/M1.md) | The working plan for M1 — the `mt-inline` tokenizer port. Current. |
| [`spec/README.md`](spec/README.md) | The conformance ratchet, and how it turns on at M2. |
| [`bench/corpus/README.md`](bench/corpus/README.md) | Corpus provenance. |
| [`tools/diff/README.md`](tools/diff/README.md) | The differential harness. |

## Licence

MIT, as MarkText is.
