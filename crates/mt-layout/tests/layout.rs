//! Block flow against the real font set — M3 S1's gate, the half that needs
//! glyphs.
//!
//! The rest of the block-flow suite is in `src/flow/tests.rs`, where the
//! collection is empty and every vertical number is margin arithmetic alone.
//! What needs bytes lives here: a line box's **height**, a column's
//! **max-content width**, a **break position**, and therefore every assertion
//! that a block is the right size rather than merely in the right place.
//!
//! `cargo xtask deps` forbids `std::fs::` under `crates/mt-layout/src/` and its
//! own message sanctions this directory as the way out.

use std::path::{Path, PathBuf};

use mt_doc::{Align, Block, BulletMarker, CodeKind, Document, Edit, NodeId, OrderDelim, Text};
use mt_layout::display::{BlockKind, DisplayItem, DisplayList};
use mt_layout::fonts::{Face, FaceList, Fonts};
use mt_layout::text::TextShaper;
use mt_layout::theme::Theme;
use mt_layout::{LayoutOptions, LayoutTree, layout};

// ---------------------------------------------------------------------------
// The committed face set
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<crate> is two levels below the repo root")
        .to_path_buf()
}

fn read_face(face: &Face) -> Vec<u8> {
    let path = repo_root().join("assets/fonts").join(&face.file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The committed twelve, registered and wired exactly as a shell must do it —
/// the same three lines `tests/fonts.rs` documents as the shell's whole loop.
fn bundled_fonts() -> Fonts {
    let list = FaceList::bundled();
    let mut fonts = Fonts::new();
    for face in &list.faces {
        fonts
            .register_face(face, read_face(face))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    fonts.wire(&list).expect("wiring the committed set");
    fonts
}

// ---------------------------------------------------------------------------
// Document fixtures
// ---------------------------------------------------------------------------

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
        *self.doc.children(parent).last().expect("just inserted")
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

fn lay_out(doc: &Doc, theme: &Theme) -> DisplayList {
    let mut fonts = bundled_fonts();
    layout(&doc.doc, theme, f32::INFINITY, &mut fonts).expect("the committed faces resolve")
}

/// Plan, build, place — and **return the collection that built it**.
///
/// `Blob::id()` is a process-unique counter, so two `bundled_fonts()` calls
/// register the same twelve files under twelve different ids. A tree built
/// against one and emitted against the other resolves nothing and
/// `emit` returns `FontError::UnresolvedFont` — which is the error path being
/// unreachable through `layout()` and very reachable through a test helper that
/// drops its fonts on the floor. Both drafts of
/// `a_long_fence_line_overflows_its_box_instead_of_wrapping` hit it.
fn built(doc: &Doc, theme: &Theme, width: f32, options: &LayoutOptions) -> (LayoutTree, Fonts) {
    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    let mut tree = LayoutTree::plan(&doc.doc, theme, width, options);
    tree.build_all(&doc.doc, &mut fonts, &mut shaper);
    tree.place();
    (tree, fonts)
}

/// [`built`], for the callers that only measure and never emit.
fn built_tree(doc: &Doc, theme: &Theme, width: f32, options: &LayoutOptions) -> LayoutTree {
    built(doc, theme, width, options).0
}

fn glyph_runs(list: &DisplayList, kind: BlockKind) -> usize {
    list.blocks
        .iter()
        .filter(|b| b.kind == kind)
        .flat_map(|b| b.items.iter())
        .filter(|i| matches!(i, DisplayItem::Glyphs(_)))
        .count()
}

// ---------------------------------------------------------------------------
// Line boxes
// ---------------------------------------------------------------------------

/// The base case every vertical number in the engine rests on: an unwrapped
/// paragraph is exactly `line-height × font-size` tall, because parley's
/// `LineHeight::FontSizeRelative` distributes the leading around the font's
/// ascent and descent and the two halves sum back to the requested height
/// (`parley/src/layout/line_break.rs:123-140`). 1.6 × 16 = 25.6.
#[test]
fn a_one_line_paragraph_is_exactly_the_themes_line_height() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "one short line");
    let tree = built_tree(
        &doc,
        &Theme::muya_default(),
        f32::INFINITY,
        &LayoutOptions::default(),
    );
    let (_, height) = tree.shaped_size(0).expect("a paragraph has text");
    assert!((height - 25.6).abs() < 1e-3, "got {height}");
    let list = lay_out(&doc, &Theme::muya_default());
    assert!((list.blocks[0].bounds.height - 25.6).abs() < 1e-3);
    // …and the document is that plus the container's bottom padding, with no
    // leading or trailing margin.
    assert!((list.height - 125.6).abs() < 1e-3);
}

/// A heading's line box is its own `1.4`, not the editor's `1.6`, and it is
/// `1.4 × 30 = 42` for an h1 rather than `1.4 × 16`.
#[test]
fn a_heading_line_box_uses_the_heading_line_height_and_the_heading_size() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::AtxHeading {
            level: 1,
            text: Text::from("Title"),
        },
    );
    let list = lay_out(&doc, &Theme::muya_default());
    assert!(
        (list.blocks[0].bounds.height - 42.0).abs() < 1e-3,
        "got {}",
        list.blocks[0].bounds.height
    );
}

