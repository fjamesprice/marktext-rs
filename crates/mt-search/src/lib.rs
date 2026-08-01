//! # `mt-search` — in-document find/replace and project search
//!
//! ## Contract
//!
//! Two related jobs: find/replace within an open `Document`, and grep across a
//! folder. Built on `ignore` + `grep-searcher` — **linked, not spawned**,
//! replacing the `@vscode/ripgrep` subprocess wrapper (§8).
//!
//! That change is a strict improvement, and it was one of the four concrete
//! reasons Rust was chosen over C++ (NATIVE-REWRITE-PLAN.md §4.3): no process
//! spawn, no shipped ripgrep binary, no argv quoting, results streamed
//! directly as structs.
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU. No dependency on `mt-ui` or `mt-app`.** The
//!   find/replace bar is a widget in `mt-ui`; this crate returns matches.
//! - Project search reads the filesystem through `ignore`'s walker; that is
//!   this crate's own concern and does not license in-document search to touch
//!   the disk.
//! - In-document search operates on a `Document`, so replace produces
//!   `mt_doc::Edit` values and goes through the normal undo path — a
//!   replace-all must be one undo step, not thousands.
//!
//! ## M0 status
//!
//! Stub. `mt-search` is M5 (§9).
