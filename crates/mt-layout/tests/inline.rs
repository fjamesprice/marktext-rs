//! S2's gate, and the part of it that is not self-referential.
//!
//! §7 grades the layout goldens *"strong but self-referential: it proves
//! nothing changed, not that anything is right"*, and §6's S2 gate answers
//! that with four words — **asserted, not eyeballed**. A test that recomputes
//! its expectation from the code path under test satisfies neither. So every
//! number in this file comes from somewhere other than parley:
//!
//! | § | What is asserted | Where the expectation comes from |
//! |---|---|---|
//! | Bidi | the **visual** left-to-right order of the emitted runs | `unicode-bidi`, an independent UAX #9 implementation, with U+FFFC standing in for an inline box — S0's E1 method, and the experiment D2's pin rests on |
//! | CJK | the break positions of `cjk.md`'s long unspaced line | UAX #14: a break is permitted between two ideographs (no rule forbids `ID ÷ ID`) and forbidden before `。` (LB13, `× CL`) |
//! | Emoji | grapheme-cluster counts for ZWJ sequences and skin-tone modifiers | UAX #29 § 3, and `bench/corpus/emoji.md`'s own table, whose third column already carries the codepoint counts |
//! | Ligatures | that `fi`/`fl`/`ff` are one glyph over two graphemes, and that the corpus's Arabic is **not** ligated | the faces' own `GSUB`, read through cluster boundaries rather than through a glyph id |
//! | The map | D13's five invariants over 2 210 leaves of the corpus | the token ranges the walk carried, which is the point of `MapRun::token` |
//!
//! Two of those answers are findings rather than confirmations and are stated
//! where they are measured: the Arabic lam-alef is a *required* ligature that
//! Noto Sans Arabic — the face the corpus's Arabic resolves to — does not
//! form, and `MapKind::Substituted` is unreachable from the corpus at an LTR
//! base.
//!
//! # Why this directory
//!
//! It reads `bench/corpus/`. `cargo xtask deps` forbids `std::fs::` anywhere
//! under `crates/mt-layout/src/` — including inside a `#[cfg(test)]` module —
//! and its own error message sanctions this directory as the way out.
//!
//! # The one dependency, and why it is dev-only
//!
//! `unicode-bidi` is the only third-party crate this crate's tests reach for,
//! and it is here for the single reason that an oracle's whole value is
//! independence. Expectations transcribed from parley's output would make the
//! test agree with itself, which is the failure §7 names. It is a
//! `[dev-dependencies]` entry, so it is in no shipped binary and changes
//! nothing about §1's headless rules.

use std::path::{Path, PathBuf};

use mt_doc::{Block, Document, Edit, Text};
use mt_layout::display::{BlockKind, DisplayItem, DisplayList};
use mt_layout::fonts::{Face, FaceList, Fonts};
use mt_layout::inline::{self, InlineSyntax, InlineText, MapKind};
use mt_layout::layout;
use mt_layout::text::{BaseDirection, InlineBoxSpec, ShapedText, TextRequest, TextShaper};
use mt_layout::theme::Theme;
use unicode_bidi::{BidiInfo, Level};

// ---------------------------------------------------------------------------
// The committed face set, and the corpus
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

/// The committed twelve, registered and wired exactly as a shell must do it.
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

fn corpus(name: &str) -> String {
    let path = repo_root().join("bench/corpus").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// One corpus line by **one-based** number, with an anchor assertion.
///
/// E1's rule, kept: pulling a line out by number is only safe if an edit to the
/// corpus fails the harness loudly instead of silently measuring different
/// text. `contains` rather than equality so that a purely cosmetic edit
/// elsewhere on the line is not a false alarm.
fn corpus_line(file: &str, number: usize, anchor: &str) -> String {
    let src = corpus(file);
    let line = src
        .lines()
        .nth(number - 1)
        .unwrap_or_else(|| panic!("{file} has no line {number}"))
        .to_string();
    assert!(
        line.contains(anchor),
        "{file}:{number} no longer contains {anchor:?} — it is now {line:?}. \
         Re-anchor this case rather than deleting it."
    );
    line
}

// ---------------------------------------------------------------------------
// Two ways to lay one leaf's text out
// ---------------------------------------------------------------------------

/// The whole pipeline: a one-paragraph document through `layout`, at the
/// theme's own column. What the goldens serialize.
fn one_paragraph(text: &str, theme: &Theme) -> DisplayList {
    let mut doc = Document::new();
    let first = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: first }]);
    let root = doc.root();
    doc.apply(&[Edit::InsertNode {
        parent: root,
        index: 0,
        block: Block::Paragraph {
            text: Text::from(text),
        },
    }]);
    let mut fonts = bundled_fonts();
    layout(&doc, theme, f32::INFINITY, &mut fonts).expect("the committed faces resolve")
}

