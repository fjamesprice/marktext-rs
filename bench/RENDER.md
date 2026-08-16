# RENDER.md — `vello_cpu` scroll frames, measured

The frame rate `docs/M3.md` **D1** has been deferred against since M3 S1, and
which D1 says *"S4 measures … with `vello_cpu` as its first act"* and *"S4 may
not report a backend as chosen without that number."* Produced in M3 S4 by
`cargo xtask frames` (`xtask/src/frames.rs`) against the working tree at
`cc4001b`, 2026-08-16.

Companion to `bench/BASELINE.md`, which measures the Electron original. Same
machine, same reporting convention, same rule about honestly missing rows.

## 1. The number, up front

**3.583 ms**, median of **88 first-visit frames** — 11 scroll offsets × 8
separate process launches — over `bench/corpus/5mb.md` at 1200 × 800, release
profile, `vello_cpu` with `num_threads = 0`.

**It clears the 16.67 ms budget, and it is not close.** The median frame uses
**21.5 %** of a 60 fps budget (≈ 279 fps equivalent). The **single worst frame
seen anywhere in the primary set is 8.241 ms** — 49 % of the budget, ≈ 121 fps
— and that one is the top-of-document frame in the launch that landed on the
slowest cores this CPU has (§6). No frame in any 1200 × 800 configuration
measured here exceeded 8.241 ms.

This is D17's **pessimistic** definition: a full-viewport repaint, no dirty
rect, no scroll blit, no glyph atlas beyond the one `vello_cpu` keeps for
itself, single-threaded. Everything S6 adds subtracts from it.

| | |
|---|---|
| Frame, median (n=88, 8 launches) | **3.583 ms** |
| Frame, full range | **1.330 – 8.241 ms** |
| 60 fps budget | 16.67 ms |
| Verdict | **clears, by 4.7× at the median and 2.0× at the worst frame** |

## 2. Everything D17 requires stated beside the frame time

| Row | Value |
|---|---|
| **Frame time** | 3.583 ms median, 1.330–8.241 ms, n = 88 frames from **8 separate process launches** |
| **Viewport** | **1200 × 800 px** — MarkText's own default editor-window *content* size (§4) |
| **Scroll offsets sampled** | **11**: eighths of the scrollable range (0 → 2 776 542.75 px), plus the two densest viewport-height windows in the document, found by search (§4) |
| **Blocks intersecting the viewport** | **9 to 34** of 54 896, per offset — see §3 |
| **Cull cost, separated** | **151.8 µs** median for all 54 896 bounds tests — **4.2 % of the median frame** (§3.3) |
| **Rasterization floor** | **0.492 ms** median for a frame with 0 blocks in view (cull + background fill + `render_with`) |
| **Layout time, excluded** | **11 585 ms** median (6 700 ms pinned to a P-core). Parse 88.6 ms, highlight spans 55.0 ms. Not in any frame time above. See §5 for why this deserves someone's attention and why it is not this file's row |
| **`num_threads`** | **0**. `multithreading` is not a default feature of `vello_cpu` 0.2.0 and is not enabled. No threaded second number is reported — see §5 |
| **Hardware** | Intel Core i5-13600K (14C/20T: **6 P-cores + 8 E-cores**), 65 320 MB RAM (63.8 GB), Samsung SSD 990 PRO 2TB among the disks present, Windows 11 Pro 10.0.26200 — the same physical box `bench/BASELINE.md` §4 and the E3 spike used, independently re-confirmed for this file |
| **Profile** | **release** (`opt-level = 3`, `lto = "fat"`, `codegen-units = 1`). The command refuses to run in a debug profile without `--allow-debug` |
| **Backend** | `vello_cpu` 0.2.0, `RenderMode::OptimizeSpeed`, parley pinned at `a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e` |

## 3. Per-offset detail

`5mb.md` lays out to **54 896 blocks**, **252 216 display items**, 650.00 px
wide × **2 777 342.75 px** tall (`bench/layout-goldens/5mb.dark.txt` header).
The theme is `dark`, parsed at `muya-default`.

### 3.1 Frames, pass 1 — the primary

Each offset seen for the first time in its process. n = 8 per row.

