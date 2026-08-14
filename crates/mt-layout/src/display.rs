//! The neutral display list — M3 §5 D8.
//!
//! > **Decision: the neutral list. `parley::Layout` is held per block *inside*
//! > `mt-layout` and appears in no public type. The font reference crosses the
//! > seam as a `FontId` indexing the collection the shell supplied under D7.**
//!
//! # What this buys, and what it does not
//!
//! It does **not** protect the goldens. The parley churn D2 pinned around was
//! *geometric* (#697 moved line height 24.00 → 28.40), and a neutral list
//! carries the new numbers just as faithfully as parley's own type would. What
//! it buys is the consumers: `mt-export`'s PDF writer at M6 and `mt-render` at
//! S4 both take this list as input, and exposing `parley::Layout` would make a
//! milestone two ahead depend on the internal walk of a git-pinned 0.x crate.
//!
//! # The two rules this module exists to keep
//!
//! 1. **No public item's signature names a parley type.** Not `Layout`, not
//!    `FontData`, not `Glyph`, not `InlineBoxKind`. The `parley::Layout` a
//!    block is laid out into lives in a private field of
//!    [`ShapedText`](crate::text::ShapedText), and the walk that turns it into
//!    this list is the only place the two vocabularies meet.
//! 2. **Plain `f32` geometry only.** Never `kurbo::Rect`, never `peniko`.
//!    C9 measured that parley demoted peniko to a dev-dependency and replaced
//!    `kurbo::Rect` with its own `BoundingBox`, so the peniko/kurbo boundary
//!    now sits at the `mt-layout` → `mt-render` seam rather than inside
//!    `mt-layout`. This module is that boundary.
//!
//! # Why there is a brush type here at all
//!
//! C9: parley's `Brush` is **parley's own trait** — a blanket impl over
//! `Clone + PartialEq + Default + Debug`, not `peniko::Brush`. So `mt-layout`
//! defines its own brush in either design and that part of the seam was always
//! free. [`Brush`] below is what parley's `Layout<B>` is parameterised by
//! *and* what this list carries; there is no conversion between the two.
//!
//! # Most of a theme is not glyphs
//!
//! Of D4's 148 required theme fields, the great majority describe rectangles
//! and rules: the blockquote bar, the code-block background and border, table
//! cell borders, the thematic break, list bullets, the task checkbox's box,
//! ring and check. `mt-render` computes **no** geometry, so every one of those
//! has to arrive here already positioned — which is why [`DisplayItem`] has
//! non-text arms and why [`FilledRect`] carries a corner radius and a
//! rotation.

use std::ops::Range;

use mt_doc::{Block, NodeId};

use crate::fonts::FontId;
use crate::theme::{Color, LineStyle};

// ---------------------------------------------------------------------------
// Brush
// ---------------------------------------------------------------------------

/// The display list's paint: 8-bit sRGB with 8-bit alpha.
///
/// This is `mt-layout`'s own type by necessity rather than preference — see the
/// module doc. It satisfies parley's `Brush` bound
/// (`Clone + PartialEq + Default + Debug`) through that trait's blanket impl,
/// so the same type parameterises the `parley::Layout` behind the seam and the
/// [`GlyphRun`]s in front of it.
///
/// It is deliberately **not** [`theme::Color`](crate::theme::Color): that type
/// carries the CSS keyword `inherit`, which is a question and not an answer. A
/// display list holds answers. [`Brush::resolve`] is where the question is
/// settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Brush {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha; 255 is opaque.
    pub a: u8,
}

impl Default for Brush {
    /// Opaque black.
    ///
    /// `Default` exists only because parley's `Brush` bound requires it. It is
    /// opaque black rather than the derived transparent black on purpose: a
    /// default that renders nothing turns a missed assignment into invisible
    /// text, which is the same class of silent failure D4's required theme
    /// fields and D7's hard registration error both exist to rule out.
    fn default() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
}

