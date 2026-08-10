# E3 — layout at scale (M3 S0, decision D9)

## The answer

**Is eager layout of 5 MB comfortably inside 800 ms? No — it is not inside
800 ms at all.**

Building one `parley::Layout` per block over `bench/corpus/5mb.md` takes a
**median of 813 ms** (range 805–829 ms over eight process launches, best-of-3
within each; an earlier sweep of eight gave 796–829 ms) on an Intel i5-13600K. `mt_md::parse` adds **81–84 ms** before
layout can start, so file-open end to end is **≈ 896 ms — 12 % over the gate**,
on desktop hardware considerably faster than the median machine MarkText runs
on.

**D9 therefore resolves to its second branch: laziness is load-bearing, and
S1's data structures have to anticipate it.** It is not an optimization that can
be given a stage of its own later.

With one caveat that changes the shape of the decision rather than the answer,
and which D9 does not currently consider: **the work is embarrassingly parallel,
and parallel eager layout meets the gate.** 20 threads bring layout to
**334 ms**, so parse + layout lands at **≈ 418 ms** with no laziness anywhere.
See "The third option" below.

---

## Hardware, and why it has to be stated

The 800 ms gate is an absolute target on unstated hardware, so:

| | |
| --- | --- |
| CPU | Intel Core i5-13600K — 14 cores / 20 threads (6 P-cores + 8 E-cores), 3.5 GHz base |
| RAM | 63.8 GB |
| Disk | Samsung SSD 990 PRO 2TB, NVMe |
| OS | Windows 11 Pro 10.0.26200 |
| Toolchain | rustc 1.97.1, `x86_64-pc-windows-msvc`, **`--release`** |
| File cache | **Warm.** Every input was read repeatedly before and during measurement; no number here includes a cold disk read. |

This is a fast desktop CPU released in 2022. A laptop, a CI runner, or a
five-year-old machine will be slower — so *"12 % over on this box"* is the
optimistic reading of the gate, not the pessimistic one.

**A note on the P-core/E-core split.** Single-threaded runs vary 3–4 %
between process launches (795.6 – 829.2 ms across sixteen) depending on which core class
Windows schedules them onto. Every headline number below is best-of-N *within* a
process, reported alongside the between-launch distribution, because the
within-process figure alone would understate the real variance.

## Method

### Driving the real parser

The spike path-depends on `mt-md` and `mt-doc`, the way `fuzz/` does, rather
than re-implementing a markdown parser and measuring the spike's parser instead
of the project's. The exact call is:

```rust
let parsed = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
```

Blocks come from an iterative pre-order walk of `parsed.document`, taking every
node for which `Block::text()` is `Some` — i.e. every leaf. The texts are
materialised into `String`s **up front and untimed**, so the timed loop measures
parley and not `Text::to_str`'s rope traversal. That collection step costs 4.0 ms
for `5mb.md` and is reported separately rather than hidden.

### What is measured, and what is not

One `Layout` per leaf block, from the block's **plain text as a single style
run**. This does *not* run `mt-inline` to produce per-token style runs and does
not push `InlineBox`es for images or math. Every number here is therefore a
**lower bound** on real inline layout.

That gap is bounded rather than left hanging: `eager --styled` synthesises a bold
run every twelfth word, which costs **886 ms vs 805 ms**, i.e. real inline layout
should be expected to run **~10–13 % above** these figures. The conclusion does
not turn on it — it makes the gate miss worse, not better.

### Fonts

Deterministic and explicitly registered, using the no-I/O path E1 proved:

```rust
Collection::new(CollectionOptions { shared: false, system_fonts: false })
// + register_fonts(Blob<u8>, None) for each face
```

The faces are MarkText's own four bundled **DejaVu Sans Mono** `.ttf` files
(`packages/muya/src/assets/styles/fonts/`). No filesystem enumeration, so the
numbers reproduce on a machine with a different font set installed.

Running instead with system enumeration (`FontContext::new()`) gives 882 ms
against 805 ms — but that arm resolves a *different face*, because DejaVu Sans
Mono is not installed on this machine, so it is a font-versus-font comparison,
not an enumeration cost. The honest statement is that font strategy moves the
total by roughly 10 % and changes no conclusion.

### Peak RSS, and why every mode is its own process

Peak working set is a process-wide monotonic counter: once a phase has touched
900 MB, every later reading in that process reports 900 MB regardless of what
was freed. So each scenario is a subcommand run in **its own process**, and the
shell is the driver. Nothing spawns children.

The reading uses `K32GetProcessMemoryInfo` — `GetProcessMemoryInfo` as exported
from kernel32, which every Rust binary already links — declared as a nine-line
`unsafe extern "system"` block rather than pulling in `sysinfo` (~20 transitive
crates) or `windows-sys` (a large generated binding set) for one struct field.

### Reproducing

