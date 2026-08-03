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
//! | `\s` | `header`'s `(\s+\|$)` and `tail_header`'s `^(\s+#+)(\s*)$` |
//! | `(?i)` | `html_escape`'s alternation |
//! | `.` | **S2** — `inline_code`'s `.{2,}` and `inline_math`'s `\\.` |
//! | `\w` | **S2** — the emoji word boundary at `lexer.ts:206` |
//! | `\d` | **S2** — `emoji`'s content class `[a-z_\d+-]` |
//! | `\s` (again) | **S2** — `UNICODE_WHITESPACE_REG`, the flanking decision |
//! | `\w` (again) | **S5** — `auto_link`'s email local part |
//! | `\d` (again) | **S5** — `auto_link`'s scheme and the extension's port |
//! | `(?i)` (again) | **S5** — `auto_link`, whose flag S1 *removed* |
//!
//! S1's version of this table said three rows were unreachable and named this
//! file as the place to extend when they became reachable. S2 was that stage
//! for `emoji`, `inline_code` and `inline_math`; **S5 is the last of them**,
//! and it closes the note S2 left — *"the remaining unreached user of `\w` and
//! `\d` is the autolink host"*. Every class in M1.md §4 C2's table is now
//! exercised by at least one rule that produces a token, so the file no longer
//! has a row waiting on a stage.
//!
//! Every S2 row below was measured the same way as the S1 rows — muya's
//! `tokenizer` run over the input at the pinned reference commit, with the
//! `type` of each token recorded. The S2 sweep covered 207 inputs; 194 agreed
//! exactly on type, `raw`, every field and `range`, six differ only because
//! muya reaches an S3–S5 rule the port has not written, and the remaining
//! seven are the registered `emoji-nested-boundary` divergence. The S5 rows
//! come from a 49,751-input sweep in which 6885 inputs produce an autolink and
//! all 6885 agree exactly.

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

// ===========================================================================
// S2 — the three classes S1 could not reach
// ===========================================================================

