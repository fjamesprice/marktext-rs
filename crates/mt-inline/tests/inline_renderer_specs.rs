//! The 49 cases of `packages/muya/src/inlineRenderer/__tests__/`, transcribed.
//!
//! RUST-REWRITE-PLAN.md §3 rule 3 and §14 step 5: *behaviour is defined by the
//! TypeScript tests, not by the spec* — **port every one of those specs first,
//! then make them pass.** M1.md §6 S0 is that port.
//!
//! | Module | Source spec file | Cases |
//! |---|---|---:|
//! | [`commonmark_examples`] | `commonmarkExamples.spec.ts` | 25 |
//! | [`auto_link_trailing_punct`] | `autoLinkTrailingPunct.spec.ts` | 10 |
//! | [`emoji_word_boundary`] | `emojiWordBoundary.spec.ts` | 4 |
//! | [`link_followed_by_autolink`] | `linkFollowedByAutolink.spec.ts` | 3 |
//! | [`reference_link_image_anchor`] | `referenceLinkImageAnchor.spec.ts` | 3 |
//! | [`auto_link_encoding`] | `autoLinkEncoding.spec.ts` | 2 |
//! | [`inline_math_escape`] | `inlineMathEscape.spec.ts` | 2 |
//! | | | **49** |
//!
//! Every test carries the issue or commit number its TypeScript original
//! names. Those numbers are the reason each case exists, and they are the
//! first thing to read when one of them starts failing.
//!
//! # The pending ratchet — why these do not fail the build
//!
//! Most of them failed for most of the milestone, and were supposed to: §14
//! step 5 wants them transcribed first and made to pass afterwards, so they
//! are the target to build against. **As of S5 all 49 pass and [`PENDING`] is
//! empty**, which is M1's exit gate. The ratchet below stays anyway — see
//! [`PENDING`] for why an empty list is a state to hold rather than a
//! mechanism to retire.
//!
//! But a build that is red for months is a build nobody reads, and by the time
//! a real regression appears in one of the other thirteen crates the signal is
//! long gone. So the 49 are wrapped in the same ratchet the repository already
//! uses twice — `spec/expected-failures.json` for CommonMark conformance and
//! `spec/divergences.json` for intentional differences from muya. Every case
//! that does not pass yet is listed in [`PENDING`], and:
//!
//! | State | Result |
//! |---|---|
//! | Listed, still failing | passes — it is on the list |
//! | Listed, now **passing** | **fails** — delist it |
//! | Unlisted, passing | passes |
//! | Unlisted, now failing | **fails**, with the original panic |
//!
//! Row two is the one that matters and the one people find surprising: getting
//! *better* must also fail the build, so the list is forced back down instead
//! of accumulating entries that later let a fixed case quietly regress. Row
//! four is what keeps this from being `#[ignore]` in disguise — the moment a
//! case passes and is delisted, it is a normal test with normal teeth.
//!
//! The list shrinking to nothing is M1's exit gate, restated as data. Its
//! length is the milestone's progress meter.
//!
//! Kept as a Rust `const` rather than a JSON file in `spec/` because it has
//! exactly one consumer — this binary — and a `const` needs no parser, no
//! dependency and no path resolution. Move it to `spec/` if `xtask` ever needs
//! to report on it.
//!
//! **Caveat:** the ratchet uses `catch_unwind`, so `cargo test --release`
//! aborts rather than catching (the release profile sets `panic = "abort"`).
//! CI runs the dev profile, where this is fine.
//!
//! # Offset convention — M1.md §5 D1, decided
//!
//! **Every offset in this crate, and therefore in every assertion below, is a
//! UTF-8 byte offset into the block text.** muya's `range.start` / `range.end`
//! are JavaScript string indices — UTF-16 code units — and the two differ for
//! every non-ASCII character. Three of the inputs below are non-ASCII
//! (`пристаням__стремятся__`, the two CJK link cases, and the non-breaking
//! spaces of CommonMark example 353), so this is not hypothetical.
//!
//! The conversion to UTF-16 lives in exactly one place: the boundary where a
//! token is serialized for comparison against the TypeScript engine. Nothing
//! in this crate or these tests knows about UTF-16.
//!
//! In practice these tests assert on *token shape* — `type` sequences, counts,
//! a handful of field values — rather than on numeric ranges, which is
//! deliberate on muya's side and is what made them transcribable before the
//! tokenizer exists. Field values are read through [`Span::of`], so the
//! convention is applied rather than restated: `link.href.of(src)` is the text
//! at those byte offsets, and it is wrong by construction if the offsets are
//! UTF-16.
//!
//! **If you add a test here that asserts a numeric offset, it asserts bytes.**
//! Write the expected number as a byte count and say so in a comment.
//!
//! # One deliberate deviation
//!
//! `autoLinkEncoding.spec.ts` is the only one of the seven that is not a
//! tokenizer test: it boots a full `Muya` instance and inspects the rendered
//! `<a href>` in the DOM. `mt-inline` has no renderer and no DOM, so
//! [`auto_link_encoding`] asserts the tokenizer half of #3548 — that the
//! `auto_link` token's `href` is the literal source between the angle
//! brackets, never re-encoded — and the rendering half is owed at the
//! milestone that lands the renderer. See that module's docs.

use std::panic::{self, AssertUnwindSafe};

use mt_inline::{
    AutoLink, AutoLinkExtension, CodeEmojiMath, Emphasis, Image, Label, Labels, Link,
    ReferenceLink, Span, Token, TokenKind, TokenizerOptions,
};

// ---------------------------------------------------------------------------
// The pending list
// ---------------------------------------------------------------------------