/// A code block's box is its text plus `1em` of padding on each side plus a 1px
/// border, and every one of those `em`s is `14.4` — the block's own 90% size —
/// rather than the 16 it inherited. Three lines at `1.6 × 14.4` is 69.12.
#[test]
fn a_code_block_measures_in_its_own_shrunken_em() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: "rust".into(),
            fence_len: Some(3),
            text: Text::from("let a = 1;\nlet b = 2;\nlet c = 3;"),
        },
    );
    let list = lay_out(&doc, &Theme::muya_default());
    let block = &list.blocks[0];
    let expected = 3.0 * 1.6 * 14.4 + 2.0 * 14.4 + 2.0;
    assert!(
        (block.bounds.height - expected).abs() < 1e-2,
        "got {} want {expected}",
        block.bounds.height
    );
    assert_eq!(block.language.as_deref(), Some("rust"));
}

// ---------------------------------------------------------------------------
// D9 — one `Layout` per fence
// ---------------------------------------------------------------------------

/// D9 measured one `Layout` per code line at 1.38× slower and 2.21× the memory,
/// and generalised the lesson. A 200-line fence must therefore be **one** block
/// holding **one** `ShapedText` that reports 200 lines — not 200 of anything.
#[test]
fn a_two_hundred_line_fence_is_one_layout() {
    let body: String = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from(body.as_str()),
        },
    );
    let tree = built_tree(
        &doc,
        &Theme::muya_default(),
        f32::INFINITY,
        &LayoutOptions::default(),
    );
    assert_eq!(tree.len(), 1, "one block");
    assert_eq!(tree.built_count(), 1, "one built text");
    assert_eq!(
        tree.shaped_line_count(0),
        Some(200),
        "and it holds every line"
    );
}

/// The gutter is one more `Layout` for the whole fence, not one per number.
#[test]
fn the_line_number_gutter_is_one_more_layout_and_not_one_per_number() {
    let body: String = (0..50)
        .map(|i| format!("row {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from(body.as_str()),
        },
    );
    let options = LayoutOptions {
        code_block_line_numbers: true,
        ..LayoutOptions::default()
    };
    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    let mut tree = LayoutTree::plan(&doc.doc, &Theme::muya_default(), f32::INFINITY, &options);
    tree.build_all(&doc.doc, &mut fonts, &mut shaper);
    tree.place();
    let list = tree.emit(&fonts).expect("faces resolve");
    assert_eq!(tree.len(), 1);
    // The gutter's numbers land level with the code's lines: 50 numbers, 50
    // code lines, one glyph run per line on each side.
    assert_eq!(tree.shaped_line_count(0), Some(50));
    assert_eq!(glyph_runs(&list, BlockKind::CodeBlock), 100);
    // …and the text is pushed right by the 2.5em gutter rather than the 1em
    // padding.
    let (x, _) = tree.content_origin(0);
    assert!(
        (x - 37.0).abs() < 1e-3,
        "1px border + 2.5em of 14.4: got {x}"
    );
}

