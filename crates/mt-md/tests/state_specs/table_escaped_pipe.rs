//! `tableEscapedPipe.spec.ts` — 4 cases.
//!
//! #4849. GFM escapes `|` inside a table cell with a backslash; after the
//! table is parsed that escape is a literal `|`, so `` `\|` `` must display as
//! `<code>|</code>`. The stored cell text re-added the escape, which leaked
//! into the editor's inline-code display — the backslash rule does not apply
//! inside code. Serialization re-escapes on its own, so the round trip is kept
//! either way.
//!
//! This is **M2.md §4 C2's largest leaf class**: `table.cell` accounts for 924
//! of the 1,319 leaves whose text differs from a naive source slice, by trim
//! and by exactly this `\|` resolution. D2's rule for `table.cell` is "trim,
//! and resolve `\|` → `|`", and the first case here is that rule's test.
//!
//! The first two cases originally read the rendered DOM — `td code`
//! `textContent`, and a `<tr>` count. Their expressible half is the parsed
//! *state*, which is where the escape resolution actually happens: the DOM
//! only displayed what the cell text held.

#![allow(unused_imports)]
use crate::*;

/// The spec's shared table: two data rows, both with inline code holding
/// escaped pipes.
const TABLE: &str = "| a | b |\n| --- | --- |\n| `\\|` | x |\n| `\\|\\|` | y |\n";

spec_cases! { "table_escaped_pipe",

/// D2's `table.cell` rule. The escape belongs to the *table* syntax, so it is
/// gone from the cell's text; the inline tokenizer then sees `` `|` `` and
/// emits a code span containing a bare pipe.
fn an_escaped_pipe_inside_code_is_stored_as_a_bare_pipe() {
    let doc = parse(super::TABLE, MUYA_DEFAULT);
    let table_node = top_named(&doc, "table").expect("a table state");
    let rows = kids(&doc, table_node);
    let first_cells: Vec<String> = rows[1..]
        .iter()
        .map(|r| text(&doc, kids(&doc, *r)[0]))
        .collect();
    assert_eq!(first_cells, vec!["`|`".to_string(), "`||`".to_string()]);
}

/// The escaped pipes must not have been read as column separators.
fn keeps_the_table_structure_two_columns_three_rows() {
    let doc = parse(super::TABLE, MUYA_DEFAULT);
    let table_node = top_named(&doc, "table").expect("a table state");
    let rows = kids(&doc, table_node);
    assert_eq!(rows.len(), 3, "header plus two body rows");
    assert_eq!(kids(&doc, rows[2]).len(), 2, "the last row has two cells");
}

fn round_trips_the_escaped_pipes() {
    let md = round_trip(super::TABLE, MUYA_DEFAULT);
    assert!(md.contains("`\\|`"), "{md:?}");
    assert!(md.contains("`\\|\\|`"), "{md:?}");
}

fn round_trips_an_escaped_pipe_in_plain_cell_text() {
    let md = round_trip("| a | b |\n| --- | --- |\n| x \\| y | z |\n", MUYA_DEFAULT);
    assert!(md.contains("x \\| y"), "{md:?}");
}

}
