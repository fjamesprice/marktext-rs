//! [`mt_doc::Document`] → markdown. `packages/muya/src/state/stateToMarkdown.ts`,
//! 626 lines, transcribed.
//!
//! # What this is a port of, and what that word is doing
//!
//! `ExportMarkdown` is a small class with four fields and one interesting idea:
//! a **stack of open list metas**, pushed on descent into a bullet / ordered /
//! task list and popped on ascent, from which the item serializer reads the
//! bullet, the delimiter, the start number and the looseness. Everything else
//! is one function per block kind emitting that kind's syntax around
//! [`mt_doc::Block::text`].
//!
//! Two of those functions are not that, and they are where the round trip is
//! won or lost:
//!
//! - [`ExportMarkdown::serialize_table`] pads every column to a **visual**
//!   width computed with muya's own [`crate::string_width`], not with a
//!   character count and not with UAX #11. The `+ 2` and the `- 1` in its
//!   arithmetic are load-bearing: the first reserves the space on each side of
//!   a cell, the second pays for the leading one back.
//! - [`escape_text`] re-adds the table `\|` escape that S2 deliberately
//!   resolved on the way in (§4 C2, muya #4849). The two halves have to
//!   compose exactly or `tableEscapedPipe`'s round trip is lossy in a direction
//!   `cargo xtask blocks` cannot see, because that harness compares state and
//!   both engines' state agrees.
//!
//! # Infallible, and what that costs
//!
//! [`crate::serialize`] returns a `String` rather than a `Result` from S4 on,
//! for the reason S3 gave when it did the same to [`crate::parse`]: a `Result`
//! whose `Err` arm no input can reach is the shape this milestone distrusts.
//!
//! The question is sharper here than it was there, because `serialize` takes a
//! `&Document` and a `Document` can be **hand-built with a shape
//! `MarkdownToState` cannot produce**. `stateToMarkdown.spec.ts` has two cases
//! for exactly that — a body row with more cells than the header, and one with
//! fewer — and both assert that muya does not crash. It does not: the
//! serializer truncates the long row against the header's column count and
//! emits the short one short. So the question was never "can it fail" but "what
//! does muya do", and the specs answer it: **degraded output, not an error.**
//! The port reproduces the degradation.
//!
//! There are three shapes for which muya's answer is a JavaScript `TypeError`
//! rather than degraded output, and all three are documents `parse` cannot
//! build:
//!
//! | Shape | muya | here |
//! |---|---|---|
//! | a `table` with no rows | `Cannot read properties of undefined (reading 'children')` | emits nothing |
//! | a `list-item` with no enclosing list | `Cannot destructure property 'loose' of undefined` | emits nothing |
//! | a `table.row` or `table.cell` at top level | `debug.warn`, emits nothing | emits nothing |
//!
//! The third is muya's own behaviour and is reproduced. The first two are
//! reproduced *as* the third — a crash is not a behaviour worth porting, and
//! the alternative is an `Err` arm reachable only from a document no parse
//! produces, which is the shape this crate has now twice decided against.
//! [`tests::the_three_shapes_that_throw_in_javascript_emit_nothing_here`] pins
//! all three so the decision is checkable rather than merely written down.
//!
//! `mt-cli` has no `--to-markdown` today; when S5 gives it `--to-html` the same
//! rule applies — the exit code that means "unimplemented" stays defined, and
//! there is no exit code for "your document was strange", because there is no
//! document for which this function declines to answer.

use mt_doc::{
    Align, Block, BulletMarker, CodeKind, DiagramKind, Document, FrontmatterLang, FrontmatterStyle,
    MathStyle, NodeId, OrderDelim,
};

use crate::string_width::string_width;
use crate::{ListIndentation, Options};

/// `const SETEXT_SAFE_BULLET_MARKER = '*'`.
///
/// A nested list of empty `-` items serializes as `- ` lines, and a `- ` line
/// under a paragraph reparses as a **setext underline** rather than as a list.
/// muya's answer is to change the nested marker rather than to make the parent
/// list loose, which would change the document's meaning to fix its spelling.
const SETEXT_SAFE_BULLET_MARKER: BulletMarker = BulletMarker::Star;

