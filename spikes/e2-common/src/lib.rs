//! Shared "build 1" foundation for E2 (M3 S0, C10/C11): parley text layout +
//! `vello_cpu` CPU rendering, from bundled font bytes, no filesystem font
//! enumeration (E1's `default-features = false` finding).
//!
//! Every other E2 build (`e2-vello-gpu`, `e2-tiny-skia`, `e2-treesitter-*`)
//! depends on this crate and calls [`baseline_dist_kb_or_hash`] (well, calls
//! [`render_baseline`]) so that its own "+X" delta is measured against
//! *exactly* this code, not a re-typed lookalike.
//!
//! Re-exports `parley` and `vello_cpu` so downstream `main.rs` files do not
//! need to redeclare either as a direct dependency (and cannot accidentally
//! pick a different version).

pub use parley;
pub use vello_cpu;

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

/// MarkText's own bundled font directory (the Electron app this project is
/// rewriting). Open Sans there is `.woff`, which skrifa rejects (E1 finding);
/// DejaVu Sans Mono is `.ttf` and works.
pub const FONT_DIR: &str = r"C:\Dev\marktext\packages\muya\src\assets\styles\fonts";
pub const FONT_FILE: &str = "DejaVuSansMono.ttf";

/// Real text, not a placeholder — the DCE guard depends on genuine work
/// happening (see module docs on each `main.rs`).
pub const SAMPLE_TEXT: &str =
    "The quick brown fox jumps over the lazy dog 0123456789 — E2 binary-size harness, build ";

/// Assemble a `FontContext` with system-font enumeration switched off
/// (`system_fonts: false`, E1's proven pattern) and DejaVu Sans Mono
/// registered from bytes read off disk (in a real `mt-layout` this would be
/// `include_bytes!`; a spike must not vendor the font into the repo). Returns
/// the context and the registered family name to select it by.
pub fn font_context() -> (FontContext, String) {
    let mut font_cx = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    let path = std::path::Path::new(FONT_DIR).join(FONT_FILE);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("cannot read font at {}: {e}", path.display()));
    let blob = Blob::new(std::sync::Arc::new(bytes) as _);
    let result = font_cx.collection.register_fonts(blob, None);
    let family = result
        .first()
        .and_then(|(fid, _)| font_cx.collection.family_name(*fid))
        .unwrap_or("DejaVu Sans Mono")
        .to_string();
    assert!(
        !result.is_empty(),
        "DejaVu Sans Mono was rejected by parley's font registration"
    );
    (font_cx, family)
}

/// Build a single-line `Layout` of `text` at `size` px using `family`.
pub fn build_layout(
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<()>,
    family: &str,
    text: &str,
    size: f32,
) -> Layout<()> {
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    let mut layout: Layout<()> = builder.build(text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());
    layout
}

/// Render a parley `Layout` with `vello_cpu` into premultiplied-RGBA pixels.
/// Returns `(width, height, pixel_bytes)`. This is "build 1" in full: real
/// text, real shaping, real CPU rasterization, a result the caller must
/// consume (hash it) rather than drop, so LTO cannot prove the work
/// unobservable and fold it away.
pub fn render_baseline(layout: &Layout<()>) -> (u16, u16, Vec<u8>) {
    use vello_cpu::color::palette::css::BLACK;
    use vello_cpu::kurbo::Affine;
    use vello_cpu::{
        Glyph as VGlyph, Level, Pixmap, RasterizerSettings, RenderContext, RenderMode,
        RenderSettings, Resources,
    };

    let w = (layout.width().ceil().max(1.0)) as u16;
    let h = (layout.height().ceil().max(1.0)) as u16;

    let settings = RenderSettings {
        level: Level::new(),
        num_threads: 0,
    };
    let rasterizer_settings = RasterizerSettings {
        render_mode: RenderMode::OptimizeSpeed,
        ..Default::default()
    };
    let mut ctx = RenderContext::new_with(w, h, settings);
    let mut resources = Resources::new();

    ctx.set_transform(Affine::IDENTITY);
    ctx.set_paint(BLACK);

    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(gr) = item {
                let run = gr.run();
                let font = run.font();
                let size = run.font_size();
                let glyphs: Vec<VGlyph> = gr
                    .positioned_glyphs()
                    .map(|g| VGlyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                    })
                    .collect();
                ctx.glyph_run(&mut resources, font)
                    .font_size(size)
                    .hint(true)
                    .fill_glyphs(glyphs.into_iter());
            }
        }
    }
    ctx.flush();

    let mut pixmap = Pixmap::new(w, h);
    ctx.render_with(&mut pixmap, &mut resources, rasterizer_settings);
    let bytes = pixmap.data_as_u8_slice().to_vec();
    (w, h, bytes)
}

/// FNV-1a. A hashing *crate* would itself show up in `cargo bloat` and muddy
/// the attribution this experiment exists to produce, so this is hand-rolled
/// (a dozen lines, and it is not on any hot path that would care).
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Run the build-1 baseline end to end for a given `tag` (folded into the
/// sample text so each build's hash is visibly distinct) and print the
/// result. Used by every E2 binary as the "no special argument" path.
pub fn run_and_print_baseline(tag: &str) {
    let (mut font_cx, family) = font_context();
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();
    let text = format!("{SAMPLE_TEXT}{tag}");
    let layout = build_layout(&mut font_cx, &mut layout_cx, &family, &text, 16.0);
    let (w, h, pixels) = render_baseline(&layout);
    let hash = fnv1a(&pixels);
    println!(
        "[baseline/vello_cpu] tag={tag:?} family={family:?} layout={:.1}x{:.1} pixmap={w}x{h} \
         bytes={} hash={hash:016x}",
        layout.width(),
        layout.height(),
        pixels.len()
    );
}
