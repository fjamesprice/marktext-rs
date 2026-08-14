//! Block-flow tests that need no font bytes.
//!
//! Registering a face needs the filesystem, and `cargo xtask deps` forbids
//! filesystem calls anywhere under this crate's `src/` — including inside a
//! `#[cfg(test)]` module, and including in a comment that merely spells the
//! path prefix out, which is why this sentence does not. That is
//! less limiting than it sounds: an empty [`Fonts`] still shapes, so every
//! block still reaches the *built* state and every margin, indent, inset and
//! marker below is exercised. What an empty collection cannot produce is a
//! glyph, so anything whose answer is a **text height** or a **break position**
//! lives in `crates/mt-layout/tests/layout.rs` instead.
//!
//! That split is load-bearing rather than incidental: with no faces, every
//! block's text measures zero high, which means the vertical numbers here are
//! *purely* the margin-collapsing arithmetic with nothing else mixed in.

use super::*;

use mt_doc::{
    Align, Block, BulletMarker, CodeKind, DiagramKind, DiagramLang, Edit, FrontmatterLang,
    FrontmatterStyle, MathStyle, OrderDelim, Text, Underline,
};

use crate::display::DisplayItem;
use crate::theme::{CheckboxShape, ListMarker};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A document builder. `Document::new` starts with one empty paragraph, which
/// every test here would otherwise have to remember.
struct Doc {
    doc: Document,
}

impl Doc {
    fn new() -> Doc {
        let mut doc = Document::new();
        let first = doc.children(doc.root())[0];
        doc.apply(&[Edit::RemoveNode { node: first }]);
        Doc { doc }
    }

    fn root(&self) -> NodeId {
        self.doc.root()
    }

    fn push(&mut self, parent: NodeId, block: Block) -> NodeId {
        let index = self.doc.children(parent).len();
        self.doc.apply(&[Edit::InsertNode {
            parent,
            index,
            block,
        }]);
        *self
            .doc
            .children(parent)
            .last()
            .expect("just inserted a child")
    }

    fn para(&mut self, parent: NodeId, text: &str) -> NodeId {
        self.push(
            parent,
            Block::Paragraph {
                text: Text::from(text),
            },
        )
    }
}

fn plan(doc: &Doc, theme: &Theme) -> LayoutTree {
    LayoutTree::plan(&doc.doc, theme, f32::INFINITY, &LayoutOptions::default())
}

/// Plan, build against an empty collection, place. Text measures zero high, so
/// every `y` below is margin arithmetic alone.
fn placed(doc: &Doc, theme: &Theme) -> LayoutTree {
    let mut fonts = Fonts::new();
    let mut shaper = TextShaper::new();
    let mut tree = plan(doc, theme);
    tree.build_all(&doc.doc, &mut fonts, &mut shaper);
    tree.place();
    tree
}

fn emitted(doc: &Doc, theme: &Theme) -> DisplayList {
    let fonts = Fonts::new();
    let tree = placed(doc, theme);
    tree.emit(&fonts).expect("nothing to resolve without faces")
}

fn find(list: &DisplayList, kind: BlockKind) -> &BlockDisplay {
    list.blocks
        .iter()
        .find(|b| b.kind == kind)
        .unwrap_or_else(|| panic!("no {} in the display list", kind.name()))
}

fn fills(block: &BlockDisplay) -> Vec<FilledRect> {
    block
        .items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Rect(r) => Some(*r),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The content column
// ---------------------------------------------------------------------------

/// §2 says the column is 800px and the padding is `0 50px 100px`; the CSS says
/// `box-sizing: border-box`, so those two numbers **subtract**. 700 and 650,
/// not 800 and 750.
#[test]
fn the_content_column_is_the_border_box_less_its_padding() {
    for (theme, expected) in [(Theme::muya_default(), 700.0), (Theme::dark(), 650.0)] {
        let doc = Doc::new();
        let tree = plan(&doc, &theme);
        assert_eq!(tree.content_width(), expected, "{}", theme.name);
        assert_eq!(
            tree.content_width(),
            theme.content_width_px(),
            "the tree and the theme must agree on the one number the gate turns on"
        );
    }
}

#[test]
fn a_narrow_viewport_shrinks_the_column_and_a_wide_one_does_not() {
    let theme = Theme::muya_default();
    let doc = Doc::new();
    let at =
        |w: f32| LayoutTree::plan(&doc.doc, &theme, w, &LayoutOptions::default()).content_width();
    assert_eq!(at(f32::INFINITY), 700.0, "max-width caps it");
    assert_eq!(at(4000.0), 700.0);
    assert_eq!(at(800.0), 700.0);
    assert_eq!(at(400.0), 300.0, "narrower than the maximum, so it shrinks");
    assert_eq!(at(80.0), 0.0, "the padding cannot make it negative");
    assert_eq!(at(-5.0), 0.0);
    assert_eq!(at(f32::NAN), 700.0, "a NaN width degrades to the theme's");
}

// ---------------------------------------------------------------------------
// Margin collapsing
// ---------------------------------------------------------------------------

fn tops(tree: &LayoutTree, kinds: &[BlockKind]) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, k) in kinds.iter().enumerate() {
        assert_eq!(tree.kind(i), *k, "block {i}");
        out.push(tree.blocks[i].bounds.y);
    }
    out
}

/// The trap. Two `0.5em` margins between two paragraphs collapse to one 8px
/// gap; summing them would double-space the whole corpus.
#[test]
fn two_paragraphs_are_separated_by_one_collapsed_margin_not_two() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "one");
    doc.para(root, "two");
    doc.para(root, "three");
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(
        tops(
            &tree,
            &[
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Paragraph
            ]
        ),
        vec![0.0, 8.0, 16.0],
        "8px apart, not 16"
    );
}

