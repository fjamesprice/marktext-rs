//! E1 — does parley place an inline box by the bidi visual order of the run it
//! lands in? This is the **parley 0.11.0** arm; `e1-parley-main` is the same
//! file against a pinned git `main`, and the diff between the two files is
//! itself an answer (secondary question 2, "breaking churn").
//!
//! ## Why the corpus needs doctoring, and how
//!
//! `bench/corpus/rtl.md` has bold, emphasis, code spans and a link inside RTL
//! runs, and **no inline image** — so there is no inline box in it to measure.
//! The box is therefore *synthesized*: the harness picks a byte offset that is
//! strictly inside a word of a known script run and pushes an `InlineBox`
//! there. That is exactly what `mt-layout` will do for an inline image, inline
//! code chip or inline math box, so the synthesis is faithful; but it means
//! this experiment measures parley's behaviour for a box the *author* did not
//! place, and the offsets below are the harness's choice, not the corpus's.
//!
//! Markdown syntax (`**`, `*`) is stripped before layout, because `mt-layout`
//! is handed inline text after `mt-md` has consumed the markers.
//!
//! ## Why the expectations are not hand-written
//!
//! Grading parley against a hand-written "the box should be here" would only
//! prove the author's grasp of UAX #9. Instead the harness inserts U+FFFC
//! OBJECT REPLACEMENT CHARACTER at the same byte offset, runs `unicode-bidi`
//! over that string, and reads the *expected* visual neighbours of the box off
//! an independent UAX #9 implementation. U+FFFC is the right stand-in by
//! construction: it is what CSS and parley's own source say an inline box
//! should be treated as — see the TODO at `parley/src/shape/mod.rs`, which
//! admits the box currently takes "the bidi level of the previous run" instead.

use std::ops::Range;

use parley::{
    Alignment, AlignmentOptions, FontContext, InlineBox, InlineBoxKind, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use unicode_bidi::BidiInfo;

/// Printed into the dump so a pasted result can never be attributed to the
/// wrong arm of the experiment.
const ARM: &str = "parley 0.11.0 (crates.io, `parley = \"=0.11.0\"`)";

const FONT_SIZE: f32 = 16.0;
const BOX_W: f32 = 40.0;
const BOX_H: f32 = 24.0;
/// U+FFFC, the object replacement character; the bidi stand-in for the box.
const OBJ: char = '\u{FFFC}';

fn main() {
    println!("=== E1 bidi/inline-box harness — arm: {ARM} ===");
    println!("corpus: bench/corpus/rtl.md (unmodified; markdown markers stripped at layout time)");
    println!(
        "box: width {BOX_W}, height {BOX_H}, kind InFlow; font size {FONT_SIZE}; single line (max_advance = None)"
    );
    println!();

    let corpus = corpus();
    let cases = cases(&corpus);

    let mut font_cx = FontContext::new();
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();

    let mut verdicts = Vec::new();
    for case in &cases {
        let pass = run_case(&mut font_cx, &mut layout_cx, case);
        verdicts.push((case.name, pass));
    }

    baseline_probe(&mut font_cx, &mut layout_cx);
    joining_probe(&mut font_cx, &mut layout_cx);

    println!();
    println!("=== SUMMARY — arm: {ARM} ===");
    for (name, pass) in &verdicts {
        println!("  {:<34} {}", name, if *pass { "PASS" } else { "FAIL" });
    }
    let failed = verdicts.iter().filter(|(_, p)| !p).count();
    println!(
        "  {} of {} cases place the box in the bidi-correct visual slot.",
        verdicts.len() - failed,
        verdicts.len()
    );
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

fn corpus() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bench")
        .join("corpus")
        .join("rtl.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read corpus at {}: {e}", path.display()))
}

/// One-based line lookup with an anchor assertion, so that an edit to the
/// corpus fails the harness loudly instead of silently measuring other text.
fn corpus_line(corpus: &str, n: usize, anchor: &str) -> String {
    let line = corpus
        .lines()
        .nth(n - 1)
        .unwrap_or_else(|| panic!("corpus has no line {n}"));
    assert!(
        line.contains(anchor),
        "corpus line {n} no longer contains {anchor:?}; found {line:?}"
    );
    line.replace("**", "").replace('*', "").trim().to_string()
}

