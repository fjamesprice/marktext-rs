//! D15's seam, checked against the stylesheets it was transcribed from.
//!
//! # Why this file exists, and it is the stage's method rather than a habit
//!
//! M3 §6's S2 record: *"the two reviews are not the same review at different
//! times. The cross-check finds what the reference says and the code does not;
//! the golden review finds what the code emits and nothing could have said. A
//! stage that runs only one of them ships the other's class of defect."* S3's
//! **first** review is this file, and it runs before a golden is regenerated.
//!
//! [`CodePalette`](mt_layout::theme::CodePalette)'s 33 fields had **never been
//! read by anything outside their own tests** when S3 opened — D15 says so —
//! so this is the first time a transcription made at S1 is compared against the
//! artifact it claims to describe. Every literal below is transcribed **from
//! the CSS cited beside it**, never read back out of the TOML: a test that
//! reads the theme file to check the theme file is an identity, and the S1
//! geometry table in `theme.rs` is written the same way for the same reason.
//!
//! # The two reference sheets, and which DOM each one reaches
//!
//! | theme | Prism sheet | where |
//! |---|---|---|
//! | `muya-default` | muya's own `light.theme.css` | `packages/muya/src/assets/styles/prismjs/` — the sheet a **no-theme** install gets, since D4's finding is that the default `light` theme has no `.theme.css` file at all |
//! | `dark` | `dark.theme.css` | `packages/desktop/src/renderer/src/assets/themes/prismjs/`, paired by the hand-written switch in `util/themeColor.ts:73-75` |
//!
//! Neither sheet's *container* rule reaches muya v2, which is the S3 finding
//! `theme.rs`'s `plain` field documents: every one of the 31 shipped Prism
//! sheets spells it `code[class*='language-'], pre.ag-paragraph`, and `ag-` is
//! the legacy editor's prefix while the `language-` class lives only on a
//! detached `div` (`codeBlockContent/index.ts:183-188`). So the plain colour is
//! `.mu-code-block { color: var(--editor-color-50) }` (`blockSyntax.css:207`)
//! in both, and [`plain_is_the_block_default_and_pushes_no_run`] is the
//! assertion.
//!
//! # What the differential could not see
//!
//! `cargo xtask highlight` compares **span boundaries** against Prism, 2,504
//! fences agreeing span for span, and it is blind to every number in this file:
//! a palette that painted every class black would pass it. That asymmetry is
//! why D15 records the class-precedence choice in prose and why the colours are
//! checked here.

use std::path::{Path, PathBuf};

use mt_doc::{Block, CodeKind, Document, Edit, NodeId, Text};
use mt_layout::display::{DisplayItem, DisplayList};
use mt_layout::fonts::{FaceList, Fonts};
use mt_layout::theme::{CodePalette, Theme};
use mt_layout::{Brush, CodeSpans, HighlightSpan, LayoutOptions, LayoutTree, text::TextShaper};

// ---------------------------------------------------------------------------
// The committed face set — the same three lines every other suite uses
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<crate> is two levels below the repo root")
        .to_path_buf()
}

fn bundled_fonts() -> Fonts {
    let list = FaceList::bundled();
    let mut fonts = Fonts::new();
    for face in &list.faces {
        let path = repo_root().join("assets/fonts").join(&face.file);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        fonts
            .register_face(face, bytes)
            .unwrap_or_else(|e| panic!("{e}"));
    }
    fonts.wire(&list).expect("wiring the committed set");
    fonts
}

// ---------------------------------------------------------------------------
// One fence, one span, one emitted colour
// ---------------------------------------------------------------------------

/// The token text every case uses: seven ASCII letters in one word, so the
/// fence is one line, one shaped run per style, and no break can confuse a
/// count.
const TOKEN: &str = "keyword";

