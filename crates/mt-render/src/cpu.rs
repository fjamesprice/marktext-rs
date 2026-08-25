//! D1's one backend: `vello_cpu` 0.2.0, single-threaded, software.
//!
//! The API usage is E2's, unchanged, because E2 is the only place in this
//! repository where this stack has been made to draw a glyph:
//! `spikes/e2-vello-cpu-parley-main/src/main.rs:52-105`. What is added here is
//! everything the display list has that a one-paragraph spike does not — three
//! non-glyph primitives, a viewport, a clip and a font table.
//!
//! # Which blocks are clipped, and why the answer is not "all of them"
//!
//! D18 says *"a renderer must clip to `bounds` rather than assume
//! containment"*, citing `display.rs:250-256` — a code fence's glyphs carry
//! coordinates right of `bounds.max_x()` because muya's default is
//! `wrapCodeBlocks: false`, so a long line **scrolls rather than wraps**.
//! [`BlockDisplay::overflow_x`](mt_layout::BlockDisplay::overflow_x) is how
//! much horizontal range that clipping hides.
//!
//! **Applied unconditionally, that rule erases things that are not overflow**,
//! and the display list already in the tree says by how much. Scanning the 24
//! committed goldens for items that leave their block's `bounds`:
//!
//! | Items outside `bounds` | Count | `overflow_x` |
//! |---|---:|---|
//! | `table.cell` border rects, right and bottom | 3,132 | `0.00` |
//! | `atx-heading` glyph runs, left | 210 | `0.00` |
//! | `atx-heading` rects, bottom | 96 | `0.00` |
//! | `list-item` / `task-list-item` markers, left | 56 | `0.00` |
//! | `paragraph` glyph runs, right — by 0.08–0.54 px of trailing-space advance | 15 | `0.00` |
//! | `html-block` glyph runs, right | 2 | **`15.69`, `63.69`** |
//!
//! A table cell's borders are 1 px *wider* than the cell because collapsed
//! borders share a pixel — S1's own golden review recorded exactly that — so
//! its right border sits entirely at `bounds.max_x()` and its bottom border
//! entirely at `bounds.max_y()`. Clipping every block to its bounds deletes
//! **both, in every cell of every table**, and deletes 7.20 px from the first
//! glyph run of every ATX heading. That is a renderer inventing a defect and
//! then freezing it in a golden that looks deliberate, which is the failure
//! D18's own sentence is trying to prevent, pointing the other way.
//!
//! **So a block is clipped exactly when it says it overflows.** `overflow_x` is
//! the display list's own statement that something must be hidden — its doc
//! calls it *"how much horizontal scroll range that clipping hides"* — and
//! reading a field is not deriving a coordinate. Under this rule 2 of the
//! corpus's blocks clip, both of them the html-blocks that need it, and nothing
//! that should be painted is erased.
//!
//! This is a **divergence from D18 as written** and it is recorded here rather
//! than absorbed. The two other readings were considered and are worse: an
//! unconditional clip loses 3,494 items the corpus can name, and a right-edge-
//! only clip at `bounds.max_x()` still loses the last column's right border in
//! every table.
//!
//! # What is drawn, and what is deliberately not
//!
//! All four [`DisplayItem`] arms are handled and one of them draws nothing:
//! [`InlineBox`](DisplayItem::InlineBox). M3 *places* replaced elements and
//! fills none of them (`display.rs:574-579`) — the image decoder is M4's, the
//! math and diagram renderers are M6's, nothing in this workspace decodes an
//! image, and the corpus resolves zero of them. The failed-image placeholder's
//! ground rect and 20×20 icon are **already on the list** as ordinary
//! [`Rect`](DisplayItem::Rect) items; drawing anything here would draw them
//! twice.
//!
//! # Two paint choices this stack cannot read off the theme
//!
//! Neither is expressible in the theme model and neither has a MarkText number
//! to transcribe, so both are named here rather than buried:
//!
//! 1. **Dash geometry.** CSS does not specify what `dashed` looks like. This
//!    uses Blink's own ratio — dash and gap both `3 × width` — since the
//!    reference *was* Chromium. `dotted` is `width` on, `width` off, with butt
//!    caps; **no shipped theme uses it**, so the first theme that does is the
//!    one that gets to argue about round dots.
//! 2. **Variable-font instances.** `NotoEmoji[wght].ttf` is variable and the
//!    display list carries no normalized coordinates —
//!    [`mt_layout::GlyphRun`] has font, size, direction, range,
//!    baseline, offset, advance, brush and glyphs, and nothing else. So every
//!    face draws at its **default instance**. That is right for all twelve
//!    bundled faces today; it stops being right the moment a theme asks for a
//!    weight parley can synthesize from an axis.

use mt_layout::theme::LineStyle;
use mt_layout::{Brush, DisplayItem, DisplayList, FilledRect, GlyphRun, Rect, StrokedLine};
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{Affine, BezPath, Cap, RoundedRect, Shape, Stroke};
use vello_cpu::{
    Glyph as VelloGlyph, Level, PixmapMut, RasterizerSettings, RenderContext, RenderMode,
    RenderSettings, Resources,
};

