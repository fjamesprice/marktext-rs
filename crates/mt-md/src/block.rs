//! Markdown → the block tree. `pulldown-cmark`'s event stream mapped onto
//! [`mt_doc::Block`], with everything `marked` does that `pulldown-cmark` does
//! not.
//!
//! M2.md §5 D1: *"adopt `pulldown-cmark`, and write the mapping layer as a
//! named deliverable rather than as glue."* This is that deliverable.
//!
//! # What this module is not
//!
//! It is **not** a markdown parser. `pulldown-cmark` decides where every block
//! begins and ends, and §4 C1 measured that the decision is right: over 1344
//! inputs, not one disagreement with `MarkdownToState` was the parser putting
//! a block boundary somewhere `marked` does not. What is here is the distance
//! between "a correct CommonMark event stream" and "muya's `TState[]`", which
//! is larger than §4's *"`pulldown-cmark` for block structure only"* suggests.
//!
//! It also does **not** read inline events, ever. A leaf's text is unparsed
//! inline source (§4 C2) and `mt-inline` is what tokenizes it; feeding
//! `pulldown-cmark`'s inline events into the document would destroy the marker
//! spans the WYSIWYG reveal depends on. Inline events are read here for
//! exactly two structural purposes — deciding where a tight list item's
//! synthetic paragraph starts and ends, and reading `TaskListMarker` — and
//! their *content* is never looked at.
//!
//! # The mechanisms, and where each one lives
//!
//! §4 C1 named four and the measurement found more. Each has a section below
//! and a test naming the input that produced it.
//!
//! | # | Mechanism | Function |
//! |---:|---|---|
//! | 1 | Reference definitions are consumed and emit no event | `scan_definitions` |
//! | 2 | Empty containers get a synthetic empty paragraph (muya #1735) | `Builder::close_frame` |
//! | 3 | A bullet list splits where task-ness changes (`compatibleTaskList`) | `split_list` |
//! | 4 | Block math — `$$…$$` and ```` ```math ```` | `Builder::finish_paragraph`, `code_block` |
//! | 5 | A tight list item carries its inlines with no `Paragraph` wrapper | `Builder::open_synthetic` |
//! | 6 | Front matter is not markdown and is split off first | `front_matter` |
//! | 7 | Every `meta` field `pulldown-cmark` does not carry | this whole file |
//!
//! Mechanism 5 is the one §4 C1 records as having cost a measurement: a
//! mapping that only creates nodes for block tags gives every tight item no
//! children at all, which is 90 false disagreements, and the synthetic
//! paragraph that fixes it must be closed by a **block** tag and never by
//! `Emphasis`, `Link` or `Strikethrough` — closing on any `Start` splits every
//! item containing emphasis into several paragraphs, which is 5 more.
//! `is_block_tag` is where that distinction is written down.
//!
//! # Leaf text is S2's
//!
//! Every leaf built here carries the **empty string**. §4 C2 measured that a
//! leaf's text is not a source slice but a per-kind reconstruction, and D2's
//! eight rules plus the container-prefix stripper are S2's whole content.
//! `cargo xtask blocks` therefore compares names and `meta` at S1 and says so
//! rather than quietly excluding the field.
//!
//! Two decisions here nonetheless *read* the source — whether an HTML block is
//! a lone `<img>`, and whether a paragraph is really a `$$` math block. Both
//! are decisions about which **block** to build rather than about what text it
//! holds, and both are muya's own: they are taken on the source range and
//! nothing is stored.

use std::collections::HashSet;
use std::ops::Range;

use mt_doc::{
    Align, Block, BulletMarker, CodeKind, DiagramKind, DiagramLang, Document, Edit,
    FrontmatterLang, FrontmatterStyle, MathStyle, NodeId, OrderDelim, Text, Underline,
};
use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, Options as CmarkOptions, Parser, Tag, TagEnd,
};

use crate::Options;

/// Markdown → [`Document`], block structure and `meta` only.
///
/// **Not [`crate::parse`].** That entry point stays `Err(Unimplemented)` until
/// S3, because D6 stages the three ratchets by entry point and `parse`
/// returning `Ok` is what wakes the first of them. This is the function
/// `parse` will *call* at S3 — S3's work is the label-map pass and leaf text
/// on top of this, not a rewrite of it — and it is what `cargo xtask blocks`
/// drives today.
///
/// Every leaf's text is the empty string; see this module's header.
pub fn parse_blocks(markdown: &str, options: Options) -> Document {
    to_document(build(markdown, options))
}

/// A block and its children, before the arena has ids to put in them.
///
/// The container variants of [`Block`] hold `Vec<NodeId>`, and an id exists
/// only once the node is in a [`Document`] — so the mapping pass builds this
/// and [`to_document`] turns it into nodes.
#[derive(Debug, Clone, PartialEq)]
struct Built {
    block: Block,
    children: Vec<Built>,
    /// The source range this block came from. Unused at S1 beyond ordering;
    /// S2 reconstructs leaf text from it and §10 owes M4 a mapping back.
    range: Range<usize>,
}

impl Built {
    fn leaf(block: Block, range: Range<usize>) -> Self {
        Built {
            block,
            children: Vec::new(),
            range,
        }
    }
}

/// Insert a built tree into a fresh [`Document`].
///
/// # Through `Document::apply`, and why that is not a detour
///
/// `apply` is the only public way to change a document, deliberately: M2.md
/// §5 D9's confirmation is the constraint *"no edit path may bypass
/// `Document::apply`"*, which is what keeps the CRDT option open and what
/// makes the undo stack a complete record. A parser is the one caller that
/// could argue for an exception — it is building, not editing — and taking the
/// exception would mean `mt-doc` growing a second write path that the
/// invariant does not cover.
///
/// So the new node's id is read out of the inverse batch: `InsertNode`'s
/// inverse is `RemoveNode { node }` carrying exactly the id that was minted.
/// That is obscure enough to be worth this paragraph and cheap enough not to
/// be worth an API.
///
/// A fresh parse legitimately leaves every node in the dirty set — nothing has
/// been laid out yet — so the cost of going through `apply` is a revision
/// counter that counts blocks rather than edits, which no consumer reads as
/// anything but "changed".
fn to_document(top: Vec<Built>) -> Document {
    let mut doc = Document::new();
    // `Document::new` is a root holding one empty paragraph, which is exactly
    // `markdownToState`'s own `states.length ? states : [{ name: 'paragraph',
    // text: '' }]` fallback. So an empty parse needs no special case; a
    // non-empty one drops the placeholder.
    if top.is_empty() {
        return doc;
    }

    let root = doc.root();
    let placeholder = doc.children(root)[0];
    for (index, built) in top.into_iter().enumerate() {
        insert(&mut doc, root, index, built);
    }
    doc.apply(&[Edit::RemoveNode { node: placeholder }]);
    // `RemoveNode` detaches rather than frees, and nothing here holds an
    // inverse that could name the placeholder again.
    doc.prune_detached();
    doc
}