/// Lay a one-token fence out with a single highlight span over the whole text,
/// and return its display list.
fn one_token(theme: &Theme, class: Option<&str>) -> (DisplayList, Fonts) {
    let mut doc = Document::new();
    let first = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: first }]);
    let root = doc.root();
    doc.apply(&[Edit::InsertNode {
        parent: root,
        index: 0,
        block: Block::CodeBlock {
            kind: CodeKind::Fenced,
            fence_len: Some(3),
            info: "rust".to_string(),
            text: Text::from(TOKEN),
        },
    }]);
    let node: NodeId = *doc.children(root).last().expect("just inserted");

    let mut code_spans = CodeSpans::new();
    if let Some(class) = class {
        code_spans.insert(node, vec![HighlightSpan::new(0..TOKEN.len(), class)]);
    }
    let options = LayoutOptions {
        code_spans,
        ..LayoutOptions::default()
    };

    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    let mut tree = LayoutTree::plan(&doc, theme, f32::INFINITY, &options);
    tree.build_all(&doc, &mut fonts, &mut shaper);
    tree.place();
    let list = tree.emit(&fonts).expect("the committed faces resolve");
    (list, fonts)
}

/// Every glyph run's brush, in emission order.
fn brushes(list: &DisplayList) -> Vec<Brush> {
    list.blocks
        .iter()
        .flat_map(|b| b.items.iter())
        .filter_map(|i| match i {
            DisplayItem::Glyphs(run) => Some(run.brush),
            _ => None,
        })
        .collect()
}

/// Every filled rect's brush, in emission order — the code block's own ground
/// first, then any token background.
fn rect_brushes(list: &DisplayList) -> Vec<Brush> {
    list.blocks
        .iter()
        .flat_map(|b| b.items.iter())
        .filter_map(|i| match i {
            DisplayItem::Rect(r) => Some(r.brush),
            _ => None,
        })
        .collect()
}

/// The one glyph run a one-token fence emits, and its colour.
fn emitted(theme: &Theme, class: &str) -> Brush {
    let (list, _fonts) = one_token(theme, Some(class));
    let runs = brushes(&list);
    assert_eq!(
        runs.len(),
        1,
        "{}: a one-token fence is one glyph run, not {}",
        theme.name,
        runs.len()
    );
    runs[0]
}

// ---------------------------------------------------------------------------
// The cross-check: every class, both sheets
// ---------------------------------------------------------------------------

/// `slategray`, the one **named** CSS colour in either sheet.
///
/// CSS Color 4's `<named-color>` table: `slategray` is `#708090`. Spelled here
/// rather than in the table below because a named colour is the one value a
/// reader cannot check by eye against the stylesheet.
const SLATEGRAY: Brush = Brush::rgb(0x70, 0x80, 0x90);

/// `muya-default`'s expected colour for every one of the 32 classes.
///
/// Transcribed from `packages/muya/src/assets/styles/prismjs/light.theme.css`,
/// line numbers in the comments. Three-digit hex expands the way CSS expands
/// it — `#905` is `#990055`, not `#090505`.
fn light_expected(class: &str) -> Brush {
    match class {
        // :28-33
        "comment" | "prolog" | "doctype" | "cdata" => SLATEGRAY,
        // :35-37
        "punctuation" => Brush::rgb(0x99, 0x99, 0x99),
        // :39-41 — `opacity: 0.7` and no colour of its own, so the block's.
        "namespace" => Brush::rgba(0x80, 0x80, 0x80, 0xb3),
        // :43-49
        "property" | "tag" | "boolean" | "number" | "constant" | "symbol" => {
            Brush::rgb(0x99, 0x00, 0x55)
        }
        // :52-57
        "selector" | "attr-name" | "string" | "char" | "builtin" => Brush::rgb(0x66, 0x99, 0x00),
        // :60-64
        "inserted" => Brush::rgb(0x22, 0x86, 0x3a),
        // :66-70
        "deleted" => Brush::rgb(0xb3, 0x1d, 0x28),
        // :72-77
        "operator" | "entity" | "url" => Brush::rgb(0x9a, 0x6e, 0x3a),
        // :80-83
        "atrule" | "attr-value" | "keyword" => Brush::rgb(0x00, 0x77, 0xaa),
        // :86-88
        "function" | "class-name" => Brush::rgb(0xdd, 0x4a, 0x68),
        // :91-94
        "regex" | "important" | "variable" => Brush::rgb(0xee, 0x99, 0x00),
        // :97-99 and :102-103 — weight and style only, so the block's colour.
        "bold" | "italic" => Brush::rgb(0x80, 0x80, 0x80),
        other => panic!("no expectation transcribed for `.token.{other}`"),
    }
}