| # | scroll y | blocks | items | glyphs | painted px | median | min | max |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 0.00 | 14 | 73 | 1 244 | 116 525 | 6.584 ms | 3.505 | 8.241 |
| 1 | 347 067.84 | 13 | 73 | 1 322 | 170 627 | 3.764 | 2.051 | 5.352 |
| 2 | 360 000.00 † | **34** | 98 | 784 | 38 122 | 2.311 | 1.330 | 4.119 |
| 3 | 694 135.69 | 21 | 72 | 1 646 | 74 051 | 4.374 | 2.333 | 7.048 |
| 4 | 1 041 203.50 | 11 | 82 | 2 265 | 77 816 | 4.545 | 2.463 | 7.011 |
| 5 | 1 388 271.38 | 15 | 88 | 1 431 | 121 255 | 3.339 | 1.891 | 4.500 |
| 6 | 1 735 339.25 | 18 | 68 | 1 439 | 125 804 | 3.452 | 1.976 | 5.527 |
| 7 | 1 920 800.00 ‡ | 9 | 90 | **2 556** | 80 765 | 4.584 | 2.553 | 6.715 |
| 8 | 2 082 407.00 | 15 | 73 | 1 411 | 122 899 | 3.455 | 1.877 | 4.114 |
| 9 | 2 429 475.00 | 23 | 82 | 1 626 | 55 369 | 3.274 | 1.794 | 4.769 |
| 10 | 2 776 542.75 | 15 | 81 | 1 131 | 106 725 | 2.581 | 1.459 | 3.513 |
| | | | | | **all** | **3.583** | **1.330** | **8.241** |

† the densest window in the document by **block** count.
‡ the densest by **glyph** count.

### 3.2 Frames, pass 2 — the same offsets revisited, reported and not the headline

`VelloCpuRenderer` keeps a glyph cache in its `Resources`, so a second visit to
an offset is cheaper. This is what a scroll *back* costs, and it is the number a
warm-cache harness would have quoted as the frame time.

| # | median | min | max |
|---:|---:|---:|---:|
| 0 | 3.103 ms | 1.727 | 4.455 |
| 1 | 3.343 | 1.771 | 4.544 |
| 2 | 2.076 | 1.158 | 2.774 |
| 3 | 4.051 | 2.171 | 5.122 |
| 4 | 4.240 | 2.413 | 5.956 |
| 5 | 3.291 | 1.816 | 4.346 |
| 6 | 3.510 | 1.915 | 4.405 |
| 7 | 4.518 | 2.417 | 5.296 |
| 8 | 3.254 | 1.824 | 4.348 |
| 9 | 3.174 | 1.756 | 4.663 |
| 10 | 2.579 | 1.418 | 4.252 |
| **all** | **3.319** | **1.158** | **5.956** |

The cache is worth **7 %** at the median and everything at offset 0, where pass
1's 6.584 ms falls to 3.103 ms — the first frame in a process pays for every
face and size the document opens with. Pass 1 is the primary precisely because
that first frame is a real frame a real reader sees.

### 3.3 D18's cull, priced

D18 chose a **linear** scan over all 54 896 blocks per frame, as a finding
rather than a default, and said the S4 measurement would be the instrument that
priced it: *"54 896 bounds tests per frame is either invisible against
rasterization or it is the whole frame."*

**It is neither. It is 4.2 % of a frame.** Measured as the same
`mt_render::is_visible_in` loop over the same `list.blocks`, outside the
renderer, with the hit count taken through `black_box`:

| | median | min | max |
|---|---:|---:|---:|
| Cull, 54 896 bounds tests | **151.8 µs** | 69.4 µs | 665.5 µs |
| … as a share of the 3.583 ms median frame | **4.2 %** | | |
| … pinned to a P-core (§6) | 72.0 µs | 67.5 | 124.7 |
| … pinned to an E-core (§6) | 135.8 µs | 114.7 | 218.8 |

That is **1.3–2.8 ns per bounds test**. The scan does not need an index today.

**But it is the fixed cost, and S6 is where it starts to matter.** The whole
frame decomposes as

```
3.583 ms  median frame
  0.152     D18's cull over all 54 896 blocks          4.2 %
  0.340     rasterization floor  (0.492 ms floor frame − the cull)   9.5 %
  3.091     per-block encode + paint of the 9–34 blocks in view     86.3 %
```

