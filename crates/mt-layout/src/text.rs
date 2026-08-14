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
//! It is **not** block flow, and it is **not** the tokenizer. It lays out one
//! leaf's worth of text — as one style or as N, and with holes reserved for
//! replaced elements — and says how tall the result is. Stacking blocks,
//! margins, list indents, table columns and the theme's rectangles belong to
//! [`crate::flow`]; deciding *which* bytes are visible and which spans are bold
//! belongs to [`crate::inline`], which owns no theme and produces no
//! geometry. This module is the primitive all three call, plus the walk that
//! turns its output neutral.
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

use crate::display::{
    Brush, DisplayItem, FilledRect, Glyph, GlyphRun, InlineBoxFlow, InlineBoxItem, Rect,
};
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

/// One stretch of a request's text at a style of its own.
///
/// **Fully resolved rather than a delta**, and the runs a request carries are
/// non-overlapping and in order. Nesting is therefore already flattened by the
/// time parley sees it — bold inside italic arrives as one run that is both,
/// rather than as two overlapping pushes composed by the style builder — which
/// is what makes the resolved style a thing a test can name.
///
/// Every field is the same shape as the [`TextRequest`] field it overrides, and
/// a run whose field equals the request's is not pushed at all. That matters
/// for more than speed: a `push` that repeats the default still ends the
/// previous style run, and a style-run boundary is a shaping boundary, so a
/// needless push costs a kerning pair.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRun<'a> {
    /// Byte range in [`TextRequest::text`].
    pub range: std::ops::Range<usize>,
    /// The font stack for this run — the theme's inline-code stack for a code
    /// span, and the request's own for everything else.
    pub families: &'a [String],
    /// Font size in px, already resolved against whatever `em` the theme
    /// expressed it in.
    pub font_size: f32,
    /// CSS weight.
    pub weight: u16,
    /// Whether to ask for an italic face.
    pub italic: bool,
    /// The colour to paint this run's glyphs.
    pub brush: Brush,
    /// Draw a line under this run, at the **face's own** underline position
    /// and thickness.
    ///
    /// A link, and nothing else. muya has no `text-decoration` rule for `<a>`
    /// anywhere — `a.mu-inline-rule` (`inlineSyntax.css:33-35`) sets colour
    /// alone — so the underline is the browser's UA sheet, and a UA underline
    /// is positioned from the font's `post` table rather than from a
    /// stylesheet. Following the same mechanism rather than inventing two theme
    /// numbers is what makes this faithful; see [`ShapedText::emit`] for where
    /// the metrics are read.
    pub underline: bool,
    /// Draw a line through this run, at the face's own strikeout position and
    /// thickness.
    ///
    /// `<del>`, and nothing else. `grep line-through` over muya's stylesheets
    /// returns **zero hits**, so this too is the UA sheet reading the font —
    /// `OS/2`'s `yStrikeoutPosition` and `yStrikeoutSize` this time.
    pub strikethrough: bool,
}

/// A ground to paint **behind** a stretch of the text.
///
/// Inline code's background (`inlineSyntax.css:60-70`) and the footnote
/// identifier's pill (`:599-609`), which are the same shape: a rounded rect
/// under a run, sized from the run's own font metrics plus a padding expressed
/// in that run's `em`.
///
/// It is a request-level concept rather than a [`StyleRun`] field because it is
/// **not** a text style: it changes no glyph, it must be emitted before the
/// glyphs it sits under, and it fragments across a line break the way a CSS
/// inline box does — see [`ShapedText::emit`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextGround {
    /// Byte range in [`TextRequest::text`].
    pub range: std::ops::Range<usize>,
    /// Horizontal padding in px, applied at the true start and true end of the
    /// range only — CSS's `box-decoration-break: slice`, which is the default.
    pub padding_x: f32,
    /// Vertical padding in px, applied on every fragment.
    pub padding_y: f32,
    /// Corner radius in px.
    pub corner_radius: f32,
    /// The fill.
    pub brush: Brush,
}

