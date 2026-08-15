//! The grammar generator — `cargo xtask grammars` (docs/M3.md §5 D14).
//!
//! ```text
//! reference clone ──► tools/dump-prism-grammars.mjs ──► target/xtask/grammars.json
//!                                                                 │
//!                                             this module ────────┘
//!                                                    │  translate every JS regex
//!                                                    │  compile it with fancy-regex
//!                                                    ▼
//!                                    crates/mt-highlight/src/generated.rs  (committed)
//! ```
//!
//! # Why this is an `xtask` subcommand and not a `build.rs`
//!
//! D14 requires *"a committed generator … its output is committed Rust,
//! compiled in"*, and the reason a build script is wrong is the same reason
//! `corpus` and `layout --update` are subcommands: **the generator's input is
//! not in this repository.** It is `prismjs` inside the *marktext clone*, which
//! most checkouts do not have and CI resolves through `$MARKTEXT_DIR`. A
//! `build.rs` would make `cargo build` fail on a machine that merely lacks the
//! reference, and would run Node on every build for a result that changes only
//! when somebody deliberately moves the prismjs version.
//!
//! The other half of the reason is review. `bench/layout-goldens/README.md`
//! establishes the pattern this follows: a regeneration is a **reviewable
//! event**, so the tool is explicit, `--check` is what CI runs, and the diff is
//! in the tree where a human sees it.
//!
//! # Why Node dumps and Rust translates
//!
//! `tools/dump-prism-grammars.mjs` writes JSON and translates nothing. Every
//! JS-regex → Rust-regex decision is here, in Rust, because that is the only
//! side that can **check its own answer**: [`translate`]'s output is compiled
//! with the very `fancy-regex` the interpreter will use before a byte is
//! emitted. A translator written in the Node half could only be checked by eye.
//!
//! # What gets emitted, and why it is not all 297
//!
//! **The generator loads all 297** — that is not optional, D14 consequence 2:
//! thirteen grammar files mutate *other* grammars at load time, so a grammar's
//! content is a property of the load set and both sides of S3's differential
//! must agree on it. What it *emits* is the reference closure of
//! [`crate::highlight::PORTED`] — the ported languages, plus every grammar
//! reachable from them through `inside`.
//!
//! Three reasons, in the order they weigh:
//!
//! 1. **A grammar nobody compared is not coverage.** `highlight.rs` refuses a
//!    coverage number that can be raised by editing a list. Committing 297
//!    grammars of which sixteen have ever been checked against Prism would
//!    put 281 unverified tables in the binary and invite exactly that reading.
//! 2. **It is the ratchet D3 asks for.** *"Port languages in usage order and
//!    let `cargo xtask highlight` count coverage as a number that only goes
//!    up."* One list drives both the emitted data and the differential, so
//!    adding a language is a one-line edit plus a regeneration — and the
//!    differential is what decides whether the edit survives.
//! 3. **The whole set does not translate yet, which is a fact rather than a
//!    forecast.** The measurement was run: emitting all 292 grammar-registering
//!    languages produces **1,715 grammars, 10,182 patterns**, and **four
//!    patterns in three languages cannot be translated at all** — `bqn`'s
//!    `character-literal` matches a surrogate *pair* by hand
//!    (`[\uD800-\uDBFF][\uDC00-\uDFFF]`), which is not expressible over Unicode
//!    scalar values, and `coq` and `icu-message-format` contain ECMAScript's
//!    empty class `[]`. The run then **overflows the stack** inside
//!    fancy-regex's recursive-descent parser on one of the remaining patterns.
//!    Both are solvable and neither is solved here.
//!
//! **Compile time is explicitly *not* the reason, because it was measured and
//! it is not a problem.** The all-292 file is **4,545,789 B** and still builds
//! in **0.39 s** dev / **1.03 s** release; the committed sixteen-language file
//! is 382,044 B. What the measurement does settle is D14's question about D3's
//! *"< 1.5 MB for all 297"*: the JS regex source alone is **1.09 MB** and
//! translation takes it to **2.23 MB** (+103.8 %), so that estimate is already
//! exceeded by the pattern text before any of the surrounding table is counted.
//!
//! **The generator is general either way**: it holds no per-language knowledge
//! and nothing about the sixteen is hard-coded here.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Exit code the dumper uses for "the reference engine is not available".
const EXIT_UNIMPLEMENTED: i32 = 3;

/// Where the committed output goes.
const GENERATED: &[&str] = &["crates", "mt-highlight", "src", "generated.rs"];

// ---------------------------------------------------------------------------
// The JS → Rust regex translator
// ---------------------------------------------------------------------------

/// ECMAScript's `\w`, as a character-class **body**.
///
/// **ASCII, and that is the whole point.** None of Prism's 10,182 regexes
/// carries the `u` flag, and without it `\w` in ECMAScript is exactly
/// `[a-zA-Z0-9_]` (ECMA-262 §22.2.2.9, table 66). `regex`'s `\w` is Unicode —
/// `\p{Alphabetic}` plus marks, digits, connectors and joiners — so copying
/// `\w` through would silently widen every identifier rule in every grammar.
const JS_WORD: &str = "0-9A-Za-z_";

/// ECMAScript's `\s`, as a character-class body.
///
/// `WhiteSpace ∪ LineTerminator` (ECMA-262 §12.2 and §12.3): TAB, LF, VT, FF,
/// CR, ZWNBSP, LS, PS, and the whole `Zs` category — which is where SPACE and
/// NBSP come from. Written with `\p{Zs}` rather than as eleven code points
/// because it is both shorter and exactly the spec's own wording.
///
/// It differs from `regex`'s `\s` in **two** code points and both matter here
/// only in that they are wrong for free otherwise: `regex` includes U+0085
/// (NEL), which ECMAScript does not, and omits U+FEFF, which ECMAScript
/// includes.
const JS_SPACE: &str = r"\t\n\x0B\x0C\r\u{FEFF}\u{2028}\u{2029}\p{Zs}";