/// Collapsing takes the **maximum**. A heading's `1rem` beats a paragraph's
/// `0.5em` on both sides, so the gaps are 16 and 16 rather than 24 and 24.
#[test]
fn a_heading_between_two_paragraphs_takes_the_larger_margin() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "before");
    doc.push(
        root,
        Block::AtxHeading {
            level: 1,
            text: Text::from("H"),
        },
    );
    doc.para(root, "after");
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(
        tops(
            &tree,
            &[
                BlockKind::Paragraph,
                BlockKind::AtxHeading,
                BlockKind::Paragraph
            ]
        ),
        vec![0.0, 16.0, 32.0]
    );
}

/// Rule 2. A blockquote has `padding: 0 30px` — no *vertical* padding — so its
/// own `0.5em` and its first paragraph's `0.5em` collapse into one 8px gap
/// above the quote, and the quote's top edge lands on the paragraph's. The bar
/// is drawn from that edge, so getting this wrong is 8px of visible bar hanging
/// above the text.
#[test]
fn a_blockquote_collapses_through_to_its_first_child() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "before");
    let quote = doc.push(root, Block::BlockQuote { children: vec![] });
    doc.para(quote, "quoted");
    doc.para(root, "after");
    let tree = placed(&doc, &Theme::muya_default());
    let ys = tops(
        &tree,
        &[
            BlockKind::Paragraph,
            BlockKind::BlockQuote,
            BlockKind::Paragraph,
            BlockKind::Paragraph,
        ],
    );
    assert_eq!(ys[1], 8.0, "one collapsed 0.5em above the quote");
    assert_eq!(
        ys[2], ys[1],
        "and nothing between the quote's edge and its first paragraph"
    );
    assert_eq!(ys[3], 16.0, "one collapsed 0.5em below it too");
}

/// Rule 3. A code block's `padding: 1em` blocks the collapse, so its child
/// margins — here, the block's own top padding — stay inside. Its `1.5em`
/// top margin resolves against its **own** 14.4px, so the gap above it is
/// 21.6px and not 24.
#[test]
fn a_code_block_does_not_collapse_through_its_padding() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "before");
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: "rust".into(),
            fence_len: Some(3),
            text: Text::from("fn main() {}"),
        },
    );
    let tree = placed(&doc, &Theme::muya_default());
    let ys = tops(&tree, &[BlockKind::Paragraph, BlockKind::CodeBlock]);
    assert!(
        (ys[1] - 21.6).abs() < 1e-3,
        "1.5em of 14.4px beats the paragraph's 8px: got {}",
        ys[1]
    );
    // 1px border + 14.4px padding above the text.
    let (_, cy) = tree.content_origin(1);
    assert!((cy - (ys[1] + 15.4)).abs() < 1e-3, "border then padding");
}

/// Rule 4. Nothing above the first block.
#[test]
fn the_document_does_not_begin_with_a_margin() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::AtxHeading {
            level: 1,
            text: Text::from("Title"),
        },
    );
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(tree.blocks[0].bounds.y, 0.0);
}

/// …and nothing after the last one but the container's bottom padding.
#[test]
fn the_document_ends_at_its_last_block_plus_the_container_padding() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "only");
    let list = emitted(&doc, &Theme::muya_default());
    // Zero-height text, no leading margin, no trailing margin.
    assert_eq!(list.height, 100.0);
}

#[test]
fn an_empty_document_is_just_the_container_padding() {
    let doc = Doc::new();
    let list = emitted(&doc, &Theme::muya_default());
    assert!(list.blocks.is_empty());
    assert_eq!(list.height, 100.0);
    assert_eq!(list.width, 700.0);
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

#[test]
fn list_indent_is_thirty_pixels_per_level() {
    let mut doc = Doc::new();
    let root = doc.root();
    let outer = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: false,
            children: vec![],
        },
    );
    let item = doc.push(outer, Block::ListItem { children: vec![] });
    let inner = doc.push(
        item,
        Block::BulletList {
            marker: BulletMarker::Star,
            loose: false,
            children: vec![],
        },
    );
    let inner_item = doc.push(inner, Block::ListItem { children: vec![] });
    doc.para(inner_item, "deep");

    let tree = placed(&doc, &Theme::muya_default());
    // 0 list, 1 item, 2 list, 3 item, 4 paragraph.
    assert_eq!(tree.content_origin(1).0, 30.0);
    assert_eq!(tree.content_origin(3).0, 60.0);
    assert_eq!(tree.content_origin(4).0, 60.0);
    assert_eq!(tree.block_content_width(4), 640.0, "700 − 2 × 30");
}

/// `li > ol, li > ul { margin: 0 }` — a nested list carries no vertical margin,
/// so it starts flush with its parent item.
#[test]
fn a_nested_list_carries_no_vertical_margin() {
    let mut doc = Doc::new();
    let root = doc.root();
    let outer = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: false,
            children: vec![],
        },
    );
    let item = doc.push(outer, Block::ListItem { children: vec![] });
    doc.para(item, "text");
    let inner = doc.push(
        item,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: false,
            children: vec![],
        },
    );
    let inner_item = doc.push(inner, Block::ListItem { children: vec![] });
    doc.para(inner_item, "deep");
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(tree.blocks[0].edges.margin_top, 8.0, "the outer list");
    assert_eq!(tree.blocks[3].edges.margin_top, 0.0, "the nested one");
}

/// `ul ul { circle }`, `ul ul ul { square }`, and the three-deep selector still
/// matches at four, so the last entry repeats.
#[test]
fn bullets_cycle_disc_circle_square_with_depth() {
    let mut doc = Doc::new();
    let root = doc.root();
    let mut parent = root;
    let mut items = Vec::new();
    for _ in 0..4 {
        let list = doc.push(
            parent,
            Block::BulletList {
                marker: BulletMarker::Dash,
                loose: false,
                children: vec![],
            },
        );
        let item = doc.push(list, Block::ListItem { children: vec![] });
        items.push(item);
        parent = item;
    }
    let tree = placed(&doc, &Theme::muya_default());
    let markers: Vec<_> = tree
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::ListItem)
        .map(|b| b.marker.clone())
        .collect();
    assert_eq!(
        markers,
        vec![
            Some(Marker::Bullet(ListMarker::Disc)),
            Some(Marker::Bullet(ListMarker::Circle)),
            Some(Marker::Bullet(ListMarker::Square)),
            Some(Marker::Bullet(ListMarker::Square)),
        ]
    );
}

