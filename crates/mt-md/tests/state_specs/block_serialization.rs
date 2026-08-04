//! `blockSerialization.spec.ts` — 19 of its 22 cases.
//!
//! `markdown → state → markdown` identity for every non-list block type.
//! These lock the current contract rather than change it, so that a future
//! refactor of `stateToMarkdown` cannot silently break one.
//!
//! **Three of the file's 22 are out of scope**, and the module docs here say
//! so rather than leaving them to be re-discovered: the
//! `codeBlock — setting lang promotes an indented block to fenced` describe
//! block boots a `Muya` instance and drives `codeBlock.lang = 'js'`. Its three
//! cases assert (1) the live block object's `meta` and a rAF-flushed OT op,
//! (2) a DOM class swap from `mu-indented-code` to `mu-fenced-code`, and
//! (3) a characterized bug where the setter dispatches an op for `meta.type`
//! and not for `meta.lang`, so the language is dropped on save. All three are
//! about `block/commonMark/codeBlock.ts` and the editor's operation cache,
//! which is M4's. The *model* half — that promoting indented → fenced is a
//! `SetMeta` and not a `ReplaceBlock`, so the body survives — is asserted in
//! `mt_doc`'s `edit::tests::set_meta_promotes_an_indented_code_block_to_fenced`.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "block_serialization",

fn round_trips_an_atx_heading_at_every_level() {
    let md = "# h1\n\n## h2\n\n### h3\n\n#### h4\n\n##### h5\n\n###### h6\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_setext_h1_with_equals_underline() {
    let md = "Hello world\n===========\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_setext_h2_with_dashes_underline() {
    let md = "Hello world\n-----------\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_thematic_break() {
    let md = "before\n\n---\n\nafter\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_fenced_code_block_with_a_language_tag() {
    let md = "```js\nconst x = 1;\nconst y = 2;\n```\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_fenced_code_block_without_a_language_tag() {
    let md = "```\nplain code\ntwo lines\n```\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_fenced_code_block_containing_blank_lines() {
    let md = "```js\nline 1\n\nline 3\n```\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_single_line_blockquote() {
    let md = "> quoted\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_multi_line_blockquote() {
    let md = "> first line\n> second line\n> third line\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_nested_blockquote() {
    let md = "> outer\n>\n> > inner quoted\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_dollar_dollar_math_block() {
    let md = "$$\na^2 + b^2 = c^2\n$$\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

/// The column width `stateToMarkdown` emits is `max(5, cell + 2)`, so a table
/// of single-character cells canonicalises to width-5 columns.
fn round_trips_a_simple_2x2_table_with_default_alignment() {
    let md = "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_table_with_explicit_alignment() {
    let md = "| a   | b   | c   |\n|:--- |:---:| ---:|\n| 1   | 2   | 3   |\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

/// Pipes inside cells are `\|`-escaped at parse time
/// (`markdownToState.restoreTableEscapeCharacters`) and again at serialize
/// time. Column 2 holds `b \|piped`, nine characters, so its width is 11.
fn round_trips_a_cell_containing_an_escaped_pipe() {
    let md = "| a   | b \\|piped |\n| --- | --------- |\n| 1   | 2         |\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn serialises_an_empty_trailing_cell_as_a_blank_cell() {
    let md = "| a   | b   |\n| --- | --- |\n| 1   |     |\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

/// An indented block carries no info string, so `meta.lang` is the **empty
/// string** — not `undefined`, and not absent.
fn parses_a_four_space_indented_block_as_an_indented_code_block() {
    let doc = parse("    code\n", BLOCKS);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "code-block");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::CodeBlock {
            kind: CodeKind::Indented,
            info: String::new(),
            fence_len: None,
        }
    );
    assert_eq!(text(&doc, states[0]), "code");
}

/// `serializeCodeBlock`'s `type !== 'fenced'` branch prefixes every line with
/// exactly four spaces.
fn round_trips_an_indented_code_block() {
    let md = "    code\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

fn round_trips_a_multi_line_indented_code_block() {
    let md = "    line one\n    line two\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

/// `FRONT_REG` (`utils/marked/frontMatter.ts`) requires two newlines after the
/// closing `---`, so canonical YAML front matter has a blank line before the
/// body.
fn round_trips_a_yaml_frontmatter_block() {
    let md = "---\ntitle: hello\nauthor: world\n---\n\n# body\n";
    assert_eq!(round_trip(md, BLOCKS), md);
}

}