```
cd spikes
cargo build --release -p e3-layout-main
cargo build --release -p e3-layout-0-11
E=./target/release/e3-layout-main.exe

# 1. parse/layout split
$E parse   ../bench/corpus/5mb.md 5
$E eager   ../bench/corpus/5mb.md 800 3
$E phases  ../bench/corpus/5mb.md 800 3

# 2. RSS targets (reps=1 so the peak is one generation's)
$E eager   ../bench/corpus/1mb.md 800 1
$E eager   ../bench/corpus/250kb.md 800 3

# 3. C5's re-break claim, at both width pairs that matter
$E rebreak ../bench/corpus/5mb.md 800 750  5
$E rebreak ../bench/corpus/5mb.md 800 1600 5

# 4. the Code variant. The dense input is GENERATED, not committed —
#    bench/corpus/ has a --check generator and CI fails on a file it did not write.
$E gen-code-dense .gen/code-dense.md 2000 60     # seed 0xE3C0DE, 6144687 bytes
$E code .gen/code-dense.md per-fence 3
$E code .gen/code-dense.md per-line  3
$E layout-overhead 200000

# 5. the option D9 does not consider
for t in 1 2 4 6 8 12 14 20; do $E parallel ../bench/corpus/5mb.md 800 $t 3; done

# 6. the 0.11.0 spot-check
./target/release/e3-layout-0-11.exe eager ../bench/corpus/5mb.md 800 3
```

`.gen/` is gitignored for the same reason `fuzz/corpus/` is: it is derived, the
generator and seed are recorded, and committing 6 MB to reproduce a number that
a one-line command reproduces would be storing an artifact instead of a recipe.

---

## 1. Parse versus layout

| | best | share of the 800 ms gate |
| --- | ---: | ---: |
| `mt_md::parse` | **80.9 ms** (5 reps, spread 17.9 %) | 10 % |
| `collect_texts` (rope → `String`) | 4.0 ms | 0.5 % |
| **layout, one `Layout` per block** | **805 ms** best / **813 ms** median | **102 %** |
| end to end | **≈ 896 ms** median | **112 %** |

Layout gets essentially the whole budget and overruns it. Parse is not the
problem and was never going to be — **layout is 90 % of file-open**.

Structure of `5mb.md`:

```
arena nodes 54897, leaf blocks with text 39308, leaf text 5080697 bytes (96.9 % of file)
  paragraph          19406 blocks     4824395 bytes
  atx-heading         9812 blocks      146073 bytes
  table.cell          8136 blocks       46164 bytes
  code-block          1954 blocks       64065 bytes
```

## 2. Eager layout — the distribution

Over 39 308 blocks, 92 758 lines broken:

| | value |
| --- | ---: |
| mean | 22 055 ns |
| median | 10 000 ns |
| p99 | 103 400 ns |
| max | 516 100 ns (block #4576, `paragraph`, 484 bytes) |
| throughput | 6.5 MB/s of file, 48 828 blocks/s |

The mean is **2.2× the median**, exactly as D9's framing anticipated — a long
tail. But the tail is not where the time is: p99 × 1 % of blocks accounts for
about 41 ms of the 805. **The cost is the bulk, not the tail**, which is
important because it means viewport-limited laziness (lay out what is visible)
wins proportionally, with no pathological block to trip over.

### Where the time goes inside parley

| phase | time | share | per block |
| --- | ---: | ---: | ---: |
| `ranged_builder` + `push_default` | 11.9 ms | 1.5 % | 302 ns |
| **`builder.build` — shaping** | **720.2 ms** | **90.3 %** | 18 322 ns |
| `break_all_lines` | 64.5 ms | 8.1 % | 1 641 ns |
| `align` | 1.1 ms | 0.1 % | 27 ns |

**Shaping is the whole cost.** This is the single most actionable number in E3:
deferring only line-breaking buys 8 %, so S6's laziness has to defer
`builder.build` — i.e. the whole `Layout`, not part of it. It also rules out any
design that eagerly shapes and lazily breaks.

## 3. Peak RSS

All `Layout`s held live simultaneously — the eager case's real cost, and the
first milestone that holds thousands of blocks at once (§9):

| document | parse + collect | all `Layout`s live | §12.1 target |
| --- | ---: | ---: | --- |
| `250kb.md` | 9.7 MiB | 28.4 MiB | — |
| **`1mb.md`** | 13.7 MiB | **84.6 MiB** (88.7 MB) | **≤ 120 MB — passes** |
| `5mb.md` | 35.9 MiB | **379.3 MiB** | — |

§12.1's 1 MB target passes, but read it carefully before banking it: **84.6 MiB
is layout alone.** There is no window, no `winit`, no `wgpu`/`vello` device, no
`mt-render`, no glyph atlas and no swapchain in that process. The remaining
headroom to 120 MB is about 31 MB, and a GPU device plus a swapchain at 4K
routinely costs more than that on its own.

The per-document multiplier is the thing to carry forward: holding every
`Layout` costs **roughly 70× the document's byte size** (1 MB → 71 MiB of
`Layout`s above the parse baseline). At 5 MB it is 343 MiB.

