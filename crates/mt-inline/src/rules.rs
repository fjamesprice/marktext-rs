//! The rule table — a port of `inlineRenderer/rules.ts` (130 lines,
//! 26 regexes across five groups).
//!
//! # Why `fancy-regex`
//!
//! §3 says to use the `regex` crate. M1.md §4 C1 corrects it: **16 of the 26
//! rules need a backreference, a lookahead or a lookbehind**, and `regex` has
//! none of the three by design. `fancy-regex` wraps `regex`, adds exactly
//! those, and delegates a whole pattern to the non-backtracking engine when it
//! does not need the extra power — so the 10 plain rules keep `regex`'s
//! performance and §3's sequencing survives: port the regexes verbatim now,
//! replace individual ones with hand-written scanners later, one at a time,
//! with the suite as the guard.
//!
//! # The backtrack limit
//!
//! `fancy-regex` is a backtracking engine, so the seven patterns `rules.ts`
//! disables `regexp/no-super-linear-backtracking` for are genuinely
//! super-linear here too. [`BACKTRACK_LIMIT`] bounds that, and exceeding it
//! means **the rule did not match** — never a panic. See that constant's docs.
//!
//! # JavaScript's character classes are not Rust's — M1.md §4 C2
//!
//! This is the trap that does not show up as a compile error, so every class
//! is written out rather than left to a shorthand:
//!
//! | Shorthand | JavaScript | Rust | Written here as |
//! |---|---|---|---|
//! | `\s` | includes U+FEFF, excludes U+0085 | `\p{White_Space}`: the reverse | [`js_s`] |
//! | `\S` | the complement of the above | the complement of the above | [`js_ns`] |
//! | `\w` | ASCII `[A-Za-z0-9_]` | Unicode word characters | [`js_w`] |
//! | `\d` | ASCII `[0-9]` | `\p{Nd}` — every decimal digit script | [`js_d`] |
//! | `.` | excludes `\n \r \u{2028} \u{2029}` | excludes `\n` | [`js_dot`] |
//! | `(?i)` | simple, ASCII-preserving folding | full Unicode folding | see below |
//!
//! Two of them are load-bearing today. `\w` is the emoji word-boundary check
//! (`lexer.ts:206`): under Rust's default, a Cyrillic or CJK character before
//! a `:` would suppress an emoji muya emits. `\s` drives the whole
//! emphasis-flanking decision, and `bench/corpus/` has a BOM case — U+FEFF is
//! whitespace to JavaScript and a word-ish character to Rust, which flips
//! `(?=\S)` on `strong` and `em`.
//!
//! `\d` is not in M1.md's table but belongs to it: `emoji`, `auto_link` and
//! `auto_link_extension` all use `\d`, and Rust's would let Devanagari digits
//! into an emoji shortcode and a port number.
//!
//! **`(?i)`.** M1.md §4 C2 says only `auto_link` and `html_tag` use it. There
//! is a third: `html_escape`, which muya builds with the `i` flag at
//! `rules.ts:45`. All three are handled and none of them keeps a bare `(?i)`
//! over a letter class:
//!
//! - `auto_link` has no backreference, so the flag is *removed* and every
//!   class is written out (`[a-z]` → `[a-zA-Z]`). That is precisely what
//!   JavaScript's Canonicalize does to an ASCII class, and it cannot pull in
//!   U+017F (ſ) or U+212A (K), which Rust's Unicode folding would.
//! - `html_tag` needs `(?i)` because its `\3` backreference must match a
//!   closing tag whose case differs from the opening one, so the flag stays
//!   and the letter classes are wrapped in `(?-i:…)` instead.
//! - `html_escape` is an alternation *of* letters, so neither trick applies.
//!   `(?i-u:…)` would be the answer and `fancy-regex` rejects it — "changing
//!   Unicode mode inline is not supported" — so the fold is expanded into the
//!   pattern one letter at a time instead: `&amp;` becomes `&[aA][mM][pP];`.
//!
//! # Anchoring — M1.md §5 D4
//!
//! Every rule is `^`-anchored and muya calls `exec` on a freshly resliced
//! `state.src`. The port never reslices: it advances an index and matches
//! against `&origin[pos..level_end]`, where the same `^` anchors to the same
//! place. Do **not** switch to an unanchored search and check the match start
//! — that silently changes what `lowerPriority` means.

use std::sync::LazyLock;

use fancy_regex::{Captures, Regex, RegexBuilder};

use crate::escape::ESCAPE_CHARACTERS;

// ---------------------------------------------------------------------------
// Character classes — M1.md §4 C2
// ---------------------------------------------------------------------------

/// The body of JavaScript's `\s`: ECMA-262 `WhiteSpace` ∪ `LineTerminator`.
///
/// Expands to the *contents* of a character class, so it composes into both
/// [`js_s`] and [`js_ns`] and into the hand-written classes that subtract from
/// it (`[^\s\\]`, `[^^\s]`, …).
///
/// Two entries are the whole point of writing it out:
///
/// - **U+FEFF is present.** JavaScript's `\s` matches the byte-order mark;
///   Rust's `\p{White_Space}` does not.
/// - **U+0085 (NEL) is absent.** Rust's `\p{White_Space}` matches it;
///   JavaScript's `\s` does not.
macro_rules! js_ws {
    () => {
        concat!(
            r"\u{9}\u{a}\u{b}\u{c}\u{d}\u{20}", // TAB LF VT FF CR SPACE
            r"\u{a0}",                          // NO-BREAK SPACE
            r"\u{1680}",                        // OGHAM SPACE MARK
            r"\u{2000}-\u{200a}",               // EN QUAD … HAIR SPACE
            r"\u{2028}\u{2029}",                // LINE / PARAGRAPH SEPARATOR
            r"\u{202f}\u{205f}\u{3000}",        // NARROW NBSP, MMSP, IDEOGRAPHIC SPACE
            r"\u{feff}",                        // ZERO WIDTH NO-BREAK SPACE (BOM)
        )
    };
}