/// `Document` → markdown, with `options.list_indentation` as
/// `ExportMarkdown`'s only setting.
#[must_use]
pub fn to_markdown(doc: &Document, options: Options) -> String {
    let mut export = ExportMarkdown {
        doc,
        list_type: Vec::new(),
        is_loose_parent_list: true,
        list_indentation: options.list_indentation,
        depth: 0,
    };
    let top = doc.children(doc.root()).to_vec();
    export.convert(&top, "", "")
}

/// One entry of `ExportMarkdown._listType` — the `meta` of an open list.
///
/// muya pushes `deepClone(state.meta)` and `_serializeListItem` discriminates
/// with `'marker' in listInfo`, so bullet lists and task lists are one case
/// there and are one variant here. The clone matters and is not incidental:
/// the item serializer **mutates** `listInfo.start++` as it walks, so the copy
/// is what stops serializing a document from renumbering it.
#[derive(Debug, Clone, Copy)]
enum ListMeta {
    /// `bullet-list` and `task-list`.
    Unordered { marker: BulletMarker, loose: bool },
    /// `order-list`.
    Ordered {
        start: u32,
        delimiter: OrderDelim,
        loose: bool,
    },
}

impl ListMeta {
    fn loose(self) -> bool {
        match self {
            ListMeta::Unordered { loose, .. } | ListMeta::Ordered { loose, .. } => loose,
        }
    }

    /// `'delimiter' in meta ? meta.delimiter : meta.marker` — what
    /// `_serializeListBlock` returns and `lastListBullet` remembers.
    ///
    /// A `char` rather than an enum because the two vocabularies are compared
    /// against each other: *"changing the bullet or ordered list delimiter
    /// starts a new list"* is one comparison over both.
    fn bullet_marker_or_delimiter(self) -> char {
        match self {
            ListMeta::Unordered { marker, .. } => marker_char(marker),
            ListMeta::Ordered { delimiter, .. } => delimiter_char(delimiter),
        }
    }
}

fn marker_char(marker: BulletMarker) -> char {
    match marker {
        BulletMarker::Dash => '-',
        BulletMarker::Plus => '+',
        BulletMarker::Star => '*',
    }
}

fn delimiter_char(delimiter: OrderDelim) -> char {
    match delimiter {
        OrderDelim::Period => '.',
        OrderDelim::Paren => ')',
    }
}

struct ExportMarkdown<'a> {
    doc: &'a Document,
    /// Stack of currently-open list metas, pushed on descent and popped on
    /// ascent.
    list_type: Vec<ListMeta>,
    /// `_isLooseParentList` — the helper that corrects the first tight item in
    /// a nested list. Starts `true`, which is what makes the very first block
    /// of a document not get a leading blank line.
    is_loose_parent_list: bool,
    list_indentation: ListIndentation,
    /// How many [`ExportMarkdown::convert`] frames are open — the depth of the
    /// blocks the innermost one is walking, in [`crate::MAX_NESTING_DEPTH`]'s
    /// unit.
    ///
    /// muya has no counterpart: `_convertStatesToMarkdown` recurses freely and
    /// dies of a `RangeError` at whatever depth V8's stack runs out, which S7
    /// measured at 1,757 on Node 24's default stack and at 938 with
    /// `--stack-size=500`. This function is handed a [`Document`] rather than a
    /// parse, so it cannot assume [`crate::block::parse_blocks`] built it, and
    /// §11.3's clause is not satisfiable by a limit that moves with the
    /// interpreter.
    depth: usize,
}

