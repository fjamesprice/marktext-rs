//! `cargo xtask frames` — D17's scroll-frame measurement, and the number D1
//! has been deferred against since M3 S1.
//!
//! ```text
//! bench/corpus/5mb.md ──► mt_md::parse ──► layout_with ──► DisplayList   (timed once, NOT a frame)
//!                                                              │
//!        assets/fonts/faces.toml ──► Fonts + D16's FontTable ───┤
//!                                                              ▼
//!               for each scroll offset:  Renderer::render(list, frame, &mut Pixels)   ◄── one frame
//! ```
//!
//! # What a frame is here, verbatim from the plan of record
//!
//! docs/M3.md **D17**: *"one frame is `RenderContext::render_with` over a
//! viewport-sized `Pixmap` at a given scroll offset, from a `DisplayList` laid
//! out once and reused. Layout is timed and reported separately and is not in
//! the frame. No dirty rect, no blit, no atlas, no culling beyond D18's.
//! Reported as N separate process launches, median plus full min–max range, on
//! the hardware sentence `bench/BASELINE.md:95-99` uses, with the profile
//! named."*
//!
//! **One clause of that had to be read rather than transcribed, and this is the
//! reading.** `RenderContext::render_with` is `vello_cpu`'s *rasterize* call —
//! it takes a `PixmapMut` and no display list, so timing it alone would time a
//! step in which `5mb.md`'s display list does not appear, and the clause *"from
//! a `DisplayList` laid out once and reused"* would have nothing to attach to.
//! The unit that turns a display list into pixels is
//! [`Renderer::render`](mt_render::Renderer::render), which is D18's cull, the
//! per-item encode, and `render_with`, in that order and nothing else. **That
//! is what this file times as a frame**, and it is the larger of the two
//! candidate readings, which is the direction D17 says to err in.
//!
//! # Why the cull is timed twice and a frame past the end of the document is
//! timed at all
//!
//! D18 chose a **linear** scan over all 54,896 blocks per frame and said so as
//! a finding, not a default: *"54,896 bounds tests per frame is either
//! invisible against rasterization or it is the whole frame, and S4 will have
//! the number rather than the intuition."* This is that instrument, and it
//! prices the scan two ways because neither alone is honest:
//!
//! 1. **`cull` on its own** — the same [`mt_render::is_visible_in`] loop over
//!    the same `list.blocks`, outside the renderer, counting hits through
//!    `black_box`. Cheap and direct, but it walks the block array with nothing
//!    between iterations evicting cache, so it is a *lower* bound on what the
//!    scan costs inside a real frame.
//! 2. **A frame at an offset past the bottom of the document** — the real
//!    `Renderer::render` path with `blocks_drawn == 0`. That is the cull, plus
//!    the background fill, plus `render_with` over a scene holding one
//!    rectangle: the fixed floor of a frame at this viewport size, with the
//!    per-block work removed and nothing else changed.
//!
//! Their difference is the rasterization floor; a real frame minus the floor is
//! the per-block encode and paint. That decomposition is as far as this harness
//! can go **without editing `mt-render`**, and it deliberately does not: encode
//! and `render_with` are not separable from outside the crate, and adding a
//! stopwatch to the shipped renderer to split them would change the thing being
//! measured for the sake of a row nobody has to act on.
//!
//! # Two passes, and the primary number is the pessimistic one
//!
//! [`VelloCpuRenderer`](mt_render::VelloCpuRenderer) holds a glyph cache inside
//! its `Resources`, so the *second* visit to a scroll offset is cheaper than
//! the first. This runs the whole offset sweep twice per launch:
//!
//! - **Pass 1 — the primary.** Each offset seen for the first time in this
//!   process. Every glyph is a cache miss the first time its face and size are
//!   asked for, exactly as they are when a reader scrolls into a region of a
//!   document they have not been in yet.
//! - **Pass 2 — reported, clearly labelled, not the headline.** The same
//!   offsets revisited. It is what a scroll *back* costs and it is the number a
//!   warm-cache harness would have quoted as the frame time.
//!
//! D17's whole argument is that everything S4 leaves out only makes the number
//! better, so the bound it reports must be the one no later stage can erode.
//! Pass 1 is that one.
//!
//! # `num_threads`
//!
//! [`mt_render::NUM_THREADS`] is `0` and `multithreading` is not a default
//! feature of `vello_cpu` 0.2.0, so every number here is single-threaded. D17:
//! *"a frame rate quoted without the thread count is two different measurements
//! wearing one label."* It is printed in the header of every run rather than
//! left to the report to remember.
//!
//! # Running it
//!
//! ```sh
//! # RELEASE. The alias `cargo xtask` is `cargo run --package xtask`, i.e. the
//! # dev profile, and a debug frame time is not a smaller version of a release
//! # one — see crates/mt-inline/benches/tokenizer.rs:35-39.
//! cargo run --release --package xtask -- frames
//! cargo run --release --package xtask -- frames --input 1mb.md --offsets 5
//! ```
//!
//! This command **refuses to run in a debug profile** unless `--allow-debug` is
//! passed, and prints the profile in its header either way. That is one step
//! stronger than `crates/mt-inline/benches/tokenizer.rs`'s convention, which
//! only labels the profile — the tokenizer bench gets release for free from
//! `cargo bench`, and an `xtask` subcommand does not.
//!
//! # This is the first timing code in `xtask/src/`
//!
//! D17 says so explicitly (*"there is no `Instant`, `elapsed`, `Duration` or
//! `SystemTime` anywhere in `xtask/src/`"*) and names the convention to copy:
//! `crates/mt-inline/benches/tokenizer.rs:19-27`, which rejects `criterion`
//! because *"what is needed here is a number per stage that can be compared
//! with the previous stage's, not a confidence interval"*. So: a wall clock, no
//! new dependency, one line per measurement, and the aggregation across
//! launches done outside the process, because **N separate process launches** is
//! the unit `bench/BASELINE.md` §4 reports in and a best-of-M loop inside one
//! process is the friendlier number it deliberately does not quote.
//!
//! The per-measurement lines are prefixed `frame`, `cull` and `layout` and are
//! whitespace-delimited `key=value` pairs precisely so that the aggregator can
//! be four lines of shell. `bench/RENDER.md` §7 holds the one that produced the
//! committed numbers.
//!
//! # What it deliberately does not do
//!
//! - **No `assert_corpus_fully_covered`.** That gate belongs to the artifact
//!   `cargo xtask layout` writes; running it here would add tens of seconds per
//!   launch to re-prove something the goldens already gate on every CI run.
//! - **No PNG.** Nothing is written to disk. Looking at a frame is the smoke
//!   test's job (`layout::tests::block_kinds_renders_to_a_png_a_human_can_look_at`)
//!   and freezing one is D19's.
//! - **No warm-up frame before pass 1**, for the reason pass 1 is the primary.