/// `dark`'s expected colour for every one of the 32 classes.
///
/// Transcribed from
/// `packages/desktop/src/renderer/src/assets/themes/prismjs/dark.theme.css`.
/// The block's own colour here is `--editorColor50: rgba(255,255,255,0.5)` —
/// `#ffffff80` — and **not** the `#f8f8f2` of that sheet's `:6-9` container
/// rule, whose two selectors both target DOM muya v2 does not build.
fn dark_expected(class: &str) -> Brush {
    match class {
        // :46-51
        "comment" | "prolog" | "doctype" | "cdata" => SLATEGRAY,
        // :53-55
        "punctuation" => Brush::rgb(0xf8, 0xf8, 0xf2),
        // :57-59 — 0.7 × the block's own alpha. `--editorColor50` is
        // `rgba(255,255,255,0.5)`, so 128 × 0.7 = 89.6 → **90**, and the two
        // opacities compound to an effective 0.35. The light sheet's
        // `.namespace` is the same declaration over an opaque colour, so it
        // reads 255 × 0.7 = 178.5 → 179 there.
        "namespace" => Brush::rgba(0xff, 0xff, 0xff, 0x5a),
        // :61-66
        "property" | "tag" | "constant" | "symbol" => Brush::rgb(0xf9, 0x26, 0x72),
        // :68-71
        "boolean" | "number" => Brush::rgb(0xae, 0x81, 0xff),
        // :73-79
        "selector" | "attr-name" | "string" | "char" | "builtin" => Brush::rgb(0xa6, 0xe2, 0x2e),
        // :81-84
        "inserted" => Brush::rgb(0x22, 0x86, 0x3a),
        // :86-89
        "deleted" => Brush::rgb(0xb3, 0x1d, 0x28),
        // :91-97
        "operator" | "entity" | "url" => Brush::rgb(0xe6, 0x7e, 0x65),
        // :99-104
        "atrule" | "attr-value" | "function" | "class-name" => Brush::rgb(0xe6, 0xdb, 0x74),
        // :106-108
        "keyword" => Brush::rgb(0x66, 0xd9, 0xef),
        // :110-114
        "regex" | "important" | "variable" => Brush::rgb(0xfd, 0x97, 0x1f),
        // :116-122
        "bold" | "italic" => Brush::rgba(0xff, 0xff, 0xff, 0x80),
        other => panic!("no expectation transcribed for `.token.{other}`"),
    }
}

/// **The cross-check.** All 32 classes × both themes, against the stylesheets.
///
/// This asserts what a *renderer* would paint, not what the TOML holds: the
/// span goes through `CodeSpans`, the resolver, the shaper and `emit`, and the
/// number read back is a `GlyphRun::brush` out of the display list. A palette
/// field that never reached a glyph — which is what all 33 of them were until
/// this stage — would fail here rather than pass silently.
#[test]
fn every_token_class_paints_the_colour_its_stylesheet_gives_it() {
    for (theme, expected) in [
        (Theme::muya_default(), light_expected as fn(&str) -> Brush),
        (Theme::dark(), dark_expected as fn(&str) -> Brush),
    ] {
        for class in CodePalette::TOKEN_CLASSES {
            assert_eq!(
                emitted(&theme, class),
                expected(class),
                "{}: `.token.{class}` paints the wrong colour",
                theme.name
            );
        }
    }
}

