//! E3 — how long does eager layout of a 5 MB document actually take, and what
//! does it cost to hold? This is the **git `main`** arm; `e3-layout-0-11` is
//! the same file against the released 0.11.0.
//!
//! Takes M3.md §5 **D9** (lazy layout: from day one, or after measurement) and
//! feeds the S6 gate *"5 MB opens ≤ 800 ms"*. It also tests, rather than
//! assumes, §4 **C5**'s claim that a width change is the cheap case, and
//! measures §5's `Code { lines: Vec<Layout> }` sketch against one `Layout` per
//! fence.
//!
//! ## Every mode is its own process, and that is deliberate
//!
//! Peak working set is a **process-wide, monotonic** counter: once a phase has
//! touched 900 MB, every later reading in that process reports 900 MB no matter
//! what was freed. A single binary that measured eager layout and then the code
//! variants would report the first phase's peak for all of them. So each
//! scenario is a subcommand, run in its own process, and `results/` prints the
//! command lines. Nothing here spawns children — the shell is the driver, which
//! also means every number in the write-up can be reproduced one line at a time.
//!
//! ## What this measures, and what it does not
//!
//! One `parley::Layout` per **leaf block**, built from the block's plain text as
//! a single style run. It does *not* run `mt-inline` over the text to produce
//! per-token style runs, and it does not push `InlineBox`es for images or math.
//! That makes every number here a **lower bound** on real inline layout: more
//! style runs means more itemisation and more shaped runs. `eager --styled`
//! bounds that gap by synthesising style runs at a realistic density rather than
//! leaving it unquantified.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use mt_doc::{Block, Document};
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    StyleProperty,
};

/// Printed by every mode so a pasted number can never be attributed to the
/// wrong arm.
const ARM: &str = "parley git main @ a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e";

const FONT_SIZE: f32 = 16.0;

/// muya's `--editor-area-width` default.
const WIDTH_DEFAULT: f32 = 800.0;

/// MarkText's own bundled monospace faces. Explicitly registered, system font
/// enumeration off, so the numbers reproduce on a machine with a different set
/// of fonts installed. E1 proved these four `.ttf` files register and the eight
/// Open Sans `.woff` files do not.
const FACE_DIR: &str = r"C:\Dev\marktext\packages\muya\src\assets\styles\fonts";
const FACES: &[&str] = &[
    "DejaVuSansMono.ttf",
    "DejaVuSansMono-Bold.ttf",
    "DejaVuSansMono-Oblique.ttf",
    "DejaVuSansMono-BoldOblique.ttf",
];
const FAMILY: &str = "DejaVu Sans Mono";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["parse", file, reps] => mode_parse(file, reps.parse().unwrap()),
        ["eager", file, width, reps, rest @ ..] => mode_eager(
            file,
            width.parse().unwrap(),
            reps.parse().unwrap(),
            rest.contains(&"--styled"),
            rest.contains(&"--system-fonts"),
        ),
        ["rebreak", file, w1, w2, reps] => mode_rebreak(
            file,
            w1.parse().unwrap(),
            w2.parse().unwrap(),
            reps.parse().unwrap(),
        ),
        ["code", file, variant, reps] => {
            mode_code(file, variant, reps.parse().unwrap());
        }
        ["layout-overhead", n] => mode_layout_overhead(n.parse().unwrap()),
        ["phases", file, width, reps] => {
            mode_phases(file, width.parse().unwrap(), reps.parse().unwrap());
        }
        ["parallel", file, width, threads, reps] => mode_parallel(
            file,
            width.parse().unwrap(),
            threads.parse().unwrap(),
            reps.parse().unwrap(),
        ),
        ["gen-code-dense", out, fences, lines] => {
            mode_gen_code_dense(out, fences.parse().unwrap(), lines.parse().unwrap());
        }
        _ => {
            eprintln!(
                "e3 — layout scale harness ({ARM})\n\
                 \n\
                 modes:\n  \
                 parse <file> <reps>\n  \
                 eager <file> <width> <reps> [--styled] [--system-fonts]\n  \
                 rebreak <file> <w1> <w2> <reps>\n  \
                 code <file> per-fence|per-line <reps>\n  \
                 layout-overhead <n>\n  \
                 gen-code-dense <out.md> <fences> <lines-per-fence>\n\
                 \n\
                 Run each in its OWN process: peak working set is monotonic per\n\
                 process, so two scenarios in one process share one peak."
            );
            std::process::exit(2);
        }
    }
}