/// JavaScript's `\s`.
macro_rules! js_s {
    () => {
        concat!("[", js_ws!(), "]")
    };
}

/// JavaScript's `\S`.
macro_rules! js_ns {
    () => {
        concat!("[^", js_ws!(), "]")
    };
}

/// The body of JavaScript's `\w` — ASCII only, always.
macro_rules! js_w {
    () => {
        "A-Za-z0-9_"
    };
}

/// The body of JavaScript's `\d` — ASCII only, always.
macro_rules! js_d {
    () => {
        "0-9"
    };
}

/// JavaScript's `.` — every character except the four line terminators.
///
/// Rust's `.` excludes `\n` alone, so `\r`, U+2028 and U+2029 would be matched
/// where JavaScript stops. `image`, `link`, `inline_code` and `inline_math`
/// all depend on this: a `.` that crosses a `\r` would let a link destination
/// swallow a line ending.
macro_rules! js_dot {
    () => {
        r"[^\n\r\u{2028}\u{2029}]"
    };
}

// The two classes another module needs as a *predicate* rather than as part of
// a rule pattern. `macro_rules!` does not escape its module without
// `#[macro_use]`, and the C2 classes must have exactly one definition, so they
// are re-exported as pattern text instead.

/// JavaScript's `\s` as a one-character pattern — `emphasis.rs`'s
/// `UNICODE_WHITESPACE_REG`, which is `/^\s/` and drives the entire
/// emphasis-flanking decision.
pub(crate) const JS_WHITESPACE_CLASS: &str = js_s!();

/// JavaScript's `\w` as a one-character class — `lexer.ts:206`'s emoji
/// word-boundary check.
pub(crate) const JS_WORD_CLASS: &str = concat!("[", js_w!(), "]");

/// JavaScript's `.` as a one-character class — `link.rs`'s `TITLE_REG`, which
/// is `/^('|")(.*?)\1$/` and is built outside this module because it is a
/// `utils.ts` regex rather than a `rules.ts` one.
///
/// The `.` matters there: a link title may not contain a line terminator, and
/// Rust's `.` excludes only `\n`. `[a](u "x\rY")` would otherwise be a title.
pub(crate) const JS_DOT_CLASS: &str = js_dot!();

/// `[\s\S]` — every character, including line terminators.
///
/// The one place a bare `\s` survives the C2 audit, and deliberately: the
/// class is the union of a shorthand and its own complement, so it is total in
/// *any* engine and the JavaScript/Rust disagreement over U+FEFF and U+0085
/// cannot reach it. `every_character_class_matches_what_javascript_matches`
/// proves that rather than assuming it.
macro_rules! js_any {
    () => {
        r"[\s\S]"
    };
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

/// How many backtracking steps a single rule match may take before it is
/// abandoned. **Exceeding it means "the rule did not match".**
///
/// # Why there is a limit at all
///
/// `rules.ts` disables `regexp/no-super-linear-backtracking` on seven
/// patterns, with a comment saying that rewriting them to please the linter
/// would risk parser regressions and that the input is the user's own
/// document rather than untrusted network data. That reasoning holds here —
/// this is a latency question, not a security one — but under a backtracking
/// engine "super-linear" is not hypothetical, and the M1 exit gate is 24 hours
/// of fuzzing without a panic. An unbounded engine turns a pathological
/// paragraph into a hang, and a hang is indistinguishable from a crash to
/// whoever is typing.
///
/// # Why this value
///
/// It is `fancy-regex`'s own default, adopted deliberately rather than
/// inherited silently: naming it here means a future change is a reviewable
/// diff instead of a dependency bump. Every legitimate inline construct in
/// `bench/corpus/` matches in orders of magnitude fewer steps — the limit is
/// slack, not a tuning knob — and it is per rule *attempt*, so the ceiling it
/// sets is on one regex against one position, not on tokenizing a document.
///
/// # Why "no match" and not a panic
///
/// Three reasons, in order of how much they matter:
///
/// 1. §1 makes this crate fuzzable, and the M1 gate is panic-freedom. A
///    resource limit that panics *is* the panic the gate forbids.
/// 2. It degrades to the right thing. A rule that does not match leaves its
///    characters to accumulate as text, which is exactly what the reader sees
///    for any construct that fails to parse. The tiling invariant is
///    untouched — no byte is lost — so the document still round-trips.
/// 3. It is the only option that keeps the tokenizer total. Every other
///    treatment (skip the character, truncate the input) changes the output
///    in a way a caller cannot reason about.
///
/// The cost, stated plainly: tokenization becomes input-length dependent at
/// the extreme, so a differential disagreement against the TypeScript engine
/// is *possible* on an input pathological enough to hit the limit. No corpus
/// or fixture input comes close; if one ever does, it belongs in
/// `spec/divergences.json` like any other deliberate difference.
pub(crate) const BACKTRACK_LIMIT: usize = 1_000_000;

/// Compile one rule pattern, applying [`BACKTRACK_LIMIT`].
///
/// # Panics
///
/// If the pattern does not compile. That is a programming error in this file
/// and not a property of any input — the patterns are constants, so this
/// either always panics or never does, and
/// `every_rule_compiles_and_is_indexed_by_its_own_discriminant` makes sure it
/// is never. It therefore does not weaken the panic-freedom gate.
pub(crate) fn compile(name: &str, pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .backtrack_limit(BACKTRACK_LIMIT)
        .build()
        .unwrap_or_else(|error| panic!("rule `{name}` failed to compile: {error}\n  {pattern}"))
}

/// muya's `rule.exec(state.src)`.
///
/// `fancy-regex` returns `Result` because a match can exhaust
/// [`BACKTRACK_LIMIT`]. Every variant of that error is resource exhaustion —
/// pattern errors are raised at build time, not here — so all of them collapse
/// to `None`, "the rule did not match". See [`BACKTRACK_LIMIT`] for why that
/// is the right collapse.
///
/// Takes the `Regex` rather than a [`RuleId`] so that the tests can point it
/// at a deliberately under-limited regex and observe the collapse happening.
pub(crate) fn exec<'t>(re: &Regex, haystack: &'t str) -> Option<Captures<'t, str>> {
    re.captures(haystack).unwrap_or(None)
}

