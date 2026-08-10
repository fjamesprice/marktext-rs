# E2 — binary size (M3 S0, C10/C11, D1/D3)

## The answer

**§12.2's table is replaced below, measured under the profile it claims to use.
Both levers survive in direction; neither survives in magnitude.**

| # | Build | Stripped `dist` size | Δ over build 0 | Δ over build 1 |
|---|---|---:|---:|---:|
| 0 | floor (`fn main() {}`) | **98,304 B** (0.098 MB) | — | — |
| 1 | `vello_cpu` + parley (no wgpu, no tiny-skia) | **2,481,664 B** (2.482 MB) | +2,383,360 B (2.383 MB) | — |
| 2 | build 1 + `vello`/`wgpu` | **6,799,360 B** (6.799 MB) | +6,701,056 B (6.701 MB) | **+4,317,696 B (4.318 MB)** |
| 3 | build 1 + `tiny-skia` | **2,871,808 B** (2.872 MB) | +2,773,504 B (2.774 MB) | +390,144 B (0.390 MB) |
| 4a | build 1 + tree-sitter core + **1** grammar | **3,661,312 B** (3.661 MB) | +3,563,008 B (3.563 MB) | +1,179,648 B (1.180 MB) |
| 4b | build 1 + tree-sitter core + **20** grammars | **28,388,864 B** (28.389 MB) | +28,290,560 B (28.291 MB) | **+25,907,200 B (25.907 MB)** |

- **Lever 1 (drop `wgpu`/`vello`)**: §12.2 claims **−4–6 MB**. Measured **−4.318 MB** (row 2 minus row 1) — inside the claimed range, at its low edge. Survives, with a correction: see below, the plan's own mechanism ("ship `tiny-skia` only") is not achievable, because `tiny-skia` cannot render text.
- **Lever 2 (tree-sitter grammars on demand)**: §12.2 claims **−3–4 MB**. Measured **−25.907 MB** is available by not bundling 20 grammars (row 4b minus row 1), and the marginal cost **per grammar averages 1.301 MB** (24,727,552 B / 19, from row 4b minus row 4a). §12.2's claim is **understated by roughly 6–8×**. This is the largest finding in the experiment.
- **C10's static-linkage claim: confirmed directly.** `tree-sitter-rust-0.24.2/bindings/rust/build.rs` compiles a pre-generated `src/parser.c` (6.5 MB of checked-in C source) and `src/scanner.c` via the `cc` crate into a static library, linked into the final binary. Nothing about this is a feature flag; "on demand" would mean a real subsystem (dylib + `dlopen`, or a wasm engine), exactly as C10 argues.

## Versions, profile, and environment

