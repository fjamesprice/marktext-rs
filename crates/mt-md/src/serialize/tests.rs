//! Unit tests for the serializer.
//!
//! One reproducer per rule, in the shape `block/tests.rs` uses: the wide claims
//! — that `serialize(parse(s))` reparses to the same tree over 1357 inputs, and
//! that it is the identity on all but an enumerated set — belong to
//! `tests/round_trip.rs`, and what is here is the rule that a failure should
//! name.
//!
//! Every expected value in this file was measured against the running engine
//! (`ExportMarkdown.generate` over `MarkdownToState.generate`), not derived
//! from reading `stateToMarkdown.ts`. Three of them are not what reading it
//! suggests and each says so where it sits.

use super::*;
use crate::parse;
use mt_doc::{Edit, Text, Underline};

const SPEC: Options = Options::SPEC;

/// `markdown → state → markdown`, the shape most of these assert on.
fn round_trip(src: &str) -> String {
    with_options(src, SPEC)
}

fn with_options(src: &str, options: Options) -> String {
    to_markdown(&parse(src, options).document, options)
}

/// Build a document from a literal block tree, for the rules whose input is a
/// `Document` no parse produces.
enum S {
    Leaf(Block),
    Node(Block, Vec<S>),
}

fn doc_of(states: Vec<S>) -> Document {
    let mut doc = Document::new();
    let seed = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: seed }]);
    doc.prune_detached();
    let root = doc.root();
    for (index, state) in states.iter().enumerate() {
        attach(&mut doc, root, index, state);
    }
    doc
}

fn attach(doc: &mut Document, parent: NodeId, index: usize, state: &S) {
    let block = match state {
        S::Leaf(block) | S::Node(block, _) => block.clone(),
    };
    doc.apply(&[Edit::InsertNode {
        parent,
        index,
        block,
    }]);
    let id = doc.children(parent)[index];
    if let S::Node(_, children) = state {
        for (i, child) in children.iter().enumerate() {
            attach(doc, id, i, child);
        }
    }
}

fn cell(text: &str, align: Align) -> S {
    S::Leaf(Block::TableCell {
        align,
        text: Text::from(text),
    })
}

fn row(cells: Vec<S>) -> S {
    S::Node(
        Block::TableRow {
            children: Vec::new(),
        },
        cells,
    )
}

// --- the block kinds, one line each -----------------------------------------

#[test]
fn every_simple_block_kind_round_trips_its_own_syntax() {
    for src in [
        "para\n",
        "# h1\n",
        "###### h6\n",
        "Hello world\n===========\n",
        "Hello world\n-----------\n",
        "---\n",
        "```js\nconst x = 1;\n```\n",
        "```\nplain\n```\n",
        "    indented\n",
        "    line one\n    line two\n",
        "<div>\n  <p>x</p>\n</div>\n",
        "> quoted\n",
        "> first\n> second\n",
        "> outer\n>\n> > inner quoted\n",
        "- a\n- b\n",
        "1. a\n2. b\n",
        "- [ ] a\n- [x] b\n",
        "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n",
    ] {
        assert_eq!(round_trip(src), src, "{src:?}");
    }
}

#[test]
fn math_and_diagram_fences_key_on_their_own_meta() {
    let muya = Options::MUYA_DEFAULT;
    assert_eq!(with_options("$$\nx^2\n$$\n", muya), "$$\nx^2\n$$\n");
    assert_eq!(
        with_options("```math\nx^2\n```\n", muya),
        "```math\nx^2\n```\n"
    );
    // A tilde math fence is promoted by muya and re-emitted with backticks.
    assert_eq!(
        with_options("~~~math\nx^2\n~~~\n", muya),
        "```math\nx^2\n```\n"
    );
    for kind in ["mermaid", "plantuml", "vega-lite", "flowchart", "sequence"] {
        let src = format!("```{kind}\nbody\n```\n");
        assert_eq!(with_options(&src, muya), src, "{kind}");
    }
}

/// The four front-matter delimiter styles, and the fact that the switch is on
/// `lang` — a `yaml` block whose style says `+` still emits `---`.
#[test]
fn front_matter_delimiters_come_from_the_language_except_for_json() {
    let front = Options {
        front_matter: true,
        ..Options::MUYA_DEFAULT
    };
    for src in [
        "---\na: 1\n---\n\nbody\n",
        "+++\na = 1\n+++\n\nbody\n",
        ";;;\n{\"a\": 1}\n;;;\n\nbody\n",
    ] {
        assert_eq!(with_options(src, front), src, "{src:?}");
    }

    let odd = doc_of(vec![S::Leaf(Block::Frontmatter {
        lang: FrontmatterLang::Yaml,
        style: FrontmatterStyle::Plus,
        text: Text::from("a: 1"),
    })]);
    assert_eq!(to_markdown(&odd, SPEC), "---\na: 1\n---\n");
}

