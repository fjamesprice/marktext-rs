//! Block flow — `Document` + theme + width → positioned blocks.
//!
//! This is S1's gate: every `mt_doc::Block` variant placed at the numbers in
//! M3.md §2's geometry table, in either shipped theme, at any width.
//!
//! # What a leaf's text is, since S2
//!
//! **Not `Block::text()`.** A leaf whose kind answers
//! [`BlockKind::lays_out_inline_markdown`] is tokenized by [`crate::inline`],
//! its markers are hidden because a read-only viewer has no caret (C6), and
//! what reaches parley is the resulting **visible string** plus the style runs
//! `LayoutTree::style_runs` resolves against the theme. `**bold**` is four
//! columns here, not eight, and every [`GlyphRun::text_range`] is an offset
//! into that shorter string — [`BlockDisplay::text_map`] is the way back.
//!
//! Everything else — a code block, a list marker, a line-number gutter, a
//! footnote's `[^id]:` label — is shaped verbatim, because its text is either
//! source or synthetic and a tokenizer has no business in either.
//!
//! [`BlockKind::lays_out_inline_markdown`]: crate::display::BlockKind::lays_out_inline_markdown
//! [`GlyphRun::text_range`]: crate::display::GlyphRun::text_range
//! [`BlockDisplay::text_map`]: crate::display::BlockDisplay::text_map
//!
//! # The content column is not the number in §2's table
//!
//! §2 reads *"Content column: `--editor-area-width`, 800px"*, and separately
//! *"Container padding: `0 50px 100px`"*. They compose, and the composition is
//! **subtractive**:
//!
//! ```text
//! .mu-container { box-sizing: border-box;        /* blockSyntax.css:17 */
//!                 max-width: var(--editor-area-width, 800px);   /* :18 */
//!                 padding: 0 50px 100px; }                      /* :21 */
//! ```
//!
//! `box-sizing: border-box` makes the 800px include the padding, so the text
//! column is **700px** in `muya-default` and **650px** in `dark` — which is
//! what [`Theme::content_width_px`] already returns, and what C7's
//! editor-width note independently says (the user setting injects
//! `calc(100px + <value>)` and *"the `+100px` is the container's `0 50px`
//! padding"*). muya's own sheet states the same arithmetic a second time at
//! `inlineSyntax.css:197`: `max-width: calc(var(--editor-area-width, 800px) -
//! 100px)`.
//!
//! [`Theme::content_width_px`]: crate::theme::Theme::content_width_px
//!
//! ## How `available_width_px` interacts with it
//!
//! `max-width` is a *maximum*, and `margin: 0 auto` centres what is left. So
//! the border-box column is `min(available_width_px, content_width_px)` and the
//! text column is that less `2 × container_padding_x_px`, floored at zero. Pass
//! [`f32::INFINITY`] — or simply the theme's own `content_width_px` — to get the
//! theme's full column; pass a narrower viewport and the column shrinks with
//! it. The centring offset is **not** applied: the display list's origin is the
//! container's content box, so a golden does not move when a window is resized
//! past the maximum. Placing the column inside a viewport is `mt-ui`'s
//! translation, not layout's.
//!
//! # Vertical flow: margins collapse, and this is the exact rule implemented
//!
//! CSS collapses adjacent vertical margins to their **maximum**, not their sum.
//! Adding both would double-space every document in the corpus and the goldens
//! would freeze it. What is implemented:
//!
//! 1. **Adjacent siblings collapse.** The gap between two blocks is
//!    `max(previous.margin_bottom, next.margin_top)`. Implemented as a running
//!    `pending_margin` that takes a maximum and is *materialized* only when a
//!    border, a padding edge or a line of text is reached — which is what makes
//!    a run of three or more margins collapse to one maximum rather than
//!    pairwise.
//! 2. **Parent and first/last child collapse through a container that has
//!    neither a vertical border nor vertical padding.** That is the blockquote
//!    (`padding: 0 30px`), the three list kinds
//!    (`padding-inline-start: 30px`), and list items (nothing at all). A
//!    blockquote's own `0.5em` and its first paragraph's `0.5em` therefore
//!    produce `0.5em` above the quote and **nothing** between the quote's top
//!    edge and the paragraph's — and the blockquote's `::before` bar starts at
//!    the paragraph's top, which is visible and would be wrong by 8px if this
//!    were not implemented.
//! 3. **Nothing collapses through a code block** (`padding: 1em`), **a table
//!    figure** (`padding: 0.5em 0`) **or a footnote** (`padding: 1.2em 2em
//!    0.05em 1em`). Their children's margins stay inside the padding.
//! 4. **At the document root the leading and trailing margins are dropped.** A
//!    document does not begin with blank space, and muya says so itself:
//!    `ul.mu-bullet-list:first-child { margin-top: 0 }` and the matching
//!    `:last-child` rule (`blockSyntax.css:434-441`) exist precisely to zero
//!    them. The first block's border edge lands at
//!    `container_padding_top_px`, which is `0` in both shipped themes.
//!
//! ## Deliberate simplifications, and why the corpus cannot reach them
//!
//! - **Self-collapsing empty blocks.** CSS collapses an empty block's own top
//!   and bottom margins together. Here a container with **zero** children is
//!   given a content edge instead, so its two margins do not merge. Reaching it
//!   needs an empty blockquote, list or footnote, and `mt_md::parse` produces
//!   none — a container exists only because something was inside it. A
//!   paragraph is never empty in this sense: an empty string still has a line
//!   box.
//! - **Negative margins.** No theme field can be negative except
//!   `gutter_letter_spacing_px`, which is not a margin. The `max` rule is
//!   therefore the whole of CSS's rule; the `min`-of-negatives half is
//!   unreachable and unimplemented.
//! - **`:first-child` margin zeroing inside a padded container.** muya's
//!   explicit `ul:first-child { margin-top: 0 }` agrees with rule 2 everywhere
//!   rule 2 applies, and differs only for a list that is the first child of a
//!   *padded* container — a list opening a footnote or a table figure. Not
//!   modelled; it would cost 8px in a shape the corpus does not contain.
//! - **Floats and clearance.** Markdown has neither.
//!
//! # Unbuilt blocks are a state, not a missing value — D9
//!
//! D9: *"What must be deferred is `builder.build` itself — which means the data
//! structure holds **unbuilt** blocks, and 'a block that has no `Layout` yet'
//! is a first-class state in the tree from S1, not a cache miss bolted on at
//! S6."* [`LayoutTree::plan`] therefore does no shaping at all: it resolves
//! every unit, every box edge, every horizontal position and every text style,
//! and leaves each block's [`ShapedText`] unbuilt.
//! [`LayoutTree::build`] shapes exactly one block and needs nothing from any
//! other. The eager driver S1 needs is [`LayoutTree::build_all`], which is a
//! loop over that one call — so S6 substitutes a viewport-driven driver by
//! replacing the loop, not the tree.
//!
//! ## The ordering problem S6 still has to solve, stated precisely
//!
//! A block's `y` depends on the heights of everything before it, and a height
//! needs a shaped `Layout`. So a genuinely lazy driver cannot place block *n*
//! without having built blocks *0..n*. S1 does not solve that and does not have
//! to. What S1 owes is not foreclosing it, and what S6 will need is exactly
//! three things:
//!
//! 1. **An estimated height for an unbuilt block.** [`LayoutTree::place`]
//!    currently treats one as zero. The cheap estimator is `ceil(text_len /
//!    chars_per_line) × line_height`, and every input it needs — the block's
//!    resolved font size, line height and content width — is already on the
//!    planned block before anything is shaped.
//! 2. **A way to re-place from a block onwards.** `place` is a single pass over
//!    the tree from the root; making it resumable means carrying the `Flow`
//!    cursor (`y` plus the pending margin plus the unresolved tops) across
//!    calls. Nothing in the data structure prevents that — the cursor is three
//!    fields and already exists as a local.
//! 3. **A revision check.** `Document::revision` and `Document::dirty` exist
//!    for this; the tree keys on `NodeId`, and `Document::get` returns `None`
//!    for a stale handle rather than a different block (M2 D8).
//!
//! D9 also names a second lever that needs none of this — layout is
//! embarrassingly parallel, and `build` was written so that
//! `blocks.par_iter_mut()` is legal in principle: it takes `&Document` shared
//! and touches only its own block. Only `Fonts` and `TextShaper` are `&mut`,
//! and both are per-thread values by construction.
//!
//! ## Measured, on the same corpus D9 used, and one number D9 does not have
//!
//! `5mb.md`, release, i5-13600K, the committed face set, `muya-default`:
//!
//! | phase | time |
//! |---|---:|
//! | `mt_md::parse` | 80 ms |
//! | `plan` — 54,896 blocks, **nothing shaped** | **6 ms** |
//! | `build_all` + `place` | 743 ms |
//! | `emit` | **468 ms** |
//!
//! Two things follow. The first is that D9's data-structure requirement is
//! affordable: planning the *whole* document costs 6 ms, so a lazy driver can
//! plan eagerly and defer only shaping, which is the split D9 asked for.
//!
//! The second is a **finding D9 does not contain**: D9's 813 ms was shaping
//! alone, and building the display list is a further 468 ms — 36 % of a
//! 1,297 ms end-to-end. A lazy driver that defers shaping but still emits the
//! whole list would miss the 800 ms gate on emit alone. Emit is per-block and
//! [`DisplayList::blocks`] is in document order precisely so a viewport can
//! take a slice, so the fix is available; it is just not the fix D9's
//! measurement points at.
//!
//! # One `parley::Layout` per fence, never one per line
//!
//! D9 measured one `Layout` per code line at **1.38× slower and 2.21× the
//! memory**, and generalised: *any design decision that multiplies `Layout`
//! count is the expensive one, whatever it optimizes.* A code block is one
//! [`ShapedText`] over the fence's whole text; parley breaks the `\n`s. The
//! line-number gutter, when enabled, is **one more** — a single synthetic
//! string, not one per number.

use std::ops::Range;

use mt_doc::{Align, Block, Document, NodeId, OrderDelim};

