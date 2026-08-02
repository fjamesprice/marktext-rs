//! `generator` — the inverse of the tokenizer (`lexer.ts:900-927`).
//!
//! # Why this is here at S1 rather than S6
//!
//! M1.md §6 schedules `generator` in S6 and then says, in the paragraph under
//! the stage table, to build it as soon as S1 lands instead:
//!
//! > S6's round-trip property is the strongest single check in the milestone
//! > and it is nearly free — `generator` is a concatenation of `raw` fields.
//! > Build it as soon as S1 lands rather than waiting for S6; it will catch
//! > tiling errors in every stage after it.
//!
//! That is the whole argument. `generator(tokenize(s)) == s` is the tiling
//! invariant restated as an equality over strings, it costs one function, and
//! it turns every future stage's new handler into a tested one the moment it
//! is written. `tokensToPlainText` and the escape-character *map* stay in S6;
//! only this half moves.
//!
//! # Two modes
//!
//! muya's signature is `generator(tokens, rebuildWrappers = false)`. Rust has
//! no default arguments, so the default is [`generator`] and the opt-in is
//! [`generator_rebuilding_wrappers`]. The comment at `lexer.ts:918` explains
//! why it is opt-in: only `format()` mutates a wrapper's children, while
//! `backspaceHandler` trims a marker off `raw` and needs it echoed verbatim.
//!
//! **The round-trip property uses the default mode, and must.** Rebuilding is
//! deliberately not an identity: a `strong` token's `raw` is
//! `marker + content + backlash + marker`, and rebuilding emits
//! `marker + children + marker`, dropping the backslash run. That is correct
//! — the run is escape syntax the children already account for — but it means
//! `generator_rebuilding_wrappers` is a *normaliser*, not a round-trip.

use crate::token::{Token, TokenKind};

/// `generator(tokens)` — concatenate every token's `raw`.
///
/// Takes `src` because a [`Token`]'s `raw` is a [`crate::Span`] rather than an
/// owned `String`; it must be the same text that was tokenized.
///
/// # Panics
///
/// If `src` is not the text the tokens came from, so that a span falls out of
/// bounds or off a `char` boundary. Passing the wrong string is the only way
/// to get there.
pub fn generator(src: &str, tokens: &[Token]) -> String {
    let mut result = String::new();
    for token in tokens {
        result.push_str(token.raw.of(src));
    }
    result
}

/// `generator(tokens, true)` — as [`generator`], but marker-wrapped tokens are
/// rebuilt from their children rather than echoed from a possibly stale `raw`.
///
/// marktext #2063: `format()` edits a wrapper's children in place and leaves
/// `raw` describing the text before the edit, so a serializer that trusts
/// `raw` writes the old content back out.
pub fn generator_rebuilding_wrappers(src: &str, tokens: &[Token]) -> String {
    let mut result = String::new();
    for token in tokens {
        result.push_str(&rebuild_wrapper_token(src, token));
    }
    result
}