impl Brush {
    /// An opaque brush.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A brush with an explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Fully transparent — nothing is painted.
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    /// Whether this brush paints anything at all.
    ///
    /// Worth asking before pushing an item: `dark.toml`'s code block has a
    /// `border_width_px` of 0 and several themes have transparent tints, and a
    /// display list that carries invisible primitives makes every golden
    /// larger and every renderer slower for no pixels.
    pub const fn is_visible(self) -> bool {
        self.a != 0
    }

    /// Resolve a theme colour against the colour it would inherit.
    ///
    /// Three of muya's own defaults are `inherit` (`--strong-color`,
    /// `--em-color`, `--list-marker-color`), so this is the normal path rather
    /// than an edge case.
    pub const fn resolve(color: Color, inherited: Brush) -> Brush {
        match color {
            Color::Rgba { r, g, b, a } => Brush { r, g, b, a },
            Color::Inherit => inherited,
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// An axis-aligned rectangle in document coordinates, y down.
///
/// Y-down because parley's glyph positions have been Y-down since 0.8.0 (C9),
/// and a display list carrying two conventions is a defect waiting to be
/// found by a renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width; never negative.
    pub width: f32,
    /// Height; never negative.
    pub height: f32,
}

impl Rect {
    /// A rectangle from its origin and size.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The right edge.
    pub fn max_x(&self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge.
    pub fn max_y(&self) -> f32 {
        self.y + self.height
    }

    /// This rectangle moved by `(dx, dy)`.
    pub fn translated(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// A whole document laid out at one width, in one theme.
///
/// The unit `mt-render` draws, `mt-export` writes to PDF, and D10's goldens
/// serialize. Because M3 owns the type, a golden's *shape* changes only when
/// M3 changes it (D8).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayList {
    /// The width layout was performed at — `[metrics] content_width_px`
    /// less the container's horizontal padding.
    pub width: f32,
    /// Total height of every block, including the container's bottom padding.
    pub height: f32,
    /// Blocks in document order.
    ///
    /// Document order rather than paint order, because D9 makes laziness
    /// load-bearing: the viewport picks a contiguous slice of this vector, and
    /// a dirty block re-lays out in place while the blocks after it have their
    /// `y` shifted.
    pub blocks: Vec<BlockDisplay>,
}

/// One block's positioned contents.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDisplay {
    /// The `mt_doc` node this came from — M4 hit-tests back through it, and
    /// D9's incremental relayout keys on it.
    pub node: NodeId,
    /// Which kind of block, for the reason D11 gives.
    pub kind: BlockKind,
    /// The block's border box in document coordinates.
    pub bounds: Rect,
    /// The language this block's text is written in, for the five kinds
    /// [`BlockKind::uses_code_block_box`] names, and `None` for everything
    /// else.
    ///
    /// **This is the "with the language named" half of D11**, and it is a field
    /// rather than drawn chrome on purpose. D11 forbids inventing placeholder
    /// chrome, so nothing here paints a badge; what the decision needs is that
    /// the mapping *exists*, is computed once by `mt-layout`
    /// ([`crate::flow::code_language`]), and is visible to the two consumers
    /// that will need it — S3's highlighter, which asks which grammar to run,
    /// and D10's goldens, where `code-block lang=rust` beside `math-block
    /// lang=latex` is the evidence that D11 landed rather than a claim that it
    /// did.
    ///
    /// For a `CodeBlock` it is `Block::highlight_language()` — the **first
    /// word** of the info string, never the whole string — so it is `None` for
    /// a bare fence.
    pub language: Option<String>,
    /// Everything to draw, **in paint order** — backgrounds before borders
    /// before glyphs.
    ///
    /// Paint order here and document order in [`DisplayList::blocks`] is not
    /// an inconsistency: a renderer needs one, incremental relayout needs the
    /// other, and the two happen to be different questions.
    pub items: Vec<DisplayItem>,
}

/// One thing to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    /// Positioned glyphs sharing a font, size, direction and brush.
    Glyphs(GlyphRun),
    /// A hole left for a replaced element — an image, inline math, a diagram.
    InlineBox(InlineBoxItem),
    /// A filled rectangle: code-block background, blockquote bar, table cell
    /// border, list bullet, checkbox box.
    Rect(FilledRect),
    /// A stroked straight line: the thematic break, and any border a theme
    /// gives a style other than `solid`.
    Line(StrokedLine),
}

// ---------------------------------------------------------------------------
// Block kind — D11
// ---------------------------------------------------------------------------

/// Which `mt_doc::Block` a [`BlockDisplay`] came from.
///
/// **D11 is why this exists rather than being derivable from the items.**
/// M3 lays math and diagrams out as their own source in the code-block style,
/// because there is no TeX renderer and no mermaid renderer; at M6, `mt-math`
/// and `mt-diagram` **replace that arm**. They find it through this
/// discriminator rather than by threading a new path through layout — which is
/// only possible if [`MathBlock`](Self::MathBlock) and
/// [`Diagram`](Self::Diagram) are distinguishable here even though they render
/// identically today.
///
/// One variant per `mt_doc::Block` variant, one-to-one and enforced by
/// [`BlockKind::name`] agreeing with `Block::name` for all nineteen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// `Block::Paragraph`.
    Paragraph,
    /// `Block::AtxHeading`.
    AtxHeading,
    /// `Block::SetextHeading`.
    SetextHeading,
    /// `Block::ThematicBreak`.
    ThematicBreak,
    /// `Block::CodeBlock`.
    CodeBlock,
    /// `Block::HtmlBlock`.
    HtmlBlock,
    /// `Block::MathBlock` — **D11's arm.** Laid out as source in the
    /// code-block style at M3; `mt-math` replaces it at M6.
    MathBlock,
    /// `Block::Frontmatter`.
    Frontmatter,
    /// `Block::Diagram` — **D11's other arm.** `mt-diagram` replaces it at M6.
    Diagram,
    /// `Block::TableCell`.
    TableCell,
    /// `Block::BlockQuote`.
    BlockQuote,
    /// `Block::BulletList`.
    BulletList,
    /// `Block::OrderList`.
    OrderList,
    /// `Block::ListItem`.
    ListItem,
    /// `Block::TaskList`.
    TaskList,
    /// `Block::TaskListItem`.
    TaskListItem,
    /// `Block::Table`.
    Table,
    /// `Block::TableRow`.
    TableRow,
    /// `Block::Footnote`.
    Footnote,
}

impl BlockKind {
    /// Every variant, in `mt_doc::Block`'s declaration order.
    ///
    /// Public because the golden dumper and M6's replacement arms both want to
    /// iterate the set, and a hand-written list in two places is a list that
    /// disagrees with itself.
    pub const ALL: [BlockKind; 19] = [
        BlockKind::Paragraph,
        BlockKind::AtxHeading,
        BlockKind::SetextHeading,
        BlockKind::ThematicBreak,
        BlockKind::CodeBlock,
        BlockKind::HtmlBlock,
        BlockKind::MathBlock,
        BlockKind::Frontmatter,
        BlockKind::Diagram,
        BlockKind::TableCell,
        BlockKind::BlockQuote,
        BlockKind::BulletList,
        BlockKind::OrderList,
        BlockKind::ListItem,
        BlockKind::TaskList,
        BlockKind::TaskListItem,
        BlockKind::Table,
        BlockKind::TableRow,
        BlockKind::Footnote,
    ];