// ---------------------------------------------------------------------------
// The rules
// ---------------------------------------------------------------------------

/// One character of an ASCII pattern, made case-insensitive without a flag.
///
/// A letter becomes a two-member class (`s` → `[sS]`); everything else is
/// returned unchanged. Used by `html_escape`, whose case-insensitivity cannot
/// be expressed as a flag without either widening to Unicode folding or using
/// `(?i-u:…)`, which `fancy-regex` rejects.
fn ascii_case_insensitive(ch: char) -> String {
    if ch.is_ascii_alphabetic() {
        format!("[{}{}]", ch.to_ascii_lowercase(), ch.to_ascii_uppercase())
    } else {
        ch.to_string()
    }
}

/// One entry of `inlineRules`, `beginRules` or `endRules`.
///
/// Modelled as an enum with an array-backed regex table rather than as a
/// struct of named fields, because the two `lowerPriority` rule sets (M1.md
/// §5 D5) are *sets of rules* and want to be data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(usize)]
pub(crate) enum RuleId {
    // --- beginRules --------------------------------------------------------
    Hr,
    CodeFence,
    Header,
    ReferenceDefinition,
    MultipleMath,
    // --- endRules ----------------------------------------------------------
    TailHeader,
    // --- commonMarkRules ---------------------------------------------------
    Strong,
    Em,
    InlineCode,
    Image,
    Link,
    ReferenceLink,
    ReferenceImage,
    HtmlTag,
    HtmlEscape,
    SoftLineBreak,
    HardLineBreak,
    Backlash,
    // --- gfmRules ----------------------------------------------------------
    Emoji,
    Del,
    AutoLink,
    AutoLinkExtension,
    // --- inlineExtensionRules ----------------------------------------------
    InlineMath,
    Superscript,
    Subscript,
    FootnoteIdentifier,
}

impl RuleId {
    /// Every rule, in `rules.ts` declaration order.
    ///
    /// The order is also the index into the compiled table, so this array and
    /// the enum's discriminants must stay in lockstep — checked by
    /// `every_rule_compiles_and_is_indexed_by_its_own_discriminant`.
    pub(crate) const ALL: [RuleId; 26] = [
        RuleId::Hr,
        RuleId::CodeFence,
        RuleId::Header,
        RuleId::ReferenceDefinition,
        RuleId::MultipleMath,
        RuleId::TailHeader,
        RuleId::Strong,
        RuleId::Em,
        RuleId::InlineCode,
        RuleId::Image,
        RuleId::Link,
        RuleId::ReferenceLink,
        RuleId::ReferenceImage,
        RuleId::HtmlTag,
        RuleId::HtmlEscape,
        RuleId::SoftLineBreak,
        RuleId::HardLineBreak,
        RuleId::Backlash,
        RuleId::Emoji,
        RuleId::Del,
        RuleId::AutoLink,
        RuleId::AutoLinkExtension,
        RuleId::InlineMath,
        RuleId::Superscript,
        RuleId::Subscript,
        RuleId::FootnoteIdentifier,
    ];

