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
    assert_eq!(block_math("$\nx\n$\n"), Some(6));
    assert_eq!(block_math("$$\nx\n$$\n"), Some(8));
    // A backslash consumes the following newline, so it cannot close the block.
    assert_eq!(block_math("$$\nx\\\n$$\n"), None);
}

// --- what S1 does not do, recorded rather than left silent -------------------

/// `Options::footnote` has no effect on the block pass, and this is where that
/// is written down rather than discovered.
///
/// muya's footnotes are its own `marked` block extension — `[^id]:` with a
/// 4-space de-indent and a recursive lex, `utils/marked/extensions/footnote.ts`
/// — not GFM's, so `pulldown-cmark`'s `ENABLE_FOOTNOTES` is not a substitute
/// and is deliberately off ([`cmark_options`]). Both option sets in S1's 1344
/// inputs have `footnote: false`, so the gate says nothing about this either
/// way.
///
/// Under `footnote: false` — which is what both engines are actually driven
/// with — muya emits a plain paragraph, and so does the port. **Owed to S3**,
/// where the label-map pass lands; M2.md's owed list carries it.
#[test]
fn a_footnote_definition_is_a_paragraph_and_the_footnote_option_changes_nothing_yet() {
    assert_eq!(names("[^a]: note\n", MUYA), ["paragraph"]);
    assert_eq!(
        names("text\n\n[^a]: note\n", MUYA),
        ["paragraph", "paragraph"]
    );

    let with_footnotes = Options {
        footnote: true,
        ..MUYA
    };
    assert_eq!(
        names("[^a]: note\n", with_footnotes),
        names("[^a]: note\n", MUYA),
        "turning the option on changes nothing today — that is the gap, not a design"
    );
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
    let logical = LogicalText::of(src, 0..src.len(), false);
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
