//! The walk, the visible string and D13's map — no fonts and no filesystem.
//!
//! Everything here is a pure function of a `&str`: `mt_inline::tokenize` needs
//! no faces and neither does building a visible string, so the whole of C6 is
//! testable in `src/`. Only the *shaping* of the result needs bytes, and that
//! lives in `tests/layout.rs`.

use super::*;

fn plain(text: &str) -> InlineText {
    lay_out(text, &InlineSyntax::default(), BaseDirection::Ltr, None)
}

fn kinds(map: &VisibleTextMap) -> Vec<(MapKind, Range<usize>, Range<usize>)> {
    map.runs()
        .iter()
        .map(|r| (r.kind, r.visible.clone(), r.block.clone()))
        .collect()
}

/// Every visible offset, mapped back and checked against the token that
/// produced it — D13's property test in its smallest form.
fn every_offset_round_trips(text: &str) {
    every_offset_round_trips_with(text, BaseDirection::Ltr);
}

fn every_offset_round_trips_with(text: &str, base_direction: BaseDirection) {
    let laid = lay_out(text, &InlineSyntax::default(), base_direction, None);
    assert_eq!(laid.map.block_len(), text.len());
    assert_eq!(laid.map.visible_len(), laid.visible.len());
    for v in 0..=laid.visible.len() {
        let b = laid.map.to_block(v);
        assert!(
            b <= text.len(),
            "{text:?}: visible {v} mapped outside the block text"
        );
    }
    for b in 0..=text.len() {
        if let Some(v) = laid.map.to_visible(b) {
            assert!(
                v <= laid.visible.len(),
                "{text:?}: block {b} mapped outside the visible text"
            );
        }
    }
    // The runs tile the block text with no gap and no overlap, which is what
    // makes both lookups a partition rather than a search.
    let mut at = 0;
    for run in laid.map.runs() {
        assert_eq!(run.block.start, at, "{text:?}: a gap before {run:?}");
        assert!(run.block.end > run.block.start, "{text:?}: empty {run:?}");
        assert!(
            run.token.start <= run.block.start && run.block.end <= run.token.end,
            "{text:?}: {run:?} is not inside its own token"
        );
        at = run.block.end;
    }
    assert_eq!(at, text.len(), "{text:?}: the runs stop short");
}

// ---------------------------------------------------------------------------
// The four cases D13 names
// ---------------------------------------------------------------------------

/// **A leaf with no markers at all.** Visible is block, byte for byte, and the
/// map is one copied run — the case that must stay free, because it is most of
/// every document.
#[test]
fn a_leaf_with_no_markers_is_its_own_visible_text() {
    let laid = plain("just some prose, with punctuation.");
    assert_eq!(laid.visible, "just some prose, with punctuation.");
    assert_eq!(
        kinds(&laid.map),
        vec![(MapKind::Copied, 0..34, 0..34)],
        "one run, not one per word"
    );
    // One style run at the default style, which `flow` then drops entirely.
    assert_eq!(laid.runs.len(), 1);
    assert_eq!(laid.runs[0].style, InlineStyle::default());
    for i in 0..=laid.visible.len() {
        assert_eq!(laid.map.to_block(i), i);
        assert_eq!(laid.map.to_visible(i), Some(i));
    }
    every_offset_round_trips("just some prose, with punctuation.");
}

/// **A hidden marker at the start of a leaf.** An ATX heading's `# ` is the
/// commonest one in any document, and it is the case where a naive map that
/// starts both spaces at zero is wrong from the first byte.
#[test]
fn a_marker_at_the_start_of_a_leaf_shifts_every_offset_after_it() {
    let laid = plain("# Heading");
    assert_eq!(laid.visible, "Heading");
    assert_eq!(
        kinds(&laid.map),
        vec![(MapKind::Hidden, 0..0, 0..2), (MapKind::Copied, 0..7, 2..9),]
    );
    // Visible 0 is the `H`, which is block 2 — not block 0.
    assert_eq!(laid.map.to_block(0), 2);
    assert_eq!(laid.map.to_block(7), 9);
    // And the two bytes of `# ` have no visible position at all. Not the
    // nearest one; none.
    assert_eq!(laid.map.to_visible(0), None);
    assert_eq!(laid.map.to_visible(1), None);
    assert_eq!(laid.map.to_visible(2), Some(0));
    every_offset_round_trips("# Heading");
}

