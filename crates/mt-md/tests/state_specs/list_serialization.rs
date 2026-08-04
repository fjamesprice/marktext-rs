//! `listSerialization.spec.ts` — 30 cases.
//!
//! Empty list items, the `listIndentation` option (marktext `02841ffd`,
//! PR #916), blockquote nesting (marktext `5f191681`, PR #840), looseness
//! (`preferLooseListItem`) and ordered-list start + delimiter.
//!
//! The long fixtures are written flush against the left margin because they
//! are compared byte-for-byte: a fixture that is indented to match the
//! surrounding code is a different fixture.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "list_serialization",

fn keeps_consecutive_empty_task_items_on_separate_lines() {
    assert_eq!(round_trip("- [ ] \n- [ ] \n", NO_EXT), "- [ ] \n- [ ] \n");
    assert_eq!(round_trip("- [ ] \n- [x] \n", NO_EXT), "- [ ] \n- [x] \n");
}

fn keeps_consecutive_empty_bullet_items_on_separate_lines() {
    assert_eq!(round_trip("- \n- \n", NO_EXT), "- \n- \n");
}

fn keeps_adjacent_empty_bullet_items_before_a_following_paragraph() {
    let md = "- \n- \n- \n\nz\n";
    let out = round_trip(md, NO_EXT);
    assert_eq!(out, md);

    let reparsed = parse(&out, NO_EXT);
    let states = top(&reparsed);
    assert_eq!(name(&reparsed, states[0]), "bullet-list");
    assert_eq!(kids(&reparsed, states[0]).len(), 3);
    assert_eq!(name(&reparsed, states[1]), "paragraph");
}

fn keeps_an_empty_bullet_item_between_populated_sibling_items() {
    let md = "- a\n- \n- c\n";
    let out = round_trip(md, NO_EXT);
    assert_eq!(out, md);

    let reparsed = parse(&out, NO_EXT);
    let list = top(&reparsed)[0];
    assert_eq!(name(&reparsed, list), "bullet-list");
    let items = kids(&reparsed, list);
    assert_eq!(items.len(), 3);
    assert_eq!(names(&reparsed, &kids(&reparsed, items[1])), ["paragraph"]);
    assert_eq!(text(&reparsed, kids(&reparsed, items[1])[0]), "");
}

/// `SETEXT_SAFE_BULLET_MARKER`: a nested list of empty `-` items would
/// serialize as `- ` lines that reparse as a setext underline, so the
/// serializer switches the nested marker to `*` rather than making the parent
/// list loose.
fn uses_an_alternate_nested_marker_instead_of_making_a_tight_list_loose() {
    let nested = bullet_list(BulletMarker::Dash, false, vec![empty_item(), empty_item()]);
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        false,
        vec![list_item(vec![para("a"), nested])],
    )]);
    let out = serialize(&doc, NO_EXT);
    assert_eq!(out, "- a\n  * \n  * \n");

    let reparsed = parse(&out, NO_EXT);
    let outer = top(&reparsed)[0];
    assert_eq!(name(&reparsed, outer), "bullet-list");
    assert_eq!(
        meta(&reparsed, outer),
        BlockMeta::BulletList { marker: BulletMarker::Dash, loose: false }
    );
    let parent_item = kids(&reparsed, outer)[0];
    assert_eq!(text(&reparsed, kids(&reparsed, parent_item)[0]), "a");
    assert!(
        child_named(&reparsed, parent_item, "setext-heading").is_none(),
        "the empty items must not have reparsed as a setext underline"
    );
    let inner = child_named(&reparsed, parent_item, "bullet-list").expect("a nested bullet-list");
    assert_eq!(
        meta(&reparsed, inner),
        BlockMeta::BulletList { marker: BulletMarker::Star, loose: false }
    );
    assert_eq!(kids(&reparsed, inner).len(), 2);
    assert_eq!(serialize(&reparsed, NO_EXT), out);
}