| | |
|---|---|
| toolchain | cargo 1.97.1 / rustc 1.97.1, Windows 11, `x86_64-pc-windows-msvc` |
| profile measured | `dist` (see below — copied from root `Cargo.toml`, then this workspace's pre-existing `[profile.release]` merged rather than deleted) |
| profile for `cargo bloat` | `dist-bloat` — `dist` with `strip = false`, `debug = 1` |
| `vello_cpu` | `0.2.0` |
| `vello` (GPU) | `0.9.0`, pulling `wgpu 29.0.4` and `naga 29.0.4` |
| `tiny-skia` | `0.12.0` |
| `tree-sitter` | `0.26.12`, `tree-sitter-language 0.1.7` |
| `parley` | `0.11.0` (crates.io, pinned `=0.11.0`) for all four required builds; `main` @ `a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e` (same rev E1 pinned) for the optional delta |
| `syntect` | `5.3.0`, `default-onig` vs `default-fancy` features |
| `accesskit` | `0.21.1` |
| `ignore` / `grep-searcher` / `grep-regex` | `0.4.33` / `0.1.17` / `0.1.14` |
| C toolchain | MSVC via Visual Studio 2022 Build Tools, auto-detected by `cc`/`find-msvc-tools` — **already installed**, nothing added for this experiment. `cl.exe` was not on `PATH`; the grammar builds succeeded anyway, so cc-rs's own MSVC discovery is doing real work. This experiment cannot speak to the cost of provisioning a C toolchain from zero, only to the fact that one is required. |

### The profile — copied from root, and one honest discrepancy

The brief for this experiment said "the `spikes/` workspace has no `[profile]`
table of its own." **That was wrong** — `spikes/Cargo.toml` already carries a
`[profile.release]` with `debug = true`, added for E3 so a profiler has
symbols to point at. Rather than silently deleting E3's line, the fields that
matter for a size measurement were added alongside it:

```toml
[profile.release]
debug = true      # E3's, pre-existing — see note below
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"

[profile.dist]          # size-tuned; see root Cargo.toml and RUST-REWRITE-PLAN.md §12
inherits = "release"
opt-level = "s"

[profile.dist-bloat]
inherits = "dist"
strip = false
debug = 1
```

This is root's `[profile.release]`/`[profile.dist]` **verbatim for every field
that affects `.exe` bytes**, plus E3's `debug = true` left in place. On the
MSVC target this workspace builds for, `debug` controls whether a *separate*
`.pdb` is emitted — it does not put anything into the `.exe` — and
`strip = "symbols"` still fully strips the executable regardless. Confirmed
in practice: every stripped `dist` binary below is what a profiler-blind
shipping build would produce; nothing here is a byte larger because `debug`
stayed `true`.

## Method

Every build under `spikes/e2-*` is `--profile dist`, run as `cargo build -p
<name> --profile dist`; binaries land in `spikes/target/dist/`. A second copy
of each is built `--profile dist-bloat` for `cargo bloat --crates -n 40 -p
<name> --profile dist-bloat` (installed via `cargo install cargo-bloat`,
version `0.12.1`). **File sizes below are always from the stripped `dist`
build; `cargo bloat` output is always from the unstripped `dist-bloat` twin —
the two are not the same number and are not meant to be added.**

### The DCE guard, and the evidence it held

`e2-common` (a new path-dependency crate, `spikes/e2-common`, not counted by
`cargo xtask deps` since that only scans `crates/`) is the "build 1"
foundation: it registers DejaVu Sans Mono from bytes read off disk at
runtime (E1's proven pattern — `Collection::new(CollectionOptions { shared:
false, system_fonts: false })`, no filesystem font enumeration), lays out a
line of real text with parley, renders it with `vello_cpu` into a pixmap,
hashes the premultiplied-RGBA bytes with a hand-rolled FNV-1a (not a crate —
a hashing crate would itself show up in `cargo bloat` and muddy the
attribution), and prints the hash. Every other build depends on this same
crate, so "build 1 + X" is *exactly* build 1's code plus X's, not a
re-typed lookalike that could quietly drift.

Renderer builds (`e2-vello-gpu`, `e2-tiny-skia`) dispatch on
`std::env::args()`, which `opt-level = "s"` + `lto = "fat"` cannot evaluate
at compile time, so neither branch is provably dead:

- `e2-vello-gpu`: no argument → build 1's `vello_cpu` path; `gpu` → builds a
  real `vello::Scene` from the same layout, initializes `wgpu`, renders to a
  texture, reads the pixels back, hashes and prints them.
- `e2-tiny-skia`: no argument → build 1's `vello_cpu` path; `skia` → three
  real `tiny-skia` shapes (filled rect, semi-transparent overlapping rect,
  stroked circle — the same shape as `vello_cpu`'s own `examples/basic.rs`,
  so the two renderers are exercised comparably), hashed and printed.
- `e2-treesitter-{1,20}` need no dispatch: nothing in them is mutually
  exclusive, so the baseline render and every grammar's parse run
  unconditionally, every invocation, each result printed.

Both branches were actually run, not just compiled:

```
e2-vello-cpu.exe            [baseline/vello_cpu] ... hash=87ed427e5c5468c2
e2-vello-gpu.exe            [baseline/vello_cpu] ... hash=4d46c956f3168834
e2-vello-gpu.exe gpu        [vello/wgpu] GPU render OK: 867x19 bytes=68096 hash=9e4f42b80d987203
e2-tiny-skia.exe            [baseline/vello_cpu] ... hash=f9d9f41729514495
e2-tiny-skia.exe skia       [tiny-skia] pixmap=100x100 bytes=40000 hash=00bd6c4e1e4e30df
e2-treesitter-1.exe         [tree-sitter] grammars=1 ... root_kind="source_file" node_count=20
e2-treesitter-20.exe        [tree-sitter] grammars=20 ... total_nodes_across_all_grammars=752
```

The GPU path initialized and rendered successfully on this machine (it does
not have to, per the brief — only linkage is being measured).

**Proof in `cargo bloat`, not just in file size**: build 2 (6.80 MB) > build
1 (2.48 MB) as required, and the per-crate table attributes real, large
byte counts to exactly the crates that should be new — `naga` (898.2 KiB),
`wgpu_hal` (384.5 KiB), `wgpu_core` (338.9 KiB), `wgpu` (79.2 KiB), `ash`
(35.0 KiB), `glow` (22.6 KiB), `gpu_allocator` (15.0 KiB) — none of which
appear in build 1's table. Build 1 itself attributes 193.0 KiB to
`vello_cpu`, 280.0 KiB to `skrifa`, 262.2 KiB to `harfrust`, 54.2 KiB to
`parley` — all present, all nonzero, confirming the baseline is genuinely
linked and not folded to a stub.

## `cargo bloat --crates -n 40` — verbatim, `dist-bloat` profile

### Build 0 — floor

```
 File  .text    Size Crate
42.2%  59.2% 40.5KiB std
 2.8%   3.9%  2.7KiB [Unknown]
 0.3%   0.4%    247B
 0.0%   0.0%     30B `__scrt_common_main_seh'
 0.0%   0.0%      1B e2_floor
71.4% 100.0% 68.5KiB .text section size, the file size is 96.0KiB
```

### Build 1 — `vello_cpu` + parley

