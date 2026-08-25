# `bench/render-goldens/` — D19's pixel goldens

46 PNGs and a manifest, written **only** by `cargo xtask render --update` and
compared by `cargo xtask render` on **exact byte equality**. Read
`xtask/src/render.rs`'s module doc for the decision; this file is the operating
note.

## What is in here

| Set | Count | What it discharges |
|---|---:|---|
| One block per `BlockKind` × theme | 38 | §11.3's *"golden images per block type"*. The list is `BlockKind::ALL`, all 19 |
| Four whole inputs × theme | 8 | bidi order, CJK line breaking, ZWJ clusters, highlighted fences — properties that live *between* blocks or *inside* runs, which a per-kind crop cannot show |

`MANIFEST.txt` is compared like an image and is the half a human can review: it
records, per image, the corpus input and block index it came from, the crop
rect, the byte count and a SHA-256. **A crop rect that moved shows up there as a
text diff even when the pixels happen to hash the same.**

## The crop is `paint_bounds`, not `bounds`

D19 as written says *"cropped to the block's `bounds`"*, and that is wrong —
3,511 items in the corpus lie outside the block that owns them. Cropping to
bounds would produce a `list-item` image **with no bullet** and reduce all 1,044
`table.cell`s to two borders of four. The crop reads
`BlockDisplay::paint_bounds`, which `mt-layout` computes for exactly this.

## Ten images are nearly blank, and that is correct

`bullet-list`, `order-list`, `task-list`, `table` and `table.row` are containers
and carry `items = 0` by design, so their images are background plus nothing.
They are kept because **a container that starts painting is a change worth
failing on**. The run prints which kinds these are on every invocation; a *leaf*
kind appearing in that list is a finding, not a container.

## If this fails

**Do not add a threshold.** Comparison is exact and stays exact until a measured
divergence proves otherwise — D19 part 2, and D10's reasoning before it: *a
threshold written before the first divergence is a threshold that will absorb
it.*

1. **The manifest differs too** → geometry moved. `cargo xtask layout` will name
   the block; fix that first, because these pixels are downstream of it.
2. **Only images differ, same platform** → a real rendering change. Open the
   image, decide, and `--update` deliberately.
3. **Only images differ, and only on one OS** → this is the experiment D19 set
   up. Record the divergence in `docs/M3.md` **before** loosening anything: the
   escape hatch is a per-channel max-Δ plus a cap on differing pixels, and the
   divergence that forced it has to be written down or the number becomes one
   nobody can account for.

The rasterizer's SIMD level is pinned (`mt_render::RENDER_LEVEL`) and threads
are fixed at 0, so a same-architecture divergence is not expected and would
itself be the finding.