fn uses_the_same_safe_marker_under_ordered_and_task_list_parents() {
    let ordered = doc_of(vec![order_list(
        1,
        OrderDelim::Period,
        false,
        vec![list_item(vec![
            para("a"),
            bullet_list(BulletMarker::Dash, false, vec![empty_item(), empty_item()]),
        ])],
    )]);
    let ordered_out = serialize(&ordered, NO_EXT);
    assert_eq!(ordered_out, "1. a\n   * \n   * \n");
    let reparsed = parse(&ordered_out, NO_EXT);
    let list = top(&reparsed)[0];
    assert_eq!(name(&reparsed, list), "order-list");
    assert_eq!(
        meta(&reparsed, list),
        BlockMeta::OrderList { start: 1, delimiter: OrderDelim::Period, loose: false }
    );
    let nested = child_named(&reparsed, kids(&reparsed, list)[0], "bullet-list").expect("nested");
    assert_eq!(kids(&reparsed, nested).len(), 2);

    let task = doc_of(vec![task_list(
        BulletMarker::Dash,
        false,
        vec![task_item(
            false,
            vec![
                para("a"),
                bullet_list(BulletMarker::Dash, false, vec![empty_item(), empty_item()]),
            ],
        )],
    )]);
    let task_out = serialize(&task, NO_EXT);
    assert_eq!(task_out, "- [ ] a\n  * \n  * \n");
    let reparsed = parse(&task_out, NO_EXT);
    let list = top(&reparsed)[0];
    assert_eq!(name(&reparsed, list), "task-list");
    assert_eq!(
        meta(&reparsed, list),
        BlockMeta::TaskList { marker: BulletMarker::Dash, loose: false }
    );
    let nested = child_named(&reparsed, kids(&reparsed, list)[0], "bullet-list").expect("nested");
    assert_eq!(kids(&reparsed, nested).len(), 2);
}

fn handles_a_first_empty_nested_item_followed_by_a_non_empty_item() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        false,
        vec![list_item(vec![
            para("a"),
            bullet_list(
                BulletMarker::Dash,
                false,
                vec![empty_item(), list_item(vec![para("b")])],
            ),
        ])],
    )]);
    let out = serialize(&doc, NO_EXT);
    assert_eq!(out, "- a\n  * \n  * b\n");

    let reparsed = parse(&out, NO_EXT);
    let parent_item = kids(&reparsed, top(&reparsed)[0])[0];
    let nested = child_named(&reparsed, parent_item, "bullet-list").expect("nested");
    let items = kids(&reparsed, nested);
    assert_eq!(items.len(), 2);
    assert_eq!(text(&reparsed, kids(&reparsed, items[1])[0]), "b");
}

fn does_not_rewrite_nested_dash_lists_whose_first_item_is_not_empty() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        false,
        vec![list_item(vec![
            para("a"),
            bullet_list(
                BulletMarker::Dash,
                false,
                vec![list_item(vec![para("b")]), empty_item()],
            ),
        ])],
    )]);
    let out = serialize(&doc, NO_EXT);
    assert_eq!(out, "- a\n  - b\n  - \n");

    let reparsed = parse(&out, NO_EXT);
    let parent_item = kids(&reparsed, top(&reparsed)[0])[0];
    let nested = child_named(&reparsed, parent_item, "bullet-list").expect("nested");
    assert_eq!(
        meta(&reparsed, nested),
        BlockMeta::BulletList { marker: BulletMarker::Dash, loose: false }
    );
    assert_eq!(kids(&reparsed, nested).len(), 2);
    assert_eq!(serialize(&reparsed, NO_EXT), out);
}

fn keeps_already_loose_parent_lists_on_their_original_dash_marker() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        true,
        vec![list_item(vec![
            para("a"),
            bullet_list(BulletMarker::Dash, false, vec![empty_item(), empty_item()]),
        ])],
    )]);
    let out = serialize(&doc, NO_EXT);
    assert_eq!(out, "- a\n\n  - \n  - \n");

    let reparsed = parse(&out, NO_EXT);
    let outer = top(&reparsed)[0];
    assert_eq!(
        meta(&reparsed, outer),
        BlockMeta::BulletList { marker: BulletMarker::Dash, loose: true }
    );
    let nested =
        child_named(&reparsed, kids(&reparsed, outer)[0], "bullet-list").expect("nested");
    assert_eq!(
        meta(&reparsed, nested),
        BlockMeta::BulletList { marker: BulletMarker::Dash, loose: false }
    );
    assert_eq!(kids(&reparsed, nested).len(), 2);
}

fn serializes_parser_created_empty_list_items_as_separate_lines() {
    assert_eq!(round_trip("- \n- \n", NO_EXT), "- \n- \n");
}

/// marktext `02841ffd` (PR #916). `stateToMarkdown.ts` splits "subsequent
/// paragraph indent" (marker width) from "nested list indent" (configurable);
/// these four fixtures pin the second.
fn indent_by_1_space_round_trips_the_marktext_fixture() {
    let md = r"start

- foo
- foo
  - foo
  - foo
    - foo
    - foo
      - foo
  - foo
- foo

sep

1. foo
2. foo
   1. foo
   2. foo
      1. foo
   3. foo
3. foo
   20. foo
       141. foo
            1. foo
";
    assert_eq!(
        round_trip(md, NO_EXT.with_list_indentation(ListIndentation::Spaces(1))),
        md
    );
}

