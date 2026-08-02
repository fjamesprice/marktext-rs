//! M1.md §4 C2, checked against the engine rather than against the standard.
//!
//! C2 is the correction that *"JavaScript and Rust regex classes are not the
//! same"*, and it calls the differences **faithful-port traps that will not
//! show up as compile errors**. `rules.rs` writes every class out explicitly
//! because of it, and has a unit test —
//! `every_character_class_matches_what_javascript_matches` — asserting that
//! the classes contain what ECMA-262 says they contain.
//!
//! That test asserts a *reading of the standard*. This file asserts the
//! *behaviour of muya*, which is what §3 rule 3 says defines correctness:
//!
//! > **Behaviour is defined by the TS tests, not by the spec.**
//!
//! The two are not the same claim. A class can be right about ECMA-262 and
//! still be plugged into the wrong place in a rule, and only running both
//! engines catches that.
//!
//! # Provenance
//!
//! Every expectation below was produced by running muya's `tokenizer` from
//! `packages/muya/src/inlineRenderer/lexer.ts` at the pinned reference commit
//! (`fjamesprice/marktext@e52106fd`, the `MARKTEXT_REF` in `.github/
//! workflows/ci.yml`) over the input beside it, with default options, and
//! recording the `type` of each token it returned. They are measurements, not
//! predictions.
//!
//! To reproduce or extend, from the repository root with the marktext clone
//! as a sibling:
//!
//! ```js
//! // node --import tsx thisfile.mjs
//! import { pathToFileURL } from 'node:url';
//! const { tokenizer } = await import(pathToFileURL(
//!     '../marktext/packages/muya/src/inlineRenderer/lexer.ts').href);
//! console.log(tokenizer('#x', {}).map(t => t.type));
//! ```
//!
//! # Why this is not `cargo xtask diff`
//!
//! It should be, eventually. The differential harness (§11.2) compares
//! **block state**, not token streams, so it cannot run these today; when it
//! grows a token-stream mode this file becomes redundant and should be
//! deleted in favour of it. Until then, hand-transcribed goldens are the
//! difference between "the classes are checked against muya" and "the classes
//! are checked against my reading of ECMA-262", and the first is what the
//! milestone's verification strategy rests on.
//!
//! # Scope
//!
//! Only the rules S1 implements can be checked this way, because only they
//! produce a token. Each C2 class is exercised through a rule that reaches
//! it:
//!
//! | Class | Reached through |
//! |---|---|
//! | `\s` | `header`'s `(\s+|$)` and `tail_header`'s `^(\s+#+)(\s*)$` |
//! | `(?i)` | `html_escape`'s alternation |
//! | `.` | not reachable at S1 — `image`, `link`, `inline_code`, `inline_math` are S2/S3 |
//! | `\w`, `\d` | not reachable at S1 — `emoji` and the autolinks are S2/S5 |
//!
//! The last two rows are the ones to extend when those stages land; the two
//! rules that use `\w` and `\d` are the emoji word boundary and the autolink
//! host, which is where C2 says the damage would be.

use mt_inline::{Token, tokenize};

fn types(src: &str) -> Vec<&'static str> {
    tokenize(src).iter().map(Token::type_str).collect()
}

