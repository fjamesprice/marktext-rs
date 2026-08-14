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
//! So this module produces three things from one walk of `mt_inline`'s token
//! tree:
//!
//! 1. the **visible string**, which is what gets shaped;
//! 2. a list of [`InlineRun`]s over it, saying which spans are strong, emphatic,
//!    code, link text and so on — [`crate::flow`] resolves those against the
//!    theme into the style runs parley is given;
//! 3. the [`VisibleTextMap`], D13's public run list, which is how any caller
//!    gets from a shaped offset back to a block-text offset.
//!
//! # C6's premise is wrong in one word, and D13 corrects it
//!
//! C6 assumes markers are *deleted*, so that the visible string is a
//! subsequence of the block text and the map is a list of surviving subranges.
//! **Three kinds substitute rather than delete** — see [`MapKind::Substituted`]
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
    Cursor, MarkerState, Span, Token, TokenKind, TokenizerOptions, escape_character, marker_state,
    marker_state_of_range, tokenizer,
};

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
    /// neither length constrains the other. Three kinds do this:
    ///
    /// - `html_escape` — `&amp;` becomes one `&`
    ///   (muya draws it as `content: attr(data-character)`);
    /// - a soft or hard line break becomes one space;
    /// - `emoji` **would** become one emoji glyph, and does not here, because
    ///   `mt-inline` ships no shortcode table and `mt-layout` will not invent
    ///   one. A shortcode is [`Copied`](Self::Copied) whole, colons included —
    ///   see the `Emoji` arm of the walk. **If a table ever arrives, this is
    ///   the variant that gains a third member**, and the run list is already
    ///   shaped for it.
    ///
    /// An offset *inside* such a run has no meaningful partner on the other
    /// side, so both directions answer with the run's start.
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
    /// Inside `~~`. **Carried but not yet drawn**: muya has no `line-through`
    /// rule anywhere, so the strikethrough is the browser's UA `<del>` and the
    /// decoration it needs is a later commit's.
    pub del: bool,
    /// Inside `` ` ``. Takes the theme's inline-code stack and its `0.8em`.
    pub code: bool,
    /// Inside a link, reference link or autolink — the anchor text, never the
    /// href, which is a marker and hidden with the rest.
    pub link: bool,
    /// A footnote identifier's own text, which is drawn smaller.
    pub footnote: bool,
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
/// **`labels` is empty and that is a limitation, not a choice.** A reference
/// link is a reference link only if its label resolves, and the label table is
/// collected per *document* by `mt_md::labels`. Until it is threaded down here,
/// `[text][ref]` tokenizes as plain text and the [`TokenKind::ReferenceLink`]
/// and [`TokenKind::ReferenceImage`] arms below are unreachable. They are
/// written anyway, because the arm being absent when the table arrives is the
/// expensive half.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}

impl InlineSyntax {
    fn options(self) -> TokenizerOptions {
        let mut options = TokenizerOptions::muya_default();
        // Left **on**, which is what makes an ATX heading's `# ` a marker this
        // hides rather than three glyphs it draws.
        options.has_begin_rules = true;
        options.syntax.footnote = self.footnote;
        options.syntax.super_sub_script = self.super_sub_script;
        options
    }
}

/// Tokenize one leaf's text and reduce it to what a reader sees.
///
/// `cursor` is the caret, and `None` — a read-only viewer — hides every marker.
/// It is a parameter rather than a constant because M4's editor is the whole
/// reason `mt_inline::marker` exists, and because a hard-coded `None` would
/// make the reveal path unreachable *and* untested.
pub fn lay_out(text: &str, syntax: InlineSyntax, cursor: Option<Cursor>) -> InlineText {
    let tokens = tokenizer(text, &syntax.options());
    let mut walk = Walk {
        src: text,
        cursor,
        out: InlineText {
            visible: String::with_capacity(text.len()),
            runs: Vec::new(),
            map: VisibleTextMap::default(),
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
            // Unreachable with `cursor: None`. The revealed marker's own colour
            // (`.mu-gray`, `--editor-color-30`) is a run this does not yet
            // distinguish; M4 owns it along with the caret that reaches it.
            MarkerState::Revealed => self.copy(span, token, style),
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
            // `$`s are markers like any other.
            TokenKind::InlineMath(c) => self.wrapped(token, c.content, style),
            TokenKind::SuperSubScript { content, .. } => self.wrapped(token, *content, style),
            TokenKind::FootnoteIdentifier { content, .. } => self.wrapped(
                token,
                *content,
                InlineStyle {
                    footnote: true,
                    ..style
                },
            ),

            // --- the emoji shortcode, which does NOT substitute ------------
            //
            // The reference draws `:smile:` as one glyph through
            // `content: attr(data-emoji)` (`inlineSyntax.css:90-100`), which
            // makes it D13's third substituting kind. **`mt-inline` has no
            // shortcode table** — no name-to-codepoint map exists anywhere in
            // the crate — and inventing one here would be inventing data, so
            // the shortcode is copied whole, colons included.
            //
            // Colons *included* is the deliberate half: hiding the two `:`
            // markers the way every other marker is hidden would render the
            // bare word `smile`, which reads as text the author wrote rather
            // than as an unrendered shortcode. A visible `:smile:` is at least
            // honest about what it is.
            TokenKind::Emoji(_) => self.copy(token.raw, token.range, style),

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

            // --- images: nothing to show, and a hole to come ---------------
            //
            // Hidden **whole**, alt text included: muya renders an `<img>`, and
            // an image's alt text is an attribute rather than something a
            // reader sees. The replaced box that belongs at this offset is
            // D12's, and pushing it is the next commit's job — which is why
            // this is a clean hidden run rather than a placeholder string
            // something would later have to remove.
            TokenKind::Image(_) | TokenKind::ReferenceImage(_) => {
                self.marker(
                    token.range,
                    token.range,
                    marker_state(token, self.cursor),
                    style,
                );
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
                Some(decoded) => self.substitute(decoded, token.range, token.range, style),
                None => self.copy(token.raw, token.range, style),
            },

            // --- line breaks become one space ------------------------------
            TokenKind::SoftLineBreak { .. } | TokenKind::HardLineBreak { .. } => {
                self.substitute(" ", token.range, token.range, style);
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
            | TokenKind::ReferenceDefinition(_)
            | TokenKind::TailHeader { .. } => {
                self.marker(
                    token.range,
                    token.range,
                    marker_state(token, self.cursor),
                    style,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