// ---------------------------------------------------------------------------
// Peak working set, without a dependency
// ---------------------------------------------------------------------------
//
// `GetProcessMemoryInfo` lives in psapi.dll on paper, but Windows also exports
// it from kernel32.dll as `K32GetProcessMemoryInfo`, which is what this links
// against — kernel32 is already linked by every Rust binary, so this costs one
// struct definition and no `-l` flag. The alternative was a crate (`sysinfo`,
// ~20 transitive deps, or `windows-sys`, a large generated binding set) for one
// field; the plan's own habit is to write the four lines rather than take the
// tree.

#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(
        process: isize,
        counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
}

fn memory() -> (u64, u64) {
    let mut pmc = ProcessMemoryCounters {
        cb: size_of::<ProcessMemoryCounters>() as u32,
        ..Default::default()
    };
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &raw mut pmc,
            size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    assert!(ok != 0, "K32GetProcessMemoryInfo failed");
    (
        pmc.working_set_size as u64,
        pmc.peak_working_set_size as u64,
    )
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn report_memory(label: &str) {
    let (cur, peak) = memory();
    println!(
        "  {label:<28} working set {:>8.1} MiB   peak working set {:>8.1} MiB",
        mib(cur),
        mib(peak)
    );
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

/// A `FontContext` with **no filesystem enumeration** — the path E1 proved, and
/// the one `mt-layout`'s "No I/O" constraint requires. The four faces are read
/// here rather than `include_bytes!`d only because a spike should not vendor
/// 1.2 MB of fonts into the repository.
fn font_context(system_fonts: bool) -> FontContext {
    if system_fonts {
        // The comparison arm: whatever this machine has installed, discovered
        // by walking the filesystem.
        return FontContext::new();
    }
    let mut cx = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    for face in FACES {
        let path = std::path::Path::new(FACE_DIR).join(face);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("cannot read face {}: {e}", path.display()));
        let blob = Blob::new(std::sync::Arc::new(bytes) as _);
        let registered = cx.collection.register_fonts(blob, None);
        assert!(!registered.is_empty(), "face {face} registered 0 families");
    }
    cx
}

// ---------------------------------------------------------------------------
// Document -> block texts
// ---------------------------------------------------------------------------

struct BlockText {
    name: &'static str,
    text: String,
}

/// Every leaf block's text, in document order.
///
/// The walk is iterative rather than recursive because `MAX_NESTING_DEPTH` is
/// 128 and a generated corpus can hit it; and the texts are materialised **up
/// front, untimed**, so that the timed loop measures parley and not
/// `Text::to_str`'s rope traversal or the tree walk. `results/` reports the
/// collection cost separately for exactly that reason.
fn collect_texts(doc: &Document) -> Vec<BlockText> {
    let mut out = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        if let Some(block) = doc.block(id)
            && let Some(text) = block.text()
        {
            out.push(BlockText {
                name: block.name(),
                text: text.to_str().into_owned(),
            });
        }
        for &child in doc.children(id).iter().rev() {
            stack.push(child);
        }
    }
    out
}

fn code_block_texts(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        if let Some(Block::CodeBlock { text, .. }) = doc.block(id) {
            out.push(text.to_str().into_owned());
        }
        for &child in doc.children(id).iter().rev() {
            stack.push(child);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Layout construction
// ---------------------------------------------------------------------------

/// One `Layout` for one block's text, broken and aligned at `width`.
///
/// `styled` synthesises inline style runs at roughly the density generated
/// prose carries markup — a bold run every ~12 words — to bound how far the
/// plain-text numbers under-report real inline layout.
fn build(
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<()>,
    text: &str,
    width: f32,
    styled: bool,
) -> Layout<()> {
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(FONT_SIZE));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(FAMILY)));
    if styled {
        let mut at = 0usize;
        let mut word = 0usize;
        let bytes = text.as_bytes();
        while at < text.len() {
            let next = bytes[at..]
                .iter()
                .position(|b| *b == b' ')
                .map_or(text.len(), |p| at + p);
            word += 1;
            if word.is_multiple_of(12) && next > at {
                builder.push(StyleProperty::FontWeight(FontWeight::new(700.0)), at..next);
            }
            at = next + 1;
        }
    }
    let mut layout: Layout<()> = builder.build(text);
    layout.break_all_lines(Some(width));
    layout.align(Alignment::Start, AlignmentOptions::default());
    layout
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