use crate::display::{BlockDisplay, BlockKind, Brush, DisplayItem, DisplayList, FilledRect, Rect};
use crate::fonts::{FontError, Fonts};
use crate::images::ImageSizes;
use crate::inline::{self, InlineImage, InlineRun, InlineSyntax, VisibleTextMap};
use crate::paint;
use crate::text::{
    BaseDirection, InlineBoxSpec, ShapedText, StyleRun, TextGround, TextRequest, TextShaper,
};
use crate::theme::{CheckboxTop, CodeBlock, Color, ListMarker, OrderedMarker, TextAlign, Theme};
use crate::units::Units;

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Knobs that are muya *options* rather than theme properties.
///
/// Kept apart from [`Theme`] because they are: `codeBlockLineNumbers` lives in
/// `packages/muya/src/config/index.ts:323`, not in any stylesheet, and no theme
/// file can set it. Its default here is muya's default — **`false`**.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutOptions {
    /// muya's `wrapCodeBlocks`, default `false`
    /// (`packages/muya/src/config/index.ts:324`, the line immediately after
    /// `codeBlockLineNumbers`).
    ///
    /// **Off, a long fence line scrolls; it does not wrap.** The base rule is
    /// `.mu-code-block .mu-code { overflow: auto }`
    /// (`blockSyntax.css:230-236`) over the UA's `pre { white-space: pre }`;
    /// `white-space: pre-wrap` appears only inside
    /// `.mu-code-wrap .mu-code-block .mu-code` (`:239-245`), the class this
    /// option toggles. The block's box stays the column and its glyphs run
    /// past it — see [`BlockDisplay::overflow_x`].
    pub wrap_code_blocks: bool,
    /// muya's `codeBlockLineNumbers`, default `false`.
    ///
    /// When on, a fenced or indented code block takes an extra
    /// `code_block.gutter_width_em` of left padding
    /// (`.mu-code-block:has(.mu-line-numbers-rows) { padding-left: 2.5em }`)
    /// and its lines are numbered. The four D11 kinds that share the code-block
    /// box are **not** numbered: muya only ever puts the gutter inside
    /// `.mu-code-block`.
    pub code_block_line_numbers: bool,
    /// The two `mt_md::Options` flags that change what a leaf's text
    /// *tokenizes to* rather than how it is drawn.
    ///
    /// They are here rather than read from the document because `mt-layout`
    /// may not depend on `mt-md` — D5 forbids the edge and S7 asserts its
    /// absence — so a caller that parsed with something other than
    /// `Options::MUYA_DEFAULT` has to say so. The default matches
    /// `MUYA_DEFAULT`: both **off**.
    ///
    /// It also carries the document's reference definitions, without which
    /// `[text][ref]` is four literal brackets. See [`InlineSyntax`].
    pub inline_syntax: InlineSyntax,
    /// Every inline image whose bitmap the caller has already decoded —
    /// **D12**.
    ///
    /// Empty is the correct value for a caller with no image loader, and it is
    /// what the goldens use: `mt-layout` opens nothing, so an image with no
    /// entry takes the reference's own no-bitmap geometry rather than a
    /// guessed number. See [`crate::images`].
    pub images: ImageSizes,
}

/// Lay a document out at a width, in a theme.
///
/// The eager driver: plan, build every block, place, emit. It is written in
/// terms of [`LayoutTree`]'s per-block calls rather than beside them, so that
/// S6's lazy driver replaces this function and nothing else.
///
/// # Errors
///
/// [`FontError::UnresolvedFont`] if a shaped run resolves to a face `fonts`
/// does not hold. That can only happen if `fonts` was mutated between shaping
/// and emitting, which this function does not do — so in practice it is
/// unreachable here and is surfaced rather than swallowed because a dropped
/// glyph run is invisible text.
pub fn layout(
    document: &Document,
    theme: &Theme,
    available_width_px: f32,
    fonts: &mut Fonts,
) -> Result<DisplayList, FontError> {
    let mut shaper = TextShaper::new();
    layout_with(
        document,
        theme,
        available_width_px,
        fonts,
        &mut shaper,
        &LayoutOptions::default(),
    )
}

/// [`layout`], with a caller-owned shaper and muya's options.
///
/// The shaper is a parameter because `LayoutContext` caches shaping plans
/// across calls and D9 makes both laziness and parallelism live options: a
/// per-thread shaper reused across many layouts is the shape both want.
pub fn layout_with(
    document: &Document,
    theme: &Theme,
    available_width_px: f32,
    fonts: &mut Fonts,
    shaper: &mut TextShaper,
    options: &LayoutOptions,
) -> Result<DisplayList, FontError> {
    let mut tree = LayoutTree::plan(document, theme, available_width_px, options);
    tree.build_all(document, fonts, shaper);
    tree.place();
    tree.emit(fonts)
}

/// The language a block's source is written in, for the five kinds that lay out
/// in the code-block box — D11's *"with the language named"*.
///
/// `CodeBlock` answers with `Block::highlight_language()`, the first word of the
/// info string, which is `None` for a bare fence. The other four are named by
/// their own metadata and never by an info string:
///
/// | Kind | Language | Why |
/// |---|---|---|
/// | `MathBlock` | `latex` | D11, verbatim |
/// | `Diagram` | `DiagramKind::info_lang()` | D11: **reuse** `mt-md`'s table rather than fork it. It now lives in `mt-doc`, so `mt-layout` needs no edge on `mt-md` and D5 stays true by construction |
/// | `Frontmatter` | its `lang` | judgement call — see below |
/// | `HtmlBlock` | `html` | judgement call — see below |
///
/// # The two D11 does not name
///
/// D11 decides math and diagrams. `HtmlBlock` and `Frontmatter` are extended
/// onto the same rule here, on D11's own reasoning: *"a previewer with no
/// renderer at all is in that failure state permanently, and the honest
/// presentation of a permanent failure state is the source with its language
/// labelled."* A viewer that renders HTML would need a sanitizer and an HTML
/// layout engine; one that renders frontmatter would need to decide what
/// rendered frontmatter even means. Neither exists, so both are permanently in
/// the state D11 describes.
///
/// It is barely an extension: **muya's own stylesheet already groups all five**
/// — `.mu-code-block, .mu-frontmatter, .mu-html-container, .mu-math-container,
/// .mu-diagram-container` share one rule at `blockSyntax.css:194-215` — and
/// [`BlockKind::uses_code_block_box`] was written from that grouping before D11
/// was applied to it. Recorded as a judgement call anyway, because the
/// milestone document decides math and diagrams explicitly and these two by
/// implication.
pub fn code_language(block: &Block) -> Option<&str> {
    match block {
        Block::CodeBlock { .. } => block.highlight_language(),
        Block::MathBlock { .. } => Some("latex"),
        Block::Diagram { kind, .. } => Some(kind.info_lang()),
        Block::Frontmatter { lang, .. } => Some(lang.info_lang()),
        Block::HtmlBlock { .. } => Some("html"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Per-block plan
// ---------------------------------------------------------------------------

/// Which of the theme's three font stacks a block's text uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontStack {
    Body,
    Code,
    /// `font-family: monospace` on the footnote's `[^id]:` label —
    /// `blockSyntax.css:1025`. A bare CSS generic with no named stack in front
    /// of it, which is a different thing from [`FontStack::Code`] even though
    /// both resolve to DejaVu Sans Mono under the committed set.
    FootnoteLabel,
}

/// Whether a block's text wraps at its content width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wrap {
    /// Everything in normal flow.
    AtContentWidth,
    /// Shaped unbroken. Three callers, for three different reasons.
    ///
    /// A **table cell**, because its column width is not known until the other
    /// cells have been measured: it is shaped unbroken — that *is* its
    /// max-content width — and re-broken during placement. One `Layout`, one
    /// shaping pass, one re-break; C5 measured re-breaking as the cheap half.
    ///
    /// A **code block**, unless `LayoutOptions::wrap_code_blocks` is on.
    /// muya's default is `wrapCodeBlocks: false`
    /// (`packages/muya/src/config/index.ts:324`) and the base rule is
    /// `.mu-code-block .mu-code { overflow: auto }`
    /// (`blockSyntax.css:230-236`) over the UA's `pre { white-space: pre }`.
    /// The `white-space: pre-wrap` at `:242` is inside
    /// `.mu-code-wrap .mu-code-block .mu-code`, opened at `:239` — the class
    /// the option toggles, not the base rule. A long fence line therefore
    /// **scrolls**, and the overflow is reported as
    /// [`BlockDisplay::overflow_x`](crate::display::BlockDisplay::overflow_x).
    ///
    /// A **marker or label** — an ordered number, a footnote's `[^id]:` — which
    /// is one short unbreakable token placed by its own edge.
    Never,
}

/// Everything needed to shape one block's text, resolved before any shaping
/// happens.
#[derive(Debug, Clone, PartialEq)]
struct TextStyle {
    stack: FontStack,
    font_size: f32,
    /// A multiple of `font_size`, CSS's unitless `line-height`.
    line_height: f32,
    weight: u16,
    align: TextAlign,
    brush: Brush,
    wrap: Wrap,
    letter_spacing: f32,
    /// The paragraph's base direction — `Ltr` everywhere today, for the
    /// reference-parity reason [`TextRequest::new`] argues at length.
    ///
    /// A planned field rather than a constant in `shape` because it is the one
    /// input M3-R1's workaround keys on, and because M5 exposes the
    /// reference's own two-valued `dir` preference by setting it here.
    base_direction: BaseDirection,
}

/// A block's margins, border and padding, resolved to px.
///
/// One `border` rather than four: the only bordered block in either shipped
/// theme is the code block, whose border is uniform, and a table cell's border
/// is an overlay rather than part of its box (`blockSyntax.css:621-632`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct BoxEdges {
    margin_top: f32,
    margin_bottom: f32,
    border: f32,
    padding_top: f32,
    padding_right: f32,
    padding_bottom: f32,
    padding_left: f32,
}

impl BoxEdges {
    fn top(&self) -> f32 {
        self.border + self.padding_top
    }
    fn bottom(&self) -> f32 {
        self.border + self.padding_bottom
    }
}

/// D9's first-class state: a block either has a `Layout` or does not.
///
/// Deliberately **not** `Option<ShapedText>`. The difference is not
/// representational — it is that `Option` reads as "a value that might be
/// missing on the way to being filled", and D9 requires the unbuilt case to be
/// a state the tree is legitimately in, including after a full plan and
/// forever if nothing scrolls it into view.
///
/// The built arm is **boxed**, and that is a D9 decision rather than a clippy
/// waiver. An unbuilt tree is the state a lazy driver spends most of its life
/// in — `plan` returns 54,896 blocks for `5mb.md`, none of them built — so the
/// unbuilt variant has to be cheap. Boxing makes it a pointer instead of a
/// whole inline `parley::Layout`, and pays for it with one allocation at
/// exactly the moment shaping is already allocating.
#[derive(Debug)]
enum ShapeState {
    Unbuilt,
    Built(Box<ShapedText>),
}

