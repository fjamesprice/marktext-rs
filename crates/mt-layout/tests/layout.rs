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
use mt_layout::text::{InlineBoxSpec, TextRequest, TextShaper};
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

// ---------------------------------------------------------------------------
// `InlineBox::baseline` against a real face
// ---------------------------------------------------------------------------

/// The companion to `src/text.rs`'s four unit tests, and the half they cannot
/// state: **the baseline a box is aligned to is the *text*'s.**
///
/// In `src/text.rs` no face can be registered, so a line's ascent comes only
/// from the boxes on it and "the line's baseline" is a number the boxes
/// themselves set. Here the line has real glyphs, the ascent is Open Sans's,
/// and a box shorter than that ascent is placed *down* from the top of the
/// line by an amount neither the test nor the box chose. That is the claim D2
/// moved the parley pin for.
#[test]
fn an_inline_box_is_aligned_to_the_text_baseline_and_not_to_the_line_top() {
    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    let families = vec!["Open Sans".to_string()];

    // The same line twice: once as plain text, once with a box in the middle.
    let plain = TextRequest::new("Hi there", &families, 16.0, 1.6);
    let text_baseline = shaper.shape(&mut fonts, &plain).first_baseline();
    assert!(
        text_baseline > 0.0,
        "the face must have resolved for this test to mean anything"
    );

    // 12px tall, shorter than the line's ascent, so a naive "place it at the
    // top of the line" would also put it above the baseline — but at the wrong
    // y. `Some(12.0)` and `None` are the same geometry stated two ways, and
    // both must land the bottom edge on the text baseline.
    let boxes = [
        InlineBoxSpec {
            id: 1,
            index: 2,
            width: 24.0,
            height: 12.0,
            baseline: Some(12.0),
        },
        InlineBoxSpec {
            id: 2,
            index: 3,
            width: 24.0,
            height: 12.0,
            baseline: None,
        },
    ];
    let mut request = TextRequest::new("Hi there", &families, 16.0, 1.6);
    request.inline_boxes = &boxes;
    let shaped = shaper.shape(&mut fonts, &request);
    let mut out = Vec::new();
    shaped.emit(&fonts, 0.0, 0.0, &mut out).expect("resolves");

    // Neither box is tall enough to change the line's ascent, so the baseline
    // is still the text's.
    assert!((shaped.first_baseline() - text_baseline).abs() < 1e-3);

    let placed: Vec<_> = out
        .iter()
        .filter_map(|i| match i {
            DisplayItem::InlineBox(b) => Some(*b),
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 2, "both boxes are emitted");
    for b in &placed {
        assert!(
            (b.y + b.height - text_baseline).abs() < 1e-3,
            "box {} bottom edge {} must sit on the text baseline {text_baseline}",
            b.id,
            b.y + b.height
        );
        assert!(
            b.y > 0.5,
            "box {} must be pushed down from the line top, not pinned to it: y = {}",
            b.id,
            b.y
        );
    }
    assert!(
        (placed[0].y - placed[1].y).abs() < 1e-3,
        "`Some(height)` and `None` are the same instruction"
    );

    // And the boxes really are in the line: the run after the second box
    // starts to the right of it.
    let advance: f32 = out
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g.advance),
            _ => None,
        })
        .sum();
    assert!(
        shaped.width() > advance,
        "the line grew by the boxes' width: {} vs {advance}",
        shaped.width()
    );
}

// ---------------------------------------------------------------------------
// Inline layout against the real font set — S2
// ---------------------------------------------------------------------------

fn glyph_run_list(block: &mt_layout::display::BlockDisplay) -> Vec<&mt_layout::display::GlyphRun> {
    block
        .items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g),
            _ => None,
        })
        .collect()
}