## 4. Re-break at a new width — C5 tested, not assumed

M3.md §4 C5 asserts from parley's docs that a width change is the cheap case.
**It is right, by a factor of fifteen.**

| widths | (a) re-break existing | (b) rebuild from scratch | ratio |
| --- | ---: | ---: | ---: |
| 800 → 750 (muya default → what all 32 themes set) | **54.6 ms** | 825.9 ms | **15.1×** |
| 800 → 1600 (large jump) | **54.3 ms** | 802.1 ms | **14.8×** |

Per block: re-break **1 376 ns**, rebuild **19 963 ns**.

The large jump does not hide the cost — the ratio is flat, because re-breaking
does not re-shape and the amount of shaping is what varies with content, not
with width. Re-breaking the entire 5 MB document at a new width costs 54 ms,
which is inside a 60 fps frame budget twice over at document scale, so **the
"visible blocks plus a viewport margin" restriction §5 puts on width changes is
unnecessary**: on this hardware the whole document can be re-broken eagerly on
every width change.

To keep this honest, the re-break loop alternates 800/750/800/… so no repetition
can be a no-op re-break at the width the `Layout` already carries.

## 5. `Code { lines: Vec<Layout> }` — §5's most suspicious allocation

Measured on a generated code-dense input (2 000 fences × 60 lines = **120 000
code lines**, 6 144 687 bytes, seed `0xE3C0DE`), because `bench/corpus/50-code-fences.md`
has only 60 code lines in total and cannot show the effect.

| variant | `Layout`s | build time | peak RSS |
| --- | ---: | ---: | ---: |
| one `Layout` per fence | 2 000 | **863 ms** | **274.9 MiB** |
| one `Layout` per **code line** (§5's sketch) | 120 000 | **1 194 ms** | **606.8 MiB** |
| | **60×** | **1.38× slower** | **2.21× the memory (+332 MiB)** |

### The constant that decides it

```
size_of::<parley::Layout<()>>()            = 328 B
resident memory per one-line Layout        = 3 956 B      (200 000 samples)
build time per one-line Layout             = 6 360 ns
```

The struct is 328 bytes; the **resident cost is 3 956 bytes — twelve times
that** — because a `Layout` owns several independently-allocated `Vec`s (glyphs,
runs, clusters, lines, items) and each one is a separate heap block with its own
rounding and header. There is no configuration in which one `Layout` per line is
cheap; the fixed cost *is* the cost for a 30-byte line.

Scaled, per the sketch:

| code lines in a document | `Vec<Layout>` memory | build time |
| ---: | ---: | ---: |
| 10 000 | 37.7 MiB | 63.6 ms |
| 50 000 | 188.7 MiB | 318.0 ms |
| 100 000 | 377.4 MiB | 636.0 ms |
| 500 000 | 1 887 MiB | 3 180 ms |

**A 100 000-line source file — a single vendored bundle pasted into a note —
would spend 377 MiB and 636 ms on the code block alone**, three times §12.1's
entire RSS budget, before the rest of the document exists.

**Verdict: `Code { lines: Vec<Layout> }` should not survive into S1.** One
`Layout` per fence is cheaper on both axes. If per-line addressing is needed for
gutter numbers or per-line highlighting, it should come from the single
`Layout`'s existing line list (`Layout::len()` / `Layout::lines()` already give
one entry per broken line) rather than from one `Layout` per line.

## 6. Linearity — is parley 0.7.0's non-linear bug really gone?

**Yes. Nothing superlinear survives, and the residual is sublinear.**

Cost per byte, bucketed by block size, over the same 39 308 blocks:

| block size (bytes) | blocks | mean ns | **ns/byte** |
| --- | ---: | ---: | ---: |
| 0..64 | 21 137 | 5 616 | **367.39** |
| 64..128 | 5 115 | 18 028 | **186.64** |
| 128..256 | 4 608 | 29 776 | **162.68** |
| 256..512 | 7 166 | 57 638 | **152.72** |
| 512..1024 | 1 282 | 82 501 | **147.82** |

ns/byte **falls** monotonically and converges to ≈ 147. That is the signature of
`cost = fixed + k·bytes` with a large `fixed`, which is the opposite of the
0.7.0 bug — a superlinear cost would show ns/byte climbing.

Confirmed at document scale, where throughput is flat across a 20× size range:

| document | throughput |
| --- | ---: |
| `250kb.md` | 6.7 MB/s |
| `1mb.md` | 6.6 MB/s |
| `5mb.md` | 6.5 MB/s |

So C5's worry is closed. The performance problem M3 has is **not** non-linearity
— it is a per-`Layout` fixed cost of a few microseconds multiplied by 39 308
blocks. That reframes the whole of D9: the enemy is block *count*, not document
*size*, and it is the same enemy that kills §5's per-code-line sketch.

