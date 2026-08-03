//! Marker reveal — RUST-REWRITE-PLAN.md §3.1, as corrected by M1.md §5 D8.
//!
//! > The rule that makes MarkText feel like MarkText.
//!
//! In the editor, a token renders *decorated* (`**bold**` shows as **bold**)
//! until the caret is on it, at which point its markers appear so they can be
//! edited. The reference implementation is
//! `inlineRenderer/renderer/index.ts:111`:
//!
//! ```js
//! private _checkConflicted(block, token, cursor = {}) {
//!     const anchor = cursor.anchor || cursor.start;
//!     const focus = cursor.focus || cursor.end;
//!     if (!anchor || !focus || (cursor.block && cursor.block !== block))
//!         return false;
//!     const { start, end } = token.range;
//!     return conflict([start, end], [anchor.offset, anchor.offset])
//!         || conflict([start, end], [focus.offset, focus.offset]);
//! }
//! ```
//!
//! # §3.1's sketch is wrong in two of its three clauses
//!
//! §3.1 says: *"A token's markers reveal when the caret is inside `token.range`
//! (inclusive of edges) or the selection intersects it. … Ancestor tokens
//! reveal when a descendant reveals."* The full accounting is M1.md §5 D8;
//! the short version:
//!
//! 1. **"the caret is inside `token.range`, inclusive of edges"** — correct.
//!    `conflict` is `!(a1[1] < a2[0] || a2[1] < a1[0])`, which for a degenerate
//!    `[o, o]` is exactly [`Span::contains_offset`].
//! 2. **"or the selection intersects it"** — **false.** The two offsets are
//!    tested *separately*, each as its own degenerate range, so a selection
//!    running from before a token to after it reveals **nothing**: neither
//!    endpoint lands on it. Only an endpoint on or inside a token reveals that
//!    token. Measured, not read — see the tests below.
//! 3. **"ancestors reveal when a descendant reveals"** — **false, and there is
//!    no mechanism for it.** The only channel is `getClassName(outerClass, …)`,
//!    which is *parent → child* (the wrong direction) and which **overrides**
//!    the child's own computation rather than combining with it
//!    (`outerClass || (…)`). In the pinned revision nothing ever gives it a
//!    value — all six `dispatch` call sites omit the key — so it is
//!    permanently `undefined`: 193 `getClassName` calls across seven booted
//!    documents, every one with `outerClass === undefined`. The legacy
//!    `packages/muyajs` copy shows it *used* to work, before the refactor to an
//!    options object dropped the field. D8 has the enumeration.
//!
//! # So the predicate is per-token, and that is exact rather than a compromise
//!
//! §3.1 asks whether [`marker_state`] can keep a per-token signature at all,
//! given clause 3. It can: with no propagation in either direction, the reveal
//! decision for a token reads nothing but that token's `range` and the cursor.
//! No ancestor chain, no descendant scan, no order dependence — which is also
//! what makes it *"cheap to test headlessly"*, as §3.1 wants.
//!
//! What changes is the *arguments*. §3.1 sketches
//! `marker_state(token, caret: Option<usize>, sel: Option<Range<usize>>)`, and
//! that decomposition encodes clause 2: a caret and a selection as separate
//! things, with the selection to be intersected. The engine has neither — it
//! has an anchor offset and a focus offset, equal when the selection is
//! collapsed. [`Cursor`] is those two, and a caret is
//! [`Cursor::collapsed`].
//!
//! # What is deliberately *not* here
//!
//! Two renderer behaviours sit on top of this predicate, and neither belongs to
//! `mt-inline` (§1: no renderer, ever). They are recorded because a future
//! renderer that omits them will look correct and feel wrong:
//!
//! - **`header.ts:17` tests a synthetic range.** It calls `getClassName` with
//!   `{ range: { start, end: end - content.length } }` — the marker extent
//!   alone, not the token's. So an ATX heading's `#`s reveal when the caret is
//!   on the `#`s, not when it is anywhere in the heading text.
//! - **`htmlTag.ts:121` overrides the predicate for a childless tag.**
//!   `children?.length ? getClassName(…) : CLASS_NAMES.MU_GRAY` — a void or
//!   empty element is *always* revealed, cursor or no cursor.
//!
//! Both are choices about *which range to test* and *whether to test at all*.
//! The predicate itself is unconditional, and this module is the predicate.