/// **Inline horizontal padding advances; inline vertical padding does not.**
/// CSS 2.1 § 10.3.2 against § 10.6.1, which is the whole asymmetry of
/// `code.mu-inline-rule { padding: 0.2em 0.4em }` (`inlineSyntax.css:60-70`).
///
/// At a 16 px paragraph the code is `0.8em` → 12.80 and its `0.4em` is
/// `0.4 × 12.8` = **5.12**, so the code's glyphs start 5.12 px after the text
/// before them and the text after them starts 5.12 px past the code's last
/// glyph. The ground is the same box, drawn: it starts where the run before
/// ended and is `advance + 2 × 5.12` wide. The `0.2em` adds 2.56 above and below
/// and changes **no** line's height — the paragraph is the theme's own 25.60.
#[test]
fn inline_code_padding_moves_the_text_across_and_not_the_line_down() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "before `code` after");
    let list = lay_out(&doc, &theme);
    let block = &list.blocks[0];
    let runs = glyph_run_list(block);
    assert_eq!(runs.len(), 3, "before, code, after: {runs:#?}");
    let pad = 16.0 * theme.inline_code.font_size_em * theme.inline_code.padding_x_em;
    assert!((pad - 5.12).abs() < 1e-4, "0.4em of 0.8em of 16px");
    assert!(
        (runs[1].offset - (runs[0].offset + runs[0].advance + pad)).abs() < 1e-3,
        "the code is pushed right by its own padding: {runs:#?}"
    );
    assert!(
        (runs[2].offset - (runs[1].offset + runs[1].advance + pad)).abs() < 1e-3,
        "and so is everything after it: {runs:#?}"
    );
    // The ground brackets the run it belongs to, on both sides.
    let ground = block
        .items
        .iter()
        .find_map(|i| match i {
            DisplayItem::Rect(r) if r.corner_radius == theme.inline_code.corner_radius_px => {
                Some(*r)
            }
            _ => None,
        })
        .expect("inline code paints a ground");
    assert!(
        (ground.rect.x - (runs[1].offset - pad)).abs() < 1e-3,
        "{ground:?}"
    );
    assert!(
        (ground.rect.width - (runs[1].advance + 2.0 * pad)).abs() < 1e-3,
        "{ground:?}"
    );
    assert!(ground.rect.x > 0.0, "and it is not at a negative x");
    // The vertical half of the same declaration paints outside the line box and
    // grows nothing — CSS 2.1 § 10.6.1.
    assert!(
        (block.bounds.height - 25.6).abs() < 1e-3,
        "got {}",
        block.bounds.height
    );
    assert!(
        ground.rect.height > 25.6 * 0.5,
        "the 0.2em is painted even though it is not counted: {ground:?}"
    );
}

/// The whole of C6 in one measurement: `**bold**` is narrower than `bold` was
/// wide plus four asterisks, because the asterisks are not there.
///
/// Measured rather than asserted structurally, because "the markers are hidden"
/// is a claim about pixels and the only way to be wrong about it quietly is to
/// check the string instead of the glyphs.
#[test]
fn a_hidden_marker_costs_no_glyphs_and_no_width() {
    let theme = Theme::muya_default();
    let mut marked = Doc::new();
    let root = marked.root();
    marked.para(root, "**bold**");
    let mut plain = Doc::new();
    let root = plain.root();
    plain.para(root, "bold");

    let a = lay_out(&marked, &theme);
    let b = lay_out(&plain, &theme);
    let ga = glyph_run_list(&a.blocks[0]);
    let gb = glyph_run_list(&b.blocks[0]);
    let count = |runs: &[&mt_layout::display::GlyphRun]| -> usize {
        runs.iter().map(|r| r.glyphs.len()).sum()
    };
    assert_eq!(count(&ga), 4, "four letters, no asterisks");
    assert_eq!(count(&gb), 4);
    // Bold is wider than regular at the same size, so this is not an equality —
    // what must hold is that the marked-up line is nowhere near four asterisks
    // wider, and that its glyph ranges index the *visible* string.
    let advance =
        |runs: &[&mt_layout::display::GlyphRun]| -> f32 { runs.iter().map(|r| r.advance).sum() };
    assert!(
        advance(&ga) < advance(&gb) * 1.5,
        "{} vs {}",
        advance(&ga),
        advance(&gb)
    );
    assert!(ga.iter().all(|r| r.text_range.end <= 4), "{ga:#?}");

    // …and the map is the way back to the eight-byte original.
    let map = a.blocks[0]
        .text_map
        .as_ref()
        .expect("a paragraph is a leaf");
    assert_eq!(map.block_len(), 8);
    assert_eq!(map.visible_len(), 4);
    assert_eq!(map.to_block(0), 2);
    assert_eq!(map.to_visible(0), None);
}

/// A style run really does change the face parley picks: the bold span
/// resolves to a different `FontId` than the text around it.
///
/// This is the assertion that a `push` over a range did anything at all. With
/// one style run the whole paragraph would be one `FontId`; the count of
/// distinct faces is the smallest observable that cannot be faked.
#[test]
fn a_strong_span_resolves_to_a_different_face_than_the_text_around_it() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "regular **bold** regular");
    let list = lay_out(&doc, &theme);
    let runs = glyph_run_list(&list.blocks[0]);
    let faces: Vec<_> = runs.iter().map(|r| r.font).collect();
    let distinct: std::collections::BTreeSet<_> = faces.iter().collect();
    assert_eq!(
        distinct.len(),
        2,
        "one regular face and one bold one: {faces:?}"
    );
    // The middle run is the bold one, and it covers exactly `bold` in the
    // visible string — which is bytes 8..12 of "regular bold regular".
    let bold = runs
        .iter()
        .find(|r| r.text_range == (8..12))
        .unwrap_or_else(|| panic!("no run over the bold span: {runs:#?}"));
    assert_ne!(bold.font, runs[0].font);
}

