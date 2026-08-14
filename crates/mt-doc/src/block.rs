//! The block tree — a 1:1 transcription of `packages/muya/src/state/types.ts`.
//!
//! Every variant below corresponds to exactly one member of the TypeScript
//! `TState` union, and the `name` discriminant string is preserved verbatim so
//! that state JSON emitted by this crate and by `@muyajs/core` compare equal.
//! See [`Block::name`] and the test at the bottom of this file, which is the
//! machine-checked form of that promise.

use crate::document::NodeId;
use crate::text::Text;

/// A block in the document tree.
///
/// Leaf blocks own their text; container blocks own their children. There is
/// no third category — a block that owns both does not exist in muya's model
/// and must not be introduced here.
///
/// # The four constraints that are easy to get wrong
///
/// These are carried from RUST-REWRITE-PLAN.md §2 and from comments in the
/// TypeScript source. Each one has bitten the upstream implementation.
///
/// 1. **[`Block::CodeBlock`]`::info` is the full verbatim info string** — see
///    the field docs.
/// 2. **Reference definitions are not a block type** — see [`Block::Paragraph`].
/// 3. **`fence_len` must be preserved** — see [`Block::CodeBlock`].
/// 4. **The enum maps 1:1 onto the TypeScript union** — see [`Block::name`].
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    // ---- leaves: own text ----
    /// `IParagraphState` — `{ name: 'paragraph', text: string }`.
    ///
    /// **Constraint 2: reference definitions are not a block type.** A line
    /// like `[label]: https://example.com "title"` round-trips as a
    /// `Paragraph` whose `text` is the raw line, exactly as muya does it. A
    /// separate pass over the tree collects those lines into a label map for
    /// the inline lexer (`InlineRenderer.collectReferenceDefinitions` regex-
    /// scans paragraphs to build it).
    ///
    /// `ILinkReferenceDefinitionState` exists in `types.ts` but is marked
    /// `@deprecated`, is unused across the entire TypeScript codebase, and is
    /// slated for removal in muya v0.3. **Do not add a variant for it.**
    /// Doing so would break the 1:1 mapping and desynchronise the differential
    /// harness in §11.2.
    Paragraph { text: Text },

    /// `IAtxHeadingState` — `meta: { level }`. `level` is 1–6.
    AtxHeading { level: u8, text: Text },

    /// `ISetextHeadingState` — `meta: { level, underline }`.
    SetextHeading {
        level: u8,
        underline: Underline,
        text: Text,
    },

    /// `IThematicBreakState`. Carries `text` because the source form
    /// (`---`, `***`, `___`, with arbitrary spacing) round-trips verbatim.
    ThematicBreak { text: Text },

    /// `ICodeBlockState` — `meta: { type, lang, fenceLength? }`.
    CodeBlock {
        /// `meta.type`: `"indented"` or `"fenced"`.
        kind: CodeKind,

        /// **Constraint 1: this is the full verbatim info string, not the
        /// language.**
        ///
        /// Maps to `meta.lang` in TypeScript, whose doc comment
        /// (`packages/muya/src/state/types.ts:34`) is explicit: it holds the
        /// entire fenced info string — `js`, `js title="x"`, or a Pandoc /
        /// RMarkdown `{…}` block. The language used for syntax highlighting
        /// is its **first word**; muya derives it via `firstWordOfInfo()`.
        ///
        /// **Never assume a single word.** Never normalise, trim beyond what
        /// the source had, or lowercase this field — it is re-emitted verbatim
        /// by the serializer, and any change is a round-trip data loss.
        info: String,

        /// **Constraint 3: `fence_len` must be preserved.**
        ///
        /// Maps to `meta.fenceLength`. `None` for indented code blocks. There
        /// is an upstream fix specifically for code-fence-length round-tripping
        /// — a fence opened with ```` ```` ```` (four backticks) must be
        /// re-emitted with four, not normalised to three, or a fence
        /// containing a three-backtick sequence silently breaks.
        ///
        /// # Widened from `u8` at M2 S4, and the reason is the constraint above
        ///
        /// M0 wrote this as `Option<u8>` and M2.md §10's "Owed by S1" recorded
        /// what that costs: ``"`".repeat(300)`` opens a 300-backtick fence,
        /// `marked` carries a JavaScript number, and a `u8` saturates at 255 —
        /// so the serializer would re-emit 255 backticks and the block would no
        /// longer contain its own body. That is precisely the round-trip loss
        /// constraint 3 exists to prevent, in the one shape M0's type could not
        /// express.
        ///
        /// It is `u32` rather than `usize` to match
        /// [`Block::OrderList::start`], which is the other field carrying a
        /// JavaScript number, and because a fence longer than four billion
        /// characters is a document nobody has. No fixture in
        /// `cargo xtask blocks`' 1344 reaches even 256, which is why this
        /// waited for the stage that would have lost the data rather than
        /// being fixed where it was found.
        fence_len: Option<u32>,

        text: Text,
    },

    /// `IHtmlBlockState`. The raw HTML source; sanitization happens at render
    /// or export time, never here.
    HtmlBlock { text: Text },

    /// `IMathBlockState` — `meta: { mathStyle }`.
    MathBlock { style: MathStyle, text: Text },

    /// `IFrontmatterState` — `meta: { lang, style }`.
    Frontmatter {
        lang: FrontmatterLang,
        style: FrontmatterStyle,
        text: Text,
    },

    /// `IDiagramState` — `meta: { lang, type }`.
    ///
    /// Note the field rename: TypeScript's `meta.type` becomes `kind` here
    /// because `type` is a Rust keyword. The serialized JSON key stays
    /// `"type"`.
    Diagram {
        lang: DiagramLang,
        kind: DiagramKind,
        text: Text,
    },

    /// `ITableCellState` — `meta: { align }`. Discriminant is `"table.cell"`.
    TableCell { align: Align, text: Text },

    // ---- containers: own children ----
    /// `IBlockQuoteState`.
    BlockQuote { children: Vec<NodeId> },

    /// `IBulletListState` — `meta: { marker, loose }`.
    BulletList {
        marker: BulletMarker,
        loose: bool,
        children: Vec<NodeId>,
    },

    /// `IOrderListState` — `meta: { start, loose, delimiter }`.
    OrderList {
        start: u32,
        delimiter: OrderDelim,
        loose: bool,
        children: Vec<NodeId>,
    },

    /// `IListItemState`.
    ListItem { children: Vec<NodeId> },

    /// `ITaskListState` — `meta: { marker, loose }`.
    TaskList {
        marker: BulletMarker,
        loose: bool,
        children: Vec<NodeId>,
    },

    /// `ITaskListItemState` — `meta: { checked }`.
    TaskListItem {
        checked: bool,
        children: Vec<NodeId>,
    },

    /// `ITableState`.
    Table { children: Vec<NodeId> },

    /// `ITableRowState`. Discriminant is `"table.row"`.
    TableRow { children: Vec<NodeId> },

    /// `IFootnoteBlockState` — `meta: { identifier }`.
    Footnote {
        identifier: String,
        children: Vec<NodeId>,
    },
}

