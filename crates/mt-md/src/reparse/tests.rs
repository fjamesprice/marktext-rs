//! S6's three gate clauses, at their narrowest, plus the pieces they are made
//! of. The wide form — over generated edit sequences, against a full reparse —
//! is `crates/mt-md/tests/reparse_properties.rs`.

use super::*;
use crate::state::to_state_json;

const SPEC: Options = Options::SPEC;
const MUYA: Options = Options::MUYA_DEFAULT;

fn ids(doc: &Document) -> Vec<NodeId> {
    fn walk(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
        for child in doc.children(id) {
            out.push(*child);
            walk(doc, *child, out);
        }
    }
    let mut out = Vec::new();
    walk(doc, doc.root(), &mut out);
    out
}

/// The tree, as `dump_state` would serialize it — which is the comparison every
/// harness in this milestone makes, so an incremental tree that matches here
/// matches there.
fn state(doc: &Document) -> String {
    to_state_json(doc)
}

// --- clause 1: a leaf-text edit reparses one block -------------------------

/// **The gate's counter.** Not a timer: the claim is that the parser rebuilt
/// exactly one block, and the only way to say that is to count.
#[test]
fn an_edit_to_a_leafs_text_reparses_one_block() {
    let mut doc = Incremental::new("# h\n\nalpha\n\nbravo\n", SPEC);
    let at = doc.source().find("alpha").expect("there") + 5;

    let report = doc.edit(at, 0, "X");

    assert_eq!(report.blocks_reparsed, 1);
    assert_eq!(report.blocks_total, 3);
    assert!(matches!(report.path, ReparsePath::LeafText { .. }));
    assert_eq!(doc.source(), "# h\n\nalphaX\n\nbravo\n");
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), SPEC).document)
    );
}

/// And the block it rebuilt is one block *of the tree* rather than one block of
/// a region — the distinction §4.1 step 1 exists for. A paragraph three
/// containers deep is still one.
#[test]
fn one_block_stays_one_block_inside_a_quote_inside_a_list() {
    let source = "- a\n\n- > deep text\n\n- c\n";
    let mut doc = Incremental::new(source, SPEC);
    let at = doc.source().find("deep").expect("there") + 4;

    let report = doc.edit(at, 0, "er");

    assert_eq!(report.blocks_reparsed, 1);
    assert!(matches!(report.path, ReparsePath::LeafText { .. }));
    assert_eq!(doc.source(), "- a\n\n- > deeper text\n\n- c\n");
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), SPEC).document)
    );
}

/// The dirty set is D7's, and this is the distinction D7 settled at S0 so that
/// S6 would not have to revisit every call site: a text edit is `Dirt::Text` on
/// **one** node and nothing else.
#[test]
fn a_leaf_text_edit_dirties_one_node_and_marks_it_text() {
    let mut doc = Incremental::new("alpha\n\nbravo\n", SPEC);
    doc.document_mut().clear_dirty();
    let at = doc.source().find("alpha").expect("there") + 5;

    let report = doc.edit(at, 0, "X");
    let ReparsePath::LeafText { node } = report.path else {
        panic!("expected the leaf-text path, got {:?}", report.path);
    };

    assert_eq!(doc.document().dirty().len(), 1);
    assert_eq!(doc.document().dirty().dirt(node), Some(mt_doc::Dirt::Text));
}

/// The guards, each with the input that needs it. Every one of these takes the
/// region path instead, which is correct rather than fast — and the property
/// test is what proves the *fast* one is never wrong.
#[test]
fn the_leaf_text_path_declines_anything_that_could_move_a_block_boundary() {
    let decline = |source: &str, at: usize, remove: usize, insert: &str, why: &str| {
        let mut doc = Incremental::new(source, SPEC);
        let report = doc.edit(at, remove, insert);
        assert!(
            matches!(report.path, ReparsePath::Region),
            "{why}: took the leaf-text path"
        );
    };

    decline("alpha\n", 5, 0, "\nbravo", "a newline splits the block");
    decline("alpha\n", 0, 0, "#", "at a line start, `#` is a heading");
    decline(
        "alpha\n",
        0,
        0,
        "    ",
        "at a line start, four spaces are code",
    );
    decline(
        "a b\n",
        3,
        0,
        "|",
        "a pipe changes a header row's cell count",
    );
    decline("$a\n", 2, 0, "$", "a second dollar can open block math");
    decline(
        "a\n\nb\n",
        3,
        1,
        "x",
        "an edit at a block's very start moves its range",
    );
    decline(
        "# h\n",
        3,
        0,
        "x",
        "an atx heading's text is not the stripper's",
    );
    decline(
        "> 1 x\n",
        3,
        0,
        ".",
        "`1` then `.` is an ordered list marker",
    );
    decline(
        "- - x\n",
        4,
        0,
        "y",
        "the line's first character is a bullet",
    );
}