/// `\w`, through the emoji word boundary (`lexer.ts:206`).
///
/// JavaScript's `\w` is `[A-Za-z0-9_]`, always. Rust's is Unicode-aware, so
/// under a bare `\w` every one of the six non-ASCII rows below would suppress
/// an emoji that muya emits — and five of them are ordinary prose characters.
/// This is the row M1.md §4 C2 calls out by name.
#[test]
fn the_word_class_agrees_with_muya_at_the_emoji_boundary() {
    const CASES: &[(&str, &[&str])] = &[
        // ASCII word characters suppress the emoji. This is #1677 itself.
        ("12:00-14:00", &["text"]),
        ("hello:smile:", &["text"]),
        ("_:smile:", &["text"]),
        // Non-word ASCII does not.
        ("-:smile:", &["text", "emoji"]),
        (".:smile:", &["text", "emoji"]),
        ("(:smile:)", &["text", "emoji", "text"]),
        // …and neither does anything non-ASCII, because JavaScript's \w is
        // ASCII-only. Rust's default would swallow every one of these.
        ("д:smile:", &["text", "emoji"]),         // Cyrillic
        ("буква:smile:", &["text", "emoji"]),     // a whole Cyrillic word
        ("中:smile:", &["text", "emoji"]),        // CJK
        ("é:smile:", &["text", "emoji"]),         // Latin-1 letter
        ("ÿ:smile:", &["text", "emoji"]),         // Latin-1 letter
        ("\u{1f642}:smile:", &["text", "emoji"]), // an astral emoji
        ("。:smile:", &["text", "emoji"]),        // CJK full stop
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// `\d`, through `emoji`'s content class `[a-z_\d+-]`.
///
/// JavaScript's `\d` is `[0-9]`. Rust's is `\p{Nd}` — every decimal-digit
/// script — so a bare `\d` would admit Arabic-Indic and Devanagari digits into
/// a shortcode that muya rejects.
#[test]
fn the_digit_class_agrees_with_muya_inside_a_shortcode() {
    const CASES: &[(&str, &[&str])] = &[
        // ASCII digits are shortcode characters.
        (":1:", &["emoji"]),
        (":a1:", &["emoji"]),
        (":a7:", &["emoji"]),
        // Other decimal-digit scripts are not.
        (":\u{664}:", &["text"]),  // ARABIC-INDIC FOUR
        (":a\u{664}:", &["text"]), // …not even after an ASCII letter
        (":a\u{96d}:", &["text"]), // DEVANAGARI SEVEN
        // …and the class is `[a-z…]`, so a non-ASCII letter is out too.
        (":д:", &["text"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// `.`, through `inline_code`'s `.{2,}` and `inline_math`'s `\\.`.
///
/// JavaScript's `.` excludes **all four** line terminators — `\n`, `\r`,
/// U+2028 and U+2029. Rust's excludes `\n` alone, so under a bare `.` every
/// `\r`/U+2028/U+2029 row below would produce a *longer* code span or a math
/// span where muya produces none.
///
/// The extents matter as much as the types here, so this checks `raw` rather
/// than the type sequence alone: `` ``x`\ry`` `` is `inline_code("``x`")` in
/// muya — a **one**-backtick marker, reached by backtracking `` (`{1,3}) `` —
/// and would be the whole eight-character span if `.` crossed the `\r`.
#[test]
fn the_dot_class_agrees_with_muya_at_every_line_terminator() {
    const CASES: &[(&str, &[&str], &[&str])] = &[
        // inline_code, through `.{2,}`. An ordinary character is crossed…
        ("``x`zy``", &["inline_code"], &["``x`zy``"]),
        // …and each of the three terminators JavaScript excludes is not.
        ("``x`\ry``", &["inline_code", "text"], &["``x`", "\ry``"]),
        (
            "``x`\u{2028}y``",
            &["inline_code", "text"],
            &["``x`", "\u{2028}y``"],
        ),
        (
            "``x`\u{2029}y``",
            &["inline_code", "text"],
            &["``x`", "\u{2029}y``"],
        ),
        // `\n` is excluded by both engines, so this row is the control: it
        // would look the same however `.` were written.
        (
            "``x`\ny``",
            &["inline_code", "soft_line_break", "text"],
            &["``x`", "\n", "y``"],
        ),
        // inline_math, through `\\.`. An ordinary character is crossed…
        ("$a\\.b$", &["inline_math"], &["$a\\.b$"]),
        // …and a terminator is not, so the whole expression stays text.
        ("$a\\\rb$", &["text"], &["$a\\\rb$"]),
        ("$a\\\u{2028}b$", &["text"], &["$a\\\u{2028}b$"]),
        ("$a\\\u{2029}b$", &["text"], &["$a\\\u{2029}b$"]),
        (
            "$a\\\nb$",
            &["text", "soft_line_break", "text"],
            &["$a\\", "\n", "b$"],
        ),
    ];

    for (src, expected_types, expected_raws) in CASES {
        assert_eq!(&types(src), expected_types, "input: {src:?}");
        let raws: Vec<&str> = tokenize(src).iter().map(|t| t.raw.of(src)).collect();
        assert_eq!(&raws, expected_raws, "input: {src:?}");
    }
}

/// `\s` again, now that it drives the emphasis-flanking decision as well as
/// `header` and `tail_header`.
///
/// `UNICODE_WHITESPACE_REG` is `/^\s/` and `canOpenEmphasis` tests it against
/// the character after an opener. U+FEFF is whitespace to JavaScript and not
/// to Rust, so a bare `\p{White_Space}` would *open* an emphasis span muya
/// refuses; U+0085 is whitespace to Rust and not to JavaScript, so it would
/// *refuse* one muya opens. The corpus has a BOM case, which is what makes the
/// first of those a real input rather than a hypothetical.
#[test]
fn the_whitespace_class_agrees_with_muya_in_the_flanking_decision() {
    const CASES: &[(&str, &[&str])] = &[
        // U+00A0 — whitespace to both. CommonMark example 353.
        ("*\u{a0}a\u{a0}*", &["text"]),
        // U+FEFF — whitespace to JavaScript only. `**` is followed by a BOM,
        // so `(?=\S)` fails and there is no strong span.
        ("**\u{feff}a\u{feff}**", &["text"]),
        // U+0085 — whitespace to Rust only. muya opens the span; a
        // `\p{White_Space}` port would not.
        ("**\u{85}a\u{85}**", &["strong"]),
        // The plain forms, for contrast.
        ("** a **", &["text"]),
        ("**a**", &["strong"]),
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
        // S1
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
        // S2 — \w
        "12:00-14:00",
        "hello:smile:",
        "_:smile:",
        "-:smile:",
        "д:smile:",
        "буква:smile:",
        "中:smile:",
        "\u{1f642}:smile:",
        "。:smile:",
        // S2 — \d
        ":1:",
        ":\u{664}:",
        ":a\u{96d}:",
        ":д:",
        // S2 — .
        "``x`zy``",
        "``x`\ry``",
        "``x`\u{2028}y``",
        "``x`\u{2029}y``",
        "``x`\ny``",
        "$a\\.b$",
        "$a\\\rb$",
        "$a\\\u{2028}b$",
        "$a\\\u{2029}b$",
        "$a\\\nb$",
        // S2 — \s in the flanking decision
        "*\u{a0}a\u{a0}*",
        "**\u{feff}a\u{feff}**",
        "**\u{85}a\u{85}**",
        "** a **",
        "**a**",
        // S5 — \d, \w and (?i) at the autolink host
        "<a1://example.com>",
        "<a\u{664}://example.com>",
        "<a\u{96d}://example.com>",
        "www.example.com:80 end",
        "www.example.com:808080 end",
        "www.example.com:\u{668}\u{660} end",
        "<foo@example.com>",
        "<\u{434}foo@example.com>",
        "<foo\u{434}@example.com>",
        "<\u{4e2d}@example.com>",
        "<HTTP://example.com>",
        "<a\u{17f}://example.com>",
        "<a\u{212a}://example.com>",
        "WWW.example.com end",
        "https://EXAMPLE.com end",
    ];

    for src in INPUTS {
        assert_eq!(
            &mt_inline::generator(src, &tokenize(src)),
            src,
            "input: {src:?}"
        );
    }
}

// ===========================================================================
// S5 — the autolink host, which is the last unreached user of `\w` and `\d`
// ===========================================================================

/// `\d`, through `auto_link`'s scheme class and the extension rule's port.
///
/// muya's `auto_link` is `[a-z][a-z\d+.\-]{1,31}:` and its extension rule has
/// `(?::\d{1,5})?`. JavaScript's `\d` is `[0-9]`; Rust's is `\p{Nd}`, so a bare
/// `\d` would admit an Arabic-Indic or Devanagari digit into a URI scheme and
/// into a port number — and both would then autolink something muya leaves
/// alone.
///
/// The scheme rows land on `html_tag` rather than on `text` when they fail,
/// because `html_tag` matches `<a…>` happily; that is the measured answer and
/// it is the shape S4's shadowing note is about, read from the other side.
#[test]
fn the_digit_class_agrees_with_muya_in_a_scheme_and_a_port() {
    const CASES: &[(&str, &[&str])] = &[
        // auto_link's scheme: an ASCII digit is a scheme character…
        ("<a1://example.com>", &["auto_link"]),
        ("<http2://example.com>", &["auto_link"]),
        // …and no other decimal-digit script is.
        ("<a\u{664}://example.com>", &["html_tag"]), // ARABIC-INDIC FOUR
        ("<a\u{96d}://example.com>", &["html_tag"]), // DEVANAGARI SEVEN
        // The extension rule's port: `(?::\d{1,5})?`.
        ("www.example.com:80 end", &["auto_link_extension", "text"]),
        (
            "www.example.com:80808 end",
            &["auto_link_extension", "text"],
        ),
        // Six digits is one too many, so the optional group cannot match and
        // the lookahead then fails against the `:`.
        ("www.example.com:808080 end", &["text"]),
        ("www.example.com:\u{668}\u{660} end", &["text"]),
        (
            "https://127.0.0.1:8080/a end",
            &["auto_link_extension", "text"],
        ),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// `\w`, through `auto_link`'s email local part
/// `[\w.!#$%&'*+/=?^`{|}~-]+@`.
///
/// JavaScript's `\w` is ASCII, so a Cyrillic letter breaks the local part and
/// the whole autolink fails; under Rust's Unicode-aware default every row
/// below would become an `auto_link` that muya leaves as text. This is the
/// same class as the emoji boundary above, at the only other rule that uses
/// `\w`.
#[test]
fn the_word_class_agrees_with_muya_in_an_email_local_part() {
    const CASES: &[(&str, &[&str])] = &[
        // ASCII local parts autolink, including the class's punctuation.
        ("<foo@example.com>", &["auto_link"]),
        ("<foo_bar@example.com>", &["auto_link"]),
        ("<foo+special@Bar.baz-bar0.com>", &["auto_link"]),
        // Non-ASCII does not, in either position of the local part.
        ("<\u{434}foo@example.com>", &["text"]), // Cyrillic, leading
        ("<\u{4e2d}@example.com>", &["text"]),   // CJK
        ("<\u{e9}@example.com>", &["text"]),     // Latin-1 letter
        // …and `html_tag` is what catches the interior case instead, because
        // its tag name is the ASCII prefix and `[^\n<>]*` eats the rest. The
        // leading cases are `text` only because a tag name must *start*
        // `[a-zA-Z]`. Measured; it is the same shadowing shape S4 recorded,
        // read from the other side.
        ("<foo\u{434}@example.com>", &["html_tag"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// `(?i)` a third time, through the rule whose flag S1 **removed**.
///
/// M1.md §4 C2: `auto_link` has no backreference, so instead of keeping the
/// flag its classes are written out (`[a-z]` → `[a-zA-Z]`). That is what
/// JavaScript's Canonicalize does to an ASCII class anyway, and unlike a Rust
/// `(?i)` it cannot admit U+017F (ſ) or U+212A (K) as a scheme character. Both
/// halves are checked here: the fold still works, and it does not reach past
/// ASCII.
#[test]
fn auto_links_case_insensitivity_agrees_with_muya_without_the_flag() {
    const CASES: &[(&str, &[&str])] = &[
        ("<HTTP://example.com>", &["auto_link"]),
        ("<Http://example.com>", &["auto_link"]),
        ("<MAILTO:FOO@BAR.BAZ>", &["auto_link"]),
        // …and the two characters Rust's Unicode folding would let in.
        ("<a\u{17f}://example.com>", &["html_tag"]), // LATIN SMALL LETTER LONG S
        ("<a\u{212a}://example.com>", &["html_tag"]), // KELVIN SIGN
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// The extension rule has **no** `i` flag in muya, so unlike `auto_link` its
/// classes really are lowercase-only. `WWW.EXAMPLE.COM` does not autolink,
/// which is easy to "fix" by mistake when porting the two rules side by side.
#[test]
fn the_extension_rule_is_case_sensitive_and_muya_agrees() {
    const CASES: &[(&str, &[&str])] = &[
        ("www.example.com end", &["auto_link_extension", "text"]),
        ("https://example.com end", &["auto_link_extension", "text"]),
        ("WWW.example.com end", &["text"]),
        ("HTTPS://example.com end", &["text"]),
        ("Www.example.com end", &["text"]),
        // The host is lowercase-only too, but only in the `www` alternative's
        // label class — the `https?://` alternative's host is `[a-z0-9\-._~]`.
        ("www.EXAMPLE.com end", &["text"]),
        ("https://EXAMPLE.com end", &["text"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

// ===========================================================================
// S2 — the CJK flanking cases the M1.md §6 gate names
// ===========================================================================

/// `bench/corpus/cjk.md`'s flanking cases, tokenized identically to the
/// TypeScript.
///
/// M1.md §6's S2 gate: *"CJK flanking cases in `bench/corpus/cjk.md` tokenize
/// identically to TypeScript."* `round_trip.rs` already runs the whole file
/// for the tiling property; what that cannot check is *which* tokens come out,
/// because tiling holds just as well when a `**粗体**` stays text. These are
/// the file's flanking lines, with the token types muya produced for each.
///
/// The widening is `CJK_REG` in `emphasis.rs` and it is deliberately
/// non-standard — CommonMark §6.2 counts only whitespace and punctuation as
/// boundaries, CJK ideographs are `Lo` and so are neither, and a literal
/// reading denies emphasis to nearly every CJK paragraph. muya widens it,
/// Typora/markdownlint/Joplin widen it, and marktext/marktext#4307 tracks it.
#[test]
fn the_cjk_flanking_lines_of_the_corpus_agree_with_muya() {
    const CASES: &[(&str, &[&str])] = &[
        // The file's headline case: `**` glued to CJK on both sides, no space.
        (
            "中文**粗体**紧邻中文字符，没有空格——这是 CJK flanking 的关键用例。",
            &["text", "strong", "text"],
        ),
        (
            "这是一段中文文本，包含**粗体**和*斜体*，以及一个 `代码片段`。",
            &[
                "text",
                "strong",
                "text",
                "em",
                "text",
                "inline_code",
                "text",
            ],
        ),
        (
            "日本語の文章です。**太字**と*斜体*、そして`コード`を含みます。",
            &[
                "text",
                "strong",
                "text",
                "em",
                "text",
                "inline_code",
                "text",
            ],
        ),
        (
            "한국어 문장입니다. **굵게**와 *기울임*, 그리고 `코드`를 포함합니다.",
            &[
                "text",
                "strong",
                "text",
                "em",
                "text",
                "inline_code",
                "text",
            ],
        ),
        // `_` needs a boundary on both sides, and CJK provides one.
        ("中文__粗体__紧邻", &["text", "strong", "text"]),
        // A CJK quotation mark is punctuation, so this would work without the
        // widening — kept because it is the case the utils.ts comment names.
        ("中文“**加粗**”中文", &["text", "strong", "text"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}

/// The widening is **additive**, which is the load-bearing half of the
/// `CJK_REG` comment: CJK is only ever an extra way to *accept* a boundary,
/// never a way to reject something CommonMark accepts. So a Latin input with
/// the same shape is unchanged — `a__b__c` stays text — and every CJK block
/// the regex covers behaves alike.
#[test]
fn the_cjk_widening_only_ever_adds_a_boundary() {
    const CASES: &[(&str, &[&str])] = &[
        // Latin intra-word `_`: refused, exactly as CommonMark says.
        ("a__b__c", &["text"]),
        // One representative from each range of CJK_REG, at both edges.
        ("\u{3040}**a**\u{30ff}", &["text", "strong", "text"]), // Kana
        ("\u{3400}**a**\u{4dbf}", &["text", "strong", "text"]), // Ext A
        ("\u{4e00}**a**\u{9fff}", &["text", "strong", "text"]), // Unified
        ("\u{f900}**a**\u{faff}", &["text", "strong", "text"]), // Compatibility
        ("\u{ac00}**a**\u{d7af}", &["text", "strong", "text"]), // Hangul
        ("\u{ff66}**a**\u{ff9d}", &["text", "strong", "text"]), // Halfwidth kana
        // Plane 2, which the regex covers in full even though its own comment
        // claims to stop at U+2A6DF. Port the regex, not the comment.
        ("\u{20000}**a**\u{20000}", &["text", "strong", "text"]),
        ("\u{2a6e0}**a**\u{2a6e0}", &["text", "strong", "text"]),
        ("\u{2ffff}**a**\u{2ffff}", &["text", "strong", "text"]),
        // …and one code point past the end of plane 2, which is not CJK. `**`
        // has no intra-word rule so the span still forms; `__` is the probe.
        ("\u{30000}__a__\u{30000}", &["text"]),
        ("\u{20000}__a__\u{20000}", &["text", "strong", "text"]),
    ];

    for (src, expected) in CASES {
        assert_eq!(&types(src), expected, "input: {src:?}");
    }
}
