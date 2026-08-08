//! Unit tests for the mapping layer.
//!
//! These are the mechanisms with a named input, in the shape register rule 2
//! asks for: *"every entry names concrete inputs, and those inputs become
//! tests"*. The wide claim — that the tree agrees with `MarkdownToState` over
//! 1344 inputs — is `cargo xtask blocks`'s, not these; what is here is one
//! reproducer per mechanism, so that a regression names the mechanism rather
//! than a count.

use super::*;
use crate::state::to_state;
use serde_json::{Value, json};

fn state(src: &str, options: Options) -> Value {
    to_state(&parse_blocks(src, options))
}

fn names(src: &str, options: Options) -> Vec<String> {
    fn walk(value: &Value, depth: usize, out: &mut Vec<String>) {
        for block in value.as_array().into_iter().flatten() {
            out.push(format!(
                "{}{}",
                "  ".repeat(depth),
                block["name"].as_str().unwrap_or("?")
            ));
            if let Some(children) = block.get("children") {
                walk(children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&state(src, options), 0, &mut out);
    out
}

const SPEC: Options = Options::SPEC;
const MUYA: Options = Options::MUYA_DEFAULT;

// --- the empty document -----------------------------------------------------

/// `states.length ? states : [{ name: 'paragraph', text: '' }]`.
#[test]
fn an_empty_document_is_one_empty_paragraph() {
    assert_eq!(names("", SPEC), ["paragraph"]);
    assert_eq!(names("\n\n", SPEC), ["paragraph"]);
}

// --- mechanism 5: the tight list item's synthetic paragraph -----------------

/// The 90-case mistake §4 C1 records: a tight item's inline events arrive with
/// no `Paragraph` wrapper, and a mapping that only builds nodes for block tags
/// leaves every tight item childless.
#[test]
fn a_tight_list_item_gets_a_paragraph_that_pulldown_cmark_does_not_emit() {
    assert_eq!(
        names("- a\n- b\n", SPEC),
        [
            "bullet-list",
            "  list-item",
            "    paragraph",
            "  list-item",
            "    paragraph"
        ]
    );
}

/// The 5-case mistake: the synthetic paragraph must be closed by a **block**
/// tag. Closed on any `Start`, this item becomes three paragraphs.
#[test]
fn emphasis_inside_a_tight_item_does_not_split_its_paragraph() {
    assert_eq!(
        names("- *b* c\n", SPEC),
        ["bullet-list", "  list-item", "    paragraph"]
    );
    assert_eq!(
        names("- a [x](y) ~~z~~ b\n", SPEC),
        ["bullet-list", "  list-item", "    paragraph"]
    );
}

/// …and a real block child does close it, so an item holding a paragraph and a
/// fence has two children rather than one.
#[test]
fn a_block_child_closes_a_tight_items_synthetic_paragraph() {
    assert_eq!(
        names("- a\n  ```\n  x\n  ```\n", SPEC),
        [
            "bullet-list",
            "  list-item",
            "    paragraph",
            "    code-block"
        ]
    );
}

// --- mechanism 2: empty containers (muya #1735) -----------------------------

#[test]
fn an_empty_block_quote_holds_a_synthetic_empty_paragraph() {
    assert_eq!(
        names(">\nbar\n", SPEC),
        ["block-quote", "  paragraph", "paragraph"]
    );
    assert_eq!(names("> \n> \n", SPEC), ["block-quote", "  paragraph"]);
}

#[test]
fn an_empty_list_item_holds_a_synthetic_empty_paragraph() {
    assert_eq!(
        names("-\n", SPEC),
        ["bullet-list", "  list-item", "    paragraph"]
    );
    assert_eq!(
        names("- a\n- \n- c\n", SPEC),
        [
            "bullet-list",
            "  list-item",
            "    paragraph",
            "  list-item",
            "    paragraph",
            "  list-item",
            "    paragraph",
        ]
    );
}

/// The synthesis is a block quote's and a list item's, not a list's and not a
/// table's — `markdownToState`'s `block-end` case tests
/// `tokenType === 'blockquote' || tokenType === 'list-item'`.
#[test]
fn a_table_row_with_no_cells_gets_no_filler() {
    // `| a |\n| - |` is a header-only table: one row, one cell, no filler
    // anywhere.
    assert_eq!(
        names("| a |\n| - |\n", SPEC),
        ["table", "  table.row", "    table.cell"]
    );
}

// --- mechanism 3: compatibleTaskList ----------------------------------------

/// `spec/fixtures/marktext-round-trip/common/Links.md`'s shape: GFM sees one
/// list of four, muya sees a `bullet-list` of two then a `task-list` of two.
#[test]
fn a_list_splits_where_task_ness_changes() {
    assert_eq!(
        names("- a\n- b\n- [ ] c\n- [x] d\n", SPEC),
        [
            "bullet-list",
            "  list-item",
            "    paragraph",
            "  list-item",
            "    paragraph",
            "task-list",
            "  task-list-item",
            "    paragraph",
            "  task-list-item",
            "    paragraph",
        ]
    );
}

/// An ordered list never splits, and its items are never task items — even
/// though GFM does flag the marker. Measured against the running engine:
/// `1. [ ] a` is an `order-list` holding a `list-item`.
#[test]
fn an_ordered_list_never_splits_into_task_items() {
    assert_eq!(
        names("1. [ ] a\n", SPEC),
        ["order-list", "  list-item", "    paragraph"]
    );
}

#[test]
fn each_split_part_takes_its_marker_from_its_own_first_item() {
    let s = state("* a\n* [ ] b\n", SPEC);
    assert_eq!(s[0]["meta"]["marker"], json!("*"));
    assert_eq!(s[1]["meta"]["marker"], json!("*"));
    assert_eq!(s[1]["name"], json!("task-list"));
}

// --- mechanism 1: reference definitions -------------------------------------

#[test]
fn a_reference_definition_becomes_a_paragraph_in_tree_position() {
    assert_eq!(
        names("[foo]: /a\n\n[foo]\n", SPEC),
        ["paragraph", "paragraph"]
    );
}

/// D3's first surviving reason: the definition belongs to the block quote, and
/// `RefDefs`' span for this input excludes the `> ` that says so.
#[test]
fn a_definition_inside_a_block_quote_stays_inside_it() {
    assert_eq!(
        names("> [foo]: /url\n", SPEC),
        ["block-quote", "  paragraph"]
    );
}

/// D3's second surviving reason, and the one M2.md did not have: a definition
/// after a paragraph at the same level is absorbed into it, so there is one
/// block and not two. Reproduced by construction — `pulldown-cmark` keeps both
/// lines in one paragraph, so the scan never sees a gap.
#[test]
fn a_definition_after_a_paragraph_is_absorbed_into_it() {
    assert_eq!(names("text\n[foo]: /a\n", SPEC), ["paragraph"]);
    // …and one *before* a paragraph is not.
    assert_eq!(names("[foo]: /a\ntext\n", SPEC), ["paragraph", "paragraph"]);
}

/// M2.md §5 D3's reproducer, corrected. `marked` drops a repeated label
/// exactly as `RefDefs` does — measured, muya emits **one** paragraph — so the
/// port drops it too. The label set is document-global and case-insensitive.
#[test]
fn a_repeated_definition_label_emits_no_second_paragraph() {
    assert_eq!(
        names("[foo]: /a\n[foo]: /b\n\n[foo]\n", SPEC),
        ["paragraph", "paragraph"]
    );
    assert_eq!(
        names("[FOO]: /a\n[foo]: /b\n\n[Foo]\n", SPEC),
        ["paragraph", "paragraph"]
    );
    // Distinct labels are two definitions and therefore two paragraphs.
    assert_eq!(
        names("[foo]: /a\n[bar]: /b\n\n[foo]\n", SPEC),
        ["paragraph", "paragraph", "paragraph"]
    );
}

#[test]
fn a_multi_line_definition_is_one_paragraph() {
    assert_eq!(names("[foo]:\n/a\n\"t\"\n", SPEC), ["paragraph"]);
}

// --- mechanism 4: block math ------------------------------------------------

#[test]
fn dollar_dollar_math_is_a_math_block_only_when_math_is_on() {
    assert_eq!(names("$$\nx^2\n$$\n", MUYA), ["math-block"]);
    assert_eq!(
        state("$$\nx^2\n$$\n", MUYA)[0]["meta"]["mathStyle"],
        json!("")
    );
    assert_eq!(names("$$\nx^2\n$$\n", SPEC), ["paragraph"]);
}

/// The rule ends at the closing delimiter, so what follows is its own
/// paragraph — even though `pulldown-cmark` put both in one.
#[test]
fn text_after_a_math_block_is_a_separate_paragraph() {
    assert_eq!(
        names("$$\nx\n$$\nmore\n", MUYA),
        ["math-block", "paragraph"]
    );
}

#[test]
fn block_math_is_recognised_inside_a_container() {
    assert_eq!(
        names("> $$\n> x\n> $$\n", MUYA),
        ["block-quote", "  math-block"]
    );
}

/// `walkTokens` rewrites a ```` ```math ```` fence in place, and only when both
/// `math` and `isGitlabCompatibilityEnabled` are on.
#[test]
fn a_math_fence_is_gitlab_math_under_muya_defaults() {
    assert_eq!(names("```math\nx\n```\n", MUYA), ["math-block"]);
    assert_eq!(
        state("```math\nx\n```\n", MUYA)[0]["meta"]["mathStyle"],
        json!("gitlab")
    );
    assert_eq!(names("```math\nx\n```\n", SPEC), ["code-block"]);
    // The test is on the whole info string, not its first word.
    assert_eq!(names("```math title=x\nx\n```\n", MUYA), ["code-block"]);
}

/// The greedy-then-backtrack shape of `\${1,2}`: three dollars match nothing.
#[test]
fn three_dollars_are_not_a_math_fence() {
    assert_eq!(block_math("$$$\nx\n$$$\n"), None);
    assert_eq!(
        block_math("$\nx\n$\n"),
        Some(BlockMath {
            content: 2..3,
            consumed: 6
        })
    );
    assert_eq!(
        block_math("$$\nx\n$$\n"),
        Some(BlockMath {
            content: 3..4,
            consumed: 8
        })
    );
    // A backslash consumes the following newline, so it cannot close the block.
    assert_eq!(block_math("$$\nx\\\n$$\n"), None);
}

// --- what S1 does not do, recorded rather than left silent -------------------

/// `Options::footnote` gates muya's own `marked` block extension, and this is
/// where that stopped being a gap.
///
/// S1 wrote this test to record that the flag did **nothing** —
/// `pulldown-cmark`'s `ENABLE_FOOTNOTES` is GFM's syntax and not muya's, so it
/// is not a substitute and stays off ([`cmark_options`]). S3 answered the half
/// that was about the label map (`[^a]: note` registers the label `^a` with
/// footnotes *off*, in both engines). S5 landed the extension itself, so the
/// test is flipped rather than deleted: the `false` arm still asserts exactly
/// what S1 asserted, because both option sets the harnesses drive have
/// `footnote: false` and that behaviour may not move.
///
/// Every expected value below was measured from the running engine with
/// `{ footnote: true }`, not derived from the regex.
#[test]
fn the_footnote_option_gates_muyas_own_block_extension() {
    let on = Options {
        footnote: true,
        ..MUYA
    };

    // Off — S1's assertions, unchanged. This is the arm the 1344 exercise.
    assert_eq!(names("[^a]: note\n", MUYA), ["paragraph"]);
    assert_eq!(
        names("text\n\n[^a]: note\n", MUYA),
        ["paragraph", "paragraph"]
    );

    // On — a container, with the identifier in `meta` and the body re-lexed.
    let s = state("text[^1]\n\n[^1]: definition\n", on);
    assert_eq!(s[0]["name"], json!("paragraph"));
    assert_eq!(s[1]["name"], json!("footnote"));
    assert_eq!(s[1]["meta"], json!({ "identifier": "1" }));
    assert_eq!(s[1]["children"][0]["text"], json!("definition"));

    // The four-space de-indent, and the recursive lex it exists for: without
    // it the body is an indented code block rather than a list.
    let s = state("t[^n]\n\n[^n]: intro\n\n    - item a\n    - item b\n", on);
    assert_eq!(
        names_of(&s[1]),
        ["paragraph", "bullet-list"],
        "measured from muya with footnote: true"
    );

    // The identifier class excludes `^`, `[`, `]` and whitespace, and the
    // lookbehind lets a backslash escape the closing bracket.
    assert_eq!(names("[^a b]: note\n", on), ["paragraph"]);
    assert_eq!(names("[^^a]: note\n", on), ["paragraph"]);
    assert_eq!(names("[^a\\]: note\n", on), ["paragraph"]);

    // A definition may interrupt a paragraph — that is what the extension's
    // `start()` hook buys, and `pulldown-cmark` has no such rule.
    assert_eq!(
        names("Lorem\n[^1]: def\n", on),
        ["paragraph", "footnote", "  paragraph"]
    );

    // …but not inside a fence, where the lexer never reaches a token boundary.
    assert_eq!(names("```\n[^1]: def\n```\n", on), ["code-block"]);
}

/// The names of a state's children, for the nested assertions above.
fn names_of(state: &serde_json::Value) -> Vec<String> {
    state["children"]
        .as_array()
        .expect("a container")
        .iter()
        .map(|child| child["name"].as_str().expect("a name").to_string())
        .collect()
}

// --- mechanism 6: front matter ----------------------------------------------

#[test]
fn front_matter_is_split_off_before_the_parser_sees_it() {
    let s = state("---\na: 1\n---\n\n# H\n", MUYA);
    assert_eq!(s[0]["name"], json!("frontmatter"));
    assert_eq!(s[0]["meta"], json!({ "lang": "yaml", "style": "-" }));
    assert_eq!(s[1]["name"], json!("atx-heading"));
    // Off by option, and off for the spec suites.
    assert_ne!(
        state("---\na: 1\n---\n\n# H\n", SPEC)[0]["name"],
        json!("frontmatter")
    );
}

#[test]
fn the_four_front_matter_delimiters_map_to_their_langs() {
    for (src, lang, style) in [
        ("---\nx\n---\n\ny\n", "yaml", "-"),
        ("+++\nx\n+++\n\ny\n", "toml", "+"),
        (";;;\nx\n;;;\n\ny\n", "json", ";"),
        ("{\nx\n}\n\ny\n", "json", "{"),
    ] {
        let s = state(src, MUYA);
        assert_eq!(s[0]["name"], json!("frontmatter"), "{src:?}");
        assert_eq!(
            s[0]["meta"],
            json!({ "lang": lang, "style": style }),
            "{src:?}"
        );
    }
}

/// The trailing rule is `\n{2,}` or one-or-two newlines at end of input — so a
/// single newline before a following paragraph is not front matter at all.
#[test]
fn front_matter_needs_a_blank_line_after_it_unless_it_ends_the_document() {
    assert_ne!(
        state("---\nx\n---\ny\n", MUYA)[0]["name"],
        json!("frontmatter")
    );
    assert_eq!(
        state("---\nx\n---\n", MUYA)[0]["name"],
        json!("frontmatter")
    );
}

// --- mechanism 7: meta recovery ---------------------------------------------

/// The correction to `mt_doc::Underline`: `meta.underline` is the literal run,
/// which the measurement `"Foo\n====\n" → underline "===="` is the reproducer
/// for.
#[test]
fn a_setext_underline_keeps_its_length() {
    assert_eq!(
        state("Foo\n====\n", SPEC)[0]["meta"],
        json!({ "level": 1, "underline": "====" })
    );
    assert_eq!(
        state("Foo\n-\n", SPEC)[0]["meta"],
        json!({ "level": 2, "underline": "-" })
    );
    assert_eq!(
        state("Hello world\n===========\n", SPEC)[0]["meta"],
        json!({ "level": 1, "underline": "===========" })
    );
}

#[test]
fn an_atx_heading_is_never_read_as_setext() {
    assert_eq!(state("# Foo #\n", SPEC)[0]["name"], json!("atx-heading"));
    assert_eq!(state("###### x\n", SPEC)[0]["meta"], json!({ "level": 6 }));
}

#[test]
fn a_fence_longer_than_three_carries_its_length_and_three_does_not() {
    assert_eq!(
        state("```\nx\n```\n", SPEC)[0]["meta"],
        json!({ "type": "fenced", "lang": "" })
    );
    assert_eq!(
        state("````\nx\n````\n", SPEC)[0]["meta"],
        json!({ "type": "fenced", "lang": "", "fenceLength": 4 })
    );
    assert_eq!(
        state("~~~~ruby\nz\n~~~~\n", SPEC)[0]["meta"],
        json!({ "type": "fenced", "lang": "ruby", "fenceLength": 4 })
    );
}

#[test]
fn an_indented_code_block_is_indented_with_an_empty_lang() {
    assert_eq!(
        state("    let a = 1\n", SPEC)[0]["meta"],
        json!({ "type": "indented", "lang": "" })
    );
}

/// Constraint 1: the whole info string, never the first word.
#[test]
fn a_code_fence_keeps_its_whole_info_string() {
    assert_eq!(
        state("```  js  title=x  \nz\n```\n", SPEC)[0]["meta"]["lang"],
        json!("js  title=x")
    );
}

#[test]
fn the_five_diagram_languages_become_diagram_blocks() {
    for (lang, kind, diagram_lang) in [
        ("mermaid", "mermaid", "yaml"),
        ("plantuml", "plantuml", "yaml"),
        ("vega-lite", "vega-lite", "json"),
        ("flowchart", "flowchart", "yaml"),
        ("sequence", "sequence", "yaml"),
    ] {
        let s = state(&format!("```{lang}\nx\n```\n"), SPEC);
        assert_eq!(s[0]["name"], json!("diagram"), "{lang}");
        assert_eq!(s[0]["meta"], json!({ "type": kind, "lang": diagram_lang }));
    }
    // The *first word* decides, so an info string with arguments still counts.
    assert_eq!(
        state("```mermaid x\nz\n```\n", SPEC)[0]["name"],
        json!("diagram")
    );
}

#[test]
fn an_ordered_lists_start_and_delimiter_come_from_the_source() {
    assert_eq!(
        state("5. x\n", SPEC)[0]["meta"],
        json!({ "loose": false, "start": 5, "delimiter": "." })
    );
    assert_eq!(
        state("1) x\n2) y\n", SPEC)[0]["meta"],
        json!({ "loose": false, "start": 1, "delimiter": ")" })
    );
    assert_eq!(state("0. x\n", SPEC)[0]["meta"]["start"], json!(0));
}

#[test]
fn the_three_bullet_markers_are_kept() {
    for marker in ["-", "+", "*"] {
        assert_eq!(
            state(&format!("{marker} x\n"), SPEC)[0]["meta"]["marker"],
            json!(marker)
        );
    }
}

#[test]
fn a_blank_line_between_items_makes_the_list_loose() {
    assert_eq!(state("- a\n- b\n", SPEC)[0]["meta"]["loose"], json!(false));
    assert_eq!(state("- a\n\n- b\n", SPEC)[0]["meta"]["loose"], json!(true));
}

/// The looseness case `pulldown-cmark`'s paragraph wrapping cannot show,
/// because there is no paragraph anywhere in the list.
#[test]
fn a_loose_list_of_code_blocks_is_still_loose() {
    let src = "- ```\n  x\n  ```\n\n- ```\n  y\n  ```\n";
    assert_eq!(state(src, SPEC)[0]["meta"]["loose"], json!(true));
    let tight = "- ```\n  x\n  ```\n- ```\n  y\n  ```\n";
    assert_eq!(state(tight, SPEC)[0]["meta"]["loose"], json!(false));
}

#[test]
fn table_cell_alignment_comes_from_the_delimiter_row() {
    let s = state("| a | b | c |\n| :- | :-: | -: |\n| 1 | 2 | 3 |\n", SPEC);
    let row = |i: usize| {
        s[0]["children"][i]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["meta"]["align"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(row(0), ["left", "center", "right"]);
    assert_eq!(row(1), ["left", "center", "right"]);
}

#[test]
fn a_task_items_checked_state_is_its_meta() {
    let s = state("- [ ] a\n- [x] b\n", SPEC);
    assert_eq!(s[0]["children"][0]["meta"], json!({ "checked": false }));
    assert_eq!(s[0]["children"][1]["meta"], json!({ "checked": true }));
}

/// `- [ ]` alone: `marked`'s own `listIsTask` needs a non-space after the
/// marker, and `compatibleTaskList.normalizeEmptyTaskItem` is what makes this
/// a task item anyway. Measured against the running engine.
#[test]
fn an_empty_checkbox_is_still_a_task_item() {
    assert_eq!(
        names("- [ ]\n", SPEC),
        ["task-list", "  task-list-item", "    paragraph"]
    );
    // …and a checkbox with no space after it is not a task at all.
    assert_eq!(
        names("- [x]b\n", SPEC),
        ["bullet-list", "  list-item", "    paragraph"]
    );
}

// --- the html block's one special case --------------------------------------

#[test]
fn an_html_block_holding_one_image_is_a_paragraph() {
    assert_eq!(names("<img src=\"x\">\n", SPEC), ["paragraph"]);
    assert_eq!(names("<div>\nx\n</div>\n", SPEC), ["html-block"]);
    assert_eq!(
        names("<img src=\"x\">\n<img src=\"y\">\n", SPEC),
        ["html-block"]
    );
}

// --- the document, not just the tree ----------------------------------------

/// Every block reaches the arena through `Document::apply`, so a parse leaves
/// a document whose invariants hold: parents name their children, the root has
/// no stray placeholder, and nothing is detached.
#[test]
fn a_parse_leaves_a_well_formed_document() {
    let doc = parse_blocks("# H\n\n> q\n\n- a\n- b\n", MUYA);
    let root = doc.root();
    assert_eq!(doc.children(root).len(), 3);
    for id in doc.children(root) {
        assert_eq!(doc.node(*id).parent(), Some(root));
    }
    let before = doc.arena_len();
    let mut doc = doc;
    assert_eq!(doc.prune_detached(), 0, "a parse leaves nothing detached");
    assert_eq!(doc.arena_len(), before);
}

#[test]
fn the_placeholder_paragraph_does_not_survive_a_non_empty_parse() {
    let doc = parse_blocks("# H\n", SPEC);
    assert_eq!(doc.children(doc.root()).len(), 1);
    assert_eq!(
        doc.block(doc.children(doc.root())[0])
            .map(mt_doc::Block::name),
        Some("atx-heading")
    );
}

// --- the helpers, where a bug would be silent -------------------------------

#[test]
fn logical_offsets_map_back_to_source_offsets() {
    let src = "> $$\n> x\n> $$\n";
    let logical = LogicalText::of(src, 0..src.len(), &[Prefix::Quote { first_line: 0 }]);
    assert_eq!(logical.text, "$$\nx\n$$\n");
    assert_eq!(logical.source_offset(0), 2);
    assert_eq!(logical.source_offset(3), 7);
    assert_eq!(logical.source_offset(5), 11);
}

#[test]
fn a_label_is_normalised_the_way_marked_normalises_it() {
    assert_eq!(normalise_label("Foo"), "foo");
    assert_eq!(normalise_label("A  B\tC"), "a b c");
}

#[test]
fn has_blank_line_needs_two_newlines_with_only_spaces_between() {
    assert!(has_blank_line("\n\n"));
    assert!(has_blank_line("a\n  \nb"));
    assert!(!has_blank_line("a\nb\n"));
    assert!(!has_blank_line("\n"));
}

// ===========================================================================
// S2 — leaf text (M2.md §5 D2)
// ===========================================================================
//
// One reproducer per rule, each measured against the running engine before it
// was written down. The wide claim — 4,985 of 4,985 leaves agree — is
// `cargo xtask blocks`'s; these name the mechanism a regression broke.

/// Every leaf's text, in document order, so a rule can be asserted without
/// spelling out the whole `TState` tree.
fn texts(src: &str, options: Options) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        for block in value.as_array().into_iter().flatten() {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                out.push(text.to_string());
            }
            if let Some(children) = block.get("children") {
                walk(children, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&state(src, options), &mut out);
    out
}

// --- the container-prefix stripper ------------------------------------------

/// D2's *"one part that is a scanner rather than a rewrite"*, and the reason
/// `content_start` was deleted rather than promoted: it stripped **all**
/// leading whitespace and **every** `>` run, and both of those are content
/// here.
#[test]
fn container_prefixes_come_off_and_the_blocks_own_indent_stays() {
    assert_eq!(texts("> hello\n> world\n", SPEC), ["hello\nworld"]);
    assert_eq!(texts("- foo\n  bar\n", SPEC), ["foo\nbar"]);
    // One `>` per level, and no more: the second is content at depth one.
    assert_eq!(texts("> > a\n", SPEC), ["a"]);
    // The block's own ` {0,3}` indent is `marked`'s `cap[0]` and survives —
    // `pulldown-cmark`'s range starts after it, which was 30 of S2's first 47
    // disagreements.
    assert_eq!(texts("  aaa\n bbb\n", SPEC), ["  aaa\n bbb"]);
    assert_eq!(texts(" ***\n", SPEC), [" ***"]);
    assert_eq!(texts(">    not code\n", SPEC), ["   not code"]);
}

/// `Tokenizer.list`'s `indent > 4 ? 1 : indent`, which keeps a six-space item
/// from swallowing what is really an indented code block.
#[test]
fn a_list_items_dedent_is_capped_at_four_columns() {
    assert_eq!(
        texts("1.      indented code\n\n   paragraph\n", SPEC),
        [" indented code", "paragraph"]
    );
}

/// `nextLineWithoutTabs = nextLine.replace(/\t/g, '    ')` — flat, not
/// tab-stop aware, and applied to the whole line rather than to its indent.
#[test]
fn a_tab_in_a_list_items_continuation_line_becomes_four_spaces() {
    assert_eq!(texts("- foo\n\n\tbar\n", SPEC), ["foo", "  bar"]);
}

/// **The dedent's cut is a `char` boundary, and the S7 repair that made it
/// one.** `str::trim` calls U+2028 whitespace, so a continuation line of one
/// wide space takes the dedent branch with `indent = 2` landing inside a
/// three-byte character, and the arm's `text[cut..]` was a panic — §11.3's
/// *"malformed input must never panic"*, with `panic = "abort"` behind it.
///
/// Every whitespace character `str::trim` accepts and CommonMark does **not**
/// count as blank, one row each, all at `indent = 2`:
///
/// | Line | Bytes | Before | Now |
/// |---|---|---|---|
/// | U+2028 LINE SEPARATOR | 3 | **panic** | `""` |
/// | U+2029 PARAGRAPH SEPARATOR | 3 | **panic** | `""` |
/// | U+3000 IDEOGRAPHIC SPACE | 3 | **panic** | `""` |
/// | U+1680 OGHAM SPACE MARK | 3 | **panic** | `""` |
/// | U+202F NARROW NO-BREAK SPACE | 3 | **panic** | `""` |
/// | U+0085 NEL | 2 | `""` | `""` |
/// | U+00A0 NO-BREAK SPACE | 2 | `""` | `""` |
/// | U+000B, U+000C | 1 | `""` | `""` |
///
/// So the repair does not choose a new answer, it extends the one the
/// two-byte rows already gave: at `indent = 2` their cut was a boundary and
/// `text[2..]` was already empty.
///
/// # Why empty, and not the other three readings
///
/// The arm transcribes `nextLineWithoutTabs.slice(indent)`, and JavaScript
/// slices in **UTF-16 code units** — one per character here — so the line is
/// shorter than `indent` and `String.prototype.slice` past the end is `''`.
/// Enumerated over every whitespace-only line of one to three characters at
/// `indent` 1..=6, of the **47,264** rows whose byte cut lands mid-character:
/// empty reproduces `slice(indent)` on **26,035**, snapping the cut up to the
/// next boundary on **20,584**, and snapping it down or keeping the line whole
/// on **0**. It is also the answer the three sibling arms of this same `match`
/// already give, via `str::get(..).unwrap_or("")`.
///
/// # What this does not fix, measured against the running engine
///
/// muya puts the wide line **outside** the list — `- a\n\u{2028}` is a
/// `bullet-list › list-item › paragraph "a"` followed by a sibling
/// `paragraph "\u{2028}"` — because `marked`'s `list` rule is an `m`-flagged
/// regular expression and JavaScript's regular expressions treat U+2028 as a
/// line terminator, while the `src.split('\n', 1)` that feeds the continuation
/// loop does not. The port takes its block structure from `pulldown-cmark`,
/// which keeps the line inside the item, so the disagreement is upstream of
/// this function and no choice of cut can close it. It costs nothing measured:
/// no input in `cargo xtask blocks`' 1344, `cargo xtask diff`'s 22 or
/// `round_trip.rs`' 1346 has this shape.
#[test]
fn a_wide_whitespace_continuation_line_dedents_to_nothing() {
    for line in [
        "\u{2028}", "\u{2029}", "\u{3000}", "\u{1680}", "\u{202f}", "\u{85}", "\u{a0}", "\u{b}",
        "\u{c}",
    ] {
        assert_eq!(texts(&format!("- a\n{line}"), SPEC), ["a"], "{line:?}");
    }

    // The character need not be alone on the line, and the item need not come
    // first — row four of the reproducer table this replaced.
    assert_eq!(texts("- a\n \u{2028}", SPEC), ["a"]);
    assert_eq!(texts("- a\n\u{2028}\u{2028}", SPEC), ["a"]);
    assert_eq!(texts("- [ ] a\n\u{2028}", SPEC), ["a"]);
    assert_eq!(texts("\u{3000}\n- a", SPEC), ["\u{3000}", "a"]);

    // The neighbours that never took the dedent branch, unchanged: a line with
    // content is not whitespace-only, and a tab takes the sibling arm.
    assert_eq!(texts("- a\n\u{2028}b", SPEC), ["a\n\u{2028}b"]);
    assert_eq!(texts("- a\n\t\u{2028}", SPEC), ["a\n  \u{2028}"]);
    assert_eq!(texts("- a\n  \u{2028}", SPEC), ["a\n\u{2028}"]);
}

/// `blockquoteSetextReplace`, which inserts four spaces rather than removing
/// anything — and only for a lazy line that follows another lazy line.
#[test]
fn a_lazy_setext_underline_inside_a_quote_is_pushed_to_four_spaces() {
    assert_eq!(texts("> foo\nbar\n===\n", SPEC), ["foo\nbar\n    ==="]);
    // The first line of a continuation group has no `\n` before it inside
    // `currentRaw`, so the pattern cannot match.
    assert_eq!(texts("> foo\n===\n", SPEC), ["foo\n==="]);
    assert_eq!(texts("> a\n> b\n===\n", SPEC), ["a\nb\n==="]);
}

/// `rtrim(cap[0], '\n')` leaves a whitespace-only last line inside the quote's
/// text, and `marked`'s paragraph rule takes it as a continuation because the
/// lookahead that would refuse it needs the newline the rtrim removed.
#[test]
fn a_quotes_last_whitespace_only_line_belongs_to_the_paragraph() {
    assert_eq!(texts(">\n> foo\n>  \n", SPEC), ["foo\n "]);
    // In the middle it is a blank line for both engines.
    assert_eq!(texts(">\n> foo\n>  \n> bar\n", SPEC), ["foo", "bar"]);
}

/// The four spaces an absorbed indented code block loses — and keeps, at the
/// top level, where `paragraph` swallows the line before `code` is tried.
#[test]
fn an_indented_continuation_line_is_dedented_only_where_marked_restarts_its_scan() {
    assert_eq!(texts("foo\n    bar\n", SPEC), ["foo\n    bar"]);
    assert_eq!(texts("> foo\n    - bar\n", SPEC), ["foo\n- bar"]);
    assert_eq!(
        texts("  1.  A paragraph\n    with two lines.\n", SPEC),
        ["A paragraph\nwith two lines."]
    );
}

// --- the per-kind rules -----------------------------------------------------

/// **Reconstructed, not sliced.** `'#'` repeated, a space, then the heading's
/// content — so the closing sequence goes and the run of spaces collapses, and
/// a bare `#` is `"# "`, trailing space included, which is measured rather
/// than tidied.
#[test]
fn an_atx_heading_is_rebuilt_rather_than_sliced() {
    assert_eq!(texts("##   Foo   ##\n", SPEC), ["## Foo"]);
    assert_eq!(texts("#\n", SPEC), ["# "]);
    assert_eq!(texts("# \n", SPEC), ["# "]);
    // CommonMark wants a space before the closing run; without one the hashes
    // are content.
    assert_eq!(texts("# foo#\n", SPEC), ["# foo#"]);
}

/// `Tokenizer.lheading`'s `cap[1].trim()` — of the whole string, so the first
/// line loses its indent and the rest keep theirs.
#[test]
fn a_setext_headings_text_is_the_lines_above_the_underline() {
    assert_eq!(texts("Foo\nbar\n===\n", SPEC), ["Foo\nbar"]);
    assert_eq!(texts("   Foo\n   bar\n  ===\n", SPEC), ["Foo\n   bar"]);
}

/// `codeRemoveIndent`'s alternation is ordered: ` {1,4}` is tried first and is
/// greedy, so a line with any leading space never reaches the ` {0,3}\t` arm.
#[test]
fn indented_code_loses_one_to_four_columns_per_line_and_its_trailing_newline() {
    assert_eq!(texts("    let a = 1\n", SPEC), ["let a = 1"]);
    assert_eq!(texts("        foo\n    bar\n", SPEC), ["    foo\nbar"]);
    assert_eq!(strip_code_indent("  \tfoo"), "\tfoo");
    assert_eq!(strip_code_indent("\tfoo"), "foo");
    assert_eq!(strip_code_indent("     foo"), " foo");
}

/// `indentCodeCompensation` tests the **raw** against a backtick fence, so a
/// tilde fence never compensates however indented it is.
#[test]
fn a_fences_indent_is_compensated_for_backticks_and_not_for_tildes() {
    assert_eq!(texts("  ```js\n    x\n  ```\n", SPEC), ["  x"]);
    assert_eq!(
        texts("   ```\n   aaa\n    aaa\n  aaa\n   ```\n", SPEC),
        ["aaa\n aaa\n  aaa"]
    );
    // A tilde fence keeps every column, which is `marked`'s and is transcribed.
    assert_eq!(texts("  ~~~\n    x\n  ~~~\n", SPEC), ["    x"]);
}

/// muya #1265, and the reason it is implemented rather than skipped: the flag
/// is `false` in **both** option sets the gate drives, so no fixture can reach
/// it and nothing but this test would ever say whether it works. The expected
/// values were measured against the running engine with the flag flipped.
#[test]
fn trim_unnecessary_code_block_empty_lines_is_off_in_both_option_sets_and_works() {
    let trimming = Options {
        trim_unnecessary_code_block_empty_lines: true,
        ..MUYA
    };
    const { assert!(!MUYA.trim_unnecessary_code_block_empty_lines) };
    const { assert!(!SPEC.trim_unnecessary_code_block_empty_lines) };
    assert_eq!(texts("```\n\n\na\n\n\n```\n", MUYA), ["\n\na\n\n"]);
    assert_eq!(texts("```\n\n\na\n\n\n```\n", trimming), ["a"]);
    assert_eq!(texts("```\n\nx\n\n```\n", trimming), ["x"]);
    // Both ends come off whichever one triggered the branch.
    assert_eq!(texts("~~~\n\n\n~~~\n", trimming), [""]);
}

/// #4849: the table pipe escape is resolved into the stored text so the editor
/// shows a literal pipe inside inline code; `escapeText` re-adds it on
/// serialize.
#[test]
fn a_table_cell_is_trimmed_and_its_pipe_escape_resolved() {
    assert_eq!(
        texts("|a\\|b|c|\n|---|---|\n| d | e |\n", SPEC),
        ["a|b", "c", "d", "e"]
    );
}

/// The frontmatter rule strips all leading whitespace and **exactly one**
/// trailing whitespace character.
#[test]
fn front_matter_loses_all_leading_whitespace_and_exactly_one_trailing() {
    // `/^\s+/` is greedy over *all* leading whitespace, the space included.
    assert_eq!(front_matter_text("\n\n a\n\n"), "a\n");
    assert_eq!(front_matter_text("a"), "a");
    assert_eq!(texts("---\ntitle: x\n\n---\n\n", MUYA), ["title: x\n"]);
}

/// `hr`'s text is the source line minus trailing newlines — leading indent and
/// inner spacing both kept.
#[test]
fn a_thematic_breaks_text_keeps_its_indent_and_its_spacing() {
    assert_eq!(
        texts("   - - -  - --   --- --- ----\n", SPEC),
        ["   - - -  - --   --- --- ----"]
    );
}

/// A definition's paragraph is its raw with trailing newlines removed, and
/// `Tokenizer.def`'s match **starts at the indent** — which is why the gap
/// scanner had to stop stripping leading whitespace.
#[test]
fn a_reference_definitions_paragraph_keeps_its_own_indent() {
    assert_eq!(texts("  [foo]: /a\n", SPEC), ["  [foo]: /a"]);
    assert_eq!(texts("> [foo]: /a\n", SPEC), ["[foo]: /a"]);
    // A repeated label emits nothing at all — `tokens.links` is global.
    assert_eq!(
        texts("[foo]: /a\n[foo]: /b\n\n[foo]\n", SPEC),
        ["[foo]: /a", "[foo]"]
    );
}

/// The task marker comes off the item's text — except in the one combination
/// where `marked` puts it back and `compatibleTaskList` never takes it off,
/// because its ordered branch does not call `stripTaskMarker`.
#[test]
fn the_task_marker_survives_only_in_a_loose_ordered_list() {
    assert_eq!(texts("- [ ] a\n", SPEC), ["a"]);
    assert_eq!(texts("- [ ] a\n\n- [x] b\n", SPEC), ["a", "b"]);
    assert_eq!(texts("1. [ ] a\n", SPEC), ["a"]);
    assert_eq!(texts("1. [ ] a\n\n2. [x] b\n", SPEC), ["[ ] a", "[x] b"]);
    // The restored marker keeps its own case and is re-spaced to one space.
    assert_eq!(texts("1.  [X]   a\n\n2. b\n", SPEC), ["[X] a", "b"]);
}

/// The block-math tokenizer trims its content, and the gitlab fence's
/// `multiplemath` case trims where `_buildCodeState` does not.
#[test]
fn a_math_blocks_text_is_trimmed_by_both_routes_in() {
    assert_eq!(texts("$$\n  x^2\n$$\n", MUYA), ["x^2"]);
    assert_eq!(texts("```math\n  x^2\n```\n", MUYA), ["x^2"]);
    // Not math: a plain fence keeps its indentation.
    assert_eq!(texts("```\n  x^2\n```\n", MUYA), ["  x^2"]);
}

/// An html block's text is trimmed after `trimTrailingBlankLines`; and the
/// lone-image special case is a **paragraph** carrying the same text.
#[test]
fn an_html_blocks_text_is_trimmed_and_a_lone_img_is_a_paragraph() {
    assert_eq!(texts("<div>\n  a\n</div>\n", SPEC), ["<div>\n  a\n</div>"]);
    assert_eq!(names("<img src=\"x\">\n", SPEC), ["paragraph"]);
    assert_eq!(texts("<img src=\"x\">\n", SPEC), ["<img src=\"x\">"]);
}

/// The empty-container filler carries `end..end` and its text must stay `""`.
#[test]
fn the_empty_container_filler_has_no_text_to_reconstruct() {
    assert_eq!(texts(">\n", SPEC), [""]);
    assert_eq!(texts("-\n", SPEC), [""]);
}

// ---------------------------------------------------------------------------
// The source ranges — M2.md §10's owed item, decided at S3
// ---------------------------------------------------------------------------

/// **S2's claim about what the ranges are for, as a test.**
///
/// M2.md §10's correction says the container half of the reverse offset map is
/// not lost by deferring it, *"because `strip_lines` needs the source lines and
/// the ancestors' prefixes; the ancestors are the tree, and their prefixes are
/// computable from the ancestors' source ranges. So the whole stripper is
/// re-runnable for one block at a time, lazily, from `(src, tree, ranges)`."*
///
/// That sentence is the reason `parse` returns a [`crate::SourceMap`] at all,
/// and until it was run it was an argument rather than a fact. Here it is run:
/// the block quote's prefix is rebuilt from **its own range** — not from
/// anything the parse kept — and `strip_lines` re-derives the inner
/// paragraph's text from the source.
///
/// What this does **not** claim is the per-kind half. An atx heading is rebuilt
/// from scratch and a code block loses columns after stripping; each needs its
/// own inverse and each belongs beside its rule. §10 says so and this test is
/// deliberately about the half that is free.
#[test]
fn the_stripper_is_re_runnable_from_src_tree_and_ranges() {
    let src = "> foo\n> bar\n";
    let (doc, map) = parse_blocks_with_ranges(src, SPEC);

    let quote = doc.children(doc.root())[0];
    assert_eq!(doc.block(quote).map(Block::name), Some("block-quote"));
    let paragraph = doc.children(quote)[0];
    assert_eq!(text_of(&doc, paragraph), "foo\nbar");

    // Rebuild the ancestor's prefix from its range alone, exactly as a caller
    // holding only `(src, tree, ranges)` would have to.
    let quote_range = map.get(quote).expect("the quote has a range");
    let prefix = Prefix::Quote {
        first_line: line_start_of(src, quote_range.start),
    };

    let lines = strip_lines(src, &map.get(paragraph).expect("range"), &[prefix]);
    assert_eq!(joined_text(&lines), "foo\nbar");
}

/// The ranges are a fact about one parse, and the map says so by not being
/// reachable from a [`Document`]. This pins the other half of that contract:
/// the map a parse hands back is complete for the tree it was built with.
#[test]
fn the_map_has_one_entry_per_node_including_the_empty_document_fallback() {
    let (doc, map) = parse_blocks_with_ranges("", SPEC);
    assert_eq!(map.len(), 1, "the fallback paragraph carries 0..0");
    assert_eq!(map.get(doc.children(doc.root())[0]), Some(0..0));

    let src = "# a\n\n- b\n- c\n";
    let (doc, map) = parse_blocks_with_ranges(src, SPEC);
    fn count(doc: &Document, id: NodeId) -> usize {
        doc.children(id)
            .iter()
            .map(|c| 1 + count(doc, *c))
            .sum::<usize>()
    }
    assert_eq!(map.len(), count(&doc, doc.root()));
    assert!(map.get(doc.root()).is_none(), "the root has no block");
}

/// **`pulldown-cmark` 0.13.4 panics on `"> - [a]: /x\n\t"`**, and `mt_md::parse`
/// inherits it.
///
/// Found by S6's generated edit sequences, minimised here to the four
/// ingredients it needs: a block quote, a list item, a link reference
/// definition, and a following line that is whitespace-only and ends in a tab.
/// The panic is `Option::unwrap()` on `None` inside `OffsetIter::next`
/// (`parse.rs:2199`), reached before any of this crate's code runs — the
/// probe below drives the parser directly, so nothing about the mapping layer
/// is in the frame.
///
/// **Recorded rather than worked around.** `parse`'s doc comment says it is
/// total — *"every string is a document, exactly as `MarkdownToState.generate()`
/// is total"* — and that is now false for one input class; `docs/upstream-issues.md`
/// carries it and M2.md §10 owes the decision to whichever stage first has a
/// user who can type it. 0.13.4 is the latest published version, so there is no
/// upgrade to take, and catching the panic is not open either: the release
/// profile is `panic = "abort"`.
///
/// `#[should_panic]` makes this a **ratchet in the useful direction**: the day
/// the upstream fix lands, this test fails and says so.
#[test]
#[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
fn pulldown_cmark_panics_on_a_definition_in_a_quoted_list_item_before_a_tab_line() {
    let count = pulldown_cmark::Parser::new_ext("> - [a]: /x\n\t", cmark_options())
        .into_offset_iter()
        .count();
    unreachable!("pulldown-cmark 0.13.4 panics before it can return {count}");
}

/// The three neighbours that do **not** panic, so the reproducer above is a
/// statement about four ingredients rather than about block quotes.
#[test]
fn the_neighbours_of_the_upstream_panic_parse_normally() {
    for src in [
        "> [a]: /x\n\t",
        "- [a]: /x\n\t",
        "> - x\n\t",
        "> - [a]: /x\n\tb",
    ] {
        let count = pulldown_cmark::Parser::new_ext(src, cmark_options())
            .into_offset_iter()
            .count();
        assert!(count > 0, "{src:?}");
    }
}

// --- the nesting limit, §11.3 -----------------------------------------------

/// The deepest node in `src`'s tree, counting a top-level block as 1.
fn tree_depth(src: &str, options: Options) -> usize {
    let doc = parse_blocks(src, options);
    let mut deepest = 0;
    let mut stack: Vec<(NodeId, usize)> =
        doc.children(doc.root()).iter().map(|id| (*id, 1)).collect();
    while let Some((id, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        stack.extend(doc.children(id).iter().map(|child| (*child, depth + 1)));
    }
    deepest
}

/// **The recursion §11.3 found is this crate's, not `pulldown-cmark`'s** — and
/// that had to be measured rather than assumed, because the fuzz target drives
/// `parse`, which calls into the parser.
///
/// Its `OffsetIter` is a position loop: 50,000 nested block quotes, which is
/// 391× the depth at which this crate's own walks used to abort on the same
/// stack, produce a full event stream and no growth in frames. So the fix
/// belongs here and there is nothing to add to `docs/upstream-issues.md`
/// beside the abort that file already records.
#[test]
fn pulldown_cmark_is_iterative_over_container_depth() {
    let src = "> ".repeat(50_000) + "x";
    let events = pulldown_cmark::Parser::new_ext(&src, cmark_options())
        .into_offset_iter()
        .count();
    // Start + End per quote, plus the paragraph's own three events.
    assert!(events > 100_000, "{events}");
}

/// **The reproducer.** `"> "` × 2000 — 4,001 bytes — aborted
/// `mt-cli --dump-state` before S7: *"thread 'main' has overflowed its stack"*,
/// which `panic = "abort"` (§12) makes a crash with no `catch_unwind` and no
/// `Result`. Depth 1000 was fine and 2000 was not.
///
/// Every depth here used to abort on `mt-cli`'s 1 MiB main thread, and 50,000
/// is here because the first repair S7 wrote — a pass over the finished tree —
/// turned the abort into a **35.5 s** parse at depth 8000 rather than into an
/// answer. §12 has no budget row for that, so the limit is enforced in
/// [`Builder::would_exceed_depth`], where it keeps the prefix chain bounded and
/// the whole path linear.
#[test]
fn a_document_two_thousand_containers_deep_returns_instead_of_aborting() {
    for depth in [2000usize, 8000, 50_000] {
        let src = "> ".repeat(depth) + "x";
        assert!(
            tree_depth(&src, MUYA) <= MAX_NESTING_DEPTH,
            "depth {depth} built past the limit"
        );
    }
}

/// The limit holds for every nesting shape, not only for the one that found it.
///
/// Six shapes, because the reservation
/// ([`Builder::would_exceed_depth`], [`deepest_legal_depth`]) is per-kind and a
/// list costs two levels where a quote costs one: a bullet list is
/// `list › list-item`, an ordered list the same, and the alternating shapes are
/// what would catch a reservation that is right for one kind and wrong for the
/// pair.
#[test]
fn no_input_of_any_shape_or_size_builds_a_tree_past_the_limit() {
    for marker in ["> ", "- ", "* ", "20. ", "- > ", "> - "] {
        for depth in [126usize, 127, 128, 129, 200, 2000] {
            for options in [SPEC, MUYA] {
                let src = marker.repeat(depth) + "x";
                let built = tree_depth(&src, options);
                assert!(
                    built <= MAX_NESTING_DEPTH,
                    "{marker:?} × {depth} built {built} deep"
                );
            }
        }
    }
}

/// At the limit the port **stops opening containers and the rest of the source
/// becomes one paragraph's text** — the first of the three candidates S7
/// weighed, and the only one that keeps `serialize(parse(s)) == s` here.
///
/// `"> "` × 130 keeps 127 quotes, and the paragraph carries the three markers
/// they did not consume. Serializing re-emits exactly the 127 prefixes it
/// stripped and the text supplies the remainder, so the output *is* the input —
/// which makes the fixed point §11.3's first row asks for hold by identity
/// rather than by luck. Truncating would lose the `x`; reparenting the deep
/// leaves would lose the markers between them and the second parse would
/// differ from the first.
#[test]
fn the_source_below_the_limit_becomes_one_paragraphs_text() {
    let src = "> ".repeat(130) + "x";
    assert_eq!(
        texts(&src, MUYA),
        ["> > > x"],
        "the three quotes the limit refused, verbatim"
    );
    assert_eq!(tree_depth(&src, MUYA), MAX_NESTING_DEPTH);
    assert_eq!(
        crate::serialize(&parse_blocks(&src, MUYA), MUYA),
        format!("{src}\n"),
        "the round trip is the identity at the limit — modulo the final newline\n\
         `ExportMarkdown` adds to every document, which is 348 of the identity\n\
         exceptions `tests/round_trip.rs` already enumerates and is not this\n\
         limit's doing"
    );
}

/// The two halves of the limit must allow exactly the same depths.
///
/// [`Builder::would_exceed_depth`] decides on a `pulldown-cmark` tag as the
/// walk runs; [`deepest_legal_depth`] decides on a finished [`Block`] when
/// [`clamp_depth`] checks the composed document. If the second is stricter it
/// re-cuts a tree the first already clamped — which is not hypothetical: S7's
/// first pairing cost a level and broke the identity above.
#[test]
fn the_two_halves_of_the_limit_allow_the_same_depths() {
    let quote = Tag::BlockQuote(None);
    let list = Tag::List(None);
    for (tag, block) in [
        (
            &quote,
            Block::BlockQuote {
                children: Vec::new(),
            },
        ),
        (
            &list,
            Block::BulletList {
                loose: false,
                marker: BulletMarker::Dash,
                children: Vec::new(),
            },
        ),
        (
            &Tag::Item,
            Block::ListItem {
                children: Vec::new(),
            },
        ),
        (&Tag::Paragraph, Block::Paragraph { text: Text::new() }),
    ] {
        let allowed = deepest_legal_depth(&block);
        // `would_exceed_depth` reads the depth off the frame stack, so a stack
        // of `n` frames (root included) is asking about depth `n`.
        let builder = |depth: usize| Builder {
            src: "",
            options: MUYA,
            stack: Vec::with_capacity(depth),
            seen_labels: HashSet::new(),
            prefixes: Vec::new(),
            overflow: 0,
        };
        for depth in [allowed, allowed + 1] {
            let mut b = builder(depth);
            b.stack.resize_with(depth, || Frame {
                kind: FrameKind::Root,
                range: 0..0,
                children: Vec::new(),
                scanned_to: 0,
            });
            assert_eq!(
                b.would_exceed_depth(tag),
                depth > allowed,
                "{block:?} at depth {depth}"
            );
        }
    }
}

/// **Three things S6's generated edits found about the ranges, none of which
/// any fixture reaches.** Characterizations rather than fixes: each is recorded
/// in M2.md §10 with the reason it is not repaired here.
#[test]
fn the_ranges_are_not_injective_and_do_not_always_nest() {
    // 1 — under a `footnote`, every node carries the definition's own range,
    // because the body `marked` lexes is a de-indented copy (S5, §10).
    // `SourceMap::is_exact` is what makes that visible to a caller instead of
    // a surprise; S6 added it because its leaf-text path re-derives text from a
    // range and would otherwise have trusted this one.
    let options = Options {
        footnote: true,
        ..MUYA
    };
    let (doc, map) = parse_blocks_with_ranges("[^a]: note\n", options);
    let footnote = doc.children(doc.root())[0];
    let paragraph = doc.children(footnote)[0];
    assert_eq!(doc.block(footnote).map(Block::name), Some("footnote"));
    assert_eq!(map.get(footnote), map.get(paragraph), "not injective");
    assert!(map.is_exact(footnote));
    assert!(!map.is_exact(paragraph), "the body is a copy, not a slice");

    // 2 — the same, one mechanism later: the `state.top = false` re-lex maps
    // its ranges out of a de-indented copy line by line, so they are true but
    // not tight.
    let (doc, map) = parse_blocks_with_ranges("3. foo\n   20. foo\n", SPEC);
    let item = doc.children(doc.children(doc.root())[0])[0];
    let nested = doc.children(item)[1];
    assert_eq!(doc.block(nested).map(Block::name), Some("order-list"));
    assert!(
        map.is_exact(doc.children(item)[0]),
        "the head paragraph is a slice"
    );
    assert!(!map.is_exact(nested), "the re-lexed tail is not");

    // 3 — and a child's range can escape its parent's. A tab inside a list
    // item's marker padding makes the stripper's per-line map inexact
    // (`StrippedLine::exact`, the one case where a line's text is not a slice
    // of the source), and the definition scanner's range is computed from the
    // stripped line, so the paragraph lands one byte to the left of its item.
    // `tests/source_ranges.rs` asserts nesting over the corpus, which contains
    // no such document; this is the input that shows the invariant is about the
    // corpus rather than about every string.
    let src = "-\t- [a]: /x[xter\n";
    let (doc, map) = parse_blocks_with_ranges(src, SPEC);
    let outer_item = doc.children(doc.children(doc.root())[0])[0];
    let inner_item = doc.children(doc.children(outer_item)[0])[0];
    let paragraph = doc.children(inner_item)[0];
    let (inner, leaf) = (
        map.get(inner_item).expect("a range"),
        map.get(paragraph).expect("a range"),
    );
    assert!(
        leaf.start < inner.start,
        "measured: the paragraph is at {leaf:?} inside an item at {inner:?}"
    );
}

fn text_of(doc: &Document, id: NodeId) -> String {
    doc.block(id)
        .and_then(Block::text)
        .expect("a leaf")
        .to_str()
        .into_owned()
}

/// **The disagreement §10 owed S4, fixed** — and the same thirteen probes S3
/// measured, now asserting agreement where they asserted the symptom.
///
/// This test is S3's `an_empty_task_marker_does_not_yet_take_a_lazy_continuation`
/// with its first half inverted and its second half untouched, which is what
/// the pair was built for: the two `PENDING` entries that asserted muya's
/// answer delist in the same commit, and the six probes that already agreed are
/// the boundary the fix was not allowed to move.
///
/// Every expected value here is measured from the running engine, not read off
/// `Tokenizer.list`. Three of them are not what reading it suggests — see
/// `Builder::fold_empty_task_marker_continuations` for the rules and
/// `bare_task_marker_column` for the one-quantifier difference between the two
/// regexes that decide it.
#[test]
fn an_empty_task_marker_takes_a_lazy_continuation() {
    let task = ["task-list", "  task-list-item", "    paragraph"];

    // The seven that used to differ. muya: one task item holding the
    // continuation, and nothing after it.
    assert_eq!(names("- [ ] \ntext\n", SPEC), task);
    assert_eq!(texts("- [ ] \ntext\n", SPEC), ["text"]);
    assert_eq!(texts("- [x] \ntext\n", SPEC), ["text"]);
    assert_eq!(
        state("- [x] \ntext\n", SPEC)[0]["children"][0]["meta"],
        json!({ "checked": true })
    );
    // Every line of the continuation, with the ones that reach the marker's
    // column dedented by it and the ones that do not appended verbatim.
    assert_eq!(texts("- [ ] \ntext\nmore\n", SPEC), ["text\nmore"]);
    assert_eq!(texts("- [ ] \ntext\n  cont\n", SPEC), ["text\ncont"]);
    assert_eq!(texts("  - [ ] \n  text\n", SPEC), ["  text"]);
    // Inside a block quote, measured from the quote's content column.
    assert_eq!(texts("> - [ ] \n> text\n", SPEC), ["text"]);
    // The list the folded paragraph had interrupted, merged back.
    assert_eq!(texts("- [ ] \ntext\n- [ ] b\n", SPEC), ["text", "b"]);
    // A blank line in the gap is where the merged list's looseness comes from.
    assert_eq!(
        state("- [ ] \ntext\n\n- [ ] b\n", SPEC)[0]["meta"],
        json!({ "loose": true, "marker": "-" })
    );
    // An **ordered** list keeps the marker in the item's text, because
    // `compatibleTaskList` only strips it in the bullet branch.
    assert_eq!(
        names("1. [ ] \ntext\n", SPEC),
        ["order-list", "  list-item", "    paragraph"]
    );
    assert_eq!(texts("1. [ ] \ntext\n", SPEC), ["[ ] \ntext"]);
    // The four-item spec case, whose last item takes the continuation.
    assert_eq!(
        texts("- [ ] a\n\n- [ ] \n- [ ] \n- [ ] \ntext\n", SPEC),
        ["a", "", "", "text"]
    );

    // The boundary, where the two engines already agreed and the fix was not
    // allowed to move anything: a blank line, a block start, an indented line,
    // and no task marker at all.
    for (agreed, after) in [
        ("- [ ] \n\ntext\n", &["paragraph"][..]),
        ("- [ ] \n# head\n", &["atx-heading"][..]),
        ("- [ ] \n> quote\n", &["block-quote", "  paragraph"][..]),
        ("- [ ] \n---\n", &["thematic-break"][..]),
    ] {
        let expected: Vec<&str> = task.iter().chain(after).copied().collect();
        assert_eq!(names(agreed, SPEC), expected, "{agreed:?}");
    }
    // Already inside the item, so there was never anything to fold.
    assert_eq!(texts("- [ ] \n  text\n", SPEC), ["text"]);
    assert_eq!(texts("- [ ] \n    code\n", SPEC), ["  code"]);
    // No task marker means no disagreement.
    assert_eq!(texts("- \ntext\n", SPEC), ["", "text"]);
    assert_eq!(texts("- a\ntext\n", SPEC), ["a\ntext"]);
    // Task-ness still splits a list, so a bullet item after a folded task item
    // is a second list in both engines.
    assert_eq!(
        names("- [ ] \ntext\n- b\n", SPEC),
        [
            "task-list",
            "  task-list-item",
            "    paragraph",
            "bullet-list",
            "  list-item",
            "    paragraph"
        ]
    );
}

/// **The three inputs the fix deliberately leaves disagreeing**, with muya's
/// answer beside each. M2.md §10's "Owed by S4" carries them; this is the
/// reproducer half, so a later stage that fixes one flips a test rather than
/// discovering the shape again.
///
/// All three are the same wider mechanism: `marked` re-lexes a list item's
/// dedented content **line by line** with `state.top = false`, so any line can
/// start a block inside an item and CommonMark's paragraph-interruption rules
/// never apply there. Reproducing that is a change to how every list item's
/// children are built, which is S1's layer and is not what §10 asked S4 for.
#[test]
fn the_three_shapes_the_task_marker_fix_does_not_reach() {
    // 1 — no blank after the `]`, so `TASK_MARKER_PREFIX_REG` does not match
    // and muya does not make it a task item at all:
    //     bullet-list › list-item › paragraph "[ ]\ntext"
    assert_eq!(
        names("- [ ]\ntext\n", SPEC),
        [
            "task-list",
            "  task-list-item",
            "    paragraph",
            "paragraph"
        ]
    );
    assert_eq!(texts("- [ ]\ntext\n", SPEC), ["", "text"]);

    // 2 — a setext underline in the continuation. muya re-lexes the item's
    // content, so `===` makes a heading *inside* the item:
    //     task-list › task-list-item › setext-heading "text"
    assert_eq!(texts("- [ ] \ntext\n===\n", SPEC), ["", "text"]);

    // 3 — and with `---` the underline is an `hr` to `marked`'s continuation
    // loop, so muya emits task-list › item › paragraph "text" **and** a
    // thematic break, where the port reads one setext heading.
    assert_eq!(
        names("- [ ] \ntext\n---\n", SPEC),
        [
            "task-list",
            "  task-list-item",
            "    paragraph",
            "setext-heading"
        ]
    );
}

// --- S7: the port's line grid, and the five panics behind one disagreement ---

/// **The root cause the soak found, stated directly.** The block structure comes
/// from `pulldown-cmark`, which ends a line at a bare `\r`; the text is
/// re-derived `marked`-style, and `marked` spells a line `[^\n]`. Where the two
/// grids disagreed, an offset from one was measured against a string cut by the
/// other and the resulting index need not have been a character boundary.
///
/// The asymmetry is the fix: a **lone** `\r` terminates a line so that the two
/// engines' line *starts* agree, and the `\r` of a `\r\n` does not, so that a
/// CRLF line keeps the `\r` that is content to `marked` and to muya
/// ([`crate::normalize_source`]'s recorded measurement, `"a\r\nb\r\n"` → one
/// paragraph `"a\r\nb\r"`).
#[test]
fn a_lone_carriage_return_ends_a_line_and_a_crlfs_does_not() {
    // A lone `\r`: byte 2 starts its own line, byte 1 terminates the line
    // before it, and that line ends at byte 1.
    assert_eq!(line_start_of("=\r#", 2), 2);
    assert_eq!(line_end_of("=\r#", 0), 1);
    assert_eq!(line_start_of("=\r#", 0), 0);

    // A `\r\n`: the `\n` terminates and the `\r` is content. Both were already
    // true before this fix and both must stay true — every fixture that reaches
    // `--require-ts` comes through the file layer and is `\n`-only, but a caller
    // handing `parse` a CRLF string directly is exactly the muya measurement.
    assert_eq!(
        line_end_of("a\r\nb", 0),
        2,
        "the `\\n`, so the text keeps `\\r`"
    );
    assert_eq!(line_start_of("a\r\nb", 3), 3);
    assert_eq!(line_start_of("a\r\nb", 2), 0, "a `\\r` is not a line start");

    // `preceded_by_blank_line`'s `start == 0` fast path is *"a block at the
    // start of the document"*, and this is the input that used to reach it from
    // byte 2 — the one that made `reparse::hard_boundary` lie. See
    // `crate::reparse::tests::a_lone_carriage_return_hides_a_paragraph_merge_from_the_region_reparse`.
    assert!(!preceded_by_blank_line("=\r#", 2));
    assert!(
        preceded_by_blank_line("=\r\r#", 3),
        "the middle line is blank"
    );
    // And the CRLF half of the same question: a line after an ordinary CRLF
    // line is *not* blank-preceded, which reading that `\r` as a terminator
    // would have got backwards for every CRLF document there is.
    assert!(!preceded_by_blank_line("a\r\nb", 3));
    assert!(preceded_by_blank_line("a\r\n\r\nb", 6));
}

/// **`block.rs`'s two sites on one wrong index** — the `.get(marker_len..)` that
/// returned `""` silently and the `&text[marker_len..]` one line further on that
/// panicked. Both read a marker length measured against `"0é\r1. x"`, which is
/// one line to the port and two to the parser that reported the item.
///
/// The assertion is the **right answer** rather than the absence of a panic:
/// the item's first line is byte 4, its marker is the two ASCII bytes `1.`, and
/// its content indent is three columns. Before the fix `first_line` was `0`,
/// `marker_len` was `2` measured on a string whose byte 2 is inside the `é`, and
/// `indent` came from the blank-line arm — `rest` was `""` — instead of from the
/// item's own text.
#[test]
fn an_items_marker_is_measured_on_its_own_line_after_a_lone_carriage_return() {
    let src = "0é\r1. x";
    let parsed = crate::parse(src, SPEC);
    let doc = &parsed.document;
    let list = doc.children(doc.root())[1];
    let item = doc.children(list)[0];
    let ancestors = vec![(
        doc.block(item).expect("a block"),
        parsed.source_map.get(item).expect("a range"),
    )];

    assert_eq!(
        prefix_chain(src, &ancestors),
        vec![Prefix::Item {
            indent: 3,
            marker_len: 2,
            first_line: 4,
        }]
    );
    // Which is the prefix the same item gets with the `\r` spelled `\n`, because
    // `pulldown-cmark` reads the two documents the same way and the grid now
    // does too. `"x"` rather than `""` is the wrong answer repaired.
    assert_eq!(texts("0é\r1. x", SPEC), ["0é", "x"]);
    assert_eq!(texts("0é\n1. x", SPEC), ["0é", "x"]);
}

/// The four soak reproducers for the `apply_prefix` slice, at both option sets.
///
/// The soak's own message — *"index 2 … inside `'\u{2028}'` (bytes 1..4)"* — is
/// the second of these. §11.3: *"malformed input must never panic"*.
#[test]
fn parse_does_not_panic_on_a_lone_carriage_return_before_a_list_item() {
    for options in [SPEC, MUYA] {
        for src in ["0é\r1. x", "0\u{2028}\r1. x", "0\u{2028}\r- x", "12é\r1. x"] {
            let _ = crate::parse(src, options);
        }
    }
    // The controls that isolate the ingredient: only a `\r` *not* followed by a
    // newline, with a digit run before it and a list item after it, ever reached
    // the slice.
    for src in ["0é\n1. x", "0é\r\n1. x", "0\u{2028}1. x", "\r0é\r1. x"] {
        let _ = crate::parse(src, SPEC);
    }
}

/// The other half of the marker fix: `marked`'s `bullet` is
/// `/ {0,3}(?:[*+-]|\d{1,9}[.)])/` and it **requires** the delimiter, where the
/// port added one byte for a character it never looked at. On the item's own
/// line `pulldown-cmark` guarantees the `.` or `)` is there, so this changes
/// nothing a real list does; on any other line it is the difference between a
/// length and a guess.
#[test]
fn an_ordered_markers_delimiter_is_checked_rather_than_assumed() {
    assert_eq!(texts("1. x", SPEC), ["x"]);
    assert_eq!(texts("1) x", SPEC), ["x"]);
    // Ten digits is past `\d{1,9}`, so `marked` finds no bullet and neither does
    // this — the tenth character is a digit and not a delimiter.
    assert_eq!(texts("1234567890. x", SPEC), ["1234567890. x"]);
}

/// `indentCodeCompensation` mixed two units — a byte count of a Unicode-trimmed
/// run against a count of ASCII spaces — so a body line beginning with an
/// ideographic space passed the `>=` on three bytes and then cut one byte into
/// the character.
///
/// The assertion is `marked`'s answer and not merely the absence of a panic:
/// `indentToCode` is `" "` (one code unit), `indentInNode` is `"\u{3000}"` (one
/// code unit), `1 >= 1` holds, so `node.slice(1)` drops the wide space.
#[test]
fn an_indented_backtick_fence_compensates_a_wide_space_by_characters() {
    for options in [SPEC, MUYA] {
        assert_eq!(texts(" ```\n\u{3000}x\n ```", options), ["x"]);
    }
    // Two columns of compensation against a one-character run: `1 >= 2` is
    // false, so the line is kept whole.
    assert_eq!(texts("  ```\n\u{3000}x\n  ```", SPEC), ["\u{3000}x"]);
    // The ASCII case, where bytes and characters agree, is unchanged.
    assert_eq!(texts(" ```\n  x\n ```", SPEC), [" x"]);
    // A line that is nothing but its indent slices to nothing, which is
    // `String.prototype.slice` past the end.
    assert_eq!(texts(" ```\n\u{3000}\n ```", SPEC), [""]);
}

/// `item_blank_lines` accumulates `Built::range` ends and asks
/// [`has_blank_line`] of the gaps between them. §10 already records that those
/// ranges are neither injective nor always nested, so neither end is a boundary
/// by construction — and the soak reached both ends, one input each.
///
/// Snapping down is sound because the question is only *"are there two newlines
/// in this gap"*: a newline is ASCII and can never be inside the character being
/// snapped over, so a gap moved by those few bytes has the same answer.
#[test]
fn parse_does_not_panic_on_an_item_whose_child_ranges_are_not_on_boundaries() {
    for options in [SPEC, MUYA] {
        let _ = crate::parse("*\t[a]:\u{a0}", options);
        let _ = crate::parse("+\t[a]:\u{a0}_\u{feff}\n\\\t", options);
    }
}

/// The grid change must be invisible to every document with no lone `\r`, which
/// is every document the file layer produces — [`crate::normalize_source`] folds
/// both spellings to `\n` before the editor sees the text. This is the guard on
/// that claim at the level the corpus harnesses cannot reach, because they all
/// read through that function: a CRLF string handed straight to `parse`, whose
/// text keeps its `\r` exactly as muya's does.
#[test]
fn a_crlf_document_parses_exactly_as_it_did_before_the_grid_changed() {
    // `lib.rs`'s recorded muya measurement, reproduced through `parse`.
    assert_eq!(texts("a\r\nb\r\n", SPEC), ["a\r\nb\r"]);
    assert_eq!(texts("a\r\nb\r\n", MUYA), ["a\r\nb\r"]);
    assert_eq!(texts("a\r\n\r\nb\r\n", SPEC), ["a\r", "b\r"]);
    assert_eq!(
        names("> a\r\n> b\r\n", SPEC),
        ["block-quote", "  paragraph"]
    );
    // Inside a list item the last `\r` is *not* kept, because `pulldown-cmark`
    // ends an item's range at its last non-space byte where it ends a top-level
    // paragraph's at its last content byte — the asymmetry `leaf_text_edit`'s
    // "strictly inside" guard already names. Measured, not chosen, and measured
    // to be the same before and after this change.
    assert_eq!(texts("- a\r\n  b\r\n", SPEC), ["a\r\nb"]);
}