/// The two guards no option set every harness drives can reach, at the option
/// sets that can.
#[test]
fn the_leaf_text_path_declines_a_coarse_range_and_a_movable_front_matter() {
    // Every node under a `footnote` carries the *definition's* range (S5, §10),
    // so `SourceMap::is_exact` is `false` for it and no re-derivation may trust
    // it. This is the check that stopped a list marker from being spliced as
    // though it were a paragraph's own text.
    let options = Options {
        footnote: true,
        ..MUYA
    };
    let mut doc = Incremental::new("[^a]: note text\n", options);
    let at = doc.source().find("note").expect("there") + 2;
    let report = doc.edit(at, 0, "x");
    assert!(matches!(report.path, ReparsePath::Region));
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), options).document)
    );

    // **The front-matter closing delimiter is not anchored to a line start.**
    // Typing `---` at the end of the last paragraph of a document that merely
    // *begins* with `---` turns the whole document into one front-matter block,
    // from a keystroke otherwise indistinguishable from typing a word.
    let source = "---\na\n\nbody\n\nmore\n";
    let mut doc = Incremental::new(source, MUYA);
    assert_eq!(
        doc.document()
            .block(doc.document().children(doc.document().root())[0])
            .map(Block::name),
        Some("thematic-break"),
        "no front matter yet — there is no closing delimiter"
    );

    let report = doc.edit(source.len() - 1, 0, "---");
    assert!(matches!(report.path, ReparsePath::Region));
    assert_eq!(report.region, 0..doc.source().len());
    assert_eq!(
        doc.document()
            .block(doc.document().children(doc.document().root())[0])
            .map(Block::name),
        Some("frontmatter"),
        "measured: one keystroke, and the document is one block"
    );
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), MUYA).document)
    );
}

// --- clause 2: a boundary edit equals a full reparse -----------------------

/// The shapes §4.1 names as boundary triggers, one at a time, each asserted
/// against the full reparse of the same string.
#[test]
fn a_boundary_trigger_edit_produces_the_full_reparses_tree() {
    let cases: &[(&str, usize, usize, &str)] = &[
        // A `#` typed at the start of a line — §4.1's own example.
        ("alpha\n\nbravo\n", 7, 0, "# "),
        // A blank line splits one paragraph into two.
        ("alpha bravo\n", 5, 1, "\n\n"),
        // A blank line removed joins two paragraphs into one.
        ("alpha\n\nbravo\n", 6, 1, ""),
        // A list marker appears.
        ("alpha\n\nbravo\n", 7, 0, "- "),
        // A fence opens and swallows what follows.
        ("alpha\n\nbravo\n", 0, 0, "```\n"),
        // A fence closes again.
        ("```\nalpha\n\nbravo\n", 0, 4, ""),
        // A table pipe.
        ("a b\n- | -\n", 1, 1, "|"),
        // A setext underline.
        ("alpha\n\nbravo\n", 6, 0, "===\n"),
        // A block quote marker.
        ("alpha\n\nbravo\n", 7, 0, "> "),
        // Two lists that merge when the blank line between them goes.
        ("- a\n\n- b\n", 4, 1, ""),
        // A list that splits when a blank line arrives.
        ("- a\n- b\n", 4, 0, "\n"),
        // Everything deleted.
        ("alpha\n\nbravo\n", 0, 13, ""),
        // An html block that grows a terminator.
        ("<pre>\nalpha\n\nbravo\n", 11, 0, "\n</pre>"),
    ];

    for (source, at, remove, insert) in cases {
        let mut doc = Incremental::new(source, SPEC);
        doc.edit(*at, *remove, insert);
        let expected = crate::parse(doc.source(), SPEC);
        assert_eq!(
            state(doc.document()),
            state(&expected.document),
            "{source:?} with {at}..{} → {insert:?}",
            at + remove
        );
        assert_eq!(
            ranges_in_tree_order(doc.document(), doc.source_map()),
            ranges_in_tree_order(&expected.document, &expected.source_map),
            "the ranges disagree for {source:?}"
        );
    }
}