## 7. Which parley — the D2 spot-check

Identical harness, identical output (**92 758 lines broken on both**), same
machine, run alternately:

| | parley 0.11.0 | parley `main` @ `a0752c7` | delta |
| --- | ---: | ---: | ---: |
| layout, `5mb.md` (median of launches) | **714 ms** | **813 ms** | **`main` 13.9 % slower** |
| peak RSS, `5mb.md` | **271.5 MiB** | **379.9 MiB** | **`main` +40 %** |
| peak RSS, `1mb.md` | **62.3 MiB** | **84.6 MiB** | **`main` +36 %** |

By phase:

| phase | 0.11.0 | `main` | delta |
| --- | ---: | ---: | ---: |
| `builder.build` (shaping) | 668.7 ms | 737.9 ms | +10.3 % |
| **`break_all_lines`** | **37.6 ms** | **65.6 ms** | **+74.5 %** |
| builder setup / `align` | 12.3 / 1.1 ms | 12.2 / 1.1 ms | unchanged |

`break_all_lines` got **74 % slower** on `main`, which points squarely at PR
**#697** ("Calculate line box sizing following CSS") — the change E1 already
found moves every vertical metric. The 10 % shaping regression and the extra
108 MiB most plausibly come from #715's grapheme/"atom" restructuring, which
added a per-cluster layer; that attribution is a hypothesis, not a bisect.

**This complicates the recommendation E1 made.** E1 concluded `main` was
"strictly better on all four things E1 measured" and that the port cost two
lines. That remains true of *correctness* — but E3 puts a price on it: **13.7 %
of the open-time budget and 40 % of the memory**, in the milestone whose gates
are precisely open time and memory. D2 now has a genuine trade-off rather than a
free upgrade, and neither experiment alone would have shown it.

## 8. The third option D9 does not consider

Eager layout is embarrassingly parallel: each block's `Layout` depends on that
block's text, the theme and the width, and nothing else. `FontContext` is
`Clone`; `LayoutContext` is documented as one-per-thread. Measured with
`std::thread::scope` and a **byte-balanced** static split (an equal-*count*
split measures the tail, not parallelism), no `rayon`, no dependency:

| threads | best | speedup | throughput |
| ---: | ---: | ---: | ---: |
| 1 | 794.2 ms | 1.00× | 6.6 MB/s |
| 2 | 482.6 ms | 1.65× | 10.9 MB/s |
| 4 | 421.4 ms | 1.88× | 12.4 MB/s |
| 6 | 417.3 ms | 1.90× | 12.6 MB/s |
| 8 | 348.8 ms | 2.28× | 15.0 MB/s |
| 12 | 365.0 ms | 2.18× | 14.4 MB/s |
| 14 | 364.7 ms | 2.18× | 14.4 MB/s |
| **20** | **334.4 ms** | **2.37×** | 15.7 MB/s |

**Parse (81 ms) + parallel layout (334 ms) = 415 ms — comfortably inside the
800 ms gate, with no laziness of any kind.**

Scaling plateaus at ≈ 2.4× on 20 threads, far below linear. The most likely
cause is allocator contention — a `Layout` performs several small allocations
and the Windows heap serialises them — which would also explain why the curve
flattens between 4 and 6 threads and why the hybrid core layout makes it
non-monotonic. That is a hypothesis; E3 did not profile the allocator.

This does not overturn the D9 answer (single-threaded eager layout misses the
gate, which is what D9 asked). It adds a second lever that is **much cheaper to
build than laziness** — no viewport tracking, no invalidation, no scroll-driven
layout queue, no change to S1's data structures — and one that composes with
laziness rather than competing with it. S6 should cost both before committing to
the lazy architecture alone.

---

## Where this contradicts the plan

1. **The 800 ms gate is missed by eager layout, so D9's first branch never
   applies.** D9 offers *"if eager layout of 5 MB is comfortably inside 800 ms,
   laziness is an optimization with a stage of its own."* It is not inside
   800 ms at all: 813 ms median for layout, ≈ 896 ms with parse, on a
   13600K. The second branch is the operative one and S1 must anticipate it.

2. **§5's `Code { lines: Vec<Layout> }` is refuted, on the axis the sketch was
   least worried about.** It is 1.38× slower and **2.21× the memory** — and the
   deciding constant is that a `Layout` costs **3 956 bytes resident**, twelve
   times its 328-byte struct. It should not reach S1.

3. **§4 C5 is confirmed, and its consequence is stronger than §5 allows.**
   Re-break is 15× cheaper than rebuild, and re-breaking the *entire* 5 MB
   document costs 54 ms. §5's *"even then only for visible blocks plus a
   viewport margin"* is therefore unnecessary caution for width changes: the
   whole document can be re-broken eagerly. The viewport restriction is needed
   for *building* `Layout`s, which is a different operation.