struct Case {
    name: &'static str,
    /// What the reader needs to know that the numbers do not say.
    note: &'static str,
    text: String,
    /// Byte offset at which the synthesized `InlineBox` is pushed.
    box_at: usize,
    /// Human description of the offset, e.g. `inside "مرحبا" after 3 chars`.
    box_at_desc: String,
}

/// Byte offset `chars_in` characters into the first occurrence of `word`.
/// Char-boundary-safe by construction, which `InlineBox::index` requires.
fn inside(text: &str, word: &str, chars_in: usize) -> usize {
    let start = text
        .find(word)
        .unwrap_or_else(|| panic!("{word:?} not found in {text:?}"));
    let off: usize = word.chars().take(chars_in).map(char::len_utf8).sum();
    start + off
}

fn case(name: &'static str, note: &'static str, text: String, word: &str, chars_in: usize) -> Case {
    let box_at = inside(&text, word, chars_in);
    let where_ = if chars_in == 0 {
        format!("at the leading edge of {word:?}")
    } else {
        format!("inside {word:?}, {chars_in} char(s) in")
    };
    Case {
        name,
        note,
        box_at,
        box_at_desc: format!("byte {box_at} — {where_}"),
        text,
    }
}

/// A case whose offset is a raw byte index rather than a position within a
/// word — used for the paragraph-edge cases, where there is no word to be
/// inside of.
fn case_at(
    name: &'static str,
    note: &'static str,
    text: String,
    box_at: usize,
    where_: &str,
) -> Case {
    assert!(
        text.is_char_boundary(box_at),
        "offset {box_at} splits a code point"
    );
    Case {
        name,
        note,
        box_at,
        box_at_desc: format!("byte {box_at} — {where_}"),
        text,
    }
}

fn cases(corpus: &str) -> Vec<Case> {
    // L24: the "Mixed in one paragraph" opening line — an LTR sentence that
    // contains an Arabic run and a Hebrew run. Only this source line is used;
    // the paragraph soft-wraps over three, and one line keeps the layout to a
    // single line so x-coordinates are directly comparable.
    let mixed = corpus_line(corpus, 24, "An English sentence");
    // L10: an Arabic paragraph with an English word in the middle.
    let arabic = corpus_line(corpus, 10, "Rust");
    // L18: a Hebrew paragraph with an English word in the middle.
    let hebrew = corpus_line(corpus, 18, "parley");
    // L26: pure LTR, the control.
    let ltr = corpus_line(corpus, 26, "caret motion");

    // Copies for the embedding-boundary cases at the end of the list; the
    // in-word cases below consume the originals.
    let (mixed2, arabic2, ltr2) = (mixed.clone(), arabic.clone(), ltr.clone());

    vec![
        case(
            "c-control-pure-ltr",
            "Control. Base LTR, no RTL anywhere. Whatever this looks like is what \
             'placed correctly' means when bidi is not involved.",
            ltr,
            "caret",
            3,
        ),
        case(
            "a1-ltr-para-box-in-arabic",
            "LTR paragraph, box inside the Arabic run. The box must land between two \
             Arabic letters, i.e. INSIDE the RTL run's visual extent.",
            mixed.clone(),
            "مرحبا",
            3,
        ),
        case(
            "a2-ltr-para-box-in-hebrew",
            "Same LTR paragraph, box inside the Hebrew run.",
            mixed,
            "שלום",
            2,
        ),
        case(
            "b1-arabic-para-box-in-arabic",
            "RTL (Arabic) base paragraph, box inside Arabic text.",
            arabic.clone(),
            "المنتصف",
            3,
        ),
        case(
            "b2-arabic-para-box-in-english",
            "RTL (Arabic) base paragraph, box inside the embedded English word — an \
             LTR island inside an RTL paragraph, the nested case M3-R1 names.",
            arabic,
            "Rust",
            2,
        ),
        case(
            "d1-hebrew-para-box-in-hebrew",
            "RTL (Hebrew) base paragraph, box inside Hebrew text.",
            hebrew.clone(),
            "בעברית",
            3,
        ),
        case(
            "d2-hebrew-para-box-in-english",
            "RTL (Hebrew) base paragraph, box inside the embedded English word.",
            hebrew,
            "parley",
            3,
        ),
        // ---- embedding-boundary cases -------------------------------------
        //
        // The cases above all put the box strictly *inside* a word, where the
        // run parley splits at the box index has the same bidi level on both
        // sides. `parley/src/shape/mod.rs` gives the box "the bidi level of
        // the previous run" and carries a TODO admitting it should instead run
        // the box through bidi analysis as U+FFFC. Those two rules only differ
        // where the box sits at a level boundary — which is precisely where a
        // MarkText inline image lands when a paragraph opens with one, or when
        // one separates an Arabic phrase from an English one.
        case_at(
            "e1-rtl-para-box-at-paragraph-start",
            "An inline image as the FIRST thing in an Arabic paragraph. There is no \
             preceding run for the box to inherit a level from. UAX #9 gives U+FFFC \
             the paragraph level here (sos = R), so it belongs at the far RIGHT.",
            arabic2.clone(),
            0,
            "before the first letter of an RTL paragraph",
        ),
        case_at(
            "e2-rtl-para-box-at-paragraph-end",
            "The mirror image: an inline image as the last thing in an Arabic paragraph.",
            arabic2.clone(),
            arabic2.len(),
            "after the final character of an RTL paragraph",
        ),
        case(
            "e3-ltr-para-box-at-ltr-to-rtl-boundary",
            "LTR paragraph, box exactly where the Arabic run begins — an image between \
             an English word and an Arabic one.",
            mixed2.clone(),
            "مرحبا",
            0,
        ),
        case(
            "e4-rtl-para-box-at-rtl-to-ltr-boundary",
            "RTL paragraph, box exactly where the embedded English word begins — the \
             level-1 to level-2 boundary.",
            arabic2,
            "Rust",
            0,
        ),
        case_at(
            "e5-ltr-para-box-at-paragraph-start",
            "Control for e1: the same paragraph-leading box, but LTR throughout.",
            ltr2,
            0,
            "before the first letter of an LTR paragraph",
        ),
    ]
}

