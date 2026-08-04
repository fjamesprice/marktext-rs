//! The document: an arena of nodes, plus the dirty set that drives
//! incremental layout (RUST-REWRITE-PLAN.md §2).

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
        let mut arena = Arena::new();
        let root = arena.insert(Node {
            parent: None,
            body: NodeBody::Root {
                children: Vec::new(),
            },
        });
        let paragraph = arena.insert(Node {
            parent: Some(root),
            body: NodeBody::Block(Block::Paragraph {
                text: crate::Text::new(),
            }),
        });
        arena
            .get_mut(root)
            .expect("just inserted")
            .children_mut()
            .push(paragraph);

        Document {
            arena,
            root,
            revision: 0,
            dirty: DirtySet::default(),
        }
    }

    /// The root node.
    ///
    /// The root is a synthetic container with **no `TState` counterpart** —
    /// see [`NodeBody`]. Its children are the document's top-level blocks, and
    /// they are what `mt_md::dump_state` serializes.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Monotonic revision counter, bumped by every [`crate::Document::apply`]
    /// batch. Consumers cache against it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Borrow a node.
    ///
    /// # Panics
    ///
    /// If `id` does not address a live node. A `NodeId` is generational
    /// ([`Arena`]), so a handle held across the edit that removed its node is
    /// *detected* rather than silently resolving to whatever now occupies the
    /// slot. Use [`Document::get`] where absence is expected.
    pub fn node(&self, id: NodeId) -> &Node {
        self.get(id)
            .unwrap_or_else(|| panic!("{id:?} does not address a live node"))
    }

    /// Borrow a node, or `None` if the handle is stale.
    ///
    /// This is the return that M2.md §5 D8 buys with the generation counter:
    /// after an incremental reparse recycles a slot, a `NodeId` cached by
    /// `mt-layout` reports `None` instead of addressing a different block.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.arena.get(id)
    }

    /// The block at `id`, or `None` at the root (which has no block) or for a
    /// stale handle.
    pub fn block(&self, id: NodeId) -> Option<&Block> {
        self.get(id).and_then(Node::block)
    }

    /// The children of `id`, in document order. Empty for a leaf.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.get(id).map_or(&[], Node::children)
    }

    /// The set of nodes changed since the last layout pass.
    pub fn dirty(&self) -> &DirtySet {
        &self.dirty
    }

    /// Clear the dirty set. Called by `mt-layout` after a relayout pass.
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// How many nodes the arena currently holds, live or detached.
    ///
    /// Exists for [`Document::prune_detached`]'s tests and for the invariant
    /// property in `tests/edit_inverse.rs`: an apply-then-undo pair must not
    /// grow the arena without bound.
    pub fn arena_len(&self) -> usize {
        self.arena.len()
    }

    /// Free every node not reachable from the root.
    ///
    /// [`crate::Edit::RemoveNode`] and [`crate::Edit::ReplaceBlock`] **detach**
    /// a subtree rather than freeing it, so that their inverse can put back
    /// the same nodes with the same ids. `Document::apply`'s "Removal
    /// detaches" section is the argument; the short form is that a batch may
    /// edit a node and then remove it, and undoing that batch has to undo the
    /// edit *after* restoring the node.
    ///
    /// So the detached nodes are reachable only from an inverse batch, i.e.
    /// from the undo stack. **Call this only when no undo entry can still name
    /// them** — the intended caller is the history trimming M4 lands
    /// (`history/index.ts`'s coalescing policy), which is the only place that
    /// knows an inverse has been dropped. Returns the number of nodes freed.
    pub fn prune_detached(&mut self) -> usize {
        let mut reachable = vec![false; self.arena.slots.len()];
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            let index = id.index as usize;
            if reachable[index] {
                continue;
            }
            reachable[index] = true;
            stack.extend_from_slice(self.children(id));
        }

        // Collect first, then free: `Arena::remove` needs `&mut self.arena`
        // and the scan borrows it.
        let doomed: Vec<NodeId> = reachable
            .iter()
            .enumerate()
            .filter(|(index, live)| !**live && self.arena.slots[*index].value.is_some())
            .map(|(index, _)| NodeId {
                index: index as u32,
                generation: self.arena.slots[index].generation,
            })
            .collect();

        let freed = doomed.len();
        for id in doomed {
            self.arena.remove(id);
        }
        freed
    }

    // --- internals used by `edit.rs` ---------------------------------------

    pub(crate) fn arena_mut(&mut self) -> &mut Arena<Node> {
        &mut self.arena
    }

    pub(crate) fn dirty_mut(&mut self) -> &mut DirtySet {
        &mut self.dirty
    }

    pub(crate) fn bump_revision(&mut self) {
        self.revision += 1;
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
    body: NodeBody,
}