impl ExportMarkdown<'_> {
    fn block(&self, id: NodeId) -> &Block {
        self.doc
            .block(id)
            .expect("only the root has no block, and the root is never serialized")
    }

    fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.doc.children(id).to_vec()
    }

    fn text(&self, id: NodeId) -> String {
        self.block(id)
            .text()
            .map(|t| t.to_str().into_owned())
            .unwrap_or_default()
    }

    /// `_convertStatesToMarkdown`.
    ///
    /// The one addition to the transcription is the depth clamp on
    /// [`ExportMarkdown::depth`]: a document deeper than
    /// [`crate::MAX_NESTING_DEPTH`] serializes down to the limit and stops,
    /// rather than aborting the process. `parse` cannot produce one
    /// (`block::clamp_depth`), so this is reachable only from a hand-built
    /// document — the shape `mt_doc::Edit::InsertNode` allows and
    /// `crates/mt-doc/tests/edit_inverse.rs` generates.
    fn convert(&mut self, states: &[NodeId], indent: &str, list_indent: &str) -> String {
        // The blocks about to be converted sit at `self.depth + 1`, and a
        // container may not sit at `MAX_NESTING_DEPTH` — it would have to put
        // its own content below it. `block::deepest_legal_depth` is the same
        // arithmetic on the other half of the limit.
        if self.depth + 1 >= crate::MAX_NESTING_DEPTH {
            // The leaves, at this indent — `crate::leaves_below`'s docs carry
            // the argument. They cannot recurse back into here: a leaf has no
            // children, and the two arms that would (`block-quote`, the lists)
            // are containers.
            let leaves = crate::leaves_below(self.doc, states);
            return self.convert_inner(&leaves, indent, list_indent);
        }
        self.depth += 1;
        let markdown = self.convert_inner(states, indent, list_indent);
        self.depth -= 1;
        markdown
    }

    fn convert_inner(&mut self, states: &[NodeId], indent: &str, list_indent: &str) -> String {
        let mut result: Vec<String> = Vec::new();
        // "helper for CommonMark 264" — the example where a change of bullet
        // character starts a new list rather than continuing the old one.
        let mut last_list_bullet: Option<char> = None;
        let mut previous_state: Option<NodeId> = None;

        for state in states {
            let block = self.block(*state);
            if !is_any_list(block) {
                last_list_bullet = None;
            }

            if is_any_list(block) {
                let marker_override = (!self.is_loose_parent_list
                    && previous_state.is_some_and(|p| self.is_non_blank_paragraph(p))
                    && self.starts_with_empty_dash_bullet_item(*state))
                .then_some(SETEXT_SAFE_BULLET_MARKER);
                last_list_bullet = Some(self.serialize_list_block(
                    *state,
                    &mut result,
                    indent,
                    list_indent,
                    last_list_bullet,
                    marker_override,
                ));
            } else if matches!(block, Block::ListItem { .. } | Block::TaskListItem { .. }) {
                self.serialize_list_item_block(*state, &mut result, indent, list_indent);
            } else {
                self.serialize_simple_block(*state, &mut result, indent);
            }

            previous_state = Some(*state);
        }

        result.concat()
    }

    /// `previousState?.name === 'paragraph' && previousState.text.trim() !== ''`.
    fn is_non_blank_paragraph(&self, id: NodeId) -> bool {
        matches!(self.block(id), Block::Paragraph { text } if !text.to_str().trim().is_empty())
    }

    /// `_serializeSimpleBlock`.
    ///
    /// muya's `default:` arm is a `debug.warn` and no output. It is reachable
    /// here for `list-item` / `task-list-item` (which `convert` routes away
    /// before this is called), `table.row` and `table.cell` — the last two only
    /// from a hand-built document, since a parse never emits either outside a
    /// `table`.
    fn serialize_simple_block(&mut self, id: NodeId, result: &mut Vec<String>, indent: &str) {
        let block = self.block(id).clone();
        match &block {
            Block::Frontmatter { lang, style, .. } => {
                let text = self.text(id);
                result.push(serialize_front_matter(*lang, *style, &text));
            }
            // `case 'paragraph':` falls through to `case 'thematic-break':` in
            // the TypeScript — one arm, two names.
            Block::Paragraph { .. } | Block::ThematicBreak { .. } => {
                insert_line_break(result, indent);
                result.push(serialize_text_paragraph(&self.text(id), indent));
            }
            Block::AtxHeading { .. } => {
                insert_line_break(result, indent);
                result.push(serialize_atx_heading(&self.text(id), indent));
            }
            Block::SetextHeading { underline, .. } => {
                insert_line_break(result, indent);
                result.push(serialize_setext_heading(
                    &self.text(id),
                    &underline.to_source(),
                    indent,
                ));
            }
            Block::CodeBlock {
                kind,
                info,
                fence_len,
                ..
            } => {
                insert_line_break(result, indent);
                result.push(serialize_code_block(
                    &self.text(id),
                    *kind,
                    info,
                    *fence_len,
                    indent,
                ));
            }
            Block::HtmlBlock { .. } => {
                insert_line_break(result, indent);
                result.push(prefix_every_line(&self.text(id), indent));
            }
            Block::MathBlock { style, .. } => {
                insert_line_break(result, indent);
                result.push(serialize_fenced_lines(
                    &self.text(id),
                    indent,
                    match style {
                        MathStyle::Default => "$$",
                        MathStyle::Gitlab => "```math",
                    },
                    match style {
                        MathStyle::Default => "$$",
                        MathStyle::Gitlab => "```",
                    },
                ));
            }
            Block::Diagram { kind, .. } => {
                insert_line_break(result, indent);
                let open = format!("```{}", diagram_type(*kind));
                result.push(serialize_fenced_lines(&self.text(id), indent, &open, "```"));
            }
            Block::BlockQuote { .. } => {
                insert_line_break(result, indent);
                let children = self.children(id);
                let new_indent = format!("{indent}> ");
                let inner = self.convert(&children, &new_indent, "");
                result.push(inner);
            }
            Block::Table { .. } => {
                insert_line_break(result, indent);
                result.push(self.serialize_table(id, indent));
            }
            Block::Footnote { identifier, .. } => {
                insert_line_break(result, indent);
                let identifier = identifier.clone();
                result.push(self.serialize_footnote(id, &identifier, indent));
            }
            // `debug.warn('Unknown state type:', state.name)`, and no output.
            Block::TableRow { .. }
            | Block::TableCell { .. }
            | Block::ListItem { .. }
            | Block::TaskListItem { .. }
            | Block::BulletList { .. }
            | Block::OrderList { .. }
            | Block::TaskList { .. } => {}
        }
    }

    /// `_serializeListBlock`. Returns the marker or delimiter this list used,
    /// which the caller remembers as `lastListBullet`.
    fn serialize_list_block(
        &mut self,
        id: NodeId,
        result: &mut Vec<String>,
        indent: &str,
        list_indent: &str,
        last_list_bullet: Option<char>,
        marker_override: Option<BulletMarker>,
    ) -> char {
        let mut insert_new_line = self.is_loose_parent_list;
        self.is_loose_parent_list = true;

        let mut meta = list_meta(self.block(id)).expect("caller checked this is a list");
        if let (Some(override_marker), ListMeta::Unordered { marker, .. }) =
            (marker_override, &mut meta)
        {
            *marker = override_marker;
        }
        let bullet_marker_or_delimiter = meta.bullet_marker_or_delimiter();

        // "Start a new list without separation due changing the bullet or
        // ordered list delimiter starts a new list."
        if last_list_bullet.is_some_and(|last| last != bullet_marker_or_delimiter) {
            insert_new_line = false;
        }

        if insert_new_line {
            insert_line_break(result, indent);
        }

        self.list_type.push(meta);
        let children = self.children(id);
        let inner = self.convert(&children, indent, list_indent);
        result.push(inner);
        self.list_type.pop();

        bullet_marker_or_delimiter
    }

    /// `_startsWithEmptyDashBulletItem`.
    ///
    /// Only a **bullet** list, only a `-` marker, and only when the first item
    /// is either childless or leads with a blank paragraph. Every one of those
    /// three is load-bearing: it is the shape whose serialization would reparse
    /// as a setext underline.
    fn starts_with_empty_dash_bullet_item(&self, id: NodeId) -> bool {
        let Block::BulletList {
            marker: BulletMarker::Dash,
            ..
        } = self.block(id)
        else {
            return false;
        };
        let Some(first_item) = self.doc.children(id).first().copied() else {
            return false;
        };
        let Some(first_child) = self.doc.children(first_item).first().copied() else {
            return true;
        };
        matches!(self.block(first_child), Block::Paragraph { text } if text.to_str().trim().is_empty())
    }

    /// `_serializeListItemBlock`.
    fn serialize_list_item_block(
        &mut self,
        id: NodeId,
        result: &mut Vec<String>,
        indent: &str,
        list_indent: &str,
    ) {
        // muya destructures `{ loose }` off the top of the stack, which throws
        // for an orphan list item. Emitting nothing is the port's answer — see
        // the module docs.
        let Some(loose) = self.list_type.last().map(|m| m.loose()) else {
            return;
        };

        self.is_loose_parent_list = loose;
        if loose {
            insert_line_break(result, indent);
        }

        let item_indent = format!("{indent}{list_indent}");
        let item = self.serialize_list_item(id, &item_indent);
        result.push(item);
        self.is_loose_parent_list = true;
    }

    /// `_serializeListItem`.
    fn serialize_list_item(&mut self, id: NodeId, indent: &str) -> String {
        let Some(list_info) = self.list_type.last_mut() else {
            return String::new();
        };

        let mut item_marker = match list_info {
            ListMeta::Unordered { marker, .. } => format!("{} ", marker_char(*marker)),
            ListMeta::Ordered {
                start, delimiter, ..
            } => {
                // "GitHub and Bitbucket limit the list count to 99 but this is
                // nowhere defined. We limit the number to 99 for Daring
                // Fireball Markdown to prevent indentation issues."
                let mut n = *start;
                if (matches!(self.list_indentation, ListIndentation::Dfm) && n > 99)
                    || n > 999_999_999
                {
                    n = 1;
                }
                // The *stored* start advances, not the possibly-reset `n`, so
                // a list that trips the cap emits the same number twice rather
                // than restarting.
                *start = start.saturating_add(1);
                format!("{n}{} ", delimiter_char(*delimiter))
            }
        };

        // Subsequent-paragraph indentation, computed **before** the task
        // marker is appended below. That is why `- [ ] foo` continues on a
        // two-space indent rather than a six-space one.
        let new_indent = format!("{indent}{}", " ".repeat(item_marker.len()));

        // Extra indentation for a NESTED list, on top of the parent item's
        // content column. A numeric "N spaces" is an indentation *level*
        // relative to that column, not an absolute column count: for a `- `
        // marker, N=1 gives 2 columns and N=4 gives 5. Only `dfm` pins a hard
        // four-column nest regardless of marker width.
        let nested_list_indent = match self.list_indentation {
            ListIndentation::Dfm => " ".repeat(4usize.saturating_sub(item_marker.len())),
            ListIndentation::Spaces(n) => " ".repeat(usize::from(n).saturating_sub(1)),
        };

        if let Block::TaskListItem { checked, .. } = self.block(id) {
            item_marker.push_str(if *checked { "[x] " } else { "[ ] " });
        }

        let children = self.children(id);
        if children.is_empty() {
            return format!("{indent}{item_marker}\n");
        }

        let inner = self.convert(&children, &new_indent, &nested_list_indent);
        // `.substring(newIndent.length)`: the first line's indent is replaced
        // by the marker. JavaScript's `substring` clamps rather than panicking,
        // and so does this.
        let body = inner.get(new_indent.len()..).unwrap_or("");
        format!("{indent}{item_marker}{body}")
    }

    /// `_serializeTable`.
    fn serialize_table(&mut self, id: NodeId, indent: &str) -> String {
        let rows = self.children(id);
        // muya reads `state.children[0].children` unguarded. See the module
        // docs for why a table with no rows emits nothing here.
        let Some(header) = rows.first().copied() else {
            return String::new();
        };

        let table_data: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                self.children(*row)
                    .into_iter()
                    .map(|cell| escape_text(self.text(cell).trim()))
                    .collect()
            })
            .collect();

        // `{ width: 5, align }` per **header** cell: the header row decides how
        // many columns there are and what each one's alignment is, and a body
        // row's own `align` is never read.
        let mut column_width: Vec<(usize, Align)> = self
            .children(header)
            .into_iter()
            .map(|cell| {
                (
                    5,
                    match self.block(cell) {
                        Block::TableCell { align, .. } => *align,
                        _ => Align::None,
                    },
                )
            })
            .collect();

        for row in &table_data {
            // `Math.min(tableData[i].length, columnWidth.length)` — a row with
            // more cells than the header cannot widen a column that does not
            // exist.
            for (j, cell) in row.iter().take(column_width.len()).enumerate() {
                // "add 2, because have two space around text"
                column_width[j].0 = column_width[j].0.max(string_width(cell) + 2);
            }
        }

        let mut result: Vec<String> = Vec::new();
        for (i, row) in table_data.iter().enumerate() {
            let cells: Vec<String> = row
                .iter()
                .take(column_width.len())
                .enumerate()
                .map(|(j, cell)| {
                    // Pad by visual column width, not code-unit length, so
                    // combining marks and wide characters stay aligned
                    // (#1983). One leading space + cell + fill.
                    let fill = column_width[j]
                        .0
                        .saturating_sub(1)
                        .saturating_sub(string_width(cell));
                    format!(" {cell}{}", " ".repeat(fill))
                })
                .collect();
            result.push(format!("{indent}|{}|", cells.join("|")));

            if i == 0 {
                let cut_off: Vec<String> = column_width
                    .iter()
                    .map(|(width, align)| {
                        let raw = "-".repeat(width.saturating_sub(2));
                        match align {
                            Align::Left => format!(":{raw} "),
                            Align::Center => format!(":{raw}:"),
                            Align::Right => format!(" {raw}:"),
                            Align::None => format!(" {raw} "),
                        }
                    })
                    .collect();
                result.push(format!("{indent}|{}|", cut_off.join("|")));
            }
        }

        format!("{}\n", result.join("\n"))
    }

    /// `_serializeFootnote`.
    ///
    /// ```text
    /// [^id]: first paragraph
    ///
    ///     continuation block indented by four spaces
    /// ```
    ///
    /// The `[^id]: ` prefix sits on the first child's first line and everything
    /// after it stays at the four-space indent. muya strips the inner indent
    /// with `inner.replace(innerIndent, '')` — a **string** pattern, so
    /// JavaScript replaces the first occurrence only, wherever it is.
    /// `replacen(.., 1)` is that, and it is not `strip_prefix`: an inner block
    /// that does not start at column 0 would have its *later* four spaces eaten
    /// instead, and reproducing that is the point of transcribing rather than
    /// improving.
    fn serialize_footnote(&mut self, id: NodeId, identifier: &str, indent: &str) -> String {
        let children = self.children(id);
        let inner_indent = format!("{indent}    ");
        let inner = self.convert(&children, &inner_indent, "");
        let stripped = inner.replacen(&inner_indent, "", 1);
        format!("{indent}[^{identifier}]: {stripped}")
    }
}

