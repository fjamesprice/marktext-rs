//! # `mt-layout` — block flow + inline layout
//!
//! ## Contract
//!
//! Consume a [`Document`](mt_doc::Document) plus a [`Theme`] plus an available
//! width, produce a **positioned display list**. Nothing is drawn here;
//! geometry is decided here and only here.
//!
//! Layout is the single source of truth for geometry, shared by screen
//! rendering, PDF export, hit-testing, and print. One engine, three consumers
//! — which is why PDF export matches the screen *by construction* (§8).
//!
//! ## The display list is neutral, and that is a decision (M3 §5 D8)
//!
//! §5 sketched `BlockContent::Text { layout: parley::Layout<Brush>, … }` — a
//! public type of this crate exposing parley's. **It does not.** A
//! [`parley::Layout`] is held per block inside [`text::ShapedText`], whose
//! field is private, and no public signature in this crate names a parley
//! type. What crosses the seam is [`DisplayList`]: positioned [`GlyphRun`]s,
//! [`InlineBoxItem`]s, [`FilledRect`]s and [`StrokedLine`]s over plain `f32`,
//! with this crate's own [`Brush`] — never `kurbo`, never `peniko`.
//!
//! The reason is the consumers rather than purity. `mt-export`'s PDF writer
//! (M6) computes no geometry of its own and takes this list as its input;
//! exposing `parley::Layout` would make a milestone two ahead depend on the
//! internal walk of a git-pinned 0.x crate. [`BlockKind`] carries a
//! discriminator for the same reason: D11 lays math and diagrams out as their
//! own source in the code-block style, and `mt-math`/`mt-diagram` replace that
//! arm at M6 by finding it, not by re-plumbing layout.
//!
//! The font reference is the one named exception, and it is resolved rather
//! than deferred: a [`FontId`] indexes the collection the shell supplied, keyed
//! on parley's own blob identity, so the handle never crosses.
//!
//! ## No I/O, and the collection arrives from above (M3 §5 D7)
//!
//! - **No GPU.** Turning the display list into pixels is `mt-render`'s job.
//!   M3 §5 D1 ships `vello_cpu` alone behind the §6 trait; `tiny-skia` is
//!   struck outright, because its README puts text rendering out of scope and
//!   a backend that cannot draw a character is not a fallback (§4 C2).
//! - **No windowing.** No `winit`, no surfaces, no event loop. Available width
//!   arrives as an `f32` parameter.
//! - **No I/O.** parley is depended on with `default-features = false`,
//!   because its default `system` feature enables filesystem font
//!   enumeration. Faces are registered from **bytes** through [`Fonts`]; this
//!   crate enumerates nothing and opens nothing. [`FaceList`] says which files
//!   to read and how to wire them, and the shell does the reading.
//! - **No dependency on `mt-ui` or `mt-app`,** ever. Nor on `mt-md`: D11's
//!   language table lives on [`mt_doc::DiagramKind`] so that both consumers
//!   share one table, which also keeps D5's "must not reference
//!   `mt_md::reparse`" true by construction.
//!
//! An empty registration result is a **hard error**, never a warning. skrifa
//! rejects `.woff` silently, and a font that fails to load without saying so
//! produces a subtly wrong layout everywhere with no diagnostic (§8 M3-R10).
//!
//! ## Why parley is a direct fit (§0, §5)
//!
//! Because muya stores leaf text as a raw string and re-derives inline
//! structure by tokenizing it, inline content is **styled runs over a single
//! string plus a handful of replaced elements** (image, inline math, emoji).
//! That is exactly parley's data model. There is no arbitrary inline-widget
//! nesting to invent.
//!
//! Cursor motion and hit-testing must go through parley's cluster API so that
//! grapheme clusters, bidi runs and ligatures behave correctly without bespoke
//! Unicode code — but D8 fixes the *shape* of that answer without pretending
//! to know its content: if M4 needs cluster-level behaviour, this crate grows
//! a **method** (point in, offset out) rather than exposing the `Layout`.
//!
//! ## Incrementality, on the axis that turned out to be real (§4 C5, §5 D9)
//!
//! §5 said re-wrapping happens only on width change, and even then only for
//! visible blocks. **The axis is the other way round.** parley re-linebreaks
//! and re-aligns an existing `Layout` cheaply, but a content or style change
//! requires a *new* one — measured over `5mb.md` at **54 ms to re-break every
//! block against 810 ms to rebuild them**, a 15.1× ratio. So a width change is
//! the cheap case and needs no viewport heuristic at all; laziness is needed
//! for the **first** layout, not for reflow.
//!
//! Which is why "a block that has no `Layout` yet" is a **first-class state**
//! here from S1 rather than a cache bolted on later. S0 measured shaping at
//! 90.3 % of layout cost, so a design that defers only line-breaking defers
//! 8 %. [`LayoutTree`] holds `Unbuilt` blocks as an enum, not an `Option`;
//! building one block touches no other, and the eager driver is a loop over
//! the per-block build so that S6 can substitute a viewport-driven one without
//! changing the data structure.
//!
//! Two constants that shape every design decision above them. A
//! `parley::Layout` costs **3,956 B resident** against a 328 B struct, so cost
//! scales with **block count × a fixed per-`Layout` price**, not with document
//! size — ns/byte actually *falls* as documents grow. One `Layout` per code
//! fence, never one per line: the split measured 1.38× slower and 2.21× the
//! memory. **Any design decision that multiplies `Layout` count is the
//! expensive one, whatever it optimizes.**
//!
//! ## Status
//!
//! **M3 S2, in progress.** Block flow, the theme model (D4), the face list
//! (D7), the display list (D8) and the golden format (D10) landed at S1.
//! S2 adds per-leaf tokenization into style runs with the markers hidden
//! ([`inline`]), D13's [`VisibleTextMap`] on the display list, and the
//! [`InlineBoxSpec`] plumbing that lets a caller reserve a hole in a line.
//! `**bold**` is four columns now, not eight.
//!
//! It also places D12's inline images — sized from an [`ImageSizes`] table the
//! shell builds, and taking the reference's own no-bitmap geometry when the
//! shell resolved nothing — and draws the inline decorations: inline code's
//! ground, a footnote identifier's pill, and the `del`/link rules, whose
//! thickness and offset come from the **face's** metrics because muya has no
//! CSS for either and a browser would read the font too.
//!
//! What S2 still owes: the emoji shortcode, which is copied verbatim because
//! `mt-inline` ships no name table, and the deliberate line-by-line
//! cross-check of the emitted geometry against `inlineSyntax.css`, which §6
//! puts *before* the goldens are regenerated because a golden written first
//! freezes whatever it was shown.
//!
//! Geometry is guarded two ways: `theme::tests` asserts §2's table as
//! constants, and `cargo xtask layout` compares the display list for every
//! corpus file at both theme widths against committed goldens on **exact
//! equality**. A golden update is a reviewable event on the same footing as
//! moving the parley pin (§8 M3-R6) — both show up as a one-line header diff
//! above the geometry that moved.

