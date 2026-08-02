//! # `mt-inline` — the inline tokenizer
//!
//! ## Contract
//!
//! Given a leaf block's raw markdown text, produce a token tree in which
//! **every token preserves its source bytes and its byte range**. That is the
//! whole contract, and it is stricter than "parse the inlines correctly".
//!
//! This is a faithful port of `packages/muya/src/inlineRenderer/lexer.ts`
//! (1003 lines) and `rules.ts` (130 lines). RUST-REWRITE-PLAN.md §3 names it
//! **the single highest-fidelity-risk component in the project**, and it is
//! small enough to port line by line rather than reinterpret. Do not
//! reinterpret it.
//!
//! ## Dependency constraints
//!
//! Per §1, `mt-inline` is **pure**: it sits at the bottom of the dependency
//! graph beside `mt-doc` and depends on neither it nor anything else in the
//! workspace.
//!
//! - **No windowing.** **No GPU.** **No I/O.**
//! - **No dependency on `mt-ui` or `mt-app`,** ever.
//! - Input is `&str`, output is a `Vec<Token>`. Nothing else crosses the
//!   boundary.
//!
//! Being pure is what makes the fuzzing gate in §11.3 and the 24-hour
//! no-panic requirement at the M1 exit gate (§9) achievable.
//!
//! ## Offsets are UTF-8 byte offsets — M1.md §5 D1, decided
//!
//! **Internally, every offset in this crate is a UTF-8 byte offset into the
//! block text.** muya's `range.start` / `range.end` are JavaScript string
//! indices, i.e. UTF-16 code units, and the two differ for every non-ASCII
//! character. The conversion to UTF-16 belongs in exactly **one** place: the
//! boundary where a token is serialized for comparison against the TypeScript
//! engine. Nothing else in the crate may know about UTF-16.
//!
//! Reading `lexer.ts` in full settles this rather than merely recommending it.
//! Offsets are pure bookkeeping inside the tokenizer: `state.pos` is only ever
//! incremented by a match length, stored into a token's `range`, and compared
//! against **zero** (`tryFootnote`, `tryAutoLinkExtension`). It is never used
//! to index anything — with exactly one exception,
//! `state.originSrc[state.pos - 1]` at `lexer.ts:206` and `:578`, which is
//! D3 site 1, the emoji-boundary bug, and which the port fixes by indexing
//! relative to the current level anyway. So the unit of measurement cannot
//! change any tokenization decision, and byte offsets are free.
//!
//! Two consequences worth stating:
//!
//! - `utils.ts`'s `lastCodePointChar` and `codePointCharAt` (lines 104–130)
//!   exist purely to reassemble surrogate pairs that JavaScript splits.
//!   Rust's `char` iteration gives that for free. Port their *callers*
//!   faithfully; do not port the functions.
//! - Ranges are what §3.1's marker-reveal predicate compares the caret
//!   against and what `highlights` intersect with. Both are internal to the
//!   Rust side, so both stay in bytes.
//!
//! ## The three non-negotiable porting rules (§3)
//!
//! 1. **Ranges are byte offsets, and they must tile the input exactly.**
//!    Assert in debug builds that concatenating every token's `raw` slice
//!    reproduces the source byte-for-byte. This single invariant catches most
//!    porting errors immediately, and it is checked over the whole corpus at
//!    the M1 gate. M1.md §4 C3 qualifies it: the invariant holds **per
//!    tokenizer level**, and the cross-level half is that every child span is
//!    contained in its parent's ([`Span::contains_span`]).
//! 2. **`Marker` and backslash spans are retained, not normalised away.**
//!    They are what the renderer reveals when the caret is inside a token,
//!    and what the serializer re-emits. A tokenizer that "cleans up" markers
//!    is a tokenizer that loses user data.
//! 3. **Behaviour is defined by the TypeScript tests, not by the spec.**
//!    `packages/muya/src/inlineRenderer/__tests__/` covers autolink trailing
//!    punctuation, autolink encoding, emoji word boundaries, inline-math
//!    escaping, CJK flanking for `strong`, reference-link/image anchors, and
//!    link-followed-by-autolink. §14 step 5 is explicit: **port every one of
//!    those specs to Rust as failing tests first, then make them pass.** All
//!    49 of them are in `tests/inline_renderer_specs.rs`.
//!
//! ## Handler precedence is the contract
//!
//! `INLINE_HANDLERS` (`lexer.ts:792`) is a fixed, ordered array and the loop
//! takes the first handler that returns `true`. Port it as an ordered array
//! too, **not** as a match on the next character — the order *is* the spec,
//! and the TypeScript carries a comment saying so.
//!
//! ## Regex engine
//!
//! §3 says to use `regex` with pre-compiled `LazyLock<Regex>`. M1.md §4 C1
//! corrects this: 16 of the 26 rules need backreferences, lookahead or
//! lookbehind, none of which the `regex` crate has by design. `fancy-regex`
//! is what S1 wired in, and it delegates to the non-backtracking engine for
//! the 10 rules that do not need the extra power.
//!
//! It is a backtracking engine, so the seven patterns `rules.ts` disables the
//! super-linear-backtracking lint for are super-linear here too. Every rule is
//! built with an explicit backtrack limit and **exceeding it means the rule
//! did not match, never a panic** — see `rules::BACKTRACK_LIMIT` for the value
//! and the three reasons. The tiling invariant is untouched by a rule giving
//! up: no byte is lost, it just stays text.
//!
//! ## Marker reveal (§3.1)
//!
//! The rule that makes MarkText feel like MarkText: a token's markers reveal
//! when the caret is inside `token.range` (inclusive of edges) or the
//! selection intersects it; ancestors reveal when a descendant reveals;
//! everything else renders decorated. It is a pure function of
//! `(tokens, caret, selection)`, so it is cheap to test headlessly — and it is
//! load-bearing for the product's identity. Get it exactly right early.
//! Lands in S6.
//!
//! ## Deliberate divergences from muya
//!
//! M1.md §5 D3 is decided: **muya's bugs are fixed in the port, not
//! reproduced**, and every fix is registered in `spec/divergences.json`
//! *before* it is made. Read that file before changing tokenizer behaviour;
//! `cargo xtask divergences` is what keeps it honest.
//!
//! ## Status: S1
//!
//! Landed: the token types (S0), the 49 transcribed specs (S0), the rule table
//! with `fancy-regex` behind it, the tokenizer loop with the ordered
//! `INLINE_HANDLERS` array and `pushPending`, `consumeBeginRules`, the debug
//! tiling assertion, and [`generator`] — pulled forward from S6 because
//! `generator(tokenize(s)) == s` is the tiling invariant restated as an
//! equality, and building it now makes every later stage's handler tested the
//! moment it is written (M1.md §6).
//!
//! Implemented handlers: `header` `hr` `code_fence` `multiple_math`
//! `tail_header` `backlash` `html_escape` `soft_line_break` `hard_line_break`.
//! **The other eleven are present in their exact precedence positions and
//! return `false`**, so anything they would match accumulates as text. That is
//! correct rather than merely tolerable: an unmatched construct is text, the
//! input still tiles, and the round-trip still holds. Emphasis is S2, links
//! and images S3, HTML S4, autolinks S5.
//!
//! Not yet read by anything: [`TokenizerOptions::labels`] (S3),
//! [`TokenizerOptions::syntax`] (S2) and [`TokenizerOptions::highlights`]
//! (S6, the post-pass).
//!
//! The remaining spec cases fail without failing the build: every case that
//! does not pass yet is listed in `PENDING` in
//! `tests/inline_renderer_specs.rs`, and that list only shrinks — a listed
//! case that starts passing fails CI until it is delisted. Same ratchet as
//! `spec/expected-failures.json` and `spec/divergences.json`, for the same
//! reason. **The list emptying is M1's exit gate**, and its length is the
//! milestone's progress meter.