/// Spec cases that do not pass yet.
///
/// **This list only shrinks.** Delete an entry the moment its case passes —
/// the harness fails the build if you do not. Grouped by source spec file and
/// listed in file order, so a stage that fixes one file's worth of cases is
/// one contiguous deletion.
///
/// Empty is the M1 exit gate.
///
/// # The nine S1 delisted, and why that is not cheating
///
/// S1 implements no rule that any of these 49 cases is *about*. It still
/// delisted nine, all of them negative assertions — "does not emit a link",
/// "is not an emoji", "no strong here". They pass **vacuously**: nothing that
/// could emit the forbidden token exists yet.
///
/// The ratchet demanded the delisting (row two of the table above) and the
/// demand is right, which is the part worth stating. A vacuous pass and a real
/// pass are the same observation — *this input does not produce that token* —
/// and the input is the whole point. Leaving them listed would mean the list
/// no longer measures what is implemented, and, worse, that the moment S2
/// implements emphasis and `*\u{a0}a\u{a0}*` starts emitting an `em`, the
/// harness would say "still pending" instead of "regression".
///
/// Delisted, they are exactly what they were written to be: live guards that
/// fail the stage that breaks them. Every one of them names the stage that
/// will make it non-vacuous —
///
/// | Delisted at S1 | Becomes a real test at | |
/// |---|---|---|
/// | example 353, example 387 | S2 (`strong`, `em`) | **done** |
/// | example 475 | S2 (`strong`, `em`) | **done** |
/// | the two emoji word-boundary cases | S2 (`emoji`) | **done** |
/// | the sup/sub whitespace case | S2 (`super_sub_script`) | **done** |
/// | example 520 | S3 (`link`, `reference_link`) | **done** |
/// | the undefined reference label | S3 (`reference_link`) | **done** |
/// | the intra-word extension autolink | S5 (`auto_link_extension`) | **done** |
///
/// — and none of them can be made to pass by *not* implementing the rule,
/// because each stage's positive cases are still on this list.
///
/// **The stage in that column is the one that implements what the assertion
/// forbids, not the one that implements what the input contains.** S1's
/// version of this table put examples 475 and 520 at S4 because both inputs
/// hold an HTML tag; that is the wrong reading and S2 caught it. 475 forbids
/// `strong` and `em`, so it went live the moment `tryStrongEm` did — and it is
/// a real assertion now: `**<a href="**">` reaches the handler and is refused
/// by `lowerPriority`, whose rule set contains `html_tag`'s *regex* whether or
/// not `tryHtmlTag` exists. 520 forbids `link`, so it goes live at S3 for the
/// same reason, not at S4.
///
/// S3 is where that reading was tested rather than argued, and it held: 520's
/// `[foo <bar attr="](baz)">` now reaches `tryLink`, and `lowerPriority` with
/// `linkValidateRules` — which contains `html_tag` — is what refuses it, with
/// `tryHtmlTag` still returning `false`. The undefined-reference-label row
/// went live the same way: `[text][undefined-label]` reaches
/// `tryReferenceLink` and is refused by the *labels* lookup, not by absence.
/// All nine are now non-vacuous: S5 finished the table by making
/// `xhttps://example.com` reach `tryAutoLinkExtension`, where the boundary
/// allow-list `[* _~(]` refuses it because `x` is not in the list.
///
/// # The eleven delisted at S2
///
/// All eleven are positive assertions and none is vacuous: `example_521`
/// (`inline_code` outranks a tentative link), the two `super_sub_script`
/// cases, the two `strong`/`em`-plus-code cases (#1071), the two escaped-dollar
/// cases (#3778), the two remaining `emojiWordBoundary` cases, and both of
/// `inlineMathEscape`.
///
/// # The fourteen delisted at S3
///
/// The three `referenceLinkImageAnchor` cases, the three
/// `linkFollowedByAutolink` cases, and eight of `commonmarkExamples`: the
/// defined reference label, the four title cases and the three balanced-paren
/// cases. What was left was exactly S5 — every remaining entry an autolink.
/// **S4 delists nothing**, which is not a gap: `html_tag`'s two spec cases
/// (475 and 520) were already delisted as the rules that forbid them landed,
/// so S4's gate is its own attribute-scanner tests and the D3 site 2 fix.
///
/// # S4 confirmed that rather than assuming it
///
/// The prediction was checked after the fact, as the ratchet's row two makes
/// possible: implementing `try_html_tag` turned **no** listed case green, so
/// the list was still those fifteen. That is the useful direction of the
/// check — had something started passing, the ratchet would have failed the
/// build and said which, and that would have been information about a case
/// whose stage attribution was wrong. It did not.
///
/// Examples 475 and 520 were *already* non-vacuous before S4, because
/// `lowerPriority` runs `html_tag`'s **regex** whether or not `tryHtmlTag`
/// exists. Implementing the handler therefore could not have changed them,
/// which is why "S4 delists nothing" was predictable rather than lucky.
///
/// # The fifteen delisted at S5, which is the last of them
///
/// Every remaining entry was an autolink case, and the two autolink handlers
/// are the two S5 wrote: the three `commonmarkExamples` cases (an
/// angle-bracket autolink, a bare URL, a bare `www.` URL), all ten of
/// `autoLinkTrailingPunct`, and both of `autoLinkEncoding`.
///
/// One case that was **delisted back at S1** also goes live here rather than
/// being delisted by S5 — `does_not_start_an_extension_autolink_inside_a_word`,
/// which has been a vacuous pass for four stages because nothing could emit
/// the token it forbids. `xhttps://example.com` now reaches
/// `tryAutoLinkExtension` and is refused by the boundary guard, which is an
/// allow-list of `[* _~(]` and does not contain `x`. That empties the last row
/// of the S1 table above.
///
/// **`PENDING` is empty, which is M1's exit gate.** Every one of the 49 is a
/// live test with normal teeth, and row four of the table applies to all of
/// them: one that starts failing fails the build with its own diagnostic.
///
/// The ratchet stays. An empty list is not a spent mechanism — it is the state
/// the mechanism exists to reach and to hold, and the two bookkeeping tests
/// below still prove it works. S6 and S7 add no spec cases, so the only way
/// this list grows again is a regression, which is exactly when the machinery
/// should be there.
const PENDING: &[&str] = &[];

/// Run one transcribed spec case under the ratchet.
///
/// An **unlisted** case runs untouched — it panics straight through, so
/// libtest reports the real assertion failure at its real location with no
/// wrapper in the way. Only a **listed** case is caught, and the only thing
/// that can go wrong for one of those is that it started passing.
///
/// The expected panics of listed cases are not silenced: libtest captures
/// per-test output and prints it only for failures, so a normal run is clean
/// while `cargo test -- --nocapture` still shows exactly which cases are
/// pending and why. That is the more useful of the two behaviours, and it
/// needs no panic hook to get it.
fn spec_case(name: &str, body: impl FnOnce()) {
    spec_case_in(PENDING, name, body);
}

/// [`spec_case`], with the list injected.
///
/// Split out at S5, when `PENDING` became empty. The ratchet's own proof —
/// row two, *getting better must also fail the build* — needs a **listed**
/// case to run against, and with an empty list there is no longer a real one
/// to borrow. Testing the mechanism against a synthetic list is what keeps
/// emptying `PENDING` from silently retiring the mechanism that got it there.
fn spec_case_in(pending: &[&str], name: &str, body: impl FnOnce()) {
    if !pending.contains(&name) {
        body();
        return;
    }

    let outcome = panic::catch_unwind(AssertUnwindSafe(body));

    assert!(
        outcome.is_err(),
        "`{name}` now PASSES but is still listed in PENDING.\n\
         Delete it from that list — the list only shrinks, and a stale entry \
         lets a fixed case quietly regress later.\n\
         See the module docs and docs/M1.md §6."
    );
}

// ---------------------------------------------------------------------------
// Helpers
//
// Mirrors of the `topTypes` / `findByType` / `filter(t => t.type === X)`
// shapes the TypeScript specs use, so each test below reads like its original.
// ---------------------------------------------------------------------------

/// `tokenizer(src)` with muya's defaults.
fn tokenize(src: &str) -> Vec<Token> {
    mt_inline::tokenize(src)
}

/// `tokenizer(src, { labels })`. Keys are lowercased on the way in, as
/// `collectReferenceDefinitions` does.
fn tokenize_with_labels(src: &str, labels: &[(&str, &str)]) -> Vec<Token> {
    let labels: Labels = labels
        .iter()
        .map(|(label, href)| {
            (
                label.to_lowercase(),
                Label {
                    href: (*href).to_string(),
                    title: String::new(),
                },
            )
        })
        .collect();
    mt_inline::tokenizer(src, &TokenizerOptions::muya_default().with_labels(labels))
}

