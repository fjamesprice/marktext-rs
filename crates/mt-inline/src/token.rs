//! The token types — a port of `inlineRenderer/types.ts` (246 lines,
//! 20 interfaces, 26 `type` strings).
//!
//! RUST-REWRITE-PLAN.md §3 sketches a `TokenKind`; M1.md §3 records that the
//! sketch is incomplete in ways that matter and that **`types.ts` is
//! authoritative**. This module is the reconciliation. Every deliberate
//! difference from either document is called out in a doc comment at the point
//! it applies, so a reader of this file never has to hold both plans in their
//! head to know why a field is shaped the way it is.
//!
//! # Offsets are UTF-8 byte offsets (M1.md §5 D1)
//!
//! Every [`Span`] in this module is a pair of **UTF-8 byte offsets into the
//! top-level block text** that was handed to [`crate::tokenizer`] — *not*
//! UTF-16 code-unit indices, which is what muya's `range.start` / `range.end`
//! are. See the crate-root docs for the full statement of the convention and
//! where the conversion lives.
//!
//! Child tokens carry **absolute** spans into that same top-level text, not
//! spans relative to the substring they were tokenized from. That matches
//! muya: `tokenizerFac` is called with an absolute `pos` base for every nested
//! level.
//!
//! # Two structural notes that apply to the whole module
//!
//! **One enum variant per wire `type` string.** muya groups several `type`
//! values into one interface (`BeginRuleToken` covers four, `StrongEmToken`
//! two, `CodeEmojiMathToken` three). This module keeps muya's *payload*
//! grouping — those variants share a payload struct — but gives each wire
//! `type` string its own variant, so [`TokenKind::type_str`] is a total
//! function with no nested dispatch and `match` exhaustiveness lines up with
//! the differential harness's comparison key. §3's sketch splits `Strong`/`Em`
//! the same way; it groups the begin rules, and this module splits those too,
//! for uniformity.
//!
//! **A capture group that the pattern allows to be absent is `Option<Span>`.**
//! JavaScript collapses "group did not participate" and "group matched the
//! empty string" — muya writes `to[3] || ''` in a dozen places — because an
//! absent group has no position to report. Rust can represent the difference,
//! so it does. `None` serializes as `""` at the wire boundary, which keeps the
//! comparison against TypeScript exact while retaining information muya throws
//! away. Groups that always participate are a plain [`Span`].
//!
//! **Four fields are exceptions and serialize as `undefined`**, because muya
//! writes them without the `|| ''`: [`CodeEmojiMath::backlash`]
//! (`lexer.ts:227`), [`ReferenceDefinition::left_title_space`]
//! (`lexer.ts:102`) and — added at S4 — [`HtmlTag::content`] and
//! [`HtmlTag::close_tag`] (`lexer.ts:690` and `:693`). Each is noted where it
//! is declared. They matter only at the wire boundary, and S3 found the first
//! of them by diffing token streams against the running engine.
//!
//! S4 raised the count from two to four rather than finding a new *kind* of
//! exception: `html_tag`'s optional group is `(?:([\s\S]*?)(<\/\3 *>))?`, so
//! an element with no close tag has `content: undefined` and
//! `closeTag: undefined` where the rest of the module would give `''`. The
//! number is in this header because it is the sort of claim that quietly stops
//! being true, and a stage that adds a handler is the stage that should check
//! it.

use std::ops::Range;

// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

/// A half-open range of **UTF-8 byte offsets** into the block text.
///
/// §3 writes these as `Range<usize>`. This is a `Copy` struct instead: token
/// payloads carry up to eleven spans each, `Range` is deliberately not `Copy`,
/// and threading `.clone()` through the tokenizer would obscure the port. The
/// two convert freely in both directions, so the deviation is confined to the
/// declaration site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// An empty span at `at`. The tokenizer uses this where muya produces an
    /// empty string that still has a defined position.
    pub const fn empty_at(at: usize) -> Self {
        Self { start: at, end: at }
    }

    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// The text this span covers.
    ///
    /// # Panics
    ///
    /// If the span is out of bounds or does not fall on `char` boundaries.
    /// The tokenizer only ever produces spans that satisfy both, so a panic
    /// here means the span and the string come from different sources. Use
    /// [`Span::get`] on any path that must not panic.
    pub fn of<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }

    /// The text this span covers, or `None` if it is out of bounds or not on
    /// `char` boundaries.
    pub fn get<'a>(&self, src: &'a str) -> Option<&'a str> {
        src.get(self.start..self.end)
    }

    /// Whether `other` lies entirely within `self`.
    ///
    /// M1.md §4 C3: the tiling invariant holds *per level*, and the
    /// cross-level half of it is that every child span is contained in its
    /// parent's. This is that half.
    pub const fn contains_span(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Whether `offset` falls inside the span, **inclusive of both edges**.
    ///
    /// Inclusive because §3.1's marker-reveal predicate is: a token's markers
    /// reveal when the caret is inside `token.range` *including* its edges.
    pub const fn contains_offset(&self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }
}