```
 File  .text     Size Crate
14.6%  18.3% 354.4KiB fearless_simd
11.5%  14.5% 280.0KiB skrifa
10.8%  13.5% 262.2KiB harfrust
 9.9%  12.4% 239.4KiB std
 8.0%  10.0% 193.0KiB vello_cpu
 7.8%   9.7% 188.5KiB read_fonts
 2.6%   3.2%  61.9KiB vello_common
 2.3%   2.8%  54.6KiB glifo
 2.2%   2.8%  54.2KiB parley
 1.5%   1.9%  36.7KiB e2_common
 1.4%   1.8%  34.2KiB kurbo
 1.4%   1.7%  33.4KiB png
 1.3%   1.6%  30.7KiB fontique
 0.7%   0.8%  16.4KiB hashbrown
 0.5%   0.7%  12.7KiB smallvec
 0.4%   0.6%  10.7KiB fdeflate
 0.4%   0.5%  10.3KiB color
 0.4%   0.4%   8.5KiB icu_segmenter
 0.3%   0.4%   7.2KiB enum2$<read_fonts
 0.2%   0.3%   5.5KiB enum2$<skrifa
 0.1%   0.1%   2.7KiB [Unknown]
 0.1%   0.1%   1.9KiB simd_adler32
 0.1%   0.1%   1.9KiB icu_collections
 0.1%   0.1%   1.7KiB enum2$<harfrust
 0.1%   0.1%   1.6KiB parlance
 0.0%   0.1%   1.1KiB foldhash
 0.0%   0.1%    1021B crc32fast
 0.0%   0.0%     631B guillotiere
 0.0%   0.0%     630B memmap2
 0.0%   0.0%     573B enum2$<kurbo
 0.0%   0.0%     559B peniko
 0.0%   0.0%     448B enum2$<glifo
 0.0%   0.0%     382B arrayvec
 0.0%   0.0%     372B font_types
 0.0%   0.0%     247B
 0.0%   0.0%     155B enum2$<vello_common
 0.0%   0.0%     154B icu_properties
 0.0%   0.0%     135B bytemuck
 0.0%   0.0%     106B enum2$<core
 0.0%   0.0%      99B enum2$<png
 0.0%   0.0%      35B And 2 more crates. Use -n N to show more.
79.8% 100.0%   1.9MiB .text section size, the file size is 2.4MiB
```

### Build 2 — build 1 + `vello`/`wgpu`

```
 File  .text     Size Crate
13.5%  17.5% 898.2KiB naga
10.2%  13.2% 677.6KiB std
 9.0%  11.6% 595.7KiB skrifa
 5.9%   7.6% 389.5KiB read_fonts
 5.8%   7.5% 384.5KiB wgpu_hal
 5.3%   6.9% 352.5KiB fearless_simd
 5.1%   6.6% 338.9KiB wgpu_core
 4.0%   5.1% 262.4KiB harfrust
 3.2%   4.1% 211.6KiB vello_cpu
 2.1%   2.7% 139.3KiB hashbrown
 1.2%   1.5%  79.2KiB wgpu
 0.9%   1.1%  57.7KiB vello_common
 0.8%   1.1%  56.2KiB parley
 0.8%   1.1%  55.6KiB e2_vello_gpu
 0.7%   1.0%  49.2KiB vello
 0.7%   0.9%  48.5KiB vello_encoding
 0.7%   0.9%  45.1KiB vello_shaders
 0.6%   0.8%  40.8KiB kurbo
 0.5%   0.7%  35.5KiB png
 0.5%   0.7%  35.0KiB ash
 0.5%   0.6%  32.9KiB glifo
 0.4%   0.6%  29.0KiB fontique
 0.4%   0.5%  28.1KiB smallvec
 0.4%   0.5%  25.1KiB e2_common
 0.4%   0.5%  24.8KiB enum2$<naga
 0.3%   0.4%  22.6KiB glow
 0.2%   0.3%  15.3KiB codespan_reporting
 0.2%   0.3%  15.0KiB gpu_allocator
 0.2%   0.2%  11.2KiB enum2$<skrifa
 0.2%   0.2%  10.7KiB fdeflate
 0.2%   0.2%  10.3KiB color
 0.1%   0.2%   9.4KiB enum2$<read_fonts
 0.1%   0.2%   8.5KiB icu_segmenter
 0.1%   0.2%   8.4KiB wgpu_types
 0.1%   0.2%   8.2KiB indexmap
 0.1%   0.1%   7.5KiB parking_lot
 0.1%   0.1%   6.2KiB arrayvec
 0.1%   0.1%   5.8KiB half
 0.1%   0.1%   5.4KiB libm
 0.1%   0.1%   4.1KiB enum2$<core
 0.8%   1.1%  54.1KiB And 47 more crates. Use -n N to show more.
77.2% 100.0%   5.0MiB .text section size, the file size is 6.5MiB
```

### Build 3 — build 1 + `tiny-skia`

```
 File  .text     Size Crate
12.6%  15.4% 354.4KiB fearless_simd
11.3%  13.8% 317.1KiB tiny_skia
10.0%  12.2% 280.1KiB skrifa
 9.3%  11.4% 262.2KiB harfrust
 8.9%  10.8% 248.7KiB std
 6.9%   8.4% 193.1KiB vello_cpu
 6.7%   8.2% 189.3KiB read_fonts
 2.2%   2.7%  61.9KiB vello_common
 1.9%   2.4%  54.6KiB glifo
 1.9%   2.4%  54.3KiB parley
 1.3%   1.5%  35.5KiB e2_common
 1.2%   1.5%  34.2KiB kurbo
 1.2%   1.5%  33.4KiB png
 1.1%   1.3%  30.7KiB fontique
 0.8%   1.0%  22.3KiB tiny_skia_path
 0.6%   0.7%  16.4KiB hashbrown
 0.5%   0.6%  12.7KiB smallvec
 0.4%   0.5%  11.1KiB e2_tiny_skia
 0.4%   0.5%  10.7KiB fdeflate
 0.4%   0.4%  10.3KiB color
 0.3%   0.4%   8.5KiB icu_segmenter
 0.3%   0.3%   7.2KiB enum2$<read_fonts
 0.2%   0.2%   5.5KiB enum2$<skrifa
 0.1%   0.1%   2.7KiB [Unknown]
 0.1%   0.1%   2.1KiB enum2$<tiny_skia
 0.1%   0.1%   1.9KiB simd_adler32
 0.1%   0.1%   1.9KiB icu_collections
 0.1%   0.1%   1.7KiB enum2$<harfrust
 0.1%   0.1%   1.6KiB parlance
 0.0%   0.0%   1.1KiB foldhash
 0.0%   0.0%    1021B crc32fast
 0.0%   0.0%     631B guillotiere
 0.0%   0.0%     630B memmap2
 0.0%   0.0%     573B enum2$<kurbo
 0.0%   0.0%     563B arrayvec
 0.0%   0.0%     559B peniko
 0.0%   0.0%     448B enum2$<glifo
 0.0%   0.0%     372B font_types
 0.0%   0.0%     247B
 0.0%   0.0%     214B bytemuck
 0.0%   0.0%     544B And 5 more crates. Use -n N to show more.
81.8% 100.0%   2.2MiB .text section size, the file size is 2.7MiB
```