// ---------------------------------------------------------------------------
// Ground truth (independent UAX #9)
// ---------------------------------------------------------------------------

struct Expected {
    para_level: u8,
    /// Level `unicode-bidi` resolves for the U+FFFC standing in for the box.
    obj_level: u8,
    left: Option<char>,
    right: Option<char>,
    /// Byte index -> resolved level, over the text *with* U+FFFC inserted.
    levels: Vec<u8>,
    /// Byte indices of the with-U+FFFC text, in visual (left-to-right) order.
    visual: Vec<usize>,
    with_obj: String,
}

impl Expected {
    /// Map a byte offset in the original text to the with-U+FFFC text.
    fn map(&self, b: usize, box_at: usize) -> usize {
        if b < box_at { b } else { b + OBJ.len_utf8() }
    }
    fn level_at(&self, b: usize, box_at: usize) -> u8 {
        let i = self.map(b, box_at);
        *self.levels.get(i).unwrap_or(&self.para_level)
    }
}

fn expected(text: &str, box_at: usize) -> Expected {
    let mut with_obj = String::with_capacity(text.len() + OBJ.len_utf8());
    with_obj.push_str(&text[..box_at]);
    with_obj.push(OBJ);
    with_obj.push_str(&text[box_at..]);

    let bidi = BidiInfo::new(&with_obj, None);
    let para = bidi
        .paragraphs
        .first()
        .expect("bidi found no paragraph")
        .clone();
    let levels: Vec<u8> = bidi.levels.iter().map(|l| l.number()).collect();

    // Visual order: `visual_runs` returns level runs already ordered
    // left-to-right; within an odd-level run the characters run right-to-left.
    let (run_levels, runs) = bidi.visual_runs(&para, para.range.clone());
    let mut visual: Vec<usize> = Vec::with_capacity(with_obj.len());
    for run in &runs {
        let idxs: Vec<usize> = with_obj[run.clone()]
            .char_indices()
            .map(|(i, _)| run.start + i)
            .collect();
        if run_levels[run.start].is_rtl() {
            visual.extend(idxs.into_iter().rev());
        } else {
            visual.extend(idxs);
        }
    }

    let pos = visual
        .iter()
        .position(|&b| b == box_at)
        .expect("U+FFFC not found in the visual order");
    let at = |i: usize| with_obj[visual[i]..].chars().next().unwrap();
    let left = visual[..pos]
        .iter()
        .rev()
        .map(|&b| with_obj[b..].chars().next().unwrap())
        .find(|c| !c.is_whitespace());
    let _ = at;
    let right = visual[pos + 1..]
        .iter()
        .map(|&b| with_obj[b..].chars().next().unwrap())
        .find(|c| !c.is_whitespace());

    Expected {
        para_level: para.level.number(),
        obj_level: levels[box_at],
        left,
        right,
        levels,
        visual,
        with_obj,
    }
}

