//! # `mt-ui` — widgets, floats, panes, accessibility tree
//!
//! ## Contract
//!
//! A small retained-mode widget layer: tabs, sidebar file tree, command
//! palette, find/replace bar, preference panes, and the ~20 floating surfaces
//! muya has — inline format toolbar, table tools, image tools, emoji picker,
//! quick-insert menu, language selector, footnote tool. Popup positioning
//! replaces `@floating-ui/dom` in roughly 300 lines (§8).
//!
//! ## Accessibility is a contract, not a feature (§7, R6)
//!
//! **Every widget contributes to the AccessKit tree.** `accesskit` bridges to
//! UIA on Windows, AT-SPI on Linux, and NSAccessibility on macOS. The document
//! maps to a tree of text nodes with character ranges, so screen readers get
//! real text navigation rather than an opaque canvas.
//!
//! The tree is built **in the same phase as the caret** (M4) and snapshotted
//! in CI from the first commit that has one. §10 lists screen-reader support
//! on all three platforms as non-negotiable regardless of schedule: it cannot
//! credibly be retrofitted, and R6 exists because retrofitting is exactly what
//! projects do.
//!
//! ## Dependency constraints
//!
//! `mt-ui` sits **above** the headless core. The direction that matters is the
//! one that must never appear: `mt-doc`, `mt-inline`, `mt-md`, and
//! `mt-layout` must never depend on this crate. Those four compile and test
//! without a window, and that property is what makes §11 runnable in CI.
//!
//! `mt-ui` depends downward on `mt-doc`, `mt-layout`, and `mt-render`. It does
//! not own the window or the event loop — that is `mt-app`.
//!
//! ## M0 status
//!
//! Stub. `mt-ui` is M5 (§9); its AccessKit obligations begin in M4.