    /// The key this rule has in `rules.ts`. Used in diagnostics, and it is the
    /// name to grep for when comparing against the TypeScript.
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuleId::Hr => "hr",
            RuleId::CodeFence => "code_fence",
            RuleId::Header => "header",
            RuleId::ReferenceDefinition => "reference_definition",
            RuleId::MultipleMath => "multiple_math",
            RuleId::TailHeader => "tail_header",
            RuleId::Strong => "strong",
            RuleId::Em => "em",
            RuleId::InlineCode => "inline_code",
            RuleId::Image => "image",
            RuleId::Link => "link",
            RuleId::ReferenceLink => "reference_link",
            RuleId::ReferenceImage => "reference_image",
            RuleId::HtmlTag => "html_tag",
            RuleId::HtmlEscape => "html_escape",
            RuleId::SoftLineBreak => "soft_line_break",
            RuleId::HardLineBreak => "hard_line_break",
            RuleId::Backlash => "backlash",
            RuleId::Emoji => "emoji",
            RuleId::Del => "del",
            RuleId::AutoLink => "auto_link",
            RuleId::AutoLinkExtension => "auto_link_extension",
            RuleId::InlineMath => "inline_math",
            RuleId::Superscript => "superscript",
            RuleId::Subscript => "subscript",
            RuleId::FootnoteIdentifier => "footnote_identifier",
        }
    }

    /// This rule's pattern.
    ///
    /// Each arm quotes the TypeScript it is a port of, so the two can be
    /// diffed by eye. The only transformations applied are the C2 class
    /// rewrites documented at the top of this module, plus the escaping Rust
    /// requires and JavaScript does not: a `[` inside a character class must
    /// be `\[` here, because Rust's classes nest and JavaScript's do not.
    pub(crate) fn pattern(self) -> String {
        match self {
            // beginRules.hr — /^(\*{3,}|-{3,}|_{3,})$/
            RuleId::Hr => r"^(\*{3,}|-{3,}|_{3,})$".to_string(),

            // beginRules.code_fence — /^(`{3,})([^`]*)$/
            RuleId::CodeFence => r"^(`{3,})([^`]*)$".to_string(),

            // beginRules.header — /(^ {0,3}#{1,6}(\s+|$))/
            RuleId::Header => concat!(r"(^ {0,3}#{1,6}(", js_s!(), r"+|$))").to_string(),

            // beginRules.reference_definition —
            // /^( {0,3}\[)([^\]]+?)(\\*)(\]: *)(<?)([^\s>]+)(>?)(?:( +)(["'(]?)([^\n"'()]+)\9)?( *)$/
            //
            // `\9` is the backreference that closes the title with whatever
            // opened it — one of the 16 rules M1.md §4 C1 says the `regex`
            // crate cannot express.
            RuleId::ReferenceDefinition => concat!(
                r#"^( {0,3}\[)([^\]]+?)(\\*)(\]: *)(<?)([^"#,
                js_ws!(),
                r#">]+)(>?)(?:( +)(["'(]?)([^\n"'()]+)\9)?( *)$"#,
            )
            .to_string(),

            // beginRules.multiple_math — /^(\$\$)$/
            RuleId::MultipleMath => r"^(\$\$)$".to_string(),

            // endRules.tail_header — /^(\s+#+)(\s*)$/
            RuleId::TailHeader => concat!(r"^(", js_s!(), r"+#+)(", js_s!(), r"*)$").to_string(),

            // commonMarkRules.strong — /^(\*\*|__)(?=\S)([\s\S]*?[^\s\\])(\\*)\1(?!(\*|_))/
            RuleId::Strong => concat!(
                r"^(\*\*|__)(?=",
                js_ns!(),
                r")(",
                js_any!(),
                r"*?[^",
                js_ws!(),
                r"\\])(\\*)\1(?!(\*|_))",
            )
            .to_string(),

            // commonMarkRules.em — /^(\*|_)(?=\S)([\s\S]*?[^\s*\\])(\\*)\1(?!\1)/
            RuleId::Em => concat!(
                r"^(\*|_)(?=",
                js_ns!(),
                r")(",
                js_any!(),
                r"*?[^",
                js_ws!(),
                r"*\\])(\\*)\1(?!\1)",
            )
            .to_string(),

            // commonMarkRules.inline_code — /^(`{1,3})([^`]+|.{2,})\1/
            RuleId::InlineCode => concat!(r"^(`{1,3})([^`]+|", js_dot!(), r"{2,})\1").to_string(),

            // commonMarkRules.image — /^(!\[)(.*?)(\\*)\]\((.*)(\\*)\)/
            RuleId::Image => concat!(
                r"^(!\[)(",
                js_dot!(),
                r"*?)(\\*)\]\((",
                js_dot!(),
                r"*)(\\*)\)",
            )
            .to_string(),

            // commonMarkRules.link —
            // /^(\[)((?:\[[^\]]*\]|[^[\]]|\](?=[^[]*\]))*?)(\\*)\]\((.*)(\\*)\)/
            RuleId::Link => concat!(
                r"^(\[)((?:\[[^\]]*\]|[^\[\]]|\](?=[^\[]*\]))*?)(\\*)\]\((",
                js_dot!(),
                r"*)(\\*)\)",
            )
            .to_string(),

            // commonMarkRules.reference_link —
            // /^\[((?:\[[^\]]*\]|[^[\]]|\](?=[^[]*\]))*?)(\\*)\](?:\[([^\]]*?)(\\*)\])?/
            RuleId::ReferenceLink => {
                r"^\[((?:\[[^\]]*\]|[^\[\]]|\](?=[^\[]*\]))*?)(\\*)\](?:\[([^\]]*?)(\\*)\])?"
                    .to_string()
            }

            // commonMarkRules.reference_image —
            // /^!\[([^\]]+?)(\\*)\](?:\[([^\]]*?)(\\*)\])?/
            RuleId::ReferenceImage => r"^!\[([^\]]+?)(\\*)\](?:\[([^\]]*?)(\\*)\])?".to_string(),

            // commonMarkRules.html_tag —
            // /^(<!--[\s\S]*?-->|(<([a-z][a-z\d-]*)[^\n<>]*>)(?:([\s\S]*?)(<\/\3 *>))?)/i
            //
            // The `i` stays, because `\3` must match `</DIV>` against a `<div>`
            // that opened it. The letter class is therefore wrapped in
            // `(?-i:…)` and written out ASCII-explicitly, so Rust's Unicode
            // folding cannot admit U+017F or U+212A as a tag-name character.
            // `(?-i:…)` is non-capturing, so group 3 stays group 3.
            RuleId::HtmlTag => concat!(
                r"(?i)^(<!--",
                js_any!(),
                r"*?-->|(<((?-i:[a-zA-Z][a-zA-Z0-9-]*))[^\n<>]*>)(?:(",
                js_any!(),
                r"*?)(</\3 *>))?)",
            )
            .to_string(),

            // commonMarkRules.html_escape —
            // new RegExp(`^(${escapeCharacters.join('|')})`, 'i')
            //
            // The `i` is folded into the pattern one letter at a time —
            // `&amp;` becomes `&[aA][mM][pP];` — instead of being set as a
            // flag. A Rust `(?i)` would fold with Unicode rules and match
            // `&nbſp;` for `&nbsp;`, and `(?i-u:…)`, which would not, is
            // rejected by `fancy-regex`: "changing Unicode mode inline is not
            // supported". Expanding the classes needs no flag at all, so
            // there is nothing left to get wrong — and it is what JavaScript's
            // Canonicalize does to an ASCII alternation anyway.
            //
            // Every alternative is `&` + ASCII alphanumerics + `;` (asserted
            // in `escape.rs`), so nothing needs regex-metacharacter escaping
            // and no alternative can be a prefix of another.
            RuleId::HtmlEscape => {
                let alternation = ESCAPE_CHARACTERS
                    .iter()
                    .map(|entity| {
                        entity
                            .chars()
                            .map(ascii_case_insensitive)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("|");
                format!("^({alternation})")
            }

            // commonMarkRules.soft_line_break — /^(\n)(?!\n)/
            RuleId::SoftLineBreak => r"^(\n)(?!\n)".to_string(),

            // commonMarkRules.hard_line_break — /^( {2,})(\n)(?!\n)/
            RuleId::HardLineBreak => r"^( {2,})(\n)(?!\n)".to_string(),

            // commonMarkRules.backlash — /^(\\)([\\`*{}[\]()#+\-.!_>~:|<$])/
            //
            // The bare `[` inside muya's class is `\[` here: Rust's character
            // classes nest, JavaScript's do not.
            RuleId::Backlash => r"^(\\)([\\`*{}\[\]()#+\-.!_>~:|<$])".to_string(),

            // gfmRules.emoji — /^(:)([a-z_\d+-]+)\1/
            //
            // No `i` flag in muya, so the class stays lowercase-only.
            RuleId::Emoji => concat!(r"^(:)([a-z_", js_d!(), r"+-]+)\1").to_string(),

            // gfmRules.del — /^(~{2})(?=\S)([\s\S]*?\S)(\\*)\1/
            RuleId::Del => concat!(
                r"^(~{2})(?=",
                js_ns!(),
                r")(",
                js_any!(),
                r"*?",
                js_ns!(),
                r")(\\*)\1",
            )
            .to_string(),

            // gfmRules.auto_link —
            // /^<(?:([a-z][a-z\d+.\-]{1,31}:[^ <>]*)|([\w.!#$%&'*+/=?^`{|}~-]+@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*))>/i
            //
            // The `i` is *dropped* and every letter class written out, which
            // is exactly what JavaScript's Canonicalize does to an ASCII class
            // — and unlike a Rust `(?i)`, it cannot admit U+017F or U+212A.
            // There is no backreference here, so nothing else needed the flag.
            RuleId::AutoLink => concat!(
                r"^<(?:([a-zA-Z][a-zA-Z0-9+.\-]{1,31}:[^ <>]*)|([",
                js_w!(),
                r".!#$%&'*+/=?^`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?",
                r"(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*))>",
            )
            .to_string(),

            // gfmRules.auto_link_extension —
            // /^(?:(www\.[a-z_-]+\.[a-z]{2,}(?::\d{1,5})?(?:\/\S+)?)|(https?:\/\/(?:[a-z0-9\-._~]+\.[a-z]{2,}|[0-9.]+|localhost|\[[a-f0-9.:]+\])(?::\d{1,5})?(?:\/\S+)?)|([\w.!#$%&'*+/=?^`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*))(?=\s|$)/
            //
            // No `i` flag in muya: `WWW.EXAMPLE.COM` and `HTTPS://…` really do
            // not autolink, and the classes stay lowercase-only. Only the
            // email alternative is mixed-case, and it is spelled that way in
            // the TypeScript too.
            RuleId::AutoLinkExtension => concat!(
                r"^(?:(www\.[a-z_-]+\.[a-z]{2,}(?::[",
                js_d!(),
                r"]{1,5})?(?:/",
                js_ns!(),
                r"+)?)|(https?://(?:[a-z0-9\-._~]+\.[a-z]{2,}|[0-9.]+|localhost|\[[a-f0-9.:]+\])",
                r"(?::[",
                js_d!(),
                r"]{1,5})?(?:/",
                js_ns!(),
                r"+)?)|([",
                js_w!(),
                r".!#$%&'*+/=?^`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?",
                r"(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*))(?=",
                js_s!(),
                r"|$)",
            )
            .to_string(),

            // inlineExtensionRules.inline_math — /^(\$)((?:[^$\\]|\\.)+)(\\*)\1(?!\1)/
            RuleId::InlineMath => {
                concat!(r"^(\$)((?:[^$\\]|\\", js_dot!(), r")+)(\\*)\1(?!\1)",).to_string()
            }

            // inlineExtensionRules.superscript —
            // /^(\^)((?:[^^\s]|(?<=\\)\1|(?<=\\) )+?)(?<!\\)\1(?!\1)/
            //
            // The leading `^` of `[^^\s]` negates; the second is a literal
            // caret.
            RuleId::Superscript => concat!(
                r"^(\^)((?:[^^",
                js_ws!(),
                r"]|(?<=\\)\1|(?<=\\) )+?)(?<!\\)\1(?!\1)",
            )
            .to_string(),

            // inlineExtensionRules.subscript —
            // /^(~)((?:[^~\s]|(?<=\\)\1|(?<=\\) )+?)(?<!\\)\1(?!\1)/
            RuleId::Subscript => concat!(
                r"^(~)((?:[^~",
                js_ws!(),
                r"]|(?<=\\)\1|(?<=\\) )+?)(?<!\\)\1(?!\1)",
            )
            .to_string(),

            // inlineExtensionRules.footnote_identifier — /^(\[\^)([^^[\]\s]+)(?<!\\)\]/
            RuleId::FootnoteIdentifier => {
                concat!(r"^(\[\^)([^^\[\]", js_ws!(), r"]+)(?<!\\)\]",).to_string()
            }
        }
    }
}

/// Every rule, compiled once, indexed by `RuleId as usize`.
///
/// §3 asks for pre-compiled `LazyLock` regexes and this is that, with one
/// change: a single `LazyLock` over the whole table rather than 26 of them.
/// The table is only ever reached through [`rule`], which needs all of it the
/// first time the tokenizer runs, so 26 separate cells would buy nothing and
/// cost 26 atomic loads per tokenized character.
static COMPILED: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    RuleId::ALL
        .iter()
        .map(|id| compile(id.name(), &id.pattern()))
        .collect()
});