/// One leaf's worth of text, and how to lay it out.
///
/// The scalar style fields are the **defaults**: they apply to any byte no
/// [`runs`](Self::runs) entry covers, and to the whole string when `runs` is
/// empty. An empty `runs` is not a degenerate case but the common one — a
/// paragraph with no inline markup, a code block, a list marker, a
/// line-number gutter.
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
    /// Stretches of [`text`](Self::text) that differ from the scalar defaults
    /// above, non-overlapping and in ascending order.
    ///
    /// Empty means "the whole string at the defaults", which is what every
    /// caller that is not laying out inline markdown passes.
    pub runs: &'a [StyleRun<'a>],
    /// Holes to reserve in the line for replaced elements, in any order.
    ///
    /// Filled by [`crate::flow`] for every inline image — **D12**. Empty for
    /// every other caller, which is most of them.
    pub inline_boxes: &'a [InlineBoxSpec],
    /// Rounded rectangles to paint behind stretches of the text, in any order.
    ///
    /// Copied into the [`ShapedText`] rather than resolved here, because their
    /// geometry depends on where the lines fell and a re-break moves them.
    pub grounds: &'a [TextGround],
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
            runs: &[],
            inline_boxes: &[],
            grounds: &[],
        }
    }
}

/// The byte range of **one** glyph run, which is not what parley's
/// `Run::text_range()` answers.
///
/// # The defect this exists for
///
/// parley's `GlyphRun` is *"a sequence of fully positioned glyphs with the same
/// style"* and its `run()` is the enclosing **shaped** run — one font, one bidi
/// level — which a style change does not split. So a paragraph reading
/// `See [the plan](…) and …` yields three `GlyphRun`s (plain, link, plain) that
/// all share one `Run`, and `Run::text_range()` answers all three with the
/// whole item's range. Before S2 there was one style per leaf, run and item
/// were the same thing, and the range was right by accident; with markers
/// hidden and styles per token it is a superset, and
/// [`GlyphRun::text_range`](crate::GlyphRun::text_range) claims to be the
/// range of *these* glyphs — the thing M4 hit-tests and D13's map converts.
///
/// # How it is recovered
///
/// parley groups a run's glyphs into `GlyphRun`s by walking
/// `visual_clusters()` and cutting where the style index changes, so a glyph
/// index within the line item identifies the cluster it came from. This walks
/// the same clusters, accumulating glyph counts, and unions the byte ranges of
/// those falling in `glyph_start .. glyph_start + glyph_count`. A cluster that
/// contributes **no** glyph — a default-ignorable such as ZWJ, VS16 or the RLM
/// of [`crate::RTL_MARK`] — is invisible to parley's grouping, so it is
/// attributed to the group its index falls inside, which keeps the ranges
/// tiling instead of leaving its bytes in neither.
fn glyph_run_text_range(
    run: &parley::Run<'_, Brush>,
    glyph_start: usize,
    glyph_count: usize,
) -> std::ops::Range<usize> {
    let end = glyph_start + glyph_count;
    let mut at = 0usize;
    let mut lo = usize::MAX;
    let mut hi = 0usize;
    for cluster in run.visual_clusters() {
        let n = cluster.glyphs().count();
        // `n.max(1)` for the overlap test only: a zero-glyph cluster occupies
        // the slot it sits at without advancing past it.
        if at < end && at + n.max(1) > glyph_start {
            let range = cluster.text_range();
            lo = lo.min(range.start);
            hi = hi.max(range.end);
        }
        at += n;
    }
    if lo == usize::MAX {
        // No cluster claimed a glyph — an empty run. The item's own range is
        // the only answer there is, and it is empty too.
        run.text_range()
    } else {
        lo..hi
    }
}