use crate::backend::{Frame, FrameStats, Pixels, RenderError, Renderer};
use crate::cull::is_visible_in;
use crate::fonts::FontTable;

/// Dash and gap length as a multiple of the stroke width, for
/// [`LineStyle::Dashed`]. See this module's doc.
const DASH_RATIO: f64 = 3.0;

/// Flattening tolerance for the two curved things this crate draws: a rounded
/// rectangle's corners and a clip rectangle's (straight) outline. 0.1 px is
/// `vello_cpu`'s own working figure and is well below one device pixel.
const CURVE_TOLERANCE: f64 = 0.1;

/// `vello_cpu`'s worker count, fixed at zero — single-threaded.
///
/// `multithreading` is **not** a default feature of `vello_cpu` 0.2.0 and is
/// not enabled here. D17 requires this number to be stated beside any frame
/// time, because *a frame rate quoted without the thread count is two different
/// measurements wearing one label*.
pub const NUM_THREADS: u16 = 0;

/// D1's backend.
///
/// Holds the pieces that are expensive to build and cheap to reuse — the
/// context, the glyph cache inside [`Resources`], and D16's font table — so
/// that D17's *"N separate process launches"* time a repaint rather than a
/// warm-up.
pub struct VelloCpuRenderer {
    fonts: FontTable,
    ctx: RenderContext,
    resources: Resources,
    rasterizer: RasterizerSettings,
}

impl std::fmt::Debug for VelloCpuRenderer {
    /// Hand-written because `RenderContext` and `Resources` are not `Debug`,
    /// and because what a reader wants from this type is the font table's
    /// length — D16's one failure mode is a table that is the wrong length.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VelloCpuRenderer")
            .field("faces", &self.fonts.len())
            .field("num_threads", &NUM_THREADS)
            .finish_non_exhaustive()
    }
}

impl VelloCpuRenderer {
    /// A renderer over D16's table.
    ///
    /// The table's order is the caller's obligation and cannot be checked here
    /// — see [`FontTable`]. An empty one is *allowed* rather than rejected,
    /// because a display list with no glyph runs is a legitimate thing to draw
    /// (a page of rules and rectangles) and the failure that matters —
    /// *this run names a face I do not have* — is reported per run, with the
    /// table's length in the message.
    pub fn new(fonts: FontTable) -> VelloCpuRenderer {
        VelloCpuRenderer {
            fonts,
            // 1×1 rather than 0×0: every frame calls `reset_and_resize` before
            // it draws, so this size is never rendered, and a zero-sized
            // dispatcher is a shape the library is not asked to have.
            ctx: RenderContext::new_with(
                1,
                1,
                RenderSettings {
                    level: Level::new(),
                    num_threads: NUM_THREADS,
                },
            ),
            resources: Resources::new(),
            rasterizer: RasterizerSettings {
                render_mode: RenderMode::OptimizeSpeed,
                ..Default::default()
            },
        }
    }

    /// The font table this renderer draws with.
    pub fn fonts(&self) -> &FontTable {
        &self.fonts
    }

    // -- the four arms ----------------------------------------------------

    /// [`DisplayItem::Glyphs`] — E2's call, with the handle from D16's table.
    fn draw_glyphs(&mut self, run: &GlyphRun) -> Result<(), RenderError> {
        // Cloned rather than borrowed: `FontData` is two `Arc`-backed fields,
        // so this is a refcount bump, and it keeps the borrow of `self.fonts`
        // from outliving the `&mut self.ctx` the builder needs.
        let font = self
            .fonts
            .get(run.font)
            .ok_or(RenderError::UnknownFont {
                font: run.font,
                table_len: self.fonts.len(),
            })?
            .clone();
        self.ctx.set_paint(paint(run.brush));
        self.ctx
            .glyph_run(&mut self.resources, &font)
            .font_size(run.font_size)
            .hint(true)
            // `Glyph::x`/`y` are already absolute document coordinates, y down
            // — the same convention `vello_cpu` wants — so this is a field
            // rename and not a transform.
            .fill_glyphs(run.glyphs.iter().map(|g| VelloGlyph {
                id: g.id,
                x: g.x,
                y: g.y,
            }));
        Ok(())
    }

    /// [`DisplayItem::Rect`] — the fast path for the common case, a path for
    /// the two that are not.
    fn draw_rect(&mut self, item: &FilledRect) {
        self.ctx.set_paint(paint(item.brush));
        if item.corner_radius == 0.0 && item.rotation_deg == 0.0 {
            // `fill_rect`'s own fast path: axis-aligned transform, no skew, so
            // it never builds a path at all. This is the overwhelming majority
            // of the non-glyph list — every code-block border, every table cell
            // border, every blockquote bar.
            self.ctx.fill_rect(&to_kurbo(item.rect));
        } else {
            self.ctx.fill_path(&rounded_rotated_path(item));
        }
    }

    /// [`DisplayItem::Line`] — a stroke, because a CSS border style is not
    /// expressible as a fill.
    fn draw_line(&mut self, item: &StrokedLine, stroke: Stroke) {
        self.ctx.set_paint(paint(item.brush));
        self.ctx.set_stroke(stroke);
        self.ctx.stroke_path(&line_path(item));
    }
}

