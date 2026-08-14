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
layout-golden v3                       <- format version
theme          muya-default            ┐
theme-width    800.00                  │
content-width  700.00                  │
parley         a0752c7b…               │
faces          ce47b224… rev 1         ├ provenance: ten fields, one per line
options        wrap-code-blocks=false line-numbers=false
parse          muya-default  footnote=false math=true super-sub=false …
input          rtl.md                  │
input-bytes    1103                    │
form           full                    ┘

list           width=700.00 height=863.40 blocks=27
count paragraph      8                 ┐ the census: one line per BlockKind,
count atx-heading    4                 │ all nineteen, always, including the
count setext-heading 0                 ┘ zeroes

block 0 atx-heading bounds=[0.00 0.00 700.00 42.00] items=1
  glyphs font=OpenSans-Bold size=30.00 fill=#4d4d4dff rtl=0 text=0..34 origin=[0.00 32.00] advance=513.87 n=34 seq=6d58f8fc
block 1 paragraph bounds=[0.00 58.00 700.00 102.40] items=4
  glyphs font=OpenSans-Regular size=16.00 fill=#4d4d4dff rtl=0 text=0..69 origin=[0.00 76.00] advance=499.73 n=67 seq=771c52d4
```

**The blank line after the provenance is a seam with a job**: everything above
it describes how the golden was produced, everything below it is geometry. The
both-themes-must-differ check cuts there, and so should your eye.

### The header fields

| Field | What it pins |
|---|---|
| `layout-golden v3` | the format. Bumped when the serialization changes shape, so a file written by an older tool is identifiable from the artifact. **v2** added `parse`; **v3** added the glyph run's `fill=` |
| `theme` | which of the two shipped themes |
| `theme-width` | `[metrics] content_width_px` — the CSS `max-width`, **800 or 750** |
| `content-width` | that less `2 × container_padding_x_px` — **700 or 650**, the column text actually wraps in |
| `parley` | **the commit `Cargo.lock` resolved parley to**, not the one `Cargo.toml` asked for. D2 pinned it and says a pin move is a reviewable event; this line is what makes that an artifact rather than an instruction |
| `faces` | SHA-256 of `assets/fonts/faces.toml`, and its `[meta] revision` |
| `options` | `LayoutOptions`, both at muya's own defaults. A golden generated with either flag on is a **different artifact** |
| `parse` | `mt_md::Options`: a preset label, then every field spelled out. `muya-default` everywhere except `block-kinds.md`, which is `muya-default+footnote` — see below |
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
swap is a one-line diff at the top of all twenty-four files, exactly as a pin
move is.

`cargo xtask layout` additionally **fails** if `Cargo.lock`'s parley revision
and `faces.toml`'s `[meta] parley_rev` disagree: a face list measured against
one shaper and used with another is a coverage claim about a program that is
not running.

### One input is parsed with a non-default option, and the header says so

`Block::Footnote` **cannot occur in a default document.**
`Options::MUYA_DEFAULT` sets `footnote: false` because
`packages/muya/src/config` does, so `[^id]: …` parses as a paragraph — which is
exactly what `bench/corpus/README.md` already says happens to reference
definitions. The choice was therefore between never exercising the `footnote`
block kind in a golden and turning the extension on for something.

Turning it on **corpus-wide** was rejected twice over: it would rewrite every
golden here, and it would make the whole set describe a configuration MarkText
does not ship. So `cargo xtask layout` parses `bench/corpus/block-kinds.md` —
and only that file — with `footnote: true`, and both of its goldens open with

```text
parse          muya-default+footnote  footnote=true math=true …
```

while the other twenty-two say `muya-default  footnote=false …`. **The label is
in the artifact rather than only in the source** so that nobody reads that one
golden as the default configuration. Every other harness in `xtask` — `blocks`,
`diff`, `divergences`, `fuzz-seed` — reads the file at `MUYA_DEFAULT` like all
the others, so to them the `[^why]:` line is a paragraph.

The mechanism is a table (`PARSE_OVERRIDES` in `xtask/src/layout.rs`), not a
special case: one entry today, and every field of `mt_md::Options` is spelled
out on the line so a change to any of muya's defaults shows up as a one-line
diff in all twenty-four files rather than silently shifting the geometry
underneath them.

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
  glyphs font=<face file stem> size= fill=#rrggbbaa rtl=0|1 text=<start>..<end> origin=[x baseline] advance= n= seq=
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

A glyph run's fields are in two groups. `font size fill` are the run's
**resolved style**; `rtl text origin advance n seq` are where it landed and
what is in it. `fill` was added at **v3**, and its absence had been a hole in
the instrument rather than a tidiness: until inline styling existed every run
in a block carried the block's own colour, so the field said nothing. Now it
carries a link's `--link-color`, a `<strong>`'s theme override, inline code's
`--editor-color` and an unresolved emoji shortcode's `--delete-color`, and
**every one of those decisions was invisible to the golden** that was about to
freeze the display list. `rect` has printed `fill=` and `line` `stroke=` since
v1; this is the third painted item catching up.

`text=<start>..<end>` is the range in the leaf's **visible** text — markers
hidden — and it is the range of *that run's* glyphs, which is not what parley's
own `Run::text_range()` answers for a run split by style. See
`glyph_run_text_range` in `crates/mt-layout/src/text.rs`.

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
| `250kb.md` | 2,686 | 6,483 | **802,129 B** | **804,459 B** | 1,113 / 1,105 B |
| `1mb.md` | 10,744 | 26,173 | **3,242,062 B** | **3,265,949 B** | 1,121 / 1,113 B |
| `5mb.md` | 54,896 | 133,498 | **16,658,514 B** | **16,763,332 B** | 1,129 / 1,121 B |

Full goldens for those three would be **41.5 MB** across both themes, against
**6.7 KB** as digests, and `5mb.md`'s would be a single 16 MB, 190,000-line
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

`250kb.md` at 802 KB is the marginal case, and the measurement that decided it
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

`bench/corpus/block-kinds.md` was **added at M3 S1 for this directory's sake**
and is generated like everything else. Until it existed, five of the nineteen
`mt_doc::Block` variants appeared in no golden at all — `setext-heading`,
`html-block`, `frontmatter`, `diagram` and `footnote` — three of them being
exactly the kinds D11 decides, so the decision was implemented and never once
exercised by the artifact meant to verify it. See `corpus::block_kinds` for the
three constraints that shaped the file (ASCII only, front matter first, small
enough to read).

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

Today: **349 distinct codepoints across 12 inputs, 0 tofu.**

---

## M3-R6's by-eye review — which file to open, per block kind

> **First goldens per block kind reviewed explicitly at S1 and the review
> recorded.** — M3-R6

All nineteen kinds have somewhere to look. References are to the
`muya-default` file; the `dark` twin differs only in column width and palette,
so read one and diff the other.

| Block kind | Open | At |
|---|---|---|
| `paragraph` | `rtl.muya-default.txt` | `block 1` — four wrapped lines, one run each |
| `atx-heading` | `rtl.muya-default.txt` | `block 0` (h1: 30px in a 42.00px line box), `block 2` (h2: 24px) |
| `setext-heading` | `block-kinds.muya-default.txt` | `block 1` (h1) and `block 3` (h2) — same geometry as the ATX forms, which is the thing to check |
| `thematic-break` | `10kb.muya-default.txt` | `block 65` — the only `line` item in the corpus. `style=dashed`, at the box centre **minus 1.00** |
| `code-block` | `50-code-fences.muya-default.txt` | `block 3` (`lang=rust`), `block 35` (bare fence, **no** `lang=`) |
| `html-block` | `block-kinds.muya-default.txt` | `block 5` — `lang=html`, and the only `overflow=` in the directory (15.69px: an 80-column line in a 669.2px code column that scrolls rather than wraps) |
| `math-block` | `100-inline-math.muya-default.txt` | `block 2` — `lang=latex`, DejaVu Sans Mono at 14.40px |
| `frontmatter` | `block-kinds.muya-default.txt` | `block 0` — `lang=yaml`, at the very top of the document |
| `diagram` | `block-kinds.muya-default.txt` | `block 9`–`block 13` — all five kinds in order: `mermaid`, `plantuml`, `vega-lite`, `flowchart`, `sequence` |
| `table` / `table.row` / `table.cell` | `20-tables.muya-default.txt` | `block 3` / `4` / `5`; `cjk.muya-default.txt` `block 21` for narrow max-content columns |
| `block-quote` | `rtl.muya-default.txt` | `block 13` — the 2px bar at inset 15, starting at the child paragraph's top edge (margin collapse-through, visible) |
| `bullet-list` / `list-item` | `rtl.muya-default.txt` | `block 4`–`block 9` — 5.60px disc at `radius=2.80` |
| `order-list` | `10kb.muya-default.txt` | `block 12`–`block 13` — the marker run at `origin=[8.65 …]`, placed by its **trailing** edge |
| `task-list` / `task-list-item` | `10kb.muya-default.txt` | `block 37`–`block 42` — ring 18px `radius=9.00`, box 14px `radius=7.00`, two `rot=-45.00` rects for the tick. `block 42` is the unchecked one |
| `footnote` | `block-kinds.muya-default.txt` | `block 16` — tint at `#f7f7f7cc` (alpha 204 = the theme's 0.8 opacity), the `[^why]` label in DejaVu Sans Mono **Bold** at 14px absolute, and `block 17`'s nested paragraph at **12.80px** where the 0.8em compounds |