fn ranges_in_tree_order(doc: &Document, map: &SourceMap) -> Vec<Range<usize>> {
    ids(doc)
        .into_iter()
        .map(|id| map.get(id).unwrap_or(usize::MAX..usize::MAX))
        .collect()
}

// --- clause 3: untouched blocks keep their ids -----------------------------

/// The claim §8's risk row is about, in the two shapes the gate names.
#[test]
fn the_node_ids_of_untouched_blocks_survive_both_paths() {
    // Step 1.
    let mut doc = Incremental::new("alpha\n\nbravo\n\ncharlie\n", SPEC);
    let before = ids(doc.document());
    let at = doc.source().find("bravo").expect("there") + 5;
    doc.edit(at, 0, "X");
    let after = ids(doc.document());
    assert_eq!(before, after, "a text edit must not renumber anything");

    // Step 2: a `#` at the start of the middle paragraph rewrites that block
    // and nothing else.
    let mut doc = Incremental::new("alpha\n\nbravo\n\ncharlie\n", SPEC);
    let before = ids(doc.document());
    let at = doc.source().find("bravo").expect("there");
    let report = doc.edit(at, 0, "# ");
    let after = ids(doc.document());

    assert!(matches!(report.path, ReparsePath::Region));
    assert_eq!(before.len(), after.len());
    assert_eq!(before[0], after[0], "the first paragraph is untouched");
    assert_eq!(before[2], after[2], "the last paragraph is untouched");
    assert_eq!(
        doc.document().block(after[1]).map(Block::name),
        Some("atx-heading")
    );
    assert_eq!(
        before[1], after[1],
        "a paragraph that became a heading keeps its id — same kind of change, \
         same node, which is what `mt-layout`'s cache wants"
    );
}

/// Deeper: an edit inside one list item must not renumber the item beside it,
/// because a list is one top-level block and the whole list is reparsed.
#[test]
fn a_region_reparse_keeps_the_ids_of_the_siblings_inside_it() {
    let mut doc = Incremental::new("- alpha\n- bravo\n- charlie\n", SPEC);
    let before = ids(doc.document());
    let at = doc.source().find("bravo").expect("there");

    let report = doc.edit(at, 0, "# ");

    assert!(matches!(report.path, ReparsePath::Region));
    let after = ids(doc.document());
    assert_eq!(before.len(), after.len());
    assert_eq!(before, after, "only the one leaf changed kind");
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), SPEC).document)
    );
}

// --- the region, and the guards that widen it ------------------------------

/// The region is the enclosing top-level blocks and not the whole document —
/// the claim that makes the counter mean anything on the region path too.
#[test]
fn the_region_is_the_enclosing_top_level_blocks() {
    let source = "alpha\n\nbravo\n\ncharlie\n\ndelta\n";
    let mut doc = Incremental::new(source, SPEC);
    let at = source.find("charlie").expect("there");

    let report = doc.edit(at, 0, "# ");

    assert_eq!(
        report.blocks_reparsed, 1,
        "one paragraph became one heading"
    );
    assert!(report.region.start > 0, "{:?}", report.region);
    assert!(
        report.region.end < doc.source().len(),
        "{:?}",
        report.region
    );
}

/// Guard 1. A reference definition anywhere at or before the region's end makes
/// `seen_labels` undecidable from the region, so the region becomes the
/// document — and the answer is still right, which is the point.
#[test]
fn a_reference_definition_widens_the_region_to_the_whole_document() {
    let source = "[a]: /one\n\nalpha\n\nbravo\n";
    let mut doc = Incremental::new(source, SPEC);
    let at = source.find("alpha").expect("there");

    let report = doc.edit(at, 0, "# ");

    assert_eq!(report.region, 0..doc.source().len());
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), SPEC).document)
    );
}