// --- the code fence ---------------------------------------------------------

/// #1841 and M0's constraint 3. `stored` is `meta.fenceLength`, absent for a
/// three-backtick fence, so an ordinary block does not grow one.
#[test]
fn the_code_fence_is_the_longest_of_three_the_stored_length_and_the_body() {
    assert_eq!(code_fence_length("x", None), 3);
    assert_eq!(code_fence_length("x", Some(4)), 4);
    // An all-backtick line in the body forces the fence one longer than it.
    assert_eq!(code_fence_length("```\nfoo\n```", None), 4);
    assert_eq!(code_fence_length("`````", None), 6);
    // A line that is backticks *and something else* does not count.
    assert_eq!(code_fence_length("``` js", None), 3);
    // The trim is muya's: an indented all-backtick line still counts.
    assert_eq!(code_fence_length("  ```  ", None), 4);
}

/// §10's "Owed by S1" item 2, discharged. `fence_len` was a `u8`, so this
/// round-tripped as 255 backticks and the block stopped containing its own
/// body.
#[test]
fn a_fence_longer_than_255_characters_survives_the_widened_field() {
    let fence = "`".repeat(300);
    let src = format!("{fence}\nx\n{fence}\n");
    assert_eq!(round_trip(&src), src);
    assert_eq!(code_fence_length("x", Some(300)), 300);
}

/// The empty info string is *falsy* in JavaScript, which is why the fence and
/// the info are two arms rather than one concatenation — and why an absent
/// language never emits the string `undefined`.
#[test]
fn a_language_less_fence_emits_no_info_string() {
    let out = round_trip("```\nplain\n```\n");
    assert_eq!(out, "```\nplain\n```\n");
    assert!(!out.contains("undefined"), "{out:?}");
}

// --- tables -----------------------------------------------------------------

/// `escapeText`'s negative lookbehind, in the three shapes marktext #3563's
/// `/([^\\])\|/g` got wrong.
#[test]
fn escape_text_escapes_a_leading_pipe_and_both_of_a_consecutive_pair() {
    assert_eq!(escape_text("|lead"), "\\|lead");
    assert_eq!(escape_text("a||b"), "a\\|\\|b");
    assert_eq!(escape_text("a|b"), "a\\|b");
    // Already escaped: the lookbehind is what stops it doubling.
    assert_eq!(escape_text("a\\|b"), "a\\|b");
    assert_eq!(escape_text("no pipes"), "no pipes");
}

/// The column width is `max(5, visual width + 2)` per column, measured with
/// muya's `stringWidth` — so a combining mark costs nothing and a wide
/// character costs two.
#[test]
fn table_columns_are_padded_by_visual_width() {
    let doc = doc_of(vec![S::Node(
        Block::Table {
            children: Vec::new(),
        },
        vec![
            row(vec![cell("A", Align::None)]),
            row(vec![cell("n\u{254}x", Align::None)]),
            row(vec![cell("a\u{28a}\u{32f}x", Align::None)]),
        ],
    )]);
    assert_eq!(
        to_markdown(&doc, SPEC),
        "| A   |\n| --- |\n| n\u{254}x |\n| a\u{28a}\u{32f}x |\n"
    );

    let wide = doc_of(vec![S::Node(
        Block::Table {
            children: Vec::new(),
        },
        vec![
            row(vec![cell("id", Align::None)]),
            row(vec![cell("中文", Align::None)]),
        ],
    )]);
    assert_eq!(to_markdown(&wide, SPEC), "| id   |\n| ---- |\n| 中文 |\n");
}

/// The delimiter row comes from the **header** row's aligns; a body row's own
/// `align` is never read.
#[test]
fn the_delimiter_row_is_the_header_rows_alignment() {
    let doc = doc_of(vec![S::Node(
        Block::Table {
            children: Vec::new(),
        },
        vec![
            row(vec![cell("a", Align::Center), cell("b", Align::None)]),
            row(vec![cell("1", Align::Right), cell("2", Align::Left)]),
        ],
    )]);
    let md = to_markdown(&doc, SPEC);
    assert_eq!(md.lines().nth(1), Some("|:---:| --- |"));
}