impl Block {
    /// The `name` discriminant this block serializes to, matching the
    /// TypeScript `TState['name']` union exactly.
    ///
    /// **Constraint 4** lives here. This function is the 1:1 mapping in
    /// executable form, and it is deliberately implemented (rather than left
    /// as `todo!()`) at M0 so the correspondence is locked in and testable
    /// from the first commit — it is the precondition for the differential
    /// harness in §11.2.
    ///
    /// Note the two dotted names: `table.row` and `table.cell`.
    pub fn name(&self) -> &'static str {
        match self {
            Block::Paragraph { .. } => "paragraph",
            Block::AtxHeading { .. } => "atx-heading",
            Block::SetextHeading { .. } => "setext-heading",
            Block::ThematicBreak { .. } => "thematic-break",
            Block::CodeBlock { .. } => "code-block",
            Block::HtmlBlock { .. } => "html-block",
            Block::MathBlock { .. } => "math-block",
            Block::Frontmatter { .. } => "frontmatter",
            Block::Diagram { .. } => "diagram",
            Block::TableCell { .. } => "table.cell",
            Block::BlockQuote { .. } => "block-quote",
            Block::BulletList { .. } => "bullet-list",
            Block::OrderList { .. } => "order-list",
            Block::ListItem { .. } => "list-item",
            Block::TaskList { .. } => "task-list",
            Block::TaskListItem { .. } => "task-list-item",
            Block::Table { .. } => "table",
            Block::TableRow { .. } => "table.row",
            Block::Footnote { .. } => "footnote",
        }
    }

    /// Whether this block owns text (a leaf) rather than children.
    ///
    /// Mirrors the `TLeafState` / `TContainerState` split in `types.ts`.
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            Block::Paragraph { .. }
                | Block::AtxHeading { .. }
                | Block::SetextHeading { .. }
                | Block::ThematicBreak { .. }
                | Block::CodeBlock { .. }
                | Block::HtmlBlock { .. }
                | Block::MathBlock { .. }
                | Block::Frontmatter { .. }
                | Block::Diagram { .. }
                | Block::TableCell { .. }
        )
    }

    /// The block's text, if it is a leaf.
    pub fn text(&self) -> Option<&Text> {
        match self {
            Block::Paragraph { text }
            | Block::AtxHeading { text, .. }
            | Block::SetextHeading { text, .. }
            | Block::ThematicBreak { text }
            | Block::CodeBlock { text, .. }
            | Block::HtmlBlock { text }
            | Block::MathBlock { text, .. }
            | Block::Frontmatter { text, .. }
            | Block::Diagram { text, .. }
            | Block::TableCell { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The block's text, mutably, if it is a leaf.
    ///
    /// The only writer is [`crate::Edit::SpliceText`], which is why this is
    /// crate-private: a leaf's text must not be reachable except through an
    /// edit, or the undo stack stops being a complete record of what changed
    /// (§2.2, and M2.md §5 D9 — "no edit path may bypass `Document::apply`" is
    /// what keeps the CRDT option open).
    pub(crate) fn text_mut(&mut self) -> Option<&mut Text> {
        match self {
            Block::Paragraph { text }
            | Block::AtxHeading { text, .. }
            | Block::SetextHeading { text, .. }
            | Block::ThematicBreak { text }
            | Block::CodeBlock { text, .. }
            | Block::HtmlBlock { text }
            | Block::MathBlock { text, .. }
            | Block::Frontmatter { text, .. }
            | Block::Diagram { text, .. }
            | Block::TableCell { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The block's children, if it is a container.
    pub fn children(&self) -> Option<&[NodeId]> {
        match self {
            Block::BlockQuote { children }
            | Block::BulletList { children, .. }
            | Block::OrderList { children, .. }
            | Block::ListItem { children }
            | Block::TaskList { children, .. }
            | Block::TaskListItem { children, .. }
            | Block::Table { children }
            | Block::TableRow { children }
            | Block::Footnote { children, .. } => Some(children),
            _ => None,
        }
    }

    /// The block's children, mutably, if it is a container.
    ///
    /// Crate-private for the same reason as [`Block::text_mut`]: the tree is
    /// reshaped only by [`crate::Edit`].
    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            Block::BlockQuote { children }
            | Block::BulletList { children, .. }
            | Block::OrderList { children, .. }
            | Block::ListItem { children }
            | Block::TaskList { children, .. }
            | Block::TaskListItem { children, .. }
            | Block::Table { children }
            | Block::TableRow { children }
            | Block::Footnote { children, .. } => Some(children),
            _ => None,
        }
    }

    /// Extract this block's metadata as a [`BlockMeta`], for [`crate::Edit::SetMeta`].
    ///
    /// `None` for the seven variants whose TypeScript interface has no `meta`
    /// object — paragraph, thematic break, html block, block quote, list item,
    /// table and table row. There is nothing to set on those, and returning a
    /// synthetic empty meta would make [`Block::set_meta`] silently accept an
    /// edit that cannot mean anything.
    pub fn meta(&self) -> Option<BlockMeta> {
        Some(match self {
            Block::AtxHeading { level, .. } => BlockMeta::AtxHeading { level: *level },
            Block::SetextHeading {
                level, underline, ..
            } => BlockMeta::SetextHeading {
                level: *level,
                underline: *underline,
            },
            Block::CodeBlock {
                kind,
                info,
                fence_len,
                ..
            } => BlockMeta::CodeBlock {
                kind: *kind,
                info: info.clone(),
                fence_len: *fence_len,
            },
            Block::MathBlock { style, .. } => BlockMeta::MathBlock { style: *style },
            Block::Frontmatter { lang, style, .. } => BlockMeta::Frontmatter {
                lang: *lang,
                style: *style,
            },
            Block::Diagram { lang, kind, .. } => BlockMeta::Diagram {
                lang: *lang,
                kind: *kind,
            },
            Block::TableCell { align, .. } => BlockMeta::TableCell { align: *align },
            Block::BulletList { marker, loose, .. } => BlockMeta::BulletList {
                marker: *marker,
                loose: *loose,
            },
            Block::OrderList {
                start,
                delimiter,
                loose,
                ..
            } => BlockMeta::OrderList {
                start: *start,
                delimiter: *delimiter,
                loose: *loose,
            },
            Block::TaskList { marker, loose, .. } => BlockMeta::TaskList {
                marker: *marker,
                loose: *loose,
            },
            Block::TaskListItem { checked, .. } => BlockMeta::TaskListItem { checked: *checked },
            Block::Footnote { identifier, .. } => BlockMeta::Footnote {
                identifier: identifier.clone(),
            },
            Block::Paragraph { .. }
            | Block::ThematicBreak { .. }
            | Block::HtmlBlock { .. }
            | Block::BlockQuote { .. }
            | Block::ListItem { .. }
            | Block::Table { .. }
            | Block::TableRow { .. } => return None,
        })
    }

    /// Apply a [`BlockMeta`] to this block in place.
    ///
    /// Fails if the meta variant does not match the block variant. Text and
    /// children are untouched, which is the whole point of `SetMeta` existing
    /// beside `ReplaceBlock`: changing a heading's level must not cost the
    /// node its identity, and therefore must not cost `mt-layout` its cache.
    pub fn set_meta(&mut self, meta: BlockMeta) -> Result<(), MetaMismatch> {
        match (self, meta) {
            (Block::AtxHeading { level, .. }, BlockMeta::AtxHeading { level: new }) => {
                *level = new;
            }
            (
                Block::SetextHeading {
                    level, underline, ..
                },
                BlockMeta::SetextHeading {
                    level: new_level,
                    underline: new_underline,
                },
            ) => {
                *level = new_level;
                *underline = new_underline;
            }
            (
                Block::CodeBlock {
                    kind,
                    info,
                    fence_len,
                    ..
                },
                BlockMeta::CodeBlock {
                    kind: new_kind,
                    info: new_info,
                    fence_len: new_fence_len,
                },
            ) => {
                *kind = new_kind;
                *info = new_info;
                *fence_len = new_fence_len;
            }
            (Block::MathBlock { style, .. }, BlockMeta::MathBlock { style: new }) => {
                *style = new;
            }
            (
                Block::Frontmatter { lang, style, .. },
                BlockMeta::Frontmatter {
                    lang: new_lang,
                    style: new_style,
                },
            ) => {
                *lang = new_lang;
                *style = new_style;
            }
            (
                Block::Diagram { lang, kind, .. },
                BlockMeta::Diagram {
                    lang: new_lang,
                    kind: new_kind,
                },
            ) => {
                *lang = new_lang;
                *kind = new_kind;
            }
            (Block::TableCell { align, .. }, BlockMeta::TableCell { align: new }) => {
                *align = new;
            }
            (
                Block::BulletList { marker, loose, .. },
                BlockMeta::BulletList {
                    marker: new_marker,
                    loose: new_loose,
                },
            ) => {
                *marker = new_marker;
                *loose = new_loose;
            }
            (
                Block::OrderList {
                    start,
                    delimiter,
                    loose,
                    ..
                },
                BlockMeta::OrderList {
                    start: new_start,
                    delimiter: new_delimiter,
                    loose: new_loose,
                },
            ) => {
                *start = new_start;
                *delimiter = new_delimiter;
                *loose = new_loose;
            }
            (
                Block::TaskList { marker, loose, .. },
                BlockMeta::TaskList {
                    marker: new_marker,
                    loose: new_loose,
                },
            ) => {
                *marker = new_marker;
                *loose = new_loose;
            }
            (Block::TaskListItem { checked, .. }, BlockMeta::TaskListItem { checked: new }) => {
                *checked = new;
            }
            (
                Block::Footnote { identifier, .. },
                BlockMeta::Footnote {
                    identifier: new_identifier,
                },
            ) => {
                *identifier = new_identifier;
            }
            _ => return Err(MetaMismatch),
        }
        Ok(())
    }

    /// The language to hand to the syntax highlighter for a code block: the
    /// **first word** of [`Block::CodeBlock::info`], never the whole string.
    ///
    /// Mirrors muya's `firstWordOfInfo()` — `info.match(/\S*/)?.[0] ?? ''`,
    /// `utils/index.ts:58` — which takes the run of non-whitespace at position
    /// 0 and is therefore the empty string whenever the info string *starts*
    /// with whitespace, not the first word later in the line. Reproduced
    /// exactly: `" js"` highlights as nothing, and that is muya's answer too.
    ///
    /// Returns `None` for non-code blocks and for code blocks with an empty
    /// info string.
    pub fn highlight_language(&self) -> Option<&str> {
        let Block::CodeBlock { info, .. } = self else {
            return None;
        };
        let first = &info[..info.find(char::is_whitespace).unwrap_or(info.len())];
        (!first.is_empty()).then_some(first)
    }
}

/// Returned by [`Block::set_meta`] when the meta variant does not match the
/// block variant it was applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaMismatch;

/// The per-block metadata payload carried by [`crate::Edit::SetMeta`].
///
/// **Decision not covered by the plan.** §2.2 names `BlockMeta` in the
/// `SetMeta` edit but does not define it. It is modelled here as an enum
/// mirroring the `meta` object of each TypeScript state interface, so that
/// `SetMeta` can change a heading's level or a task item's checked state
/// without going through `ReplaceBlock` (which would discard the block's
/// text and children, and therefore its identity).
///
/// Blocks whose TypeScript interface has no `meta` object — paragraph,
/// thematic break, html block, and every container except lists, task lists,
/// task list items and footnotes — have no variant here, because there is
/// nothing to set.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockMeta {
    AtxHeading {
        level: u8,
    },
    SetextHeading {
        level: u8,
        underline: Underline,
    },
    CodeBlock {
        kind: CodeKind,
        info: String,
        /// See [`Block::CodeBlock::fence_len`] — widened from `u8` at M2 S4.
        fence_len: Option<u32>,
    },
    MathBlock {
        style: MathStyle,
    },
    Frontmatter {
        lang: FrontmatterLang,
        style: FrontmatterStyle,
    },
    Diagram {
        lang: DiagramLang,
        kind: DiagramKind,
    },
    TableCell {
        align: Align,
    },
    BulletList {
        marker: BulletMarker,
        loose: bool,
    },
    OrderList {
        start: u32,
        delimiter: OrderDelim,
        loose: bool,
    },
    TaskList {
        marker: BulletMarker,
        loose: bool,
    },
    TaskListItem {
        checked: bool,
    },
    Footnote {
        identifier: String,
    },
}

// ---------------------------------------------------------------------------
// Meta value types.
//
// muya stores these as strings. They are enums here because the set of legal
// values is closed and documented in `types.ts`, and because an illegal value
// should be a parse error in `mt-md`, not a silent round-trip corruption. Each
// carries its source spelling so serialization is exact.
// ---------------------------------------------------------------------------

/// `ICodeBlockState.meta.type` — `"indented"` | `"fenced"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    Indented,
    Fenced,
}

/// `ISetextHeadingState.meta.underline` — the **literal underline run**.
///
/// # Corrected at M2 S1, against the running engine
///
/// M0 transcribed this as a two-value enum (`Equals` | `Dashes`) from
/// `types.ts:18`, which reads:
///
/// ```text
/// underline: string; // "===" | "---";
/// ```
///
/// The declared type is `string` and the comment is aspirational.
/// `walkTokens` writes `token.marker = /\n {0,3}(=+|-+)/.exec(raw)![1]` — the
/// **whole run** — so muya's state for `Hello world\n===========` carries
/// `underline: "==========="`, and asked directly it does:
///
/// ```text
/// "Foo\n====\n" → { level: 1, underline: "====" }
/// "Foo\n-\n"    → { level: 2, underline: "-" }
/// ```
///
/// A two-value enum can only answer `"==="`, which is a **round-trip data
/// loss** on any document whose underline is not exactly three characters —
/// §10 marks lossless round-trip non-negotiable, so this is a fix rather than
/// a divergence to register. It is not a rare shape either: 25 of the 652
/// CommonMark fixtures and 25 of the 672 GFM ones contain an underline run
/// that is not three characters long.
///
/// The character still decides the level (`=` → 1, `-` → 2), which is why the
/// variants survive; only the length was missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    /// A run of `=`, level 1. The payload is the run's length in characters.
    Equals(u32),
    /// A run of `-`, level 2. The payload is the run's length in characters.
    Dashes(u32),
}