/// Guard 2. Footnote segmentation is a whole-document scan whose extents run
/// forward through blank lines, so the region is the document whenever the
/// option is on.
#[test]
fn footnotes_take_the_whole_document() {
    let options = Options {
        footnote: true,
        ..MUYA
    };
    let source = "alpha\n\n[^a]: note\n\nbravo\n";
    let mut doc = Incremental::new(source, options);
    let at = source.find("bravo").expect("there");

    let report = doc.edit(at, 0, "# ");

    assert_eq!(report.region, 0..doc.source().len());
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), options).document)
    );
}

/// Guard 3, and the reason it is not "ask `front_matter` about the region": the
/// trailing rule is `\n{2,}` **or** one-or-two newlines at end of input, so a
/// truncated string can grow front matter a full parse does not see.
#[test]
fn front_matter_is_decided_from_the_whole_source() {
    let source = "---\ntitle: x\n---\nnot front matter\n\nalpha\n";
    let mut doc = Incremental::new(source, MUYA);
    assert_ne!(
        doc.document()
            .block(doc.document().children(doc.document().root())[0])
            .map(Block::name),
        Some("frontmatter"),
        "the single newline after `---` is what stops it being front matter"
    );

    let at = source.find("alpha").expect("there");
    doc.edit(at, 0, "# ");
    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), MUYA).document)
    );
}

/// And when there *is* front matter, an edit inside the body still agrees.
#[test]
fn an_edit_after_real_front_matter_agrees_with_a_full_parse() {
    let source = "---\ntitle: x\n---\n\nalpha\n\nbravo\n";
    let mut doc = Incremental::new(source, MUYA);
    let at = source.find("bravo").expect("there");

    doc.edit(at, 0, "## ");

    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), MUYA).document)
    );
}

/// The widening loop's own reason to exist: a fence opened inside the region
/// runs past the region's end, and the first parse has to be thrown away.
#[test]
fn an_unterminated_fence_widens_the_region_and_the_tree_still_agrees() {
    let source = "alpha\n\nbravo\n\ncharlie\n";
    let mut doc = Incremental::new(source, SPEC);
    let at = source.find("bravo").expect("there");

    doc.edit(at, 0, "```\n");

    assert_eq!(
        state(doc.document()),
        state(&crate::parse(doc.source(), SPEC).document)
    );
}

/// The empty-document fallback, one edit later: `markdownToState`'s own
/// `states.length ? states : [{ name: 'paragraph', text: '' }]`.
#[test]
fn deleting_everything_leaves_the_placeholder_paragraph() {
    let mut doc = Incremental::new("alpha\n", SPEC);
    doc.edit(0, 6, "");

    assert_eq!(doc.source(), "");
    let root = doc.document().root();
    assert_eq!(doc.document().children(root).len(), 1);
    assert_eq!(
        state(doc.document()),
        state(&crate::parse("", SPEC).document)
    );
    assert_eq!(doc.source_map().len(), 1);
}

// --- the inverse batch -----------------------------------------------------

/// The reparse goes through `Document::apply` (D9) and therefore has an
/// inverse, and the inverse restores the tree — including the ids, because
/// `RemoveNode` detaches rather than frees.
#[test]
fn the_inverse_batch_undoes_the_reparse_including_the_node_ids() {
    let mut doc = Incremental::new("alpha\n\nbravo\n\ncharlie\n", SPEC);
    let before_ids = ids(doc.document());
    let before_state = state(doc.document());
    let at = doc.source().find("bravo").expect("there");

    let report = doc.edit(at, 0, "- ");
    assert_ne!(state(doc.document()), before_state);

    doc.document_mut().apply(&report.inverse);
    assert_eq!(state(doc.document()), before_state);
    assert_eq!(ids(doc.document()), before_ids);
}

// --- the pieces ------------------------------------------------------------

