# `bench/layout-goldens/` — the textual layout goldens

Per docs/M3.md §5 **D10**. One line-oriented text file per
(`bench/corpus/` file × shipped theme), holding a stable serialization of the
display list `mt_layout::layout` produces. Written and compared by exactly one
program:

```sh
cargo xtask layout            # compare, on exact equality
cargo xtask layout --update   # rewrite. The SOLE writer.
```

§11.3 asks for *golden images per block type per platform, with a perceptual
diff threshold*. That is S4's instrument, on `mt-render`. `mt-layout` is
headless and deserves a **textual** golden, because **a text diff names the
block that moved and a pixel diff does not**. Two artifacts, two stages, two
thresholds: exact equality for layout, a perceptual threshold for pixels.

---

## A golden update is a reviewable event

**On the same footing as a parley pin move.** This is
`spec/expected-failures.json`'s ONE-WAY DOOR rule pointed at a new artifact,
and it is the whole of M3-R6's mitigation:

> **M3-R6 — goldens rot into rubber stamps. A golden nobody reads passes
> forever.**

Four rules, and none of them is enforceable by a program:

1. **Never run `--update` to make a red build green.** Run it because you
   changed something that *should* have moved the geometry, and then read the
   diff to check that it moved the way you intended. A diff you did not predict
   is a finding.
2. **Regenerate in the same commit as the change that caused it**, never in a
   commit of its own. A commit titled "update goldens" is a commit nobody can
   review, because the reviewer has nothing to compare the numbers against.
3. **Say in the commit message what moved and why.** "Tight lists drop their
   item margins, so every `10kb.md` list item rises by 8px" is reviewable.
   "Regenerated goldens" is not.
4. **A diff in the header is a bigger event than a diff in the body.** The
   header pins the shaper and the font set; if it moved, *every* number below
   it moved for a reason that has nothing to do with the code you were writing.

`--update` is never implied by any other flag, refuses to run alongside
`--glyphs` or `--measure`, and prints the first three rules back at you when it
writes anything.

---

## The format

Line-oriented custom text — **not JSON, and specifically not `insta`.** A
golden's whole job is to make `git diff` name the block that moved, and brace
and indent noise defeats that. `insta`'s review flow is an accept-all button,
which is precisely the rubber-stamp failure mode M3-R6 names; adopting it would
import the risk the register already lists. Hand-rolled over dependency is also
the house style, for the reason `xtask/src/deps.rs:14-17` gives.

Every file has the same three parts, separated by blank lines:

```text
layout-golden v1                       <- format version
theme          muya-default            ┐
theme-width    800.00                  │
content-width  700.00                  │
parley         a0752c7b…               ├ provenance: ten fields, one per line
faces          ce47b224… rev 1         │
options        wrap-code-blocks=false line-numbers=false
input          rtl.md                  │
input-bytes    1103                    │
form           full                    ┘

list           width=700.00 height=863.40 blocks=27
count paragraph      8                 ┐ the census: one line per BlockKind,
count atx-heading    4                 │ all nineteen, always, including the
count setext-heading 0                 ┘ zeroes

block 0 atx-heading bounds=[0.00 0.00 700.00 42.00] items=1
  glyphs font=OpenSans-Bold size=30.00 rtl=0 text=0..34 origin=[0.00 32.00] advance=513.87 n=34 seq=6d58f8fc
block 1 paragraph bounds=[0.00 58.00 700.00 102.40] items=4
  glyphs font=OpenSans-Regular size=16.00 rtl=0 text=0..69 origin=[0.00 76.00] advance=499.73 n=67 seq=771c52d4
```

**The blank line after the provenance is a seam with a job**: everything above
it describes how the golden was produced, everything below it is geometry. The
both-themes-must-differ check cuts there, and so should your eye.

### The header fields