    /// The same string `Block::name` returns.
    ///
    /// Shared spelling rather than a second vocabulary: D10's goldens are read
    /// beside `cargo xtask blocks`' state trees, and a reader should not have
    /// to translate `"math-block"` into `"MathBlock"` to compare them.
    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Paragraph => "paragraph",
            BlockKind::AtxHeading => "atx-heading",
            BlockKind::SetextHeading => "setext-heading",
            BlockKind::ThematicBreak => "thematic-break",
            BlockKind::CodeBlock => "code-block",
            BlockKind::HtmlBlock => "html-block",
            BlockKind::MathBlock => "math-block",
            BlockKind::Frontmatter => "frontmatter",
            BlockKind::Diagram => "diagram",
            BlockKind::TableCell => "table.cell",
            BlockKind::BlockQuote => "block-quote",
            BlockKind::BulletList => "bullet-list",
            BlockKind::OrderList => "order-list",
            BlockKind::ListItem => "list-item",
            BlockKind::TaskList => "task-list",
            BlockKind::TaskListItem => "task-list-item",
            BlockKind::Table => "table",
            BlockKind::TableRow => "table.row",
            BlockKind::Footnote => "footnote",
        }
    }

    /// Whether D11 routes this kind through the code-block box.
    ///
    /// `MathBlock`, `Diagram`, `Frontmatter` and `HtmlBlock` share
    /// [`CodeBlock`](crate::theme::CodeBlock)'s geometry — the theme's own doc
    /// comment says so — and this is the predicate the block-flow phase asks
    /// rather than re-deriving the set at each call site.
    pub fn uses_code_block_box(self) -> bool {
        matches!(
            self,
            BlockKind::CodeBlock
                | BlockKind::HtmlBlock
                | BlockKind::MathBlock
                | BlockKind::Frontmatter
                | BlockKind::Diagram
        )
    }
}

