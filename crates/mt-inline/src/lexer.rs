//! The tokenizer loop — a port of `inlineRenderer/lexer.ts`.
//!
//! # What is here at S1
//!
//! The loop, `pushPending`, `consumeBeginRules`, the ordered
//! [`INLINE_HANDLERS`] array, and the nine plain handlers M1.md §6 S1 lists:
//! `header` `hr` `code_fence` `multiple_math` `tail_header` `backlash`
//! `html_escape` `soft_line_break` `hard_line_break`. The other eleven
//! handlers are present, in their exact precedence positions, and return
//! `false`. Each says which stage fills it in.
//!
//! That is not a placeholder pattern for its own sake. **The array order is
//! the rule-precedence contract** (`lexer.ts:790`, and the TypeScript carries
//! a comment saying so), so it is written down once, in full, and stages 2–5
//! fill in bodies rather than rearranging entries.
//!
//! # No reslicing — M1.md §5 D4
//!
//! `lexer.ts` does `state.src = state.src.substring(n)` on every token *and
//! every unmatched character*, which is O(n²). This port advances an index
//! into the original `&str` instead and matches against `&origin[pos..end]`.
//! Same semantics, no allocation: every rule is `^`-anchored and JavaScript
//! `exec` is called on a fresh substring, so the anchor lands in the same
//! place either way.
//!
//! # A level is a range, not a substring
//!
//! muya's nested `tokenizerFac` calls take the captured *string* plus an
//! absolute `pos` base, which is what makes D3 site 1 possible: `originSrc` is
//! the child string while `pos` indexes the parent. [`LexState`] carries the
//! whole top-level text plus the current level's [`Span`] instead, so
//! "the character before `pos` at this level" is `pos > level.start`, and the
//! two can no longer disagree. The emoji-boundary fix (S2) therefore falls out
//! of the data layout rather than being a special case.
//!
//! # Pending text
//!
//! muya accumulates plain text into `state.pending`, a `String`, alongside
//! `state.pendingStartPos`. Here it is an `Option<usize>`: the start offset
//! when there is pending text, `None` when there is not, with the end always
//! being `pos`. The two are equivalent because **pending is always exactly
//! `origin[start..pos]`** — see [`LexState::pending`] for the one place where
//! that is not obvious, which is D3 site 3.

use crate::TokenizerOptions;
use crate::rules::{RuleId, exec, rule};
use crate::token::{BeginRule, Span, Token, TokenKind};

use fancy_regex::Captures;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// muya's `ILexState`, for one tokenizer level.
///
/// # Fields muya has and this does not
///
/// - `originSrc` and `src` — replaced by [`origin`](Self::origin) plus
///   [`level`](Self::level). See the module docs.
/// - `inlineRules` — a constant here, not a parameter. muya threads it so that
///   `tokenizerFac` can be called with a different set; nothing ever does.
/// - `labels`, `options`, `superSubScript`, `footnote` — arrive with the
///   handlers that read them, in S2 and S3. Adding them now would be eleven
///   unread fields.
pub(crate) struct LexState<'a> {
    /// The whole top-level block text. Every [`Span`] this crate produces is
    /// an offset into *this*, at every nesting depth.
    origin: &'a str,
    /// This level's extent within [`origin`](Self::origin). The top level is
    /// the whole string; a nested level is one capture group of its parent.
    level: Span,
    /// The cursor. Starts at `level.start` and ends at `level.end`.
    pos: usize,
    /// Where the run of accumulated plain text begins, or `None` if there is
    /// none. Its end is always [`pos`](Self::pos).
    ///
    /// # Why an offset is enough — D3 site 3
    ///
    /// This models `state.pending` (a `String`) and `state.pendingStartPos`
    /// together, and it is only sound because pending text is always a
    /// contiguous slice of the source. It is, at all three sites that write
    /// it:
    ///
    /// - the main loop appends the character at `pos` and advances, so the run
    ///   grows contiguously;
    /// - `pushPending` empties it;
    /// - [`try_backlash`] sets it to the escaped character, which sits at
    ///   `marker.end` — exactly where muya puts `pendingStartPos`.
    ///
    /// The third one is D3 site 3. muya writes `state.pending +=
    /// state.pending + backTo[2]` (`lexer.ts:135`), which reads as a doubling
    /// bug; it is inert, because `pushPending` cleared `pending` on the line
    /// above, so it evaluates to `'' + ('' + x)`. The port writes the plain
    /// assignment. No divergence register entry: nothing diverges. But note
    /// that were it *not* inert, pending would hold text that is not a source
    /// slice and this representation would be wrong rather than merely
    /// different — which is why the reasoning is recorded here and not only in
    /// the handler.
    pending: Option<usize>,
    /// The output.
    tokens: Vec<Token>,
    /// muya's `state.top`: true only for the outermost call.
    ///
    /// Read by [`try_tail_header`] and, from S5, by
    /// [`try_auto_link_extension`].
    top: bool,
}

