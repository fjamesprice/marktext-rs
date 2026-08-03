//! `tokensToPlainText` — the reader-facing text of a token stream
//! (`lexer.ts:936`).
//!
//! Every marker, delimiter, URL and tag dropped: `**bold**` → `bold`,
//! `[text](url)` → `text`, `![alt](src)` → `alt`. muya's comment says what it
//! is *for*, and the reason matters because it constrains the port:
//!
//! > Mirrors the visible `textContent` a rendered heading yields, so a slug
//! > derived from this matches the anchor id the HTML export injects
//! > (`state/markdownToHtml.ts` injects ids from `heading.textContent`). The
//! > TOC uses it to show and slug headings by their rendered text instead of
//! > raw source (#4811).
//!
//! So this is not "strip the syntax, roughly". It is the *other* half of an
//! equality with the DOM: if it and the renderer disagree, a table-of-contents
//! entry links to an anchor that does not exist. That is why the arms below are
//! transcribed one for one rather than collapsed into "recurse if there are
//! children, else take the content".
//!
//! # It is the inverse of [`crate::generator`], not its sibling
//!
//! `generator` concatenates `raw` and reproduces the source byte for byte
//! ([the round-trip property](crate::generator)). This drops everything
//! `generator` keeps. Both walk the same tree; only one of them is a bijection.
//!
//! # Exhaustive on purpose
//!
//! muya's `switch` names twenty token types and lets the other six fall into
//! `default: break`. The Rust `match` **lists those six** instead of using a
//! wildcard, so a twenty-seventh token type added later is a compile error
//! rather than a token that silently contributes no text. That is the one
//! deliberate structural difference from the TypeScript in this file.

use crate::escape::escape_character;
use crate::token::{Token, TokenKind};

/// `tokensToPlainText(tokens)` (`lexer.ts:936`).
///
/// Takes `src` because a [`Token`]'s fields are [`crate::Span`]s rather than
/// owned strings; it must be the same text that was tokenized.
///
/// # Panics
///
/// If `src` is not the text the tokens came from, so that a span falls out of
/// bounds or off a `char` boundary — the same contract as
/// [`crate::generator`].
pub fn tokens_to_plain_text(src: &str, tokens: &[Token]) -> String {
    let mut result = String::new();
    push_plain_text(&mut result, src, tokens);
    result
}