#[test]
fn a_minimal_splice_is_the_difference_and_nothing_around_it() {
    assert_eq!(minimal_splice("abc", "abc"), None);
    assert_eq!(minimal_splice("abc", "abXc"), Some((2, 0, "X".to_string())));
    assert_eq!(minimal_splice("abc", "ac"), Some((1, 1, String::new())));
    assert_eq!(minimal_splice("", "a"), Some((0, 0, "a".to_string())));
    assert_eq!(minimal_splice("a", ""), Some((0, 1, String::new())));
    // Multi-byte: the splice has to land on character boundaries or
    // `Edit::SpliceText` refuses it.
    let (at, remove, insert) = minimal_splice("héllo", "héllo!").expect("differs");
    assert!("héllo".is_char_boundary(at));
    assert_eq!((remove, insert.as_str()), (0, "!"));
    let (at, remove, _) = minimal_splice("aéb", "ab").expect("differs");
    assert!("aéb".is_char_boundary(at) && "aéb".is_char_boundary(at + remove));
}

#[test]
fn a_definition_shaped_line_is_recognised_wherever_it_is_indented_three_or_less() {
    assert!(defines_a_label("[a]: /x\n"));
    assert!(defines_a_label("text\n   [a]: /x\n"));
    assert!(
        !defines_a_label("text\n    [a]: /x\n"),
        "four spaces is code"
    );
    assert!(!defines_a_label("a [b]: c\n"), "not at a line start");
    assert!(!defines_a_label("[a] no colon\n"));
}

#[test]
fn mergeable_says_yes_only_where_a_blank_line_can_be_crossed() {
    let list = Block::BulletList {
        marker: mt_doc::BulletMarker::Dash,
        loose: false,
        children: Vec::new(),
    };
    let ordered = Block::OrderList {
        start: 1,
        delimiter: mt_doc::OrderDelim::Period,
        loose: false,
        children: Vec::new(),
    };
    let para = Block::Paragraph {
        text: mt_doc::Text::new(),
    };
    let indented = Block::CodeBlock {
        kind: mt_doc::CodeKind::Indented,
        info: String::new(),
        fence_len: None,
        text: mt_doc::Text::new(),
    };
    let fenced = Block::CodeBlock {
        kind: mt_doc::CodeKind::Fenced,
        info: String::new(),
        fence_len: Some(3),
        text: mt_doc::Text::new(),
    };

    assert!(
        mergeable(&list, &ordered, 0),
        "another list at column 0 is more of the same list"
    );
    assert!(
        mergeable(&list, &para, 2),
        "an indented paragraph after a blank line joins the last item"
    );
    assert!(
        !mergeable(&list, &para, 0),
        "a paragraph at column 0 is its own block in every parse — this is the \
         case that keeps a region from widening every time it ends beside a list"
    );
    assert!(mergeable(&indented, &para, 4));
    assert!(!mergeable(&indented, &para, 0));
    assert!(
        !mergeable(&fenced, &fenced, 4),
        "a fence has its own terminator"
    );
    assert!(!mergeable(&para, &para, 4), "a blank line ends a paragraph");
}

/// `prefix_chain` is §10's container half, used rather than argued: the chain
/// is rebuilt from the ancestors' **ranges** and reproduces the parse's own.
#[test]
fn the_prefix_chain_rebuilt_from_ranges_re_derives_the_leaf_text() {
    let source = "- > quoted\n  > lines\n";
    let parsed = crate::parse(source, SPEC);
    let doc = &parsed.document;

    let list = doc.children(doc.root())[0];
    let item = doc.children(list)[0];
    let quote = doc.children(item)[0];
    let paragraph = doc.children(quote)[0];

    let ancestors: Vec<(&Block, Range<usize>)> = [item, quote]
        .into_iter()
        .map(|id| {
            (
                doc.block(id).expect("a block"),
                parsed.source_map.get(id).expect("a range"),
            )
        })
        .collect();
    let prefixes = block::prefix_chain(source, &ancestors);
    let range = parsed.source_map.get(paragraph).expect("a range");

    assert_eq!(
        block::paragraph_text(source, &range, &prefixes),
        doc.block(paragraph)
            .and_then(Block::text)
            .expect("a leaf")
            .to_str()
    );
}