/// `tokenizer(src).map(t => t.type)`.
fn types(tokens: &[Token]) -> Vec<&'static str> {
    tokens.iter().map(Token::type_str).collect()
}

/// `tokens.find(t => t.type === ty)`.
fn find<'a>(tokens: &'a [Token], ty: &str) -> Option<&'a Token> {
    tokens.iter().find(|t| t.type_str() == ty)
}

/// `tokens.filter(t => t.type === ty)`.
fn all_of<'a>(tokens: &'a [Token], ty: &str) -> Vec<&'a Token> {
    tokens.iter().filter(|t| t.type_str() == ty).collect()
}

/// muya renders an absent optional group as `''`; so does this.
fn opt(src: &str, span: Option<Span>) -> &str {
    span.map_or("", |s| s.of(src))
}

// Payload accessors. Each panics with the actual `type` string, which is the
// diagnostic you want when a test fails because the wrong rule matched.

fn link_of(token: &Token) -> &Link {
    match &token.kind {
        TokenKind::Link(l) => l,
        other => panic!("expected a link token, got `{}`", other.type_str()),
    }
}

fn image_of(token: &Token) -> &Image {
    match &token.kind {
        TokenKind::Image(i) => i,
        other => panic!("expected an image token, got `{}`", other.type_str()),
    }
}

fn emphasis_of(token: &Token) -> &Emphasis {
    match &token.kind {
        TokenKind::Strong(e) | TokenKind::Em(e) | TokenKind::Del(e) => e,
        other => panic!("expected strong/em/del, got `{}`", other.type_str()),
    }
}

fn chunk_of(token: &Token) -> &CodeEmojiMath {
    match &token.kind {
        TokenKind::InlineCode(c) | TokenKind::Emoji(c) | TokenKind::InlineMath(c) => c,
        other => panic!(
            "expected inline_code/emoji/inline_math, got `{}`",
            other.type_str()
        ),
    }
}

/// `(marker, content)` of a `super_sub_script` token.
fn super_sub_of(token: &Token) -> (Span, Span) {
    match &token.kind {
        TokenKind::SuperSubScript { marker, content } => (*marker, *content),
        other => panic!("expected super_sub_script, got `{}`", other.type_str()),
    }
}

fn reference_link_of(token: &Token) -> &ReferenceLink {
    match &token.kind {
        TokenKind::ReferenceLink(l) => l,
        other => panic!(
            "expected a reference_link token, got `{}`",
            other.type_str()
        ),
    }
}

fn auto_link_of(token: &Token) -> &AutoLink {
    match &token.kind {
        TokenKind::AutoLink(a) => a,
        other => panic!("expected an auto_link token, got `{}`", other.type_str()),
    }
}

fn auto_link_ext_of(token: &Token) -> &AutoLinkExtension {
    match &token.kind {
        TokenKind::AutoLinkExtension(a) => a,
        other => panic!(
            "expected an auto_link_extension token, got `{}`",
            other.type_str()
        ),
    }
}

/// The trimmed extent of an extended autolink, as its `www` or `url` field.
///
/// `autoLinkTrailingPunct.spec.ts` reads `token.url`; every case there is a
/// url or www autolink, and `trimAutoLinkExtent` rewrites whichever
/// participated.
fn ext_target<'a>(src: &'a str, token: &Token) -> &'a str {
    let ext = auto_link_ext_of(token);
    opt(src, ext.target())
}

// ===========================================================================
// The ratchet's own bookkeeping
// ===========================================================================

/// Row two of the table: **getting better must also fail the build.** This is
/// the whole reason the ratchet is not `#[ignore]`, so it is checked rather
/// than assumed — run a listed name with a body that succeeds and require the
/// harness to reject it.
///
/// Against a synthetic list since S5, because [`PENDING`] is empty. See
/// [`spec_case_in`]: the mechanism has to keep being tested after it has
/// finished its job, or the last delisting quietly turns it off.
#[test]
fn a_listed_case_that_starts_passing_fails_the_build() {
    let pending = ["a_case_that_is_listed"];
    let outcome = panic::catch_unwind(|| spec_case_in(&pending, pending[0], || {}));
    let message = outcome
        .expect_err("a listed case that passes must fail the build")
        .downcast::<String>()
        .expect("the harness explains itself in a String panic");
    assert!(message.contains("PENDING"), "unhelpful message: {message}");
}

/// …and a listed case that still fails is still tolerated, which is row one.
/// Trivial with a non-empty list and worth stating now that the real one is
/// empty: the two rows together are what "ratchet" means.
#[test]
fn a_listed_case_that_still_fails_is_tolerated() {
    let pending = ["a_case_that_is_listed"];
    spec_case_in(&pending, pending[0], || panic!("still red, as listed"));
}

/// Row four: an **unlisted** case is not wrapped at all, so its own diagnostic
/// reaches libtest unchanged. Every one of the 49 is unlisted as of S5, so this
/// is now the path all of them take.
#[test]
fn an_unlisted_case_fails_with_its_own_diagnostic_not_the_harness_message() {
    let outcome = panic::catch_unwind(|| {
        spec_case("this-name-is-deliberately-not-in-pending", || {
            panic!("the case's own diagnostic")
        });
    });
    let payload = outcome.expect_err("the body panicked, so the case must fail");
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .expect("the body's own payload, not the harness's");
    assert_eq!(message, "the case's own diagnostic");
}

/// A duplicate would be a copy-paste that silently governs one case twice, and
/// would survive the list shrinking to "empty" while still holding an entry.
#[test]
fn the_pending_list_has_no_duplicates() {
    let mut seen: Vec<&str> = Vec::new();
    for name in PENDING {
        assert!(!seen.contains(name), "`{name}` is listed twice in PENDING");
        seen.push(name);
    }
}

/// **The M1 exit gate, as one number.**
///
/// It went 49 → 40 (S1) → 29 (S2) → 15 (S3) → 15 (S4) → **0** (S5) → 0 (S6),
/// and zero is where §6's *"S7's gate is `PENDING` is empty"* lands. The
/// assertion stays rather than being deleted along with the list: it is now the
/// thing that fails if a later stage tries to park a regression on the list
/// instead of fixing it.
///
/// **S6 delists nothing and adds nothing**, which is the state to expect from
/// here on. S6's work is `tokensToPlainText`, the `highlights` post-pass and
/// the marker-reveal predicate — three consumers of a finished token tree, none
/// of which any of the 49 spec cases asserts on. So the number moving *at all*
/// after S5 means a case regressed.
#[test]
fn the_pending_list_is_the_expected_length() {
    assert_eq!(
        PENDING.len(),
        0,
        "PENDING has been empty since M1 S5, and that is the milestone's exit \
         gate. Nothing after S5 adds spec cases, so a case appearing on this \
         list is a regression, not progress: it must be fixed rather than \
         listed, because listing it would mean M1 no longer holds."
    );
}

