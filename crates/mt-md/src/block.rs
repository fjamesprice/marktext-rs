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
//! | 8 | Every leaf's **text**, which is not a source slice | `strip_lines` + D2's per-kind rules |
//!
//! Mechanism 5 is the one §4 C1 records as having cost a measurement: a
//! mapping that only creates nodes for block tags gives every tight item no
//! children at all, which is 90 false disagreements, and the synthetic
//! paragraph that fixes it must be closed by a **block** tag and never by
//! `Emphasis`, `Link` or `Strikethrough` — closing on any `Start` splits every
//! item containing emphasis into several paragraphs, which is 5 more.
//! `is_block_tag` is where that distinction is written down.
//!
//! # Leaf text — S2, and where the rules came from
//!
//! §4 C2 measured that a leaf's `text` is **not** a slice of the document: it
//! is the block's content with its block and container syntax removed, and in
//! four cases rewritten. S1 built every leaf with the empty string and S2
//! filled them in. Two halves:
//!
//! - [`strip_lines`] — the container-prefix stripper D2 asks for, composed
//!   outermost-first over the source lines a block's range touches.
//! - The per-kind rules below it — `atx_text`, `setext_text`, `code_text`,
//!   `table_cell_text`, `front_matter_text`, and the trims each leaf kind
//!   applies on top.
//!
//! **Both are transcriptions of `marked` 18.0.5 rather than readings of
//! CommonMark**, because a leaf's text is `marked`'s `token.text` and nothing
//! else. M2.md's brief describes `marked` as a minified bundle to be probed
//! through `node -e`; it ships **`lib/marked.esm.js.map` with
//! `sourcesContent`**, so `Tokenizer.ts`, `Lexer.ts`, `rules.ts` and
//! `helpers.ts` are readable in full. Every rule here names the function it
//! came from, and the measured reproducers in `tests.rs` are what proves the
//! reading — the source said what to look for and the running engine said
//! whether it was right.
//!
//! Two decisions here nonetheless read the source for a *structural* reason —
//! whether an HTML block is a lone `<img>`, and whether a paragraph is really
//! a `$$` math block. Both decide which **block** to build rather than what
//! text it holds, and both are muya's own.

use std::collections::HashSet;
use std::ops::Range;

use mt_doc::{
    Align, Block, BulletMarker, CodeKind, DiagramKind, DiagramLang, Document, Edit,
    FrontmatterLang, FrontmatterStyle, MathStyle, NodeId, OrderDelim, Text, Underline,
};
use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, Options as CmarkOptions, Parser, Tag, TagEnd,
};

use crate::{Options, SourceMap};

/// Markdown → [`Document`]: block structure, `meta` and leaf text.
///
/// **Not [`crate::parse`].** That entry point stays `Err(Unimplemented)` until
/// S3, because D6 stages the three ratchets by entry point and `parse`
/// returning `Ok` is what wakes the first of them. This is the function
/// `parse` will *call* at S3 — S3's remaining work is the label-map pass, not
/// a rewrite of this — and it is what `cargo xtask blocks` drives today.
///
/// # The source ranges are kept, and they are kept beside the document
///
/// They exist while this function runs ([`Built::range`]) and [`mt_doc::Block`]
/// maps 1:1 onto the TypeScript `TState` union, so there is nowhere on a
/// `Block` to put them. S3 decided where instead — [`crate::SourceMap`], a side
/// table returned *with* the document rather than stored *in* it, for the
/// reason its own docs give: a range is a fact about one parse, and a
/// `Document` outlives its parse. [`parse_blocks_with_ranges`] is that
/// signature; this one drops them, for the callers that only want the tree.
pub fn parse_blocks(markdown: &str, options: Options) -> Document {
    parse_blocks_with_ranges(markdown, options).0
}

