# `spec/` — CommonMark and GFM conformance

Per RUST-REWRITE-PLAN.md §11.1 and §14 step 2, `packages/muya/test/spec/` was
copied here **unchanged**. The fixtures are language-agnostic JSON, so the
Rust engine runs against exactly the bar the TypeScript engine is held to.

Run it with:

```sh
cargo xtask conformance
```

---

## The ratchet

`expected-failures.json` lists the spec examples the engine is known to fail.
Two assertions, pointing in opposite directions:

| State | Verdict | CI |
|---|---|---|
| Unlisted, passes | `Pass` | green |
| Listed, fails | `ExpectedFailure` | green |
| **Listed, passes** | `UnexpectedPass` | **fails** — delist it |
| **Unlisted, fails** | `UnexpectedFailure` | **fails** — regression |
| Engine unimplemented | `Skipped` | green, and says so |

The third row is the one people find surprising and it is the important one:
*getting better must also fail the build*, so that the list is forced back down
rather than accumulating stale entries that would later let compliance quietly
fall.

**Compliance can only go up.** The floor is muya's current 87.7 % CommonMark
0.31 and 86.3 % GFM 0.29 (`conformance.md`), which is also the M2 exit gate
(§9). Because `pulldown-cmark` handles blocks at >99 %, the realistic target is
meaningfully higher and most residual failures should be in block-tree mapping
rather than in parsing.

The runner lives in `xtask/src/conformance.rs`; the HTML normaliser it compares
through is a port of `runner.ts`'s `normalizeHtml`, in `xtask/src/html.rs`.

---

## Current status: skipped-but-present

`mt_md::render_to_static_html` returns `Unimplemented`, so every one of the
1,324 examples reports `Skipped` and the runner exits 0 with a loud summary.

That is not the same as "not wired up". Running today already checks:

- both fixture files parse, at the sizes `conformance.md` states (652 + 672);
- `expected-failures.json` parses and every listed number names a real example
  (a stale number silently weakens the ratchet by one example);
- the listed failures leave enough room to clear the stated floor;
- the ratchet's own decision table, unit-tested against synthetic inputs;
- the HTML normaliser, unit-tested against every behaviour `runner.ts`
  documents.

## How to flip it on at M2

**There is nothing to flip.** The runner calls `mt_md::render_to_static_html`
and treats `Err(Unimplemented)` as a skip. The first run where that function
returns `Ok` is the first run where the ratchet enforces — no flag, no code
change, no chance of forgetting.

What *does* need doing, once, on that first run:

1. **Re-baseline.** The inherited `expected-failures.json` is muya's failure
   list, and the Rust engine will fail a different set. Regenerate it from
   actual results, and read the diff carefully — you are choosing the floor,
   and the floor can never be raised again.
2. **Do not let the list grow past the gate.** Regenerating is not a licence
   to list everything that fails. `cargo xtask conformance` fails if the list
   is large enough to cap the pass rate below the floor, but that is a
   backstop, not a review.
3. **Update `conformance.md`** in the same commit, with per-section pass rates,
   so the numbers in the repo describe the Rust engine rather than muya.
4. **Delete `conformance::tests::every_suite_is_skipped_at_m0`.** It exists to
   make the transition deliberate: it fails the moment the engine starts
   rendering, and its message points here.

---

## The divergence register

`divergences.json` is the second register in this directory, and it points the
other way: `expected-failures.json` lists places the engine is **worse** than it
should be, `divergences.json` lists places it is deliberately **different** from
muya.

It exists because of docs/M1.md §5 D3. M1's verification strategy is agreement
with the TypeScript engine, and D3 decides that muya's bugs are *fixed* in the
port rather than reproduced bug-for-bug — so every fix is a disagreement, and
without somewhere to record it **a fixed bug and a botched port look identical
in the differential harness**. Four rules:

1. A differential disagreement on a **registered** input is expected. A
   disagreement on **any other** input is a failure, exactly as before.
2. Every entry names concrete inputs, and those inputs become Rust tests
   asserting the **fixed** behaviour.