// ---------------------------------------------------------------------------
// The 800 / 750 split — a named S1 gate clause
// ---------------------------------------------------------------------------

/// The gate: *"Both theme widths (800/750) produce different, correct goldens
/// from the same document."* Different in the column, different in where the
/// text breaks, and different in the code-block border (C1) — three axes from
/// one document.
#[test]
fn the_two_themes_produce_different_and_correct_output_from_one_document() {
    let mut doc = Doc::new();
    let root = doc.root();
    let long = "A paragraph long enough that the difference between a seven hundred \
         pixel column and a six hundred and fifty pixel column decides where it \
         breaks, which is the whole point of laying the same document out twice. "
        .repeat(6);
    doc.para(root, &long);
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from("code"),
        },
    );

    let muya = lay_out(&doc, &Theme::muya_default());
    let dark = lay_out(&doc, &Theme::dark());

    assert_eq!(muya.width, 700.0);
    assert_eq!(dark.width, 650.0);
    assert_ne!(
        muya.blocks[0].bounds.height, dark.blocks[0].bounds.height,
        "the narrower column must break the paragraph differently"
    );
    // C1: 30 of 32 themes zero the code-block border, so the same fence is 2px
    // shorter and 2px narrower in `dark`.
    let border_fills = |l: &DisplayList| {
        l.blocks
            .iter()
            .find(|b| b.kind == BlockKind::CodeBlock)
            .map(|b| {
                b.items
                    .iter()
                    .filter(|i| matches!(i, DisplayItem::Rect(_)))
                    .count()
            })
            .expect("a code block")
    };
    assert_eq!(border_fills(&muya), 5, "background plus four border fills");
    assert_eq!(border_fills(&dark), 1, "background only");
    assert_ne!(muya, dark, "the two lists differ");
}

/// P3 measured `cjk.md`'s deliberately-long unspaced line at **681.49px**, which
/// is under `muya-default`'s 700 and over `dark`'s 650. So it wraps in exactly
/// one of the two shipped themes, and that is the content behind S2's
/// CJK break-position clause — worth pinning now so that the corroboration for
/// the content-column reading is a test rather than a note.
#[test]
fn the_long_cjk_line_wraps_in_dark_and_not_in_muya_default() {
    // `bench/corpus/cjk.md:34`, verbatim.
    let line =
        "中文没有空格所以断行规则完全依赖UAX14的CJK规则而不是空格这一行故意很长用来触发换行。";
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, line);

    let muya = built_tree(
        &doc,
        &Theme::muya_default(),
        f32::INFINITY,
        &LayoutOptions::default(),
    );
    let dark = built_tree(
        &doc,
        &Theme::dark(),
        f32::INFINITY,
        &LayoutOptions::default(),
    );

    let (width, _) = muya.shaped_size(0).expect("a paragraph");
    assert!(
        (width - 681.49).abs() < 0.5,
        "P3 measured 681.49px for this line; got {width}"
    );
    assert_eq!(muya.shaped_line_count(0), Some(1), "700px fits it");
    assert_eq!(dark.shaped_line_count(0), Some(2), "650px does not");
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