/// Byte offset of every line start in a paragraph's display list, first
/// included, in top-to-bottom order.
///
/// `ShapedText::break_offsets` answers this directly and is not reachable
/// through `LayoutTree`, so it is recovered from the public display list —
/// which has the side benefit that the assertion is about the artifact a
/// renderer and a golden both consume, not about an internal.
fn line_starts(list: &DisplayList) -> Vec<usize> {
    let block = list
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::Paragraph)
        .expect("a paragraph");
    let mut lines: Vec<(f32, usize)> = Vec::new();
    for item in &block.items {
        let DisplayItem::Glyphs(g) = item else {
            continue;
        };
        match lines
            .iter_mut()
            .find(|(y, _)| (*y - g.baseline).abs() < 0.01)
        {
            Some((_, start)) => *start = (*start).min(g.text_range.start),
            None => lines.push((g.baseline, g.text_range.start)),
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.into_iter().map(|(_, start)| start).collect()
}

/// Tokenize one source line and shape its visible text as **a single line**,
/// with a box reserved for every inline image.
///
/// Single line, at infinite width, for E1's reason: the question a bidi
/// assertion asks is which visual *slot* each run landed in, and a line break
/// mixed into that would make a failure ambiguous between the two. The boxes
/// are 40 × 24 for the same reason and are E1's own numbers — a box's size
/// cannot move it between slots, only along the axis it already sits on, so
/// borrowing D12's theme-dependent fail-box geometry would add a variable
/// without adding a question.
fn shape_one_line(
    source: &str,
    base: BaseDirection,
    theme: &Theme,
) -> (InlineText, Vec<DisplayItem>) {
    let laid = inline::lay_out(source, &InlineSyntax::default(), base, None);
    let boxes: Vec<InlineBoxSpec> = laid
        .images
        .iter()
        .enumerate()
        .map(|(i, image)| InlineBoxSpec {
            id: i as u64,
            index: image.visible_index,
            width: 40.0,
            height: 24.0,
            baseline: None,
        })
        .collect();
    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    let mut request = TextRequest::new(&laid.visible, &theme.fonts.body, 16.0, 1.6);
    request.base_direction = base;
    request.inline_boxes = &boxes;
    let shaped = shaper.shape(&mut fonts, &request);
    let mut items = Vec::new();
    shaped
        .emit(&fonts, 0.0, 0.0, &mut items)
        .expect("the committed faces resolve");
    (laid, items)
}

/// Shape a string as one line at the body stack and hand back the layout, for
/// the clauses that ask it about clusters rather than about positions.
///
/// The collection comes back with it. Nothing here calls `emit`, so nothing
/// needs to resolve a handle — but `tests/layout.rs` records what happens when
/// a helper drops its `Fonts` on the floor, and a cluster query is one method
/// away from needing the same thing.
fn shape_for_clusters(text: &str, theme: &Theme) -> (Fonts, ShapedText) {
    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    let request = TextRequest::new(text, &theme.fonts.body, 16.0, 1.6);
    let shaped = shaper.shape(&mut fonts, &request);
    (fonts, shaped)
}

// ---------------------------------------------------------------------------
// 1. Bidi visual order, graded against `unicode-bidi`
// ---------------------------------------------------------------------------

/// The visual left-to-right character sequence **`unicode-bidi` says** this
/// text should have, with U+FFFC standing in for a box at each of `boxes`.
///
/// U+FFFC is the correct stand-in by construction: it is what CSS specifies
/// for a replaced inline element, and it is what parley's own TODO
/// (`shape/mod.rs:204-208`) says an inline box *should* be analysed as. E1
/// used it for the same reason and this is the same code path's expectation,
/// widened from *"the box's two visual neighbours"* to the whole line.
///
/// An RTL run's characters are emitted in reverse, which is what "visual
/// order" means for it. The same operation is applied to parley's side in
/// [`visual_order_emitted`], so the two strings are comparable character for
/// character.
fn visual_order_expected(text: &str, base: BaseDirection, boxes: &[usize]) -> String {
    let mut with_boxes = String::with_capacity(text.len() + boxes.len() * 3);
    let mut at = 0;
    for &index in boxes {
        with_boxes.push_str(&text[at..index]);
        with_boxes.push('\u{fffc}');
        at = index;
    }
    with_boxes.push_str(&text[at..]);

    let level = match base {
        BaseDirection::Ltr => Level::ltr(),
        BaseDirection::Rtl => Level::rtl(),
        // `Auto` is first-strong detection, which is a *different* question
        // and is not what any paragraph in this crate is laid out at.
        BaseDirection::Auto => panic!("the oracle grades an explicit base direction"),
    };
    let info = BidiInfo::new(&with_boxes, Some(level));
    assert_eq!(
        info.paragraphs.len(),
        1,
        "a case is one paragraph, so that reordering is line-independent"
    );
    let para = &info.paragraphs[0];
    let (levels, runs) = info.visual_runs(para, para.range.clone());
    let mut out = String::with_capacity(with_boxes.len());
    for run in runs {
        let slice = &with_boxes[run.clone()];
        if levels[run.start].is_rtl() {
            out.extend(slice.chars().rev());
        } else {
            out.push_str(slice);
        }
    }
    out
}

/// The same sequence, read off the **display list** parley produced.
///
/// Items are sorted by x, which is the only definition of "visual order" the
/// neutral list carries — `GlyphRun` has an `offset` and an `is_rtl` and no
/// notion of a bidi level, exactly as E1 recorded (`Run::is_rtl()` is parley's
/// only public direction reader).
fn visual_order_emitted(visible: &str, items: &[DisplayItem]) -> String {
    let mut sorted: Vec<(f32, String)> = Vec::new();
    for item in items {
        match item {
            DisplayItem::Glyphs(run) => {
                let slice = &visible[run.text_range.clone()];
                let text = if run.is_rtl {
                    slice.chars().rev().collect()
                } else {
                    slice.to_string()
                };
                sorted.push((run.offset, text));
            }
            DisplayItem::InlineBox(b) => sorted.push((b.x, "\u{fffc}".to_string())),
            // A ground, an underline or a strikethrough is painted *behind* or
            // *through* the run it belongs to and adds no character.
            DisplayItem::Rect(_) | DisplayItem::Line(_) => {}
        }
    }
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    sorted.into_iter().map(|(_, text)| text).collect()
}

/// One graded case.
struct BidiCase {
    name: &'static str,
    line: usize,
    anchor: &'static str,
    base: BaseDirection,
    /// What makes the case worth having, asserted rather than described: the
    /// resolved level `unicode-bidi` gives this character's first occurrence.
    level_probe: Option<(&'static str, u8)>,
}

/// Six cases from `rtl.md`, every one of them graded character-for-character
/// against `unicode-bidi`.
///
/// The coverage §6 asks for: the LTR-base paragraph that mixes Arabic *and*
/// Hebrew; a level-2 LTR island inside an RTL run, twice (Arabic and Hebrew
/// paragraphs, which is where level 2 actually occurs — in an LTR-base
/// paragraph a Latin word inside Arabic stays at level 0, so the island the
/// gate names is only reachable at an RTL base); and the inline image the
/// corpus gained at S2 open, at both base directions.
///
/// **The RTL-base cases are not dead weight.** `mt-layout` lays every
/// paragraph out at `Ltr` today for the reference-parity reason
/// `TextRequest::new` argues, so `BaseDirection::Rtl` is M5's `dir` preference
/// arriving early — and it is the *only* condition under which M3-R1's
/// workaround fires. A workaround nothing exercises is indistinguishable from
/// a workaround that does not work, which is the argument that put the image
/// in the corpus in the first place.
#[test]
fn every_bidi_case_in_the_corpus_matches_an_independent_uax9_implementation() {
    let theme = Theme::muya_default();
    let cases = [
        BidiCase {
            name: "mixed-ltr-base",
            line: 29,
            anchor: "An English sentence",
            base: BaseDirection::Ltr,
            // Arabic in an LTR paragraph: R at an even embedding level goes up
            // one, never two (UAX #9 I1).
            level_probe: Some(("مرحبا", 1)),
        },
        BidiCase {
            name: "arabic-para-ltr-base",
            line: 10,
            anchor: "Rust",
            base: BaseDirection::Ltr,
            // The Latin island in an LTR-base paragraph is *not* level 2.
            level_probe: Some(("Rust", 0)),
        },
        BidiCase {
            name: "arabic-para-rtl-base-level-2-island",
            line: 10,
            anchor: "Rust",
            base: BaseDirection::Rtl,
            // The same word, one base direction over: L at an odd level goes
            // up one to 2. This is the island the gate names.
            level_probe: Some(("Rust", 2)),
        },
        BidiCase {
            name: "hebrew-para-rtl-base-level-2-island",
            line: 18,
            anchor: "parley",
            base: BaseDirection::Rtl,
            level_probe: Some(("parley", 2)),
        },
        BidiCase {
            name: "image-at-byte-zero-ltr-base",
            line: 24,
            anchor: "![diagram]",
            base: BaseDirection::Ltr,
            level_probe: None,
        },
        BidiCase {
            name: "image-at-byte-zero-rtl-base",
            line: 24,
            anchor: "![diagram]",
            base: BaseDirection::Rtl,
            level_probe: None,
        },
    ];

    let mut graded = 0;
    for case in &cases {
        let source = corpus_line("rtl.md", case.line, case.anchor);
        let (laid, items) = shape_one_line(&source, case.base, &theme);
        let boxes: Vec<usize> = laid.images.iter().map(|i| i.visible_index).collect();

        let expected = visual_order_expected(&laid.visible, case.base, &boxes);
        let emitted = visual_order_emitted(&laid.visible, &items);
        assert_eq!(
            emitted, expected,
            "{}: parley's visual order disagrees with unicode-bidi\n  \
             visible  {:?}\n  expected {expected:?}\n  emitted  {emitted:?}",
            case.name, laid.visible
        );

        // A dropped run would leave both sides internally consistent and both
        // wrong, so the sequence is also checked to be a permutation of the
        // text it came from.
        let mut from_text: Vec<char> = laid
            .visible
            .chars()
            .chain(std::iter::repeat_n('\u{fffc}', boxes.len()))
            .collect();
        let mut from_layout: Vec<char> = emitted.chars().collect();
        from_text.sort_unstable();
        from_layout.sort_unstable();
        assert_eq!(
            from_text, from_layout,
            "{}: the emitted runs do not cover the visible string exactly once",
            case.name
        );

        if let Some((needle, level)) = case.level_probe {
            let at = laid
                .visible
                .find(needle)
                .unwrap_or_else(|| panic!("{}: {needle:?} is not in the visible text", case.name));
            let base_level = match case.base {
                BaseDirection::Ltr => Level::ltr(),
                BaseDirection::Rtl => Level::rtl(),
                BaseDirection::Auto => unreachable!(),
            };
            let info = BidiInfo::new(&laid.visible, Some(base_level));
            assert_eq!(
                info.levels[at].number(),
                level,
                "{}: unicode-bidi resolves {needle:?} at level {} — the case is \
                 no longer testing what its name says",
                case.name,
                info.levels[at].number()
            );
        }
        graded += 1;
    }
    assert_eq!(graded, 6, "six cases, all graded");
}

/// M3-R1's workaround, asserted rather than described.
///
/// `RTL_MARK` is a *substitution* for the image's bytes, so the two things
/// that have to be true are that the mark is in the visible string and that the
/// box ends up at the right-hand end of the line — where `unicode-bidi` puts a
/// U+FFFC at offset 0 of a paragraph at base level 1, and where parley without
/// the workaround does not (E1 case `e1-rtl-para-box-at-paragraph-start`, the
/// one failure of twelve on both pinned versions).
#[test]
fn the_rtl_workaround_moves_a_leading_box_to_the_end_of_the_line() {
    let theme = Theme::muya_default();
    let source = corpus_line("rtl.md", 24, "![diagram]");

    let (laid, items) = shape_one_line(&source, BaseDirection::Rtl, &theme);
    assert!(
        laid.visible.starts_with(mt_layout::inline::RTL_MARK),
        "an image leading an RTL paragraph substitutes a right-to-left mark"
    );
    let boxed = items
        .iter()
        .find_map(|i| match i {
            DisplayItem::InlineBox(b) => Some(*b),
            _ => None,
        })
        .expect("one box");
    let rightmost_glyph = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) => Some(g.offset + g.advance),
            _ => None,
        })
        .fold(f32::NEG_INFINITY, f32::max);
    // Everything except the mark's own zero-advance run is to the box's left.
    let text_right_of_box = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(g) if g.offset >= boxed.x => {
                Some(laid.visible[g.text_range.clone()].to_string())
            }
            _ => None,
        })
        .collect::<String>();
    assert_eq!(
        text_right_of_box,
        mt_layout::inline::RTL_MARK,
        "the only thing right of the box is the mark that put it there"
    );
    assert!(
        boxed.x + boxed.width <= rightmost_glyph + 0.01,
        "the box is inside the line, at its right-hand end: box {}..{}, line ends {rightmost_glyph}",
        boxed.x,
        boxed.x + boxed.width
    );

    // The control, in the same test so that the two cannot drift apart: at an
    // LTR base the same image is at the *left*, which is also what the oracle
    // says and is what every golden in the tree currently shows.
    let (_, ltr_items) = shape_one_line(&source, BaseDirection::Ltr, &theme);
    let ltr_box = ltr_items
        .iter()
        .find_map(|i| match i {
            DisplayItem::InlineBox(b) => Some(*b),
            _ => None,
        })
        .expect("one box");
    assert_eq!(ltr_box.x, 0.0, "an LTR-base paragraph puts it at the left");
}