fn is_any_list(block: &Block) -> bool {
    matches!(
        block,
        Block::BulletList { .. } | Block::OrderList { .. } | Block::TaskList { .. }
    )
}

/// `deepClone(state.meta)` for the three list kinds, and `None` for everything
/// else.
fn list_meta(block: &Block) -> Option<ListMeta> {
    match block {
        Block::BulletList { marker, loose, .. } | Block::TaskList { marker, loose, .. } => {
            Some(ListMeta::Unordered {
                marker: *marker,
                loose: *loose,
            })
        }
        Block::OrderList {
            start,
            delimiter,
            loose,
            ..
        } => Some(ListMeta::Ordered {
            start: *start,
            delimiter: *delimiter,
            loose: *loose,
        }),
        _ => None,
    }
}

/// `_insertLineBreak`.
///
/// > Blank lines inside a list item should be empty, not carry the item's
/// > indent as trailing whitespace. For blockquote-style indents like `> ` we
/// > keep the `>` so the quote stays continuous — only strip the trailing run
/// > of plain spaces.
///
/// `indent.replace(/ +$/, '')`, and nothing when this is the first block at
/// this level.
fn insert_line_break(result: &mut Vec<String>, indent: &str) {
    if result.is_empty() {
        return;
    }
    result.push(format!("{}\n", indent.trim_end_matches(' ')));
}