The four D11 kinds that share the code-block box — `code-block`, `html-block`,
`math-block`, `frontmatter`, plus `diagram` — are worth reading together: all
five carry the same five rects at `muya-default` (a tint and four 1px border
edges) and **one** rect at `dark`, which is the C1 finding that thirty of the
thirty-two shipped themes zero the code-block border. It moves the glyph
origin too, 15.40 → 14.40.

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
- **Four of the twelve committed faces never appear**: both Open Sans italics
  and the two DejaVu Sans Mono obliques. Nothing asks for italic text or
  oblique monospace until S2 gives inline runs their own styles.
  (`DejaVuSansMono-Bold` appears exactly twice — it is the footnote label,
  which the theme sets to weight 600 in the bare `monospace` generic.)

All nineteen block kinds **are** exercised, and
`every_block_kind_is_exercised_by_at_least_one_golden` keeps them that way. The
assertion is two-way: a kind that stops being exercised is a corpus regression,
and a kind that starts being exercised is a golden nobody has read yet. Either
way, open the golden before editing the list.

---

## Files

Twenty-four goldens, **687,653 bytes**, all LF (`.gitattributes` is
`* text=auto eol=lf` for the whole repository).

| Input | `muya-default` | `dark` | Form |
|---|---:|---:|---|
| `100-inline-math.md` | 18,326 | 16,370 | full |
| `10kb.md` | 28,671 | 29,764 | full |
| `1mb.md` | 1,121 | 1,113 | digest |
| `20-tables.md` | 198,203 | 198,195 | full |
| `250kb.md` | 1,113 | 1,105 | digest |
| `50-code-fences.md` | 34,590 | 25,398 | full |
| `5mb.md` | 1,129 | 1,121 | digest |
| `block-kinds.md` | 8,330 | 6,973 | full |
| `cjk.md` | 21,021 | 21,146 | full |
| `emoji.md` | 22,640 | 22,632 | full |
| `empty.md` | 1,042 | 1,034 | full |
| `rtl.md` | 13,312 | 13,304 | full |