#[test]
fn ordered_numbering_honours_start_and_delimiter() {
    let mut doc = Doc::new();
    let root = doc.root();
    let list = doc.push(
        root,
        Block::OrderList {
            start: 7,
            delimiter: OrderDelim::Paren,
            loose: false,
            children: vec![],
        },
    );
    for _ in 0..3 {
        doc.push(list, Block::ListItem { children: vec![] });
    }
    let tree = placed(&doc, &Theme::muya_default());
    let texts: Vec<&str> = tree
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::ListItem)
        .map(|b| b.aux_text.as_str())
        .collect();
    assert_eq!(texts, vec!["7)", "8)", "9)"]);
}

#[test]
fn the_other_four_counter_styles_are_implemented_rather_than_left_to_panic() {
    assert_eq!(ordinal(OrderedMarker::Decimal, 3, OrderDelim::Period), "3.");
    assert_eq!(
        ordinal(OrderedMarker::LowerAlpha, 27, OrderDelim::Period),
        "aa.",
        "bijective base 26"
    );
    assert_eq!(
        ordinal(OrderedMarker::UpperAlpha, 26, OrderDelim::Period),
        "Z."
    );
    assert_eq!(
        ordinal(OrderedMarker::LowerRoman, 1994, OrderDelim::Period),
        "mcmxciv."
    );
    assert_eq!(
        ordinal(OrderedMarker::UpperRoman, 4, OrderDelim::Paren),
        "IV)"
    );
    // Outside the counter style's range CSS falls back to decimal.
    assert_eq!(
        ordinal(OrderedMarker::LowerRoman, 4000, OrderDelim::Period),
        "4000."
    );
    assert_eq!(
        ordinal(OrderedMarker::LowerAlpha, 0, OrderDelim::Period),
        "0."
    );
}

/// §2: 12×12 target inside an 18×18 ring at `inset-inline-start: -23px`, with
/// `top` centred on the first line box. The inset is relative to the **item's**
/// content edge, which is 30px in, so the ring's left edge lands at 30 − 23 − 2.
#[test]
fn the_task_checkbox_sits_twenty_three_pixels_left_of_its_item() {
    let mut doc = Doc::new();
    let root = doc.root();
    let list = doc.push(
        root,
        Block::TaskList {
            marker: BulletMarker::Dash,
            loose: false,
            children: vec![],
        },
    );
    let item = doc.push(
        list,
        Block::TaskListItem {
            checked: false,
            children: vec![],
        },
    );
    doc.para(item, "todo");
    let list = emitted(&doc, &Theme::muya_default());
    let item = find(&list, BlockKind::TaskListItem);
    let r = fills(item);
    assert_eq!(r.len(), 2, "an unchecked box is a ring");
    // Item content x = 30 (task_indent_px); box x = 30 − 23 = 7; ring = box − 2.
    // Item content y = 0; box y = 1.6 × 0.5 × 16 − 7 = 5.8; ring = box − 2.
    assert_eq!(r[0].rect.x, 5.0);
    assert!((r[0].rect.y - 3.8).abs() < 1e-4, "got {}", r[0].rect.y);
    assert_eq!((r[0].rect.width, r[0].rect.height), (18.0, 18.0));
    assert_eq!(r[0].corner_radius, 9.0);
}

#[test]
fn a_checked_task_item_draws_a_tick() {
    let mut doc = Doc::new();
    let root = doc.root();
    let list = doc.push(
        root,
        Block::TaskList {
            marker: BulletMarker::Dash,
            loose: false,
            children: vec![],
        },
    );
    doc.push(
        list,
        Block::TaskListItem {
            checked: true,
            children: vec![],
        },
    );
    let list = emitted(&doc, &Theme::muya_default());
    assert_eq!(fills(find(&list, BlockKind::TaskListItem)).len(), 4);
}

/// The theme's `CheckboxShape` is a discriminator rather than a number, so a
/// theme that squares the box must not silently keep muya's radius.
#[test]
fn the_theme_decides_the_checkbox_shape() {
    assert_eq!(Theme::muya_default().checkbox.shape, CheckboxShape::Circle);
}

// ---------------------------------------------------------------------------
// Blockquote, code block, thematic break
// ---------------------------------------------------------------------------

#[test]
fn a_nested_blockquote_loses_its_trailing_padding_and_keeps_its_leading_one() {
    let mut doc = Doc::new();
    let root = doc.root();
    let outer = doc.push(root, Block::BlockQuote { children: vec![] });
    let inner = doc.push(outer, Block::BlockQuote { children: vec![] });
    doc.para(inner, "deep");
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(tree.blocks[0].edges.padding_left, 30.0);
    assert_eq!(tree.blocks[0].edges.padding_right, 30.0);
    assert_eq!(tree.blocks[1].edges.padding_left, 30.0);
    assert_eq!(
        tree.blocks[1].edges.padding_right, 0.0,
        "a quote inside a quote drops its trailing padding"
    );
    assert_eq!(tree.block_content_width(2), 700.0 - 30.0 - 30.0 - 30.0);
}

#[test]
fn the_blockquote_bar_spans_the_quote_it_was_emitted_after() {
    let mut doc = Doc::new();
    let root = doc.root();
    let quote = doc.push(root, Block::BlockQuote { children: vec![] });
    doc.para(quote, "a");
    doc.para(quote, "b");
    let list = emitted(&doc, &Theme::muya_default());
    let quote = find(&list, BlockKind::BlockQuote);
    let bar = fills(quote)[0];
    assert_eq!(bar.rect.x, quote.bounds.x + 15.0);
    assert_eq!(bar.rect.width, 2.0);
    assert_eq!(bar.rect.height, quote.bounds.height);
    assert_eq!(bar.rect.height, 8.0, "one collapsed margin between the two");
}