/// [`parse_blocks`], keeping the per-node source ranges M2.md §10 owes forward.
///
/// The ranges are what makes the container-prefix stripper re-runnable one
/// block at a time from `(src, tree, ranges)` — S6's region reparse, M4's caret
/// and the reverse offset map §10 asks for are all that same need. See
/// [`crate::SourceMap`] for what the map does and does not promise.
pub fn parse_blocks_with_ranges(markdown: &str, options: Options) -> (Document, SourceMap) {
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
    /// The source range this block came from — what S2 reconstructs leaf text
    /// from, and what M2.md §10's owed reverse map would be built out of.
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
fn to_document(top: Vec<Built>) -> (Document, SourceMap) {
    let mut doc = Document::new();
    let mut ranges = SourceMap::default();
    // `Document::new` is a root holding one empty paragraph, which is exactly
    // `markdownToState`'s own `states.length ? states : [{ name: 'paragraph',
    // text: '' }]` fallback. So an empty parse needs no special case; a
    // non-empty one drops the placeholder.
    if top.is_empty() {
        // The fallback paragraph came from nowhere in the source, and `0..0` is
        // how the map says so — `src[0..0]` is the empty string, which is the
        // paragraph's text. Recording it keeps the invariant total: **every
        // live node except the root has a range**, which is what
        // `every_node_has_a_range_and_the_ranges_nest` checks.
        ranges.insert(doc.children(doc.root())[0], 0..0);
        return (doc, ranges);
    }

    let root = doc.root();
    let placeholder = doc.children(root)[0];
    for (index, built) in top.into_iter().enumerate() {
        insert(&mut doc, &mut ranges, root, index, built);
    }
    doc.apply(&[Edit::RemoveNode { node: placeholder }]);
    // `RemoveNode` detaches rather than frees, and nothing here holds an
    // inverse that could name the placeholder again.
    doc.prune_detached();
    (doc, ranges)
}

fn insert(doc: &mut Document, ranges: &mut SourceMap, parent: NodeId, index: usize, built: Built) {
    let inverse = doc.apply(&[Edit::InsertNode {
        parent,
        index,
        block: built.block,
    }]);
    let id = match inverse.as_slice() {
        [Edit::RemoveNode { node }] => *node,
        other => unreachable!("InsertNode's inverse is RemoveNode; got {other:?}"),
    };
    ranges.insert(id, built.range);
    for (child_index, child) in built.children.into_iter().enumerate() {
        insert(doc, ranges, id, child_index, child);
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
pub(crate) fn cmark_options() -> CmarkOptions {
    CmarkOptions::ENABLE_TABLES
        | CmarkOptions::ENABLE_STRIKETHROUGH
        | CmarkOptions::ENABLE_TASKLISTS
}

// ---------------------------------------------------------------------------
// The footnote block extension — `utils/marked/extensions/footnote.ts`
// ---------------------------------------------------------------------------

/// What one pass over the source hands the builder when footnotes are on.
#[derive(Debug, PartialEq, Eq)]
enum Segment {
    /// Ordinary markdown, walked by `pulldown-cmark`.
    Body(Range<usize>),
    /// `[^id]: …`, consumed by the extension and re-lexed from a de-indented
    /// copy of its own body.
    Footnote {
        identifier: String,
        /// The **cleaned** body: not a slice of the source. See
        /// [`footnote_block`] for what that costs the range map.
        body: String,
        /// The definition's extent in the source, `raw` in `marked`'s terms.
        range: Range<usize>,
    },
}

/// muya's own `marked` block extension, transcribed —
/// `utils/marked/extensions/footnote.ts`, 114 lines.
///
/// # Why this is a source scan and not a `pulldown-cmark` feature
///
/// `ENABLE_FOOTNOTES` is **GFM's** footnote syntax and this is not it. muya's
/// rule is
///
/// ```text
/// /^\[\^([^^[\]\s]+)(?<!\\)\]:([\s\S]*?)(?=\n *\n {0,3}[^ ]|$)/
/// ```
///
/// with a `start()` hook that returns the index of a `\n[^id]:` so that a
/// paragraph *terminates* there — which is why a definition may interrupt a
/// paragraph, and why the body runs on through blank lines for as long as the
/// line after each one is indented four spaces or more. Neither behaviour is
/// `pulldown-cmark`'s, and `pulldown-cmark` has no extension point, so the
/// split happens before it sees the text. M2.md §10's "Owed by S1" item 1
/// predicted exactly this and left the flag off for exactly this reason.
///
/// # The one place this is narrower than `marked`, named rather than hidden
///
/// `marked` runs its extensions inside the nested `blockTokens` call a list
/// item's dedented content gets, so `- [^a]: x` holds a footnote in muya and a
/// paragraph here. This scanner works on the document's own lines, so a
/// definition is only recognised at column 0. **Not registered** — rule 3
/// makes an entry with no failing differential case stale, and both option
/// sets every harness drives have `footnote: false`, so no input in the 1344
/// or the 22 reaches it at all. M2.md §10 carries it.
fn footnote_segments(src: &str, body_at: usize) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut at = body_at;
    let mut body_start = body_at;
    let mut fence: Option<(char, usize)> = None;

    while at < src.len() {
        let line_end = line_end_at(src, at);
        let line = &src[at..line_end];

        // A fence's inside is not a token boundary, so the extension never
        // runs there. Tracked with a line scan because that is all it takes;
        // an indented code block cannot contain a column-0 line at all.
        match fence {
            Some((marker, len)) => {
                let closing = line.trim_start_matches(' ');
                if closing.starts_with(marker)
                    && closing.chars().take_while(|c| *c == marker).count() >= len
                    && closing.trim_start_matches(marker).trim().is_empty()
                {
                    fence = None;
                }
                at = line_end + 1;
                continue;
            }
            None => {
                if let Some((marker, len)) = opening_fence(line) {
                    fence = Some((marker, len));
                    at = line_end + 1;
                    continue;
                }
            }
        }

        let Some((identifier, after_colon)) = footnote_head(src, at) else {
            at = line_end + 1;
            continue;
        };

        if at > body_start {
            segments.push(Segment::Body(body_start..at));
        }
        let end = footnote_extent(src, after_colon);
        segments.push(Segment::Footnote {
            identifier,
            body: clean_footnote_body(&src[after_colon..end]),
            range: at..end,
        });
        at = end;
        // The `\n *\n` in the lookahead is not consumed, so the next region
        // starts at the newline the definition stopped before.
        body_start = end;
    }

    if body_start < src.len() {
        segments.push(Segment::Body(body_start..src.len()));
    }
    if segments.is_empty() {
        segments.push(Segment::Body(body_at..src.len()));
    }
    segments
}

/// ```` ``` ```` or `~~~` at up to three spaces of indent — the marker and the
/// run length, which a closing fence must match or exceed.
fn opening_fence(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    for marker in ['`', '~'] {
        let run = rest.chars().take_while(|c| *c == marker).count();
        if run >= 3 {
            return Some((marker, run));
        }
    }
    None
}

/// `^\[\^([^^[\]\s]+)(?<!\\)\]:` at `at` — the identifier and the offset just
/// past the colon.
///
/// The lookbehind is what lets `[^foo\]: bar` stay a paragraph, and the
/// character class excludes `^` itself, so `[^^x]` is not a definition.
fn footnote_head(src: &str, at: usize) -> Option<(String, usize)> {
    let rest = src.get(at..)?;
    let body = rest.strip_prefix("[^")?;
    let mut end = 0;
    for (index, ch) in body.char_indices() {
        if ch == ']' {
            end = index;
            break;
        }
        if ch == '^' || ch == '[' || ch.is_whitespace() {
            return None;
        }
        end = index + ch.len_utf8();
    }
    let identifier = body.get(..end)?;
    if identifier.is_empty() || identifier.ends_with('\\') || !body[end..].starts_with("]:") {
        return None;
    }
    Some((identifier.to_string(), at + 2 + end + 2))
}

/// `([\s\S]*?)(?=\n *\n {0,3}[^ ]|$)` — everything up to the first blank line
/// that is followed by a line indented three spaces or fewer.
fn footnote_extent(src: &str, from: usize) -> usize {
    let mut at = from;
    while at < src.len() {
        let Some(newline) = src[at..].find('\n').map(|i| at + i) else {
            return src.len();
        };
        // `\n *\n`
        let mut probe = newline + 1;
        probe += src[probe..].bytes().take_while(|b| *b == b' ').count();
        if !src[probe..].starts_with('\n') {
            at = newline + 1;
            continue;
        }
        // ` {0,3}[^ ]`
        let after = probe + 1;
        let indent = src[after..].bytes().take_while(|b| *b == b' ').count();
        if indent <= 3
            && src[after + indent..]
                .bytes()
                .next()
                .is_some_and(|b| b != b' ')
        {
            return newline;
        }
        at = newline + 1;
    }
    src.len()
}

/// The extension's five chained `replace`s, in its order.
///
/// The first-line `^ {4}` strip is the one that is not obvious and the
/// extension's own comment says why: the per-line rule is anchored to a
/// preceding `\n`, so without it `[^id]:\n    text` would lex as an indented
/// code block rather than a paragraph.
fn clean_footnote_body(rest: &str) -> String {
    let cleaned = rest.trim_start_matches([' ', '\t']);
    let cleaned = cleaned.trim_start_matches('\n');
    let cleaned = cleaned.strip_prefix("    ").unwrap_or(cleaned);
    let mut out = String::with_capacity(cleaned.len());
    let mut at = 0;
    while let Some(newline) = cleaned[at..].find('\n').map(|i| at + i) {
        out.push_str(&cleaned[at..=newline]);
        let after = newline + 1;
        // `\n {4}(?=\S)` — four spaces, and only when real content follows.
        if cleaned[after..].starts_with("    ")
            && cleaned[after + 4..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace())
        {
            at = after + 4;
        } else {
            at = after;
        }
    }
    out.push_str(&cleaned[at..]);
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// A `footnote` container whose children are the de-indented body, re-lexed.
///
/// # Every node under a footnote carries the footnote's own range
///
/// The body `marked` lexes is a **de-indented copy** of the source, not a
/// slice of it, so an offset into that copy does not map to a document offset
/// by addition — the same fact §4 C2 records about leaf text, one level up.
/// Rather than record a range into a string the document does not contain,
/// every node here carries the definition's own extent: coarse, and true.
/// M2.md §10 owes the finer map to whichever stage first needs a caret inside
/// a footnote.
fn footnote_block(identifier: String, body: &str, range: Range<usize>, options: Options) -> Built {
    // `lexer.blockTokens(cleaned, [])` — the nested lex has the same
    // extensions and no front matter, because front matter is split off the
    // document before the lexer runs at all.
    let mut children = build(
        body,
        Options {
            front_matter: false,
            ..options
        },
    );
    for child in &mut children {
        retarget(child, &range);
    }
    Built {
        block: Block::Footnote {
            identifier,
            children: Vec::new(),
        },
        children,
        range,
    }
}

fn retarget(built: &mut Built, range: &Range<usize>) {
    built.range = range.clone();
    for child in &mut built.children {
        retarget(child, range);
    }
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
    /// The enclosing containers' prefixes, outermost first — D2's stripper.
    ///
    /// Kept in step with the frame stack rather than derived from it, because
    /// a [`Prefix::Item`] is computed once from the item's first line and
    /// re-deriving it per leaf would be the same scan run once per block.
    prefixes: Vec<Prefix>,
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
                text: Text::from(front_matter_text(&markdown[text_range.clone()])),
            },
            text_range,
        ));
        body_at = rest_at;
    }

    // `marked`'s `this.tokens.links` is one map for the whole lex, including
    // the nested `blockTokens` call a list item's content gets — so it is
    // threaded across every region rather than reset per region.
    let mut seen_labels = HashSet::new();

    if options.footnote {
        for segment in footnote_segments(markdown, body_at) {
            match segment {
                Segment::Body(region) => {
                    top.extend(build_region(markdown, options, region, &mut seen_labels));
                }
                Segment::Footnote {
                    identifier,
                    body,
                    range,
                } => top.push(footnote_block(identifier, &body, range, options)),
            }
        }
    } else {
        top.extend(build_region(
            markdown,
            options,
            body_at..markdown.len(),
            &mut seen_labels,
        ));
    }
    top
}

/// One region of the document, walked by `pulldown-cmark` as if it were the
/// whole of it.
///
/// That is `marked`'s own model rather than a simplification of it: its block
/// lexer is a position loop with no cross-block state except `tokens.links`,
/// so a block extension that consumes a span and hands the loop back its tail
/// is exactly a split into regions.
fn build_region(
    src: &str,
    options: Options,
    region: Range<usize>,
    seen_labels: &mut HashSet<String>,
) -> Vec<Built> {
    let mut builder = Builder {
        src,
        options,
        stack: vec![Frame {
            kind: FrameKind::Root,
            range: region.clone(),
            children: Vec::new(),
            scanned_to: region.start,
        }],
        seen_labels: std::mem::take(seen_labels),
        prefixes: Vec::new(),
    };
    builder.walk(region);

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
    builder.fold_empty_task_marker_continuations(&mut root.children);
    *seen_labels = std::mem::take(&mut builder.seen_labels);
    root.children
}

impl<'a> Builder<'a> {
    /// Walk `pulldown-cmark`'s events over `src[region]`.
    ///
    /// Offsets are shifted back to whole-document offsets by the region's
    /// start, which is non-zero when front matter was split off (S1) or when a
    /// footnote definition split the document into regions (S5).
    fn walk(&mut self, region: Range<usize>) {
        let body_at = region.start;
        let body = &self.src[region];
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
                    // `hr`'s raw is `rtrim(cap[0], '\n')` and `markdownToState`
                    // re-applies `/\n+$/`, so the text is the source line —
                    // **leading indent kept**, trailing spaces and tabs kept.
                    let raw = stripped_raw(&self.strip(&range));
                    let block = Block::ThematicBreak {
                        text: Text::from(raw.trim_end_matches('\n').to_string()),
                    };
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
        // D2's stripper: a quote and an item add a prefix to every line their
        // children live on, and the item's is computed once, here, from its
        // own first line.
        match kind {
            FrameKind::BlockQuote => self.prefixes.push(Prefix::Quote {
                first_line: line_start_of(self.src, range.start),
            }),
            FrameKind::Item { .. } => {
                let prefix = self.item_prefix(&range);
                self.prefixes.push(prefix);
            }
            _ => {}
        }
        self.stack.push(Frame {
            kind,
            range,
            children: Vec::new(),
            scanned_to,
        });
    }