/// Column widths are max-content plus padding, and the table is shrink-to-fit
/// rather than stretched — `.mu-table-inner` carries no `width: 100%`.
#[test]
fn table_columns_are_max_content_plus_padding_and_the_table_does_not_stretch() {
    let mut doc = Doc::new();
    let root = doc.root();
    let table = doc.push(root, Block::Table { children: vec![] });
    for cells in [["Name", "A much longer heading"], ["x", "y"]] {
        let row = doc.push(table, Block::TableRow { children: vec![] });
        for cell in cells {
            doc.push(
                row,
                Block::TableCell {
                    align: Align::None,
                    text: Text::from(cell),
                },
            );
        }
    }
    let list = lay_out(&doc, &Theme::muya_default());
    let cells: Vec<_> = list
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::TableCell)
        .collect();
    assert_eq!(cells.len(), 4);
    // Column 1 is wider than column 0 because its widest cell is wider.
    assert!(cells[1].bounds.width > cells[0].bounds.width);
    // Both columns are the same width in both rows.
    assert_eq!(cells[0].bounds.width, cells[2].bounds.width);
    assert_eq!(cells[1].bounds.width, cells[3].bounds.width);
    // Shrink to fit: the table is narrower than the column it sits in.
    let table_width = cells[0].bounds.width + cells[1].bounds.width;
    assert!(table_width < list.width, "got {table_width}");
    // Every cell in a row shares the row's height.
    assert_eq!(cells[0].bounds.height, cells[1].bounds.height);
    // A one-line cell is one line box plus 6px of padding above and below.
    assert!(
        (cells[0].bounds.height - (25.6 + 12.0)).abs() < 1e-2,
        "got {}",
        cells[0].bounds.height
    );
}

/// When the intrinsic widths do not fit, every column scales by the same factor
/// and the table lands exactly on the content width. A browser would overflow
/// instead; this is the documented divergence.
#[test]
fn an_over_wide_table_is_scaled_to_the_column_rather_than_overflowing_it() {
    let long = "a word ".repeat(40);
    let mut doc = Doc::new();
    let root = doc.root();
    let table = doc.push(root, Block::Table { children: vec![] });
    let row = doc.push(table, Block::TableRow { children: vec![] });
    for _ in 0..3 {
        doc.push(
            row,
            Block::TableCell {
                align: Align::None,
                text: Text::from(long.as_str()),
            },
        );
    }
    let list = lay_out(&doc, &Theme::muya_default());
    let cells: Vec<_> = list
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::TableCell)
        .collect();
    let total: f32 = cells.iter().map(|c| c.bounds.width).sum();
    assert!((total - list.width).abs() < 1e-2, "got {total}");
    for c in &cells {
        assert!(c.bounds.x + c.bounds.width <= list.width + 1.0);
    }
}

// ---------------------------------------------------------------------------
// Everything else, with glyphs on
// ---------------------------------------------------------------------------

/// An ordered marker is real text, right-aligned in the indent, and it is
/// emitted on the **item** rather than on the list.
#[test]
fn an_ordered_marker_is_shaped_text_beside_its_item() {
    let mut doc = Doc::new();
    let root = doc.root();
    let list = doc.push(
        root,
        Block::OrderList {
            start: 9,
            delimiter: OrderDelim::Period,
            loose: false,
            children: vec![],
        },
    );
    for _ in 0..2 {
        let item = doc.push(list, Block::ListItem { children: vec![] });
        doc.para(item, "item");
    }
    let out = lay_out(&doc, &Theme::muya_default());
    assert_eq!(glyph_runs(&out, BlockKind::ListItem), 2, "9. and 10.");
    // A CSS `outside` marker is placed by its trailing edge: right edge at the
    // item's content edge less the 0.5em gap, growing leftwards. `10.` is wider
    // than `9.` and therefore starts further left, rather than spilling over
    // the item's own text.
    let mut lefts = Vec::new();
    for item in out.blocks.iter().filter(|b| b.kind == BlockKind::ListItem) {
        for run in &item.items {
            if let DisplayItem::Glyphs(g) = run {
                assert!(
                    (g.offset + g.advance - 22.0).abs() < 0.5,
                    "trailing edge at 30 - 0.5em: {} + {}",
                    g.offset,
                    g.advance
                );
                lefts.push(g.offset);
            }
        }
    }
    assert_eq!(lefts.len(), 2);
    assert!(lefts[1] < lefts[0], "`10.` starts further left than `9.`");
}

