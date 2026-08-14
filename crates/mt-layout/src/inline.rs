//! Per-leaf inline layout: tokenize, hide the markers, and record where the
//! visible text came from — M3 §4 C6 and §5 D13.
//!
//! # What this module answers
//!
//! MarkText is WYSIWYG. A markdown marker is shown only when a caret is near
//! it, and `mt_inline::marker` implements exactly that predicate as a pure
//! function of `(token.range, cursor)`. **A read-only viewer has no caret**, so
//! [`marker_state`] answers `Hidden` for every token in every leaf and the
//! string handed to parley is not the block's text: `**bold**` is eight bytes
//! of block text and four of visible text.
//!
//! So this module produces four things from one walk of `mt_inline`'s token
//! tree:
//!
//! 1. the **visible string**, which is what gets shaped;
//! 2. a list of [`InlineRun`]s over it, saying which spans are strong, emphatic,
//!    code, link text and so on — [`crate::flow`] resolves those against the
//!    theme into the style runs parley is given;
//! 3. the [`VisibleTextMap`], D13's public run list, which is how any caller
//!    gets from a shaped offset back to a block-text offset;
//! 4. a list of [`InlineImage`]s — where a replaced box goes and what its `src`
//!    is, with no size, because D12 puts the size above this seam.
//!
//! # C6's premise is wrong in one word, and D13 corrects it
//!
//! C6 assumes markers are *deleted*, so that the visible string is a
//! subsequence of the block text and the map is a list of surviving subranges.
//! **Some kinds substitute rather than delete** — see [`MapKind::Substituted`]
//! — so a subrange list cannot express the result and a map that treats a
//! substitution as a deletion puts a caret inside a character. Hence a run list
//! whose runs carry a stored [`MapKind`] rather than one inferred from the two
//! lengths, because a substitution whose two sides happen to be the same length
//! is indistinguishable from a copy by arithmetic alone.
//!
//! # The gap rule
//!
//! Rather than reconstruct each token kind's marker arithmetic from its payload
//! spans, the walk states the invariant once: **a token's bytes that are not
//! covered by its content are its markers.** For a container that means the
//! bytes between its children; for `` `code` `` it means the bytes outside
//! `content`. That is one rule instead of sixteen, it cannot drift from
//! `mt-inline`'s handlers, and it picks up the backslash runs (`Emphasis`'s
//! third capture group, the two halves of a [`BacklashPair`](mt_inline::BacklashPair))
//! without naming them.

use std::ops::Range;

use mt_inline::{
    Cursor, Labels, MarkerState, Span, Token, TokenKind, TokenizerOptions, escape_character,
    marker_state, marker_state_of_range, tokenizer,
};

use crate::text::BaseDirection;

// ---------------------------------------------------------------------------
// The map — D13
// ---------------------------------------------------------------------------

/// What happened to a stretch of block text on its way into the visible string.
///
/// **Stored on every [`MapRun`] rather than inferred from the two lengths.**
/// D13's argument: a substitution whose source and result are the same length
/// — `&#38;` is not, but a hypothetical one-byte entity would be — is
/// indistinguishable from a copy by arithmetic, and a caller that guessed wrong
/// would offset a caret into the middle of a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapKind {
    /// The bytes are in the visible string unchanged, so an offset inside the
    /// run maps across by simple addition.
    Copied,
    /// The visible bytes are **different bytes** from the block bytes, and
    /// neither length constrains the other. One kind does this:
    ///
    /// - `html_escape` — `&amp;` becomes one `&`
    ///   (muya draws it as `content: attr(data-character)`).
    ///
    /// Two more were expected here and are not:
    ///
    /// - a **line break** does not become a space. `.mu-content` is
    ///   `white-space: pre-wrap` (`blockSyntax.css:948`), so the `\n` is a
    ///   break rather than a collapsible space, and `exportStyle.css:92-105`
    ///   says so in as many words — *"render soft line breaks the way the
    ///   editor does (`.mu-content` is pre-wrap) … yet still shows the break
    ///   (#3676)"*. It is [`Copied`](Self::Copied);
    /// - `emoji` **would** become one emoji glyph, and does not here, because
    ///   `mt-inline` ships no shortcode table and `mt-layout` will not invent
    ///   one. A shortcode is [`Copied`](Self::Copied) whole, colons included —
    ///   see the `Emoji` arm of the walk. **If a table ever arrives, this is
    ///   the variant that gains a second member**, and the run list is already
    ///   shaped for it.
    ///
    /// An offset *inside* such a run has no meaningful partner on the other
    /// side, so both directions answer with the run's start.
    ///
    /// A second producer arrived with D12 and is not a rendering of anything:
    /// an image that leads a right-to-left paragraph substitutes its bytes for
    /// one [`RTL_MARK`], so that M3-R1's workaround costs no new variant and
    /// the run list keeps tiling both sides.
    Substituted,
    /// A marker. The bytes are in the block text and nowhere in the visible
    /// string, so [`MapRun::visible`] is empty and
    /// [`VisibleTextMap::to_visible`] answers `None` for every byte of it.
    Hidden,
}