// ---------------------------------------------------------------------------
// parley side
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Kind {
    Run {
        bytes: Range<usize>,
        rtl: bool,
        glyphs: usize,
        notdef: usize,
        baseline: f32,
    },
    Box {
        id: u64,
        y: f32,
        h: f32,
        /// `PositionedInlineBox::baseline` — absent before PR #639.
        baseline: Option<f32>,
    },
}

#[derive(Debug)]
struct Item {
    x0: f32,
    x1: f32,
    kind: Kind,
}

fn run_case(font_cx: &mut FontContext, layout_cx: &mut LayoutContext<()>, case: &Case) -> bool {
    println!("---------------------------------------------------------------");
    println!("CASE {}", case.name);
    println!("  note: {}", case.note);
    println!("  text ({} bytes): {:?}", case.text.len(), case.text);
    println!("  box at: {}", case.box_at_desc);

    let exp = expected(&case.text, case.box_at);

    let mut builder = layout_cx.ranged_builder(font_cx, &case.text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(FONT_SIZE));
    builder.push_inline_box(InlineBox {
        id: 1,
        kind: InlineBoxKind::InFlow,
        index: case.box_at,
        width: BOX_W,
        height: BOX_H,
    });
    let mut layout: Layout<()> = builder.build(&case.text);
    // `None` == infinite: keep everything on one line so the x axis alone
    // decides the question.
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    println!(
        "  layout: width {:.2}, height {:.2}, {} line(s)",
        layout.width(),
        layout.height(),
        layout.len()
    );
    println!(
        "  unicode-bidi: paragraph level {} ({}), level resolved for U+FFFC at the box offset: {}",
        exp.para_level,
        if exp.para_level % 2 == 1 {
            "RTL"
        } else {
            "LTR"
        },
        exp.obj_level
    );

    let mut items: Vec<Item> = Vec::new();
    for (li, line) in layout.lines().enumerate() {
        let m = line.metrics();
        println!(
            "  line {li}: baseline {:.2}, advance {:.2}, text_range {:?}",
            m.baseline,
            m.advance,
            line.text_range()
        );
        for item in line.items() {
            match item {
                PositionedLayoutItem::GlyphRun(gr) => {
                    let run = gr.run();
                    let glyphs: Vec<_> = gr.glyphs().collect();
                    let notdef = glyphs.iter().filter(|g| g.id == 0).count();
                    items.push(Item {
                        x0: gr.offset(),
                        x1: gr.offset() + gr.advance(),
                        kind: Kind::Run {
                            bytes: run.text_range(),
                            rtl: run.is_rtl(),
                            glyphs: glyphs.len(),
                            notdef,
                            baseline: gr.baseline(),
                        },
                    });
                }
                PositionedLayoutItem::InlineBox(b) => {
                    items.push(Item {
                        x0: b.x,
                        x1: b.x + b.width,
                        kind: Kind::Box {
                            id: b.id,
                            y: b.y,
                            h: b.height,
                            // parley 0.11.0's `PositionedInlineBox` has no
                            // `baseline` field; PR #639 is unreleased.
                            baseline: None,
                        },
                    });
                }
            }
        }
    }

    // `Line::items()` already yields visual order; sorting by x makes that
    // claim checkable rather than assumed.
    items.sort_by(|a, b| a.x0.total_cmp(&b.x0));

    println!("  items, left to right:");
    println!(
        "    {:>8} {:>8}  {:<6} {:<12} {:>4} {:>7} {:>4}  text / box",
        "x0", "x1", "kind", "bytes", "rtl", "lvl(ub)", "tofu",
    );
    for it in &items {
        match &it.kind {
            Kind::Run {
                bytes,
                rtl,
                glyphs,
                notdef,
                baseline,
            } => {
                let lvl = exp.level_at(bytes.start, case.box_at);
                println!(
                    "    {:>8.2} {:>8.2}  {:<6} {:<12} {:>4} {:>7} {:>4}  {:?} ({} glyphs, baseline {:.2})",
                    it.x0,
                    it.x1,
                    "run",
                    format!("{}..{}", bytes.start, bytes.end),
                    rtl,
                    lvl,
                    notdef,
                    &case.text[bytes.clone()],
                    glyphs,
                    baseline
                );
            }
            Kind::Box { id, y, h, baseline } => {
                println!(
                    "    {:>8.2} {:>8.2}  {:<6} {:<12} {:>4} {:>7} {:>4}  <INLINE BOX id={} y={:.2} h={:.2} baseline={:?}>",
                    it.x0,
                    it.x1,
                    "BOX",
                    format!("@{}", case.box_at),
                    "-",
                    exp.obj_level,
                    "-",
                    id,
                    y,
                    h,
                    baseline
                );
            }
        }
    }

    // ---- verdict -------------------------------------------------------
    let bi = items
        .iter()
        .position(|i| matches!(i.kind, Kind::Box { .. }))
        .expect("parley emitted no inline box");

    let actual_left = items[..bi]
        .iter()
        .rev()
        .find_map(|i| edge_char(&case.text, i, false));
    let actual_right = items[bi + 1..]
        .iter()
        .find_map(|i| edge_char(&case.text, i, true));

    let pass = actual_left == exp.left && actual_right == exp.right;

    println!("  expected (unicode-bidi, U+FFFC stand-in):");
    println!("    left neighbour  {}", show(exp.left));
    println!("    right neighbour {}", show(exp.right));
    println!("  actual (parley):");
    println!("    left neighbour  {}", show(actual_left));
    println!("    right neighbour {}", show(actual_right));
    println!("  VERDICT: {}", if pass { "PASS" } else { "FAIL" });
    if !pass {
        println!(
            "    expected visual order around the box: ...{}...",
            visual_window(&exp, case.box_at)
        );
    }
    println!();
    pass
}

