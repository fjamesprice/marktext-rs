//! # `mt-app` — the application shell
//!
//! ## Contract
//!
//! The `winit` shell: window and event loop, menus (`muda`), dialogs (`rfd`),
//! credential storage (`keyring`), preferences and session (`serde`).
//! Replaces Electron's main process — 9,627 lines of it (§8).
//!
//! There is **no process boundary and no IPC**. The renderer/main split and
//! the entire `shared/types/ipc.ts` contract disappear; deleting IPC is a
//! large chunk of the latency win.
//!
//! ## Three things built in from the start, never retrofitted (§7)
//!
//! **IME.** `winit`'s `Ime::{Enabled, Preedit, Commit, Disabled}`. The preedit
//! string renders as a transient styled run at the caret **without entering
//! the document or the undo stack**; only `Commit` produces an
//! `mt_doc::Edit`. The candidate window is positioned via
//! `set_ime_cursor_area` using the caret rect from `mt-layout`. `zh-CN`,
//! `zh-TW`, `ja`, and `ko` are shipped locales, so this is a correctness
//! requirement verified by native speakers, not a nice-to-have (§10, R5).
//!
//! **Accessibility.** The AccessKit tree is assembled here from `mt-ui`'s
//! contributions and bridged to the platform. Snapshotted in CI (§11.3).
//!
//! **Keybindings.** The three platform tables in
//! `packages/desktop/src/main/keyboard/keybindings{Darwin,Linux,Windows}.ts`
//! port as **data, not code** — a TOML table keyed by command id, with user
//! overrides layered on top.
//!
//! ## Migration obligation
//!
//! The M5 exit gate requires preferences, keybindings, and session to import
//! from an existing install: all 72 keys of `preference.json` read verbatim,
//! **keys with no native equivalent preserved on write, never dropped**.
//!
//! ## Dependency constraints
//!
//! `mt-app` is the top of the graph and may depend on anything below it.
//! Nothing may depend on `mt-app`. In particular `mt-doc`, `mt-inline`,
//! `mt-md`, and `mt-layout` must never depend on this crate or on `mt-ui`:
//! those four compile and test with no windowing, no GPU, and no I/O, which is
//! what makes §11 runnable in CI on all three platforms.
//!
//! ## M0 status
//!
//! Stub. `mt-app` is M5 (§9).