impl From<Range<usize>> for Span {
    fn from(r: Range<usize>) -> Self {
        Self {
            start: r.start,
            end: r.end,
        }
    }
}

impl From<Span> for Range<usize> {
    fn from(s: Span) -> Self {
        s.start..s.end
    }
}

// ---------------------------------------------------------------------------
// Highlight
// ---------------------------------------------------------------------------

/// A search highlight intersected with a token's range.
///
/// muya's `IHighlight` is `{ start, end, active: boolean | undefined }`, and
/// `union()` (`utils/index.ts:68`) produces one by intersecting a highlight
/// with a token range, carrying `active` through unchanged. `active` is a
/// genuine tri-state on the TypeScript side — `undefined` means "not the
/// active match", which is distinct from `false` — so it is `Option<bool>`
/// here rather than being flattened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    pub span: Span,
    pub active: Option<bool>,
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/// One inline token.
///
/// # `range` and `raw`
///
/// §3 lists both, and both are kept. In muya they are computed independently —
/// `raw` is the matched *string*, `range` is `{ pos, pos + raw.length }` — and
/// in every one of the sixteen handlers they describe the same extent. That
/// equality is emergent rather than structural, which is exactly why it is
/// worth keeping two fields and asserting `range == raw` in debug builds:
/// D3 site 3 (`state.pending += state.pending + backTo[2]`, `lexer.ts:135`) is
/// a latent bug of precisely the shape that assertion catches, and a text
/// token is the one place the two are accumulated separately.
///
/// Note that `raw` is *not* the same as the number of bytes a handler
/// consumes. M1.md §4 C3 names the three where they differ: `tryBacklash`
/// (`raw` is the `\` alone, consumption is 2), `tryTailHeader` (the trailing
/// whitespace group is left in the input) and `tryAutoLinkExtension` (`raw` is
/// the trimmed extent and the trimmed tail re-enters the loop). All three are
/// implemented as of S5, so the equality `range == raw` that `check_tiling`
/// asserts is now checked against every one of the sixteen handlers rather
/// than against thirteen of them.
///
/// # No `parent`
///
/// Every muya token carries `parent: Token[]`, a back-reference into the array
/// it lives in. M1.md §3: it is a renderer navigation aid, not data; nothing
/// in the tokenizer reads it; Rust has no cheap cyclic back-reference.
/// Dropped. A consumer that needs the parent later gets an index path.
///
/// # Size
///
/// Around 264 bytes, dominated by the largest payloads (`Image`,
/// `ReferenceDefinition`). §3 says correct first, hand-optimize later with the
/// suite as the guard — but for a 5 MB document this is the number §12's
/// memory budget will eventually care about, so it is recorded here rather
/// than rediscovered at M3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// The token's extent. Compared against the caret by §3.1's marker-reveal
    /// predicate and intersected with search highlights.
    pub range: Span,
    /// The exact source slice. Concatenating these over one tokenizer level
    /// reproduces that level's input byte for byte (§3 rule 1, as qualified by
    /// M1.md §4 C3).
    pub raw: Span,
    /// Empty except during a search.
    ///
    /// M1.md §5 D7: §3 puts a `SmallVec<[Highlight; 2]>` on every token; muya
    /// makes the field optional and only allocates on intersection. An inline
    /// capacity of two on every token in a 5 MB document is a measurable cost
    /// for a field that is empty except while the find bar is open, so this is
    /// a plain `Vec` that stays unallocated until the post-pass fills it.
    pub highlights: Vec<Highlight>,
}