/// `rebuildWrapperToken` (`lexer.ts:900`).
///
/// Only `strong`, `em`, `del` and `html_tag` are rebuilt. Link and image keep
/// their stored `raw` — muya's comment says so explicitly, and the reason is
/// that their destination and title are not represented in `children`, so
/// rebuilding would lose them.
fn rebuild_wrapper_token(src: &str, token: &Token) -> String {
    match &token.kind {
        TokenKind::Strong(emphasis) | TokenKind::Em(emphasis) | TokenKind::Del(emphasis) => {
            let marker = emphasis.marker.of(src);
            let mut out = String::from(marker);
            out.push_str(&generator_rebuilding_wrappers(src, &emphasis.children));
            out.push_str(marker);
            out
        }

        // muya tests `openTag != null && closeTag != null && children != null`.
        // `openTag` is set on both branches of `tryHtmlTag`, so the test that
        // does work is the other two: a comment has neither, and an element
        // with no content has `children: []` — which is truthy in JavaScript
        // and so *is* rebuilt, to `openTag + '' + closeTag`. `Some(vec![])`
        // reproduces that; see `HtmlTag`'s docs for why the distinction is
        // modelled at all.
        TokenKind::HtmlTag(tag) => match (tag.close_tag, &tag.children) {
            (Some(close_tag), Some(children)) => {
                let mut out = String::from(tag.open_tag.of(src));
                out.push_str(&generator_rebuilding_wrappers(src, children));
                out.push_str(close_tag.of(src));
                out
            }
            _ => token.raw.of(src).to_string(),
        },

        _ => token.raw.of(src).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{Emphasis, HtmlTag, HtmlTagName, Span};
    use crate::tokenize;

    /// The round-trip property, on everything S1 can produce. `tests/` runs
    /// the same statement over the corpus; this is the fast unit-level copy so
    /// a break shows up in `cargo test -p mt-inline --lib`.
    #[test]
    fn tokenize_then_generate_is_the_identity() {
        let cases = [
            "",
            "plain text",
            "# Heading ##  ",
            "```rust",
            "$$",
            "---",
            r"an escape \* and a double \\ and a dollar \$",
            "an entity &amp; and a non-entity &notreal;",
            "soft\nbreak",
            "hard  \nbreak",
            "blank\n\nline",
            "中文 with a \u{feff}BOM and 🇬🇧 flags",
            "**not yet bold** and `not yet code`",
        ];
        for src in cases {
            assert_eq!(generator(src, &tokenize(src)), src, "input: {src:?}");
        }
    }

    /// The default mode is a concatenation of `raw`, full stop — it does not
    /// consult `children`, so it is the mode the round-trip property can rely
    /// on even while a token's children are being rewritten.
    #[test]
    fn the_default_mode_ignores_children_entirely() {
        let src = "**ab**";
        let strong = Token {
            kind: TokenKind::Strong(Emphasis {
                marker: Span::new(0, 2),
                // Deliberately wrong: not what `raw` describes.
                children: Vec::new(),
                backlash: Span::new(4, 4),
            }),
            range: Span::new(0, 6),
            raw: Span::new(0, 6),
            highlights: Vec::new(),
        };
        assert_eq!(generator(src, &[strong]), "**ab**");
    }

    /// …and the opt-in mode does the opposite, which is the point of #2063.
    #[test]
    fn rebuilding_a_wrapper_uses_its_children_and_not_its_raw() {
        let src = "**stale**fresh";
        let child = Token {
            kind: TokenKind::Text {
                content: Span::new(9, 14),
            },
            range: Span::new(9, 14),
            raw: Span::new(9, 14),
            highlights: Vec::new(),
        };
        let strong = Token {
            kind: TokenKind::Strong(Emphasis {
                marker: Span::new(0, 2),
                children: vec![child],
                backlash: Span::new(7, 7),
            }),
            range: Span::new(0, 9),
            raw: Span::new(0, 9),
            highlights: Vec::new(),
        };
        assert_eq!(generator(src, std::slice::from_ref(&strong)), "**stale**");
        assert_eq!(
            generator_rebuilding_wrappers(src, &[strong]),
            "**fresh**",
            "the whole point of rebuildWrapperToken"
        );
    }

    /// A comment has no `closeTag` and no `children`, so it falls through to
    /// its `raw` even in rebuild mode.
    #[test]
    fn a_comment_html_tag_is_never_rebuilt() {
        let src = "<!-- c -->";
        let comment = Token {
            kind: TokenKind::HtmlTag(HtmlTag {
                tag: HtmlTagName::Comment,
                open_tag: Span::new(0, 10),
                close_tag: None,
                content: None,
                attrs: Vec::new(),
                children: None,
            }),
            range: Span::new(0, 10),
            raw: Span::new(0, 10),
            highlights: Vec::new(),
        };
        assert_eq!(generator_rebuilding_wrappers(src, &[comment]), src);
    }
}
