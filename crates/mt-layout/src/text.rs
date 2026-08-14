//! The one place parley and the display list meet — M3 §5 D8.
//!
//! Everything parley-shaped is confined to this module and to the private
//! halves of [`crate::fonts`]. [`ShapedText`] holds a `parley::Layout` in a
//! private field; [`ShapedText::emit`] performs the walk
//! `Layout → lines() → Line::items() → PositionedLayoutItem::{GlyphRun,
//! InlineBox}` and appends [`DisplayItem`]s. No public signature in this crate
//! names a parley type, which is the whole of D8's mechanical content.
//!
//! # What this module is not
//!
//! It is **not** block flow. It lays out one run of uniformly styled text —
//! one leaf's worth — and says how tall the result is. Stacking blocks,
//! margins, list indents, table columns and the theme's rectangles belong to
//! the block-flow phase, and the styled-run API that gives a leaf bold spans
//! and inline code belongs to S2. What is here is the primitive both of those
//! call, plus the walk that turns its output neutral.
//!
//! # "Nearly a rename" — audited
//!
//! D8 claims the neutral form is "very nearly a rename, not a translation",
//! citing the two spike renderers. Against D2's pin that is true of the run
//! itself and false in three specific places, each recorded where it bites:
//! [`crate::display::Glyph`] (parley's `Glyph` has a fourth field),
//! [`crate::display::InlineBoxItem`] (`baseline` is an `Option`, and there is
//! a seventh field of a parley enum type), and the font handle, which is D8's
//! own named exception and is resolved to a [`FontId`](crate::fonts::FontId)
//! here rather than carried.

use parley::{
    Alignment, AlignmentOptions, BaseDirection as ParleyBaseDirection, FontFamily, FontFamilyName,
    FontStyle, FontWeight, GenericFamily, InlineBox as ParleyInlineBox, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty,
};

use crate::display::{Brush, DisplayItem, Glyph, GlyphRun, InlineBoxFlow, InlineBoxItem};
use crate::fonts::{FontError, Fonts};
use crate::theme::TextAlign;

/// The paragraph's default writing direction.
///
/// The neutral form of parley's `BaseDirection`. It is here because D2 pinned
/// `main` partly for `set_base_direction` (#708): a document whose base
/// direction is RTL cannot be laid out correctly without it, and 0.11.0 has no
/// such call.
///
/// The variant this crate actually uses is [`Ltr`](Self::Ltr) — see
/// [`TextRequest::new`] for why, and for the reference evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaseDirection {
    /// Infer from the first strong character — the CSS `dir="auto"` rule.
    ///
    /// **Not what MarkText does.** Its editor sets an explicit `dir` from a
    /// preference whose enum is `["ltr", "rtl"]` with no `auto` at all, so this
    /// variant reproduces nothing the reference can produce. It is carried
    /// because it is a real parley setting and M5 may expose it; nothing in
    /// `mt-layout` selects it.
    #[default]
    Auto,
    /// Left to right.
    Ltr,
    /// Right to left.
    Rtl,
}

/// A hole to reserve in the text for a replaced element.
///
/// The **input** half of [`InlineBoxItem`]; that type is the output half. The
/// neutral form of parley's `InlineBox`, minus its `kind`: `mt-layout` places
/// only in-flow boxes, because nothing in markdown floats, and
/// [`InlineBoxFlow`] carries the other two variants on the way out for the
/// reason it gives.
///
/// M3 places these and draws nothing in them — see [`InlineBoxItem`] for who
/// owns the drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineBoxSpec {
    /// An identifier of the caller's choosing, echoed back on
    /// [`InlineBoxItem::id`] so a box in the output can be matched to the token
    /// that produced it without a side table.
    pub id: u64,
    /// Byte offset into [`TextRequest::text`] — the **visible** string, not the
    /// block's text — at which the box sits.
    ///
    /// Must be a `char` boundary. parley says so and does not check it.
    pub index: usize,
    /// Width in px.
    pub width: f32,
    /// Height in px.
    pub height: f32,
    /// Baseline relative to the box's top edge, or `None` to align the box's
    /// **bottom edge** to the text baseline.
    ///
    /// **Asserted rather than assumed.** parley resolves this as
    /// `baseline.unwrap_or(height)` and places the box at
    /// `line.baseline − resolved`, which is exactly what
    /// [`InlineBoxItem::baseline`]'s doc comment claims and had never been
    /// executed here. See the four tests below and
    /// `an_inline_box_is_aligned_to_the_text_baseline_and_not_to_the_line_top`
    /// in `tests/layout.rs`. The line's own ascent grows to fit whichever of
    /// the two the caller chose, so a tall box pushes the line down rather than
    /// overlapping the line above.
    pub baseline: Option<f32>,
}