impl Token {
    /// muya's `type` string. **This is the differential harness's comparison
    /// key**, so it is the one thing in this module that may never drift from
    /// the TypeScript.
    pub fn type_str(&self) -> &'static str {
        self.kind.type_str()
    }

    /// This token's children, or `None` if it is a leaf.
    ///
    /// `None` and `Some(&[])` are different states, deliberately: an
    /// `html_tag` with no content has `children: undefined` in muya, while one
    /// whose content tokenized to nothing has `children: []`. `[]` is truthy
    /// in JavaScript, so `tokensToPlainText` (`lexer.ts:964`) takes different
    /// branches for the two. Every other container always has a `Vec`.
    pub fn children(&self) -> Option<&[Token]> {
        match &self.kind {
            TokenKind::Strong(e) | TokenKind::Em(e) | TokenKind::Del(e) => Some(&e.children),
            TokenKind::Link(l) => Some(&l.children),
            TokenKind::ReferenceLink(l) => Some(&l.children),
            TokenKind::HtmlTag(t) => t.children.as_deref(),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// TokenKind
// ---------------------------------------------------------------------------

/// The 26 `type` strings of `types.ts`, one variant each.
///
/// Ordered as `types.ts` orders its union, which is also roughly the order the
/// handlers appear in `INLINE_HANDLERS` (`lexer.ts:792`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // --- begin rules: matched only at offset 0 of a top-level tokenize -----
    /// `# `, `## `, … — `beginRules.header`.
    Header(BeginRule),
    /// `***`, `---`, `___` — `beginRules.hr`.
    Hr(BeginRule),
    /// ` ```lang ` — `beginRules.code_fence`.
    CodeFence(BeginRule),
    /// `$$` — `beginRules.multiple_math`. Not GFM; a MarkText extension.
    MultipleMath(BeginRule),
    /// `[label]: href "title"` — `beginRules.reference_definition`.
    ReferenceDefinition(ReferenceDefinition),

    // --- inline rules -----------------------------------------------------
    /// Accumulated plain text, flushed by `pushPending`.
    Text {
        /// Always equal to [`Token::raw`]. Kept because `types.ts` has it and
        /// the differential harness compares fields.
        content: Span,
    },
    /// A backslash escape.
    ///
    /// **Spelled muya's way, on purpose.** muya spells it `backlash`
    /// throughout — the token type, the field name and the rule name — and it
    /// is a typo that is load-bearing, because the differential harness
    /// compares `type` strings. M1.md §3 permits a Rust-side `Backslash` only
    /// if it serializes as `backlash`; using muya's spelling here removes the
    /// chance of getting that wrong.
    ///
    /// muya also carries `content` on this token, which is **always** `''`
    /// (`lexer.ts:130`) — the escaped character goes into `pending` and
    /// surfaces in the *next* text token. It is not modelled; the wire form
    /// emits `content: ""`.
    Backlash {
        /// The `\` itself. Equal to [`Token::raw`]; the handler consumes two
        /// bytes but reports one.
        marker: Span,
    },
    /// `**bold**` / `__bold__`.
    Strong(Emphasis),
    /// `*em*` / `_em_`.
    Em(Emphasis),
    /// `~~struck~~`.
    Del(Emphasis),
    /// `` `code` ``.
    InlineCode(CodeEmojiMath),
    /// `:smile:`.
    Emoji(CodeEmojiMath),
    /// `$a+b$`. Not GFM; a MarkText extension.
    InlineMath(CodeEmojiMath),
    /// `^sup^` and `~sub~`.
    ///
    /// One variant covering both, as in `types.ts`: the marker distinguishes
    /// them and the two rules produce identical token shapes.
    SuperSubScript { marker: Span, content: Span },
    /// `[^note]`.
    FootnoteIdentifier { marker: Span, content: Span },
    /// `![alt](src "title")`.
    Image(Image),
    /// `[anchor](href "title")`.
    Link(Link),
    /// `[anchor][label]` / `[label]`.
    ReferenceLink(ReferenceLink),
    /// `![alt][label]` / `![label]`.
    ReferenceImage(ReferenceImage),
    /// `&amp;` and the other 537 entries of `config/escapeCharacter.ts`.
    HtmlEscape { escape_character: Span },
    /// A bare `https://…`, `www.…` or `a@b.c` — the GFM §6.9 extension.
    AutoLinkExtension(AutoLinkExtension),
    /// `<https://…>` — the CommonMark §6.5 form.
    AutoLink(AutoLink),
    /// Raw HTML: `<span …>…</span>` or `<!-- … -->`.
    HtmlTag(HtmlTag),
    /// A single `\n`.
    SoftLineBreak { line_break: Span, is_at_end: bool },
    /// Two or more spaces then `\n`.
    HardLineBreak {
        spaces: Span,
        line_break: Span,
        is_at_end: bool,
    },
    /// The trailing `#`s of a closed ATX heading.
    TailHeader { marker: Span },
}

impl TokenKind {
    /// muya's `type` string for this variant.
    ///
    /// The differential harness compares these, so they are the port's wire
    /// contract. Note `backlash` and `super_sub_script` in particular.
    pub fn type_str(&self) -> &'static str {
        match self {
            TokenKind::Header(_) => "header",
            TokenKind::Hr(_) => "hr",
            TokenKind::CodeFence(_) => "code_fence",
            TokenKind::MultipleMath(_) => "multiple_math",
            TokenKind::ReferenceDefinition(_) => "reference_definition",
            TokenKind::Text { .. } => "text",
            TokenKind::Backlash { .. } => "backlash",
            TokenKind::Strong(_) => "strong",
            TokenKind::Em(_) => "em",
            TokenKind::Del(_) => "del",
            TokenKind::InlineCode(_) => "inline_code",
            TokenKind::Emoji(_) => "emoji",
            TokenKind::InlineMath(_) => "inline_math",
            TokenKind::SuperSubScript { .. } => "super_sub_script",
            TokenKind::FootnoteIdentifier { .. } => "footnote_identifier",
            TokenKind::Image(_) => "image",
            TokenKind::Link(_) => "link",
            TokenKind::ReferenceLink(_) => "reference_link",
            TokenKind::ReferenceImage(_) => "reference_image",
            TokenKind::HtmlEscape { .. } => "html_escape",
            TokenKind::AutoLinkExtension(_) => "auto_link_extension",
            TokenKind::AutoLink(_) => "auto_link",
            TokenKind::HtmlTag(_) => "html_tag",
            TokenKind::SoftLineBreak { .. } => "soft_line_break",
            TokenKind::HardLineBreak { .. } => "hard_line_break",
            TokenKind::TailHeader { .. } => "tail_header",
        }
    }
}