/// A bullet list draws shapes and no glyphs on its items; the glyphs belong to
/// the paragraphs inside them.
#[test]
fn a_bullet_marker_is_a_shape_and_not_a_glyph() {
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
    doc.para(item, "bullet");
    let out = lay_out(&doc, &Theme::muya_default());
    assert_eq!(glyph_runs(&out, BlockKind::ListItem), 0);
    assert_eq!(glyph_runs(&out, BlockKind::Paragraph), 1);
}

/// Nothing in the corpus should reach `.notdef` through the committed set, and
/// a block-flow bug that shaped text against the wrong stack would show up here
/// before it showed up in a golden.
#[test]
fn a_mixed_script_document_lays_out_with_no_tofu() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "Hello العربية 😀 中文");
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: "text".into(),
            fence_len: Some(3),
            text: Text::from("fn main() { println!(\"hi\"); }"),
        },
    );
    let out = lay_out(&doc, &Theme::muya_default());
    let mut glyphs = 0;
    for block in &out.blocks {
        for item in &block.items {
            if let DisplayItem::Glyphs(run) = item {
                for g in &run.glyphs {
                    assert_ne!(g.id, 0, "tofu in {}", block.kind.name());
                    glyphs += 1;
                }
            }
        }
    }
    assert!(glyphs > 20, "something was laid out");
}

/// A deeply nested structure — code inside a blockquote inside a list item —
/// is the compounding case the unit rule has to survive end to end.
#[test]
fn a_code_block_inside_a_blockquote_inside_a_list_item() {
    let mut doc = Doc::new();
    let root = doc.root();
    let list = doc.push(
        root,
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: true,
            children: vec![],
        },
    );
    let item = doc.push(list, Block::ListItem { children: vec![] });
    let quote = doc.push(item, Block::BlockQuote { children: vec![] });
    doc.push(
        quote,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from("nested"),
        },
    );
    let tree = built_tree(
        &doc,
        &Theme::muya_default(),
        f32::INFINITY,
        &LayoutOptions::default(),
    );
    // list(0) item(1) quote(2) code(3): 30px of list indent, then 30px of
    // blockquote padding, then 1px of border and 14.4px of code padding.
    let (x, _) = tree.content_origin(3);
    assert!((x - (30.0 + 30.0 + 1.0 + 14.4)).abs() < 1e-3, "got {x}");
    // The blockquote's trailing padding is 30px too — it is not nested in
    // another quote — so the code column is 700 − 30 − 60 − 2 − 28.8.
    let width = tree.block_content_width(3);
    assert!(
        (width - (700.0 - 30.0 - 60.0 - 2.0 - 28.8)).abs() < 1e-3,
        "got {width}"
    );
    // And the code text is 14.4px, not 16 and not 0.9 × 0.9 × 16.
    let (_, height) = tree.shaped_size(3).expect("code has text");
    assert!((height - 1.6 * 14.4).abs() < 1e-2, "got {height}");
}

/// Layout is deterministic: the same document, theme and width give a
/// byte-identical list. S7 makes this a property; here it is the smoke test
/// that D10's exact-equality goldens are even possible.
#[test]
fn the_same_input_lays_out_identically_twice() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "determinism");
    doc.push(
        root,
        Block::AtxHeading {
            level: 2,
            text: Text::from("and a heading"),
        },
    );
    let theme = Theme::muya_default();
    assert_eq!(lay_out(&doc, &theme), lay_out(&doc, &theme));
}

// ---------------------------------------------------------------------------
// Post-review fixes that need glyphs
// ---------------------------------------------------------------------------