impl From<&Block> for BlockKind {
    fn from(block: &Block) -> BlockKind {
        match block {
            Block::Paragraph { .. } => BlockKind::Paragraph,
            Block::AtxHeading { .. } => BlockKind::AtxHeading,
            Block::SetextHeading { .. } => BlockKind::SetextHeading,
            Block::ThematicBreak { .. } => BlockKind::ThematicBreak,
            Block::CodeBlock { .. } => BlockKind::CodeBlock,
            Block::HtmlBlock { .. } => BlockKind::HtmlBlock,
            Block::MathBlock { .. } => BlockKind::MathBlock,
            Block::Frontmatter { .. } => BlockKind::Frontmatter,
            Block::Diagram { .. } => BlockKind::Diagram,
            Block::TableCell { .. } => BlockKind::TableCell,
            Block::BlockQuote { .. } => BlockKind::BlockQuote,
            Block::BulletList { .. } => BlockKind::BulletList,
            Block::OrderList { .. } => BlockKind::OrderList,
            Block::ListItem { .. } => BlockKind::ListItem,
            Block::TaskList { .. } => BlockKind::TaskList,
            Block::TaskListItem { .. } => BlockKind::TaskListItem,
            Block::Table { .. } => BlockKind::Table,
            Block::TableRow { .. } => BlockKind::TableRow,
            Block::Footnote { .. } => BlockKind::Footnote,
        }
    }
}

// ---------------------------------------------------------------------------
// Glyph runs
// ---------------------------------------------------------------------------

/// A run of positioned glyphs sharing a font, size, direction and brush.
///
/// The neutral form of parley's `GlyphRun`. Field for field this really is
/// nearly a rename, as D8 claimed — the corrections are recorded on
/// [`Glyph`] and [`InlineBoxItem`], not here.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    /// The face, as an index into the collection the shell registered under
    /// D7. See [`FontId`].
    pub font: FontId,
    /// Font size in px, already scaled.
    pub font_size: f32,
    /// Whether the run reads right to left. Visual order is already baked into
    /// the glyph positions; this is here for the renderer's underline
    /// direction and for M4's caret affinity.
    pub is_rtl: bool,
    /// Byte range in the **block's own text**, not the document's.
    ///
    /// S2 owes the map from these offsets to visible-text offsets (C6); this
    /// range is the block-text side of it.
    ///
    /// # Two runs in the list are not into any block's text
    ///
    /// A list marker (`1.`, `9)`) and a code block's line numbers are text
    /// `mt-layout` *generates*; there is nothing in the document they are a
    /// range of, and their ranges index the generated string instead. They are
    /// distinguishable without a flag: a marker is the only glyph run a
    /// **container** block ever carries, and line numbers appear only when
    /// `LayoutOptions::code_block_line_numbers` is on, which is off by default.
    /// Recorded rather than flagged because a `bool` here would cost every run
    /// in the list a byte for two cases, and M4 has to know the distinction
    /// either way.
    pub text_range: Range<usize>,
    /// Baseline y, in document coordinates. Every [`Glyph::y`] is already
    /// relative to the same origin, so this is redundant for drawing and
    /// load-bearing for decorations: an underline is placed from the baseline,
    /// not from a glyph.
    pub baseline: f32,
    /// x of the first glyph's origin.
    pub offset: f32,
    /// Total advance of the run.
    ///
    /// Carried because [`Glyph`] drops parley's per-glyph `advance`; with
    /// `offset` and this, every advance is recoverable (the next glyph's `x`
    /// gives all but the last, and `offset + advance` gives the last), so the
    /// drop is lossless rather than merely small.
    pub advance: f32,
    /// The colour to paint the glyphs.
    pub brush: Brush,
    /// The glyphs, in visual order.
    pub glyphs: Vec<Glyph>,
}