/// Inline code changes size, family **and** colour in one run, and the line box
/// does not shrink to the code span's own smaller line height.
#[test]
fn an_inline_code_span_is_smaller_and_in_the_code_face_without_shrinking_the_line() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "call `fn` now");
    let list = lay_out(&doc, &theme);
    let block = &list.blocks[0];
    let runs = glyph_run_list(block);
    let code = runs
        .iter()
        .find(|r| r.text_range == (5..7))
        .unwrap_or_else(|| panic!("no run over `fn`: {runs:#?}"));
    assert_eq!(code.font_size, 16.0 * theme.inline_code.font_size_em);
    assert_ne!(
        code.font, runs[0].font,
        "a monospace face, not the body one"
    );
    assert_eq!(
        code.brush,
        mt_layout::Brush::resolve(theme.colors.editor, mt_layout::Brush::default())
    );
    // The surrounding 16px text still sets the line, so the paragraph is the
    // theme's own line box and not `1.6 × 12.8`.
    assert!(
        (block.bounds.height - 25.6).abs() < 1e-3,
        "got {}",
        block.bounds.height
    );
}

/// An ATX heading's `#` is a marker like any other; the **space** after it is
/// not, and its negative margin is where the heading's first glyph comes from.
///
/// # Why this is six glyphs and not five
///
/// The expectation was `# Title` → five glyphs, the whole `# ` hidden. It is six
/// against `header.ts:44-52`: the `#`s go in `span.mu-hide.mu-remove`
/// (`font-size: 0` — `inlineSyntax.css:21-30`) and the space in
/// `span.mu-header-tight-space.mu-remove`, which carries no hide class at all
/// and whose only rule is `margin-left: -0.3em` (`:335-337`). So the space is
/// drawn and the heading is pulled back under it: at 30 px the margin is −9.00
/// and Open Sans Bold's space is 0.2598em → 7.79, netting **−1.21** for the
/// first glyph of `Title`. The box is unchanged, because a horizontal margin is
/// an advance and nothing else.
#[test]
fn an_atx_headings_hash_is_hidden_and_its_space_carries_a_negative_margin() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.push(
        root,
        Block::AtxHeading {
            level: 1,
            text: Text::from("# Title"),
        },
    );
    let list = lay_out(&doc, &theme);
    let block = &list.blocks[0];
    let runs = glyph_run_list(block);
    let glyphs: usize = runs.iter().map(|r| r.glyphs.len()).sum();
    assert_eq!(glyphs, 6, "the space and `Title`, not `# Title`");
    // The space and the title are one shaped run in one face, so the run's own
    // origin is the margin and the second glyph is the `T`.
    let margin = 30.0 * theme.inline.header_tight_space_margin_left_em;
    assert!((margin - -9.0).abs() < 1e-6, "−0.3em of 30px");
    assert!(
        (runs[0].offset - margin).abs() < 1e-3,
        "got {}",
        runs[0].offset
    );
    let title_x = runs[0].glyphs[1].x;
    assert!(
        (-2.0..0.0).contains(&title_x),
        "`T` is pulled left of the column but not by the whole margin: got {title_x}"
    );
    assert!((block.bounds.height - 42.0).abs() < 1e-3, "1.4 × 30");
    let map = block.text_map.as_ref().expect("a heading is a leaf");
    assert_eq!((map.block_len(), map.visible_len()), (7, 6));
}

/// Every leaf in a mixed document maps every visible offset back to a
/// block-text offset inside the token that produced it — the gate's property
/// test, over real shaped output rather than over the walk alone.
#[test]
fn every_visible_offset_in_a_laid_out_document_maps_back_inside_its_token() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    for line in [
        "plain text with no markup at all",
        "**bold** and *italic* and `code` and ~~gone~~",
        "a [link](https://example.com/x) and an ![image](./i.png)",
        "an &amp; entity and a soft\nbreak",
        "中文**粗体**紧邻中文字符，没有空格",
    ] {
        doc.para(root, line);
    }
    let list = lay_out(&doc, &theme);
    for block in &list.blocks {
        let Some(map) = &block.text_map else { continue };
        for run in glyph_run_list(block) {
            assert!(
                run.text_range.end <= map.visible_len(),
                "a glyph run indexes past the visible string: {run:?}"
            );
            for v in [run.text_range.start, run.text_range.end] {
                let b = map.to_block(v);
                assert!(b <= map.block_len());
            }
        }
        for run in map.runs() {
            assert!(run.token.start <= run.block.start && run.block.end <= run.token.end);
        }
    }
}

