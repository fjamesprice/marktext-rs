//! # `mt-export` — HTML and PDF
//!
//! ## Contract
//!
//! Emit HTML (from `mt-md`) and PDF (from `mt-layout` via `pdf-writer`),
//! replacing Chromium's `printToPDF` (§8).
//!
//! **Routing PDF export through `mt-layout` rather than a separate renderer
//! means export matches the screen by construction** — a fidelity improvement
//! over the Chromium path, at the cost of implementing the PDF emitter. The
//! M6 exit gate is "PDF matches screen on the corpus" (§9); that gate is only
//! achievable if this crate never computes geometry of its own.
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU.** A PDF must be producible headlessly, from the
//!   CLI, on a machine with no display — that is what makes it testable in CI.
//! - **No dependency on `mt-ui` or `mt-app`.** Export is a document operation,
//!   not an application one; the menu item lives in `mt-app` and calls in.
//! - Writing the output file is the caller's job (`mt-fs`); this crate
//!   produces bytes.
//!
//! ## M0 status
//!
//! Stub. `mt-export` is M6 (§9).