use crate::token::{Span, Token};

/// Whether a token's markers are shown.
///
/// §3.1's enum, unchanged. In the renderer these are the two class names
/// `getClassName` returns: `MU_GRAY` for [`MarkerState::Revealed`] and
/// `MU_HIDE` for [`MarkerState::Hidden`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerState {
    /// Rendered decorated — the markers are not shown.
    Hidden,
    /// The markers are shown, so they can be edited.
    Revealed,
}

/// A selection in the block being rendered, as two offsets into its text.
///
/// muya's `IRenderCursor` carries `{ start, end, anchor, focus, block }` and
/// `_checkConflicted` resolves it to two `INodeOffset`s: `cursor.anchor ||
/// cursor.start` and `cursor.focus || cursor.end`. Both of those pairs mean the
/// same thing to this predicate — the difference is DOM-selection bookkeeping
/// upstream — so the port carries one pair.
///
/// **A caret is a collapsed selection**, `anchor == focus`; see
/// [`Cursor::collapsed`]. There is no separate caret concept in the engine, and
/// §3.1's sketch inventing one is what made its second clause look plausible.
///
/// # The `block` field is the caller's, not this crate's
///
/// `_checkConflicted` returns `false` outright when `cursor.block` is set and
/// is not the block being rendered — a cursor in another paragraph reveals
/// nothing here. `mt-inline` has no blocks (§1), so that test cannot live in
/// this crate. It becomes the caller's [`Option`]: pass `None` for a cursor
/// that belongs elsewhere (or for no cursor at all), which is the same answer
/// `_checkConflicted` gives. `an_absent_cursor_reveals_nothing` pins it.
///
/// Offsets are UTF-8 bytes, like every other offset in this crate (M1.md §5
/// D1). They need not lie on `char` boundaries — the predicate only compares
/// them — but a caller feeding it UTF-16 offsets will get plausible wrong
/// answers rather than a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cursor {
    /// Where the selection was started.
    pub anchor: usize,
    /// Where it currently ends. Equal to `anchor` when collapsed.
    pub focus: usize,
}

impl Cursor {
    /// A caret: a selection whose two ends coincide.
    pub const fn collapsed(offset: usize) -> Self {
        Self {
            anchor: offset,
            focus: offset,
        }
    }

    /// A selection from `anchor` to `focus`. Not normalised — the engine never
    /// compares the two to each other, so a backwards selection is not a
    /// different thing.
    pub const fn new(anchor: usize, focus: usize) -> Self {
        Self { anchor, focus }
    }
}

/// Whether `token`'s markers reveal for `cursor` — `_checkConflicted`
/// (`renderer/index.ts:111`).
///
/// `None` means there is no cursor applicable to this block, which reveals
/// nothing; see [`Cursor`].
///
/// Reveals when the token's range contains **either endpoint**, inclusive of
/// both edges. It does *not* reveal for a selection that merely spans the
/// token — see this module's header, clause 2.
pub fn marker_state(token: &Token, cursor: Option<Cursor>) -> MarkerState {
    marker_state_of_range(token.range, cursor)
}

/// [`marker_state`], over a bare range.
///
/// For a caller that tests a range the token does not carry. muya has exactly
/// one — `header.ts:17` builds a **synthetic token** whose range is the marker
/// extent rather than the heading's, so that a heading's `#`s reveal for a
/// caret on the `#`s and not for one anywhere in the text. This exists so that
/// a renderer can express it without fabricating a [`Token`], and it is the
/// reason the predicate is not simply inlined into [`marker_state`].
pub fn marker_state_of_range(range: Span, cursor: Option<Cursor>) -> MarkerState {
    match cursor {
        Some(cursor) if reveals(range, cursor) => MarkerState::Revealed,
        _ => MarkerState::Hidden,
    }
}