fn insert(doc: &mut Document, parent: NodeId, index: usize, built: Built) {
    let inverse = doc.apply(&[Edit::InsertNode {
        parent,
        index,
        block: built.block,
    }]);
    let id = match inverse.as_slice() {
        [Edit::RemoveNode { node }] => *node,
        other => unreachable!("InsertNode's inverse is RemoveNode; got {other:?}"),
    };
    for (child_index, child) in built.children.into_iter().enumerate() {
        insert(doc, id, child_index, child);
    }
}

// ---------------------------------------------------------------------------
// The parser options
// ---------------------------------------------------------------------------

/// The `pulldown-cmark` feature set §4 C1 measured, and nothing else.
///
/// - **Tables** and **strikethrough** because muya runs `marked` with
///   `gfm: true`, which is its default.
/// - **Task lists** because `Block` has `TaskList` and `TaskListItem`
///   variants; `compatibleTaskList` is what turns the marker into them.
///
/// Deliberately absent:
///
/// - `ENABLE_FOOTNOTES` — muya's footnotes are its own block extension with a
///   4-space de-indent and a recursive lex (`utils/marked/extensions/footnote.ts`),
///   not GFM's, and both option sets in S1's input set have `footnote: false`.
///   See [`Options::footnote`](crate::Options::footnote) and M2.md's owed list.
/// - `ENABLE_MATH` — `pulldown-cmark`'s is an *inline* `$`/`$$` extension with
///   different delimiters from muya's block rule. Block math is mechanism 4
///   here; inline math is `mt-inline`'s and was M1's.
/// - `ENABLE_SMART_PUNCTUATION`, `ENABLE_HEADING_ATTRIBUTES` and the rest —
///   they rewrite content, and this port never lets the block parser touch
///   inline text.
fn cmark_options() -> CmarkOptions {
    CmarkOptions::ENABLE_TABLES
        | CmarkOptions::ENABLE_STRIKETHROUGH
        | CmarkOptions::ENABLE_TASKLISTS
}

// ---------------------------------------------------------------------------
// Mechanism 6 — front matter
// ---------------------------------------------------------------------------

/// muya's `utils/marked/frontMatter.ts`, transcribed.
///
/// ```text
/// /^(?:---\n([\s\S]+?)---|\+\+\+\n([\s\S]+?)\+\+\+|;;;\n([\s\S]+?);;;|\{\n([\s\S]+?)\})(?:\n{2,}|\n{1,2}$)/
/// ```
///
/// Two things about it that a "sensible" reimplementation would get wrong, and
/// which are reproduced rather than improved:
///
/// - **The closing delimiter is not anchored to a line start.** `---\nfoo---\n\n`
///   is front matter whose text is `foo`, because `[\s\S]+?` is lazy and the
///   regex takes the first `---` it can.
/// - **The trailing rule is `\n{2,}` or one-or-two newlines at end of input**,
///   so front matter followed by a single newline and then a paragraph is not
///   front matter at all.
///
/// Returns the token's `(text_range, lang, style)` and the byte offset the
/// rest of the document starts at.
fn front_matter(src: &str) -> Option<(Range<usize>, FrontmatterLang, FrontmatterStyle, usize)> {
    const DELIMITERS: [(&str, &str, FrontmatterLang, FrontmatterStyle); 4] = [
        (
            "---\n",
            "---",
            FrontmatterLang::Yaml,
            FrontmatterStyle::Dash,
        ),
        (
            "+++\n",
            "+++",
            FrontmatterLang::Toml,
            FrontmatterStyle::Plus,
        ),
        (
            ";;;\n",
            ";;;",
            FrontmatterLang::Json,
            FrontmatterStyle::Semicolon,
        ),
        ("{\n", "}", FrontmatterLang::Json, FrontmatterStyle::Brace),
    ];

    for (open, close, lang, style) in DELIMITERS {
        if !src.starts_with(open) {
            continue;
        }
        // `[\s\S]+?` — at least one character, then the earliest close.
        let body_start = open.len();
        let mut at = body_start + 1;
        while at <= src.len() {
            let Some(found) = src[at..].find(close).map(|i| at + i) else {
                break;
            };
            let after = found + close.len();
            if let Some(end) = trailing_blank_run(src, after) {
                return Some((body_start..found, lang, style, end));
            }
            // The regex would backtrack `[\s\S]+?` one character and look
            // again, which is the same as looking for the next `close`.
            at = found + 1;
        }
    }
    None
}