/// Whatever a spec case does or does not tokenize to, it tokenizes: every one
/// of the 49 inputs comes back with tokens that tile it exactly.
///
/// The 49 assert on token *shape*, so a case can pass while the token stream
/// around it has a hole in it. §3 rule 1 does not permit a hole, and
/// [`mt_inline::generator`] is that rule as an equality. Checking it here
/// means every spec input is also a round-trip fixture, which is what M1.md
/// §6 asks for by pulling `generator` forward into S1.
#[test]
fn every_spec_input_round_trips_through_the_generator() {
    // The inputs of all 49 cases, in the order the modules below use them.
    // Labels do not affect tiling, so the reference cases appear once.
    const INPUTS: &[&str] = &[
        r#"**<a href="**">"#,
        "*\u{a0}a\u{a0}*",
        "пристаням__стремятся__",
        r#"[foo <bar attr="](baz)">"#,
        "[foo`](/uri)`",
        "[text][undefined-label]",
        "[text][ref]",
        "text^sup^",
        "text~sub~",
        "text ^ foo ^ bar",
        "<https://example.com>",
        "https://example.com/path",
        "www.example.com",
        "xhttps://example.com",
        r#"[text](http://example.com "Example title")"#,
        "[text](http://example.com 'Example title')",
        r#"![alt](http://example.com/x.png "Pic title")"#,
        "[text](http://example.com)",
        "**`word 1`**, **`word 2`**, **`word 3`**",
        "*`word 1`*, *`word 2`*, *`word 3`*",
        "![alt](path/to/(file).png)",
        "[text](path/to/(file).html)",
        "see ![alt](first.png) and also (parens) here",
        r"It costs **\$20** to **\$30** online.",
        r"a *\$1* and *\$2* b",
        "http://some.domain.name/path/to/resource: rest",
        "https://example.com/a/b. Next sentence.",
        "https://example.com/a:b:c! end",
        "https://example.com/a/b end",
        "(https://en.wikipedia.org/wiki/Foo_(bar)) end",
        "https://example.com/foo(bar) end",
        "https://example.com/foo?bar=1&amp; end",
        "https://example.com/a;b; end",
        "https://example.com/a<b end",
        "(see https://example.com/path). rest",
        "12:00-14:00",
        "hello:smile:",
        ":smile:",
        "lunch :100: today",
        "支持[CommonMark 规范](https://spec.commonmark.org/)、其他",
        "支持[CommonMark 规范](https://spec.commonmark.org/)、 \
         [GitHub Flavored Markdown 规范](https://github.github.com/gfm/)",
        "see [docs](https://example.com/path)next",
        "[![alt](https://example.com/badge.svg)][ref]",
        "[![alt](https://example.com/badge.svg)][missing]",
        "<https://www.google.com/search?q=marktext%20foo%20bar>",
        "<https://example.com/a?b=c&d=e>",
        r"$y = \$10000$",
        "$a+b$",
    ];

    for src in INPUTS {
        let tokens = tokenize(src);
        assert_eq!(
            &mt_inline::generator(src, &tokens),
            src,
            "the token stream does not reproduce its input"
        );
    }
}

// ===========================================================================
// commonmarkExamples.spec.ts — 25 cases
// ===========================================================================

/// Defensive regression tests for the CommonMark 0.29 spec examples that the
/// legacy marktext inline lexer failed before commit 57cd04c5 (Apr 2019, PR
/// #957), plus six later marktext fixes that the new muya lexer inherits.
///
/// They assert on the *shape* of the token stream rather than running the full
/// markdown→HTML pipeline, because the inline lexer is what marktext patched
/// and what muya forked from.
mod commonmark_examples {
    use super::*;

    // --- CommonMark 0.29 examples (marktext 57cd04c5) ----------------------