fn indent_by_2_spaces_round_trips_the_marktext_fixture() {
    let md = r"start

- foo
- foo
   - foo
   - foo
      - foo
      - foo
         - foo
   - foo
- foo

sep

1. foo
2. foo
    1. foo
    2. foo
        1. foo
    3. foo
3. foo
    20. foo
         141. foo
               1. foo
";
    assert_eq!(
        round_trip(md, NO_EXT.with_list_indentation(ListIndentation::Spaces(2))),
        md
    );
}

fn indent_by_3_spaces_round_trips_the_marktext_fixture() {
    let md = r"start

- foo
- foo
    - foo
    - foo
        - foo
        - foo
            - foo
    - foo
- foo

sep

1. foo
2. foo
     1. foo
     2. foo
          1. foo
     3. foo
3. foo
     20. foo
           141. foo
                  1. foo
";
    assert_eq!(
        round_trip(md, NO_EXT.with_list_indentation(ListIndentation::Spaces(3))),
        md
    );
}

/// marktext `5f191681` (PR #840), from `test/unit/data/common/Blockquotes.md`.
fn round_trips_an_ordered_list_nested_inside_a_blockquote() {
    let md = "> 1. Lorem Ipsum is simply dummy text 1\n\
              > 2. Lorem Ipsum is simply dummy text 2\n\
              > 3. Lorem Ipsum is simply dummy text 3\n";
    assert_eq!(round_trip(md, NO_EXT), md);
}

fn round_trips_a_bullet_list_nested_inside_a_blockquote() {
    let md = "> - one\n> - two\n> - three\n";
    assert_eq!(round_trip(md, NO_EXT), md);
}

fn round_trips_a_blockquote_nested_inside_a_list_item() {
    let md = "- foo\n- > bar\n- baz\n";
    assert_eq!(round_trip(md, NO_EXT), md);
}

fn round_trips_a_loose_list_with_a_subsequent_paragraph() {
    let md = "- foo\n\n  Second paragraph in the same item.\n\n- bar\n";
    assert_eq!(round_trip(md, NO_EXT), md);
}

fn round_trips_a_loose_list_containing_a_fenced_code_block() {
    let md = "- foo\n\n  ```\n  code line 1\n  code line 2\n  ```\n\n- bar\n";
    assert_eq!(round_trip(md, NO_EXT), md);
}

/// A latent `insertLineBreak` bug marktext shipped: a blank line inside a list
/// item carried the item's indent as trailing whitespace (`"  \n"` rather than
/// `"\n"`). Fixed as part of the `stateToMarkdown` baseline.
fn does_not_emit_trailing_whitespace_on_blank_lines_inside_a_list_item() {
    let out = round_trip("- foo\n\n  bar\n", NO_EXT);
    for line in out.split('\n') {
        if line.trim().is_empty() {
            assert_eq!(line, "", "a blank line carried trailing whitespace: {out:?}");
        }
    }
}

fn round_trips_an_ordered_list_with_two_digit_item_numbers() {
    let md = "1. one\n2. two\n3. three\n4. four\n5. five\n\
              6. six\n7. seven\n8. eight\n9. nine\n10. ten\n";
    assert_eq!(round_trip(md, NO_EXT), md);
}

fn indent_by_4_spaces_round_trips_the_marktext_fixture() {
    let md = r"start

- foo
- foo
     - foo
     - foo
          - foo
          - foo
               - foo
     - foo
- foo

sep

1. foo
2. foo
      1. foo
      2. foo
            1. foo
      3. foo
3. foo
      20. foo
             141. foo
                     1. foo
";
    assert_eq!(
        round_trip(md, NO_EXT.with_list_indentation(ListIndentation::Spaces(4))),
        md
    );
}

/// Daring Fireball Markdown: nested items indent by a hard four spaces
/// regardless of marker width — note `99.` rather than `141.`, which is what
/// the fixed indent produces.
fn indent_using_daring_fireball_round_trips_the_marktext_fixture() {
    let md = r"start

- foo
- foo
    - foo
    - foo
        - foo
        - foo
            - foo
    - foo
- foo

sep

1. foo
2. foo
    1. foo
    2. foo
        1. foo
    3. foo
3. foo
    20. foo
        99. foo
            1. foo
";
    assert_eq!(round_trip(md, NO_EXT.with_list_indentation(ListIndentation::Dfm)), md);
}

