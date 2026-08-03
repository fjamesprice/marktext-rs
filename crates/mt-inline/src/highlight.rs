//! Search highlights — `union` (`utils/index.ts:68`) and the post-pass that
//! attaches them to tokens (`lexer.ts:873`).
//!
//! `tokenizer()` takes a list of highlights — the find bar's matches over the
//! *block's* text — and, after tokenizing, walks the whole tree intersecting
//! every token's range with every highlight. A token that overlaps a match
//! carries the clipped intersection so the renderer can mark exactly the
//! overlapping run rather than the whole token.
//!
//! # This is the only part of the tokenizer that is not about tokenizing
//!
//! It reads no rules, consumes no input and cannot change a token boundary. It
//! is a second pass over a finished tree, and `lexer.ts:890` skips it entirely
//! when the highlight list is empty:
//!
//! ```js
//! if (highlights.length)
//!     postTokenizer(tokens);
//! ```
//!
//! That guard is why M1.md §5 D7 decided [`Token::highlights`] stays a plain
//! `Vec` rather than §3's `SmallVec<[Highlight; 2]>`: the field is empty except
//! while a search is running, and an inline capacity of two on every token in a
//! 5 MB document is a real memory cost for a field that is almost always empty.
//! The guard is reproduced here rather than being an optimisation — the walk is
//! O(tokens × highlights) and a document with no search should not pay for it.
//!
//! # Units
//!
//! Highlights arrive in the **same units as token ranges**, which in this crate
//! is UTF-8 bytes (M1.md §5 D1) and in muya is UTF-16 code units. [`union`] is
//! pure arithmetic and unit-agnostic, so nothing inside it changes; what
//! changes is the boundary. **A caller handing this crate offsets computed by a
//! UTF-16 search will get wrong highlights on any non-ASCII line**, silently,
//! because every offset is still a valid `usize`. Converting is the caller's
//! job, at the same boundary D1 puts every other conversion.

use crate::token::{Highlight, Span, Token};

/// `union(tokenRange, highlight)` — the intersection, or `None`.
///
/// ```js
/// if (!(tEnd <= lStart || lEnd <= tStart)) { … }
/// return null;
/// ```
///
/// Three properties of that test, all of which the port must keep:
///
/// - **Overlap is strict.** The rejection test is `<=`, not `<`, so ranges that
///   merely *touch* at a point do not intersect: `[0,3)` and `[3,6)` give
///   `None`. A highlight ending exactly where a token begins produces no
///   intersection on either side of the boundary.
/// - **A degenerate range can still intersect.** An empty range strictly inside
///   the other passes the test and yields an empty intersection — `[5,5)`
///   against `[0,10)` is `Some([5,5))`. Only a *touching* empty range fails.
///   The tokenizer never produces an empty token range, so this is reachable
///   only through a hand-built highlight; it is written down because the
///   obvious reading of "strict overlap" is that nothing empty ever
///   intersects, and that reading is wrong.
/// - **`active` is copied, never merged.** It comes from the highlight and
///   describes the *match*, not the intersection: `Some(true)` is the match the
///   find bar has stepped to, and muya's `undefined` — [`None`] here — is a
///   genuine third state meaning "not the active match", distinct from
///   `Some(false)`.
///
/// muya's two branches (`lStart < tStart` and otherwise) are `max` written out;
/// the `end` expression is already `min`.
pub fn union(token_range: Span, highlight: Highlight) -> Option<Highlight> {
    let (t_start, t_end) = (token_range.start, token_range.end);
    let (l_start, l_end) = (highlight.span.start, highlight.span.end);

    if t_end <= l_start || l_end <= t_start {
        return None;
    }

    Some(Highlight {
        span: Span::new(t_start.max(l_start), t_end.min(l_end)),
        active: highlight.active,
    })
}