4. **C5's diagnosis of the cost axis is right but understates it.** C5 notes
   `LayoutContext` reuse is "allocator-level, not shaped-run-level". Measured,
   shaping is **90.3 %** of eager layout — so no amount of context reuse,
   pooling or arena work addresses the cost, and any lazy design must defer
   `builder.build` itself rather than any later phase.

5. **§12.1's ≤ 120 MB for `1mb.md` passes, but the margin is not what it looks
   like.** 84.6 MiB is `mt-layout` with no window, no GPU device, no renderer
   and no glyph atlas. About 31 MB remains for everything M3 has not built yet.
   The target should be restated as a budget with named line items, or it will
   be met at S0 and missed at S5 with nobody having done anything wrong.

6. **The problem is block count, not document size.** Layout is linear —
   sublinear, in fact, converging to 147 ns/byte, with throughput flat across a
   20× size range — and parley 0.7.0's non-linearity is genuinely gone. What
   costs is a multi-microsecond fixed price per `Layout`, paid 39 308 times.
   §5's framing ("never lay out the whole document synchronously") reaches the
   right conclusion from the wrong premise, and the distinction matters because
   it is also what refutes the per-code-line sketch and what makes batching, not
   streaming, the natural mitigation.

7. **D2's pin decision is not the free upgrade E1 implied.** E1 found `main`
   strictly better on correctness for a two-line port. E3 finds it **13.7 %
   slower and 40 % hungrier**, with `break_all_lines` alone 74 % slower — in the
   milestone gated on open time and RSS. The two experiments have to be read
   together, and D2 should record both.

8. **D9's framing is binary and the option space is not.** Eager-versus-lazy
   omits parallel-eager, which meets the gate at **415 ms end to end** with no
   invalidation machinery at all. Whatever D9 decides, it should say why it did
   not take the cheaper lever first.

---

## Raw dumps

Everything below is verbatim stdout. Nothing is elided.

### A. `parse`, `eager`, `phases` — `5mb.md`, parley `main`

```
=== e3 parse — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  call: mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT)
  parse                        best      80.9 ms   worst      95.3 ms   spread  17.9 %   n=5
                               all reps (ms): 95.3 80.9 85.9 80.9 84.4
  arena nodes 54897, leaf blocks with text 39308, leaf text 5080697 bytes (96.9 % of file)
  collect_texts (rope -> String, untimed elsewhere): 3.6 ms
  leaf blocks by kind:
    paragraph          19406 blocks     4824395 bytes
    atx-heading         9812 blocks      146073 bytes
    table.cell          8136 blocks       46164 bytes
    code-block          1954 blocks       64065 bytes
  after parse                  working set     40.0 MiB   peak working set     60.2 MiB

=== e3 eager — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; styled runs: false; system font enumeration: false
  fonts: DejaVu Sans Mono (4 faces, explicitly registered, no filesystem enumeration)
  after parse + collect        working set     36.3 MiB   peak working set     41.1 MiB
  layout (all blocks)          best     805.0 ms   worst     873.7 ms   spread   8.5 %   n=3
                               all reps (ms): 805.0 843.4 873.7
  instrumented sum 866.9 ms vs wall 873.7 ms — per-block Instant overhead 6.7 ms (0.8 %)
  per-block distribution over 39308 blocks:
    mean          22055 ns
    median        10000 ns
    p99          103400 ns
    max          516100 ns  (block #4576, kind paragraph, 484 bytes)
  throughput: 6.5 MB/s of file, 6.3 MB/s of leaf text, 48828 blocks/s
  total lines broken: 92758
  linearity — layout cost against block size:
    block size (bytes)   blocks       total ns      mean ns    ns/byte
    0..64                 21137      118707400         5616     367.39
    64..128                5115       92214000        18028     186.64
    128..256               4608      137210100        29776     162.68
    256..512               7166      413031400        57638     152.72
    512..1024              1282      105766800        82501     147.82
  all Layouts held live        working set    379.3 MiB   peak working set    379.6 MiB
  39308 Layouts live; size_of::<Layout<()>>() = 328 B, so the Vec spine alone is 12.3 MiB

=== e3 phases — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px
  phase breakdown, best of 3 (per phase, 39308 blocks):
    ranged_builder + push_default        11.9 ms    1.5 %        302 ns/block
    builder.build (SHAPING)             720.2 ms   90.3 %      18322 ns/block
    break_all_lines                      64.5 ms    8.1 %       1641 ns/block
    align                                 1.1 ms    0.1 %         27 ns/block
    sum                                 797.7 ms
  after phases                 working set     41.8 MiB   peak working set    378.8 MiB
```

### B. `eager` — `1mb.md` and `250kb.md`, parley `main`