impl Underline {
    /// The heading level this underline implies: 1 for `=`, 2 for `-`.
    pub fn level(self) -> u8 {
        match self {
            Underline::Equals(_) => 1,
            Underline::Dashes(_) => 2,
        }
    }

    /// The run's length in characters.
    ///
    /// Named `run_len` rather than `len` because a `len` without an
    /// `is_empty` is a clippy error, and an `is_empty` here would be a method
    /// that can only answer `false`: a setext underline of zero characters is
    /// not a setext underline.
    pub fn run_len(self) -> u32 {
        match self {
            Underline::Equals(n) | Underline::Dashes(n) => n,
        }
    }

    /// The literal run, which is what `meta.underline` serializes to.
    pub fn to_source(self) -> String {
        let c = match self {
            Underline::Equals(_) => '=',
            Underline::Dashes(_) => '-',
        };
        std::iter::repeat_n(c, self.run_len() as usize).collect()
    }
}

/// `IMathMeta.mathStyle` — `""` | `"gitlab"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathStyle {
    /// `$$…$$`. Serializes to the empty string, not `"default"`.
    Default,
    /// ```` ```math ```` — GitLab compatibility mode.
    Gitlab,
}

/// `IFrontmatterMeta.lang` — `"yaml"` | `"toml"` | `"json"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterLang {
    Yaml,
    Toml,
    Json,
}