impl<'a> LexState<'a> {
    /// muya's `state.src` — the level's input from the cursor to its end.
    fn rest(&self) -> &'a str {
        &self.origin[self.pos..self.level.end]
    }

    /// The absolute span of capture group `index`, or `None` if the group did
    /// not participate.
    ///
    /// Capture offsets are relative to [`rest`](Self::rest), which begins at
    /// [`pos`](Self::pos); this is the one place that conversion happens.
    fn group(&self, caps: &Captures<'_, str>, index: usize) -> Option<Span> {
        caps.get(index)
            .map(|m| Span::new(self.pos + m.start(), self.pos + m.end()))
    }

    /// The absolute span of a capture group the pattern guarantees.
    ///
    /// # Panics
    ///
    /// If the group did not participate. That means the pattern and the code
    /// reading it disagree about which groups are optional — a port error, not
    /// an input, so it cannot fire for some documents and not others.
    fn required(&self, caps: &Captures<'_, str>, index: usize, rule: RuleId) -> Span {
        self.group(caps, index).unwrap_or_else(|| {
            panic!(
                "`{}` matched but capture group {index} did not participate",
                rule.name()
            )
        })
    }

    /// `pushPending` (`lexer.ts:42`) — flush accumulated plain text as a
    /// `text` token.
    fn push_pending(&mut self) {
        if let Some(start) = self.pending {
            debug_assert!(
                start < self.pos,
                "a `Some` pending run is non-empty by construction"
            );
            let span = Span::new(start, self.pos);
            self.tokens
                .push(leaf(TokenKind::Text { content: span }, span));
        }
        self.pending = None;
    }

    /// Push a token that has already been given its span, and advance past it.
    ///
    /// Every handler ends this way, so the "flush, push, advance" order is
    /// stated once. `consumed_to` is separate from the token's span because
    /// three handlers report less than they eat or eat less than they report —
    /// M1.md §4 C3 names them: [`try_backlash`] (reports the `\`, eats two),
    /// [`try_tail_header`] (leaves the trailing whitespace group in the
    /// input), and `try_auto_link_extension` in S5 (reports and eats the
    /// trimmed extent).
    fn emit(&mut self, kind: TokenKind, span: Span, consumed_to: usize) {
        self.push_pending();
        self.tokens.push(leaf(kind, span));
        self.pos = consumed_to;
    }
}