/// `(?:\n{2,}|\n{1,2}$)` — the offset just past the match, or `None`.
fn trailing_blank_run(src: &str, at: usize) -> Option<usize> {
    let newlines = src[at..].bytes().take_while(|b| *b == b'\n').count();
    let end = at + newlines;
    if newlines >= 2 || (newlines >= 1 && end == src.len()) {
        Some(end)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// What the stack holds.
#[derive(Debug)]
enum FrameKind {
    Root,
    BlockQuote,
    List {
        /// `Some(n)` for an ordered list, `None` for a bullet one.
        start: Option<u64>,
        /// Set when any item's content was wrapped in a real `Paragraph`,
        /// which is `pulldown-cmark`'s only direct signal that the list is
        /// loose.
        any_real_paragraph: bool,
        /// Per item, in order: `(a blank line between its own blocks, a blank
        /// line at its very end)`. The second is why this is collected here
        /// rather than decided per item — see [`list_is_loose`].
        item_blanks: Vec<(bool, bool)>,
    },
    Item {
        /// `Some(checked)` once a `TaskListMarker` has been seen.
        task: Option<bool>,
        /// Whether `pulldown-cmark` wrapped any of this item's content in a
        /// real `Paragraph`, which is its way of saying the list is loose.
        real_paragraph: bool,
        /// The open synthetic paragraph's range, if one is open.
        synthetic: Option<Range<usize>>,
    },
    Table {
        alignments: Vec<Alignment>,
    },
    TableRow,
    TableCell {
        align: Align,
    },
    Paragraph,
    Heading {
        level: HeadingLevel,
    },
    CodeBlock {
        kind: CodeBlockKind<'static>,
    },
    HtmlBlock,
    /// A block tag this port does not model — a footnote definition or a
    /// definition list, neither of which is enabled. Its children are dropped
    /// rather than reparented, so an accidental enabling is visible as missing
    /// blocks rather than as blocks in the wrong place.
    Unmodelled,
}

#[derive(Debug)]
struct Frame {
    kind: FrameKind,
    range: Range<usize>,
    children: Vec<Built>,
    /// The offset the next gap scan starts from — everything before it has
    /// either become a child or been scanned for reference definitions.
    scanned_to: usize,
}

struct Builder<'a> {
    src: &'a str,
    options: Options,
    stack: Vec<Frame>,
    /// `marked`'s `Lexer.tokens.links`, which is document-global: a reference
    /// definition whose label has already been defined **anywhere earlier**
    /// emits no block at all. See `scan_definitions`.
    seen_labels: HashSet<String>,
}

/// Markdown → the top-level built blocks.
fn build(markdown: &str, options: Options) -> Vec<Built> {
    let mut top = Vec::new();
    let mut body_at = 0;

    if options.front_matter
        && let Some((text_range, lang, style, rest_at)) = front_matter(markdown)
    {
        top.push(Built::leaf(
            Block::Frontmatter {
                lang,
                style,
                text: Text::new(),
            },
            text_range,
        ));
        body_at = rest_at;
    }

    let mut builder = Builder {
        src: markdown,
        options,
        stack: vec![Frame {
            kind: FrameKind::Root,
            range: body_at..markdown.len(),
            children: Vec::new(),
            scanned_to: body_at,
        }],
        seen_labels: HashSet::new(),
    };
    builder.walk(body_at);

    // The root's own trailing gap. Every other container's is scanned when its
    // `End` arrives; the root has no `End`, and `[foo]\n\n[foo]: /bar` — a
    // paragraph followed by a definition, which is 108 of the CommonMark and
    // GFM fixtures — is exactly the shape that falls in it.
    let mut root = builder
        .stack
        .pop()
        .expect("the root frame is never popped by the walk");
    let end = root.range.end;
    builder.scan_gap(&mut root, end);
    debug_assert!(builder.stack.is_empty(), "unbalanced frames");
    top.extend(root.children);
    top
}

impl<'a> Builder<'a> {
    /// Walk `pulldown-cmark`'s events over `src[body_at..]`.
    ///
    /// Offsets are shifted back to whole-document offsets by `body_at`, which
    /// is non-zero only when front matter was split off.
    fn walk(&mut self, body_at: usize) {
        let body = &self.src[body_at..];
        let shift = |r: Range<usize>| (r.start + body_at)..(r.end + body_at);

        for (event, range) in Parser::new_ext(body, cmark_options())
            .into_offset_iter()
            .map(|(e, r)| (e, shift(r)))
        {
            match event {
                Event::Start(tag) => {
                    if is_block_tag(&tag) {
                        self.close_synthetic();
                        self.push_frame(&tag, range);
                    } else {
                        // Mechanism 5's second half: an inline `Start` must
                        // not close the synthetic paragraph, and must open one
                        // if this is the item's first inline event.
                        self.open_synthetic(&range);
                    }
                }
                Event::End(tag_end) => {
                    if is_block_tag_end(&tag_end) {
                        self.close_synthetic();
                        self.close_frame(range);
                    } else {
                        self.open_synthetic(&range);
                    }
                }
                Event::Rule => {
                    self.close_synthetic();
                    let block = Block::ThematicBreak { text: Text::new() };
                    self.emit(Built::leaf(block, range));
                }
                Event::TaskListMarker(checked) => {
                    // Not an inline event for the synthetic paragraph's
                    // purposes: muya renders the marker from `checked` and
                    // strips it from the text, so the item's paragraph starts
                    // *after* it.
                    self.set_task(checked);
                }
                _ => self.open_synthetic(&range),
            }
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.stack
            .last_mut()
            .expect("the root frame is always there")
    }

    fn push_frame(&mut self, tag: &Tag<'_>, range: Range<usize>) {
        let scanned_to = range.start;
        let kind = match tag {
            Tag::Paragraph => FrameKind::Paragraph,
            Tag::Heading { level, .. } => FrameKind::Heading { level: *level },
            Tag::BlockQuote(_) => FrameKind::BlockQuote,
            Tag::CodeBlock(kind) => FrameKind::CodeBlock {
                kind: owned_code_kind(kind),
            },
            Tag::HtmlBlock => FrameKind::HtmlBlock,
            Tag::List(start) => FrameKind::List {
                start: *start,
                any_real_paragraph: false,
                item_blanks: Vec::new(),
            },
            Tag::Item => FrameKind::Item {
                task: None,
                real_paragraph: false,
                synthetic: None,
            },
            Tag::Table(alignments) => FrameKind::Table {
                alignments: alignments.clone(),
            },
            // muya emits a `table.row` for the header too — `markdownToState`
            // pushes `{ name: 'table.row', children: header.map(...) }` before
            // the body rows — so `TableHead` and `TableRow` are the same frame.
            Tag::TableHead | Tag::TableRow => FrameKind::TableRow,
            Tag::TableCell => FrameKind::TableCell {
                align: self.next_cell_alignment(),
            },
            _ => FrameKind::Unmodelled,
        };
        self.stack.push(Frame {
            kind,
            range,
            children: Vec::new(),
            scanned_to,
        });
    }

    /// `align[i] || 'none'` — the alignment of the cell about to open, by its
    /// index in the row.
    fn next_cell_alignment(&self) -> Align {
        let index = self.stack.last().map_or(0, |row| row.children.len());
        self.stack
            .iter()
            .rev()
            .find_map(|frame| match &frame.kind {
                FrameKind::Table { alignments } => Some(alignments),
                _ => None,
            })
            .and_then(|alignments| alignments.get(index))
            .map_or(Align::None, |a| match a {
                Alignment::None => Align::None,
                Alignment::Left => Align::Left,
                Alignment::Center => Align::Center,
                Alignment::Right => Align::Right,
            })
    }

    fn close_frame(&mut self, range: Range<usize>) {
        let mut frame = self.stack.pop().expect("every End follows a Start");
        // A frame's `End` range is authoritative: `Start` and `End` carry the
        // same span for every block tag, but taking it here means a future
        // `pulldown-cmark` that narrows one of them cannot silently shift a
        // gap.
        frame.range = range;

        match frame.kind {
            // Containers whose gaps can hold reference definitions. A list, a
            // table and a table row cannot: their only legal children are
            // items, rows and cells.
            FrameKind::BlockQuote | FrameKind::Item { .. } => {
                let end = frame.range.end;
                self.scan_gap(&mut frame, end);
            }
            _ => {}
        }

        let built = match frame.kind {
            FrameKind::Root => unreachable!("the root frame is popped by `build`, not by an End"),
            FrameKind::Unmodelled => return,

            FrameKind::Paragraph => {
                let built = self.finish_paragraph(&frame);
                // A real paragraph is `pulldown-cmark`'s signal that the list
                // is loose — see the looseness note on `FrameKind::List`.
                self.mark_real_paragraph();
                self.emit_all(built);
                return;
            }

            FrameKind::Heading { level } => self.heading(level, frame.range.clone()),

            FrameKind::CodeBlock { ref kind } => {
                code_block(self.src, kind, &frame.range, self.options)
            }

            FrameKind::HtmlBlock => html_block(self.src, &frame.range),

            FrameKind::BlockQuote => Built {
                // Mechanism 2, muya #1735: `>` alone is a block quote holding
                // one empty paragraph, not an empty block quote.
                children: empty_container_filler(frame.children, &frame.range),
                block: Block::BlockQuote {
                    children: Vec::new(),
                },
                range: frame.range,
            },

            FrameKind::Item {
                task,
                real_paragraph,
                ..
            } => {
                let blanks = self.item_blank_lines(&frame);
                self.record_item_looseness(real_paragraph, blanks);
                let block = match task {
                    Some(checked) => Block::TaskListItem {
                        checked,
                        children: Vec::new(),
                    },
                    None => Block::ListItem {
                        children: Vec::new(),
                    },
                };
                Built {
                    block,
                    children: empty_container_filler(frame.children, &frame.range),
                    range: frame.range,
                }
            }

            FrameKind::List {
                start,
                any_real_paragraph,
                ref item_blanks,
            } => {
                let loose = list_is_loose(any_real_paragraph, item_blanks);
                // Mechanism 3: one `pulldown-cmark` list can be several muya
                // lists.
                let built = split_list(self.src, start, loose, frame.children, &frame.range);
                self.emit_all(built);
                return;
            }

            FrameKind::Table { .. } => Built {
                block: Block::Table {
                    children: Vec::new(),
                },
                children: frame.children,
                range: frame.range,
            },

            FrameKind::TableRow => Built {
                block: Block::TableRow {
                    children: Vec::new(),
                },
                children: frame.children,
                range: frame.range,
            },

            FrameKind::TableCell { align } => Built::leaf(
                Block::TableCell {
                    align,
                    text: Text::new(),
                },
                frame.range,
            ),
        };

        self.emit(built);
    }

    /// Mechanism 4 — a paragraph that is really a `$$…$$` math block.
    ///
    /// muya registers block math as a `marked` block-level extension, so it is
    /// tried at the start of every block; `pulldown-cmark` has no such hook and
    /// produces a paragraph. The rule is transcribed in [`block_math`].
    ///
    /// Returns one or two blocks: the math block, and the paragraph holding
    /// whatever followed the closing delimiter inside the same
    /// `pulldown-cmark` paragraph.
    fn finish_paragraph(&self, frame: &Frame) -> Vec<Built> {
        let paragraph =
            |range: Range<usize>| Built::leaf(Block::Paragraph { text: Text::new() }, range);
        if !self.options.math {
            return vec![paragraph(frame.range.clone())];
        }
        let logical = LogicalText::of(self.src, frame.range.clone(), false);
        let Some(consumed) = block_math(&logical.text) else {
            return vec![paragraph(frame.range.clone())];
        };

        let math_end = logical.source_offset(consumed);
        let mut out = vec![Built::leaf(
            Block::MathBlock {
                style: MathStyle::Default,
                text: Text::new(),
            },
            frame.range.start..math_end,
        )];
        if consumed < logical.text.trim_end().len() {
            out.push(paragraph(math_end..frame.range.end));
        }
        out
    }

    fn heading(&self, level: HeadingLevel, range: Range<usize>) -> Built {
        // `walkTokens`'s test, transcribed: `/\n {0,3}(=+|-+)/.exec(token.raw)`.
        // Unanchored and on the whole raw, which is what tells atx from setext
        // — an atx heading's raw is one line, so it cannot match.
        let block = match setext_underline(&self.src[range.clone()]) {
            Some(underline) => Block::SetextHeading {
                level: underline.level(),
                underline,
                text: Text::new(),
            },
            None => Block::AtxHeading {
                level: heading_level(level),
                text: Text::new(),
            },
        };
        Built::leaf(block, range)
    }

    // --- mechanism 5: the tight list item's synthetic paragraph ------------

    /// Open, or extend, the synthetic paragraph a tight list item needs.
    ///
    /// A tight item carries its inline events with **no `Paragraph`
    /// wrapper**, so a mapping that only creates nodes for block tags gives
    /// every tight item no children (§4 C1: 90 false disagreements). muya's
    /// state has a `paragraph` there, because `marked` gives the item a `text`
    /// token and `markdownToState`'s `text` case builds one.
    ///
    /// Only inside an `Item` frame: at every other level `pulldown-cmark`
    /// wraps inline content in a real `Paragraph`.
    fn open_synthetic(&mut self, range: &Range<usize>) {
        if let FrameKind::Item {
            ref mut synthetic, ..
        } = self.top().kind
        {
            match synthetic {
                Some(open) => open.end = open.end.max(range.end),
                None => *synthetic = Some(range.clone()),
            }
        }
    }

    /// Close the synthetic paragraph, if one is open.
    ///
    /// Called on every **block** tag and on nothing else. §4 C1: ending it on
    /// any `Start` splits every item containing emphasis into several
    /// paragraphs.
    fn close_synthetic(&mut self) {
        let frame = self.top();
        let FrameKind::Item {
            ref mut synthetic, ..
        } = frame.kind
        else {
            return;
        };
        let Some(range) = synthetic.take() else {
            return;
        };
        let built = Built::leaf(Block::Paragraph { text: Text::new() }, range.clone());
        // Emitted through the same path as a real child so that the gap before
        // it is scanned for reference definitions.
        self.emit(built);
    }

    fn set_task(&mut self, checked: bool) {
        for frame in self.stack.iter_mut().rev() {
            if let FrameKind::Item { ref mut task, .. } = frame.kind {
                *task = Some(checked);
                return;
            }
        }
    }

    fn mark_real_paragraph(&mut self) {
        if let FrameKind::Item {
            ref mut real_paragraph,
            ..
        } = self.top().kind
        {
            *real_paragraph = true;
        }
    }

    fn record_item_looseness(&mut self, real_paragraph: bool, blanks: (bool, bool)) {
        if let FrameKind::List {
            ref mut any_real_paragraph,
            ref mut item_blanks,
            ..
        } = self.top().kind
        {
            *any_real_paragraph |= real_paragraph;
            item_blanks.push(blanks);
        }
    }

    /// Where an item's blank lines are: `(between its own blocks, at its very
    /// end)`.
    ///
    /// `marked` sets `list.loose` when an item's token stream contains a
    /// `space` token matching `/\n.*\n/`. `pulldown-cmark` reports looseness
    /// only by wrapping content in `Paragraph`, which is enough for every list
    /// whose items contain a paragraph and **not** enough for one whose items
    /// hold only code blocks — measured against the running engine:
    ///
    /// ```text
    /// "- ```\n  x\n  ```\n\n- ```\n  y\n  ```\n" → loose: true, and no paragraph anywhere
    /// ```
    ///
    /// So the blank line is looked for directly, in the parts of the item's
    /// range that did not become children. A blank line *inside* a child — in
    /// a fenced code block, say — is not counted, which is the same
    /// distinction `marked` draws by only looking at `space` tokens.
    ///
    /// The two positions are reported separately because `marked` treats them
    /// differently; see [`list_is_loose`].
    fn item_blank_lines(&self, frame: &Frame) -> (bool, bool) {
        let mut at = frame.range.start;
        let mut internal = false;
        for child in &frame.children {
            if child.range.start > at && has_blank_line(&self.src[at..child.range.start]) {
                internal = true;
            }
            at = at.max(child.range.end);
        }
        let trailing = at < frame.range.end && has_blank_line(&self.src[at..frame.range.end]);
        (internal, trailing)
    }

    // --- mechanism 1: reference definitions ---------------------------------

    /// Emit a child, scanning the source it skipped over first.
    fn emit(&mut self, built: Built) {
        self.emit_all(vec![built]);
    }

    fn emit_all(&mut self, built: Vec<Built>) {
        let Some(first) = built.first() else {
            return;
        };
        let (start, end) = (first.range.start, built[built.len() - 1].range.end);
        let mut frame = self.stack.pop().expect("emit needs a frame");
        if matches!(
            frame.kind,
            FrameKind::Root | FrameKind::BlockQuote | FrameKind::Item { .. }
        ) {
            self.scan_gap(&mut frame, start);
        }
        frame.children.extend(built);
        frame.scanned_to = frame.scanned_to.max(end);
        self.stack.push(frame);
    }

    /// Scan `frame.scanned_to..up_to` for reference definitions and append the
    /// paragraphs they become.
    fn scan_gap(&mut self, frame: &mut Frame, up_to: usize) {
        if frame.scanned_to >= up_to {
            frame.scanned_to = frame.scanned_to.max(up_to);
            return;
        }
        let gap = frame.scanned_to..up_to;
        let in_list_item = matches!(frame.kind, FrameKind::Item { .. });
        let found = scan_definitions(self.src, gap.clone(), in_list_item, &mut self.seen_labels);
        frame.children.extend(found);
        frame.scanned_to = up_to;
    }
}

/// Whether a list is loose, `marked`'s way.
///
/// `Tokenizer.list` is the reference:
///
/// ```js
/// r.loose || (o ? r.loose = true
///               : this.rules.other.doubleBlankLine.test(c) && (o = true))
/// ```
///
/// `o` is set by an item whose raw **ends** with a blank line, and the *next*
/// iteration is what turns it into `loose`. So a trailing blank line on the
/// **last** item never makes the list loose — there is no next iteration — and
/// `marked` then trims that item's raw outright (`r.items.at(-1).raw.trimEnd()`),
/// so the later `space`-token test cannot see it either.
///
/// That single asymmetry is 23 of S1's 1344, in one direction, and it is the
/// shape of every ordinary document: a list, a blank line, then a paragraph.
///
/// ```text
/// "- one\n\n two\n"                   → tight, and pulldown-cmark's item range
///                                       ends with the blank line
/// "- foo\n- bar\n\n<!-- -->\n\n- baz" → the first list is tight
/// "- a\n  - b\n  - c\n\n- d\n  - e"   → the *inner* list is tight, the outer loose
/// ```
fn list_is_loose(any_real_paragraph: bool, item_blanks: &[(bool, bool)]) -> bool {
    if any_real_paragraph {
        return true;
    }
    let last = item_blanks.len().saturating_sub(1);
    item_blanks
        .iter()
        .enumerate()
        .any(|(index, (internal, trailing))| *internal || (*trailing && index != last))
}

/// Mechanism 2 — muya's #1735 fix.
///
/// > Fix #1735 the blockquote maybe empty. like bellow: `>` / `bar`
///
/// `markdownToState`'s `block-end` case pushes `{ name: 'paragraph', text: '' }`
/// when the level it is leaving is empty and the container was a **blockquote
/// or a list item** — not a list and not a footnote, which is why this is
/// called from exactly those two arms.
fn empty_container_filler(children: Vec<Built>, range: &Range<usize>) -> Vec<Built> {
    if !children.is_empty() {
        return children;
    }
    vec![Built::leaf(
        Block::Paragraph { text: Text::new() },
        range.end..range.end,
    )]
}

// ---------------------------------------------------------------------------
// Mechanism 3 — compatibleTaskList
// ---------------------------------------------------------------------------

/// One `pulldown-cmark` list, as the one-to-three muya lists it maps to.
///
/// `utils/marked/compatibleTaskList.ts`, 189 lines, exists because `Block` has
/// `BulletList` and `TaskList` as separate variants and **no mixed variant**.
/// GFM has one list of four items where `spec/fixtures/marktext-round-trip/common/Links.md`
/// has two plain items followed by two `- [ ]` ones; muya produces a
/// `bullet-list` of two followed by a `task-list` of two, and the port has to
/// split identically or the 1:1 mapping stops holding.
///
/// Three details from the TypeScript that a reimplementation loses:
///
/// - **An ordered list never splits.** `compatibleTaskList` takes the
///   `ordered === true` branch, sets `listItemType = 'order'` for every item
///   and pushes the list whole — so `1. [ ] a` is an `order-list` holding a
///   `list-item`, measured, even though `marked` did flag the item as a task.
/// - **Each split part keeps the original list's `loose`**, because `cache`
///   copies `loose` from the token it was split out of.
/// - **The marker comes from the part's own first item**, because `markdownToState`
///   reads `token.items[0].bulletMarkerOrDelimiter` and `token.items` is the
///   split part's.
fn split_list(
    src: &str,
    start: Option<u64>,
    loose: bool,
    items: Vec<Built>,
    range: &Range<usize>,
) -> Vec<Built> {
    if items.is_empty() {
        // `compatibleTaskList`'s bullet branch pushes nothing when a list has
        // no items — `cache` is never assigned — so the list disappears. The
        // ordered branch pushes the empty list. Reproduced rather than
        // smoothed over.
        return match start {
            Some(_) => vec![Built {
                block: Block::OrderList {
                    start: 1,
                    delimiter: OrderDelim::Period,
                    loose,
                    children: Vec::new(),
                },
                children: Vec::new(),
                range: range.clone(),
            }],
            None => Vec::new(),
        };
    }

    if let Some(start) = start {
        // `/^\d+$/.test(String(start)) ? Number(start) : 1` — `pulldown-cmark`
        // only ever reports a parsed integer here, so the guard is satisfied
        // by construction. Values above `u32::MAX` saturate; `marked` would
        // carry a JavaScript number.
        let delimiter = match marker_char(src, &items[0].range) {
            Some(')') => OrderDelim::Paren,
            _ => OrderDelim::Period,
        };
        return vec![Built {
            block: Block::OrderList {
                start: u32::try_from(start).unwrap_or(u32::MAX),
                delimiter,
                loose,
                children: Vec::new(),
            },
            // `listItemType = 'order'` for **every** item, whatever `marked`
            // flagged: the ordered branch of `compatibleTaskList` never reads
            // `item.task`. Measured — `1. [ ] a` is an `order-list` holding a
            // `list-item`, and GFM does report the marker.
            children: items.into_iter().map(demote_task_item).collect(),
            range: range.clone(),
        }];
    }

    let mut out: Vec<Built> = Vec::new();
    let mut group: Vec<Built> = Vec::new();
    let mut group_is_task = false;

    for item in items {
        let is_task = matches!(item.block, Block::TaskListItem { .. });
        if !group.is_empty() && is_task != group_is_task {
            out.push(bullet_or_task_list(
                src,
                group_is_task,
                loose,
                std::mem::take(&mut group),
            ));
        }
        group_is_task = is_task;
        group.push(item);
    }
    if !group.is_empty() {
        out.push(bullet_or_task_list(src, group_is_task, loose, group));
    }
    out
}

/// A task item inside an ordered list is a plain list item.
fn demote_task_item(mut item: Built) -> Built {
    if let Block::TaskListItem { .. } = item.block {
        item.block = Block::ListItem {
            children: Vec::new(),
        };
    }
    item
}

fn bullet_or_task_list(src: &str, task: bool, loose: bool, items: Vec<Built>) -> Built {
    // `bulletMarkerOrDelimiter || '-'`: an item whose raw does not match
    // `BULL_REG` yields `''`, which muya's `||` turns into `-`.
    let marker = match marker_char(src, &items[0].range) {
        Some('+') => BulletMarker::Plus,
        Some('*') => BulletMarker::Star,
        _ => BulletMarker::Dash,
    };
    let range = items[0].range.start..items[items.len() - 1].range.end;
    let block = if task {
        Block::TaskList {
            marker,
            loose,
            children: Vec::new(),
        }
    } else {
        Block::BulletList {
            marker,
            loose,
            children: Vec::new(),
        }
    };
    Built {
        block,
        children: items,
        range,
    }
}

/// `BULL_REG` — `/^ {0,3}([*+-]|\d{1,9}(?:\.|\)))/` against an item's source,
/// returning the character muya keeps: the bullet for an unordered item, the
/// delimiter (`.` or `)`) for an ordered one.
fn marker_char(src: &str, range: &Range<usize>) -> Option<char> {
    let s = &src[range.clone()];
    let indent = s.len() - s.trim_start_matches([' ', '\t']).len();
    // `{0,3}` counts spaces, but `pulldown-cmark`'s item range already starts
    // at the marker in every measured case; the trim is belt and braces.
    let rest = &s[indent..];
    let mut chars = rest.chars();
    match chars.next()? {
        c @ ('*' | '+' | '-') => Some(c),
        c if c.is_ascii_digit() => {
            let digits = rest
                .chars()
                .take(9)
                .take_while(char::is_ascii_digit)
                .count();
            match rest[digits..].chars().next() {
                Some(d @ ('.' | ')')) => Some(d),
                _ => None,
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Mechanism 1 — reference definitions
// ---------------------------------------------------------------------------

/// Find the reference definitions inside a gap and return the paragraphs muya
/// emits for them.
///
/// # Why the port scans rather than reading `parser.reference_definitions()`
///
/// M2.md §5 D3 decides this, and S1 re-measured its reasons against the
/// running engine. **One of the three does not hold and two do.** The
/// correction is written up in M2.md; the short form is that reading `RefDefs`
/// fails for reasons of *position*, not of loss:
///
/// - A definition inside a block quote belongs to the block quote
///   (`cm#218`), and `RefDefs`' span for `> [foo]: /url` is `2..13` — the
///   `> ` is excluded, so the span does not say which container it came from.
///   Scanning per container gets tree position for free.
/// - **A definition that follows a paragraph at the same level is absorbed
///   into it.** `marked`'s `def` branch appends the raw to `tokens.at(-1)`
///   when that is a paragraph or text, so `text\n[foo]: /a` is **one**
///   paragraph. `RefDefs` reports it as a separate definition with its own
///   span, so a `RefDefs`-driven port would emit a block muya does not have.
///   `pulldown-cmark` also keeps those two lines in one paragraph, so a gap
///   scan never sees it — the mechanism is reproduced by construction.
/// - `RefDefs` is a map, so its iteration order is not document order.
///
/// # The rule that *is* reproduced here
///
/// `marked` drops a definition whose label was already defined — and so does
/// `RefDefs`, which is why M2.md's original reproducer is wrong about it:
///
/// ```text
/// "[foo]: /a\n[foo]: /b\n\n[foo]\n" → muya emits ONE paragraph, "[foo]: /a"
/// "[FOO]: /a\n[foo]: /b\n\n[Foo]\n" → muya emits ONE paragraph, "[FOO]: /a"
/// ```
///
/// The label set is **document-global**: `this.tokens.links` belongs to the
/// `Lexer`, and a nested `blockTokens` call for a list item's content shares
/// it. Hence `seen` is threaded through the whole walk rather than per
/// container.
///
/// `marked` normalises a label with `toLowerCase()` and `/\s+/g → ' '`, which
/// is not Unicode case folding — another reason the two collapse sets are not
/// interchangeable.
fn scan_definitions(
    src: &str,
    gap: Range<usize>,
    in_list_item: bool,
    seen: &mut HashSet<String>,
) -> Vec<Built> {
    let logical = LogicalText::of(src, gap, in_list_item);
    let text = logical.text.as_str();
    let mut out = Vec::new();

    let mut at = 0;
    while at < text.len() {
        let line_end = line_end_at(text, at);
        if text[at..line_end].trim().is_empty() {
            at = line_end + 1;
            continue;
        }
        let Some((label, after)) = definition_head(text, at) else {
            // Non-blank source `pulldown-cmark` consumed that this scanner
            // does not recognise as a definition. It produces no block —
            // inventing one would be a guess — and `cargo xtask blocks` is
            // what reports the resulting disagreement.
            at = line_end + 1;
            continue;
        };

        // The definition's own last line, then any following lines that are
        // neither blank nor another definition: `[foo]:\n/a\n"t"` is one
        // definition and muya emits one paragraph holding all three lines.
        let mut end = line_end_at(text, after);
        let mut probe = end + 1;
        while probe < text.len() {
            let probe_end = line_end_at(text, probe);
            if text[probe..probe_end].trim().is_empty() || definition_head(text, probe).is_some() {
                break;
            }
            end = probe_end;
            probe = probe_end + 1;
        }

        // `marked` registers the label in `this.tokens.links` and pushes a
        // token only if it was not already there; a repeat emits nothing.
        if seen.insert(label) {
            out.push(Built::leaf(
                Block::Paragraph { text: Text::new() },
                logical.source_offset(at)..logical.source_offset(end),
            ));
        }
        at = end + 1;
    }
    out
}

fn line_end_at(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i)
}

/// `[label]:` starting at `at` — the normalised label and the offset just past
/// the colon.
///
/// Deliberately not a full transcription of `marked`'s `def` rule: whatever is
/// in a gap has **already been consumed by `pulldown-cmark` as a link
/// reference definition**, so the question here is only where one definition
/// ends and the next begins, and what its label is.
///
/// Three things the one-line version of this got wrong, each of which is a
/// fixture:
///
/// - **A label may span lines.** `[\nfoo\n]: /url` is `commonmark#208`, and
///   `[Foo\n  bar]: /url` is `commonmark#541`.
/// - **A label may contain an escaped bracket.** `[ref\[]: /uri` is
///   `commonmark#549`; stopping at any `[` loses it.
/// - **A backslash may itself be escaped.** `[bar\\]: /uri` is
///   `commonmark#550`, where the `]` *does* close the label because the
///   preceding backslash is escaped.
fn definition_head(text: &str, at: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let mut i = at;
    // ` {0,3}` — the leading indent a definition may carry.
    let indent = bytes[i..].iter().take_while(|b| **b == b' ').count();
    if indent > 3 {
        return None;
    }
    i += indent;
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    i += 1;

    let label_start = i;
    while i < bytes.len() {
        match bytes[i] {
            // `\\.` is one unit, so an escaped `]` does not close the label
            // and an escaped `\` does not shield the `]` after it.
            b'\\' => i += 2,
            b']' => {
                let label = &text[label_start..i];
                return (bytes.get(i + 1) == Some(&b':')).then(|| (normalise_label(label), i + 2));
            }
            _ => i += 1,
        }
    }
    None
}

/// `/\\([\p{P}\p{S}])/gu → '$1'` — `marked`'s `anyPunctuation`, applied to a
/// code fence's info string.
///
/// **Restricted to ASCII, deliberately and with the gap named.** `marked`'s
/// class is Unicode `P` ∪ `S`; CommonMark §2.4 only makes ASCII punctuation
/// escapable, and Rust's `is_ascii_punctuation` is exactly ASCII `P` ∪ `S`.
/// The two differ on inputs like ```` ```\— ````, where `marked` emits `—`
/// and this emits `\—`. No fixture in S1's 1344 contains one, and register
/// rule 3 makes an entry with no failing case **stale** — so this is recorded
/// here and in M2.md rather than registered. It is also the direction the
/// project has chosen before: where `marked` disagrees with the spec, the port
/// follows the spec and registers it *when it can be shown to happen*.
fn unescape_punctuation(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(next) = chars.peek()
            && next.is_ascii_punctuation()
        {
            out.push(*next);
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

/// `t[1].toLowerCase().replace(/\s+/g, ' ')`.
fn normalise_label(label: &str) -> String {
    let lowered = label.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut in_space = false;
    for c in lowered.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Leaves whose kind depends on their source
// ---------------------------------------------------------------------------

fn code_block(
    src: &str,
    kind: &CodeBlockKind<'_>,
    range: &Range<usize>,
    options: Options,
) -> Built {
    let (code_kind, info) = match kind {
        CodeBlockKind::Indented => (CodeKind::Indented, String::new()),
        // **Not `info` from the event.** `pulldown-cmark` resolves HTML
        // entities and backslash escapes in the info string, so
        // ```` ```f&ouml;&ouml; ```` arrives as `föö`; `marked` keeps the
        // source text and muya's `meta.lang` is therefore `f&ouml;&ouml;`
        // (`commonmark#34`, `gfm#330`). Constraint 1 on
        // `Block::CodeBlock::info` says never to normalise this field, and a
        // decoded entity is a normalisation — so it is re-read from the
        // source. `_buildCodeState` does `(infoString || '').trim()`, which is
        // the trim below.
        CodeBlockKind::Fenced(_) => (CodeKind::Fenced, raw_info(src, range)),
    };

    // `walkTokens` runs before `markdownToState`, so a ```` ```math ```` fence
    // never reaches `_buildCodeState` at all: it is rewritten in place into a
    // `multiplemath` token with `mathStyle: 'gitlab'`. The test is
    // `token.type === 'code' && token.lang === 'math'` on `marked`'s already-
    // trimmed `lang`, so it is the whole info string rather than its first
    // word — ```` ```math title=x ```` stays a code block.
    if options.math && options.gitlab_compatibility && info == "math" {
        return Built::leaf(
            Block::MathBlock {
                style: MathStyle::Gitlab,
                text: Text::new(),
            },
            range.clone(),
        );
    }

    // `firstWordOfInfo` — `info.match(/\S*/)?.[0]`, anchored at 0, so an info
    // string starting with whitespace yields the empty string. `info` has
    // already been trimmed, which is why that cannot bite here and does in
    // `Block::highlight_language`.
    let lang = &info[..info.find(char::is_whitespace).unwrap_or(info.len())];
    let diagram = match lang {
        "mermaid" => Some((DiagramKind::Mermaid, DiagramLang::Yaml)),
        "plantuml" => Some((DiagramKind::PlantUml, DiagramLang::Yaml)),
        "vega-lite" => Some((DiagramKind::VegaLite, DiagramLang::Json)),
        "flowchart" => Some((DiagramKind::Flowchart, DiagramLang::Yaml)),
        "sequence" => Some((DiagramKind::Sequence, DiagramLang::Yaml)),
        _ => None,
    };
    if let Some((kind, lang)) = diagram {
        return Built::leaf(
            Block::Diagram {
                lang,
                kind,
                text: Text::new(),
            },
            range.clone(),
        );
    }

    Built::leaf(
        Block::CodeBlock {
            kind: code_kind,
            info,
            fence_len: fence_length(src, range, code_kind),
            text: Text::new(),
        },
        range.clone(),
    )
}

/// The info string as the source spells it, entities and escapes intact.
fn raw_info(src: &str, range: &Range<usize>) -> String {
    let block = &src[range.clone()];
    let first_line = block.split('\n').next().unwrap_or(block);
    let after_indent = first_line.trim_start_matches(' ');
    let Some(fence) = after_indent
        .chars()
        .next()
        .filter(|c| *c == '`' || *c == '~')
    else {
        return String::new();
    };
    // `lang: cap[2].trim().replace(anyPunctuation, '$1')` — trimmed, then
    // backslash-unescaped. Entities are *not* resolved, which is why this
    // reads the source rather than `pulldown-cmark`'s already-decoded info.
    unescape_punctuation(after_indent.trim_start_matches(fence).trim())
}

/// `/^ {0,3}([`~]{3,})/.exec(raw)?.[1].length`, kept only where muya keeps it.
///
/// `_buildCodeState` spreads the key in as
/// `...(isFenced && fenceLength && fenceLength > 3 ? { fenceLength } : {})`, so
/// `None` here means "no `fenceLength` key" rather than "unknown", and the
/// serializer does not have to know the rule.
///
/// **Owed to S4.** `fence_len` is a `u8` (M0's shape), so a fence of more than
/// 255 characters saturates and the serializer would re-emit 255 of them —
/// a round-trip loss on `"`".repeat(300)`. No fixture in the 1344 has one, and
/// widening the field is a change to `mt-doc`'s public API that belongs with
/// the serializer that would lose the data.
fn fence_length(src: &str, range: &Range<usize>, kind: CodeKind) -> Option<u8> {
    if kind != CodeKind::Fenced {
        return None;
    }
    let s = &src[range.clone()];
    let after_indent = s.trim_start_matches(' ');
    if s.len() - after_indent.len() > 3 {
        return None;
    }
    let fence = after_indent.chars().next()?;
    if fence != '`' && fence != '~' {
        return None;
    }
    let len = after_indent.chars().take_while(|c| *c == fence).count();
    (len > 3).then(|| u8::try_from(len).unwrap_or(u8::MAX))
}

/// `markdownToState`'s `html` case, including the `<img>` special case.
///
/// > TODO: Treat html state which only contains one img as paragraph, we maybe
/// > add image state in the future.
///
/// `/^<img[^<>]+>$/` against the **trimmed** text, so `<img src="x">` is a
/// paragraph and `<img>\n<img>` is an html block.
fn html_block(src: &str, range: &Range<usize>) -> Built {
    let text = src[range.clone()].trim();
    let single_image = text.starts_with("<img")
        && text.ends_with('>')
        && text.len() > 5
        && !text[4..text.len() - 1].contains(['<', '>']);
    let block = if single_image {
        Block::Paragraph { text: Text::new() }
    } else {
        Block::HtmlBlock { text: Text::new() }
    };
    Built::leaf(block, range.clone())
}

/// `walkTokens`'s setext test: `/\n {0,3}(=+|-+)/` over the heading's raw.
fn setext_underline(raw: &str) -> Option<Underline> {
    let bytes = raw.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'\n' {
            continue;
        }
        let mut at = i + 1;
        let spaces = bytes[at..].iter().take_while(|b| **b == b' ').count();
        if spaces > 3 {
            continue;
        }
        at += spaces;
        let Some(&c) = bytes.get(at) else { continue };
        if c != b'=' && c != b'-' {
            continue;
        }
        let run = bytes[at..].iter().take_while(|b| **b == c).count() as u32;
        return Some(if c == b'=' {
            Underline::Equals(run)
        } else {
            Underline::Dashes(run)
        });
    }
    None
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn owned_code_kind(kind: &CodeBlockKind<'_>) -> CodeBlockKind<'static> {
    match kind {
        CodeBlockKind::Indented => CodeBlockKind::Indented,
        CodeBlockKind::Fenced(info) => CodeBlockKind::Fenced(info.to_string().into()),
    }
}

// ---------------------------------------------------------------------------
// Mechanism 4 — the block math rule
// ---------------------------------------------------------------------------

/// muya's `blockKatex` rule, transcribed:
///
/// ```text
/// /^(\${1,2})\n((?:\\[\s\S]|[^\\])+?)\n\1[ \t]*(?:\n|$)/
/// ```
///
/// Returns how many bytes of `text` the match consumed, or `None`.
///
/// Two details that fall out of the regex rather than out of intuition:
///
/// - `\${1,2}` is **greedy**, so `$$` is preferred; but `$$$\nx\n$$$` matches
///   nothing, because after backtracking to one `$` the next character is
///   still not a newline.
/// - `(?:\\[\s\S]|[^\\])+?` means a backslash **consumes the next character**,
///   so a `\` immediately before a newline makes that newline part of the
///   content and unable to close the block.
fn block_math(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let dollars = bytes.iter().take_while(|b| **b == b'$').count().min(2);
    for open in (1..=dollars).rev() {
        if bytes.get(open) != Some(&b'\n') {
            continue;
        }
        let content_start = open + 1;
        let mut at = content_start;
        while at < bytes.len() {
            if bytes[at] == b'\\' {
                // `\\[\s\S]` — one unit, two bytes.
                at += 2;
                continue;
            }
            if bytes[at] == b'\n' && at > content_start {
                let mut after = at + 1;
                if !text[after..].starts_with(&text[..open]) {
                    at += 1;
                    continue;
                }
                after += open;
                let trailing = bytes[after..]
                    .iter()
                    .take_while(|b| **b == b' ' || **b == b'\t')
                    .count();
                after += trailing;
                match bytes.get(after) {
                    Some(b'\n') => return Some(after + 1),
                    None => return Some(after),
                    _ => {}
                }
            }
            at += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Source-scanning helpers
// ---------------------------------------------------------------------------

/// A source line inside a range, as absolute offsets. `end` excludes the
/// newline.
#[derive(Debug, Clone)]
struct Line {
    start: usize,
    end: usize,
}

struct Lines {
    at: usize,
    end: usize,
    src_end: usize,
    newlines: Vec<usize>,
}

impl Lines {
    fn new(src: &str, range: Range<usize>) -> Self {
        let newlines = src[range.clone()]
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| range.start + i)
            .collect();
        Lines {
            at: range.start,
            end: range.end,
            src_end: range.end,
            newlines,
        }
    }
}

impl Iterator for Lines {
    type Item = Line;

    fn next(&mut self) -> Option<Line> {
        if self.at >= self.end {
            return None;
        }
        let next_newline = self
            .newlines
            .iter()
            .copied()
            .find(|n| *n >= self.at)
            .unwrap_or(self.src_end);
        let line = Line {
            start: self.at,
            end: next_newline.min(self.end),
        };
        self.at = next_newline + 1;
        Some(line)
    }
}

/// Where a line's content starts, after the container syntax a gap scan has to
/// look past.
///
/// **This is not D2's container-prefix stripper**, which is S2's and has to be
/// exact because a leaf's text depends on it. This one only has to find the
/// first character that could begin a reference definition, so it strips
/// indentation and block-quote markers and — on a list item's first line only
/// — the bullet.
fn content_start(src: &str, line: &Line, strip_marker: bool) -> usize {
    let mut at = line.start;
    let bytes = src.as_bytes();
    loop {
        while at < line.end && (bytes[at] == b' ' || bytes[at] == b'\t') {
            at += 1;
        }
        if at < line.end && bytes[at] == b'>' {
            at += 1;
            continue;
        }
        break;
    }
    if strip_marker {
        let rest = &src[at..line.end];
        if let Some(marker) = marker_char(
            src,
            &Range {
                start: at,
                end: line.end,
            },
        ) {
            let width = if matches!(marker, '.' | ')') {
                rest.chars().take_while(char::is_ascii_digit).count() + 1
            } else {
                1
            };
            let mut after = at + width;
            while after < line.end && (bytes[after] == b' ' || bytes[after] == b'\t') {
                after += 1;
            }
            at = after;
        }
    }
    at
}

/// A source range with its container syntax removed line by line, plus the map
/// back to source offsets.
///
/// Used only by mechanism 4, which needs to ask whether a paragraph *is* a
/// math block. Same caveat as [`content_start`]: it is a detection aid, not
/// D2's stripper.
struct LogicalText {
    text: String,
    /// `(logical offset, source offset)` at the start of each line.
    lines: Vec<(usize, usize)>,
}

impl LogicalText {
    /// `strip_marker` also removes a list marker from the **first** line,
    /// which is what a gap inside a list item needs and what a paragraph's
    /// range must not have.
    fn of(src: &str, range: Range<usize>, strip_marker: bool) -> Self {
        let mut text = String::new();
        let mut lines = Vec::new();
        for (index, line) in Lines::new(src, range).enumerate() {
            let at = content_start(src, &line, strip_marker && index == 0);
            lines.push((text.len(), at));
            text.push_str(&src[at..line.end]);
            text.push('\n');
        }
        LogicalText { text, lines }
    }

    /// The source offset a logical offset corresponds to.
    ///
    /// Exact within a line, because the stripper only ever removes a prefix —
    /// so the two run in step from each line's content start. A logical offset
    /// that lands on the synthetic `\n` a line was joined with maps to that
    /// line's source end.
    fn source_offset(&self, logical: usize) -> usize {
        let mut answer = self.lines.first().map_or(0, |(_, s)| *s);
        for (index, (logical_start, source_start)) in self.lines.iter().enumerate() {
            if *logical_start > logical {
                break;
            }
            let next_logical = self
                .lines
                .get(index + 1)
                .map_or(self.text.len(), |(l, _)| *l);
            // `next_logical - 1` is the line's own `\n`; clamping there keeps
            // the offset inside this line's source.
            answer = source_start + (logical.min(next_logical.saturating_sub(1)) - logical_start);
        }
        answer
    }
}

/// `/\n.*\n/` — `marked`'s `anyLine`, which is how it decides a `space` token
/// means the list is loose.
fn has_blank_line(gap: &str) -> bool {
    let mut seen_newline = false;
    let mut blank_since = true;
    for c in gap.chars() {
        match c {
            '\n' => {
                if seen_newline && blank_since {
                    return true;
                }
                seen_newline = true;
                blank_since = true;
            }
            ' ' | '\t' | '\r' => {}
            _ => blank_since = false,
        }
    }
    false
}

/// Whether a tag is block-level.
///
/// **This is §4 C1's second measurement mistake, in one function.** The
/// synthetic paragraph a tight list item needs must be closed by a block tag
/// and never by `Emphasis`, `Link` or `Strikethrough`; closing on any `Start`
/// splits every item containing emphasis into several paragraphs, and that
/// presented as a plausible `marked`-versus-CommonMark list finding in
/// `10kb.md` before it was chased down.
fn is_block_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

fn is_block_tag_end(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
            | TagEnd::MetadataBlock(_)
    )
}

#[cfg(test)]
mod tests;