// ---------------------------------------------------------------------------
// Payloads
// ---------------------------------------------------------------------------

/// `header` | `hr` | `code_fence` | `multiple_math` — muya's `BeginRuleToken`.
///
/// Only `header` and `code_fence` have a `content` group at all, and none of
/// the four has a `backlash` group; muya normalises the absent groups to `''`
/// (`lexer.ts:76-78`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginRule {
    /// Capture 1. The `#`s, the `***`, the backtick run, the `$$`.
    pub marker: Span,
    /// Capture 2 — `header` and `code_fence` only.
    pub content: Option<Span>,
    /// Capture 3 — never present for any of the four begin rules. Modelled so
    /// the struct matches `BeginRuleToken` field for field.
    pub backlash: Option<Span>,
}

/// `[label]: <href> "title"` — eleven fields, one per capture group of
/// `beginRules.reference_definition`, exactly as `types.ts` declares it.
///
/// Gated on `isLengthEven(def[3])` in `consumeBeginRules`.
///
/// `types.ts` declares `leftTitleSpace: string`, but capture 8 sits inside the
/// optional `(?: … )?` group and `lexer.ts:102` reads it without a `|| ''`
/// fallback — so it really can be `undefined` and the TypeScript type is
/// wrong about it. `Option<Span>` records what the regex actually permits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceDefinition {
    /// Capture 1 — up to three spaces and the `[`.
    pub left_bracket: Span,
    /// Capture 2.
    pub label: Span,
    /// Capture 3 — the run of `\` before the `]`.
    pub backlash: Span,
    /// Capture 4 — `]:` and the spaces after it.
    pub right_bracket: Span,
    /// Capture 5 — the optional `<`.
    pub left_href_marker: Span,
    /// Capture 6.
    pub href: Span,
    /// Capture 7 — the optional `>`.
    pub right_href_marker: Span,
    /// Capture 8 — the spaces before the title. Absent when there is no title.
    pub left_title_space: Option<Span>,
    /// Capture 9 — the `"`, `'` or `(` opening the title. Backreferenced as
    /// `\9` by the closing delimiter, which is one of the sixteen rules
    /// M1.md §4 C1 says the `regex` crate cannot express.
    pub title_marker: Option<Span>,
    /// Capture 10.
    pub title: Option<Span>,
    /// Capture 11 — trailing spaces. Outside the optional group, so always
    /// present.
    pub right_title_space: Span,
}

/// `strong` | `em` | `del` — muya's `StrongEmToken` and `DelToken`, which are
/// structurally identical.
///
/// The content group is not stored: it is `raw` minus the markers and the
/// backlash run, and it is already represented by `children`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emphasis {
    /// Capture 1 — `**`, `__`, `*`, `_` or `~~`.
    pub marker: Span,
    /// The content group, tokenized. `tokenizerFac` is entered with the
    /// absolute base `pos + marker.len()`, so these spans are absolute.
    pub children: Vec<Token>,
    /// Capture 3 — the run of `\` before the closing marker. Always present
    /// (the group can match empty) and gated on `isLengthEven`.
    pub backlash: Span,
}