/// `_serializeTextParagraph`, and `_serializeHtmlBlock`, which differ only in
/// how they spell the same loop.
fn serialize_text_paragraph(text: &str, indent: &str) -> String {
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| format!("{indent}{line}"))
        .collect();
    format!("{}\n", lines.join("\n"))
}

/// `_serializeHtmlBlock`: `for (const line of lines) result.push(indent + line + '\n')`.
///
/// Identical output to [`serialize_text_paragraph`] for every input — the two
/// are kept apart because the TypeScript keeps them apart, and because a future
/// change to one of them should not silently be a change to both.
fn prefix_every_line(text: &str, indent: &str) -> String {
    text.split('\n')
        .map(|line| format!("{indent}{line}\n"))
        .collect()
}

/// `_serializeAtxHeading`: `text.match(/(#{1,6})(.*)/)`, then
/// `` `${match?.[1]} ${match?.[2].trim()}` ``.
///
/// Three things about that regex that a rewrite would get wrong, and all three
/// are reachable from a hand-built `Document`:
///
/// - It is **unanchored**, so a heading whose text does not start with `#` is
///   rebuilt from the first `#` anywhere in it — including one on a later line.
/// - `.` does not match a newline, so only the rest of *that* line survives.
/// - When there is no `#` at all the template interpolates two `undefined`s and
///   the emitted line is the literal string `undefined undefined`. That is
///   absurd and it is what muya does; it is reproduced rather than repaired,
///   because the alternative is a port that disagrees with the reference engine
///   on a document the reference engine can be handed.
///
/// D2 guarantees that a parsed heading's text is `'#' × level + ' ' + content`,
/// so none of the three is reachable from `parse`.
fn serialize_atx_heading(text: &str, indent: &str) -> String {
    let heading = match text.find('#') {
        Some(start) => {
            let hashes = text[start..]
                .chars()
                .take_while(|c| *c == '#')
                .count()
                .min(6);
            let after = &text[start + hashes..];
            let rest = &after[..after.find('\n').unwrap_or(after.len())];
            format!("{} {}", "#".repeat(hashes), rest.trim())
        }
        None => "undefined undefined".to_string(),
    };
    format!("{indent}{heading}\n")
}

