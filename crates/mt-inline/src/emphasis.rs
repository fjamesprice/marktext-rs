//! `validateEmphasize` and its supporting cast — the emphasis half of
//! `inlineRenderer/utils.ts`.
//!
//! # What is here, and what is not
//!
//! `utils.ts` is 360 lines and splits cleanly in two. This is the half that
//! decides whether a `*`/`_` run may open or close an emphasis span:
//! `PUNCTUATION_REG`, `CJK_REG`, `UNICODE_WHITESPACE_REG`, `canOpenEmphasis`,
//! `canCloseEmphasis`, `validateEmphasize` and `lowerPriority`, plus
//! `isLengthEven` from `utils/index.ts`. The rest of `utils.ts` is
//! [`crate::link`] — `parseSrcAndTitle` and `correctUrl`, landed in S3 — and
//! [`crate::html`] — `getAttributes`, `validWidthAndHeight` and
//! `WHITELIST_ATTRIBUTES`, which are M1.md §5 D2 and landed in S4.
//!
//! Two functions of `utils.ts` are **deliberately not ported at all**:
//! `lastCodePointChar` and `codePointCharAt` (lines 104–130). M1.md §5 D1 is
//! explicit about why — they exist to reassemble surrogate pairs that
//! JavaScript's `charAt` splits, and Rust's `char` iteration gives that for
//! free. Their *callers* are ported faithfully, including the four
//! empty-string defaults documented on [`can_open_emphasis`] and
//! [`can_close_emphasis`], which are the part of those call sites that
//! actually carries behaviour.
//!
//! # The character classes are measurements, not transcriptions
//!
//! [`PUNCTUATION_CLASS`] and [`CJK_CLASS`] were produced by running muya's own
//! regexes over every scalar value from U+0000 to U+10FFFF and recording which
//! ones matched, then writing the resulting ranges out. That matters because
//! both tables spell their non-BMP members as **surrogate-pair alternatives** —
//! `\uD800[\uDD00-\uDD02\uDF9F\uDFD0]` and friends — which is a JavaScript
//! encoding artefact and not a range in any sense Rust shares. Transcribing
//! those by eye is exactly the kind of silent, untestable error §3 rule 3 says
//! to defend against by measuring.
//!
//! Two things the measurement settled that reading would not have:
//!
//! - **U+005C REVERSE SOLIDUS is not in `PUNCTUATION_REG`.** The commented-out
//!   `punctuation` array at `utils.ts:5` lists it and CommonMark §2.1 counts it
//!   as ASCII punctuation, but the live regex runs `…@[\]^_`…`, which is `[`
//!   then an escaped `]`, with no `\\` anywhere. A backslash is therefore not a
//!   flanking boundary. Faithful, and surprising enough to be worth a test.
//! - **`CJK_REG`'s non-BMP branch is wider than its own comment.** The comment
//!   says *"U+20000–U+2A6DF CJK Unified Ideographs Extension B"*, but
//!   `[\uD840-\uD87F][\uDC00-\uDFFF]` is the whole of **U+20000–U+2FFFF** —
//!   Extensions B through G and the unassigned space between them. The regex is
//!   what runs, so the regex is what is ported.
//!
//! # `\s` is load-bearing here — M1.md §4 C2
//!
//! `UNICODE_WHITESPACE_REG` is `/^\s/`, and it drives the whole flanking
//! decision: it is tested against the character before an opener and after a
//! closer, six times across the two functions. C2's disagreement is therefore
//! at its most expensive right here, so this module takes the class from
//! [`rules::JS_WHITESPACE_CLASS`] rather than writing `\p{White_Space}` — the
//! two differ on U+FEFF (JavaScript: whitespace) and U+0085 (Rust:
//! whitespace), in opposite directions, and `bench/corpus/` has a BOM case.
//!
//! # Cost
//!
//! Every predicate here is a compiled regex run against a single character,
//! which is not how one would write this from scratch — a sorted range table
//! and a binary search would be a fraction of the cost. It is written this way
//! because muya writes it this way, and §3's sequencing is *port verbatim
//! first, hand-optimize later with the suite as the guard*. [`lower_priority`]
//! is the bigger number anyway: M1.md §5 D5 records it as O(offset × |rules|)
//! regex executions and asks for a benchmark before M1 closes.

use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::rules::{self, RuleId, compile, exec, rule};
use crate::token::Span;

// ---------------------------------------------------------------------------
// The character classes
// ---------------------------------------------------------------------------

