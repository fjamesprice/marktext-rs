//! # `mt-layout` — block flow + inline layout
//!
//! ## Contract
//!
//! Consume a `Document` plus a theme, produce a **positioned display list**.
//! Nothing is drawn here; geometry is decided here and only here.
//!
//! Layout is the single source of truth for geometry, shared by screen
//! rendering, PDF export, hit-testing, and print. One engine, three consumers
//! — which is why PDF export matches the screen *by construction* (§8).
//!
//! ## Dependency constraints
//!
//! Per §1:
//!
//! - **No GPU.** `mt-layout` produces a display list; turning it into pixels
//!   is `mt-render`'s job, behind a trait with both a `vello`/`wgpu` and a
//!   `tiny-skia` backend. Nothing here may assume either exists.
//! - **No windowing.** No `winit`, no surfaces, no event loop. Available width
//!   arrives as an `f32` parameter.
//! - **No I/O.** Fonts arrive through `fontique`'s collection, not through
//!   `std::fs` calls made here.
//! - **No dependency on `mt-ui` or `mt-app`,** ever.
//!
//! This is the load-bearing constraint for §11.3's golden-data layout tests:
//! layout is testable headlessly against golden data on a CI runner with no
//! display and no GPU.
//!
//! ## Why parley is a direct fit (§0, §5)
//!
//! Because muya stores leaf text as a raw string and re-derives inline
//! structure by tokenizing it, inline content is **styled runs over a single
//! string plus a handful of replaced elements** (image, inline math, emoji).
//! That is exactly `parley`'s data model. There is no arbitrary inline-widget
//! nesting to invent — the "inline-flow layout with arbitrary nested widgets"
//! hard part from the earlier plan largely evaporates.
//!
//! Build a `RangedBuilder` over the block's text, push style ranges derived
//! from the token tree, push `InlineBox` placeholders for the replaced
//! elements. Parley returns lines, runs, glyph positions, and cluster
//! boundaries — which is simultaneously the shaping, bidi, font-fallback and
//! line-breaking answer, *and* the hit-testing and cursor-motion answer.
//! Cursor motion and hit-testing must go through parley's cluster API so that
//! grapheme clusters, bidi runs, and ligatures behave correctly without
//! bespoke Unicode code.
//!
//! ## Incrementality is the performance contract (§5)
//!
//! A dirty block re-lays out; blocks after it get their `y` shifted by the
//! height delta; nothing else is touched. Re-wrapping the whole document
//! happens only on width change — and even then only for visible blocks plus
//! a viewport margin, with the rest laid out lazily on scroll.
//!
//! **Never lay out the whole document synchronously.** This is precisely what
//! Electron cannot do, and it is where the 5 MB-file target (§12.1: open in
//! ≤ 800 ms) is won or lost.
//!
//! ## M0 status
//!
//! Stub. `mt-layout` is M3 (§9). §13 R2 asks for the awkward parley cases —
//! inline-widget baseline alignment, nested bidi interacting with inline
//! boxes — to be exercised in **M3 week 1, not month 5**.

pub mod display;
pub mod flow;
pub mod fonts;
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
pub use text::{BaseDirection, ShapedText, TextRequest, TextShaper};
pub use theme::{Theme, ThemeError};
pub use units::Units;