mod escape;
mod generator;
mod lexer;
mod rules;
mod token;

use std::collections::BTreeMap;

pub use generator::{generator, generator_rebuilding_wrappers};
pub use token::{
    AutoLink, AutoLinkExtension, AutoLinkKind, BacklashPair, BeginRule, CodeEmojiMath, Emphasis,
    Highlight, HtmlTag, HtmlTagName, Image, ImageAttrs, Link, ReferenceDefinition, ReferenceImage,
    ReferenceLink, Span, Token, TokenKind,
};

/// A reference-definition target: `[label]: href "title"`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Label {
    pub href: String,
    pub title: String,
}

/// muya's `Labels` map, collected from the document's reference definitions.
///
/// Keys are **lowercased**, because CommonMark §6.5 matches link labels
/// case-insensitively and `collectReferenceDefinitions` lowercases on the way
/// in; `tryReferenceLink` lowercases the candidate before the lookup
/// (`lexer.ts:415`).
///
/// A `BTreeMap` rather than a `HashMap` because nothing needs hashing speed
/// here and deterministic iteration makes a `Debug` dump of the options
/// diffable.
pub type Labels = BTreeMap<String, Label>;

/// `ITokenizerFacOptions` — the two syntax extensions the caller can disable.
///
/// Both are threaded down every nested `tokenizerFac` call unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxOptions {
    /// `^sup^` and `~sub~`. Default `true`.
    pub super_sub_script: bool,
    /// `[^note]`. Default `false`.
    pub footnote: bool,
}