    /// `Tokenizer.list`'s per-item `indent`, computed from the item's first
    /// line with the *outer* containers already stripped off it.
    ///
    /// ```js
    /// let line = expandTabs(cap[2].split('\n', 1)[0], cap[1].length);
    /// let blankLine = !line.trim();
    /// if (blankLine)  indent = cap[1].length + 1;
    /// else { indent = line.search(/[^ ]/); indent = indent > 4 ? 1 : indent;
    ///        itemContents = line.slice(indent); indent += cap[1].length; }
    /// ```
    ///
    /// `indent > 4 ? 1 : indent` is the rule that keeps `-     foo` — five
    /// spaces, which would be an indented code block — as one column of marker
    /// padding and four of content.
    fn item_prefix(&self, range: &Range<usize>) -> Prefix {
        let first_line = line_start_of(self.src, range.start);
        let end = line_end_of(self.src, first_line);
        let mut text = self.src[first_line..end].to_string();
        let (mut source_start, mut exact) = (first_line, true);
        for prefix in &self.prefixes {
            let next = apply_prefix(*prefix, first_line, None, text, source_start, exact);
            text = next.0;
            source_start = next.1;
            exact = next.2;
        }

        // `cap[1]` — ` {0,3}` then the bullet or `\d{1,9}[.)]`.
        let leading = text.bytes().take_while(|b| *b == b' ').count().min(3);
        let marker_len = match text[leading..].chars().next() {
            Some('*' | '+' | '-') => leading + 1,
            Some(c) if c.is_ascii_digit() => {
                let digits = text[leading..]
                    .chars()
                    .take(9)
                    .take_while(char::is_ascii_digit)
                    .count();
                leading + digits + 1
            }
            // No marker on the item's first line. `pulldown-cmark` does not
            // produce one, so this is unreachable rather than tolerated — but
            // a zero-width prefix is the answer that changes nothing.
            _ => 0,
        };

        let rest = text.get(marker_len..).unwrap_or("");
        let expanded = expand_tabs(rest, marker_len);
        let indent = if expanded.trim().is_empty() {
            marker_len + 1
        } else {
            let found = expanded.find(|c| c != ' ').unwrap_or(0);
            marker_len + if found > 4 { 1 } else { found }
        };
        Prefix::Item {
            indent,
            marker_len,
            first_line,
        }
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
                let raw = stripped_raw(&self.strip(&frame.range));
                code_block(self.src, &raw, kind, &frame.range, self.options)
            }

            FrameKind::HtmlBlock => {
                let raw = stripped_raw(&self.strip(&frame.range));
                html_block(&raw, &frame.range)
            }

            FrameKind::BlockQuote => {
                self.absorb_trailing_blank_quote_line(&mut frame);
                // Before the prefix is popped, because the fold measures the
                // task marker's column in the *container-stripped* line.
                self.fold_empty_task_marker_continuations(&mut frame.children);
                self.prefixes.pop();
                Built {
                    // Mechanism 2, muya #1735: `>` alone is a block quote
                    // holding one empty paragraph, not an empty block quote.
                    children: empty_container_filler(frame.children, &frame.range),
                    block: Block::BlockQuote {
                        children: Vec::new(),
                    },
                    range: frame.range,
                }
            }

            FrameKind::Item {
                task,
                real_paragraph,
                ..
            } => {
                let blanks = self.item_blank_lines(&frame);
                self.record_item_looseness(real_paragraph, blanks);
                self.fold_empty_task_marker_continuations(&mut frame.children);
                self.prefixes.pop();
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

            FrameKind::TableCell { align } => {
                let raw = stripped_raw(&self.strip(&frame.range));
                Built::leaf(
                    Block::TableCell {
                        align,
                        text: Text::from(table_cell_text(&raw)),
                    },
                    frame.range,
                )
            }
        };