impl ShapeState {
    fn built(&self) -> Option<&ShapedText> {
        match self {
            ShapeState::Unbuilt => None,
            ShapeState::Built(t) => Some(t),
        }
    }
}

/// The marker a list draws beside one of its items.
#[derive(Debug, Clone, PartialEq)]
enum Marker {
    /// A bullet shape, chosen by nesting depth.
    Bullet(ListMarker),
    /// An ordered list's number, already formatted with its delimiter.
    Ordered,
    /// A task item's checkbox, and whether it is ticked.
    Checkbox(bool),
}

/// One block, planned. Geometry that depends only on the tree, the theme and
/// the width is filled by [`LayoutTree::plan`]; `y`, `height` and a table
/// cell's `x`/`width` wait for [`LayoutTree::place`].
#[derive(Debug)]
struct Planned {
    node: NodeId,
    kind: BlockKind,
    children: Vec<usize>,
    units: Units,
    edges: BoxEdges,
    /// `None` for containers, which own children rather than text.
    style: Option<TextStyle>,
    text: ShapeState,
    /// The block's max-content width, measured when it was built. Kept
    /// separately so that a table's column sizing does not depend on whether
    /// the cell has been re-broken since.
    intrinsic_width: f32,
    /// D13's map, filled in by [`LayoutTree::build`] and handed to the display
    /// list. `None` for a container, and the identity for a leaf whose text is
    /// not inline markdown.
    map: Option<VisibleTextMap>,
    /// [`InlineBoxItem::id`]s of the images this block reserved a box for that
    /// have **no bitmap** — D12's `.mu-image-fail` / `.mu-empty-image`. Those
    /// are the ones `emit_block` draws a ground and an icon for; a resolved
    /// image draws neither, because `.mu-inline-image.mu-image-success` sets
    /// `background: transparent` (`inlineSyntax.css:392-394`) and the bitmap
    /// itself is the renderer's to paint into the box.
    ///
    /// [`InlineBoxItem::id`]: crate::display::InlineBoxItem::id
    image_placeholders: Vec<u64>,
    /// Synthetic text that is not the block's own: an ordered list marker, or a
    /// code block's line numbers. Never both — no block kind needs two.
    aux_text: String,
    aux_style: Option<TextStyle>,
    aux: ShapeState,
    /// The column the synthetic text wraps in. Used only by the line-number
    /// gutter, which really is a fixed-width box (`width: 2.5em`); an ordered
    /// marker does not wrap at all and is placed by its trailing edge instead.
    aux_width: f32,
    /// A footnote label's `(left, top)` against the block's padding box.
    label_offset: (f32, f32),
    marker: Option<Marker>,
    language: Option<String>,
    /// The colour text inside this block inherits — body colour, or a
    /// blockquote's `--blockquote-text-color`. Carried because a list marker's
    /// `inherit` (C4: muya's own default) resolves against the *item's* colour
    /// and the item is a container with no style of its own.
    color: Brush,
    /// The cumulative CSS `opacity` in force on this block, `1.0` outside a
    /// footnote. See [`dim`].
    opacity: f32,
    /// True for a `CodeBlock` when the gutter option is on.
    line_numbers: bool,
    content_x: f32,
    content_width: f32,
    bounds: Rect,
    bottom_y: f32,
}

impl Planned {
    fn content_y(&self) -> f32 {
        self.bounds.y + self.edges.top()
    }

    /// How far the laid-out text runs past the content box's right edge.
    ///
    /// Non-zero for an unwrapped code block (`wrapCodeBlocks: false`, the muya
    /// default) and for any block holding one unbreakable run wider than its
    /// column. See [`BlockDisplay::overflow_x`].
    fn overflow_x(&self) -> f32 {
        self.text
            .built()
            .map_or(0.0, |t| (t.width() - self.content_width).max(0.0))
    }
}

// ---------------------------------------------------------------------------
// The tree
// ---------------------------------------------------------------------------

/// A planned document: every block's style and horizontal geometry, and each
/// block's shaping state.
///
/// Holds its own copy of the [`Theme`] rather than borrowing one, because a
/// relayout must use the theme it was planned with — mixing a plan made under
/// one theme with an emit made under another is a class of bug S6's incremental
/// driver would otherwise be able to write.
#[derive(Debug)]
pub struct LayoutTree {
    theme: Theme,
    options: LayoutOptions,
    blocks: Vec<Planned>,
    roots: Vec<usize>,
    /// The text column: `min(available, content_width_px) − 2 × padding_x`.
    content_width: f32,
    /// Set by [`LayoutTree::place`].
    height: f32,
}

impl LayoutTree {
    /// Walk the document and resolve everything that does not need a font.
    ///
    /// **Shapes nothing.** Every block comes back
    /// [`unbuilt`](LayoutTree::is_built), which is D9's requirement and also
    /// what makes this call cheap enough for S6 to make on a 5 MB file before
    /// deciding what is visible.
    pub fn plan(
        document: &Document,
        theme: &Theme,
        available_width_px: f32,
        options: &LayoutOptions,
    ) -> LayoutTree {
        // `max-width` with `box-sizing: border-box`: the column never exceeds
        // the theme's, the padding is inside it, and neither can go negative.
        // `f32::min` returns the non-NaN side, so a NaN width degrades to the
        // theme's column rather than poisoning every coordinate.
        let column = available_width_px.min(theme.metrics.content_width_px);
        let content_width = (column - 2.0 * theme.metrics.container_padding_x_px).max(0.0);

        let mut tree = LayoutTree {
            theme: theme.clone(),
            options: options.clone(),
            blocks: Vec::new(),
            roots: Vec::new(),
            content_width,
            height: 0.0,
        };

        let ctx = Ctx {
            units: Units::root(theme.metrics.font_size_px, theme.metrics.root_font_size_px),
            color: Brush::resolve(theme.colors.editor, Brush::default()),
            content_x: 0.0,
            content_width,
            list_depth: 0,
            quote_depth: 0,
            in_list_item: false,
            in_tight_list: false,
            opacity: 1.0,
            tight_item_child: false,
            in_header_row: false,
        };
        let roots = document.children(document.root()).to_vec();
        tree.roots = tree.plan_children(document, theme, &roots, &ctx);
        tree
    }

    /// How many blocks the tree holds. Containers count: a blockquote is a
    /// block, and so is every list item.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether the document laid out to nothing.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The text column this tree was planned at.
    pub fn content_width(&self) -> f32 {
        self.content_width
    }

    /// The `mt_doc` node block `index` came from.
    pub fn node(&self, index: usize) -> NodeId {
        self.blocks[index].node
    }

    /// Which kind of block `index` is.
    pub fn kind(&self, index: usize) -> BlockKind {
        self.blocks[index].kind
    }

    /// Whether block `index` has every `Layout` it needs yet.
    ///
    /// A container with no marker is reported built from the start: it owns no
    /// text, so there is nothing to defer and nothing a lazy driver would gain
    /// by visiting it. An ordered list's *item* is a container that still has
    /// something to shape — its number — so the question is asked of both
    /// slots.
    pub fn is_built(&self, index: usize) -> bool {
        let b = &self.blocks[index];
        let text = b.style.is_none() || matches!(b.text, ShapeState::Built(_));
        let aux = b.aux_style.is_none() || matches!(b.aux, ShapeState::Built(_));
        text && aux
    }

    /// How many blocks have been built. The number a lazy driver reports and
    /// the number `building_one_block_builds_no_other` asserts on.
    pub fn built_count(&self) -> usize {
        (0..self.blocks.len()).filter(|&i| self.is_built(i)).count()
    }

    /// How many lines block `index`'s text broke into, or `None` if it holds no
    /// text or has not been built.
    ///
    /// Public because it is the only way to state D9's *"one `Layout` per
    /// fence, never one per line"* as an assertion rather than as a comment: a
    /// 200-line fence must be **one** block whose **one** `ShapedText` reports
    /// 200 lines.
    pub fn shaped_line_count(&self, index: usize) -> Option<usize> {
        self.blocks[index].text.built().map(ShapedText::line_count)
    }

    /// Block `index`'s laid-out text size, or `None` if it holds no text or has
    /// not been built.
    pub fn shaped_size(&self, index: usize) -> Option<(f32, f32)> {
        self.blocks[index]
            .text
            .built()
            .map(|t| (t.width(), t.height()))
    }

    /// Where block `index`'s text starts, in document coordinates. Only
    /// meaningful after [`LayoutTree::place`].
    pub fn content_origin(&self, index: usize) -> (f32, f32) {
        let b = &self.blocks[index];
        (b.content_x, b.content_y())
    }

    /// Block `index`'s content width — the column its text wraps at.
    pub fn block_content_width(&self, index: usize) -> f32 {
        self.blocks[index].content_width
    }

