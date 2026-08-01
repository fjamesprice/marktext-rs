//! Invertible edit operations (RUST-REWRITE-PLAN.md §2.2).

use crate::block::{Block, BlockMeta};
use crate::document::{Document, NodeId};

/// A single, invertible mutation of a [`Document`].
///
/// Every variant is invertible, so undo is a stack of inverse batches — there
/// is no snapshotting. Batches are grouped into user-visible undo steps by a
/// coalescing policy (time gap + edit-kind change) matching muya's
/// `history/index.ts`.
///
/// # On collaborative editing
///
/// muya's `ot-json1` + `ot-text-unicode` layer is **not** carried forward
/// (§2.2). This enum is nevertheless shaped as discrete invertible ops
/// specifically so a CRDT (`loro`) can be slid underneath later without
/// redesigning the model. That is the one deferred-architecture bet in the
/// plan; §14 flags it as an open decision to confirm at M2 start. Changing
/// this enum's shape is the thing that would foreclose the option.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Replace `remove` bytes at byte offset `at` in a leaf's text with
    /// `insert`. The inverse is another `SpliceText` with the removed text.
    SpliceText {
        node: NodeId,
        at: usize,
        remove: usize,
        insert: String,
    },

    /// Replace a block's metadata without touching its text or children —
    /// so the node keeps its identity. The inverse carries the old meta.
    SetMeta { node: NodeId, meta: BlockMeta },

    /// Insert a new block as `parent`'s child at `index`. Inverse is
    /// [`Edit::RemoveNode`].
    InsertNode {
        parent: NodeId,
        index: usize,
        block: Block,
    },

    /// Remove a node and its subtree. Inverse is [`Edit::InsertNode`] carrying
    /// the removed block back.
    RemoveNode { node: NodeId },

    /// Reparent a node. Inverse is another `MoveNode` back to the old
    /// `(parent, index)`.
    MoveNode {
        node: NodeId,
        new_parent: NodeId,
        index: usize,
    },

    /// Replace a node's block wholesale — the paragraph-to-heading kind of
    /// transformation. Inverse carries the old block.
    ReplaceBlock { node: NodeId, block: Block },
}

impl Document {
    /// Apply a batch of edits, returning the inverse batch.
    ///
    /// The returned `Vec<Edit>` is ordered so that applying it undoes this
    /// call exactly — i.e. it is the reverse of the forward order, with each
    /// edit replaced by its own inverse. Pushing it onto the undo stack is
    /// the whole of undo.
    ///
    /// Marks every touched node layout-dirty in the document's
    /// [`crate::DirtySet`], which is what drives incremental relayout in
    /// `mt-layout` (§5).
    pub fn apply(&mut self, _edits: &[Edit]) -> Vec<Edit> {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }
}
