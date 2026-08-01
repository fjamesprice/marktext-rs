//! # `mt-render` — display list → pixels
//!
//! ## Contract
//!
//! Rasterize the display list `mt-layout` produced. Two backends behind one
//! trait (§6):
//!
//! | Backend | Crate | Role |
//! |---|---|---|
//! | GPU | `vello` on `wgpu` | Default |
//! | CPU | `tiny-skia` | Fallback for VMs, RDP, old drivers |
//!
//! The CPU fallback is **an M3 deliverable, not an afterthought** (§6, R7).
//! It is tested in a VM in CI.
//!
//! Glyph rasterization goes through `swash`, cached in an atlas keyed by
//! `(font, size, subpixel offset, glyph)`.
//!
//! ## Dependency constraints
//!
//! `mt-render` is the first crate in the graph permitted to touch the GPU, and
//! it consumes `mt-layout` downward. It must not depend on `mt-ui` or
//! `mt-app`: widgets and windows are above it.
//!
//! It decides *no geometry*. Everything positioned arrives from `mt-layout`;
//! if `mt-render` computes a coordinate, PDF export and the screen have
//! already diverged.
//!
//! ## Redraw policy: on event only (§6)
//!
//! No animation loop. No timers. No idle polling. Caret blink is the sole
//! timer and it stops when the window loses focus. This is the "~0 % idle CPU"
//! target in §12.1, and it is trivially lost by accident — one stray
//! `request_redraw()` in a frame callback is all it takes.
//!
//! Dirty-rect rendering: redraw only changed blocks plus the caret rect.
//! Scrolling blits and renders the newly exposed band.
//!
//! ## An open decision, not a foregone conclusion (§12.2)
//!
//! `wgpu` + `vello` is ~4–6 MB of a ~17–25 MB binary. For a markdown editor
//! `tiny-skia` may be both smaller and fast enough. **Benchmark both in M3 and
//! let the data pick the default** — dropping the GPU stack plus loading
//! tree-sitter grammars on demand lands the binary at ~8–11 MB, which clears
//! the stretch target.
//!
//! ## M0 status
//!
//! Stub. `mt-render` is M3 (§9).