| Field | What it pins |
|---|---|
| `layout-golden v1` | the format. Bumped when the serialization changes shape, so a file written by an older tool is identifiable from the artifact |
| `theme` | which of the two shipped themes |
| `theme-width` | `[metrics] content_width_px` — the CSS `max-width`, **800 or 750** |
| `content-width` | that less `2 × container_padding_x_px` — **700 or 650**, the column text actually wraps in |
| `parley` | **the commit `Cargo.lock` resolved parley to**, not the one `Cargo.toml` asked for. D2 pinned it and says a pin move is a reviewable event; this line is what makes that an artifact rather than an instruction |
| `faces` | SHA-256 of `assets/fonts/faces.toml`, and its `[meta] revision` |
| `options` | `LayoutOptions`, both at muya's own defaults. A golden generated with either flag on is a **different artifact** |
| `input` / `input-bytes` | which corpus file, CRLF-normalised |
| `form` | `full` or `digest` — see below |

**The `faces` hash is an extension of D10**, which names only theme, width and
the parley revision. It is here for D10's own argument: the font set determines
the geometry exactly as much as the shaper does. Reorder the fallback chain and
every fallback run in the corpus moves; `faces.toml` says so itself —

> **THE ORDER OF `[[face]]` ENTRIES IS LOAD-BEARING.** … Reordering these
> entries is a golden-changing event on the same footing as moving the parley
> pin (D2) — review it as one.

Without this field that sentence has no artifact behind it. With it, a face
swap is a one-line diff at the top of all twenty-two files, exactly as a pin
move is.

`cargo xtask layout` additionally **fails** if `Cargo.lock`'s parley revision
and `faces.toml`'s `[meta] parley_rev` disagree: a face list measured against
one shaper and used with another is a coverage claim about a program that is
not running.

### Floats are two decimals, fixed

D10 property 4. Goldens are produced from a pinned font set (D7) and a pinned
parley (D2), so **a platform that disagrees at 2 dp is a finding**, and
loosening the precision is a reviewable event rather than a fix — that
distinction is what keeps a threshold from quietly absorbing a real divergence.

`-0.00` is normalised to `0.00`. IEEE-754 has two zeroes and the sign bit
survives arithmetic that produces one, so without the normalisation a golden
could differ between platforms on a value that is numerically identical — a
false positive whose obvious "fix" is exactly the loosening the paragraph above
forbids.

### Granularity: per block and per item, with a glyph-sequence hash

D10 leaves granularity open. It is **one line per block and one per display
item**, because a per-glyph dump is roughly twenty times larger and makes the
diff name a glyph rather than a block — which defeats the property the format
exists for.

What a per-item line would otherwise miss is caught by `seq=`, the first eight
hex digits of a SHA-256 over the run's `(id, x, y)` sequence. A shaping change
that preserves the glyph count *and* the total advance but re-substitutes or
re-orders glyphs inside the run — a ligature change, a `GSUB` change — still
fails. The hash covers the **formatted two-decimal text**, not the raw `f32`
bits, so the artifact's sensitivity is 2 dp everywhere rather than 2 dp in the
numbers you can read and full precision in the one you cannot.

`cargo xtask layout --glyphs --only <SUBSTR>` prints the per-glyph view for
debugging. It writes nothing, compares nothing, and labels itself
`form  full+glyphs` in the header so a saved dump can never be mistaken for a
golden.

### The item lines

```text
  glyphs font=<face file stem> size= rtl=0|1 text=<start>..<end> origin=[x baseline] advance= n= seq=
  rect [x y w h] fill=#rrggbbaa [radius=] [rot=]
  line [x0 y0]-[x1 y1] width= style=solid|dashed|dotted|none stroke=#rrggbbaa
  inline-box id= [x y w h] baseline=<f|none> flow=in-flow|out-of-flow|custom-out-of-flow
```

`radius=`, `rot=`, and the block line's `lang=` and `overflow=` appear **only
when they say something**. `lang=` is D11's evidence that math, diagrams,
front matter and HTML carry their language; `overflow=` is the tell that a
block scrolls rather than wraps.