/// `PUNCTUATION_REG` (`utils.ts:7`), as the body of a Rust character class.
///
/// Laid out in the order `utils.ts` lays it out, so the two can be diffed by
/// eye a line at a time. Three mechanical changes and nothing else:
///
/// - a literal `[` is `\[`, because Rust's character classes nest and
///   JavaScript's do not;
/// - `\xA1` and `;` become `\u{a1}` and `\u{37e}`;
/// - the nine surrogate-pair alternatives at the end become the scalar values
///   they encode. Each keeps its JavaScript source in a comment beside it.
///
/// Note what is *absent*: `\u{5c}`, REVERSE SOLIDUS. See the module docs.
const PUNCTUATION_CLASS: &str = concat!(
    // ASCII, verbatim from the TypeScript.
    r##"!"#$%&'()*+,\-./:;<=>?@\[\]^_`{|}~"##,
    // Latin-1 and Greek.
    r"\u{a1}\u{a7}\u{ab}\u{b6}\u{b7}\u{bb}\u{bf}\u{37e}\u{387}",
    // Armenian, Hebrew.
    r"\u{55a}-\u{55f}\u{589}\u{58a}\u{5be}\u{5c0}\u{5c3}\u{5c6}\u{5f3}\u{5f4}",
    // Arabic, Syriac, N'Ko.
    r"\u{609}\u{60a}\u{60c}\u{60d}\u{61b}\u{61e}\u{61f}\u{66a}-\u{66d}\u{6d4}",
    r"\u{700}-\u{70d}\u{7f7}-\u{7f9}\u{830}-\u{83e}\u{85e}",
    // Devanagari, Gujarati, Sinhala, Thai, Tibetan.
    r"\u{964}\u{965}\u{970}\u{af0}\u{df4}\u{e4f}\u{e5a}\u{e5b}",
    r"\u{f04}-\u{f12}\u{f14}\u{f3a}-\u{f3d}\u{f85}\u{fd0}-\u{fd4}\u{fd9}\u{fda}",
    // Myanmar, Georgian, Ethiopic, UCAS, Ogham, Runic.
    r"\u{104a}-\u{104f}\u{10fb}\u{1360}-\u{1368}\u{1400}\u{166d}\u{166e}",
    r"\u{169b}\u{169c}\u{16eb}-\u{16ed}",
    // Buginese, Khmer, Mongolian, Limbu.
    r"\u{1735}\u{1736}\u{17d4}-\u{17d6}\u{17d8}-\u{17da}\u{1800}-\u{180a}",
    r"\u{1944}\u{1945}\u{1a1e}\u{1a1f}\u{1aa0}-\u{1aa6}\u{1aa8}-\u{1aad}",
    // Balinese, Batak, Lepcha, Ol Chiki, Vedic.
    r"\u{1b5a}-\u{1b60}\u{1bfc}-\u{1bff}\u{1c3b}-\u{1c3f}\u{1c7e}\u{1c7f}",
    r"\u{1cc0}-\u{1cc7}\u{1cd3}",
    // General Punctuation and the mathematical brackets.
    r"\u{2010}-\u{2027}\u{2030}-\u{2043}\u{2045}-\u{2051}\u{2053}-\u{205e}",
    r"\u{207d}\u{207e}\u{208d}\u{208e}\u{2308}-\u{230b}\u{2329}\u{232a}",
    r"\u{2768}-\u{2775}\u{27c5}\u{27c6}\u{27e6}-\u{27ef}\u{2983}-\u{2998}",
    r"\u{29d8}-\u{29db}\u{29fc}\u{29fd}\u{2cf9}-\u{2cfc}\u{2cfe}\u{2cff}\u{2d70}",
    r"\u{2e00}-\u{2e2e}\u{2e30}-\u{2e42}",
    // CJK Symbols and Punctuation.
    r"\u{3001}-\u{3003}\u{3008}-\u{3011}\u{3014}-\u{301f}\u{3030}\u{303d}",
    r"\u{30a0}\u{30fb}",
    // Yijing, Vai, Cyrillic Extended-B, Bamum, Syloti Nagri, Phags-pa,
    // Saurashtra, Devanagari Extended, Kayah Li, Rejang, Javanese, Cham,
    // Tai Viet, Meetei Mayek.
    r"\u{a4fe}\u{a4ff}\u{a60d}-\u{a60f}\u{a673}\u{a67e}\u{a6f2}-\u{a6f7}",
    r"\u{a874}-\u{a877}\u{a8ce}\u{a8cf}\u{a8f8}-\u{a8fa}\u{a8fc}\u{a92e}\u{a92f}",
    r"\u{a95f}\u{a9c1}-\u{a9cd}\u{a9de}\u{a9df}\u{aa5c}-\u{aa5f}\u{aade}\u{aadf}",
    r"\u{aaf0}\u{aaf1}\u{abeb}",
    // Arabic Presentation Forms, CJK Compatibility Forms, Small Form Variants,
    // Halfwidth and Fullwidth Forms.
    r"\u{fd3e}\u{fd3f}\u{fe10}-\u{fe19}\u{fe30}-\u{fe52}\u{fe54}-\u{fe61}",
    r"\u{fe63}\u{fe68}\u{fe6a}\u{fe6b}\u{ff01}-\u{ff03}\u{ff05}-\u{ff0a}",
    r"\u{ff0c}-\u{ff0f}\u{ff1a}\u{ff1b}\u{ff1f}\u{ff20}\u{ff3b}-\u{ff3d}\u{ff3f}",
    r"\u{ff5b}\u{ff5d}\u{ff5f}-\u{ff65}",
    // --- the nine surrogate-pair alternatives, decoded --------------------
    // \uD800[\uDD00-\uDD02\uDF9F\uDFD0]  Aegean, Ugaritic, Old Persian
    r"\u{10100}-\u{10102}\u{1039f}\u{103d0}",
    // 𐕯                       Caucasian Albanian citation mark
    r"\u{1056f}",
    // \uD802[\uDC57\uDD1F\uDD3F\uDE50-\uDE58\uDE7F\uDEF0-\uDEF6\uDF39-\uDF3F\uDF99-\uDF9C]
    r"\u{10857}\u{1091f}\u{1093f}\u{10a50}-\u{10a58}\u{10a7f}\u{10af0}-\u{10af6}",
    r"\u{10b39}-\u{10b3f}\u{10b99}-\u{10b9c}",
    // \uD804[\uDC47-\uDC4D\uDCBB\uDCBC\uDCBE-\uDCC1\uDD40-\uDD43\uDD74\uDD75
    //        \uDDC5-\uDDC9\uDDCD\uDDDB\uDDDD-\uDDDF\uDE38-\uDE3D\uDEA9]
    r"\u{11047}-\u{1104d}\u{110bb}\u{110bc}\u{110be}-\u{110c1}\u{11140}-\u{11143}",
    r"\u{11174}\u{11175}\u{111c5}-\u{111c9}\u{111cd}\u{111db}\u{111dd}-\u{111df}",
    r"\u{11238}-\u{1123d}\u{112a9}",
    // \uD805[\uDCC6\uDDC1-\uDDD7\uDE41-\uDE43\uDF3C-\uDF3E]
    r"\u{114c6}\u{115c1}-\u{115d7}\u{11641}-\u{11643}\u{1173c}-\u{1173e}",
    // \uD809[\uDC70-\uDC74]              Cuneiform punctuation
    r"\u{12470}-\u{12474}",
    // \uD81A[\uDE6E\uDE6F\uDEF5\uDF37-\uDF3B\uDF44]  Mro, Bassa Vah, Pahawh
    r"\u{16a6e}\u{16a6f}\u{16af5}\u{16b37}-\u{16b3b}\u{16b44}",
    // 𛲟                       Duployan full stop
    r"\u{1bc9f}",
    // \uD836[\uDE87-\uDE8B]              SignWriting punctuation
    r"\u{1da87}-\u{1da8b}",
);

