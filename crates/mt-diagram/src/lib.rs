//! # `mt-diagram` — native diagram subset
//!
//! ## Contract
//!
//! Lay out the diagram kinds MarkText supports, built on `layout-rs` plus a
//! native flowchart and sequence-diagram implementation; replaces `mermaid`
//! and `flowchart.js` (§8). PlantUML ships only as an encoder in v1
//! (`flate2` + base64 — a PlantUML link is just an encoded URL), so links
//! resolve even though nothing is rendered locally.
//!
//! ## The rule that outranks scope (§10, R4)
//!
//! **Never render a diagram block blank.** Mermaid beyond the native subset is
//! deferred to v1.1 and Vega/Vega-Lite is dropped from v1 — but in both cases
//! the block is retained as a passthrough code fence that round-trips
//! losslessly and renders **as source**, with a clear message. A block that
//! silently disappears is data loss, and this is an editor people trust with
//! their notes.
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU.** Produces geometry for `mt-layout` to place and
//!   `mt-render` to rasterize.
//! - **No dependency on `mt-ui` or `mt-app`.**
//! - The optional `mmdc` passthrough is the one place a subprocess is
//!   contemplated, and it is opt-in, absent by default, and must degrade to
//!   "render as source" when Node is not installed — never to an error dialog
//!   and never to a blank block.
//!
//! ## M0 status
//!
//! Stub. `mt-diagram` is M6 (§9); the scope question (native subset vs
//! external CLI vs drop) is an open decision.