/// One positioned glyph.
///
/// # Correction to D8
///
/// D8 names this type `PositionedGlyph { id, x, y }`. On D2's pin there is no
/// `PositionedGlyph`: `GlyphRun::positioned_glyphs()` yields
/// `parley_engine::Glyph`, which has a **fourth** field, `advance: f32`. It is
/// dropped here — E2's renderer drops it too
/// (`spikes/e2-vello-cpu-parley-main/src/main.rs:85-91`) — and
/// [`GlyphRun::advance`] exists so that dropping it loses nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Glyph id in the face, **not** a codepoint. `0` is `.notdef` — tofu.
    pub id: u32,
    /// x of the glyph origin, in document coordinates.
    pub x: f32,
    /// y of the glyph origin, in document coordinates, y down.
    pub y: f32,
}

// ---------------------------------------------------------------------------
// Inline boxes
// ---------------------------------------------------------------------------

/// A hole in the text for a replaced element.
///
/// M3 places these and draws nothing in them; the image decoder is M4's and
/// the math and diagram renderers are M6's. What M3 owes is that the hole is
/// in the right place, which is exactly what D2 pinned `main` for — 0.11.0 has
/// no `InlineBox::baseline` and cannot align one.
///
/// # Correction to D8
///
/// D8 gives the shape as `{ id, x, y, width, height, baseline }`. Two fields
/// are off on D2's pin. `baseline` is `Option<f32>`, not `f32` — `None` means
/// "align my bottom edge to the text baseline", which is a different
/// instruction rather than a missing number, and flattening it would silently
/// mis-place every box that does not declare one. And there is a **seventh**
/// field, `kind: InlineBoxKind`, which is a parley enum and therefore the
/// *second* non-plain type on the walk that D8's "every field except one"
/// count missed. It is neutralised as [`InlineBoxFlow`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineBoxItem {
    /// The id the caller pushed, used to match a box in the output back to the
    /// element in the input.
    pub id: u64,
    /// Left edge in document coordinates.
    pub x: f32,
    /// Top edge in document coordinates.
    pub y: f32,
    /// Width in px.
    pub width: f32,
    /// Height in px.
    pub height: f32,
    /// Baseline **relative to [`y`](Self::y)**, or `None` if the box's bottom
    /// edge aligns with the text baseline.
    pub baseline: Option<f32>,
    /// Whether the box took up space in the line.
    pub flow: InlineBoxFlow,
}

impl InlineBoxItem {
    /// The box's rectangle.
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

/// Whether an inline box participates in line layout.
///
/// The neutral form of parley's `InlineBoxKind`. M3 emits only
/// [`InFlow`](Self::InFlow) — nothing in markdown floats — but the variant set
/// is carried whole rather than collapsed to a `bool`, because a two-valued
/// field that has to grow a third value later is a breaking change to a type
/// M6 consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InlineBoxFlow {
    /// Takes up space in the line — CSS `display: inline-block`.
    #[default]
    InFlow,
    /// Positioned as if zero-width — CSS `position: absolute`.
    OutOfFlow,
    /// Not positioned by the text engine at all; the caller lays it out.
    CustomOutOfFlow,
}

