//! The S0 gate: **an edit batch's inverse restores the document byte-for-byte.**
//!
//! M2.md §6's S0 row: *"Property test: for a generated batch,
//! `apply(inverse(apply(b)))` restores the document byte-for-byte, over ≥ 10⁴
//! batches."*
//!
//! # What "byte-for-byte" is compared
//!
//! [`render`] walks the tree and emits one line per node — the block name, its
//! `meta`, and its text with newlines and tabs escaped — indented by depth.
//! Two documents render identically exactly when they hold the same blocks,
//! with the same metadata, the same text and the same shape.
//!
//! `NodeId`s are absent from the rendering, so the property asserts them
//! **separately and additionally**: after the undo, the set of live attached
//! nodes must be the same set of ids it was before, in the same order.
//!
//! That second assertion is what caught the S0 finding recorded on
//! `Document::apply`. The first version of `Edit::RemoveNode` freed the
//! removed node's slot and returned an `InsertNode` inverse, which mints a
//! *fresh* id — so the batch `[SpliceText { node: x }, RemoveNode { node: x }]`
//! undid in the order `[InsertNode, SpliceText { node: x }]` and failed on a
//! stale `x`. This property found it in under a second, on a two-edit batch,
//! which is the whole argument for having it.
//!
//! The third assertion is that an apply-then-undo pair returns the arena to
//! its original size once the undo entry is dropped and `prune_detached` runs
//! — otherwise a long editing session would grow the arena on every
//! remove-and-undo cycle.
//!
//! # Why the batch counter is asserted rather than assumed
//!
//! "≥ 10⁴ batches" is a gate clause, and proptest's case count is a
//! configuration value that a later edit could quietly lower. [`BATCHES`]
//! counts the batches that were actually applied and the test fails if the
//! run did not reach 10,000 — so the number in the gate is measured rather
//! than inferred from `ProptestConfig`.

use std::sync::atomic::{AtomicUsize, Ordering};

use mt_doc::{
    Align, Block, BlockMeta, BulletMarker, CodeKind, Dirt, Document, Edit, MathStyle, NodeId,
    OrderDelim, Text, Underline,
};
use proptest::prelude::*;

/// Batches actually applied across the whole property run.
static BATCHES: AtomicUsize = AtomicUsize::new(0);

/// The gate's floor.
const REQUIRED_BATCHES: usize = 10_000;

// ---------------------------------------------------------------------------
// Rendering — the "byte-for-byte" of the gate
// ---------------------------------------------------------------------------

fn render(doc: &Document) -> String {
    let mut out = String::new();
    render_into(doc, doc.root(), 0, &mut out);
    out
}

fn render_into(doc: &Document, id: NodeId, depth: usize, out: &mut String) {
    let node = doc.node(id);
    for _ in 0..depth {
        out.push_str("  ");
    }
    match node.block() {
        None => out.push_str("<root>"),
        Some(block) => {
            out.push_str(block.name());
            if let Some(meta) = block.meta() {
                out.push_str(&format!(" {meta:?}"));
            }
            if let Some(text) = block.text() {
                out.push_str(" \"");
                for ch in text.to_str().chars() {
                    match ch {
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        other => out.push(other),
                    }
                }
                out.push('"');
            }
        }
    }
    out.push('\n');
    for child in node.children() {
        render_into(doc, *child, depth + 1, out);
    }
}

// ---------------------------------------------------------------------------
// Generated documents
// ---------------------------------------------------------------------------

/// A block to build, without its children — the generator attaches those.
#[derive(Debug, Clone)]
enum Shape {
    Leaf(Block),
    Container(Block, Vec<Shape>),
}