impl FrontmatterLang {
    /// The language name this frontmatter dialect serializes to.
    ///
    /// Lives here for the same reason [`DiagramKind::info_lang`] does — see
    /// that method's doc comment for the argument.
    pub fn info_lang(self) -> &'static str {
        match self {
            FrontmatterLang::Yaml => "yaml",
            FrontmatterLang::Toml => "toml",
            FrontmatterLang::Json => "json",
        }
    }
}

/// `IFrontmatterMeta.style` — `"-"` | `"+"` | `";"` | `"{"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterStyle {
    /// `---` delimiters.
    Dash,
    /// `+++` delimiters.
    Plus,
    /// `;;;` delimiters.
    Semicolon,
    /// `{ … }` delimiters.
    Brace,
}

/// `IDiagramMeta.lang` — `"yaml"` | `"json"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramLang {
    Yaml,
    Json,
}

/// `IDiagramMeta.type`.
///
/// `VegaLite` is retained even though §10 drops Vega rendering in v1: the
/// block must still round-trip losslessly as a passthrough code fence and
/// render as source. It must never silently vanish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Mermaid,
    PlantUml,
    VegaLite,
    Flowchart,
    Sequence,
}

impl DiagramKind {
    /// The info-string word this diagram kind was opened with — `mermaid`,
    /// `plantuml`, `vega-lite`, `flowchart`, `sequence`.
    ///
    /// # Why the table lives in `mt-doc` and not next to its callers
    ///
    /// M3.md §5 D11 lays math and diagram blocks out as their own source in
    /// the code-block style *with the language named*, and says the mapping
    /// must **reuse** the one `mt-md` already has rather than introducing a
    /// parallel table. At the point that was written there were three
    /// identical copies inside `mt-md` (`html.rs`, `serialize.rs`,
    /// `state.rs`), and `mt-layout` would have made a fourth.
    ///
    /// [`DiagramKind`] is this crate's type, so a method on it is the one
    /// place all four consumers can reach. It also means `mt-layout` needs no
    /// dependency edge on `mt-md`, which keeps D5's *"`mt-layout` must not
    /// reference `mt_md::reparse`"* true by construction rather than by
    /// inspection.
    ///
    /// The **reverse** direction — info string → `DiagramKind` — deliberately
    /// stays in `mt-md::block`: it also yields a [`DiagramLang`], it is
    /// parser-shaped rather than data-shaped, and only one caller wants it.
    pub fn info_lang(self) -> &'static str {
        match self {
            DiagramKind::Mermaid => "mermaid",
            DiagramKind::PlantUml => "plantuml",
            DiagramKind::VegaLite => "vega-lite",
            DiagramKind::Flowchart => "flowchart",
            DiagramKind::Sequence => "sequence",
        }
    }
}