/// **A substituting run.** `&amp;` is five block bytes and one visible byte, and
/// the run says `Substituted` rather than leaving a caller to notice that the
/// lengths differ.
#[test]
fn an_html_entity_substitutes_and_says_so() {
    let laid = plain("a &amp; b");
    assert_eq!(laid.visible, "a & b");
    assert_eq!(
        kinds(&laid.map),
        vec![
            (MapKind::Copied, 0..2, 0..2),
            (MapKind::Substituted, 2..3, 2..7),
            (MapKind::Copied, 3..5, 7..9),
        ]
    );
    // The whole entity answers with its own start in both directions: there is
    // no meaningful offset *inside* one ampersand.
    assert_eq!(laid.map.to_block(2), 2);
    for b in 2..7 {
        assert_eq!(laid.map.to_visible(b), Some(2), "block {b}");
    }
    assert_eq!(laid.map.to_visible(7), Some(3));
    every_offset_round_trips("a &amp; b");
}

/// A line break is the second substituting kind, and the one that is easy to
/// mistake for a copy because one byte becomes one byte.
///
/// **This is exactly why [`MapKind`] is stored and not inferred.** `\n` → `' '`
/// has identical lengths on both sides; arithmetic cannot tell it from a copy,
/// and a caller that assumed a copy would hand back a `\n` where the reader saw
/// a space.
#[test]
fn a_line_break_substitutes_one_byte_for_one_byte_and_is_still_not_a_copy() {
    let laid = plain("one\ntwo");
    assert_eq!(laid.visible, "one two");
    assert_eq!(
        kinds(&laid.map),
        vec![
            (MapKind::Copied, 0..3, 0..3),
            (MapKind::Substituted, 3..4, 3..4),
            (MapKind::Copied, 4..7, 4..7),
        ]
    );
    assert_eq!(laid.map.runs()[1].kind, MapKind::Substituted);
    every_offset_round_trips("one\ntwo");
}

/// **Nested emphasis.** The inner run is both, which is the whole reason
/// [`InlineStyle`] is a set of flags rather than an enum.
#[test]
fn bold_inside_italic_is_one_run_that_is_both() {
    let laid = plain("*a **b** c*");
    assert_eq!(laid.visible, "a b c");
    let styles: Vec<_> = laid
        .runs
        .iter()
        .map(|r| (r.range.clone(), r.style.em, r.style.strong))
        .collect();
    assert_eq!(
        styles,
        vec![(0..2, true, false), (2..3, true, true), (3..5, true, false)],
        "the middle run is emphatic *and* strong"
    );
    every_offset_round_trips("*a **b** c*");
}

// ---------------------------------------------------------------------------
// The rest of the classification
// ---------------------------------------------------------------------------

#[test]
fn inline_code_hides_its_backticks_and_marks_its_content() {
    let laid = plain("see `x + 1` here");
    assert_eq!(laid.visible, "see x + 1 here");
    assert!(laid.runs.iter().any(|r| r.style.code && r.range == (4..9)));
    every_offset_round_trips("see `x + 1` here");
}

#[test]
fn a_links_anchor_is_visible_and_its_href_is_not() {
    let laid = plain("read [the plan](https://example.com) now");
    assert_eq!(laid.visible, "read the plan now");
    let link: Vec<_> = laid
        .runs
        .iter()
        .filter(|r| r.style.link)
        .map(|r| r.range.clone())
        .collect();
    assert_eq!(link, vec![5..13], "the anchor, and nothing else");
    // The href's bytes exist in the block text and nowhere in the visible one.
    let href = "read [the plan](".len();
    assert_eq!(laid.map.to_visible(href), None);
    every_offset_round_trips("read [the plan](https://example.com) now");
}

