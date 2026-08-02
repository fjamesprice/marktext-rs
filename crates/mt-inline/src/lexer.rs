//! The tokenizer loop — a port of `inlineRenderer/lexer.ts`.
//!
//! # What is here at S2
//!
//! The loop, `pushPending`, `consumeBeginRules`, the ordered
//! [`INLINE_HANDLERS`] array, and thirteen of its sixteen handlers: the nine
//! plain ones from S1 (`header` `hr` `code_fence` `multiple_math`
//! `tail_header` `backlash` `html_escape` `soft_line_break`
//! `hard_line_break`) plus S2's four — [`try_strong_em`], [`try_chunks`],
//! [`try_super_sub_script`] and [`try_footnote`], which between them produce
//! eight of the 26 token types. The remaining three ([`try_image`],
//! [`try_link`] and friends, [`try_html_tag`], the two autolinks) are present,
//! in their exact precedence positions, and return `false`. Each says which
//! stage fills it in.
//!
//! That is not a placeholder pattern for its own sake. **The array order is
//! the rule-precedence contract** (`lexer.ts:790`, and the TypeScript carries
//! a comment saying so), so it is written down once, in full, and stages 3–5
//! fill in bodies rather than rearranging entries.
//!
//! # Nesting starts here
//!
//! S2 is the first stage in which [`tokenizer_fac`] calls itself: `strong`,
//! `em` and `del` tokenize their content group as a child level. Two
//! consequences worth knowing before reading a handler:
//!
//! - **Child spans are absolute.** A nested call is given the capture group's
//!   [`Span`] and produces offsets into the same top-level text, exactly as
//!   muya passes `state.pos + to[1].length` as the child's base.
//! - **[`check_tiling`]'s cross-level half is now live.** Until S2 the "every
//!   child span is contained in its parent's" assertion was vacuously true
//!   because nothing had children.
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

use crate::emphasis::{is_length_even, is_word_character, lower_priority, validate_emphasize};
use crate::rules::{self, RuleId, exec, rule};
use crate::token::{BeginRule, CodeEmojiMath, Emphasis, Span, Token, TokenKind};
use crate::{SyntaxOptions, TokenizerOptions};

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
/// - `options` — muya carries the whole options object only so that it can be
///   passed down to a nested `tokenizerFac`; the two fields any handler reads
///   are `superSubScript` and `footnote`, which are [`syntax`](Self::syntax).
/// - `labels` — arrives with `tryReferenceLink`, in S3.
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
    /// muya's `state.superSubScript` and `state.footnote`, destructured from
    /// `options` at `lexer.ts:812` and threaded down every nested call
    /// unchanged.
    ///
    /// S1 left this out rather than carry a dead field. S2 is where it starts
    /// being read: [`try_super_sub_script`] and [`try_footnote`] are gated on
    /// it, and both gates are load-bearing — `footnote` is `false` by default,
    /// so `[^1]` is plain text unless a caller asks otherwise.
    syntax: SyntaxOptions,
}