// ---------------------------------------------------------------------------
// Non-text primitives
// ---------------------------------------------------------------------------

/// A filled rectangle, optionally rounded and optionally rotated.
///
/// The workhorse of the non-glyph half of the list: code-block background,
/// code-block border (four rects, or one rect behind one inset rect), the
/// blockquote bar, table cell borders, list bullets, and all three parts of
/// the task checkbox.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilledRect {
    /// The rectangle, before rotation.
    pub rect: Rect,
    /// Corner radius in px; `0.0` for a square corner.
    ///
    /// A circle is `min(width, height) / 2` — which is how muya's `disc`
    /// bullet and its `border-radius: 50%` checkbox are expressed, rather than
    /// by giving the list an ellipse primitive it would use twice.
    pub corner_radius: f32,
    /// Rotation in degrees, clockwise, about the rectangle's centre.
    ///
    /// Almost always `0.0`. It is here because the theme model has two fields
    /// that cannot be expressed without it —
    /// [`Checkbox::unchecked_rotation_deg`](crate::theme::Checkbox::unchecked_rotation_deg)
    /// and
    /// [`Checkbox::check_rotation_deg`](crate::theme::Checkbox::check_rotation_deg),
    /// the −90° and 45° that make three shipped themes' checkmark a checkmark
    /// — and `mt-render` computes no geometry, so the rotation has to arrive
    /// already decided.
    pub rotation_deg: f32,
    /// The fill.
    pub brush: Brush,
}

impl FilledRect {
    /// A plain axis-aligned fill.
    pub const fn new(rect: Rect, brush: Brush) -> Self {
        Self {
            rect,
            corner_radius: 0.0,
            rotation_deg: 0.0,
            brush,
        }
    }
}

/// A stroked straight line.
///
/// Distinct from a thin [`FilledRect`] because CSS border styles are not
/// expressible as a fill: `thematic_break.style` is `dashed` in muya's own
/// default, and a renderer needs to be told that rather than to guess it from
/// an aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokedLine {
    /// Start x.
    pub x0: f32,
    /// Start y.
    pub y0: f32,
    /// End x.
    pub x1: f32,
    /// End y.
    pub y1: f32,
    /// Stroke width in px.
    pub width: f32,
    /// Solid, dashed, dotted, or none.
    pub style: LineStyle,
    /// The stroke colour.
    pub brush: Brush,
}