/// `ITableCellMeta.align` — `"none"` | `"left"` | `"center"` | `"right"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// `IBulletListState.meta.marker` / `ITaskListMeta.marker` — `"-"` | `"+"` | `"*"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletMarker {
    Dash,
    Plus,
    Star,
}

/// `IOrderListState.meta.delimiter` — `"."` | `")"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDelim {
    Period,
    Paren,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Text;

    /// The `name` discriminants of the TypeScript `TState` union, transcribed
    /// from `packages/muya/src/state/types.ts`.
    ///
    /// `link-reference-definition` is deliberately absent: it is a deprecated,
    /// unused stub in the TypeScript source and reference definitions
    /// round-trip as `paragraph`. See [`Block::Paragraph`].
    const TS_STATE_NAMES: &[&str] = &[
        "paragraph",
        "atx-heading",
        "setext-heading",
        "thematic-break",
        "code-block",
        "html-block",
        "math-block",
        "frontmatter",
        "diagram",
        "table.cell",
        "block-quote",
        "order-list",
        "bullet-list",
        "table",
        "task-list",
        "task-list-item",
        "list-item",
        "table.row",
        "footnote",
    ];

    /// One instance of every [`Block`] variant. If a variant is added, this
    /// fails to compile until it is listed — which is the point.
    fn every_variant() -> Vec<Block> {
        let t = || Text::Inline(String::new());
        vec![
            Block::Paragraph { text: t() },
            Block::AtxHeading {
                level: 1,
                text: t(),
            },
            Block::SetextHeading {
                level: 1,
                underline: Underline::Equals(3),
                text: t(),
            },
            Block::ThematicBreak { text: t() },
            Block::CodeBlock {
                kind: CodeKind::Fenced,
                info: String::new(),
                fence_len: Some(3),
                text: t(),
            },
            Block::HtmlBlock { text: t() },
            Block::MathBlock {
                style: MathStyle::Default,
                text: t(),
            },
            Block::Frontmatter {
                lang: FrontmatterLang::Yaml,
                style: FrontmatterStyle::Dash,
                text: t(),
            },
            Block::Diagram {
                lang: DiagramLang::Yaml,
                kind: DiagramKind::Mermaid,
                text: t(),
            },
            Block::TableCell {
                align: Align::None,
                text: t(),
            },
            Block::BlockQuote { children: vec![] },
            Block::BulletList {
                marker: BulletMarker::Dash,
                loose: false,
                children: vec![],
            },
            Block::OrderList {
                start: 1,
                delimiter: OrderDelim::Period,
                loose: false,
                children: vec![],
            },
            Block::ListItem { children: vec![] },
            Block::TaskList {
                marker: BulletMarker::Dash,
                loose: false,
                children: vec![],
            },
            Block::TaskListItem {
                checked: false,
                children: vec![],
            },
            Block::Table { children: vec![] },
            Block::TableRow { children: vec![] },
            Block::Footnote {
                identifier: String::new(),
                children: vec![],
            },
        ]
    }

    /// Constraint 4: the enum maps 1:1 onto the TypeScript union — same
    /// cardinality, same names, no extras, no omissions.
    ///
    /// This is the precondition for the §11.2 differential harness. If it
    /// fails, the two engines can no longer be compared and the harness is
    /// meaningless.
    #[test]
    fn block_names_match_typescript_union_one_to_one() {
        let mut ours: Vec<&str> = every_variant().iter().map(Block::name).collect();
        let mut theirs: Vec<&str> = TS_STATE_NAMES.to_vec();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(
            ours, theirs,
            "Block no longer maps 1:1 onto TState in packages/muya/src/state/types.ts. \
             The §11.2 differential harness depends on this correspondence."
        );
    }

    #[test]
    fn block_names_are_unique() {
        let names: Vec<&str> = every_variant().iter().map(Block::name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate discriminant name");
    }

    /// Constraint 2: there is no reference-definition block variant.
    #[test]
    fn no_link_reference_definition_variant() {
        assert!(
            !every_variant()
                .iter()
                .any(|b| b.name() == "link-reference-definition"),
            "reference definitions must round-trip as Paragraph, not as their own block"
        );
    }

    /// Every variant answers exactly one of [`Block::text`] and
    /// [`Block::children`], and `is_leaf` agrees with which. "A block that
    /// owns both does not exist in muya's model and must not be introduced
    /// here" is the type's own doc comment; this is it in executable form.
    #[test]
    fn every_variant_owns_text_or_children_but_never_both() {
        for block in every_variant() {
            let name = block.name();
            let has_text = block.text().is_some();
            let has_children = block.children().is_some();
            assert!(
                has_text != has_children,
                "{name} owns text={has_text} children={has_children}"
            );
            assert_eq!(has_text, block.is_leaf(), "{name}: is_leaf disagrees");
        }
    }

    /// The twelve variants with a `meta` object round-trip through
    /// [`Block::meta`] and [`Block::set_meta`]; the seven without report
    /// `None`.
    ///
    /// `meta()` returns `Option` rather than `BlockMeta` — a **correction to
    /// the M0 signature**, recorded in M2.md's S0 section. `BlockMeta` has no
    /// variant for a paragraph, deliberately ("there is nothing to set"), so
    /// the original return type was unimplementable for seven of nineteen
    /// variants.
    #[test]
    fn meta_round_trips_for_every_variant_that_has_one() {
        let mut with_meta = 0;
        for mut block in every_variant() {
            let name = block.name();
            let Some(meta) = block.meta() else {
                continue;
            };
            with_meta += 1;
            let before = block.clone();
            assert_eq!(
                block.set_meta(meta),
                Ok(()),
                "{name}: its own meta must apply to it"
            );
            assert_eq!(block, before, "{name}: setting its own meta changed it");
        }
        assert_eq!(with_meta, 12, "twelve variants carry a meta object");
    }

    #[test]
    fn the_seven_variants_without_a_meta_object_report_none() {
        let without: Vec<&str> = every_variant()
            .iter()
            .filter(|b| b.meta().is_none())
            .map(Block::name)
            .collect();
        assert_eq!(
            without,
            vec![
                "paragraph",
                "thematic-break",
                "html-block",
                "block-quote",
                "list-item",
                "table",
                "table.row",
            ]
        );
    }

    #[test]
    fn set_meta_rejects_a_meta_from_a_different_variant() {
        let mut heading = Block::AtxHeading {
            level: 1,
            text: Text::Inline(String::new()),
        };
        assert_eq!(
            heading.set_meta(BlockMeta::TableCell {
                align: Align::Center
            }),
            Err(MetaMismatch)
        );
        assert_eq!(
            heading,
            Block::AtxHeading {
                level: 1,
                text: Text::Inline(String::new())
            },
            "a rejected set_meta must not have half-applied"
        );
    }

    /// `SetMeta` exists so that changing a heading's level does not cost the
    /// node its text — which is what `ReplaceBlock` would cost it.
    #[test]
    fn set_meta_leaves_text_and_children_alone() {
        let mut heading = Block::AtxHeading {
            level: 1,
            text: Text::Inline("Title".to_string()),
        };
        heading
            .set_meta(BlockMeta::AtxHeading { level: 3 })
            .unwrap();
        assert_eq!(
            heading,
            Block::AtxHeading {
                level: 3,
                text: Text::Inline("Title".to_string())
            }
        );
    }

    // --- constraint 1: the info string is not the language -----------------

    /// Constraint 1. `meta.lang` holds the **whole** info string; the
    /// highlight language is its first word. Getting this backwards is the
    /// #4770 data loss, and `codeFenceInfoString.spec.ts` is its regression.
    #[test]
    fn highlight_language_is_the_first_word_of_the_info_string() {
        let code = |info: &str| Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: info.to_string(),
            fence_len: Some(3),
            text: Text::Inline(String::new()),
        };
        assert_eq!(code("js").highlight_language(), Some("js"));
        assert_eq!(code("js title=\"app.js\"").highlight_language(), Some("js"));
        assert_eq!(
            code("{example, listing1-name}").highlight_language(),
            Some("{example,")
        );
        assert_eq!(code("").highlight_language(), None);
    }

    /// muya's `firstWordOfInfo` is `info.match(/\S*/)?.[0]`, which is anchored
    /// at position 0 — so an info string that *starts* with whitespace yields
    /// the empty string rather than the first word after it. Reproduced
    /// rather than improved: the port's highlight language must be muya's, and
    /// this is the input where a "sensible" `split_whitespace().next()` would
    /// differ.
    #[test]
    fn a_leading_space_in_the_info_string_yields_no_language_as_muya_does() {
        let block = Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: " js".to_string(),
            fence_len: Some(3),
            text: Text::Inline(String::new()),
        };
        assert_eq!(block.highlight_language(), None);
    }

    /// The single copy of the diagram-kind table, so that a sixth consumer
    /// added later fails here rather than drifting quietly. The five strings
    /// are the info-string words `mt-md::block`'s parser recognises.
    #[test]
    fn every_diagram_kind_names_the_info_string_it_was_opened_with() {
        let all = [
            (DiagramKind::Mermaid, "mermaid"),
            (DiagramKind::PlantUml, "plantuml"),
            (DiagramKind::VegaLite, "vega-lite"),
            (DiagramKind::Flowchart, "flowchart"),
            (DiagramKind::Sequence, "sequence"),
        ];
        for (kind, lang) in all {
            assert_eq!(kind.info_lang(), lang, "{kind:?}");
        }
    }

    #[test]
    fn every_frontmatter_lang_names_itself() {
        assert_eq!(FrontmatterLang::Yaml.info_lang(), "yaml");
        assert_eq!(FrontmatterLang::Toml.info_lang(), "toml");
        assert_eq!(FrontmatterLang::Json.info_lang(), "json");
    }

    #[test]
    fn highlight_language_is_none_for_every_non_code_block() {
        for block in every_variant() {
            if block.name() == "code-block" {
                continue;
            }
            assert_eq!(
                block.highlight_language(),
                None,
                "{} is not a code block",
                block.name()
            );
        }
    }

    #[test]
    fn leaf_container_split_matches_typescript() {
        // TLeafState in types.ts, minus the deprecated link-reference-definition.
        let leaves: Vec<&str> = every_variant()
            .iter()
            .filter(|b| b.is_leaf())
            .map(Block::name)
            .collect();
        assert_eq!(
            leaves,
            vec![
                "paragraph",
                "atx-heading",
                "setext-heading",
                "thematic-break",
                "code-block",
                "html-block",
                "math-block",
                "frontmatter",
                "diagram",
                "table.cell",
            ]
        );
    }
}