    /// Shape exactly one block, and nothing else.
    ///
    /// Idempotent, and independent: it reads the block's own planned style, its
    /// own text out of `document`, and no other block's anything. That
    /// independence is D9's requirement and is asserted by
    /// `building_one_block_builds_no_other`.
    pub fn build(
        &mut self,
        index: usize,
        document: &Document,
        fonts: &mut Fonts,
        shaper: &mut TextShaper,
    ) {
        if self.is_built(index) {
            return;
        }
        if let Some(style) = self.blocks[index].style.clone() {
            // A stale handle lays out as empty rather than panicking or
            // borrowing another block's text: M2 D8's generational arena
            // reports the staleness, and losing a block's content is a better
            // answer than showing someone else's.
            // `Text::to_str` borrows for the inline representation and only
            // materializes a rope, so this copies nothing for the blocks there
            // are thousands of and copies once for the rare 8 KiB-plus fence.
            let node = self.blocks[index].node;
            let text = document
                .get(node)
                .and_then(|n| n.block())
                .and_then(|b| b.text())
                .map_or(std::borrow::Cow::Borrowed(""), |t| t.to_str());
            let content_width = self.blocks[index].content_width;
            let opacity = self.blocks[index].opacity;
            // The two halves of C6. A leaf whose text is inline markdown is
            // tokenized and shaped as its *visible* text; every other leaf's
            // text is already what a reader sees, so it is shaped verbatim and
            // gets the identity map rather than no map at all.
            let (shaped, map) = if self.blocks[index].kind.lays_out_inline_markdown() {
                let laid = inline::lay_out(
                    &text,
                    &self.options.inline_syntax,
                    style.base_direction,
                    None,
                );
                let runs = self.style_runs(&style, &laid.runs, opacity);
                let grounds = self.grounds(&style, &laid.runs, opacity);
                let (boxes, placeholders) = self.image_boxes(&laid.images, content_width);
                let shaped = self.shape_with(
                    &style,
                    &laid.visible,
                    &runs,
                    &grounds,
                    &boxes,
                    content_width,
                    fonts,
                    shaper,
                );
                self.blocks[index].image_placeholders = placeholders;
                (shaped, laid.map)
            } else {
                let shaped = self.shape(&style, &text, &[], content_width, fonts, shaper);
                (shaped, VisibleTextMap::identity(text.len()))
            };
            self.blocks[index].intrinsic_width = shaped.width();
            self.blocks[index].text = ShapeState::Built(Box::new(shaped));
            self.blocks[index].map = Some(map);

            // The gutter's text is a function of where this block's own lines
            // fell, so it can only be generated now — and it is generated as
            // one string for the whole fence, not one per number (D9).
            if self.blocks[index].line_numbers {
                let numbers = {
                    let shaped = self.blocks[index].text.built().expect("just built it");
                    line_number_text(&text, shaped)
                };
                self.blocks[index].aux_text = numbers;
            }
        }
        if let Some(aux_style) = self.blocks[index].aux_style.clone() {
            let aux_text = std::mem::take(&mut self.blocks[index].aux_text);
            let width = self.blocks[index].aux_width;
            // Synthetic text — a list marker, a line-number gutter, a
            // footnote's `[^id]:` label. It is not in any document, so there is
            // nothing to tokenize and no map to build.
            let shaped = self.shape(&aux_style, &aux_text, &[], width, fonts, shaper);
            self.blocks[index].aux_text = aux_text;
            self.blocks[index].aux = ShapeState::Built(Box::new(shaped));
        }
    }

    /// Build every block. S1's eager driver, and deliberately nothing more than
    /// a loop over [`LayoutTree::build`].
    pub fn build_all(&mut self, document: &Document, fonts: &mut Fonts, shaper: &mut TextShaper) {
        for i in 0..self.blocks.len() {
            self.build(i, document, fonts, shaper);
        }
    }

    /// Assign every block a `y` and a height.
    ///
    /// An unbuilt block contributes **zero** text height. That is not a
    /// silently wrong answer so much as the place S6 has to put an estimator;
    /// the module doc says exactly what it needs.
    pub fn place(&mut self) {
        let mut flow = Flow::default();
        for root in self.roots.clone() {
            self.place_block(root, &mut flow);
        }
        flow.resolve(&mut self.blocks);

        // Rule 4: the document's leading margin is dropped, so the first
        // block's border edge lands at the container's top padding.
        let lead = self.roots.first().map_or(0.0, |&r| self.blocks[r].bounds.y);
        let dy = self.theme.metrics.container_padding_top_px - lead;
        let mut bottom = self.theme.metrics.container_padding_top_px;
        for b in &mut self.blocks {
            b.bounds.y += dy;
            b.bottom_y += dy;
            b.bounds.height = (b.bottom_y - b.bounds.y).max(0.0);
            bottom = bottom.max(b.bottom_y);
        }
        // …and the trailing margin with it: the height is the last block's
        // bottom edge, not the flow cursor past its margin.
        self.height = bottom + self.theme.metrics.container_padding_bottom_px;
    }