/// One leaf's worth of uniformly styled text, and how to lay it out.
///
/// Uniform on purpose: S2 replaces the single style with runs derived from
/// `mt-inline`'s token tree, and the fields here are the ones that will become
/// per-run then. Nothing else about the request changes.
#[derive(Debug, Clone)]
pub struct TextRequest<'a> {
    /// The text to lay out. Byte offsets in the output are into **this**
    /// string.
    pub text: &'a str,
    /// The font stack, in fallback order. CSS generic names (`sans-serif`,
    /// `monospace`, `emoji`) are recognised and resolved through the
    /// collection's generic families — which is why [`Fonts::wire`] is not
    /// optional.
    ///
    /// [`Fonts::wire`]: crate::fonts::Fonts::wire
    pub families: &'a [String],
    /// Font size in px.
    pub font_size: f32,
    /// Line height as a multiple of the font size — CSS's unitless
    /// `line-height`, which is what `[metrics] line_height` in a theme is.
    pub line_height: f32,
    /// CSS weight. 400 for body, 700 for a bold heading.
    pub weight: u16,
    /// Whether to ask for an italic face.
    pub italic: bool,
    /// The column to wrap at, or `None` for a single unwrapped line.
    pub max_width: Option<f32>,
    /// Horizontal alignment within `max_width`.
    pub align: TextAlign,
    /// The paragraph's base direction.
    pub base_direction: BaseDirection,
    /// The colour to paint the glyphs.
    pub brush: Brush,
    /// Extra advance after every cluster, in px. CSS `letter-spacing`.
    ///
    /// Zero everywhere except the code block's line-number gutter, whose
    /// `letter-spacing: -1px` (`blockSyntax.css:336`) is a real theme field and
    /// would otherwise have no consumer.
    pub letter_spacing: f32,
    /// Holes to reserve in the line for replaced elements, in any order.
    ///
    /// Empty for every caller today: this is the plumbing half of D12, and the
    /// producer that fills it — an image with a size supplied from above the
    /// seam — is the other half. It is here first and separately because the
    /// alignment behaviour it depends on is the one thing D2 moved the parley
    /// pin for and had never been executed, let alone asserted.
    pub inline_boxes: &'a [InlineBoxSpec],
}

impl<'a> TextRequest<'a> {
    /// A request with the theme's body defaults, which is what every caller
    /// starts from and then overrides two fields of.
    ///
    /// # The base direction defaults to `Ltr`, not `Auto`, and that is deliberate
    ///
    /// [`BaseDirection::Auto`] is the CSS `dir="auto"` rule — infer from the
    /// first strong character — and it is *not* what the reference does. The
    /// desktop editor sets `dir` explicitly from a user preference
    /// (`packages/desktop/src/renderer/src/components/editorWithTabs/editor.vue:5`,
    /// `:dir="textDirection"`), and that preference is
    /// `{"enum": ["ltr", "rtl"], "default": "ltr"}`
    /// (`packages/desktop/src/main/preferences/schema.json:200-204`). There is
    /// no `auto` in the enum at all. So a default install lays **every**
    /// paragraph out at base level 0 — a Hebrew paragraph is left-aligned in
    /// MarkText, and `Auto` would right-align it.
    ///
    /// Parity is the only thing a golden can be checked against, and `Auto` is
    /// a divergence nobody chose. **Bidi correctness is unaffected**: the UBA
    /// still reorders runs within each line, which is what the gate's "bidi
    /// renders correctly" clause is about; only the paragraph's *base* level
    /// and hence its alignment change.
    ///
    /// The field stays on the request rather than being hard-coded because D2
    /// pinned parley partly for `set_base_direction` (#708). That is the
    /// mechanism by which M5 exposes the same two-valued setting the reference
    /// has, as a document or theme property, instead of inferring one.
    pub fn new(text: &'a str, families: &'a [String], font_size: f32, line_height: f32) -> Self {
        Self {
            text,
            families,
            font_size,
            line_height,
            weight: 400,
            italic: false,
            max_width: None,
            align: TextAlign::Start,
            base_direction: BaseDirection::Ltr,
            brush: Brush::default(),
            letter_spacing: 0.0,
            inline_boxes: &[],
        }
    }
}