// ---------------------------------------------------------------------------
// 2. The CJK line, in `dark` only
// ---------------------------------------------------------------------------

/// `cjk.md`'s deliberately long unspaced line, broken where UAX #14 says it
/// may be and nowhere else.
///
/// # The derivation, before the measurement
///
/// The line is 46 characters and 122 bytes: sixteen ideographs, the ASCII run
/// `UAX14`, one ideograph, the ASCII run `CJK`, twenty more ideographs and a
/// final `。`. There is no space in it, so every break opportunity is a
/// UAX #14 one:
///
/// - **A break between two ideographs is permitted.** Class `ID` has no
///   prohibition against a following `ID`, so `LB31` — *break everywhere
///   else* — applies. Every ideograph boundary is therefore a candidate.
/// - **A break before `。` is forbidden.** U+3002 IDEOGRAPHIC FULL STOP is
///   class `CL`, and `LB13` is `× CL`. So byte 119 is **not** a candidate and
///   the last legal break in the line is byte **116**, before the final `行`.
///
/// A greedy line breaker takes the last candidate that fits. At 16 px an
/// ideograph advances exactly 16.00, so the prefix ending at 116 is
/// 681.49 − 2 × 16 = **649.49 px** and the next candidate would be 665.49 px.
///
/// # Which theme, and why only one
///
/// The two themes lay out at content widths of 700 px (`muya-default`) and
/// 650 px (`dark`) — `Theme::content_width_px()`, asserted as constants in
/// `theme::tests`. 649.49 ≤ 650 < 665.49, so **`dark` breaks at 116**;
/// 681.49 ≤ 700, so **`muya-default` does not break at all**.
///
/// The absence is asserted explicitly and this comment is why: a "break
/// position" assertion at 700 px would be `assert_eq!(breaks, [])`, which
/// reads as coverage and is not. The real assertion is that the line fits, and
/// stating it that way means nobody later "fixes" a missing case by widening
/// the theme or by deleting the arm.
///
/// # S1's numbers are a starting point, not an expectation
///
/// S1's `complex-scripts` experiment measured `[]` at 700 and `[116]` at 650
/// **before** marker hiding existed, and the offsets are into the *visible*
/// text now. They agree here only because this particular line contains no
/// markdown marker, so its visible text is its block text byte for byte —
/// which the test asserts rather than assumes.
#[test]
fn the_long_cjk_line_breaks_where_uax14_permits_and_only_in_dark() {
    let line = corpus_line("cjk.md", 34, "UAX14");
    assert_eq!(line.len(), 122, "the line is 122 bytes");
    assert_eq!(line.chars().count(), 46, "and 46 characters");

    // No marker in it, so the visible offsets below are also block offsets.
    let laid = inline::lay_out(&line, &InlineSyntax::default(), BaseDirection::Ltr, None);
    assert_eq!(
        laid.visible, line,
        "this line has no markdown marker, so hiding markers changes nothing"
    );

    // The two candidates the derivation turns on, read out of the line rather
    // than trusted: 116 is an ideograph boundary, 119 is the one before `。`.
    assert_eq!(&line[116..119], "行");
    assert_eq!(&line[119..122], "。");

    let dark = one_paragraph(&line, &Theme::dark());
    assert_eq!(
        line_starts(&dark),
        vec![0, 116],
        "dark's 650 px column breaks at the last UAX #14 candidate that fits"
    );

    let muya = one_paragraph(&line, &Theme::muya_default());
    assert_eq!(
        line_starts(&muya),
        vec![0],
        "muya-default's 700 px column fits the whole 681.49 px line, so there is \
         no break to assert here — see this test's own note on why the empty \
         case is stated as 'one line' and not as 'no breaks'"
    );
}