3. An entry with **no failing differential case is stale** — the fix is either
   unimplemented or the divergence was imaginary — and the runner says so.
4. `upstream` holds the marktext issue URL once filed. File them.

### One register, two layers — added at M2 S0

M2 produces disagreements at a second layer: anywhere `marked` and CommonMark
differ about **blocks** and the port follows CommonMark (docs/M2.md §5 D4).
Those go in this same file, with a required `layer` field, rather than in a
fourth register — for the reason the next section gives, and because M1's
negative control (*run the register's own inputs and fail if none of them
disagrees*) is what makes a green run mean anything and should not have to be
built twice.

| `layer` | Runner | What it compares |
|---|---|---|
| `"inline"` | `cargo xtask divergences` | token streams (`xtask/src/tokens.rs`) |
| `"block"` | `cargo xtask diff` | block state (`mt-cli --dump-state`) |

**Each runner runs only its own entries, and the filtering is not cosmetic.**
An entry is `Stale` when every one of its inputs *agrees*, and a comparator
that cannot see a divergence reports agreement — so an inline entry run through
`cargo xtask diff` would be `Stale` on every input and fail the build for a
reason that has nothing to do with the port. That is not hypothetical: M1 S6
built a text-level comparator that agreed on all 5,586 inputs and was
*structurally unable* to see the one entry whose divergence lives in `attrs`.

`layer` is **required**. A default would file an entry under whichever layer
the default named, and the runner that does not own it would never run it — a
tolerated disagreement nobody checks, which is the state rule 3 exists to
prevent. A missing or misspelled `layer` is a parse error.

```sh
cargo xtask divergences                # the register + a 4,264-input sweep, ~22 s
cargo xtask divergences --no-sweep     # the register alone, for a fast local loop
cargo xtask divergences --full-sweep   # 41,009 inputs, ~120 s; the nightly soak
cargo xtask divergences --require-ts   # fail, don't skip, without the TS engine
```

| State | Verdict | CI |
|---|---|---|
| A registered input disagrees | `Confirmed` | green |
| **Every registered input agrees** | `Stale` | **fails** — implement the fix or delete the entry |
| Any input could not be compared | `Skipped` | green, and says so |
| Duplicate id, empty `inputs`, unknown key, shared input | — | **fails** — malformed register |
| **An unregistered input disagrees** | — | **fails** — rule 1's other half |

The last row is what makes the register a claim about the port rather than about
itself. Confirming every entry proves only that the *tolerated* disagreements
still hold; the sweep is the set on which the two engines must agree exactly.

`Stale` is the analogue of the conformance ratchet's `UnexpectedPass`, and it
fails for the same reason: a register entry that no longer holds is one that
silently widens what the harness tolerates. The shape is deliberately the same
as `expected-failures.json` — one mechanism to keep honest rather than two.

The runner lives in `xtask/src/divergences.rs`.

### The third and fourth instances of the same ratchet

`crates/mt-inline/tests/inline_renderer_specs.rs` carries a `PENDING` list with
the identical decision table, over the 49 transcribed muya inline specs.
`crates/mt-md/tests/state_specs.rs` carries a fourth, added at M2 S0, over the
187 transcribed `state/__tests__` specs. Neither is in `spec/` because each has
a single consumer — its own test binary — so each is a Rust `const` rather than
JSON, with no parser and no path resolution. Move one here if `xtask` ever
needs to report on it.

**Four registers, one shape, on purpose.** If you change how one of them
decides, change the other three or write down why not.

Both `PENDING` lists carry M1 S5's **injected-list variant**: row two of the
decision table — *a listed case that starts passing fails the build* — needs a
listed case that passes to test against, and by construction the list never
contains one. So the table is also driven over an injected list, and over the
real `spec_case` with a real entry. M1 discovered this when emptying `PENDING`
would otherwise have switched the ratchet off silently; `mt-md`'s has it from
the first commit rather than from the last.

### Current status: **enforcing**, as of M1 S7

Three entries, 27 registered inputs, **27 of 27 disagreeing**. The sweep runs
4,264 further inputs on every commit and 41,009 nightly, and a disagreement on
any of them is a failure.