/// ECMAScript's `.` — anything except the **four** line terminators.
///
/// `regex`'s `.` excludes only `\n`. The other three (CR, LS, PS) are the
/// difference, and a `.` copied through would match a CR that Prism's would
/// not — which is not hypothetical in a file that has been through a Windows
/// checkout.
const JS_DOT: &str = r"[^\n\r\u{2028}\u{2029}]";

/// ECMAScript's `\b`, spelled with lookaround.
///
/// **`fancy-regex` cannot express this any other way.** `regex` would write it
/// `(?-u:\b)`, but fancy-regex rejects inline Unicode-mode changes outright
/// (*"Changing Unicode mode inline is not supported"*), and its own `\b` is
/// Unicode-aware while ECMAScript's is ASCII. The divergence is not theoretical:
/// on `importé`, `/\bimport\b/` matches in a browser (é is not an ECMAScript
/// word character, so there is a boundary) and does not match with a Unicode
/// `\b`, so a keyword would highlight in MarkText and not in the port.
///
/// The cost is real and is recorded rather than discovered: 68 characters per
/// occurrence, and a pattern that had no fancy construct before now has one, so
/// it leaves `regex`'s non-backtracking engine. See the size table in
/// [`translate`].
const JS_WORD_BOUNDARY: &str =
    r"(?:(?<![0-9A-Za-z_])(?=[0-9A-Za-z_])|(?<=[0-9A-Za-z_])(?![0-9A-Za-z_]))";

/// ECMAScript's `\B`, the complement of [`JS_WORD_BOUNDARY`].
const JS_NOT_WORD_BOUNDARY: &str =
    r"(?:(?<=[0-9A-Za-z_])(?=[0-9A-Za-z_])|(?<![0-9A-Za-z_])(?![0-9A-Za-z_]))";