/// `inline_code` | `emoji` | `inline_math` — muya's `CodeEmojiMathToken`.
///
/// `backlash` is `Option` because **only `inline_math` has a third capture
/// group**: `inline_code` is `/^(`{1,3})([^`]+|.{2,})\1/` and `emoji` is
/// `/^(:)([a-z_\d+-]+)\1/`, both two groups. `lexer.ts:199` still gates all
/// three on `isLengthEven(to[3])`, which for those two is
/// `isLengthEven(undefined)` → `isLengthEven('')` → `0 % 2 === 0` → always
/// true. The port must keep that gate vacuous for them rather than "fixing"
/// it into something that can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEmojiMath {
    /// Capture 1 — the backtick run, the `:` or the `$`.
    pub marker: Span,
    /// Capture 2.
    pub content: Span,
    /// Capture 3 — `inline_math` only.
    ///
    /// **The one field whose wire form is `undefined` rather than `""`.**
    /// This module's header says an absent optional group serializes as `""`,
    /// because muya writes `to[3] || ''` in a dozen places — but not here:
    /// `lexer.ts:227` writes a bare `backlash: to[3]`, so an `inline_code` or
    /// `emoji` token really does carry `undefined`. Found at S3 by diffing
    /// token streams against the running engine, where a dumper that assumed
    /// the general rule reported ten false disagreements.
    /// [`ReferenceDefinition::left_title_space`] is the only other exception.
    pub backlash: Option<Span>,
}

/// The `{ first, second }` backslash-run pair carried by links and images.
///
/// `second` is absent for a shortcut reference link or image — the
/// `(?:\[([^\]]*?)(\\*)\])?` group did not participate — and always present
/// for `image` and `link`, whose capture 5 sits outside any optional group.
/// muya writes `rLinkTo[4] || ''` at the two reference sites and passes
/// `imageTo[5]` / `linkTo[5]` straight through at the other two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BacklashPair {
    pub first: Span,
    pub second: Option<Span>,
}

/// The render-facing attribute bag on an `image` token.
///
/// These are `String`, not `Span`, and that is the one place in this module
/// where a field genuinely cannot be a source slice: `lexer.ts:330` builds
/// them as `src + encodeURI(backlash.second)` and `alt + encodeURI(...)`.
/// **Percent-encoding produces bytes that are not in the source.** The bare
/// `src` / `alt` / `title` fields on [`Image`] are the unencoded slices;
/// M1.md §3 flags the difference explicitly.
///
/// `types.ts` gives this an index signature (`[key: string]: string`), but
/// only these three keys are ever written for an `image` token —
/// `getAttributes` and its open-ended attribute map belong to `html_tag`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageAttrs {
    pub src: String,
    pub title: String,
    pub alt: String,
}

/// `![alt](src "title")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Capture 1 — `![`.
    pub marker: Span,
    /// Capture 2, unencoded.
    pub alt: Span,
    /// Capture 4 — destination and title together, before `parseSrcAndTitle`
    /// splits them. `correctUrl` may have rewritten it to stop at the matching
    /// `)`.
    pub src_and_title: Span,
    /// `parseSrcAndTitle(src_and_title).src` — a sub-slice of `src_and_title`.
    pub src: Span,
    /// `parseSrcAndTitle(src_and_title).title`, `None` when no quoted title
    /// was recognised. muya returns `''` for both "no title" and "empty
    /// title", and its `if (title)` test then treats them identically, so
    /// `None` is faithful to the behaviour as well as to the shape.
    pub title: Option<Span>,
    /// Captures 3 and 5.
    pub backlash: BacklashPair,
    /// The percent-encoded copies the renderer uses. See [`ImageAttrs`].
    pub attrs: ImageAttrs,
}

/// `[anchor](href "title")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Capture 1 — `[`.
    pub marker: Span,
    /// Capture 2 — the link text, verbatim.
    pub anchor: Span,
    /// Capture 4 — destination and title together, possibly rewritten by
    /// `correctUrl`.
    pub href_and_title: Span,
    /// `parseSrcAndTitle(href_and_title).src`.
    pub href: Span,
    /// `parseSrcAndTitle(href_and_title).title`. See [`Image::title`].
    pub title: Option<Span>,
    /// The anchor, tokenized, based at `pos + marker.len()`.
    pub children: Vec<Token>,
    /// Captures 3 and 5.
    pub backlash: BacklashPair,
}

/// `[anchor][label]`, `[anchor][]` or `[label]`.
///
/// There is no `marker` field — `types.ts` does not declare one, and the
/// handler tokenizes children from `pos + 1` rather than from a captured
/// marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceLink {
    /// `!!rLinkTo[3]` — true only when the `[label]` part is present **and
    /// non-empty**, so the collapsed form `[anchor][]` is not a full link.
    pub is_full_link: bool,
    /// Capture 1 — the text between the first pair of brackets.
    pub anchor: Span,
    /// Capture 3 when non-empty, otherwise capture 1. Looked up in
    /// [`crate::Labels`] lowercased, per CommonMark §6.5.
    pub label: Span,
    /// The anchor, tokenized, based at `pos + 1`.
    pub children: Vec<Token>,
    /// Captures 2 and 4.
    pub backlash: BacklashPair,
}