impl Renderer for VelloCpuRenderer {
    fn name(&self) -> &'static str {
        "vello_cpu"
    }

    fn render(
        &mut self,
        list: &DisplayList,
        frame: &Frame,
        target: &mut Pixels,
    ) -> Result<FrameStats, RenderError> {
        let mut stats = FrameStats {
            blocks_total: list.blocks.len(),
            ..FrameStats::default()
        };
        let (width, height) = (target.width(), target.height());
        if width == 0 || height == 0 {
            return Ok(stats);
        }
        self.ctx.reset_and_resize(width, height);

        // The ground, in device space, before the scroll translate — so a
        // viewport whose size disagrees with the target's still gets a fully
        // painted frame rather than a band of transparency.
        self.ctx.set_transform(Affine::IDENTITY);
        if frame.background.is_visible() {
            self.ctx.set_paint(paint(frame.background));
            self.ctx.fill_rect(&vello_cpu::kurbo::Rect::new(
                0.0,
                0.0,
                f64::from(width),
                f64::from(height),
            ));
        }

        // **D18's one derived coordinate, and the only one in this crate.**
        // Everything below is drawn at the coordinate already on it.
        self.ctx.set_transform(Affine::translate((
            -f64::from(frame.viewport.x),
            -f64::from(frame.viewport.y),
        )));

        for block in &list.blocks {
            if !is_visible_in(block, frame.viewport) {
                continue;
            }
            stats.blocks_drawn += 1;

            // See this module's doc for why this is a predicate and not an
            // unconditional push.
            //
            // **The rect is `clip`, not `bounds`, and that is S4's correction.**
            // `bounds` is the border box; muya's `overflow: auto` sits on a
            // child inside the padding, so the browser clips at the *content*
            // box. Clipping at `bounds` left `padding_right + border_width`
            // — 15.40 px under `muya-default` — painted over the block's own
            // right border. `mt-layout` hands the rect over precisely so that
            // this crate does not derive it (D18).
            let clip = block.clip;
            if clip.is_some() {
                stats.blocks_clipped += 1;
            }
            let mut clip_open = false;

            for item in &block.items {
                // The clip covers **content, not chrome**. The background and
                // the four borders belong to the outer box, and clipping them
                // to the content box would erase them outright; in this list
                // that distinction is exactly `Glyphs` against the rest, which
                // is a variant match and not a computed box.
                //
                // Toggling on contiguous runs costs one push and one pop for a
                // code block rather than one per glyph run, and is correct in
                // whatever order the items arrive — which matters, because
                // `items`' documented paint order is not one the corpus
                // strictly satisfies.
                let wants_clip = clip.is_some() && matches!(item, DisplayItem::Glyphs(_));
                if wants_clip != clip_open {
                    if let Some(rect) = clip.filter(|_| wants_clip) {
                        self.ctx
                            .push_clip_layer(&to_kurbo(rect).to_path(CURVE_TOLERANCE));
                    } else {
                        self.ctx.pop_layer();
                    }
                    clip_open = wants_clip;
                }
                match item {
                    DisplayItem::Glyphs(run) => {
                        if !run.brush.is_visible() || run.glyphs.is_empty() {
                            stats.items_skipped += 1;
                            continue;
                        }
                        // A missing face aborts the frame rather than leaving a
                        // hole in it — see `RenderError`'s own doc. The clip
                        // layer is left unpopped, which is why the next frame
                        // starts with `reset_and_resize`.
                        self.draw_glyphs(run)?;
                        stats.items_drawn += 1;
                    }
                    DisplayItem::Rect(rect) => {
                        if !rect.brush.is_visible() {
                            stats.items_skipped += 1;
                            continue;
                        }
                        self.draw_rect(rect);
                        stats.items_drawn += 1;
                    }
                    DisplayItem::Line(line) => match stroke_for(line) {
                        Some(stroke) if line.brush.is_visible() => {
                            self.draw_line(line, stroke);
                            stats.items_drawn += 1;
                        }
                        _ => stats.items_skipped += 1,
                    },
                    // M3 places these and draws nothing in them. See the module
                    // doc: the placeholder chrome is already on the list.
                    DisplayItem::InlineBox(_) => stats.items_skipped += 1,
                }
            }

            if clip_open {
                self.ctx.pop_layer();
            }
        }

        self.ctx.flush();
        let view = PixmapMut::new(width, height, target.data_mut())
            .expect("Pixels always holds exactly width × height × 4 bytes");
        self.ctx
            .render_with(view, &mut self.resources, self.rasterizer);
        Ok(stats)
    }
}

// ---------------------------------------------------------------------------
// The four conversions, and none of them is a placement decision
// ---------------------------------------------------------------------------

/// `mt_layout::Rect` → `kurbo::Rect`.
///
/// The display list is origin + size; kurbo is two corners. The `+` is
/// [`Rect::max_x`]/[`Rect::max_y`], which are the display list's own
/// accessors — the same arithmetic `mt-layout` does when it writes a golden.
fn to_kurbo(rect: Rect) -> vello_cpu::kurbo::Rect {
    vello_cpu::kurbo::Rect::new(
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.max_x()),
        f64::from(rect.max_y()),
    )
}