The face is named by its **file**, not by its `FontId`. A `FontId` is an index
into whatever order the shell happened to register in, so `font7` would be both
unreadable and unstable under a `faces.toml` reorder that changed nothing about
the glyphs.

### Two things the format deliberately does not carry

- **`BlockDisplay::node`.** A `NodeId` is an arena slot, an artifact of
  `mt-md`'s allocation order rather than of geometry, so putting it in a golden
  would make an unrelated parser change rewrite every file. What must hold is
  that the block → node mapping is injective, and `cargo xtask layout` asserts
  that on every run instead.
- **Trailing whitespace, twice.** parley's `Layout::width()` **excludes**
  trailing whitespace where a glyph run's `offset + advance` **includes** it,
  so the two disagree on any line ending in spaces. The golden reports the
  **run's** `origin` and `advance`, because those are what a renderer draws
  from and what M4 will hit-test against. `Layout::width()` reaches a golden
  only indirectly, through `BlockDisplay::overflow_x`, which is the one place
  `mt-layout` deliberately asks the *ink* question rather than the advance one.

---

## Two forms: full, and digest

D10 says one file per (corpus file × theme), which taken literally includes the
corpus's three large prose files. **Measured** — `cargo xtask layout --measure`
prints this table, so the cut can be re-checked rather than believed:

| Input | Blocks | Items | Full golden, `muya-default` | Full golden, `dark` | Committed |
|---|---:|---:|---:|---:|---:|
| `250kb.md` | 2,686 | 6,483 | **801,984 B** | **804,314 B** | 968 / 960 B |
| `1mb.md` | 10,744 | 26,173 | **3,241,917 B** | **3,265,804 B** | 976 / 968 B |
| `5mb.md` | 54,896 | 133,498 | **16,658,369 B** | **16,763,187 B** | 984 / 976 B |

Full goldens for those three would be **41.5 MB** across both themes, against
**5.8 KB** as digests, and `5mb.md`'s would be a single 16 MB, 190,000-line
file. **A golden a human cannot open is M3-R6's failure mode with extra
steps**, so the three get a digest instead:

```text
<the same ten header fields, with form = digest>

list           width=700.00 height=2783862.25 blocks=54896
full-sha256    c9870aa79afbddb0a2f9664a3c3df12cee9f6b5b84e0085b990a92c1840f9cde
full-bytes     16657490
count paragraph      19406
… all nineteen kinds …
```

`full-sha256` is a SHA-256 over the block section a full golden would have
carried, and `full-bytes` is its length — a few hundred bytes short of the
whole-file figures in the table above, which include the header and the census.
**The comparison is still exact
equality** — a one-pixel drift anywhere in 54,896 blocks still fails the build.
It just fails with a changed hash instead of a 16 MB diff.

`250kb.md` at 800 KB is the marginal case, and the measurement that decided it
is not the byte count but the census: `250kb.md`, `1mb.md` and `5mb.md` all
come from the same `prose()` generator in `xtask/src/corpus.rs` and exercise
the same six block kinds — heading, paragraph, bullet list, code fence, block
quote, table — every one of which `10kb.md`, `20-tables.md`,
`50-code-fences.md` and `rtl.md` already cover in a golden you can read. So the
800 KB buys 9,000 lines of duplicate diff and no new coverage.

The tool needs no other per-file knowledge: `DIGEST_INPUTS` in
`xtask/src/layout.rs` is a three-name list, and everything else — header,
census, precision, comparison — is identical in both forms.

---

## What is an input

Every `bench/corpus/*.md` **except `README.md`**, which is excluded
deliberately. `bench/corpus/` is a *generated* corpus —
`cargo xtask corpus --check` verifies that every committed file still matches
the generator — and its `README.md` is hand-written documentation *about* that
corpus. Laying it out would couple prose edits to golden churn: fixing a typo
in the provenance table would rewrite two goldens and put a layout diff in a
documentation commit, which erodes exactly the review ritual above.

`empty.md` is 0 bytes and is kept, because it is the edge case:
`mt_md::parse("")` returns a document with **one empty paragraph**, so the
golden is one 25.60px line box plus the container's 100px bottom padding, and
`height` is 125.60 rather than 0.