/// `CJK_REG` (`utils.ts:97`), as the body of a Rust character class.
///
/// The ranges are in `utils.ts`'s order, which is not code-point order —
/// U+F900 precedes U+AC00 there — because keeping it makes the two diffable.
///
/// The last range is the one to read twice. `utils.ts` writes it as
/// `[\uD840-\uD87F][\uDC00-\uDFFF]` and comments it as
/// *"U+20000–U+2A6DF CJK Unified Ideographs Extension B"*. Those disagree: the
/// surrogate pairs cover **U+20000–U+2FFFF**, which is Extension B and
/// everything after it up to the end of plane 2. The regex is what runs.
///
/// The whole table is a **deliberate non-standard extension**, and `utils.ts`
/// carries a 30-line comment saying so: CommonMark §6.2 counts only Unicode
/// whitespace and punctuation as flanking boundaries, CJK ideographs are Lo and
/// so are neither, and a literal reading therefore denies emphasis to almost
/// any CJK paragraph, because CJK does not put spaces between words. Typora,
/// markdownlint and Joplin all widen the check the same way. The widening is
/// **additive** — CJK is only ever an extra way to *accept*, never a way to
/// reject something CommonMark accepts — so Latin inputs are untouched.
/// Upstream tracking: marktext/marktext#4307.
const CJK_CLASS: &str = concat!(
    r"\u{3040}-\u{30ff}",   // Hiragana + Katakana
    r"\u{3400}-\u{4dbf}",   // CJK Unified Ideographs Extension A
    r"\u{4e00}-\u{9fff}",   // CJK Unified Ideographs
    r"\u{f900}-\u{faff}",   // CJK Compatibility Ideographs
    r"\u{ac00}-\u{d7af}",   // Hangul Syllables
    r"\u{ff66}-\u{ff9d}",   // Halfwidth Katakana
    r"\u{20000}-\u{2ffff}", // [\uD840-\uD87F][\uDC00-\uDFFF] — plane 2, all of it
);

/// `UNICODE_WHITESPACE_REG` (`utils.ts:66`) — `/^\s/`, with JavaScript's `\s`.
static UNICODE_WHITESPACE_REG: LazyLock<Regex> =
    LazyLock::new(|| compile("UNICODE_WHITESPACE_REG", rules::JS_WHITESPACE_CLASS));

static PUNCTUATION_REG: LazyLock<Regex> =
    LazyLock::new(|| compile("PUNCTUATION_REG", &format!("[{PUNCTUATION_CLASS}]")));

static CJK_REG: LazyLock<Regex> = LazyLock::new(|| compile("CJK_REG", &format!("[{CJK_CLASS}]")));

/// `/\w/` at `lexer.ts:206` — the emoji word-boundary check.
///
/// Not a `utils.ts` table, but the same shape of test as the three above and
/// the same C2 trap, so it shares their machinery rather than growing a second
/// copy of it. ASCII only, always: under Rust's Unicode-aware `\w` a Cyrillic
/// or CJK character before a `:` would suppress an emoji that muya emits.
static WORD_REG: LazyLock<Regex> = LazyLock::new(|| compile("\\w", rules::JS_WORD_CLASS));

/// Whether `ch` matches `class_`.
///
/// **`None` matches nothing**, which is precisely JavaScript's `RegExp.test('')`
/// for all four classes — and reproducing that is the whole reason the callers
/// below thread `Option<char>` around instead of substituting a placeholder
/// character. See [`can_open_emphasis`] for the four sites where it matters.
///
/// The class patterns are all `^`-anchored or single-character, and `ch` is
/// exactly one scalar value, so anchoring is immaterial; `encode_utf8` writes
/// into a stack buffer, so there is no allocation per test.
fn matches(class_: &Regex, ch: Option<char>) -> bool {
    let Some(ch) = ch else { return false };
    let mut buf = [0u8; 4];
    exec(class_, ch.encode_utf8(&mut buf)).is_some()
}

/// `UNICODE_WHITESPACE_REG.test(ch)`.
///
/// `pub(crate)` for one caller outside this module: [`crate::link`]'s
/// `js_trim`, which needs the same class as a predicate because
/// `parseSrcAndTitle` calls `String.prototype.trim` three times and
/// **JavaScript's `trim` and Rust's `str::trim` disagree** — on U+FEFF and
/// U+0085, in opposite directions, exactly as `\s` does. Sharing this keeps
/// C2's "one definition of the class" intact, and the two halves of `utils.ts`
/// sharing a character class is what the TypeScript does too.
pub(crate) fn is_unicode_whitespace(ch: Option<char>) -> bool {
    matches(&UNICODE_WHITESPACE_REG, ch)
}

/// `PUNCTUATION_REG.test(ch)`.
fn is_punctuation(ch: Option<char>) -> bool {
    matches(&PUNCTUATION_REG, ch)
}

/// `CJK_REG.test(ch)`.
fn is_cjk(ch: Option<char>) -> bool {
    matches(&CJK_REG, ch)
}

/// `/\w/.test(ch)` — JavaScript's `\w`, i.e. `[A-Za-z0-9_]`.
pub(crate) fn is_word_character(ch: Option<char>) -> bool {
    matches(&WORD_REG, ch)
}

/// The three-way flanking boundary test both `canOpen` and `canClose` apply:
/// *"Unicode whitespace, or punctuation, or — the non-standard widening — CJK"*.
///
/// Written once because it appears four times in `utils.ts`, twice per
/// function, and the four copies must not be allowed to drift apart.
fn is_flanking_boundary(ch: Option<char>) -> bool {
    is_unicode_whitespace(ch) || is_punctuation(ch) || is_cjk(ch)
}

// ---------------------------------------------------------------------------
// isLengthEven
// ---------------------------------------------------------------------------

/// `isLengthEven` (`utils/index.ts:48`) — `(str = '') => str.length % 2 === 0`.
///
/// The gate on every backslash run that precedes a closing marker: an odd run
/// means the marker itself is escaped, so the construct does not close there.
///
/// # `None` is `true`, and must stay `true`
///
/// muya calls this as `isLengthEven(to[3])` for all four of `tryChunks`'
/// rules, but **only `inline_math` and `del` have a third capture group** —
/// `inline_code` is `/^(`{1,3})([^`]+|.{2,})\1/` and `emoji` is
/// `/^(:)([a-z_\d+-]+)\1/`, both two groups. For those two the argument is
/// `undefined`, the default parameter substitutes `''`, and the gate is
/// vacuously true at every call.
///
/// That is not a bug to tidy up on the way past. Making it fallible for those
/// two rules would suppress code spans and emoji that muya emits, and nothing
/// downstream reads a backslash run that does not exist. `token.rs` records the
/// same thing on [`crate::CodeEmojiMath::backlash`], which is `Option` for
/// exactly this reason.
///
/// A backslash run is all `\`, which is ASCII, so a byte length and a UTF-16
/// length agree and the parity is the same number either way.
pub(crate) fn is_length_even(run: Option<Span>) -> bool {
    run.map_or(0, |span| span.len()) % 2 == 0
}

