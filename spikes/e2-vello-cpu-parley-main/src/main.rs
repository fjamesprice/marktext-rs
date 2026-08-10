//! E2 optional — build 1's exact program (parley + `vello_cpu`) against
//! parley git `main` instead of the pinned 0.11.0 release. The delta against
//! `e2-vello-cpu`'s `dist` size is "main vs 0.11.0" from the task list of
//! cheap extras. Not folded into `e2-common` because that crate pins
//! `parley = "=0.11.0"`; this duplicates its ~40 lines rather than fighting
//! Cargo's single-version-per-workspace-build feature unification.
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

const FONT_DIR: &str = r"C:\Dev\marktext\packages\muya\src\assets\styles\fonts";
const SAMPLE_TEXT: &str =
    "The quick brown fox jumps over the lazy dog 0123456789 — E2 binary-size harness, build parley-main";

fn main() {
    let mut font_cx = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    let path = std::path::Path::new(FONT_DIR).join("DejaVuSansMono.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read font: {e}"));
    let blob = Blob::new(std::sync::Arc::new(bytes) as _);
    let result = font_cx.collection.register_fonts(blob, None);
    let family = result
        .first()
        .and_then(|(fid, _)| font_cx.collection.family_name(*fid))
        .unwrap_or("DejaVu Sans Mono")
        .to_string();

    let mut layout_cx: LayoutContext<()> = LayoutContext::new();
    let mut builder = layout_cx.ranged_builder(&mut font_cx, SAMPLE_TEXT, 1.0, true);
    builder.push_default(StyleProperty::FontSize(16.0));
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(&family)));
    let mut layout: Layout<()> = builder.build(SAMPLE_TEXT);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    let (w, h, pixels) = render(&layout);
    let hash = fnv1a(&pixels);
    println!(
        "[parley-main baseline] family={family:?} layout={:.1}x{:.1} pixmap={w}x{h} bytes={} hash={hash:016x}",
        layout.width(),
        layout.height(),
        pixels.len()
    );
}

fn render(layout: &Layout<()>) -> (u16, u16, Vec<u8>) {
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
                // parley `main`'s `Run::font()` returns `&FontInstance`, not
                // `&FontData` as in 0.11.0 — a breaking change M3.md's D2
                // already flags. `.font` unwraps to the same `FontData`
                // `vello_cpu::glyph_run` wants.
                let font = &run.font().font;
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
    (w, h, pixmap.data_as_u8_slice().to_vec())
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