/// One run of D13's visible-text ↔ block-text map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRun {
    /// The run's extent in the visible string. **Empty** exactly when
    /// [`kind`](Self::kind) is [`MapKind::Hidden`].
    pub visible: Range<usize>,
    /// The run's extent in the block's own text. Never empty.
    pub block: Range<usize>,
    /// The **whole range of the token this run came from**, which always
    /// contains [`block`](Self::block).
    ///
    /// Carried because D13's property test — *"every visible offset maps back
    /// to a block-text offset whose token contains it"* — is only checkable if
    /// the token range survives the walk. A map built by accumulating lengths
    /// can only agree with itself.
    ///
    /// For a marker run this is the enclosing token (the whole `**bold**` for
    /// the `**`); for a copied run it is the token that owns the content (the
    /// inner `text`, not the `strong` around it).
    pub token: Range<usize>,
    /// Which of the three things happened.
    pub kind: MapKind,
}

/// The map from one leaf's visible text back to its block text — **D13**.
///
/// Public API rather than an internal detail, because §10 owes it forward to
/// M4's caret and M5's search: nothing can place a click, a hit region or a
/// search highlight without it, and re-deriving it means re-tokenizing.
///
/// # The two directions are not symmetric, and this type does not pretend
///
/// [`to_block`](Self::to_block) is **total**: every visible offset came from
/// somewhere. [`to_visible`](Self::to_visible) returns `Option` and answers
/// `None` for any byte inside a hidden marker, because a hidden marker has no
/// visible position at all. **It does not clamp to the nearest visible
/// offset** — a clamped answer is a lie the caller cannot detect, and a search
/// that matches inside `**` needs to know that is what happened rather than
/// receive the offset of the `b` next to it. Callers that want snapping can
/// snap; this does not do it for them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisibleTextMap {
    runs: Vec<MapRun>,
    visible_len: usize,
    block_len: usize,
}

impl VisibleTextMap {
    /// A map for a leaf that was not tokenized — a code block, a line-number
    /// gutter, a list marker — where the visible text *is* the block text.
    ///
    /// One copied run, so callers never have to special-case the absence of a
    /// map to get the identity.
    pub fn identity(len: usize) -> VisibleTextMap {
        VisibleTextMap {
            runs: if len == 0 {
                Vec::new()
            } else {
                vec![MapRun {
                    visible: 0..len,
                    block: 0..len,
                    token: 0..len,
                    kind: MapKind::Copied,
                }]
            },
            visible_len: len,
            block_len: len,
        }
    }

    /// The runs, in ascending order of both offsets.
    ///
    /// They **tile the block text**: `runs[i].block.end == runs[i + 1].block.start`,
    /// the first starts at 0 and the last ends at [`block_len`](Self::block_len).
    /// The visible side tiles too, except that hidden runs contribute nothing.
    pub fn runs(&self) -> &[MapRun] {
        &self.runs
    }

    /// Length of the visible string, in bytes.
    pub fn visible_len(&self) -> usize {
        self.visible_len
    }

    /// Length of the block's own text, in bytes.
    pub fn block_len(&self) -> usize {
        self.block_len
    }

    /// Visible offset → block offset. **Total.**
    ///
    /// An offset inside a substituted run answers with the run's block start,
    /// because the visible bytes of `&amp;`'s `&` do not correspond one for one
    /// with anything.
    ///
    /// # Panics
    ///
    /// Debug builds only, if `visible` is past the end of the visible string.
    /// Release builds answer [`block_len`](Self::block_len), which is the
    /// end-of-text answer and the only one that keeps this total.
    pub fn to_block(&self, visible: usize) -> usize {
        debug_assert!(
            visible <= self.visible_len,
            "visible offset {visible} is past the end of a {} byte visible string",
            self.visible_len
        );
        // The first run that has not already ended at or before `visible`.
        // `visible.end` is non-decreasing across runs, so this is a partition.
        // Hidden runs have an empty visible range and are skipped by the same
        // test, which is why the predicate is `<=` and not `<`.
        let index = self.runs.partition_point(|r| r.visible.end <= visible);
        match self.runs.get(index) {
            Some(run) if run.kind == MapKind::Copied => {
                run.block.start + (visible - run.visible.start)
            }
            Some(run) => run.block.start,
            None => self.block_len,
        }
    }