S6's dirty rect and scroll blit attack the 86 %. **The cull is the one term
they cannot reduce** — a one-line dirty rect still scans all 54 896 blocks — so
a frame that S6 makes 5× cheaper would be ~30 % cull, and a frame 10× cheaper
would be ~45 % cull. D18's linear scan is correct for S4 and is the first thing
to re-price at S6, which is a different statement from the one an intuition
would have made in either direction.

### 3.4 The floor: a frame with nothing in it

An offset one viewport past the bottom of the document, so the cull runs over
all 54 896 blocks and rejects every one. What remains is the background fill and
`render_with` over a scene holding one rectangle.

| viewport | floor, median | range |
|---|---:|---:|
| 1200 × 800 | 0.492 ms | 0.280–1.033 |
| 1920 × 1080 | 1.045 ms | 0.947–1.144 |
| 3840 × 2160 | 4.279 ms | 3.916–4.680 |

0.49 → 1.05 → 4.28 ms against 0.96 → 2.07 → 8.29 Mpx is very nearly linear in
pixel count, which is what a single-threaded software rasterizer clearing and
compositing a full-viewport buffer should look like and is the reason §4 gives
for stating the viewport size as loudly as the frame time.

### 3.5 Viewport-size contrast — clearly labelled, not the primary

D17 fixes the frame; it does not fix the window. A maximized window is a real
configuration, so the same run at two larger viewports, **n = 3 launches each**:

| viewport | Mpx | pass 1 median | range | pass 2 median | clears 16.67 ms? |
|---|---:|---:|---:|---:|---|
| **1200 × 800** (primary) | 0.96 | **3.583 ms** | 1.330–8.241 | 3.319 | **yes** |
| 1920 × 1080 | 2.07 | 5.715 ms | 3.682–10.179 | 5.343 | yes |
| 3840 × 2160 | 8.29 | 13.109 ms | **9.898–29.832** | 12.228 | **median yes, worst frame no** |
| 3840 × 2160, pinned to P-cores | 8.29 | 6.616 ms | 5.392–13.889 | 6.237 | yes |

**The 4K row is the one a reader should not skip.** Unpinned, its worst frame is
29.8 ms — 1.8× over budget — and its median is inside budget with only 21 %
headroom. Pinned to a P-core the same configuration halves to a 6.6 ms median
and a 13.9 ms worst frame, so the 4K miss is core placement (§6) and not the
rasterizer running out of road. Still: **a full-viewport software repaint of a
4K window is within a small factor of the budget, and it is the configuration
where S6's dirty rect stops being headroom and starts being load-bearing.**
This is not a D1 finding — the backend is the same either way — but it is the
row a D1 re-opening argument would reach for, so it is stated rather than left
to be discovered.

## 4. Method

**Hardware.** Intel Core i5-13600K (14C/20T — **6 performance cores + 8
efficiency cores**, 20 logical processors), 65 320 MB RAM (63.8 GB), disks
present include a Samsung SSD 990 PRO 2TB, Windows 11 Pro 10.0.26200. Every
field re-read off this machine for this file rather than copied from
`bench/BASELINE.md` §4; they agree. The P/E split is called out here because §6
shows it is the largest single source of spread in every number in this file,
and `bench/BASELINE.md` §4's sentence does not mention it.

**Profile.** `release` — `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`,
`panic = "abort"` (root `Cargo.toml`). `cargo xtask frames` prints the profile
in its header and **refuses to run under `debug_assertions`** without an
explicit `--allow-debug`, which is one step stronger than
`crates/mt-inline/benches/tokenizer.rs`'s convention of merely labelling it.
That file's reason applies unchanged: a debug figure and a release figure differ
by more than an order of magnitude and a table without the label is worse than
no table.

**What one frame is.** `mt_render::Renderer::render(&list, &frame, &mut
Pixels)` — D18's cull over every block, the per-item encode into the
`RenderContext`, and `vello_cpu`'s `render_with` into a viewport-sized pixmap.
The `DisplayList` is laid out **once per process** and reused for every frame in
that process; the renderer, its context, its `Resources` glyph cache and D16's
font table are built once and reused.