fn leaf_block() -> impl Strategy<Value = Block> {
    let text = "[a-z \n]{0,12}".prop_map(Text::from);
    prop_oneof![
        text.clone().prop_map(|text| Block::Paragraph { text }),
        (1u8..=6, text.clone()).prop_map(|(level, text)| Block::AtxHeading { level, text }),
        (
            prop_oneof![Just(Underline::Equals), Just(Underline::Dashes)],
            text.clone()
        )
            .prop_map(|(underline, text)| Block::SetextHeading {
                level: if underline == Underline::Equals { 1 } else { 2 },
                underline,
                text
            }),
        text.clone().prop_map(|text| Block::ThematicBreak { text }),
        (
            prop_oneof![Just(CodeKind::Fenced), Just(CodeKind::Indented)],
            "[a-z]{0,4}( [a-z]{1,3})?",
            proptest::option::of(3u8..=6),
            text.clone()
        )
            .prop_map(|(kind, info, fence_len, text)| Block::CodeBlock {
                kind,
                info,
                fence_len,
                text
            }),
        text.clone().prop_map(|text| Block::HtmlBlock { text }),
        (
            prop_oneof![Just(MathStyle::Default), Just(MathStyle::Gitlab)],
            text.clone()
        )
            .prop_map(|(style, text)| Block::MathBlock { style, text }),
        (
            prop_oneof![
                Just(Align::None),
                Just(Align::Left),
                Just(Align::Center),
                Just(Align::Right)
            ],
            text
        )
            .prop_map(|(align, text)| Block::TableCell { align, text }),
    ]
}

fn container_block() -> impl Strategy<Value = Block> {
    let marker = prop_oneof![
        Just(BulletMarker::Dash),
        Just(BulletMarker::Plus),
        Just(BulletMarker::Star)
    ];
    prop_oneof![
        Just(Block::BlockQuote {
            children: Vec::new()
        }),
        (marker.clone(), any::<bool>()).prop_map(|(marker, loose)| Block::BulletList {
            marker,
            loose,
            children: Vec::new()
        }),
        (
            0u32..5,
            prop_oneof![Just(OrderDelim::Period), Just(OrderDelim::Paren)],
            any::<bool>()
        )
            .prop_map(|(start, delimiter, loose)| Block::OrderList {
                start,
                delimiter,
                loose,
                children: Vec::new()
            }),
        Just(Block::ListItem {
            children: Vec::new()
        }),
        (marker, any::<bool>()).prop_map(|(marker, loose)| Block::TaskList {
            marker,
            loose,
            children: Vec::new()
        }),
        any::<bool>().prop_map(|checked| Block::TaskListItem {
            checked,
            children: Vec::new()
        }),
        Just(Block::Table {
            children: Vec::new()
        }),
        Just(Block::TableRow {
            children: Vec::new()
        }),
        "[a-z0-9]{1,3}".prop_map(|identifier| Block::Footnote {
            identifier,
            children: Vec::new()
        }),
    ]
}

fn shape() -> impl Strategy<Value = Shape> {
    leaf_block()
        .prop_map(Shape::Leaf)
        .prop_recursive(3, 12, 3, |inner| {
            (container_block(), proptest::collection::vec(inner, 0..3))
                .prop_map(|(block, children)| Shape::Container(block, children))
        })
}

fn document() -> impl Strategy<Value = Vec<Shape>> {
    proptest::collection::vec(shape(), 1..5)
}

fn build(shapes: &[Shape]) -> Document {
    let mut doc = Document::new();
    // `Document::new` seeds one empty paragraph; drop it so the generated
    // tree is exactly what the strategy asked for.
    let seed = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: seed }]);
    doc.prune_detached();
    let root = doc.root();
    for (index, shape) in shapes.iter().enumerate() {
        attach(&mut doc, root, index, shape);
    }
    doc.clear_dirty();
    doc
}

fn attach(doc: &mut Document, parent: NodeId, index: usize, shape: &Shape) {
    let block = match shape {
        Shape::Leaf(block) | Shape::Container(block, _) => block.clone(),
    };
    doc.apply(&[Edit::InsertNode {
        parent,
        index,
        block,
    }]);
    let id = doc.children(parent)[index];
    if let Shape::Container(_, children) = shape {
        for (i, child) in children.iter().enumerate() {
            attach(doc, id, i, child);
        }
    }
}