    /// Block offset → visible offset, or `None` if that byte is inside a hidden
    /// marker. **Partial, and deliberately so** — see the type's own note.
    pub fn to_visible(&self, block: usize) -> Option<usize> {
        if block >= self.block_len {
            return Some(self.visible_len);
        }
        // Block ranges are non-empty and contiguous, so exactly one run
        // contains `block`.
        let index = self.runs.partition_point(|r| r.block.end <= block);
        let run = self.runs.get(index)?;
        match run.kind {
            MapKind::Copied => Some(run.visible.start + (block - run.block.start)),
            MapKind::Substituted => Some(run.visible.start),
            MapKind::Hidden => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Style runs — the semantic half
// ---------------------------------------------------------------------------

/// The inline styling in force over a stretch of visible text.
///
/// Semantic rather than resolved on purpose: this module owns no theme, so it
/// says *"this is strong"* and [`crate::flow`] says what a strong run's weight
/// is. Everything nests, which is why these are independent flags and not an
/// enum — bold inside italic is one run that is both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InlineStyle {
    /// Inside `**` / `__`.
    pub strong: bool,
    /// Inside `*` / `_`.
    pub em: bool,
    /// Inside `~~`. Drawn as a rule at the face's own strikeout position and
    /// thickness, because muya has no `line-through` rule anywhere and a
    /// browser's UA `<del>` reads `OS/2` for both numbers.
    pub del: bool,
    /// Inside `` ` ``. Takes the theme's inline-code stack and its `0.8em`.
    pub code: bool,
    /// Inside a link, reference link or autolink — the anchor text, never the
    /// href, which is a marker and hidden with the rest. Underlined from the
    /// face's `post` table, for [`del`](Self::del)'s reason.
    pub link: bool,
    /// A footnote identifier's own text, which is drawn smaller.
    pub footnote: bool,
    /// The source between a pair of `$`s. M3 has no TeX renderer, so what is
    /// drawn is the source — and `.mu-math` gives it `font-family: monospace`
    /// and `color: var(--editor-color)` of its own
    /// (`inlineSyntax.css:165-173`), which is *not* the surrounding text's
    /// family or colour.
    pub math: bool,
    /// The word between the colons of a shortcode `mt-inline` has no table for.
    ///
    /// The reference's own state for a shortcode it cannot resolve:
    /// `emoji.ts:15-16` picks `.mu-warn` over the hide class, and
    /// `.mu-warn.mu-emoji-marked-text` is `color: var(--delete-color)`
    /// (`inlineSyntax.css:106-109`). The colons themselves are
    /// `span.mu-warn.mu-emoji-marker`, which no rule in the sheet matches, so
    /// they stay the surrounding colour — hence the flag is on the content and
    /// not on the whole token.
    pub emoji_unresolved: bool,
    /// The character an entity decoded to. `.mu-html-escape::before` sets
    /// `color: var(--editor-color)` (`inlineSyntax.css:139-143`), so an entity
    /// inside a blockquote is the editor's colour and not the quote's — the
    /// same rule inline code follows.
    pub html_escape: bool,
    /// A reference definition's punctuation — `[`, `]: href "`, the closing
    /// quote. `.mu-reference-marker`: `--editor-color-50` at `0.9em`
    /// (`inlineSyntax.css:578-581`).
    pub reference_marker: bool,
    /// A reference definition's label. `.mu-reference-label`: `font-weight:
    /// 600`, and no font-size, so it stays at `1em`
    /// (`inlineSyntax.css:589-593`).
    pub reference_label: bool,
    /// A reference definition's title. `.mu-reference-title`: `0.9em`
    /// (`inlineSyntax.css:583-587`).
    pub reference_title: bool,
    /// `.mu-gray` — `--editor-color-30` (`inlineSyntax.css:1-4`). A marker the
    /// caret has revealed, and the backslash run inside a reference
    /// definition, which `referenceDefinition.ts:70` gives this class
    /// unconditionally.
    pub gray: bool,
}

/// One run of uniformly styled **visible** text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRun {
    /// Byte range in the visible string.
    pub range: Range<usize>,
    /// What is in force over it.
    pub style: InlineStyle,
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// An inline image found by the walk, and where its box belongs — **D12**.
///
/// Deliberately carries no width and no height. `mt-inline`'s `Image` token has
/// no dimension of any kind, markdown has nowhere to put one, and this module
/// owns no theme and may not open a file. The size is resolved one layer up
/// from [`ImageSizes`](crate::images::ImageSizes), which the shell builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    /// Byte offset into [`InlineText::visible`] at which the box sits.
    ///
    /// Always on a `char` boundary, and normally at the offset the token's own
    /// hidden bytes reached. The one exception is the RTL workaround — see
    /// [`RTL_MARK`].
    pub visible_index: usize,
    /// The token's own `src`, as it appears in the source.
    ///
    /// The **unencoded** slice and not `ImageAttrs::src`, which is
    /// `encodeURI`'d: the shell resolves this against a directory, and a path
    /// with a space in it would never match its own percent-encoded spelling.
    ///
    /// Empty is a real state — `![alt]()` — and D12 gives it the same geometry
    /// as a failed load under a different class name.
    pub src: String,
    /// The token's extent in the block's own text, which is also the box's
    /// [`id`](crate::display::InlineBoxItem::id) once flow has it.
    pub block: Range<usize>,
}

/// U+200F RIGHT-TO-LEFT MARK, inserted before an inline box that would
/// otherwise sit at offset 0 of a right-to-left paragraph — **M3-R1's
/// workaround**.
///
/// # What is actually wrong upstream
///
/// parley assigns an inline box the bidi level of the shaped run *before* it
/// (`parley/src/shape/mod.rs:204-208`), and for a box before the **first** run
/// there is no previous run, so it takes `BidiLevel::new(0)` — level 0, not the
/// paragraph's level. Upstream's own TODO on those lines says what the right
/// answer is: the box should be analysed as a U+FFFC object replacement
/// character and take *that* character's level. In a paragraph at base level 1,
/// a leading U+FFFC resolves to level 1 and the box belongs at the right-hand
/// end of the line; parley puts it at the left.
///
/// # Why a right-to-left mark fixes it
///
/// The mark is a strong R character, so it shapes into a run of its own at
/// level 1 and the box is no longer before the first run. It then takes that
/// run's level — 1 — which is the level the U+FFFC would have had. The mark is
/// default-ignorable and contributes no advance, so the line is unchanged
/// apart from the box moving to the end it belongs at.
///
/// # Reachability, stated rather than implied
///
/// The condition is a **paragraph** at base level 1, and `mt-layout` lays every
/// paragraph out at [`BaseDirection::Ltr`] today for the reference-parity
/// reason [`TextRequest::new`](crate::TextRequest::new) argues at length — so
/// nothing in `bench/corpus/` triggers it and no golden moves. `rtl.md` carries
/// the construct anyway, because the day M5 exposes the reference's own
/// two-valued `dir` preference is the day this fires, and a workaround with no
/// input is indistinguishable from a workaround that does not work.
pub const RTL_MARK: &str = "\u{200f}";

/// One leaf's text, tokenized and reduced to what a reader sees.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineText {
    /// The string handed to parley. **Not the block's text.**
    pub visible: String,
    /// Style runs over [`visible`](Self::visible), merged so that adjacent runs
    /// never carry the same [`InlineStyle`], and tiling it completely.
    pub runs: Vec<InlineRun>,
    /// D13's map.
    pub map: VisibleTextMap,
    /// The images that need a box reserved, in document order.
    pub images: Vec<InlineImage>,
}

