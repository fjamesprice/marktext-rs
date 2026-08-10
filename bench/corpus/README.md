# `bench/corpus/` — provenance

The corpus specified by RUST-REWRITE-PLAN.md §14 step 4. Regenerate with:

```sh
cargo xtask corpus            # write
cargo xtask corpus --check    # verify the committed files match the generator
```

CI runs `--check`, so a hand edit here is caught rather than silently becoming
the new baseline.

## Why generated rather than collected

Every file is produced deterministically by `xtask/src/corpus.rs` from a fixed
seed, using a four-line xorshift PRNG written in that file. Regeneration is
byte-identical on every platform and every Rust version — which is what makes
`--check` meaningful, and what keeps a 5 MB benchmark input from being
something one developer happens to have on disk.

The alternative — collecting real documents — was rejected because the corpus
is committed and shared, and real markdown carries provenance and licensing
questions that a benchmark input should not have.

## What each file is for

The corpus serves three purposes simultaneously, which is why it is worth
being deliberate about content rather than just size:

1. **Performance regression suite** (§12.1). "Open 5 MB document ≤ 800 ms"
   needs a 5 MB document that exists on every machine.
2. **Differential harness corpus** (§11.2). Every file goes through both the
   TypeScript and Rust engines on every commit.
3. **Tiling-invariant corpus** (§3, M1 exit gate). "Concatenating every token's
   `raw` reproduces the source byte-for-byte" is checked over these files.

| File | Size | Provenance | What it is for |
|---|---:|---|---|
| `empty.md` | 0 B | — | The degenerate case. Cold-start and empty-RSS measurement (§12.1: ≤ 60 MB), and the parser's zero-block path. |
| `10kb.md` | ~10 KB | Generated: README-shaped, fixed seed | The single most common thing a markdown editor is pointed at. Dense in *variety* rather than volume: multi-level headings, a table, nested and task lists, a fenced block, links, an image, a reference definition, a footnote. |
| `250kb.md` | ~250 KB | Generated: prose, fixed seed | Spec-document scale. The size at which incremental reparse (§4.1) starts to matter. |
| `1mb.md` | ~1 MB | Generated: prose, fixed seed | §12.1's RSS target (≤ 120 MB). |
| `5mb.md` | ~5 MB | Generated: prose, fixed seed | §12.1's headline target: open in ≤ 800 ms. This is the case Electron cannot do and where the incremental-layout design is won or lost (§5). |
| `50-code-fences.md` | ~4 KB | Generated | The two `CodeBlock` constraints from §2. Every fourth fence carries a **full info string** (`rust title="example-4.txt" {.numberLines}`) so a parser that reduces `info` to its first word is caught; every seventh uses a **four-backtick fence containing a three-backtick run**, so a parser that normalises `fence_len` to 3 corrupts the file. Ends with an indented code block, which has no fence length at all. |
| `100-inline-math.md` | ~9 KB | Generated | Inline math is a *replaced element* in inline layout (§5) — a parley `InlineBox` with a baseline. This is the density test, plus escaped `\$` delimiters that must not become math. |
| `20-tables.md` | ~6 KB | Generated | Tables need two layout passes (§5): measure natural column widths, then distribute. Includes all three alignment forms and cells containing escaped pipes, the classic GFM table round-trip hazard. |
| `rtl.md` | ~1 KB | Generated: hand-written Arabic and Hebrew | The Unicode Bidirectional Algorithm is a live requirement — MarkText has an upstream RTL fix in its history (`43bd8b77`). Every line mixes scripts, because pure-RTL text hides exactly the bugs that matter: mixed runs, numbers inside RTL, an RTL URL, and an RTL table. |
| `cjk.md` | ~1 KB | Generated: hand-written zh-CN, zh-TW, ja, ko | Those four are shipped locales, so CJK is a correctness requirement (§7, §10). Carries the **CJK flanking cases for `strong`** — `中文**粗体**紧邻` with no spaces — which `inlineRenderer/__tests__` covers and the lexer port must reproduce exactly. Also a deliberately long unspaced line for UAX #14 line breaking. |
| `emoji.md` | ~1 KB | Generated: hand-written | The cheapest way to break grapheme-cluster handling. ZWJ sequences (`👨‍👩‍👧‍👦`), skin-tone modifiers, regional-indicator flags — each one grapheme, several codepoints. Arrow keys must not split them (§5, via parley's cluster API) and `mt-inline` must not tokenise inside them. Includes `:shortcode:` form, which muya lexes as emoji tokens. |

## What is deliberately not here yet

- ~~**`BASELINE.md`.**~~ **Built at M3 S0, 2026-08-10 — see [`../BASELINE.md`](../BASELINE.md).**
  NATIVE-REWRITE-PLAN.md §1.1 obliged capturing the real Electron numbers for
  every §12.1 row before the targets mean anything — "without this file the
  rewrite has no scoreboard and the targets are unfalsifiable". It needed a full
  `pnpm install` and packaged build of the marktext clone, which M0 deliberately
  did not do and M3 S0 did.

  The scoreboard it produced is worth knowing before reading the table above:
  packaged Electron MarkText takes **818 ms** to reach an editable caret, holds
  **485 MB** RSS on an empty document, takes **43.3 s** to open `1mb.md`, and
  **never finishes opening `5mb.md`** — it stays responsive and burns CPU
  indefinitely. So `5mb.md`'s line in the table above — *"this is the case
  Electron cannot do"* — was written as an expectation and is now a measurement.
- **`criterion` harnesses.** §12.1's latency budgets run in CI on fixed
  hardware; a GitHub-hosted runner is not fixed hardware. The corpus exists
  now so that the benchmarks have inputs the day the machine does.