/// `mt_layout::Brush` → `vello_cpu`'s colour.
///
/// Both are 8-bit sRGB with 8-bit alpha, so this is a field rename with no
/// colour-space conversion in it. `mt-layout` defines its own brush because
/// parley's `Brush` is parley's own trait rather than `peniko::Brush` (C9);
/// this function is that decision's entire cost.
fn paint(brush: Brush) -> AlphaColor<Srgb> {
    AlphaColor::from_rgba8(brush.r, brush.g, brush.b, brush.a)
}

/// The path for a [`FilledRect`] that is rounded, rotated, or both.
///
/// `corner_radius` is the item's; the rotation's centre is
/// `kurbo::Rect::center()`, which is what
/// [`FilledRect::rotation_deg`]'s own doc names — *"clockwise, about the
/// rectangle's centre"*. Positive degrees read clockwise on screen because the
/// coordinate system is y-down, so no sign flip is needed and none is applied.
///
/// It is reached only by the two checkbox parts three shipped themes rotate
/// (−90° for the unchecked box, 45° for the check) and by every rounded fill:
/// the code-block ground's 3 px radius, inline code's, and the `min(w,h)/2`
/// that makes a `disc` bullet a circle.
fn rounded_rotated_path(item: &FilledRect) -> BezPath {
    let base = to_kurbo(item.rect);
    let mut path = if item.corner_radius > 0.0 {
        RoundedRect::from_rect(base, f64::from(item.corner_radius)).to_path(CURVE_TOLERANCE)
    } else {
        base.to_path(CURVE_TOLERANCE)
    };
    if item.rotation_deg != 0.0 {
        path.apply_affine(Affine::rotate_about(
            f64::from(item.rotation_deg).to_radians(),
            base.center(),
        ));
    }
    path
}

/// The two-point path a [`StrokedLine`] is.
fn line_path(line: &StrokedLine) -> BezPath {
    let mut path = BezPath::new();
    path.move_to((f64::from(line.x0), f64::from(line.y0)));
    path.line_to((f64::from(line.x1), f64::from(line.y1)));
    path
}