```
=== e3 eager — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/1mb.md (1048796 bytes)
  width 800 px; styled runs: false; system font enumeration: false
  fonts: DejaVu Sans Mono (4 faces, explicitly registered, no filesystem enumeration)
  after parse + collect        working set     13.7 MiB   peak working set     13.7 MiB
  layout (all blocks)          best     161.0 ms   worst     161.0 ms   spread   0.0 %   n=1
                               all reps (ms): 161.0
  instrumented sum 160.0 ms vs wall 161.0 ms — per-block Instant overhead 1.0 ms (0.6 %)
  per-block distribution over 7716 blocks:
    mean          20736 ns
    median        10000 ns
    p99           82000 ns
    max          140000 ns  (block #0, kind atx-heading, 12 bytes)
  throughput: 6.5 MB/s of file, 6.3 MB/s of leaf text, 47927 blocks/s
  total lines broken: 18442
  linearity — layout cost against block size:
    block size (bytes)   blocks       total ns      mean ns    ns/byte
    0..64                  4097       19615200         4788     313.78
    64..128                1038       17348700        16714     171.80
    128..256                895       24913400        27836     151.87
    256..512               1406       76325400        54285     143.03
    512..1024               280       21793100        77832     139.62
  all Layouts held live        working set     84.9 MiB   peak working set     84.9 MiB
  7716 Layouts live; size_of::<Layout<()>>() = 328 B, so the Vec spine alone is 2.4 MiB

=== e3 eager — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/250kb.md (256224 bytes)
  width 800 px; styled runs: false; system font enumeration: false
  fonts: DejaVu Sans Mono (4 faces, explicitly registered, no filesystem enumeration)
  after parse + collect        working set      9.4 MiB   peak working set      9.7 MiB
  layout (all blocks)          best      38.4 ms   worst      39.4 ms   spread   2.6 %   n=3
                               all reps (ms): 39.4 38.7 38.4
  instrumented sum 38.2 ms vs wall 38.4 ms — per-block Instant overhead 0.3 ms (0.7 %)
  per-block distribution over 1938 blocks:
    mean          19687 ns
    median        10000 ns
    p99           78000 ns
    max           95100 ns  (block #1872, kind paragraph, 472 bytes)
  throughput: 6.7 MB/s of file, 6.5 MB/s of leaf text, 50452 blocks/s
  total lines broken: 4580
  linearity — layout cost against block size:
    block size (bytes)   blocks       total ns      mean ns    ns/byte
    0..64                  1037        4896800         4722     309.88
    64..128                 236        3770500        15977     167.15
    128..256                247        6616400        26787     149.07
    256..512                366       18940100        51749     138.92
    512..1024                52        3928700        75552     134.89
  all Layouts held live        working set     28.4 MiB   peak working set     28.4 MiB
  1938 Layouts live; size_of::<Layout<()>>() = 328 B, so the Vec spine alone is 0.6 MiB
```

### C. parley 0.11.0 spot-check — `parse`, `eager`, `phases`, `1mb.md`

```
=== e3 parse — arm: parley 0.11.0 (crates.io) ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  call: mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT)
  parse                        best      81.6 ms   worst      95.6 ms   spread  17.2 %   n=5
                               all reps (ms): 95.6 81.6 90.1 83.9 84.6
  arena nodes 54897, leaf blocks with text 39308, leaf text 5080697 bytes (96.9 % of file)
  collect_texts (rope -> String, untimed elsewhere): 4.1 ms
  leaf blocks by kind:
    paragraph          19406 blocks     4824395 bytes
    atx-heading         9812 blocks      146073 bytes
    table.cell          8136 blocks       46164 bytes
    code-block          1954 blocks       64065 bytes
  after parse                  working set     40.0 MiB   peak working set     60.1 MiB

=== e3 eager — arm: parley 0.11.0 (crates.io) ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; styled runs: false; system font enumeration: false
  fonts: DejaVu Sans Mono (4 faces, explicitly registered, no filesystem enumeration)
  after parse + collect        working set     36.7 MiB   peak working set     41.1 MiB
  layout (all blocks)          best     744.9 ms   worst     759.0 ms   spread   1.9 %   n=3
                               all reps (ms): 759.0 758.8 744.9
  instrumented sum 739.1 ms vs wall 744.9 ms — per-block Instant overhead 5.8 ms (0.8 %)
  per-block distribution over 39308 blocks:
    mean          18803 ns
    median         8000 ns
    p99           85800 ns
    max          277300 ns  (block #35589, kind paragraph, 615 bytes)
  throughput: 7.0 MB/s of file, 6.8 MB/s of leaf text, 52766 blocks/s
  total lines broken: 92758
  linearity — layout cost against block size:
    block size (bytes)   blocks       total ns      mean ns    ns/byte
    0..64                 21137       92966100         4398     287.72
    64..128                5115       74352900        14536     150.49
    128..256               4608      115907300        25153     137.42
    256..512               7166      357798600        49930     132.29
    512..1024              1282       98091300        76514     137.09
  all Layouts held live        working set    272.2 MiB   peak working set    272.2 MiB
  39308 Layouts live; size_of::<Layout<()>>() = 280 B, so the Vec spine alone is 10.5 MiB

=== e3 phases — arm: parley 0.11.0 (crates.io) ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px
  phase breakdown, best of 3 (per phase, 39308 blocks):
    ranged_builder + push_default        12.0 ms    1.7 %        306 ns/block
    builder.build (SHAPING)             667.5 ms   93.0 %      16982 ns/block
    break_all_lines                      37.2 ms    5.2 %        948 ns/block
    align                                 1.1 ms    0.1 %         27 ns/block
    sum                                 717.9 ms
  after phases                 working set     40.7 MiB   peak working set    271.7 MiB

=== e3 eager — arm: parley 0.11.0 (crates.io) ===
  profile: release; file: ../bench/corpus/1mb.md (1048796 bytes)
  width 800 px; styled runs: false; system font enumeration: false
  fonts: DejaVu Sans Mono (4 faces, explicitly registered, no filesystem enumeration)
  after parse + collect        working set     13.9 MiB   peak working set     13.9 MiB
  layout (all blocks)          best     142.7 ms   worst     142.7 ms   spread   0.0 %   n=1
                               all reps (ms): 142.7
  instrumented sum 141.7 ms vs wall 142.7 ms — per-block Instant overhead 1.0 ms (0.7 %)
  per-block distribution over 7716 blocks:
    mean          18370 ns
    median         7800 ns
    p99           77800 ns
    max          134600 ns  (block #0, kind atx-heading, 12 bytes)
  throughput: 7.3 MB/s of file, 7.1 MB/s of leaf text, 54062 blocks/s
  total lines broken: 18442
  linearity — layout cost against block size:
    block size (bytes)   blocks       total ns      mean ns    ns/byte
    0..64                  4097       16644000         4062     266.25
    64..128                1038       14513200        13982     143.72
    128..256                895       21439800        23955     130.69
    256..512               1406       68676400        48845     128.70
    512..1024               280       20472300        73115     131.16
  all Layouts held live        working set     62.6 MiB   peak working set     62.6 MiB
  7716 Layouts live; size_of::<Layout<()>>() = 280 B, so the Vec spine alone is 2.1 MiB
```