/// **Finding 2.** The reference does not infer a base direction. The desktop
/// editor sets `dir` explicitly (`editorWithTabs/editor.vue:5`,
/// `:dir="textDirection"`) from a preference whose schema is
/// `{"enum": ["ltr", "rtl"], "default": "ltr"}`
/// (`main/preferences/schema.json:200-204`) — there is no `auto` in the enum.
/// So a Hebrew paragraph in a default install is laid out at base level 0 and
/// **starts at the left margin**, which `BaseDirection::Auto` would not do.
///
/// Bidi is unaffected: the run still comes back RTL-flagged, because the UBA
/// reorders within the line whatever the paragraph's base level is.
#[test]
fn an_rtl_paragraph_is_laid_out_at_a_left_to_right_base() {
    let mut doc = Doc::new();
    let root = doc.root();
    // Hebrew first, so `Auto` would infer an RTL base and right-align it.
    doc.para(root, "שלום עולם");
    let out = lay_out(&doc, &Theme::muya_default());
    let mut runs = 0;
    for item in &out.blocks[0].items {
        if let DisplayItem::Glyphs(run) = item {
            runs += 1;
            assert!(run.is_rtl, "the run itself is still RTL — bidi still works");
            assert!(
                run.offset < 1.0,
                "an LTR base starts at the left margin, not the right: {}",
                run.offset
            );
        }
    }
    assert_eq!(runs, 1);
}

/// The same question for a table cell. `blockSyntax.css:598-615` sets a
/// **physical** `text-align: left` on `th`/`td`, and `flow.rs` maps
/// `Align::None` to `TextAlign::Start`. Under the LTR base that finding 2
/// installs, `Start` resolves to left for every cell in every document, so the
/// physical/logical distinction is unobservable and no second fix is needed.
/// Asserted rather than argued, because it stops being true the day a base
/// direction becomes a setting.
#[test]
fn a_table_cells_start_alignment_resolves_to_the_left_edge() {
    let mut doc = Doc::new();
    let root = doc.root();
    let table = doc.push(root, Block::Table { children: vec![] });
    let row = doc.push(table, Block::TableRow { children: vec![] });
    doc.push(
        row,
        Block::TableCell {
            align: Align::None,
            text: Text::from("שלום"),
        },
    );
    let out = lay_out(&doc, &Theme::muya_default());
    let cell = out
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::TableCell)
        .expect("a cell");
    for item in &cell.items {
        if let DisplayItem::Glyphs(run) = item {
            assert!(
                (run.offset - (cell.bounds.x + 13.0)).abs() < 0.5,
                "flush with the cell's left padding edge: {} vs {}",
                run.offset,
                cell.bounds.x + 13.0
            );
        }
    }
}