/// `![alt][label]`, `![alt][]` or `![label]`.
///
/// No `children`: a reference image's alt text is not tokenized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceImage {
    pub is_full_link: bool,
    /// Capture 1.
    pub alt: Span,
    /// Capture 3 when non-empty, otherwise capture 1.
    pub label: Span,
    /// Captures 2 and 4.
    pub backlash: BacklashPair,
}

/// Which alternative of `gfmRules.auto_link_extension` matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoLinkKind {
    /// `www.example.com`
    Www,
    /// `https://example.com`
    Url,
    /// `user@example.com`
    Email,
}

impl AutoLinkKind {
    /// muya's `linkType` string.
    pub fn as_str(self) -> &'static str {
        match self {
            AutoLinkKind::Www => "www",
            AutoLinkKind::Url => "url",
            AutoLinkKind::Email => "email",
        }
    }
}

/// A bare autolink — GFM §6.9.
///
/// Exactly one of the three spans is `Some`, and `link_type` says which;
/// `types.ts` carries all four fields and the differential harness compares
/// them, so all four are kept rather than collapsed into one span.
///
/// For `Www` and `Url`, the span is the extent **after** `trimAutoLinkExtent`
/// has run, which is a prefix of the regex match.
///
/// `Email` is *documented* as never trimmed, because `lexer.ts:592` guards the
/// trim with `if (!email)` — but S5 established that the guard is
/// **unobservable**: an email match always ends with an alphanumeric and can
/// never contain a `<`, so no rule the trim owns could fire on one anyway. The
/// guard is reproduced regardless, and `lexer.rs`'s
/// `the_email_alternatives_extent_would_be_unchanged_by_the_trim_anyway`
/// proves the claim rather than restating it.
///
/// # S0 wrote these three types and S5 is the first code to construct one
///
/// They were written from `types.ts` five stages before anything filled them
/// in, so the shape was a prediction. It held: all four fields are needed
/// (`types.ts` carries them and the differential harness compares them),
/// [`AutoLinkExtension::target`] and [`AutoLinkKind::as_str`] were both the
/// right accessors, and nothing had to change. Worth saying because the
/// opposite happened at S4 — `HtmlTag::attrs` carried an `Option` that could
/// not be `None` and it was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoLinkExtension {
    pub link_type: AutoLinkKind,
    /// Capture 1.
    pub www: Option<Span>,
    /// Capture 2.
    pub url: Option<Span>,
    /// Capture 3.
    pub email: Option<Span>,
}

impl AutoLinkExtension {
    /// The one span that participated.
    pub fn target(&self) -> Option<Span> {
        match self.link_type {
            AutoLinkKind::Www => self.www,
            AutoLinkKind::Url => self.url,
            AutoLinkKind::Email => self.email,
        }
    }
}

/// `<https://example.com>` or `<user@example.com>` — CommonMark §6.5.
///
/// muya also stores `marker: '<'`, a literal that is always the byte at
/// `raw.start`. Not modelled; the wire form emits `"<"`.
///
/// **`href` is the literal source between the angle brackets, never
/// re-encoded** — that is the tokenizer half of marktext #3548. The renderer's
/// `encodeURI` was the bug; the token must hand it through untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoLink {
    /// Capture 1 — the scheme-qualified URL.
    pub href: Option<Span>,
    /// Capture 2 — the bare email address.
    pub email: Option<Span>,
    /// `!!autoLTo[1]` — a link rather than an email.
    pub is_link: bool,
}

/// The `tag` field of an `html_tag` token.
///
/// The comment branch of `tryHtmlTag` (`lexer.ts:653`) sets `tag` to the
/// literal `'<!---->'`, which is not a slice of the source — hence an enum
/// rather than a `Span`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlTagName {
    /// `<!-- … -->`. Reports as `<!---->`.
    Comment,
    /// Capture 3 of `html_tag`, verbatim and un-lowercased.
    Name(Span),
}