/// `_serializeSetextHeading`. The text is trimmed as a whole, each line is
/// indented, and the underline is emitted trimmed on its own line.
fn serialize_setext_heading(text: &str, underline: &str, indent: &str) -> String {
    let lines: Vec<String> = text
        .trim()
        .split('\n')
        .map(|line| format!("{indent}{line}"))
        .collect();
    format!("{}\n{indent}{}\n", lines.join("\n"), underline.trim())
}

/// `_serializeCodeBlock`.
///
/// `meta.lang` holds the full info string verbatim, so it is emitted as-is —
/// constraint 1 on [`mt_doc::Block::CodeBlock::info`], and #4770's data loss in
/// the direction that caused it. An **empty** info string is falsy in
/// JavaScript, which is why the two arms exist rather than one that appends a
/// possibly-empty string.
///
/// The fence is always backticks, whatever the source used: a `~~~` fence
/// re-serializes as ```` ``` ````, which `gitlabMath.spec.ts`'s tilde case
/// asserts on purpose.
fn serialize_code_block(
    text: &str,
    kind: CodeKind,
    info: &str,
    fence_len: Option<u32>,
    indent: &str,
) -> String {
    let mut result = String::new();
    if kind == CodeKind::Fenced {
        let fence = "`".repeat(code_fence_length(text, fence_len));
        if info.is_empty() {
            result.push_str(&format!("{indent}{fence}\n"));
        } else {
            result.push_str(&format!("{indent}{fence}{info}\n"));
        }
        for line in text.split('\n') {
            result.push_str(&format!("{indent}{line}\n"));
        }
        result.push_str(&format!("{indent}{fence}\n"));
    } else {
        for line in text.split('\n') {
            result.push_str(&format!("{indent}    {line}\n"));
        }
    }
    result
}