/// A theme's family names as parley's, with CSS generics recognised.
///
/// `sans-serif`, `monospace` and `emoji` become `FontFamilyName::Generic` and
/// resolve through the collection's generic families, which is why
/// [`Fonts::wire`](crate::fonts::Fonts::wire) is not optional.
fn family_list(families: &[String]) -> Vec<FontFamilyName<'static>> {
    families
        .iter()
        .map(|name| match GenericFamily::parse(name) {
            Some(generic) => FontFamilyName::Generic(generic),
            None => FontFamilyName::Named(name.clone().into()),
        })
        .collect()
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
        let mut builder = self
            .cx
            .ranged_builder(fonts.context_mut(), request.text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(
            family_list(request.families).into(),
        )));
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
        // The style runs, each pushing only what it changes. `push` after
        // `push_default` is how parley layers a range on top of the whole
        // string, and a property a run does not mention keeps the default.
        for run in request.runs {
            if run.families != request.families {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::List(family_list(run.families).into())),
                    run.range.clone(),
                );
            }
            if run.font_size != request.font_size {
                builder.push(StyleProperty::FontSize(run.font_size), run.range.clone());
            }
            if run.weight != request.weight {
                builder.push(
                    StyleProperty::FontWeight(FontWeight::new(f32::from(run.weight))),
                    run.range.clone(),
                );
            }
            if run.italic != request.italic {
                builder.push(
                    StyleProperty::FontStyle(if run.italic {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    }),
                    run.range.clone(),
                );
            }
            if run.brush != request.brush {
                builder.push(StyleProperty::Brush(run.brush), run.range.clone());
            }
            // Pushed as parley's own decoration properties rather than measured
            // here, because parley resolves `Decoration::offset`/`size` from
            // the **run's face** when they are `None`, and the face is the
            // browser's own source for both numbers. `emit` reads them back
            // out. Both default to off, so a `false` is never pushed and never
            // costs a style-run boundary.
            if run.underline {
                builder.push(StyleProperty::Underline(true), run.range.clone());
            }
            if run.strikethrough {
                builder.push(StyleProperty::Strikethrough(true), run.range.clone());
            }
        }
        // Deliberately **not** pushed per run: `LineHeight`. It is
        // `FontSizeRelative`, so parley recomputes it from each run's own font
        // size, which is what CSS's unitless `line-height` does — a 0.8em code
        // span inside a paragraph gets a 0.8em line box and the surrounding
        // text keeps the line at its own height.
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
        ShapedText {
            layout,
            grounds: request.grounds.to_vec(),
        }
    }
}