/// The stroke a [`StrokedLine`]'s style asks for, or `None` if it asks for
/// nothing.
///
/// **`LineStyle::None` draws nothing whatever `width` says** — the theme
/// model's own words (`theme.rs:1015-1023`), and the reason this returns an
/// `Option` rather than a `Stroke` with an unreachable arm. A non-positive
/// width is the same answer arrived at from the other side: CSS
/// `border-width: 0` is not a hairline.
///
/// Butt caps, because a CSS rule ends where its box ends; kurbo's default is
/// `Cap::Round`, which would make every 2 px `hr` one pixel wider at each end
/// than the geometry `mt-layout` computed.
fn stroke_for(line: &StrokedLine) -> Option<Stroke> {
    if matches!(line.style, LineStyle::None) || line.width <= 0.0 {
        return None;
    }
    let width = f64::from(line.width);
    let stroke = Stroke::new(width).with_caps(Cap::Butt);
    Some(match line.style {
        LineStyle::Solid => stroke,
        LineStyle::Dashed => stroke.with_dashes(0.0, [DASH_RATIO * width, DASH_RATIO * width]),
        LineStyle::Dotted => stroke.with_dashes(0.0, [width, width]),
        LineStyle::None => unreachable!("returned above"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FontData;
    use mt_layout::{
        BlockDisplay, BlockKind, DisplayList, Glyph, InlineBoxFlow, InlineBoxItem, StrokedLine,
    };

    const WHITE: Brush = Brush::rgb(255, 255, 255);
    const RED: Brush = Brush::rgb(255, 0, 0);
    const BLUE: Brush = Brush::rgb(0, 0, 255);

    /// A `NodeId` has no public constructor and none is wanted — it is an arena
    /// slot, and `mt-doc` is the only thing that may mint one. An empty
    /// document's root is the cheapest real id, and nothing here resolves it.
    fn a_node() -> mt_doc::NodeId {
        mt_doc::Document::new().root()
    }

    fn block(bounds: Rect, items: Vec<DisplayItem>) -> BlockDisplay {
        // `paint_bounds` is derived here the same way `mt-layout` derives it,
        // rather than being set to `bounds`. A helper that quietly made every
        // test block self-contained would make the culling tests pass for a
        // reason production never enjoys.
        let paint_bounds = items
            .iter()
            .fold(bounds, |acc, item| acc.union(item.extent()));
        BlockDisplay {
            node: a_node(),
            kind: BlockKind::Paragraph,
            bounds,
            language: None,
            overflow_x: 0.0,
            clip: None,
            paint_bounds,
            items,
            text_map: None,
        }
    }

    /// The face `assets/fonts/` ships, loaded into a one-entry table.
    fn mono_table() -> (FontTable, mt_layout::FontId) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/fonts/DejaVuSansMono.ttf");
        let bytes = std::fs::read(&path).expect("the bundled face is committed");
        let mut table = FontTable::new();
        let id = table.push(FontData::new(bytes.into(), 0));
        (table, id)
    }

    /// Glyph 40 in DejaVu Sans Mono is `A` — any id with ink would do.
    fn an_a(font: mt_layout::FontId, x: f32) -> GlyphRun {
        GlyphRun {
            font,
            font_size: 16.0,
            is_rtl: false,
            text_range: 0..1,
            baseline: 16.0,
            offset: x,
            advance: 9.6,
            brush: RED,
            glyphs: vec![Glyph { id: 40, x, y: 16.0 }],
        }
    }

    fn list(blocks: Vec<BlockDisplay>) -> DisplayList {
        DisplayList {
            width: 100.0,
            height: 100.0,
            blocks,
        }
    }

    fn frame(background: Brush) -> Frame {
        Frame {
            viewport: Rect::new(0.0, 0.0, 20.0, 20.0),
            background,
        }
    }

    /// Draw into a 20×20 buffer with a transparent ground, so that
    /// "was anything painted?" is a question about the items alone.
    fn draw(list: &DisplayList) -> (Pixels, FrameStats) {
        draw_at(list, frame(Brush::TRANSPARENT), 20, 20)
    }

    fn draw_at(list: &DisplayList, frame: Frame, w: u16, h: u16) -> (Pixels, FrameStats) {
        let mut renderer = VelloCpuRenderer::new(FontTable::new());
        let mut target = Pixels::new(w, h);
        let stats = renderer
            .render(list, &frame, &mut target)
            .expect("no glyph run, so no face can be missing");
        (target, stats)
    }

    // -- the trait --------------------------------------------------------

    #[test]
    fn the_backend_names_itself_and_says_it_is_single_threaded() {
        let renderer = VelloCpuRenderer::new(FontTable::new());
        assert_eq!(renderer.name(), "vello_cpu");
        assert_eq!(NUM_THREADS, 0, "D17 reports this beside every frame time");
        assert!(format!("{renderer:?}").contains("num_threads"));
    }

    // -- Rect -------------------------------------------------------------

    #[test]
    fn a_filled_rect_paints_its_own_pixels_and_no_others() {
        let item = DisplayItem::Rect(FilledRect::new(Rect::new(4.0, 4.0, 6.0, 6.0), RED));
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![item],
        )]));
        assert_eq!(stats.items_drawn, 1);
        assert_eq!(px.pixel(6, 6), [255, 0, 0, 255], "inside the rect");
        assert_eq!(px.pixel(2, 2), [0, 0, 0, 0], "outside it");
        assert_eq!(
            px.painted_pixels(),
            36,
            "a 6×6 axis-aligned fill is exactly 36 pixels — no antialiased fringe"
        );
    }

    #[test]
    fn a_transparent_brush_draws_nothing_at_all() {
        let item = DisplayItem::Rect(FilledRect::new(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            Brush::TRANSPARENT,
        ));
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![item],
        )]));
        assert!(px.is_blank());
        assert_eq!((stats.items_drawn, stats.items_skipped), (0, 1));
    }

    /// A radius rounds the corners off, so a rounded fill of the same rect
    /// covers strictly fewer pixels. Asserted as an inequality rather than a
    /// count because the corner's coverage is antialiased and a count would be
    /// asserting the rasterizer's filter rather than the geometry.
    #[test]
    fn a_corner_radius_removes_the_corner_pixels() {
        let square = FilledRect::new(Rect::new(2.0, 2.0, 16.0, 16.0), RED);
        let round = FilledRect {
            corner_radius: 8.0,
            ..square
        };
        let (flat, _) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Rect(square)],
        )]));
        let (curved, _) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Rect(round)],
        )]));
        assert_eq!(
            flat.pixel(2, 2),
            [255, 0, 0, 255],
            "square corner is filled"
        );
        assert_eq!(curved.pixel(2, 2), [0, 0, 0, 0], "round corner is not");
        assert!(curved.painted_pixels() < flat.painted_pixels());
    }

    /// The two checkbox parts three themes rotate are the only non-zero
    /// `rotation_deg` on the list, so this is the arm that carries them.
    #[test]
    fn a_rotation_moves_the_fill_without_changing_its_area() {
        let flat = FilledRect::new(Rect::new(6.0, 9.0, 8.0, 2.0), BLUE);
        let turned = FilledRect {
            rotation_deg: 90.0,
            ..flat
        };
        let (a, _) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Rect(flat)],
        )]));
        let (b, _) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Rect(turned)],
        )]));
        // Horizontal bar through the centre; the same bar stood on end.
        assert_eq!(a.pixel(7, 9), [0, 0, 255, 255]);
        assert_eq!(a.pixel(9, 6), [0, 0, 0, 0]);
        assert_eq!(b.pixel(9, 6), [0, 0, 255, 255]);
        assert_eq!(b.pixel(7, 9), [0, 0, 0, 0]);
        assert_eq!(a.painted_pixels(), b.painted_pixels());
    }

    // -- Line -------------------------------------------------------------

    #[test]
    fn a_solid_rule_paints_a_band_of_its_own_width() {
        let line = StrokedLine::horizontal(10.0, 2.0, 18.0, 2.0, RED);
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Line(line)],
        )]));
        assert_eq!(stats.items_drawn, 1);
        // Centred on y = 10 with width 2 ⇒ rows 9 and 10, columns 2..18.
        assert_eq!(px.pixel(10, 9), [255, 0, 0, 255]);
        assert_eq!(px.pixel(10, 10), [255, 0, 0, 255]);
        assert_eq!(px.pixel(10, 8), [0, 0, 0, 0]);
        assert_eq!(
            px.pixel(1, 9),
            [0, 0, 0, 0],
            "butt caps: the rule ends where x0 says, not half a width earlier"
        );
        assert_eq!(px.painted_pixels(), 32);
    }

    /// `theme.rs:1015-1023`: *"No line drawn at all, whatever the width says."*
    #[test]
    fn line_style_none_draws_nothing_whatever_the_width_says() {
        let line = StrokedLine {
            style: LineStyle::None,
            width: 8.0,
            ..StrokedLine::horizontal(10.0, 0.0, 20.0, 8.0, RED)
        };
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Line(line)],
        )]));
        assert!(px.is_blank());
        assert_eq!((stats.items_drawn, stats.items_skipped), (0, 1));
        assert!(stroke_for(&line).is_none());
    }

    #[test]
    fn an_invisible_stroke_draws_nothing_either() {
        let line = StrokedLine::horizontal(10.0, 0.0, 20.0, 4.0, Brush::TRANSPARENT);
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Line(line)],
        )]));
        assert!(px.is_blank());
        assert_eq!(stats.items_skipped, 1);
    }

    /// muya's own default `hr` is `2px dashed`, so this is the arm the corpus
    /// reaches. The property asserted is that a dash *is* a gap — a dashed rule
    /// paints strictly less than a solid one over the same segment.
    #[test]
    fn a_dashed_rule_leaves_gaps_and_a_solid_one_does_not() {
        let solid = StrokedLine::horizontal(10.0, 0.0, 20.0, 2.0, RED);
        let dashed = StrokedLine {
            style: LineStyle::Dashed,
            ..solid
        };
        let (a, _) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Line(solid)],
        )]));
        let (b, _) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Line(dashed)],
        )]));
        assert_eq!(a.painted_pixels(), 40, "20 px × 2 px, fully covered");
        assert!(b.painted_pixels() < a.painted_pixels());
        assert!(b.painted_pixels() > 0, "a dashed rule is still a rule");
    }

    // -- InlineBox --------------------------------------------------------

    /// M3 places these and draws nothing in them (`display.rs:574-579`). The
    /// placeholder chrome is on the list as `Rect` items and drawing anything
    /// here would draw it twice.
    #[test]
    fn an_inline_box_is_a_hole_and_stays_one() {
        let item = DisplayItem::InlineBox(InlineBoxItem {
            id: 7,
            x: 2.0,
            y: 2.0,
            width: 16.0,
            height: 16.0,
            baseline: None,
            flow: InlineBoxFlow::InFlow,
        });
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![item],
        )]));
        assert!(px.is_blank());
        assert_eq!((stats.items_drawn, stats.items_skipped), (0, 1));
    }

    // -- Glyphs -----------------------------------------------------------

    /// The seam's one failure: a run naming a face the table does not hold.
    /// It is an error rather than a hole, because a hole makes a well-formed
    /// frame.
    #[test]
    fn a_run_whose_face_is_missing_from_the_table_is_an_error_not_a_hole() {
        let run = GlyphRun {
            font: mt_layout::FontId::from_index(3),
            font_size: 16.0,
            is_rtl: false,
            text_range: 0..1,
            baseline: 10.0,
            offset: 0.0,
            advance: 8.0,
            brush: RED,
            glyphs: vec![Glyph {
                id: 42,
                x: 0.0,
                y: 10.0,
            }],
        };
        let mut renderer = VelloCpuRenderer::new(FontTable::new());
        let mut target = Pixels::new(20, 20);
        let err = renderer
            .render(
                &list(vec![block(
                    Rect::new(0.0, 0.0, 20.0, 20.0),
                    vec![DisplayItem::Glyphs(run)],
                )]),
                &frame(Brush::TRANSPARENT),
                &mut target,
            )
            .expect_err("the table is empty");
        assert_eq!(
            err,
            RenderError::UnknownFont {
                font: mt_layout::FontId::from_index(3),
                table_len: 0
            }
        );
    }

    /// A run with an invisible brush or no glyphs never reaches the table, so
    /// it cannot fail on a face it was never going to draw.
    #[test]
    fn an_invisible_or_empty_run_is_skipped_before_the_font_is_looked_up() {
        let base = GlyphRun {
            font: mt_layout::FontId::from_index(9),
            font_size: 16.0,
            is_rtl: false,
            text_range: 0..1,
            baseline: 10.0,
            offset: 0.0,
            advance: 8.0,
            brush: Brush::TRANSPARENT,
            glyphs: vec![Glyph {
                id: 42,
                x: 0.0,
                y: 10.0,
            }],
        };
        let empty = GlyphRun {
            brush: RED,
            glyphs: Vec::new(),
            ..base.clone()
        };
        let (px, stats) = draw(&list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Glyphs(base), DisplayItem::Glyphs(empty)],
        )]));
        assert!(px.is_blank());
        assert_eq!((stats.items_drawn, stats.items_skipped), (0, 2));
    }

    /// D16's hand-off, end to end: a real face, a real glyph id, and pixels.
    /// The face is one of the twelve `assets/fonts/` ships and is read here
    /// rather than fabricated, because a stub blob rasterizes to nothing and
    /// would make this test pass for the wrong reason.
    #[test]
    fn a_glyph_run_draws_through_the_table_the_shell_handed_over() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/fonts/DejaVuSansMono.ttf");
        let bytes = std::fs::read(&path).expect("the bundled face is committed");
        let mut table = FontTable::new();
        let id = table.push(FontData::new(bytes.into(), 0));

        // Glyph 40 in DejaVu Sans Mono is `A` — any id with ink would do; what
        // is being asserted is that the handle reached the rasterizer.
        let run = GlyphRun {
            font: id,
            font_size: 16.0,
            is_rtl: false,
            text_range: 0..1,
            baseline: 16.0,
            offset: 2.0,
            advance: 9.6,
            brush: RED,
            glyphs: vec![Glyph {
                id: 40,
                x: 2.0,
                y: 16.0,
            }],
        };
        let mut renderer = VelloCpuRenderer::new(table);
        let mut target = Pixels::new(20, 20);
        let stats = renderer
            .render(
                &list(vec![block(
                    Rect::new(0.0, 0.0, 20.0, 20.0),
                    vec![DisplayItem::Glyphs(run)],
                )]),
                &frame(Brush::TRANSPARENT),
                &mut target,
            )
            .expect("the face is in the table");
        assert_eq!(stats.items_drawn, 1);
        assert!(
            target.painted_pixels() > 10,
            "a 16 px glyph should cover more than a few pixels; got {}",
            target.painted_pixels()
        );
    }

    // -- the ground -------------------------------------------------------

    #[test]
    fn the_ground_covers_the_whole_target_and_a_transparent_one_covers_nothing() {
        let (opaque, _) = draw_at(&list(vec![]), frame(WHITE), 4, 3);
        assert_eq!(opaque.painted_pixels(), 12);
        assert_eq!(opaque.pixel(3, 2), [255, 255, 255, 255]);
        let (clear, _) = draw_at(&list(vec![]), frame(Brush::TRANSPARENT), 4, 3);
        assert!(clear.is_blank());
    }

    // -- culling and clipping ---------------------------------------------

    /// D18, through the counters rather than through pixels: the block above
    /// the viewport is skipped, the one straddling its top edge is not.
    #[test]
    fn culling_skips_a_block_outside_the_viewport_and_keeps_one_that_straddles_it() {
        let far = block(
            Rect::new(0.0, -500.0, 20.0, 10.0),
            vec![DisplayItem::Rect(FilledRect::new(
                Rect::new(0.0, -500.0, 20.0, 10.0),
                RED,
            ))],
        );
        let straddling = block(
            Rect::new(0.0, -5.0, 20.0, 10.0),
            vec![DisplayItem::Rect(FilledRect::new(
                Rect::new(0.0, -5.0, 20.0, 10.0),
                BLUE,
            ))],
        );
        let inside = block(
            Rect::new(0.0, 10.0, 20.0, 5.0),
            vec![DisplayItem::Rect(FilledRect::new(
                Rect::new(0.0, 10.0, 20.0, 5.0),
                BLUE,
            ))],
        );
        let (px, stats) = draw(&list(vec![far, straddling, inside]));
        assert_eq!(stats.blocks_total, 3);
        assert_eq!(stats.blocks_drawn, 2, "the far block never reaches an item");
        assert_eq!(stats.items_drawn, 2);
        assert_eq!(
            px.pixel(10, 0),
            [0, 0, 255, 255],
            "the straddling block's visible half is painted"
        );
        assert_eq!(px.pixel(10, 12), [0, 0, 255, 255]);
        assert!(
            px.data().chunks_exact(4).all(|p| p[0] == 0),
            "nothing red survived, so the culled block drew nothing"
        );
    }

    /// The scroll offset is the one coordinate this crate derives, so it gets
    /// its own assertion: the same list at two offsets paints the same rect at
    /// two places.
    #[test]
    fn the_scroll_offset_translates_the_frame_and_nothing_else_does() {
        let l = list(vec![block(
            Rect::new(0.0, 0.0, 20.0, 40.0),
            vec![DisplayItem::Rect(FilledRect::new(
                Rect::new(4.0, 20.0, 4.0, 4.0),
                RED,
            ))],
        )]);
        let top = Frame {
            viewport: Rect::new(0.0, 0.0, 20.0, 20.0),
            background: Brush::TRANSPARENT,
        };
        let scrolled = Frame {
            viewport: Rect::new(0.0, 20.0, 20.0, 20.0),
            background: Brush::TRANSPARENT,
        };
        let (a, _) = draw_at(&l, top, 20, 20);
        let (b, _) = draw_at(&l, scrolled, 20, 20);
        assert!(a.is_blank(), "at offset 0 the rect is below the viewport");
        assert_eq!(b.pixel(5, 1), [255, 0, 0, 255]);
        assert_eq!(b.painted_pixels(), 16);
    }

    /// **S4's correction, in the two halves that make it a correction.** A
    /// clipping block cuts its *content* at [`BlockDisplay::clip`] — the
    /// content box — and leaves its *chrome* alone. The old version of this
    /// test asserted the opposite of the second half: it clipped a `Rect` at
    /// `bounds` and called that the contract, which is how 15.40 px of text
    /// came to be painted over a border for a whole stage.
    #[test]
    fn a_clipping_block_cuts_its_glyphs_and_not_its_chrome() {
        let (table, font) = mono_table();

        // Chrome: a rect spanning the whole block, in a block whose clip is
        // half as wide. It must survive in full — a border lives on the outer
        // box and the browser never clips it.
        let chrome = DisplayItem::Rect(FilledRect::new(Rect::new(0.0, 4.0, 20.0, 4.0), RED));
        let mut b = block(Rect::new(0.0, 0.0, 20.0, 20.0), vec![chrome]);
        b.overflow_x = 10.0;
        b.clip = Some(Rect::new(0.0, 0.0, 10.0, 20.0));

        let mut renderer = VelloCpuRenderer::new(table);
        let mut target = Pixels::new(20, 20);
        let stats = renderer
            .render(&list(vec![b]), &frame(Brush::TRANSPARENT), &mut target)
            .expect("no glyphs");
        assert_eq!(stats.blocks_clipped, 1);
        assert_eq!(
            target.pixel(15, 5),
            [255, 0, 0, 255],
            "chrome is not clipped: this pixel is past the clip and must survive"
        );
        assert_eq!(target.painted_pixels(), 80);

        // Content: a glyph entirely to the right of the clip disappears.
        let (table, font2) = mono_table();
        assert_eq!(font, font2);
        let mut b = block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Glyphs(an_a(font2, 12.0))],
        );
        b.overflow_x = 10.0;
        b.clip = Some(Rect::new(0.0, 0.0, 10.0, 20.0));

        let mut renderer = VelloCpuRenderer::new(table);
        let mut target = Pixels::new(20, 20);
        renderer
            .render(&list(vec![b]), &frame(Brush::TRANSPARENT), &mut target)
            .expect("the face is in the table");
        assert_eq!(
            target.painted_pixels(),
            0,
            "a glyph wholly right of the clip is content, and content is cut"
        );
    }

    /// The control for the test above: the same glyph, no clip, really does ink.
    /// Without this, a clip that silently drew nothing at all would pass.
    #[test]
    fn the_same_glyph_unclipped_does_paint() {
        let (table, font) = mono_table();
        let b = block(
            Rect::new(0.0, 0.0, 20.0, 20.0),
            vec![DisplayItem::Glyphs(an_a(font, 12.0))],
        );
        let mut renderer = VelloCpuRenderer::new(table);
        let mut target = Pixels::new(20, 20);
        let stats = renderer
            .render(&list(vec![b]), &frame(Brush::TRANSPARENT), &mut target)
            .expect("the face is in the table");
        assert_eq!(stats.blocks_clipped, 0);
        assert!(target.painted_pixels() > 5);
    }

    // -- degenerate targets -------------------------------------------------

    #[test]
    fn a_zero_sized_target_renders_nothing_and_does_not_panic() {
        let (px, stats) = draw_at(
            &list(vec![block(
                Rect::new(0.0, 0.0, 20.0, 20.0),
                vec![DisplayItem::Rect(FilledRect::new(
                    Rect::new(0.0, 0.0, 20.0, 20.0),
                    RED,
                ))],
            )]),
            frame(WHITE),
            0,
            0,
        );
        assert_eq!(px.data().len(), 0);
        assert_eq!(stats.blocks_drawn, 0);
        assert_eq!(stats.blocks_total, 1);
    }

    /// The same renderer, two frames, two sizes — D17 times N repaints from one
    /// warm context and a context that could not be re-sized would make that a
    /// measurement of construction.
    #[test]
    fn one_renderer_draws_successive_frames_at_different_sizes() {
        let l = list(vec![block(
            Rect::new(0.0, 0.0, 40.0, 40.0),
            vec![DisplayItem::Rect(FilledRect::new(
                Rect::new(0.0, 0.0, 40.0, 40.0),
                RED,
            ))],
        )]);
        let f = Frame {
            viewport: Rect::new(0.0, 0.0, 40.0, 40.0),
            background: Brush::TRANSPARENT,
        };
        let mut renderer = VelloCpuRenderer::new(FontTable::new());
        let mut small = Pixels::new(4, 4);
        let mut large = Pixels::new(30, 30);
        renderer.render(&l, &f, &mut small).expect("first frame");
        renderer.render(&l, &f, &mut large).expect("second frame");
        renderer.render(&l, &f, &mut small).expect("third frame");
        assert_eq!(small.painted_pixels(), 16);
        assert_eq!(large.painted_pixels(), 900);
    }

    // -- PNG ---------------------------------------------------------------

    /// D19 leans on this: *"PNG encode and decode arrive at zero cost — `png`
    /// is a default feature of `vello_cpu` 0.2.0."* If it ever stops being one,
    /// this fails here rather than at the moment a golden is written.
    #[test]
    fn a_frame_encodes_to_a_png() {
        let (px, _) = draw_at(&list(vec![]), frame(WHITE), 4, 3);
        let png = px.to_png().expect("encode");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