use std::path::Path;
use std::time::Instant;

use mt_layout::{Brush, Rect, TextShaper, Theme, layout_with};
use mt_render::{Frame, Pixels, Renderer};

use crate::layout::{
    Provenance, build_collection, build_font_table, layout_options, parse_options, themes,
};

// ---------------------------------------------------------------------------
// Defaults, and each one is a choice that has to be defensible
// ---------------------------------------------------------------------------

/// The input D17 names, and the only one whose block count the plan quotes.
const DEFAULT_INPUT: &str = "5mb.md";

/// The theme whose golden header D17 cites for the 54,896 figure.
///
/// The block count is identical at both themes — the census in
/// `5mb.muya-default.txt` matches `5mb.dark.txt` line for line — so this
/// selects a palette, not a geometry. `dark` is named because it is the file
/// D17 points at.
const DEFAULT_THEME: &str = "dark";

/// The viewport, in pixels: MarkText's own default editor window content size.
///
/// **Read off the reference implementation rather than invented**, in the same
/// spirit as `bench/BASELINE.md` §4 taking its readiness criterion from the
/// project's own e2e helper:
/// `packages/desktop/src/main/windows/editor.ts:96-98` in the sibling clone
/// creates every editor window through `windowStateKeeper({ defaultWidth: 1200,
/// defaultHeight: 800 })`, and `editorWinOptions` sets `useContentSize: true`
/// (`packages/desktop/src/main/config.ts:23`), so 1200 × 800 is the **content**
/// area a first-run MarkText window has, not the outer frame.
///
/// It is also the pessimistic choice among the honest ones: the real editor
/// pane is smaller than the window (there is a sidebar and a tab bar above it),
/// so a viewport of the whole content area rasterizes strictly more pixels per
/// frame than the editor ever will.
const DEFAULT_WIDTH: u16 = 1200;
const DEFAULT_HEIGHT: u16 = 800;