// ---------------------------------------------------------------------------
// The drawn inline decorations, against real faces
// ---------------------------------------------------------------------------

fn rects(list: &DisplayList, kind: BlockKind) -> Vec<mt_layout::FilledRect> {
    list.blocks
        .iter()
        .filter(|b| b.kind == kind)
        .flat_map(|b| b.items.iter())
        .filter_map(|i| match i {
            DisplayItem::Rect(r) => Some(*r),
            _ => None,
        })
        .collect()
}

/// Inline code's ground is a rounded rect **behind** the run — `padding:
/// 0.2em 0.4em`, `border-radius: 3px`, `background: var(--code-block-bg-color)`
/// (`inlineSyntax.css:60-70`) — and the `em` is the code's own 0.8em, not the
/// paragraph's.
#[test]
fn an_inline_code_span_is_painted_on_a_rounded_ground_before_its_glyphs() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "before `code` after");
    let list = lay_out(&doc, &theme);
    let grounds = rects(&list, BlockKind::Paragraph);
    assert_eq!(grounds.len(), 1, "one code span, one ground: {grounds:#?}");
    let ground = grounds[0];
    assert_eq!(ground.corner_radius, theme.inline_code.corner_radius_px);
    assert_eq!(ground.corner_radius, 3.0);

    // Paint order: `BlockDisplay::items` is paint order and a ground sits
    // behind the glyphs it belongs to, so it must come first.
    let para = list
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::Paragraph)
        .expect("a paragraph");
    assert!(
        matches!(para.items.first(), Some(DisplayItem::Rect(_))),
        "the ground is painted before any glyph run"
    );

    // The padding is the code's own em: 16 × 0.8 × 0.4 on each side.
    let em = theme.metrics.font_size_px * theme.inline_code.font_size_em;
    let code_run = para
        .items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g),
            _ => None,
        })
        .find(|g| g.font_size == em)
        .expect("the code span shapes at 0.8em");
    let pad = em * theme.inline_code.padding_x_em;
    assert!((ground.rect.x - (code_run.offset - pad)).abs() < 0.01);
    assert!((ground.rect.width - (code_run.advance + 2.0 * pad)).abs() < 0.01);
    // It does not grow the line: a paragraph with a code span is exactly as
    // tall as one without, because CSS pads the inline box and not the line box.
    let mut plain = Doc::new();
    let plain_root = plain.root();
    plain.para(plain_root, "before code after");
    assert_eq!(
        lay_out(&plain, &theme).height,
        list.height,
        "inline code's padding is not line height"
    );
}

/// `del` and a link are drawn from the **face's own** metrics, because muya has
/// no CSS for either — `grep line-through` is empty and `a.mu-inline-rule` sets
/// colour alone — so both are the browser's UA sheet, which reads `post` and
/// `OS/2`.
///
/// Asserted as *relationships to the baseline* rather than as literal pixels,
/// because the literal is Open Sans's and belongs in a golden.
#[test]
fn a_strikethrough_and_an_underline_come_from_the_faces_own_metrics() {
    let theme = Theme::muya_default();
    let mut doc = Doc::new();
    let root = doc.root();
    doc.para(root, "~~struck~~ and [linked](https://example.com)");
    let list = lay_out(&doc, &theme);
    let para = list
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::Paragraph)
        .expect("a paragraph");
    let baseline = para
        .items
        .iter()
        .find_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g.baseline),
            _ => None,
        })
        .expect("glyphs");
    let lines = rects(&list, BlockKind::Paragraph);
    assert_eq!(lines.len(), 2, "one strikethrough, one underline");

    let strike = lines
        .iter()
        .find(|r| r.rect.y < baseline)
        .expect("a strikeout sits above the baseline");
    let underline = lines
        .iter()
        .find(|r| r.rect.y > baseline)
        .expect("an underline sits below it");
    for rule in [strike, underline] {
        assert_eq!(rule.corner_radius, 0.0);
        assert!(
            rule.rect.height > 0.0 && rule.rect.height < theme.metrics.font_size_px * 0.25,
            "a face-derived thickness, not a guess: {rule:?}"
        );
        assert!(rule.rect.width > 0.0);
    }
    // Both take the run's own colour, which is what `text-decoration-color:
    // currentColor` means: the link's rule is the link colour and the `del`'s
    // is the paragraph's.
    assert_ne!(strike.brush, underline.brush);
    assert_eq!(
        underline.brush,
        mt_layout::Brush::resolve(theme.colors.link, mt_layout::Brush::default())
    );
}