    /// Walk the placed tree and produce the display list.
    ///
    /// # Errors
    ///
    /// See [`layout`].
    pub fn emit(&self, fonts: &Fonts) -> Result<DisplayList, FontError> {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            blocks.push(self.emit_block(b, fonts)?);
        }
        Ok(DisplayList {
            width: self.content_width,
            height: self.height,
            blocks,
        })
    }

    // --- shaping -----------------------------------------------------------

    fn families(&self, stack: FontStack) -> &[String] {
        match stack {
            FontStack::Body => &self.theme.fonts.body,
            FontStack::Code => &self.theme.fonts.code,
            FontStack::FootnoteLabel => &self.theme.footnote.label_fonts,
        }
    }

    fn shape(
        &self,
        style: &TextStyle,
        text: &str,
        runs: &[StyleRun<'_>],
        content_width: f32,
        fonts: &mut Fonts,
        shaper: &mut TextShaper,
    ) -> ShapedText {
        self.shape_with(style, text, runs, &[], &[], content_width, fonts, shaper)
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_with(
        &self,
        style: &TextStyle,
        text: &str,
        runs: &[StyleRun<'_>],
        grounds: &[TextGround],
        boxes: &[InlineBoxSpec],
        content_width: f32,
        fonts: &mut Fonts,
        shaper: &mut TextShaper,
    ) -> ShapedText {
        let mut request = TextRequest::new(
            text,
            self.families(style.stack),
            style.font_size,
            style.line_height,
        );
        request.weight = style.weight;
        request.align = style.align;
        request.brush = style.brush;
        request.letter_spacing = style.letter_spacing;
        request.base_direction = style.base_direction;
        request.runs = runs;
        request.grounds = grounds;
        request.inline_boxes = boxes;
        request.max_width = match style.wrap {
            Wrap::AtContentWidth => Some(content_width.max(0.0)),
            Wrap::Never => None,
        };
        shaper.shape(fonts, &request)
    }

    /// Resolve `mt-inline`'s semantic runs against the theme and the block's
    /// own style.
    ///
    /// The split is deliberate: [`crate::inline`] owns no theme and says only
    /// *"this stretch is strong"*, and this is the one place that says what a
    /// strong run's weight is. Adjacent runs whose resolved style is identical
    /// are merged, and any run that ends up identical to the block's own style
    /// is dropped — because a style-run boundary is a shaping boundary, so a
    /// needless push costs a kerning pair and a paragraph with no markup must
    /// shape exactly as it did before any of this existed.
    ///
    /// # The colour cascade is CSS's, not a precedence table
    ///
    /// `colors.strong` and `colors.em` default to `inherit`
    /// (`inlineSyntax.css:49-57` sets `color` on `<strong>`/`<em>` and nothing
    /// else), and `inherit` for a `<strong>` inside an `<a>` is **the link's
    /// colour**, not the paragraph's. So the brush is built by applying each
    /// enclosing element's rule in the order the DOM nests them — link, then
    /// emphasis, then code — rather than by picking a winner. `dark` sets both
    /// to real colours and is where the difference shows.
    fn style_runs(&self, style: &TextStyle, runs: &[InlineRun], opacity: f32) -> Vec<StyleRun<'_>> {
        let theme = &self.theme;
        let base = self.families(style.stack);
        let mut out: Vec<StyleRun<'_>> = Vec::new();
        for run in runs {
            let s = run.style;
            let mut font_size = style.font_size;
            let mut families = base;
            if s.code {
                // `font-size: 0.8em` against the **surrounding** text, so
                // inline code in an h1 is 24px — `inlineSyntax.css:65`. The
                // stack is the theme's own literal list rather than the code
                // block's, which is the point of `fonts.inline_code` being a
                // separate field.
                font_size *= theme.inline_code.font_size_em;
                families = &theme.fonts.inline_code;
            }
            if s.footnote {
                font_size *= theme.inline.footnote_identifier_font_size_em;
            }
            let mut brush = style.brush;
            if s.link {
                brush = inline_brush(theme.colors.link, brush, opacity);
            }
            if s.strong {
                brush = inline_brush(theme.colors.strong, brush, opacity);
            }
            if s.em {
                brush = inline_brush(theme.colors.em, brush, opacity);
            }
            if s.code {
                // `color: var(--editor-color)` — and note it is the editor's
                // colour and not the inherited one, so inline code inside a
                // blockquote does **not** take `blockquote_text`.
                brush = inline_brush(theme.colors.editor, brush, opacity);
            }
            let resolved = StyleRun {
                range: run.range.clone(),
                families,
                font_size,
                weight: if s.strong {
                    theme.inline.strong_weight
                } else {
                    style.weight
                },
                italic: s.em && theme.inline.em_italic,
                brush,
                // Both are the UA sheet reading the face — see `StyleRun`.
                underline: s.link,
                strikethrough: s.del,
            };
            match out.last_mut() {
                Some(last)
                    if last.range.end == resolved.range.start
                        && last.families == resolved.families
                        && last.font_size == resolved.font_size
                        && last.weight == resolved.weight
                        && last.italic == resolved.italic
                        && last.brush == resolved.brush
                        && last.underline == resolved.underline
                        && last.strikethrough == resolved.strikethrough =>
                {
                    last.range.end = resolved.range.end;
                }
                _ => out.push(resolved),
            }
        }
        // A run that changes nothing is no run at all. Dropped **after**
        // merging rather than never emitted, because two adjacent runs that
        // each differ from the default may be identical to each other, and
        // dropping first would leave the merge unable to see that. This is what
        // makes a paragraph with no markup produce an empty list, and a
        // paragraph with one link produce exactly one entry.
        out.retain(|r| {
            r.families != base
                || r.font_size != style.font_size
                || r.weight != style.weight
                || r.italic
                || r.brush != style.brush
                || r.underline
                || r.strikethrough
        });
        out
    }

    /// The rounded grounds behind inline code and behind a footnote
    /// identifier's pill.
    ///
    /// Not part of [`LayoutTree::style_runs`] because a ground is not a text
    /// style: it changes no glyph, it is painted before the glyphs rather than
    /// with them, and it survives the `retain` that drops a run which changes
    /// nothing. Both paddings are in the **run's own** `em`, which is what a
    /// CSS length on the same rule as a `font-size` means.
    fn grounds(&self, style: &TextStyle, runs: &[InlineRun], opacity: f32) -> Vec<TextGround> {
        let theme = &self.theme;
        let bg = inline_brush(theme.colors.code_block_bg, style.brush, opacity);
        if !bg.is_visible() {
            return Vec::new();
        }
        let mut out: Vec<TextGround> = Vec::new();
        for run in runs {
            // Code wins over the footnote pill when both are set: a `[^id]`
            // cannot contain a code span, so the pair is unreachable, and
            // choosing rather than emitting two overlapping rects is the answer
            // that stays right if it ever becomes reachable.
            let (em, padding_x_em, padding_y_em, radius) = if run.style.code {
                (
                    style.font_size * theme.inline_code.font_size_em,
                    theme.inline_code.padding_x_em,
                    theme.inline_code.padding_y_em,
                    theme.inline_code.corner_radius_px,
                )
            } else if run.style.footnote {
                (
                    style.font_size * theme.inline.footnote_identifier_font_size_em,
                    theme.inline.footnote_identifier_padding_x_em,
                    theme.inline.footnote_identifier_padding_y_em,
                    theme.inline.footnote_identifier_corner_radius_px,
                )
            } else {
                continue;
            };
            let ground = TextGround {
                range: run.range.clone(),
                padding_x: em * padding_x_em,
                padding_y: em * padding_y_em,
                corner_radius: radius,
                brush: bg,
            };
            match out.last_mut() {
                Some(last)
                    if last.range.end == ground.range.start
                        && last.padding_x == ground.padding_x
                        && last.padding_y == ground.padding_y
                        && last.corner_radius == ground.corner_radius =>
                {
                    last.range.end = ground.range.end;
                }
                _ => out.push(ground),
            }
        }
        out
    }

    /// D12: one [`InlineBoxSpec`] per inline image, plus the ids of the ones
    /// that need the no-bitmap chrome drawn behind them.
    ///
    /// The id is the image token's **block-text start offset**, which is unique
    /// within a leaf and is what matches a box in the display list back to the
    /// token that produced it without a side table.
    fn image_boxes(
        &self,
        images: &[InlineImage],
        content_width: f32,
    ) -> (Vec<InlineBoxSpec>, Vec<u64>) {
        let mut boxes = Vec::with_capacity(images.len());
        let mut placeholders = Vec::new();
        for image in images {
            let id = image.block.start as u64;
            let size = self.options.images.get(&image.src);
            let (width, height) = match size {
                // `max-width: 100%` on the container *and* on the `<img>`
                // (`inlineSyntax.css:374-390`), which for a replaced element
                // scales the height with it rather than squashing it.
                Some(size) if size.width_px > content_width && size.width_px > 0.0 => {
                    let scale = content_width / size.width_px;
                    (content_width, size.height_px * scale)
                }
                Some(size) => (size.width_px, size.height_px),
                // No entry is a completed failure, not a pending load, so it is
                // `.mu-image-fail`/`.mu-empty-image`: `width: 100%` of the
                // containing block and `height: 50px`
                // (`inlineSyntax.css:434-440`). A box as wide as the column
                // cannot share a line with anything, which is the visible
                // consequence D12 predicts.
                None => {
                    placeholders.push(id);
                    (
                        content_width.max(0.0),
                        self.theme.inline.image_placeholder_height_px,
                    )
                }
            };
            boxes.push(InlineBoxSpec {
                id,
                index: image.visible_index,
                width,
                height,
                // `.mu-inline-image` is an `inline-block` with `font-size: 0`,
                // `line-height: 0` and an empty container, so its baseline is
                // its bottom margin edge — and `.mu-image-loading` says
                // `vertical-align: bottom` in as many words
                // (`inlineSyntax.css:442-448`). `None` is exactly that
                // instruction; see `InlineBoxSpec::baseline`.
                baseline: None,
            });
        }
        (boxes, placeholders)
    }

    fn text_height(&self, index: usize) -> f32 {
        self.blocks[index]
            .text
            .built()
            .map_or(0.0, ShapedText::height)
    }

    // --- placement ---------------------------------------------------------

    fn place_block(&mut self, index: usize, flow: &mut Flow) {
        let edges = self.blocks[index].edges;
        let kind = self.blocks[index].kind;
        let is_leaf = self.blocks[index].style.is_some();
        let has_children = !self.blocks[index].children.is_empty();

        flow.add_margin(edges.margin_top);

        // Rule 2, and the "zero children get a content edge" simplification.
        let collapse_top = !is_leaf && has_children && edges.top() == 0.0;
        if collapse_top {
            flow.pending_tops.push(index);
        } else {
            flow.resolve(&mut self.blocks);
            self.blocks[index].bounds.y = flow.y;
            flow.y += edges.top();
        }

        if kind == BlockKind::Table {
            self.place_table(index, flow);
        } else if is_leaf {
            flow.y += self.text_height(index);
        } else {
            for child in self.blocks[index].children.clone() {
                self.place_block(child, flow);
            }
        }

        let collapse_bottom =
            !is_leaf && has_children && edges.bottom() == 0.0 && kind != BlockKind::Table;
        if collapse_bottom {
            self.blocks[index].bottom_y = flow.y;
        } else {
            // Rule 3: with bottom padding present, the last child's margin
            // materializes *inside* it rather than escaping.
            flow.resolve(&mut self.blocks);
            flow.y += edges.bottom();
            self.blocks[index].bottom_y = flow.y;
        }
        flow.add_margin(edges.margin_bottom);
    }

    /// Rows and cells are placed here rather than through [`Self::place_block`]
    /// because a table is not normal flow: no margin collapses inside it, and a
    /// cell's width is decided by its whole column rather than by its parent.
    ///
    /// # The column rule, and it is an approximation rather than a port
    ///
    /// CSS's automatic table layout is a two-pass algorithm over per-column
    /// min-content and max-content widths with a distribution rule that browsers
    /// implement differently from each other and from the spec. Porting it is
    /// not worth what it costs. What is implemented:
    ///
    /// 1. Each column's width is the **max-content** width of its widest cell —
    ///    the cell's text shaped unbroken — plus that cell's horizontal padding.
    /// 2. If the columns fit, that is the table's width. It is **not** stretched
    ///    to the content column: muya sets no `width: 100%` on
    ///    `.mu-table-inner`, so a narrow table really is narrow.
    /// 3. If they do not fit, every column is scaled by the same factor until
    ///    they do, and the cells re-break inside.
    ///
    /// Step 3 is where a browser and this differ most: a browser distributes the
    /// shortfall between each column's min-content and max-content width, and
    /// overflows the container when even the min-contents do not fit. Scaling
    /// proportionally is simpler, always fits, and is wrong in the same
    /// direction for every column. **Recorded as a divergence a later stage may
    /// need to revisit**, not as a port.
    ///
    /// `table.cell_min_width_em` is deliberately **not** applied as a floor —
    /// see `the_ten_em_cell_floor_is_a_dead_declaration_in_the_reference`.
    ///
    /// Cell content is top-aligned. The UA default is baseline alignment, which
    /// differs only when cells in one row have different font metrics; here they
    /// never do, because the only per-cell style difference is the header row's
    /// weight and a header row is entirely headers.
    fn place_table(&mut self, index: usize, flow: &mut Flow) {
        let rows = self.blocks[index].children.clone();
        let avail = self.blocks[index].content_width;
        let table_x = self.blocks[index].content_x;

        let mut cols: Vec<f32> = Vec::new();
        for &row in &rows {
            for (j, &cell) in self.blocks[row].children.iter().enumerate() {
                let e = self.blocks[cell].edges;
                let want = self.blocks[cell].intrinsic_width + e.padding_left + e.padding_right;
                if j >= cols.len() {
                    cols.push(0.0);
                }
                cols[j] = cols[j].max(want);
            }
        }
        let total: f32 = cols.iter().sum();
        if total > avail && total > 0.0 {
            let scale = avail / total;
            for c in cols.iter_mut() {
                *c *= scale;
            }
        }
        let table_width: f32 = cols.iter().sum();

        for &row in &rows {
            let row_top = flow.y;
            let cells = self.blocks[row].children.clone();
            let mut row_height = 0.0f32;
            let mut x = table_x;
            for (j, &cell) in cells.iter().enumerate() {
                let width = cols.get(j).copied().unwrap_or(0.0);
                let e = self.blocks[cell].edges;
                let inner = (width - e.padding_left - e.padding_right).max(0.0);
                let align = self.blocks[cell]
                    .style
                    .as_ref()
                    .map_or(TextAlign::Start, |s| s.align);
                self.blocks[cell].bounds.x = x;
                self.blocks[cell].bounds.width = width;
                self.blocks[cell].content_x = x + e.padding_left;
                self.blocks[cell].content_width = inner;
                if let ShapeState::Built(t) = &mut self.blocks[cell].text {
                    t.rebreak(Some(inner), align);
                }
                row_height =
                    row_height.max(self.text_height(cell) + e.padding_top + e.padding_bottom);
                x += width;
            }
            for &cell in &cells {
                self.blocks[cell].bounds.y = row_top;
                self.blocks[cell].bottom_y = row_top + row_height;
            }
            let r = &mut self.blocks[row];
            r.bounds.x = table_x;
            r.bounds.width = table_width;
            r.content_x = table_x;
            r.content_width = table_width;
            r.bounds.y = row_top;
            r.bottom_y = row_top + row_height;
            flow.y += row_height;
        }
    }

    // --- emit --------------------------------------------------------------

    fn emit_block(&self, b: &Planned, fonts: &Fonts) -> Result<BlockDisplay, FontError> {
        let colors = &self.theme.colors;
        let mut items: Vec<DisplayItem> = Vec::new();

        // Backgrounds first, then borders, then decorations, then glyphs —
        // `BlockDisplay::items` is paint order.
        // Every brush below is dimmed by the block's cumulative CSS opacity;
        // `style.brush` was already dimmed at plan time.
        let paint_brush = |c| dim(Brush::resolve(c, Brush::default()), b.opacity);

        if b.kind.uses_code_block_box() {
            let bg = paint_brush(colors.code_block_bg);
            if bg.is_visible() {
                items.push(DisplayItem::Rect(FilledRect {
                    rect: b.bounds,
                    corner_radius: self.theme.code_block.corner_radius_px,
                    rotation_deg: 0.0,
                    brush: bg,
                }));
            }
            paint::push_border(
                &mut items,
                b.bounds,
                self.theme.code_block.border_width_px,
                paint_brush(colors.editor_10),
            );
        }

        match b.kind {
            BlockKind::Footnote => {
                let bg = paint_brush(colors.editor_04);
                if bg.is_visible() {
                    items.push(DisplayItem::Rect(FilledRect::new(b.bounds, bg)));
                }
            }
            BlockKind::BlockQuote => {
                items.push(DisplayItem::Rect(paint::blockquote_bar(
                    &self.theme,
                    b.bounds,
                    paint_brush(colors.blockquote_border),
                )));
            }
            BlockKind::TableCell => {
                // `blockSyntax.css:621-632` draws the border on an absolutely
                // positioned `::before` that is `calc(100% + 1px)` in both
                // axes, so a cell's right and bottom borders land on top of its
                // neighbours' left and top ones. That is what
                // `border-collapse: collapse` looks like when it is emulated
                // rather than implemented, and reproducing the overlay
                // reproduces the collapse.
                let w = self.theme.table.cell_border_px;
                paint::push_border(
                    &mut items,
                    Rect::new(
                        b.bounds.x,
                        b.bounds.y,
                        b.bounds.width + w,
                        b.bounds.height + w,
                    ),
                    w,
                    paint_brush(colors.table_border),
                );
            }
            BlockKind::ThematicBreak => {
                items.push(DisplayItem::Line(paint::thematic_break_rule(
                    &self.theme,
                    Rect::new(
                        b.content_x,
                        b.content_y(),
                        b.content_width,
                        b.bounds.height - b.edges.top() - b.edges.bottom(),
                    ),
                    paint_brush(colors.hr),
                )));
            }
            _ => {}
        }

        if let Some(marker) = &b.marker {
            self.emit_marker(b, marker, &mut items, fonts)?;
        }

        // The thematic break's source text is `opacity: 0` unless the caret is
        // in it (`blockSyntax.css:189-191`), and a read-only viewer has no
        // caret — C6's `cursor: None` seen from the block level. It is still
        // shaped, because the block's height is the height of the line box that
        // hidden text occupies.
        if b.kind != BlockKind::ThematicBreak {
            if let Some(text) = b.text.built() {
                let first = items.len();
                text.emit(fonts, b.content_x, b.content_y(), &mut items)?;
                self.emit_image_placeholders(b, first, &mut items);
            }
        }

        if b.kind == BlockKind::Footnote {
            if let Some(label) = b.aux.built() {
                // `position: absolute; top: 0.2em; left: 0` against the
                // figure's padding box, plus the label's own `padding: 0 1em`
                // (`blockSyntax.css:1012-1019`). The figure has no border, so
                // its padding box is its border box.
                let (dx, dy) = b.label_offset;
                label.emit(fonts, b.bounds.x + dx, b.bounds.y + dy, &mut items)?;
            }
        }

        if b.line_numbers {
            if let Some(numbers) = b.aux.built() {
                let em = b.units.own_px();
                let cb = &self.theme.code_block;
                // `position: absolute; left: 0; top: 1em` against the code
                // block's padding box (`blockSyntax.css:325-332`).
                numbers.emit(
                    fonts,
                    b.bounds.x + b.edges.border,
                    b.bounds.y + b.edges.border + em * cb.gutter_top_em,
                    &mut items,
                )?;
            }
        }

        Ok(BlockDisplay {
            node: b.node,
            kind: b.kind,
            bounds: b.bounds,
            language: b.language.clone(),
            overflow_x: b.overflow_x(),
            items,
            text_map: b.map.clone(),
        })
    }

    /// D12's chrome for an image with no bitmap: the ground, and the icon's
    /// square.
    ///
    /// Appended after the text rather than interleaved with it, because a
    /// placeholder is `width: 100%` of the column and so shares its line with
    /// nothing — there is no glyph anywhere it can paint over. It is skipped
    /// entirely for an image the shell did size, whose ground the reference
    /// makes transparent (`inlineSyntax.css:392-394`) and whose bitmap belongs
    /// to the renderer.
    ///
    /// **The label is not drawn**, and that is D12's decision rather than an
    /// omission: it is `content: attr(fail-text)` where the attribute is
    /// `i18n.t('Load image failed')` (`image.ts:89-90`) — a localized UI string
    /// and not document content. M3 has no locale, and an invented English one
    /// would put chrome inside a display list that is also PDF export's input.
    /// Owed to S5, which is the first thing in this project with a UI language.
    fn emit_image_placeholders(&self, b: &Planned, from: usize, items: &mut Vec<DisplayItem>) {
        if b.image_placeholders.is_empty() {
            return;
        }
        let inline = &self.theme.inline;
        let ground = dim(
            Brush::resolve(self.theme.colors.code_block_bg, Brush::default()),
            b.opacity,
        );
        // The icon is a font glyph in the reference and this repository has no
        // icon set, so what is drawn is its box. `--icon-color` has no theme
        // field either; `editor_50` is the neutral chrome colour the same
        // stylesheet uses for every other unemphasised mark.
        let icon = dim(
            Brush::resolve(self.theme.colors.editor_50, Brush::default()),
            b.opacity,
        );
        let mut chrome = Vec::new();
        for item in &items[from..] {
            let DisplayItem::InlineBox(bx) = item else {
                continue;
            };
            if !b.image_placeholders.contains(&bx.id) {
                continue;
            }
            if ground.is_visible() {
                chrome.push(DisplayItem::Rect(FilledRect {
                    rect: bx.rect(),
                    corner_radius: inline.image_corner_radius_px,
                    rotation_deg: 0.0,
                    brush: ground,
                }));
            }
            if icon.is_visible() {
                chrome.push(DisplayItem::Rect(FilledRect::new(
                    Rect::new(
                        bx.x + inline.image_icon_inset_px,
                        bx.y + inline.image_icon_inset_px,
                        inline.image_icon_px,
                        inline.image_icon_px,
                    ),
                    icon,
                )));
            }
        }
        items.append(&mut chrome);
    }

    fn emit_marker(
        &self,
        b: &Planned,
        marker: &Marker,
        items: &mut Vec<DisplayItem>,
        fonts: &Fonts,
    ) -> Result<(), FontError> {
        let em = b.units.own_px();
        let metrics = &self.theme.metrics;
        // The first line box's vertical midpoint — the same anchor muya uses for
        // the checkbox (`blockSyntax.css:502`), reused for bullets so that a
        // task list and a bullet list at the same font size mark the same row.
        let mid = b.content_y() + 0.5 * metrics.line_height * em;
        let right = b.content_x - paint::MARKER_GAP_EM * em;

        match marker {
            Marker::Bullet(shape) => paint::push_bullet(
                items,
                shape,
                b.content_x,
                b.content_y(),
                right,
                mid,
                em,
                Brush::resolve(self.theme.colors.list_marker, b.color),
                Brush::resolve(self.theme.colors.editor_bg, Brush::default()),
            ),
            Marker::Ordered => {
                if let Some(number) = b.aux.built() {
                    // Placed by its trailing edge, on the same rule the bullets
                    // use, and free to extend left past the indent.
                    number.emit(fonts, right - number.width(), b.content_y(), items)?;
                }
            }
            Marker::Checkbox(checked) => {
                let cb = &self.theme.checkbox;
                let top = match &cb.top {
                    // `calc(--mu-line-height * 0.5 * --mu-font-size - 7px)`.
                    // The editor's metrics and not the item's `em`, because the
                    // CSS reads the custom properties on purpose — a Chromium
                    // `<input>` ignores an inherited `font-size` and an `em`
                    // here would freeze at the UA form-control size.
                    CheckboxTop::LineCentered(t) => {
                        metrics.line_height * 0.5 * metrics.font_size_px - t.offset_px
                    }
                    CheckboxTop::Em(t) => t.em * em,
                };
                paint::push_checkbox(
                    items,
                    &self.theme,
                    b.content_x + cb.inset_inline_start_px,
                    b.content_y() + top,
                    *checked,
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The flow cursor
// ---------------------------------------------------------------------------

/// The collapsed-margin cursor.
///
/// `pending_margin` is the maximum of every margin in the run that has not yet
/// been materialized, which is what makes three adjacent `0.5em` margins
/// collapse to one `0.5em` rather than to `1.5em`. `pending_tops` holds the
/// blocks whose top border edge is *inside* that run — the collapse-through
/// containers of rule 2 — and they all resolve to the same `y` when the run
/// ends, which is exactly what "the parent's edge is where the collapsed margin
/// ends" means.
#[derive(Debug, Default)]
struct Flow {
    y: f32,
    pending_margin: f32,
    pending_tops: Vec<usize>,
}

impl Flow {
    fn add_margin(&mut self, margin: f32) {
        self.pending_margin = self.pending_margin.max(margin);
    }

    fn resolve(&mut self, blocks: &mut [Planned]) {
        self.y += self.pending_margin;
        self.pending_margin = 0.0;
        for index in self.pending_tops.drain(..) {
            blocks[index].bounds.y = self.y;
        }
    }
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// What a block inherits from its ancestors.
#[derive(Debug, Clone, Copy)]
struct Ctx {
    units: Units,
    color: Brush,
    content_x: f32,
    content_width: f32,
    /// How many **bullet or ordered** lists enclose this node. Drives the
    /// disc → circle → square cascade, and counts exactly what the selectors
    /// at `blockSyntax.css:445-456` can see: every one of them requires a
    /// `ul.mu-bullet-list` or `ol.mu-order-list` ancestor, and a task list
    /// carries neither class (`gfm/taskList/index.ts:42`). So a task list in
    /// the chain is **transparent** — it neither adds a level nor resets one.
    list_depth: u32,
    quote_depth: u32,
    in_list_item: bool,
    /// This node is a list item of a list whose `loose` is false, i.e. one
    /// muya gave `mu-tight-list` to.
    in_tight_list: bool,
    /// The product of every ancestor's CSS `opacity`. See [`dim`].
    opacity: f32,
    /// This node is a direct child of such an item — the `p` of
    /// `.mu-tight-list > li > p` (`blockSyntax.css:401-404`).
    tight_item_child: bool,
    in_header_row: bool,
}

impl LayoutTree {
    fn plan_children(
        &mut self,
        document: &Document,
        theme: &Theme,
        nodes: &[NodeId],
        ctx: &Ctx,
    ) -> Vec<usize> {
        nodes
            .iter()
            .filter_map(|&node| self.plan_block(document, theme, node, ctx))
            .collect()
    }

    // The theme arrives as a parameter rather than through `self.theme`
    // because the walk mutates `self.blocks`, and cloning 148 fields per block
    // to satisfy the borrow checker would put a real cost on the one pass D9
    // needs to stay cheap on a 5 MB file.
    fn plan_block(
        &mut self,
        document: &Document,
        theme: &Theme,
        node: NodeId,
        ctx: &Ctx,
    ) -> Option<usize> {
        let block = document.get(node)?.block()?;
        let kind = BlockKind::from(block);
        let colors = &theme.colors;
        let metrics = &theme.metrics;

        let mut units = ctx.units.child();
        let mut edges = BoxEdges::default();
        let mut style: Option<TextStyle> = None;
        let mut child_color = ctx.color;
        let mut line_numbers = false;
        let mut aux_style: Option<TextStyle> = None;
        let mut aux_width = 0.0f32;
        let mut child_opacity = ctx.opacity;
        let mut label: Option<(String, TextStyle, f32, f32)> = None;

        match block {
            Block::Paragraph { .. } => {
                // `.mu-tight-list > li > p { margin: 0; padding: 0 }`
                // (`blockSyntax.css:400-404`), at a specificity that beats
                // `.mu-container p`. muya pushes `mu-tight-list` whenever a
                // list's `meta.loose` is false — `bulletList/index.ts:46-47`,
                // `orderList/index.ts:44-45`, `gfm/taskList/index.ts:43-44` —
                // so `Block::{BulletList,OrderList,TaskList}::loose` is a
                // layout input and not just round-trip metadata. Only the
                // inter-item gaps go; the list's own outer margin is
                // untouched.
                if !ctx.tight_item_child {
                    edges.margin_top = units.em(metrics.block_margin_em);
                    edges.margin_bottom = edges.margin_top;
                }
                style = Some(body_style(&units, metrics.line_height, ctx.color));
            }
            Block::ThematicBreak { .. } => {
                // muya renders it as a paragraph carrying the hidden source, so
                // it takes the paragraph margin and a full line box.
                edges.margin_top = units.em(metrics.block_margin_em);
                edges.margin_bottom = edges.margin_top;
                style = Some(body_style(&units, metrics.line_height, ctx.color));
            }
            Block::AtxHeading { level, .. } | Block::SetextHeading { level, .. } => {
                let level = (*level).clamp(1, 6) as usize;
                let h = &theme.headings;
                units = units.with_font_size(units.font_size_em(h.scale_em[level - 1]));
                edges.margin_top = units.rem(h.margin_rem);
                edges.margin_bottom = edges.margin_top;
                let inherited = Brush::resolve(colors.editor_80, ctx.color);
                let mut s = body_style(
                    &units,
                    h.line_height,
                    Brush::resolve(colors.heading[level - 1], inherited),
                );
                s.weight = if h.bold { 700 } else { 400 };
                s.align = h.align;
                style = Some(s);
            }
            Block::CodeBlock { .. }
            | Block::HtmlBlock { .. }
            | Block::MathBlock { .. }
            | Block::Frontmatter { .. }
            | Block::Diagram { .. } => {
                let cb = &theme.code_block;
                units = units.with_font_size(units.font_size_pct(cb.font_size_pct));
                edges.margin_top = units.em(cb.margin_top_em);
                edges.margin_bottom = units.em(cb.margin_bottom_em);
                edges.border = cb.border_width_px;
                let pad = units.em(cb.padding_em);
                edges.padding_top = pad;
                edges.padding_right = pad;
                edges.padding_bottom = pad;
                edges.padding_left = pad;
                line_numbers = self.options.code_block_line_numbers
                    && matches!(block, Block::CodeBlock { .. });
                if line_numbers {
                    edges.padding_left = units.em(cb.gutter_width_em);
                    // `text-align: right` inside the gutter less each number's
                    // own `padding-right: 0.8em` (`blockSyntax.css:359`).
                    aux_width = units.em(cb.gutter_width_em - cb.gutter_padding_end_em);
                    aux_style = Some(gutter_style(
                        &units,
                        cb,
                        Brush::resolve(colors.editor_30, Brush::default()),
                    ));
                }
                let mut s = body_style(
                    &units,
                    cb.line_height,
                    Brush::resolve(colors.editor_50, ctx.color),
                );
                s.stack = FontStack::Code;
                // `wrapCodeBlocks: false` is muya's default, and the base rule
                // is `overflow: auto` over the UA's `white-space: pre`. An
                // unwrapped fence overflows its box rather than growing it;
                // `BlockDisplay::overflow_x` is how far.
                if !self.options.wrap_code_blocks {
                    s.wrap = Wrap::Never;
                }
                style = Some(s);
            }
            Block::BlockQuote { .. } => {
                let bq = &theme.blockquote;
                units = units.with_font_size(units.font_size_em(bq.font_size_em));
                edges.margin_top = units.em(bq.margin_em);
                edges.margin_bottom = edges.margin_top;
                edges.padding_left = bq.padding_x_px;
                // `blockquote blockquote { padding-right: 0 }`.
                edges.padding_right = if ctx.quote_depth > 0 {
                    bq.nested_padding_end_px
                } else {
                    bq.padding_x_px
                };
                child_color = Brush::resolve(colors.blockquote_text, ctx.color);
            }
            Block::BulletList { .. } | Block::OrderList { .. } | Block::TaskList { .. } => {
                let lists = &theme.lists;
                // `li > ol.mu-order-list, li > ul.mu-bullet-list { margin: 0 }`
                // — `blockSyntax.css:421-425`. **`ul.mu-task-list` is not in
                // that selector**, and `gfm/taskList/index.ts:42` gives it only
                // `MU_TASK_LIST`, so a task list nested in a list item keeps
                // the shared `margin: 0.5em 0`. Unreachable from today's
                // corpus; modelled anyway, because a corpus file that acquires
                // the shape later would rewrite a golden with no visible cause.
                let nested_zeroed = ctx.in_list_item && !matches!(block, Block::TaskList { .. });
                let margin = if nested_zeroed {
                    lists.nested_margin_em
                } else {
                    lists.margin_em
                };
                edges.margin_top = units.em(margin);
                edges.margin_bottom = edges.margin_top;
                edges.padding_left = if matches!(block, Block::TaskList { .. }) {
                    lists.task_indent_px
                } else {
                    lists.indent_px
                };
            }
            Block::ListItem { .. } | Block::TaskListItem { .. } | Block::TableRow { .. } => {}
            Block::Table { .. } => {
                let t = &theme.table;
                edges.margin_top = units.em(t.figure_margin_top_em);
                edges.margin_bottom = units.em(t.figure_margin_bottom_em);
                // C2: the figure's own `padding: 0.5em 0` plus the inner
                // `<table>`'s `margin: 0.5em 0`, which `.mu-table-inner` never
                // resets and which cannot collapse out through the padding.
                // Net clearance is 1.5em above and 1em below, and §2's
                // block-spacing row is wrong to list `table` under the 0.5em
                // rule — `figure:not(.mu-table)` excludes it explicitly.
                edges.padding_top = units.em(t.figure_padding_y_em + t.inner_margin_em);
                edges.padding_bottom = edges.padding_top;
            }
            Block::TableCell { align, .. } => {
                let t = &theme.table;
                edges.padding_top = t.cell_padding_y_px;
                edges.padding_bottom = t.cell_padding_y_px;
                edges.padding_left = t.cell_padding_x_px;
                edges.padding_right = t.cell_padding_x_px;
                let mut s = body_style(&units, metrics.line_height, ctx.color);
                s.align = match align {
                    Align::None | Align::Left => TextAlign::Start,
                    Align::Center => TextAlign::Center,
                    Align::Right => TextAlign::End,
                };
                s.weight = if ctx.in_header_row && t.header_bold {
                    700
                } else {
                    400
                };
                s.wrap = Wrap::Never;
                style = Some(s);
            }
            Block::Footnote { identifier, .. } => {
                let f = &theme.footnote;
                units = units.with_font_size(units.font_size_em(f.font_size_em));
                edges.margin_top = units.em(f.margin_y_em);
                edges.margin_bottom = edges.margin_top;
                edges.padding_top = units.em(f.padding_top_em);
                edges.padding_bottom = units.em(f.padding_bottom_em);
                edges.padding_left = units.em(f.padding_start_em);
                edges.padding_right = units.em(f.padding_end_em);
                // `opacity: 0.8` is on the whole figure, so it reaches every
                // descendant — see `dim`.
                child_opacity = ctx.opacity * f.opacity;
                // The `[^id]:` label: an absolutely-positioned box against the
                // figure's padding box, at an **absolute** 14px in a monospace
                // generic, so its `top` and `padding` resolve against 14 and
                // not against the footnote's 12.8. `::before` is `[^` and
                // `::after` is `]:` (`blockSyntax.css:1029-1035`), which is why
                // the brackets are here and not in `identifier`.
                let mut s = body_style(
                    &units.child().with_font_size(f.label_font_size_px),
                    metrics.line_height,
                    dim(
                        Brush::resolve(colors.editor, Brush::default()),
                        child_opacity,
                    ),
                );
                s.stack = FontStack::FootnoteLabel;
                s.weight = f.label_weight;
                s.wrap = Wrap::Never;
                label = Some((
                    format!("[^{identifier}]:"),
                    s,
                    f.label_padding_x_em * f.label_font_size_px,
                    f.label_top_em * f.label_font_size_px,
                ));
            }
        }

        // Every brush this block will paint with is dimmed once, here, by the
        // opacity in force on it. `child_opacity` and not `ctx.opacity`:
        // `opacity` on an element dims the element itself as well as its
        // subtree, so a footnote's own tint is already at 0.8.
        if let Some(style) = &mut style {
            style.brush = dim(style.brush, child_opacity);
        }

        let content_x = ctx.content_x + edges.border + edges.padding_left;
        let content_width =
            (ctx.content_width - edges.padding_left - edges.padding_right - 2.0 * edges.border)
                .max(0.0);

        // The footnote label rides in the aux slot, the same one an ordered
        // marker uses: no block kind needs two pieces of synthetic text.
        let mut label_offset = (0.0f32, 0.0f32);
        let mut aux_text = String::new();
        if let Some((text, style, dx, dy)) = label {
            aux_text = text;
            aux_style = Some(style);
            label_offset = (dx, dy);
        }

        let index = self.blocks.len();
        self.blocks.push(Planned {
            node,
            kind,
            children: Vec::new(),
            units,
            edges,
            style,
            text: ShapeState::Unbuilt,
            intrinsic_width: 0.0,
            map: None,
            image_placeholders: Vec::new(),
            aux_text,
            aux_style,
            aux: ShapeState::Unbuilt,
            aux_width,
            label_offset,
            marker: None,
            language: code_language(block).map(str::to_owned),
            color: child_color,
            opacity: child_opacity,
            line_numbers,
            content_x,
            content_width,
            bounds: Rect::new(ctx.content_x, 0.0, (ctx.content_width).max(0.0), 0.0),
            bottom_y: 0.0,
        });

        if let Some(children) = block.children() {
            let is_list = matches!(
                block,
                Block::BulletList { .. } | Block::OrderList { .. } | Block::TaskList { .. }
            );
            // Only a bullet or ordered list is visible to the marker cascade;
            // see `Ctx::list_depth`.
            let is_marker_list =
                matches!(block, Block::BulletList { .. } | Block::OrderList { .. });
            let is_item = matches!(block, Block::ListItem { .. } | Block::TaskListItem { .. });
            let tight = match block {
                Block::BulletList { loose, .. }
                | Block::OrderList { loose, .. }
                | Block::TaskList { loose, .. } => !*loose,
                _ => false,
            };
            let child_ctx = Ctx {
                units,
                color: child_color,
                content_x,
                content_width,
                list_depth: ctx.list_depth + u32::from(is_marker_list),
                quote_depth: ctx.quote_depth + u32::from(matches!(block, Block::BlockQuote { .. })),
                in_list_item: is_item,
                // A list tells its items whether it is tight; an item passes
                // that on to its own direct children, which is exactly the
                // reach of `.mu-tight-list > li > p`.
                in_tight_list: if is_list { tight } else { false },
                opacity: child_opacity,
                tight_item_child: is_item && ctx.in_tight_list,
                in_header_row: ctx.in_header_row,
            };
            let child_nodes = children.to_vec();
            let planned = if matches!(block, Block::Table { .. }) {
                // The first row is the header, and `th { font-weight: bold }`
                // reaches its cells and no others.
                child_nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(row, &child)| {
                        let mut ctx = child_ctx;
                        ctx.in_header_row = row == 0;
                        self.plan_block(document, theme, child, &ctx)
                    })
                    .collect()
            } else {
                self.plan_children(document, theme, &child_nodes, &child_ctx)
            };
            if is_list {
                self.assign_markers(document, theme, block, ctx.list_depth, &planned);
            }
            self.blocks[index].children = planned;
        }

        Some(index)
    }

    /// Give each of a list's items its marker.
    ///
    /// Done from the list rather than the item because every input is the
    /// list's: the bullet shape comes from how deep the *list* is nested, and an
    /// ordered item's number comes from the list's `start` plus the item's
    /// position in it.
    fn assign_markers(
        &mut self,
        document: &Document,
        theme: &Theme,
        list: &Block,
        depth_before: u32,
        items: &[usize],
    ) {
        let markers = &theme.lists.bullet_markers;
        // `ul ul { circle }`, `ul ul ul { square }`, and the three-deep
        // selector still matches at four, so the last entry repeats.
        let bullet = markers
            .get(depth_before as usize)
            .or_else(|| markers.last())
            .cloned();

        for (position, &item) in items.iter().enumerate() {
            let marker = match list {
                Block::OrderList {
                    start, delimiter, ..
                } => {
                    let n = start.saturating_add(position as u32);
                    let brush = Brush::resolve(theme.colors.list_marker, self.blocks[item].color);
                    self.blocks[item].aux_text = ordinal(theme.lists.ordered_marker, n, *delimiter);
                    self.blocks[item].aux_style = Some(ordered_marker_style(
                        &self.blocks[item].units,
                        theme.metrics.line_height,
                        brush,
                    ));
                    Some(Marker::Ordered)
                }
                _ => {
                    // A task item draws a checkbox and `list-style-type: none`
                    // (`blockSyntax.css:484-486`), whatever list it is in.
                    match document.get(self.blocks[item].node).and_then(|n| n.block()) {
                        Some(Block::TaskListItem { checked, .. }) => {
                            Some(Marker::Checkbox(*checked))
                        }
                        _ => bullet.clone().map(Marker::Bullet),
                    }
                }
            };
            self.blocks[item].marker = marker;
        }
    }
}

// ---------------------------------------------------------------------------
// Style helpers
// ---------------------------------------------------------------------------

/// Apply a CSS `opacity` to a brush.
///
/// # This is an approximation, and the approximation is deliberate
///
/// CSS `opacity` is a **group** operation: the subtree is composited first and
/// the result is then made translucent. The display list has no layer
/// primitive and D8's contract is that `mt-render` computes no geometry and
/// makes no decisions, so a group opacity would mean asking every renderer to
/// allocate an offscreen surface for one block kind.
///
/// Instead each primitive's own alpha is multiplied. The two differ only where
/// two translucent things inside the group overlap — for a footnote that is
/// glyphs over the `--editor-color-04` tint, both of which are opaque before
/// dimming, so the visible difference is the tint showing through the text at
/// 0.8 rather than the text sitting on an already-composited 0.8 background.
/// **Recorded as a divergence** rather than hidden: if M6's PDF export needs
/// exact group semantics, this is the function to replace and
/// `BlockDisplay` is where the group would have to be declared.
/// An inline run's colour: `inherit` takes the surrounding brush, which the
/// block's opacity has already dimmed, and anything else is an absolute colour
/// that has not been and must be.
fn inline_brush(color: Color, inherited: Brush, opacity: f32) -> Brush {
    match color {
        Color::Inherit => inherited,
        _ => dim(Brush::resolve(color, inherited), opacity),
    }
}

fn dim(brush: Brush, opacity: f32) -> Brush {
    if opacity >= 1.0 {
        return brush;
    }
    let a = (f32::from(brush.a) * opacity.clamp(0.0, 1.0)).round();
    Brush::rgba(brush.r, brush.g, brush.b, a as u8)
}

fn body_style(units: &Units, line_height: f32, brush: Brush) -> TextStyle {
    TextStyle {
        stack: FontStack::Body,
        font_size: units.own_px(),
        line_height,
        weight: 400,
        align: TextAlign::Start,
        brush,
        wrap: Wrap::AtContentWidth,
        letter_spacing: 0.0,
        base_direction: BaseDirection::Ltr,
    }
}

/// The line-number gutter's text style.
///
/// Two approximations, both forced by the display list having no transform:
///
/// - `transform: scale(0.8)` (`blockSyntax.css:364`) is a *paint* scale in CSS,
///   which leaves the line box at the code's size and shrinks the glyphs inside
///   it. Here the glyphs are shaped at `0.8 ×` the code size and the line-height
///   multiple is raised by the reciprocal, so the **line pitch is identical**
///   and the numbers stay level with their code lines. What differs is the
///   baseline within each line box, by the difference between centring an
///   11.52px glyph and painting a scaled 14.4px one.
/// - `text-align: right` inside `2.5em − 0.8em` of usable gutter, from
///   `gutter_width_em` and the per-number `padding-right`.
fn gutter_style(units: &Units, cb: &CodeBlock, brush: Brush) -> TextStyle {
    let scale = if cb.gutter_scale > 0.0 {
        cb.gutter_scale
    } else {
        1.0
    };
    TextStyle {
        stack: FontStack::Code,
        font_size: units.own_px() * scale,
        line_height: cb.line_height / scale,
        weight: 400,
        align: TextAlign::End,
        brush,
        wrap: Wrap::AtContentWidth,
        letter_spacing: cb.gutter_letter_spacing_px,
        base_direction: BaseDirection::Ltr,
    }
}

/// An ordered marker's text style.
///
/// Unwrapped, and left-aligned in a box of its own width: a CSS `outside`
/// marker is placed by its **trailing** edge and grows leftwards from there,
/// so a `100.` in a 30px indent hangs further into the gutter rather than
/// spilling over the item's text. Right-aligning inside a fixed strip would do
/// the opposite, which is how the first version of this got it wrong.
fn ordered_marker_style(units: &Units, line_height: f32, brush: Brush) -> TextStyle {
    let mut s = body_style(units, line_height, brush);
    s.wrap = Wrap::Never;
    s
}

/// An ordered list's marker text.
///
/// # A deliberate divergence from Chromium, recorded rather than hidden
///
/// CSS `list-style: decimal` renders `1.` whatever the source wrote, because
/// the marker's suffix is the counter style's and not the document's. So muya
/// shows `1.` for a `1)` list. The brief for this phase asks for the
/// **delimiter** to be honoured, and it is: `1)` renders `1)`. That is more
/// faithful to the source and less faithful to the browser, and it is the one
/// place in this module where those two pull apart.
fn ordinal(style: OrderedMarker, n: u32, delimiter: OrderDelim) -> String {
    let body = match style {
        OrderedMarker::Decimal => n.to_string(),
        OrderedMarker::LowerAlpha => alphabetic(n, false),
        OrderedMarker::UpperAlpha => alphabetic(n, true),
        OrderedMarker::LowerRoman => roman(n, false),
        OrderedMarker::UpperRoman => roman(n, true),
    };
    let suffix = match delimiter {
        OrderDelim::Period => '.',
        OrderDelim::Paren => ')',
    };
    format!("{body}{suffix}")
}

/// CSS `lower-alpha` / `upper-alpha`: bijective base-26, so 26 is `z` and 27 is
/// `aa`. Zero has no representation in the counter style; CSS falls back to
/// decimal, and so does this.
fn alphabetic(n: u32, upper: bool) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let base = if upper { b'A' } else { b'a' };
    let mut out = Vec::new();
    let mut n = n;
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push(base + rem as u8);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII by construction")
}

/// CSS `lower-roman` / `upper-roman`, which is defined for 1..=3999 and falls
/// back to decimal outside it.
fn roman(n: u32, upper: bool) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    let mut out = String::new();
    let mut n = n;
    for (value, glyph) in TABLE {
        while n >= value {
            out.push_str(glyph);
            n -= value;
        }
    }
    if upper { out.to_uppercase() } else { out }
}

/// The gutter's text: one number per **source** line, padded with blank lines so
/// that a source line which wrapped onto several visual rows keeps its single
/// number level with the row it started on.
///
/// One string and therefore one `Layout` for the whole fence — D9's rule
/// applied to the decoration as well as to the content.
fn line_number_text(text: &str, shaped: &ShapedText) -> String {
    let visual: Vec<usize> = std::iter::once(0).chain(shaped.break_offsets()).collect();
    let sources: Vec<Range<usize>> = source_lines(text);
    let mut out = String::new();
    let mut cursor = 0usize;
    for (i, line) in sources.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&(i + 1).to_string());
        // How many visual rows this source line occupies.
        let mut rows = 0usize;
        while cursor < visual.len() && visual[cursor] < line.end {
            cursor += 1;
            rows += 1;
        }
        for _ in 1..rows.max(1) {
            out.push('\n');
        }
    }
    out
}

/// Byte ranges of the text's newline-separated lines. A trailing newline does
/// not open a further line, matching how a code fence's body is stored.
fn source_lines(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            out.push(start..i + 1);
            start = i + 1;
        }
    }
    if start < text.len() || out.is_empty() {
        out.push(start..text.len());
    }
    out
}

#[cfg(test)]
#[path = "flow/tests.rs"]
mod tests;