/// Correction C1 in its layout-visible form: muya's code block has a 1px border
/// and `dark`'s has none, so the same fence is 2px narrower in content and
/// carries four fewer fills.
#[test]
fn dark_drops_the_code_block_border_and_muya_default_keeps_it() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from("x"),
        },
    );
    let muya = emitted(&doc, &Theme::muya_default());
    let dark = emitted(&doc, &Theme::dark());
    // background + 4 border fills, against background alone.
    assert_eq!(fills(find(&muya, BlockKind::CodeBlock)).len(), 5);
    assert_eq!(fills(find(&dark, BlockKind::CodeBlock)).len(), 1);
}

#[test]
fn the_thematic_break_is_a_rule_and_never_its_own_source_text() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::ThematicBreak {
            text: Text::from("---"),
        },
    );
    let list = emitted(&doc, &Theme::muya_default());
    let hr = find(&list, BlockKind::ThematicBreak);
    assert_eq!(hr.items.len(), 1);
    assert!(matches!(hr.items[0], DisplayItem::Line(_)));
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

fn table_doc() -> Doc {
    let mut doc = Doc::new();
    let root = doc.root();
    let table = doc.push(root, Block::Table { children: vec![] });
    for row_text in [["Head A", "Head B"], ["a", "b"]] {
        let row = doc.push(table, Block::TableRow { children: vec![] });
        for cell in row_text {
            doc.push(
                row,
                Block::TableCell {
                    align: Align::Center,
                    text: Text::from(cell),
                },
            );
        }
    }
    doc
}

#[test]
fn table_cells_pad_six_by_thirteen_and_the_header_is_bold() {
    let doc = table_doc();
    let tree = placed(&doc, &Theme::muya_default());
    let cells: Vec<usize> = (0..tree.len())
        .filter(|&i| tree.kind(i) == BlockKind::TableCell)
        .collect();
    assert_eq!(cells.len(), 4);
    for &c in &cells {
        let e = tree.blocks[c].edges;
        assert_eq!((e.padding_top, e.padding_left), (6.0, 13.0));
        assert_eq!((e.padding_bottom, e.padding_right), (6.0, 13.0));
        assert_eq!(
            tree.blocks[c].style.as_ref().unwrap().align,
            TextAlign::Center,
            "per-column alignment comes from Align"
        );
    }
    assert_eq!(tree.blocks[cells[0]].style.as_ref().unwrap().weight, 700);
    assert_eq!(tree.blocks[cells[2]].style.as_ref().unwrap().weight, 400);
}

/// C2: §2's block-spacing row lists `table` under the 0.5em rule and is wrong.
/// `figure:not(.mu-table)` excludes it, and the figure's own margin plus its
/// padding plus the inner table's un-reset margin come to 1.5em above and 1em
/// below.
#[test]
fn a_table_clears_one_and_a_half_em_above_and_one_below() {
    let doc = table_doc();
    let tree = placed(&doc, &Theme::muya_default());
    let e = tree.blocks[0].edges;
    assert_eq!(tree.kind(0), BlockKind::Table);
    assert_eq!(e.margin_top, 8.0, "the figure's own 0.5em");
    assert_eq!(e.margin_bottom, 0.0, "`margin: 0.5em 0 0` is asymmetric");
    assert_eq!(
        e.padding_top, 16.0,
        "0.5em of figure padding plus the inner table's 0.5em"
    );
    assert_eq!(e.padding_bottom, 16.0);
    // 8 above the figure + 16 inside it = 24 = 1.5em; 16 + 0 = 16 = 1em below.
}

/// Every cell's border is drawn on a box one pixel larger than the cell, which
/// is how `blockSyntax.css:621-632` emulates `border-collapse: collapse`.
#[test]
fn a_cell_border_overlaps_its_neighbour_rather_than_doubling() {
    let doc = table_doc();
    let list = emitted(&doc, &Theme::muya_default());
    let cells: Vec<&BlockDisplay> = list
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::TableCell)
        .collect();
    let first = fills(cells[0]);
    assert_eq!(first.len(), 4, "four fills make one 1px border");
    let right_edge = first[3].rect.x + first[3].rect.width;
    assert_eq!(
        right_edge,
        cells[0].bounds.x + cells[0].bounds.width + 1.0,
        "the overlay is `calc(100% + 1px)` wide"
    );
    assert_eq!(
        cells[1].bounds.x,
        cells[0].bounds.x + cells[0].bounds.width,
        "and the next cell starts where this one ended"
    );
}

/// The declaration §2 does not mention and D4 transcribed anyway. It has no
/// effect in the reference, so applying it here would make every table 160px a
/// column wider than MarkText draws it.
#[test]
fn the_ten_em_cell_floor_is_a_dead_declaration_in_the_reference() {
    let theme = Theme::muya_default();
    assert_eq!(theme.table.cell_min_width_em, 10.0);
    let doc = table_doc();
    let tree = placed(&doc, &theme);
    let widths: Vec<f32> = (0..tree.len())
        .filter(|&i| tree.kind(i) == BlockKind::TableCell)
        .map(|i| tree.blocks[i].bounds.width)
        .collect();
    // With no faces the text measures zero wide, so a column is exactly its
    // padding. A 10em floor would have made each 160 + 26 = 186.
    assert_eq!(widths, vec![26.0, 26.0, 26.0, 26.0]);
}

// ---------------------------------------------------------------------------
// D11
// ---------------------------------------------------------------------------

#[test]
fn every_code_box_kind_names_its_language_and_nothing_else_does() {
    let cases: Vec<(Block, Option<&str>)> = vec![
        (
            Block::CodeBlock {
                kind: CodeKind::Fenced,
                info: "rust ignore".into(),
                fence_len: Some(3),
                text: Text::new(),
            },
            Some("rust"),
        ),
        (
            Block::CodeBlock {
                kind: CodeKind::Fenced,
                info: String::new(),
                fence_len: Some(3),
                text: Text::new(),
            },
            None,
        ),
        (
            Block::MathBlock {
                style: MathStyle::Default,
                text: Text::new(),
            },
            Some("latex"),
        ),
        (
            Block::Diagram {
                lang: DiagramLang::Json,
                kind: DiagramKind::VegaLite,
                text: Text::new(),
            },
            Some("vega-lite"),
        ),
        (
            Block::Frontmatter {
                lang: FrontmatterLang::Toml,
                style: FrontmatterStyle::Plus,
                text: Text::new(),
            },
            Some("toml"),
        ),
        (Block::HtmlBlock { text: Text::new() }, Some("html")),
        (Block::Paragraph { text: Text::new() }, None),
    ];
    for (block, expected) in cases {
        assert_eq!(code_language(&block), expected, "{}", block.name());
    }
}