/// Reusable scratch for laying text out.
///
/// Holds parley's `LayoutContext`, which caches shaping plans and style
/// resolution across calls. One per thread; D9 makes laziness load-bearing and
/// S6 makes layout parallel, so this is deliberately a value the caller owns
/// rather than a global.
pub struct TextShaper {
    cx: LayoutContext<Brush>,
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TextShaper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextShaper").finish_non_exhaustive()
    }
}

impl TextShaper {
    /// A fresh shaper.
    pub fn new() -> TextShaper {
        TextShaper {
            cx: LayoutContext::new(),
        }
    }

    /// Shape and break one request.
    pub fn shape(&mut self, fonts: &mut Fonts, request: &TextRequest<'_>) -> ShapedText {
        let families: Vec<FontFamilyName<'static>> = request
            .families
            .iter()
            .map(|name| match GenericFamily::parse(name) {
                Some(generic) => FontFamilyName::Generic(generic),
                None => FontFamilyName::Named(name.clone().into()),
            })
            .collect();

        let mut builder = self
            .cx
            .ranged_builder(fonts.context_mut(), request.text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(families.into())));
        builder.push_default(StyleProperty::FontSize(request.font_size));
        // `FontSizeRelative` and not `MetricsRelative`: CSS's unitless
        // `line-height` multiplies the font size, and `--mu-line-height: 1.6`
        // is a unitless number. Using the font's own metrics instead would
        // make line spacing depend on which face won fallback, which is
        // exactly the machine-dependence D7 removed from the font set.
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            request.line_height,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            request.weight,
        ))));
        builder.push_default(StyleProperty::FontStyle(if request.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        }));
        builder.push_default(StyleProperty::Brush(request.brush));
        if request.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(request.letter_spacing));
        }
        builder.set_base_direction(match request.base_direction {
            BaseDirection::Auto => ParleyBaseDirection::Auto,
            BaseDirection::Ltr => ParleyBaseDirection::Ltr,
            BaseDirection::Rtl => ParleyBaseDirection::Rtl,
        });
        for spec in request.inline_boxes {
            builder.push_inline_box(ParleyInlineBox {
                id: spec.id,
                // `mt-layout` pushes only in-flow boxes. See `InlineBoxSpec`.
                kind: parley::InlineBoxKind::InFlow,
                index: spec.index,
                width: spec.width,
                height: spec.height,
                baseline: spec.baseline,
            });
        }

        let mut layout = builder.build(request.text);
        layout.break_all_lines(request.max_width);
        layout.align(
            match request.align {
                TextAlign::Start => Alignment::Start,
                TextAlign::Center => Alignment::Center,
                TextAlign::End => Alignment::End,
            },
            AlignmentOptions::default(),
        );
        ShapedText { layout }
    }
}

/// One laid-out run of text.
///
/// **The only place in the crate that stores a parley type**, and it is
/// private. Kept rather than immediately flattened because D9 makes laziness
/// load-bearing — a width change re-breaks lines without re-shaping — and
/// because M4's hit-testing, if it needs cluster-level behaviour, grows a
/// *method* here rather than an exposed `Layout` (D8).
pub struct ShapedText {
    layout: Layout<Brush>,
}

impl std::fmt::Debug for ShapedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShapedText")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("lines", &self.line_count())
            .finish()
    }
}

impl ShapedText {
    /// The widest line's advance. Not the column it was broken at.
    pub fn width(&self) -> f32 {
        self.layout.width()
    }

    /// Total height of every line.
    pub fn height(&self) -> f32 {
        self.layout.height()
    }

    /// How many lines the text broke into.
    pub fn line_count(&self) -> usize {
        self.layout.lines().count()
    }

    /// Byte offset of every line start after the first.
    ///
    /// The break positions, in the form S2's line-break assertions and the
    /// `complex-scripts` comparison are both written in.
    pub fn break_offsets(&self) -> Vec<usize> {
        self.layout
            .lines()
            .skip(1)
            .map(|line| line.text_range().start)
            .collect()
    }