/// `plain` is the block's own colour, so a span that resolves to it must push
/// **no run at all** — D15's second inherited contract.
///
/// There is no `plain` class to spell, so the case is made the way the browser
/// makes it: an *unstyled* class. A class no rule matches inherits, and what a
/// fence's text inherits is `.mu-code-block`'s colour.
#[test]
fn plain_is_the_block_default_and_pushes_no_run() {
    for theme in [Theme::muya_default(), Theme::dark()] {
        let bare = brushes(&one_token(&theme, None).0);
        let unstyled = brushes(&one_token(&theme, Some("not-a-prism-class")).0);
        assert_eq!(
            bare, unstyled,
            "{}: an unstyled class must lay out exactly as no span at all",
            theme.name
        );
        assert_eq!(bare.len(), 1, "{}: one run", theme.name);
        assert_eq!(
            bare[0],
            Brush::resolve(theme.code_palette.plain.color, Brush::default()),
            "{}: the fence default is the palette's `plain`",
            theme.name
        );
        assert_eq!(
            bare[0],
            Brush::resolve(theme.colors.editor_50, Brush::default()),
            "{}: …which is `blockSyntax.css:207`",
            theme.name
        );
    }
}

/// The two classes with a background are the only two, and the background is a
/// filled rect **behind** the glyphs rather than a colour on them.
///
/// `.token.inserted { background: #f0fff4 }` and `.token.deleted { background:
/// #ffeef0 }` are byte-identical in both sheets — the one place the Monokai
/// derivative did not recolour — which is itself worth asserting, because two
/// palettes that agree by accident and two that agree by transcription look the
/// same in a TOML diff.
#[test]
fn inserted_and_deleted_are_the_only_grounded_classes() {
    for theme in [Theme::muya_default(), Theme::dark()] {
        let grounded: Vec<&str> = CodePalette::TOKEN_CLASSES
            .into_iter()
            .filter(|c| {
                theme
                    .code_palette
                    .by_class(c)
                    .expect("every listed class resolves")
                    .background
                    != mt_layout::theme::Color::rgba(0, 0, 0, 0)
            })
            .collect();
        assert_eq!(grounded, ["inserted", "deleted"], "{}", theme.name);

        // The code block itself paints one ground; a grounded token adds a
        // second, and the second is the token's.
        let plain_rects = rect_brushes(&one_token(&theme, Some("keyword")).0);
        let inserted = rect_brushes(&one_token(&theme, Some("inserted")).0);
        let deleted = rect_brushes(&one_token(&theme, Some("deleted")).0);
        assert_eq!(inserted.len(), plain_rects.len() + 1, "{}", theme.name);
        assert_eq!(
            inserted.last().copied(),
            Some(Brush::rgb(0xf0, 0xff, 0xf4)),
            "{}: `.token.inserted`'s background",
            theme.name
        );
        assert_eq!(
            deleted.last().copied(),
            Some(Brush::rgb(0xff, 0xee, 0xf0)),
            "{}: `.token.deleted`'s background",
            theme.name
        );
    }
}