---

## Three gates run before any golden is written or compared

Each is a **silent** failure that produces a perfectly well-formed golden,
which is why none of them is a warning.

1. **Every face file is verified against its SHA-256 in `faces.toml`**, length
   first. A face that has drifted from its digest shapes different advances,
   and the golden it produces is indistinguishable from an ordinary layout
   change.
2. **An empty registration is an error.** `Collection::register_fonts` returns
   a `Vec`, not a `Result`, and MarkText's own `.woff` files come back empty
   with no error and no panic — D7's rule and M3-R10's reason.
3. **`assert_corpus_fully_covered`** resolves every distinct codepoint of every
   input, through **both** theme font stacks, and hard-fails listing the
   offenders if any reaches glyph id 0. A codepoint that produces *no* glyph is
   fine — that is the default-ignorable case (ZWJ, VS16) and a `cmap` check
   gets it wrong.

Gate 3 exists because `assets/fonts/NotoSansCJKsc-Regular-corpus-subset.otf` is
a `pyftsubset` of Noto Sans CJK **derived from `bench/corpus/` itself**, so it
stops covering the corpus the moment the corpus changes — and **a tofu advance
is a perfectly valid-looking golden.** It has a width and a position; the file
it produces is well-formed. Without the gate, a corpus edit quietly rewrites
the goldens full of `.notdef` boxes and the diff reads as geometry.
`assets/fonts/README.md` §6 has the contract and the regeneration command.

Today: **346 distinct codepoints across 11 inputs, 0 tofu.**

---

## What the goldens do *not* yet assert

Stated here so a green run is not read as more than it is.

- **No inline structure at all.** S2 owes it. Today each leaf is one style run
  of the block's literal text: `**bold**` is eight characters, no marker is
  hidden, no link or inline-code run exists, and no `InlineBox` is ever pushed
  — `inline-box` appears zero times in all twenty-two files. **Every
  text-bearing golden will move at S2, and that is the plan working.**
- **No syntax highlighting.** `lang=` is populated; the code palette is unread.
  S3.
- **Five of the nineteen block kinds appear in no golden**, because the corpus
  contains none of them: `setext-heading`, `html-block`, `frontmatter`,
  `diagram`, `footnote`. `cargo xtask layout` prints this on every run and
  `the_kinds_no_golden_exercises_are_exactly_the_five_the_corpus_lacks` fails
  in both directions, so a kind that gains its first golden gets read by eye
  before the list is edited. Note that `footnote` is absent because
  `Options::MUYA_DEFAULT` sets `footnote: false` — muya's own default — so
  `10kb.md`'s `[^why]:` line parses as a paragraph.
- **`overflow_x` is never non-zero** in any committed golden. The only corpus
  file with a block wide enough to overflow its column is `bench/corpus/README.md`,
  which is not an input.
- **Five of the twelve committed faces never appear**: both Open Sans italics
  and the three non-regular DejaVu Sans Mono faces. Nothing asks for italic or
  bold monospace until S2 gives inline runs their own styles.

---

## Files

Twenty-two goldens, **669,160 bytes**, all LF (`.gitattributes` is
`* text=auto eol=lf` for the whole repository).

| Input | `muya-default` | `dark` | Form |
|---|---:|---:|---|
| `100-inline-math.md` | 18,181 | 16,225 | full |
| `10kb.md` | 28,526 | 29,619 | full |
| `1mb.md` | 976 | 968 | digest |
| `20-tables.md` | 198,058 | 198,050 | full |
| `250kb.md` | 968 | 960 | digest |
| `50-code-fences.md` | 34,445 | 25,253 | full |
| `5mb.md` | 984 | 976 | digest |
| `cjk.md` | 20,876 | 21,001 | full |
| `emoji.md` | 22,495 | 22,487 | full |
| `empty.md` | 897 | 889 | full |
| `rtl.md` | 13,167 | 13,159 | full |