/// `stateToMarkdown.spec.ts`'s two degraded-table cases, which are the reason
/// this function is infallible. Neither shape is producible by `parse`.
#[test]
fn a_body_row_wider_or_narrower_than_the_header_degrades_rather_than_failing() {
    let table = |rows: Vec<S>| {
        doc_of(vec![S::Node(
            Block::Table {
                children: Vec::new(),
            },
            rows,
        )])
    };

    let wide = table(vec![
        row(vec![cell("a", Align::None), cell("b", Align::None)]),
        row(vec![
            cell("1", Align::None),
            cell("2", Align::None),
            cell("3", Align::None),
        ]),
    ]);
    let md = to_markdown(&wide, SPEC);
    assert!(md.contains("| a"), "{md:?}");
    assert!(!md.contains("| 3"), "surplus cells are dropped: {md:?}");

    let narrow = table(vec![
        row(vec![
            cell("a", Align::None),
            cell("b", Align::None),
            cell("c", Align::None),
        ]),
        row(vec![cell("1", Align::None)]),
    ]);
    let md = to_markdown(&narrow, SPEC);
    assert_eq!(md.lines().last(), Some("| 1   |"));
}

/// The module docs' table, as a test. All three throw a `TypeError` in muya or
/// warn and emit nothing; none is reachable from `parse`.
#[test]
fn the_three_shapes_that_throw_in_javascript_emit_nothing_here() {
    let empty_table = doc_of(vec![S::Node(
        Block::Table {
            children: Vec::new(),
        },
        vec![],
    )]);
    assert_eq!(to_markdown(&empty_table, SPEC), "");

    let orphan_item = doc_of(vec![S::Node(
        Block::ListItem {
            children: Vec::new(),
        },
        vec![S::Leaf(Block::Paragraph {
            text: Text::from("x"),
        })],
    )]);
    assert_eq!(to_markdown(&orphan_item, SPEC), "");

    let orphan_row = doc_of(vec![row(vec![cell("x", Align::None)])]);
    assert_eq!(to_markdown(&orphan_row, SPEC), "");
}

// --- lists ------------------------------------------------------------------

/// `meta.loose` is the serializer's only consumer, and it is what puts a blank
/// line between items.
#[test]
fn looseness_decides_the_blank_lines_between_items() {
    assert_eq!(round_trip("- a\n- b\n"), "- a\n- b\n");
    assert_eq!(round_trip("- a\n\n- b\n"), "- a\n\n- b\n");
}

/// The item's continuation indent is the marker width — computed **before**
/// the task marker is appended, which is why a task item's second line is two
/// columns in and not six.
#[test]
fn a_task_items_continuation_indent_is_the_bullet_width_not_the_marker_width() {
    assert_eq!(round_trip("- [ ] a\n\n  more\n"), "- [ ] a\n\n  more\n");
}

/// `SETEXT_SAFE_BULLET_MARKER`. A nested list of empty `-` items would
/// serialize as `- ` lines that reparse as a setext underline, so the nested
/// marker becomes `*` rather than the parent list becoming loose.
#[test]
fn an_empty_nested_dash_list_under_a_paragraph_switches_to_a_star_marker() {
    let empty_item = || {
        S::Node(
            Block::ListItem {
                children: Vec::new(),
            },
            vec![S::Leaf(Block::Paragraph { text: Text::new() })],
        )
    };
    let doc = doc_of(vec![S::Node(
        Block::BulletList {
            marker: BulletMarker::Dash,
            loose: false,
            children: Vec::new(),
        },
        vec![S::Node(
            Block::ListItem {
                children: Vec::new(),
            },
            vec![
                S::Leaf(Block::Paragraph {
                    text: Text::from("a"),
                }),
                S::Node(
                    Block::BulletList {
                        marker: BulletMarker::Dash,
                        loose: false,
                        children: Vec::new(),
                    },
                    vec![empty_item(), empty_item()],
                ),
            ],
        )],
    )]);
    assert_eq!(to_markdown(&doc, SPEC), "- a\n  * \n  * \n");
}

/// The ordered list's number is `meta.start + i`, and the stored start is
/// mutated on a **clone**, so serializing twice gives the same answer.
#[test]
fn an_ordered_lists_numbering_starts_at_meta_start_and_does_not_renumber_the_document() {
    let parsed = parse("3. one\n4. two\n", SPEC).document;
    assert_eq!(to_markdown(&parsed, SPEC), "3. one\n4. two\n");
    assert_eq!(to_markdown(&parsed, SPEC), "3. one\n4. two\n");
}

/// The `listIndentation` option, which is the only one this function reads.
#[test]
fn list_indentation_shifts_a_nested_list_by_one_level_per_count() {
    let src = "- a\n  - b\n";
    for (indentation, expected) in [
        (ListIndentation::Spaces(1), "- a\n  - b\n"),
        (ListIndentation::Spaces(2), "- a\n   - b\n"),
        (ListIndentation::Spaces(3), "- a\n    - b\n"),
        (ListIndentation::Spaces(4), "- a\n     - b\n"),
        (ListIndentation::Dfm, "- a\n    - b\n"),
    ] {
        let options = SPEC.with_list_indentation(indentation);
        assert_eq!(with_options(src, options), expected, "{indentation:?}");
    }
}