// ---------------------------------------------------------------------------
// 3. Emoji cluster counts
// ---------------------------------------------------------------------------

/// One expected emoji answer. `codepoints` is UAX #29's input, `clusters` its
/// output, and `glyphs` is the face's — three different numbers that are only
/// coincidentally equal.
struct EmojiCase {
    sequence: &'static str,
    codepoints: usize,
    clusters: usize,
    glyphs: usize,
}

/// Every emoji sequence in `emoji.md` is **one grapheme cluster**, and three
/// of them are more than one glyph.
///
/// # Why a cluster count is not a glyph count
///
/// The gate asks for clusters, and until S2 nothing in this crate could answer:
/// the display list carries glyphs and advances, and `👨‍👩‍👧‍👦` is one cluster
/// whatever the face does with it. `ShapedText::clusters` is D8's *"a method,
/// point in and answer out"* arriving a milestone early with a real caller.
///
/// # The three numbers, and what each of them proves
///
/// - **Codepoints** are the corpus's, and `emoji.md`'s own table at `:36-39`
///   already declares them for three of these sequences — 7, 4 and 2 — so that
///   column is read and asserted rather than retyped.
/// - **Clusters** are UAX #29 § 3's answer, which is a property of the *text*:
///   GB9c/GB11 keep a ZWJ sequence together and GB9 keeps an Emoji_Modifier
///   with its base. One, in every case, however many codepoints.
/// - **Glyphs** are the face's answer and are *not* redundant. Noto Emoji has
///   a single glyph for the family, the two-parent, the four-person and the
///   profession sequences and for every skin-tone and hair-style compound —
///   but not for the rainbow flag, the pirate flag or the eye-in-speech
///   bubble, which come back as one cluster of two, two and three glyphs. The
///   difference is information: it says the cluster is right and the face is
///   incomplete, and a test that asserted only one number could not tell those
///   apart.
#[test]
fn every_emoji_sequence_is_one_cluster_however_many_codepoints_and_glyphs() {
    let theme = Theme::muya_default();

    // `emoji.md:14` — the ZWJ line, in source order.
    let zwj = [
        EmojiCase {
            sequence: "👨‍👩‍👧‍👦",
            codepoints: 7,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👩‍👩‍👦",
            codepoints: 5,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👨‍👨‍👧‍👧",
            codepoints: 7,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👩‍💻",
            codepoints: 3,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👨‍🚀",
            codepoints: 3,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "🧑‍🔬",
            codepoints: 3,
            clusters: 1,
            glyphs: 1,
        },
        // The three the face has no single glyph for.
        EmojiCase {
            sequence: "🏳️‍🌈",
            codepoints: 4,
            clusters: 1,
            glyphs: 2,
        },
        EmojiCase {
            sequence: "🏴‍☠️",
            codepoints: 4,
            clusters: 1,
            glyphs: 2,
        },
        EmojiCase {
            sequence: "👁️‍🗨️",
            codepoints: 5,
            clusters: 1,
            glyphs: 3,
        },
    ];
    // `emoji.md:18` — base plus modifier, and two that are modifier *and* ZWJ.
    let modifiers = [
        EmojiCase {
            sequence: "👋🏻",
            codepoints: 2,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👋🏼",
            codepoints: 2,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👋🏽",
            codepoints: 2,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👋🏾",
            codepoints: 2,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👋🏿",
            codepoints: 2,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "🤝🏽",
            codepoints: 2,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👨🏿‍🦱",
            codepoints: 4,
            clusters: 1,
            glyphs: 1,
        },
        EmojiCase {
            sequence: "👩🏻‍🦰",
            codepoints: 4,
            clusters: 1,
            glyphs: 1,
        },
    ];

    for (number, anchor, cases) in [(14usize, "👨‍👩‍👧‍👦", &zwj[..]), (18, "👋🏻", &modifiers[..])]
    {
        let line = corpus_line("emoji.md", number, anchor);
        let laid = inline::lay_out(&line, &InlineSyntax::default(), BaseDirection::Ltr, None);
        assert_eq!(
            laid.visible, line,
            "an emoji line carries no marker, so nothing is hidden"
        );
        let (_fonts, shaped) = shape_for_clusters(&laid.visible, &theme);

        // Space-separated in the corpus, and no sequence contains a space, so
        // the split is the sequence list. Asserting it against the table below
        // is what makes this read the corpus rather than a copy of it.
        let words: Vec<&str> = laid.visible.split(' ').filter(|w| !w.is_empty()).collect();
        assert_eq!(
            words.len(),
            cases.len(),
            "emoji.md:{number} has {} sequences, the table has {}",
            words.len(),
            cases.len()
        );

        let mut at = 0usize;
        for (word, case) in words.iter().zip(cases) {
            let start = laid.visible[at..]
                .find(word)
                .expect("the words came from this string")
                + at;
            let range = start..start + word.len();
            at = range.end;

            assert_eq!(
                *word, case.sequence,
                "emoji.md:{number} sequence order changed"
            );
            assert_eq!(
                word.chars().count(),
                case.codepoints,
                "{word:?} is {} codepoints, the table says {}",
                word.chars().count(),
                case.codepoints
            );

            let clusters = shaped.clusters(range.clone());
            assert_eq!(
                clusters.len(),
                case.clusters,
                "{word:?} is {} grapheme cluster(s), UAX #29 says {}",
                clusters.len(),
                case.clusters
            );
            // The cluster covers the whole sequence, which is the claim that
            // makes the count meaningful: one cluster of the wrong extent
            // would count the same.
            assert_eq!(
                clusters[0].text_range, range,
                "{word:?}'s single cluster does not span the whole sequence"
            );
            let glyphs: usize = clusters.iter().map(|c| c.glyphs).sum();
            assert_eq!(
                glyphs, case.glyphs,
                "{word:?} draws {glyphs} glyph(s) from the committed faces, expected {}",
                case.glyphs
            );
            assert!(
                !clusters[0].is_ligature_continuation,
                "{word:?} is a whole cluster, not the tail of one"
            );
        }
    }
}