impl Node {
    /// The node's parent, or `None` at the root and for a detached subtree's
    /// top node.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// The node's block, or `None` at the root.
    pub fn block(&self) -> Option<&Block> {
        match &self.body {
            NodeBody::Root { .. } => None,
            NodeBody::Block(block) => Some(block),
        }
    }

    /// The node's children, in document order. Empty for a leaf block.
    pub fn children(&self) -> &[NodeId] {
        match &self.body {
            NodeBody::Root { children } => children,
            NodeBody::Block(block) => block.children().unwrap_or(&[]),
        }
    }

    /// Whether this is the document root.
    pub fn is_root(&self) -> bool {
        matches!(self.body, NodeBody::Root { .. })
    }

    // --- internals used by `edit.rs` ---------------------------------------

    pub(crate) fn new_block(parent: Option<NodeId>, block: Block) -> Self {
        Node {
            parent,
            body: NodeBody::Block(block),
        }
    }

    pub(crate) fn set_parent(&mut self, parent: Option<NodeId>) {
        self.parent = parent;
    }

    pub(crate) fn block_mut_internal(&mut self) -> Option<&mut Block> {
        match &mut self.body {
            NodeBody::Root { .. } => None,
            NodeBody::Block(block) => Some(block),
        }
    }

    pub(crate) fn children_mut_internal(&mut self) -> &mut Vec<NodeId> {
        self.children_mut()
    }

    /// Replace the node's block, returning the old one. The root has no block
    /// and `edit.rs` refuses to reach here with it.
    pub(crate) fn replace_block(&mut self, block: Block) -> Block {
        match std::mem::replace(&mut self.body, NodeBody::Block(block)) {
            NodeBody::Block(old) => old,
            NodeBody::Root { children } => {
                // Restore rather than corrupt, then say so loudly. Unreachable
                // via `Edit`, which returns `EditError::Root` first.
                self.body = NodeBody::Root { children };
                panic!("the document root has no block to replace")
            }
        }
    }

    fn children_mut(&mut self) -> &mut Vec<NodeId> {
        match &mut self.body {
            NodeBody::Root { children } => children,
            NodeBody::Block(block) => block
                .children_mut()
                .expect("a leaf block has no children to mutate"),
        }
    }
}

/// What a [`Node`] holds.
///
/// **S0 decision, recorded because §2's sketch does not cover it.** §2 writes
/// `Node { parent, block: Block }`, and the document root has no `Block` it
/// could hold: [`Block`] maps 1:1 onto the TypeScript `TState` union, `TState`
/// is what an *array* of top-level blocks is made of, and there is no root
/// member. Adding a twentieth variant for it would break
/// `block_names_match_typescript_union_one_to_one`, which is the precondition
/// for the §11.2 differential harness.
///
/// The reference engine has the same split: muya's tree is rooted at
/// `ScrollPage` (`block/scrollPage.ts`), whose `blockName` is `scrollpage` and
/// which likewise has no `TState` member. So this models what muya does rather
/// than inventing a shape.
///
/// Children still live in exactly one place per node kind — in the `Root`
/// variant here, in the container variant of [`Block`] everywhere else — so
/// M0's reason for not putting `children` on [`Node`] is preserved.
#[derive(Debug)]
enum NodeBody {
    /// The document root. Its children are the top-level blocks.
    Root { children: Vec<NodeId> },
    /// Every other node.
    Block(Block),
}