// ---------------------------------------------------------------------------
// lowerPriority
// ---------------------------------------------------------------------------

/// `lowerPriority` (`utils.ts:140`).
///
/// CommonMark §6.4 rule 17: *"inline code spans, links, images and HTML tags
/// group more tightly than emphasis"*. So before accepting an emphasis span of
/// length `offset`, walk every position inside it and ask whether some
/// higher-priority rule starts there and runs **past** the closing marker. If
/// one does, the emphasis loses: `*[foo*](bar)` is a link containing a `*`, not
/// an `em` containing a `[`.
///
/// `src` is the level's remaining input — muya's `state.src` — so a rule is
/// free to match beyond `offset`, which is the entire point. `rules` is one of
/// the two sets of M1.md §5 D5, and **which set is passed is the decision**:
/// [`rules::VALIDATE_RULES`] for emphasis and emoji, [`rules::LINK_VALIDATE_RULES`]
/// for links (S3).
///
/// # The two marktext fixes that look like noise
///
/// Both are load-bearing and both have a transcribed spec.
///
/// **`ignoreIndex` — #1071.** Without it, `**`word 1`**, **`word 2`**` bolded
/// only the last span. The scan reaches the position of a code span's opening
/// backtick, finds `inline_code` matching to the *next* span's closing
/// backtick — which is past this emphasis's closer — and vetoes. Recording the
/// end of every already-matched construct and skipping it stops the scan from
/// re-entering text a higher-priority rule has already claimed.
///
/// **The backslash-parity skip — #3778.** A character preceded by an odd number
/// of backslashes is escaped, so it cannot open anything. Without the skip the
/// `\$` in `It costs **\$20** to **\$30** online.` reads as an `inline_math`
/// delimiter whose partner is the *next* `\$`, which runs past the first
/// `**` closer, and the first bold is suppressed.
///
/// # Two byte-offset details
///
/// muya indexes UTF-16 code units and this indexes UTF-8 bytes (D1). Two places
/// where that needs an argument rather than an assertion:
///
/// - **Positions inside a multi-byte character are skipped.** JavaScript can
///   `substring` into the middle of a surrogate pair; Rust cannot slice off a
///   `char` boundary. It does not matter, because **every rule in both sets is
///   `^`-anchored on an ASCII character** — a backtick, `!`, `[`, `<`, `&`,
///   `\n`, a space, `:`, `~`, `$`, `w`, `h` or an ASCII local-part class — so
///   no rule can match at a position that is not a `char` boundary in the first
///   place. `no_rule_can_match_at_a_non_ascii_position` proves that rather than
///   asserting it.
/// - **`ignoreIndex` stores the start of a match's last character**, where muya
///   stores `i + to[0].length - 1`, the index of its last code unit. For a
///   character in the BMP those are the same position. For one outside it muya
///   stores the low surrogate, which is not a position any rule could have
///   matched at anyway — so the two agree on every input, and this one is
///   expressible in bytes.
pub(crate) fn lower_priority(src: &str, offset: usize, rules: &[RuleId]) -> bool {
    let bytes = src.as_bytes();
    let mut ignore_index: Vec<usize> = Vec::new();

    for i in 0..offset {
        if ignore_index.contains(&i) {
            continue;
        }
        // See the second byte-offset note above.
        if !src.is_char_boundary(i) {
            continue;
        }

        // A character preceded by an odd number of backslashes is escaped
        // (`\$`), so it cannot open a higher-priority construct and must not
        // lower the surrounding emphasis. #3778.
        let mut backslashes = 0usize;
        let mut j = i;
        while j > 0 && bytes[j - 1] == b'\\' {
            backslashes += 1;
            j -= 1;
        }
        if backslashes % 2 == 1 {
            continue;
        }

        let text = &src[i..];

        for &id in rules {
            let Some(caps) = exec(rule(id), text) else {
                continue;
            };
            let matched = caps.get(0).expect("a match always has group 0").as_str();

            if matched.len() > offset - i {
                // It runs past the closing marker: the other construct wins.
                return false;
            }
            if let Some(last) = matched.chars().next_back() {
                ignore_index.push(i + matched.len() - last.len_utf8());
            }
        }
    }

    true
}

// ---------------------------------------------------------------------------
// canOpenEmphasis / canCloseEmphasis
// ---------------------------------------------------------------------------