/// The `\s` disagreement, at both S1 sites that use it.
///
/// The two entries that matter are the first two, and they point in opposite
/// directions:
///
/// - **U+0085 NEL** is in Rust's `\p{White_Space}` and *not* in JavaScript's
///   `\s`. A bare `\s` would make `#<NEL>x` a heading that muya reads as text.
/// - **U+FEFF BOM** is in JavaScript's `\s` and *not* in Rust's. A bare `\s`
///   would make `<BOM>###` plain text where muya emits a `tail_header` — and
///   a BOM is the first character of a great many real files.
#[test]
fn the_whitespace_class_agrees_with_muya() {
    const CASES: &[(&str, &[&str])] = &[
        // NEL: Rust White_Space has it, JavaScript \s does not
        ("#\u{85}x", &["text"]),
        // BOM: JavaScript \s has it, Rust White_Space does not
        ("#\u{feff}x", &["header", "text"]),
        // LINE SEPARATOR
        ("#\u{2028}x", &["header", "text"]),
        // PARAGRAPH SEPARATOR
        ("#\u{2029}x", &["header", "text"]),
        // NO-BREAK SPACE
        ("#\u{a0}x", &["header", "text"]),
        // IDEOGRAPHIC SPACE
        ("#\u{3000}x", &["header", "text"]),
        // VERTICAL TAB
        ("#\u{b}x", &["header", "text"]),
        // FORM FEED
        ("#\u{c}x", &["header", "text"]),
        // tail_header opener, NEL
        ("x\u{85}###", &["text"]),
        // tail_header opener, BOM
        ("x\u{feff}###", &["text", "tail_header"]),
        // tail_header opener, NBSP
        ("x\u{a0}###", &["text", "tail_header"]),
        // tail_header opener, IDEOGRAPHIC SPACE
        ("x\u{3000}###", &["text", "tail_header"]),
        // tail_header trailing group, NEL
        ("x ###\u{85}", &["text"]),
        // tail_header trailing group, BOM
        ("x ###\u{feff}", &["text", "tail_header", "text"]),
        // tail_header at offset 0
        ("\u{feff}###", &["tail_header"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// The `(?i)` disagreement, through `html_escape`.
///
/// muya builds that rule with the `i` flag, and JavaScript's Canonicalize
/// leaves a non-ASCII character alone when its uppercase form is ASCII. Rust's
/// `(?i)` does not: it folds with full Unicode rules, so `(?i)s` matches
/// U+017F (ſ) and `(?i)k` matches U+212A (K). `rules.rs` avoids the flag
/// entirely here and expands the fold a letter at a time.
#[test]
fn case_insensitive_matching_agrees_with_muya() {
    const CASES: &[(&str, &[&str])] = &[
        // html_escape, the i flag
        ("&AMP;", &["html_escape"]),
        ("&Amp;", &["html_escape"]),
        ("&SECT;", &["html_escape"]),
        // LONG S folds to `s` under Rust (?i), not under JavaScript i
        ("&nb\u{17f}p;", &["text"]),
        ("&\u{17f}ect;", &["text"]),
        // KELVIN SIGN folds to `k` under Rust (?i)
        ("\u{212a};", &["text"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// Two line-terminator cases that look like they should involve a class and
/// do not — which is the point.
///
/// `hard_line_break` is `^( {2,})(\n)(?!\n)`: literal spaces, not `\s`. So a
/// BOM or a NEL sitting between the spaces and the newline breaks the run and
/// leaves a *soft* break, whichever engine reads it. Recorded because the
/// obvious "fix" when porting is to generalise those spaces to whitespace.
#[test]
fn line_break_spaces_are_literal_and_agree_with_muya() {
    const CASES: &[(&str, &[&str])] = &[
        ("a  \u{feff}\nb", &["text", "soft_line_break", "text"]),
        ("a \u{85}\nb", &["text", "soft_line_break", "text"]),
        // …and the plain forms, for contrast.
        ("a  \nb", &["text", "hard_line_break", "text"]),
        ("a \nb", &["text", "soft_line_break", "text"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// Every input above also tiles. A class disagreement that produced the right
/// token types but the wrong extents would slip past the assertions above.
#[test]
fn every_class_case_still_tiles() {
    const INPUTS: &[&str] = &[
        "#\u{85}x",
        "#\u{feff}x",
        "#\u{2028}x",
        "#\u{2029}x",
        "#\u{a0}x",
        "#\u{3000}x",
        "#\u{b}x",
        "#\u{c}x",
        "x\u{85}###",
        "x\u{feff}###",
        "x\u{a0}###",
        "x\u{3000}###",
        "x ###\u{85}",
        "x ###\u{feff}",
        "\u{feff}###",
        "&AMP;",
        "&nb\u{17f}p;",
        "\u{212a};",
        "a\u{2028}b",
        "a\u{2029}b",
        "a  \u{feff}\nb",
        "a \u{85}\nb",
    ];

    for src in INPUTS {
        assert_eq!(
            &mt_inline::generator(src, &tokenize(src)),
            src,
            "input: {src:?}"
        );
    }
}