impl<'a> LexState<'a> {
    /// muya's `state.src` — the level's input from the cursor to its end.
    fn rest(&self) -> &'a str {
        &self.origin[self.pos..self.level.end]
    }

    /// muya's `state.pending` as a string.
    ///
    /// `canOpenEmphasis` reads it for the character preceding an opening
    /// marker, so it has to be materialised at exactly one call site. Empty
    /// when there is no pending run — muya's `''`, which
    /// `lastCodePointChar` turns into the `'\n'` default.
    ///
    /// **Not "all the text before `pos`".** `pushPending` empties the run
    /// whenever a token is emitted, so after `` `code`**bold** `` the pending
    /// run is empty and the `**` is treated as though it began a line. That is
    /// muya's behaviour and reproducing it is the reason this reads the run
    /// rather than `origin[level.start..pos]`.
    fn pending_text(&self) -> &'a str {
        match self.pending {
            Some(start) => &self.origin[start..self.pos],
            None => "",
        }
    }

    /// The character before [`pos`](Self::pos) **at this level**, or `None`
    /// when the cursor is at the level's start.
    ///
    /// # This is M1.md §5 D3 site 1, and it is a one-liner because of D4
    ///
    /// muya writes `state.originSrc[state.pos - 1]` (`lexer.ts:206`, and again
    /// at `:578`). In a nested `tokenizerFac` call `originSrc` is the **child
    /// substring** while `pos` is an **absolute** offset into the top-level
    /// text, so the two index different strings and the read lands on whatever
    /// character happens to sit at that offset in the child — or off the end.
    /// `**a :smile:**` and `**:smile:**` lose their emoji; `*x :smile:*` and
    /// `[a :smile:](u)` keep theirs, by luck.
    ///
    /// Here there is only ever **one string**. A level is `(origin, level)`
    /// rather than a substring plus a base, so "the character before the
    /// cursor at this level" cannot disagree with itself. `None` at
    /// `pos == level.start` is a boundary and allows the emoji, which is
    /// correct at every call site: a child level is only ever entered after
    /// `*`, `_`, `~`, `[` or a `>`-terminated open tag, none of which are word
    /// characters — and it is what the top level already does at `pos == 0`.
    ///
    /// Registered as `emoji-nested-boundary` in `spec/divergences.json` before
    /// it was made, per D3's rule; the two inputs named there are asserted in
    /// this module's tests.
    fn preceding_char(&self) -> Option<char> {
        self.origin[self.level.start..self.pos].chars().next_back()
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

/// `tryStrongEm` (`lexer.ts:143`) — `**bold**` / `__bold__` / `*em*` / `_em_`.
///
/// # The `return` that is not a `continue`
///
/// muya tries `strong` then `em`, and the inner block ends:
///
/// ```js
/// if (isValid) { … return true }
/// return false            // lexer.ts:186
/// ```
///
/// That `return` exits the **whole handler**, not the iteration. So when
/// `strong` matches with an even backslash run and `validateEmphasize` then
/// rejects it, `em` is never tried at that position. Reading the two-element
/// array as a loop and writing `continue` is the natural mistake, and it is
/// reproduced here.
///
/// Note where the `return` is *not*: an odd backslash run fails the
/// `isLengthEven` gate **before** the block is entered, so that case really
/// does fall through to `em`. `**a\**` is the smallest witness, and it is why
/// the gate is checked outside the `if` in this port too.
///
/// ## …and it appears to be unobservable, which is why this comment exists
///
/// Searched for a distinguishing input against the running engine — two
/// reimplementations of this decision over muya's own rules and its own
/// `validateEmphasize`, one with the `return` and one with a `continue` —
/// across every `(src, pending)` pair with `|src| ≤ 6` over the alphabet
/// ``*_a `$~.,\`` and a structured sweep of longer markers, bodies and tails.
/// **Two million pairs, zero disagreements.**
///
/// Half of that has a proof rather than a sample. `strong`'s pattern carries
/// `(?=\S)`, so `canOpenEmphasis`'s whitespace test can never be what rejects
/// it; its punctuation test rejects when `src[2]` is punctuation and the
/// preceding character is not a flanking boundary; and `em` at the same
/// position sees `src[1]`, which is the second marker character and therefore
/// *always* punctuation — so `em` needs the same boundary and fails whenever
/// `strong` did. The `_` clause and the run-atomicity guard both read only the
/// marker character and `pending`, which are identical for the two rules. So
/// `canOpenEmphasis` can never distinguish them. `canCloseEmphasis` and
/// `lowerPriority` see different offsets and are only covered by the sweep.
///
/// The `return` stays because a faithful port is what M1 is for and because
/// "no counterexample under length 6" is not a theorem. What the paragraph
/// buys is that nobody has to redo the search: if this is ever rewritten as a
/// `continue`, **no test will catch it**, and that is worth knowing before
/// rather than after.
fn try_strong_em(state: &mut LexState<'_>) -> bool {
    type IntoKind = fn(Emphasis) -> TokenKind;
    const EM_RULES: [(RuleId, IntoKind); 2] = [
        (RuleId::Strong, TokenKind::Strong),
        (RuleId::Em, TokenKind::Em),
    ];

    let origin = state.origin;

    for (id, into_kind) in EM_RULES {
        let Some(caps) = exec(rule(id), state.rest()) else {
            continue;
        };
        let backlash = state.required(&caps, 3, id);
        if !is_length_even(Some(backlash)) {
            continue;
        }

        let whole = state.required(&caps, 0, id);
        let marker = state.required(&caps, 1, id);
        let content = state.required(&caps, 2, id);

        // `state.pending` is read *before* the flush, as muya does: the
        // preceding character comes from the run that `emit` is about to
        // consume.
        let is_valid = validate_emphasize(
            state.rest(),
            whole.len(),
            marker.of(origin),
            state.pending_text(),
            rules::VALIDATE_RULES,
        );
        if !is_valid {
            return false;
        }

        // muya tokenizes `to[2]` with the absolute base `pos + to[1].length`.
        // The lookahead between groups 1 and 2 is zero-width, so that base is
        // where group 2 starts — asserted rather than recomputed, because if
        // the pattern ever grew something between them the two would silently
        // disagree and every child span would be shifted.
        debug_assert_eq!(content.start, marker.end);
        let children = tokenizer_fac(origin, content, false, false, state.syntax);

        state.emit(
            into_kind(Emphasis {
                marker,
                children,
                backlash,
            }),
            whole,
            whole.end,
        );
        return true;
    }

    false
}

/// `tryChunks` (`lexer.ts:194`) — `inline_code`, `del`, `emoji`, `inline_math`,
/// in that precedence order.
///
/// Three of the four are leaves carrying their content verbatim; `del` is the
/// odd one out and tokenizes its content as children, like `strong` and `em`.
/// It does **not** go through `validateEmphasize` — `~~` has no flanking rule.
///
/// # The emoji word boundary, and the second `return`
///
/// `emoji` alone is gated twice more (#1677): the `:` must not be glued to a
/// word character, and nothing higher-priority may run past the shortcode.
/// Failing either is another `return false` from the whole handler rather than
/// a `continue`, so a rejected emoji stops `inline_math` from being tried at
/// the same position. Same shape as [`try_strong_em`]'s, and reproduced for the
/// same reason.
///
/// The boundary read is M1.md §5 D3 site 1 — see [`LexState::preceding_char`].
fn try_chunks(state: &mut LexState<'_>) -> bool {
    const CHUNKS: [RuleId; 4] = [
        RuleId::InlineCode,
        RuleId::Del,
        RuleId::Emoji,
        RuleId::InlineMath,
    ];

    let origin = state.origin;

    for id in CHUNKS {
        let Some(caps) = exec(rule(id), state.rest()) else {
            continue;
        };
        // `inline_code` and `emoji` have only two capture groups, so this is
        // `isLengthEven(undefined)` — vacuously true. Deliberately kept
        // vacuous; see `is_length_even`.
        let backlash = state.group(&caps, 3);
        if !is_length_even(backlash) {
            continue;
        }

        let whole = state.required(&caps, 0, id);
        let marker = state.required(&caps, 1, id);
        let content = state.required(&caps, 2, id);

        if id == RuleId::Emoji {
            // An emoji opener must sit at a word boundary: the `:` in
            // `12:00-14:00` is not the start of a shortcode (#1677).
            let preceded_by_word = is_word_character(state.preceding_char());
            if preceded_by_word || !lower_priority(state.rest(), whole.len(), rules::VALIDATE_RULES)
            {
                return false;
            }
        }

        let kind = if id == RuleId::Del {
            debug_assert_eq!(content.start, marker.end);
            TokenKind::Del(Emphasis {
                marker,
                children: tokenizer_fac(origin, content, false, false, state.syntax),
                backlash: state.required(&caps, 3, id),
            })
        } else {
            let chunk = CodeEmojiMath {
                marker,
                content,
                backlash,
            };
            match id {
                RuleId::InlineCode => TokenKind::InlineCode(chunk),
                RuleId::Emoji => TokenKind::Emoji(chunk),
                RuleId::InlineMath => TokenKind::InlineMath(chunk),
                _ => unreachable!("CHUNKS has exactly four entries"),
            }
        };

        state.emit(kind, whole, whole.end);
        return true;
    }

    false
}

/// `trySuperSubScript` (`lexer.ts:262`) — `^sup^` and `~sub~`.
///
/// One token type for both, as `types.ts` has it; the marker distinguishes
/// them. muya writes the rule choice as `superscript.exec(src) ||
/// subscript.exec(src)`, so `^` is tried first and the two never compete —
/// their markers are different characters.
///
/// Gated on `options.syntax.super_sub_script`, which defaults to `true`.
fn try_super_sub_script(state: &mut LexState<'_>) -> bool {
    if !state.syntax.super_sub_script {
        return false;
    }

    let Some((id, caps)) = [RuleId::Superscript, RuleId::Subscript]
        .into_iter()
        .find_map(|id| exec(rule(id), state.rest()).map(|caps| (id, caps)))
    else {
        return false;
    };

    let whole = state.required(&caps, 0, id);
    let marker = state.required(&caps, 1, id);
    let content = state.required(&caps, 2, id);

    state.emit(
        TokenKind::SuperSubScript { marker, content },
        whole,
        whole.end,
    );
    true
}

/// `tryFootnote` (`lexer.ts:289`) — `[^note]`.
///
/// Gated on `options.syntax.footnote`, which defaults to **`false`**, so
/// `[^1]` is plain text unless a caller asks for the extension.
///
/// # `state.pos === 0` is not a fourth D3 site
///
/// It looks like one. `pos` is an absolute offset, so the guard fires at the
/// start of the block and cannot fire in a nested level, where `pos` is at
/// least the parent's marker length; a footnote may therefore open at the start
/// of `**[^1]**`'s content but not at the start of the block. That is the same
/// *shape* as D3 site 1 — an absolute offset used where a relative one might be
/// meant — so it was worked out rather than assumed, and it is **reproduced**.
///
/// Two reasons, and the first is the decisive one:
///
/// - **There is nothing here for the absolute offset to disagree with.** D3
///   site 1 is a bug because it indexes `originSrc`, the *child* string, at an
///   *absolute* offset — two things that describe different strings. This
///   compares `pos` against a literal `0` and reads no string at all, so there
///   is no second operand to be inconsistent with. `pos == 0` means exactly
///   what it says: the very beginning of the block text.
/// - **"The beginning of the block" is the right thing to test.** A `[^…]` in
///   that position is the label of a footnote *definition* line
///   (`[^1]: the note`), which belongs to the block layer, not an inline
///   reference. A nested level is by construction not the beginning of a
///   block, so the guard correctly does not fire there.
///
/// No `spec/divergences.json` entry, then — nothing diverges. Worth the
/// paragraph because "the other absolute-offset site was a bug" is a
/// reasonable thing for a later reader to assume, and this is where that
/// assumption is answered.
fn try_footnote(state: &mut LexState<'_>) -> bool {
    if state.pos == 0 || !state.syntax.footnote {
        return false;
    }

    let id = RuleId::FootnoteIdentifier;
    let Some(caps) = exec(rule(id), state.rest()) else {
        return false;
    };

    let whole = state.required(&caps, 0, id);
    let marker = state.required(&caps, 1, id);
    let content = state.required(&caps, 2, id);

    state.emit(
        TokenKind::FootnoteIdentifier { marker, content },
        whole,
        whole.end,
    );
    true
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
/// top-level call that is the whole string; a nested call passes the capture
/// group it is descending into, and every span the call produces is still an
/// absolute offset into `origin`.
///
/// S2 is where the nested call first happens — `strong`, `em` and `del`
/// tokenize their content — and it is what makes the cross-level half of
/// M1.md §4 C3 live rather than vacuous: [`check_tiling`] now has children to
/// check against their parents.
pub(crate) fn tokenizer_fac(
    origin: &str,
    level: Span,
    has_begin_rules: bool,
    top: bool,
    syntax: SyntaxOptions,
) -> Vec<Token> {
    let mut state = LexState {
        origin,
        level,
        pos: level.start,
        pending: None,
        tokens: Vec::new(),
        top,
        syntax,
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
/// `options.labels` is not read yet — the handlers that consult it are S3 —
/// and neither is `options.highlights`, whose post-pass is S6.
/// [`crate::tokenizer`] says so where a caller will see it.
pub(crate) fn tokenizer(src: &str, options: &TokenizerOptions) -> Vec<Token> {
    tokenizer_fac(
        src,
        Span::new(0, src.len()),
        options.has_begin_rules,
        true,
        options.syntax,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenize;

    fn types(src: &str) -> Vec<&'static str> {
        tokenize(src).iter().map(Token::type_str).collect()
    }

    /// The same, for a child level.
    fn types_of(tokens: &[Token]) -> Vec<&'static str> {
        tokens.iter().map(Token::type_str).collect()
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

    /// Nothing implemented can match here, so every rule falls through and the
    /// whole thing accumulates. That is the loop's default path and the one
    /// every unimplemented handler relies on.
    ///
    /// The input is S3–S5 only — links, images, HTML, autolinks. It used to
    /// include `**bold**` and `` `code` ``, and S2 is what took them out: a
    /// handler that starts working turns this test red, which is the point of
    /// keeping it pointed at whatever is *still* unimplemented. Narrow it again
    /// at S3, S4 and S5 until there is nothing left to put in it.
    #[test]
    fn unmatched_constructs_accumulate_as_text_rather_than_being_dropped() {
        let src = "[link](url) and ![img](src) and <span>x</span> and https://x.y";
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
    // The S2 handlers
    // -----------------------------------------------------------------------

    #[test]
    fn strong_and_em_carry_their_marker_and_tokenize_their_content() {
        let src = "a **bold** b";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["text", "strong", "text"]);

        let TokenKind::Strong(emphasis) = &tokens[1].kind else {
            panic!("expected a strong");
        };
        assert_eq!(emphasis.marker.of(src), "**");
        assert_eq!(emphasis.backlash.of(src), "");
        assert_eq!(types_of(&emphasis.children), ["text"]);
        assert_eq!(emphasis.children[0].raw.of(src), "bold");

        assert_eq!(types("*em*"), ["em"]);
        assert_eq!(types("__bold__"), ["strong"]);
        assert_eq!(types("_em_"), ["em"]);
    }

    /// Children carry **absolute** spans into the top-level text, not offsets
    /// into the substring they were tokenized from. That is muya's convention
    /// (`tokenizerFac` takes an absolute base) and it is what makes the
    /// cross-level tiling check meaningful.
    #[test]
    fn child_spans_are_absolute_offsets_into_the_top_level_text() {
        let src = "xx **a `b` c** yy";
        let tokens = tokenize(src);
        let TokenKind::Strong(emphasis) = &tokens[1].kind else {
            panic!("expected a strong");
        };
        let code = &emphasis.children[1];
        assert_eq!(code.type_str(), "inline_code");
        assert_eq!(code.raw.of(src), "`b`");
        // `xx **a ` is seven bytes, so the code span sits at 7..10 of the
        // top-level text and at 3..6 of the substring it was tokenized from.
        assert_eq!(code.raw, Span::new(7, 10), "absolute, not 3..6");
    }

    /// A rejected emphasis leaves its markers in the text rather than emitting
    /// a half-token, at both markers and at both lengths.
    ///
    /// `пристаням__стремятся__` is CommonMark example 387 and is the
    /// run-atomicity guard in `canOpenEmphasis`, not the `return` at
    /// `lexer.ts:186` — see [`try_strong_em`], which records why that `return`
    /// has no known observable consequence. `**` has no intra-word rule, so
    /// `x**a**y` really is a strong span; only `_` is restricted.
    #[test]
    fn a_rejected_emphasis_leaves_its_markers_as_text() {
        assert_eq!(types("пристаням__стремятся__"), ["text"]);
        assert_eq!(types("a__b__c"), ["text"], "`_` may not open intra-word");
        assert_eq!(
            types("x**a**y"),
            ["text", "strong", "text"],
            "`*` may, and does"
        );
    }

    /// The `isLengthEven` gate sits **outside** the block that returns, so an
    /// odd backslash run really does fall through to the next rule — where, in
    /// this input, `em` cannot match either, and the `\*` ends up as a
    /// `backlash` token instead.
    ///
    /// Measured: muya gives `text("**a") backlash("\") text("*")`.
    #[test]
    fn an_odd_backslash_run_is_not_stopped_by_the_return() {
        let src = r"**a\**";
        assert_eq!(types(src), ["text", "backlash", "text"]);
        assert_eq!(raws(src), ["**a", "\\", "**"]);

        // An even run closes the span, and the run is reported on the token.
        let src = r"**a\\**";
        let tokens = tokenize(src);
        assert_eq!(types(src), ["strong"]);
        let TokenKind::Strong(emphasis) = &tokens[0].kind else {
            panic!("expected a strong");
        };
        assert_eq!(emphasis.backlash.of(src), r"\\");
    }

    #[test]
    fn the_four_chunk_rules_have_the_precedence_of_their_array() {
        assert_eq!(types("`code`"), ["inline_code"]);
        assert_eq!(types("~~struck~~"), ["del"]);
        assert_eq!(types(":smile:"), ["emoji"]);
        assert_eq!(types("$a+b$"), ["inline_math"]);
    }

    /// `del` is the only chunk with children; the other three carry content
    /// verbatim.
    #[test]
    fn del_tokenizes_its_content_and_the_other_three_do_not() {
        let src = "~~a `b` c~~";
        let tokens = tokenize(src);
        let TokenKind::Del(emphasis) = &tokens[0].kind else {
            panic!("expected a del");
        };
        assert_eq!(
            types_of(&emphasis.children),
            ["text", "inline_code", "text"]
        );

        let src = "`a *b* c`";
        let tokens = tokenize(src);
        let TokenKind::InlineCode(chunk) = &tokens[0].kind else {
            panic!("expected an inline_code");
        };
        assert_eq!(chunk.content.of(src), "a *b* c");
        assert!(tokens[0].children().is_none());
    }

    /// `inline_code` and `emoji` have no third capture group, so their
    /// `isLengthEven` gate is `isLengthEven(undefined)` — vacuously true. Kept
    /// vacuous rather than "fixed" into something that can fail.
    #[test]
    fn the_backlash_gate_is_vacuous_for_the_two_rules_without_a_third_group() {
        let src = "`a\\`";
        let tokens = tokenize(src);
        let TokenKind::InlineCode(chunk) = &tokens[0].kind else {
            panic!("expected an inline_code even though the content ends in a backslash");
        };
        assert_eq!(chunk.backlash, None, "no third group to report");
        assert_eq!(chunk.content.of(src), "a\\");
    }

    #[test]
    fn super_sub_script_is_one_token_type_distinguished_by_its_marker() {
        for (src, marker, content) in [("x^2^", "^", "2"), ("H~2~O", "~", "2")] {
            let tokens = tokenize(src);
            let token = tokens
                .iter()
                .find(|t| t.type_str() == "super_sub_script")
                .unwrap_or_else(|| panic!("no super_sub_script in {src:?}"));
            let TokenKind::SuperSubScript {
                marker: m,
                content: c,
            } = token.kind
            else {
                unreachable!()
            };
            assert_eq!(m.of(src), marker);
            assert_eq!(c.of(src), content);
        }
    }

    #[test]
    fn super_sub_script_can_be_switched_off() {
        let options = TokenizerOptions {
            syntax: crate::SyntaxOptions {
                super_sub_script: false,
                ..crate::SyntaxOptions::default()
            },
            ..TokenizerOptions::muya_default()
        };
        let tokens = crate::tokenizer("x^2^", &options);
        assert_eq!(
            tokens.iter().map(Token::type_str).collect::<Vec<_>>(),
            ["text"]
        );
    }

    /// Footnotes are **off** by default, and even switched on they may not open
    /// at offset 0 of the block. See [`try_footnote`] for why that is not a
    /// fourth D3 site.
    #[test]
    fn footnotes_are_off_by_default_and_never_open_at_offset_zero() {
        assert_eq!(types("a [^1] b"), ["text"], "off by default");

        let with_footnotes = TokenizerOptions {
            syntax: crate::SyntaxOptions {
                footnote: true,
                ..crate::SyntaxOptions::default()
            },
            ..TokenizerOptions::muya_default()
        };
        let kinds = |src| {
            crate::tokenizer(src, &with_footnotes)
                .iter()
                .map(Token::type_str)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds("a [^1] b"), ["text", "footnote_identifier", "text"]);
        assert_eq!(
            kinds("[^1] b"),
            ["text"],
            "pos == 0: not an inline footnote"
        );
        // …and in a nested level `pos` is never 0, so one may open at the very
        // start of the content. `pos` is absolute in both engines, so this is
        // muya's behaviour and not a consequence of the port's layout.
        assert_eq!(kinds("**[^1]**"), ["strong"]);
        let tokens = crate::tokenizer("**[^1]**", &with_footnotes);
        let TokenKind::Strong(emphasis) = &tokens[0].kind else {
            panic!("expected a strong");
        };
        assert_eq!(types_of(&emphasis.children), ["footnote_identifier"]);
    }

    // -----------------------------------------------------------------------
    // M1.md §5 D3 site 1 — the nested emoji boundary
    // -----------------------------------------------------------------------

    /// The registered fix, on the two inputs `spec/divergences.json` names.
    ///
    /// muya emits **no emoji** for either: `tryChunks` reads
    /// `state.originSrc[state.pos - 1]`, and in a nested call `originSrc` is
    /// the child substring while `pos` is an absolute offset, so the
    /// word-boundary test lands on the wrong character. Measured against the
    /// running engine — `**a :smile:**` gives one `text` child and
    /// `**:smile:**` gives one `text` child.
    ///
    /// Register rule 2: *"every entry names concrete inputs, and those inputs
    /// become Rust tests asserting the FIXED behaviour."* This is that test.
    #[test]
    fn a_nested_emoji_survives_the_registered_boundary_fix() {
        let src = "**a :smile:**";
        let tokens = tokenize(src);
        let TokenKind::Strong(emphasis) = &tokens[0].kind else {
            panic!("expected a strong");
        };
        assert_eq!(types_of(&emphasis.children), ["text", "emoji"]);
        assert_eq!(emphasis.children[1].raw.of(src), ":smile:");

        let src = "**:smile:**";
        let tokens = tokenize(src);
        let TokenKind::Strong(emphasis) = &tokens[0].kind else {
            panic!("expected a strong");
        };
        assert_eq!(
            types_of(&emphasis.children),
            ["emoji"],
            "at pos == level.start there is no preceding character at this \
             level, which counts as a boundary"
        );
    }

    /// The rest of the registered class: the same fix at the other three
    /// container rules, and the case where muya's misread character is a digit
    /// rather than a letter.
    #[test]
    fn the_boundary_fix_applies_at_every_nesting_site() {
        for src in ["__a :smile:__", "~~a :smile:~~", "**a :smile: b**"] {
            let tokens = tokenize(src);
            let children = tokens[0].children().expect("a container");
            assert!(
                children.iter().any(|t| t.type_str() == "emoji"),
                "no emoji in {src:?}: {:?}",
                types_of(children)
            );
        }

        // `**:100:**` — muya reads `originSrc[1]`, which is `1`, a word
        // character, and suppresses the emoji. Nothing at this level precedes
        // the `:`.
        let tokens = tokenize("**:100:**");
        assert_eq!(types_of(tokens[0].children().expect("a strong")), ["emoji"]);
    }

    /// The other side of the fix: a *genuine* word boundary still suppresses
    /// the emoji, at every level. The four `emojiWordBoundary` spec cases are
    /// all top level; these are the nested equivalents.
    #[test]
    fn a_real_word_character_still_suppresses_a_nested_emoji() {
        for src in ["**a:smile:**", "**12:00-14:00**", "*hello:smile:*"] {
            let tokens = tokenize(src);
            let children = tokens[0].children().expect("a container");
            assert_eq!(
                types_of(children),
                ["text"],
                "{src:?} must not produce an emoji"
            );
        }
    }

    /// D3's note that `*x :smile:*` and `[a :smile:](u)` "survive by luck" in
    /// muya: they agree with the port today, and they must keep agreeing, so
    /// they are pinned as ordinary tests rather than left to the register.
    #[test]
    fn the_inputs_muya_gets_right_by_luck_are_unchanged_by_the_fix() {
        let src = "*x :smile:*";
        let tokens = tokenize(src);
        let TokenKind::Em(emphasis) = &tokens[0].kind else {
            panic!("expected an em");
        };
        assert_eq!(types_of(&emphasis.children), ["text", "emoji"]);
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
