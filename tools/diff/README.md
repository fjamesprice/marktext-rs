# Differential testing against the TypeScript engine (§11.2)

There are **two** of these, one per layer, and they share everything except what
they compare.

```text
block state (§11.2) — what mt-md will produce
corpus file ──┬──► node → @muyajs/core → state JSON ──┐
              │                                        ├──► assert equal
              └──► mt-md (via mt-cli --dump-state) ────┘

token stream (M1 §5 D3) — what mt-inline produces
input string ─┬──► node → muya tokenizer → token JSON ─┐
              │                                         ├──► assert equal
              └──► mt_inline::tokenizer ────────────────┘
```

```sh
cargo xtask diff                    # block state, the whole corpus
cargo xtask diff --require-ts       # fail, don't skip, if the TS engine is missing
cargo xtask diff bench/corpus/cjk.md

cargo xtask divergences             # token streams: the register + a 4,264-input sweep
cargo xtask divergences --no-sweep  # the register alone, for a fast local loop
cargo xtask divergences --full-sweep  # 41,009 inputs, ~120 s; the nightly soak runs this
```

**Neither subsumes the other.** Block state does not carry inline tokens, and a
token stream has no blocks. The token-stream half landed at M1 S7 and is what
makes `spec/divergences.json` enforceable — before it, every register entry
reported `SKIPPED` and reverting a fix still exited 0. The rest of this document
is about the block-state half; `xtask/src/tokens.rs` documents the other.

## Why this is the highest-value test asset in the project

It exists only because §0 keeps the state shape identical between the two
engines: muya's `TState` union and `mt_doc::Block` map 1:1, so both sides emit
the same JSON and can be compared structurally.

What that buys is a change of kind, not of degree. *"Did I port the 898-line
lexer correctly?"* is a judgement call — you read the diff, you squint, you
decide. Run over the whole corpus on every commit, it is a boolean.

§13 R1 — *"lexer port diverges subtly; markdown renders almost right"* — names
this as the mitigation, and it is why §14 step 3 puts building it **before**
writing any of `mt-inline`.

## The three pieces

| Piece | Where | Role |
|---|---|---|
| TypeScript dumper | `tools/diff/dump-ts-state.mjs` | Loads `MarkdownToState` from the muya source and prints state JSON. Knows nothing about the Rust side. |
| Rust dumper | `mt-cli --dump-state` | Same input, same output shape. Currently a stub. |
| Comparator | `xtask/src/diff.rs` | Spawns both, compares, reports per-file pass/fail. All the pass/fail logic lives here, in one place. |

The token-stream half has the same three, one layer down — and the third piece
is a function call rather than a process, because `xtask` can depend on
`mt-inline` directly:

| Piece | Where | Role |
|---|---|---|
| TypeScript dumper | `tools/diff/dump-ts-tokens.mjs` | Loads `tokenizer` from `inlineRenderer/lexer.ts` with a happy-dom `DOMParser` registered, which `getAttributes` needs or `html_tag` throws. |
| Rust side | `xtask/src/tokens.rs::wire_with` | Serializes an `mt_inline::Token` into muya's wire shape. |
| Comparator | `xtask/src/divergences.rs` | Runs the register's own inputs as a negative control, then the sweep, and reports per-entry `ok`/`STALE` plus any unregistered disagreement. |

## Contracts the two dumpers share

Both must agree on all four of these, or a disagreement stops being
interpretable:

1. **Options.** `MUYA_DEFAULT_OPTIONS` in the Node script mirrors
   `mt_md::Options::MUYA_DEFAULT`; `SPEC_OPTIONS` mirrors `Options::SPEC`.
   Driving the two engines with different flags produces differences that mean
   nothing. The token dumper's `MUYA_DEFAULT_SYNTAX` mirrors
   `mt_inline::TokenizerOptions::muya_default()` for the same reason — and it
   accepts per-input `labels` and `footnote` because three of the 26 token types
   are unreachable without them, so a comparator driven by the defaults alone
   would ship `reference_link`, `reference_image` and `footnote_identifier`
   never having been compared at all.
2. **Input normalisation.** Strip a UTF-8 BOM, normalise CRLF to LF. muya
   normalises to LF internally, so a CRLF corpus file would otherwise
   disagree for a reason that has nothing to do with parsing.
3. **Canonical JSON.** Object keys sorted recursively; array order preserved,
   because block order is semantic. serde_json's default map is a `BTreeMap`
   and sorts for free; the JavaScript side pays the same cost explicitly in
   `canonicalize()`.
4. **Exit codes.** `0` success, `1` error, `3` unimplemented/unavailable. The
   third is what lets the comparator report SKIPPED instead of failing CI, and
   it is why "not built yet" and "broken" are never confused.

## Skip semantics, and why they are not a loophole

Two things can be missing, and they are reported distinctly:

- **The Rust engine is unimplemented** (`mt-cli` exits 3). Expected until M2.
- **The TypeScript engine is unavailable** — no marktext clone, or its
  dependencies are not installed (the dumper exits 3).

Both produce SKIP. That is honest at M0, but it is also the one failure mode
that could quietly turn this whole investment into a no-op that still reports
green. So CI passes `--require-ts`, which makes a missing reference engine a
hard failure. A broken checkout step fails loudly instead of silently
disabling the harness.

## Running it locally

The comparator resolves the marktext clone in this order: `--marktext <DIR>`,
then `$MARKTEXT_DIR`, then a sibling `../marktext`. With both repositories
checked out next to each other, no configuration is needed.

The muya engine needs its dependencies, but only its own — not Electron:

```sh
cd ../marktext
pnpm install --filter @muyajs/core --ignore-scripts
```

That is ~1,100 packages and about 20 seconds. A full root `pnpm install` also
downloads Electron (~120 MB) and rebuilds native modules against its ABI, none
of which this harness touches.

The TypeScript source is loaded through `tsx` (a devDependency of this repo)
rather than Node's native type stripping, because `markdownToState.ts` uses a
constructor parameter property — `constructor(private _options: …)` — which is
not erasable syntax.

## Current status

Every file SKIPs, because `mt_md::dump_state` is a stub. The TypeScript half
works today and is exercised on every run over the full 22-file corpus, so the
day `mt-inline` lands the comparison starts happening with no change to any of
these three pieces.

## Extending it

§11.2: *"Extend it to serialized markdown and exported HTML as those land."*
Concretely, as each becomes available:

- **M2, serializer.** Add `--dump-markdown` alongside `--dump-state` and
  compare against `StateToMarkdown`. That checks the round-trip's output side,
  which the state comparison does not reach.
- **M2, HTML.** Compare `renderToStaticHTML` output through the same normaliser
  the conformance ratchet uses — which also validates the Rust port of
  `normalizeHtml` in `xtask/src/html.rs` against its TypeScript original, the
  one part of the ratchet that cannot be checked any other way.