### Build 4a — build 1 + tree-sitter core + 1 grammar (`tree-sitter-rust`)

```
 File  .text     Size Crate
 9.9%  17.6% 354.4KiB fearless_simd
 7.8%  13.9% 280.0KiB skrifa
 7.3%  13.0% 262.2KiB harfrust
 6.7%  11.9% 240.0KiB std
 5.4%   9.6% 193.0KiB vello_cpu
 5.3%   9.4% 188.5KiB read_fonts
 1.7%   3.1%  61.9KiB vello_common
 1.5%   2.7%  54.6KiB glifo
 1.5%   2.7%  54.2KiB parley
 1.3%   2.3%  47.0KiB
 1.0%   1.8%  35.5KiB e2_common
 1.0%   1.7%  34.2KiB kurbo
 0.9%   1.7%  33.4KiB png
 0.9%   1.5%  30.7KiB fontique
 0.7%   1.2%  23.9KiB tree_sitter
 0.5%   0.8%  16.4KiB hashbrown
 0.4%   0.6%  12.7KiB smallvec
 0.3%   0.5%  10.7KiB fdeflate
 0.3%   0.5%  10.3KiB color
 0.2%   0.4%   8.5KiB icu_segmenter
 0.2%   0.4%   7.2KiB enum2$<read_fonts
 0.2%   0.3%   5.5KiB enum2$<skrifa
 0.1%   0.2%   3.3KiB [Unknown]
 0.1%   0.1%   1.9KiB simd_adler32
 0.1%   0.1%   1.9KiB icu_collections
 0.0%   0.1%   1.7KiB enum2$<harfrust
 0.0%   0.1%   1.6KiB parlance
 0.0%   0.1%   1.2KiB e2_treesitter_1
 0.0%   0.1%   1.1KiB foldhash
 0.0%   0.0%    1021B crc32fast
 0.0%   0.0%     707B tree_sitter_rust
 0.0%   0.0%     631B guillotiere
 0.0%   0.0%     630B memmap2
 0.0%   0.0%     573B enum2$<kurbo
 0.0%   0.0%     559B peniko
 0.0%   0.0%     448B enum2$<glifo
 0.0%   0.0%     382B arrayvec
 0.0%   0.0%     372B font_types
 0.0%   0.0%     155B enum2$<vello_common
 0.0%   0.0%     154B icu_properties
 0.0%   0.0%     370B And 4 more crates. Use -n N to show more.
56.1% 100.0%   2.0MiB .text section size, the file size is 3.5MiB
```

**`tree_sitter_rust` shows as 707 B here — read that number as almost
meaningless, not as "the grammar is cheap".** See "The number that needed
explaining" below.

### Build 4b — build 1 + tree-sitter core + 20 grammars

```
 File  .text     Size Crate
 2.4%  21.6% 659.5KiB
 1.3%  11.6% 354.4KiB fearless_simd
 1.0%   9.2% 280.0KiB skrifa
 0.9%   8.6% 262.2KiB harfrust
 0.9%   7.9% 240.1KiB std
 0.7%   6.3% 193.0KiB vello_cpu
 0.7%   6.2% 188.5KiB read_fonts
 0.5%   4.3% 130.0KiB tree_sitter_bash
 0.4%   3.6% 111.0KiB tree_sitter_md
 0.2%   2.0%  61.9KiB vello_common
 0.2%   2.0%  59.9KiB tree_sitter_css
 0.2%   1.8%  54.6KiB glifo
 0.2%   1.8%  54.2KiB parley
 0.1%   1.2%  35.5KiB e2_common
 0.1%   1.1%  34.2KiB kurbo
 0.1%   1.1%  33.4KiB png
 0.1%   1.0%  30.7KiB fontique
 0.1%   0.9%  26.2KiB tree_sitter_c_sharp
 0.1%   0.8%  24.3KiB tree_sitter_cpp
 0.1%   0.8%  24.0KiB tree_sitter
 0.1%   0.7%  20.6KiB tree_sitter_c
 0.1%   0.5%  16.4KiB hashbrown
 0.0%   0.4%  12.7KiB smallvec
 0.0%   0.4%  10.7KiB tree_sitter_php
 0.0%   0.3%  10.7KiB fdeflate
 0.0%   0.3%  10.4KiB tree_sitter_kotlin_ng
 0.0%   0.3%  10.3KiB color
 0.0%   0.3%   9.0KiB tree_sitter_typescript
 0.0%   0.3%   8.5KiB icu_segmenter
 0.0%   0.3%   8.5KiB tree_sitter_java
 0.0%   0.3%   8.3KiB tree_sitter_toml_ng
 0.0%   0.3%   7.8KiB tree_sitter_ruby
 0.0%   0.2%   7.2KiB enum2$<read_fonts
 0.0%   0.2%   5.5KiB enum2$<skrifa
 0.0%   0.1%   3.3KiB [Unknown]
 0.0%   0.1%   2.7KiB tree_sitter_python
 0.0%   0.1%   1.9KiB simd_adler32
 0.0%   0.1%   1.9KiB icu_collections
 0.0%   0.1%   1.7KiB enum2$<harfrust
 0.0%   0.1%   1.7KiB e2_treesitter_20
 0.0%   0.3%  10.4KiB And 23 more crates. Use -n N to show more.
11.0% 100.0%   3.0MiB .text section size, the file size is 27.1MiB
```