/// **Finding 3, measured.** The code column in `muya-default` is
/// `700 − 2×14.4 − 2×1 = 669.2`, and `bench/corpus/README.md:7` already holds a
/// 78-character fence line that exceeds it. Wrapped, such a line breaks in two;
/// unwrapped — muya's default — it stays one line, the block's box stays the
/// column, and the excess is reported as `overflow_x` so S4 can clip and scroll
/// rather than guess.
///
/// The fixture carries spaces rather than being 80 hyphens, because a hyphen
/// run turned out **not** to break here: parley leaves the overflowing tail on
/// the line rather than breaking after the last `HY` that fits. Interesting,
/// and beside the point being tested — a code line that wraps has to be a code
/// line that *can* wrap.
#[test]
fn a_long_fence_line_overflows_its_box_instead_of_wrapping() {
    // No trailing space: parley's `Layout::width()` excludes trailing
    // whitespace while a glyph run's `offset + advance` includes it, and the
    // two have to agree for `overflow_x` to be checkable against the glyphs.
    let long = "abcdefghi ".repeat(8).trim_end().to_string();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: String::new(),
            fence_len: Some(3),
            text: Text::from(long.as_str()),
        },
    );
    let theme = Theme::muya_default();

    let (unwrapped, fonts) = built(&doc, &theme, f32::INFINITY, &LayoutOptions::default());
    let (width, _) = unwrapped.shaped_size(0).expect("code has text");
    assert!(
        (unwrapped.block_content_width(0) - 669.2).abs() < 1e-2,
        "the code column: {}",
        unwrapped.block_content_width(0)
    );
    assert!(width > 669.2, "the fixture must actually overflow: {width}");
    assert_eq!(unwrapped.shaped_line_count(0), Some(1), "it does not wrap");

    let list = unwrapped.emit(&fonts).expect("faces resolve");
    let block = &list.blocks[0];
    assert!(
        (block.overflow_x - (width - 669.2)).abs() < 1e-2,
        "overflow_x is how far the glyphs run past the content box: {}",
        block.overflow_x
    );
    // The box itself is still the column — a renderer clips to `bounds`.
    assert!((block.bounds.width - 700.0).abs() < 1e-3);
    let rightmost = block
        .items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g.offset + g.advance),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    let content_right = unwrapped.content_origin(0).0 + unwrapped.block_content_width(0);
    assert!(
        rightmost > content_right,
        "the glyphs really do leave the content box: {rightmost} vs {content_right}"
    );
    assert!(
        (rightmost - (content_right + block.overflow_x)).abs() < 1e-2,
        "and `overflow_x` is exactly how far"
    );

    // With the option on it wraps, and nothing overflows.
    let (wrapped, fonts) = built(
        &doc,
        &theme,
        f32::INFINITY,
        &LayoutOptions {
            wrap_code_blocks: true,
            ..LayoutOptions::default()
        },
    );
    assert_eq!(wrapped.shaped_line_count(0), Some(2));
    let list = wrapped.emit(&fonts).expect("faces resolve");
    assert_eq!(list.blocks[0].overflow_x, 0.0);
}

/// **Finding 7, drawn.** The label reaches the display list as real glyphs in
/// the monospace generic, at the figure's padding-box origin plus its own
/// `left: 0; padding: 0 1em` and `top: 0.2em`.
#[test]
fn the_footnote_label_is_drawn_above_its_body() {
    let mut doc = Doc::new();
    let root = doc.root();
    let footnote = doc.push(
        root,
        Block::Footnote {
            identifier: "1".into(),
            children: vec![],
        },
    );
    doc.para(footnote, "the note body");
    let out = lay_out(&doc, &Theme::muya_default());
    let figure = out
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::Footnote)
        .expect("a footnote");
    let label: Vec<_> = figure
        .items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g),
            _ => None,
        })
        .collect();
    assert_eq!(label.len(), 1, "`[^1]:` is one run");
    let run = label[0];
    assert_eq!(run.glyphs.len(), 5, "[ ^ 1 ] :");
    for g in &run.glyphs {
        assert_ne!(g.id, 0, "the monospace generic must resolve");
    }
    assert!((run.offset - (figure.bounds.x + 14.0)).abs() < 0.5);
    assert_eq!(run.font_size, 14.0);
    assert_eq!(run.brush.a, 204, "dimmed by the figure's 0.8");
    // The body starts below the label, which is what the 1.2em padding-top is
    // for; before this fix the gap was there and the label was not.
    let body = out
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::Paragraph)
        .expect("a body paragraph");
    assert!(body.bounds.y > run.baseline);
}

/// **Finding 4, end to end.** The rule's painted band is the top 2px of a 4px
/// border box centred on the line, so its centre is `0.5H − 1` and not `0.5H`.
#[test]
fn the_thematic_break_rule_sits_half_its_width_above_the_line_centre() {
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::ThematicBreak {
            text: Text::from("---"),
        },
    );
    let out = lay_out(&doc, &Theme::muya_default());
    let hr = &out.blocks[0];
    assert!((hr.bounds.height - 25.6).abs() < 1e-3, "one body line box");
    match &hr.items[0] {
        DisplayItem::Line(line) => {
            assert!(
                (line.y0 - 11.8).abs() < 1e-3,
                "12.8 is the border box's centre; the border is its top 2px: {}",
                line.y0
            );
        }
        other => panic!("expected a rule, got {other:?}"),
    }
}