/// `.token.important` and `.token.bold` are `font-weight: bold`, and
/// `.token.italic` is `font-style: italic` — **the three classes that move a
/// glyph**.
///
/// Recorded as its own test because it is the one way a highlight span is not
/// colour-only: D15 says *"a highlight span changes colour, not metrics"* and
/// that is true of 29 of the 32 classes. Bold DejaVu Sans Mono is the same
/// advance as Book — a monospace face by definition — so the *width* does not
/// move; the face does, and a golden that showed one and not the other would be
/// wrong.
#[test]
fn three_classes_change_the_face_rather_than_the_colour() {
    for theme in [Theme::muya_default(), Theme::dark()] {
        let (plain, _f) = one_token(&theme, None);
        let plain_run = one_run(&plain);
        for (class, want_bold, want_italic) in [
            ("bold", true, false),
            ("important", true, false),
            ("italic", false, true),
        ] {
            let (list, _f) = one_token(&theme, Some(class));
            let run = one_run(&list);
            assert_ne!(
                run.font, plain_run.font,
                "{}: `.token.{class}` must resolve a different face",
                theme.name
            );
            assert_eq!(
                run.advance, plain_run.advance,
                "{}: `.token.{class}` is monospace, so the advance is unchanged",
                theme.name
            );
            // And the *block* does not move either. parley has no strut (S2's
            // second golden finding), so a line sized by a bold face could
            // legitimately differ from one sized by the Book face — DejaVu Sans
            // Mono's four styles share `hhea` ascent and descent, and this is
            // the assertion that says so rather than the assumption.
            assert_eq!(
                list.height, plain.height,
                "{}: `.token.{class}` must not move the block",
                theme.name
            );
            let style = theme
                .code_palette
                .by_class(class)
                .expect("a listed class resolves");
            assert_eq!((style.bold, style.italic), (want_bold, want_italic));
        }
    }
}

fn one_run(list: &DisplayList) -> mt_layout::GlyphRun {
    let runs: Vec<_> = list
        .blocks
        .iter()
        .flat_map(|b| b.items.iter())
        .filter_map(|i| match i {
            DisplayItem::Glyphs(run) => Some(run.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 1, "expected exactly one glyph run");
    runs.into_iter().next().expect("just checked")
}

// ---------------------------------------------------------------------------
// The two contracts the resolver inherits
// ---------------------------------------------------------------------------

/// Adjacent spans of the same class merge into one run, and a run identical to
/// the block default is never pushed.
///
/// Both matter for the same reason `style_runs` gives: a style-run boundary is
/// a shaping boundary, so a needless push costs a kerning pair — and
/// `mt_highlight::flatten` *deliberately* does not merge adjacent same-class
/// spans, so the fence-shaping side is where the merge has to happen.
#[test]
fn adjacent_spans_of_one_class_shape_as_one_run() {
    let theme = Theme::muya_default();
    let mut doc = Document::new();
    let first = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: first }]);
    let root = doc.root();
    doc.apply(&[Edit::InsertNode {
        parent: root,
        index: 0,
        block: Block::CodeBlock {
            kind: CodeKind::Fenced,
            fence_len: Some(3),
            info: "rust".to_string(),
            text: Text::from("abcdef"),
        },
    }]);
    let node = *doc.children(root).last().expect("just inserted");

    let lay = |spans: Vec<HighlightSpan>| {
        let mut code_spans = CodeSpans::new();
        code_spans.insert(node, spans);
        let options = LayoutOptions {
            code_spans,
            ..LayoutOptions::default()
        };
        let mut fonts = bundled_fonts();
        let mut shaper = TextShaper::new();
        let mut tree = LayoutTree::plan(&doc, &theme, f32::INFINITY, &options);
        tree.build_all(&doc, &mut fonts, &mut shaper);
        tree.place();
        let list = tree.emit(&fonts).expect("faces resolve");
        brushes(&list)
    };

    // Two touching `keyword` spans are one run, and byte-identical to one span
    // covering both.
    let split = lay(vec![
        HighlightSpan::new(0..3, "keyword"),
        HighlightSpan::new(3..6, "keyword"),
    ]);
    let whole = lay(vec![HighlightSpan::new(0..6, "keyword")]);
    assert_eq!(split, whole);
    assert_eq!(split.len(), 1, "one run, not two");

    // A span whose class the palette does not style pushes nothing, so the
    // fence shapes exactly as an unhighlighted one.
    let unstyled = lay(vec![HighlightSpan::new(0..6, "attr-equals")]);
    assert_eq!(unstyled, lay(Vec::new()));
    assert_eq!(unstyled.len(), 1);
}