/// Daring Fireball caps the emitted number at 99 while the *stored* start keeps
/// advancing, so a list that trips the cap emits `1.` twice rather than
/// restarting from 1 and counting up.
#[test]
fn daring_fireball_caps_the_item_number_without_resetting_the_counter() {
    let doc = doc_of(vec![S::Node(
        Block::OrderList {
            start: 100,
            delimiter: OrderDelim::Period,
            loose: false,
            children: Vec::new(),
        },
        vec![
            S::Node(
                Block::ListItem {
                    children: Vec::new(),
                },
                vec![S::Leaf(Block::Paragraph {
                    text: Text::from("a"),
                })],
            ),
            S::Node(
                Block::ListItem {
                    children: Vec::new(),
                },
                vec![S::Leaf(Block::Paragraph {
                    text: Text::from("b"),
                })],
            ),
        ],
    )]);
    assert_eq!(
        to_markdown(&doc, SPEC.with_list_indentation(ListIndentation::Dfm)),
        "1. a\n1. b\n"
    );
    // Under a numeric indentation the cap is 999,999,999 and 100 is under it.
    assert_eq!(to_markdown(&doc, SPEC), "100. a\n101. b\n");
}

// --- indentation and blank lines --------------------------------------------

/// `_insertLineBreak` strips the trailing run of *spaces* from the indent but
/// keeps a `>` — a blank line inside a list item is empty, and a blank line
/// inside a block quote still carries the quote.
#[test]
fn a_blank_line_carries_a_quote_marker_but_never_trailing_spaces() {
    let out = round_trip("- a\n\n  b\n");
    assert_eq!(out, "- a\n\n  b\n");
    assert!(
        out.lines()
            .all(|l| l.trim_end() == l || !l.trim().is_empty()),
        "{out:?}"
    );
    assert_eq!(round_trip("> a\n>\n> b\n"), "> a\n>\n> b\n");
}

// --- the atx heading rebuild ------------------------------------------------

/// The regex is unanchored, `.` stops at a newline, and a text with no `#` at
/// all interpolates two `undefined`s. All three are muya's, none is reachable
/// from `parse`, and the last is the one a rewrite would "fix".
#[test]
fn the_atx_heading_regex_is_transcribed_including_the_shape_that_reads_as_a_bug() {
    let heading = |text: &str| {
        to_markdown(
            &doc_of(vec![S::Leaf(Block::AtxHeading {
                level: 1,
                text: Text::from(text),
            })]),
            SPEC,
        )
    };
    assert_eq!(heading("# Foo"), "# Foo\n");
    assert_eq!(heading("###   Foo   "), "### Foo\n");
    // Unanchored: the rebuild starts at the first `#` anywhere.
    assert_eq!(heading("junk # Foo"), "# Foo\n");
    // `.` does not match a newline.
    assert_eq!(heading("# Foo\nbar"), "# Foo\n");
    // Seven hashes: `#{1,6}` is greedy and takes six, the seventh is content.
    assert_eq!(heading("####### Foo"), "###### # Foo\n");
    assert_eq!(heading("no hash"), "undefined undefined\n");
}

/// The setext heading trims its text as a whole and its underline separately,
/// and the underline is the literal run — the correction S1 had to make to
/// `mt_doc::Underline` is what makes an eleven-character underline survive.
#[test]
fn a_setext_underline_is_re_emitted_at_its_own_length() {
    assert_eq!(
        round_trip("Hello world\n===========\n"),
        "Hello world\n===========\n"
    );
    let doc = doc_of(vec![S::Leaf(Block::SetextHeading {
        level: 2,
        underline: Underline::Dashes(1),
        text: Text::from("x"),
    })]);
    assert_eq!(to_markdown(&doc, SPEC), "x\n-\n");
}

// --- footnotes --------------------------------------------------------------

/// The `[^id]: ` prefix replaces the **first** occurrence of the inner indent,
/// wherever it is — `String.prototype.replace` with a string pattern, not a
/// prefix strip.
#[test]
fn a_footnote_puts_its_label_on_the_first_line_and_indents_the_rest() {
    let doc = doc_of(vec![S::Node(
        Block::Footnote {
            identifier: "1".to_string(),
            children: Vec::new(),
        },
        vec![
            S::Leaf(Block::Paragraph {
                text: Text::from("first"),
            }),
            S::Leaf(Block::Paragraph {
                text: Text::from("second"),
            }),
        ],
    )]);
    assert_eq!(to_markdown(&doc, SPEC), "[^1]: first\n\n    second\n");
}