#[test]
fn a_bare_url_is_its_own_anchor_and_an_angle_bracket_autolink_is_not() {
    let bare = plain("go to https://example.com now");
    assert_eq!(bare.visible, "go to https://example.com now");
    assert!(bare.runs.iter().any(|r| r.style.link));

    let angled = plain("go to <https://example.com> now");
    assert_eq!(
        angled.visible, "go to https://example.com now",
        "the angle brackets are markers"
    );
    every_offset_round_trips("go to <https://example.com> now");
}

#[test]
fn strikethrough_is_recorded_even_though_nothing_draws_it_yet() {
    let laid = plain("~~gone~~");
    assert_eq!(laid.visible, "gone");
    assert!(laid.runs.iter().all(|r| r.style.del));
    every_offset_round_trips("~~gone~~");
}

/// An image contributes nothing visible — alt text is an attribute, not
/// something a reader sees — and the hole that belongs at its offset is the
/// next commit's inline box.
#[test]
fn an_image_is_hidden_whole_including_its_alt_text() {
    let laid = plain("before ![a diagram](./d.png) after");
    assert_eq!(laid.visible, "before  after");
    let image = laid
        .map
        .runs()
        .iter()
        .find(|r| r.kind == MapKind::Hidden)
        .expect("the image is a hidden run");
    assert_eq!(image.block, 7..28);
    assert_eq!(image.visible, 7..7);
    every_offset_round_trips("before ![a diagram](./d.png) after");
}

/// **`mt-inline` has no emoji shortcode table**, so the shortcode is copied
/// whole rather than substituted for a glyph that would have to be invented.
///
/// Colons included, deliberately: hiding them the way every other marker is
/// hidden would render the bare word `smile`, which reads as something the
/// author typed. This is the one place the walk declines to hide a marker, and
/// the test exists so that adding a table later is a visible change to an
/// assertion rather than a silent one.
#[test]
fn an_emoji_shortcode_is_copied_whole_because_there_is_no_table_to_substitute_from() {
    let laid = plain("hi :smile: there");
    assert_eq!(laid.visible, "hi :smile: there");
    assert!(
        laid.map
            .runs()
            .iter()
            .all(|r| r.kind == MapKind::Copied || r.kind == MapKind::Hidden)
    );
    every_offset_round_trips("hi :smile: there");
}

#[test]
fn a_backslash_escape_hides_the_backslash_and_keeps_the_character() {
    let laid = plain(r"a \* b");
    assert_eq!(laid.visible, "a * b");
    assert_eq!(laid.map.to_visible(2), None, "the backslash itself");
    assert_eq!(laid.map.to_visible(3), Some(2), "the escaped `*`");
    every_offset_round_trips(r"a \* b");
}

#[test]
fn a_thematic_break_hides_completely() {
    let laid = plain("---");
    assert_eq!(laid.visible, "");
    assert_eq!(kinds(&laid.map), vec![(MapKind::Hidden, 0..0, 0..3)]);
    assert_eq!(laid.map.to_visible(0), None);
    // Total in the other direction even with nothing to map: offset 0 of an
    // empty visible string is the end of the block text.
    assert_eq!(laid.map.to_block(0), 3);
}

#[test]
fn a_footnote_identifier_is_its_own_run_when_the_syntax_is_enabled() {
    let off = plain("see [^why] here");
    assert_eq!(
        off.visible, "see [^why] here",
        "with footnotes off it is ordinary text"
    );
    assert!(off.runs.iter().all(|r| !r.style.footnote));

    let syntax = InlineSyntax {
        footnote: true,
        ..InlineSyntax::default()
    };
    let on = lay_out("see [^why] here", &syntax, BaseDirection::Ltr, None);
    assert_eq!(on.visible, "see why here");
    let marked: Vec<_> = on
        .runs
        .iter()
        .filter(|r| r.style.footnote)
        .map(|r| r.range.clone())
        .collect();
    assert_eq!(marked, vec![4..7]);
}

// ---------------------------------------------------------------------------
// The map's own contract
// ---------------------------------------------------------------------------