impl Default for SyntaxOptions {
    fn default() -> Self {
        Self {
            super_sub_script: true,
            footnote: false,
        }
    }
}

/// `ITokenizerOptions` — everything [`tokenizer`] accepts besides the text.
///
/// The defaults are muya's destructuring defaults at `lexer.ts:855-861`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizerOptions {
    /// Search highlights to intersect with every token range in a post-pass.
    ///
    /// Empty by default, and the post-pass is skipped entirely when it is
    /// empty (`lexer.ts:890`). M1.md §5 D7.
    pub highlights: Vec<Highlight>,
    /// Whether to try `header` / `hr` / `code_fence` / `multiple_math` /
    /// `reference_definition` at offset 0. Default `true`.
    pub has_begin_rules: bool,
    /// Reference-definition targets, keyed lowercase.
    pub labels: Labels,
    pub syntax: SyntaxOptions,
}

impl Default for TokenizerOptions {
    /// Delegates to [`TokenizerOptions::muya_default`]. **Not derived**: a
    /// derived `Default` would give `has_begin_rules: false`, silently
    /// changing what a paragraph starting with `#` tokenizes to.
    fn default() -> Self {
        Self::muya_default()
    }
}

impl TokenizerOptions {
    /// muya's defaults: begin rules on, no labels, sup/sub on, footnotes off
    /// (`lexer.ts:855-861`).
    pub fn muya_default() -> Self {
        Self {
            highlights: Vec::new(),
            has_begin_rules: true,
            labels: Labels::new(),
            syntax: SyntaxOptions::default(),
        }
    }

    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    pub fn with_highlights(mut self, highlights: Vec<Highlight>) -> Self {
        self.highlights = highlights;
        self
    }
}

/// Tokenize one leaf block's text.
///
/// The port of `tokenizer()` (`lexer.ts:854`). Spans in the result are UTF-8
/// byte offsets into `src`; see the crate docs for the convention.
///
/// Concatenating every returned token's `raw` reproduces `src` byte for byte
/// — that is §3 rule 1, and [`generator`] is it as a function.
///
/// # What S1 does not do yet
///
/// Five of the sixteen rule groups are unimplemented (see the crate docs), so
/// emphasis, links, images, HTML and autolinks currently tokenize as text.
/// Three options are consequently ignored, and are documented as ignored
/// rather than quietly honoured-later:
///
/// - `options.labels` — read by the reference-link handlers, S3.
/// - `options.syntax` — read by the sup/sub and footnote handlers, S2.
/// - `options.highlights` — the intersection post-pass is S6.
///
/// `options.has_begin_rules` is honoured.
pub fn tokenizer(src: &str, options: &TokenizerOptions) -> Vec<Token> {
    lexer::tokenizer(src, options)
}

/// Tokenize with muya's default options.
///
/// A convenience for the common call; `tokenizer(src, &TokenizerOptions::muya_default())`.
pub fn tokenize(src: &str) -> Vec<Token> {
    tokenizer(src, &TokenizerOptions::muya_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults are behaviour, not decoration: `has_begin_rules` silently
    /// changes what a paragraph starting with `#` tokenizes to.
    #[test]
    fn muya_defaults_match_lexer_ts() {
        let options = TokenizerOptions::muya_default();
        assert!(options.has_begin_rules);
        assert!(options.syntax.super_sub_script);
        assert!(!options.syntax.footnote);
        assert!(options.highlights.is_empty());
        assert!(options.labels.is_empty());
    }

    #[test]
    fn derived_default_is_the_muya_default() {
        assert_eq!(
            TokenizerOptions::default(),
            TokenizerOptions::muya_default()
        );
    }

    #[test]
    fn labels_are_looked_up_lowercased() {
        // Not a behaviour test of the tokenizer — a statement of the key
        // convention, so a future caller populating the map does not have to
        // rediscover it from lexer.ts:415.
        let mut labels = Labels::new();
        labels.insert(
            "ref".to_string(),
            Label {
                href: "https://example.com".to_string(),
                title: String::new(),
            },
        );
        let options = TokenizerOptions::muya_default().with_labels(labels);
        assert!(options.labels.contains_key(&"REF".to_lowercase()));
    }
}
