//! Invertible edit operations (RUST-REWRITE-PLAN.md §2.2).

use crate::block::{Block, BlockMeta};
use crate::document::{Dirt, Document, NodeId};

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
/// plan; §14 flags it as an open decision to confirm at M2 start.
///
/// **Confirmed deferred at M2 start — M2.md §5 D9.** The confirmation is a
/// constraint on M3–M5 rather than on this enum: no edit path may bypass
/// [`Document::apply`]. Changing this enum's shape is the thing that would
/// foreclose the option, and S0 has not.
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

    /// Remove a node and its subtree.
    ///
    /// **The inverse is [`Edit::MoveNode`], not [`Edit::InsertNode`]** — see
    /// [`Document::apply`]'s "Removal detaches" section, which is the one
    /// place S0 read §2.2 and found the obvious reading did not work.
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

/// Why an edit could not be applied.
///
/// Every variant is a caller bug rather than a user-visible condition:
/// `mt-md`'s parser and `mt-ui`'s commands construct edits from ids they just
/// read out of the document. It is a `Result` rather than a panic because
/// [`Document::apply`] is reachable from an undo stack, and an undo that
/// panics loses the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The `NodeId` does not address a live node — a handle held across the
    /// edit that removed it (M2.md §5 D8).
    StaleNode(NodeId),
    /// `SpliceText` addressed a container, or `InsertNode`/`MoveNode`
    /// addressed a leaf as a parent.
    WrongKind(NodeId),
    /// An index past the end of the parent's children.
    IndexOutOfRange { parent: NodeId, index: usize },
    /// `SetMeta`'s payload does not match the block's variant.
    MetaMismatch(NodeId),
    /// `MoveNode` would have made a node its own ancestor.
    WouldCycle(NodeId),
    /// `RemoveNode` addressed a node that is already detached — it is live in
    /// the arena but not attached to the tree, because an earlier
    /// `RemoveNode` took it out and nothing has put it back.
    Detached(NodeId),
    /// The document root cannot be removed, moved, replaced or spliced.
    Root,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::StaleNode(id) => write!(f, "{id:?} does not address a live node"),
            EditError::WrongKind(id) => {
                write!(f, "{id:?} is the wrong kind of block for this edit")
            }
            EditError::IndexOutOfRange { parent, index } => {
                write!(f, "index {index} is past the end of {parent:?}'s children")
            }
            EditError::MetaMismatch(id) => write!(f, "the meta does not match {id:?}'s variant"),
            EditError::WouldCycle(id) => {
                write!(f, "moving {id:?} there would make it its own ancestor")
            }
            EditError::Detached(id) => write!(f, "{id:?} is already detached from the tree"),
            EditError::Root => f.write_str("the document root cannot be edited"),
        }
    }
}

