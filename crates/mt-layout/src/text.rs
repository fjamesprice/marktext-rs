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
    FontStyle, FontWeight, GenericFamily, Layout, LayoutContext, LineHeight, PositionedLayoutItem,
    StyleProperty,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaseDirection {
    /// Infer from the first strong character — the CSS `dir="auto"` rule, and
    /// what a markdown document with no other signal should get.
    #[default]
    Auto,
    /// Left to right.
    Ltr,
    /// Right to left.
    Rtl,
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
}

impl<'a> TextRequest<'a> {
    /// A request with the theme's body defaults, which is what every caller
    /// starts from and then overrides two fields of.
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
            base_direction: BaseDirection::Auto,
            brush: Brush::default(),
            letter_spacing: 0.0,
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
        assert_eq!(request.base_direction, BaseDirection::Auto);
        assert_eq!(request.brush, Brush::default());
        assert_eq!(request.letter_spacing, 0.0);
    }
}