/// One grapheme cluster of laid-out text — **D8's method, arriving early**.
///
/// D8 fixed the *shape* of any cluster-level answer without deciding its
/// content: *"if M4 needs cluster-level behaviour, `mt-layout` grows a method
/// — point in, offset out — rather than exposing the `Layout`"*. This is that
/// shape, and its first caller is S2's gate rather than M4: the gate asks for
/// **cluster counts** for ZWJ sequences and skin-tone modifiers, and a cluster
/// count is not a glyph count. Nothing else on the display list can answer it,
/// because [`GlyphRun`] carries glyphs.
///
/// A cluster here is a **grapheme cluster** in the [UAX #29 § 3][uax-grapheme]
/// sense, intersected with one shaped run: parley's own type says so, and the
/// consequence worth stating is that the count is a property of the *text*,
/// not of the font. `👨‍👩‍👧‍👦` is one cluster whether the face draws it as one
/// glyph or as four, which is exactly why [`glyphs`](Self::glyphs) is reported
/// beside it rather than instead of it.
///
/// [uax-grapheme]: https://www.unicode.org/reports/tr29/#Grapheme_Cluster_Boundaries
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCluster {
    /// The cluster's extent in the shaped string — the **visible** text, so
    /// [`VisibleTextMap`](crate::inline::VisibleTextMap) is what turns it back
    /// into block text.
    pub text_range: std::ops::Range<usize>,
    /// How many glyphs the face produced for it.
    ///
    /// Usually one, and **not** constrained to be: a ZWJ sequence the face has
    /// no ligature for is one cluster of several glyphs, and the far side of
    /// the same coin is a ligature — several clusters where only the first
    /// carries glyphs, which is what
    /// [`is_ligature_start`](Self::is_ligature_start) names.
    pub glyphs: usize,
    /// This cluster begins a shaped cluster that spans **more than one**
    /// grapheme — the definition of a ligature, and the only way to observe
    /// one without reading the face's `GSUB`.
    ///
    /// `fi` shaped as a single glyph gives two clusters: the `f` with this set
    /// and one glyph, and the `i` with
    /// [`is_ligature_continuation`](Self::is_ligature_continuation) set and
    /// none.
    pub is_ligature_start: bool,
    /// This cluster is inside a shaped cluster that began at an earlier
    /// grapheme, so its glyphs belong to that one and
    /// [`glyphs`](Self::glyphs) is zero.
    pub is_ligature_continuation: bool,
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
    /// Owned rather than borrowed, because a re-break moves every fragment and
    /// the request is long gone by then.
    grounds: Vec<TextGround>,
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

    /// Every grapheme cluster overlapping `text_range`, in **logical** order.
    ///
    /// Byte offsets in and clusters out — D8's *"a method, not an exposed
    /// `Layout`"*, and the reason [`TextCluster`] exists. An empty range
    /// yields nothing; a range that covers the whole string yields every
    /// cluster exactly once, because a cluster belongs to exactly one run of
    /// exactly one line.
    ///
    /// The range is **clipped, not snapped**: a cluster that merely overlaps
    /// the argument is included whole, so a caller that hands in the range of
    /// one emoji gets that emoji's cluster and not a fragment of it.
    pub fn clusters(&self, text_range: std::ops::Range<usize>) -> Vec<TextCluster> {
        let mut out = Vec::new();
        if text_range.is_empty() {
            return out;
        }
        for line in self.layout.lines() {
            for run in line.runs() {
                let run_range = run.text_range();
                if run_range.start >= text_range.end || text_range.start >= run_range.end {
                    continue;
                }
                for cluster in run.clusters() {
                    let range = cluster.text_range();
                    if range.start >= text_range.end || text_range.start >= range.end {
                        continue;
                    }
                    out.push(TextCluster {
                        text_range: range,
                        glyphs: cluster.glyphs().count(),
                        is_ligature_start: cluster.is_ligature_start(),
                        is_ligature_continuation: cluster.is_ligature_continuation(),
                    });
                }
            }
        }
        // Runs come back in visual order and a bidi line reorders them, so the
        // logical order the caller asked for has to be restored here rather
        // than assumed. Sorting by start is total: two clusters of the same
        // layout never share a start.
        out.sort_by_key(|c| c.text_range.start);
        out
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
    ///
    /// # The two decorations, and where their numbers come from
    ///
    /// A strikethrough and an underline are the only things drawn here that are
    /// not a copied number, and neither has a MarkText source: `grep
    /// line-through` over muya's stylesheets is empty and `a.mu-inline-rule`
    /// sets colour alone, so both are the **browser's UA sheet**, which derives
    /// thickness and position from the *font*. That mechanism is reachable on
    /// this pin — `Run::font_metrics()` surfaces skrifa's `post.underline*` and
    /// `OS/2.yStrikeout*`, already scaled to the run's size — so it is used
    /// rather than replaced by two invented theme fields. The faces are pinned
    /// and their SHA-256s are in `faces.toml`, so the numbers are as
    /// deterministic as a theme constant would have been and are additionally
    /// *right* for whichever face won fallback.
    ///
    /// `y = baseline − offset` and `height = size` for both, which is the rule
    /// every one of parley's own four example renderers uses.
    pub fn emit(
        &self,
        fonts: &Fonts,
        origin_x: f32,
        origin_y: f32,
        out: &mut Vec<DisplayItem>,
    ) -> Result<(), FontError> {
        for line in self.layout.lines() {
            // Grounds first, because `BlockDisplay::items` is paint order and a
            // ground is by definition behind the glyphs it belongs to.
            //
            // One fragment per line, with the horizontal padding applied only
            // at the range's true start and true end — CSS's default
            // `box-decoration-break: slice`, so a code span broken across two
            // lines is padded on the outside and flush at the break.
            //
            // The x-extent is the union of the *whole* glyph runs the ground
            // overlaps, which is exact because a ground is only ever requested
            // for a style that also changes the font, and a style-run boundary
            // is a shaping boundary. A theme that made inline code the same
            // family and size as body text would widen its ground to the
            // surrounding run; that is the one case this approximates, and it
            // is a theme that has asked for an invisible box.
            let line_range = line.text_range();
            for ground in &self.grounds {
                if ground.range.start >= line_range.end || line_range.start >= ground.range.end {
                    continue;
                }
                let mut min_x = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut ascent = 0.0f32;
                let mut descent = 0.0f32;
                let mut baseline = 0.0f32;
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(gr) = item else {
                        continue;
                    };
                    let range = gr.run().text_range();
                    if range.start >= ground.range.end || ground.range.start >= range.end {
                        continue;
                    }
                    min_x = min_x.min(gr.offset());
                    max_x = max_x.max(gr.offset() + gr.advance());
                    // The *content area* of an inline box, which is what CSS
                    // pads and paints — the face's ascent plus descent, not the
                    // line box, so a code span does not grow its own line.
                    let metrics = gr.run().font_metrics();
                    ascent = ascent.max(metrics.ascent);
                    descent = descent.max(metrics.descent);
                    baseline = gr.baseline();
                }
                if min_x > max_x {
                    continue;
                }
                let pad_left = if ground.range.start >= line_range.start {
                    ground.padding_x
                } else {
                    0.0
                };
                let pad_right = if ground.range.end <= line_range.end {
                    ground.padding_x
                } else {
                    0.0
                };
                out.push(DisplayItem::Rect(FilledRect {
                    rect: Rect::new(
                        min_x - pad_left + origin_x,
                        baseline - ascent - ground.padding_y + origin_y,
                        (max_x - min_x) + pad_left + pad_right,
                        ascent + descent + 2.0 * ground.padding_y,
                    ),
                    corner_radius: ground.corner_radius,
                    rotation_deg: 0.0,
                    brush: ground.brush,
                }));
            }
            // Where in the current line item's glyphs the next glyph run
            // starts. parley splits one shaped run into one `GlyphRun` per
            // *style*, and every one of them answers `run().text_range()` with
            // the whole item's range — so the range has to be recovered from
            // the clusters. See `glyph_run_text_range`.
            let mut item_key: Option<std::ops::Range<usize>> = None;
            let mut glyph_start = 0usize;
            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(gr) => {
                        let run = gr.run();
                        let key = run.text_range();
                        if item_key.as_ref() != Some(&key) {
                            item_key = Some(key);
                            glyph_start = 0;
                        }
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
                        let glyphs: Vec<Glyph> = gr
                            .positioned_glyphs()
                            .map(|g| Glyph {
                                id: g.id,
                                x: g.x + origin_x,
                                y: g.y + origin_y,
                            })
                            .collect();
                        let text_range = glyph_run_text_range(run, glyph_start, glyphs.len());
                        glyph_start += glyphs.len();
                        out.push(DisplayItem::Glyphs(GlyphRun {
                            font,
                            font_size: run.font_size(),
                            is_rtl: run.is_rtl(),
                            text_range,
                            baseline: gr.baseline() + origin_y,
                            offset: gr.offset() + origin_x,
                            advance: gr.advance(),
                            brush: gr.style().brush,
                            glyphs,
                        }));
                        // Decorations after the glyphs, which is the order
                        // parley's own renderers use and the only order a
                        // strikethrough can be drawn in.
                        let style = gr.style();
                        let metrics = run.font_metrics();
                        for (decoration, offset, size) in [
                            (
                                style.underline.as_ref(),
                                metrics.underline_offset,
                                metrics.underline_size,
                            ),
                            (
                                style.strikethrough.as_ref(),
                                metrics.strikethrough_offset,
                                metrics.strikethrough_size,
                            ),
                        ] {
                            let Some(decoration) = decoration else {
                                continue;
                            };
                            // `None` on either field means "take the face's",
                            // and nothing in this crate sets either — see the
                            // method's own note on why the face is the source.
                            let offset = decoration.offset.unwrap_or(offset);
                            let size = decoration.size.unwrap_or(size);
                            out.push(DisplayItem::Rect(FilledRect::new(
                                Rect::new(
                                    gr.offset() + origin_x,
                                    gr.baseline() - offset + origin_y,
                                    gr.advance(),
                                    size,
                                ),
                                decoration.brush,
                            )));
                        }
                    }
                    PositionedLayoutItem::InlineBox(b) => {
                        // parley restarts its own glyph cursor at every box,
                        // so this one restarts with it.
                        item_key = None;
                        glyph_start = 0;
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