struct Dist {
    n: usize,
    total: Duration,
    mean_ns: f64,
    median_ns: u64,
    p99_ns: u64,
    max_ns: u64,
    max_index: usize,
}

fn distribution(mut samples: Vec<(usize, u64)>) -> Dist {
    let n = samples.len();
    let total_ns: u128 = samples.iter().map(|(_, ns)| *ns as u128).sum();
    let (max_index, max_ns) = samples
        .iter()
        .copied()
        .max_by_key(|(_, ns)| *ns)
        .unwrap_or((0, 0));
    samples.sort_unstable_by_key(|(_, ns)| *ns);
    let at = |q: f64| samples[((n as f64 * q) as usize).min(n.saturating_sub(1))].1;
    Dist {
        n,
        total: Duration::from_nanos(total_ns as u64),
        mean_ns: total_ns as f64 / n.max(1) as f64,
        median_ns: at(0.50),
        p99_ns: at(0.99),
        max_ns,
        max_index,
    }
}

fn best_of(times: &[Duration]) -> (Duration, Duration, f64) {
    let best = *times.iter().min().unwrap();
    let worst = *times.iter().max().unwrap();
    let spread = (worst.as_secs_f64() - best.as_secs_f64()) / best.as_secs_f64() * 100.0;
    (best, worst, spread)
}

fn print_reps(label: &str, times: &[Duration]) -> Duration {
    let (best, worst, spread) = best_of(times);
    let mut all = String::new();
    for t in times {
        let _ = write!(all, "{:.1} ", t.as_secs_f64() * 1000.0);
    }
    println!(
        "  {label:<28} best {:>9.1} ms   worst {:>9.1} ms   spread {:>5.1} %   n={}",
        best.as_secs_f64() * 1000.0,
        worst.as_secs_f64() * 1000.0,
        spread,
        times.len()
    );
    println!("  {:<28} all reps (ms): {}", "", all.trim_end());
    best
}

fn header(mode: &str, file: &str) -> String {
    let bytes = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    println!("=== e3 {mode} — arm: {ARM} ===");
    println!("  profile: release; file: {file} ({bytes} bytes)");
    std::fs::read_to_string(file).unwrap_or_else(|e| panic!("cannot read {file}: {e}"))
}

// ---------------------------------------------------------------------------
// Mode: parse
// ---------------------------------------------------------------------------

fn mode_parse(file: &str, reps: usize) {
    let source = header("parse", file);
    println!("  call: mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT)");

    let mut times = Vec::new();
    let mut parsed = None;
    for _ in 0..reps {
        let t = Instant::now();
        let p = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
        times.push(t.elapsed());
        parsed = Some(p);
    }
    print_reps("parse", &times);

    let parsed = parsed.unwrap();
    let t = Instant::now();
    let texts = collect_texts(&parsed.document);
    let collect = t.elapsed();

    let text_bytes: usize = texts.iter().map(|b| b.text.len()).sum();
    println!(
        "  arena nodes {}, leaf blocks with text {}, leaf text {} bytes ({:.1} % of file)",
        parsed.document.arena_len(),
        texts.len(),
        text_bytes,
        text_bytes as f64 / source.len() as f64 * 100.0
    );
    println!(
        "  collect_texts (rope -> String, untimed elsewhere): {:.1} ms",
        collect.as_secs_f64() * 1000.0
    );

    let mut by_kind: Vec<(&str, usize, usize)> = Vec::new();
    for b in &texts {
        match by_kind.iter_mut().find(|(n, _, _)| *n == b.name) {
            Some(e) => {
                e.1 += 1;
                e.2 += b.text.len();
            }
            None => by_kind.push((b.name, 1, b.text.len())),
        }
    }
    by_kind.sort_by_key(|(_, c, _)| std::cmp::Reverse(*c));
    println!("  leaf blocks by kind:");
    for (name, count, bytes) in &by_kind {
        println!("    {name:<16} {count:>7} blocks  {bytes:>10} bytes");
    }
    report_memory("after parse");
}