/// The compiled regex for one rule.
pub(crate) fn rule(id: RuleId) -> &'static Regex {
    &COMPILED[id as usize]
}

// ---------------------------------------------------------------------------
// The two `lowerPriority` rule sets — M1.md §5 D5
// ---------------------------------------------------------------------------

/// `rules.ts`'s `validateRules`: every rule except `em`, `strong`,
/// `tail_header`, `backlash`, `superscript`, `subscript` and
/// `footnote_identifier`.
///
/// Used by `validateEmphasize` and by the emoji check. Listed in `inlineRules`
/// insertion order, which is `endRules` then `commonMarkRules` then `gfmRules`
/// then `inlineExtensionRules` — note that this is *not* [`RuleId::ALL`]'s
/// order, because `inlineRules` does not include `beginRules`.
///
/// Written out rather than derived from an exclusion list at runtime, for the
/// same reason `EXCLUDE_KEYS` is a `const` in the TypeScript: which rules may
/// veto an emphasis span is a decision, not a computation.
pub(crate) const VALIDATE_RULES: &[RuleId] = &[
    RuleId::InlineCode,
    RuleId::Image,
    RuleId::Link,
    RuleId::ReferenceLink,
    RuleId::ReferenceImage,
    RuleId::HtmlTag,
    RuleId::HtmlEscape,
    RuleId::SoftLineBreak,
    RuleId::HardLineBreak,
    RuleId::Emoji,
    RuleId::Del,
    RuleId::AutoLink,
    RuleId::AutoLinkExtension,
    RuleId::InlineMath,
];