### The number that needed explaining

**`.text` is 3.0 MiB of a 27.1 MiB file — 89% of build 4b is outside the
section `cargo bloat --crates` measures**, and that gap is where almost the
entire 25.9 MB tree-sitter cost actually lives. `cargo bloat` attributes
bytes by demangling Rust symbol names in `.text` (executable code); a
grammar's compiled parse table — the giant `const` state-transition arrays
`parser.c` compiles down to — is **read-only data**, not code, so it lands
in `.rdata`, invisible to this view. This is *why* grammars with a hand-
written external scanner (`tree_sitter_bash` 130.0 KiB, `tree_sitter_md`
111.0 KiB, `tree_sitter_css` 59.9 KiB — languages with stateful lexing:
heredocs, indentation, embedded blocks) show up large in `.text` while
table-only grammars (`tree_sitter_rust` 707 B, `tree_sitter_json`, absent
from the top 40 entirely) look almost free — **the `.text` number measures
whether a grammar has custom C scanner code, not how big its grammar is.**
The real per-grammar cost is not recoverable from `cargo bloat --crates` at
all for this crate family; it is only visible in the whole-binary size
delta between build 4a (1 grammar) and build 4b (20 grammars), which is why
that delta — not the crate table — is what the headline numbers above are
built from.

### Build (optional) — `syntect` regex-onig

```
 File  .text     Size Crate
 9.4%  31.6% 101.0KiB
 8.5%  28.6%  91.3KiB std
 2.8%   9.2%  29.5KiB e2_syntect_onig
 2.4%   8.2%  26.2KiB onig_sys
 1.6%   5.3%  17.0KiB syntect
 0.9%   3.0%   9.7KiB miniz_oxide
 0.6%   2.1%   6.8KiB hashbrown
 0.3%   1.2%   3.7KiB bincode
 0.3%   1.0%   3.3KiB [Unknown]
 0.1%   0.5%   1.6KiB plist
 0.1%   0.3%   1.0KiB serde_core
 0.1%   0.3%     893B once_cell
 0.1%   0.2%     781B simd_adler32
 0.1%   0.2%     587B onig
 0.0%   0.1%     317B flate2
 0.0%   0.1%     261B enum2$<syntect
 0.0%   0.0%     161B yaml_rust
 0.0%   0.0%      30B `__scrt_common_main_seh'
29.9% 100.0% 319.5KiB .text section size, the file size is 1.0MiB
```

`onig_sys` — the compiled C library `oniguruma`, confirming C11's "pulls a
C library" claim directly — is real and present at 26.2 KiB of `.text`, with
(per the same `.text`-vs-file-size gap as tree-sitter) its own Unicode
tables almost certainly accounting for most of the remaining ~700 KiB
outside `.text`.

### Build (optional) — `syntect` regex-fancy

```
 File  .text     Size Crate
15.6%  31.5% 270.4KiB regex_automata
12.0%  24.1% 206.7KiB std
 6.1%  12.2% 105.2KiB aho_corasick
 5.3%  10.6%  91.2KiB regex_syntax
 3.1%   6.2%  53.3KiB fancy_regex
 1.7%   3.4%  29.3KiB e2_syntect_fancy
 1.3%   2.7%  23.2KiB syntect
 0.9%   1.9%  15.9KiB memchr
 0.7%   1.4%  12.1KiB hashbrown
 0.6%   1.1%   9.7KiB miniz_oxide
 0.2%   0.4%   3.7KiB bincode
 0.2%   0.3%   2.7KiB [Unknown]
 0.1%   0.3%   2.3KiB enum2$<fancy_regex
 0.1%   0.2%   1.6KiB plist
 0.1%   0.2%   1.4KiB enum2$<regex_syntax
 0.1%   0.1%   1.0KiB serde_core
 0.1%   0.1%     914B enum2$<regex_automata
 0.0%   0.1%     833B once_cell
 0.0%   0.1%     781B simd_adler32
 0.0%   0.0%     424B bit_set
 0.0%   0.0%     319B flate2
 0.0%   0.0%     261B enum2$<syntect
 0.0%   0.0%     247B
 0.0%   0.0%     194B enum2$<memchr
 0.0%   0.0%     161B yaml_rust
 0.0%   0.0%     179B And 2 more crates. Use -n N to show more.
49.7% 100.0% 859.0KiB .text section size, the file size is 1.7MiB
```

`regex_automata` + `aho_corasick` + `regex_syntax` + `fancy_regex` sum to
520.1 KiB of `.text` here, versus `onig_sys`'s 26.2 KiB — the fancy backend
is carrying vastly more compiled *code*, the onig backend vastly more
compiled *data* (Oniguruma's Unicode tables). **Both stripped `dist`
binaries land at the identical file size, 1,098,240 bytes**, confirmed by
byte-exact `stat` and by two *different* MD5 hashes (`0e41846…` onig,
`e325b78…` fancy) — the content differs, PE section-alignment padding
apparently rounds both to the same total. Reported as measured, not
over-interpreted: at this configuration, size is not a tiebreaker between
the two regex backends for D3; C11's already-sourced *speed* difference
("about half the speed" for `regex-fancy`) is.

### Build (optional) — `accesskit`

```
 File  .text    Size Crate