/// Translate one `RegExp.prototype.source` into `fancy-regex` syntax.
///
/// # The dialect gap, measured over all 10,182 patterns
///
/// | Construct | Occurrences | Translation | Why |
/// |---|---:|---|---|
/// | `\s` `\S` | 12,977 / 3,449 | [`JS_SPACE`] | `regex`'s `\s` has U+0085 and lacks U+FEFF |
/// | `\b` `\B` | 6,082 / 312 | [`JS_WORD_BOUNDARY`] | ECMAScript's is ASCII, `regex`'s is Unicode |
/// | `\/` | 5,249 | `/` | a regex *literal*'s escape; not a regex construct at all |
/// | `\d` `\D` | 4,177 | `[0-9]` | `regex`'s `\d` is `\p{Nd}` |
/// | `\w` `\W` | 3,606 / 5 | [`JS_WORD`] | `regex`'s `\w` is Unicode |
/// | `.` | 1,625 patterns | [`JS_DOT`] | `regex`'s `.` excludes only `\n` |
/// | `\uNNNN` `\xNN` | 2,166 / 2,097 | the code point, re-spelled | printable ASCII stays printable so the diff is readable; everything else is `\u{XXXX}` |
/// | `\0` | 6 | `\x00` | fancy-regex reads `\0` as *back-reference to group 0* and refuses to compile |
/// | `[^]` | 4 | `(?s:.)` | `regex` rejects an empty negated class |
/// | `\ud800-\udfff` in a class | 1 (`yaml.key`) | dropped | not Unicode scalar values; see [`scalar_ranges`] |
/// | `\1`…`\5` | 890 | verbatim | back-references; fancy-regex has them |
/// | `(?:` `(?=` `(?!` | 32,347 | verbatim | fancy-regex has lookahead |
///
/// Two constructs the survey expected and **did not find**, recorded because
/// their absence is what makes this tractable: there is **no `(?<=` or `(?<!`
/// anywhere** in Prism's 10,182 patterns (every lookaround is forward), and no
/// named group. There is also no `s`, `u` or `y` flag — Prism writes `[\s\S]`
/// where another author would write `/./s`.
///
/// # The one non-mechanical rewrite, and why it is not a weakening
///
/// `[\s\S]` — the dot-all idiom, and by a wide margin the most common class in
/// the corpus — becomes `(?s:.)` rather than the 90-character union of a set
/// with its own complement. That is an identity, not a tolerance: a set unioned
/// with its complement is the universe, and `(?s:.)` is the universe. The same
/// applies to `[\w\W]`, `[\d\D]` and `[^]`. It is called out because it is the
/// only place this function is cleverer than a table lookup.
///
/// # The one gap that is **not** closed, named rather than hidden
///
/// **A non-`u` ECMAScript regex matches UTF-16 code units; `regex` matches
/// Unicode scalar values.** So `.`, and any negated class, matches *one code
/// unit* in a browser and *one character* here, and for a non-BMP character
/// those differ: `/[^']/` consumes half of `😀` in Prism and all of it here.
/// There is no translation that closes this — Rust's `str` cannot hold half a
/// character — so it is recorded rather than papered over.
///
/// **It only bites where a pattern needs exactly one code unit.** Under `*`,
/// `+` or `*?` the two engines agree, because two iterations of a one-unit
/// matcher consume what one iteration of a one-character matcher does; `"😀"`
/// tokenizes identically as a JavaScript string. The measured effect, over a
/// 307-file sweep of real source (407,114 Prism spans): **two** disagreements,
/// both `rust.char` — `'😀'` and `'𝄞'` are `char` tokens here and plain text in
/// Prism, because that pattern's `[^\\\r\n\t']` is unquantified and an astral
/// character is two units. Zero disagreements over the 2,507 corpus fences.
///
/// The near-miss fixes are worse and were tried on paper: excluding astral
/// characters from single-unit constructs fixes `rust.char` and **breaks every
/// string containing an emoji**, and making the exclusion depend on the
/// enclosing quantifier is still wrong for `{2}`. A port that trades two known
/// disagreements for an unknown number is not an improvement.
///
/// # Failing loudly
///
/// Every construct not in the table above is an `Err`, including ones that
/// would happily compile as something else — `\p{L}` (an *identity escape* in a
/// non-`u` ECMAScript regex, a Unicode property in `regex`), legacy octal
/// escapes, `\cX`, and a lone surrogate outside a character class, where there
/// is no range to clamp. The generator refuses to emit rather than emit
/// something that compiles and is wrong.
fn translate(source: &str, case_insensitive: bool, multiline: bool) -> Result<String, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len() * 2);
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];

        if c == '\\' {
            let Some(&next) = chars.get(i + 1) else {
                return Err("trailing backslash".to_string());
            };
            i += 2;
            match next {
                '\\' => out.push_str(r"\\"),
                // A regex *literal*'s delimiter escape. `RegExp.prototype.source`
                // keeps it, and it is not a regex construct in any dialect.
                '/' => out.push('/'),
                'n' => out.push_str(r"\n"),
                'r' => out.push_str(r"\r"),
                't' => out.push_str(r"\t"),
                'f' => out.push_str(r"\x0C"),
                'v' => out.push_str(r"\x0B"),
                // ECMAScript's `\0` is NUL only when no digit follows; with one
                // it is a legacy octal escape, which this refuses.
                '0' => {
                    if chars.get(i).is_some_and(char::is_ascii_digit) {
                        return Err("legacy octal escape `\\0` followed by a digit".to_string());
                    }
                    out.push_str(r"\x00");
                }
                'x' | 'u' => {
                    let (code, width) = hex_escape(&chars, i, next)?;
                    i += width;
                    out.push_str(&escaped(scalar(code)?));
                }
                'd' => out.push_str("[0-9]"),
                'D' => out.push_str("[^0-9]"),
                'w' => {
                    let _ = write!(out, "[{JS_WORD}]");
                }
                'W' => {
                    let _ = write!(out, "[^{JS_WORD}]");
                }
                's' => {
                    let _ = write!(out, "[{JS_SPACE}]");
                }
                'S' => {
                    let _ = write!(out, "[^{JS_SPACE}]");
                }
                'b' => out.push_str(JS_WORD_BOUNDARY),
                'B' => out.push_str(JS_NOT_WORD_BOUNDARY),
                '1'..='9' => {
                    out.push('\\');
                    out.push(next);
                    // Multi-digit group references are copied whole.
                    while let Some(d) = chars.get(i).filter(|d| d.is_ascii_digit()) {
                        out.push(*d);
                        i += 1;
                    }
                }
                // Escaped punctuation is the same in both dialects. Listed
                // explicitly rather than matched by `is_ascii_punctuation`, so
                // that a character neither dialect agrees on cannot slip by.
                '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$'
                | '-' => {
                    out.push('\\');
                    out.push(next);
                }
                other => {
                    return Err(format!(
                        "unrecognised escape `\\{other}`; in a non-`u` ECMAScript regex this is an \
                         identity escape, and `regex` may read it as something else entirely"
                    ));
                }
            }
            continue;
        }

        match c {
            '[' => {
                // The dot-all idioms first: a set unioned with its complement is
                // the universe, and spelling it that way keeps thousands of
                // patterns short. See the function's doc comment.
                const UNIVERSE: &[&str] = &[
                    "[^]", r"[\s\S]", r"[\S\s]", r"[\w\W]", r"[\W\w]", r"[\d\D]", r"[\D\d]",
                ];
                let rest: String = chars[i..].iter().take(8).collect();
                if let Some(idiom) = UNIVERSE.iter().find(|u| rest.starts_with(**u)) {
                    out.push_str("(?s:.)");
                    i += idiom.chars().count();
                    continue;
                }
                let (text, width) = translate_class(&chars, i)?;
                out.push_str(&text);
                i += width;
            }
            '.' => {
                out.push_str(JS_DOT);
                i += 1;
            }
            '(' => {
                let opener = group_opener(&chars, i)?;
                out.push_str(opener);
                i += opener.chars().count();
            }
            '{' => {
                // ECMAScript treats a `{` that does not begin a valid quantifier
                // as a literal (Annex B); `regex` reads it as a malformed
                // repetition. `\{` is right in both readings.
                match quantifier_len(&chars, i) {
                    Some(n) => {
                        out.extend(&chars[i..i + n]);
                        i += n;
                    }
                    None => {
                        out.push_str(r"\{");
                        i += 1;
                    }
                }
            }
            '}' => {
                out.push_str(r"\}");
                i += 1;
            }
            ']' => {
                out.push_str(r"\]");
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    // Flags become a wrapping group rather than a `(?i)` prefix, because a
    // prefix's scope is the enclosing group and a top-level `|` would make that
    // ambiguous to a reader even where it is not ambiguous to the parser. The
    // group is non-capturing, so `lookbehind`'s reliance on **group 1** — see
    // the interpreter — survives it untouched.
    let mut flags = String::new();
    if case_insensitive {
        flags.push('i');
    }
    if multiline {
        flags.push('m');
    }
    Ok(if flags.is_empty() {
        out
    } else {
        format!("(?{flags}:{out})")
    })
}

/// One thing between `[` and `]`: a single code point, or a whole set.
enum ClassItem {
    /// A code point, which may be a range endpoint.
    Cp(u32),
    /// A shorthand (`\d`, `\S`, …) already rendered as class-body text. It can
    /// never be a range endpoint, in either dialect.
    Set(String),
}

/// Translate a whole character class, returning its text and its width in
/// `char`s.
///
/// **Parsed into items rather than rewritten character by character**, and the
/// reason is `yaml.key`. That one pattern contains
/// `[^…\ud800-\udfff￾￿]`, and `\u{D800}` is not a Unicode scalar
/// value, so `regex` refuses it outright. A character-at-a-time rewriter cannot
/// fix that, because the fix is to **clamp a range** — which means knowing that
/// `\ud800` and `\udfff` are the endpoints of one item rather than two escapes
/// with a hyphen between them. See [`scalar_ranges`] for why dropping them is an
/// identity rather than a repair.
///
/// Parsing also settles three smaller differences in one place:
///
/// - a literal `[` inside a class is a **nested class** to `regex` and a literal
///   to ECMAScript, so every code point is re-emitted escaped;
/// - `&&`, `--` and `~~` are set operators to `regex` and literals to
///   ECMAScript, and cannot survive a byte copy;
/// - ECMAScript's `[]` is the empty class and `[]]` is *"never match, then a
///   literal `]`"* — **not** POSIX's "a leading `]` is a literal". Both are
///   refused rather than guessed at; neither occurs.
fn translate_class(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let mut i = start + 1;
    let negated = chars.get(i) == Some(&'^');
    if negated {
        i += 1;
    }
    if chars.get(i) == Some(&']') {
        return Err(
            "empty character class `[]` matches nothing in ECMAScript and is a parse \
                    error in `regex`"
                .to_string(),
        );
    }

    enum Emit {
        Item(ClassItem),
        Range(u32, u32),
    }
    let mut items: Vec<Emit> = Vec::new();
    loop {
        match chars.get(i) {
            None => return Err("unterminated character class".to_string()),
            Some(']') => {
                i += 1;
                break;
            }
            _ => {}
        }
        let (item, width) = class_atom(chars, i)?;
        i += width;
        // A `-` is a range operator only between two code points; anywhere else
        // it is a literal, which is ECMAScript's rule and `regex`'s too.
        if let ClassItem::Cp(lo) = item {
            if chars.get(i) == Some(&'-') && chars.get(i + 1).is_some_and(|c| *c != ']') {
                let (hi, width) = class_atom(chars, i + 1)?;
                match hi {
                    ClassItem::Cp(hi) => {
                        if hi < lo {
                            return Err(format!("reversed class range \\u{lo:04X}-\\u{hi:04X}"));
                        }
                        i += 1 + width;
                        items.push(Emit::Range(lo, hi));
                        continue;
                    }
                    // `[a-\d]` is a syntax error in a `u`-mode ECMAScript regex
                    // and a literal `-` in Annex B. Refused: guessing which one
                    // Prism meant is exactly what this generator must not do.
                    ClassItem::Set(_) => {
                        return Err("a class shorthand cannot be a range endpoint".to_string());
                    }
                }
            }
        }
        items.push(Emit::Item(item));
    }

    let mut body = String::new();
    for item in &items {
        match item {
            Emit::Item(ClassItem::Cp(c)) => {
                if let Ok(c) = scalar(*c) {
                    body.push_str(&escaped(c));
                }
            }
            Emit::Item(ClassItem::Set(s)) => body.push_str(s),
            Emit::Range(lo, hi) => {
                for (lo, hi) in scalar_ranges(*lo, *hi) {
                    if lo == hi {
                        body.push_str(&escaped(lo));
                    } else {
                        let _ = write!(body, "{}-{}", escaped(lo), escaped(hi));
                    }
                }
            }
        }
    }
    Ok((
        format!("[{}{body}]", if negated { "^" } else { "" }),
        i - start,
    ))
}

/// One atom inside a character class, and its width in `char`s.
fn class_atom(chars: &[char], i: usize) -> Result<(ClassItem, usize), String> {
    let Some(&c) = chars.get(i) else {
        return Err("unterminated character class".to_string());
    };
    if c != '\\' {
        return Ok((ClassItem::Cp(c as u32), 1));
    }
    let Some(&next) = chars.get(i + 1) else {
        return Err("trailing backslash".to_string());
    };
    let cp = |c: char| Ok((ClassItem::Cp(c as u32), 2));
    match next {
        'd' => Ok((ClassItem::Set("0-9".to_string()), 2)),
        'D' => Ok((ClassItem::Set("[^0-9]".to_string()), 2)),
        'w' => Ok((ClassItem::Set(JS_WORD.to_string()), 2)),
        'W' => Ok((ClassItem::Set(format!("[^{JS_WORD}]")), 2)),
        's' => Ok((ClassItem::Set(JS_SPACE.to_string()), 2)),
        'S' => Ok((ClassItem::Set(format!("[^{JS_SPACE}]")), 2)),
        // ECMA-262 §22.2.2.4: inside a class `\b` is U+0008, not a boundary.
        'b' => Ok((ClassItem::Cp(0x08), 2)),
        'n' => Ok((ClassItem::Cp(0x0A), 2)),
        'r' => Ok((ClassItem::Cp(0x0D), 2)),
        't' => Ok((ClassItem::Cp(0x09), 2)),
        'f' => Ok((ClassItem::Cp(0x0C), 2)),
        'v' => Ok((ClassItem::Cp(0x0B), 2)),
        '0' => {
            if chars.get(i + 2).is_some_and(char::is_ascii_digit) {
                return Err("legacy octal escape `\\0` followed by a digit".to_string());
            }
            Ok((ClassItem::Cp(0), 2))
        }
        'x' | 'u' => {
            let (code, width) = hex_escape(chars, i + 2, next)?;
            Ok((ClassItem::Cp(code), 2 + width))
        }
        '\\' | '/' | ']' | '[' | '-' | '^' | '.' | '*' | '+' | '?' | '(' | ')' | '{' | '}'
        | '|' | '$' => cp(next),
        other => Err(format!(
            "unrecognised escape `\\{other}` inside a character class"
        )),
    }
}

/// `\xHH` or `\uHHHH` starting at `i` (just past the `x`/`u`): its value and its
/// width in `char`s.
fn hex_escape(chars: &[char], i: usize, kind: char) -> Result<(u32, usize), String> {
    let width = if kind == 'x' { 2 } else { 4 };
    let hex: String = chars.get(i..i + width).unwrap_or(&[]).iter().collect();
    if hex.len() != width || !hex.chars().all(|h| h.is_ascii_hexdigit()) {
        // `\u{…}` is *not* this: without the `u` flag it means a literal `u`
        // repeated, which nothing should rely on and this refuses to guess at.
        return Err(format!(
            "`\\{kind}` not followed by {width} hex digits: `\\{kind}{hex}`"
        ));
    }
    Ok((u32::from_str_radix(&hex, 16).expect("hex digits"), width))
}

/// A code point as a `char`, or an error if it is a lone surrogate.
///
/// Outside a character class there is nothing to clamp — `\uD800` on its own can
/// only be an attempt to match a lone surrogate, which cannot occur in a Rust
/// `&str` — so it is an error rather than a silent deletion. None occurs.
fn scalar(code: u32) -> Result<char, String> {
    char::from_u32(code).ok_or_else(|| {
        format!("`\\u{code:04X}` is a lone surrogate, which cannot occur in a Rust `str`")
    })
}

/// `lo..=hi` intersected with the Unicode scalar values.
///
/// **This is an identity on `&str` input, not a repair.** The haystack is always
/// a Rust `str`, so no code point in `D800..=DFFF` can ever appear in it; a
/// range that includes them matches exactly what the clamped range matches,
/// whether the class is negated or not. `yaml.key` is the pattern that forces
/// this — it excludes the whole surrogate block by hand, which ECMAScript needs
/// because a non-`u` regex runs over UTF-16 code units and Rust does not.
fn scalar_ranges(lo: u32, hi: u32) -> Vec<(char, char)> {
    const GAP: (u32, u32) = (0xD800, 0xDFFF);
    let mut out = Vec::new();
    let mut push = |a: u32, b: u32| {
        if a <= b {
            if let (Some(a), Some(b)) = (char::from_u32(a), char::from_u32(b)) {
                out.push((a, b));
            }
        }
    };
    push(lo, hi.min(GAP.0 - 1));
    push(lo.max(GAP.1 + 1), hi);
    out
}

/// One code point, spelled so that it is a literal in a `regex` character class.
///
/// Printable ASCII stays printable, because a generated file that spelled every
/// space as `\u{20}` would be unreadable and D14 wants a reviewable diff. The
/// seven characters `regex` gives meaning to inside a class are escaped, and
/// three of those (`[`, `&`, `~`) are meaningful to `regex` alone.
fn escaped(c: char) -> String {
    if matches!(c, '\\' | ']' | '^' | '-' | '[' | '&' | '~') {
        return format!("\\{c}");
    }
    if ('\u{20}'..='\u{7E}').contains(&c) {
        return c.to_string();
    }
    format!("\\u{{{:04X}}}", c as u32)
}

/// The group opener at `i`, or an error naming what was found.
///
/// Every form ECMAScript can write and `fancy-regex` accepts, and nothing else.
/// `(?<name>` and the two lookbehinds are here for completeness even though the
/// survey found none: a prismjs bump that introduces one should keep working,
/// while one that introduces `(?i)` — which ECMAScript cannot write at all —
/// should stop the generator.
fn group_opener(chars: &[char], i: usize) -> Result<&'static str, String> {
    if chars.get(i + 1) != Some(&'?') {
        return Ok("(");
    }
    let third = chars.get(i + 2).copied();
    match third {
        Some(':') => Ok("(?:"),
        Some('=') => Ok("(?="),
        Some('!') => Ok("(?!"),
        Some('<') => match chars.get(i + 3).copied() {
            Some('=') => Ok("(?<="),
            Some('!') => Ok("(?<!"),
            _ => Ok("(?<"),
        },
        other => Err(format!(
            "unrecognised group opener `(?{}`",
            other.map(String::from).unwrap_or_default()
        )),
    }
}