fn push_plain_text(out: &mut String, src: &str, tokens: &[Token]) {
    for token in tokens {
        match &token.kind {
            // --- the content-bearing leaves -------------------------------
            // `text` carries `content` equal to its `raw`; the other five carry
            // the capture group between their markers.
            TokenKind::Text { content } => out.push_str(content.of(src)),
            TokenKind::InlineCode(chunk)
            | TokenKind::InlineMath(chunk)
            | TokenKind::Emoji(chunk) => out.push_str(chunk.content.of(src)),
            TokenKind::SuperSubScript { content, .. }
            | TokenKind::FootnoteIdentifier { content, .. } => out.push_str(content.of(src)),

            // --- the containers whose children are the text ---------------
            TokenKind::Strong(emphasis) | TokenKind::Em(emphasis) | TokenKind::Del(emphasis) => {
                push_plain_text(out, src, &emphasis.children);
            }
            TokenKind::Link(link) => push_plain_text(out, src, &link.children),
            TokenKind::ReferenceLink(link) => push_plain_text(out, src, &link.children),

            // --- images: the alt text, which is never tokenized -----------
            TokenKind::Image(image) => out.push_str(image.alt.of(src)),
            TokenKind::ReferenceImage(image) => out.push_str(image.alt.of(src)),

            // --- raw HTML -------------------------------------------------
            // `if (token.children) … else if (token.content)`, and **`[]` is
            // truthy in JavaScript**. So an element whose content tokenized to
            // nothing takes the *first* branch and contributes `''`; only a
            // comment, which has no `children` key at all, falls past it. This
            // is the consumer that makes `HtmlTag`'s three-state
            // `Option<Vec<Token>>` load-bearing rather than decorative — see
            // `HtmlTag::children`.
            //
            // **The `content` arm is unreachable**, and it is transcribed
            // anyway. `tryHtmlTag` has exactly two branches: the comment branch
            // (`lexer.ts:652`) sets neither `content` nor `children`, and the
            // element branch (`:686`) sets `children: htmlTo[4] ? … : []` —
            // an array either way, so `if (token.children)` always wins there.
            // No producer leaves `children` absent while `content` is present.
            // `the_html_tag_content_arm_cannot_fire` proves it over every
            // `html_tag` shape; the arm stays because §3 says port faithfully,
            // and because a `_ => {}` would hide which of the two states it was
            // covering. Fourth of its kind, after S2's rule-16 guard, S3's
            // `correctUrl` group 5 and S5's `if (!email)`.
            TokenKind::HtmlTag(tag) => match (&tag.children, tag.content) {
                (Some(children), _) => push_plain_text(out, src, children),
                (None, Some(content)) => out.push_str(content.of(src)),
                (None, None) => {}
            },

            // --- a backslash escape contributes nothing -------------------
            // muya writes `token.raw.replace(/^\\/, '')` under a comment
            // saying "the escaped char is `raw` minus its leading `\`".
            // **The comment is wrong and the code is right.** M1.md §4 C3:
            // `tryBacklash` sets `raw` to capture 1, which is the `\` *alone*
            // (`lexer.ts:118`, rule `/^(\\)([\\`*{}[\]()#+\-.!_>~:|<$])/`);
            // the escaped character goes into `pending` and surfaces in the
            // **next** `text` token. So the expression strips the only
            // character there is and yields `''` — and the escaped character
            // still reaches the output, one token later. Transcribed as the
            // strip rather than as a `continue`, because the strip is what
            // muya evaluates and
            // `a_backlash_tokens_raw_is_the_backslash_alone` is what pins the
            // premise.
            TokenKind::Backlash { .. } => {
                let raw = token.raw.of(src);
                out.push_str(raw.strip_prefix('\\').unwrap_or(raw));
            }

            // --- an entity decodes, or stands for itself ------------------
            // `escapeCharactersMap[token.escapeCharacter] ?? token.raw`. The
            // fallback is **live**: `rules.ts:45` builds the rule with the `i`
            // flag while the map is case-sensitive, so `&AMP;` is a token whose
            // key is not in the map and which therefore renders as `&AMP;`.
            // See `escape::escape_character`.
            TokenKind::HtmlEscape {
                escape_character: e,
            } => {
                let entity = e.of(src);
                out.push_str(escape_character(entity).unwrap_or_else(|| token.raw.of(src)));
            }

            // --- autolinks ------------------------------------------------
            // `<http://x>` shows the source between the angle brackets rather
            // than `href`, because `href` may carry a `mailto:` scheme the
            // author never typed. `replace(/^<|>$/g, '')` removes at most one
            // of each, which is what stripping the prefix and then the suffix
            // does.
            TokenKind::AutoLink(_) => {
                let raw = token.raw.of(src);
                let raw = raw.strip_prefix('<').unwrap_or(raw);
                out.push_str(raw.strip_suffix('>').unwrap_or(raw));
            }
            // A bare URL has no markers to drop, so it is its own text.
            TokenKind::AutoLinkExtension(_) => out.push_str(token.raw.of(src)),

            // --- line breaks become one space -----------------------------
            TokenKind::SoftLineBreak { .. } | TokenKind::HardLineBreak { .. } => out.push(' '),

            // --- muya's `default: break` ----------------------------------
            // The six token types that carry no reader-facing text: the four
            // begin-rule markers, the whole `reference_definition` line, and an
            // ATX heading's tail `#`s. Listed rather than wildcarded — see the
            // module header.
            //
            // Note `header` and `code_fence` *do* have a `content` group, and
            // it is deliberately not emitted: a heading's plain text comes from
            // the inline tokens that follow the marker token, not from the
            // marker token itself.
            TokenKind::Header(_)
            | TokenKind::Hr(_)
            | TokenKind::CodeFence(_)
            | TokenKind::MultipleMath(_)
            | TokenKind::ReferenceDefinition(_)
            | TokenKind::TailHeader { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;
    use crate::{Label, Labels, SyntaxOptions, TokenizerOptions, tokenize, tokenizer};

    fn plain(src: &str) -> String {
        tokens_to_plain_text(src, &tokenize(src))
    }

    /// The three examples muya's own comment gives.
    #[test]
    fn markers_delimiters_urls_and_tags_are_dropped() {
        assert_eq!(plain("**bold**"), "bold");
        assert_eq!(plain("[text](url)"), "text");
        assert_eq!(plain("![alt](src)"), "alt");
    }

    /// #4811's actual use: a heading's text, with the marker gone and the
    /// inline syntax resolved, is what the TOC shows and what the exported
    /// anchor id is slugged from.
    #[test]
    fn a_headings_plain_text_is_what_a_toc_entry_would_show() {
        assert_eq!(plain("# A **bold** heading ##"), "A bold heading");
    }

    #[test]
    fn nesting_recurses() {
        assert_eq!(plain("**a *b* c**"), "a b c");
        assert_eq!(plain("[a **b** c](https://example.com)"), "a b c");
        assert_eq!(plain("~~a `b` c~~"), "a b c");
    }

    /// The premise the `backlash` arm rests on, and the reason muya's comment
    /// on that arm is wrong: `raw` is the `\` **alone**, so stripping a leading
    /// backslash from it yields the empty string. The escaped character is not
    /// lost — it is the next token.
    #[test]
    fn a_backlash_tokens_raw_is_the_backslash_alone() {
        let src = r"a \* b";
        let tokens = tokenize(src);
        let backlash = tokens
            .iter()
            .find(|t| matches!(t.kind, TokenKind::Backlash { .. }))
            .expect("`\\*` is an escape");
        assert_eq!(backlash.raw.of(src), "\\");
        assert_eq!(
            backlash.raw.of(src).strip_prefix('\\'),
            Some(""),
            "muya's `raw.replace(/^\\\\/, '')` contributes nothing"
        );
        // …and the `*` still reaches the output, from the following text token.
        assert_eq!(plain(src), "a * b");
    }

    /// Both halves of `escapeCharactersMap[…] ?? token.raw`, including the
    /// fallback — which is reachable because the rule is case-insensitive and
    /// the map is not.
    #[test]
    fn an_entity_decodes_and_an_unmapped_spelling_stands_for_itself() {
        assert_eq!(plain("a &amp; b"), "a & b");
        assert_eq!(plain("&nbsp;"), "\u{a0}");
        assert_eq!(plain("a &AMP; b"), "a &AMP; b");
        // Not an entity of the 269 at all: never a token, so it is plain text
        // and arrives unchanged by a different route.
        assert_eq!(plain("a &notreal; b"), "a &notreal; b");
    }

    /// An autolink shows its source, not its `href` — `mailto:` is added by
    /// the tokenizer and must not appear to the reader.
    #[test]
    fn an_autolink_shows_the_text_between_its_angle_brackets() {
        assert_eq!(plain("<https://example.com>"), "https://example.com");
        assert_eq!(plain("<user@example.com>"), "user@example.com");
        assert_eq!(plain("https://example.com"), "https://example.com");
    }

    #[test]
    fn a_line_break_becomes_one_space() {
        assert_eq!(plain("a\nb"), "a b");
        assert_eq!(plain("a  \nb"), "a b");
    }

    /// The six types in the `default` arm, one input each where possible, so
    /// that "carries no reader-facing text" is a test rather than a comment.
    #[test]
    fn the_six_textless_token_types_contribute_nothing() {
        assert_eq!(plain("---"), "");
        assert_eq!(plain("```rust"), "");
        assert_eq!(plain("$$"), "");
        // `header` contributes nothing itself; the text after it is separate
        // tokens, so an empty heading is empty.
        assert_eq!(plain("## "), "");
        // `tail_header` — and it swallows the space before the `#`s, because
        // `rules.ts:16` is `/^(\s+#+)(\s*)$/` and `raw` is capture **1**. So
        // the closed form loses the separator the open form keeps, which is
        // muya's behaviour and is why this reads "h" and not "h ".
        assert_eq!(plain("# h #"), "h");
        assert_eq!(plain("# h"), "h");
        assert_eq!(plain("[ref]: https://example.com \"t\""), "");
    }

    /// `html_tag`'s three child states, which is the whole reason
    /// `HtmlTag::children` is an `Option<Vec<Token>>` and not a `Vec<Token>`.
    #[test]
    fn an_html_tags_three_child_states_take_three_different_branches() {
        // `children: [tokens]` — recursed into.
        assert_eq!(plain("<span>text</span>"), "text");
        // `children: []` — truthy in JavaScript, so the first branch still
        // wins and the `content` (which is `''`) is never consulted.
        assert_eq!(plain("<span></span>"), "");
        // `children: undefined` — a comment. `tryHtmlTag`'s comment branch
        // sets no `content` either, so it contributes nothing: the whole
        // comment, delimiters and text alike, is invisible to a reader.
        assert_eq!(plain("<!--note-->"), "");
        // A void tag: `children: []` again, from the element branch.
        assert_eq!(plain("<br>"), "");
    }

    /// The fourth provably-dead branch of the milestone — see the `html_tag`
    /// arm. `else if (token.content)` needs a token with **absent** children
    /// and **present** content, and `tryHtmlTag` produces no such token: the
    /// comment branch sets neither field, the element branch always sets
    /// `children` to an array.
    ///
    /// Swept rather than argued, over every `html_tag` shape the two branches
    /// can take.
    #[test]
    fn the_html_tag_content_arm_cannot_fire() {
        let cases = [
            "<!---->",
            "<!--note-->",
            "<!-- multi\nline -->",
            "<br>",
            "<span>",
            "<span></span>",
            "<span>x</span>",
            "<div class=\"a\">x</div>",
            "<img src=\"a.png\">",
            "<a href=\"u\">t</a>",
            "<ruby>x<rt>y</rt></ruby>",
            "<b><i>x</i></b>",
        ];
        let mut html_tags = 0;
        for src in cases {
            for token in tokenize(src) {
                let TokenKind::HtmlTag(tag) = &token.kind else {
                    continue;
                };
                html_tags += 1;
                assert!(
                    !(tag.children.is_none() && tag.content.is_some()),
                    "{src:?} reached the unreachable arm"
                );
            }
        }
        assert!(html_tags >= cases.len(), "the sweep stopped producing tags");
    }

    /// A reference link's children are its anchor; a reference image's alt is
    /// a plain capture group with no children at all.
    #[test]
    fn reference_forms_use_the_anchor_and_the_alt() {
        let mut labels = Labels::new();
        labels.insert(
            "ref".to_string(),
            Label {
                href: "https://example.com".to_string(),
                title: String::new(),
            },
        );
        let options = TokenizerOptions::muya_default().with_labels(labels);

        let src = "[an **anchor**][ref]";
        assert_eq!(
            tokens_to_plain_text(src, &tokenizer(src, &options)),
            "an anchor"
        );

        let src = "![some alt][ref]";
        assert_eq!(
            tokens_to_plain_text(src, &tokenizer(src, &options)),
            "some alt"
        );
    }

    /// `super_sub_script` and `footnote_identifier` are the two arms behind a
    /// syntax flag, and `footnote` is **off** by default — so the default-
    /// options reading of `[^1]` is plain text, and the arm is only reachable
    /// when a caller asks for it.
    #[test]
    fn the_two_optional_syntaxes_still_contribute_their_content() {
        assert_eq!(plain("x^2^ and y~1~"), "x2 and y1");
        assert_eq!(plain("a[^1]"), "a[^1]");

        let src = "a[^1]";
        let options = TokenizerOptions {
            syntax: SyntaxOptions {
                footnote: true,
                ..SyntaxOptions::default()
            },
            ..TokenizerOptions::muya_default()
        };
        assert_eq!(tokens_to_plain_text(src, &tokenizer(src, &options)), "a1");
    }

    /// Offsets are bytes (M1.md §5 D1), so a multi-byte character in a nested
    /// level is the case where a wrong unit would slice mid-character and
    /// panic rather than merely disagree.
    #[test]
    fn multibyte_content_survives_nesting() {
        assert_eq!(plain("**中文 🇬🇧**"), "中文 🇬🇧");
        assert_eq!(plain("[中文](https://example.com)"), "中文");
    }

    /// Not a bijection, unlike [`crate::generator`] — stated as a test so the
    /// two are never confused.
    #[test]
    fn it_is_not_the_round_trip_property() {
        let src = "**bold**";
        let tokens = tokenize(src);
        assert_eq!(crate::generator(src, &tokens), src);
        assert_ne!(tokens_to_plain_text(src, &tokens), src);
    }

    /// A leaf token with an empty span is still a valid span, and the whole
    /// walk is `push_str`, so an empty stream is the empty string rather than
    /// a panic.
    #[test]
    fn an_empty_input_is_the_empty_string() {
        assert_eq!(plain(""), "");
        assert_eq!(tokens_to_plain_text("", &[]), "");
        assert_eq!(Span::empty_at(0).of(""), "");
    }
}