40.5%  60.8% 45.3KiB std
 2.4%   3.6%  2.7KiB [Unknown]
 0.9%   1.4%  1.1KiB e2_accesskit
 0.2%   0.3%    247B
 0.0%   0.1%     50B accesskit
 0.0%   0.0%     30B `__scrt_common_main_seh'
66.5% 100.0% 74.5KiB .text section size, the file size is 112.0KiB
```

### Build (optional) — `ignore` + `grep-searcher` + `grep-regex`

```
 File  .text     Size Crate
20.4%  32.4% 276.6KiB regex_automata
15.9%  25.2% 214.9KiB std
 8.6%  13.6% 116.4KiB aho_corasick
 7.4%  11.7%  99.9KiB regex_syntax
 2.5%   3.9%  33.4KiB ignore
 1.8%   2.8%  24.0KiB globset
 1.3%   2.0%  17.2KiB memchr
 1.2%   1.9%  15.8KiB e2_ignore_grep
 0.8%   1.3%  11.1KiB walkdir
 0.7%   1.1%   9.8KiB grep_searcher
 0.2%   0.3%   2.7KiB [Unknown]
 0.1%   0.2%   1.7KiB grep_regex
 0.1%   0.1%   1.3KiB enum2$<regex_syntax
 0.1%   0.1%     914B enum2$<regex_automata
 0.0%   0.1%     632B enum2$<ignore
 0.0%   0.1%     567B grep_matcher
 0.0%   0.0%     275B winapi_util
 0.0%   0.0%     247B
 0.0%   0.0%     194B enum2$<memchr
 0.0%   0.0%     185B same_file
 0.0%   0.0%     312B And 3 more crates. Use -n N to show more.
63.0% 100.0% 853.0KiB .text section size, the file size is 1.3MiB
```

## Proposed replacement for §12.2's table