/// Length in `char`s of a valid `{n}` / `{n,}` / `{n,m}` at `i`, if there is one.
fn quantifier_len(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let digits = |j: &mut usize| {
        let start = *j;
        while chars.get(*j).is_some_and(char::is_ascii_digit) {
            *j += 1;
        }
        *j > start
    };
    if !digits(&mut j) {
        return None;
    }
    if chars.get(j) == Some(&',') {
        j += 1;
        digits(&mut j);
    }
    if chars.get(j) == Some(&'}') {
        Some(j + 1 - i)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// The dump
// ---------------------------------------------------------------------------

/// Run the Node dumper and parse its envelope.
///
/// `Ok(Err(reason))` is "the reference engine is not available", the same
/// three-way split every harness in this crate uses.
fn dump(repo_root: &Path) -> Result<Result<Value, String>, String> {
    let dir = repo_root.join("target").join("xtask");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let out_path = dir.join("grammars.json");
    let script = repo_root.join("tools").join("dump-prism-grammars.mjs");

    let output = match Command::new("node")
        .current_dir(repo_root)
        .arg(&script)
        .arg("--out")
        .arg(&out_path)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return Ok(Err(format!(
                "cannot run `node`: {e}. Install Node 20.19+ and check out the marktext clone."
            )));
        }
    };
    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if code == EXIT_UNIMPLEMENTED {
        return Ok(Err(stderr));
    }
    if code != 0 {
        return Err(format!("dump-prism-grammars.mjs exited {code}:\n{stderr}"));
    }
    let text = std::fs::read_to_string(&out_path)
        .map_err(|e| format!("cannot read {}: {e}", out_path.display()))?;
    Ok(Ok(serde_json::from_str(&text).map_err(|e| {
        format!("dump-prism-grammars.mjs produced invalid JSON: {e}")
    })?))
}