/// **Characterization, not a spec for desired behaviour.** The desktop
/// preferences UI offers a `'tab'` list-indentation option and neither engine
/// ever implemented it: `stateToMarkdown`'s constructor branches on `'dfm'`
/// and `typeof === 'number'` and everything else falls through to a 1-space
/// indent. `mt_md::ListIndentation` has no `Tab` variant for the same reason,
/// so this transcribes as the 1-space fixture plus the claim that they agree.
///
/// If `'tab'` ever gets a real implementation this assertion SHOULD fail.
fn treats_the_unimplemented_tab_option_as_a_1_space_indent() {
    let md = r"start

- foo
- foo
  - foo
  - foo
    - foo
    - foo
      - foo
  - foo
- foo

sep

1. foo
2. foo
   1. foo
   2. foo
      1. foo
   3. foo
3. foo
   20. foo
       141. foo
            1. foo
";
    // `ListIndentation::spaces` clamps exactly as muya's constructor does, so
    // the out-of-range value the UI would produce lands on 1 the same way.
    let tab_equivalent = NO_EXT.with_list_indentation(ListIndentation::spaces(0));
    assert_eq!(round_trip(md, tab_equivalent), md);
    assert_eq!(
        round_trip(md, tab_equivalent),
        round_trip(md, NO_EXT.with_list_indentation(ListIndentation::Spaces(1)))
    );
}

/// `meta.loose` is seeded from `muya.options.preferLooseListItem` at list
/// creation; the serializer is its only consumer.
fn inserts_blank_lines_between_items_when_loose_is_true() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        true,
        vec![
            list_item(vec![para("foo")]),
            list_item(vec![para("bar")]),
            list_item(vec![para("baz")]),
        ],
    )]);
    assert_eq!(serialize(&doc, NO_EXT), "- foo\n\n- bar\n\n- baz\n");
}

fn keeps_items_adjacent_when_loose_is_false() {
    let doc = doc_of(vec![bullet_list(
        BulletMarker::Dash,
        false,
        vec![
            list_item(vec![para("foo")]),
            list_item(vec![para("bar")]),
            list_item(vec![para("baz")]),
        ],
    )]);
    assert_eq!(serialize(&doc, NO_EXT), "- foo\n- bar\n- baz\n");
}

fn list_meta_loose_carries_the_prefer_loose_list_item_flag_verbatim() {
    let loose = parse("- foo\n\n- bar\n", NO_EXT);
    let list = top(&loose)[0];
    assert_eq!(name(&loose, list), "bullet-list");
    assert_eq!(
        meta(&loose, list),
        BlockMeta::BulletList { marker: BulletMarker::Dash, loose: true }
    );

    let tight = parse("- foo\n- bar\n", NO_EXT);
    let list = top(&tight)[0];
    assert_eq!(
        meta(&tight, list),
        BlockMeta::BulletList { marker: BulletMarker::Dash, loose: false }
    );
}

/// The parser stores `start = 3`; the serializer renders `3.`, `4.` — the item
/// number is `meta.start + i`, not a re-count from 1.
fn keeps_a_non_1_start_number_through_the_round_trip() {
    assert_eq!(round_trip("3. one\n4. two\n", NO_EXT), "3. one\n4. two\n");
}

fn parses_the_start_number_into_order_list_meta_start() {
    let doc = parse("3. one\n4. two\n", NO_EXT);
    let list = top(&doc)[0];
    assert_eq!(name(&doc, list), "order-list");
    assert_eq!(
        meta(&doc, list),
        BlockMeta::OrderList { start: 3, delimiter: OrderDelim::Period, loose: false }
    );
}

fn emits_the_configured_delimiter_for_an_ordered_list() {
    let doc = doc_of(vec![order_list(
        1,
        OrderDelim::Paren,
        false,
        vec![list_item(vec![para("one")]), list_item(vec![para("two")])],
    )]);
    assert_eq!(serialize(&doc, NO_EXT), "1) one\n2) two\n");
}

fn combines_a_non_1_start_with_the_paren_delimiter() {
    let doc = doc_of(vec![order_list(
        5,
        OrderDelim::Paren,
        false,
        vec![list_item(vec![para("one")]), list_item(vec![para("two")])],
    )]);
    assert_eq!(serialize(&doc, NO_EXT), "5) one\n6) two\n");
}

}
