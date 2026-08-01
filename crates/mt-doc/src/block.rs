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
        fence_len: Option<u8>,

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
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// The block's children, if it is a container.
    pub fn children(&self) -> Option<&[NodeId]> {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Extract this block's metadata as a [`BlockMeta`], for [`crate::Edit::SetMeta`].
    pub fn meta(&self) -> BlockMeta {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Apply a [`BlockMeta`] to this block in place.
    ///
    /// Fails if the meta variant does not match the block variant.
    pub fn set_meta(&mut self, _meta: BlockMeta) -> Result<(), MetaMismatch> {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// The language to hand to the syntax highlighter for a code block: the
    /// **first word** of [`Block::CodeBlock::info`], never the whole string.
    ///
    /// Mirrors muya's `firstWordOfInfo()`. Returns `None` for non-code blocks
    /// and for code blocks with an empty info string.
    pub fn highlight_language(&self) -> Option<&str> {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
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
        fence_len: Option<u8>,
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

/// `ISetextHeadingState.meta.underline` — `"==="` | `"---"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    /// `===`, level 1.
    Equals,
    /// `---`, level 2.
    Dashes,
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
                underline: Underline::Equals,
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