/// D11's five kinds share the code-block box, and share it *at the numbers*:
/// same margins, same padding, same border, same font stack.
#[test]
fn math_html_frontmatter_and_diagrams_lay_out_in_the_code_block_box() {
    let mut doc = Doc::new();
    let root = doc.root();
    let source = "a\nb";
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: "rust".into(),
            fence_len: Some(3),
            text: Text::from(source),
        },
    );
    doc.push(
        root,
        Block::MathBlock {
            style: MathStyle::Default,
            text: Text::from(source),
        },
    );
    doc.push(
        root,
        Block::Diagram {
            lang: DiagramLang::Yaml,
            kind: DiagramKind::Mermaid,
            text: Text::from(source),
        },
    );
    doc.push(
        root,
        Block::Frontmatter {
            lang: FrontmatterLang::Yaml,
            style: FrontmatterStyle::Dash,
            text: Text::from(source),
        },
    );
    doc.push(
        root,
        Block::HtmlBlock {
            text: Text::from(source),
        },
    );
    let tree = placed(&doc, &Theme::muya_default());
    let reference = tree.blocks[0].edges;
    for i in 1..5 {
        assert_eq!(tree.blocks[i].edges, reference, "block {i}");
        assert_eq!(
            tree.blocks[i].style.as_ref().unwrap().stack,
            FontStack::Code
        );
    }
    let list = emitted(&doc, &Theme::muya_default());
    assert_eq!(
        list.blocks
            .iter()
            .map(|b| b.language.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("rust"),
            Some("latex"),
            Some("mermaid"),
            Some("yaml"),
            Some("html")
        ]
    );
}

// ---------------------------------------------------------------------------
// D9 — unbuilt is a state
// ---------------------------------------------------------------------------

fn ten_paragraphs() -> Doc {
    let mut doc = Doc::new();
    let root = doc.root();
    for i in 0..10 {
        doc.para(root, &format!("paragraph {i}"));
    }
    doc
}

/// Planning resolves every unit and every horizontal position and shapes
/// nothing. That is what makes the plan cheap enough for S6 to run on a whole
/// 5 MB file before deciding what is visible.
#[test]
fn planning_builds_nothing() {
    let doc = ten_paragraphs();
    let tree = plan(&doc, &Theme::muya_default());
    assert_eq!(tree.len(), 10);
    assert_eq!(tree.built_count(), 0);
    for i in 0..tree.len() {
        assert!(!tree.is_built(i));
        assert_eq!(
            tree.block_content_width(i),
            700.0,
            "resolved without a font"
        );
    }
}

/// D9's hardest requirement, as an assertion: building one block must not build
/// any other. If this ever fails, S6's viewport driver cannot exist.
#[test]
fn building_one_block_builds_no_other() {
    let doc = ten_paragraphs();
    let mut fonts = Fonts::new();
    let mut shaper = TextShaper::new();
    let mut tree = plan(&doc, &Theme::muya_default());

    tree.build(4, &doc.doc, &mut fonts, &mut shaper);
    assert!(tree.is_built(4));
    assert_eq!(tree.built_count(), 1);
    for i in 0..tree.len() {
        assert_eq!(tree.is_built(i), i == 4, "block {i}");
    }

    // Idempotent: building it again is a no-op, not a second `Layout`.
    tree.build(4, &doc.doc, &mut fonts, &mut shaper);
    assert_eq!(tree.built_count(), 1);

    tree.build_all(&doc.doc, &mut fonts, &mut shaper);
    assert_eq!(tree.built_count(), 10);
}

/// A container has no text, so there is nothing to defer and a lazy driver
/// gains nothing by visiting it. Reported built from the start, which keeps
/// `built_count` meaningful as "how much shaping has happened".
#[test]
fn containers_have_nothing_to_build() {
    let mut doc = Doc::new();
    let root = doc.root();
    let quote = doc.push(root, Block::BlockQuote { children: vec![] });
    doc.para(quote, "inside");
    let tree = plan(&doc, &Theme::muya_default());
    assert!(tree.is_built(0), "the blockquote");
    assert!(!tree.is_built(1), "the paragraph");
    assert_eq!(tree.built_count(), 1);
}

/// Placing with nothing built must not panic, and must produce the margin
/// arithmetic with zero-height content — which is what the estimator S6 adds
/// will replace.
#[test]
fn placing_an_unbuilt_tree_uses_zero_heights_and_does_not_panic() {
    let doc = ten_paragraphs();
    let mut tree = plan(&doc, &Theme::muya_default());
    tree.place();
    assert_eq!(tree.blocks[9].bounds.y, 8.0 * 9.0);
}

// ---------------------------------------------------------------------------
// Coverage of the whole enum, and the odd corners
// ---------------------------------------------------------------------------