// ---------------------------------------------------------------------------
// Mode: eager
// ---------------------------------------------------------------------------

fn mode_eager(file: &str, width: f32, reps: usize, styled: bool, system_fonts: bool) {
    let source = header("eager", file);
    println!("  width {width} px; styled runs: {styled}; system font enumeration: {system_fonts}");
    if !system_fonts {
        println!(
            "  fonts: {FAMILY} ({} faces, explicitly registered, no filesystem enumeration)",
            FACES.len()
        );
    }

    // Parse is not part of the layout number; it has its own mode.
    let parsed = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
    let texts = collect_texts(&parsed.document);
    report_memory("after parse + collect");

    let mut font_cx = font_context(system_fonts);
    // One `LayoutContext` for the whole document, which is what parley's docs
    // ask for ("one per application or per thread") and what C5 identifies as
    // allocator-level rather than shaping-level reuse.
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();

    let mut wall = Vec::new();
    let mut layouts: Vec<Layout<()>> = Vec::new();
    let mut per_block: Vec<(usize, u64)> = Vec::new();

    for rep in 0..reps {
        layouts.clear();
        layouts.shrink_to_fit();
        per_block.clear();
        let t = Instant::now();
        for (i, b) in texts.iter().enumerate() {
            let t1 = Instant::now();
            let l = build(&mut font_cx, &mut layout_cx, &b.text, width, styled);
            per_block.push((i, t1.elapsed().as_nanos() as u64));
            layouts.push(l);
        }
        wall.push(t.elapsed());
        if rep + 1 < reps {
            // Keep the peak honest: only the final generation is held live.
            layouts.clear();
            layouts.shrink_to_fit();
        }
    }

    let best = print_reps("layout (all blocks)", &wall);
    let d = distribution(per_block.clone());
    println!(
        "  instrumented sum {:.1} ms vs wall {:.1} ms — per-block Instant overhead {:.1} ms ({:.1} %)",
        d.total.as_secs_f64() * 1000.0,
        wall.last().unwrap().as_secs_f64() * 1000.0,
        (wall.last().unwrap().as_secs_f64() - d.total.as_secs_f64()) * 1000.0,
        (wall.last().unwrap().as_secs_f64() / d.total.as_secs_f64() - 1.0) * 100.0
    );

    println!("  per-block distribution over {} blocks:", d.n);
    println!("    mean   {:>12.0} ns", d.mean_ns);
    println!("    median {:>12} ns", d.median_ns);
    println!("    p99    {:>12} ns", d.p99_ns);
    println!(
        "    max    {:>12} ns  (block #{}, kind {}, {} bytes)",
        d.max_ns,
        d.max_index,
        texts[d.max_index].name,
        texts[d.max_index].text.len()
    );

    let text_bytes: usize = texts.iter().map(|b| b.text.len()).sum();
    println!(
        "  throughput: {:.1} MB/s of file, {:.1} MB/s of leaf text, {:.0} blocks/s",
        source.len() as f64 / best.as_secs_f64() / 1e6,
        text_bytes as f64 / best.as_secs_f64() / 1e6,
        texts.len() as f64 / best.as_secs_f64()
    );
    println!(
        "  total lines broken: {}",
        layouts.iter().map(parley::Layout::len).sum::<usize>()
    );

    linearity(&texts, &per_block);

    report_memory("all Layouts held live");
    println!(
        "  {} Layouts live; size_of::<Layout<()>>() = {} B, so the Vec spine alone is {:.1} MiB",
        layouts.len(),
        size_of::<Layout<()>>(),
        mib((layouts.len() * size_of::<Layout<()>>()) as u64)
    );
    // Touch the layouts so nothing above can be optimised out.
    std::hint::black_box(&layouts);
}