/// `conflict([start, end], [o, o]) || conflict([start, end], [o', o'])`.
///
/// `conflict` (`utils/index.ts:64`) is `!(a1[1] < a2[0] || a2[1] < a1[0])`, and
/// against a degenerate `[o, o]` that is `start <= o && o <= end` — which is
/// [`Span::contains_offset`], inclusive at both edges. **The two endpoints are
/// separate tests**, which is what makes a selection spanning the token reveal
/// nothing.
fn reveals(range: Span, cursor: Cursor) -> bool {
    range.contains_offset(cursor.anchor) || range.contains_offset(cursor.focus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;
    use crate::tokenize;

    /// `**bold**`, whose `range` is the whole construct including markers.
    fn strong() -> Token {
        let tokens = tokenize("**bold**");
        assert!(matches!(tokens[0].kind, TokenKind::Strong(_)));
        tokens[0].clone()
    }

    #[test]
    fn a_caret_inside_the_token_reveals_it() {
        let token = strong();
        assert_eq!(token.range, Span::new(0, 8));
        for offset in 1..8 {
            assert_eq!(
                marker_state(&token, Some(Cursor::collapsed(offset))),
                MarkerState::Revealed,
                "offset {offset}"
            );
        }
    }

    /// §3.1's one correct clause: *inclusive* of both edges. A caret sitting
    /// just before the opening `*` reveals, which is what makes typing at the
    /// start of a bold run feel right.
    #[test]
    fn both_edges_count() {
        let token = strong();
        assert_eq!(
            marker_state(&token, Some(Cursor::collapsed(0))),
            MarkerState::Revealed
        );
        assert_eq!(
            marker_state(&token, Some(Cursor::collapsed(8))),
            MarkerState::Revealed
        );
        assert_eq!(
            marker_state(&token, Some(Cursor::collapsed(9))),
            MarkerState::Hidden
        );
    }

    /// **The correction.** §3.1 says the markers reveal when "the selection
    /// intersects" the token. They do not: `_checkConflicted` tests the two
    /// endpoints separately, so a selection running from before the token to
    /// after it reveals nothing.
    ///
    /// Measured against the running engine over 2835 swept
    /// `(range, anchor, focus, cursor shape)` combinations, zero
    /// disagreements — M1.md §5 D8.
    #[test]
    fn a_selection_that_spans_the_token_does_not_reveal_it() {
        let src = "a **bold** b";
        let tokens = tokenize(src);
        let token = &tokens[1];
        assert_eq!(token.range, Span::new(2, 10));

        assert_eq!(
            marker_state(token, Some(Cursor::new(0, 12))),
            MarkerState::Hidden,
            "the whole line selected, and the token still hides"
        );
        assert_eq!(
            marker_state(token, Some(Cursor::new(1, 11))),
            MarkerState::Hidden
        );
        // …but an endpoint landing on it does reveal it, from either end.
        assert_eq!(
            marker_state(token, Some(Cursor::new(0, 5))),
            MarkerState::Revealed
        );
        assert_eq!(
            marker_state(token, Some(Cursor::new(5, 12))),
            MarkerState::Revealed
        );
        // …including exactly on an edge.
        assert_eq!(
            marker_state(token, Some(Cursor::new(0, 2))),
            MarkerState::Revealed
        );
        assert_eq!(
            marker_state(token, Some(Cursor::new(10, 12))),
            MarkerState::Revealed
        );
    }

    /// A selection is not normalised, so `new(a, f)` and `new(f, a)` decide the
    /// same way. muya never compares the two offsets to each other.
    #[test]
    fn a_backwards_selection_decides_the_same_way() {
        let token = strong();
        assert_eq!(
            marker_state(&token, Some(Cursor::new(4, 20))),
            marker_state(&token, Some(Cursor::new(20, 4)))
        );
        assert_eq!(
            marker_state(&token, Some(Cursor::new(4, 20))),
            MarkerState::Revealed
        );
    }

    /// `None` covers both "no selection" and "a selection in another block",
    /// which `_checkConflicted` treats identically.
    #[test]
    fn an_absent_cursor_reveals_nothing() {
        let token = strong();
        assert_eq!(marker_state(&token, None), MarkerState::Hidden);
    }

    /// **No propagation, in either direction** — §3.1's third clause. A caret
    /// inside a child does not reveal the parent, and a caret on the parent's
    /// marker does not reveal the child. Each token answers for itself.
    #[test]
    fn reveal_does_not_propagate_between_a_parent_and_its_child() {
        let src = "**a `c` b**";
        let tokens = tokenize(src);
        let strong = &tokens[0];
        assert_eq!(strong.range, Span::new(0, 11));

        let TokenKind::Strong(emphasis) = &strong.kind else {
            panic!("`**…**` is strong");
        };
        let code = emphasis
            .children
            .iter()
            .find(|t| matches!(t.kind, TokenKind::InlineCode(_)))
            .expect("`` `c` `` is a code span");
        assert_eq!(code.range, Span::new(4, 7));

        // A caret inside the code span reveals the code span — and the strong,
        // but only because the strong's range *contains* that offset, not
        // because the child told it to.
        let inside_child = Some(Cursor::collapsed(5));
        assert_eq!(marker_state(code, inside_child), MarkerState::Revealed);
        assert_eq!(marker_state(strong, inside_child), MarkerState::Revealed);

        // The case that separates the two readings: a caret on the strong's own
        // marker is outside the code span, and the code span stays hidden.
        let on_parent_marker = Some(Cursor::collapsed(1));
        assert_eq!(
            marker_state(strong, on_parent_marker),
            MarkerState::Revealed
        );
        assert_eq!(marker_state(code, on_parent_marker), MarkerState::Hidden);

        // And the case that would separate them the other way if ancestors
        // revealed with descendants — a sibling text token's caret.
        let after_child = Some(Cursor::collapsed(9));
        assert_eq!(marker_state(code, after_child), MarkerState::Hidden);
    }

    /// It is a pure function of `(range, cursor)` — §3.1's own reason for
    /// wanting it early. Same token, same cursor, same answer, and no token
    /// order dependence.
    #[test]
    fn it_is_pure_and_order_independent() {
        let src = "**a** *b* ~~c~~";
        let tokens = tokenize(src);
        let cursor = Some(Cursor::collapsed(7));

        let forward: Vec<MarkerState> = tokens.iter().map(|t| marker_state(t, cursor)).collect();
        let backward: Vec<MarkerState> = tokens
            .iter()
            .rev()
            .map(|t| marker_state(t, cursor))
            .collect();
        assert_eq!(
            forward,
            backward.into_iter().rev().collect::<Vec<_>>(),
            "the answer does not depend on the walk order"
        );
        assert_eq!(
            forward,
            tokens
                .iter()
                .map(|t| marker_state(t, cursor))
                .collect::<Vec<_>>()
        );
    }

    /// [`marker_state_of_range`] is what `header.ts` needs: it tests the marker
    /// extent rather than the token's own range, so a caret in the heading text
    /// leaves the `#` decorated.
    #[test]
    fn a_bare_range_is_what_the_headers_synthetic_token_needs() {
        let src = "# heading";
        let tokens = tokenize(src);
        let header = &tokens[0];
        let TokenKind::Header(begin) = &header.kind else {
            panic!("`# ` is a header");
        };
        let content = begin.content.expect("`header` has a content group");
        let marker_extent = Span::new(header.range.start, header.range.end - content.len());

        // The caret in the heading text: the token's own range says reveal, the
        // synthetic marker range says hide. The renderer uses the second.
        let in_the_text = Some(Cursor::collapsed(header.range.end));
        assert_eq!(marker_state(header, in_the_text), MarkerState::Revealed);
        assert_eq!(
            marker_state_of_range(marker_extent, in_the_text),
            MarkerState::Hidden
        );
        // …and on the marker itself, both agree.
        let on_the_marker = Some(Cursor::collapsed(1));
        assert_eq!(
            marker_state_of_range(marker_extent, on_the_marker),
            MarkerState::Revealed
        );
    }

    /// Offsets are bytes, so the multi-byte case is where a caller using the
    /// wrong unit gets a plausible wrong answer rather than a panic. Stated as
    /// a test because "plausible wrong answer" is the failure mode D1 warns
    /// about.
    #[test]
    fn offsets_are_bytes() {
        let src = "**中**";
        let tokens = tokenize(src);
        assert_eq!(tokens[0].range, Span::new(0, 7), "3 bytes, not 1 code unit");
        assert_eq!(
            marker_state(&tokens[0], Some(Cursor::collapsed(7))),
            MarkerState::Revealed
        );
        assert_eq!(
            marker_state(&tokens[0], Some(Cursor::collapsed(8))),
            MarkerState::Hidden
        );
    }
}