    /// §6.4 example 475 — `**<a href="**">`. The stars are inside an HTML
    /// attribute value, so neither pair may open or close a strong span.
    #[test]
    fn example_475_stars_inside_an_html_attribute_do_not_start_strong() {
        spec_case(
            "example_475_stars_inside_an_html_attribute_do_not_start_strong",
            || {
                let types = types(&tokenize(r#"**<a href="**">"#));
                assert!(!types.contains(&"strong"), "tokens: {types:?}");
                assert!(!types.contains(&"em"), "tokens: {types:?}");
            },
        );
    }

    /// §6.4 example 353 — `* a *` with **non-breaking spaces (U+00A0)** around
    /// the `a`. NBSP counts as Unicode whitespace, so the `*` markers are
    /// followed/preceded by whitespace and cannot open emphasis.
    ///
    /// The U+00A0s are the entire point of the case and are invisible in the
    /// TypeScript source, so they are written as escapes here. They are also
    /// why M1.md §4 C2 insists the whitespace class be defined explicitly
    /// rather than left to `\s`: JavaScript's `\s` and Rust's
    /// `\p{White_Space}` do not agree on U+FEFF or U+0085.
    #[test]
    fn example_353_star_adjacent_to_a_non_breaking_space_does_not_open_em() {
        spec_case(
            "example_353_star_adjacent_to_a_non_breaking_space_does_not_open_em",
            || {
                let types = types(&tokenize("*\u{a0}a\u{a0}*"));
                assert!(!types.contains(&"em"), "tokens: {types:?}");
                assert!(!types.contains(&"strong"), "tokens: {types:?}");
            },
        );
    }

    /// §6.4 example 387 — `пристаням__стремятся__`. The intraword `__` rule
    /// says `_` flanked by alphanumerics on both sides cannot open or close
    /// emphasis, so the output is literal text.
    #[test]
    fn example_387_intraword_double_underscore_does_not_open_em_or_strong() {
        spec_case(
            "example_387_intraword_double_underscore_does_not_open_em_or_strong",
            || {
                let types = types(&tokenize("пристаням__стремятся__"));
                assert!(!types.contains(&"em"), "tokens: {types:?}");
                assert!(!types.contains(&"strong"), "tokens: {types:?}");
            },
        );
    }

    /// §6.6 example 520 — `[foo <bar attr="](baz)">`. The HTML tag spans what
    /// would otherwise be the link closer, and HTML tags bind more tightly
    /// than links, so no link forms.
    #[test]
    fn example_520_html_tag_takes_precedence_over_a_tentative_link() {
        spec_case(
            "example_520_html_tag_takes_precedence_over_a_tentative_link",
            || {
                let types = types(&tokenize(r#"[foo <bar attr="](baz)">"#));
                assert!(!types.contains(&"link"), "tokens: {types:?}");
                assert!(!types.contains(&"reference_link"), "tokens: {types:?}");
            },
        );
    }

    /// §6.6 example 521 — a code span sits inside what looks like a link, and
    /// code spans bind more tightly, so the link never forms.
    #[test]
    fn example_521_code_span_takes_precedence_over_a_tentative_link() {
        spec_case(
            "example_521_code_span_takes_precedence_over_a_tentative_link",
            || {
                let types = types(&tokenize("[foo`](/uri)`"));
                assert!(!types.contains(&"link"), "tokens: {types:?}");
                assert!(!types.contains(&"reference_link"), "tokens: {types:?}");
                assert!(types.contains(&"inline_code"), "tokens: {types:?}");
            },
        );
    }

    // --- reference link (marktext d9f64bab, issue #921, PR #947) -----------

    /// A `[text][label]` needs the label defined in the `labels` map;
    /// otherwise it stays text (`lexer.ts:415`).
    #[test]
    fn does_not_emit_reference_link_when_the_label_is_undefined() {
        spec_case(
            "does_not_emit_reference_link_when_the_label_is_undefined",
            || {
                let types = types(&tokenize("[text][undefined-label]"));
                assert!(!types.contains(&"reference_link"), "tokens: {types:?}");
            },
        );
    }

    #[test]
    fn emits_reference_link_when_the_label_is_defined() {
        spec_case("emits_reference_link_when_the_label_is_defined", || {
            let tokens = tokenize_with_labels("[text][ref]", &[("ref", "http://example.com")]);
            let types = types(&tokens);
            assert!(types.contains(&"reference_link"), "tokens: {types:?}");
        });
    }

    // --- superscript / subscript (marktext 8e32838b, PR #1531) -------------

    #[test]
    fn parses_caret_wrapped_text_as_super_sub_script_with_a_caret_marker() {
        spec_case(
            "parses_caret_wrapped_text_as_super_sub_script_with_a_caret_marker",
            || {
                let src = "text^sup^";
                let tokens = tokenize(src);
                let token = find(&tokens, "super_sub_script")
                    .unwrap_or_else(|| panic!("no super_sub_script; tokens: {:?}", types(&tokens)));
                let (marker, content) = super_sub_of(token);
                assert_eq!(marker.of(src), "^");
                assert_eq!(content.of(src), "sup");
            },
        );
    }

    #[test]
    fn parses_tilde_wrapped_text_as_super_sub_script_with_a_tilde_marker() {
        spec_case(
            "parses_tilde_wrapped_text_as_super_sub_script_with_a_tilde_marker",
            || {
                let src = "text~sub~";
                let tokens = tokenize(src);
                let token = find(&tokens, "super_sub_script")
                    .unwrap_or_else(|| panic!("no super_sub_script; tokens: {:?}", types(&tokens)));
                let (marker, content) = super_sub_of(token);
                assert_eq!(marker.of(src), "~");
                assert_eq!(content.of(src), "sub");
            },
        );
    }

    /// The marker must abut non-whitespace on both sides.
    #[test]
    fn does_not_parse_super_sub_script_when_the_marker_is_surrounded_by_whitespace() {
        spec_case(
            "does_not_parse_super_sub_script_when_the_marker_is_surrounded_by_whitespace",
            || {
                let types = types(&tokenize("text ^ foo ^ bar"));
                assert!(!types.contains(&"super_sub_script"), "tokens: {types:?}");
            },
        );
    }

    // --- auto link (marktext c0853f64, PR #1421) ---------------------------

    #[test]
    fn parses_an_angle_bracket_autolink_as_auto_link() {
        spec_case("parses_an_angle_bracket_autolink_as_auto_link", || {
            let types = types(&tokenize("<https://example.com>"));
            assert!(types.contains(&"auto_link"), "tokens: {types:?}");
        });
    }

    #[test]
    fn parses_a_bare_url_as_auto_link_extension() {
        spec_case("parses_a_bare_url_as_auto_link_extension", || {
            let types = types(&tokenize("https://example.com/path"));
            assert!(types.contains(&"auto_link_extension"), "tokens: {types:?}");
        });
    }

    #[test]
    fn parses_a_bare_www_url_as_auto_link_extension() {
        spec_case("parses_a_bare_www_url_as_auto_link_extension", || {
            let types = types(&tokenize("www.example.com"));
            assert!(types.contains(&"auto_link_extension"), "tokens: {types:?}");
        });
    }

    /// The boundary guard requires the preceding character to be one of
    /// `[* _~(]` or the start of input, so `xhttps://…` must not autolink.
    #[test]
    fn does_not_start_an_extension_autolink_inside_a_word() {
        spec_case("does_not_start_an_extension_autolink_inside_a_word", || {
            let types = types(&tokenize("xhttps://example.com"));
            assert!(!types.contains(&"auto_link_extension"), "tokens: {types:?}");
        });
    }

    // --- GFM link/image title (marktext ad5ddbf9, GFM example 558, PR #917) -

    #[test]
    fn extracts_the_title_from_a_link_with_a_double_quoted_title() {
        spec_case(
            "extracts_the_title_from_a_link_with_a_double_quoted_title",
            || {
                let src = r#"[text](http://example.com "Example title")"#;
                let tokens = tokenize(src);
                let link = link_of(find(&tokens, "link").expect("a link token"));
                assert_eq!(link.href.of(src), "http://example.com");
                assert_eq!(opt(src, link.title), "Example title");
            },
        );
    }

    #[test]
    fn extracts_the_title_from_a_link_with_a_single_quoted_title() {
        spec_case(
            "extracts_the_title_from_a_link_with_a_single_quoted_title",
            || {
                let src = "[text](http://example.com 'Example title')";
                let tokens = tokenize(src);
                let link = link_of(find(&tokens, "link").expect("a link token"));
                assert_eq!(link.href.of(src), "http://example.com");
                assert_eq!(opt(src, link.title), "Example title");
            },
        );
    }

    #[test]
    fn extracts_the_title_from_an_image() {
        spec_case("extracts_the_title_from_an_image", || {
            let src = r#"![alt](http://example.com/x.png "Pic title")"#;
            let tokens = tokenize(src);
            let image = image_of(find(&tokens, "image").expect("an image token"));
            assert_eq!(image.src.of(src), "http://example.com/x.png");
            assert_eq!(opt(src, image.title), "Pic title");
        });
    }

    /// muya reports a missing title as `''`; the port reports `None`, which
    /// renders as `""` at the wire boundary. See [`super::opt`].
    #[test]
    fn leaves_the_title_empty_when_the_destination_has_no_title() {
        spec_case(
            "leaves_the_title_empty_when_the_destination_has_no_title",
            || {
                let src = "[text](http://example.com)";
                let tokens = tokenize(src);
                let link = link_of(find(&tokens, "link").expect("a link token"));
                assert_eq!(link.href.of(src), "http://example.com");
                assert_eq!(opt(src, link.title), "");
            },
        );
    }

    // --- repeated bold + inline_code (marktext d937fac0, issue #1071) ------

    /// `**\`word 1\`**, **\`word 2\`**` used to bold only the LAST instance:
    /// `lowerPriority` walked every position in the candidate span without
    /// remembering which characters an earlier matching rule had already
    /// consumed, so a code span ending mid-span fooled the strong rule into
    /// thinking another rule extended past the closer. The fix tracks consumed
    /// positions in `ignoreIndex` (`utils.ts:142`).
    #[test]
    fn emits_a_strong_token_for_every_double_star_wrapped_code_span_in_a_sequence() {
        spec_case(
            "emits_a_strong_token_for_every_double_star_wrapped_code_span_in_a_sequence",
            || {
                let tokens = tokenize("**`word 1`**, **`word 2`**, **`word 3`**");
                let strongs = all_of(&tokens, "strong");
                assert_eq!(strongs.len(), 3, "tokens: {:?}", types(&tokens));

                // And each must contain a code span, not literal text.
                for strong in strongs {
                    let inner = types(&emphasis_of(strong).children);
                    assert!(inner.contains(&"inline_code"), "children: {inner:?}");
                }
            },
        );
    }

    #[test]
    fn does_not_flip_the_same_bug_to_em_with_a_single_star_and_code() {
        spec_case(
            "does_not_flip_the_same_bug_to_em_with_a_single_star_and_code",
            || {
                let tokens = tokenize("*`word 1`*, *`word 2`*, *`word 3`*");
                assert_eq!(
                    all_of(&tokens, "em").len(),
                    3,
                    "tokens: {:?}",
                    types(&tokens)
                );
            },
        );
    }

    // --- link / image dest with parens (marktext 57af8304, issue #1169) ----

    #[test]
    fn parses_an_image_destination_containing_balanced_parens() {
        spec_case(
            "parses_an_image_destination_containing_balanced_parens",
            || {
                let src = "![alt](path/to/(file).png)";
                let tokens = tokenize(src);
                let token = find(&tokens, "image")
                    .unwrap_or_else(|| panic!("tokens: {:?}", types(&tokens)));
                assert_eq!(image_of(token).src.of(src), "path/to/(file).png");
            },
        );
    }

    #[test]
    fn parses_a_link_destination_containing_balanced_parens() {
        spec_case(
            "parses_a_link_destination_containing_balanced_parens",
            || {
                let src = "[text](path/to/(file).html)";
                let tokens = tokenize(src);
                let token =
                    find(&tokens, "link").unwrap_or_else(|| panic!("tokens: {:?}", types(&tokens)));
                assert_eq!(link_of(token).href.of(src), "path/to/(file).html");
            },
        );
    }

    /// The real shape of the marktext regression: the greedy `(.*)` in the
    /// image regexp gobbled all the way to the LAST `)`, swallowing both
    /// `first.png` and the unrelated `(parens)` text. `correctUrl` /
    /// `findClosingBracket` walk the destination to the matching `)`.
    #[test]
    fn stops_at_the_first_matching_paren_when_more_appear_later_on_the_line() {
        spec_case(
            "stops_at_the_first_matching_paren_when_more_appear_later_on_the_line",
            || {
                let src = "see ![alt](first.png) and also (parens) here";
                let tokens = tokenize(src);
                let token = find(&tokens, "image")
                    .unwrap_or_else(|| panic!("tokens: {:?}", types(&tokens)));
                let image_raw = token.raw;
                assert_eq!(image_of(token).src.of(src), "first.png");

                // The trailing text — including the unrelated `(parens)` group
                // — stays outside the image token. Concatenating every `raw`
                // is §3 rule 1, the tiling invariant, which is what the
                // TypeScript's `.join('')` is testing here without naming it.
                let joined: String = tokens.iter().map(|t| t.raw.of(src)).collect();
                assert!(
                    joined.contains("and also (parens) here"),
                    "joined: {joined:?}"
                );
                assert!(!image_raw.of(src).contains("parens"));
            },
        );
    }

    // --- bold with escaped dollar signs (#3778) ---------------------------

    /// `lowerPriority` scanned for a higher-priority construct that would
    /// overlap the emphasis, but treated an escaped `\$` as a real `$` math
    /// delimiter — so the first `**bold**` on a line that also held a later
    /// `\$` was suppressed.
    #[test]
    fn emits_a_strong_token_for_every_span_containing_an_escaped_dollar() {
        spec_case(
            "emits_a_strong_token_for_every_span_containing_an_escaped_dollar",
            || {
                let tokens = tokenize(r"It costs **\$20** to **\$30** online.");
                assert_eq!(
                    all_of(&tokens, "strong").len(),
                    2,
                    "tokens: {:?}",
                    types(&tokens)
                );
            },
        );
    }

    #[test]
    fn still_emits_em_for_star_wrapped_spans_containing_an_escaped_dollar() {
        spec_case(
            "still_emits_em_for_star_wrapped_spans_containing_an_escaped_dollar",
            || {
                let tokens = tokenize(r"a *\$1* and *\$2* b");
                assert_eq!(
                    all_of(&tokens, "em").len(),
                    2,
                    "tokens: {:?}",
                    types(&tokens)
                );
            },
        );
    }
}

// ===========================================================================
// autoLinkTrailingPunct.spec.ts — 10 cases
// ===========================================================================

/// #2096 — an extended (bare) autolink swallowed trailing punctuation because
/// the path component matches `\S+`. Per GFM §6.9 the match is greedy, so the
/// extent is trimmed **after** the regex by `trimAutoLinkExtent`
/// (`lexer.ts:528`), mirroring cmark-gfm's `autolink_delim`.
mod auto_link_trailing_punct {
    use super::*;

    /// The `url` (or `www`) field of the first `auto_link_extension` token —
    /// the TypeScript's `autoLinkExt(src)!.url`.
    fn url_of(src: &str) -> &str {
        let tokens = tokenize(src);
        let token = find(&tokens, "auto_link_extension")
            .unwrap_or_else(|| panic!("no auto_link_extension; tokens: {:?}", types(&tokens)));
        ext_target(src, token)
    }

    // --- trailing punctuation (#2096) --------------------------------------

    #[test]
    fn excludes_a_trailing_colon_from_the_link() {
        spec_case("excludes_a_trailing_colon_from_the_link", || {
            let src = "http://some.domain.name/path/to/resource: rest";
            let tokens = tokenize(src);
            let token = find(&tokens, "auto_link_extension")
                .unwrap_or_else(|| panic!("no auto_link_extension; tokens: {:?}", types(&tokens)));
            assert_eq!(
                ext_target(src, token),
                "http://some.domain.name/path/to/resource"
            );
            // `raw` is trimmed too, so the colon re-enters the loop as text.
            assert_eq!(
                token.raw.of(src),
                "http://some.domain.name/path/to/resource"
            );
        });
    }

    #[test]
    fn excludes_a_trailing_period_at_the_end_of_a_sentence() {
        spec_case(
            "excludes_a_trailing_period_at_the_end_of_a_sentence",
            || {
                assert_eq!(
                    url_of("https://example.com/a/b. Next sentence."),
                    "https://example.com/a/b"
                );
            },
        );
    }

    #[test]
    fn keeps_interior_punctuation_and_only_trims_the_trailing_run() {
        spec_case(
            "keeps_interior_punctuation_and_only_trims_the_trailing_run",
            || {
                assert_eq!(
                    url_of("https://example.com/a:b:c! end"),
                    "https://example.com/a:b:c"
                );
            },
        );
    }

    #[test]
    fn leaves_a_clean_url_untouched() {
        spec_case("leaves_a_clean_url_untouched", || {
            assert_eq!(
                url_of("https://example.com/a/b end"),
                "https://example.com/a/b"
            );
        });
    }

    // --- GFM §6.9 extent trimming -----------------------------------------

    /// An unmatched trailing `)` is excluded when the link has more `)` than
    /// `(`, so an autolink can sit inside parentheses.
    #[test]
    fn excludes_a_trailing_paren_when_parens_are_unbalanced() {
        spec_case(
            "excludes_a_trailing_paren_when_parens_are_unbalanced",
            || {
                assert_eq!(
                    url_of("(https://en.wikipedia.org/wiki/Foo_(bar)) end"),
                    "https://en.wikipedia.org/wiki/Foo_(bar)"
                );
            },
        );
    }

    #[test]
    fn keeps_a_trailing_paren_when_parens_are_balanced() {
        spec_case("keeps_a_trailing_paren_when_parens_are_balanced", || {
            assert_eq!(
                url_of("https://example.com/foo(bar) end"),
                "https://example.com/foo(bar)"
            );
        });
    }

    /// A trailing `;` closing an `&entity;`-looking reference is excluded.
    #[test]
    fn excludes_a_trailing_entity_reference() {
        spec_case("excludes_a_trailing_entity_reference", || {
            assert_eq!(
                url_of("https://example.com/foo?bar=1&amp; end"),
                "https://example.com/foo?bar=1"
            );
        });
    }

    #[test]
    fn keeps_a_bare_trailing_semicolon_that_is_not_an_entity() {
        spec_case(
            "keeps_a_bare_trailing_semicolon_that_is_not_an_entity",
            || {
                assert_eq!(
                    url_of("https://example.com/a;b; end"),
                    "https://example.com/a;b;"
                );
            },
        );
    }

    /// A `<` ends the autolink.
    #[test]
    fn ends_the_link_at_a_less_than_character() {
        spec_case("ends_the_link_at_a_less_than_character", || {
            assert_eq!(
                url_of("https://example.com/a<b end"),
                "https://example.com/a"
            );
        });
    }

    /// The rules interleave and apply repeatedly: for `").` the `.` is
    /// trimmed, which leaves the `)` unbalanced, so it goes too.
    #[test]
    fn applies_the_rules_repeatedly_for_a_trailing_paren_then_period() {
        spec_case(
            "applies_the_rules_repeatedly_for_a_trailing_paren_then_period",
            || {
                assert_eq!(
                    url_of("(see https://example.com/path). rest"),
                    "https://example.com/path"
                );
            },
        );
    }
}

// ===========================================================================
// emojiWordBoundary.spec.ts — 4 cases
// ===========================================================================

/// #1677 — the emoji rule `/^(:)([a-z_\d+-]+)\1/` matched the colons inside a
/// timestamp range like `12:00-14:00`, so `:00-14:` tokenized as an invalid
/// emoji and rendered with the red `mu-warn` styling. An emoji opener must sit
/// at a word boundary: a `:` glued to a preceding letter or digit is not the
/// start of a shortcode.
///
/// All four cases are **top level**, which is why none of them is disturbed by
/// the D3 site-1 fix: muya's boundary check reads `state.originSrc[state.pos
/// - 1]`, and at the top level `originSrc` and `pos` agree. The fix only
/// changes nested calls, and the new tests for that behaviour arrive with it
/// in S2 — see `spec/divergences.json`.
mod emoji_word_boundary {
    use super::*;

    #[test]
    fn does_not_treat_the_colons_in_a_timestamp_range_as_an_emoji() {
        spec_case(
            "does_not_treat_the_colons_in_a_timestamp_range_as_an_emoji",
            || {
                assert!(all_of(&tokenize("12:00-14:00"), "emoji").is_empty());
            },
        );
    }

    #[test]
    fn does_not_treat_a_colon_glued_to_a_word_as_an_emoji() {
        spec_case("does_not_treat_a_colon_glued_to_a_word_as_an_emoji", || {
            assert!(all_of(&tokenize("hello:smile:"), "emoji").is_empty());
        });
    }

    #[test]
    fn still_recognises_an_emoji_at_the_start_of_the_text() {
        spec_case("still_recognises_an_emoji_at_the_start_of_the_text", || {
            assert!(!all_of(&tokenize(":smile:"), "emoji").is_empty());
        });
    }

    #[test]
    fn still_recognises_an_emoji_after_whitespace() {
        spec_case("still_recognises_an_emoji_after_whitespace", || {
            assert!(!all_of(&tokenize("lunch :100: today"), "emoji").is_empty());
        });
    }
}

// ===========================================================================
// linkFollowedByAutolink.spec.ts — 3 cases
// ===========================================================================

/// #4671 — a well-formed inline or reference link must not be vetoed just
/// because its destination URL also matches the GFM extended (bare-URL)
/// autolink rule. Extended autolinks are a post-process over plain text and
/// bind LESS tightly than a link, so they may never override `[text](url)`.
///
/// This is what `linkValidateRules` exists for (M1.md §5 D5): `tryLink` and
/// `tryReferenceLink` pass only `inline_code`, `html_tag` and `auto_link` to
/// `lowerPriority`, never the full `validateRules`.
mod link_followed_by_autolink {
    use super::*;

    #[test]
    fn recognizes_a_link_whose_closing_paren_is_followed_by_a_cjk_comma() {
        spec_case(
            "recognizes_a_link_whose_closing_paren_is_followed_by_a_cjk_comma",
            || {
                let src = "支持[CommonMark 规范](https://spec.commonmark.org/)、其他";
                let tokens = tokenize(src);
                let links = all_of(&tokens, "link");
                assert_eq!(links.len(), 1, "tokens: {:?}", types(&tokens));
                assert_eq!(
                    link_of(links[0]).href.of(src),
                    "https://spec.commonmark.org/"
                );
                assert_eq!(
                    links[0].raw.of(src),
                    "[CommonMark 规范](https://spec.commonmark.org/)"
                );
            },
        );
    }

    #[test]
    fn recognizes_the_first_of_two_links_on_one_line() {
        spec_case("recognizes_the_first_of_two_links_on_one_line", || {
            let src = "支持[CommonMark 规范](https://spec.commonmark.org/)、 \
                       [GitHub Flavored Markdown 规范](https://github.github.com/gfm/)";
            let tokens = tokenize(src);
            let links = all_of(&tokens, "link");
            assert_eq!(links.len(), 2, "tokens: {:?}", types(&tokens));
            assert_eq!(
                link_of(links[0]).href.of(src),
                "https://spec.commonmark.org/"
            );
            assert_eq!(
                link_of(links[1]).href.of(src),
                "https://github.github.com/gfm/"
            );
        });
    }

    #[test]
    fn recognizes_a_link_whose_destination_url_is_immediately_followed_by_text() {
        spec_case(
            "recognizes_a_link_whose_destination_url_is_immediately_followed_by_text",
            || {
                let src = "see [docs](https://example.com/path)next";
                let tokens = tokenize(src);
                let links = all_of(&tokens, "link");
                assert_eq!(links.len(), 1, "tokens: {:?}", types(&tokens));
                assert_eq!(link_of(links[0]).href.of(src), "https://example.com/path");
                assert_eq!(links[0].raw.of(src), "[docs](https://example.com/path)");
            },
        );
    }
}

// ===========================================================================
// referenceLinkImageAnchor.spec.ts — 3 cases
// ===========================================================================

/// #4865 — a full reference link whose text is an image, `[![alt](img)][ref]`
/// (the standard README-badge pattern), must tokenize as ONE `reference_link`
/// carrying the image as its child. The anchor group used `[^\]]+?`, which
/// stopped at the image's inner `]`, so the input fragmented into a bare image
/// plus a separate empty reference link.
///
/// This is the only one of the seven spec files that reaches into token
/// fields rather than asserting on shape alone.
mod reference_link_image_anchor {
    use super::*;

    const REF: &[(&str, &str)] = &[("ref", "https://example.com/dst")];

    #[test]
    fn tokenizes_a_reference_link_wrapping_an_image_as_a_single_token() {
        spec_case(
            "tokenizes_a_reference_link_wrapping_an_image_as_a_single_token",
            || {
                let src = "[![alt](https://example.com/badge.svg)][ref]";
                let result = tokenize_with_labels(src, REF);

                assert_eq!(result.len(), 1, "tokens: {:?}", types(&result));
                assert_eq!(result[0].type_str(), "reference_link");

                let link = reference_link_of(&result[0]);
                assert!(link.is_full_link);
                assert_eq!(link.label.of(src), "ref");

                assert_eq!(link.children.len(), 1);
                let child = &link.children[0];
                assert_eq!(child.type_str(), "image");
                // `attrs` is the percent-encoded render-facing copy, so it is
                // an owned String rather than a span — see `ImageAttrs`.
                assert_eq!(image_of(child).attrs.src, "https://example.com/badge.svg");
                assert_eq!(image_of(child).attrs.alt, "alt");
            },
        );
    }

    #[test]
    fn still_tokenizes_a_plain_text_reference_link_unchanged() {
        spec_case(
            "still_tokenizes_a_plain_text_reference_link_unchanged",
            || {
                let src = "[text][ref]";
                let result = tokenize_with_labels(src, REF);

                assert_eq!(result.len(), 1, "tokens: {:?}", types(&result));
                assert_eq!(result[0].type_str(), "reference_link");

                let link = reference_link_of(&result[0]);
                assert_eq!(link.children.len(), 1);
                assert_eq!(link.children[0].type_str(), "text");
            },
        );
    }

    /// With no matching definition it is not a reference link (CommonMark
    /// §6.5); the image stays standalone and no link is fabricated.
    #[test]
    fn does_not_treat_an_image_anchor_as_a_link_when_the_ref_is_undefined() {
        spec_case(
            "does_not_treat_an_image_anchor_as_a_link_when_the_ref_is_undefined",
            || {
                let result = tokenize("[![alt](https://example.com/badge.svg)][missing]");
                let types = types(&result);
                assert!(!types.contains(&"reference_link"), "tokens: {types:?}");
                assert!(types.contains(&"image"), "tokens: {types:?}");
            },
        );
    }
}

// ===========================================================================
// autoLinkEncoding.spec.ts — 2 cases
// ===========================================================================

/// #3548 — a CommonMark autolink's href is the literal source between `<` and
/// `>`, which the author has already percent-encoded. The renderer used
/// `encodeURI(href)`, re-encoding `%` to `%25` and turning `%20` into `%2520`,
/// so the followed link pointed at the wrong URL. Standard `[text](url)` links
/// never encode the href, so autolinks must match.
///
/// # Deviation from the TypeScript original
///
/// `autoLinkEncoding.spec.ts` is the only one of the seven that is not a
/// tokenizer test. It boots a `Muya` instance against `happy-dom`, renders,
/// and reads `a.mu-auto-link[href]`. `mt-inline` has no renderer and no DOM,
/// and §1 forbids it ever having one.
///
/// Split, then, into the half each layer owns:
///
/// - **Here (M1):** the `auto_link` token's `href` is the literal source
///   between the angle brackets — no encoding, no decoding, no normalisation.
///   That is the precondition the fix rests on; if the tokenizer mangles the
///   href, no renderer can un-mangle it.
/// - **Owed (the milestone that lands the renderer, §9 M3/M4):** the anchor's
///   `href` attribute is emitted from that span verbatim rather than through
///   an `encodeURI` equivalent. Tracked in docs/M1.md §10 alongside the
///   outstanding `normalizeHtml` check, which is owed for the same reason —
///   the consuming layer does not exist yet.
///
/// **S5 closed the first half and not the second.** These two cases now pass,
/// and `PENDING` is empty, so nothing in this repository is red on account of
/// #3548 any more. That is exactly the situation in which an owed check becomes
/// a forgotten one, which is why §10 carries it rather than this file alone:
/// a green tokenizer test is not evidence that the rendered `<a href>` is
/// right, because no renderer exists to be wrong yet.
mod auto_link_encoding {
    use super::*;

    /// The TypeScript boots Muya with a trailing `\n`, which is block-level
    /// framing; the block text the tokenizer sees is the autolink itself.
    #[test]
    fn keeps_an_already_encoded_percent_twenty_intact_instead_of_double_encoding_it() {
        spec_case(
            "keeps_an_already_encoded_percent_twenty_intact_instead_of_double_encoding_it",
            || {
                let src = "<https://www.google.com/search?q=marktext%20foo%20bar>";
                let tokens = tokenize(src);
                let token = find(&tokens, "auto_link")
                    .unwrap_or_else(|| panic!("no auto_link; tokens: {:?}", types(&tokens)));
                let href = opt(src, auto_link_of(token).href);

                assert!(href.contains("%20"), "href: {href:?}");
                assert!(!href.contains("%2520"), "href: {href:?}");
                assert_eq!(href, "https://www.google.com/search?q=marktext%20foo%20bar");
            },
        );
    }

    #[test]
    fn does_not_re_encode_reserved_characters_in_a_query_string() {
        spec_case(
            "does_not_re_encode_reserved_characters_in_a_query_string",
            || {
                let src = "<https://example.com/a?b=c&d=e>";
                let tokens = tokenize(src);
                let token = find(&tokens, "auto_link")
                    .unwrap_or_else(|| panic!("no auto_link; tokens: {:?}", types(&tokens)));
                assert_eq!(
                    opt(src, auto_link_of(token).href),
                    "https://example.com/a?b=c&d=e"
                );
            },
        );
    }
}

// ===========================================================================
// inlineMathEscape.spec.ts — 2 cases
// ===========================================================================

/// #4555 — an escaped dollar `\$` inside an inline math span broke the block's
/// rendering. The `inline_math` content group did not allow backslash escapes,
/// so the inner `\$` was read as the closing delimiter and the expression was
/// truncated. The rule is now
/// `/^(\$)((?:[^$\\]|\\.)+)(\\*)\1(?!\1)/`.
mod inline_math_escape {
    use super::*;

    fn math_content<'a>(src: &'a str, tokens: &[Token]) -> Option<&'a str> {
        find(tokens, "inline_math").map(|t| chunk_of(t).content.of(src))
    }

    #[test]
    fn keeps_an_escaped_dollar_inside_the_math_expression() {
        spec_case("keeps_an_escaped_dollar_inside_the_math_expression", || {
            let src = r"$y = \$10000$";
            let tokens = tokenize(src);
            assert_eq!(math_content(src, &tokens), Some(r"y = \$10000"));
        });
    }

    #[test]
    fn still_tokenizes_a_plain_expression_unchanged() {
        spec_case("still_tokenizes_a_plain_expression_unchanged", || {
            let src = "$a+b$";
            let tokens = tokenize(src);
            assert_eq!(math_content(src, &tokens), Some("a+b"));
        });
    }
}