*D17 names `RenderContext::render_with` specifically, and that clause had to be
read rather than transcribed.* `render_with` is `vello_cpu`'s rasterize call: it
takes a `PixmapMut` and no display list, so timing it alone would time a step in
which `5mb.md`'s display list does not appear, and D17's own qualifier — *"from
a `DisplayList` laid out once and reused"* — would have nothing to attach to.
The unit that turns a display list into pixels is `Renderer::render`, and that
is what is timed. It is the larger of the two candidate readings, which is the
direction D17 says to err in. **The consequence is that the encode and the
rasterize are not separated in this file**, because they are not separable from
outside `mt-render` and putting a stopwatch inside the shipped renderer would
change the thing being measured. §3.3's decomposition separates the cull and the
fixed rasterization floor, which is what D18 actually needed priced.

**Viewport — 1200 × 800 px, and it is read off the reference implementation.**
`packages/desktop/src/main/windows/editor.ts:96-98` in the sibling MarkText
clone creates every editor window through `windowStateKeeper({ defaultWidth:
1200, defaultHeight: 800 })`, and `editorWinOptions` sets `useContentSize: true`
(`packages/desktop/src/main/config.ts:22`), so 1200 × 800 is the **content**
area of a first-run MarkText window rather than the outer frame. Same spirit as
`bench/BASELINE.md` §4 taking its readiness criterion from the project's own e2e
helper: a code-derived definition rather than an invented one. It is also the
pessimistic choice among the honest ones — the real editor pane is smaller than
the window content area, because a sidebar and a tab bar sit inside it, so this
rasterizes strictly more pixels per frame than the editor ever will. §3.5
reports two larger viewports as contrast.

The viewport's **x is 0**, so the 650 px content column sits at the left and the
remaining 550 px is background. A shell would centre the column; that is a
translate, it changes no block's visibility (the column is inside the viewport
either way) and it changes no painted pixel count.

**Scroll offsets — 11, and two of them were searched for rather than spaced.**
Nine are eighths of the scrollable range `list.height − viewport height`, so the
last is the true bottom of the document rather than a viewport hanging off the
end. **An evenly spaced sweep is a sample of a document, not a bound on it**, so
the whole file is additionally swept in 3 471 non-overlapping viewport-height
windows and the two densest are added to the sampled set: the window holding the
most **glyphs** (the unit the rasterizer works in) and the one holding the most
**blocks** (the unit the cull counts). They then go through exactly the same
machinery and appear on the same rows — a worst case measured by a special path
is a worst case nobody can compare with the ordinary one.

*The first version of that search maximized `DisplayItem` count and found a
viewport that rendered faster than average.* One `DisplayItem::Glyphs` is one
item whether it holds three glyphs or three hundred, so an item count measures
dispatches and the cost is glyphs; the search was corrected before the numbers
in this file were taken. Offset 7 in §3.1 — 9 blocks, 90 items, **2 556 glyphs**
— is 4.584 ms, and offset 2 — **34 blocks**, 98 items, 784 glyphs — is 2.311 ms.
Glyphs predict the frame; blocks and items do not.

**N = 8 separate process launches**, matching `bench/BASELINE.md` §4's
**cold-start** convention, which is the largest of its three (cold 8 / warm 5 /
per-file 3). It gets the largest N because unlike BASELINE's per-file row there
was no time budget forcing a reduction: a launch is ≈ 7–13 s, so eight is about
90 seconds. Every statistic is **between-launch**, never a best-of-M loop inside
one process — same rule, same reason. Each of the eleven offsets is rendered
once per pass per launch, so a per-offset row is n = 8 and the pooled row is
n = 88. The contrast configurations in §3.5 and §6 use **n = 3**, matching
BASELINE's per-file convention.

**Every table cell is a median plus the full min–max range.** No number here is
a single run except where a table says n = 1.

**The census — proof the timed frames were not blank.** Nine to thirty-four
blocks of 54 896 is a small enough number to deserve an independent check, and
*"`items_drawn` was 73"* is a counter the renderer increments, not a pixel. So
after all timing, every offset is re-rendered **outside every timer** and the
pixels differing from the ground colour are counted off the buffer; the ground
colour is read from the floor frame, which is entirely background, rather than
converted from the theme by hand. Every offset paints **38 122 to 170 627** of
960 000 pixels. Cross-check: ~1 300 glyphs at roughly 50 painted pixels each is
~65 000, and the counts land where they should. The command **errors out** if
any offset paints nothing.