/// `canOpenEmphasis` (`utils.ts:223`) — CommonMark §6.2 left-flanking.
///
/// `src` is the level's remaining input, positioned at the opening marker.
/// `pending` is the run of plain text accumulated since the last token, which
/// is where the *preceding* character comes from.
///
/// # The two empty-string defaults, which are not the same default
///
/// `utils.ts` writes them a line apart and they differ, deliberately:
///
/// | Site | JavaScript | Value | Matches |
/// |---|---|---|---|
/// | preceded, `pending` empty | `lastCodePointChar(pending) \|\| '\n'` | `'\n'` | whitespace |
/// | followed, past end of `src` | `codePointCharAt(src, mLen) ?? ''` | `''` | nothing |
///
/// `'\n'` is whitespace, so an opener at the very start of a line is treated as
/// preceded by whitespace — which is what CommonMark §6.2 means by *"the
/// beginning and the end of the line count as Unicode whitespace"*. `''`
/// matches neither the whitespace class, nor the punctuation class, nor CJK, so
/// a marker at the very end of the input is treated as followed by a character
/// that is none of those — and therefore is **not** rejected by the
/// whitespace test on the next line. Substituting `'\n'` there instead would
/// reject it. The asymmetry is the behaviour.
///
/// This is why the preceded value is a `char` here and the followed value is an
/// `Option<char>`: the type carries which default applies, so the two cannot be
/// collapsed by accident. [`can_close_emphasis`] has the mirror image, and the
/// mirror is the other way round.
fn can_open_emphasis(src: &str, marker: &str, pending: &str) -> bool {
    let marker_char = marker.chars().next().expect("a marker is never empty");

    // CommonMark §6.4 emphasis runs are atomic: once the FULL `_` or `*` run
    // has been rejected as an opener, the lexer must not try to re-open from
    // the second character of the same run. Without this guard,
    // `пристаням__стремятся__` opens `em` at the second `_`, because `pending`
    // then ends with `_` — a punctuation character — and the ordinary flanking
    // rule is satisfied. That is CommonMark example 387, and it has a
    // transcribed spec.
    //
    // muya compares `pending.charAt(pending.length - 1)`, the last code *unit*.
    // `ends_with` compares the last `char`; the two agree because `marker_char`
    // is `*` or `_`, and the last byte of a multi-byte character is never an
    // ASCII one.
    if pending.ends_with(marker_char) {
        return false;
    }

    let preceded = pending.chars().next_back().unwrap_or('\n');
    let followed = src[marker.len()..].chars().next();

    // (1) not followed by Unicode whitespace,
    if is_unicode_whitespace(followed) {
        return false;
    }

    // and either (2a) not followed by a punctuation character, or (2b)
    // followed by punctuation and preceded by Unicode whitespace or
    // punctuation — widened by CJK_REG, additively.
    if is_punctuation(followed) && !is_flanking_boundary(Some(preceded)) {
        return false;
    }

    // `_` additionally may not open intra-word: `/_/.test(marker)` is an
    // unanchored test for an underscore anywhere in the marker, so it covers
    // both `_` and `__`.
    if marker.contains('_') && !is_flanking_boundary(Some(preceded)) {
        return false;
    }

    true
}

/// `canCloseEmphasis` (`utils.ts:273`) — CommonMark §6.2 right-flanking.
///
/// `offset` is the length of the whole candidate match, so the closing marker
/// occupies `src[offset - marker.len()..offset]`.
///
/// # The other two empty-string defaults, mirrored the other way
///
/// | Site | JavaScript | Value | Matches |
/// |---|---|---|---|
/// | preceded, nothing before the closer | `lastCodePointChar(…)` | `''` | nothing |
/// | followed, past end of `src` | `codePointCharAt(src, offset) \|\| '\n'` | `'\n'` | whitespace |
///
/// Compare [`can_open_emphasis`]: there the *preceded* value falls back to
/// `'\n'` and the *followed* value to `''`; here it is the reverse. Both
/// choices are the useful one for their site — a closer at the end of the line
/// counts as followed by whitespace, which is what lets `**bold**` close at all
/// — but the preceded default is unreachable rather than chosen: `offset` is at
/// least `2 × marker.len() + 1`, so there is always at least the opening marker
/// in front of the closer. It is reproduced because a defaulted value that
/// cannot arise today is one that can arise after the next change to a rule.
fn can_close_emphasis(src: &str, offset: usize, marker: &str) -> bool {
    let preceded = src[..offset - marker.len()].chars().next_back();
    let followed = src[offset..].chars().next().unwrap_or('\n');

    // (1) not preceded by Unicode whitespace,
    if is_unicode_whitespace(preceded) {
        return false;
    }

    // and either (2a) not preceded by punctuation, or (2b) preceded by
    // punctuation and followed by Unicode whitespace or punctuation — widened
    // by CJK_REG, symmetrically with canOpenEmphasis.
    if is_punctuation(preceded) && !is_flanking_boundary(Some(followed)) {
        return false;
    }

    if marker.contains('_') && !is_flanking_boundary(Some(followed)) {
        return false;
    }

    true
}

// ---------------------------------------------------------------------------
// validateEmphasize
// ---------------------------------------------------------------------------

/// The four markers `validateEmphasize` can be handed, each with the
/// `SHORTER_REG` and `CLOSE_REG` that `utils.ts:323-328` would build for it.
///
/// muya constructs both with `new RegExp` **on every call**, from the marker:
///
/// ```js
/// new RegExp(` \\${marker.split('').join('\\')}[^\\${marker.charAt(0)}]`)
/// new RegExp(`[^\\${marker.charAt(0)}]\\${marker.split('').join('\\')}`)
/// ```
///
/// Only `strong` (`\*\*|__`) and `em` (`\*|_`) reach here — `tryChunks` does
/// not validate `del` — so the marker is one of exactly four strings and the
/// eight regexes are constants. Precomputing them removes a regex compile from
/// the inner loop of every emphasis attempt and, more usefully, makes the
/// expansion readable: the `\\${…}` template is not obvious, and getting the
/// **literal leading space** in `SHORTER_REG` wrong is a silent behaviour
/// change rather than a syntax error.
///
/// `\_` is written as a bare `_` because Rust's regex parser rejects an escape
/// on a word character; `_` is never a metacharacter, so the two are the same
/// pattern. Inside a class, `[^*]` needs no escape either.
const EMPHASIS_MARKERS: [(&str, &str, &str); 4] = [
    // marker      SHORTER_REG      CLOSE_REG
    ("**", r" \*\*[^*]", r"[^*]\*\*"),
    ("__", r" __[^_]", r"[^_]__"),
    ("*", r" \*[^*]", r"[^*]\*"),
    ("_", r" _[^_]", r"[^_]_"),
];

static EMPHASIS_REGEXES: LazyLock<Vec<(Regex, Regex)>> = LazyLock::new(|| {
    EMPHASIS_MARKERS
        .iter()
        .map(|(marker, shorter, close)| {
            (
                compile(&format!("SHORTER_REG({marker})"), shorter),
                compile(&format!("CLOSE_REG({marker})"), close),
            )
        })
        .collect()
});

/// The `(SHORTER_REG, CLOSE_REG)` pair for `marker`.
///
/// # Panics
///
/// If `marker` is not one of the four. That is a port error — the only callers
/// are `tryStrongEm`'s two rules, whose capture 1 is `\*\*|__` or `\*|_` — so
/// it either always panics or never does, exactly like
/// [`crate::rules::compile`].
fn emphasis_regexes(marker: &str) -> &'static (Regex, Regex) {
    let index = EMPHASIS_MARKERS
        .iter()
        .position(|(m, _, _)| *m == marker)
        .unwrap_or_else(|| {
            panic!(
                "validateEmphasize was given the marker `{marker}`, which is not one of the four \
                 `strong`/`em` markers"
            )
        });
    &EMPHASIS_REGEXES[index]
}