/// `_codeFenceLength`.
///
/// > The opening fence must be longer than any all-backtick line in the body
/// > (else that line closes the block early), at least as long as the original
/// > fence, and never shorter than the markdown minimum of 3.
///
/// `Math.max(3, stored ?? 3, longestInterior + 1)`. The `stored` term is
/// `meta.fenceLength`, which is `None` for a three-backtick fence — so an
/// ordinary block does not grow one, and a four-backtick one does not shrink.
fn code_fence_length(text: &str, stored: Option<u32>) -> usize {
    let mut longest_interior = 0;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.bytes().all(|b| b == b'`') {
            longest_interior = longest_interior.max(trimmed.len());
        }
    }
    let stored = stored.map_or(3, |n| n as usize);
    3.max(stored).max(longest_interior + 1)
}

/// `_serializeMathBlock` and `_serializeDiagramBlock`, which are the same
/// function with different delimiters.
fn serialize_fenced_lines(text: &str, indent: &str, open: &str, close: &str) -> String {
    let mut result = format!("{indent}{open}\n");
    for line in text.split('\n') {
        result.push_str(&format!("{indent}{line}\n"));
    }
    result.push_str(&format!("{indent}{close}\n"));
    result
}

/// `_serializeFrontMatter`.
///
/// The switch is on **`meta.lang`**, not on `meta.style`, and only the `json`
/// arm consults the style at all. So a `yaml` front matter whose style says
/// `+` still emits `---`, which is muya's and is the reason this is a
/// transcription rather than a lookup table over the pair.
fn serialize_front_matter(lang: FrontmatterLang, style: FrontmatterStyle, text: &str) -> String {
    let (start, end) = match lang {
        FrontmatterLang::Yaml => ("---", "---"),
        FrontmatterLang::Toml => ("+++", "+++"),
        FrontmatterLang::Json => {
            if style == FrontmatterStyle::Semicolon {
                (";;;", ";;;")
            } else {
                ("{", "}")
            }
        }
    };
    let mut result = format!("{start}\n");
    for line in text.split('\n') {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(end);
    result.push('\n');
    result
}

/// See [`DiagramKind::info_lang`]: the table moved to `mt-doc` at M3 S1 so
/// that `mt-layout` reuses it instead of adding a fourth copy (M3.md §5 D11).
fn diagram_type(kind: DiagramKind) -> &'static str {
    kind.info_lang()
}

/// `function escapeText(str) { return str.replace(/(?<!\\)\|/g, '\\|') }`.
///
/// **The lookbehind is the whole rule.** marktext #3563 shipped
/// `/([^\\])\|/g`, which needs a non-backslash character *before* the pipe — so
/// a pipe at the very start of a cell was never escaped, and reopening the file
/// read it as a column separator and ate the rest of the cell. The negative
/// lookbehind has no such requirement, and it is also why two consecutive pipes
/// are both escaped: JavaScript's `replace` tests the lookbehind against the
/// **original** string, where the second `|`'s predecessor is a `|` and not a
/// backslash.
///
/// This is the other half of S2's `table.cell` rule. muya resolves the table
/// `\|` escape into a literal `|` on the way in, deliberately and with the
/// issue number on it (#4849, so that `` `\|` `` displays as `<code>|</code>`),
/// and this is what puts it back. The two have to compose or the round trip is
/// lossy in a direction `cargo xtask blocks` cannot see, because that harness
/// compares state and both engines' state agrees.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut previous = None;
    for ch in s.chars() {
        if ch == '|' && previous != Some('\\') {
            out.push('\\');
        }
        out.push(ch);
        previous = Some(ch);
    }
    out
}

#[cfg(test)]
mod tests;