/// The first non-whitespace character at a run's visual left (`from_left`) or
/// visual right edge. For an RTL run the visual left edge is the *logical end*.
fn edge_char(text: &str, item: &Item, from_left: bool) -> Option<char> {
    let Kind::Run { bytes, rtl, .. } = &item.kind else {
        return None;
    };
    let s = &text[bytes.clone()];
    let scan_logical_forward = if *rtl { !from_left } else { from_left };
    if scan_logical_forward {
        s.chars().find(|c| !c.is_whitespace())
    } else {
        s.chars().rev().find(|c| !c.is_whitespace())
    }
}

fn show(c: Option<char>) -> String {
    match c {
        Some(c) => format!("{c:?} (U+{:04X})", c as u32),
        None => "<line edge>".to_string(),
    }
}

/// Ten visual characters either side of the box, as `unicode-bidi` orders them.
fn visual_window(exp: &Expected, box_at: usize) -> String {
    let pos = exp.visual.iter().position(|&b| b == box_at).unwrap();
    let lo = pos.saturating_sub(10);
    let hi = (pos + 11).min(exp.visual.len());
    exp.visual[lo..hi]
        .iter()
        .map(|&b| {
            let c = exp.with_obj[b..].chars().next().unwrap();
            if c == OBJ { '\u{2588}' } else { c }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Extra probe: does splitting a run at an inline box break cursive joining?
// ---------------------------------------------------------------------------
//
// This is not one of the questions E1 was sent to answer; it fell out of the
// numbers. Placing the box *inside* an Arabic word made the whole line's width
// change on 0.11.0 — which can only happen if the shaper produced different
// glyphs. Arabic is a joining script: a letter takes an initial/medial/final/
// isolated form depending on its neighbours, so a run boundary in the middle of
// a word is not a cosmetic split. An inline image inside an Arabic word is a
// real MarkText document, so the question is worth the twenty lines.

fn joining_probe(font_cx: &mut FontContext, layout_cx: &mut LayoutContext<()>) {
    println!("---------------------------------------------------------------");
    println!("PROBE: does an inline box break cursive joining? — arm {ARM}");
    // Arabic (joining) and Hebrew (non-joining) side by side; only the first
    // should be able to change.
    for (label, word, split_at) in [
        ("arabic  ", "مرحبا بالعالم", 6usize),
        ("hebrew  ", "שלום עולם", 4usize),
        ("latin   ", "handwriting", 4usize),
    ] {
        let (no_box, w0) = shape(font_cx, layout_cx, word, None);
        let (with_box, w1) = shape(font_cx, layout_cx, word, Some(split_at));
        let mut a = no_box.clone();
        let mut b = with_box.clone();
        a.sort_unstable();
        b.sort_unstable();
        println!(
            "  {label} {word:?} split at byte {split_at}: glyph ids {}  (text advance {:.2} -> {:.2}, delta {:+.2})",
            if a == b { "UNCHANGED" } else { "CHANGED  " },
            w0,
            w1,
            w1 - w0
        );
        if a != b {
            println!("      without box: {no_box:?}");
            println!("      with box   : {with_box:?}");
        }
    }
    println!();
}

/// Glyph ids for `text`, optionally with an inline box pushed at `split_at`,
/// plus the total advance of the *text* only (the box's own width removed).
fn shape(
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<()>,
    text: &str,
    split_at: Option<usize>,
) -> (Vec<u32>, f32) {
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(FONT_SIZE));
    if let Some(index) = split_at {
        builder.push_inline_box(InlineBox {
            id: 99,
            kind: InlineBoxKind::InFlow,
            index,
            width: BOX_W,
            height: BOX_H,
        });
    }
    let mut layout: Layout<()> = builder.build(text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    // Collected per run and ordered by logical byte offset, so the comparison
    // is not confounded by visual reordering.
    let mut runs: Vec<(usize, Vec<u32>)> = Vec::new();
    let mut advance = 0.0;
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(gr) = item {
                advance += gr.advance();
                runs.push((
                    gr.run().text_range().start,
                    gr.glyphs().map(|g| g.id).collect(),
                ));
            }
        }
    }
    runs.sort_by_key(|(start, _)| *start);
    (runs.into_iter().flat_map(|(_, g)| g).collect(), advance)
}

// ---------------------------------------------------------------------------
// Secondary question 1: PR #639 (`baseline` on inline boxes)
// ---------------------------------------------------------------------------

fn baseline_probe(font_cx: &mut FontContext, layout_cx: &mut LayoutContext<()>) {
    println!("---------------------------------------------------------------");
    println!("PROBE: inline-box baseline (PR #639) — arm {ARM}");
    const TEXT: &str = "Ax box Ay";
    let mut builder = layout_cx.ranged_builder(font_cx, TEXT, 1.0, true);
    builder.push_default(StyleProperty::FontSize(FONT_SIZE));
    // parley 0.11.0's `InlineBox` has no `baseline` field. PR #639 merged
    // 2026-07-08, after 0.11.0 was cut, so there is nothing to set here and
    // the box's bottom edge is pinned to the text baseline unconditionally.
    builder.push_inline_box(InlineBox {
        id: 7,
        kind: InlineBoxKind::InFlow,
        index: 3,
        width: BOX_W,
        height: BOX_H,
    });
    let mut layout: Layout<()> = builder.build(TEXT);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    println!("  InlineBox::baseline           : ABSENT (field does not exist in 0.11.0)");
    println!("  PositionedInlineBox::baseline : ABSENT (field does not exist in 0.11.0)");
    println!("  Layout::first_baseline()      : ABSENT (method does not exist in 0.11.0)");
    println!("  Layout::last_baseline()       : ABSENT (method does not exist in 0.11.0)");
    for line in layout.lines() {
        println!(
            "  line metrics baseline         : {:.4}",
            line.metrics().baseline
        );
        for item in line.items() {
            if let PositionedLayoutItem::InlineBox(b) = item {
                println!(
                    "  box: x {:.4} y {:.4} w {:.4} h {:.4}  (bottom edge y+h = {:.4})",
                    b.x,
                    b.y,
                    b.width,
                    b.height,
                    b.y + b.height
                );
            }
        }
    }
    println!();
}
