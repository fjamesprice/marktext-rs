//! # `mt-doc` — document model, buffers, edits, undo
//!
//! ## Contract
//!
//! `mt-doc` owns the in-memory representation of an open document: the block
//! tree, the per-block text buffers, the invertible edit operations that
//! mutate it, and the undo stack built from their inverses. It answers
//! "what is the document?" and nothing else.
//!
//! It does **not** parse markdown (that is `mt-md`), tokenize inline syntax
//! (`mt-inline`), decide where anything is drawn (`mt-layout`), or read or
//! write files (`mt-fs`).
//!
//! ## Dependency constraints
//!
//! Per RUST-REWRITE-PLAN.md §1, dependency direction is strictly downward and
//! `mt-doc` sits at the bottom:
//!
//! - **No windowing.** No `winit`, no window handles, no event loop.
//! - **No GPU.** No `wgpu`, no `vello`, no surfaces.
//! - **No I/O.** No `std::fs`, no `std::net`, no `notify`. A `Document` is
//!   constructed from a `&str` a caller already has.
//! - **No dependency on `mt-ui` or `mt-app`,** ever. This is what lets §11
//!   run headlessly in CI on all three platforms.
//!
//! Together with `mt-inline`, `mt-md`, and `mt-layout`, this crate must
//! compile and pass its tests with no display attached.
//!
//! ## Why the shape is what it is
//!
//! §0 of the plan: muya's leaf blocks store **raw markdown source**, not a
//! parsed inline AST. `IParagraphState` is `{ name: 'paragraph', text: string }`
//! — and so is a heading, a table cell, a code block. Inline structure is
//! re-derived by tokenizing `text` on every render pass. That makes the
//! document fundamentally *text*, so edits are text ops rather than tree
//! surgery, and it makes serialization of a leaf block a matter of emitting
//! its `text` verbatim.
//!
//! The [`Block`] enum is therefore a deliberate 1:1 transcription of the
//! TypeScript union in `packages/muya/src/state/types.ts`. That correspondence
//! is not cosmetic: it is what makes the differential harness in §11.2
//! possible, because both engines can emit the same state JSON and be
//! compared byte-for-byte. **Do not "improve" the shape.**
//!
//! ## Status — implemented at M2 S0
//!
//! M0 left type skeletons with `todo!()` bodies. M2 S0 filled them in, and the
//! four decisions that shaped the result are recorded in `docs/M2.md`:
//!
//! - **§5 D7** — [`DirtySet`] carries `(NodeId, `[`Dirt`]`)`, because §4.1's
//!   incremental reparse is a requirement on the type rather than an
//!   optimisation. Settled *before* `edit.rs` was written.
//! - **§5 D8** — [`NodeId`] is generational, the [`Arena`] is hand-rolled, and
//!   [`Text::Rope`] is `ropey`. Promotion at 8 KiB; no demotion.
//! - **§5 D9** — the CRDT deferral is confirmed, which is a constraint on
//!   M3–M5: **no edit path may bypass [`Document::apply`]**. That is why a
//!   leaf's text and a container's children have no public mutable accessor.
//! - **S0's own two** — [`Node`]'s body is not a bare [`Block`] (the document
//!   root has no `TState` counterpart), and [`Block::meta`] returns an
//!   `Option` (seven of nineteen variants have no `meta` object). Both are
//!   written up in `docs/M2.md`'s S0 section.
//!
//! `mt-md` is still M2 S1 onward: this crate answers "what is the document?"
//! and nothing about how markdown becomes one.

mod block;
mod document;
mod edit;
mod text;

pub use block::{
    Align, Block, BlockMeta, BulletMarker, CodeKind, DiagramKind, DiagramLang, FrontmatterLang,
    FrontmatterStyle, MathStyle, MetaMismatch, OrderDelim, Underline,
};
pub use document::{Arena, Dirt, DirtySet, Document, Node, NodeId};
pub use edit::{Edit, EditError};
pub use text::{ROPE_THRESHOLD, Rope, Text};