        self.emit(built);
    }

    /// A source range with every enclosing container's prefix removed.
    fn strip(&self, range: &Range<usize>) -> Vec<StrippedLine> {
        strip_lines(self.src, range, &self.prefixes)
    }

    // -----------------------------------------------------------------------
    // Mechanism 5 — an empty task marker takes a lazy continuation
    // -----------------------------------------------------------------------

    /// `- [ ] \ntext` is **one** task item in `marked` and was an item plus a
    /// paragraph here until S4. This is the fix §10's "Owed by S3" asked for,
    /// and it is stated as the rule rather than as the symptom.
    ///
    /// # The mechanism, from both sides
    ///
    /// To `marked` a list item's content is the source text after the bullet —
    /// for `- [ ] ` that is the string `"[ ] "`, which is **not blank** — so
    /// `Tokenizer.list`'s continuation loop keeps consuming lines into the item
    /// exactly as it would after `- a`. The `[ ] ` prefix comes off afterwards,
    /// in `compatibleTaskList.normalizeEmptyTaskItem`, whose two regexes
    /// (`EMPTY_TASK_REG` and `TASK_MARKER_PREFIX_REG`) are what make the item a
    /// task item at all — `marked`'s own `listIsTask` is `/^\[[ xX]\] +\S/` and
    /// a marker at end of line has no `\S` after it, so `marked` never flags one.
    ///
    /// To GFM the `[ ]` is a **task-list marker** rather than paragraph
    /// content, so `pulldown-cmark` leaves the item empty, closes the list, and
    /// starts a new block at column 0.
    ///
    /// # What is folded, and the four rules that bound it
    ///
    /// Only the shape above, and only where `marked`'s loop would have
    /// consumed the line:
    ///
    /// 1. The list's **last** item is a single line that is nothing but a
    ///    bullet or an ordered marker, a task checkbox, and trailing blanks —
    ///    [`bare_task_marker_column`]. An item with any content is one both
    ///    engines already agree about.
    /// 2. The next sibling is a **paragraph** beginning on the immediately
    ///    following line. A blank line closes the item's paragraph in `marked`
    ///    too (`- [ ] \n\ntext` agrees today), and a heading, a quote, a fence
    ///    or a thematic break breaks the loop by its own rule.
    /// 3. Each of that paragraph's lines is dedented by the marker's column if
    ///    it reaches it, and appended verbatim if it does not — which is
    ///    `nextLineWithoutTabs.slice(indent)` against `nextLine`, the two arms
    ///    of the loop's one `if`.
    /// 4. An **ordered** list keeps the marker in the item's text and a bullet
    ///    list does not, because `compatibleTaskList` only calls
    ///    `stripTaskTextPrefix` in its bullet branch. `1. [ ] \ntext` is an
    ///    `order-list` holding `"[ ] \ntext"`; `- [ ] \ntext` is a `task-list`
    ///    holding `"text"`. Measured, both.
    ///
    /// Then, and only then, a following list of the same kind is **merged**
    /// back: `- [ ] \ntext\n- [ ] b` is one `marked` list all along, and the
    /// port only saw two because the paragraph interrupted it. A blank line in
    /// the gap makes the merged list loose, which is where `marked` gets
    /// looseness from as well.
    ///
    /// # What it deliberately does not do
    ///
    /// `marked` re-lexes an item's dedented content **line by line** with
    /// `state.top = false`, so any line at all can start a block inside an
    /// item, and CommonMark's paragraph-interruption rules do not apply there.
    /// That is a wider disagreement than this one and S4 found it rather than
    /// fixing it — M2.md §10's "Owed by S4" names it with its reproducers, and
    /// `tests::an_empty_task_marker_takes_a_lazy_continuation` pins the
    /// boundary of what *is* folded.
    fn fold_empty_task_marker_continuations(&self, children: &mut Vec<Built>) {
        let mut i = 0;
        while i + 1 < children.len() {
            if self.fold_one_task_marker(children, i) {
                // The same index again: a merge can expose another bare marker
                // at the end of the merged list. It terminates because a fold
                // always gives the last item content, and a merge always
                // removes a sibling.
                continue;
            }
            i += 1;
        }
    }

    /// One fold at `i`, or `false` if the shape is not there.
    fn fold_one_task_marker(&self, children: &mut Vec<Built>, i: usize) -> bool {
        let Some((indent, marker_line)) = self.bare_task_marker(&children[i]) else {
            return false;
        };
        let ordered = matches!(children[i].block, Block::OrderList { .. });

        let Block::Paragraph { text } = &children[i + 1].block else {
            return false;
        };
        let continuation = text.to_str().into_owned();
        let paragraph_range = children[i + 1].range.clone();

        // Rule 2, and the boundary it is protecting is one byte wide: an
        // item's range may or may not include the newline that ends it, so the
        // gap is measured from the last **content** byte. `- [ ] \n\ntext`
        // agrees with muya today and must keep agreeing — a blank line closes
        // the item's paragraph in `marked` as well.
        let item_end = trim_trailing_blanks(
            self.src,
            children[i]
                .children
                .last()
                .expect("bare_task_marker found a last item")
                .range
                .end,
        );
        if paragraph_range.start < item_end
            || has_blank_line(&self.src[item_end..paragraph_range.start])
        {
            return false;
        }

        let folded = dedent_continuation(&continuation, indent);
        let text = if ordered {
            // Rule 4: `itemContents` starts at the marker, so the ordered
            // branch's text keeps `"[ ] "` and its trailing blanks.
            format!("{}\n{folded}", &marker_line[indent..])
        } else {
            folded
        };

        children.remove(i + 1);
        let list = &mut children[i];
        list.range.end = list.range.end.max(paragraph_range.end);
        let item = list
            .children
            .last_mut()
            .expect("bare_task_marker found a last item");
        item.range.end = item.range.end.max(paragraph_range.end);
        item.children = vec![Built::leaf(
            Block::Paragraph {
                text: Text::from(text),
            },
            paragraph_range,
        )];

        self.merge_interrupted_list(children, i);
        true
    }

    /// The list the folded paragraph had interrupted, put back.
    fn merge_interrupted_list(&self, children: &mut Vec<Built>, i: usize) {
        if i + 1 >= children.len() || !same_list_kind(&children[i].block, &children[i + 1].block) {
            return;
        }
        // From the last **content** byte, for the reason `trim_trailing_blanks`
        // gives: a paragraph's range includes the newline that ends it, so an
        // untrimmed gap is one `\n` short of the blank line it is looking for.
        let Some(gap_start) = children[i]
            .children
            .last()
            .map(|item| trim_trailing_blanks(self.src, item.range.end))
        else {
            return;
        };
        let Some(gap_end) = children[i + 1]
            .children
            .first()
            .map(|item| item.range.start)
        else {
            return;
        };
        if gap_end < gap_start {
            return;
        }
        let blank = has_blank_line(&self.src[gap_start..gap_end]);
        let next = children.remove(i + 1);
        let next_loose = list_looseness(&next.block);
        let list = &mut children[i];
        list.range.end = list.range.end.max(next.range.end);
        list.children.extend(next.children);
        // `marked` reads looseness off the blank line between two items, and
        // the blank line the port can see is the one this gap holds.
        if let Some(loose) = list_looseness_mut(&mut list.block) {
            *loose = *loose || next_loose || blank;
        }
    }

    /// The marker column and the item's container-stripped first line, when the
    /// last item of `list` is nothing but a task marker on one line.
    fn bare_task_marker(&self, list: &Built) -> Option<(usize, String)> {
        let ordered = match &list.block {
            Block::TaskList { .. } => false,
            Block::OrderList { .. } => true,
            // A bullet list's items are not task items by construction: a bare
            // marker line makes `split_list` produce a `TaskList`.
            _ => return None,
        };
        let item = list.children.last()?;
        // Exactly the empty-container filler, or an item whose only content
        // `pulldown-cmark` consumed into its `TaskListMarker` event.
        if item.children.len() != 1 {
            return None;
        }
        match &item.children[0].block {
            Block::Paragraph { text } if text.to_str().is_empty() => {}
            _ => return None,
        }

        let stripped = stripped_raw(&self.strip(&item.range));
        let mut lines = stripped.split('\n');
        let first = lines.next()?;
        if lines.any(|rest| !rest.trim().is_empty()) {
            return None;
        }
        let column = bare_task_marker_column(first, ordered)?;
        Some((column, first.to_string()))
    }

    /// A block quote's **last** line, when stripping it leaves whitespace
    /// rather than nothing, belongs to the paragraph above it.
    ///
    /// `Tokenizer.blockquote` builds the quote's text as `rtrim(cap[0], '\n')`
    /// with the markers removed, so `">\n> foo\n>  \n"` becomes `"\nfoo\n "` —
    /// and `marked`'s paragraph rule takes that trailing `" "` as a
    /// continuation line, because the lookahead that would refuse it (`| +\n`)
    /// needs a newline the rtrim has already removed. CommonMark, and
    /// therefore `pulldown-cmark`, calls the same line blank and ends the
    /// paragraph before it.
    ///
    /// **Only the last line**, and that is not a simplification: a
    /// whitespace-only line in the *middle* does match `| +\n` and does end
    /// the paragraph, in both engines. `">\n> foo\n>  \n> bar\n"` is two
    /// paragraphs for muya as well.
    fn absorb_trailing_blank_quote_line(&self, frame: &mut Frame) {
        let end = frame.range.end;
        let last = if self.src[..end].ends_with('\n') {
            end - 1
        } else {
            end
        };
        let line = line_start_of(self.src, last.saturating_sub(1).max(frame.range.start));
        let Some(child) = frame.children.last_mut() else {
            return;
        };
        if child.range.end > line {
            return;
        }
        let Block::Paragraph { text } = &mut child.block else {
            return;
        };
        let stripped = strip_lines(self.src, &(line..last), &self.prefixes);
        let Some(only) = stripped.first().filter(|_| stripped.len() == 1) else {
            return;
        };
        if only.text.is_empty() || !only.text.bytes().all(|b| b == b' ' || b == b'\t') {
            return;
        }
        *text = Text::from(format!("{}\n{}", text.to_str(), only.text));
        child.range.end = last;
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
        let paragraph = |range: Range<usize>| self.paragraph(range);
        if !self.options.math {
            return vec![paragraph(frame.range.clone())];
        }
        let logical = LogicalText::of(self.src, frame.range.clone(), &self.prefixes);
        let Some(math) = block_math(&logical.text) else {
            return vec![paragraph(frame.range.clone())];
        };

        let math_end = logical.source_offset(math.consumed);
        let mut out = vec![Built::leaf(
            Block::MathBlock {
                style: MathStyle::Default,
                // `blockKatex`'s tokenizer: `text: match[2].trim()`.
                text: Text::from(logical.text[math.content].trim().to_string()),
            },
            frame.range.start..math_end,
        )];
        if math.consumed < logical.text.trim_end().len() {
            out.push(paragraph(math_end..frame.range.end));
        }
        out
    }

    /// A paragraph, with `marked`'s `Tokenizer.paragraph` text.
    ///
    /// `text` is `cap[1]` minus a trailing newline, where `cap[1]` is the run
    /// of non-blank lines the paragraph rule matched. `pulldown-cmark`'s range
    /// can carry a trailing blank line that `marked`'s `cap[1]` cannot — the
    /// paragraph rule's negative lookahead includes ` +\n` — so trailing blank
    /// lines come off here rather than being sliced in.
    fn paragraph(&self, range: Range<usize>) -> Built {
        let lines = self.strip(&range);
        Built::leaf(
            Block::Paragraph {
                text: Text::from(joined_text(&lines)),
            },
            range,
        )
    }

    fn heading(&self, level: HeadingLevel, range: Range<usize>) -> Built {
        let lines = self.strip(&range);
        // `walkTokens`'s test, transcribed: `/\n {0,3}(=+|-+)/.exec(token.raw)`.
        // Unanchored and on the whole raw, which is what tells atx from setext
        // — an atx heading's raw is one line, so it cannot match.
        let block = match setext_underline(&stripped_raw(&lines)) {
            Some(underline) => Block::SetextHeading {
                level: underline.level(),
                underline,
                // `Tokenizer.lheading`: `cap[1].trim()`, where `cap[1]` is
                // every line above the underline. `trim()` is of the whole
                // string, so the first line loses its indent and the others
                // keep theirs — measured on `   Foo\n   bar\n  ===`, whose
                // text is `Foo\n   bar`.
                text: Text::from(setext_text(&lines)),
            },
            None => Block::AtxHeading {
                level: heading_level(level),
                // **Reconstructed, not sliced** — `${'#'.repeat(depth)} ${text}`.
                // So `##   Foo   ##` is `## Foo`, and a bare `#` is `"# "`
                // with the trailing space, which is measured rather than
                // tidied.
                text: Text::from(atx_text(&lines, heading_level(level))),
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
        // `marked` gives a tight item a run of one-line `text` tokens which the
        // lexer merges with `\n`, and `markdownToState`'s `text` case merges
        // any that survive again with `\n` — so the text is the item's own
        // lines, de-indented, joined. Which is what the stripper produces.
        let built = self.paragraph(range.clone());
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
        let found = scan_definitions(self.src, gap.clone(), &self.prefixes, &mut self.seen_labels);
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
    let items = strip_task_markers(src, items, start.is_some(), loose);
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

/// The task marker comes out of the item's text — except in the one case where
/// `marked` puts it back and nothing takes it off again.
///
/// The mechanism is three passes deep and the net effect is not the obvious
/// one, so it is written out:
///
/// 1. `Tokenizer.list` sets `item.task` from `/^\[[ xX]\] +\S/` and strips
///    `/^\[[ xX]\] +/` off both the item's text and its first token's.
/// 2. It then **puts the marker back** — normalised to exactly one space —
///    when the list is `loose`, by unshifting a `checkbox` token and
///    prepending its raw to `tokens[0].text`.
/// 3. `compatibleTaskList.stripTaskMarker` removes it again — but only in the
///    **bullet** branch. The ordered branch (`token.ordered === true`) never
///    calls it.
///
/// So a loose *ordered* list keeps the marker in its text and every other
/// combination loses it. Measured against the running engine:
///
/// ```text
/// "1. [ ] a\n"              → order-list, list-item, paragraph "a"
/// "1. [ ] a\n\n2. [x] b\n"  → order-list, list-item, paragraph "[ ] a"
/// "- [ ] a\n\n- [x] b\n"    → task-list, task-list-item, paragraph "a"
/// ```
///
/// The marker is only ever removed from the item's **first** child, which is
/// `tokens[0]`, and `listIsTask`'s trailing `\S` guarantees that child is a
/// paragraph.
///
/// # The empty task marker's lazy continuation is upstream of this function
///
/// S3 found `"- [ ] \ntext\n"` disagreeing here and S4 fixed it, but **not in
/// this function** — the disagreement is about which lines are *in the item*,
/// which is decided before any marker is stripped. The fix is
/// [`Builder::fold_empty_task_marker_continuations`], and this function's
/// contribution to it is rule 4: an ordered list keeps the marker in the item's
/// text because `compatibleTaskList`'s ordered branch never calls
/// `stripTaskMarker`, which is the same asymmetry the loose-ordered case above
/// is about.
fn strip_task_markers(src: &str, items: Vec<Built>, ordered: bool, loose: bool) -> Vec<Built> {
    items
        .into_iter()
        .map(|mut item| {
            if !matches!(item.block, Block::TaskListItem { .. }) {
                return item;
            }
            let Some(first) = item.children.first_mut() else {
                return item;
            };
            let Block::Paragraph { text } = &mut first.block else {
                return item;
            };
            let body = text.to_str().into_owned();
            // **Not `let Some(…) else { return }`.** `pulldown-cmark` consumes
            // the marker into its own `TaskListMarker` event, so a loose item's
            // `Paragraph` range often starts *after* it and there is nothing to
            // strip — while the restore below still has to happen. Gating the
            // restore on the strip having fired is the bug that made
            // `1. [ ] a\n\n2. [x] b\n` come out as muya's tight answer.
            let stripped = strip_task_prefix(&body).unwrap_or(&body);
            *text = if ordered && loose {
                // `checkboxToken.raw = taskRaw[0] + ' '`, where `taskRaw` is
                // `/\[[ xX]\]/.exec(item.raw)` — so the marker's own case is
                // kept and its spacing is normalised to one space.
                let marker = task_checkbox(&src[item.range.clone()]).unwrap_or("[ ]");
                Text::from(format!("{marker} {stripped}"))
            } else {
                Text::from(stripped.to_string())
            };
            item
        })
        .collect()
}

/// `compatibleTaskList`'s `EMPTY_TASK_REG` — `/^ {0,3}[*+-][ \t]+\[([ x])\][ \t]*$/i` —
/// widened to the ordered marker `Tokenizer.list` accepts, and answering with
/// the **column of the `[`** rather than with a boolean.
///
/// That column is `marked`'s `indent`, which the continuation loop dedents by,
/// and it is the same number by two different routes: `marked` computes
/// `cap[1].length + line.search(/[^ ]/)` — the bullet plus the run of spaces
/// after it — and that is where the `[` lands.
///
/// `line` is the item's **container-stripped** first line, so a task item
/// inside a block quote measures its column from the quote's content and not
/// from the file.
fn bare_task_marker_column(line: &str, ordered: bool) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut at = 0;
    // ` {0,3}`
    while at < 3 && bytes.get(at) == Some(&b' ') {
        at += 1;
    }
    if ordered {
        let digits = bytes[at..]
            .iter()
            .take(9)
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 || !matches!(bytes.get(at + digits), Some(b'.' | b')')) {
            return None;
        }
        at += digits + 1;
    } else {
        if !matches!(bytes.get(at), Some(b'*' | b'+' | b'-')) {
            return None;
        }
        at += 1;
    }
    // `[ \t]+`
    let spaces = bytes[at..]
        .iter()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    if spaces == 0 {
        return None;
    }
    let column = at + spaces;
    // `\[[ xX]\]`
    if bytes.get(column) != Some(&b'[')
        || !matches!(bytes.get(column + 1), Some(b' ' | b'x' | b'X'))
        || bytes.get(column + 2) != Some(&b']')
    {
        return None;
    }
    // `[ \t]+$`, and the `+` is `TASK_MARKER_PREFIX_REG`'s rather than
    // `EMPTY_TASK_REG`'s. The two regexes differ by exactly this quantifier and
    // the difference decides a real input: `- [ ]` alone is an empty task item
    // (`EMPTY_TASK_REG`, `[ \t]*$`), but `- [ ]\ntext` is **not a task item at
    // all** — `TASK_MARKER_PREFIX_REG` needs a blank after the `]` and there is
    // a newline there, so muya emits a `list-item` whose text is `"[ ]\ntext"`.
    // Only the prefix form can fold, so only the prefix form is spelled here.
    let trailing = &bytes[column + 3..];
    if trailing.is_empty() || trailing.iter().any(|b| !matches!(b, b' ' | b'\t')) {
        return None;
    }
    Some(column)
}

/// `at`, walked back over trailing blanks and newlines.
///
/// A `pulldown-cmark` item range may or may not include the newline that ends
/// it, and the fold's "is there a blank line between these two blocks" question
/// is off by one if it does. Measuring from the last content byte makes the
/// answer independent of that.
fn trim_trailing_blanks(src: &str, at: usize) -> usize {
    src[..at].trim_end_matches([' ', '\t', '\r', '\n']).len()
}

/// The two arms of the continuation loop's one `if`, per line.
///
/// A line whose first non-space character is at or past `indent` is **dedented**
/// by exactly `indent` (`nextLineWithoutTabs.slice(indent)`); a line that does
/// not reach it is appended **verbatim** (`nextLine`), because that arm is
/// paragraph continuation rather than item content. Getting this backwards is
/// the difference between `- [ ] \ntext\n  cont` holding `"text\ncont"` and
/// holding `"text\n  cont"`, and muya holds the first.
fn dedent_continuation(text: &str, indent: usize) -> String {
    text.split('\n')
        .map(|line| {
            let lead = line.len() - line.trim_start_matches(' ').len();
            if lead >= indent {
                &line[indent..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `meta.loose`, for the three list variants that carry it.
fn list_looseness(block: &Block) -> bool {
    match block {
        Block::TaskList { loose, .. }
        | Block::BulletList { loose, .. }
        | Block::OrderList { loose, .. } => *loose,
        _ => false,
    }
}

fn list_looseness_mut(block: &mut Block) -> Option<&mut bool> {
    match block {
        Block::TaskList { loose, .. }
        | Block::BulletList { loose, .. }
        | Block::OrderList { loose, .. } => Some(loose),
        _ => None,
    }
}

/// Whether two lists were one `marked` list before a folded paragraph split
/// them: the same variant, and the same marker or delimiter.
///
/// `- [ ] \ntext\n- b` is deliberately **not** a merge — `compatibleTaskList`
/// splits a bullet list wherever task-ness changes, so a task list followed by
/// a bullet list is what muya emits for it too.
fn same_list_kind(a: &Block, b: &Block) -> bool {
    match (a, b) {
        (
            Block::TaskList { marker: x, .. } | Block::BulletList { marker: x, .. },
            Block::TaskList { marker: y, .. } | Block::BulletList { marker: y, .. },
        ) => std::mem::discriminant(a) == std::mem::discriminant(b) && x == y,
        (Block::OrderList { delimiter: x, .. }, Block::OrderList { delimiter: y, .. }) => x == y,
        _ => false,
    }
}

/// `/^\[[ xX]\] +/` — the prefix, if the text carries one.
fn strip_task_prefix(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('[')?;
    let mut chars = rest.chars();
    let mark = chars.next()?;
    if !matches!(mark, ' ' | 'x' | 'X') || chars.next()? != ']' {
        return None;
    }
    let after = &rest[2..];
    let spaces = after.bytes().take_while(|b| *b == b' ').count();
    (spaces > 0).then(|| &after[spaces..])
}

/// `/\[[ xX]\]/.exec(raw)?.[0]` — the first checkbox anywhere in the item.
fn task_checkbox(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    (0..bytes.len().saturating_sub(2)).find_map(|i| {
        (bytes[i] == b'[' && matches!(bytes[i + 1], b' ' | b'x' | b'X') && bytes[i + 2] == b']')
            .then(|| &raw[i..i + 3])
    })
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
    prefixes: &[Prefix],
    seen: &mut HashSet<String>,
) -> Vec<Built> {
    let logical = LogicalText::of(src, gap, prefixes);
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
            // `markdownToState`'s `def` case is `token.raw.replace(/\n+$/, '')`
            // and `Tokenizer.def`'s raw is already `rtrim(cap[0], '\n')`, so
            // the text is the definition's lines including its own ` {0,3}`
            // indent — which `cap[0]` starts with.
            out.push(Built::leaf(
                Block::Paragraph {
                    text: Text::from(text[at..end].trim_end_matches('\n').to_string()),
                },
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
    raw: &str,
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

    let text = code_text(raw, code_kind, options);

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
                // The `multiplemath` case trims; `_buildCodeState` does not.
                text: Text::from(text.trim().to_string()),
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
                // `_buildCodeState`'s diagram branch uses `value`, the same
                // string the code branch does — no trim.
                text: Text::from(text),
            },
            range.clone(),
        );
    }

    Built::leaf(
        Block::CodeBlock {
            kind: code_kind,
            info,
            fence_len: fence_length(raw, code_kind),
            text: Text::from(text),
        },
        range.clone(),
    )
}

/// `marked`'s `token.text` for a code block, then `markdownToState`'s two
/// post-passes.
///
/// Three rules, not one, and the first is the one §4 C2 measured 263 cases of:
///
/// - **Indented.** `raw.replace(/^(?: {1,4}| {0,3}\t)/gm, '')` — one to four
///   spaces, *or* up to three spaces and a tab, off each line. Then
///   `markdownToState` does `text.replace(/\n$/, '')` because marked ≥17
///   appends a trailing newline that a fenced block does not have.
/// - **Fenced.** The lines between the fences, then `indentCodeCompensation`.
/// - **`trimUnnecessaryCodeBlockEmptyLines`** (muya #1265) — off in both of
///   S1's option sets, so no fixture reaches it; implemented anyway, because a
///   flag that exists and does nothing is the shape this milestone keeps
///   finding out about the hard way.
fn code_text(raw: &str, kind: CodeKind, options: Options) -> String {
    let mut value = match kind {
        CodeKind::Indented => {
            let trimmed = trim_trailing_blank_lines(raw);
            let text = trimmed
                .split('\n')
                .map(strip_code_indent)
                .collect::<Vec<_>>()
                .join("\n");
            // `codeBlockStyle === 'indented' ? text.replace(/\n$/, '') : text`.
            text.strip_suffix('\n').map_or(text.clone(), str::to_string)
        }
        CodeKind::Fenced => fenced_code_text(raw),
    };

    // Fix #1265: `if (trim && (endsWith('\n') || startsWith('\n')))`, then
    // **both** ends come off regardless of which one triggered it.
    if options.trim_unnecessary_code_block_empty_lines
        && (value.ends_with('\n') || value.starts_with('\n'))
    {
        value = value
            .trim_end_matches('\n')
            .trim_start_matches('\n')
            .to_string();
    }
    value
}

/// `/^(?: {1,4}| {0,3}\t)/` off one line — `marked`'s `codeRemoveIndent`.
///
/// **The alternation is ordered and the order is load-bearing.** ` {1,4}` is
/// tried first and is greedy, so `"  \tfoo"` loses its two spaces and keeps the
/// tab; only a line with no leading space at all reaches the ` {0,3}\t` arm,
/// where the `{0,3}` can therefore only ever be zero.
fn strip_code_indent(line: &str) -> &str {
    let bytes = line.as_bytes();
    let spaces = bytes.iter().take_while(|b| **b == b' ').count();
    if spaces > 0 {
        return &line[spaces.min(4)..];
    }
    if bytes.first() == Some(&b'\t') {
        return &line[1..];
    }
    line
}

/// `marked`'s `trimTrailingBlankLines`: drop trailing blank lines but **keep a
/// single one**, because `lines.length - end <= 2` returns the string untouched.
fn trim_trailing_blank_lines(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut end = lines.len() as isize - 1;
    while end >= 0 && lines[end as usize].trim_matches([' ', '\t']).is_empty() {
        end -= 1;
    }
    if lines.len() as isize - end <= 2 {
        return s.to_string();
    }
    lines[..(end + 1) as usize].join("\n")
}

/// `cap[3]` of the fences rule, then `indentCodeCompensation`.
///
/// `cap[3]` is everything between the info line's newline and the newline that
/// precedes the closing fence — so it never ends with a newline, and an
/// unclosed fence simply runs to the end of the block.
fn fenced_code_text(raw: &str) -> String {
    let mut lines: Vec<&str> = raw.split('\n').collect();
    // A trailing newline in `raw` gives a final empty element that is not a
    // line of content.
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let Some(open) = lines.first().copied() else {
        return String::new();
    };
    let indent = open.bytes().take_while(|b| *b == b' ').count();
    let after_indent = &open[indent..];
    let Some(fence_char) = after_indent
        .chars()
        .next()
        .filter(|c| *c == '`' || *c == '~')
    else {
        return String::new();
    };
    let fence_len = after_indent
        .chars()
        .take_while(|c| *c == fence_char)
        .count();

    let is_closing = |line: &str| {
        let i = line.bytes().take_while(|b| *b == b' ').count();
        if i > 3 {
            return false;
        }
        let rest = &line[i..];
        rest.chars().take_while(|c| *c == fence_char).count() >= fence_len
            && rest
                .trim_start_matches(fence_char)
                .trim_start_matches(['`', '~'])
                .chars()
                .all(|c| c == ' ')
    };

    let mut body = &lines[1.min(lines.len())..];
    if body.last().is_some_and(|line| is_closing(line)) {
        body = &body[..body.len() - 1];
    }
    let text = body.join("\n");

    // `indentCodeCompensation(raw, text)` — `/^(\s+)(?:```)/` on the **raw**,
    // so a `~~~` fence never compensates however indented it is. Transcribed,
    // because it is `marked`'s and it is what muya shows.
    if indent == 0 || fence_char != '`' {
        return text;
    }
    text.split('\n')
        .map(|node| {
            let node_indent = node.len() - node.trim_start().len();
            if node_indent >= indent {
                &node[indent..]
            } else {
                node
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
/// **Paid at S4.** `fence_len` was a `u8` (M0's shape), so a fence of more than
/// 255 characters saturated and the serializer would have re-emitted 255 of
/// them — a round-trip loss on ``"`".repeat(300)``. `mt_doc::Block::CodeBlock`
/// carries a `u32` from S4 on, and
/// [`tests::a_fence_longer_than_255_characters_survives_the_widened_field`] is
/// the reproducer §10 owed.
fn fence_length(s: &str, kind: CodeKind) -> Option<u32> {
    if kind != CodeKind::Fenced {
        return None;
    }
    let after_indent = s.trim_start_matches(' ');
    if s.len() - after_indent.len() > 3 {
        return None;
    }
    let fence = after_indent.chars().next()?;
    if fence != '`' && fence != '~' {
        return None;
    }
    let len = after_indent.chars().take_while(|c| *c == fence).count();
    (len > 3).then(|| u32::try_from(len).unwrap_or(u32::MAX))
}

/// `markdownToState`'s `html` case, including the `<img>` special case.
///
/// > TODO: Treat html state which only contains one img as paragraph, we maybe
/// > add image state in the future.
///
/// `/^<img[^<>]+>$/` against the **trimmed** text, so `<img src="x">` is a
/// paragraph and `<img>\n<img>` is an html block.
fn html_block(raw: &str, range: &Range<usize>) -> Built {
    // `Tokenizer.html` sets `text = trimTrailingBlankLines(cap[0])` and the
    // `html` case then trims, so both passes are here.
    let text = trim_trailing_blank_lines(raw).trim().to_string();
    let single_image = text.starts_with("<img")
        && text.ends_with('>')
        && text.len() > 5
        && !text[4..text.len() - 1].contains(['<', '>']);
    let block = if single_image {
        Block::Paragraph {
            text: Text::from(text),
        }
    } else {
        Block::HtmlBlock {
            text: Text::from(text),
        }
    };
    Built::leaf(block, range.clone())
}

// ---------------------------------------------------------------------------
// D2 — the per-kind rules that are not a strip
// ---------------------------------------------------------------------------

/// The stripped lines joined into a paragraph's text.
///
/// `Tokenizer.paragraph`'s `cap[1]` cannot end in a blank line — the rule's
/// negative lookahead includes ` +\n` — but `pulldown-cmark`'s paragraph range
/// can, so trailing blank lines come off here.
///
/// **Only genuinely empty lines**, not whitespace-only ones. `">\n> foo\n>  \n"`
/// strips to `"\nfoo\n "` and muya's paragraph text is `"foo\n "`, trailing
/// space and all — a whitespace-only line is content once its `> ` is gone.
/// # And the four spaces an absorbed code block loses
///
/// `blockTokens`'s `code` branch merges an indented code block into a
/// preceding paragraph with `lastToken.text += '\n' + token.text` — and
/// `token.text` has already had `codeRemoveIndent` applied. So the same four
/// spaces survive at the top level, where the `paragraph` rule swallows the
/// line whole, and vanish wherever `marked` restarts its scan at that line:
///
/// ```text
/// "foo\n    bar\n"                       → "foo\n    bar"   (one paragraph token)
/// "  1.  A paragraph\n    with two lines." → "A paragraph\nwith two lines."
/// "> foo\n    - bar\n"                   → "foo\n- bar"
/// ```
///
/// [`StrippedLine::starts_scan`] is what tells the two apart, and it is set by
/// the container rather than guessed from the indent.
fn joined_text(lines: &[StrippedLine]) -> String {
    let mut end = lines.len();
    while end > 0 && lines[end - 1].text.is_empty() {
        end -= 1;
    }
    let mut out: Vec<&str> = Vec::with_capacity(end);
    let mut in_code = false;
    for (index, line) in lines[..end].iter().enumerate() {
        let text = line.text.as_str();
        if index > 0 && (line.starts_scan || in_code) && is_code_line(text) {
            in_code = true;
            out.push(strip_code_indent(text));
        } else {
            if line.starts_scan {
                in_code = false;
            }
            out.push(text);
        }
    }
    out.join("\n")
}

/// `marked`'s `code` rule head: `(?: {4}| {0,3}\t)[^\n]+`.
fn is_code_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let spaces = bytes.iter().take_while(|b| **b == b' ').count();
    let indent = if spaces >= 4 {
        4
    } else if bytes.get(spaces) == Some(&b'\t') {
        spaces + 1
    } else {
        return false;
    };
    bytes.len() > indent
}

/// `Tokenizer.lheading`: every line above the underline, then `.trim()`.
fn setext_text(lines: &[StrippedLine]) -> String {
    let mut content: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
    // Drop trailing blank lines, then the underline itself.
    while content.last().is_some_and(|l| l.trim().is_empty()) {
        content.pop();
    }
    content.pop();
    content.join("\n").trim().to_string()
}

/// `markdownToState`'s atx branch: `` `${'#'.repeat(+depth)} ${text}` ``, where
/// `text` is `Tokenizer.heading`'s.
///
/// **A rebuild, not a slice**, which is why `##   Foo   ##` is `## Foo` and a
/// bare `#` is `"# "` — the trailing space is muya's answer, measured, and
/// tidying it away would be a divergence with no register entry.
fn atx_text(lines: &[StrippedLine], level: u8) -> String {
    let line = lines.first().map_or("", |l| l.text.as_str());
    // `/^ {0,3}(#{1,6})(?=\s|$)(.*)/` — `cap[2]`, then `.trim()`.
    let indent = line.bytes().take_while(|b| *b == b' ').count().min(3);
    let after_hashes = line[indent..].trim_start_matches('#');
    let mut text = after_hashes.trim().to_string();

    // The closing sequence: `rtrim(text, '#')`, kept only when what is left is
    // empty or ends in a space. CommonMark requires the space, and `marked`
    // implements the requirement by testing for it after the fact.
    if text.ends_with('#') {
        let trimmed = text.trim_end_matches('#');
        if trimmed.is_empty() || trimmed.ends_with(' ') {
            text = trimmed.trim().to_string();
        }
    }
    format!("{} {text}", "#".repeat(level as usize))
}

/// `splitCells`'s per-cell tail: `cells[i].trim().replace(/\\\|/g, '|')`.
///
/// muya's own comment says why the resolved form is what is stored: *"so the
/// editor shows `` `|` `` rather than the escaped `` `\|` `` inside inline
/// code (#4849). `escapeText` re-adds the `\|` escape on serialization."*
fn table_cell_text(raw: &str) -> String {
    raw.trim().replace("\\|", "|")
}

/// `markdownToState`'s frontmatter branch:
/// `text.replace(/^\s+/, '').replace(/\s$/, '')`.
///
/// **The second is `\s$`, one character, not `\s+$`** — so `"a\n\n"` becomes
/// `"a\n"`. Transcribed rather than corrected: it is what muya stores and what
/// the serializer will be asked to round-trip.
fn front_matter_text(text: &str) -> String {
    let leading = text.trim_start();
    let mut out = leading.to_string();
    if out.chars().next_back().is_some_and(char::is_whitespace) {
        out.pop();
    }
    out
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

/// [`block_math`]'s answer: the span of `match[2]`, and `match[0].length`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockMath {
    content: Range<usize>,
    consumed: usize,
}

/// muya's `blockKatex` rule, transcribed:
///
/// ```text
/// /^(\${1,2})\n((?:\\[\s\S]|[^\\])+?)\n\1[ \t]*(?:\n|$)/
/// ```
///
/// Returns `match[2]`'s span and how many bytes of `text` the match consumed,
/// or `None`.
///
/// Two details that fall out of the regex rather than out of intuition:
///
/// - `\${1,2}` is **greedy**, so `$$` is preferred; but `$$$\nx\n$$$` matches
///   nothing, because after backtracking to one `$` the next character is
///   still not a newline.
/// - `(?:\\[\s\S]|[^\\])+?` means a backslash **consumes the next character**,
///   so a `\` immediately before a newline makes that newline part of the
///   content and unable to close the block.
fn block_math(text: &str) -> Option<BlockMath> {
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
                let content = content_start..at;
                match bytes.get(after) {
                    Some(b'\n') => {
                        return Some(BlockMath {
                            content,
                            consumed: after + 1,
                        });
                    }
                    None => {
                        return Some(BlockMath {
                            content,
                            consumed: after,
                        });
                    }
                    _ => {}
                }
            }
            at += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// D2 — the container-prefix stripper
// ---------------------------------------------------------------------------

/// What one enclosing container removes from each line inside it.
///
/// # This is the scanner M2.md §5 D2 calls for, and it is the only one
///
/// D2: *"the prefix stripper is the piece with no counterpart in
/// `pulldown-cmark`. It is the same operation for a block quote and a list
/// item, it composes … and it is the one part of D2 that is a scanner rather
/// than a rewrite. Write it once."*
///
/// S1 shipped two functions that looked like it and were not — `content_start`
/// and a `LogicalText` built on it, written for *detection*: finding a
/// reference definition in a gap, deciding whether a paragraph is really a
/// `$$` math block. Both stripped **all** leading whitespace and **every** `>`
/// run on a line, which is right for "where might a `[` be?" and wrong for
/// "what does this block's text say": a definition's own ` {0,3}` indent is
/// part of `marked`'s `cap[0]` and therefore part of the paragraph's text, and
/// a line carrying two `>` inside a singly-nested quote has one of them as
/// content.
///
/// **They are deleted rather than kept beside this**, and `LogicalText` is
/// re-based on [`strip_lines`]. Promoting them as-is would have been wrong;
/// keeping them beside the real stripper would have been two scanners that
/// agree on the fixtures and drift on the next one. What the detection callers
/// actually needed was the exact answer all along — the def scan is *more*
/// correct for seeing the indent, because that indent is the text it is about
/// to emit.
///
/// # Why a per-line prefix reproduces `marked` at all
///
/// `marked` does not strip per line at leaf level: `Tokenizer.blockquote`
/// removes `/^ {0,3}>[ \t]?/gm` from the whole quote and **re-lexes** the
/// result, and `Tokenizer.list` de-indents each item's raw and re-lexes that.
/// A leaf token's `text` is therefore a slice of an already-stripped string,
/// and the stripping of every level above it was a per-line prefix removal.
/// Composing the levels outermost-first over the original source lines gives
/// the same bytes, and keeps the map back to source offsets that a re-lex
/// throws away (M2.md §10).
///
/// The two places the composition is *not* a pure prefix removal are named on
/// the variants: `expandTabs` inside a list item rewrites content, and it is
/// the only thing that sets [`StrippedLine::exact`] to `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prefix {
    /// A block quote. `marked`'s `blockquoteSetextReplace2`,
    /// `/^ {0,3}>[ \t]?/gm`, applied once per nesting level — a line that does
    /// not match is a lazy continuation and keeps every byte.
    ///
    /// `first_line` is the quote's own first source line, which the
    /// setext-protection substitution needs; see [`setext_protected`].
    Quote { first_line: usize },
    /// A list item, with the three numbers `Tokenizer.list` computes per item.
    Item {
        /// `cap[1].length + indent` — the columns a *continuation* line loses.
        indent: usize,
        /// `cap[1].length` — ` {0,3}` plus the bullet or `1.`, in bytes.
        marker_len: usize,
        /// Where the item's first line starts in the source. That line takes
        /// the tab-stop-aware `expandTabs(rest, cap[1].length)` path; every
        /// other line takes the flat `\t → four spaces` one, which is
        /// `marked`'s asymmetry and not this port's.
        first_line: usize,
    },
}

/// One line of a leaf's text, and where it came from.
#[derive(Debug, Clone)]
struct StrippedLine {
    text: String,
    /// The source offset `text` begins at.
    ///
    /// This is half of the reverse map M2.md §10 owes M4 — see the note there
    /// on why it is half and why the other half is not S2's. It is exact
    /// except on a line where a tab inside a list item was expanded to spaces,
    /// which is the one case where `text` is not a slice of the source.
    source_start: usize,
    /// Whether a `\n` followed this line *inside the range* — so that
    /// [`stripped_raw`] can rebuild `marked`'s `token.raw` newline for newline.
    terminated: bool,
    /// Whether `marked` restarts `blockTokens` at this line, so that `code` is
    /// tried before `paragraph`. See [`joined_text`].
    starts_scan: bool,
}

/// `marked`'s `expandTabs`: tab-stop aware, starting at column `col`.
fn expand_tabs(s: &str, mut col: usize) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\t' {
            let added = 4 - (col % 4);
            for _ in 0..added {
                out.push(' ');
            }
            col += added;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

fn line_start_of(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end_of(src: &str, at: usize) -> usize {
    src[at..].find('\n').map_or(src.len(), |i| at + i)
}

/// Apply one container's prefix to one whole source line.
///
/// Takes and returns `(text, source_start, exact)` so the levels compose.
/// `/^ {0,3}>/` — the line is a quoted line rather than a lazy continuation.
fn is_quoted_line(text: &str) -> bool {
    let bytes = text.as_bytes();
    let i = bytes.iter().take(3).take_while(|b| **b == b' ').count();
    bytes.get(i) == Some(&b'>')
}

/// `blockquoteSetextReplace`: `/\n {0,3}((?:=+|-+) *)(?=\n|$)/g → '\n    $1'`,
/// applied to a quote's raw **before** the `>` markers come off.
///
/// It exists so a lazy continuation line that is nothing but `===` cannot turn
/// the paragraph above it into a setext heading — four spaces make it indented
/// code instead, and the `code` branch of `blockTokens` then folds it back into
/// the paragraph. The four spaces survive into the paragraph's text, which is
/// why this is here and not an implementation detail: `> foo\nbar\n===` has
/// muya's text as `"foo\nbar\n    ==="`.
///
/// The `\n` the pattern starts with is what makes the *first* line of a
/// continuation group ineligible, and `Tokenizer.blockquote`'s loop only ever
/// puts un-quoted lines at the **start** of a group — so the condition is
/// exactly "this line and the one before it are both lazy". Measured:
///
/// ```text
/// "> foo\nbar\n===\n"  → "foo\nbar\n    ==="   (bar is lazy, === follows it)
/// "> foo\n===\n"       → "foo\n==="            (=== starts its own group)
/// "> a\n> b\n===\n"    → "a\nb\n==="           (likewise)
/// ```
fn setext_protected(text: &str) -> Option<String> {
    let indent = text.bytes().take(4).take_while(|b| *b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &text[indent..];
    let marker = rest.chars().next().filter(|c| *c == '=' || *c == '-')?;
    let run = rest.chars().take_while(|c| *c == marker).count();
    if !rest[run..].chars().all(|c| c == ' ') {
        return None;
    }
    // `$1` is `((?:=+|-+) *)`, so the ` {0,3}` the pattern matched is replaced
    // by exactly four spaces rather than added to.
    Some(format!("    {rest}"))
}

fn apply_prefix(
    prefix: Prefix,
    line_start: usize,
    previous: Option<&str>,
    text: String,
    source_start: usize,
    exact: bool,
) -> (String, usize, bool) {
    match prefix {
        Prefix::Quote { first_line } => {
            if !is_quoted_line(&text)
                && line_start > first_line
                && previous.is_some_and(|p| !is_quoted_line(p))
                && let Some(protected) = setext_protected(&text)
            {
                return (protected, source_start, false);
            }
            let bytes = text.as_bytes();
            let mut i = 0;
            while i < 3 && bytes.get(i) == Some(&b' ') {
                i += 1;
            }
            if bytes.get(i) != Some(&b'>') {
                // A lazy continuation line. `/^ {0,3}>[ \t]?/gm` does not match
                // it and `marked` keeps it whole.
                return (text, source_start, exact);
            }
            i += 1;
            if matches!(bytes.get(i), Some(b' ' | b'\t')) {
                i += 1;
            }
            let rest = text[i..].to_string();
            (
                rest,
                if exact {
                    source_start + i
                } else {
                    source_start
                },
                exact,
            )
        }
        Prefix::Item {
            indent,
            marker_len,
            first_line,
        } => {
            if line_start == first_line {
                // `itemContents = expandTabs(cap[2], cap[1].length).slice(indent)`,
                // where `indent` here already includes `cap[1].length`.
                if marker_len >= text.len() {
                    return (String::new(), source_start + text.len(), exact);
                }
                let rest = &text[marker_len..];
                if rest.trim().is_empty() {
                    // `blankLine` — `itemContents` is never assigned and stays
                    // empty, whatever spaces the line held.
                    return (String::new(), source_start + text.len(), exact);
                }
                let cut = indent - marker_len;
                if rest.contains('\t') {
                    let expanded = expand_tabs(rest, marker_len);
                    let sliced = expanded.get(cut..).unwrap_or("").to_string();
                    (sliced, source_start + marker_len, false)
                } else {
                    let sliced = rest.get(cut..).unwrap_or("").to_string();
                    (
                        sliced,
                        if exact {
                            source_start + marker_len + cut.min(rest.len())
                        } else {
                            source_start
                        },
                        exact,
                    )
                }
            } else if text.contains('\t') {
                // `nextLineWithoutTabs = nextLine.replace(/\t/g, '    ')` —
                // **not** tab-stop aware, and applied to the whole line rather
                // than to its indent. Transcribed, not improved.
                let expanded = text.replace('\t', "    ");
                let first_non_space = expanded.find(|c| c != ' ');
                let dedent = first_non_space.is_some_and(|i| i >= indent) || text.trim().is_empty();
                if dedent {
                    (
                        expanded.get(indent..).unwrap_or("").to_string(),
                        source_start,
                        false,
                    )
                } else {
                    // Paragraph continuation: `marked` keeps the *un-expanded*
                    // line here, which is why this arm returns `text`.
                    (text, source_start, exact)
                }
            } else {
                let first_non_space = text.find(|c| c != ' ');
                let dedent = first_non_space.is_some_and(|i| i >= indent) || text.trim().is_empty();
                if dedent {
                    let cut = indent.min(text.len());
                    (
                        text[cut..].to_string(),
                        if exact {
                            source_start + cut
                        } else {
                            source_start
                        },
                        exact,
                    )
                } else {
                    (text, source_start, exact)
                }
            }
        }
    }
}

/// A source range with every enclosing container's prefix removed, line by
/// line.
///
/// Whole source lines are stripped and *then* clipped to `range`, because the
/// prefix is at the start of the line and a range may begin after it — a tight
/// list item's synthetic paragraph starts at its first inline event, which is
/// already past the marker.
fn strip_lines(src: &str, range: &Range<usize>, prefixes: &[Prefix]) -> Vec<StrippedLine> {
    let mut out = Vec::new();
    if range.is_empty() {
        return out;
    }
    let mut at = line_start_of(src, range.start);
    // The previous source line as it looked *before* each prefix was applied.
    // `Prefix::Quote`'s setext protection asks about its own level's previous
    // line, and at level `i` that is `previous[i]`.
    let mut previous: Vec<String> = Vec::new();
    if at > 0 {
        let mut text = src[line_start_of(src, at - 1)..at - 1].to_string();
        let (mut start, mut exact) = (line_start_of(src, at - 1), true);
        for prefix in prefixes {
            previous.push(text.clone());
            let next = apply_prefix(*prefix, start, None, text, start, exact);
            text = next.0;
            start = next.1;
            exact = next.2;
        }
    }

    while at < range.end {
        let end = line_end_of(src, at);
        let mut text = src[at..end].to_string();
        let mut source_start = at;
        let mut exact = true;
        let mut stages: Vec<String> = Vec::with_capacity(prefixes.len());
        for (depth, prefix) in prefixes.iter().enumerate() {
            stages.push(text.clone());
            let next = apply_prefix(
                *prefix,
                at,
                previous.get(depth).map(String::as_str),
                text,
                source_start,
                exact,
            );
            text = next.0;
            source_start = next.1;
            exact = next.2;
        }

        // Where `marked` restarts its block scan, which is what decides
        // whether an indented line is a `code` token or a paragraph line.
        let starts_scan = match prefixes.last() {
            // A list item is lexed with `state.top = false`, so the top-level
            // `paragraph` branch is skipped and `text` matches **one line**.
            // Every line is therefore a fresh scan position.
            Some(Prefix::Item { .. }) => true,
            // `Tokenizer.blockquote` lexes each continuation group with its
            // own `blockTokens` call, and a group starts at the first
            // un-quoted line after a quoted one.
            Some(Prefix::Quote { first_line }) => {
                let depth = prefixes.len() - 1;
                at > *first_line
                    && !is_quoted_line(&stages[depth])
                    && previous.get(depth).is_some_and(|p| is_quoted_line(p))
            }
            None => false,
        };
        previous = stages;

        // Clip to the range. Only the first and last line can need it.
        if exact {
            let mut lo = source_start.max(range.start);
            // **`pulldown-cmark`'s ranges start at the block's first
            // non-whitespace byte and `marked`'s `cap[0]` starts at the line's
            // ` {0,3}` indent**, which is 30 of S2's 47 first-run
            // disagreements: ` ***` as a thematic break, `   foo` as a
            // paragraph, and every indented code block's own indent, without
            // which `codeRemoveIndent` and `indentCodeCompensation` are both
            // measuring from the wrong column.
            //
            // Only whitespace is taken back, so a block that genuinely begins
            // mid-line — a table cell after its `|`, the paragraph a `$$` math
            // block leaves behind — keeps the start it was given.
            if lo > source_start
                && src[source_start..lo]
                    .bytes()
                    .all(|b| b == b' ' || b == b'\t')
            {
                lo = source_start;
            }
            let hi = end.min(range.end).max(lo);
            text = src[lo..hi].to_string();
            source_start = lo;
        }
        out.push(StrippedLine {
            text,
            source_start,
            terminated: end < range.end,
            starts_scan,
        });
        at = end + 1;
    }
    out
}

/// The lines rejoined as `marked`'s `token.raw` — newlines exactly where the
/// source had them inside the range.
fn stripped_raw(lines: &[StrippedLine]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(&line.text);
        if line.terminated {
            out.push('\n');
        }
    }
    out
}

/// A source range with its container syntax removed line by line, plus the map
/// back to source offsets.
///
/// Built from [`strip_lines`], which is D2's stripper — see its header for why
/// there is one scanner here and not two.
struct LogicalText {
    text: String,
    /// `(logical offset, source offset)` at the start of each line.
    lines: Vec<(usize, usize)>,
}

impl LogicalText {
    fn of(src: &str, range: Range<usize>, prefixes: &[Prefix]) -> Self {
        let mut text = String::new();
        let mut lines = Vec::new();
        for line in strip_lines(src, &range, prefixes) {
            lines.push((text.len(), line.source_start));
            text.push_str(&line.text);
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
