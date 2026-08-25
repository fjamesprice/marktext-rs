//! # `mt-render` — display list → pixels
//!
//! ## Contract
//!
//! Rasterize the [`DisplayList`](mt_layout::DisplayList) `mt-layout` produced,
//! as seen through a caller-supplied viewport, into a buffer of premultiplied
//! sRGB RGBA8. That is the whole of it. There is no scene graph, no retained
//! state between frames beyond a reusable context, no window, and no file.
//!
//! ## What this crate must NOT do, and it is one rule
//!
//! **`mt-render` computes no geometry.** §6's S4 gate says so and D18 states
//! the reviewable form of it, which is narrower and more useful than *"no
//! arithmetic"*:
//!
//! > no `DisplayItem` is drawn at a coordinate not already on it, and no
//! > coordinate is derived from anything but the caller's scroll offset and
//! > viewport.
//!
//! That admits a translate and excludes a layout. Concretely, and this list is
//! exhaustive so that a review can check it rather than believe it — every
//! coordinate arithmetic in this crate is one of:
//!
//! | Where | What | Why it is not layout |
//! |---|---|---|
//! | [`cpu`] | `Affine::translate((-viewport.x, -viewport.y))`, set once per frame | D18's named exception: the scroll offset |
//! | [`cull`] | `Rect::max_x()`/`max_y()` on a block's `paint_bounds` and on the viewport | comparing two rectangles the caller supplied invents no position |
//! | [`cpu`] | `Rect::max_x()`/`max_y()` in `to_kurbo` | the same `x + width`, on the way to kurbo's corner-pair form; every rect and every clip passes through here, which is why it is a row of its own rather than covered by the one above |
//! | [`cpu`] | `Rect::new(0, 0, w, h)` for the ground, from the **target's** size | the ground is the surface, not a `DisplayItem`; no item is placed by it |
//! | [`cpu`] | `RoundedRect::from_rect` corner arcs, and `to_path`'s flattening | realizing a shape the item asked for, the same class as the dash row below |
//! | [`cpu`] | `rotation_deg.to_radians()` | a unit conversion of a number already on the item |
//! | [`cpu`] | `kurbo::Rect::center()`, for [`FilledRect::rotation_deg`](mt_layout::FilledRect::rotation_deg) | the item says *"rotate me about my centre"*; the centre is a reading of the item, not a placement decision |
//! | [`cpu`] | dash lengths as multiples of [`StrokedLine::width`](mt_layout::StrokedLine::width) | realizing a stroke style, the same class of work as rasterizing a glyph outline |
//!
//! Nothing here measures text, shapes a run, reads a font metric, breaks a
//! line, or asks how wide anything is. If it did, PDF export (M6, through the
//! *same* list) and the screen would already have diverged, and §8's *"export
//! matches the screen by construction"* would be a hope rather than a
//! consequence.
//!
//! ## One backend, and the M0 stub's two were both struck (§4 C1, C2; §5 D1)
//!
//! This crate's stub described *"GPU (`vello`/`wgpu`) with a CPU
//! (`tiny-skia`) fallback"*, a `swash` glyph atlas, and *"the CPU fallback is
//! an M3 deliverable, not an afterthought"*. **Every clause of that is now
//! false**, and this paragraph replaces it rather than sitting beside it,
//! because a stub that contradicts the milestone plan is worse than an empty
//! one — it is the first thing a reader of the crate believes.
//!
//! - **`tiny-skia` is struck outright** (C2). Its own README puts text
//!   rendering out of scope, and a backend that cannot draw a character is not
//!   a fallback for a markdown previewer.
//! - **`wgpu` never enters the M3 graph** (C1, D1). E2 built it — a real
//!   `vello::Scene`, real glyphs, a real device, a real readback
//!   (`spikes/e2-vello-gpu/src/main.rs`) — and priced it: **+4,317,696 B** of
//!   `dist` on top of the **2,481,664 B** that `vello_cpu` + parley already
//!   costs, to draw a page of static text on a machine that also has to render
//!   correctly in a VM with no GPU (R7).
//! - **`swash` is not in this stack.** `vello_cpu` rasterizes glyphs through
//!   `glifo`/`skrifa` and owns its own atlas; there is no atlas to write here
//!   and no cache key to invent.
//!
//! So D1 ships **`vello_cpu` alone**, and [`VelloCpuRenderer`] is the only
//! implementor of [`Renderer`] that exists or is planned.
//!
//! ## The trait exists anyway, and D20 says what it may be shaped by
//!
//! D1's argument for keeping §6's trait is that a backend swap should be a
//! stage rather than a rewrite (R2: `vello_cpu` 0.2.0 is eight weeks old and
//! offers no API stability). **A trait with one implementor, designed against
//! a hypothetical second one, is the standard way that argument fails**: it
//! ends up shaped like the implementor it has.
//!
//! D20's answer is that the second backend is not hypothetical — it is 174
//! committed lines in `spikes/e2-vello-gpu/` — so the intersection can be
//! *read*:
//!
//! | | `vello_cpu` 0.2.0 | `vello` + `wgpu` |
//! |---|---|---|
//! | glyph run | `glyph_run(…).font_size(s).hint(true).fill_glyphs(…)` | `draw_glyphs(…).font_size(s).hint(true).draw(…)` |
//! | filled path | `fill_path` | `Scene::fill` |
//! | stroked path | `stroke_path` | `Scene::stroke` |
//! | clip | `push_clip_layer` / `pop_layer` | `Scene::push_layer` / `pop_layer` |
//! | ground colour | pixmap clear | `RenderParams::base_color` |
//! | out | `render_with(&mut Pixmap, …)` | `render_to_texture` + readback |
//!
//! [`Renderer::render`] is exactly that intersection collapsed to its unit of
//! work: **a `DisplayList` plus a [`Frame`], and a buffer of pixels out** — not
//! a scene, not a command list, not a device. It returns [`FrameStats`] because
//! D17's measurement has to state *"how many of `5mb.md`'s 54,896 blocks
//! intersect the viewport"* beside its frame time, and counting them twice is
//! how the two numbers come to disagree.
//!
//! **The gate clause this makes honest**: §6's S4 row says *"if two backends
//! ship, the same display list through both agrees within that threshold."*
//! One backend ships, so that clause has no content and must be reported as
//! having none rather than ticked.
//!
//! ## The font handle is the seam's one exception, and D16 is who closes it
//!
//! Every field on the display list is an `f32`, a `u8`, a `bool` or a
//! `Range<usize>` — except [`GlyphRun::font`](mt_layout::GlyphRun::font),
//! which is a [`FontId`](mt_layout::FontId): an index into the collection the
//! shell registered under D7. `Fonts::resolve` and `Fonts::context_mut` are
//! `pub(crate)`, deliberately, so this crate cannot turn that index into bytes
//! and **must not try**.
//!
//! D16: *the shell builds a `FontId → peniko::FontData` table in the same
//! order it built the `Fonts` collection, and `mt-render` receives it.*
//! [`FontTable`] is that table. `mt-layout` grows no accessor and **this crate
//! names no parley type**, which is the whole reason `FontId` is an index in
//! the first place (`fonts.rs:79-84`): taking a parley `FontData` accessor
//! would put a git-pinned 0.x crate on the public surface of the two consumers
//! D8 exists to protect.
//!
//! The handle type costs no dependency line: `vello_cpu` re-exports
//! `vello_common::{color, kurbo, peniko}`, so [`FontData`] arrives with the
//! rasterizer. And blob *identity* need not hold — glyph ids are per-face and
//! already resolved on the list, so two `FontData` values over the same file
//! and index draw identically even though `Blob::id()` is process-unique. **The
//! table has to agree on file and index, not on identity.**
//!
//! What can go wrong is two orderings that drift, and the mitigation is the
//! shell's: assert equal length and equal file name per index against
//! `Fonts::file_name` **before** rendering. A wrong face makes a perfectly
//! well-formed PNG, which is `assert_corpus_fully_covered`'s argument pointed
//! at a second artifact.
//!
//! ## Clipping is an obligation, not an optimization
//!
//! `display.rs:250-256`: muya's default is `wrapCodeBlocks: false` over
//! `pre { white-space: pre }`, so a long fence line **scrolls, it does not
//! wrap** — the box stays the column and the glyphs inside it really do carry
//! coordinates right of `bounds.max_x()`. A renderer that assumes containment
//! paints a fence's overflow across the page and produces a golden that looks
//! deliberate. [`BlockDisplay::overflow_x`](mt_layout::BlockDisplay::overflow_x)
//! is how much that clipping hides.
//!
//! [`cpu`]'s module doc records **which** blocks are clipped and the measured
//! reason the answer is not *"all of them"*.
//!
//! ## Dependency constraints
//!
//! `mt-render` is the first crate in the graph permitted to rasterize, and it
//! consumes `mt-layout` downward. It must not depend on `mt-ui` or `mt-app`:
//! widgets and windows are above it. It reads no file — [`FontTable`] arrives
//! built, exactly as `Fonts` does one layer down — and it opens no window.
//!
//! It is **not** one of the four crates `cargo xtask deps` holds headless, and
//! that is the point of the boundary rather than a gap in it: `mt-doc`,
//! `mt-inline`, `mt-md` and `mt-layout` compile and test without a GPU because
//! everything that could need one lives here.
//!
//! ## Redraw policy: on event only (§6, §12.1)
//!
//! No animation loop, no timers, no idle polling; the caret blink at M4 is the
//! sole timer and it stops when the window loses focus. Nothing in this crate
//! can start a loop — it has no event source and no window — which is the
//! cheapest possible enforcement of a target (*"~0 % idle CPU"*) that is
//! otherwise lost by one stray `request_redraw()`.
//!
//! Dirty-rect rendering and scroll blitting are **S6's**, and D17 fixes a frame
//! as a *full-viewport repaint from a warm display list* precisely so that they
//! are headroom rather than assumptions: everything S4 leaves out only makes
//! the number better, so a full repaint that clears 16.67 ms is a bound no
//! later stage can erode.
//!
//! ## Status
//!
//! **M3 S4, in progress.** The four [`DisplayItem`](mt_layout::DisplayItem)
//! arms, D20's trait, D16's table and D18's culling are here. What S4 still
//! owes: D17's scroll measurement and D1's verdict, the cross-check of the
//! emitted paint against the theme and the reference, and D19's pixel goldens —
//! in that order, and the goldens last, because *the review of the thing about
//! to be frozen and the review of the freeze catch different classes, and
//! neither substitutes for the other* (R6).
//!
//! One arm draws nothing on purpose:
//! [`InlineBox`](mt_layout::DisplayItem::InlineBox). M3 *places* replaced
//! elements and fills none of them — the image decoder is M4's and the math and
//! diagram renderers are M6's (`display.rs:574-579`). Nothing in this workspace
//! decodes an image and the corpus resolves zero of them, so the hole stays
//! empty. The failed-image placeholder's chrome is already on the list as
//! ordinary [`Rect`](mt_layout::DisplayItem::Rect) items and must not be drawn
//! a second time.

// Public, like `mt-layout`'s, because the reasoning lives in the module docs
// and a reader who wants to know *which blocks are clipped and why not all of
// them* should be able to reach the paragraph that answers it. Nothing here
// needs hiding: this crate has no recipe to keep, only a walk.
pub mod backend;
pub mod cpu;
pub mod cull;
pub mod fonts;

pub use backend::{Frame, FrameStats, Pixels, RenderError, Renderer};
pub use cpu::{NUM_THREADS, RENDER_LEVEL, VelloCpuRenderer};
pub use cull::{intersects, is_visible_in};
pub use fonts::FontTable;

/// The font handle the shell hands across under D16.
///
/// Re-exported so a caller building a [`FontTable`] does not have to name
/// `vello_cpu` to say what it is putting in one. It is `peniko`'s type, which
/// `vello_cpu` re-exports from `vello_common`; parley uses the same type from
/// the same crate, which is what makes D16's table a *hand-off* rather than a
/// conversion.
pub use vello_cpu::peniko::FontData;