/// How many scroll offsets to sample across the document.
///
/// Nine gives eighths of the scrollable range, endpoints included. D17 asks for
/// *"the scroll offsets sampled"* in the plural for a reason the corpus makes
/// concrete: `5mb.md` is 19,406 paragraphs, 9,812 headings, 1,954 fenced code
/// blocks and 2,034 tables interleaved (`bench/layout-goldens/5mb.dark.txt`
/// census), and a table row is a different amount of work per pixel from a
/// paragraph. The top of the file is not a sample of the file.
const DEFAULT_OFFSETS: usize = 9;

/// Sweeps per launch. See the module doc: pass 1 is the primary.
const DEFAULT_PASSES: usize = 2;

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

struct Opts {
    input: String,
    theme: String,
    width: u16,
    height: u16,
    offsets: usize,
    passes: usize,
    allow_debug: bool,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut opts = Opts {
            input: DEFAULT_INPUT.to_string(),
            theme: DEFAULT_THEME.to_string(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            offsets: DEFAULT_OFFSETS,
            passes: DEFAULT_PASSES,
            allow_debug: false,
        };
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            let mut value = |flag: &str| -> Result<String, String> {
                rest.next()
                    .ok_or_else(|| format!("{flag} needs a value"))
                    .cloned()
            };
            match arg.as_str() {
                "--allow-debug" => opts.allow_debug = true,
                "--input" => opts.input = value("--input")?,
                "--theme" => opts.theme = value("--theme")?,
                "--viewport" => {
                    let spec = value("--viewport")?;
                    let (w, h) = spec
                        .split_once(['x', 'X'])
                        .ok_or_else(|| format!("--viewport wants WxH, got {spec:?}"))?;
                    opts.width = w
                        .trim()
                        .parse()
                        .map_err(|e| format!("--viewport width {w:?}: {e}"))?;
                    opts.height = h
                        .trim()
                        .parse()
                        .map_err(|e| format!("--viewport height {h:?}: {e}"))?;
                }
                "--offsets" => {
                    let n = value("--offsets")?;
                    opts.offsets = n.parse().map_err(|e| format!("--offsets {n:?}: {e}"))?;
                }
                "--passes" => {
                    let n = value("--passes")?;
                    opts.passes = n.parse().map_err(|e| format!("--passes {n:?}: {e}"))?;
                }
                other => return Err(format!("unrecognised argument: {other}")),
            }
        }
        if opts.offsets == 0 {
            return Err("--offsets 0 measures nothing".into());
        }
        if opts.passes == 0 {
            return Err("--passes 0 measures nothing".into());
        }
        if opts.width == 0 || opts.height == 0 {
            return Err("a zero-area viewport rasterizes nothing".into());
        }
        Ok(opts)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `cargo xtask frames [--input NAME] [--theme NAME] [--viewport WxH]
/// [--offsets N] [--passes N] [--allow-debug]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let opts = Opts::parse(args)?;

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    if cfg!(debug_assertions) && !opts.allow_debug {
        return Err(
            "this is a debug build and D17's number must be a release one. Run\n  \
             cargo run --release --package xtask -- frames\n\
             or pass --allow-debug if you know why you want the other figure."
                .into(),
        );
    }

    // -- the collection, and D16's table beside it in the same order --------
    //
    // The assertion loop is the smoke render's, for the same reason: a table
    // that has drifted from the collection draws a perfectly well-formed frame
    // with the wrong faces in it, and a frame time over the wrong faces is a
    // frame time over a different document.
    let provenance = Provenance::read(repo_root)?;
    let mut fonts = build_collection(repo_root, &provenance)?;
    let table = build_font_table(repo_root, &provenance, &fonts)?;

    let theme: Theme = themes()
        .into_iter()
        .find(|t| t.name == opts.theme)
        .ok_or_else(|| format!("no shipped theme named {:?}", opts.theme))?;

    // -- the document -------------------------------------------------------
    let path = repo_root.join("bench").join("corpus").join(&opts.input);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .replace("\r\n", "\n");
    let (parse_opts, parse_label) = parse_options(&opts.input);

    println!("# cargo xtask frames — docs/M3.md D17");
    println!("profile     {profile}");
    println!(
        "backend     vello_cpu, num_threads={}",
        mt_render::NUM_THREADS
    );
    println!("parley      {}", provenance.face_list.meta.parley_rev);
    println!("input       {} ({} B)", opts.input, text.len());
    println!("theme       {}", theme.name);
    println!("parse       {parse_label}");
    println!("viewport    {}x{} px", opts.width, opts.height);

    // -- layout: timed, reported, and NOT a frame ---------------------------
    //
    // Parse and layout are timed separately because they are separate answers:
    // D9 already owns the parse+layout budget and D17 owns the frame. Reporting
    // one figure for both would let a layout regression hide inside a frame
    // rate.
    let t0 = Instant::now();
    let parsed = mt_md::parse(&text, parse_opts);
    let parse_ns = t0.elapsed().as_nanos();

    let options = layout_options(&parse_opts, parsed.labels.clone(), &parsed.document);
    let spans_ns = t0.elapsed().as_nanos() - parse_ns;

    let mut shaper = TextShaper::new();
    let t1 = Instant::now();
    let list = layout_with(
        &parsed.document,
        &theme,
        f32::INFINITY,
        &mut fonts,
        &mut shaper,
        &options,
    )
    .map_err(|e| format!("{} at {}: {e}", opts.input, theme.name))?;
    let layout_ns = t1.elapsed().as_nanos();

    let items_total: usize = list.blocks.iter().map(|b| b.items.len()).sum();
    println!(
        "layout      parse_ns={parse_ns} spans_ns={spans_ns} layout_ns={layout_ns} \
         blocks={} items={items_total} width={:.2} height={:.2}",
        list.blocks.len(),
        list.width,
        list.height
    );

    // -- the offsets --------------------------------------------------------
    //
    // Evenly spaced across the *scrollable* range — `height - viewport height`
    // — so the last sample is the true bottom of the document rather than a
    // viewport hanging off the end of it. A document shorter than the viewport
    // collapses to a single offset of 0, which is the only honest answer.
    let scrollable = (list.height - f32::from(opts.height)).max(0.0);
    let mut offsets: Vec<f32> = if opts.offsets == 1 || scrollable == 0.0 {
        vec![0.0]
    } else {
        (0..opts.offsets)
            .map(|i| scrollable * (i as f32) / ((opts.offsets - 1) as f32))
            .collect()
    };

    // -- and the two offsets an even sweep would miss -----------------------
    //
    // **An evenly spaced sweep is a sample of the document, not a bound on
    // it.** D17 wants the pessimistic figure, and the pessimistic frame is the
    // fullest viewport in the file — a screen of table rows or a highlighted
    // fence is a different amount of work from a screen of prose, and nothing
    // makes eighths of `5mb.md` land on one. So the whole document is swept in
    // non-overlapping viewport-height windows and the two densest are appended
    // to the sampled set. They then go through exactly the same pass-1/pass-2
    // machinery as the even offsets and are reported on the same lines — a
    // worst case measured by a special path is a worst case nobody can compare
    // with the ordinary one.
    //
    // **The two metrics, and why the obvious one is not among them.** The first
    // exploratory run searched by `DisplayItem` count and found a viewport that
    // rendered *faster* than average: one `DisplayItem::Glyphs` is one item
    // whether it holds three glyphs or three hundred, so an item count measures
    // dispatches and the rasterizer's work is glyphs. So:
    //
    // - **by-glyphs** — the summed length of every [`GlyphRun::glyphs`] in
    //   view. The closest cheap proxy for what `vello_cpu` is asked to
    //   rasterize.
    // - **by-blocks** — the most blocks D18's cull can admit at once, which is
    //   the other axis and the one the cull line is about.
    if !skip_dense(scrollable, opts.height) {
        let mut best_glyphs = (0usize, 0.0f32);
        let mut best_blocks = (0usize, 0.0f32);
        let mut y = 0.0f32;
        while y <= scrollable {
            let viewport = Rect::new(0.0, y, f32::from(opts.width), f32::from(opts.height));
            let mut blocks = 0usize;
            let mut glyphs = 0usize;
            for block in &list.blocks {
                if mt_render::is_visible_in(block, viewport) {
                    blocks += 1;
                    for item in &block.items {
                        if let mt_layout::DisplayItem::Glyphs(run) = item {
                            glyphs += run.glyphs.len();
                        }
                    }
                }
            }
            if glyphs > best_glyphs.0 {
                best_glyphs = (glyphs, y);
            }
            if blocks > best_blocks.0 {
                best_blocks = (blocks, y);
            }
            y += f32::from(opts.height);
        }
        println!(
            "dense       by-glyphs y={:.2} glyphs={} | by-blocks y={:.2} blocks={} \
             (window={} px, non-overlapping, {} windows)",
            best_glyphs.1,
            best_glyphs.0,
            best_blocks.1,
            best_blocks.0,
            opts.height,
            (scrollable / f32::from(opts.height)).floor() as u64 + 1
        );
        for y in [best_glyphs.1, best_blocks.1] {
            if !offsets.iter().any(|o| (*o - y).abs() < f32::EPSILON) {
                offsets.push(y);
            }
        }
        offsets.sort_by(f32::total_cmp);
    }

    let background = Brush::resolve(theme.colors.editor_bg, Brush::default());
    let mut renderer = mt_render::VelloCpuRenderer::new(table);
    let mut target = Pixels::new(opts.width, opts.height);

    // -- D18's scan, priced on its own --------------------------------------
    //
    // `black_box` on the count, because the whole loop is a pure function of
    // data the optimizer can see and an unread result is a loop it may delete.
    for (i, &y) in offsets.iter().enumerate() {
        let viewport = Rect::new(0.0, y, f32::from(opts.width), f32::from(opts.height));
        let t = Instant::now();
        let mut hits = 0usize;
        for block in &list.blocks {
            if mt_render::is_visible_in(block, viewport) {
                hits += 1;
            }
        }
        std::hint::black_box(hits);
        let ns = t.elapsed().as_nanos();
        println!(
            "cull        offset={i} y={y:.2} scanned={} hits={hits} ns={ns}",
            list.blocks.len()
        );
    }

    // -- the frames ---------------------------------------------------------
    for pass in 1..=opts.passes {
        for (i, &y) in offsets.iter().enumerate() {
            let viewport = Rect::new(0.0, y, f32::from(opts.width), f32::from(opts.height));
            let frame = Frame {
                viewport,
                background,
            };
            let t = Instant::now();
            let stats = renderer
                .render(&list, &frame, &mut target)
                .map_err(|e| format!("frame at y={y:.2}: {e}"))?;
            let ns = t.elapsed().as_nanos();
            println!(
                "frame       pass={pass} offset={i} y={y:.2} blocks={} clipped={} items={} \
                 skipped={} ns={ns}",
                stats.blocks_drawn, stats.blocks_clipped, stats.items_drawn, stats.items_skipped
            );
        }
    }

    // -- the floor: a real frame with nothing in the viewport ---------------
    //
    // One viewport height past the bottom of the document, so the cull runs
    // over all 54,896 blocks and rejects every one. What is left is the
    // background fill and `render_with` over a scene of one rectangle: the
    // fixed cost of a frame at this viewport size. Run in both passes' spirit —
    // once, after the sweep, when every cache is as warm as it will get, which
    // makes it the *smallest* floor and therefore the most conservative thing
    // to subtract from a frame.
    let ground: [u8; 4];
    {
        let y = list.height + f32::from(opts.height);
        let viewport = Rect::new(0.0, y, f32::from(opts.width), f32::from(opts.height));
        let frame = Frame {
            viewport,
            background,
        };
        let t = Instant::now();
        let stats = renderer
            .render(&list, &frame, &mut target)
            .map_err(|e| format!("floor frame: {e}"))?;
        let ns = t.elapsed().as_nanos();
        if stats.blocks_drawn != 0 {
            return Err(format!(
                "the floor frame was supposed to be past the end of the document and drew {} \
                 blocks",
                stats.blocks_drawn
            ));
        }
        println!("floor       y={y:.2} blocks=0 ns={ns}");
        // Every pixel of this frame is ground, so this *is* the ground colour
        // as the renderer paints it — premultiplication, rounding and all.
        // Reading it off the buffer rather than converting the theme's `Color`
        // by hand keeps the census from having a second opinion about what
        // "background" means.
        ground = target.pixel(0, 0);
    }

    // -- the census: proof the timed frames were not blank ------------------
    //
    // **The failure this exists to catch is the one the whole exercise is
    // supposed to be immune to: a fast frame that drew nothing.** Fourteen of
    // 54,896 blocks in view is a small enough number that it deserves an
    // independent check, and "`items_drawn` was 73" is a counter the renderer
    // increments, not a pixel. So every offset is re-rendered here, **outside
    // every timer**, and the painted (non-background) pixels are counted off
    // the buffer. It costs a pass over 3.8 MB per offset and buys the one thing
    // the frame times cannot state about themselves.
    println!(
        "# census — re-rendered outside every timer; `painted` counts pixels differing from \
         ground {ground:?}, read off the floor frame"
    );
    for (i, &y) in offsets.iter().enumerate() {
        let viewport = Rect::new(0.0, y, f32::from(opts.width), f32::from(opts.height));
        let frame = Frame {
            viewport,
            background,
        };
        let stats = renderer
            .render(&list, &frame, &mut target)
            .map_err(|e| format!("census frame at y={y:.2}: {e}"))?;
        let painted = target
            .data()
            .chunks_exact(4)
            .filter(|px| *px != ground.as_slice())
            .count();
        if painted == 0 {
            return Err(format!(
                "the frame at y={y:.2} drew {} blocks and {} items and painted no pixel — every \
                 frame time in this run is a time to draw nothing",
                stats.blocks_drawn, stats.items_drawn
            ));
        }
        let glyphs: usize = list
            .blocks
            .iter()
            .filter(|b| mt_render::is_visible_in(b, viewport))
            .flat_map(|b| b.items.iter())
            .map(|item| match item {
                mt_layout::DisplayItem::Glyphs(run) => run.glyphs.len(),
                _ => 0,
            })
            .sum();
        println!(
            "census      offset={i} y={y:.2} blocks={} items={} glyphs={glyphs} \
             painted={painted} of {}",
            stats.blocks_drawn,
            stats.items_drawn,
            usize::from(opts.width) * usize::from(opts.height)
        );
    }

    Ok(0)
}

/// Whether the densest-window search has nothing to find.
///
/// A document that fits in one viewport has one window, which is already
/// offset 0, and a caller who asked for a single offset asked for the top of
/// the file specifically.
fn skip_dense(scrollable: f32, viewport_height: u16) -> bool {
    scrollable <= 0.0 || scrollable < f32::from(viewport_height)
}
