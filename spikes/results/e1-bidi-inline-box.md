# E1 — inline boxes inside RTL runs (M3 S0, risk M3-R1)

## The answer

**Does parley place an inline box correctly when that box sits inside an RTL run
which is itself inside an LTR (or mixed) paragraph?**

| parley version | answer |
| --- | --- |
| **0.11.0** (crates.io) | **YES** |
| **git `main`** @ `a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e` | **YES** |

Both versions place the box at the byte offset's **resolved bidi level**, not at
the x-offset a left-to-right sweep of the logical bytes would imply. Both split
the shaped run at the box's byte index and reorder the two halves with the rest
of the line, so the box lands *inside* the RTL run's visual extent.

The two numbers that decide it, from case `a1` — an LTR paragraph with the box
six bytes into the Arabic word `مرحبا`:

```
      190.35   231.96  run    32..51       rtl=true    "با بالعالم"
      231.96   271.96  BOX    @32                      <-- the box
      271.96   295.09  run    26..32       rtl=true    "مرح"
```

The box sits at x = 231.96, between logical bytes 32..51 on its **left** and
logical bytes 26..32 on its **right**. That inversion — higher byte offsets
further left — is the RTL run's visual order, and the box is inside it. A
logical left-to-right sweep would have put the box after all of `26..51`, at
x ≈ 254.56. It is 22.6 px to the left of that, in the correct slot.

`unicode-bidi`, asked the same question independently, says the box's visual
neighbours should be `'ب'` (U+0628) and `'ح'` (U+062D). parley's are `'ب'` and
`'ح'`. All six nested cases agree to the character.

### The one thing that is broken, on both versions

**11 of 12 cases pass on each version, and the failure is the same case on
both**: an inline box at **byte 0 of an RTL paragraph** — an inline image as the
first thing in an Arabic or Hebrew paragraph — is placed at the far **left** when
it belongs at the far **right**.

```
CASE e1-rtl-para-box-at-paragraph-start        (identical on 0.11.0 and main)
  layout: width 437.00
        0.00    40.00  BOX    @0        <-- parley puts it here
      ...
      146.83   437.00  run    0..81     "هذا نص عربي ..."
  expected left neighbour  'ه' (U+0647)   actual  <line edge>
  expected right neighbour <line edge>    actual  '.' (U+002E)
```

It should occupy x = 397.00..437.00. It occupies x = 0.00..40.00 — the full width
of the line, 397 px, wrong. Worse, case `e2` (the same box at the *end* of the
same paragraph) also lands at x = 0.00..40.00 and is *correct* there: a leading
box and a trailing box in an RTL paragraph are placed at the same coordinates and
are indistinguishable in the output.

The cause is in parley's source and is acknowledged there. On `main`,
`parley/src/shape/mod.rs`:

```rust
// TODO: this lets the inline box take the bidi level of the previous run, but in principle
// inline boxes should be included in bidi analysis as an object replacement character
// (U+FFFC). The box should then take the bidi level of that character.
let prev_bidi_level = if shaped_run_idx > 0 {
    layout.data.shaped_text.runs()[&shaped_run_idx - 1].bidi_level
} else {
    BidiLevel::new(0)
};
```

and on 0.11.0, `parley/src/layout/data.rs::push_inline_box`:

```rust
// Give the box the same bidi level as the preceding text run
// (or else default to 0 if there is not yet a text run)
let bidi_level = self.runs.last().map(|r| r.bidi_level).unwrap_or(0);
```

Inheriting the preceding run's level is *right* whenever the box splits a run,
because both halves share a level — which is why the six nested cases pass. It is
wrong only where there is no preceding run, and then the fallback is a hard-coded
level 0 rather than the paragraph level. `unwrap_or(0)` is the whole bug.

The other three embedding-boundary cases (`e2`, `e3`, `e4`) pass, because
inheriting the preceding run's level happens to agree with UAX #9's N1/N2 rules
there.

## Versions, pins and environment

| | |
| --- | --- |
| released arm | `parley = "=0.11.0"` (crates.io). CHANGELOG dates it 2026-06-24 |
| `main` arm | `parley = { git = "https://github.com/linebender/parley", rev = "a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e" }` |
| that rev | 2026-08-10, *"Move `char_style_indices` out of the per-item `ShapeOptions` (#741)"* |
| PR #639 in it? | yes — `a5ea9a77abfe34e3d3ac742e9e8bbbed0d023838`, 2026-07-08, verified with `git merge-base --is-ancestor a5ea9a7 main` |
| ground truth | `unicode-bidi` 0.3.18 |
| toolchain | cargo 1.97.1 / rustc 1.97.1, Windows 11, `x86_64-pc-windows-msvc` |
| corpus | `bench/corpus/rtl.md`, 1103 bytes, unmodified |

## Method

### The input, and what was done to it

The input is `bench/corpus/rtl.md`. The harness reads the real file at run time
and pulls four lines out of it by **one-based line number with an anchor
assertion**, so an edit to the corpus fails the harness loudly instead of
silently measuring different text:

| corpus line | used as | text |
| --- | --- | --- |
| 24 | LTR paragraph containing an Arabic run and a Hebrew run | `An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,` |
| 10 | RTL (Arabic) paragraph with an embedded English word | `هذا نص عربي … Rust في المنتصف.` |
| 18 | RTL (Hebrew) paragraph with an embedded English word | `שורה בעברית … parley באמצע המשפט.` |
| 26 | pure-LTR control | `line, which is where caret motion and hit-testing go wrong.` |

Two liberties were taken, and both are part of the finding:

1. **The paragraph is one source line, not the wrapped paragraph.** Lines 24–26
   are one soft-wrapped markdown paragraph. Using line 24 alone keeps every case
   to a single laid-out line (`break_all_lines(None)` — infinite width), so the
   x axis alone decides the question and no line-breaking interaction is mixed
   in. Line 24 already contains both an Arabic run and a Hebrew run inside an
   LTR sentence, which is exactly what case (a) asks for.

2. **Markdown syntax is stripped** (`**`, `*`) before layout, because `mt-layout`
   is handed inline text after `mt-md` has consumed the markers.

### The synthesized inline box — say this out loud

**`bench/corpus/rtl.md` contains no inline image.** It has bold, emphasis, a code
span and a link inside RTL runs, but nothing that becomes an `InlineBox`. So
there was no box in the corpus to measure and **the box is synthesized**: the
harness picks a byte offset and calls `builder.push_inline_box(InlineBox { .. })`
at it, with `width: 40.0, height: 24.0, kind: InFlow`.

That is faithful to what `mt-layout` will do — an inline image, an inline code
chip and an inline math box are all `InlineBox` at a byte offset — but it means
this experiment measures parley's behaviour for a box **the corpus author did not
place**, and the offsets are the harness's choice. Every offset is derived by
searching for a word from the corpus and stepping N characters into it, so the
offsets are readable and char-boundary-safe by construction:

```rust
fn inside(text: &str, word: &str, chars_in: usize) -> usize {
    let start = text.find(word).unwrap();
    start + word.chars().take(chars_in).map(char::len_utf8).sum::<usize>()
}
```

If M3 later wants this measured on a box the corpus *does* contain, the corpus
needs an inline image adding to an RTL paragraph. That is a one-line change to
`bench/corpus/rtl.md` and it is **not** made here — E1 does not modify inputs it
also measures.

### Why the expectations are not hand-written

Grading parley against a hand-written "the box should be here" would only prove
the author's grasp of UAX #9. Instead, for every case the harness:

1. builds a second string with **U+FFFC OBJECT REPLACEMENT CHARACTER** inserted at
   the same byte offset;
2. runs `unicode-bidi` — an independent UAX #9 implementation — over it;
3. reconstructs the full visual order from `BidiInfo::visual_runs`, reversing
   odd-level runs;