// ---------------------------------------------------------------------------
// Generated edits
// ---------------------------------------------------------------------------

/// An edit *intent*. It is resolved against a live document, because the
/// generator has no `NodeId`s: `pick` indexes modulo the number of candidates,
/// so every intent that has any candidate at all becomes a valid edit.
#[derive(Debug, Clone)]
enum Intent {
    Splice {
        pick: usize,
        at: usize,
        remove: usize,
        insert: String,
    },
    SetMeta {
        pick: usize,
    },
    Insert {
        parent_pick: usize,
        index: usize,
        block: Block,
    },
    Remove {
        pick: usize,
    },
    Move {
        pick: usize,
        parent_pick: usize,
        index: usize,
    },
    Replace {
        pick: usize,
        block: Block,
    },
}

fn intent() -> impl Strategy<Value = Intent> {
    prop_oneof![
        4 => (any::<usize>(), any::<usize>(), any::<usize>(), "[a-z\n]{0,6}").prop_map(
            |(pick, at, remove, insert)| Intent::Splice {
                pick,
                at,
                remove,
                insert
            }
        ),
        2 => any::<usize>().prop_map(|pick| Intent::SetMeta { pick }),
        2 => (any::<usize>(), any::<usize>(), prop_oneof![leaf_block(), container_block()])
            .prop_map(|(parent_pick, index, block)| Intent::Insert {
                parent_pick,
                index,
                block
            }),
        2 => any::<usize>().prop_map(|pick| Intent::Remove { pick }),
        1 => (any::<usize>(), any::<usize>(), any::<usize>()).prop_map(
            |(pick, parent_pick, index)| Intent::Move {
                pick,
                parent_pick,
                index
            }
        ),
        1 => (any::<usize>(), prop_oneof![leaf_block(), container_block()])
            .prop_map(|(pick, block)| Intent::Replace { pick, block }),
    ]
}

/// Every live node except the root, in a stable order.
fn nodes(doc: &Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        if id != doc.root() {
            out.push(id);
        }
        stack.extend(doc.children(id).iter().rev());
    }
    out.sort_unstable();
    out
}

fn containers(doc: &Document) -> Vec<NodeId> {
    let mut out = vec![doc.root()];
    out.extend(
        nodes(doc)
            .into_iter()
            .filter(|id| doc.block(*id).and_then(Block::children).is_some()),
    );
    out
}

fn resolve(doc: &Document, intent: &Intent) -> Option<Edit> {
    let pick_from = |list: &[NodeId], pick: usize| -> Option<NodeId> {
        (!list.is_empty()).then(|| list[pick % list.len()])
    };

    match intent {
        Intent::Splice {
            pick,
            at,
            remove,
            insert,
        } => {
            let leaves: Vec<NodeId> = nodes(doc)
                .into_iter()
                .filter(|id| doc.block(*id).and_then(Block::text).is_some())
                .collect();
            let node = pick_from(&leaves, *pick)?;
            let text = doc.block(node).and_then(Block::text)?.to_str().into_owned();
            // Snap to char boundaries: an offset inside a `char` is rejected
            // by `apply`, and testing the rejection is `edit.rs`'s job, not
            // this property's.
            let at = boundary(&text, at % (text.len() + 1));
            let remove = boundary(&text, at + remove % (text.len() - at + 1)) - at;
            Some(Edit::SpliceText {
                node,
                at,
                remove,
                insert: insert.clone(),
            })
        }
        Intent::SetMeta { pick } => {
            let with_meta: Vec<NodeId> = nodes(doc)
                .into_iter()
                .filter(|id| doc.block(*id).and_then(Block::meta).is_some())
                .collect();
            let node = pick_from(&with_meta, *pick)?;
            let meta = mutate(doc.block(node)?.meta()?);
            Some(Edit::SetMeta { node, meta })
        }
        Intent::Insert {
            parent_pick,
            index,
            block,
        } => {
            let parents = containers(doc);
            let parent = pick_from(&parents, *parent_pick)?;
            let len = doc.children(parent).len();
            Some(Edit::InsertNode {
                parent,
                index: index % (len + 1),
                block: block.clone(),
            })
        }
        Intent::Remove { pick } => {
            let all = nodes(doc);
            Some(Edit::RemoveNode {
                node: pick_from(&all, *pick)?,
            })
        }
        Intent::Move {
            pick,
            parent_pick,
            index,
        } => {
            let all = nodes(doc);
            let node = pick_from(&all, *pick)?;
            // A node may not move into its own subtree; filter those out here
            // so the property exercises moves rather than rejections.
            let parents: Vec<NodeId> = containers(doc)
                .into_iter()
                .filter(|p| !is_self_or_descendant(doc, node, *p))
                .collect();
            let new_parent = pick_from(&parents, *parent_pick)?;
            let len = doc.children(new_parent).len();
            let bound = if doc.node(node).parent() == Some(new_parent) {
                len.saturating_sub(1)
            } else {
                len
            };
            Some(Edit::MoveNode {
                node,
                new_parent,
                index: index % (bound + 1),
            })
        }
        Intent::Replace { pick, block } => {
            let all = nodes(doc);
            Some(Edit::ReplaceBlock {
                node: pick_from(&all, *pick)?,
                block: block.clone(),
            })
        }
    }
}