### D. `rebreak` — 800→750 and 800→1600

```
=== e3 rebreak — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  widths: build at 800 px, then re-break at 750 px
  C5 claims re-break is cheap and content change is a full rebuild; this times both.
  initial build at 800 px: 827.8 ms, 39308 Layouts
  (a) re-break existing        best      54.6 ms   worst      58.9 ms   spread   7.8 %   n=5
                               all reps (ms): 57.3 54.6 55.9 58.9 57.6
  (b) rebuild from scratch     best     825.9 ms   worst    1321.7 ms   spread  60.0 %   n=5
                               all reps (ms): 825.9 834.0 1309.0 1307.8 1321.7
  RATIO rebuild / re-break = 15.1x   (825.9 ms vs 54.6 ms)
  per block: re-break 1390 ns, rebuild 21011 ns
  after both                   working set    406.7 MiB   peak working set    716.8 MiB

=== e3 rebreak — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  widths: build at 800 px, then re-break at 1600 px
  C5 claims re-break is cheap and content change is a full rebuild; this times both.
  initial build at 800 px: 849.7 ms, 39308 Layouts
  (a) re-break existing        best      54.3 ms   worst      59.3 ms   spread   9.0 %   n=5
                               all reps (ms): 54.8 59.3 54.3 56.8 55.2
  (b) rebuild from scratch     best     802.1 ms   worst    1390.6 ms   spread  73.4 %   n=5
                               all reps (ms): 806.4 802.1 1236.1 1371.1 1390.6
  RATIO rebuild / re-break = 14.8x   (802.1 ms vs 54.3 ms)
  per block: re-break 1382 ns, rebuild 20405 ns
  after both                   working set    405.7 MiB   peak working set    711.6 MiB
```

### E. `code` per-fence vs per-line, and `layout-overhead`