impl StrokedLine {
    /// A horizontal rule.
    pub const fn horizontal(y: f32, x0: f32, x1: f32, width: f32, brush: Brush) -> Self {
        Self {
            x0,
            y0: y,
            x1,
            y1: y,
            width,
            style: LineStyle::Solid,
            brush,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- BlockKind is one-to-one with mt_doc::Block, and D11 depends on it --

    #[test]
    fn block_kind_names_match_mt_doc_block_names() {
        // Every `Block` variant, constructed so the match in `From<&Block>`
        // is exercised rather than assumed. `Text::default()` is empty text;
        // the payloads are irrelevant to the mapping.
        use mt_doc::{
            Align, BulletMarker, CodeKind, DiagramKind, DiagramLang, FrontmatterLang,
            FrontmatterStyle, MathStyle, OrderDelim, Text, Underline,
        };
        let blocks = [
            Block::Paragraph {
                text: Text::default(),
            },
            Block::AtxHeading {
                level: 1,
                text: Text::default(),
            },
            Block::SetextHeading {
                level: 1,
                underline: Underline::Equals(1),
                text: Text::default(),
            },
            Block::ThematicBreak {
                text: Text::default(),
            },
            Block::CodeBlock {
                kind: CodeKind::Fenced,
                info: String::new(),
                fence_len: Some(3),
                text: Text::default(),
            },
            Block::HtmlBlock {
                text: Text::default(),
            },
            Block::MathBlock {
                style: MathStyle::Default,
                text: Text::default(),
            },
            Block::Frontmatter {
                lang: FrontmatterLang::Yaml,
                style: FrontmatterStyle::Dash,
                text: Text::default(),
            },
            Block::Diagram {
                lang: DiagramLang::Yaml,
                kind: DiagramKind::Mermaid,
                text: Text::default(),
            },
            Block::TableCell {
                align: Align::None,
                text: Text::default(),
            },
            Block::BlockQuote { children: vec![] },
            Block::BulletList {
                marker: BulletMarker::Dash,
                loose: false,
                children: vec![],
            },
            Block::OrderList {
                start: 1,
                delimiter: OrderDelim::Period,
                loose: false,
                children: vec![],
            },
            Block::ListItem { children: vec![] },
            Block::TaskList {
                marker: BulletMarker::Dash,
                loose: false,
                children: vec![],
            },
            Block::TaskListItem {
                checked: false,
                children: vec![],
            },
            Block::Table { children: vec![] },
            Block::TableRow { children: vec![] },
            Block::Footnote {
                identifier: String::new(),
                children: vec![],
            },
        ];
        assert_eq!(
            blocks.len(),
            BlockKind::ALL.len(),
            "a Block variant was added or removed without updating BlockKind"
        );
        for (block, kind) in blocks.iter().zip(BlockKind::ALL) {
            assert_eq!(
                BlockKind::from(block),
                kind,
                "BlockKind::ALL is out of order at {}",
                block.name()
            );
            assert_eq!(
                kind.name(),
                block.name(),
                "BlockKind::name disagrees with Block::name"
            );
        }
    }

    #[test]
    fn block_kind_names_are_unique() {
        let mut names: Vec<&str> = BlockKind::ALL.iter().map(|k| k.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two BlockKinds share a name");
    }

    /// D11's whole point: M6 replaces the math and diagram arms, so they must
    /// not be the same value as each other or as `CodeBlock`, even though all
    /// three lay out identically at M3.
    #[test]
    fn math_and_diagram_stay_distinguishable_from_code() {
        assert_ne!(BlockKind::MathBlock, BlockKind::Diagram);
        assert_ne!(BlockKind::MathBlock, BlockKind::CodeBlock);
        assert_ne!(BlockKind::Diagram, BlockKind::CodeBlock);
        assert!(BlockKind::MathBlock.uses_code_block_box());
        assert!(BlockKind::Diagram.uses_code_block_box());
        assert!(!BlockKind::Paragraph.uses_code_block_box());
    }

    // ---- Brush ----------------------------------------------------------

    #[test]
    fn default_brush_is_opaque_black_not_invisible() {
        assert_eq!(Brush::default(), Brush::rgb(0, 0, 0));
        assert!(Brush::default().is_visible());
        assert!(!Brush::TRANSPARENT.is_visible());
    }

    #[test]
    fn inherit_resolves_to_the_surrounding_colour() {
        let inherited = Brush::rgb(1, 2, 3);
        assert_eq!(Brush::resolve(Color::Inherit, inherited), inherited);
        assert_eq!(
            Brush::resolve(Color::rgba(9, 8, 7, 6), inherited),
            Brush::rgba(9, 8, 7, 6)
        );
    }

    /// The bound parley's `Brush` trait imposes, asserted here so that a
    /// derive removed from [`Brush`] fails in this file rather than in a
    /// type error three modules away.
    #[test]
    fn brush_satisfies_parleys_blanket_bound() {
        fn assert_brush<T: Clone + PartialEq + Default + std::fmt::Debug>() {}
        assert_brush::<Brush>();
    }

    // ---- Geometry -------------------------------------------------------

    #[test]
    fn rect_edges_and_translation() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.max_x(), 40.0);
        assert_eq!(r.max_y(), 60.0);
        assert_eq!(r.translated(1.0, -2.0), Rect::new(11.0, 18.0, 30.0, 40.0));
    }
}