/// Raw HTML — `commonMarkRules.html_tag`.
///
/// `content`, `close_tag` and `children` are all `Option` because the comment
/// branch omits them entirely while the element branch sets `children` to `[]`
/// when there is no content. The distinction is load-bearing:
/// `tokensToPlainText` (`lexer.ts:964`) tests `if (token.children)` and `[]`
/// is truthy in JavaScript, so an element with empty children yields `''`
/// while a comment falls through to its `content` branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlTag {
    pub tag: HtmlTagName,
    /// Capture 1 for a comment, capture 2 for an element.
    pub open_tag: Span,
    /// Capture 5.
    pub close_tag: Option<Span>,
    /// Capture 4.
    pub content: Option<Span>,
    /// `getAttributes(raw)` — the whitelisted attributes of the tag.
    ///
    /// `String`, not `Span`: muya routes this through `DOMParser`, which
    /// ASCII-lowercases names and resolves character references in values, so
    /// neither is generally a source slice. A duplicate attribute keeps the
    /// first.
    ///
    /// # There is no `Option` on the value, and S4 is where that was settled
    ///
    /// This was declared `Vec<(String, Option<String>)>` until S4, with `None`
    /// meaning a valueless attribute — `<input disabled>` — on the reading
    /// that `getAttribute` reports those as `null`. **It does not.**
    /// `getAttributes` iterates `getAttributeNames()`, so every name it asks
    /// for is present, and a present-but-valueless attribute has the value
    /// `""` in the DOM; `null` means *absent*. Measured against happy-dom:
    /// `<input disabled>` gives `{"disabled": ""}`. So the inner `Option` was
    /// uninhabited, and a type that cannot be `None` should not be an
    /// `Option` — it makes every reader ask what `None` means and every writer
    /// wrap a value that is never absent. Removed, along with the test that
    /// asserted the distinction.
    ///
    /// A `Vec` rather than a map because insertion order is observable:
    /// `getAttributeNames()` returns document order, and `getAttributes`
    /// pre-seeds `title`/`src`/`alt` for `IMG` before overwriting them, which
    /// a JavaScript object preserves as first-insertion order.
    ///
    /// M1.md §5 D2 decides how this is produced without a DOM; see
    /// [`crate::html`]. It does not affect this shape.
    pub attrs: Vec<(String, String)>,
    /// Capture 4, tokenized, based at `pos + open_tag.len()`.
    pub children: Option<Vec<Token>>,
}

impl HtmlTag {
    /// The value of `name`, or `None` if the attribute is absent.
    ///
    /// A valueless attribute is `Some("")`, not `None` — see [`Self::attrs`].
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_are_byte_offsets_not_utf16_code_units() {
        // The whole point of D1, in one assertion. "中" is three UTF-8 bytes
        // and one UTF-16 code unit; muya would report `end: 1` here.
        let src = "中文";
        let span = Span::new(0, 3);
        assert_eq!(span.of(src), "中");
        assert_eq!(span.len(), 3);
    }

    #[test]
    fn a_span_off_a_char_boundary_is_none_rather_than_a_panic() {
        assert_eq!(Span::new(0, 1).get("中"), None);
    }

    #[test]
    fn containment_is_inclusive_at_both_edges() {
        // §3.1: markers reveal when the caret is inside the range *including*
        // its edges, so both endpoints must count.
        let span = Span::new(2, 5);
        assert!(span.contains_offset(2));
        assert!(span.contains_offset(5));
        assert!(!span.contains_offset(6));
    }

    #[test]
    fn child_containment_is_the_cross_level_tiling_check() {
        let parent = Span::new(0, 10);
        assert!(parent.contains_span(Span::new(2, 5)));
        assert!(parent.contains_span(parent));
        assert!(!parent.contains_span(Span::new(8, 12)));
    }