/// The tokenizer settings `mt-layout` uses.
///
/// # Why these are not read from the document
///
/// The two flags are `mt_md::Options`' `footnote` and `super_sub_script`, and
/// `mt-layout` may not depend on `mt-md` (D5 forbids the edge and S7 asserts
/// its absence), so they arrive on [`LayoutOptions`](crate::LayoutOptions)
/// instead — the same shape as `xtask`'s own `PARSE_OVERRIDES`, which is where
/// the one corpus input that turns footnotes on already lives.
///
/// # `labels` is the same shape and needs no new edge
///
/// A reference form is a reference form only if its label resolves, and the
/// label table is collected per *document*. It does **not** follow that
/// `mt-layout` has to call `mt_md::labels` to get one:
/// [`TokenizerOptions::labels`] is typed [`mt_inline::Labels`] — `mt-inline`'s
/// own type, which `mt_md::labels` merely *populates*. `mt_md::parse` hands
/// back `Parsed { document, labels, source_map }` and §3 already has the viewer
/// holding all three, so the shell passes the table down exactly the way D7 has
/// it pass a font collection down. No dependency edge, no `mt-md` reference,
/// and `[text][ref]` resolves.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineSyntax {
    /// `[^note]` — `mt_md::Options::footnote`, which `MUYA_DEFAULT` sets
    /// **off**.
    pub footnote: bool,
    /// `^sup^` and `~sub~` — `mt_md::Options::super_sub_script`, which
    /// `MUYA_DEFAULT` sets **off**.
    ///
    /// Unreachable twice over at M3 and recorded as such in §6: the option is
    /// off, and no corpus input matches either rule even with it on. The arm
    /// lays it out as an ordinary run because the reference ships no CSS for
    /// `<sup>`/`<sub>` at all, so there is no MarkText number to be faithful to
    /// and parley has no vertical-align (#208) to be faithful with.
    pub super_sub_script: bool,
    /// The document's reference definitions, keyed lowercase — the thing that
    /// separates `[text][ref]` from four literal brackets.
    ///
    /// Empty is a legitimate value (a document that defines nothing), not a
    /// stub. The walk reads it a second time for [`TokenKind::ReferenceImage`],
    /// which names a label but carries no `href` of its own.
    pub labels: Labels,
}

