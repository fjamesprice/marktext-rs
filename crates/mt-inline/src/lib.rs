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
//! ## Marker reveal (§3.1) — landed in S6, and §3.1 was wrong
//!
//! The rule that makes MarkText feel like MarkText: a token's markers reveal
//! when the caret is on it, and everything else renders decorated. It is a
//! pure function of `(token.range, cursor)`, so it is cheap to test
//! headlessly — and it is load-bearing for the product's identity.
//!
//! §3.1 states it as *"the caret is inside `token.range` (inclusive of edges)
//! or the selection intersects it … ancestors reveal when a descendant
//! reveals"*. **Two of those three clauses are false**, measured against the
//! running renderer: the two selection endpoints are tested separately, so a
//! selection *spanning* a token does not reveal it; and there is no
//! ancestor propagation, in either direction, by any mechanism. M1.md §5 D8
//! records what the engine does and [`marker`] implements that.
//!
//! ## Deliberate divergences from muya
//!
//! M1.md §5 D3 is decided: **muya's bugs are fixed in the port, not
//! reproduced**, and every fix is registered in `spec/divergences.json`
//! *before* it is made. Read that file before changing tokenizer behaviour;
//! `cargo xtask divergences` is what keeps it honest.
//!
//! ## Status: S7 — the tokenizer is complete, and so is its verification
//!
//! **All sixteen handlers are implemented and all 49 transcribed muya specs
//! pass.** `PENDING` in `tests/inline_renderer_specs.rs` is empty, which is
//! M1's exit gate for conformance. S6 added the three consumers that were
//! still owed — [`tokens_to_plain_text`], the `highlights` post-pass
//! ([`union`], run by [`tokenizer`]) and [`marker_state`] — so what M1 still
//! owes is S7 alone: invariants and soak, not behaviour.
//!
//! Landed: the token types (S0), the 49 transcribed specs (S0), the rule table
//! with `fancy-regex` behind it, the tokenizer loop with the ordered
//! `INLINE_HANDLERS` array and `pushPending`, `consumeBeginRules`, the debug
//! tiling assertion, and [`generator`] — pulled forward from S6 because
//! `generator(tokenize(s)) == s` is the tiling invariant restated as an
//! equality, and building it now makes every later stage's handler tested the
//! moment it is written (M1.md §6). Then, in S2, the emphasis half of
//! `utils.ts` (`emphasis.rs`) and the four handlers that consume it; in
//! S3 the link half (`link.rs` — `parseSrcAndTitle`, `correctUrl`,
//! `findClosingBracket`) and its four; in S4 `getAttributes` without a DOM
//! (`html.rs`, plus the HTML5 named-reference table in `entities.rs`) and the
//! `html_tag` handler that consumes it; in S5 the two autolinks together
//! with `trimAutoLinkExtent`, GFM §6.9's extent trimming; and in S6 the three
//! things that *read* a finished token tree rather than producing one —
//! [`plain_text`], [`highlight`] and [`marker`].
//!
//! Implemented handlers: `header` `hr` `code_fence` `multiple_math`
//! `reference_definition` `tail_header` `backlash` `html_escape`
//! `soft_line_break` `hard_line_break` `strong`/`em`
//! `inline_code`/`del`/`emoji`/`inline_math` `super_sub_script`
//! `footnote_identifier` `image` `link` `reference_link` `reference_image`
//! `html_tag` `auto_link` `auto_link_extension`. There are no stubs left.
//!
//! ### The stub shadowing S4 introduced is gone
//!
//! Until S4 an unimplemented handler simply left its construct as text: the
//! input still tiled, the round trip still held, and nothing produced a wrong
//! token. S4 broke that, because `auto_link` and `auto_link_extension`
//! **outrank** `html_tag` in the precedence array while returning `false`, and
//! `html_tag`'s pattern matches `<scheme:…>` — so an angle-bracket autolink
//! became an `html_tag` whose `tag` was its scheme. Twenty-six distinct corpus
//! and fixture inputs were in that class. S5 restores the precedence for all
//! twenty-six, measured rather than inferred, and with no stubs left the
//! question cannot arise again in this crate.
//!
//! **Nested tokenization is live from S2**: `strong`, `em` and `del` tokenize
//! their content as children, based at an absolute offset into the same
//! top-level text; S3 adds `link` and `reference_link`, which tokenize their
//! anchor; S4 adds `html_tag`, which tokenizes its content. So the cross-level
//! half of M1.md §4 C3 — every child span is contained in its parent's — is
//! checked against real children rather than vacuously true. The autolinks add
//! no nesting level: neither form tokenizes children.
//!
//! **Every option is now read.** [`TokenizerOptions::highlights`] became live
//! in S6: [`tokenizer`] runs the intersection post-pass over the finished tree
//! when it is non-empty, and skips it entirely when it is not (M1.md §5 D7).
//! [`TokenizerOptions::syntax`] became live in S2 — it gates
//! `super_sub_script` (on by default) and `footnote_identifier` (**off** by
//! default, so `[^1]` is plain text unless a caller asks).
//! [`TokenizerOptions::labels`] became live in S3: it is the *only* thing that
//! makes `[text][ref]` a reference link, so with the default empty map every
//! reference form is plain text.
//!
//! ### What M1 still owes, so that "complete" is not read too widely
//!
//! - ~~**S7**: proptest generators, the 24-hour libFuzzer soak, and pointing
//!   `cargo xtask divergences` at a real token-stream comparator.~~ **Built.**
//!   The register is enforced on every commit — 27 of 27 registered inputs
//!   disagree, 41,009 swept inputs agree, and reverting a fix turns it red;
//!   `tests/properties.rs` runs on all three platforms; `fuzz/` and
//!   `.github/workflows/soak.yml` carry the soak. **The 24-hour clause is the
//!   one exit-gate item still unmet** — the workflow is scheduled and has not
//!   run. M1.md's "Closing M1" states each clause and its evidence.
//! - **S7 also made four proofs falsifiable.** `validateEmphasize`'s rule-16
//!   guard, `correctUrl`'s group 5, `tryAutoLinkExtension`'s `if (!email)` and
//!   `tokensToPlainText`'s `html_tag` `content` arm are each proved
//!   unreachable and reproduced anyway. Each is now a `debug_assert!` at the
//!   site, so a fuzz or proptest run attacks the proof rather than agreeing
//!   with it. `debug_assert!` and not `unreachable!`, because the exit gate is
//!   panic-freedom and each release-build fallback is the transcribed muya
//!   behaviour.
//! - **The renderer milestone**: `autoLinkEncoding.spec.ts`'s other half.
//!   `mt-inline` guarantees the `auto_link` token's `href` is the literal
//!   source between the angle brackets; that the rendered `<a href>` is
//!   emitted from it verbatim rather than through an `encodeURI` equivalent is
//!   owed, and M1.md §10 carries it.