// ---------------------------------------------------------------------------
// The emitter
// ---------------------------------------------------------------------------

/// One pattern, in the shape the generated file holds it.
struct Emitted {
    source: String,
    lookbehind: bool,
    greedy: bool,
    class: String,
    inside: Option<usize>,
    /// `markup.tag[0]` — a comment on the emitted line, so a differential
    /// failure can be traced back to the grammar entry that produced it.
    origin: String,
}

/// Build the generated file's text.
///
/// The closure is walked from `ported` in sorted order and re-indexed, so the
/// output depends on the ported list and the reference clone's prismjs and on
/// nothing else — which is what makes "re-runnable and byte-identical" true
/// rather than aspirational.
fn render(envelope: &Value, ported: &[&str]) -> Result<(String, Stats), String> {
    let grammars = envelope
        .get("grammars")
        .and_then(Value::as_array)
        .ok_or("envelope has no `grammars`")?;
    let languages = envelope
        .get("languages")
        .and_then(Value::as_object)
        .ok_or("envelope has no `languages`")?;
    let manifest: Vec<&str> = envelope
        .get("manifest")
        .and_then(Value::as_array)
        .ok_or("envelope has no `manifest`")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let aliases = envelope
        .get("aliases")
        .and_then(Value::as_object)
        .ok_or("envelope has no `aliases`")?;

    // Deterministic breadth-first re-indexing from the sorted ported names.
    //
    // Breadth-first from a *sorted* seed, rather than depth-first from PORTED's
    // own order, so that the emitted indices depend on the ported set and the
    // reference clone and on nothing else — reordering `PORTED` must not rewrite
    // the file. `origin` is carried alongside for the emitted comments: a
    // grammar reachable only through `inside` is named by the **first** pattern
    // that reaches it, which is the shortest true path to it.
    let mut order: Vec<usize> = Vec::new();
    let mut origin: Vec<String> = Vec::new();
    let mut index_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut names: Vec<(&str, usize)> = Vec::new();
    let mut sorted_ported: Vec<&str> = ported.to_vec();
    sorted_ported.sort_unstable();
    for name in &sorted_ported {
        let old = languages
            .get(*name)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("`{name}` is in PORTED but Prism registers no grammar for it"))?
            as usize;
        let new = *index_of.entry(old).or_insert_with(|| {
            order.push(old);
            origin.push((*name).to_string());
            order.len() - 1
        });
        names.push((name, new));
    }
    let mut cursor = 0usize;
    while cursor < order.len() {
        let old = order[cursor];
        let here = origin[cursor].clone();
        cursor += 1;
        let rules = grammars
            .get(old)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("grammar {old} is missing"))?;
        for rule in rules {
            let rule_name = rule.get("name").and_then(Value::as_str).unwrap_or("?");
            for (j, pattern) in rule
                .get("patterns")
                .and_then(Value::as_array)
                .ok_or("a rule has no `patterns`")?
                .iter()
                .enumerate()
            {
                if let Some(inside) = pattern.get("inside").and_then(Value::as_u64) {
                    let inside = inside as usize;
                    if let std::collections::btree_map::Entry::Vacant(slot) = index_of.entry(inside)
                    {
                        slot.insert(order.len());
                        order.push(inside);
                        origin.push(format!("{here}.{rule_name}[{j}].inside"));
                    }
                }
            }
        }
    }

    let mut patterns: Vec<Emitted> = Vec::new();
    let mut rules_out: Vec<(String, usize, usize)> = Vec::new();
    let mut grammars_out: Vec<(usize, usize)> = Vec::new();
    let mut stats = Stats::default();

    for (new, &old) in order.iter().enumerate() {
        let origin = origin[new].clone();
        let rules = grammars[old].as_array().expect("checked above");
        let first_rule = rules_out.len();
        for rule in rules {
            let rule_name = rule
                .get("name")
                .and_then(Value::as_str)
                .ok_or("a rule has no `name`")?;
            let list = rule
                .get("patterns")
                .and_then(Value::as_array)
                .ok_or("a rule has no `patterns`")?;
            let first_pattern = patterns.len();
            for (j, pattern) in list.iter().enumerate() {
                let js = pattern
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or("a pattern has no `source`")?;
                let flags = pattern
                    .get("flags")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                for flag in flags.chars() {
                    // `g` is inert here: `matchPattern` assigns `lastIndex`
                    // before every `exec`, so a source-level `g` changes
                    // nothing. `i` and `m` are carried; anything else would
                    // change what the pattern means and there is none.
                    if !matches!(flag, 'g' | 'i' | 'm') {
                        return Err(format!(
                            "{origin}.{rule_name}[{j}]: unsupported flag `{flag}`"
                        ));
                    }
                }
                let where_ = format!("{origin}.{rule_name}[{j}]");
                let rust =
                    translate(js, flags.contains('i'), flags.contains('m')).map_err(|e| {
                        format!(
                            "{where_}: {e}
  source: /{js}/{flags}"
                        )
                    })?;
                // The check that makes this generator worth trusting: the
                // emitted pattern is compiled with the same engine the
                // interpreter will use, before it reaches the tree.
                fancy_regex::Regex::new(&rust).map_err(|e| {
                    format!(
                        "{where_}: translated pattern does not compile: {e}
  source: /{js}/{flags}
  rust:   {rust}"
                    )
                })?;
                stats.js_bytes += js.len();
                stats.rust_bytes += rust.len();
                let inside = pattern
                    .get("inside")
                    .and_then(Value::as_u64)
                    .map(|i| index_of[&(i as usize)]);
                let class = pattern
                    .get("alias")
                    .and_then(Value::as_str)
                    .unwrap_or(rule_name)
                    .to_string();
                patterns.push(Emitted {
                    source: rust,
                    lookbehind: pattern
                        .get("lookbehind")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    greedy: pattern
                        .get("greedy")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    class,
                    inside,
                    origin: where_,
                });
            }
            rules_out.push((
                rule_name.to_string(),
                first_pattern,
                patterns.len() - first_pattern,
            ));
        }
        grammars_out.push((first_rule, rules_out.len() - first_rule));
    }

    stats.grammars = grammars_out.len();
    stats.rules = rules_out.len();
    stats.patterns = patterns.len();

    // ---- text ----
    let mut f = String::with_capacity(stats.rust_bytes * 2 + 64 * 1024);
    f.push_str(&header(envelope, &sorted_ported, &stats));

    // Every field spelled out on every line, rather than functional-update
    // syntax over a default. Two reasons: `..Default::default()` is not usable
    // in a `static` initialiser without a `const` base, and — the real one — the
    // diff of a regeneration is what a human reviews, and a line that omits the
    // fields that happen to be false hides exactly the flags (`greedy`,
    // `lookbehind`) whose flipping changes tokenization the most.
    let _ = writeln!(
        f,
        "#[rustfmt::skip]
pub(crate) static PATTERNS: &[Pattern] = &["
    );
    for (i, p) in patterns.iter().enumerate() {
        let _ = writeln!(f, "    // {i} — {}", p.origin);
        let _ = writeln!(
            f,
            "    Pattern {{ source: {}, lookbehind: {}, greedy: {}, class: {:?}, inside: {} }},",
            raw(&p.source),
            p.lookbehind,
            p.greedy,
            p.class,
            match p.inside {
                Some(g) => format!("Some({g})"),
                None => "None".to_string(),
            }
        );
    }
    let _ = writeln!(f, "];\n");

    let _ = writeln!(
        f,
        "#[rustfmt::skip]
pub(crate) static RULES: &[Rule] = &["
    );
    for (name, first, count) in &rules_out {
        let _ = writeln!(
            f,
            "    Rule {{ name: {name:?}, first: {first}, count: {count} }},"
        );
    }
    let _ = writeln!(f, "];\n");

    let _ = writeln!(
        f,
        "#[rustfmt::skip]
pub(crate) static GRAMMARS: &[Grammar] = &["
    );
    for (i, (first, count)) in grammars_out.iter().enumerate() {
        let origin = &origin[i];
        let _ = writeln!(
            f,
            "    Grammar {{ first: {first}, count: {count} }}, // {i} — {origin}"
        );
    }
    let _ = writeln!(f, "];\n");

    names.sort_unstable();
    let _ = writeln!(
        f,
        "/// The ported languages: a Prism id, and the grammar it resolves to."
    );
    let _ = writeln!(
        f,
        "#[rustfmt::skip]
pub(crate) static LANGUAGES: &[(&str, u16)] = &["
    );
    for (name, index) in &names {
        let _ = writeln!(f, "    ({name:?}, {index}),");
    }
    let _ = writeln!(f, "];\n");

    let _ = writeln!(
        f,
        "/// Every id in `components.json`, sorted. **All 297, ported or not.**\n\
         ///\n\
         /// MarkText's `transformAliasToOrigin` answers *itself* for any word\n\
         /// that is a manifest id, before it ever looks at an alias table\n\
         /// (`utils/prism/loadLanguage.ts:23-54`), so resolution needs the whole\n\
         /// manifest even where highlighting needs sixteen grammars."
    );
    let _ = writeln!(
        f,
        "#[rustfmt::skip]
pub static MANIFEST: &[&str] = &["
    );
    let mut sorted_manifest = manifest.clone();
    sorted_manifest.sort_unstable();
    for id in &sorted_manifest {
        let _ = writeln!(f, "    {id:?},");
    }
    let _ = writeln!(f, "];\n");

    let _ = writeln!(
        f,
        "/// `alias → id`, sorted. First claimant in `components.json` key order\n\
         /// wins, and an alias shadowed by a real id is not here at all."
    );
    let _ = writeln!(
        f,
        "#[rustfmt::skip]
pub(crate) static ALIASES: &[(&str, &str)] = &["
    );
    let mut alias_rows: Vec<(&str, &str)> = aliases
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|v| (k.as_str(), v)))
        .collect();
    alias_rows.sort_unstable();
    for (alias, id) in &alias_rows {
        let _ = writeln!(f, "    ({alias:?}, {id:?}),");
    }
    let _ = writeln!(f, "];");

    stats.file_bytes = f.len();
    Ok((f, stats))
}

/// What a regeneration cost, printed on every run.
#[derive(Default)]
struct Stats {
    grammars: usize,
    rules: usize,
    patterns: usize,
    js_bytes: usize,
    rust_bytes: usize,
    file_bytes: usize,
}

/// A Rust raw string literal holding `s`, with as many `#` as it takes.
fn raw(s: &str) -> String {
    let mut hashes = 0usize;
    let mut run = 0usize;
    let mut after_quote = false;
    for c in s.chars() {
        match c {
            '"' => {
                after_quote = true;
                run = 0;
                hashes = hashes.max(1);
            }
            '#' if after_quote => {
                run += 1;
                hashes = hashes.max(run + 1);
            }
            _ => {
                after_quote = false;
                run = 0;
            }
        }
    }
    // A trailing `"` would terminate `r"…"`, and a `#` run needs one more.
    let pad = "#".repeat(hashes);
    format!("r{pad}\"{s}\"{pad}")
}

fn header(envelope: &Value, ported: &[&str], stats: &Stats) -> String {
    let engine = envelope
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let patches: Vec<&str> = envelope
        .get("patches")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut h = String::new();
    let _ = writeln!(
        h,
        "//! **Generated by `cargo xtask grammars`. Do not edit.**\n\
         //!\n\
         //! A snapshot of a *loaded* Prism, per docs/M3.md §5 D14. Every\n\
         //! `extend`, `insertBefore`, `RegExp(…)` construction and `.source`\n\
         //! concatenation in Prism's grammar files runs at load time and is\n\
         //! already resolved here; `rest` is merged; `inside` is an index\n\
         //! because the reference graph has cycles (`markup` → `javascript` →\n\
         //! `markup`). See `xtask/src/grammars.rs` for the JS → Rust regex\n\
         //! translation table and for why this is not all 297 languages.\n\
         //!\n\
         //! | | |\n\
         //! |---|---|\n\
         //! | engine | {engine} |"
    );
    for patch in &patches {
        let _ = writeln!(h, "//! | patch | {patch} |");
    }
    let _ = writeln!(
        h,
        "//! | ported languages | {} — {} |\n\
         //! | grammars (with the `inside` closure) | {} |\n\
         //! | rules / patterns | {} / {} |\n\
         //! | regex source, JS → Rust | {} B → {} B |",
        ported.len(),
        ported.join(", "),
        stats.grammars,
        stats.rules,
        stats.patterns,
        stats.js_bytes,
        stats.rust_bytes,
    );
    // `#[rustfmt::skip]` per item, on every static below, rather than a
    // file-level inner attribute: custom **inner** attributes are still
    // unstable, and rustfmt's `ignore` option is nightly-only. Without it
    // `cargo fmt --check` would rewrite a generated file on every run and the
    // `--check` comparison could never be byte-identical.
    let _ = writeln!(h, "\nuse crate::grammar::{{Grammar, Pattern, Rule}};\n");
    h
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `cargo xtask grammars [--check]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut check = false;
    let mut require_ts = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "--require-ts" => require_ts = true,
            other => return Err(format!("unrecognised argument: {other}")),
        }
    }

    println!("Prism grammar generator — docs/M3.md §5 D14");

    let envelope = match dump(repo_root)? {
        Ok(envelope) => envelope,
        Err(reason) => {
            if require_ts {
                return Err(format!(
                    "the Prism reference is unavailable and --require-ts was given:\n{reason}"
                ));
            }
            println!("Prism is unavailable — the committed grammars were not checked.");
            for line in reason.lines() {
                println!("  {line}");
            }
            println!("Pass --require-ts to make that a failure instead of a skip; CI does.");
            return Ok(0);
        }
    };

    let (text, stats) = render(&envelope, crate::highlight::PORTED)?;
    let path = GENERATED
        .iter()
        .fold(repo_root.to_path_buf(), |acc, part| acc.join(part));

    println!(
        "engine  {}",
        envelope
            .get("engine")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "emit    {} language(s), {} grammar(s) with the `inside` closure, {} rule(s), {} pattern(s)",
        crate::highlight::PORTED.len(),
        stats.grammars,
        stats.rules,
        stats.patterns
    );
    println!(
        "        regex source {} B of JS → {} B of Rust ({:+.1} %); {} totals {} B",
        stats.js_bytes,
        stats.rust_bytes,
        100.0 * (stats.rust_bytes as f64 / stats.js_bytes.max(1) as f64 - 1.0),
        path.strip_prefix(repo_root).unwrap_or(&path).display(),
        stats.file_bytes,
    );

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    // Normalised for the same reason every other check in this crate normalises:
    // a checkout that rewrote line endings is not a finding.
    if existing.replace("\r\n", "\n") == text {
        println!("check   the committed file is byte-identical to a fresh generation");
        return Ok(0);
    }
    if check {
        println!(
            "check   FAILED — {} differs from a fresh generation. Run `cargo xtask grammars`.",
            path.display()
        );
        if let Some((at, old, new)) = crate::diff::first_difference(
            &Value::String(existing.replace("\r\n", "\n")),
            &Value::String(text.clone()),
        ) {
            println!("        at {at}");
            let show = |s: &str| s.chars().take(200).collect::<String>();
            println!("        committed: {}", show(&old));
            println!("        fresh:     {}", show(&new));
        }
        return Ok(1);
    }
    std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!("write   {}", path.display());
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The translations the table in [`translate`] promises.
    #[test]
    fn the_dialect_gap_is_closed_where_it_is_claimed_to_be() {
        let cases: &[(&str, &str)] = &[
            (r"\/", "/"),
            (r"\d", "[0-9]"),
            (r"\w+", "[0-9A-Za-z_]+"),
            (r"[\s\S]", "(?s:.)"),
            (r"[^]", "(?s:.)"),
            (r"\0", r"\x00"),
            (r"\u00e9", r"\u{00E9}"),
            (r"\x41", "A"),
            (r"a{2,3}", "a{2,3}"),
            // Not a quantifier, so a literal brace in both readings.
            (r"a{x", r"a\{x"),
            (r"[\w-]", r"[0-9A-Za-z_\-]"),
            (r"(a)\1", r"(a)\1"),
            // `&&`, `--` and `~~` are set operators to `regex` and literals to
            // ECMAScript. Parsing the class into items rather than copying it
            // through is what makes them survive as literals.
            (r"[a&&z]", r"[a\&\&z]"),
            // The surrogate clamp. `yaml.key` excludes the whole block by hand,
            // which ECMAScript needs because a non-`u` regex runs over UTF-16
            // code units; on a Rust `str` those code points cannot occur, so
            // dropping the range matches exactly the same text.
            (r"[a\ud800-\udfffz]", "[az]"),
            (r"[\ud7ff-\ue000]", r"[\u{D7FF}\u{E000}]"),
        ];
        for (js, want) in cases {
            let got = translate(js, false, false).unwrap_or_else(|e| panic!("/{js}/: {e}"));
            assert_eq!(&got, want, "/{js}/");
        }
        assert_eq!(translate("a", true, true).unwrap(), "(?im:a)");
    }

    /// Every construct the generator refuses, refused.
    ///
    /// This is the half of D14 that matters: *"failing loudly on any construct
    /// it does not recognise rather than emitting something that compiles and
    /// is wrong."* Each of these **would** compile as something else.
    #[test]
    fn unrecognised_constructs_fail_rather_than_compiling_to_something_else() {
        for js in [
            r"\p{L}",   // identity escape in ECMAScript, a Unicode property in `regex`
            r"\cA",     // a control escape fancy-regex has no spelling for
            r"\01",     // legacy octal
            r"[a--z]",  // `a`-to-`-` is a reversed range in both dialects
            r"\u{41}",  // not a code-point escape without the `u` flag
            r"[]",      // matches nothing in ECMAScript, a parse error in `regex`
            r"[a-\dz]", // a shorthand cannot be a range endpoint
            r"\ud800",  // a lone surrogate outside a class has nothing to clamp
            "\\",       // trailing backslash
        ] {
            assert!(
                translate(js, false, false).is_err(),
                "/{js}/ should not translate"
            );
        }
    }

    /// A `[` inside a class is a literal in ECMAScript and a nested class in
    /// `regex`, which is the kind of difference that compiles and is wrong.
    ///
    /// The second case is the one that catches a reader out. ECMAScript has
    /// **no** "a leading `]` is a literal" rule — that is POSIX and Perl — so
    /// `[]]` is the empty class followed by a literal `]`, and `[]` matches
    /// nothing at all. The generator refuses it rather than reproducing either
    /// reading; no Prism pattern contains one.
    #[test]
    fn a_bracket_inside_a_class_is_escaped() {
        assert_eq!(translate(r"[[]", false, false).unwrap(), r"[\[]");
        assert!(translate(r"[]]", false, false).is_err());
        assert_eq!(translate(r"[\]]", false, false).unwrap(), r"[\]]");
    }

    /// Raw-string emission survives a pattern containing quotes and hashes.
    #[test]
    fn raw_strings_pick_enough_hashes() {
        assert_eq!(raw("a"), "r\"a\"");
        assert_eq!(raw("\""), "r#\"\"\"#");
        assert_eq!(raw("\"#"), "r##\"\"#\"##");
    }
}
