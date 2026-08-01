//! # `mt-math` — TeX layout
//!
//! ## Contract
//!
//! Lay out `$…$` inline math and `$$…$$` / ```` ```math ```` block math,
//! producing geometry `mt-layout` can place — an inline math span becomes a
//! parley `InlineBox` with a baseline, not a bitmap.
//!
//! Built on `rex` (pure Rust TeX), replacing `katex` (§8).
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU. No I/O.** (Font resources arrive from the
//!   caller, as in `mt-layout`.)
//! - **No dependency on `mt-ui` or `mt-app`.**
//! - Produces geometry, not pixels. Rasterization is `mt-render`'s.
//!
//! ## Risk R3 — this crate has a live fallback
//!
//! §13 R3: native math may fall short of KaTeX. The mitigation is to evaluate
//! `rex` **against KaTeX's own test set during M1, in parallel with the
//! lexer** — not at M6 when this crate is scheduled. If `rex` does not clear
//! the bar, the fallback is MicroTeX via FFI, accepting a C++ dependency. The
//! point of evaluating early is that the fallback is a build-system decision
//! with long lead time.
//!
//! ## M0 status
//!
//! Stub. `mt-math` ships in M6 (§9); its *evaluation* is due in M1.