4. reads off the **first non-whitespace character visually left and right of the
   U+FFFC**. That pair is the expectation.

U+FFFC is the correct stand-in by construction: it is what CSS specifies for a
replaced inline element and what parley's own TODO says the box *should* be
treated as. The harness then computes the same pair from parley's output — for
each glyph run, the visually-left edge of an RTL run is its *logical end* — and
the case passes only if both characters match.

This means a case can fail two ways, and both are real: the box in the wrong
visual slot, or the box in the right slot but parley's own run order wrong. It
also means the verdict does not depend on any coordinate the author chose.

### What is dumped

For every case: layout width/height/line count, the paragraph's resolved bidi
level and the level `unicode-bidi` resolves for the U+FFFC, then every item on
the line **sorted by x** — for glyph runs the x-advance range, logical byte
range, parley's `Run::is_rtl()`, `unicode-bidi`'s resolved level at that run's
first byte, glyph count, `.notdef` count and the run's text; for the box its
x/y/width/height and (on `main`) its `baseline`. Then the expected and actual
neighbour pair and the verdict.

Note on the accessors: parley exposes run direction as **`Run::is_rtl()` only**.
The raw `BidiLevel` is `pub(crate)` in both versions — `LayoutData::bidi_level`,
`RunData::bidi_level` — and the only public reader is the boolean. The numeric
levels in the `lvl(ub)` column therefore come from `unicode-bidi`, not from
parley. That is a small API gap worth knowing about for `mt-layout`: distinguishing
level 1 from level 3 is not possible through parley's public surface today.

### Reproducing

`spikes/` is its own cargo workspace (see `spikes/Cargo.toml` for why). From the
repository root, with `%USERPROFILE%\.cargo\bin` on PATH:

```
cd spikes

# the primary question, both arms
cargo run -p e1-parley-0-11
cargo run -p e1-parley-main

# the font-strategy probe (D7). Run these ONE AT A TIME and never with
# --workspace: they set default-features = false on parley, and cargo's feature
# resolver would unify `system` back in from the two crates above.
cargo run -p e1-fonts-0-11
cargo run -p e1-fonts-main

# secondary question 3(c), the cost of `complex-scripts`
cargo tree  -p e1-fonts-0-11 --edges normal --prefix none
cargo tree  -p e1-fonts-0-11 --edges normal --prefix none --features complex-scripts
cargo build -p e1-fonts-0-11 --release
cargo build -p e1-fonts-0-11 --release --features complex-scripts
```

The two bidi harnesses are **the same file** apart from the version-specific
edits listed under secondary question 2; `diff spikes/e1-parley-0-11/src/main.rs
spikes/e1-parley-main/src/main.rs` is itself a result.

## Results table

| case | paragraph | box at | 0.11.0 | main |
| --- | --- | --- | --- | --- |
| `c-control-pure-ltr` | LTR | inside `caret` | PASS | PASS |
| `a1-ltr-para-box-in-arabic` | **LTR** | inside `مرحبا` (**RTL run**) | **PASS** | **PASS** |
| `a2-ltr-para-box-in-hebrew` | **LTR** | inside `שלום` (**RTL run**) | **PASS** | **PASS** |
| `b1-arabic-para-box-in-arabic` | RTL | inside `المنتصف` | PASS | PASS |
| `b2-arabic-para-box-in-english` | **RTL** | inside `Rust` (**LTR island, level 2**) | **PASS** | **PASS** |
| `d1-hebrew-para-box-in-hebrew` | RTL | inside `בעברית` | PASS | PASS |
| `d2-hebrew-para-box-in-english` | **RTL** | inside `parley` (**LTR island, level 2**) | **PASS** | **PASS** |
| `e1-rtl-para-box-at-paragraph-start` | RTL | byte 0 | **FAIL** | **FAIL** |
| `e2-rtl-para-box-at-paragraph-end` | RTL | byte len | PASS | PASS |
| `e3-ltr-para-box-at-ltr-to-rtl-boundary` | LTR | leading edge of `مرحبا` | PASS | PASS |
| `e4-rtl-para-box-at-rtl-to-ltr-boundary` | RTL | leading edge of `Rust` | PASS | PASS |
| `e5-ltr-para-box-at-paragraph-start` | LTR | byte 0 | PASS | PASS |

Cases `a`–`d` are the question as asked. Cases `e` were added after `a`–`d` all
passed, because parley's source says the box inherits the *preceding run's*
level — a rule that can only diverge from UAX #9 at an embedding boundary. Four
of the five boundary cases pass; the one that fails is the one with no preceding
run at all.

## Secondary answers

### 1. PR #639 — does `main` really carry inline-box baselines?

**Yes, and it is correct to the pixel.** On `main`, `InlineBox` has
`baseline: Option<f32>`, `PositionedInlineBox` has `baseline: Option<f32>`, and
`Layout::first_baseline()` / `last_baseline()` exist and return `Option<f32>`.
Four boxes of identical 40×24 geometry over the text `Ax box Ay`, differing only
in `baseline`:

| `InlineBox::baseline` | `first_baseline()` | line baseline | box `y` | `y + baseline` |
| --- | --- | --- | --- | --- |
| `None` | `Some(24.0)` | 24.00 | 0.00 | — (bottom edge 24.00 = baseline) |
| `Some(24.0)` | `Some(24.0)` | 24.00 | 0.00 | **24.00** |
| `Some(12.0)` | `Some(14.0)` | 14.00 | 2.00 | **14.00** |
| `Some(0.0)` | `Some(14.0)` | 14.00 | 14.00 | **14.00** |

`y + baseline` equals the line baseline in every case, and the line box resizes
around the box's requested alignment (24.00 → 14.00 when the box no longer needs
its full height above the baseline). All four are **absent from 0.11.0**: no
field, no method. M3-R1's "solved upstream but unreleased" reading of case 1 is
confirmed exactly.

### 2. Breaking churn from 0.11.0 to `main`

**Two lines.** Porting the identical harness required adding `baseline: None` to
two `InlineBox` struct literals — a non-exhaustive struct literal is a hard
compile error. Nothing else was forced. (A third line was changed by choice, to
*read* `PositionedInlineBox::baseline`; the rest of the diff is the arm label and
a deliberate expansion of the baseline probe.)

Of the three changes the unreleased changelog advertises:

- **`Glyph::style_index` removed (#661) — NOT hit.** The harness reads `Glyph::id`
  only, and `GlyphRun::style_index()` still exists on `main`, so the replacement
  the changelog names is a same-name method one level up. This will bite
  `mt-layout` only if it reads style per glyph rather than per run, which is the
  wrong granularity anyway.
- **`Cluster` now spans a full grapheme cluster (#715) — NOT hit.** The harness
  never touches `Cluster`. This is the one to watch: `mt-layout`'s caret motion
  and hit-testing are exactly the code that will use `Cluster`, and the changelog
  says shaped-cluster advances are now *split evenly over the graphemes they
  overlap*. E1 did not exercise it and cannot vouch for it.
- **Line sizing for mixed inline content (#697) — HIT, as a behavioural
  difference, on every single case.** Same input, same box:

  | | 0.11.0 | `main` |
  | --- | --- | --- |
  | layout height | 24.00 | **28.40** |
  | line `metrics().baseline` | 22.00 | **24.00** |
  | box `y` (for a 24 px box) | **−2.00** | 0.00 |

  On 0.11.0 the 24 px box overflows 2 px above the line box; on `main` the line
  box grows to contain it, per the CSS line-box model. `LineMetrics::{ascent,
  descent,leading}` are gone and `block_{min,max}_coord` mean something different.
  Any vertical metric `mt-layout` computes will change value across this upgrade.

And one behavioural difference the changelog does **not** list, which E1 found by
accident and which matters more than any of the three above:

- **0.11.0 breaks Arabic cursive joining at an inline box. `main` does not.**
  Splitting `مرحبا بالعالم` at byte 6 with a box:

  ```
  0.11.0  arabic  split at byte 6: glyph ids CHANGED    (text advance 64.21 -> 64.74, delta +0.53)
              without box: [993, 991, 910, 972, 991, 910, 913, 3, 910, 913, 931, 941, 995]
              with box   : [929, 941, 995, 993, 991, 910, 972, 991, 910, 913, 3, 910, 913]
  main    arabic  split at byte 6: glyph ids UNCHANGED  (text advance 64.21 -> 64.21, delta +0.00)
  ```

  As multisets those two 0.11.0 lists differ in exactly one glyph — `931` becomes
  `929`. One Arabic letter changed contextual form because the run boundary cut
  the joining context. Hebrew (non-joining) and Latin are unchanged on both
  versions, which is the control that makes this a joining result rather than a
  shaping-cache artifact. The fix on `main` is almost certainly #740, *"Set
  shaping context across run boundaries"* (2026-08-09) — **eleven days before this
  experiment ran, and not mentioned in the changelog's breaking-change list.**

  For MarkText this is a correctness bug, not a cosmetic one: an inline image or
  inline code chip inside an Arabic word renders the neighbouring letter in the
  wrong form on 0.11.0.

### 3. Font strategy (feeds D7)

**(a) `default-features = false` — yes.** parley builds and lays out with
`default-features = false, features = ["std"]`. `std` has to be added back
because parley is `#![no_std]` and needs `std` or `libm` for float maths; `system`
is the feature under test and it stays off. Both `spikes/e1-fonts-0-11` and
`spikes/e1-fonts-main` are that build, and both lay text out. Cost:

| | with `system` (default) | `default-features = false` + `std` |
| --- | --- | --- |
| 0.11.0 | 61 crates | **49 crates** |
| `main` | 62 crates | **50 crates** |

Twelve crates — the Windows font-backend graph — drop out.

**(b) In-memory fonts with no filesystem enumeration — yes.** The API:

```rust
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};

let mut font_cx = FontContext {
    collection: Collection::new(CollectionOptions { shared: false, system_fonts: false }),
    source_cache: SourceCache::default(),
};
// -> 0 families visible

let blob = Blob::new(std::sync::Arc::new(font_bytes) as _);
let registered: Vec<(FamilyId, Vec<FontInfo>)> =
    font_cx.collection.register_fonts(blob, None);
// -> 1 family, "DejaVu Sans Mono"
```

`FontContext` is a plain struct with two public fields, so it can be constructed
directly rather than via `FontContext::new()` (which calls `Default` and enables
system discovery). The signature is identical in both versions. This is
compatible with `mt-layout`'s "No I/O" constraint: the bytes arrive as
`include_bytes!` or from a caller, and fontique never opens a path.

**(c) `complex-scripts` — present in both, free in crates, expensive in bytes.**
The feature exists in 0.10.0, 0.11.0 and on `main` (on `main` it forwards to
`parley_engine/complex-scripts`). Measured:

| | crates without | crates with | release binary without | with | delta |
| --- | --- | --- | --- | --- | --- |
| 0.11.0 | 49 | 49 | 1 319 424 B | 5 114 880 B | **+3 795 456 B (+3.62 MiB)** |
| `main` | 50 | 50 | 1 342 976 B | 5 148 160 B | **+3 805 184 B (+3.63 MiB)** |

Zero new dependencies; roughly **3.6 MiB of binary**, which is the ICU dictionary
data for CJK/Thai/Khmer/Lao/Myanmar. That is the whole decision for D7: this is
a shipped-binary-size question, not a dependency-graph question, and
`§12`'s size-tuned `dist` profile does not touch it.

**MarkText's bundled faces — `.woff` is rejected outright.** Registering every
font file in `C:\Dev\marktext\packages\muya\src\assets\styles\fonts` one at a
time, identical result on both versions:

```
file                                                bytes  magic (ascii+hex) register_fonts result
DejaVuSansMono-Bold.ttf                            331992  .... 00010000  OK — "DejaVu Sans Mono" x1
DejaVuSansMono-BoldOblique.ttf                     253580  .... 00010000  OK — "DejaVu Sans Mono" x1
DejaVuSansMono-Oblique.ttf                         251932  .... 00010000  OK — "DejaVu Sans Mono" x1
DejaVuSansMono.ttf                                 340712  .... 00010000  OK — "DejaVu Sans Mono" x1
open-sans-v27-latin-ext_latin-300.woff              27512  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-300italic.woff        29336  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-600.woff              27416  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-600italic.woff        29176  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-700.woff              26612  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-700italic.woff        28244  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-italic.woff           29256  wOFF 774F4646  REJECTED — 0 families
open-sans-v27-latin-ext_latin-regular.woff          27480  wOFF 774F4646  REJECTED — 0 families
```

`register_fonts` returns an empty `Vec` for all eight `.woff` files — no error,
no panic, silently zero families. The magic bytes say why: `wOFF` / `77 4F 46 46`
is a WOFF1 container, which is per-table zlib-compressed OpenType, and
skrifa/read-fonts parse OpenType only. **So "bundle MarkText's fonts" is not free
for Open Sans.** The choices are: convert the eight `.woff` files to `.ttf`/`.otf`
at build time (offline, one-off, ~8 × 27 KB in, larger out); add a WOFF
decompressor (a `miniz_oxide`/`flate2` dependency plus a container parser, at
startup, in a crate that is meant to have no I/O and no surprises); or drop Open
Sans and bundle something else. The four `.ttf` files register with no work at
all. Note also that `register_fonts` returning `Vec::new()` rather than a
`Result` means a font that fails to load is indistinguishable from one that
loaded zero families — `mt-layout` must check the returned `Vec` is non-empty or
it will silently render tofu.

### 4. Can Arabic and Hebrew render from the bundled faces alone?

With `system_fonts: false` and only the four DejaVu Sans Mono files registered —
identical numbers on 0.11.0 and `main`:

```
sample   text                          glyphs  notdef    width  verdict
latin    The quick brown fox 12345         25       0   240.82  renders
arabic   هذا نص عربي                       11       0   105.96  renders
hebrew   שלום עולם                          9       8    86.70  partial tofu
```

**Arabic renders. Hebrew does not.** Eight of the nine Hebrew glyphs are
`.notdef` — the eight Hebrew letters; the ninth is the space. Confirmed
independently against the font's own tables (fontTools, `DejaVuSansMono.ttf`,
3322 cmap entries):

| block | coverage |
| --- | --- |
| Arabic, U+0600–U+06FF | **99 of 256** codepoints present |
| Arabic Presentation Forms-B, U+FE70–U+FEFF | **141 of 144** present |
| Hebrew, U+0590–U+05FF | **0 of 112** present |
| GSUB script tags | `DFLT`, **`arab`**, `cyrl`, `grek`, `lao `, `latn` |

DejaVu Sans Mono ships a full Arabic shaping stack — `arab` in GSUB, the
presentation forms, the joining behaviour E1 measured in secondary question 2 —
and no Hebrew at all. So a **Hebrew**-capable face must come from the system or
be bundled; Arabic need not. Open Sans latin-ext has neither, and in any case
cannot be loaded (above).

## Where this contradicts the plan

The brief for this experiment states several things that the measurements
falsify. Recording them loudly, because the discipline is to correct the plan
rather than conform to it.

1. **"Case 2 — nested bidi interacting with inline boxes — has no dedicated
   upstream issue and is believed unsolved." This is wrong.** It works, on both
   versions, in all six nested cases: a box inside an Arabic run inside an LTR
   paragraph, inside a Hebrew run inside an LTR paragraph, inside an RTL run in
   an RTL paragraph, and — the hardest case, level 2 — inside an English word
   embedded in an Arabic and in a Hebrew paragraph. parley splits the shaped run
   at the box's byte index and reorders both halves correctly. Nobody upstream
   ticked a box saying so, but the behaviour is there and has been since at least
   0.11.0. **M3-R1's case 2 should be closed, not carried.**

2. **The real gap is narrower and different from the one the plan names.** It is
   not "nested bidi"; it is *a box with no preceding text run*, where both
   versions fall back to `unwrap_or(0)` instead of the paragraph level. That is a
   one-line upstream fix, it is documented by parley's own TODO, and it has a
   trivial workaround `mt-layout` can apply today: when an inline box is the first
   item in an RTL paragraph, insert a zero-width strong character (U+200F RLM) or
   an empty leading run before it so that a preceding run exists. **The risk
   should be rewritten to name this, at a much lower severity.**

3. **Issue #208's "Root-level default BiDi direction" is now ticked on `main`.**
   The plan lists it as unchecked. `main` has
   `RangedBuilder::set_base_direction(BaseDirection)` (also on `TreeBuilder` and
   `StyleRunBuilder`) and re-exports `parley::BaseDirection` with `Auto`/`Ltr`/
   `Rtl`, added by PR **#708**, *"Expose `BaseDirection` through paragraph
   analysis"*, 2026-07-24. It does **not** exist in 0.11.0 — `git grep
   set_base_direction v0.11.0` returns nothing. This is a second unreleased
   feature M3 needs, alongside #639, and it is arguably more important: without
   it there is no way to tell parley that an empty or digits-only paragraph in an
   RTL document is RTL, and first-strong auto-detection will get it wrong.

4. **The font claim in the brief is backwards.** The brief says "DejaVu Sans Mono
   has no Arabic". It has 99 Arabic codepoints, 141 Arabic presentation forms and
   an `arab` GSUB script; what it has none of is **Hebrew** (0 of 112). The
   practical conclusion is the reverse of the one the brief anticipated: an
   Arabic-capable face does *not* have to come from the system, a Hebrew-capable
   one does.

5. **A behavioural difference between the two versions that no changelog entry
   covers.** 0.11.0 breaks Arabic cursive joining at an inline box (one glyph
   changes contextual form, line advance shifts 0.53 px); `main` does not. The
   unreleased changelog's "Changed" section lists three breaking items and this
   is not among them — it is a silent fix, most likely #740. Anything that pins
   0.11.0 pins that bug.

6. **Minor.** The brief dates parley 0.11.0 to 2026-06-26; parley's own CHANGELOG
   says 2026-06-24. Immaterial — both precede PR #639's merge on 2026-07-08, so
   the plan's reasoning stands.

### What this implies for the pin-vs-track decision

Not E1's call, but E1 owns the evidence, so: the port cost from 0.11.0 to `main`
was **two lines**, and `main` is strictly better on all four things E1 measured —
it has #639's baselines, it has `set_base_direction`, it fixes Arabic joining
across inline boxes, and it sizes lines per CSS. The costs are that `Cluster`'s
grapheme semantics changed underneath the caret/hit-testing code E1 did *not*
exercise, and that every vertical metric moves. Neither version fixes the
paragraph-leading-box case, so that workaround is needed either way.

---

## Raw dumps

Everything below is verbatim stdout. Nothing is elided.

### `cargo run -p e1-parley-0-11`

```
=== E1 bidi/inline-box harness — arm: parley 0.11.0 (crates.io, `parley = "=0.11.0"`) ===
corpus: bench/corpus/rtl.md (unmodified; markdown markers stripped at layout time)
box: width 40, height 24, kind InFlow; font size 16; single line (max_advance = None)

---------------------------------------------------------------
CASE c-control-pure-ltr
  note: Control. Base LTR, no RTL anywhere. Whatever this looks like is what 'placed correctly' means when bidi is not involved.
  text (59 bytes): "line, which is where caret motion and hit-testing go wrong."
  box at: byte 24 — inside "caret", 3 char(s) in
  layout: width 450.88, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 0
  line 0: baseline 22.00, advance 450.88, text_range 0..59
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   165.40  run    0..24        false       0    0  "line, which is where car" (24 glyphs, baseline 22.00)
      165.40   205.40  BOX    @24             -       0    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      205.40   450.88  run    24..59       false       0    0  "et motion and hit-testing go wrong." (35 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'r' (U+0072)
    right neighbour 'e' (U+0065)
  actual (parley):
    left neighbour  'r' (U+0072)
    right neighbour 'e' (U+0065)
  VERDICT: PASS

---------------------------------------------------------------
CASE a1-ltr-para-box-in-arabic
  note: LTR paragraph, box inside the Arabic run. The box must land between two Arabic letters, i.e. INSIDE the RTL run's visual extent.
  text (98 bytes): "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,"
  box at: byte 32 — inside "مرحبا", 3 char(s) in
  layout: width 562.88, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 562.88, text_range 0..98
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   190.35  run    0..26        false       0    0  "An English sentence, then " (26 glyphs, baseline 22.00)
      190.35   231.96  run    32..51       true       1    0  "با بالعالم" (10 glyphs, baseline 22.00)
      231.96   271.96  BOX    @32             -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      271.96   295.09  run    26..32       true       1    0  "مرح" (3 glyphs, baseline 22.00)
      295.09   303.98  run    51..53       false       0    0  ", " (2 glyphs, baseline 22.00)
      303.98   492.55  run    53..80       false       0    0  "then back to English, then " (27 glyphs, baseline 22.00)
      492.55   558.44  run    80..97       true       1    0  "שלום עולם" (9 glyphs, baseline 22.00)
      558.44   562.88  run    97..98       false       0    0  "," (1 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ب' (U+0628)
    right neighbour 'ح' (U+062D)
  actual (parley):
    left neighbour  'ب' (U+0628)
    right neighbour 'ح' (U+062D)
  VERDICT: PASS

---------------------------------------------------------------
CASE a2-ltr-para-box-in-hebrew
  note: Same LTR paragraph, box inside the Hebrew run.
  text (98 bytes): "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,"
  box at: byte 84 — inside "שלום", 2 char(s) in
  layout: width 562.35, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 562.35, text_range 0..98
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   190.35  run    0..26        false       0    0  "An English sentence, then " (26 glyphs, baseline 22.00)
      190.35   254.56  run    26..51       true       1    0  "مرحبا بالعالم" (13 glyphs, baseline 22.00)
      254.56   263.45  run    51..53       false       0    0  ", " (2 glyphs, baseline 22.00)
      263.45   452.02  run    53..80       false       0    0  "then back to English, then " (27 glyphs, baseline 22.00)
      452.02   499.39  run    84..97       true       1    0  "ום עולם" (7 glyphs, baseline 22.00)
      499.39   539.39  BOX    @84             -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      539.39   557.91  run    80..84       true       1    0  "של" (2 glyphs, baseline 22.00)
      557.91   562.35  run    97..98       false       0    0  "," (1 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ו' (U+05D5)
    right neighbour 'ל' (U+05DC)
  actual (parley):
    left neighbour  'ו' (U+05D5)
    right neighbour 'ל' (U+05DC)
  VERDICT: PASS

---------------------------------------------------------------
CASE b1-arabic-para-box-in-arabic
  note: RTL (Arabic) base paragraph, box inside Arabic text.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 97 — inside "المنتصف", 3 char(s) in
  layout: width 436.10, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 436.10, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    38.41  run    97..106      true       1    0  "نتصف." (5 glyphs, baseline 22.00)
       38.41    78.41  BOX    @97             -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
       78.41   108.59  run    86..97       true       1    0  "في الم" (6 glyphs, baseline 22.00)
      108.59   113.03  run    85..86       true       1    0  " " (1 glyphs, baseline 22.00)
      113.03   145.93  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 22.00)
      145.93   436.10  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ن' (U+0646)
    right neighbour 'م' (U+0645)
  actual (parley):
    left neighbour  'ن' (U+0646)
    right neighbour 'م' (U+0645)
  VERDICT: PASS

---------------------------------------------------------------
CASE b2-arabic-para-box-in-english
  note: RTL (Arabic) base paragraph, box inside the embedded English word — an LTR island inside an RTL paragraph, the nested case M3-R1 names.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 83 — inside "Rust", 2 char(s) in
  layout: width 437.00, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 2
  line 0: baseline 22.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    69.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 22.00)
       69.48    73.93  run    85..86       true       1    0  " " (1 glyphs, baseline 22.00)
       73.93    94.38  run    81..83       false       2    0  "Ru" (2 glyphs, baseline 22.00)
       94.38   134.38  BOX    @83             -       2    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      134.38   146.83  run    83..85       false       2    0  "st" (2 glyphs, baseline 22.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'u' (U+0075)
    right neighbour 's' (U+0073)
  actual (parley):
    left neighbour  'u' (U+0075)
    right neighbour 's' (U+0073)
  VERDICT: PASS

---------------------------------------------------------------
CASE d1-hebrew-para-box-in-hebrew
  note: RTL (Hebrew) base paragraph, box inside Hebrew text.
  text (93 bytes): "שורה בעברית עם הדגשה ומילה באנגלית parley באמצע המשפט."
  box at: byte 15 — inside "בעברית", 3 char(s) in
  layout: width 445.39, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 445.39, text_range 0..93
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   100.85  run    71..93       true       1    0  "באמצע המשפט." (12 glyphs, baseline 22.00)
      100.85   105.30  run    70..71       true       1    0  " " (1 glyphs, baseline 22.00)
      105.30   148.88  run    64..70       false       2    0  "parley" (6 glyphs, baseline 22.00)
      148.88   342.30  run    15..64       true       1    0  "רית עם הדגשה ומילה באנגלית " (27 glyphs, baseline 22.00)
      342.30   382.30  BOX    @15             -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      382.30   445.39  run    0..15        true       1    0  "שורה בעב" (8 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ר' (U+05E8)
    right neighbour 'ב' (U+05D1)
  actual (parley):
    left neighbour  'ר' (U+05E8)
    right neighbour 'ב' (U+05D1)
  VERDICT: PASS

---------------------------------------------------------------
CASE d2-hebrew-para-box-in-english
  note: RTL (Hebrew) base paragraph, box inside the embedded English word.
  text (93 bytes): "שורה בעברית עם הדגשה ומילה באנגלית parley באמצע המשפט."
  box at: byte 67 — inside "parley", 3 char(s) in
  layout: width 445.39, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 2
  line 0: baseline 22.00, advance 445.39, text_range 0..93
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   100.85  run    71..93       true       1    0  "באמצע המשפט." (12 glyphs, baseline 22.00)
      100.85   105.30  run    70..71       true       1    0  " " (1 glyphs, baseline 22.00)
      105.30   128.42  run    64..67       false       2    0  "par" (3 glyphs, baseline 22.00)
      128.42   168.42  BOX    @67             -       2    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      168.42   188.88  run    67..70       false       2    0  "ley" (3 glyphs, baseline 22.00)
      188.88   445.39  run    0..64        true       1    0  "שורה בעברית עם הדגשה ומילה באנגלית " (35 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'r' (U+0072)
    right neighbour 'l' (U+006C)
  actual (parley):
    left neighbour  'r' (U+0072)
    right neighbour 'l' (U+006C)
  VERDICT: PASS

---------------------------------------------------------------
CASE e1-rtl-para-box-at-paragraph-start
  note: An inline image as the FIRST thing in an Arabic paragraph. There is no preceding run for the box to inherit a level from. UAX #9 gives U+FFFC the paragraph level here (sos = R), so it belongs at the far RIGHT.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 0 — before the first letter of an RTL paragraph
  layout: width 437.00, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    40.00  BOX    @0              -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
       40.00   109.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 22.00)
      109.48   113.93  run    85..86       true       1    0  " " (1 glyphs, baseline 22.00)
      113.93   146.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 22.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ه' (U+0647)
    right neighbour <line edge>
  actual (parley):
    left neighbour  <line edge>
    right neighbour '.' (U+002E)
  VERDICT: FAIL
    expected visual order around the box: ...برع صن اذه█...

---------------------------------------------------------------
CASE e2-rtl-para-box-at-paragraph-end
  note: The mirror image: an inline image as the last thing in an Arabic paragraph.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 106 — after the final character of an RTL paragraph
  layout: width 437.00, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    40.00  BOX    @106            -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
       40.00   109.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 22.00)
      109.48   113.93  run    85..86       true       1    0  " " (1 glyphs, baseline 22.00)
      113.93   146.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 22.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  <line edge>
    right neighbour '.' (U+002E)
  actual (parley):
    left neighbour  <line edge>
    right neighbour '.' (U+002E)
  VERDICT: PASS

---------------------------------------------------------------
CASE e3-ltr-para-box-at-ltr-to-rtl-boundary
  note: LTR paragraph, box exactly where the Arabic run begins — an image between an English word and an Arabic one.
  text (98 bytes): "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,"
  box at: byte 26 — at the leading edge of "مرحبا"
  layout: width 562.35, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 0
  line 0: baseline 22.00, advance 562.35, text_range 0..98
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   190.35  run    0..26        false       0    0  "An English sentence, then " (26 glyphs, baseline 22.00)
      190.35   230.35  BOX    @26             -       0    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      230.35   294.56  run    26..51       true       1    0  "مرحبا بالعالم" (13 glyphs, baseline 22.00)
      294.56   303.45  run    51..53       false       0    0  ", " (2 glyphs, baseline 22.00)
      303.45   492.02  run    53..80       false       0    0  "then back to English, then " (27 glyphs, baseline 22.00)
      492.02   557.91  run    80..97       true       1    0  "שלום עולם" (9 glyphs, baseline 22.00)
      557.91   562.35  run    97..98       false       0    0  "," (1 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'n' (U+006E)
    right neighbour 'م' (U+0645)
  actual (parley):
    left neighbour  'n' (U+006E)
    right neighbour 'م' (U+0645)
  VERDICT: PASS

---------------------------------------------------------------
CASE e4-rtl-para-box-at-rtl-to-ltr-boundary
  note: RTL paragraph, box exactly where the embedded English word begins — the level-1 to level-2 boundary.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 81 — at the leading edge of "Rust"
  layout: width 437.00, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 22.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    69.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 22.00)
       69.48    73.93  run    85..86       true       1    0  " " (1 glyphs, baseline 22.00)
       73.93   106.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 22.00)
      106.83   146.83  BOX    @81             -       1    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  't' (U+0074)
    right neighbour 'ة' (U+0629)
  actual (parley):
    left neighbour  't' (U+0074)
    right neighbour 'ة' (U+0629)
  VERDICT: PASS

---------------------------------------------------------------
CASE e5-ltr-para-box-at-paragraph-start
  note: Control for e1: the same paragraph-leading box, but LTR throughout.
  text (59 bytes): "line, which is where caret motion and hit-testing go wrong."
  box at: byte 0 — before the first letter of an LTR paragraph
  layout: width 450.88, height 24.00, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 0
  line 0: baseline 22.00, advance 450.88, text_range 0..59
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    40.00  BOX    @0              -       0    -  <INLINE BOX id=1 y=-2.00 h=24.00 baseline=None>
       40.00   450.88  run    0..59        false       0    0  "line, which is where caret motion and hit-testing go wrong." (59 glyphs, baseline 22.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  <line edge>
    right neighbour 'l' (U+006C)
  actual (parley):
    left neighbour  <line edge>
    right neighbour 'l' (U+006C)
  VERDICT: PASS

---------------------------------------------------------------
PROBE: inline-box baseline (PR #639) — arm parley 0.11.0 (crates.io, `parley = "=0.11.0"`)
  InlineBox::baseline           : ABSENT (field does not exist in 0.11.0)
  PositionedInlineBox::baseline : ABSENT (field does not exist in 0.11.0)
  Layout::first_baseline()      : ABSENT (method does not exist in 0.11.0)
  Layout::last_baseline()       : ABSENT (method does not exist in 0.11.0)
  line metrics baseline         : 22.0000
  box: x 23.1172 y -2.0000 w 40.0000 h 24.0000  (bottom edge y+h = 22.0000)

---------------------------------------------------------------
PROBE: does an inline box break cursive joining? — arm parley 0.11.0 (crates.io, `parley = "=0.11.0"`)
  arabic   "مرحبا بالعالم" split at byte 6: glyph ids CHANGED    (text advance 64.21 -> 64.74, delta +0.53)
      without box: [993, 991, 910, 972, 991, 910, 913, 3, 910, 913, 931, 941, 995]
      with box   : [929, 941, 995, 993, 991, 910, 972, 991, 910, 913, 3, 910, 913]
  hebrew   "שלום עולם" split at byte 4: glyph ids UNCHANGED  (text advance 65.88 -> 65.88, delta +0.00)
  latin    "handwriting" split at byte 4: glyph ids UNCHANGED  (text advance 81.83 -> 81.83, delta +0.00)


=== SUMMARY — arm: parley 0.11.0 (crates.io, `parley = "=0.11.0"`) ===
  c-control-pure-ltr                 PASS
  a1-ltr-para-box-in-arabic          PASS
  a2-ltr-para-box-in-hebrew          PASS
  b1-arabic-para-box-in-arabic       PASS
  b2-arabic-para-box-in-english      PASS
  d1-hebrew-para-box-in-hebrew       PASS
  d2-hebrew-para-box-in-english      PASS
  e1-rtl-para-box-at-paragraph-start FAIL
  e2-rtl-para-box-at-paragraph-end   PASS
  e3-ltr-para-box-at-ltr-to-rtl-boundary PASS
  e4-rtl-para-box-at-rtl-to-ltr-boundary PASS
  e5-ltr-para-box-at-paragraph-start PASS
  11 of 12 cases place the box in the bidi-correct visual slot.
```

### `cargo run -p e1-parley-main`

```
=== E1 bidi/inline-box harness — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e (2026-08-10) ===
corpus: bench/corpus/rtl.md (unmodified; markdown markers stripped at layout time)
box: width 40, height 24, kind InFlow; font size 16; single line (max_advance = None)

---------------------------------------------------------------
CASE c-control-pure-ltr
  note: Control. Base LTR, no RTL anywhere. Whatever this looks like is what 'placed correctly' means when bidi is not involved.
  text (59 bytes): "line, which is where caret motion and hit-testing go wrong."
  box at: byte 24 — inside "caret", 3 char(s) in
  layout: width 450.88, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 0
  line 0: baseline 24.00, advance 450.88, text_range 0..59
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   165.40  run    0..24        false       0    0  "line, which is where car" (24 glyphs, baseline 24.00)
      165.40   205.40  BOX    @24             -       0    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      205.40   450.88  run    24..59       false       0    0  "et motion and hit-testing go wrong." (35 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'r' (U+0072)
    right neighbour 'e' (U+0065)
  actual (parley):
    left neighbour  'r' (U+0072)
    right neighbour 'e' (U+0065)
  VERDICT: PASS

---------------------------------------------------------------
CASE a1-ltr-para-box-in-arabic
  note: LTR paragraph, box inside the Arabic run. The box must land between two Arabic letters, i.e. INSIDE the RTL run's visual extent.
  text (98 bytes): "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,"
  box at: byte 32 — inside "مرحبا", 3 char(s) in
  layout: width 562.35, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 562.35, text_range 0..98
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   190.35  run    0..26        false       0    0  "An English sentence, then " (26 glyphs, baseline 24.00)
      190.35   231.96  run    32..51       true       1    0  "با بالعالم" (10 glyphs, baseline 24.00)
      231.96   271.96  BOX    @32             -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      271.96   294.56  run    26..32       true       1    0  "مرح" (3 glyphs, baseline 24.00)
      294.56   303.45  run    51..53       false       0    0  ", " (2 glyphs, baseline 24.00)
      303.45   492.02  run    53..80       false       0    0  "then back to English, then " (27 glyphs, baseline 24.00)
      492.02   557.91  run    80..97       true       1    0  "שלום עולם" (9 glyphs, baseline 24.00)
      557.91   562.35  run    97..98       false       0    0  "," (1 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ب' (U+0628)
    right neighbour 'ح' (U+062D)
  actual (parley):
    left neighbour  'ب' (U+0628)
    right neighbour 'ح' (U+062D)
  VERDICT: PASS

---------------------------------------------------------------
CASE a2-ltr-para-box-in-hebrew
  note: Same LTR paragraph, box inside the Hebrew run.
  text (98 bytes): "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,"
  box at: byte 84 — inside "שלום", 2 char(s) in
  layout: width 562.35, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 562.35, text_range 0..98
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   190.35  run    0..26        false       0    0  "An English sentence, then " (26 glyphs, baseline 24.00)
      190.35   254.56  run    26..51       true       1    0  "مرحبا بالعالم" (13 glyphs, baseline 24.00)
      254.56   263.45  run    51..53       false       0    0  ", " (2 glyphs, baseline 24.00)
      263.45   452.02  run    53..80       false       0    0  "then back to English, then " (27 glyphs, baseline 24.00)
      452.02   499.39  run    84..97       true       1    0  "ום עולם" (7 glyphs, baseline 24.00)
      499.39   539.39  BOX    @84             -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      539.39   557.91  run    80..84       true       1    0  "של" (2 glyphs, baseline 24.00)
      557.91   562.35  run    97..98       false       0    0  "," (1 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ו' (U+05D5)
    right neighbour 'ל' (U+05DC)
  actual (parley):
    left neighbour  'ו' (U+05D5)
    right neighbour 'ל' (U+05DC)
  VERDICT: PASS

---------------------------------------------------------------
CASE b1-arabic-para-box-in-arabic
  note: RTL (Arabic) base paragraph, box inside Arabic text.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 97 — inside "المنتصف", 3 char(s) in
  layout: width 437.00, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    38.41  run    97..106      true       1    0  "نتصف." (5 glyphs, baseline 24.00)
       38.41    78.41  BOX    @97             -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
       78.41   109.48  run    86..97       true       1    0  "في الم" (6 glyphs, baseline 24.00)
      109.48   113.93  run    85..86       true       1    0  " " (1 glyphs, baseline 24.00)
      113.93   146.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 24.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ن' (U+0646)
    right neighbour 'م' (U+0645)
  actual (parley):
    left neighbour  'ن' (U+0646)
    right neighbour 'م' (U+0645)
  VERDICT: PASS

---------------------------------------------------------------
CASE b2-arabic-para-box-in-english
  note: RTL (Arabic) base paragraph, box inside the embedded English word — an LTR island inside an RTL paragraph, the nested case M3-R1 names.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 83 — inside "Rust", 2 char(s) in
  layout: width 437.00, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 2
  line 0: baseline 24.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    69.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 24.00)
       69.48    73.93  run    85..86       true       1    0  " " (1 glyphs, baseline 24.00)
       73.93    94.38  run    81..83       false       2    0  "Ru" (2 glyphs, baseline 24.00)
       94.38   134.38  BOX    @83             -       2    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      134.38   146.83  run    83..85       false       2    0  "st" (2 glyphs, baseline 24.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'u' (U+0075)
    right neighbour 's' (U+0073)
  actual (parley):
    left neighbour  'u' (U+0075)
    right neighbour 's' (U+0073)
  VERDICT: PASS

---------------------------------------------------------------
CASE d1-hebrew-para-box-in-hebrew
  note: RTL (Hebrew) base paragraph, box inside Hebrew text.
  text (93 bytes): "שורה בעברית עם הדגשה ומילה באנגלית parley באמצע המשפט."
  box at: byte 15 — inside "בעברית", 3 char(s) in
  layout: width 445.39, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 445.39, text_range 0..93
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   100.85  run    71..93       true       1    0  "באמצע המשפט." (12 glyphs, baseline 24.00)
      100.85   105.30  run    70..71       true       1    0  " " (1 glyphs, baseline 24.00)
      105.30   148.88  run    64..70       false       2    0  "parley" (6 glyphs, baseline 24.00)
      148.88   342.30  run    15..64       true       1    0  "רית עם הדגשה ומילה באנגלית " (27 glyphs, baseline 24.00)
      342.30   382.30  BOX    @15             -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      382.30   445.39  run    0..15        true       1    0  "שורה בעב" (8 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ר' (U+05E8)
    right neighbour 'ב' (U+05D1)
  actual (parley):
    left neighbour  'ר' (U+05E8)
    right neighbour 'ב' (U+05D1)
  VERDICT: PASS

---------------------------------------------------------------
CASE d2-hebrew-para-box-in-english
  note: RTL (Hebrew) base paragraph, box inside the embedded English word.
  text (93 bytes): "שורה בעברית עם הדגשה ומילה באנגלית parley באמצע המשפט."
  box at: byte 67 — inside "parley", 3 char(s) in
  layout: width 445.39, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 2
  line 0: baseline 24.00, advance 445.39, text_range 0..93
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   100.85  run    71..93       true       1    0  "באמצע המשפט." (12 glyphs, baseline 24.00)
      100.85   105.30  run    70..71       true       1    0  " " (1 glyphs, baseline 24.00)
      105.30   128.42  run    64..67       false       2    0  "par" (3 glyphs, baseline 24.00)
      128.42   168.42  BOX    @67             -       2    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      168.42   188.88  run    67..70       false       2    0  "ley" (3 glyphs, baseline 24.00)
      188.88   445.39  run    0..64        true       1    0  "שורה בעברית עם הדגשה ומילה באנגלית " (35 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'r' (U+0072)
    right neighbour 'l' (U+006C)
  actual (parley):
    left neighbour  'r' (U+0072)
    right neighbour 'l' (U+006C)
  VERDICT: PASS

---------------------------------------------------------------
CASE e1-rtl-para-box-at-paragraph-start
  note: An inline image as the FIRST thing in an Arabic paragraph. There is no preceding run for the box to inherit a level from. UAX #9 gives U+FFFC the paragraph level here (sos = R), so it belongs at the far RIGHT.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 0 — before the first letter of an RTL paragraph
  layout: width 437.00, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    40.00  BOX    @0              -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
       40.00   109.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 24.00)
      109.48   113.93  run    85..86       true       1    0  " " (1 glyphs, baseline 24.00)
      113.93   146.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 24.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'ه' (U+0647)
    right neighbour <line edge>
  actual (parley):
    left neighbour  <line edge>
    right neighbour '.' (U+002E)
  VERDICT: FAIL
    expected visual order around the box: ...برع صن اذه█...

---------------------------------------------------------------
CASE e2-rtl-para-box-at-paragraph-end
  note: The mirror image: an inline image as the last thing in an Arabic paragraph.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 106 — after the final character of an RTL paragraph
  layout: width 437.00, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    40.00  BOX    @106            -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
       40.00   109.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 24.00)
      109.48   113.93  run    85..86       true       1    0  " " (1 glyphs, baseline 24.00)
      113.93   146.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 24.00)
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  <line edge>
    right neighbour '.' (U+002E)
  actual (parley):
    left neighbour  <line edge>
    right neighbour '.' (U+002E)
  VERDICT: PASS

---------------------------------------------------------------
CASE e3-ltr-para-box-at-ltr-to-rtl-boundary
  note: LTR paragraph, box exactly where the Arabic run begins — an image between an English word and an Arabic one.
  text (98 bytes): "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,"
  box at: byte 26 — at the leading edge of "مرحبا"
  layout: width 562.35, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 0
  line 0: baseline 24.00, advance 562.35, text_range 0..98
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00   190.35  run    0..26        false       0    0  "An English sentence, then " (26 glyphs, baseline 24.00)
      190.35   230.35  BOX    @26             -       0    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      230.35   294.56  run    26..51       true       1    0  "مرحبا بالعالم" (13 glyphs, baseline 24.00)
      294.56   303.45  run    51..53       false       0    0  ", " (2 glyphs, baseline 24.00)
      303.45   492.02  run    53..80       false       0    0  "then back to English, then " (27 glyphs, baseline 24.00)
      492.02   557.91  run    80..97       true       1    0  "שלום עולם" (9 glyphs, baseline 24.00)
      557.91   562.35  run    97..98       false       0    0  "," (1 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  'n' (U+006E)
    right neighbour 'م' (U+0645)
  actual (parley):
    left neighbour  'n' (U+006E)
    right neighbour 'م' (U+0645)
  VERDICT: PASS

---------------------------------------------------------------
CASE e4-rtl-para-box-at-rtl-to-ltr-boundary
  note: RTL paragraph, box exactly where the embedded English word begins — the level-1 to level-2 boundary.
  text (106 bytes): "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية Rust في المنتصف."
  box at: byte 81 — at the leading edge of "Rust"
  layout: width 437.00, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 1 (RTL), level resolved for U+FFFC at the box offset: 1
  line 0: baseline 24.00, advance 437.00, text_range 0..106
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    69.48  run    86..106      true       1    0  "في المنتصف." (11 glyphs, baseline 24.00)
       69.48    73.93  run    85..86       true       1    0  " " (1 glyphs, baseline 24.00)
       73.93   106.83  run    81..85       false       2    0  "Rust" (4 glyphs, baseline 24.00)
      106.83   146.83  BOX    @81             -       1    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
      146.83   437.00  run    0..81        true       1    0  "هذا نص عربي يحتوي على نص عريض وكلمة إنجليزية " (45 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  't' (U+0074)
    right neighbour 'ة' (U+0629)
  actual (parley):
    left neighbour  't' (U+0074)
    right neighbour 'ة' (U+0629)
  VERDICT: PASS

---------------------------------------------------------------
CASE e5-ltr-para-box-at-paragraph-start
  note: Control for e1: the same paragraph-leading box, but LTR throughout.
  text (59 bytes): "line, which is where caret motion and hit-testing go wrong."
  box at: byte 0 — before the first letter of an LTR paragraph
  layout: width 450.88, height 28.40, 1 line(s)
  unicode-bidi: paragraph level 0 (LTR), level resolved for U+FFFC at the box offset: 0
  line 0: baseline 24.00, advance 450.88, text_range 0..59
  items, left to right:
          x0       x1  kind   bytes         rtl lvl(ub) tofu  text / box
        0.00    40.00  BOX    @0              -       0    -  <INLINE BOX id=1 y=0.00 h=24.00 baseline=None>
       40.00   450.88  run    0..59        false       0    0  "line, which is where caret motion and hit-testing go wrong." (59 glyphs, baseline 24.00)
  expected (unicode-bidi, U+FFFC stand-in):
    left neighbour  <line edge>
    right neighbour 'l' (U+006C)
  actual (parley):
    left neighbour  <line edge>
    right neighbour 'l' (U+006C)
  VERDICT: PASS

---------------------------------------------------------------
PROBE: inline-box baseline (PR #639) — arm parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e (2026-08-10)
  --- InlineBox::baseline = None
      Layout::first_baseline() = Some(24.0), Layout::last_baseline() = Some(24.0)
      line metrics baseline = 24.0000
      box: x 23.1172 y 0.0000 w 40.0000 h 24.0000 baseline None; bottom edge 24.0000; box baseline in layout coords None
  --- InlineBox::baseline = Some(24.0)
      Layout::first_baseline() = Some(24.0), Layout::last_baseline() = Some(24.0)
      line metrics baseline = 24.0000
      box: x 23.1172 y 0.0000 w 40.0000 h 24.0000 baseline Some(24.0); bottom edge 24.0000; box baseline in layout coords Some(24.0)
  --- InlineBox::baseline = Some(12.0)
      Layout::first_baseline() = Some(14.0), Layout::last_baseline() = Some(14.0)
      line metrics baseline = 14.0000
      box: x 23.1172 y 2.0000 w 40.0000 h 24.0000 baseline Some(12.0); bottom edge 26.0000; box baseline in layout coords Some(14.0)
  --- InlineBox::baseline = Some(0.0)
      Layout::first_baseline() = Some(14.0), Layout::last_baseline() = Some(14.0)
      line metrics baseline = 14.0000
      box: x 23.1172 y 14.0000 w 40.0000 h 24.0000 baseline Some(0.0); bottom edge 38.0000; box baseline in layout coords Some(14.0)

---------------------------------------------------------------
PROBE: does an inline box break cursive joining? — arm parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e (2026-08-10)
  arabic   "مرحبا بالعالم" split at byte 6: glyph ids UNCHANGED  (text advance 64.21 -> 64.21, delta +0.00)
  hebrew   "שלום עולם" split at byte 4: glyph ids UNCHANGED  (text advance 65.88 -> 65.88, delta +0.00)
  latin    "handwriting" split at byte 4: glyph ids UNCHANGED  (text advance 81.83 -> 81.83, delta +0.00)


=== SUMMARY — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e (2026-08-10) ===
  c-control-pure-ltr                 PASS
  a1-ltr-para-box-in-arabic          PASS
  a2-ltr-para-box-in-hebrew          PASS
  b1-arabic-para-box-in-arabic       PASS
  b2-arabic-para-box-in-english      PASS
  d1-hebrew-para-box-in-hebrew       PASS
  d2-hebrew-para-box-in-english      PASS
  e1-rtl-para-box-at-paragraph-start FAIL
  e2-rtl-para-box-at-paragraph-end   PASS
  e3-ltr-para-box-at-ltr-to-rtl-boundary PASS
  e4-rtl-para-box-at-rtl-to-ltr-boundary PASS
  e5-ltr-para-box-at-paragraph-start PASS
  11 of 12 cases place the box in the bidi-correct visual slot.
```

### `cargo run -p e1-fonts-0-11`

```
=== E1 font-strategy probe — arm: parley 0.11.0 (crates.io), default-features = false, features = ["std"] ===

(a) parley with default-features = false, features = ["std"]: BUILDS and RUNS.
    (this binary is that build; see this crate's Cargo.toml)

(b) FontContext assembled with CollectionOptions { system_fonts: false, .. }:
    Collection::new(CollectionOptions { shared: false, system_fonts: false })
    families visible before registering anything: 0

(c) registering MarkText's bundled faces from C:\Dev\marktext\packages\muya\src\assets\styles\fonts
    API: Collection::register_fonts(Blob<u8>, Option<FontInfoOverride>) -> Vec<(FamilyId, Vec<FontInfo>)>
    file                                                bytes  magic (ascii+hex) register_fonts result
    DejaVuSansMono-Bold.ttf                            331992  .... 00010000  OK — "DejaVu Sans Mono" x1 
    DejaVuSansMono-BoldOblique.ttf                     253580  .... 00010000  OK — "DejaVu Sans Mono" x1 
    DejaVuSansMono-Oblique.ttf                         251932  .... 00010000  OK — "DejaVu Sans Mono" x1 
    DejaVuSansMono.ttf                                 340712  .... 00010000  OK — "DejaVu Sans Mono" x1 
    open-sans-v27-latin-ext_latin-300.woff              27512  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-300italic.woff        29336  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-600.woff              27416  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-600italic.woff        29176  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-700.woff              26612  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-700italic.woff        28244  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-italic.woff           29256  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-regular.woff          27480  wOFF 774F4646  REJECTED — 0 families

    families now registered (no system enumeration): ["DejaVu Sans Mono"]
    total families in collection: 1

(4) laying out with ONLY the bundled faces available (no system fonts):
    font stack forced to "DejaVu Sans Mono"
    sample   text                          glyphs  notdef    width  verdict
    latin    The quick brown fox 12345         25       0   240.82  renders
    arabic   هذا نص عربي                       11       0   105.96  renders
    hebrew   שלום עולם                          9       8    86.70  partial tofu

=== complex-scripts ===
  this crate re-exports parley's `complex-scripts` as its own feature; enabled here: false
  the dependency cost is a `cargo tree` question, not a runtime one —
  the results file records the diff with and without.
```

### `cargo run -p e1-fonts-main`

```
=== E1 font-strategy probe — arm: parley git main @ a0752c7bd, default-features = false, features = ["std"] ===

(a) parley with default-features = false, features = ["std"]: BUILDS and RUNS.
    (this binary is that build; see this crate's Cargo.toml)

(b) FontContext assembled with CollectionOptions { system_fonts: false, .. }:
    Collection::new(CollectionOptions { shared: false, system_fonts: false })
    families visible before registering anything: 0

(c) registering MarkText's bundled faces from C:\Dev\marktext\packages\muya\src\assets\styles\fonts
    API: Collection::register_fonts(Blob<u8>, Option<FontInfoOverride>) -> Vec<(FamilyId, Vec<FontInfo>)>
    file                                                bytes  magic (ascii+hex) register_fonts result
    DejaVuSansMono-Bold.ttf                            331992  .... 00010000  OK — "DejaVu Sans Mono" x1 
    DejaVuSansMono-BoldOblique.ttf                     253580  .... 00010000  OK — "DejaVu Sans Mono" x1 
    DejaVuSansMono-Oblique.ttf                         251932  .... 00010000  OK — "DejaVu Sans Mono" x1 
    DejaVuSansMono.ttf                                 340712  .... 00010000  OK — "DejaVu Sans Mono" x1 
    open-sans-v27-latin-ext_latin-300.woff              27512  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-300italic.woff        29336  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-600.woff              27416  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-600italic.woff        29176  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-700.woff              26612  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-700italic.woff        28244  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-italic.woff           29256  wOFF 774F4646  REJECTED — 0 families
    open-sans-v27-latin-ext_latin-regular.woff          27480  wOFF 774F4646  REJECTED — 0 families

    families now registered (no system enumeration): ["DejaVu Sans Mono"]
    total families in collection: 1

(4) laying out with ONLY the bundled faces available (no system fonts):
    font stack forced to "DejaVu Sans Mono"
    sample   text                          glyphs  notdef    width  verdict
    latin    The quick brown fox 12345         25       0   240.82  renders
    arabic   هذا نص عربي                       11       0   105.96  renders
    hebrew   שלום עולם                          9       8    86.70  partial tofu

=== complex-scripts ===
  this crate re-exports parley's `complex-scripts` as its own feature; enabled here: false
  the dependency cost is a `cargo tree` question, not a runtime one —
  the results file records the diff with and without.
```