/// A handle into a [`Document`]'s arena.
///
/// Stable across edits to *other* nodes — that stability is what lets
/// [`crate::Edit`] batches and `mt-layout`'s cached [`DirtySet`] refer to
/// nodes without holding borrows.
///
/// **Generational, per M2.md §5 D8.** The `generation` half is what makes a
/// handle held across the edit that removed its node resolve to `None` rather
/// than to whichever node now occupies the recycled slot. M0 left this open
/// ("whether generational indices are needed to catch a `NodeId` used across
/// an undo boundary"); §4.1's incremental reparse is what settled it, because
/// a reparse recycles slots on every structural edit and `mt-layout`'s layout
/// cache holds ids across exactly that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    /// Slot index. First, so that `Ord` sorts by slot rather than by age —
    /// which is what makes [`DirtySet`]'s ordering stable across runs.
    index: u32,
    generation: u32,
}

/// Slot storage for nodes.
///
/// **Hand-rolled, per M2.md §5 D8.** Insert, remove, get and a free list with
/// a generation counter is a small amount of code with no behaviour of its
/// own, and there is one entry per *block* — a 5 MB document has thousands,
/// not millions. `slotmap` would be a dependency for that; `ropey`, which
/// [`crate::Text`] genuinely needs, is not. Revisit if the arena ever shows up
/// in a profile.
#[derive(Debug)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

#[derive(Debug)]
struct Slot<T> {
    /// Bumped on every *free*, so a handle minted before the free no longer
    /// matches. Starts at 0 and is odd/even-agnostic — the only property that
    /// matters is that it changes.
    generation: u32,
    value: Option<T>,
}

impl<T> Arena<T> {
    /// A new, empty arena.
    pub fn new() -> Self {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Insert a value, returning its handle.
    pub fn insert(&mut self, value: T) -> NodeId {
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                debug_assert!(slot.value.is_none(), "a free slot held a value");
                slot.value = Some(value);
                NodeId {
                    index,
                    generation: slot.generation,
                }
            }
            None => {
                let index =
                    u32::try_from(self.slots.len()).expect("more than 2^32 blocks in one document");
                self.slots.push(Slot {
                    generation: 0,
                    value: Some(value),
                });
                NodeId {
                    index,
                    generation: 0,
                }
            }
        }
    }

    /// Remove a value, freeing its slot.
    ///
    /// Returns `None` for a stale handle, and bumps the slot's generation so
    /// that every handle to it — including the one just used — is stale from
    /// here on.
    pub fn remove(&mut self, id: NodeId) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        // Wrapping rather than saturating: a saturated generation would make
        // every future handle to this slot compare equal to every past one,
        // which is the exact failure the counter exists to prevent. Wrapping
        // needs 2^32 frees of one slot to collide.
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        Some(value)
    }

    /// Borrow a value, or `None` if the handle is stale.
    pub fn get(&self, id: NodeId) -> Option<&T> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation).then_some(slot.value.as_ref())?
    }

    /// Borrow a value mutably, or `None` if the handle is stale.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// The number of live values.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// Whether the arena holds no live values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Why a node's layout is stale.
///
/// **M2.md §5 D7.** §4.1 reads as an optimisation and is not: step 1
/// re-tokenizes only the edited block, step 2 replaces a subtree. Those are
/// different amounts of invalidation, and a `DirtySet` that cannot tell them
/// apart forces `mt-layout` to assume the worse one — which throws away every
/// cached layout below the edit on every `#` typed at the start of a line.
///
/// Settled here, before `edit.rs`, because adding the distinction afterwards
/// means revisiting every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dirt {
    /// This leaf's text changed. Re-tokenize and re-lay-out **one** block;
    /// everything below it in the tree is untouched because there is nothing
    /// below a leaf.
    Text,
    /// This subtree was replaced, moved, or gained or lost a child. Everything
    /// below is new.
    Structure,
}

/// The set of nodes whose layout is stale.
///
/// The performance contract in §5 depends on this staying small: a keystroke
/// dirties one block, `mt-layout` re-lays out that block, and every block
/// after it has its `y` shifted by the height delta. Nothing else is touched.
/// **Never lay out the whole document synchronously.**
#[derive(Debug, Default)]
pub struct DirtySet {
    /// Sorted by [`NodeId`], with one entry per node. Sorted rather than
    /// insertion-ordered so that two edit batches that touch the same nodes in
    /// a different order produce the same set — otherwise `mt-layout`'s work
    /// order would depend on typing order.
    nodes: Vec<(NodeId, Dirt)>,
}