It was not always so, and the history is the reason the rules are worth
following. From S0 to S6 `xtask diff` compared **block state**, so there was no
TypeScript token stream to compare a Rust one against, every entry reported
`Skipped`, and rule 3 could not fire: revert a fix and the runner still exited
0. Five stages verified by hand instead, and S5 showed what that costs — S4
probed one divergence class with two inputs, found both agreeing, recorded that
the entry needed no widening, and 96 of 228 swept inputs disagree. A hand-run
does not merely have to be repeated; it can return a **false negative** and be
written down as a result.

S7 built the comparator: `tools/diff/dump-ts-tokens.mjs` loads muya's
`tokenizer` with a happy-dom `DOMParser` registered, `xtask/src/tokens.rs`
serializes an `mt_inline::Token` into the same wire shape, and the two are
compared field by field — which matters, because S6's text-level comparator was
structurally unable to see `html-tag-attrs-from-a-foster-parented-element` at
all.

Without a marktext clone the runner still reports `Skipped` and exits 0.
`--require-ts` makes that a failure, and CI passes it.

---

## Files

| File | Origin |
|---|---|
| `expected-failures.json` | Copied verbatim from muya. The ratchet's floor. |
| `divergences.json` | **Written here**, not inherited. The register of intentional differences from muya — see above. |
| `conformance.md` | Copied verbatim. muya's baseline, captured at PR-6a (2026-05-20). |
| `fixtures/gfm-spec-0.29-gfm.json` | Copied verbatim. 672 examples. |
| `fixtures/marktext-round-trip/` | Copied verbatim. 11 fixtures backported from marktext's `markdown-basic` tests; also used by the differential harness. |
| `runner.ts`, `*.spec.ts` | Copied verbatim. Not executed here — they are the TypeScript runner, kept as the reference the Rust port is checked against. |
| `fixtures/commonmark-spec-0.31.json` | **Generated**, not copied — see below. |

### Why the CommonMark fixtures are generated

A gap between the plan and the repository, worth stating plainly.

§11.1 says to copy `packages/muya/test/spec/` and run the ratchet against it.
That is enough for GFM, whose fixtures are a committed JSON file. It is not
enough for CommonMark: `commonmark.spec.ts` pulls those 652 examples from the
`commonmark-spec` **npm package** at test time —

```ts
import cms from 'commonmark-spec';
const examples = cms.tests;
```

— so a verbatim copy of `spec/` yields a ratchet that checks GFM and not
CommonMark, silently halving the gate including the 87.7 % floor the M2 exit
criterion is stated in.

`tools/vendor-commonmark-spec.mjs` extracts the same examples from
`commonmark-spec@0.31.2` into `fixtures/commonmark-spec-0.31.json`, in the same
`{ markdown, html, section, number }` shape the GFM file uses. The output is
committed, so CI needs neither the marktext clone nor Node to run the ratchet.

CI verifies it has not drifted:

```sh
node tools/vendor-commonmark-spec.mjs --check
```

Re-vendoring changes what the ratchet measures, so moving to a newer CommonMark
spec means re-baselining `expected-failures.json` and `conformance.md` in the
same commit.

### How much headroom the inherited list leaves

Almost none, which is the point.

| Suite | Listed failures | Best achievable | Floor | Slack |
|---|---:|---:|---:|---:|
| CommonMark 0.31 | 78 / 652 | 88.0 % | 87.7 % | 2 examples |
| GFM 0.29-gfm | 90 / 672 | 86.6 % | 86.3 % | 2 examples |

So the inherited list *is* the floor, near enough. When re-baselining at M2,
a list two entries longer than this one already fails the gate.

### A known inconsistency, inherited

`conformance.md`'s prose says 80 CommonMark and 92 GFM examples fail;
`expected-failures.json` lists 78 and 90. The prose additionally names
CommonMark **84** and **89**, and GFM **54** and **59**.

The JSON is what both runners read, so the prose is stale by four entries.
Copied as-is rather than corrected, because §11.1 says *unchanged* — worth
fixing when `conformance.md` is rewritten at M2.