**What is excluded from the frame time**, all of it timed and reported
separately: `mt_md::parse`, `mt_highlight`'s span computation, `layout_with`,
the font collection build, the `FontTable` build, D16's drift assertion, the
densest-window search, and the census. The frame timer opens immediately before
`Renderer::render` and closes immediately after it.

## 5. What this file does not measure

- **A threaded number — deliberately not reported, and not because it was
  hard.** D17 asks for `num_threads` beside the frame time and it is 0.
  Enabling `vello_cpu`'s `multithreading` feature is *not* a one-line change
  that leaves the measurement comparable: it adds a non-default feature of an
  eight-week-old 0.x crate to the workspace dependency graph, changes the
  lockfile, changes `dist` size — which is **the exact lever D1 was decided
  on** — and requires editing `mt_render::NUM_THREADS`, a constant in the
  shipped renderer. Turning it on is a decision with D1's own shape, not a
  second row in a measurement. The primary already clears by 4.7×, so a
  threaded figure could only widen a margin that is not in doubt. Left
  unmeasured rather than guessed, per `bench/BASELINE.md` §5's rule.
- **`RenderContext::render_with` in isolation, split from the encode.** Not
  separable from outside `mt-render` (§4). §3.3 separates what D18 needed.
- **GPU (`vello`/`wgpu`).** No second backend exists. M3 §6's S4 gate clause
  *"if two backends ship, the same display list through both agrees within that
  threshold"* therefore **has no content and is reported as having none rather
  than ticked**, exactly as `crates/mt-render/src/lib.rs` already states.
- **macOS / Linux.** Windows x64 only, matching `bench/BASELINE.md`'s scope and
  `docs/M3.md` §12.1's.
- **Anything S6 will add.** No dirty rect, no scroll blit, no partial
  invalidation, no glyph atlas beyond `vello_cpu`'s own. That is D17's design,
  not a gap: all three only make the number smaller.
- **A second theme.** Every number here is `dark`. The block census is
  identical at both themes (`5mb.muya-default.txt` matches `5mb.dark.txt` line
  for line), so the theme selects a palette rather than a geometry, but *"we
  expect it not to matter"* is not a measurement and the light-theme row is
  simply absent rather than asserted.
- **An idle-CPU or RSS figure** for the renderer. Not a D17 row; `mt-render`
  has no event source and cannot start a loop.
- **The layout time's own story.** `layout_with` over `5mb.md` measures
  **11 585 ms** median unpinned, **6 700 ms** pinned to a P-core (range
  6 662–6 710). It is excluded from every frame by D17 and it does not touch
  D1's verdict, but it is **≈ 8× the 813 ms** the E3 spike measured for
  `parley::Layout` construction over the same file
  (`spikes/results/e3-layout-scale.md` §7, same parley pin, same machine). The
  two are **not the same quantity** — E3 built parley layouts and stopped;
  `layout_with` plans the block tree, builds every leaf with themed style runs
  and D15's highlight spans applied, places it, and emits **252 216 display
  items** across 54 896 blocks — so this file states the gap and does not
  explain it. D9 owns that question and this row exists so that whoever asks it
  next has the number.

## 6. Why every spread in this file is bimodal: the P/E-core finding

**The largest source of variance in these measurements is not the renderer. It
is which class of core Windows put the process on.** This box is an i5-13600K:
6 performance cores (logical 0–11) and 8 efficiency cores (logical 12–19). The
eight primary launches split cleanly into two regimes:

| launch | layout | pass-1 frame median | worst frame |
|---|---:|---:|---:|
| 1 | 6 843 ms | 1.976 ms | 3.505 |
| 2 | 6 706 ms | 5.352 ms | 8.241 |
| 3 | 11 749 ms | 3.406 ms | 6.254 |
| 4 | 11 390 ms | 3.730 ms | 6.396 |
| 5 | 12 070 ms | 3.434 ms | 7.574 |
| 6 | 12 590 ms | 3.636 ms | 6.627 |
| 7 | 11 421 ms | 3.702 ms | 7.000 |
| 8 | 12 214 ms | 3.495 ms | 6.541 |