impl DirtySet {
    /// Mark a node's layout stale.
    ///
    /// [`Dirt::Structure`] wins over [`Dirt::Text`]: marking a node text-dirty
    /// after marking it structure-dirty must not narrow the invalidation, or a
    /// batch that both splices a leaf and replaces its parent would under-report.
    pub fn mark(&mut self, id: NodeId, dirt: Dirt) {
        match self.nodes.binary_search_by_key(&id, |(id, _)| *id) {
            Ok(at) => self.nodes[at].1 = self.nodes[at].1.max(dirt),
            Err(at) => self.nodes.insert(at, (id, dirt)),
        }
    }

    /// Whether anything is stale.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// How many nodes are stale.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Why `id` is stale, or `None` if it is not.
    pub fn dirt(&self, id: NodeId) -> Option<Dirt> {
        self.nodes
            .binary_search_by_key(&id, |(id, _)| *id)
            .ok()
            .map(|at| self.nodes[at].1)
    }

    /// Iterate the stale nodes and why, in [`NodeId`] order.
    ///
    /// **Not document order.** M0's doc comment said document order; a set of
    /// ids has no tree in it and cannot deliver that. `mt-layout` establishes
    /// document order by walking the tree — which it must do anyway to shift
    /// the `y` of everything after a re-laid-out block — and consults
    /// [`DirtySet::dirt`] as it goes. Corrected here rather than left as a
    /// promise nothing keeps.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, Dirt)> + '_ {
        self.nodes.iter().copied()
    }

    fn clear(&mut self) {
        self.nodes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Text;

    // --- the arena ---------------------------------------------------------

    #[test]
    fn insert_then_get_returns_the_value() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(1);
        let b = arena.insert(2);
        assert_eq!(arena.get(a), Some(&1));
        assert_eq!(arena.get(b), Some(&2));
        assert_eq!(arena.len(), 2);
        assert!(!arena.is_empty());
    }

    #[test]
    fn remove_frees_the_slot_and_returns_the_value() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(1);
        assert_eq!(arena.remove(a), Some(1));
        assert_eq!(arena.len(), 0);
        assert!(arena.is_empty());
        assert_eq!(arena.remove(a), None, "a second remove is a no-op");
    }

    #[test]
    fn a_freed_slot_is_reused() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(1);
        arena.remove(a);
        let b = arena.insert(2);
        assert_eq!(arena.len(), 1, "the slot was reused, not appended to");
        assert_ne!(a, b);
    }

    /// **D8's whole reason.** A handle held across the removal of its node
    /// must resolve to `None`, not to whatever now lives in the slot. Without
    /// the generation counter this test reads `Some(&2)`.
    #[test]
    fn a_stale_handle_does_not_address_the_node_that_reused_its_slot() {
        let mut arena: Arena<u32> = Arena::new();
        let stale = arena.insert(1);
        arena.remove(stale);
        let fresh = arena.insert(2);

        assert_eq!(arena.get(stale), None, "stale handle must not resolve");
        assert_eq!(arena.get_mut(stale), None);
        assert_eq!(arena.remove(stale), None);
        assert_eq!(arena.get(fresh), Some(&2), "the fresh handle still works");
    }

    #[test]
    fn get_mut_edits_in_place() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(1);
        *arena.get_mut(a).expect("live") = 7;
        assert_eq!(arena.get(a), Some(&7));
    }

    // --- the document ------------------------------------------------------

    /// §2's `Document::new` contract, and the shape every other test rests on.
    #[test]
    fn a_new_document_is_a_root_holding_one_empty_paragraph() {
        let doc = Document::new();
        let root = doc.root();

        assert!(doc.node(root).is_root());
        assert_eq!(doc.node(root).block(), None, "the root has no TState");
        assert_eq!(doc.node(root).parent(), None);

        let children = doc.children(root);
        assert_eq!(children.len(), 1);
        let paragraph = children[0];
        assert_eq!(doc.block(paragraph).map(Block::name), Some("paragraph"));
        assert_eq!(
            doc.block(paragraph).and_then(Block::text).map(Text::to_str),
            Some(std::borrow::Cow::Borrowed(""))
        );
        assert_eq!(doc.node(paragraph).parent(), Some(root));

        assert_eq!(doc.revision(), 0);
        assert!(doc.dirty().is_empty());
    }

    #[test]
    fn a_stale_node_id_is_none_rather_than_a_panic_through_get() {
        let mut doc = Document::new();
        let paragraph = doc.children(doc.root())[0];
        doc.arena_mut().remove(paragraph);
        assert!(doc.get(paragraph).is_none());
        assert!(doc.block(paragraph).is_none());
        assert!(doc.children(paragraph).is_empty());
    }

    #[test]
    #[should_panic(expected = "does not address a live node")]
    fn node_panics_on_a_stale_handle() {
        let mut doc = Document::new();
        let paragraph = doc.children(doc.root())[0];
        doc.arena_mut().remove(paragraph);
        let _ = doc.node(paragraph);
    }

    #[test]
    fn prune_detached_frees_everything_unreachable_from_the_root() {
        let mut doc = Document::new();
        let orphan = doc.arena_mut().insert(Node {
            parent: None,
            body: NodeBody::Block(Block::Paragraph { text: Text::new() }),
        });
        assert_eq!(doc.arena_len(), 3);
        assert_eq!(doc.prune_detached(), 1);
        assert_eq!(doc.arena_len(), 2);
        assert!(doc.get(orphan).is_none());
        // Idempotent, and it never touches what is reachable.
        assert_eq!(doc.prune_detached(), 0);
        assert_eq!(doc.children(doc.root()).len(), 1);
    }

    // --- the dirty set -----------------------------------------------------

    #[test]
    fn marking_records_the_reason() {
        let mut doc = Document::new();
        let paragraph = doc.children(doc.root())[0];
        let root = doc.root();

        doc.dirty_mut().mark(paragraph, Dirt::Text);
        doc.dirty_mut().mark(root, Dirt::Structure);

        assert!(!doc.dirty().is_empty());
        assert_eq!(doc.dirty().len(), 2);
        assert_eq!(doc.dirty().dirt(paragraph), Some(Dirt::Text));
        assert_eq!(doc.dirty().dirt(root), Some(Dirt::Structure));
    }

    /// D7: a batch that splices a leaf and then replaces its parent must not
    /// end up reporting the leaf as merely text-dirty.
    #[test]
    fn structure_wins_over_text_whichever_order_they_are_marked_in() {
        let mut set = DirtySet::default();
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(0);
        let b = arena.insert(0);

        set.mark(a, Dirt::Text);
        set.mark(a, Dirt::Structure);
        assert_eq!(set.dirt(a), Some(Dirt::Structure));

        set.mark(b, Dirt::Structure);
        set.mark(b, Dirt::Text);
        assert_eq!(set.dirt(b), Some(Dirt::Structure));
    }

    #[test]
    fn marking_the_same_node_twice_keeps_one_entry() {
        let mut set = DirtySet::default();
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(0);
        set.mark(a, Dirt::Text);
        set.mark(a, Dirt::Text);
        assert_eq!(set.len(), 1);
    }

    /// The set is sorted, so two batches that touch the same nodes in a
    /// different order hand `mt-layout` the same work.
    #[test]
    fn iteration_is_in_node_id_order_whatever_the_marking_order_was() {
        let mut arena: Arena<u32> = Arena::new();
        let ids: Vec<NodeId> = (0..4).map(|i| arena.insert(i)).collect();

        let mut forward = DirtySet::default();
        for id in &ids {
            forward.mark(*id, Dirt::Text);
        }
        let mut backward = DirtySet::default();
        for id in ids.iter().rev() {
            backward.mark(*id, Dirt::Text);
        }

        assert_eq!(
            forward.iter().collect::<Vec<_>>(),
            backward.iter().collect::<Vec<_>>()
        );
        assert_eq!(
            forward.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            ids,
            "and that order is NodeId order"
        );
    }

    #[test]
    fn clearing_empties_the_set() {
        let mut doc = Document::new();
        let paragraph = doc.children(doc.root())[0];
        doc.dirty_mut().mark(paragraph, Dirt::Text);
        doc.clear_dirty();
        assert!(doc.dirty().is_empty());
        assert_eq!(doc.dirty().dirt(paragraph), None);
        assert_eq!(doc.dirty().iter().count(), 0);
    }
}
