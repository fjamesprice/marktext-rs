//! `stateToMarkdown.spec.ts` — 11 cases.
//!
//! Table serialization: row-width mismatch (marktext `9884342f`, #4222 /
//! #4190), visual column width (#1983), per-column alignment, and pipe
//! escaping (#3563).
//!
//! Every case that builds a table by hand uses [`doc_of`], because its
//! TypeScript original hands a literal `TState[]` straight to
//! `new ExportMarkdown()` — with no arguments, which is
//! `{ listIndentation: 1 }` and therefore [`NO_EXT`] here.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "state_to_markdown",

/// marktext `9884342f`. `normalizeTable` crashed with
/// `Cannot read properties of undefined (reading 'width')` when a body row had
/// more cells than the header.
fn does_not_crash_when_a_body_row_has_more_cells_than_the_header() {
    let doc = doc_of(vec![table(vec![
        row(vec![cell("a", Align::None), cell("b", Align::None)]),
        row(vec![
            cell("1", Align::None),
            cell("2", Align::None),
            cell("3", Align::None),
            cell("4", Align::None),
        ]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    assert!(md.contains("| a"), "{md:?}");
    assert!(md.contains("| b"), "{md:?}");
    assert!(!md.contains("| 3"), "surplus cells are dropped: {md:?}");
    assert!(!md.contains("| 4"), "surplus cells are dropped: {md:?}");
}

/// The other half of `9884342f`: `Cannot read properties of undefined
/// (reading 'length')` when a body row had fewer cells than the header.
fn does_not_crash_when_a_body_row_has_fewer_cells_than_the_header() {
    let doc = doc_of(vec![table(vec![
        row(vec![
            cell("a", Align::None),
            cell("b", Align::None),
            cell("c", Align::None),
        ]),
        row(vec![cell("1", Align::None)]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    assert!(md.contains("| a"), "{md:?}");
    assert!(md.contains("| c"), "{md:?}");
    assert!(md.contains("| 1"), "{md:?}");
}

fn serialises_a_well_formed_table_normally() {
    let doc = doc_of(vec![table(vec![
        row(vec![cell("a", Align::None), cell("b", Align::None)]),
        row(vec![cell("1", Align::None), cell("2", Align::None)]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    for wanted in ["| a", "| b", "| 1", "| 2"] {
        assert!(md.contains(wanted), "{wanted:?} missing from {md:?}");
    }
}

/// #1983. Column padding used `String.prototype.length`, so a cell containing
/// a combining mark was over-measured and broke alignment. Padding must use
/// **visual** column width — `aʊ̯x` is four code units and three columns.
fn aligns_a_column_whose_cells_contain_combining_marks() {
    let doc = doc_of(vec![table(vec![
        row(vec![cell("A", Align::None)]),
        row(vec![cell("nɔx", Align::None)]),
        row(vec![cell("a\u{28a}\u{32f}x", Align::None)]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    let lines: Vec<&str> = md.split('\n').collect();
    assert_eq!(lines[0], "| A   |");
    assert_eq!(lines[1], "| --- |");
    assert_eq!(lines[2], "| nɔx |");
    assert_eq!(lines[3], "| a\u{28a}\u{32f}x |");
}

/// East-Asian wide characters are two columns each: `中文` is 4, so the inner
/// width is 4 rather than `id`'s 2.
fn widens_a_column_to_fit_east_asian_wide_characters() {
    let doc = doc_of(vec![table(vec![
        row(vec![cell("id", Align::None)]),
        row(vec![cell("中文", Align::None)]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    let lines: Vec<&str> = md.split('\n').collect();
    assert_eq!(lines[0], "| id   |");
    assert_eq!(lines[1], "| ---- |");
    assert_eq!(lines[2], "| 中文 |");
}

fn renders_the_delimiter_row_from_per_column_align() {
    let doc = doc_of(vec![table(vec![
        row(vec![
            cell("a", Align::Left),
            cell("b", Align::Center),
            cell("c", Align::Right),
        ]),
        row(vec![
            cell("1", Align::None),
            cell("2", Align::None),
            cell("3", Align::None),
        ]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    let delimiter = md.split('\n').nth(1).expect("a delimiter row");
    assert!(delimiter.contains(":---"), "left: {delimiter:?}");
    assert!(delimiter.contains(":---:"), "center: {delimiter:?}");
    assert!(delimiter.contains("---:"), "right: {delimiter:?}");
}

/// The delimiter comes from the **header** row's aligns; body-row aligns are
/// ignored. `none` renders as `| --- |`, with the dashes space-wrapped.
fn the_header_row_drives_the_delimiter_not_body_rows() {
    let doc = doc_of(vec![table(vec![
        row(vec![cell("a", Align::Center), cell("b", Align::None)]),
        row(vec![cell("1", Align::Right), cell("2", Align::Left)]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    let delimiter = md.split('\n').nth(1).expect("a delimiter row");
    assert!(delimiter.contains(":---:"));
    assert_eq!(delimiter, "|:---:| --- |");
}

fn round_trips_a_left_center_right_table_to_a_byte_stable_delimiter_row() {
    let md = "| a | b | c |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |\n";
    let first_pass = round_trip(md, NO_EXT);
    let second_pass = round_trip(&first_pass, NO_EXT);

    let delimiter = first_pass.split('\n').nth(1).expect("a delimiter row");
    assert_eq!(delimiter, "|:--- |:---:| ---:|");
    assert!(delimiter.contains(":---"));
    assert!(delimiter.contains(":---:"));
    assert!(delimiter.contains("---:"));

    assert_eq!(second_pass, first_pass, "save → reopen → save must be byte-stable");
    assert_eq!(second_pass.split('\n').nth(1), Some(delimiter));
}

/// marktext #3563. `escapeText` used `/([^\\])\|/g`, which needs a
/// non-backslash character *before* the pipe — so a pipe at the start of a
/// cell was never escaped, and on reopening it was read as a column separator
/// and the rest of the cell was eaten.
fn escapes_a_pipe_at_the_very_start_of_a_cell() {
    let doc = doc_of(vec![table(vec![row(vec![
        cell("|lead", Align::None),
        cell("b", Align::None),
    ])])]);
    let md = serialize(&doc, NO_EXT);
    assert!(md.contains("\\|lead"), "{md:?}");
}

/// The same gap in its other form: the second of two consecutive pipes.
fn escapes_both_of_two_consecutive_pipes_in_a_cell() {
    let doc = doc_of(vec![table(vec![row(vec![
        cell("a||b", Align::None),
        cell("c", Align::None),
    ])])]);
    let md = serialize(&doc, NO_EXT);
    assert!(md.contains("a\\|\\|b"), "{md:?}");
}

/// #3563's actual failure mode was *progressive* corruption across
/// save → reopen → save, so byte stability is the assertion that matters.
fn round_trips_a_cell_starting_with_a_pipe_byte_stably() {
    let doc = doc_of(vec![table(vec![
        row(vec![cell("head1", Align::None), cell("head2", Align::None)]),
        row(vec![cell("|danger", Align::None), cell("keep", Align::None)]),
    ])]);
    let md = serialize(&doc, NO_EXT);
    assert!(md.contains("\\|danger"), "{md:?}");

    let reparsed = parse(&md, NO_EXT);
    let table_node = top_named(&reparsed, "table").expect("a table state");
    let body_row = kids(&reparsed, table_node)[1];
    let cells = kids(&reparsed, body_row);
    assert_eq!(cells.len(), 2, "no phantom column, no content shift");
    assert!(text(&reparsed, cells[0]).contains("danger"));
    assert_eq!(text(&reparsed, cells[1]), "keep");

    assert_eq!(serialize(&reparsed, NO_EXT), md);
}

}