Launch 2 is the row that gives the mechanism away: a **fast** layout (6 706 ms,
the P-core figure) followed by the **slowest** frames in the whole set (5.352 ms
median, 8.241 ms worst). Nothing about the display list changed between the two
halves of that process — the scheduler moved it.

Confirmed directly by pinning with `start /affinity`, n = 3 each:

| | layout | frame, pass 1 | range | cull | floor |
|---|---:|---:|---:|---:|---:|
| **P-cores** (mask `FFF`) | 6 700 ms | **1.947 ms** | 1.373–3.814 | 72.0 µs | 0.259 ms |
| **E-cores** (mask `FF000`) | 15 853 ms | **3.433 ms** | 2.279–6.663 | 135.8 µs | 0.438 ms |
| ratio | 2.37× | **1.76×** | | 1.89× | 1.69× |
| *unpinned, N=8 (the primary)* | *11 585 ms* | *3.583 ms* | *1.330–8.241* | *151.8 µs* | *0.492 ms* |

Within a pinned configuration the spread nearly vanishes — offset 0 on P-cores
is 3.727 / 3.765 / 3.814 ms across three launches. **So the 1.330–8.241 ms
primary range is mostly a scheduler range, not a renderer range.**

**The primary number is the unpinned one anyway, and that is deliberate.** A
reader's editor runs wherever Windows puts it, on a desktop with other
applications on it, which is what the primary measures. A 6 s CPU sample taken
across every process on the box **immediately after** the primary launches — by
the method `bench/BASELINE.md` §4 uses for idle CPU — found **13.64 CPU-seconds
of background work, ≈ 11.4 % of 20 logical processors (2.3 cores)**, the bulk of
it Explorer (0.9 cores), three Firefox processes and two Discord processes. It
is a sample *after* rather than *during*, which is a real weakness of the
attribution and is why §6 rests on the pinned A/B rather than on it. Pinning to
P-cores would have reported
1.947 ms, and reporting *that* as the frame time would be choosing the
flattering configuration. The E-core row is the useful bound: **on the slowest
cores this machine has, with no contention at all, the median frame is 3.433 ms
and the worst is 6.663 ms — still 2.5× inside budget.**

The one place this changes an answer rather than a spread is the 4K row in §3.5.

## 7. Reproducing

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Build release. `cargo xtask` is `cargo run --package xtask`, i.e. the DEV
# profile — the subcommand refuses to run under it, but build explicitly anyway.
cargo build --release --package xtask

# One launch prints one `layout`, one `dense`, 11 `cull`, 11 `frame` per pass,
# one `floor` and 11 `census` lines, all whitespace-delimited key=value.
.\target\release\xtask.exe frames

# The primary: 8 separate launches, aggregated outside the process.
1..8 | ForEach-Object { .\target\release\xtask.exe frames > "runs\primary-$_.txt" }

# The P/E diagnostic of §6 (mask FFF = the 12 P-core threads, FF000 = the 8 E-cores).
cmd /c 'start /affinity FFF   /wait /b "" target\release\xtask.exe frames > runs\pcore-1.txt'
cmd /c 'start /affinity FF000 /wait /b "" target\release\xtask.exe frames > runs\ecore-1.txt'

# The viewport contrast of §3.5.
.\target\release\xtask.exe frames --viewport 3840x2160
```

The aggregator that turned those files into §3's tables — a median/min/max over
the pooled `frame`, `cull` and `floor` lines, grouped by `pass` and `offset` —
lived in this session's scratch directory and is not committed, exactly as
`bench/BASELINE.md` §7's Playwright harness is not. It is four lines of regex
over `key=value` pairs, which is the whole reason the output format is
`key=value` pairs.

`cargo xtask frames` is **not** part of `cargo xtask ci`. It is a measurement,
not a gate: it takes 90 s for eight launches, it asserts nothing about
correctness that the layout goldens and the S4 unit tests do not already assert,
and a number that fails CI when the machine is busy is a number nobody will
trust.