#[test]
fn the_identity_map_is_the_identity() {
    let map = VisibleTextMap::identity(12);
    assert_eq!(map.visible_len(), 12);
    assert_eq!(map.block_len(), 12);
    for i in 0..=12 {
        assert_eq!(map.to_block(i), i);
        assert_eq!(map.to_visible(i), Some(i));
    }
    let empty = VisibleTextMap::identity(0);
    assert!(empty.runs().is_empty());
    assert_eq!(empty.to_block(0), 0);
    assert_eq!(empty.to_visible(0), Some(0));
}

/// The token range survives the walk, which is the difference between a
/// property test that can fail and one that can only agree with itself.
#[test]
fn every_run_names_the_token_that_produced_it() {
    let laid = plain("**bold** and `code`");
    for run in laid.map.runs() {
        assert!(
            run.token.start <= run.block.start && run.block.end <= run.token.end,
            "{run:?}"
        );
    }
    // The `**` runs name the whole `**bold**`, not just themselves.
    let opener = &laid.map.runs()[0];
    assert_eq!(opener.kind, MapKind::Hidden);
    assert_eq!(opener.block, 0..2);
    assert_eq!(opener.token, 0..8);
}

/// A caret in the block text can sit inside a marker, and the honest answer is
/// that it has no visible position — not the offset of the letter beside it.
#[test]
fn block_to_visible_refuses_to_clamp() {
    let laid = plain("**bold**");
    assert_eq!(laid.visible, "bold");
    assert_eq!(laid.map.to_visible(0), None);
    assert_eq!(laid.map.to_visible(1), None);
    assert_eq!(laid.map.to_visible(2), Some(0));
    assert_eq!(laid.map.to_visible(5), Some(3));
    assert_eq!(laid.map.to_visible(6), None);
    assert_eq!(laid.map.to_visible(7), None);
    assert_eq!(laid.map.to_visible(8), Some(4), "the end of the text");
}

/// Non-ASCII everywhere, because every offset in this module is a byte offset
/// and an off-by-one lands inside a code point rather than beside it.
#[test]
fn the_map_is_in_bytes_and_survives_multibyte_text() {
    let text = "中文**粗体**结束";
    let laid = plain(text);
    assert_eq!(laid.visible, "中文粗体结束");
    for v in 0..=laid.visible.len() {
        let b = laid.map.to_block(v);
        assert!(
            text.is_char_boundary(b) || !laid.visible.is_char_boundary(v),
            "visible {v} mapped to block {b}, which is inside a code point"
        );
    }
    every_offset_round_trips(text);
}

/// The corpus lines S2's gate is written against, walked for the invariants
/// rather than for exact strings.
#[test]
fn the_gate_shapes_of_the_corpus_all_tile() {
    for text in [
        "这是一段中文文本，包含**粗体**和*斜体*，以及一个 `代码片段`。",
        "這是一段繁體中文，包含[連結](https://example.com/中文)與圖片 ![替代文字](./x.png)。",
        "**🎉bold🎉** and *🚀italic🚀* and `🐛code🐛` and [🔗link🔗](https://example.com).",
        "Hello 👋 世界 🌏 مرحبا 🕌 שלום 🕎 — one line, four scripts, five emoji.",
        "نص عادي مع **نص عريض** وكلمة Rust في المنتصف.",
        "[رابط](https://example.com/عربي)",
        "H~2~O and 2^n^ and a \\$5 price",
    ] {
        every_offset_round_trips(text);
    }
}

// ---------------------------------------------------------------------------
// Labels, and D12's images
// ---------------------------------------------------------------------------

fn with_labels(pairs: &[(&str, &str)]) -> InlineSyntax {
    InlineSyntax {
        labels: pairs
            .iter()
            .map(|(key, href)| {
                (
                    (*key).to_string(),
                    mt_inline::Label {
                        href: (*href).to_string(),
                        title: String::new(),
                    },
                )
            })
            .collect(),
        ..InlineSyntax::default()
    }
}

