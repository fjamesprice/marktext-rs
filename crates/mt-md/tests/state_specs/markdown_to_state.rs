//! `markdownToState.spec.ts` — 32 cases.
//!
//! Task-list nesting (marktext `23435ce6`, #1733 / PR #1835), setext vs.
//! thematic break, list splitting, the block-level footnote token, and
//! `trimUnnecessaryCodeBlockEmptyLines` (#1265).

#![allow(unused_imports)]
use crate::*;

spec_cases! { "markdown_to_state",

/// #1733: the legacy marked fork forgot to subtract the `[x] ` prefix from
/// the indent counter, so a nested task item read as a sibling.
fn keeps_an_empty_unchecked_task_item_after_a_populated_task_item() {
    let doc = parse("- [ ] a\n- [ ] \n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    let list = states[0];
    assert_eq!(name(&doc, list), "task-list");
    let items = kids(&doc, list);
    assert_eq!(names(&doc, &items), ["task-list-item", "task-list-item"]);
    for item in &items {
        assert_eq!(meta(&doc, *item), BlockMeta::TaskListItem { checked: false });
    }
    assert_eq!(
        child_named(&doc, items[0], "paragraph").map(|p| text(&doc, p)),
        Some("a".to_string())
    );
    assert_eq!(names(&doc, &kids(&doc, items[1])), ["paragraph"]);
    assert_eq!(text(&doc, kids(&doc, items[1])[0]), "");
}

fn parses_a_single_empty_unchecked_task_item_as_a_task_list_item() {
    let doc = parse("- [ ] \n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "task-list");
    let items = kids(&doc, states[0]);
    assert_eq!(items.len(), 1);
    assert_eq!(name(&doc, items[0]), "task-list-item");
    assert_eq!(meta(&doc, items[0]), BlockMeta::TaskListItem { checked: false });
    assert_eq!(names(&doc, &kids(&doc, items[0])), ["paragraph"]);
    assert_eq!(text(&doc, kids(&doc, items[0])[0]), "");
}

fn parses_a_single_empty_checked_task_item_as_a_checked_task_list_item() {
    let doc = parse("- [x] \n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "task-list");
    let items = kids(&doc, states[0]);
    assert_eq!(items.len(), 1);
    assert_eq!(meta(&doc, items[0]), BlockMeta::TaskListItem { checked: true });
    assert_eq!(text(&doc, kids(&doc, items[0])[0]), "");
}

fn parses_an_empty_task_marker_with_lazy_continuation_text_as_a_task_item() {
    let doc = parse("- [ ] \ntext\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "task-list");
    let items = kids(&doc, states[0]);
    assert_eq!(items.len(), 1);
    assert_eq!(meta(&doc, items[0]), BlockMeta::TaskListItem { checked: false });
    let inner = kids(&doc, items[0]);
    assert_eq!(inner.len(), 1);
    assert_eq!(name(&doc, inner[0]), "paragraph");
    assert_eq!(text(&doc, inner[0]), "text");
}

fn keeps_lazy_continuation_text_on_the_final_empty_task_marker() {
    let doc = parse("- [ ] a\n\n- [ ] \n- [ ] \n- [ ] \ntext\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "task-list");
    let items = kids(&doc, states[0]);
    assert_eq!(items.len(), 4);
    assert!(items.iter().all(|i| name(&doc, *i) == "task-list-item"));
    assert!(items
        .iter()
        .all(|i| meta(&doc, *i) == BlockMeta::TaskListItem { checked: false }));
    assert_eq!(text(&doc, kids(&doc, items[0])[0]), "a");
    assert_eq!(text(&doc, kids(&doc, items[1])[0]), "");
    assert_eq!(text(&doc, kids(&doc, items[2])[0]), "");
    assert_eq!(text(&doc, kids(&doc, items[3])[0]), "text");
}

fn does_not_treat_dash_empty_brackets_as_an_empty_task_item() {
    let doc = parse("- []\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "bullet-list");
    let item = kids(&doc, states[0])[0];
    assert_eq!(name(&doc, item), "list-item");
    assert_eq!(name(&doc, kids(&doc, item)[0]), "paragraph");
    assert_eq!(text(&doc, kids(&doc, item)[0]), "[]");
}

fn does_not_treat_dash_bracket_space_text_as_a_task_item() {
    let doc = parse("- [ ]text\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "bullet-list");
    let item = kids(&doc, states[0])[0];
    assert_eq!(name(&doc, item), "list-item");
    assert_eq!(text(&doc, kids(&doc, item)[0]), "[ ]text");
}

fn keeps_three_levels_of_task_list_nesting() {
    let doc = parse("- [ ] task1\n\n  - [ ] task1_1\n\n    - [ ] task1_1_1\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    let outer = states[0];
    assert_eq!(name(&doc, outer), "task-list");
    assert_eq!(kids(&doc, outer).len(), 1);

    let level1 = kids(&doc, outer)[0];
    assert_eq!(name(&doc, level1), "task-list-item");
    assert_eq!(meta(&doc, level1), BlockMeta::TaskListItem { checked: false });
    assert_eq!(
        child_named(&doc, level1, "paragraph").map(|p| text(&doc, p)),
        Some("task1".to_string())
    );

    let nested1 =
        child_named(&doc, level1, "task-list").expect("level 1 should contain a nested task-list");
    assert_eq!(kids(&doc, nested1).len(), 1);
    let level2 = kids(&doc, nested1)[0];
    assert_eq!(name(&doc, level2), "task-list-item");
    assert_eq!(
        child_named(&doc, level2, "paragraph").map(|p| text(&doc, p)),
        Some("task1_1".to_string())
    );

    let nested2 = child_named(&doc, level2, "task-list")
        .expect("level 2 should contain level 3 nested, not as a sibling");
    assert_eq!(kids(&doc, nested2).len(), 1);
    let level3 = kids(&doc, nested2)[0];
    assert_eq!(name(&doc, level3), "task-list-item");
    assert_eq!(
        child_named(&doc, level3, "paragraph").map(|p| text(&doc, p)),
        Some("task1_1_1".to_string())
    );
}

/// marktext `dec7502e` (PR #741): setext and atx are separate block types.
///
/// **The underline is the literal run — corrected at S1.** S0 transcribed
/// `Underline::Equals` from `types.ts:18`'s comment (`// "===" | "---"`); the
/// declared type is `string`, `walkTokens` writes the whole matched run, and
/// muya's answer for this eleven-`=` input is `"==========="`. The TypeScript
/// spec asserts only `toBeTruthy()`, so the tightening was the transcription's
/// and so was the error. See `mt_doc::Underline`'s correction note.
fn parses_setext_h1_as_setext_heading_level_1() {
    let doc = parse("Hello world\n===========\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "setext-heading");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::SetextHeading { level: 1, underline: Underline::Equals(11) }
    );
}

fn parses_setext_h2_as_setext_heading_level_2() {
    let doc = parse("Hello world\n-----------\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "setext-heading");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::SetextHeading { level: 2, underline: Underline::Dashes(11) }
    );
}

/// Positive control: atx headings stay atx.
fn parses_hash_text_as_atx_heading_not_setext() {
    let doc = parse("# Hello\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(name(&doc, states[0]), "atx-heading");
    assert_eq!(meta(&doc, states[0]), BlockMeta::AtxHeading { level: 1 });
}

/// CommonMark 264, marktext `270d33f6`.
fn starts_a_new_list_when_the_bullet_marker_changes() {
    let doc = parse("- foo\n- bar\n+ baz\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 2);
    assert_eq!(name(&doc, states[0]), "bullet-list");
    assert_eq!(kids(&doc, states[0]).len(), 2);
    assert_eq!(name(&doc, states[1]), "bullet-list");
    assert_eq!(kids(&doc, states[1]).len(), 1);
}

/// CommonMark 265, marktext `270d33f6`.
fn starts_a_new_list_when_the_ordered_delimiter_changes() {
    let doc = parse("1. foo\n2. bar\n3) baz\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 2);
    assert_eq!(name(&doc, states[0]), "order-list");
    assert_eq!(kids(&doc, states[0]).len(), 2);
    assert_eq!(name(&doc, states[1]), "order-list");
    assert_eq!(kids(&doc, states[1]).len(), 1);
}

/// marktext `70d49c30` (#832): a bullet marker must be followed by a space.
fn does_not_parse_dash_foo_without_a_space_as_a_list_item() {
    let doc = parse("-foo\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "paragraph");
    assert_eq!(text(&doc, states[0]), "-foo");
}

fn still_parses_dash_space_foo_as_a_list_item() {
    let doc = parse("- foo\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "bullet-list");
    assert_eq!(name(&doc, kids(&doc, states[0])[0]), "list-item");
}

/// marktext `372fe02f` (#870): `compatibleTaskList` splits where task-ness
/// changes. M2.md §4 C1 mechanism 3 — the port has to split identically or the
/// 1:1 mapping stops holding.
fn splits_a_mixed_task_and_bullet_sequence_into_two_lists() {
    let doc = parse("- [x] foo\n- [x] bar\n- zar\n- rar\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 2);
    let (first, second) = (states[0], states[1]);

    assert_eq!(name(&doc, first), "task-list");
    let task_items = kids(&doc, first);
    assert_eq!(task_items.len(), 2);
    assert!(task_items.iter().all(|i| name(&doc, *i) == "task-list-item"));
    assert!(task_items
        .iter()
        .all(|i| meta(&doc, *i) == BlockMeta::TaskListItem { checked: true }));

    assert_eq!(name(&doc, second), "bullet-list");
    let bullet_items = kids(&doc, second);
    assert_eq!(bullet_items.len(), 2);
    assert!(bullet_items.iter().all(|i| name(&doc, *i) == "list-item"));
    assert_eq!(
        bullet_items
            .iter()
            .map(|i| child_named(&doc, *i, "paragraph").map(|p| text(&doc, p)))
            .collect::<Vec<_>>(),
        vec![Some("zar".to_string()), Some("rar".to_string())]
    );
}

/// `utils/marked/extensions/footnote.ts` emits a block-level `footnote` token
/// when `footnote: true`; it must become a `footnote` state rather than being
/// dropped with an "Unknown type" warning.
fn converts_block_level_footnote_tokens_into_footnote_states() {
    let options = Options { footnote: true, ..NO_EXT };
    let doc = parse("text[^1]\n\n[^1]: definition", options);
    let footnote = top_named(&doc, "footnote").expect("a footnote state should be emitted");
    assert_eq!(
        meta(&doc, footnote),
        BlockMeta::Footnote { identifier: "1".to_string() }
    );
    let first = kids(&doc, footnote)[0];
    assert_eq!(name(&doc, first), "paragraph");
    assert_eq!(text(&doc, first), "definition");
}

fn round_trips_a_single_paragraph_footnote_through_state() {
    let options = Options { footnote: true, ..NO_EXT };
    let out = round_trip("text[^1]\n\n[^1]: definition\n", options);
    assert!(out.contains("[^1]: definition"), "{out:?}");
}

fn keeps_tight_nested_task_lists_nested() {
    let doc = parse("- [ ] task1\n  - [ ] task1_1\n    - [ ] task1_1_1\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    let outer = states[0];
    assert_eq!(name(&doc, outer), "task-list");

    let level1 = kids(&doc, outer)[0];
    let nested1 = child_named(&doc, level1, "task-list").expect("level 1 nests level 2");
    let level2 = kids(&doc, nested1)[0];
    assert!(
        child_named(&doc, level2, "task-list").is_some(),
        "level 2 should contain level 3 nested, not as a sibling"
    );
}

/// #1265. marked already drops one of the three blanks on each side; the
/// option being OFF leaves the remaining surrounding blanks intact.
///
/// `fence_len: None` is M0's constraint 3 in its other direction — muya omits
/// `meta.fenceLength` rather than emitting `null` when the fence is three
/// backticks, so the port must have `None` here and not `Some(3)`.
fn retains_surrounding_blank_lines_when_trim_is_false() {
    let doc = parse("```js\n\n\ncode\n\n\n```\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "code-block");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::CodeBlock {
            kind: CodeKind::Fenced,
            info: "js".to_string(),
            fence_len: None,
        }
    );
    assert_eq!(text(&doc, states[0]), "\n\ncode\n\n");
}

fn strips_leading_and_trailing_blank_lines_when_trim_is_true() {
    let options = Options { trim_unnecessary_code_block_empty_lines: true, ..NO_EXT };
    let doc = parse("```js\n\n\ncode\n\n\n```\n", options);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "code-block");
    assert_eq!(text(&doc, states[0]), "code");
}

fn keeps_interior_blank_lines_while_trimming_the_surrounding_ones() {
    let source = "```js\n\n\na\n\nb\n\n\n```\n";
    let trimmed = parse(
        source,
        Options { trim_unnecessary_code_block_empty_lines: true, ..NO_EXT },
    );
    assert_eq!(text(&trimmed, top(&trimmed)[0]), "a\n\nb");
    let kept = parse(source, NO_EXT);
    assert_eq!(text(&kept, top(&kept)[0]), "\n\na\n\nb\n\n");
}

fn honours_the_trim_option_through_a_state_to_markdown_round_trip() {
    let source = "```js\n\n\ncode\n\n\n```\n";
    assert_eq!(
        round_trip(
            source,
            Options { trim_unnecessary_code_block_empty_lines: true, ..NO_EXT }
        ),
        "```js\ncode\n```\n"
    );
    assert_eq!(round_trip(source, NO_EXT), "```js\n\n\ncode\n\n\n```\n");
}

fn parses_text_then_dashes_as_a_single_level_2_setext_heading() {
    let doc = parse("text\n---\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "setext-heading");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::SetextHeading { level: 2, underline: Underline::Dashes(3) }
    );
    assert_eq!(text(&doc, states[0]), "text");
}

fn parses_text_then_equals_as_a_single_level_1_setext_heading() {
    let doc = parse("text\n===\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "setext-heading");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::SetextHeading { level: 1, underline: Underline::Equals(3) }
    );
    assert_eq!(text(&doc, states[0]), "text");
}

fn parses_a_bare_dashes_line_as_a_single_thematic_break() {
    let doc = parse("---\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "thematic-break");
}

fn parses_three_dashes_as_a_thematic_break() {
    let doc = parse("---\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "thematic-break");
    assert_eq!(text(&doc, states[0]), "---");
}

fn parses_three_stars_as_a_thematic_break() {
    let doc = parse("***\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "thematic-break");
    assert_eq!(text(&doc, states[0]), "***");
}

fn parses_three_underscores_as_a_thematic_break() {
    let doc = parse("___\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "thematic-break");
    assert_eq!(text(&doc, states[0]), "___");
}

fn does_not_parse_mixed_markers_as_a_thematic_break() {
    let doc = parse("-*-\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "paragraph");
    assert_eq!(text(&doc, states[0]), "-*-");
}

/// The `isSingleImage` branch: a lone `<img>` is lowered to a paragraph,
/// anticipating a future image state node.
fn lowers_a_lone_img_tag_to_a_paragraph() {
    let doc = parse("<img src=\"x\">\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "paragraph");
    assert_eq!(text(&doc, states[0]), "<img src=\"x\">");
}

fn keeps_other_html_as_an_html_block_state() {
    let doc = parse("<div>x</div>\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "html-block");
    assert_eq!(text(&doc, states[0]), "<div>x</div>");
}

}