| Component | §12.2 said | Measured | Status |
|---|---:|---:|---|
| `wgpu` + `vello` (incl. shaders) | 4–6 MB | **4.318 MB** (build 2 − build 1) | **measured** |
| tree-sitter core + ~20 bundled grammars | ~4 MB | **25.907 MB** (build 4b − build 1); core alone ≈ negligible, ~1.18 MB is one grammar (`tree-sitter-rust`) | **measured** — off by ~22 MB |
| `parley` + `swash` + `fontique` + `rustybuzz` + ICU segmentation | 2–3 MB | **≈2.38 MB** (build 1 − floor), but this includes `vello_cpu`'s own renderer code, not separable from the text stack with this harness | **measured (approximate; wrong stack named — see below)** |
| `ignore` + `grep-searcher` | ~1.5 MB | **1.289 MB** | **measured** |
| Bundled fonts | 1–3 MB | **1.178 MB** usable (4× DejaVu Sans Mono `.ttf`); 0.225 MB unusable (8× Open Sans `.woff`, E1: skrifa rejects them) | **measured (raw asset bytes only — not a build, and not a real font solution; see below)** |
| `accesskit` | ~0.5 MB | **0.016 MB** for the base `accesskit` crate alone | **measured (partial / lower bound — no platform adapter linked; see below)** |
| `rex` (math) | ~0.5 MB | — | **still unmeasured** (later milestone) |
| `pulldown-cmark`, `serde`, `notify`, `pdf-writer`, `spellbook`, misc | ~1.5 MB | — | **still unmeasured** (later milestone; M2.md §6 already flags `ammonia`'s subtree as understating this row further) |
| Rust std + runtime | ~0.3 MB | **0.098 MB** | **measured** — lower than claimed; see below |
| Icons, themes, locales | ~0.5 MB | — | **still unmeasured** (not a Rust binary-size question; later milestone) |
| **"Default total"** | ~17–25 MB | **not one number — see scenarios below** | — |

### Rows that need a correction beyond the number

- **`parley` + `swash` + `fontique` + `rustybuzz`**: this stack **does not
  exist**. `spikes/Cargo.lock` for build 1 contains no `swash` and no
  `rustybuzz` package at all — parley 0.11.0's actual dependency graph is
  `skrifa` (0.42.1) + `harfrust` (0.10.0) + `icu_segmenter` (2.2.0), the
  successor stack. `cargo bloat`'s build-1 table shows `skrifa` at 280.0 KiB
  and `harfrust` at 262.2 KiB of `.text` alone — both large, both real, just
  not the crates §12.2 names. This is the same "the plan is wrong about its
  own stack" pattern the milestone's docs already flag elsewhere.
- **Bundled fonts**: the "measured" number is bytes on disk for what
  happens to already be in the repo, not a font *strategy*. It is
  monospace-only (no proportional face), and per E1's finding has some
  Arabic coverage but **zero** Hebrew glyphs and no dedicated CJK face at
  all. The honest number for a real font solution is "at least 1.18 MB, and
  materially more once CJK/Arabic/Hebrew coverage MarkText does not
  currently ship is added" — this row is measured as an artifact, not
  answered as a requirement.
- **`accesskit`**: 0.016 MB measures only the base data-structure crate
  (`Node`, `Tree`, `TreeUpdate`) — this experiment did not link a platform
  adapter (`accesskit_windows`, `accesskit_macos`, `accesskit_unix`), which
  M4 would also need and which §12.2's ~0.5 MB most likely intended to
  price. Treat 0.016 MB as a floor, not the answer.
- **Rust std + runtime**: measured 0.098 MB is genuinely lower than §12.2's
  ~0.3 MB, most likely because `fn main() {}` under `panic = "abort"` +
  `lto = "fat"` touches almost none of std — no heap collections, no
  threads, no formatting machinery beyond what a static binary needs to
  start. A real `mt-app` binary will touch more of std than this floor
  does; 0.098 MB is a genuine lower bound, not a claim that std costs less
  than the plan thought.

### "Default total" — replaced with scenarios, not a single number

§12.2's single ~17–25 MB "Default total" assumed one configuration. D1 (CPU
vs. GPU renderer) and D3 (highlighter engine) are **both still open
decisions this milestone**, and the two swings above are each worth more
than the plan's entire claimed savings from *both* its levers combined — so
a single recomputed total would hide the decision it's supposed to inform.
Instead, six scenarios, each built by summing *measured marginal deltas*
over the same floor (no double-counting: build 1's own cost is added once):

| Scenario | Renderer | Highlighter | Fonts | Total |
|---|---|---|---:|---:|
| Plan-shaped (GPU + 20 grammars) | `vello_cpu`+`wgpu` | tree-sitter, 20 grammars | +1.178 MB | **33.88 MB** |
| CPU-only + 20 grammars | `vello_cpu` only | tree-sitter, 20 grammars | +1.178 MB | **29.57 MB** |
| GPU + 1 grammar | `vello_cpu`+`wgpu` | tree-sitter, 1 grammar | +1.178 MB | **9.16 MB** |
| CPU-only + 1 grammar | `vello_cpu` only | tree-sitter, 1 grammar | +1.178 MB | **4.84 MB** |
| GPU + `syntect` (no tree-sitter) | `vello_cpu`+`wgpu` | `syntect` regex-onig | +1.178 MB | **8.98 MB** |
| CPU-only + `syntect` (no tree-sitter) | `vello_cpu` only | `syntect` regex-onig | +1.178 MB | **4.66 MB** |

None of these include `rex`, `notify`/`pdf-writer`/`spellbook`/misc, icons/
themes/locales, `accesskit`'s platform adapter, or `ignore`+`grep-searcher`
(M3 is a read-only previewer — C11 — so search and accessibility are not
M3's cost to carry, and this experiment did not price them into an M3
total for that reason, even though it priced them standalone above). Adding
`ignore`+`grep-searcher` (+1.289 MB) and `accesskit`'s *measured floor*
(+0.016 MB, understated per above) to any scenario is a same-order addition;
`rex`, the misc row, and icons/themes/locales remain genuinely unpriced.

**The single largest lever available is D3, not D1.** Going from 20
grammars to `syntect` (or to 1 grammar) saves 20–25 MB regardless of which
renderer D1 picks — several times larger than dropping the GPU backend
entirely.

## The two levers, priced

### Lever 1 — drop `wgpu`/`vello`, ship `tiny-skia` only: §12.2 claims −4–6 MB

**Survives in the number, not in the mechanism.** `tiny-skia` cannot render
text (confirmed again here: its README lists text as out of scope, and
build 3 exercises it with paths/rects only, because there is nothing else
it can do with the layout build 1 produces). "Ship `tiny-skia` only" is
therefore not a text editor that can display text — the plan's own headline
lever, taken literally, is not achievable.

What *is* achievable and *is* measured:

- Drop `wgpu`/`vello`, keep `vello_cpu` (which does the text rendering
  `tiny-skia` cannot): **−4.318 MB**, inside §12.2's claimed 4–6 MB range,
  at the low edge.
- Drop `wgpu`/`vello`, keep `vello_cpu`, *also* add `tiny-skia` for
  non-text vector drawing (UI chrome, shapes): net **−3.928 MB**
  (4.318 MB saved minus 0.390 MB `tiny-skia` costs), just under the
  claimed range.

Either way the number the plan quoted is approximately right; the sentence
describing how to get it is not.

### Lever 2 — load tree-sitter grammars on demand: §12.2 claims −3–4 MB

**Badly understated — this is the headline finding.** Twenty grammars cost
**25.907 MB** over build 1, not the ~4 MB §12.2 budgeted for "core + ~20
grammars" as a *total*, and nowhere near the −3–4 MB claimed as *savings*
from making them on-demand. The marginal cost averages **1.301 MB per
grammar** (24,727,552 B over 19 grammars, build 4b − build 4a), so removing
even three grammars from the bundle already exceeds the plan's entire
claimed lever. The achievable saving from not bundling all 20 — whether by
loading on demand (C10: a real subsystem, not a flag) or by picking a
different highlighter (D3) — is **6–8× larger** than §12.2 claims, and the
per-grammar spread is real: grammars with a hand-written external scanner
(bash, markdown, css) carry meaningfully more compiled code than table-only
grammars (rust, json), though the dominant cost either way is the parse
table itself, which lives in `.rdata` and is invisible to `cargo bloat`'s
default view (see "The number that needed explaining," above).

## C10's static-linkage claim

**Confirmed directly, by reading the build script rather than inferring it.**
`tree-sitter-rust-0.24.2/bindings/rust/build.rs`:

```rust
fn main() {
    let src_dir = std::path::Path::new("src");
    let mut c_config = cc::Build::new();
    c_config.std("c11").include(src_dir);
    #[cfg(target_env = "msvc")]
    c_config.flag("-utf-8");
    c_config.file(src_dir.join("parser.c"));
    c_config.file(src_dir.join("scanner.c"));
    c_config.compile("tree-sitter-rust");
}
```

`src/parser.c` is **6.5 MB of C source, already generated and checked into
the published crate** — not codegen'd from `grammar.json` at build time,
just compiled from it. `cc::Build::compile` invokes the C compiler and
produces a static library, which cargo links into the final binary through
ordinary static linking — the same mechanism as every other
`-sys`-style crate. There is no dynamic loading anywhere in this path, and
no feature flag controls it: the C code is compiled and linked every time
the crate is built. This matches C10's claim exactly: **"on demand" would
require shipping and `dlopen`-ing platform dynamic libraries, or embedding
a wasm engine — a subsystem, not a config change.** All 20 other grammar
crates checked (`tree-sitter-{c,cpp,java,go,ruby,php,html,css,json,yaml,
bash,md,toml-ng,c-sharp,kotlin-ng,scala,javascript,typescript,python}`)
compiled successfully via the same `cc`-crate pattern, with no manual
toolchain setup — the MSVC compiler was found automatically even though
`cl.exe` was not on `PATH`, confirming a C toolchain is a real, unavoidable
build-time dependency of bundling any of them, though this experiment ran
on a machine that already had Visual Studio 2022 installed and so cannot
speak to the cost of installing one from nothing.

## `parley` `main` vs `0.11.0` — one extra number

**512 bytes.** `e2-vello-cpu-parley-main` (build 1's exact program, against
`main` @ `a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e` instead of the pinned
release) is 2,482,176 B stripped, against build 1's 2,481,664 B — noise,
not a size argument either way for D2 (pin vs. track). Porting required one
change beyond E1's own two-line diff: `main`'s `Run::font()` returns
`&FontInstance` (a struct wrapping `{ font: FontData, synthesis: Synthesis
}`), not `&FontData` directly, as 0.11.0's did — a second small breaking
change alongside `Cluster`'s grapheme-cluster semantics, which E1 already
flagged as the one to watch and which this crate does not touch either.

## What was skipped

Nothing was skipped. All four required builds, `cargo bloat` on all six
core builds (4 required + the two tree-sitter arms), and all five optional
extras (both `syntect` regex backends, `accesskit`, `ignore`+`grep-
searcher`, `parley` `main`) completed within the time-box. The only thing
this experiment did not attempt is a clean-machine C-toolchain install (see
the environment table) and linking `accesskit`'s platform adapter crates
(flagged above as a reason its number is a lower bound, not the full cost).

## Where this contradicts the plan

1. **§12.2's headline "tree-sitter core + ~20 bundled grammars: ~4 MB" is
   off by a factor of ~6.5.** Measured: 25.9 MB. This is not a rounding
   difference; it changes which decision (D1 or D3) actually controls the
   binary-size budget. **D3 should be read as the size-critical decision,
   not D1** — the plan's own framing ("two components dominate... GPU vs.
   CPU is the real M3 decision") has this backwards once measured.
2. **The "drop `wgpu`/`vello`, ship `tiny-skia` only" lever describes an
   impossible configuration.** `tiny-skia` cannot render text; a markdown
   *editor* cannot ship without a text renderer. The number (≈4.3 MB) is
   approximately right; the sentence is not — the achievable move is "drop
   `wgpu`/`vello`, keep `vello_cpu`," full stop, with `tiny-skia` an
   independent, much smaller (+0.39 MB) addition for non-text vector work
   if wanted at all.
3. **The named `parley` dependency stack (`swash` + `rustybuzz`) does not
   exist in the crate graph.** `skrifa` + `harfrust` replaced it; both
   showed up as large, real entries in every `cargo bloat` table run here.
   Anyone re-deriving §12.2's 2–3 MB estimate from today's crates.io would
   not find the crates the plan names.
4. **Rust std + runtime measures lower than claimed** (0.098 MB vs.
   ~0.3 MB) under the exact profile the plan specifies (`lto = "fat"`,
   `panic = "abort"`). Immaterial to any decision, but worth recording
   since every other row in this table was too high, not too low.
5. **`accesskit`'s ~0.5 MB likely already prices in a platform adapter this
   experiment did not link** — the base crate alone measured at 0.016 MB,
   30× lower. Not a contradiction so much as a scope note the original row
   didn't make explicit.

## Reproducing

From `spikes/`, with `%USERPROFILE%\.cargo\bin` on `PATH`:

```
# required builds
cargo build --profile dist -p e2-floor -p e2-vello-cpu -p e2-vello-gpu \
    -p e2-tiny-skia -p e2-treesitter-1 -p e2-treesitter-20

# optional builds
cargo build --profile dist -p e2-syntect-onig -p e2-syntect-fancy \
    -p e2-accesskit -p e2-ignore-grep -p e2-vello-cpu-parley-main

# per-crate attribution (needs cargo install cargo-bloat)
cargo bloat --crates -n 40 -p <name> --profile dist-bloat

# proving the DCE guard: run the args-gated branch too
target\dist\e2-vello-gpu.exe gpu
target\dist\e2-tiny-skia.exe skia
```

`spikes/e2-common` is the shared "build 1" foundation every other E2 crate
path-depends on; `spikes/Cargo.toml` carries the profile tables and the full
member list, with a comment explaining why `debug = true` was kept rather
than deleted.

---