impl InlineSyntax {
    fn options(&self) -> TokenizerOptions {
        let mut options = TokenizerOptions::muya_default();
        // Left **on**, which is what makes an ATX heading's `# ` a marker this
        // hides rather than three glyphs it draws.
        options.has_begin_rules = true;
        options.syntax.footnote = self.footnote;
        options.syntax.super_sub_script = self.super_sub_script;
        // One clone per leaf. The map is per *document* and small — the whole
        // twelve-file corpus defines three reference definitions between them
        // — so this is cheaper than threading a prebuilt `TokenizerOptions`
        // through a signature that would then name an `mt-inline` type.
        options.labels.clone_from(&self.labels);
        options
    }
}

/// Tokenize one leaf's text and reduce it to what a reader sees.
///
/// `cursor` is the caret, and `None` — a read-only viewer — hides every marker.
/// It is a parameter rather than a constant because M4's editor is the whole
/// reason `mt_inline::marker` exists, and because a hard-coded `None` would
/// make the reveal path unreachable *and* untested.
///
/// `base_direction` is the paragraph's, and it is here for exactly one reason:
/// [`RTL_MARK`], which is a change to the *visible string* and so cannot live
/// anywhere the string has already been built.
pub fn lay_out(
    text: &str,
    syntax: &InlineSyntax,
    base_direction: BaseDirection,
    cursor: Option<Cursor>,
) -> InlineText {
    let tokens = tokenizer(text, &syntax.options());
    let mut walk = Walk {
        src: text,
        labels: &syntax.labels,
        base_direction,
        cursor,
        out: InlineText {
            visible: String::with_capacity(text.len()),
            runs: Vec::new(),
            map: VisibleTextMap::default(),
            images: Vec::new(),
        },
    };
    walk.tokens(&tokens, InlineStyle::default());
    let mut out = walk.out;
    out.map.visible_len = out.visible.len();
    out.map.block_len = text.len();
    out
}

struct Walk<'a> {
    src: &'a str,
    labels: &'a Labels,
    base_direction: BaseDirection,
    cursor: Option<Cursor>,
    out: InlineText,
}