/// The corpus's own table is the codepoint oracle, so the two cannot drift.
///
/// `emoji.md:36-39` writes the codepoint count of three sequences in its third
/// column, which makes the fixture self-describing — and makes a corpus edit
/// that changes a sequence without changing its number fail here rather than
/// somewhere subtler.
#[test]
fn the_emoji_tables_third_column_agrees_with_the_cluster_counts() {
    let theme = Theme::muya_default();
    let src = corpus("emoji.md");
    let rows: Vec<Vec<String>> = src
        .lines()
        .filter(|l| l.starts_with('|') && !l.contains("---") && !l.contains("Codepoints"))
        .map(|l| {
            l.trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect()
        })
        .collect();
    assert_eq!(rows.len(), 3, "three data rows in emoji.md's table");

    for row in &rows {
        let sequence = &row[0];
        let declared: usize = row[2].parse().expect("the third column is a number");
        assert_eq!(
            sequence.chars().count(),
            declared,
            "{sequence:?} is {} codepoints and the table says {declared}",
            sequence.chars().count()
        );
        let (_fonts, shaped) = shape_for_clusters(sequence, &theme);
        assert_eq!(
            shaped.clusters(0..sequence.len()).len(),
            1,
            "{sequence:?} is one grapheme cluster whatever its codepoint count"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Ligatures
// ---------------------------------------------------------------------------

/// The gate's fourth word, and the Latin half of it is reachable.
///
/// A ligature is a shaped cluster spanning **more than one grapheme**, which
/// is the only way to observe one without reading the face's `GSUB` or
/// hard-coding a glyph id: `TextCluster::is_ligature_start` is set on the first
/// grapheme, `is_ligature_continuation` on the rest, and only the first carries
/// glyphs. Two graphemes, one glyph — a count a display list cannot express.
///
/// `fi`, `fl` and `ff` are everywhere in the corpus's own prose and Open Sans
/// ligates all three at the body stack, which is the stack every paragraph
/// uses. So the clause is reachable and this is not a vacuous test:
/// `10kb.md` alone produces 109 of them, `20-tables.md` 16, `50-code-fences.md`
/// 7, and every one of `cjk.md`, `emoji.md`, `rtl.md`, `block-kinds.md` and
/// `100-inline-math.md` at least one.
#[test]
fn the_corpus_produces_latin_ligatures_at_the_body_stack() {
    let theme = Theme::muya_default();

    for (file, line, anchor, needle) in [
        ("cjk.md", 5usize, "flanking", "fl"),
        ("rtl.md", 3, "fix", "fi"),
        ("10kb.md", 16, "buffer", "ff"),
    ] {
        let source = corpus_line(file, line, anchor);
        let laid = inline::lay_out(&source, &InlineSyntax::default(), BaseDirection::Ltr, None);
        let at = laid
            .visible
            .find(needle)
            .unwrap_or_else(|| panic!("{file}:{line} no longer contains {needle:?}"));
        let (_fonts, shaped) = shape_for_clusters(&laid.visible, &theme);
        let clusters = shaped.clusters(at..at + needle.len());

        assert_eq!(clusters.len(), 2, "{needle:?} is two grapheme clusters");
        assert!(
            clusters[0].is_ligature_start,
            "{file}:{line} {needle:?} is not shaped as a ligature. If the face set \
             changed, that is a finding to record rather than an expectation to \
             delete."
        );
        assert_eq!(
            clusters[0].glyphs, 1,
            "{needle:?} draws as one glyph over two graphemes"
        );
        assert!(clusters[1].is_ligature_continuation);
        assert_eq!(
            clusters[1].glyphs, 0,
            "a ligature's continuation carries no glyphs of its own"
        );
    }
}

/// The Arabic half is **not** reachable from the corpus, and this records why
/// rather than asserting a ligature that is not there.
///
/// §5 D2 rejects parley 0.11.0 because *"0.11.0 breaks Arabic cursive joining
/// at an inline box"*, so an Arabic ligature would be the sharpest possible
/// control on the pin. The corpus has the input — `rtl.md:35`'s `الأول` is lam
/// followed by alef-with-hamza, and lam-alef is a *required* Arabic ligature —
/// but the face the corpus's Arabic resolves to does not form it:
///
/// | stack | face | `لا` | `الأول` |
/// |---|---|---|---|
/// | `theme.fonts.body` | Noto Sans Arabic (fallback) | two clusters, one glyph each — **no ligature** | ditto, and `أ` decomposes to two glyphs |
/// | `theme.fonts.code` | DejaVu Sans Mono | one glyph over two clusters — **ligature** | ditto |
///
/// Nothing in `bench/corpus/` puts Arabic through the monospace stack: the one
/// Arabic inline-code span, `rtl.md:13`, has ASCII in it. So **no corpus
/// construct produces an Arabic ligature through the real pipeline.** That is
/// the honest answer to the gate's fourth word for this half, and it is
/// asserted in both directions so that a face-set change — which is a
/// reviewable event on the same footing as a pin move — cannot flip it
/// silently.
#[test]
fn the_corpus_arabic_is_not_ligated_and_the_monospace_stack_shows_what_is_missing() {
    let theme = Theme::muya_default();
    let source = corpus_line("rtl.md", 35, "الأول");
    let word = "الأول";
    let at = source.find(word).expect("the anchor found it");
    // Lam at +2, alef-with-hamza at +4 — asserted, because the whole point is
    // which two characters are being asked about.
    let lam_alef = at + 2..at + 6;
    assert_eq!(&source[lam_alef.clone()], "لأ");

    let mut fonts = bundled_fonts();
    let mut shaper = TextShaper::new();
    for (stack, ligated) in [(&theme.fonts.body, false), (&theme.fonts.code, true)] {
        let request = TextRequest::new(&source, stack, 16.0, 1.6);
        let shaped = shaper.shape(&mut fonts, &request);
        let clusters = shaped.clusters(lam_alef.clone());
        assert_eq!(
            clusters.len(),
            2,
            "lam and alef are two graphemes either way"
        );
        assert_eq!(
            clusters[0].is_ligature_start,
            ligated,
            "the {} stack {} the lam-alef. Either direction changing is a finding: \
             the corpus's Arabic is shaped at the body stack, so a ligature \
             appearing there is the face set having improved, and one \
             disappearing from the monospace stack is the pin having regressed.",
            if ligated { "monospace" } else { "body" },
            if ligated {
                "should ligate"
            } else {
                "should not ligate"
            }
        );
    }
}

// ---------------------------------------------------------------------------
// 5. D13's map, as a property over the corpus
// ---------------------------------------------------------------------------

/// The twelve corpus inputs, and how much of each this test walks.
///
/// **What a "leaf" is here, said plainly.** `mt-layout` may not depend on
/// `mt-md` (§5 D5 forbids the edge and S7 asserts its absence), so this test
/// cannot ask the real parser for the real block tree. It splits each file on
/// blank lines instead and treats every chunk as one leaf's text. That is
/// **not** the leaf set `mt_md::parse` produces — a chunk can hold a whole
/// list, and a fenced block's body is included where the real pipeline would
/// shape it verbatim — but it is a **superset in shape**: `inline::lay_out`
/// takes a `&str` and D13's invariants are properties of the walk over any
/// string, so every byte of every corpus file is put through it.
///
/// **What is sampled, in numbers.** The ten inputs up to 250 KiB are walked
/// whole — 1 740 chunks — and the two large ones are strided rather than
/// truncated, so the leaves checked are spread evenly across the file instead
/// of taken from its head: `1mb.md` has 5 832 chunks and every 25th is walked,
/// `5mb.md` has 29 436 and every 125th. **2 210 leaves and 370 299 bytes in
/// total**, which keeps the walk at seconds rather than minutes in a debug
/// build. `empty.md` contributes nothing and is listed so that its zero is a
/// stated result rather than an omission.
const CORPUS_COVERAGE: [(&str, usize); 12] = [
    ("100-inline-math.md", 1),
    ("10kb.md", 1),
    ("20-tables.md", 1),
    ("250kb.md", 1),
    ("50-code-fences.md", 1),
    ("block-kinds.md", 1),
    ("cjk.md", 1),
    ("emoji.md", 1),
    ("empty.md", 1),
    ("rtl.md", 1),
    // 5 832 and 29 436 chunks; every 25th and every 125th leaves 234 and 236,
    // evenly spread across the file.
    ("1mb.md", 25),
    ("5mb.md", 125),
];

/// Blank-line-separated chunks of a corpus file. See [`CORPUS_COVERAGE`].
fn leaf_texts(source: &str, stride: usize) -> Vec<&str> {
    source
        .split("\n\n")
        .map(str::trim_end)
        .filter(|c| !c.is_empty())
        .step_by(stride)
        .collect()
}

/// D13's five invariants, over every leaf this test walks.
///
/// Each one is checked against something other than the arithmetic that
/// produced it:
///
/// 1. `to_block` is **total** and non-decreasing over `0..=visible_len`.
/// 2. The block offset it answers with lies inside the range of the **token**
///    that produced the run — the gate's own sentence, and the reason
///    `MapRun::token` is carried rather than accumulated. A map built by adding
///    up lengths could only agree with itself here.
/// 3. `to_visible` is `None` exactly for bytes inside hidden runs, and
///    round-trips for copied ones.
/// 4. The runs **tile** both spaces: contiguous, gapless and non-overlapping in
///    block offsets, and monotone with no overlap in visible offsets.
/// 5. Concatenating what each run contributes reproduces the visible string —
///    which is the string `flow::LayoutTree::build` hands to parley, byte for
///    byte.
fn assert_map_invariants(text: &str, laid: &InlineText, what: &str) {
    let map = &laid.map;
    assert_eq!(map.block_len(), text.len(), "{what}: block length");
    assert_eq!(
        map.visible_len(),
        laid.visible.len(),
        "{what}: visible length"
    );

    // --- 4. tiling ------------------------------------------------------
    let mut block_at = 0usize;
    let mut visible_at = 0usize;
    let mut rebuilt = String::with_capacity(laid.visible.len());
    for (i, run) in map.runs().iter().enumerate() {
        assert_eq!(
            run.block.start, block_at,
            "{what}: run {i} leaves a gap or overlap in the block text"
        );
        assert!(
            run.block.end > run.block.start,
            "{what}: run {i} has an empty block range"
        );
        block_at = run.block.end;

        assert!(
            run.token.start <= run.block.start && run.block.end <= run.token.end,
            "{what}: run {i}'s block range escapes its token"
        );
        assert!(
            text.is_char_boundary(run.block.start) && text.is_char_boundary(run.block.end),
            "{what}: run {i} splits a character of the block text"
        );

        match run.kind {
            MapKind::Hidden => assert!(
                run.visible.is_empty(),
                "{what}: run {i} is hidden and occupies visible space"
            ),
            MapKind::Copied => {
                assert_eq!(
                    &laid.visible[run.visible.clone()],
                    &text[run.block.clone()],
                    "{what}: run {i} says copied and is not"
                );
                rebuilt.push_str(&text[run.block.clone()]);
            }
            MapKind::Substituted => {
                assert!(
                    !run.visible.is_empty(),
                    "{what}: run {i} substitutes nothing, which is what Hidden means"
                );
                rebuilt.push_str(&laid.visible[run.visible.clone()]);
            }
        }
        if !run.visible.is_empty() {
            assert_eq!(
                run.visible.start, visible_at,
                "{what}: run {i} leaves a gap or overlap in the visible string"
            );
            visible_at = run.visible.end;
        } else {
            assert_eq!(
                run.visible.start, visible_at,
                "{what}: hidden run {i} is anchored at the wrong visible offset"
            );
        }
    }
    assert_eq!(
        block_at,
        text.len(),
        "{what}: the runs do not reach the end"
    );
    assert_eq!(
        visible_at,
        laid.visible.len(),
        "{what}: visible tail missing"
    );
    // --- 5. the string handed to parley ---------------------------------
    assert_eq!(
        rebuilt, laid.visible,
        "{what}: the runs do not spell the visible string"
    );

    // --- 1 and 2. totality, monotonicity, and the token containment ------
    let mut previous = 0usize;
    for visible in 0..=laid.visible.len() {
        if !laid.visible.is_char_boundary(visible) {
            continue;
        }
        let block = map.to_block(visible);
        assert!(
            block >= previous,
            "{what}: to_block({visible}) went backwards to {block}"
        );
        previous = block;
        assert!(
            block <= text.len(),
            "{what}: to_block({visible}) is past the end"
        );
        if visible == laid.visible.len() {
            continue;
        }
        let run = run_containing_visible(map.runs(), visible)
            .unwrap_or_else(|| panic!("{what}: no run owns visible offset {visible}"));
        assert!(
            run.token.start <= block && block < run.token.end,
            "{what}: to_block({visible}) = {block} is outside the token {:?} that \
             produced the run",
            run.token
        );
    }

    // --- 3. the partial direction ---------------------------------------
    for run in map.runs() {
        for block in run.block.clone() {
            if !text.is_char_boundary(block) {
                continue;
            }
            match run.kind {
                MapKind::Hidden => assert_eq!(
                    map.to_visible(block),
                    None,
                    "{what}: a hidden byte answered with a visible offset"
                ),
                MapKind::Copied => {
                    let visible = map
                        .to_visible(block)
                        .unwrap_or_else(|| panic!("{what}: a copied byte answered None"));
                    assert_eq!(
                        map.to_block(visible),
                        block,
                        "{what}: a copied byte does not round-trip"
                    );
                }
                MapKind::Substituted => assert_eq!(
                    map.to_visible(block),
                    Some(run.visible.start),
                    "{what}: a substituted byte answers with its run's visible start"
                ),
            }
        }
    }
}

/// The run whose visible range contains `visible`, mirroring
/// `VisibleTextMap::to_block`'s partition without borrowing its arithmetic.
fn run_containing_visible(
    runs: &[mt_layout::inline::MapRun],
    visible: usize,
) -> Option<&mt_layout::inline::MapRun> {
    runs.iter()
        .find(|r| !r.visible.is_empty() && r.visible.contains(&visible))
}

#[test]
fn every_visible_offset_in_the_corpus_maps_into_the_token_that_produced_it() {
    let mut leaves = 0usize;
    let mut bytes = 0usize;
    for (file, stride) in CORPUS_COVERAGE {
        let source = corpus(file);
        for (i, text) in leaf_texts(&source, stride).into_iter().enumerate() {
            let laid = inline::lay_out(text, &InlineSyntax::default(), BaseDirection::Ltr, None);
            assert_map_invariants(text, &laid, &format!("{file} leaf {i}"));
            leaves += 1;
            bytes += text.len();
        }
    }
    // A floor rather than an equality, so that a corpus edit is not a failure
    // here — but a *collapse* in coverage is. Measured: 2 210 leaves and
    // 370 299 bytes. `cargo xtask corpus --check` is what pins the corpus
    // itself.
    assert!(
        leaves >= 2_000 && bytes >= 300_000,
        "the property walked only {leaves} leaves ({bytes} bytes); coverage collapsed"
    );
}

/// The RTL base direction changes the visible string, so the invariants are
/// re-checked there rather than assumed to carry over.
///
/// This is the one input in the corpus where a run is `Substituted` for a
/// reason that is not a rendering — M3-R1's mark stands *for* the image's
/// bytes — and D13's claim that this needed no fourth `MapKind` is exactly the
/// claim that the run list still tiles.
#[test]
fn the_rtl_workaround_leaves_the_map_tiling_both_sides() {
    let source = corpus_line("rtl.md", 24, "![diagram]");
    let laid = inline::lay_out(&source, &InlineSyntax::default(), BaseDirection::Rtl, None);
    assert_map_invariants(&source, &laid, "rtl.md:24 at an RTL base");

    let substituted: Vec<_> = laid
        .map
        .runs()
        .iter()
        .filter(|r| r.kind == MapKind::Substituted)
        .collect();
    assert_eq!(substituted.len(), 1, "one substitution: the image's bytes");
    assert_eq!(substituted[0].block.start, 0, "at byte 0 of the block text");
    assert_eq!(
        &laid.visible[substituted[0].visible.clone()],
        mt_layout::inline::RTL_MARK
    );
    // And it answers the same thing the hidden run did: an offset inside the
    // mark is the image's block start.
    assert_eq!(laid.map.to_block(0), 0);
}

/// The map on the **display list** is the same map the walk produced, for the
/// three files the gate names.
///
/// The property above is about `inline::lay_out` in isolation; this is the
/// wire that carries it to a caller. §10 owes the map forward to M4's caret
/// and M5's search, and both read `BlockDisplay::text_map` and never call the
/// walk — so an agreement between the two is the thing that makes the property
/// mean anything downstream.
#[test]
fn the_display_lists_map_is_the_walks_map_for_every_gate_file() {
    let theme = Theme::muya_default();
    let mut checked = 0usize;
    for file in ["cjk.md", "emoji.md", "rtl.md"] {
        let source = corpus(file);
        for text in leaf_texts(&source, 1) {
            // Only paragraphs: `one_paragraph` builds one block, and a chunk
            // that is a list or a table would be a different block kind in the
            // real parse. The map is per leaf either way.
            if text.starts_with('#') || text.starts_with('|') || text.starts_with('-') {
                continue;
            }
            let list = one_paragraph(text, &theme);
            let block = list
                .blocks
                .iter()
                .find(|b| b.kind == BlockKind::Paragraph)
                .expect("a paragraph");
            let laid = inline::lay_out(text, &InlineSyntax::default(), BaseDirection::Ltr, None);
            assert_eq!(
                block.text_map.as_ref(),
                Some(&laid.map),
                "{file}: the display list's map is not the walk's"
            );
            // Every glyph run's range is an offset into the *visible* string,
            // which is the invariant `GlyphRun::text_range`'s doc states and
            // the one a caller would notice breaking.
            for item in &block.items {
                let DisplayItem::Glyphs(run) = item else {
                    continue;
                };
                assert!(
                    run.text_range.end <= laid.visible.len(),
                    "{file}: a glyph run reaches past the visible string"
                );
                assert!(
                    laid.visible.is_char_boundary(run.text_range.start)
                        && laid.visible.is_char_boundary(run.text_range.end),
                    "{file}: a glyph run splits a character"
                );
                let block_offset = laid.map.to_block(run.text_range.start);
                assert!(
                    block_offset <= text.len(),
                    "{file}: a glyph run maps past the block text"
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} paragraphs checked");
}

/// A leaf that was not tokenized gets the identity, and the identity satisfies
/// the same invariants — so a caller never has to ask which kind of map it has.
#[test]
fn the_identity_map_satisfies_every_invariant_the_walks_map_does() {
    for text in ["", "fn main() {}", "  indented\n  code\n"] {
        let laid = InlineText {
            visible: text.to_string(),
            runs: Vec::new(),
            map: mt_layout::inline::VisibleTextMap::identity(text.len()),
            images: Vec::new(),
        };
        assert_map_invariants(text, &laid, &format!("identity {text:?}"));
        assert_eq!(laid.map.to_block(text.len()), text.len());
        assert_eq!(laid.map.to_visible(text.len()), Some(text.len()));
    }
}

/// The checker itself fails when the map is wrong.
///
/// `VisibleTextMap` has no public constructor from arbitrary runs — and should
/// not gain one, because the walk is the only thing that ought to build one —
/// so the wrong map is made by handing a correct map the wrong text, which is
/// the same class of error a caller could actually commit. Without this the
/// property above could be vacuous, and a vacuous property is exactly the
/// rubber stamp §8 M3-R6 is about.
#[test]
#[should_panic(expected = "block length")]
fn the_property_checker_fails_when_the_map_does_not_describe_the_text() {
    let laid = inline::lay_out("**a**", &InlineSyntax::default(), BaseDirection::Ltr, None);
    assert_map_invariants("**ab**", &laid, "deliberately mismatched");
}

/// The property is non-vacuous: the corpus reaches every branch of the checker
/// that it can reach, and the one it cannot is named rather than left unstated.
///
/// `MapKind::Substituted` is **not** reachable from the corpus at an LTR base —
/// the only two producers are an html entity, of which `bench/corpus/` contains
/// none, and M3-R1's mark, which needs an RTL base. It is covered by
/// [`the_rtl_workaround_leaves_the_map_tiling_both_sides`] instead, and this
/// test asserts the absence so that the day an entity lands in the corpus this
/// reads as a change rather than as noise.
#[test]
fn the_map_property_exercises_every_branch_the_corpus_can_reach() {
    let mut copied = 0usize;
    let mut hidden = 0usize;
    let mut substituted = 0usize;
    let mut tokens_wider_than_their_run = 0usize;
    for (file, stride) in CORPUS_COVERAGE {
        let source = corpus(file);
        for text in leaf_texts(&source, stride) {
            let laid = inline::lay_out(text, &InlineSyntax::default(), BaseDirection::Ltr, None);
            for run in laid.map.runs() {
                match run.kind {
                    MapKind::Copied => copied += 1,
                    MapKind::Hidden => hidden += 1,
                    MapKind::Substituted => substituted += 1,
                }
                if run.token.start < run.block.start || run.block.end < run.token.end {
                    tokens_wider_than_their_run += 1;
                }
            }
        }
    }
    assert!(copied > 0 && hidden > 0, "copied {copied}, hidden {hidden}");
    assert_eq!(
        substituted, 0,
        "an html entity has landed in the corpus — the substituted branch is now \
         reachable at an LTR base and this test's note is stale"
    );
    // The token containment check is only a real check where the token is
    // *wider* than the run; where they coincide it is trivially true.
    assert!(
        tokens_wider_than_their_run > 0,
        "no run in the corpus came from a token wider than itself, so the \
         containment assertion never had anything to catch"
    );
}