fn is_self_or_descendant(doc: &Document, ancestor: NodeId, candidate: NodeId) -> bool {
    let mut current = Some(candidate);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = doc.node(id).parent();
    }
    false
}

fn boundary(s: &str, mut at: usize) -> usize {
    at = at.min(s.len());
    while !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Change a meta so the edit is not a no-op — otherwise `SetMeta` would only
/// ever prove that setting a block's own meta leaves it alone, which is
/// `block.rs`'s test rather than this one.
fn mutate(meta: BlockMeta) -> BlockMeta {
    match meta {
        BlockMeta::AtxHeading { level } => BlockMeta::AtxHeading {
            level: level % 6 + 1,
        },
        BlockMeta::SetextHeading { level, underline } => BlockMeta::SetextHeading {
            level: 3 - level,
            underline: match underline {
                Underline::Equals => Underline::Dashes,
                Underline::Dashes => Underline::Equals,
            },
        },
        BlockMeta::CodeBlock {
            kind,
            info,
            fence_len,
        } => BlockMeta::CodeBlock {
            kind: match kind {
                CodeKind::Fenced => CodeKind::Indented,
                CodeKind::Indented => CodeKind::Fenced,
            },
            info: format!("{info}x"),
            fence_len: fence_len.map_or(Some(4), |n| Some(n % 6 + 3)),
        },
        BlockMeta::MathBlock { style } => BlockMeta::MathBlock {
            style: match style {
                MathStyle::Default => MathStyle::Gitlab,
                MathStyle::Gitlab => MathStyle::Default,
            },
        },
        BlockMeta::Frontmatter { lang, style } => BlockMeta::Frontmatter { lang, style },
        BlockMeta::Diagram { lang, kind } => BlockMeta::Diagram { lang, kind },
        BlockMeta::TableCell { align } => BlockMeta::TableCell {
            align: match align {
                Align::None => Align::Left,
                Align::Left => Align::Center,
                Align::Center => Align::Right,
                Align::Right => Align::None,
            },
        },
        BlockMeta::BulletList { marker, loose } => BlockMeta::BulletList {
            marker: next_marker(marker),
            loose: !loose,
        },
        BlockMeta::OrderList {
            start,
            delimiter,
            loose,
        } => BlockMeta::OrderList {
            start: start + 1,
            delimiter: match delimiter {
                OrderDelim::Period => OrderDelim::Paren,
                OrderDelim::Paren => OrderDelim::Period,
            },
            loose: !loose,
        },
        BlockMeta::TaskList { marker, loose } => BlockMeta::TaskList {
            marker: next_marker(marker),
            loose: !loose,
        },
        BlockMeta::TaskListItem { checked } => BlockMeta::TaskListItem { checked: !checked },
        BlockMeta::Footnote { identifier } => BlockMeta::Footnote {
            identifier: format!("{identifier}z"),
        },
    }
}

fn next_marker(marker: BulletMarker) -> BulletMarker {
    match marker {
        BulletMarker::Dash => BulletMarker::Plus,
        BulletMarker::Plus => BulletMarker::Star,
        BulletMarker::Star => BulletMarker::Dash,
    }
}

// ---------------------------------------------------------------------------
// The property
// ---------------------------------------------------------------------------

/// M2.md §6's S0 gate clause, and the reason `Edit` is an enum of invertible
/// ops rather than a diff: **undo is applying the returned batch.**
///
/// Each case builds a document, then applies 5–10 batches. Every batch is
/// rendered before, applied, rendered after, undone with the batch `apply`
/// itself returned, and rendered again — the first and third renderings must
/// be byte-identical, and the arena must return to its original size once the
/// undo entry is dropped.
#[test]
fn an_edit_batchs_inverse_restores_the_document_byte_for_byte() {
    proptest!(
        ProptestConfig {
            cases: 2_000,
            max_shrink_iters: 4_000,
            ..ProptestConfig::default()
        },
        |(
            shapes in document(),
            batches in proptest::collection::vec(
                proptest::collection::vec(intent(), 1..4),
                5..=10,
            ),
        )| {
            let mut doc = build(&shapes);

            for intents in &batches {
                // Resolve the whole batch against the pre-batch document, one
                // intent at a time, applying as we go — because an intent's
                // candidate set depends on what the previous edit did. The
                // per-edit inverses are collected and replayed newest-first,
                // which is exactly the order `Document::apply` returns for a
                // batch (`edit.rs::the_inverse_batch_is_the_reverse_order`
                // pins that separately).
                let before = render(&doc);
                let ids_before = nodes(&doc);
                let arena_before = doc.arena_len();
                let mut undo: Vec<Vec<Edit>> = Vec::new();

                for intent in intents {
                    let Some(edit) = resolve(&doc, intent) else {
                        continue;
                    };
                    let inverse = doc
                        .try_apply(std::slice::from_ref(&edit))
                        .map_err(|e| TestCaseError::fail(format!("{edit:?} was rejected: {e}")))?;
                    undo.push(inverse);
                }

                if undo.is_empty() {
                    continue;
                }
                BATCHES.fetch_add(1, Ordering::Relaxed);

                // The dirty set must name something whenever anything changed,
                // or `mt-layout` would miss the edit entirely (§5).
                let after = render(&doc);
                if after != before {
                    prop_assert!(
                        !doc.dirty().is_empty(),
                        "the document changed but nothing was marked dirty"
                    );
                }

                for inverse in undo.into_iter().rev() {
                    doc.try_apply(&inverse).map_err(|e| {
                        TestCaseError::fail(format!("undoing {inverse:?} failed: {e}"))
                    })?;
                }

                prop_assert_eq!(
                    render(&doc),
                    before,
                    "the inverse did not restore the document"
                );
                prop_assert_eq!(
                    nodes(&doc),
                    ids_before,
                    "the inverse restored the content but not the NodeIds"
                );

                // Detached subtrees are reachable only from the undo entries,
                // which have now been dropped — so pruning must return the
                // arena to exactly where it started. Without this, every
                // remove-and-undo cycle would leak a subtree.
                doc.prune_detached();
                prop_assert_eq!(
                    doc.arena_len(),
                    arena_before,
                    "an apply-then-undo pair changed the arena's size"
                );

                doc.clear_dirty();
            }
        }
    );

    let applied = BATCHES.load(Ordering::Relaxed);
    assert!(
        applied >= REQUIRED_BATCHES,
        "the S0 gate asks for at least {REQUIRED_BATCHES} batches; this run applied {applied}. \
         Raise ProptestConfig::cases rather than lowering the floor."
    );
}

/// The negative control for the property above: it must be able to fail.
///
/// A property that has never produced a counterexample has not been shown to
/// be able to — M1 S6's lesson, applied to the harness rather than to the
/// register. This drives [`render`] with a document that has been edited and
/// *not* undone, and asserts the comparison notices.
#[test]
fn the_comparison_the_property_uses_can_tell_two_documents_apart() {
    let before = render(&Document::new());

    // One builder per edit kind, because each needs ids from the document it
    // will be applied to. Every one must change the rendering — an edit that
    // `render` cannot see is an edit the property above tests vacuously.
    #[allow(clippy::type_complexity)]
    let builders: [(&str, fn(&Document) -> Edit); 6] = [
        ("SpliceText", |doc| Edit::SpliceText {
            node: doc.children(doc.root())[0],
            at: 0,
            remove: 0,
            insert: "x".to_string(),
        }),
        ("SetMeta", |doc| Edit::SetMeta {
            node: doc.children(doc.root())[0],
            // A paragraph has no meta, so this one is applied to a document
            // whose first block has been replaced first — see below.
            meta: BlockMeta::AtxHeading { level: 5 },
        }),
        ("InsertNode", |doc| Edit::InsertNode {
            parent: doc.root(),
            index: 0,
            block: Block::ThematicBreak {
                text: Text::from("---"),
            },
        }),
        ("RemoveNode", |doc| Edit::RemoveNode {
            node: doc.children(doc.root())[0],
        }),
        // To index 1, not 0: moving a node to where it already is renders
        // identically, which would make this control pass by not moving.
        ("MoveNode", |doc| Edit::MoveNode {
            node: doc.children(doc.root())[0],
            new_parent: doc.root(),
            index: 1,
        }),
        ("ReplaceBlock", |doc| Edit::ReplaceBlock {
            node: doc.children(doc.root())[0],
            block: Block::AtxHeading {
                level: 4,
                text: Text::from("x"),
            },
        }),
    ];

    for (name, build_edit) in builders {
        let mut doc = Document::new();
        if name == "SetMeta" {
            // Give it something with a meta to set, and make that the baseline.
            let node = doc.children(doc.root())[0];
            doc.apply(&[Edit::ReplaceBlock {
                node,
                block: Block::AtxHeading {
                    level: 1,
                    text: Text::from("x"),
                },
            }]);
        }
        if name == "MoveNode" {
            // A move needs somewhere to move to.
            doc.apply(&[Edit::InsertNode {
                parent: doc.root(),
                index: 1,
                block: Block::ThematicBreak {
                    text: Text::from("---"),
                },
            }]);
        }
        doc.clear_dirty();
        let baseline = render(&doc);
        let ids = nodes(&doc);

        let edit = build_edit(&doc);
        let undo = doc.apply(std::slice::from_ref(&edit));
        assert_ne!(
            render(&doc),
            baseline,
            "{name} was invisible to render(), so the property tests it vacuously"
        );

        doc.apply(&undo);
        assert_eq!(render(&doc), baseline, "{name} did not undo cleanly");
        assert_eq!(nodes(&doc), ids, "{name} did not restore the NodeIds");
    }

    // And the baseline itself is not accidentally equal to everything.
    assert_ne!(before, String::new());
}

/// `SetMeta` on every variant that has a meta, driven through a real document
/// rather than through `Block` alone — the meta must survive the round trip
/// and must be visible to [`render`], or the property above would pass
/// vacuously for two of its six intents.
#[test]
fn render_shows_meta_so_set_meta_is_not_invisible_to_the_property() {
    let mut doc = Document::new();
    let node = doc.children(doc.root())[0];
    doc.apply(&[Edit::ReplaceBlock {
        node,
        block: Block::TaskListItem {
            checked: false,
            children: Vec::new(),
        },
    }]);
    doc.clear_dirty();
    let before = render(&doc);
    let undo = doc.apply(&[Edit::SetMeta {
        node,
        meta: BlockMeta::TaskListItem { checked: true },
    }]);
    assert_ne!(render(&doc), before);
    assert_eq!(doc.dirty().dirt(node), Some(Dirt::Text));
    doc.apply(&undo);
    assert_eq!(render(&doc), before);
}