/// Every one of `mt_doc::Block`'s nineteen variants reaches the display list.
/// Constructed rather than parsed so that a variant added later fails to
/// compile here.
#[test]
fn every_block_kind_lays_out() {
    let mut doc = Doc::new();
    let root = doc.root();
    let t = || Text::from("content");

    doc.push(root, Block::Paragraph { text: t() });
    doc.push(
        root,
        Block::AtxHeading {
            level: 3,
            text: t(),
        },
    );
    doc.push(
        root,
        Block::SetextHeading {
            level: 2,
            underline: Underline::Dashes(3),
            text: t(),
        },
    );
    doc.push(root, Block::ThematicBreak { text: t() });
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Indented,
            info: String::new(),
            fence_len: None,
            text: t(),
        },
    );
    doc.push(root, Block::HtmlBlock { text: t() });
    doc.push(
        root,
        Block::MathBlock {
            style: MathStyle::Gitlab,
            text: t(),
        },
    );
    doc.push(
        root,
        Block::Frontmatter {
            lang: FrontmatterLang::Json,
            style: FrontmatterStyle::Brace,
            text: t(),
        },
    );
    doc.push(
        root,
        Block::Diagram {
            lang: DiagramLang::Yaml,
            kind: DiagramKind::Flowchart,
            text: t(),
        },
    );
    let quote = doc.push(root, Block::BlockQuote { children: vec![] });
    doc.para(quote, "quoted");
    let bullets = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Plus,
            loose: true,
            children: vec![],
        },
    );
    let bullet_item = doc.push(bullets, Block::ListItem { children: vec![] });
    doc.para(bullet_item, "bullet");
    let ordered = doc.push(
        root,
        Block::OrderList {
            start: 1,
            delimiter: OrderDelim::Period,
            loose: false,
            children: vec![],
        },
    );
    let ordered_item = doc.push(ordered, Block::ListItem { children: vec![] });
    doc.para(ordered_item, "ordered");
    let tasks = doc.push(
        root,
        Block::TaskList {
            marker: BulletMarker::Star,
            loose: false,
            children: vec![],
        },
    );
    let task_item = doc.push(
        tasks,
        Block::TaskListItem {
            checked: true,
            children: vec![],
        },
    );
    doc.para(task_item, "task");
    let table = doc.push(root, Block::Table { children: vec![] });
    let row = doc.push(table, Block::TableRow { children: vec![] });
    doc.push(
        row,
        Block::TableCell {
            align: Align::Right,
            text: t(),
        },
    );
    let footnote = doc.push(
        root,
        Block::Footnote {
            identifier: "note".into(),
            children: vec![],
        },
    );
    doc.para(footnote, "footnote body");

    for theme in [Theme::muya_default(), Theme::dark()] {
        let list = emitted(&doc, &theme);
        let seen: std::collections::BTreeSet<&str> =
            list.blocks.iter().map(|b| b.kind.name()).collect();
        let all: std::collections::BTreeSet<&str> =
            BlockKind::ALL.iter().map(|k| k.name()).collect();
        assert_eq!(seen, all, "{} left a block kind unplaced", theme.name);
        for b in &list.blocks {
            assert!(
                b.bounds.width >= 0.0 && b.bounds.height >= 0.0,
                "{:?}",
                b.kind
            );
        }
        // Blocks come out in document order, and every y is monotonic within a
        // sibling chain — the property D9's viewport slice depends on.
        assert!(list.height > 0.0);
    }
}

/// A footnote's `font-size: 0.8em` compounds into everything inside it, and its
/// padding resolves against the shrunken size rather than the editor's.
#[test]
fn a_footnote_shrinks_its_children() {
    let mut doc = Doc::new();
    let root = doc.root();
    let footnote = doc.push(
        root,
        Block::Footnote {
            identifier: "a".into(),
            children: vec![],
        },
    );
    doc.para(footnote, "body");
    let tree = placed(&doc, &Theme::muya_default());
    assert!((tree.blocks[0].units.own_px() - 12.8).abs() < 1e-4);
    assert!((tree.blocks[0].edges.padding_top - 15.36).abs() < 1e-3);
    assert!((tree.blocks[0].edges.margin_top - 17.92).abs() < 1e-3);
    assert!((tree.blocks[1].units.own_px() - 12.8).abs() < 1e-4);
    assert!(
        (tree.blocks[1].edges.margin_top - 6.4).abs() < 1e-4,
        "the paragraph's 0.5em is 0.5 of 12.8"
    );
}

/// The heading scale is muya's, not the UA's — h1 is 1.875em where a browser
/// would say 2em, and h3 is 1.375em where a browser would say 1.17em.
#[test]
fn headings_take_muyas_scale_and_a_rem_margin() {
    let mut doc = Doc::new();
    let root = doc.root();
    for level in 1..=6u8 {
        doc.push(
            root,
            Block::AtxHeading {
                level,
                text: Text::from("H"),
            },
        );
    }
    let tree = placed(&doc, &Theme::muya_default());
    let sizes: Vec<f32> = (0..6)
        .map(|i| tree.blocks[i].style.as_ref().unwrap().font_size)
        .collect();
    assert_eq!(sizes, vec![30.0, 24.0, 22.0, 20.0, 18.0, 16.0]);
    for i in 0..6 {
        assert_eq!(
            tree.blocks[i].edges.margin_top, 16.0,
            "1rem is the root's 16px at every level"
        );
        assert_eq!(tree.blocks[i].style.as_ref().unwrap().weight, 700);
        assert_eq!(tree.blocks[i].style.as_ref().unwrap().line_height, 1.4);
    }
}

/// A heading level outside 1..=6 cannot come from `mt_md::parse`, but S7 fuzzes
/// `Document` directly and an index panic there would be a layout crash.
#[test]
fn an_out_of_range_heading_level_clamps_rather_than_panicking() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::AtxHeading {
            level: 0,
            text: Text::from("zero"),
        },
    );
    doc.push(
        root,
        Block::AtxHeading {
            level: 99,
            text: Text::from("many"),
        },
    );
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(tree.blocks[0].style.as_ref().unwrap().font_size, 30.0);
    assert_eq!(tree.blocks[1].style.as_ref().unwrap().font_size, 16.0);
}

// ---------------------------------------------------------------------------
// The line-number gutter
// ---------------------------------------------------------------------------

#[test]
fn source_lines_counts_a_trailing_newline_as_the_end_of_the_last_line() {
    assert_eq!(source_lines(""), vec![0..0]);
    assert_eq!(source_lines("a"), vec![0..1]);
    assert_eq!(source_lines("a\nb"), vec![0..2, 2..3]);
    assert_eq!(source_lines("a\nb\n"), vec![0..2, 2..4]);
}