/// The exact line from `bench/corpus/10kb.md:53`, which is what a golden would
/// have frozen if the label table had stayed empty.
#[test]
fn a_reference_link_shows_its_anchor_once_the_labels_are_threaded_down() {
    let text = "See [the plan][plan] and more.";

    let without = plain(text);
    assert_eq!(
        without.visible, text,
        "with no label table it is literal brackets, which is what part 1 froze"
    );

    let syntax = with_labels(&[("plan", "https://example.com/plan")]);
    let with = lay_out(text, &syntax, BaseDirection::Ltr, None);
    assert_eq!(with.visible, "See the plan and more.");
    let linked: Vec<_> = with
        .runs
        .iter()
        .filter(|r| r.style.link)
        .map(|r| r.range.clone())
        .collect();
    assert_eq!(linked, vec![4..12], "`the plan`, and not the label");
    assert_eq!(with.map.to_visible(4), None, "the opening `[` is a marker");
    assert_eq!(with.map.to_block(4), 5, "visible `t` is block `t`");
    assert_eq!(
        with.map.to_visible(14),
        None,
        "and so is every byte of `[plan]`"
    );
}

#[test]
fn an_image_contributes_no_visible_text_and_one_box_carrying_its_src() {
    let text = "before ![alt text](./diagram.png \"Architecture\") after";
    let laid = plain(text);
    assert_eq!(
        laid.visible, "before  after",
        "the alt text is an attribute in muya, not something a reader sees"
    );
    assert_eq!(laid.images.len(), 1);
    let image = &laid.images[0];
    assert_eq!(image.src, "./diagram.png");
    assert_eq!(image.visible_index, "before ".len());
    assert_eq!(image.block, 7..text.len() - " after".len());
    every_offset_round_trips(text);
}

#[test]
fn an_empty_src_is_a_real_state_and_reaches_the_box_as_an_empty_string() {
    let laid = plain("![alt]()");
    assert_eq!(laid.visible, "");
    assert_eq!(laid.images.len(), 1);
    assert_eq!(laid.images[0].src, "");
}

/// `ReferenceImage` carries `alt`, `label` and two backslash runs and no
/// `href`, so the `src` has to come back out of the table that made it a
/// reference image at all.
#[test]
fn a_reference_image_takes_its_src_from_the_same_table_that_defined_it() {
    let text = "see ![a diagram][fig] here";
    assert!(
        plain(text).images.is_empty(),
        "with no table this is plain text and not an image at all"
    );

    let syntax = with_labels(&[("fig", "./fig.png")]);
    let laid = lay_out(text, &syntax, BaseDirection::Ltr, None);
    assert_eq!(laid.visible, "see  here");
    assert_eq!(laid.images.len(), 1);
    assert_eq!(laid.images[0].src, "./fig.png");
}

/// M3-R1's workaround exists and fires on exactly one condition. **Not a test
/// of its correctness** — that needs a shaped line and a later phase's gate —
/// only that the construct reaches it and that nothing else does.
#[test]
fn an_image_leading_a_right_to_left_paragraph_is_preceded_by_a_right_to_left_mark() {
    let text = "![diagram](./x.png) مرحبا";

    let ltr = lay_out(text, &InlineSyntax::default(), BaseDirection::Ltr, None);
    assert_eq!(
        ltr.visible, " مرحبا",
        "a left-to-right paragraph is untouched"
    );
    assert_eq!(ltr.images[0].visible_index, 0);

    let rtl = lay_out(text, &InlineSyntax::default(), BaseDirection::Rtl, None);
    assert_eq!(rtl.visible, format!("{RTL_MARK} مرحبا"));
    assert_eq!(
        rtl.images[0].visible_index,
        RTL_MARK.len(),
        "the box sits after the mark, so it is no longer before the first run"
    );
    // The mark stands *for* the image's bytes, so both sides still tile and
    // D13 needed no fourth `MapKind`.
    assert_eq!(rtl.map.runs()[0].kind, MapKind::Substituted);
    assert_eq!(rtl.map.runs()[0].block, 0..19);
    assert_eq!(rtl.map.to_block(0), 0);
    every_offset_round_trips_with(text, BaseDirection::Rtl);

    // An image anywhere but the head of the paragraph is left alone, because
    // parley only mislevels a box that precedes the first shaped run.
    let later = lay_out(
        "مرحبا ![diagram](./x.png)",
        &InlineSyntax::default(),
        BaseDirection::Rtl,
        None,
    );
    assert!(!later.visible.contains(RTL_MARK));
}