```
=== e3 code — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: .gen/code-dense.md (6144687 bytes)
  variant: per-fence
  2000 CodeBlock blocks, 120000 code lines, 6089569 bytes of code
  build (per-fence)            best     863.0 ms   worst     878.7 ms   spread   1.8 %   n=3
                               all reps (ms): 878.7 864.6 863.0
  2000 Layouts built in 862.95 ms — 431475 ns per Layout
  Layouts held live            working set    275.3 MiB   peak working set    275.9 MiB

=== e3 code — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: .gen/code-dense.md (6144687 bytes)
  variant: per-line
  2000 CodeBlock blocks, 120000 code lines, 6089569 bytes of code
  build (per-line)             best    1193.9 ms   worst    1680.5 ms   spread  40.7 %   n=3
                               all reps (ms): 1237.1 1193.9 1680.5
  120000 Layouts built in 1193.95 ms — 9950 ns per Layout
  Layouts held live            working set    606.5 MiB   peak working set    606.5 MiB

=== e3 code — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/50-code-fences.md (4277 bytes)
  variant: per-fence
  51 CodeBlock blocks, 60 code lines, 1230 bytes of code
  build (per-fence)            best       0.3 ms   worst       0.5 ms   spread  95.6 %   n=5
                               all reps (ms): 0.5 0.3 0.3 0.3 0.3
  51 Layouts built in 0.28 ms — 5510 ns per Layout
  Layouts held live            working set     10.1 MiB   peak working set     10.1 MiB

=== e3 code — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/50-code-fences.md (4277 bytes)
  variant: per-line
  51 CodeBlock blocks, 60 code lines, 1230 bytes of code
  build (per-line)             best       0.3 ms   worst       0.5 ms   spread  79.1 %   n=5
                               all reps (ms): 0.5 0.3 0.3 0.3 0.3
  60 Layouts built in 0.29 ms — 4790 ns per Layout
  Layouts held live            working set     10.2 MiB   peak working set     10.2 MiB

=== e3 layout-overhead — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release
  size_of::<parley::Layout<()>>() = 328 B (stack/Vec-spine size only)
  200000 Layouts of "    let value = compute(a, b);" (30 bytes of text each)
  build time  1222.6 ms  (6113 ns per Layout)
  working set 7.0 -> 761.6 MiB (delta 754.6 MiB, peak 761.6 MiB)
  => 3956 bytes of resident memory per one-line Layout (heap + spine)
     scaled:   10000 code lines =>     37.7 MiB and    61.1 ms
     scaled:   50000 code lines =>    188.6 MiB and   305.7 ms
     scaled:  100000 code lines =>    377.3 MiB and   611.3 ms
     scaled:  500000 code lines =>   1886.5 MiB and  3056.6 ms
```

### F. `parallel` — 1 to 20 threads

```
=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 1
  layout on 1 threads          best     794.2 ms   worst     809.8 ms   spread   2.0 %   n=3
                               all reps (ms): 809.8 798.5 794.2
  39308 Layouts; 6.6 MB/s of file, 49494 blocks/s
  after parallel               working set     40.6 MiB   peak working set    379.4 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 2
  layout on 2 threads          best     482.6 ms   worst     492.2 ms   spread   2.0 %   n=3
                               all reps (ms): 492.2 482.6 490.0
  39308 Layouts; 10.9 MB/s of file, 81448 blocks/s
  after parallel               working set     41.9 MiB   peak working set    382.4 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 4
  layout on 4 threads          best     421.4 ms   worst     432.7 ms   spread   2.7 %   n=3
                               all reps (ms): 422.9 421.4 432.7
  39308 Layouts; 12.4 MB/s of file, 93269 blocks/s
  after parallel               working set     47.1 MiB   peak working set    386.1 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 6
  layout on 6 threads          best     417.3 ms   worst     430.6 ms   spread   3.2 %   n=3
                               all reps (ms): 430.6 430.2 417.3
  39308 Layouts; 12.6 MB/s of file, 94204 blocks/s
  after parallel               working set     53.6 MiB   peak working set    394.7 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 8
  layout on 8 threads          best     348.8 ms   worst     359.1 ms   spread   2.9 %   n=3
                               all reps (ms): 359.1 350.5 348.8
  39308 Layouts; 15.0 MB/s of file, 112687 blocks/s
  after parallel               working set     54.4 MiB   peak working set    394.8 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 12
  layout on 12 threads         best     365.0 ms   worst     375.1 ms   spread   2.7 %   n=3
                               all reps (ms): 375.1 368.2 365.0
  39308 Layouts; 14.4 MB/s of file, 107680 blocks/s
  after parallel               working set     60.1 MiB   peak working set    400.9 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 14
  layout on 14 threads         best     364.7 ms   worst     372.1 ms   spread   2.0 %   n=3
                               all reps (ms): 372.1 366.3 364.7
  39308 Layouts; 14.4 MB/s of file, 107793 blocks/s
  after parallel               working set     54.1 MiB   peak working set    395.2 MiB

=== e3 parallel — arm: parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e ===
  profile: release; file: ../bench/corpus/5mb.md (5243009 bytes)
  width 800 px; threads 20
  layout on 20 threads         best     334.4 ms   worst     338.2 ms   spread   1.2 %   n=3
                               all reps (ms): 334.5 338.2 334.4
  39308 Layouts; 15.7 MB/s of file, 117560 blocks/s
  after parallel               working set     57.5 MiB   peak working set    398.6 MiB
```

### G. Between-launch drift, single-threaded `eager` on `5mb.md`

```
=== 8 separate process launches, best-of-3 within each ===
  launch 1: 813.0 ms
  launch 2: 823.0 ms
  launch 3: 804.8 ms
  launch 4: 828.5 ms
  launch 5: 808.3 ms
  launch 6: 813.9 ms
  launch 7: 813.7 ms
  launch 8: 820.8 ms
  min 804.8  median 813.8  max 828.5  n=8  spread 2.9%
```