/// `postTokenizer` (`lexer.ts:873`) — attach `highlights` to every token that
/// overlaps one, recursively.
///
/// Called by [`crate::tokenizer`] and skipped entirely when `highlights` is
/// empty, per `lexer.ts:890`.
///
/// # Order
///
/// A token's highlights are pushed in `highlights` order, so the list a token
/// carries is a filtered copy of the input list rather than a sorted one. That
/// is observable — the renderer emits them in order — so the port keeps it
/// rather than sorting by position.
///
/// # Absent versus empty
///
/// muya creates `token.highlights` on the *first* push, so a token that
/// overlaps nothing has no `highlights` key at all, while this port gives every
/// token an empty `Vec`. That is M1.md §5 D7 working as decided — the field is
/// not optional here — but it is a per-field `undefined`/`[]` normalisation
/// that a token-stream comparator has to know about, and it is recorded in
/// `xtask/src/divergences.rs` beside the `|| ''` sites for the same reason.
pub(crate) fn post_tokenizer(tokens: &mut [Token], highlights: &[Highlight]) {
    for token in tokens {
        for light in highlights {
            if let Some(intersection) = union(token.range, *light) {
                token.highlights.push(intersection);
            }
        }

        // muya guards with `'children' in token && token.children &&
        // Array.isArray(token.children)`. The first two clauses are the same
        // question in Rust — a leaf has no children field and an `html_tag`
        // comment has `children: undefined` — and both are `None` here. The
        // third cannot fail: nothing assigns a non-array to `children`.
        if let Some(children) = token.children_mut() {
            post_tokenizer(children, highlights);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;
    use crate::{TokenizerOptions, tokenize, tokenizer};

    fn light(start: usize, end: usize) -> Highlight {
        Highlight {
            span: Span::new(start, end),
            active: None,
        }
    }

    #[test]
    fn the_intersection_is_clipped_to_both() {
        assert_eq!(
            union(Span::new(2, 8), light(0, 4)),
            Some(light(2, 4)),
            "highlight starts before the token"
        );
        assert_eq!(
            union(Span::new(2, 8), light(6, 20)),
            Some(light(6, 8)),
            "highlight ends after the token"
        );
        assert_eq!(
            union(Span::new(2, 8), light(3, 5)),
            Some(light(3, 5)),
            "highlight inside the token"
        );
        assert_eq!(
            union(Span::new(2, 8), light(0, 20)),
            Some(light(2, 8)),
            "token inside the highlight"
        );
    }

    /// Touching at a point is not an intersection — the reason the test is
    /// `<=` and not `<`.
    #[test]
    fn ranges_that_only_touch_do_not_intersect() {
        assert_eq!(union(Span::new(3, 6), light(0, 3)), None);
        assert_eq!(union(Span::new(0, 3), light(3, 6)), None);
        assert_eq!(union(Span::new(3, 6), light(7, 9)), None);
    }

    /// The half of "strict overlap" that is easy to state backwards: an empty
    /// range *strictly inside* the other does intersect, and yields an empty
    /// intersection.
    #[test]
    fn an_empty_range_intersects_when_it_is_strictly_inside() {
        assert_eq!(union(Span::new(5, 5), light(0, 10)), Some(light(5, 5)));
        assert_eq!(union(Span::new(0, 10), light(5, 5)), Some(light(5, 5)));
        // …and does not when it merely touches an edge.
        assert_eq!(union(Span::new(0, 0), light(0, 10)), None);
        assert_eq!(union(Span::new(10, 10), light(0, 10)), None);
    }

    /// `active` is the highlight's, unchanged, and its three states are three
    /// states.
    #[test]
    fn active_is_carried_through_untouched() {
        for active in [None, Some(false), Some(true)] {
            let intersected = union(
                Span::new(0, 10),
                Highlight {
                    span: Span::new(2, 4),
                    active,
                },
            );
            assert_eq!(intersected.expect("overlaps").active, active);
        }
    }

    #[test]
    fn the_post_pass_is_skipped_when_there_are_no_highlights() {
        let src = "**bold**";
        for token in tokenize(src) {
            assert!(token.highlights.is_empty());
        }
    }

    /// The walk reaches children, and a child's highlight is clipped to the
    /// child rather than to its parent.
    #[test]
    fn highlights_reach_nested_tokens() {
        let src = "a **bold** b";
        let options = TokenizerOptions::muya_default().with_highlights(vec![light(0, 7)]);
        let tokens = tokenizer(src, &options);

        let text = &tokens[0];
        assert_eq!(text.highlights, vec![light(0, 2)], "`a ` clipped to itself");

        let strong = &tokens[1];
        assert_eq!(strong.range, Span::new(2, 10));
        assert_eq!(strong.highlights, vec![light(2, 7)]);

        let TokenKind::Strong(emphasis) = &strong.kind else {
            panic!("`**bold**` is strong");
        };
        let child = &emphasis.children[0];
        assert_eq!(child.range, Span::new(4, 8));
        assert_eq!(
            child.highlights,
            vec![light(4, 7)],
            "the child is clipped to the child, not to its parent"
        );
    }

    /// Several highlights on one token keep the caller's order.
    #[test]
    fn a_token_carries_its_highlights_in_input_order() {
        let src = "abcdefgh";
        let options = TokenizerOptions::muya_default().with_highlights(vec![
            light(4, 6),
            light(0, 2),
            Highlight {
                span: Span::new(2, 3),
                active: Some(true),
            },
        ]);
        let tokens = tokenizer(src, &options);
        assert_eq!(
            tokens[0].highlights,
            vec![
                light(4, 6),
                light(0, 2),
                Highlight {
                    span: Span::new(2, 3),
                    active: Some(true),
                },
            ]
        );
    }

    /// A highlight that misses every token leaves the tree untouched — which is
    /// the state D7 says is the common one.
    #[test]
    fn a_highlight_outside_the_block_attaches_to_nothing() {
        let src = "abc";
        let options = TokenizerOptions::muya_default().with_highlights(vec![light(10, 20)]);
        for token in tokenizer(src, &options) {
            assert!(token.highlights.is_empty());
        }
    }

    /// The post-pass may not disturb tokenization: same tokens, plus
    /// highlights. Stated because it is a *post*-pass and a port that ran it
    /// too early would be hard to see.
    #[test]
    fn the_post_pass_changes_nothing_but_the_highlights() {
        let src = "a **b** [c](d) <e>f</e> &amp; \\*";
        let plain = tokenize(src);
        let options = TokenizerOptions::muya_default().with_highlights(vec![light(0, 100)]);
        let lit = tokenizer(src, &options);

        assert_eq!(plain.len(), lit.len());
        for (a, b) in plain.iter().zip(lit.iter()) {
            assert_eq!(a.type_str(), b.type_str());
            assert_eq!(a.range, b.range);
            assert_eq!(a.raw, b.raw);
            assert!(a.highlights.is_empty());
            assert!(!b.highlights.is_empty());
        }
    }
}