/// M3.md §4 C5 notes parley 0.7.0 fixed a bug where laying out large paragraphs
/// was **non-linear in length**. If anything superlinear survives, ns/byte
/// climbs across the buckets; if layout is linear, it is flat.
fn linearity(texts: &[BlockText], per_block: &[(usize, u64)]) {
    const EDGES: &[usize] = &[0, 64, 128, 256, 512, 1024, 2048, 4096, 8192, usize::MAX];
    println!("  linearity — layout cost against block size:");
    println!(
        "    {:<18} {:>8} {:>14} {:>12} {:>10}",
        "block size (bytes)", "blocks", "total ns", "mean ns", "ns/byte"
    );
    for w in EDGES.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let mut count = 0usize;
        let mut ns = 0u128;
        let mut bytes = 0usize;
        for (i, t) in per_block {
            let len = texts[*i].text.len();
            if len >= lo && len < hi {
                count += 1;
                ns += *t as u128;
                bytes += len;
            }
        }
        if count == 0 {
            continue;
        }
        let label = if hi == usize::MAX {
            format!("{lo}+")
        } else {
            format!("{lo}..{hi}")
        };
        println!(
            "    {:<18} {:>8} {:>14} {:>12.0} {:>10.2}",
            label,
            count,
            ns,
            ns as f64 / count as f64,
            ns as f64 / bytes.max(1) as f64
        );
    }
}

// ---------------------------------------------------------------------------
// Mode: phases — where inside parley the eager time actually goes
// ---------------------------------------------------------------------------
//
// D9 asks whether laziness is load-bearing, but S6 has to know *what* to make
// lazy. `break_all_lines` is the operation C5 says is cheap and re-runnable;
// `build` is the one that shapes. If the eager cost is mostly `build`, deferring
// line-breaking buys nothing and the whole block must be deferred. This also
// guards the headline number against a harness artifact — if `FontFamily::named`
// were doing a font query per block, it would show up here as builder time.

fn mode_phases(file: &str, width: f32, reps: usize) {
    let source = header("phases", file);
    println!("  width {width} px");

    let parsed = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
    let texts = collect_texts(&parsed.document);
    let mut font_cx = font_context(false);
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();

    let mut best = [Duration::MAX; 4];
    for _ in 0..reps {
        let mut acc = [Duration::ZERO; 4];
        let mut sink: Vec<Layout<()>> = Vec::with_capacity(texts.len());
        for b in &texts {
            let t0 = Instant::now();
            let mut builder = layout_cx.ranged_builder(&mut font_cx, &b.text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(FONT_SIZE));
            builder.push_default(StyleProperty::FontFamily(FontFamily::named(FAMILY)));
            let t1 = Instant::now();
            let mut layout: Layout<()> = builder.build(&b.text);
            let t2 = Instant::now();
            layout.break_all_lines(Some(width));
            let t3 = Instant::now();
            layout.align(Alignment::Start, AlignmentOptions::default());
            let t4 = Instant::now();
            acc[0] += t1 - t0;
            acc[1] += t2 - t1;
            acc[2] += t3 - t2;
            acc[3] += t4 - t3;
            sink.push(layout);
        }
        for i in 0..4 {
            best[i] = best[i].min(acc[i]);
        }
        std::hint::black_box(&sink);
    }

    let names = [
        "ranged_builder + push_default",
        "builder.build (SHAPING)",
        "break_all_lines",
        "align",
    ];
    let total: f64 = best.iter().map(Duration::as_secs_f64).sum();
    println!(
        "  phase breakdown, best of {reps} (per phase, {} blocks):",
        texts.len()
    );
    for (name, t) in names.iter().zip(best.iter()) {
        println!(
            "    {:<32} {:>8.1} ms  {:>5.1} %   {:>8.0} ns/block",
            name,
            t.as_secs_f64() * 1000.0,
            t.as_secs_f64() / total * 100.0,
            t.as_nanos() as f64 / texts.len() as f64
        );
    }
    println!("    {:<32} {:>8.1} ms", "sum", total * 1000.0);
    report_memory("after phases");
}