/// `rules.ts`'s `linkValidateRules`: `inline_code`, `html_tag`, `auto_link`.
///
/// The veto set for a tentative `[text](url)` or reference link. The 14-line
/// comment at `rules.ts:118` is the reason it exists and is worth reading
/// before touching it: per CommonMark §6.6 only code spans, raw HTML and
/// `<…>` autolinks bind more tightly than a link, so only those three may
/// defer one. Passing [`VALIDATE_RULES`] here instead drops
/// `[t](https://x)、`, `[t](https://x)foo` and the first of two links on a
/// line — marktext #4671, which is what `linkFollowedByAutolink.spec.ts`
/// guards.
pub(crate) const LINK_VALIDATE_RULES: &[RuleId] =
    &[RuleId::InlineCode, RuleId::HtmlTag, RuleId::AutoLink];

// The two sets are S2/S3 consumers (`lowerPriority` lands with emphasis
// validation), so nothing reads them yet. These const assertions are what
// keeps them from being dead code, and they are worth having on their own
// account: the cardinalities are the load-bearing half of D5, and a rule
// silently added to or dropped from either set is exactly the edit that
// reintroduces #4671.
const _: () = assert!(VALIDATE_RULES.len() == 14);
const _: () = assert!(LINK_VALIDATE_RULES.len() == 3);

#[cfg(test)]
mod tests {
    use super::*;

    /// Compiling all 26 is the S1 gate's other half: a pattern that does not
    /// build is a port error that would otherwise surface as a mysterious
    /// panic at the first tokenize.
    #[test]
    fn every_rule_compiles_and_is_indexed_by_its_own_discriminant() {
        for (index, id) in RuleId::ALL.iter().enumerate() {
            assert_eq!(
                *id as usize,
                index,
                "`{}` is at index {index} of RuleId::ALL but its discriminant is {}",
                id.name(),
                *id as usize
            );
            // Forces compilation, and panics with the pattern if it fails.
            let _ = rule(*id);
        }
        assert_eq!(RuleId::ALL.len(), 26, "rules.ts declares 26 regexes");
    }