/// `validateEmphasize` (`utils.ts:309`) — may this `*`/`_` run be an emphasis
/// span?
///
/// Four tests in order, and the order is muya's:
///
/// 1. [`can_open_emphasis`] — CommonMark §6.2 left-flanking;
/// 2. [`can_close_emphasis`] — right-flanking;
/// 3. §6.4 rule 16, *"when two potential spans share a closing delimiter the
///    shorter one wins"* — **which never fires; see below**;
/// 4. [`lower_priority`] — §6.4 rule 17.
///
/// `src` is the level's remaining input and `offset` the whole match length, so
/// the content is `src[marker.len()..offset - marker.len()]`. Both edges are
/// `char` boundaries because a marker is ASCII.
///
/// # Test 3 is unsatisfiable, and that is not a porting error
///
/// muya's condition is `SHORTER_REG.test(t) && !CLOSE_REG.test(t)`, and for
/// every one of the four markers **the first implies the second**, so the
/// conjunction is never true. The patterns are ` ␣M[^m]` and `[^m]M`, where `M`
/// is the escaped marker and `m` its first character:
///
/// > If `SHORTER_REG` matches at index `p`, then `t[p]` is a space and
/// > `t[p+1..]` begins with `M`. A space is `[^m]` for `m ∈ {*, _}`. So
/// > `CLOSE_REG` matches at `p` too.
///
/// So `**foo **bar baz**` is **one strong span covering the whole line**, not
/// CommonMark 0.29's `**foo <strong>bar baz</strong>` — measured against the
/// running engine, not inferred. §3 rule 3: behaviour is defined by the
/// TypeScript, not by the spec.
///
/// The two regexes are still built, still run, and still in this order,
/// because a faithful port is what M1 is for and because the day someone
/// repairs the guard upstream this is where the diff belongs. What matters is
/// that a later reader does not see a dead branch, conclude the port dropped
/// something, and "fix" it into a real behavioural divergence.
/// `the_shorter_span_guard_can_never_fire` states the implication as a test.
pub(crate) fn validate_emphasize(
    src: &str,
    offset: usize,
    marker: &str,
    pending: &str,
    rules: &[RuleId],
) -> bool {
    if !can_open_emphasis(src, marker, pending) {
        return false;
    }

    if !can_close_emphasis(src, offset, marker) {
        return false;
    }

    let marker_len = marker.len();
    let emphasize_text = &src[marker_len..offset - marker_len];
    let (shorter, close) = emphasis_regexes(marker);
    if exec(shorter, emphasize_text).is_some() && exec(close, emphasize_text).is_none() {
        return false;
    }

    lower_priority(src, offset, rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn punct(ch: char) -> bool {
        is_punctuation(Some(ch))
    }

    fn cjk(ch: char) -> bool {
        is_cjk(Some(ch))
    }

    fn ws(ch: char) -> bool {
        is_unicode_whitespace(Some(ch))
    }

    // -----------------------------------------------------------------------
    // The classes, at their boundaries
    // -----------------------------------------------------------------------

    /// The ASCII half of `PUNCTUATION_REG`, including the character that is
    /// **not** in it.
    #[test]
    fn ascii_punctuation_is_muyas_set_and_excludes_the_backslash() {
        // 0x21-0x2f, 0x3a-0x40, 0x5b, 0x5d-0x60, 0x7b-0x7e, measured from the
        // running regex.
        for ch in r##"!"#$%&'()*+,-./:;<=>?@[]^_`{|}~"##.chars() {
            assert!(punct(ch), "`{ch}` should be punctuation");
        }
        // The surprise, and the reason this test exists: `utils.ts:5`'s
        // commented-out array lists `'\\'` and CommonMark §2.1 counts it, but
        // the live class runs `…@[\]^_`…` — a `[`, then an escaped `]` — and
        // has no `\\` anywhere. A backslash is not a flanking boundary.
        assert!(!punct('\\'), "U+005C is not in muya's punctuation class");
        // Neither are the obvious non-members.
        for ch in ['a', 'Z', '0', ' ', '\n', '\u{a0}', '中'] {
            assert!(!punct(ch), "`{ch}` should not be punctuation");
        }
    }

    /// Every non-BMP member, at both edges of every range, plus the gaps.
    ///
    /// These are the ones a hand transcription gets wrong, because `utils.ts`
    /// spells them as surrogate pairs. Each was read off the running regex.
    #[test]
    fn non_bmp_punctuation_decodes_its_surrogate_pairs_correctly() {
        for cp in [
            0x10100, 0x10102, 0x1039f, 0x103d0, // \uD800[…]
            0x1056f, // 𐕯
            0x10857, 0x1091f, 0x1093f, 0x10a50, 0x10a58, 0x10a7f, 0x10af0, 0x10af6, 0x10b39,
            0x10b3f, 0x10b99, 0x10b9c, // \uD802[…]
            0x11047, 0x1104d, 0x110bb, 0x110bc, 0x110be, 0x110c1, 0x11140, 0x11143, 0x11174,
            0x11175, 0x111c5, 0x111c9, 0x111cd, 0x111db, 0x111dd, 0x111df, 0x11238, 0x1123d,
            0x112a9, // \uD804[…]
            0x114c6, 0x115c1, 0x115d7, 0x11641, 0x11643, 0x1173c, 0x1173e, // \uD805[…]
            0x12470, 0x12474, // \uD809[…]
            0x16a6e, 0x16a6f, 0x16af5, 0x16b37, 0x16b3b, 0x16b44, // \uD81A[…]
            0x1bc9f, // 𛲟
            0x1da87, 0x1da8b, // \uD836[…]
        ] {
            let ch = char::from_u32(cp).expect("a scalar value");
            assert!(punct(ch), "U+{cp:04X} should be punctuation");
        }

        // …and one code point on each side of a representative range, so the
        // ranges are ranges and not an over-wide approximation of one.
        for cp in [
            0x100ff, 0x10103, 0x1039e, 0x103d1, 0x1056e, 0x10a4f, 0x10a59, 0x1104e, 0x110bd,
            0x115d8, 0x12475, 0x1bc9e, 0x1da86, 0x1da8c,
        ] {
            let ch = char::from_u32(cp).expect("a scalar value");
            assert!(!punct(ch), "U+{cp:04X} should not be punctuation");
        }
    }

    /// `CJK_REG`, including the range where the regex and its own comment part
    /// company.
    #[test]
    fn the_cjk_class_follows_the_regex_and_not_its_comment() {
        for (lo, hi) in [
            (0x3040u32, 0x30ffu32),
            (0x3400, 0x4dbf),
            (0x4e00, 0x9fff),
            (0xf900, 0xfaff),
            (0xac00, 0xd7af),
            (0xff66, 0xff9d),
            (0x20000, 0x2ffff),
        ] {
            for cp in [lo, hi] {
                let ch = char::from_u32(cp).expect("a scalar value");
                assert!(cjk(ch), "U+{cp:04X} should be CJK");
            }
            for cp in [lo - 1, hi + 1] {
                if let Some(ch) = char::from_u32(cp) {
                    assert!(!cjk(ch), "U+{cp:04X} should not be CJK");
                }
            }
        }

        // The comment above CJK_REG says the non-BMP branch is
        // "U+20000–U+2A6DF". `[\uD840-\uD87F][\uDC00-\uDFFF]` is not: it is all
        // of plane 2. Port the regex, not the comment.
        assert!(cjk(char::from_u32(0x2a6df).expect("scalar")));
        assert!(
            cjk(char::from_u32(0x2a6e0).expect("scalar")),
            "one past the comment's claimed end, and still CJK"
        );
        assert!(cjk(char::from_u32(0x2ffff).expect("scalar")), "plane 2 end");
        assert!(!cjk(char::from_u32(0x30000).expect("scalar")), "plane 3");

        // Ordinary text is not CJK.
        for ch in ['a', '1', ' ', '，'] {
            assert!(!cjk(ch), "`{ch}` should not be CJK");
        }
        // …though a fullwidth comma is punctuation, which is the other half of
        // the boundary test.
        assert!(punct('，'));
    }

    /// The C2 whitespace disagreement, reached the way the flanking check
    /// reaches it.
    #[test]
    fn the_whitespace_class_is_javascripts_and_not_rusts() {
        assert!(ws('\u{feff}'), "U+FEFF is JavaScript whitespace");
        assert!(!ws('\u{85}'), "U+0085 is not");
        for ch in [' ', '\t', '\n', '\r', '\u{a0}', '\u{2028}', '\u{3000}'] {
            assert!(ws(ch));
        }
        assert!(!ws('a'));
    }

    #[test]
    fn the_word_class_is_ascii_only() {
        for ch in ['a', 'Z', '0', '_'] {
            assert!(is_word_character(Some(ch)));
        }
        for ch in ['д', '中', '٧', '-', ' '] {
            assert!(!is_word_character(Some(ch)), "`{ch}` is not \\w");
        }
    }

    /// The `''` default of `canOpen`'s followed character and `canClose`'s
    /// preceded character: it matches **none** of the four classes, which is
    /// what makes the asymmetry with `'\n'` observable.
    #[test]
    fn absent_characters_match_no_class_at_all() {
        assert!(!is_unicode_whitespace(None));
        assert!(!is_punctuation(None));
        assert!(!is_cjk(None));
        assert!(!is_word_character(None));
        assert!(!is_flanking_boundary(None));
        // …whereas '\n', the other default, is whitespace and therefore a
        // boundary. If these two ever agreed, the four defaults would have
        // collapsed into one.
        assert!(is_flanking_boundary(Some('\n')));
    }

    // -----------------------------------------------------------------------
    // isLengthEven
    // -----------------------------------------------------------------------

    #[test]
    fn is_length_even_is_vacuously_true_for_a_group_that_does_not_exist() {
        assert!(is_length_even(None), "isLengthEven(undefined) is true");
        assert!(is_length_even(Some(Span::new(0, 0))));
        assert!(!is_length_even(Some(Span::new(0, 1))));
        assert!(is_length_even(Some(Span::new(4, 6))));
        assert!(!is_length_even(Some(Span::new(4, 7))));
    }

    // -----------------------------------------------------------------------
    // lowerPriority
    // -----------------------------------------------------------------------

    /// The premise the byte-offset skip in [`lower_priority`] rests on: every
    /// rule in both `lowerPriority` sets is anchored on an ASCII character, so
    /// a position that is not a `char` boundary is a position no rule could
    /// have matched at.
    #[test]
    fn no_rule_can_match_at_a_non_ascii_position() {
        let non_ascii = [
            "中文**粗体**",
            "д:smile:",
            "\u{a0}`code`",
            "🙂 https://example.com",
            "，[text](url)",
        ];
        for &id in rules::VALIDATE_RULES
            .iter()
            .chain(rules::LINK_VALIDATE_RULES)
        {
            for src in non_ascii {
                assert!(
                    exec(rule(id), src).is_none(),
                    "`{}` matched a haystack starting with a non-ASCII character: {src:?}",
                    id.name()
                );
            }
        }
    }

    #[test]
    fn nothing_higher_priority_means_the_emphasis_survives() {
        // `**bold**`, offset 8. Nothing inside is a code span or a link.
        assert!(lower_priority("**bold**", 8, rules::VALIDATE_RULES));
    }

    /// CommonMark §6.4 rule 17, in one call: a code span that runs past the
    /// closing marker vetoes the emphasis.
    #[test]
    fn a_construct_running_past_the_closer_vetoes_the_emphasis() {
        // `*a`b*c`` — the code span opens inside the em and closes after it.
        let src = "*a`b*c`";
        assert!(!lower_priority(src, 5, rules::VALIDATE_RULES));
    }

    /// #3778, isolated from the tokenizer: without the parity skip the `\$`
    /// reads as a math delimiter whose partner is the next `\$`.
    #[test]
    fn an_escaped_delimiter_does_not_lower_the_emphasis() {
        let src = r"**\$20** to **\$30** online.";
        assert!(
            lower_priority(src, 8, rules::VALIDATE_RULES),
            "the escaped dollars must not be read as inline_math delimiters"
        );
        // The same input with the backslashes removed *is* vetoed, which is
        // what makes the test above about the escaping and not about the rule
        // set.
        let unescaped = r"**$20** to **$30** online.";
        assert!(!lower_priority(unescaped, 7, rules::VALIDATE_RULES));
    }

    /// #1071, isolated: without `ignoreIndex` the scan re-enters the first code
    /// span's text and finds `inline_code` reaching the *second* span's closer.
    #[test]
    fn an_already_matched_construct_is_not_rescanned() {
        let src = "**`word 1`**, **`word 2`**";
        assert!(
            lower_priority(src, 12, rules::VALIDATE_RULES),
            "the first `**…**` must survive the code span in the second"
        );
    }

    /// The rule set is the decision (M1.md §5 D5): the same input passes with
    /// the link set and fails with the emphasis set, which is #4671 in one
    /// assertion.
    #[test]
    fn the_two_rule_sets_disagree_and_that_is_the_point() {
        let src = "[CommonMark](https://spec.commonmark.org/)、其他";
        let offset = "[CommonMark](https://spec.commonmark.org/)".len();
        assert!(
            lower_priority(src, offset, rules::LINK_VALIDATE_RULES),
            "only inline_code, html_tag and auto_link may defer a link"
        );
        assert!(
            !lower_priority(src, offset, rules::VALIDATE_RULES),
            "auto_link_extension is in the wider set and would veto the link"
        );
    }

    // -----------------------------------------------------------------------
    // canOpen / canClose / validateEmphasize
    // -----------------------------------------------------------------------

    #[test]
    fn a_marker_followed_by_whitespace_cannot_open() {
        assert!(!can_open_emphasis("* a *", "*", ""));
        assert!(can_open_emphasis("*a*", "*", ""));
        // CommonMark example 353: the spaces are U+00A0, which is whitespace to
        // both engines and is the reason C2 insists the class be written out.
        assert!(!can_open_emphasis("*\u{a0}a\u{a0}*", "*", ""));
    }

    /// CommonMark example 387, at the function that implements it.
    #[test]
    fn a_run_already_rejected_cannot_reopen_from_its_second_character() {
        // The first `_` of `__` is rejected: preceded by `м`, which is neither
        // whitespace, punctuation nor CJK.
        assert!(!can_open_emphasis("__стремятся__", "__", "пристаням"));
        // …and the guard is what stops the second `_` re-opening as `em` with
        // a `pending` that now ends in the punctuation character `_`.
        assert!(!can_open_emphasis("_стремятся__", "_", "пристаням_"));
    }

    /// The CJK widening, in both directions.
    #[test]
    fn a_cjk_character_is_a_flanking_boundary() {
        // 中文**粗体**紧邻 — the opener is preceded by 文 and followed by 粗.
        assert!(can_open_emphasis("**粗体**紧邻", "**", "中文"));
        assert!(can_close_emphasis("**粗体**紧邻", "**粗体**".len(), "**"));
        // The same shape with `_`, which needs the boundary on both sides.
        assert!(can_open_emphasis("__粗体__紧邻", "__", "中文"));
        assert!(can_close_emphasis("__粗体__紧邻", "__粗体__".len(), "__"));
        // …and with a Latin letter in place of the CJK it is intra-word, so it
        // is refused. That is what "additive" means: CJK only ever accepts.
        assert!(!can_open_emphasis("__b__c", "__", "a"));
    }

    /// §6.4 rule 16's guard is unsatisfiable — the implication, stated for all
    /// four markers over inputs designed to trip it.
    ///
    /// `SHORTER_REG` is ` ␣M[^m]` and `CLOSE_REG` is `[^m]M`, so a
    /// `SHORTER_REG` match at `p` puts a space — which is `[^m]` — immediately
    /// before `M`, and `CLOSE_REG` matches at `p` as well. The conjunction
    /// muya tests can therefore never hold. See [`validate_emphasize`].
    #[test]
    fn the_shorter_span_guard_can_never_fire() {
        for (marker, _, _) in EMPHASIS_MARKERS {
            let m = marker.chars().next().expect("non-empty");
            let candidates = [
                format!("foo {marker}bar baz"),
                format!("{marker}bar baz"),
                format!("a {marker}b"),
                format!("a {marker}{m}b"),
                format!(" {marker}x"),
                format!("x {marker}y {marker}z"),
            ];
            let (shorter, close) = emphasis_regexes(marker);
            for text in &candidates {
                if exec(shorter, text).is_some() {
                    assert!(
                        exec(close, text).is_some(),
                        "SHORTER_REG matched {text:?} for marker `{marker}` but CLOSE_REG did \
                         not — the guard would fire, which cannot happen"
                    );
                }
            }
        }
    }

    /// …and the consequence, measured from the running engine: muya bolds
    /// `**foo **bar baz**` whole, where CommonMark 0.29 example 402 asks for
    /// `**foo <strong>bar baz</strong>`. §3 rule 3 — the TypeScript defines
    /// correctness, so this is the expectation and not a bug to fix.
    #[test]
    fn a_second_opener_inside_the_span_does_not_shorten_it() {
        assert!(validate_emphasize(
            "**foo **bar baz**",
            17,
            "**",
            "",
            rules::VALIDATE_RULES
        ));
        assert!(validate_emphasize(
            "*foo *bar baz*",
            14,
            "*",
            "",
            rules::VALIDATE_RULES
        ));
    }

    /// The leading space in `SHORTER_REG` is literal, so an opener glued to the
    /// preceding word does not trigger rule 16.
    #[test]
    fn shorter_reg_requires_the_literal_leading_space() {
        let (shorter, _) = emphasis_regexes("**");
        assert!(exec(shorter, "foo **bar").is_some());
        assert!(
            exec(shorter, "foo**bar").is_none(),
            "without the space it is not a candidate opener"
        );
    }

    #[test]
    fn every_marker_has_a_precomputed_pair() {
        for (marker, _, _) in EMPHASIS_MARKERS {
            let _ = emphasis_regexes(marker);
        }
        assert_eq!(EMPHASIS_MARKERS.len(), 4, "strong has two, em has two");
    }

    #[test]
    #[should_panic(expected = "not one of the four")]
    fn an_unknown_marker_is_a_port_error() {
        let _ = emphasis_regexes("~~");
    }
}