/// The gutter is muya's `codeBlockLineNumbers`, whose default is `false`
/// (`config/index.ts:323`). Off, a code block's left padding is `1em`; on, it
/// is `2.5em` and every glyph in the fence moves.
#[test]
fn the_line_number_gutter_is_off_by_default_and_widens_the_padding_when_on() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from("one\ntwo\nthree"),
        },
    );
    let theme = Theme::muya_default();
    let off = plan(&doc, &theme);
    assert!((off.blocks[0].edges.padding_left - 14.4).abs() < 1e-3);
    assert!(!off.blocks[0].line_numbers);

    let on = LayoutTree::plan(
        &doc.doc,
        &theme,
        f32::INFINITY,
        &LayoutOptions {
            code_block_line_numbers: true,
            ..LayoutOptions::default()
        },
    );
    assert!(
        (on.blocks[0].edges.padding_left - 36.0).abs() < 1e-3,
        "2.5em"
    );
    assert!(on.blocks[0].line_numbers);
}

/// muya only ever puts the gutter inside `.mu-code-block`, so D11's other four
/// kinds keep their `1em` padding even with the option on.
#[test]
fn the_gutter_never_reaches_math_diagrams_html_or_frontmatter() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::MathBlock {
            style: MathStyle::Default,
            text: Text::from("a\nb"),
        },
    );
    let tree = LayoutTree::plan(
        &doc.doc,
        &Theme::muya_default(),
        f32::INFINITY,
        &LayoutOptions {
            code_block_line_numbers: true,
            ..LayoutOptions::default()
        },
    );
    assert!(!tree.blocks[0].line_numbers);
    assert!((tree.blocks[0].edges.padding_left - 14.4).abs() < 1e-3);
}

// ---------------------------------------------------------------------------
// Post-review fixes
// ---------------------------------------------------------------------------

fn list_of(doc: &mut Doc, parent: NodeId, loose: bool, items: usize) -> NodeId {
    let list = doc.push(
        parent,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose,
            children: vec![],
        },
    );
    for i in 0..items {
        let item = doc.push(list, Block::ListItem { children: vec![] });
        doc.para(item, &format!("item {i}"));
    }
    list
}

/// **Finding 1.** `Block::{BulletList,OrderList,TaskList}::loose` is a layout
/// input, not just round-trip metadata: muya pushes `mu-tight-list` when it is
/// false (`bulletList/index.ts:46-47`) and
/// `.mu-tight-list > li > p { margin: 0 }` (`blockSyntax.css:400-404`) beats
/// `.mu-container p` on specificity. A three-item tight list is 16px shorter
/// than the loose one beside it, and `1mb.md` holds roughly 372 of them.
#[test]
fn a_tight_list_drops_the_gaps_between_its_items_and_a_loose_one_keeps_them() {
    let theme = Theme::muya_default();

    let mut tight_doc = Doc::new();
    let root = tight_doc.root();
    list_of(&mut tight_doc, root, false, 3);
    let tight = placed(&tight_doc, &theme);

    let mut loose_doc = Doc::new();
    let root = loose_doc.root();
    list_of(&mut loose_doc, root, true, 3);
    let loose = placed(&loose_doc, &theme);

    // 0 list, then item/paragraph pairs at 1/2, 3/4, 5/6.
    let paras = [2usize, 4, 6];
    let tight_tops: Vec<f32> = paras.iter().map(|&i| tight.blocks[i].bounds.y).collect();
    let loose_tops: Vec<f32> = paras.iter().map(|&i| loose.blocks[i].bounds.y).collect();
    assert_eq!(
        tight_tops,
        vec![0.0, 0.0, 0.0],
        "no text height, no margins"
    );
    assert_eq!(loose_tops, vec![0.0, 8.0, 16.0], "one 0.5em per gap");

    for &i in &paras {
        assert_eq!(tight.blocks[i].edges.margin_top, 0.0);
        assert_eq!(tight.blocks[i].edges.margin_bottom, 0.0);
        assert_eq!(loose.blocks[i].edges.margin_top, 8.0);
    }
    // The list's own outer margin is untouched by tightness.
    assert_eq!(tight.blocks[0].edges.margin_top, 8.0);
    assert_eq!(loose.blocks[0].edges.margin_top, 8.0);
}

/// Tightness reaches exactly `.mu-tight-list > li > p` and no further: a
/// paragraph inside a blockquote inside a tight list item is not a direct child
/// of the item and keeps its margin.
#[test]
fn tightness_does_not_reach_past_the_items_direct_children() {
    let mut doc = Doc::new();
    let root = doc.root();
    let list = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: false,
            children: vec![],
        },
    );
    let item = doc.push(list, Block::ListItem { children: vec![] });
    doc.para(item, "direct");
    let quote = doc.push(item, Block::BlockQuote { children: vec![] });
    doc.para(quote, "indirect");
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(tree.kind(2), BlockKind::Paragraph);
    assert_eq!(tree.blocks[2].edges.margin_top, 0.0, "the direct child");
    assert_eq!(tree.kind(4), BlockKind::Paragraph);
    assert_eq!(tree.blocks[4].edges.margin_top, 8.0, "inside the quote");
}