pub mod display;
pub mod flow;
pub mod fonts;
pub mod highlight;
pub mod images;
pub mod inline;
// Private: every item in it is `pub(crate)`. D8's public surface is the
// display list, not the recipe that fills it.
mod paint;
pub mod text;
pub mod theme;
pub mod units;

pub use display::{
    BlockDisplay, BlockKind, Brush, DisplayItem, DisplayList, FilledRect, Glyph, GlyphRun,
    InlineBoxFlow, InlineBoxItem, Rect, StrokedLine,
};
pub use flow::{LayoutOptions, LayoutTree, code_language, layout, layout_with};
pub use fonts::{
    Axis, BUNDLED_FACES_TOML, Face, FaceList, FaceListMeta, FaceRole, FaceStyle, FontError, FontId,
    Fonts,
};
pub use highlight::{CodeSpans, HighlightSpan};
pub use images::{ImageSize, ImageSizes};
pub use inline::{
    InlineImage, InlineRun, InlineStyle, InlineSyntax, InlineText, MapKind, MapRun, RTL_MARK,
    VisibleTextMap,
};
pub use text::{
    BaseDirection, InlineBoxSpec, InlineSpacing, ShapedText, StyleRun, TextCluster, TextGround,
    TextRequest, TextShaper,
};
pub use theme::{Theme, ThemeError};
pub use units::Units;