impl Walk<'_> {
    // --- primitives ------------------------------------------------------

    fn copy(&mut self, span: Span, token: Span, style: InlineStyle) {
        if span.start >= span.end {
            return;
        }
        let start = self.out.visible.len();
        self.out.visible.push_str(span.of(self.src));
        let end = self.out.visible.len();
        self.push_run(start..end, style);
        self.out.map.runs.push(MapRun {
            visible: start..end,
            block: span.start..span.end,
            token: token.start..token.end,
            kind: MapKind::Copied,
        });
    }

    fn substitute(&mut self, with: &str, span: Span, token: Span, style: InlineStyle) {
        if span.start >= span.end {
            return;
        }
        let start = self.out.visible.len();
        self.out.visible.push_str(with);
        let end = self.out.visible.len();
        self.push_run(start..end, style);
        self.out.map.runs.push(MapRun {
            visible: start..end,
            block: span.start..span.end,
            token: token.start..token.end,
            kind: MapKind::Substituted,
        });
    }

    fn hide(&mut self, span: Span, token: Span) {
        if span.start >= span.end {
            return;
        }
        let at = self.out.visible.len();
        self.out.map.runs.push(MapRun {
            visible: at..at,
            block: span.start..span.end,
            token: token.start..token.end,
            kind: MapKind::Hidden,
        });
    }

    /// A marker span: hidden when the caret is away from its token, and
    /// ordinary text when it is not.
    ///
    /// `state` is passed in rather than computed here because the reference
    /// does not always test the token's own range — an ATX heading tests a
    /// **synthetic** range covering the `#`s alone (`header.ts:17`), which is
    /// what `marker_state_of_range` is for.
    fn marker(&mut self, span: Span, token: Span, state: MarkerState, style: InlineStyle) {
        match state {
            MarkerState::Hidden => self.hide(span, token),
            // Unreachable with `cursor: None`, and styled anyway: a revealed
            // marker is `.mu-gray` — `--editor-color-30`,
            // `inlineSyntax.css:1-4` — in every renderer that reveals one, so
            // the arm that M4 will reach carries the reference's colour rather
            // than the surrounding text's.
            MarkerState::Revealed => self.copy(
                span,
                token,
                InlineStyle {
                    gray: true,
                    ..style
                },
            ),
        }
    }

    fn push_run(&mut self, range: Range<usize>, style: InlineStyle) {
        if range.is_empty() {
            return;
        }
        match self.out.runs.last_mut() {
            Some(last) if last.style == style && last.range.end == range.start => {
                last.range.end = range.end;
            }
            _ => self.out.runs.push(InlineRun { range, style }),
        }
    }

    // --- the gap rule ----------------------------------------------------

    /// Walk `children`, hiding every byte of `token` they do not cover.
    ///
    /// The whole of the marker handling for every container kind — `strong`,
    /// `em`, `del`, `link`, `reference_link` — plus the backslash runs those
    /// kinds carry between the last child and the closing marker.
    fn container(&mut self, token: &Token, children: &[Token], style: InlineStyle) {
        let state = marker_state(token, self.cursor);
        let mut at = token.range.start;
        for child in children {
            self.marker(Span::new(at, child.range.start), token.range, state, style);
            self.token(child, style);
            at = child.range.end;
        }
        self.marker(Span::new(at, token.range.end), token.range, state, style);
    }

    /// An image: no visible text, a box reserved at the offset its bytes
    /// reached, and D12's one deliberate exception to "hidden means nothing in
    /// the visible string".
    fn image(&mut self, token: &Token, src: String, style: InlineStyle) {
        let state = marker_state(token, self.cursor);
        // The workaround, written as the one condition it is rather than as a
        // constant somewhere downstream: *a box that would be the first thing
        // in a right-to-left paragraph*. See `RTL_MARK` for the upstream
        // defect, and for why nothing in the corpus reaches this today.
        let would_lead_an_rtl_paragraph = state == MarkerState::Hidden
            && self.out.visible.is_empty()
            && self.base_direction == BaseDirection::Rtl;
        if would_lead_an_rtl_paragraph {
            // A substitution and not an insertion: the mark stands *for* the
            // image's bytes, so the map still tiles both sides and D13 needs no
            // fourth `MapKind`. An offset inside it answers with the image's
            // block start, which is the same answer the hidden run gave.
            self.substitute(RTL_MARK, token.range, token.range, style);
        } else {
            self.marker(token.range, token.range, state, style);
        }
        self.out.images.push(InlineImage {
            visible_index: self.out.visible.len(),
            src,
            block: token.range.start..token.range.end,
        });
    }

    /// The same rule for a childless token whose reader-facing text is one
    /// span: `` `code` ``, `$math$`, `:emoji:`, `^sup^`, `[^note]`, `<autolink>`.
    fn wrapped(&mut self, token: &Token, content: Span, style: InlineStyle) {
        let state = marker_state(token, self.cursor);
        self.marker(
            Span::new(token.range.start, content.start),
            token.range,
            state,
            style,
        );
        self.copy(content, token.range, style);
        self.marker(
            Span::new(content.end, token.range.end),
            token.range,
            state,
            style,
        );
    }

    // --- the walk --------------------------------------------------------

    fn tokens(&mut self, tokens: &[Token], style: InlineStyle) {
        for token in tokens {
            self.token(token, style);
        }
    }

    fn token(&mut self, token: &Token, style: InlineStyle) {
        match &token.kind {
            // --- content ---------------------------------------------------
            TokenKind::Text { content } => self.copy(*content, token.range, style),

            // --- emphasis, which nests -------------------------------------
            TokenKind::Strong(e) => self.container(
                token,
                &e.children,
                InlineStyle {
                    strong: true,
                    ..style
                },
            ),
            TokenKind::Em(e) => {
                self.container(token, &e.children, InlineStyle { em: true, ..style })
            }
            TokenKind::Del(e) => {
                self.container(token, &e.children, InlineStyle { del: true, ..style })
            }

            // --- inline code, math, sup/sub, footnote id -------------------
            TokenKind::InlineCode(c) => self.wrapped(
                token,
                c.content,
                InlineStyle {
                    code: true,
                    ..style
                },
            ),
            // No TeX renderer at M3 (D11 is the block-level half of the same
            // answer), so the source between the `$`s is the content and the
            // `$`s are markers like any other. The style is `.mu-math`'s own
            // and not the surrounding paragraph's: `font-family: monospace;
            // color: var(--editor-color)` (`inlineSyntax.css:165-173`) is on
            // the element that *wraps* the KaTeX render, so it is in force
            // whether or not there is a render inside it.
            TokenKind::InlineMath(c) => self.wrapped(
                token,
                c.content,
                InlineStyle {
                    math: true,
                    ..style
                },
            ),
            TokenKind::SuperSubScript { content, .. } => self.wrapped(token, *content, style),
            TokenKind::FootnoteIdentifier { content, .. } => self.wrapped(
                token,
                *content,
                InlineStyle {
                    footnote: true,
                    ..style
                },
            ),

            // --- the emoji shortcode, and the reference has a state for it --
            //
            // The reference draws `:smile:` as one glyph through
            // `content: attr(data-emoji)` (`inlineSyntax.css:91-100`), and
            // **`mt-inline` has no shortcode table** — no name-to-codepoint
            // map exists anywhere in the crate — so that branch is out of
            // reach and inventing the table here would be inventing data.
            //
            // What is *not* out of reach is the branch muya takes when its own
            // lookup misses. `emoji.ts:15-16` reads
            // `validEmoji(token.content)` and, on a miss, uses `.mu-warn`
            // **in place of** the hide class — so the markers and the word are
            // both drawn, the word at `--delete-color`
            // (`inlineSyntax.css:106-109`) and the colons at whatever colour
            // surrounds them, because no rule in the sheet matches
            // `.mu-warn.mu-emoji-marker`. A layout engine with no table is in
            // exactly that state for every shortcode, so this is the
            // reference's own rendering of "cannot resolve this" rather than a
            // behaviour invented for the port — and it is visible in a golden,
            // which a silently body-coloured `:smile:` was not.
            TokenKind::Emoji(c) => {
                // `.mu-warn` overrides the hide class rather than composing
                // with it, so the markers are drawn whatever the caret is
                // doing — which is why these are `copy` and not `marker`.
                self.copy(
                    Span::new(token.range.start, c.content.start),
                    token.range,
                    style,
                );
                self.copy(
                    c.content,
                    token.range,
                    InlineStyle {
                        emoji_unresolved: true,
                        ..style
                    },
                );
                self.copy(
                    Span::new(c.content.end, token.range.end),
                    token.range,
                    style,
                );
            }

            // --- links: the anchor is content, the href is a marker --------
            TokenKind::Link(l) => self.container(
                token,
                &l.children,
                InlineStyle {
                    link: true,
                    ..style
                },
            ),
            TokenKind::ReferenceLink(l) => self.container(
                token,
                &l.children,
                InlineStyle {
                    link: true,
                    ..style
                },
            ),
            // `<https://x>` shows the source between the angle brackets and not
            // `href`, which may carry a `mailto:` the author never typed —
            // `plain_text.rs:159` gives the reference's own reason.
            TokenKind::AutoLink(a) => {
                let content = a.href.or(a.email).unwrap_or(token.range);
                self.wrapped(
                    token,
                    content,
                    InlineStyle {
                        link: true,
                        ..style
                    },
                );
            }
            // A bare URL has no markers to hide, so it is its own text.
            TokenKind::AutoLinkExtension(_) => {
                self.copy(
                    token.raw,
                    token.range,
                    InlineStyle {
                        link: true,
                        ..style
                    },
                );
            }

            // --- images: no visible text, and a box in its place -----------
            //
            // Hidden **whole**, alt text included: muya renders an `<img>`, and
            // an image's alt text is an attribute rather than something a
            // reader sees. What a reader sees instead is a replaced box, and
            // D12 gives it a size from above the seam.
            TokenKind::Image(i) => {
                let src = i.src.of(self.src).to_string();
                self.image(token, src, style);
            }
            // `ReferenceImage` carries `alt`, `label` and two backslash runs
            // and **no `href` at all**, so the `src` has to come back out of
            // the same table that made this a reference image in the first
            // place — `mt-inline` emits the kind only when the label is defined
            // (`lexer.rs:973`). A miss is therefore unreachable rather than
            // defended against, and it degrades to the empty `src`, which is a
            // real D12 state (`.mu-empty-image`) rather than a panic.
            TokenKind::ReferenceImage(r) => {
                let src = self
                    .labels
                    .get(r.label.of(self.src).to_lowercase().as_str())
                    .map(|label| label.href.clone())
                    .unwrap_or_default();
                self.image(token, src, style);
            }

            // --- the one substitution that survives ------------------------
            //
            // `escapeCharactersMap[token.escapeCharacter] ?? token.raw`, and the
            // fallback is live rather than defensive: the rule is
            // case-insensitive while the map is not, so `&AMP;` misses and
            // renders as its own source. 5832 of 6084 reachable spellings take
            // the fallback — `escape::escape_character` measured it.
            TokenKind::HtmlEscape {
                escape_character: e,
            } => match escape_character(e.of(self.src)) {
                Some(decoded) => self.substitute(
                    decoded,
                    token.range,
                    token.range,
                    InlineStyle {
                        html_escape: true,
                        ..style
                    },
                ),
                // No `data-character` means no `::before`, so what is drawn is
                // the literal marker text at the surrounding colour rather
                // than a glyph at `--editor-color`.
                None => self.copy(token.raw, token.range, style),
            },

            // --- line breaks are line breaks, not spaces -------------------
            //
            // **Corrected against the stylesheet.** `.mu-content` — the
            // element every leaf's text lives in — is `white-space: pre-wrap`
            // (`blockSyntax.css:948`), so a `\n` inside a paragraph is a
            // forced break and the trailing spaces of a hard break are
            // preserved. `exportStyle.css:92-105` states the intent outright:
            // the exporter gives `p` the same `pre-wrap` *"so the exported
            // HTML stays CommonMark-conformant … yet still shows the break
            // (#3676)"*. Collapsing either to one space merged every
            // multi-line paragraph in the corpus into a single wrapped run.
            //
            // Copied whole, so parley sees the `\n` and breaks on it. The
            // `↩` that `.mu-hard-line-break-space::after` adds
            // (`inlineSyntax.css:150-156`) is **not** drawn: it is
            // `content:` chrome at `opacity: 0.5`, marking a construct rather
            // than being part of it, and `exportStyle.css` carries no rule for
            // it at all.
            TokenKind::SoftLineBreak { .. } | TokenKind::HardLineBreak { .. } => {
                self.copy(token.raw, token.range, style);
            }

            // --- a backslash escape contributes nothing --------------------
            //
            // `raw` is the `\` **alone** — M1.md §4 C3 — and the escaped
            // character surfaces in the next `text` token, so hiding this whole
            // token hides exactly the backslash.
            TokenKind::Backlash { .. } => self.hide(token.range, token.range),

            // --- raw HTML: laid out as its own source ----------------------
            //
            // There is no HTML layout engine in this project and D11 gives the
            // block-level form of the same answer for `HtmlBlock`. Copied whole
            // rather than recursed into, so that `<b>x</b>` is seven visible
            // characters and not one styled one — which is wrong against
            // Chromium and *right* against the only thing M3 can draw.
            TokenKind::HtmlTag(_) => self.copy(token.raw, token.range, style),

            // --- begin rules: the block already consumed the meaning --------
            //
            // An ATX heading's `# `, a thematic break's `---`, a fence's
            // backticks, a `$$`, a whole `[label]: href` line, and an ATX
            // heading's trailing `#`s. Block flow has already read all of it;
            // what is left is a marker.
            //
            // The header tests a **synthetic** range — the `#`s alone
            // (`header.ts:17`) — rather than the token's own, which is what
            // `marker_state_of_range` exists for and the one place in this walk
            // where the two differ.
            TokenKind::Header(rule) => {
                let state = marker_state_of_range(rule.marker, self.cursor);
                self.marker(token.range, token.range, state, style);
            }
            TokenKind::Hr(_)
            | TokenKind::CodeFence(_)
            | TokenKind::MultipleMath(_)
            | TokenKind::TailHeader { .. } => {
                self.marker(
                    token.range,
                    token.range,
                    marker_state(token, self.cursor),
                    style,
                );
            }

            // --- a reference definition is drawn, not hidden ---------------
            //
            // **Corrected against the stylesheet.** Hiding the whole line
            // produced a paragraph with no items at all — `10kb.md`'s two
            // definitions were blank blocks — and `referenceDefinition.ts`
            // gives every one of its six spans a class with a rule attached
            // and **not one of them a hide class**: the marker parts are
            // `.mu-reference-marker` (`--editor-color-50` at `0.9em`,
            // `inlineSyntax.css:578-581`), the label is
            // `.mu-reference-label` (`font-weight: 600`, `:589-593`), the
            // title is `.mu-reference-title` (`0.9em`, `:583-587`), and the
            // backslash run is `.mu-gray` (`:70` of the renderer).
            //
            // The `margin: 0 5px` those last two rules also carry is **not**
            // applied: an inline margin is a shift of everything after it,
            // and neither this crate nor parley has a primitive for one. It
            // is recorded rather than approximated, because a 5px fudge
            // baked into an advance would be indistinguishable from a
            // shaping change the next time a golden moved.
            TokenKind::ReferenceDefinition(r) => {
                let marker = InlineStyle {
                    reference_marker: true,
                    ..style
                };
                // The tail is measured from the end exactly as the renderer
                // measures it, because the optional captures are absent
                // rather than empty when there is no title.
                let title_marker_len = r.title_marker.map_or(0, |s| s.len());
                let title_len = r.title.map_or(0, |s| s.len());
                let tail = token.range.end - r.right_title_space.len() - title_marker_len;
                let title_start = tail - title_len;
                self.copy(r.left_bracket, token.range, marker);
                self.copy(
                    r.label,
                    token.range,
                    InlineStyle {
                        reference_label: true,
                        ..style
                    },
                );
                self.copy(
                    r.backlash,
                    token.range,
                    InlineStyle {
                        gray: true,
                        ..style
                    },
                );
                self.copy(
                    Span::new(r.label.end + r.backlash.len(), title_start),
                    token.range,
                    marker,
                );
                self.copy(
                    Span::new(title_start, tail),
                    token.range,
                    InlineStyle {
                        reference_title: true,
                        ..style
                    },
                );
                self.copy(Span::new(tail, token.range.end), token.range, marker);
            }
        }
    }
}

#[cfg(test)]
mod tests;
