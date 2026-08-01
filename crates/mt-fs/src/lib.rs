//! # `mt-fs` — files, encodings, watching, atomic saves
//!
//! ## Contract
//!
//! All filesystem contact lives here, so that every crate below it can stay
//! headless and I/O-free. Built on `notify` (watching), `encoding_rs` +
//! `chardetng` (encoding detection and transcoding), and `tempfile` (atomic
//! replace); replaces `chokidar`, `ced`, and `write-file-atomic` (§8).
//!
//! ## The gate this crate must pass (§11.3)
//!
//! **Data safety: kill the process mid-save, repeatedly. No corruption, no
//! truncation, ever.** Not "rarely" — ever. A save is: write to a temp file in
//! the same directory, fsync, atomically rename over the target. A crash at
//! any point leaves either the old file or the new file intact, never a
//! partial one.
//!
//! This matters more than usual here: §12's `panic = "abort"` in the release
//! profile means a panic *is* a crash, mid-save included.
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU. No dependency on `mt-ui` or `mt-app`.** File
//!   dialogs are `rfd` in `mt-app`; this crate takes paths.
//! - This is the only crate permitted to perform general filesystem I/O.
//!   `mt-doc`, `mt-inline`, `mt-md`, and `mt-layout` must never grow a
//!   `std::fs` call — that would break the headless-CI property in §1 that
//!   everything in §11 depends on.
//!
//! ## M0 status
//!
//! Stub. `mt-fs` is M5 (§9).