    /// The differential harness compares these strings. All 26, spelled the
    /// way muya spells them — including the `backlash` typo.
    #[test]
    fn every_type_string_matches_muya() {
        let s = Span::new(0, 0);
        let begin = || BeginRule {
            marker: s,
            content: None,
            backlash: None,
        };
        let emphasis = || Emphasis {
            marker: s,
            children: Vec::new(),
            backlash: s,
        };
        let chunk = || CodeEmojiMath {
            marker: s,
            content: s,
            backlash: None,
        };
        let backlash = || BacklashPair {
            first: s,
            second: None,
        };

        let cases: Vec<(TokenKind, &str)> = vec![
            (TokenKind::Header(begin()), "header"),
            (TokenKind::Hr(begin()), "hr"),
            (TokenKind::CodeFence(begin()), "code_fence"),
            (TokenKind::MultipleMath(begin()), "multiple_math"),
            (
                TokenKind::ReferenceDefinition(ReferenceDefinition {
                    left_bracket: s,
                    label: s,
                    backlash: s,
                    right_bracket: s,
                    left_href_marker: s,
                    href: s,
                    right_href_marker: s,
                    left_title_space: None,
                    title_marker: None,
                    title: None,
                    right_title_space: s,
                }),
                "reference_definition",
            ),
            (TokenKind::Text { content: s }, "text"),
            (TokenKind::Backlash { marker: s }, "backlash"),
            (TokenKind::Strong(emphasis()), "strong"),
            (TokenKind::Em(emphasis()), "em"),
            (TokenKind::Del(emphasis()), "del"),
            (TokenKind::InlineCode(chunk()), "inline_code"),
            (TokenKind::Emoji(chunk()), "emoji"),
            (TokenKind::InlineMath(chunk()), "inline_math"),
            (
                TokenKind::SuperSubScript {
                    marker: s,
                    content: s,
                },
                "super_sub_script",
            ),
            (
                TokenKind::FootnoteIdentifier {
                    marker: s,
                    content: s,
                },
                "footnote_identifier",
            ),
            (
                TokenKind::Image(Image {
                    marker: s,
                    alt: s,
                    src_and_title: s,
                    src: s,
                    title: None,
                    backlash: backlash(),
                    attrs: ImageAttrs::default(),
                }),
                "image",
            ),
            (
                TokenKind::Link(Link {
                    marker: s,
                    anchor: s,
                    href_and_title: s,
                    href: s,
                    title: None,
                    children: Vec::new(),
                    backlash: backlash(),
                }),
                "link",
            ),
            (
                TokenKind::ReferenceLink(ReferenceLink {
                    is_full_link: false,
                    anchor: s,
                    label: s,
                    children: Vec::new(),
                    backlash: backlash(),
                }),
                "reference_link",
            ),
            (
                TokenKind::ReferenceImage(ReferenceImage {
                    is_full_link: false,
                    alt: s,
                    label: s,
                    backlash: backlash(),
                }),
                "reference_image",
            ),
            (
                TokenKind::HtmlEscape {
                    escape_character: s,
                },
                "html_escape",
            ),
            (
                TokenKind::AutoLinkExtension(AutoLinkExtension {
                    link_type: AutoLinkKind::Url,
                    www: None,
                    url: Some(s),
                    email: None,
                }),
                "auto_link_extension",
            ),
            (
                TokenKind::AutoLink(AutoLink {
                    href: Some(s),
                    email: None,
                    is_link: true,
                }),
                "auto_link",
            ),
            (
                TokenKind::HtmlTag(HtmlTag {
                    tag: HtmlTagName::Comment,
                    open_tag: s,
                    close_tag: None,
                    content: None,
                    attrs: Vec::new(),
                    children: None,
                }),
                "html_tag",
            ),
            (
                TokenKind::SoftLineBreak {
                    line_break: s,
                    is_at_end: false,
                },
                "soft_line_break",
            ),
            (
                TokenKind::HardLineBreak {
                    spaces: s,
                    line_break: s,
                    is_at_end: false,
                },
                "hard_line_break",
            ),
            (TokenKind::TailHeader { marker: s }, "tail_header"),
        ];

        assert_eq!(
            cases.len(),
            26,
            "types.ts declares 20 interfaces carrying 26 distinct `type` strings"
        );
        for (kind, expected) in &cases {
            assert_eq!(kind.type_str(), *expected);
        }
    }

    #[test]
    fn an_html_tag_distinguishes_absent_children_from_empty_children() {
        // `[]` is truthy in JavaScript; `undefined` is not. tokensToPlainText
        // takes different branches, so the port may not conflate them.
        let leaf = |children| Token {
            kind: TokenKind::HtmlTag(HtmlTag {
                tag: HtmlTagName::Name(Span::new(1, 4)),
                open_tag: Span::new(0, 5),
                close_tag: None,
                content: None,
                attrs: Vec::new(),
                children,
            }),
            range: Span::new(0, 5),
            raw: Span::new(0, 5),
            highlights: Vec::new(),
        };
        assert_eq!(leaf(None).children(), None);
        assert_eq!(leaf(Some(Vec::new())).children(), Some(&[][..]));
    }

    /// The replacement for `a_valueless_attribute_is_not_an_empty_one`, which
    /// asserted a distinction the DOM does not make — see [`HtmlTag::attrs`].
    /// What is left is the distinction that *is* real: absent versus present.
    #[test]
    fn an_absent_attribute_is_not_an_empty_one() {
        let tag = HtmlTag {
            tag: HtmlTagName::Name(Span::new(1, 6)),
            open_tag: Span::new(0, 20),
            close_tag: None,
            content: None,
            attrs: vec![
                ("disabled".to_string(), String::new()),
                ("class".to_string(), "x".to_string()),
            ],
            children: None,
        };
        assert_eq!(tag.attr("disabled"), Some(""));
        assert_eq!(tag.attr("class"), Some("x"));
        assert_eq!(tag.attr("id"), None);
    }
}
