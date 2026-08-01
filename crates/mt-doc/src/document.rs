//! The document: an arena of nodes, plus the dirty set that drives
//! incremental layout (RUST-REWRITE-PLAN.md §2).

// M0: fields below are written by no code yet because every method body is
// `todo!()`. Remove this allow when `Document` is implemented in M2 — a real
// unread field is a bug worth hearing about.
#![allow(dead_code)]

use crate::block::Block;

/// An open document.
#[derive(Debug)]
pub struct Document {
    arena: Arena<Node>,
    root: NodeId,
    revision: u64,
    /// Drives incremental layout: `mt-layout` re-lays out only these blocks
    /// and shifts everything after them (§5).
    dirty: DirtySet,
}

impl Document {
    /// An empty document: a root containing a single empty paragraph.
    pub fn new() -> Self {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// The root node.
    pub fn root(&self) -> NodeId {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Monotonic revision counter, bumped by every [`crate::Document::apply`]
    /// batch. Consumers cache against it.
    pub fn revision(&self) -> u64 {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Borrow a node.
    pub fn node(&self, _id: NodeId) -> &Node {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// The set of nodes changed since the last layout pass.
    pub fn dirty(&self) -> &DirtySet {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Clear the dirty set. Called by `mt-layout` after a relayout pass.
    pub fn clear_dirty(&mut self) {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// A node in the tree: a [`Block`] plus its parent link.
///
/// Children live inside the container variants of [`Block`], not here, so the
/// tree shape and the block payload cannot drift apart.
#[derive(Debug)]
pub struct Node {
    parent: Option<NodeId>,
    block: Block,
}

/// A handle into a [`Document`]'s arena.
///
/// Stable across edits to *other* nodes — that stability is what lets
/// [`crate::Edit`] batches and `mt-layout`'s cached [`DirtySet`] refer to
/// nodes without holding borrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

/// Slot storage for nodes.
///
/// **M0 note.** Deliberately hand-rolled and minimal. Whether this becomes
/// `slotmap`, `id-arena`, or stays hand-written is an M2 decision that depends
/// on whether generational indices are needed to catch use-after-free of a
/// `NodeId` across an undo boundary. The plan (§2) writes `Arena<Node>`
/// without naming a crate; that ambiguity is intentional and resolved later.
#[derive(Debug)]
pub struct Arena<T> {
    slots: Vec<Option<T>>,
    free: Vec<u32>,
}

impl<T> Arena<T> {
    /// A new, empty arena.
    pub fn new() -> Self {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Insert a value, returning its handle.
    pub fn insert(&mut self, _value: T) -> NodeId {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Remove a value, freeing its slot.
    pub fn remove(&mut self, _id: NodeId) -> Option<T> {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The set of nodes whose layout is stale.
///
/// The performance contract in §5 depends on this staying small: a keystroke
/// dirties one block, `mt-layout` re-lays out that block, and every block
/// after it has its `y` shifted by the height delta. Nothing else is touched.
/// **Never lay out the whole document synchronously.**
#[derive(Debug, Default)]
pub struct DirtySet {
    nodes: Vec<NodeId>,
}

impl DirtySet {
    /// Mark a node's layout stale.
    pub fn mark(&mut self, _id: NodeId) {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Whether anything is stale.
    pub fn is_empty(&self) -> bool {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Iterate the stale nodes in document order.
    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9");
        #[allow(unreachable_code)]
        std::iter::empty()
    }
}