    /// The first line's baseline, or `0.0` if there are no lines.
    ///
    /// Block flow needs it to place a block's first line against the previous
    /// block's margin.
    pub fn first_baseline(&self) -> f32 {
        self.layout
            .lines()
            .next()
            .map(|line| line.metrics().baseline)
            .unwrap_or(0.0)
    }

    /// Re-break at a new column without re-shaping.
    ///
    /// C5 measured that a width change is cheap and content is not; this is
    /// the call that makes that true.
    pub fn rebreak(&mut self, max_width: Option<f32>, align: TextAlign) {
        self.layout.break_all_lines(max_width);
        self.layout.align(
            match align {
                TextAlign::Start => Alignment::Start,
                TextAlign::Center => Alignment::Center,
                TextAlign::End => Alignment::End,
            },
            AlignmentOptions::default(),
        );
    }

    /// **The walk.** Append this text's positioned items, translated so the
    /// layout's origin lands at `(origin_x, origin_y)`.
    ///
    /// This is the function D8 is about: everything parley yields is either
    /// copied as a plain number or, in the single case of the font handle,
    /// resolved to a [`FontId`](crate::fonts::FontId).
    ///
    /// # Errors
    ///
    /// [`FontError::UnresolvedFont`] if a run's face is not one `fonts`
    /// registered — which can only happen if this text was shaped against a
    /// different collection. It is an error rather than a skipped run because
    /// a silently dropped run is invisible text, and invisible text is the
    /// failure mode this whole seam is built to make impossible.
    pub fn emit(
        &self,
        fonts: &Fonts,
        origin_x: f32,
        origin_y: f32,
        out: &mut Vec<DisplayItem>,
    ) -> Result<(), FontError> {
        for line in self.layout.lines() {
            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(gr) => {
                        let run = gr.run();
                        // `Run::font()` returns `&FontInstance` on `main`
                        // (it was `&FontData` on 0.11.0 — D2's breaking
                        // change), and `.font` is the `FontData` handle E2
                        // feeds straight into `vello_cpu`. This is the one
                        // field of the walk that is not already a plain
                        // number, and this line is the whole cost of D8.
                        let handle = &run.font().font;
                        let font =
                            fonts
                                .resolve(handle)
                                .ok_or_else(|| FontError::UnresolvedFont {
                                    text_range: run.text_range(),
                                })?;
                        let glyphs = gr
                            .positioned_glyphs()
                            .map(|g| Glyph {
                                id: g.id,
                                x: g.x + origin_x,
                                y: g.y + origin_y,
                            })
                            .collect();
                        out.push(DisplayItem::Glyphs(GlyphRun {
                            font,
                            font_size: run.font_size(),
                            is_rtl: run.is_rtl(),
                            text_range: run.text_range(),
                            baseline: gr.baseline() + origin_y,
                            offset: gr.offset() + origin_x,
                            advance: gr.advance(),
                            brush: gr.style().brush,
                            glyphs,
                        }));
                    }
                    PositionedLayoutItem::InlineBox(b) => {
                        out.push(DisplayItem::InlineBox(InlineBoxItem {
                            id: b.id,
                            x: b.x + origin_x,
                            y: b.y + origin_y,
                            width: b.width,
                            height: b.height,
                            // Left as `Option`, not flattened: `None` means
                            // "align my bottom edge to the text baseline",
                            // which is an instruction and not a missing value.
                            baseline: b.baseline,
                            flow: match b.kind {
                                parley::InlineBoxKind::InFlow => InlineBoxFlow::InFlow,
                                parley::InlineBoxKind::OutOfFlow => InlineBoxFlow::OutOfFlow,
                                parley::InlineBoxKind::CustomOutOfFlow => {
                                    InlineBoxFlow::CustomOutOfFlow
                                }
                            },
                        }));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // No fonts can be registered here — that needs the filesystem, and
    // `cargo xtask deps` forbids filesystem calls anywhere under this crate's
    // `src/`. Everything that needs real faces is in `tests/fonts.rs`, which
    // is not scanned. What is testable without bytes is the empty case, and
    // it is worth testing: a collection with nothing in it must not panic.

    #[test]
    fn shaping_against_an_empty_collection_yields_nothing_and_does_not_panic() {
        let mut fonts = Fonts::new();
        let mut shaper = TextShaper::new();
        let families = vec!["Open Sans".to_string()];
        let request = TextRequest::new("hello", &families, 16.0, 1.6);
        let shaped = shaper.shape(&mut fonts, &request);
        let mut out = Vec::new();
        // With no registered face there is no run to resolve, so this is the
        // empty walk rather than an unresolved one.
        shaped.emit(&fonts, 0.0, 0.0, &mut out).unwrap();
        assert!(out.iter().all(|i| !matches!(i, DisplayItem::Glyphs(_))));
    }

    #[test]
    fn a_default_request_carries_the_body_defaults() {
        let families = vec!["Open Sans".to_string(), "sans-serif".to_string()];
        let request = TextRequest::new("x", &families, 16.0, 1.6);
        assert_eq!(request.weight, 400);
        assert!(!request.italic);
        assert_eq!(request.max_width, None);
        assert_eq!(request.align, TextAlign::Start);
        // Ltr and not Auto — reference parity; see `TextRequest::new`.
        assert_eq!(request.base_direction, BaseDirection::Ltr);
        assert_eq!(request.brush, Brush::default());
        assert_eq!(request.letter_spacing, 0.0);
        assert!(request.inline_boxes.is_empty());
    }

    // --- `InlineBox::baseline` ------------------------------------------
    //
    // D2 moved the parley pin partly for this field — 0.11.0 has no
    // `InlineBox::baseline` and cannot align a box — and until now nothing in
    // this crate had ever called `push_inline_box`, so the
    // `PositionedLayoutItem::InlineBox` arm of the walk had never executed and
    // [`InlineBoxItem::baseline`]'s documented meaning had never been checked
    // against parley. The four tests below are that check, and they assert the
    // *placement*, not just that a box comes back.
    //
    // **No face can be registered here**, so a line's ascent comes entirely
    // from the boxes on it. Each test therefore puts a **second, taller box**
    // on the line, so that the box under test is placed against an ascent it
    // did not set itself and a `y` of zero cannot pass by accident.
    // `tests/layout.rs` carries the same two assertions against a real face,
    // where the ascent comes from the text instead.

    /// Shape one line of boxes and hand back the emitted items in push order
    /// together with the line's baseline.
    fn place_boxes(boxes: &[InlineBoxSpec]) -> (Vec<InlineBoxItem>, f32) {
        let mut fonts = Fonts::new();
        let mut shaper = TextShaper::new();
        let families = vec!["Open Sans".to_string()];
        // The text is empty: with no registered face it can contribute no
        // glyphs and no metrics either way, and an empty string keeps every
        // box's `index` trivially in bounds and on a `char` boundary.
        let mut request = TextRequest::new("", &families, 16.0, 1.6);
        request.inline_boxes = boxes;
        let shaped = shaper.shape(&mut fonts, &request);
        let mut out = Vec::new();
        shaped.emit(&fonts, 0.0, 0.0, &mut out).unwrap();
        let items = out
            .into_iter()
            .map(|item| match item {
                DisplayItem::InlineBox(b) => b,
                other => panic!("expected only inline boxes, got {other:?}"),
            })
            .collect();
        (items, shaped.first_baseline())
    }

    #[test]
    fn an_inline_box_with_a_baseline_puts_that_offset_on_the_lines_baseline() {
        // Three baselines including 0.0 — a box hanging entirely *below* the
        // baseline, which is the degenerate case an `unwrap_or` bug would
        // pass — plus a mid-box baseline and a bottom-edge one. The 40 px box
        // is what sets the line's ascent, so none of the three is placed at
        // `y == 0` and each `y` is a different number.
        let boxes = [
            InlineBoxSpec {
                id: 1,
                index: 0,
                width: 10.0,
                height: 20.0,
                baseline: Some(0.0),
            },
            InlineBoxSpec {
                id: 2,
                index: 0,
                width: 10.0,
                height: 20.0,
                baseline: Some(8.0),
            },
            InlineBoxSpec {
                id: 3,
                index: 0,
                width: 10.0,
                height: 20.0,
                baseline: Some(20.0),
            },
            InlineBoxSpec {
                id: 4,
                index: 0,
                width: 10.0,
                height: 40.0,
                baseline: Some(30.0),
            },
        ];
        let (items, baseline) = place_boxes(&boxes);
        assert_eq!(items.len(), 4);
        assert_eq!(baseline, 30.0, "the tallest ascent on the line wins");
        for (item, spec) in items.iter().zip(boxes.iter()) {
            let b = spec.baseline.expect("every box here declares one");
            assert_eq!(item.baseline, spec.baseline, "id {} round-trips", spec.id);
            assert_eq!(
                item.y + b,
                baseline,
                "id {}: y {} plus baseline {b} must land on the line's baseline",
                spec.id,
                item.y,
            );
        }
        // Stated as literals as well, so a change to the rule cannot be
        // absorbed by the loop's own arithmetic.
        assert_eq!(items[0].y, 30.0);
        assert_eq!(items[1].y, 22.0);
        assert_eq!(items[2].y, 10.0);
        assert_eq!(items[3].y, 0.0);
    }

    #[test]
    fn an_inline_box_without_a_baseline_sits_its_bottom_edge_on_the_baseline() {
        // `display.rs` documents `None` as "align my bottom edge to the text
        // baseline". This proves it rather than trusting the comment: parley
        // resolves `baseline.unwrap_or(height)`, so `y + height` — not `y` —
        // is the baseline.
        let boxes = [
            InlineBoxSpec {
                id: 1,
                index: 0,
                width: 10.0,
                height: 20.0,
                baseline: None,
            },
            InlineBoxSpec {
                id: 2,
                index: 0,
                width: 10.0,
                height: 40.0,
                baseline: Some(30.0),
            },
        ];
        let (items, baseline) = place_boxes(&boxes);
        assert_eq!(baseline, 30.0);
        assert_eq!(items[0].baseline, None, "`None` is carried, not flattened");
        assert_eq!(items[0].y + items[0].height, baseline);
        assert_eq!(items[0].y, 10.0, "and it is *not* placed at the line top");
    }

    #[test]
    fn a_box_taller_than_the_line_grows_the_line_box() {
        // The short box alone fixes the line at 20 px. Adding a box that needs
        // 60 above the baseline and 40 below must grow the line to 100 and
        // move the baseline down, rather than letting the tall box overflow.
        let short = [InlineBoxSpec {
            id: 1,
            index: 0,
            width: 10.0,
            height: 20.0,
            baseline: Some(15.0),
        }];
        let (small, small_baseline) = place_boxes(&short);
        assert_eq!(small_baseline, 15.0);
        assert_eq!(small[0].y, 0.0);

        let tall = [
            short[0],
            InlineBoxSpec {
                id: 2,
                index: 0,
                width: 10.0,
                height: 100.0,
                baseline: Some(60.0),
            },
        ];
        let (grown, grown_baseline) = place_boxes(&tall);
        assert_eq!(grown_baseline, 60.0, "the line's ascent grew to 60");
        // The short box is pushed down by exactly the ascent it gained, and
        // still has its own baseline on the line's.
        assert_eq!(grown[0].y, 45.0);
        assert_eq!(grown[0].y + 15.0, grown_baseline);
        assert_eq!(grown[1].y, 0.0);
    }

    #[test]
    fn an_inline_box_is_translated_by_the_walks_origin_like_everything_else() {
        // The walk adds `(origin_x, origin_y)` to glyph positions; the box arm
        // must do the same, or a block's images would ignore its `content_y`.
        let boxes = [InlineBoxSpec {
            id: 9,
            index: 0,
            width: 12.0,
            height: 20.0,
            baseline: Some(16.0),
        }];
        let mut fonts = Fonts::new();
        let mut shaper = TextShaper::new();
        let families = vec!["Open Sans".to_string()];
        let mut request = TextRequest::new("", &families, 16.0, 1.6);
        request.inline_boxes = &boxes;
        let shaped = shaper.shape(&mut fonts, &request);
        let mut out = Vec::new();
        shaped.emit(&fonts, 100.0, 250.0, &mut out).unwrap();
        let DisplayItem::InlineBox(item) = &out[0] else {
            panic!("expected an inline box, got {:?}", out[0]);
        };
        assert_eq!(item.id, 9);
        assert_eq!(item.flow, InlineBoxFlow::InFlow);
        assert_eq!(item.x, 100.0);
        assert_eq!(item.y, 250.0 + (shaped.first_baseline() - 16.0));
        assert_eq!(
            item.rect(),
            crate::display::Rect::new(100.0, 250.0, 12.0, 20.0)
        );
    }
}