/// A token whose `range` and `raw` are the same extent.
///
/// They are the same for all 26 of them — muya computes `raw` as the matched
/// string and `range` as `{ pos, pos + raw.length }` — but it computes them
/// independently, so this states the equality once instead of at 26 call
/// sites. [`check_tiling`] re-checks it on the way out.
fn leaf(kind: TokenKind, span: Span) -> Token {
    Token {
        kind,
        range: span,
        raw: span,
        highlights: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Begin rules
// ---------------------------------------------------------------------------

/// One entry of [`BEGIN_RULES`]: a rule and the variant its match becomes.
///
/// A tuple-variant constructor coerces to a function pointer, so
/// `TokenKind::Header` *is* the `fn(BeginRule) -> TokenKind` this wants. That
/// keeps the four in one table instead of a match arm with an unreachable
/// fallback.
type BeginRuleEntry = (RuleId, fn(BeginRule) -> TokenKind);

/// `consumeBeginRules`'s `beginRuleKeys` (`lexer.ts:61`), in muya's order,
/// paired with the variant each produces.
///
/// The order is muya's, not `rules.ts`'s declaration order — `beginRules`
/// declares `hr` first and `consumeBeginRules` tries `header` first — and only
/// the **first** match is taken (`break`, `lexer.ts:87`). Nothing currently
/// depends on the difference, because the four patterns are mutually
/// exclusive; it is kept because a fifth would not be.
const BEGIN_RULES: [BeginRuleEntry; 4] = [
    (RuleId::Header, TokenKind::Header),
    (RuleId::Hr, TokenKind::Hr),
    (RuleId::CodeFence, TokenKind::CodeFence),
    (RuleId::MultipleMath, TokenKind::MultipleMath),
];

/// `consumeBeginRules` (`lexer.ts:60`).
///
/// Runs only at offset 0 of a top-level tokenize, and only when the caller
/// asked for it ([`TokenizerOptions::has_begin_rules`]).
fn consume_begin_rules(state: &mut LexState<'_>) {
    for (id, into_kind) in BEGIN_RULES {
        let Some(caps) = exec(rule(id), state.rest()) else {
            continue;
        };
        let whole = state.required(&caps, 0, id);
        let payload = BeginRule {
            marker: state.required(&caps, 1, id),
            // muya normalises the absent groups with `to[2] || ''`. `None`
            // here, `""` at the wire boundary — see `token.rs`'s header.
            content: state.group(&caps, 2),
            backlash: state.group(&caps, 3),
        };
        // No `push_pending`: nothing can have accumulated at offset 0.
        state.tokens.push(leaf(into_kind(payload), whole));
        state.pos = whole.end;
        break;
    }

    consume_reference_definition(state);
}

/// The `reference_definition` half of `consumeBeginRules` (`lexer.ts:90`).
///
/// **S3.** It is gated on `isLengthEven(def[3])` and fills eleven fields, one
/// per capture group; it lands with the rest of the link machinery rather than
/// with the four plain begin rules, which is where M1.md §6 puts it. Until
/// then a definition line tokenizes as text, which tiles correctly and is
/// exactly what any not-yet-implemented rule does.
fn consume_reference_definition(_state: &mut LexState<'_>) {}

// ---------------------------------------------------------------------------
// The handlers, in `INLINE_HANDLERS` order
// ---------------------------------------------------------------------------

/// `tryBacklash` (`lexer.ts:118`).
///
/// The token's `raw` is the `\` alone; the escaped character goes into
/// `pending` and surfaces in the *next* `text` token. So this consumes two
/// bytes and reports one — the first of M1.md §4 C3's three.
fn try_backlash(state: &mut LexState<'_>) -> bool {
    let Some(caps) = exec(rule(RuleId::Backlash), state.rest()) else {
        return false;
    };
    let whole = state.required(&caps, 0, RuleId::Backlash);
    let marker = state.required(&caps, 1, RuleId::Backlash);
    let escaped = state.required(&caps, 2, RuleId::Backlash);

    state.emit(TokenKind::Backlash { marker }, marker, whole.end);

    // D3 site 3, `lexer.ts:135`: muya writes
    //     state.pending += state.pending + backTo[2]
    // which reads as a doubling bug but is inert — `pushPending`, one line
    // above, has just set `pending` to `''`, so it evaluates to `'' + ('' + x)`
    // and the result is `backTo[2]`. This is the plain assignment. It is not a
    // behavioural change, and it is deliberately *not* in
    // `spec/divergences.json`: nothing diverges, so an entry there would be
    // stale on arrival and the register's rule 3 would report it as such.
    //
    // The escaped character sits at `marker.end`, which is where muya sets
    // `pendingStartPos`, and `pos` is now `escaped.end` — so the pending run
    // is `origin[escaped.start..pos]`, the escaped character and nothing else.
    debug_assert_eq!(escaped.end, whole.end);
    state.pending = Some(escaped.start);

    true
}

/// `tryStrongEm` (`lexer.ts:143`). **S2** — needs `validateEmphasize`,
/// `lowerPriority` and `isLengthEven`.
fn try_strong_em(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryChunks` (`lexer.ts:194`) — `inline_code`, `del`, `emoji`,
/// `inline_math`. **S2**, and it carries the D3 site 1 emoji-boundary fix.
fn try_chunks(_state: &mut LexState<'_>) -> bool {
    false
}

/// `trySuperSubScript` (`lexer.ts:262`). **S2.**
fn try_super_sub_script(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryFootnote` (`lexer.ts:289`). **S2.**
fn try_footnote(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryImage` (`lexer.ts:315`). **S3** — needs `correctUrl` and
/// `parseSrcAndTitle`.
fn try_image(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryLink` (`lexer.ts:353`). **S3.**
fn try_link(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryReferenceLink` (`lexer.ts:407`). **S3.**
fn try_reference_link(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryReferenceImage` (`lexer.ts:457`). **S3.**
fn try_reference_image(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryHtmlEscape` (`lexer.ts:495`) — one of the 269 named references in
/// `config/escapeCharacter.ts`.
fn try_html_escape(state: &mut LexState<'_>) -> bool {
    let Some(caps) = exec(rule(RuleId::HtmlEscape), state.rest()) else {
        return false;
    };
    let whole = state.required(&caps, 0, RuleId::HtmlEscape);
    let escape_character = state.required(&caps, 1, RuleId::HtmlEscape);

    state.emit(TokenKind::HtmlEscape { escape_character }, whole, whole.end);
    true
}

/// `tryAutoLinkExtension` (`lexer.ts:572`). **S5** — needs
/// `trimAutoLinkExtent`.
fn try_auto_link_extension(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryAutoLink` (`lexer.ts:623`). **S5.**
fn try_auto_link(_state: &mut LexState<'_>) -> bool {
    false
}

/// `tryHtmlTag` (`lexer.ts:649`). **S4** — needs the attribute scanner
/// (M1.md §5 D2) and carries the D3 site 2 disallowed-tag fix.
fn try_html_tag(_state: &mut LexState<'_>) -> bool {
    false
}

/// `trySoftLineBreak` (`lexer.ts:719`) — a `\n` that is not the start of a
/// blank line.
fn try_soft_line_break(state: &mut LexState<'_>) -> bool {
    let Some(caps) = exec(rule(RuleId::SoftLineBreak), state.rest()) else {
        return false;
    };
    let whole = state.required(&caps, 0, RuleId::SoftLineBreak);
    let line_break = state.required(&caps, 1, RuleId::SoftLineBreak);

    // muya: `softTo.input.length === softTo[0].length`. `input` is the level's
    // remaining text and the match is anchored at its start, so the test is
    // "the match reaches the end of this level".
    let is_at_end = whole.end == state.level.end;

    state.emit(
        TokenKind::SoftLineBreak {
            line_break,
            is_at_end,
        },
        whole,
        whole.end,
    );
    true
}

/// `tryHardLineBreak` (`lexer.ts:743`) — two or more spaces then a `\n`.
fn try_hard_line_break(state: &mut LexState<'_>) -> bool {
    let Some(caps) = exec(rule(RuleId::HardLineBreak), state.rest()) else {
        return false;
    };
    let whole = state.required(&caps, 0, RuleId::HardLineBreak);
    let spaces = state.required(&caps, 1, RuleId::HardLineBreak);
    let line_break = state.required(&caps, 2, RuleId::HardLineBreak);
    let is_at_end = whole.end == state.level.end;

    state.emit(
        TokenKind::HardLineBreak {
            spaces,
            line_break,
            is_at_end,
        },
        whole,
        whole.end,
    );
    true
}

/// `tryTailHeader` (`lexer.ts:768`) — the closing `#`s of an ATX heading.
///
/// The second of M1.md §4 C3's three: `raw` and the consumption are both
/// capture 1, so the trailing whitespace of capture 2 is left in the input and
/// re-enters the loop as text.
fn try_tail_header(state: &mut LexState<'_>) -> bool {
    let Some(caps) = exec(rule(RuleId::TailHeader), state.rest()) else {
        return false;
    };
    if !state.top {
        return false;
    }
    let marker = state.required(&caps, 1, RuleId::TailHeader);

    state.emit(TokenKind::TailHeader { marker }, marker, marker.end);
    true
}

/// The fixed, priority-ordered handler list (`lexer.ts:792`).
///
/// **This array order is the rule-precedence contract.** The loop takes the
/// first handler that returns `true`; port it as an ordered array, not as a
/// match on the next character, because the order *is* the spec — the
/// TypeScript carries a comment saying exactly that, and M1.md §2 repeats it.
const INLINE_HANDLERS: [fn(&mut LexState<'_>) -> bool; 16] = [
    try_backlash,
    try_strong_em,
    try_chunks,
    try_super_sub_script,
    try_footnote,
    try_image,
    try_link,
    try_reference_link,
    try_reference_image,
    try_html_escape,
    try_auto_link_extension,
    try_auto_link,
    try_html_tag,
    try_soft_line_break,
    try_hard_line_break,
    try_tail_header,
];

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

/// `tokenizerFac` (`lexer.ts:811`) — tokenize one level.
///
/// `level` is the extent of this level's input within `origin`. For the
/// top-level call that is the whole string; a nested call (S2 onward) passes
/// the capture group it is descending into, and every span the call produces
/// is still an absolute offset into `origin`.
pub(crate) fn tokenizer_fac(
    origin: &str,
    level: Span,
    has_begin_rules: bool,
    top: bool,
) -> Vec<Token> {
    let mut state = LexState {
        origin,
        level,
        pos: level.start,
        pending: None,
        tokens: Vec::new(),
        top,
    };

    // muya: `if (beginRules && state.pos === 0)`. Both halves are kept: the
    // caller's opt-in, and the offset. Only the top-level call can satisfy the
    // second, and only it is ever given the first.
    if has_begin_rules && state.pos == 0 {
        consume_begin_rules(&mut state);
    }

    while state.pos < state.level.end {
        let consumed = INLINE_HANDLERS.iter().any(|handler| handler(&mut state));
        if consumed {
            continue;
        }

        // No rule matched: the character joins the pending run.
        if state.pending.is_none() {
            state.pending = Some(state.pos);
        }
        // muya advances one UTF-16 code unit; this advances one `char`, which
        // is the same thing except that it never lands inside a surrogate
        // pair. Nothing observable changes — both halves of a pair end up in
        // the same `text` token either way — and it is what D1 means by
        // "Rust's `char` iteration gives that for free".
        let next = state
            .rest()
            .chars()
            .next()
            .expect("the loop condition guarantees at least one character");
        state.pos += next.len_utf8();
    }

    state.push_pending();

    if cfg!(debug_assertions) {
        check_tiling(origin, level, &state.tokens);
    }

    state.tokens
}

// ---------------------------------------------------------------------------
// The tiling invariant — §3 rule 1, as qualified by M1.md §4 C3
// ---------------------------------------------------------------------------

/// Assert that `tokens` tile `level` exactly.
///
/// §3 rule 1 is *"ranges are byte offsets, and they must tile the input
/// exactly: concatenating every token's `raw` reproduces the source
/// byte-for-byte"*, and it is called the invariant that catches most porting
/// errors immediately. M1.md §4 C3 qualifies it in two ways, both checked
/// here:
///
/// - it holds **per tokenizer level**, so this is called once per
///   [`tokenizer_fac`] with that level's extent, not once over the whole
///   document;
/// - the cross-level half is that **every child span is contained in its
///   parent's**, so a nested level's tokens are checked against the token that
///   owns them.
///
/// Stated as "each token starts where the previous one ended" rather than by
/// concatenating and comparing strings: same invariant, no allocation, and the
/// failure message names the byte offset instead of dumping two documents.
///
/// Called under `cfg!(debug_assertions)`, so it costs nothing in a release
/// build. §3 asks for exactly that.
fn check_tiling(origin: &str, level: Span, tokens: &[Token]) {
    let mut cursor = level.start;

    for (index, token) in tokens.iter().enumerate() {
        assert_eq!(
            token.raw.start,
            cursor,
            "token {index} (`{}`) starts at {} but the previous token ended at {cursor}: \
             the tiling has a {} at {cursor}",
            token.type_str(),
            token.raw.start,
            if token.raw.start > cursor {
                "gap"
            } else {
                "overlap"
            }
        );
        assert!(
            token.raw.end >= token.raw.start,
            "token {index} (`{}`) has an inverted span {:?}",
            token.type_str(),
            token.raw
        );
        assert_eq!(
            token.range,
            token.raw,
            "token {index} (`{}`) has a `range` that is not its `raw`. muya computes the two \
             independently and they agree in all sixteen handlers; if they no longer do, one \
             of them is wrong",
            token.type_str()
        );
        assert!(
            origin.is_char_boundary(token.raw.start) && origin.is_char_boundary(token.raw.end),
            "token {index} (`{}`) has a span {:?} that is not on `char` boundaries",
            token.type_str(),
            token.raw
        );

        if let Some(children) = token.children() {
            for child in children {
                assert!(
                    token.range.contains_span(child.range),
                    "a `{}` child of a `{}` escapes its parent: {:?} is not inside {:?}",
                    child.type_str(),
                    token.type_str(),
                    child.range,
                    token.range
                );
            }
        }

        cursor = token.raw.end;
    }

    assert_eq!(
        cursor, level.end,
        "the tokens of this level stop at {cursor} but the level ends at {}",
        level.end
    );
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `tokenizer` (`lexer.ts:854`), minus the `highlights` post-pass.
///
/// `options.labels` and `options.syntax` are not read yet — the handlers that
/// consult them are S2 and S3 — and neither is `options.highlights`, whose
/// post-pass is S6. [`crate::tokenizer`] says so where a caller will see it.
pub(crate) fn tokenizer(src: &str, options: &TokenizerOptions) -> Vec<Token> {
    tokenizer_fac(src, Span::new(0, src.len()), options.has_begin_rules, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenize;

    fn types(src: &str) -> Vec<&'static str> {
        tokenize(src).iter().map(Token::type_str).collect()
    }

    /// The invariant, applied to the assertion that enforces it: a `text`
    /// token's `raw` is the source it came from.
    fn raws(src: &str) -> Vec<&str> {
        tokenize(src).iter().map(|t| t.raw.of(src)).collect()
    }

    // -----------------------------------------------------------------------
    // The loop
    // -----------------------------------------------------------------------

    #[test]
    fn plain_text_is_one_token() {
        assert_eq!(types("hello world"), ["text"]);
        assert_eq!(raws("hello world"), ["hello world"]);
    }

    #[test]
    fn empty_input_produces_no_tokens() {
        assert!(tokenize("").is_empty());
    }

    /// Nothing S1 implements can match here, so every rule falls through and
    /// the whole thing accumulates. That is the loop's default path and the
    /// one every unimplemented handler relies on.
    #[test]
    fn unmatched_constructs_accumulate_as_text_rather_than_being_dropped() {
        let src = "**bold** and *em* and `code` and [link](url)";
        assert_eq!(types(src), ["text"]);
        assert_eq!(raws(src), [src]);
    }

    /// Multi-byte characters advance by `char`, not by byte, and land in the
    /// same token. Both are prerequisites for D1's byte offsets.
    #[test]
    fn multibyte_text_stays_in_one_token_with_byte_spans() {
        let src = "中文 🇬🇧 مرحبا";
        let tokens = tokenize(src);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].raw, Span::new(0, src.len()));
        assert_eq!(tokens[0].raw.of(src), src);
    }

    // -----------------------------------------------------------------------
    // Begin rules
    // -----------------------------------------------------------------------

    #[test]
    fn a_header_marker_is_consumed_at_offset_zero() {
        let src = "## Title";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["header", "text"]);
        assert_eq!(tokens[0].raw.of(src), "## ");
        assert_eq!(tokens[1].raw.of(src), "Title");

        let TokenKind::Header(begin) = &tokens[0].kind else {
            panic!("expected a header");
        };
        assert_eq!(begin.marker.of(src), "## ");
        assert_eq!(begin.content.map(|s| s.of(src)), Some(" "));
        assert_eq!(begin.backlash, None, "no begin rule has a third group");
    }

    #[test]
    fn the_other_three_begin_rules_consume_their_whole_line() {
        assert_eq!(types("***"), ["hr"]);
        assert_eq!(types("---"), ["hr"]);
        assert_eq!(types("___"), ["hr"]);
        assert_eq!(types("```rust"), ["code_fence"]);
        assert_eq!(types("$$"), ["multiple_math"]);
    }

    #[test]
    fn a_code_fence_keeps_its_info_string_as_the_content_group() {
        let src = "````js";
        let tokens = tokenize(src);
        let TokenKind::CodeFence(begin) = &tokens[0].kind else {
            panic!("expected a code_fence");
        };
        assert_eq!(begin.marker.of(src), "````");
        assert_eq!(begin.content.map(|s| s.of(src)), Some("js"));
    }

    /// Only the first begin rule to match is taken (`break`, `lexer.ts:87`).
    #[test]
    fn at_most_one_begin_rule_fires() {
        // `# ` is a header; nothing else may also consume from offset 0.
        assert_eq!(types("# ***"), ["header", "text"]);
    }

    /// A begin rule is a *begin* rule: offset 0 of the top level only.
    #[test]
    fn begin_rules_do_not_fire_after_offset_zero() {
        assert_eq!(
            types("text\n# not a header"),
            ["text", "soft_line_break", "text"]
        );
    }

    #[test]
    fn begin_rules_can_be_switched_off() {
        let options = TokenizerOptions {
            has_begin_rules: false,
            ..TokenizerOptions::muya_default()
        };
        let tokens = crate::tokenizer("## Title", &options);
        assert_eq!(
            tokens.iter().map(Token::type_str).collect::<Vec<_>>(),
            ["text"]
        );
    }

    // -----------------------------------------------------------------------
    // The plain handlers
    // -----------------------------------------------------------------------

    /// The escaped character is *not* part of the `backlash` token — it is
    /// pushed into `pending` and reappears in the next `text` token. Two bytes
    /// consumed, one reported (M1.md §4 C3).
    #[test]
    fn a_backslash_escape_reports_one_byte_and_consumes_two() {
        let src = r"a \* b";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["text", "backlash", "text"]);
        assert_eq!(tokens[0].raw.of(src), "a ");
        assert_eq!(tokens[1].raw.of(src), "\\");
        assert_eq!(tokens[2].raw.of(src), "* b");

        let TokenKind::Backlash { marker } = tokens[1].kind else {
            panic!("expected a backlash");
        };
        assert_eq!(marker, tokens[1].raw);
    }

    /// D3 site 3, made observable: with muya's `+=` the escaped character
    /// would still be right (the doubling is of an empty string), and with a
    /// genuine doubling the next text token would read `**` instead of `*`.
    #[test]
    fn the_escaped_character_appears_exactly_once_in_the_following_text() {
        let src = r"\*";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["backlash", "text"]);
        assert_eq!(tokens[1].raw.of(src), "*");
    }

    #[test]
    fn a_backslash_before_an_unescapable_character_is_plain_text() {
        assert_eq!(types(r"a \b c"), ["text"]);
    }

    #[test]
    fn an_html_escape_is_its_own_token() {
        let src = "a &amp; b";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["text", "html_escape", "text"]);
        assert_eq!(tokens[1].raw.of(src), "&amp;");

        let TokenKind::HtmlEscape { escape_character } = tokens[1].kind else {
            panic!("expected an html_escape");
        };
        assert_eq!(escape_character.of(src), "&amp;");
    }

    #[test]
    fn soft_and_hard_line_breaks_are_distinguished_by_the_preceding_spaces() {
        assert_eq!(types("a\nb"), ["text", "soft_line_break", "text"]);
        assert_eq!(types("a  \nb"), ["text", "hard_line_break", "text"]);
        // One space is not a hard break, so the space stays in the text and
        // the `\n` is soft.
        assert_eq!(types("a \nb"), ["text", "soft_line_break", "text"]);
        assert_eq!(raws("a \nb"), ["a ", "\n", "b"]);
    }

    #[test]
    fn is_at_end_is_true_only_when_the_break_ends_the_level() {
        let tokens = tokenize("a\n");
        let TokenKind::SoftLineBreak { is_at_end, .. } = tokens[1].kind else {
            panic!("expected a soft_line_break");
        };
        assert!(is_at_end);

        let tokens = tokenize("a\nb");
        let TokenKind::SoftLineBreak { is_at_end, .. } = tokens[1].kind else {
            panic!("expected a soft_line_break");
        };
        assert!(!is_at_end);
    }

    /// A blank line is not a line break — `(?!\n)` on both rules.
    ///
    /// The first `\n` therefore falls through to the pending run and ends up
    /// *inside* the preceding `text` token; the second one, which is no longer
    /// followed by a newline, is a soft break. Surprising, and worth pinning:
    /// a leaf block's text does not normally contain a blank line, so this is
    /// the behaviour of a case the block layer is supposed to have already
    /// split, and it must at least tile.
    #[test]
    fn a_blank_line_is_absorbed_into_the_preceding_text() {
        let src = "a\n\nb";
        assert_eq!(types(src), ["text", "soft_line_break", "text"]);
        assert_eq!(raws(src), ["a\n", "\n", "b"]);
    }

    /// `raw` is capture 1; capture 2's trailing whitespace is left in the
    /// input and comes back round as text (M1.md §4 C3).
    #[test]
    fn a_tail_header_leaves_its_trailing_whitespace_in_the_input() {
        let src = "# Title ###  ";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["header", "text", "tail_header", "text"]);
        assert_eq!(tokens[2].raw.of(src), " ###");
        assert_eq!(tokens[3].raw.of(src), "  ");
    }

    // -----------------------------------------------------------------------
    // The tiling invariant
    // -----------------------------------------------------------------------

    /// The invariant itself, over inputs that exercise every S1 handler at
    /// once — including the two that report less than they consume.
    #[test]
    fn every_s1_construct_tiles_its_input() {
        let cases = [
            "",
            "plain",
            "# Heading ##  ",
            "```rust",
            "$$",
            "***",
            r"escape \* and \\ and \$",
            "entity &amp; and &nbsp; and &notreal;",
            "line\nbreak",
            "hard  \nbreak",
            "a\n\nb",
            "中文 with a \u{feff}BOM and an emoji 🙂",
            "\\",
            "&",
            "\n",
            "   ",
        ];
        for src in cases {
            let tokens = tokenize(src);
            // `check_tiling` already ran inside `tokenizer_fac`; this is the
            // same statement made the way §3 words it, so a change to the
            // assertion cannot quietly weaken the property.
            let rebuilt: String = tokens.iter().map(|t| t.raw.of(src)).collect();
            assert_eq!(rebuilt, src, "tiling failed for {src:?}");
        }
    }

    /// `check_tiling` is only useful if it actually fails on a gap. Fed a
    /// hand-built token list with one byte missing.
    #[test]
    #[should_panic(expected = "gap")]
    fn the_tiling_check_rejects_a_gap() {
        let token = leaf(
            TokenKind::Text {
                content: Span::new(1, 4),
            },
            Span::new(1, 4),
        );
        check_tiling("abcd", Span::new(0, 4), &[token]);
    }

    #[test]
    #[should_panic(expected = "the level ends at")]
    fn the_tiling_check_rejects_a_short_run() {
        let token = leaf(
            TokenKind::Text {
                content: Span::new(0, 2),
            },
            Span::new(0, 2),
        );
        check_tiling("abcd", Span::new(0, 4), &[token]);
    }

    /// The cross-level half of C3.
    #[test]
    #[should_panic(expected = "escapes its parent")]
    fn the_tiling_check_rejects_a_child_outside_its_parent() {
        use crate::token::Emphasis;

        let child = leaf(
            TokenKind::Text {
                content: Span::new(5, 9),
            },
            Span::new(5, 9),
        );
        let parent = leaf(
            TokenKind::Strong(Emphasis {
                marker: Span::new(0, 2),
                children: vec![child],
                backlash: Span::new(2, 2),
            }),
            Span::new(0, 4),
        );
        check_tiling("abcdefghij", Span::new(0, 4), &[parent]);
    }

    // -----------------------------------------------------------------------
    // The backtrack limit, at the tokenizer level
    // -----------------------------------------------------------------------

    /// Whatever a rule does with its budget, the tokenizer stays total: it
    /// terminates, it does not panic, and the result still tiles. That is the
    /// property the M1 fuzzing gate is really about, and it holds because a
    /// rule that gives up is indistinguishable from a rule that did not match.
    #[test]
    fn pathological_input_still_terminates_and_tiles() {
        let cases = [
            "[".repeat(2000),
            format!("[{}](", "a".repeat(2000)),
            format!("{}$", "$".repeat(500)),
            format!("&{};", "a".repeat(4000)),
            format!("{}\\", "\\".repeat(2000)),
            format!("{}#", " ".repeat(4000)),
        ];
        for src in &cases {
            let tokens = tokenize(src);
            let rebuilt: String = tokens.iter().map(|t| t.raw.of(src)).collect();
            assert_eq!(&rebuilt, src);
        }
    }
}