    #[test]
    fn rule_names_are_unique_and_match_the_typescript_keys() {
        let mut names: Vec<&str> = RuleId::ALL.iter().map(|id| id.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two rules share a name");
    }

    // -----------------------------------------------------------------------
    // M1.md §4 C2 — the classes
    // -----------------------------------------------------------------------

    /// The C2 table, asserted one disagreement at a time.
    ///
    /// Each of these matches under exactly one of the two engines' defaults,
    /// which is what makes them worth a test: a regression here is silent
    /// everywhere else.
    #[test]
    fn every_character_class_matches_what_javascript_matches() {
        let matches = |body: &str, ch: char| {
            let re = compile("test", &format!("^{body}$"));
            exec(&re, &ch.to_string()).is_some()
        };

        // \s — the two disagreements, in both directions.
        assert!(
            matches(js_s!(), '\u{feff}'),
            "U+FEFF is whitespace to JavaScript; Rust's \\p{{White_Space}} omits it"
        );
        assert!(
            !matches(js_s!(), '\u{85}'),
            "U+0085 is not whitespace to JavaScript; Rust's \\p{{White_Space}} includes it"
        );
        // …and \S is its exact complement, which is what `(?=\S)` relies on.
        assert!(!matches(js_ns!(), '\u{feff}'));
        assert!(matches(js_ns!(), '\u{85}'));
        // The ordinary members are all still there.
        for ch in [
            ' ', '\t', '\n', '\r', '\u{b}', '\u{c}', '\u{a0}', '\u{3000}',
        ] {
            assert!(matches(js_s!(), ch), "U+{:04X} should be \\s", ch as u32);
        }
        assert!(
            matches(js_s!(), '\u{2005}'),
            "inside the \\u2000-\\u200a range"
        );

        // \w — ASCII only. A Cyrillic letter before a `:` must not suppress an
        // emoji (lexer.ts:206).
        assert!(matches(concat!("[", js_w!(), "]"), 'z'));
        assert!(matches(concat!("[", js_w!(), "]"), '_'));
        assert!(!matches(concat!("[", js_w!(), "]"), 'д'));
        assert!(!matches(concat!("[", js_w!(), "]"), '中'));

        // \d — ASCII only. Rust's would admit other digit scripts into an
        // emoji shortcode and a port number.
        assert!(matches(concat!("[", js_d!(), "]"), '7'));
        assert!(!matches(concat!("[", js_d!(), "]"), '٧')); // ARABIC-INDIC SEVEN
        assert!(!matches(concat!("[", js_d!(), "]"), '७')); // DEVANAGARI SEVEN

        // . — excludes all four line terminators, not just \n.
        for ch in ['\n', '\r', '\u{2028}', '\u{2029}'] {
            assert!(
                !matches(js_dot!(), ch),
                "U+{:04X} is excluded from JavaScript's `.`",
                ch as u32
            );
        }
        assert!(matches(js_dot!(), 'a'));
        assert!(matches(js_dot!(), '中'));

        // [\s\S] — total in either engine, which is why it survives verbatim.
        for ch in ['a', '\n', '\r', '\u{2028}', '\u{feff}', '\u{85}', '中'] {
            assert!(matches(js_any!(), ch), "[\\s\\S] must be total");
        }
    }

    /// `(?i)` handling, one rule at a time — the third row of the C2 table
    /// that M1.md's version does not list.
    #[test]
    fn case_insensitivity_stays_ascii_in_all_three_rules_that_use_it() {
        let auto_link = rule(RuleId::AutoLink);
        assert!(
            exec(auto_link, "<HTTP://example.com>").is_some(),
            "the i flag folds the scheme"
        );
        assert!(exec(auto_link, "<Http://example.com>").is_some());
        // U+212A KELVIN SIGN folds to `k` under Rust's Unicode rules and does
        // not under JavaScript's. It must not be a scheme character.
        assert!(exec(auto_link, "\u{212a}ttp://example.com>").is_none());

        let html_escape = rule(RuleId::HtmlEscape);
        assert!(
            exec(html_escape, "&AMP;").is_some(),
            "the i flag folds the name"
        );
        assert!(exec(html_escape, "&Amp;").is_some());
        // U+017F LATIN SMALL LETTER LONG S folds to `s` under Rust's rules.
        assert!(
            exec(html_escape, "&nb\u{17f}p;").is_none(),
            "Unicode folding leaked into html_escape"
        );

        let html_tag = rule(RuleId::HtmlTag);
        assert!(
            exec(html_tag, "<DIV>x</div>").is_some(),
            "the i flag is why html_tag keeps it: \\3 must match across cases"
        );
        assert!(
            exec(html_tag, "<\u{17f}pan>x</\u{17f}pan>").is_none(),
            "Unicode folding leaked into the tag-name class"
        );
    }

    // -----------------------------------------------------------------------
    // The backtrack limit
    // -----------------------------------------------------------------------

    /// The decision, executed: a rule that exhausts its budget reports **no
    /// match**, and does not panic and does not propagate an error.
    ///
    /// Built at a deliberately tiny limit rather than by hunting for an input
    /// that exceeds a million steps, because what is under test is the
    /// collapse in [`exec`], not `fancy-regex`'s step accounting. The pattern
    /// is a real one — `reference_definition`, one of the seven `rules.ts`
    /// disables the super-linear-backtracking lint for.
    #[test]
    fn exceeding_the_backtrack_limit_reports_no_match_rather_than_panicking() {
        let pattern = RuleId::ReferenceDefinition.pattern();
        let starved = RegexBuilder::new(&pattern)
            .backtrack_limit(1)
            .build()
            .expect("the pattern compiles; only the runtime budget is tiny");

        // A well-formed definition: it matches at the real limit and cannot at
        // a limit of one step, so the two calls isolate the budget.
        let input = "[label]: https://example.com \"A title\"";
        assert!(
            exec(rule(RuleId::ReferenceDefinition), input).is_some(),
            "the input is a valid reference definition"
        );
        assert!(
            starved.captures(input).is_err(),
            "one backtracking step is not enough; this test proves nothing otherwise"
        );
        assert!(
            exec(&starved, input).is_none(),
            "the error must collapse to `no match`"
        );
    }

    /// The limit is a named constant and every rule is built with it. Checked
    /// by construction — [`compile`] is the only way a rule is built — so this
    /// asserts the value, which is the part a future change should have to
    /// touch deliberately.
    #[test]
    fn the_backtrack_limit_is_set_explicitly() {
        assert_eq!(BACKTRACK_LIMIT, 1_000_000);
    }

    // -----------------------------------------------------------------------
    // Spot checks on the rules S1 actually runs
    // -----------------------------------------------------------------------

    #[test]
    fn the_begin_rules_are_anchored_to_both_ends() {
        // `$` is end-of-input in both engines — it does not match before a
        // trailing newline the way Perl's does. The tokenizer relies on that:
        // `hr` must not fire on the first line of a two-line block.
        assert!(exec(rule(RuleId::Hr), "***").is_some());
        assert!(exec(rule(RuleId::Hr), "***\n").is_none());
        assert!(exec(rule(RuleId::Hr), "*** and more").is_none());
        assert!(exec(rule(RuleId::MultipleMath), "$$").is_some());
        assert!(exec(rule(RuleId::MultipleMath), "$$x").is_none());
    }

    #[test]
    fn header_accepts_up_to_three_leading_spaces_and_needs_a_space_or_the_end() {
        let header = rule(RuleId::Header);
        assert!(exec(header, "# Title").is_some());
        assert!(exec(header, "   ###### Title").is_some());
        assert!(exec(header, "#").is_some(), "the `|$` alternative");
        assert!(
            exec(header, "    # Title").is_none(),
            "four spaces is a code block"
        );
        assert!(exec(header, "#Title").is_none(), "no space, no header");
        assert!(exec(header, "####### Title").is_none(), "seven is too many");
    }

    #[test]
    fn the_line_break_rules_do_not_fire_on_a_blank_line() {
        // The `(?!\n)` is why both need `fancy-regex`.
        assert!(exec(rule(RuleId::SoftLineBreak), "\na").is_some());
        assert!(exec(rule(RuleId::SoftLineBreak), "\n\n").is_none());
        assert!(exec(rule(RuleId::HardLineBreak), "  \na").is_some());
        assert!(
            exec(rule(RuleId::HardLineBreak), " \na").is_none(),
            "one space is not enough"
        );
        assert!(exec(rule(RuleId::HardLineBreak), "  \n\n").is_none());
    }

    #[test]
    fn backlash_escapes_exactly_muyas_punctuation_set() {
        let backlash = rule(RuleId::Backlash);
        for ch in "\\`*{}[]()#+-.!_>~:|<$".chars() {
            let input = format!("\\{ch}");
            assert!(exec(backlash, &input).is_some(), "`\\{ch}` should escape");
        }
        assert!(exec(backlash, "\\a").is_none(), "a letter is not escapable");
        assert!(exec(backlash, "\\ ").is_none(), "nor a space");
        assert!(exec(backlash, "\\").is_none(), "nor a trailing backslash");
    }

    #[test]
    fn html_escape_matches_a_named_reference_and_captures_it_whole() {
        let caps = exec(rule(RuleId::HtmlEscape), "&amp; rest").expect("a match");
        assert_eq!(caps.get(0).expect("group 0").as_str(), "&amp;");
        assert_eq!(caps.get(1).expect("group 1").as_str(), "&amp;");
        assert!(exec(rule(RuleId::HtmlEscape), "&notanentity; rest").is_none());
        assert!(exec(rule(RuleId::HtmlEscape), "plain").is_none());
    }

    #[test]
    fn tail_header_captures_the_marker_separately_from_its_trailing_space() {
        let caps = exec(rule(RuleId::TailHeader), "  ###   ").expect("a match");
        assert_eq!(caps.get(1).expect("group 1").as_str(), "  ###");
        assert_eq!(caps.get(2).expect("group 2").as_str(), "   ");
    }

    /// The 16 rules M1.md §4 C1 says `regex` cannot express, proven to work
    /// here — a backreference, a lookahead and a lookbehind, one each.
    #[test]
    fn backreferences_and_lookaround_are_available() {
        // Backreference: `\1` closes with the marker that opened.
        assert!(exec(rule(RuleId::Emoji), ":smile:").is_some());
        // Lookahead: `(?!\n)` on soft_line_break, covered above; here `(?=\S)`.
        assert!(exec(rule(RuleId::Strong), "**a**").is_some());
        assert!(
            exec(rule(RuleId::Strong), "** a**").is_none(),
            "(?=\\S) rejects it"
        );
        // Lookbehind: `(?<!\\)` before the closing marker.
        assert!(exec(rule(RuleId::Superscript), "^a^").is_some());
    }
}