mod emphasis;
mod entities;
mod escape;
mod generator;
mod highlight;
mod html;
mod lexer;
mod link;
mod marker;
mod plain_text;
mod rules;
mod token;

use std::collections::BTreeMap;

pub use generator::{generator, generator_rebuilding_wrappers};
pub use highlight::union;
pub use marker::{Cursor, MarkerState, marker_state, marker_state_of_range};
pub use plain_text::tokens_to_plain_text;
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

/// muya's `InlineRenderer.getLabelInfo` (`inlineRenderer/index.ts:101`) — one
/// leaf's text, and the label it defines if it defines one.
///
/// `beginRules.reference_definition` run over the **whole** text, then
/// `label = (tokens[2] + tokens[3]).toLowerCase()`, `href = tokens[6]`,
/// `title = tokens[10] || ''`. That is the entirety of muya's label pass, and
/// [`mt_md::labels`](../mt_md/labels/index.html) is what calls it once per
/// paragraph.
///
/// # Why this is not [`tokenizer`] with `has_begin_rules: true`
///
/// It nearly is, and the difference is one line. `consumeBeginRules` gates the
/// `reference_definition` token on `isLengthEven(def[3])` — an odd run of
/// backslashes escapes the `]`, so the label never closes and the line is not a
/// definition. **`getLabelInfo` has no such gate**: it reads the raw capture
/// groups. So `[a\]: /x` yields no `reference_definition` *token* and does
/// define the label `a\`. Tokenizing to find labels would silently disagree
/// with muya on exactly that input, which is why this runs the rule directly.
///
/// # Three quirks of the rule that are reproduced rather than repaired
///
/// - **The title's closing delimiter is `\9`, the opening one.** So
///   `[a]: /u (t)` matches nothing at all — `(` must be closed by `(` — and
///   the definition contributes no label even though `marked` happily made a
///   block out of it.
/// - **`^` and `$` are not multi-line**, so a paragraph holding a definition
///   *and* anything else defines nothing. That is what keeps `text\n[a]: /u`,
///   which `marked` folds into one paragraph, from registering a label.
/// - **The label is capture 2 *plus* capture 3**, the run of backslashes before
///   the `]`. Dropping capture 3 loses the trailing `\` of `[a\\]: /u`.
///
/// Returns the key already lowercased, as `collectReferenceDefinitions` stores
/// it. muya's `if (label && info)` guard is not reproduced as a condition
/// because it cannot fail: capture 2 is `+?`, so a match always has a non-empty
/// label, and `info` is an object literal.
pub fn label_info(text: &str) -> Option<(String, Label)> {
    let id = rules::RuleId::ReferenceDefinition;
    let caps = rules::exec(rules::rule(id), text)?;
    let group = |index: usize| caps.get(index).map(|m| m.as_str());
    let required = |index: usize| {
        group(index).unwrap_or_else(|| {
            panic!(
                "`{}` matched but capture group {index} did not participate",
                id.name()
            )
        })
    };

    let label = format!("{}{}", required(2), required(3)).to_lowercase();
    Some((
        label,
        Label {
            href: required(6).to_string(),
            title: group(10).unwrap_or("").to_string(),
        },
    ))
}

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
/// Every field of [`TokenizerOptions`] is honoured as of S6.
/// `options.highlights` runs the intersection post-pass over the finished tree
/// (`lexer.ts:873`) and is skipped entirely when empty, which is the common
/// case; see [`union`].
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

    // --- getLabelInfo (M2 S3) ----------------------------------------------
    //
    // Every expectation below was measured against the running engine —
    // `beginRules.reference_definition.exec(text)` in the marktext clone, with
    // muya's own `(t[2] + t[3]).toLowerCase()`, `t[6]`, `t[10] || ''` — rather
    // than read off the pattern. Three of them are not what reading it
    // suggests.

    fn info(text: &str) -> Option<(String, Label)> {
        label_info(text)
    }

    fn label(href: &str, title: &str) -> Label {
        Label {
            href: href.into(),
            title: title.into(),
        }
    }

    #[test]
    fn a_definition_yields_its_label_href_and_title() {
        assert_eq!(
            info("[1]: https://example.com \"title\""),
            Some(("1".into(), label("https://example.com", "title")))
        );
        assert_eq!(
            info("[ref]: https://example.com 'Ref Title'"),
            Some(("ref".into(), label("https://example.com", "Ref Title")))
        );
        assert_eq!(info("[a]: /u"), Some(("a".into(), label("/u", ""))));
    }

    /// The key is lowercased and the ` {0,3}` indent is not part of it.
    #[test]
    fn the_label_is_lowercased_and_the_indent_is_not_part_of_it() {
        assert_eq!(info("  [FOO]: /a"), Some(("foo".into(), label("/a", ""))));
    }

    /// **Runs of whitespace inside a label are kept.** This is where the two
    /// reference-definition scanners part company: `marked`'s `tokens.links`
    /// key also applies `/\s+/g → ' '`, so it sees one label where this sees
    /// two. `mt_md::labels`' module docs carry the consequence.
    #[test]
    fn a_labels_internal_whitespace_is_not_collapsed() {
        assert_eq!(
            info("[foo bar]: /a").map(|(l, _)| l),
            Some("foo bar".into())
        );
        assert_eq!(
            info("[foo  bar]: /a").map(|(l, _)| l),
            Some("foo  bar".into())
        );
    }

    /// Capture 3 is part of the label. Dropping it loses the trailing
    /// backslash, and `[a\]: /x` is also the input where this and
    /// `consumeBeginRules` disagree: the token is gated on an *even* run of
    /// backslashes and this is not.
    #[test]
    fn the_backslash_run_before_the_bracket_is_part_of_the_label() {
        assert_eq!(info("[a\\]: /x"), Some(("a\\".into(), label("/x", ""))));
        assert_eq!(info("[a\\\\]: /x"), Some(("a\\\\".into(), label("/x", ""))));
        // And the token form refuses the odd one, which is the whole reason
        // this function exists rather than a call to `tokenizer`.
        assert!(!matches!(
            tokenize("[a\\]: /x").first().map(|t| &t.kind),
            Some(TokenKind::ReferenceDefinition(_))
        ));
    }

    /// `^` and `$` are not multi-line, so a paragraph that holds a definition
    /// *and* anything else defines nothing — including a trailing newline.
    #[test]
    fn the_rule_matches_the_whole_text_or_nothing() {
        assert_eq!(info("text\n[a]: /u"), None);
        assert_eq!(info("[a]: /u\n"), None);
        assert_eq!(info("[a]:\n/u\n\"t\""), None);
        assert_eq!(info("[a]:"), None);
    }

    /// **The `\9` backreference closes the title with whatever opened it**, so
    /// a parenthesised title matches nothing at all — not the definition
    /// without a title, the *whole rule* fails. Measured, because reading the
    /// pattern suggests a partial match.
    #[test]
    fn a_parenthesised_title_defeats_the_whole_rule() {
        assert_eq!(info("[a]: /u (t)"), None);
        // And an unquoted title is accepted, because capture 9 can be empty.
        assert_eq!(info("[a]: /u t"), Some(("a".into(), label("/u", "t"))));
    }

    /// `(<?)` and `(>?)` are separate captures, so the href is the bare URL —
    /// the angle brackets are not part of it.
    #[test]
    fn angle_brackets_around_the_href_are_not_part_of_it() {
        assert_eq!(info("[a]: <u> \"t\""), Some(("a".into(), label("u", "t"))));
    }

    /// A measured curiosity, pinned because it is the kind of thing a rewrite
    /// would "fix": trailing spaces after a title-less href are split between
    /// captures 8, 10 and 11, and muya's `title` ends up a single space.
    #[test]
    fn trailing_spaces_after_a_bare_href_become_a_one_space_title() {
        assert_eq!(info("[a]: /u   "), Some(("a".into(), label("/u", " "))));
    }
}