/// **Finding 5.** `li > ol.mu-order-list, li > ul.mu-bullet-list { margin: 0 }`
/// (`blockSyntax.css:421-425`) does not name `ul.mu-task-list`, and
/// `gfm/taskList/index.ts:42` gives it only `MU_TASK_LIST`. So a task list
/// nested in a list item keeps the shared `margin: 0.5em 0` where a bullet list
/// in the same position loses it.
#[test]
fn a_nested_task_list_keeps_the_margin_a_nested_bullet_list_loses() {
    let mut doc = Doc::new();
    let root = doc.root();
    let outer = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    let item = doc.push(outer, Block::ListItem { children: vec![] });
    doc.push(
        item,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    doc.push(
        item,
        Block::TaskList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    let tree = placed(&doc, &Theme::muya_default());
    assert_eq!(tree.kind(2), BlockKind::BulletList);
    assert_eq!(tree.blocks[2].edges.margin_top, 0.0);
    assert_eq!(tree.kind(3), BlockKind::TaskList);
    assert_eq!(
        tree.blocks[3].edges.margin_top, 8.0,
        "no CSS selector zeroes a nested task list's margin"
    );
}

/// **Finding 6.** Every selector in `blockSyntax.css:445-456` requires a
/// `ul.mu-bullet-list` or `ol.mu-order-list` ancestor, and a task list carries
/// neither class. A bullet list inside a task-list item is therefore still at
/// level one — `disc`, not `circle`.
#[test]
fn a_task_list_is_transparent_to_the_bullet_cascade() {
    let mut doc = Doc::new();
    let root = doc.root();
    let tasks = doc.push(
        root,
        Block::TaskList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    let task_item = doc.push(
        tasks,
        Block::TaskListItem {
            checked: false,
            children: vec![],
        },
    );
    let bullets = doc.push(
        task_item,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    doc.push(bullets, Block::ListItem { children: vec![] });
    let tree = placed(&doc, &Theme::muya_default());
    let item = tree
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::ListItem)
        .expect("a bullet item");
    assert_eq!(item.marker, Some(Marker::Bullet(ListMarker::Disc)));
}

/// …but a task list in the middle of a chain does not *reset* the cascade
/// either: `ul.mu-bullet-list ul.mu-bullet-list` is a descendant selector, so
/// the inner bullet list is still `circle`.
#[test]
fn a_task_list_in_the_middle_of_a_chain_does_not_reset_it() {
    let mut doc = Doc::new();
    let root = doc.root();
    let outer = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    let outer_item = doc.push(outer, Block::ListItem { children: vec![] });
    let tasks = doc.push(
        outer_item,
        Block::TaskList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    let task_item = doc.push(
        tasks,
        Block::TaskListItem {
            checked: false,
            children: vec![],
        },
    );
    let inner = doc.push(
        task_item,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    doc.push(inner, Block::ListItem { children: vec![] });
    let tree = placed(&doc, &Theme::muya_default());
    let markers: Vec<_> = tree
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::ListItem)
        .map(|b| b.marker.clone())
        .collect();
    assert_eq!(
        markers,
        vec![
            Some(Marker::Bullet(ListMarker::Disc)),
            Some(Marker::Bullet(ListMarker::Circle)),
        ]
    );
}

/// **Finding 7, the opacity half.** `opacity: 0.8` on `figure.mu-footnote`
/// dims the tint and every glyph under it. There is no layer primitive, so it
/// is applied per brush — see `dim`.
#[test]
fn a_footnote_dims_its_tint_and_everything_inside_it() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "outside");
    let footnote = doc.push(
        root,
        Block::Footnote {
            identifier: "a".into(),
            children: vec![],
        },
    );
    doc.para(footnote, "inside");
    let tree = placed(&doc, &theme);
    assert_eq!(tree.blocks[0].opacity, 1.0, "a paragraph outside");
    assert_eq!(tree.blocks[1].opacity, 0.8, "the figure");
    assert_eq!(tree.blocks[2].opacity, 0.8, "and its child");

    let outside = tree.blocks[0].style.as_ref().unwrap().brush;
    let inside = tree.blocks[2].style.as_ref().unwrap().brush;
    assert_eq!(outside.a, 255);
    assert_eq!(inside.a, 204, "255 x 0.8");
    assert_eq!(
        (inside.r, inside.g, inside.b),
        (outside.r, outside.g, outside.b)
    );

    let list = emitted(&doc, &theme);
    let tint = fills(find(&list, BlockKind::Footnote))[0];
    assert_eq!(tint.brush.a, 204);
}

/// **Finding 7, the label half.** `[^` + identifier + `]:` at an absolute 14px
/// in a monospace generic, weight 600, positioned against the figure's padding
/// box. The theme now carries all four, so nothing is invented at the call
/// site — which is why Correction 5 refused to draw it before.
#[test]
fn the_footnote_label_is_planned_from_theme_fields_alone() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    let footnote = doc.push(
        root,
        Block::Footnote {
            identifier: "note-1".into(),
            children: vec![],
        },
    );
    doc.para(footnote, "body");
    let tree = placed(&doc, &theme);
    let f = &tree.blocks[0];
    assert_eq!(f.aux_text, "[^note-1]:");
    let style = f.aux_style.as_ref().expect("a label style");
    assert_eq!(style.font_size, 14.0, "absolute px, not the footnote's em");
    assert_eq!(style.weight, 600);
    assert_eq!(style.stack, FontStack::FootnoteLabel);
    assert_eq!(style.wrap, Wrap::Never);
    assert_eq!(style.brush.a, 204, "dimmed by the figure's opacity");
    // `padding: 0 1em` and `top: 0.2em`, both of the label's own 14px.
    assert_eq!(f.label_offset.0, 14.0);
    assert!((f.label_offset.1 - 2.8).abs() < 1e-4);
    assert_eq!(theme.footnote.label_fonts, vec!["monospace".to_string()]);
}

/// **Finding 3.** `wrapCodeBlocks: false` is muya's default
/// (`config/index.ts:324`), and the `white-space: pre-wrap` the old comment
/// cited is inside `.mu-code-wrap`, not the base rule.
#[test]
fn code_blocks_do_not_wrap_by_default_and_do_when_the_option_is_on() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from("x"),
        },
    );
    let theme = Theme::muya_default();
    let off = plan(&doc, &theme);
    assert_eq!(off.blocks[0].style.as_ref().unwrap().wrap, Wrap::Never);
    let on = LayoutTree::plan(
        &doc.doc,
        &theme,
        f32::INFINITY,
        &LayoutOptions {
            wrap_code_blocks: true,
            ..LayoutOptions::default()
        },
    );
    assert_eq!(
        on.blocks[0].style.as_ref().unwrap().wrap,
        Wrap::AtContentWidth
    );
    assert!(!LayoutOptions::default().wrap_code_blocks, "muya's default");
}