// ---------------------------------------------------------------------------
// Mode: parallel — the option D9 does not consider
// ---------------------------------------------------------------------------
//
// D9 frames the choice as eager-versus-lazy, but eager layout of 39 308 blocks
// is embarrassingly parallel: each block's `Layout` depends on that block's
// text, the theme and the width, and on nothing else. `FontContext` is `Clone`
// and `LayoutContext` is documented as one-per-thread, so the per-thread state
// is already the shape parley wants.
//
// This is `std::thread::scope` and manual chunking rather than `rayon`: one
// dependency avoided in a crate that RUST-REWRITE-PLAN.md §1 marks pure, and
// the scheduling here is a static split of a `Vec`, which is all a fixed-size
// batch of independent work needs.

fn mode_parallel(file: &str, width: f32, threads: usize, reps: usize) {
    let source = header("parallel", file);
    println!("  width {width} px; threads {threads}");

    let parsed = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
    let texts = collect_texts(&parsed.document);
    // Cloned per thread: `FontContext` derives `Clone`, and the registered
    // faces come with it, so no thread re-reads a font file.
    let template = font_context(false);

    // Chunk by cumulative *bytes*, not by block count. Block sizes span three
    // orders of magnitude (1 B to 1 KB), so an equal-count split hands one
    // thread several times another's work and the measurement becomes a report
    // on the tail rather than on parallelism.
    let total_bytes: usize = texts.iter().map(|b| b.text.len()).sum();
    let target = total_bytes.div_ceil(threads);
    let mut bounds = vec![0usize];
    let mut acc = 0usize;
    for (i, b) in texts.iter().enumerate() {
        acc += b.text.len();
        if acc >= target && bounds.len() < threads {
            bounds.push(i + 1);
            acc = 0;
        }
    }
    bounds.push(texts.len());

    let mut times = Vec::new();
    let mut produced = 0usize;
    for _ in 0..reps {
        let chunks: Vec<&[BlockText]> = bounds
            .windows(2)
            .filter(|w| w[1] > w[0])
            .map(|w| &texts[w[0]..w[1]])
            .collect();
        let t = Instant::now();
        let all: Vec<Vec<Layout<()>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunks
                .iter()
                .map(|chunk| {
                    let mut font_cx = template.clone();
                    scope.spawn(move || {
                        let mut layout_cx: LayoutContext<()> = LayoutContext::new();
                        chunk
                            .iter()
                            .map(|b| build(&mut font_cx, &mut layout_cx, &b.text, width, false))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        times.push(t.elapsed());
        produced = all.iter().map(Vec::len).sum();
        std::hint::black_box(&all);
    }

    let best = print_reps(&format!("layout on {threads} threads"), &times);
    println!(
        "  {produced} Layouts; {:.1} MB/s of file, {:.0} blocks/s",
        source.len() as f64 / best.as_secs_f64() / 1e6,
        produced as f64 / best.as_secs_f64()
    );
    report_memory("after parallel");
}

// ---------------------------------------------------------------------------
// Mode: rebreak — testing C5's claim rather than assuming it
// ---------------------------------------------------------------------------

fn mode_rebreak(file: &str, w1: f32, w2: f32, reps: usize) {
    let source = header("rebreak", file);
    println!("  widths: build at {w1} px, then re-break at {w2} px");
    println!(
        "  C5 claims re-break is cheap and content change is a full rebuild; this times both."
    );

    let parsed = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
    let texts = collect_texts(&parsed.document);
    let mut font_cx = font_context(false);
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();

    let t = Instant::now();
    let mut layouts: Vec<Layout<()>> = texts
        .iter()
        .map(|b| build(&mut font_cx, &mut layout_cx, &b.text, w1, false))
        .collect();
    println!(
        "  initial build at {w1} px: {:.1} ms, {} Layouts",
        t.elapsed().as_secs_f64() * 1000.0,
        layouts.len()
    );

    // (a) re-break the existing Layouts. Alternating widths so that no rep can
    //     be a no-op re-break at the width the Layout already has.
    let mut rebreak = Vec::new();
    for rep in 0..reps {
        let w = if rep % 2 == 0 { w2 } else { w1 };
        let t = Instant::now();
        for l in &mut layouts {
            l.break_all_lines(Some(w));
            l.align(Alignment::Start, AlignmentOptions::default());
        }
        rebreak.push(t.elapsed());
    }
    let best_rebreak = print_reps("(a) re-break existing", &rebreak);

    // (b) rebuild every Layout from scratch — what a content change costs.
    let mut rebuild = Vec::new();
    for _ in 0..reps {
        let t = Instant::now();
        let fresh: Vec<Layout<()>> = texts
            .iter()
            .map(|b| build(&mut font_cx, &mut layout_cx, &b.text, w2, false))
            .collect();
        rebuild.push(t.elapsed());
        std::hint::black_box(&fresh);
    }
    let best_rebuild = print_reps("(b) rebuild from scratch", &rebuild);

    println!(
        "  RATIO rebuild / re-break = {:.1}x   ({:.1} ms vs {:.1} ms)",
        best_rebuild.as_secs_f64() / best_rebreak.as_secs_f64(),
        best_rebuild.as_secs_f64() * 1000.0,
        best_rebreak.as_secs_f64() * 1000.0
    );
    println!(
        "  per block: re-break {:.0} ns, rebuild {:.0} ns",
        best_rebreak.as_nanos() as f64 / layouts.len() as f64,
        best_rebuild.as_nanos() as f64 / layouts.len() as f64
    );
    report_memory("after both");
    std::hint::black_box(&layouts);
}

// ---------------------------------------------------------------------------
// Mode: code — §5's `Code { lines: Vec<Layout> }` against one Layout per fence
// ---------------------------------------------------------------------------

fn mode_code(file: &str, variant: &str, reps: usize) {
    let source = header("code", file);
    println!("  variant: {variant}");

    let parsed = mt_md::parse(&source, mt_md::Options::MUYA_DEFAULT);
    let fences = code_block_texts(&parsed.document);
    let total_lines: usize = fences.iter().map(|f| f.lines().count().max(1)).sum();
    let code_bytes: usize = fences.iter().map(String::len).sum();
    println!(
        "  {} CodeBlock blocks, {} code lines, {} bytes of code",
        fences.len(),
        total_lines,
        code_bytes
    );

    let mut font_cx = font_context(false);
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();
    let mut times = Vec::new();
    let mut layouts: Vec<Layout<()>> = Vec::new();

    for rep in 0..reps {
        layouts.clear();
        layouts.shrink_to_fit();
        let t = Instant::now();
        match variant {
            "per-fence" => {
                for f in &fences {
                    layouts.push(build(&mut font_cx, &mut layout_cx, f, WIDTH_DEFAULT, false));
                }
            }
            "per-line" => {
                // §5's sketch: `Code { lines: Vec<Layout> }`.
                for f in &fences {
                    for line in f.lines() {
                        layouts.push(build(
                            &mut font_cx,
                            &mut layout_cx,
                            line,
                            WIDTH_DEFAULT,
                            false,
                        ));
                    }
                }
            }
            other => panic!("unknown variant {other:?}; use per-fence or per-line"),
        }
        times.push(t.elapsed());
        if rep + 1 < reps {
            layouts.clear();
            layouts.shrink_to_fit();
        }
    }

    let best = print_reps(&format!("build ({variant})"), &times);
    println!(
        "  {} Layouts built in {:.2} ms — {:.0} ns per Layout",
        layouts.len(),
        best.as_secs_f64() * 1000.0,
        best.as_nanos() as f64 / layouts.len().max(1) as f64
    );
    report_memory("Layouts held live");
    std::hint::black_box(&layouts);
}

// ---------------------------------------------------------------------------
// Mode: layout-overhead — the constant that decides §5's sketch
// ---------------------------------------------------------------------------

fn mode_layout_overhead(n: usize) {
    println!("=== e3 layout-overhead — arm: {ARM} ===");
    println!("  profile: release");
    println!(
        "  size_of::<parley::Layout<()>>() = {} B (stack/Vec-spine size only)",
        size_of::<Layout<()>>()
    );

    let mut font_cx = font_context(false);
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();

    // A typical short line of source code, so the constant is the one §5's
    // sketch would actually pay rather than the cost of an empty string.
    const LINE: &str = "    let value = compute(a, b);";

    // Warm the shaping caches so the measurement is the per-Layout cost and not
    // first-use font loading.
    for _ in 0..64 {
        std::hint::black_box(build(
            &mut font_cx,
            &mut layout_cx,
            LINE,
            WIDTH_DEFAULT,
            false,
        ));
    }

    let (before, _) = memory();
    let t = Instant::now();
    let mut layouts = Vec::with_capacity(n);
    for _ in 0..n {
        layouts.push(build(
            &mut font_cx,
            &mut layout_cx,
            LINE,
            WIDTH_DEFAULT,
            false,
        ));
    }
    let elapsed = t.elapsed();
    let (after, peak) = memory();

    let delta = after.saturating_sub(before);
    println!(
        "  {n} Layouts of {:?} ({} bytes of text each)",
        LINE,
        LINE.len()
    );
    println!(
        "  build time  {:.1} ms  ({:.0} ns per Layout)",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_nanos() as f64 / n as f64
    );
    println!(
        "  working set {:.1} -> {:.1} MiB (delta {:.1} MiB, peak {:.1} MiB)",
        mib(before),
        mib(after),
        mib(delta),
        mib(peak)
    );
    println!(
        "  => {:.0} bytes of resident memory per one-line Layout (heap + spine)",
        delta as f64 / n as f64
    );
    let per = delta as f64 / n as f64;
    for k in [10_000usize, 50_000, 100_000, 500_000] {
        println!(
            "     scaled: {k:>7} code lines => {:>8.1} MiB and {:>7.1} ms",
            per * k as f64 / (1024.0 * 1024.0),
            elapsed.as_secs_f64() * 1000.0 / n as f64 * k as f64
        );
    }
    std::hint::black_box(&layouts);
}

// ---------------------------------------------------------------------------
// Mode: gen-code-dense
// ---------------------------------------------------------------------------
//
// `bench/corpus/` has a `--check` generator and CI fails on a hand edit, so a
// code-dense large file must NOT be written there. This writes one anywhere
// else, deterministically, from a fixed seed, using the same four-line
// xorshift64* that `xtask/src/corpus.rs` uses — so the file is reproducible
// byte-for-byte on any machine without becoming part of the committed corpus.

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() % (hi - lo) as u64) as usize
    }
}

fn mode_gen_code_dense(out: &str, fences: usize, lines_per_fence: usize) {
    const LANGS: &[&str] = &["rust", "js", "python", "go", "c", "sql", "yaml", "sh"];
    const IDENTS: &[&str] = &[
        "value", "buffer", "index", "result", "handle", "offset", "count", "state", "node",
        "cursor", "range", "token", "block", "layout", "width",
    ];
    const FORMS: &[&str] = &[
        "    let {a} = {b}.len() + {n};",
        "    if {a} > {n} {{ return Err(Error::{b}); }}",
        "    for {a} in 0..{n} {{ {b}.push({a}); }}",
        "    self.{a} = self.{b}.saturating_sub({n});",
        "    // {a}: derived from {b}, see issue #{n}",
        "    match {a} {{ Some({b}) => {n}, None => 0 }}",
        "    debug_assert!({a}.is_empty() || {b} < {n});",
    ];

    let mut rng = Rng::new(0xE3_C0DE);
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Code-dense synthetic input\n\n\
         Generated by `e3 gen-code-dense`, seed 0xE3C0DE, {fences} fences x {lines_per_fence} lines.\n\
         Deliberately NOT in `bench/corpus/` — that directory has a `--check` generator\n\
         and CI fails on any file it did not write.\n"
    );
    for i in 0..fences {
        let lang = LANGS[i % LANGS.len()];
        let _ = writeln!(s, "## Fence {i}\n");
        let _ = writeln!(s, "```{lang}");
        for _ in 0..lines_per_fence {
            let form = rng.pick(FORMS);
            let line = form
                .replace("{a}", rng.pick(IDENTS))
                .replace("{b}", rng.pick(IDENTS))
                .replace("{n}", &rng.range(0, 100_000).to_string());
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s, "```\n");
    }
    if let Some(parent) = std::path::Path::new(out).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(out, &s).unwrap_or_else(|e| panic!("cannot write {out}: {e}"));
    println!(
        "wrote {out}: {} bytes, {fences} fences, {} code lines (seed 0xE3C0DE)",
        s.len(),
        fences * lines_per_fence
    );
}