impl std::error::Error for EditError {}

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
    /// `mt-layout` (§5). Per M2.md §5 D7, `SpliceText` and `SetMeta` mark
    /// [`Dirt::Text`] and the other four mark [`Dirt::Structure`].
    ///
    /// # Removal detaches; it does not free
    ///
    /// **S0 finding, and the one place §2.2's obvious reading does not work.**
    /// `RemoveNode`'s doc comment says its inverse is `InsertNode` "carrying
    /// the removed block back". Written that way it is wrong in two
    /// compounding ways, and the property test in `tests/edit_inverse.rs`
    /// found the second one:
    ///
    /// 1. `InsertNode` carries **one** [`Block`], and a container `Block`
    ///    holds `Vec<NodeId>` — so restoring a subtree needs those ids still
    ///    to be live. Freeing the descendants at removal time would make the
    ///    inverse restore an empty container.
    /// 2. `InsertNode` mints a **new** id. So in the batch
    ///    `[SpliceText { node: x, .. }, RemoveNode { node: x }]` the inverse
    ///    is `[InsertNode(..), SpliceText { node: x, .. }]` — and by the time
    ///    the splice is undone, `x` is stale and the undo fails. That is not a
    ///    contrived batch: any edit that rewrites a block and then merges it
    ///    away has this shape.
    ///
    /// So `RemoveNode` **detaches**: the node and its subtree stay in the
    /// arena with the node's `parent` cleared, and the inverse is
    /// [`Edit::MoveNode`] back to the old `(parent, index)`. The node keeps
    /// its id, which makes an apply-then-undo pair restore the `NodeId`s and
    /// not merely the content — a stronger guarantee than §2.2 asks for, and
    /// the one `mt-layout`'s cache will want at M3.
    ///
    /// The price is that a detached subtree is reachable only from the undo
    /// stack. [`Document::prune_detached`] reclaims it, and its doc comment
    /// says who may call it.
    ///
    /// # Failure is all-or-nothing at the edit, not at the batch
    ///
    /// An edit that cannot be applied stops the batch and returns the error;
    /// the edits already applied stay applied. That is deliberate and it is
    /// why the error carries no partial inverse: recovering is the caller's
    /// job and the caller is `mt-md` or `mt-ui`, both of which construct
    /// batches from ids they have just read. Use [`Document::try_apply`] to
    /// see the error; [`Document::apply`] panics on it, because a batch that
    /// fails is a bug in the code that built it.
    ///
    /// # Panics
    ///
    /// If any edit in the batch cannot be applied. See [`Document::try_apply`].
    pub fn apply(&mut self, edits: &[Edit]) -> Vec<Edit> {
        self.try_apply(edits)
            .unwrap_or_else(|e| panic!("edit batch failed: {e}"))
    }

    /// [`Document::apply`], returning the failure instead of panicking.
    pub fn try_apply(&mut self, edits: &[Edit]) -> Result<Vec<Edit>, EditError> {
        let mut inverse = Vec::with_capacity(edits.len());
        for edit in edits {
            inverse.push(self.apply_one(edit)?);
        }
        // Reverse order, each edit already replaced by its own inverse: the
        // last thing done is the first thing undone.
        inverse.reverse();
        self.bump_revision();
        Ok(inverse)
    }

    fn apply_one(&mut self, edit: &Edit) -> Result<Edit, EditError> {
        match edit {
            Edit::SpliceText {
                node,
                at,
                remove,
                insert,
            } => {
                let block = self
                    .arena_mut()
                    .get_mut(*node)
                    .ok_or(EditError::StaleNode(*node))?
                    .block_mut_internal()
                    .ok_or(EditError::Root)?;
                let text = block.text_mut().ok_or(EditError::WrongKind(*node))?;

                let removed = {
                    let whole = text.to_str();
                    let end = at.checked_add(*remove).ok_or(EditError::WrongKind(*node))?;
                    if end > whole.len()
                        || !whole.is_char_boundary(*at)
                        || !whole.is_char_boundary(end)
                    {
                        return Err(EditError::WrongKind(*node));
                    }
                    whole[*at..end].to_string()
                };
                text.splice(*at, *remove, insert);
                self.dirty_mut().mark(*node, Dirt::Text);

                Ok(Edit::SpliceText {
                    node: *node,
                    at: *at,
                    remove: insert.len(),
                    insert: removed,
                })
            }

            Edit::SetMeta { node, meta } => {
                let block = self
                    .arena_mut()
                    .get_mut(*node)
                    .ok_or(EditError::StaleNode(*node))?
                    .block_mut_internal()
                    .ok_or(EditError::Root)?;
                let old = block.meta().ok_or(EditError::MetaMismatch(*node))?;
                block
                    .set_meta(meta.clone())
                    .map_err(|_| EditError::MetaMismatch(*node))?;
                self.dirty_mut().mark(*node, Dirt::Text);

                Ok(Edit::SetMeta {
                    node: *node,
                    meta: old,
                })
            }

            Edit::InsertNode {
                parent,
                index,
                block,
            } => {
                self.check_container(*parent, *index, true)?;
                let id = self.arena_mut().insert(crate::document::Node::new_block(
                    Some(*parent),
                    block.clone(),
                ));
                // Re-parent any children the inserted block already names —
                // which is how the inverse of `RemoveNode` restores a subtree
                // (see `detach`). For a freshly-built block the list is empty.
                let children = block.children().unwrap_or(&[]).to_vec();
                for child in children {
                    if let Some(node) = self.arena_mut().get_mut(child) {
                        node.set_parent(Some(id));
                    }
                }
                self.children_mut_of(*parent).insert(*index, id);
                self.dirty_mut().mark(*parent, Dirt::Structure);
                self.dirty_mut().mark(id, Dirt::Structure);

                Ok(Edit::RemoveNode { node: id })
            }

            Edit::RemoveNode { node } => {
                let target = self.get(*node).ok_or(EditError::StaleNode(*node))?;
                if target.is_root() {
                    return Err(EditError::Root);
                }
                let parent = target.parent().ok_or(EditError::Detached(*node))?;
                let index = self
                    .children(parent)
                    .iter()
                    .position(|c| c == node)
                    .expect("a node's parent must list it");

                self.children_mut_of(parent).remove(index);
                self.arena_mut()
                    .get_mut(*node)
                    .expect("checked live")
                    .set_parent(None);
                self.dirty_mut().mark(parent, Dirt::Structure);

                Ok(Edit::MoveNode {
                    node: *node,
                    new_parent: parent,
                    index,
                })
            }

            Edit::MoveNode {
                node,
                new_parent,
                index,
            } => {
                let target = self.get(*node).ok_or(EditError::StaleNode(*node))?;
                if target.is_root() {
                    return Err(EditError::Root);
                }
                // `None` means the node is detached — which is exactly the
                // state `RemoveNode` leaves it in, and re-attaching it is how
                // a removal is undone.
                let old_parent = target.parent();
                if self.is_ancestor(*node, *new_parent) {
                    return Err(EditError::WouldCycle(*node));
                }
                // Kind only here: the index bound has to be checked after the
                // node has left its old parent, because moving within one
                // parent shortens the list by one first.
                self.check_container(*new_parent, 0, false)?;

                let old_index = old_parent.map(|parent| {
                    self.children(parent)
                        .iter()
                        .position(|c| c == node)
                        .expect("a node's parent must list it")
                });
                if let (Some(parent), Some(at)) = (old_parent, old_index) {
                    self.children_mut_of(parent).remove(at);
                }
                if *index > self.children(*new_parent).len() {
                    // Put it back before reporting the failure, so a rejected
                    // move leaves the tree exactly as it found it.
                    if let (Some(parent), Some(at)) = (old_parent, old_index) {
                        self.children_mut_of(parent).insert(at, *node);
                    }
                    return Err(EditError::IndexOutOfRange {
                        parent: *new_parent,
                        index: *index,
                    });
                }
                self.children_mut_of(*new_parent).insert(*index, *node);
                self.arena_mut()
                    .get_mut(*node)
                    .expect("checked live")
                    .set_parent(Some(*new_parent));

                if let Some(parent) = old_parent {
                    self.dirty_mut().mark(parent, Dirt::Structure);
                }
                self.dirty_mut().mark(*new_parent, Dirt::Structure);
                self.dirty_mut().mark(*node, Dirt::Structure);

                Ok(match (old_parent, old_index) {
                    (Some(parent), Some(at)) => Edit::MoveNode {
                        node: *node,
                        new_parent: parent,
                        index: at,
                    },
                    // The node was detached, so undoing the move means
                    // detaching it again.
                    _ => Edit::RemoveNode { node: *node },
                })
            }

            Edit::ReplaceBlock { node, block } => {
                if self
                    .get(*node)
                    .ok_or(EditError::StaleNode(*node))?
                    .is_root()
                {
                    return Err(EditError::Root);
                }
                // Adopt whatever children the new block names, and detach the
                // old block's — symmetrical with `InsertNode`, so that
                // `ReplaceBlock` is its own inverse in both directions.
                let adopted = block.children().unwrap_or(&[]).to_vec();
                let old = self
                    .arena_mut()
                    .get_mut(*node)
                    .expect("checked live")
                    .replace_block(block.clone());
                for child in adopted {
                    if let Some(child_node) = self.arena_mut().get_mut(child) {
                        child_node.set_parent(Some(*node));
                    }
                }
                for child in old.children().unwrap_or(&[]) {
                    if let Some(child_node) = self.arena_mut().get_mut(*child) {
                        child_node.set_parent(None);
                    }
                }
                self.dirty_mut().mark(*node, Dirt::Structure);

                Ok(Edit::ReplaceBlock {
                    node: *node,
                    block: old,
                })
            }
        }
    }

    fn check_container(
        &self,
        parent: NodeId,
        index: usize,
        inclusive_bound: bool,
    ) -> Result<(), EditError> {
        let node = self.get(parent).ok_or(EditError::StaleNode(parent))?;
        if !node.is_root() && node.block().and_then(Block::children).is_none() {
            return Err(EditError::WrongKind(parent));
        }
        let len = node.children().len();
        if inclusive_bound && index > len {
            return Err(EditError::IndexOutOfRange { parent, index });
        }
        Ok(())
    }

    /// Whether `ancestor` is `node` or one of its ancestors.
    fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.get(id).and_then(crate::document::Node::parent);
        }
        false
    }

    fn children_mut_of(&mut self, parent: NodeId) -> &mut Vec<NodeId> {
        self.arena_mut()
            .get_mut(parent)
            .expect("checked live")
            .children_mut_internal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Text;
    use crate::block::{Align, BulletMarker, CodeKind, Underline};

    fn paragraph(text: &str) -> Block {
        Block::Paragraph {
            text: Text::from(text),
        }
    }

    /// A document holding one paragraph, and the paragraph's id.
    fn one_paragraph(text: &str) -> (Document, NodeId) {
        let mut doc = Document::new();
        let first = doc.children(doc.root())[0];
        doc.apply(&[Edit::ReplaceBlock {
            node: first,
            block: paragraph(text),
        }]);
        doc.clear_dirty();
        (doc, first)
    }

    fn text_of(doc: &Document, node: NodeId) -> String {
        doc.block(node)
            .and_then(Block::text)
            .expect("a leaf")
            .to_str()
            .into_owned()
    }

    // --- one round trip per variant ----------------------------------------

    #[test]
    fn splice_text_and_its_inverse() {
        let (mut doc, node) = one_paragraph("hello world");
        let undo = doc.apply(&[Edit::SpliceText {
            node,
            at: 6,
            remove: 5,
            insert: "there".to_string(),
        }]);
        assert_eq!(text_of(&doc, node), "hello there");
        assert_eq!(
            undo,
            vec![Edit::SpliceText {
                node,
                at: 6,
                remove: 5,
                insert: "world".to_string()
            }]
        );
        doc.apply(&undo);
        assert_eq!(text_of(&doc, node), "hello world");
    }

    /// The inverse's `remove` is the length of what was *inserted*, not of
    /// what was removed — an insert of 3 bytes over 0 is undone by removing 3.
    #[test]
    fn the_inverse_of_a_pure_insert_is_a_pure_delete() {
        let (mut doc, node) = one_paragraph("ac");
        let undo = doc.apply(&[Edit::SpliceText {
            node,
            at: 1,
            remove: 0,
            insert: "b".to_string(),
        }]);
        assert_eq!(text_of(&doc, node), "abc");
        assert_eq!(
            undo,
            vec![Edit::SpliceText {
                node,
                at: 1,
                remove: 1,
                insert: String::new()
            }]
        );
        doc.apply(&undo);
        assert_eq!(text_of(&doc, node), "ac");
    }

    #[test]
    fn set_meta_and_its_inverse() {
        let mut doc = Document::new();
        let node = doc.children(doc.root())[0];
        doc.apply(&[Edit::ReplaceBlock {
            node,
            block: Block::AtxHeading {
                level: 1,
                text: Text::from("Title"),
            },
        }]);

        let undo = doc.apply(&[Edit::SetMeta {
            node,
            meta: BlockMeta::AtxHeading { level: 3 },
        }]);
        assert_eq!(
            doc.block(node).and_then(Block::meta),
            Some(BlockMeta::AtxHeading { level: 3 })
        );
        assert_eq!(text_of(&doc, node), "Title", "SetMeta must not touch text");

        doc.apply(&undo);
        assert_eq!(
            doc.block(node).and_then(Block::meta),
            Some(BlockMeta::AtxHeading { level: 1 })
        );
    }

    #[test]
    fn insert_node_and_its_inverse() {
        let mut doc = Document::new();
        let root = doc.root();
        let undo = doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: paragraph("second"),
        }]);

        assert_eq!(doc.children(root).len(), 2);
        let inserted = doc.children(root)[1];
        assert_eq!(text_of(&doc, inserted), "second");
        assert_eq!(doc.node(inserted).parent(), Some(root));
        assert_eq!(undo, vec![Edit::RemoveNode { node: inserted }]);

        doc.apply(&undo);
        assert_eq!(doc.children(root).len(), 1);
    }

    #[test]
    fn remove_node_and_its_inverse() {
        let (mut doc, node) = one_paragraph("only");
        let root = doc.root();

        let undo = doc.apply(&[Edit::RemoveNode { node }]);
        assert!(doc.children(root).is_empty());
        assert!(
            doc.get(node).is_some(),
            "removal detaches rather than frees — see Document::apply"
        );
        assert_eq!(doc.node(node).parent(), None);
        assert_eq!(
            undo,
            vec![Edit::MoveNode {
                node,
                new_parent: root,
                index: 0
            }]
        );

        doc.apply(&undo);
        assert_eq!(doc.children(root).len(), 1);
        assert_eq!(
            doc.children(root)[0],
            node,
            "and the node comes back with the same id"
        );
        assert_eq!(text_of(&doc, node), "only");
    }

    /// The batch that made `RemoveNode`'s inverse `MoveNode` rather than
    /// `InsertNode`: edit a node, then remove it, in one batch. With an
    /// `InsertNode` inverse the restored node has a fresh id and undoing the
    /// splice fails on a stale handle. `tests/edit_inverse.rs` found this.
    #[test]
    fn a_batch_that_edits_a_node_and_then_removes_it_undoes_cleanly() {
        let (mut doc, node) = one_paragraph("ab");
        let root = doc.root();

        let undo = doc.apply(&[
            Edit::SpliceText {
                node,
                at: 2,
                remove: 0,
                insert: "c".to_string(),
            },
            Edit::RemoveNode { node },
        ]);
        assert!(doc.children(root).is_empty());

        doc.apply(&undo);
        assert_eq!(doc.children(root), [node]);
        assert_eq!(text_of(&doc, node), "ab");
    }

    #[test]
    fn removing_an_already_detached_node_is_rejected() {
        let (mut doc, node) = one_paragraph("a");
        doc.apply(&[Edit::RemoveNode { node }]);
        assert_eq!(
            doc.try_apply(&[Edit::RemoveNode { node }]),
            Err(EditError::Detached(node))
        );
    }

    /// The reason [`Document::detach`] exists: a removed *container* must come
    /// back with everything under it, and the inverse is a single `InsertNode`
    /// carrying one `Block`.
    #[test]
    fn removing_a_container_restores_its_whole_subtree() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: Block::BlockQuote {
                children: Vec::new(),
            },
        }]);
        let quote = doc.children(root)[1];
        doc.apply(&[
            Edit::InsertNode {
                parent: quote,
                index: 0,
                block: paragraph("inner one"),
            },
            Edit::InsertNode {
                parent: quote,
                index: 1,
                block: paragraph("inner two"),
            },
        ]);
        assert_eq!(doc.children(quote).len(), 2);

        let undo = doc.apply(&[Edit::RemoveNode { node: quote }]);
        assert_eq!(doc.children(root).len(), 1);

        doc.apply(&undo);
        let restored = doc.children(root)[1];
        assert_eq!(restored, quote, "the container keeps its id");
        assert_eq!(doc.block(restored).map(Block::name), Some("block-quote"));
        let inner = doc.children(restored).to_vec();
        assert_eq!(inner.len(), 2);
        assert_eq!(text_of(&doc, inner[0]), "inner one");
        assert_eq!(text_of(&doc, inner[1]), "inner two");
        assert_eq!(
            doc.node(inner[0]).parent(),
            Some(restored),
            "the restored children point at the restored parent, not the old id"
        );
    }

    #[test]
    fn move_node_and_its_inverse() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[
            Edit::InsertNode {
                parent: root,
                index: 1,
                block: Block::BlockQuote {
                    children: Vec::new(),
                },
            },
            Edit::InsertNode {
                parent: root,
                index: 2,
                block: paragraph("movable"),
            },
        ]);
        let quote = doc.children(root)[1];
        let movable = doc.children(root)[2];

        let undo = doc.apply(&[Edit::MoveNode {
            node: movable,
            new_parent: quote,
            index: 0,
        }]);
        assert_eq!(doc.children(root), [doc.children(root)[0], quote]);
        assert_eq!(doc.children(quote), [movable]);
        assert_eq!(doc.node(movable).parent(), Some(quote));
        assert_eq!(
            undo,
            vec![Edit::MoveNode {
                node: movable,
                new_parent: root,
                index: 2
            }]
        );

        doc.apply(&undo);
        assert_eq!(doc.children(root).len(), 3);
        assert_eq!(doc.children(root)[2], movable);
        assert!(doc.children(quote).is_empty());
    }

    #[test]
    fn moving_within_the_same_parent_reorders() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[
            Edit::InsertNode {
                parent: root,
                index: 1,
                block: paragraph("b"),
            },
            Edit::InsertNode {
                parent: root,
                index: 2,
                block: paragraph("c"),
            },
        ]);
        let [a, b, c] = <[NodeId; 3]>::try_from(doc.children(root)).expect("three");

        let undo = doc.apply(&[Edit::MoveNode {
            node: c,
            new_parent: root,
            index: 0,
        }]);
        assert_eq!(doc.children(root), [c, a, b]);
        doc.apply(&undo);
        assert_eq!(doc.children(root), [a, b, c]);
    }

    #[test]
    fn replace_block_and_its_inverse() {
        let (mut doc, node) = one_paragraph("Title");
        let undo = doc.apply(&[Edit::ReplaceBlock {
            node,
            block: Block::AtxHeading {
                level: 2,
                text: Text::from("Title"),
            },
        }]);
        assert_eq!(doc.block(node).map(Block::name), Some("atx-heading"));
        doc.apply(&undo);
        assert_eq!(doc.block(node).map(Block::name), Some("paragraph"));
        assert_eq!(text_of(&doc, node), "Title");
    }

    // --- the batch contract ------------------------------------------------

    /// §2.2: the inverse batch is the reverse order with each edit inverted.
    #[test]
    fn the_inverse_batch_is_the_reverse_order() {
        let (mut doc, node) = one_paragraph("ab");
        let undo = doc.apply(&[
            Edit::SpliceText {
                node,
                at: 2,
                remove: 0,
                insert: "c".to_string(),
            },
            Edit::SpliceText {
                node,
                at: 3,
                remove: 0,
                insert: "d".to_string(),
            },
        ]);
        assert_eq!(text_of(&doc, node), "abcd");
        // Undoing must peel `d` first, so the first inverse addresses offset 3.
        assert!(matches!(undo[0], Edit::SpliceText { at: 3, .. }));
        doc.apply(&undo);
        assert_eq!(text_of(&doc, node), "ab");
    }

    #[test]
    fn every_batch_bumps_the_revision_exactly_once() {
        let (mut doc, node) = one_paragraph("a");
        let before = doc.revision();
        doc.apply(&[
            Edit::SpliceText {
                node,
                at: 1,
                remove: 0,
                insert: "b".to_string(),
            },
            Edit::SpliceText {
                node,
                at: 2,
                remove: 0,
                insert: "c".to_string(),
            },
        ]);
        assert_eq!(doc.revision(), before + 1);
    }

    #[test]
    fn an_empty_batch_still_bumps_the_revision_and_returns_nothing() {
        let mut doc = Document::new();
        let before = doc.revision();
        assert!(doc.apply(&[]).is_empty());
        assert_eq!(doc.revision(), before + 1);
    }

    // --- D7: the two kinds of dirt -----------------------------------------

    /// D7's concrete output: `SpliceText` and `SetMeta` mark [`Dirt::Text`],
    /// the other four mark [`Dirt::Structure`].
    #[test]
    fn splice_text_marks_only_the_edited_leaf_text_dirty() {
        let (mut doc, node) = one_paragraph("a");
        doc.apply(&[Edit::SpliceText {
            node,
            at: 1,
            remove: 0,
            insert: "b".to_string(),
        }]);
        assert_eq!(doc.dirty().len(), 1);
        assert_eq!(doc.dirty().dirt(node), Some(Dirt::Text));
    }

    #[test]
    fn set_meta_is_text_dirty_not_structure_dirty() {
        let mut doc = Document::new();
        let node = doc.children(doc.root())[0];
        doc.apply(&[Edit::ReplaceBlock {
            node,
            block: Block::AtxHeading {
                level: 1,
                text: Text::from("t"),
            },
        }]);
        doc.clear_dirty();
        doc.apply(&[Edit::SetMeta {
            node,
            meta: BlockMeta::AtxHeading { level: 2 },
        }]);
        assert_eq!(doc.dirty().dirt(node), Some(Dirt::Text));
    }

    #[test]
    fn the_four_structural_edits_mark_structure() {
        let mut doc = Document::new();
        let root = doc.root();

        doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: paragraph("x"),
        }]);
        let inserted = doc.children(root)[1];
        assert_eq!(doc.dirty().dirt(root), Some(Dirt::Structure));
        assert_eq!(doc.dirty().dirt(inserted), Some(Dirt::Structure));

        doc.clear_dirty();
        doc.apply(&[Edit::ReplaceBlock {
            node: inserted,
            block: Block::ThematicBreak {
                text: Text::from("---"),
            },
        }]);
        assert_eq!(doc.dirty().dirt(inserted), Some(Dirt::Structure));

        doc.clear_dirty();
        doc.apply(&[Edit::MoveNode {
            node: inserted,
            new_parent: root,
            index: 0,
        }]);
        assert_eq!(doc.dirty().dirt(inserted), Some(Dirt::Structure));

        doc.clear_dirty();
        doc.apply(&[Edit::RemoveNode { node: inserted }]);
        assert_eq!(doc.dirty().dirt(root), Some(Dirt::Structure));
    }

    /// D7's reason for the escalation rule, as a batch rather than as a unit
    /// test of `DirtySet`.
    #[test]
    fn a_batch_that_splices_then_replaces_reports_structure() {
        let (mut doc, node) = one_paragraph("a");
        doc.apply(&[
            Edit::SpliceText {
                node,
                at: 1,
                remove: 0,
                insert: "b".to_string(),
            },
            Edit::ReplaceBlock {
                node,
                block: Block::ThematicBreak {
                    text: Text::from("---"),
                },
            },
        ]);
        assert_eq!(doc.dirty().dirt(node), Some(Dirt::Structure));
    }

    // --- rejections --------------------------------------------------------

    #[test]
    fn a_stale_node_id_is_rejected_rather_than_addressing_a_recycled_slot() {
        let (mut doc, node) = one_paragraph("a");
        doc.apply(&[Edit::RemoveNode { node }]);
        // Removal only detaches, so the slot is freed by the prune that a
        // dropped undo entry authorises. Then inserting takes it back with a
        // new generation — which is the situation D8 exists for.
        assert_eq!(doc.prune_detached(), 1);
        doc.apply(&[Edit::InsertNode {
            parent: doc.root(),
            index: 0,
            block: paragraph("different"),
        }]);

        assert_eq!(
            doc.try_apply(&[Edit::SpliceText {
                node,
                at: 0,
                remove: 0,
                insert: "!".to_string()
            }]),
            Err(EditError::StaleNode(node))
        );
        assert_eq!(
            text_of(&doc, doc.children(doc.root())[0]),
            "different",
            "and the node that took the slot is untouched"
        );
    }

    #[test]
    fn splicing_a_container_is_the_wrong_kind() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: Block::BlockQuote {
                children: Vec::new(),
            },
        }]);
        let quote = doc.children(root)[1];
        assert_eq!(
            doc.try_apply(&[Edit::SpliceText {
                node: quote,
                at: 0,
                remove: 0,
                insert: "x".to_string()
            }]),
            Err(EditError::WrongKind(quote))
        );
    }

    #[test]
    fn inserting_into_a_leaf_is_the_wrong_kind() {
        let (mut doc, node) = one_paragraph("a");
        assert_eq!(
            doc.try_apply(&[Edit::InsertNode {
                parent: node,
                index: 0,
                block: paragraph("x")
            }]),
            Err(EditError::WrongKind(node))
        );
    }

    #[test]
    fn an_index_past_the_end_is_rejected() {
        let mut doc = Document::new();
        let root = doc.root();
        assert_eq!(
            doc.try_apply(&[Edit::InsertNode {
                parent: root,
                index: 9,
                block: paragraph("x")
            }]),
            Err(EditError::IndexOutOfRange {
                parent: root,
                index: 9
            })
        );
        assert_eq!(doc.children(root).len(), 1, "and nothing was inserted");
    }

    #[test]
    fn a_meta_from_another_variant_is_rejected() {
        let (mut doc, node) = one_paragraph("a");
        // A paragraph has no meta at all, which is the sharper case: the
        // error is the same either way.
        assert_eq!(
            doc.try_apply(&[Edit::SetMeta {
                node,
                meta: BlockMeta::TableCell {
                    align: Align::Center
                }
            }]),
            Err(EditError::MetaMismatch(node))
        );
    }

    #[test]
    fn a_move_that_would_make_a_node_its_own_ancestor_is_rejected() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: Block::BlockQuote {
                children: Vec::new(),
            },
        }]);
        let outer = doc.children(root)[1];
        doc.apply(&[Edit::InsertNode {
            parent: outer,
            index: 0,
            block: Block::BlockQuote {
                children: Vec::new(),
            },
        }]);
        let inner = doc.children(outer)[0];

        assert_eq!(
            doc.try_apply(&[Edit::MoveNode {
                node: outer,
                new_parent: inner,
                index: 0
            }]),
            Err(EditError::WouldCycle(outer))
        );
        assert_eq!(doc.children(outer), [inner], "the tree is unchanged");
        assert_eq!(doc.node(inner).parent(), Some(outer));
    }

    #[test]
    fn the_root_cannot_be_removed_moved_or_replaced() {
        let mut doc = Document::new();
        let root = doc.root();
        assert_eq!(
            doc.try_apply(&[Edit::RemoveNode { node: root }]),
            Err(EditError::Root)
        );
        assert_eq!(
            doc.try_apply(&[Edit::MoveNode {
                node: root,
                new_parent: root,
                index: 0
            }]),
            Err(EditError::Root)
        );
        assert_eq!(
            doc.try_apply(&[Edit::ReplaceBlock {
                node: root,
                block: paragraph("x")
            }]),
            Err(EditError::Root)
        );
        assert_eq!(
            doc.try_apply(&[Edit::SpliceText {
                node: root,
                at: 0,
                remove: 0,
                insert: "x".to_string()
            }]),
            Err(EditError::Root)
        );
    }

    #[test]
    fn splicing_past_the_end_of_a_leaf_is_rejected_rather_than_panicking() {
        let (mut doc, node) = one_paragraph("ab");
        assert_eq!(
            doc.try_apply(&[Edit::SpliceText {
                node,
                at: 1,
                remove: 5,
                insert: String::new()
            }]),
            Err(EditError::WrongKind(node))
        );
        assert_eq!(text_of(&doc, node), "ab");
    }

    /// M1's D1 again, one layer up: an offset that splits a `char` is rejected
    /// at `apply`, not carried into `Text::splice`'s assertion.
    #[test]
    fn a_splice_offset_inside_a_char_is_rejected() {
        let (mut doc, node) = one_paragraph("é");
        assert_eq!(
            doc.try_apply(&[Edit::SpliceText {
                node,
                at: 1,
                remove: 0,
                insert: "x".to_string()
            }]),
            Err(EditError::WrongKind(node))
        );
    }

    // --- detached subtrees --------------------------------------------------

    /// The price of §2.2's shape, made visible: a removal leaves its
    /// descendants in the arena so that the inverse can restore them.
    #[test]
    fn a_removal_leaves_its_descendants_in_the_arena_for_the_inverse() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: Block::BlockQuote {
                children: Vec::new(),
            },
        }]);
        let quote = doc.children(root)[1];
        doc.apply(&[Edit::InsertNode {
            parent: quote,
            index: 0,
            block: paragraph("inner"),
        }]);
        let inner = doc.children(quote)[0];
        let before = doc.arena_len();

        let undo = doc.apply(&[Edit::RemoveNode { node: quote }]);
        assert_eq!(
            doc.arena_len(),
            before,
            "removal frees nothing — the subtree is only detached"
        );
        assert!(doc.get(inner).is_some(), "its child is detached, not freed");
        assert_eq!(
            doc.node(inner).parent(),
            Some(quote),
            "and it stays attached to the detached container, so the undo is one move"
        );
        assert_eq!(doc.node(quote).parent(), None);

        // Discard the undo entry, as M4's history trimming will, and the
        // detached subtree becomes reclaimable — both nodes, not just one.
        drop(undo);
        assert_eq!(doc.prune_detached(), 2);
        assert!(doc.get(inner).is_none());
        assert!(doc.get(quote).is_none());
        assert_eq!(doc.arena_len(), before - 2);
    }

    /// A code block promoted from indented to fenced is the transformation
    /// `SetMeta` exists for — `blockSerialization.spec.ts` drives it through
    /// the editor, and this is the model half of it.
    #[test]
    fn set_meta_promotes_an_indented_code_block_to_fenced() {
        let mut doc = Document::new();
        let node = doc.children(doc.root())[0];
        doc.apply(&[Edit::ReplaceBlock {
            node,
            block: Block::CodeBlock {
                kind: CodeKind::Indented,
                info: String::new(),
                fence_len: None,
                text: Text::from("code"),
            },
        }]);
        let undo = doc.apply(&[Edit::SetMeta {
            node,
            meta: BlockMeta::CodeBlock {
                kind: CodeKind::Fenced,
                info: "js".to_string(),
                fence_len: Some(3),
            },
        }]);
        assert_eq!(
            doc.block(node).and_then(Block::highlight_language),
            Some("js")
        );
        assert_eq!(text_of(&doc, node), "code", "the body is untouched");

        doc.apply(&undo);
        assert_eq!(
            doc.block(node).and_then(Block::meta),
            Some(BlockMeta::CodeBlock {
                kind: CodeKind::Indented,
                info: String::new(),
                fence_len: None
            })
        );
    }

    #[test]
    fn a_setext_heading_keeps_its_underline_through_set_meta() {
        let mut doc = Document::new();
        let node = doc.children(doc.root())[0];
        doc.apply(&[Edit::ReplaceBlock {
            node,
            block: Block::SetextHeading {
                level: 1,
                underline: Underline::Equals,
                text: Text::from("Hello"),
            },
        }]);
        doc.apply(&[Edit::SetMeta {
            node,
            meta: BlockMeta::SetextHeading {
                level: 2,
                underline: Underline::Dashes,
            },
        }]);
        assert_eq!(
            doc.block(node).and_then(Block::meta),
            Some(BlockMeta::SetextHeading {
                level: 2,
                underline: Underline::Dashes
            })
        );
    }

    #[test]
    fn a_bullet_lists_marker_and_looseness_survive_a_set_meta_round_trip() {
        let mut doc = Document::new();
        let root = doc.root();
        doc.apply(&[Edit::InsertNode {
            parent: root,
            index: 1,
            block: Block::BulletList {
                marker: BulletMarker::Dash,
                loose: false,
                children: Vec::new(),
            },
        }]);
        let list = doc.children(root)[1];
        let undo = doc.apply(&[Edit::SetMeta {
            node: list,
            meta: BlockMeta::BulletList {
                marker: BulletMarker::Star,
                loose: true,
            },
        }]);
        assert_eq!(
            doc.block(list).and_then(Block::meta),
            Some(BlockMeta::BulletList {
                marker: BulletMarker::Star,
                loose: true
            })
        );
        doc.apply(&undo);
        assert_eq!(
            doc.block(list).and_then(Block::meta),
            Some(BlockMeta::BulletList {
                marker: BulletMarker::Dash,
                loose: false
            })
        );
    }
}
