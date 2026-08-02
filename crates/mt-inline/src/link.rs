//! `parseSrcAndTitle` and `correctUrl` — the link half of
//! `inlineRenderer/utils.ts` — plus `findClosingBracket`, which `correctUrl`
//! calls and which lives in `utils/marked/utils.ts` because marked wrote it.
//!
//! [`crate::emphasis`] is the other half of `utils.ts`. The two share the
//! JavaScript `\s` class and nothing else.
//!
//! # `correctUrl` mutates its argument, and the mutation is read four times
//!
//! This is the part of S3 that does not survive a naïve translation.
//! `correctUrl(token)` takes the *match array* and rewrites three of its
//! entries in place — `token[0]`, `token[4]` and `token[5]` — and
//! `lexer.ts:317` and `:355` call it **before** the guard that reads them:
//!
//! ```js
//! const imageTo = state.inlineRules.image.exec(state.src);
//! correctUrl(imageTo);
//! if (!(imageTo && isLengthEven(imageTo[3]) && isLengthEven(imageTo[5])))
//!     return false;
//! ```
//!
//! So `isLengthEven(imageTo[5])` tests the **rewritten** group 5, which
//! `correctUrl` may have just invented out of the tail of group 4; and
//! `imageTo[0].length` — which becomes both the token's `raw` and the number
//! of bytes consumed — is the **rewritten** length. Rust has no mutable match
//! array, so the rewrite is modelled as a [`Destination`] returned by
//! [`correct_url`] and then used *everywhere* the match would have been.
//! Validating before correcting is the silent version of this bug: it accepts
//! and rejects the right links, and truncates none of them.
//!
//! # Group 5 is empty until `correctUrl` fills it, and that is the design
//!
//! `image` is `^(!\[)(.*?)(\\*)\]\((.*)(\\*)\)` and `link` ends the same way:
//! a greedy `(.*)` immediately followed by `(\\*)`. The greedy group eats the
//! backslashes, so **group 5 is always empty as the regex leaves it** — not
//! usually, always. The proof is short enough to keep:
//!
//! > `(.*)` gives back one character at a time from the right, so the engine
//! > tries the largest end position first. At `p` it needs `(\\*)` — a run of
//! > backslashes — then `)`. Take `q`, the index of the last `)` in the
//! > input: at `p = q` the run is empty and `\)` matches, so `p = q`
//! > succeeds. No `p > q` can, because there is no `)` left to match. The
//! > engine therefore stops at `p = q` with group 5 empty, and group 4 running
//! > to the last `)` on the line.
//!
//! Group 5 exists for what [`correct_url`] does with it: after truncating the
//! destination at the *matching* `)` rather than the last one, `/(\\+)$/`
//! re-splits any backslash run off the end of what is left. That is the whole
//! purpose of the group, and "simplifying" it away removes the only path by
//! which `isLengthEven(imageTo[5])` can ever be false.
//!
//! `the_regex_never_fills_group_five_itself` asserts the proof rather than
//! trusting it.
//!
//! # Every returned value is a span, and that is checked rather than assumed
//!
//! `token.rs` declares `src: Span` and `title: Option<Span>`, so
//! [`parse_src_and_title`] has to return sub-slices of its input. Both are:
//! the title is a capture group of a suffix of the input, and the src is a
//! substring of it trimmed at both ends. The trim is where an off-by-one turns
//! into a panic rather than a wrong answer, so [`js_trim`] counts whole `char`s
//! outward from a known boundary and never slices.

use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::emphasis::is_unicode_whitespace;
use crate::rules::{self, compile, exec};
use crate::token::Span;

// ---------------------------------------------------------------------------
// JavaScript's trim
// ---------------------------------------------------------------------------

/// One character of JavaScript's `\s`.
fn is_js_whitespace(ch: char) -> bool {
    is_unicode_whitespace(Some(ch))
}

/// `String.prototype.trim`, as a span operation.
///
/// **Not `str::trim`.** ECMA-262 trims `WhiteSpace ∪ LineTerminator`, which is
/// the same set as `\s` and therefore the same M1.md §4 C2 disagreement:
/// JavaScript trims U+FEFF and Rust does not, Rust trims U+0085 and JavaScript
/// does not. `parseSrcAndTitle` calls `trim` three times, once on a path that
/// produces a link's `href`, so both disagreements are reachable from a
/// document — `[a](\u{feff}url)` and `[a](\u{85}url)` land on opposite sides.
///
/// Returns a span rather than a `&str` because the caller needs the offsets,
/// and walks whole `char`s outward from `span`'s own boundaries so the result
/// is a sub-slice by construction.
fn js_trim(origin: &str, span: Span) -> Span {
    let text = span.of(origin);

    let mut start = 0;
    for ch in text.chars() {
        if !is_js_whitespace(ch) {
            break;
        }
        start += ch.len_utf8();
    }

    let mut end = text.len();
    for ch in text[start..].chars().rev() {
        if !is_js_whitespace(ch) {
            break;
        }
        end -= ch.len_utf8();
    }

    Span::new(span.start + start, span.start + end)
}

// ---------------------------------------------------------------------------
// findClosingBracket
// ---------------------------------------------------------------------------

/// `findClosingBracket(str, '()')` (`utils/marked/utils.ts:2`), returning a
/// **byte** index.
///
/// Walks the destination counting nesting depth and returns the position of
/// the first `)` that closes a bracket nobody opened. That is the character
/// the destination really ends at, as opposed to the last `)` on the line,
/// which is where the greedy `(.*)` stopped.
///
/// muya passes the bracket pair as a two-character string; here it is a
/// `(char, char)`, because the only call passes `'()'` and a pair cannot be
/// the wrong length.
///
/// # The backslash skip is a code-unit skip in JavaScript
///
/// `if (str[i] === '\\') { i++ }` advances one **UTF-16 code unit**, and the
/// loop's own `i++` then advances another. For an escaped BMP character that
/// skips exactly the character; for an escaped astral one it skips only the
/// high surrogate, and the low surrogate is then examined and matches none of
/// the three tests. Skipping a whole `char` here is therefore the same
/// behaviour, not merely the closest available one — the difference is a
/// character that could never have been `\`, `(` or `)` anyway.
///
/// # The early return is an optimisation, not a rule
///
/// `if (!str.includes(b[1])) return -1` cannot change the answer: the loop
/// returns a position only from the `b[1]` branch, so a string with no `)` in
/// it returns `-1` either way. Kept because muya has it and because it is the
/// common case — most destinations contain no parenthesis at all.
pub(crate) fn find_closing_bracket(text: &str, brackets: (char, char)) -> Option<usize> {
    let (open, close) = brackets;
    if !text.contains(close) {
        return None;
    }

    // `i64` rather than `i32`: the level is a difference of counts and goes
    // negative by design, and a document is not bounded by anything smaller.
    let mut level: i64 = 0;
    let mut chars = text.char_indices();

    while let Some((i, ch)) = chars.next() {
        if ch == '\\' {
            chars.next();
        } else if ch == open {
            level += 1;
        } else if ch == close {
            level -= 1;
            if level < 0 {
                return Some(i);
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// correctUrl
// ---------------------------------------------------------------------------

/// What [`correct_url`] would have written back into the match array.
///
/// The three entries `correctUrl` mutates, as spans. A caller must read these
/// and not the capture groups they came from — see the module docs for why the
/// order matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Destination {
    /// `token[0]` — the whole match, truncated to end just past the `)` that
    /// really closes the destination. This is the token's `raw` **and** the
    /// number of bytes the handler consumes.
    pub whole: Span,
    /// `token[4]` — the destination and title, truncated to the same place and
    /// with any trailing backslash run split off into [`backlash`](Self::backlash).
    pub url: Span,
    /// `token[5]` — the backslash run `/(\\+)$/` peeled off the end of the
    /// truncated destination, or an empty span where it would have been. Only
    /// this field can make `isLengthEven(token[5])` false; see the module docs.
    pub backlash: Span,
}

/// `correctUrl` (`utils.ts:341`).
///
/// The greedy `(.*)` in `image` and `link` runs to the **last** `)` on the
/// line, which is wrong the moment a line holds anything else in parentheses:
/// marktext #1169, where `see ![alt](first.png) and also (parens) here`
/// swallowed the sentence. [`find_closing_bracket`] finds the `)` that
/// actually closes the destination, and everything after it is given back to
/// the tokenizer loop.
///
/// Returns the three corrected spans unchanged when there is nothing to
/// correct, which is muya's "`lastParenIndex === -1`, leave the array alone".
pub(crate) fn correct_url(origin: &str, whole: Span, url: Span, backlash: Span) -> Destination {
    let Some(last_paren) = find_closing_bracket(url.of(origin), ('(', ')')) else {
        return Destination {
            whole,
            url,
            backlash,
        };
    };

    // muya: `const len = token[0].length - (token[4].length - lastParenIndex)`.
    // Written as muya writes it rather than as `url.start + last_paren + 1`,
    // which is the same position only because group 5 is empty on the way in
    // (module docs). Deriving it from the whole match's length instead means
    // this line stays correct if that ever stops being true.
    let len = whole.len() - (url.len() - last_paren);
    let whole = Span::new(whole.start, whole.start + len);

    let truncated = Span::new(url.start, url.start + last_paren);
    let run = trailing_backslash_run(origin, truncated);

    Destination {
        whole,
        url: Span::new(truncated.start, run.start),
        // muya's `else` branch assigns the literal `''`, which has no
        // position. The end of the truncated destination is where the group
        // would have sat had the regex produced it, and it is where the
        // uncorrected group 5 sits too.
        backlash: run,
    }
}

/// `/(\\+)$/` against `span`, as the span of the run — empty at the end when
/// there is none.
///
/// A backslash is ASCII, so counting bytes backwards cannot land inside a
/// character.
fn trailing_backslash_run(origin: &str, span: Span) -> Span {
    let bytes = span.of(origin).as_bytes();
    let mut start = span.end;
    while start > span.start && bytes[start - span.start - 1] == b'\\' {
        start -= 1;
    }
    Span::new(start, span.end)
}

// ---------------------------------------------------------------------------
// encodeURI, over the only argument it is ever given
// ---------------------------------------------------------------------------

/// `encodeURI(run)`, where `run` is a run of backslashes.
///
/// `tryImage` builds its render-facing `attrs` as
/// `imageSrc + encodeURI(imageTo[5])` and `imageTo[2] + encodeURI(imageTo[3])`
/// (`lexer.ts:330`), and those are the **only** two `encodeURI` calls in the
/// tokenizer. Both arguments are backslash runs and nothing else:
///
/// - group 3 is the pattern's own `(\\*)`, untouched;
/// - group 5 is either that same `(\\*)` — provably empty, see the module docs
///   — or what [`correct_url`] peels off with `/(\\+)$/`, which is a run of
///   backslashes by construction.
///
/// So the only character `encodeURI` can ever be handed here is U+005C, and
/// `encodeURI` maps it to `%5C`: it passes through
/// `A-Za-z0-9;,/?:@&=+$-_.!~*'()#` and percent-encodes everything else, and a
/// backslash is not in that set. A general `encodeURI` would be dead code with
/// a large surface — UTF-8 expansion, surrogate handling, the unreserved set —
/// so this is the reachable case, stated and implemented.
///
/// `every_encode_uri_argument_is_a_run_of_backslashes` is the proof that the
/// premise holds for the two capture groups.
pub(crate) fn encode_backslash_run(origin: &str, run: Span) -> String {
    debug_assert!(
        run.of(origin).bytes().all(|b| b == b'\\'),
        "encodeURI is only reachable with a backslash run here; got {:?}",
        run.of(origin)
    );
    "%5C".repeat(run.len())
}

// ---------------------------------------------------------------------------
// parseSrcAndTitle
// ---------------------------------------------------------------------------

/// `/^[^ ]+ +/` — the prefix `parseSrcAndTitle` strips to find the title.
///
/// **The space is a literal U+0020, not `\s`**, in both halves. That is a
/// second M1.md §4 C2 site in a new place, and it is the direction that does
/// *not* have a class to reuse: the split above it is `\s`-based and this is
/// not, so a destination separated from its title by a tab takes the
/// `parts.length !== 1` branch and then fails to strip anything, leaving
/// `rawTitle === text`.
///
/// `[^ ]` matches a line terminator in both engines — it is a negated class,
/// not a `.` — so no C2 rewrite applies to it.
static LEFT_WORD_REG: LazyLock<Regex> = LazyLock::new(|| compile("LEFT_WORD_REG", "^[^ ]+ +"));

/// `TITLE_REG` (`utils.ts:210`) — `/^('|")(.*?)\1$/`.
///
/// Two things in six characters of pattern. The `\1` is a backreference, so a
/// title must close with the quote that opened it and `'x"` is not a title.
/// The `.` is JavaScript's, so a title may not span a line terminator; Rust's
/// `.` excludes only `\n`, which would let `"a\rb"` through.
///
/// The lazy `(.*?)` is not lazy in effect: `$` forces it to the last possible
/// quote, so `"a"b"` has the title `a"b`.
static TITLE_REG: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        "TITLE_REG",
        &format!("^('|\")({}*?)\\1$", rules::JS_DOT_CLASS),
    )
});

/// JavaScript's `\s`, unanchored — `text.split(/\s+/)` reduced to the one
/// question the caller asks of it. See [`parse_src_and_title`].
static WHITESPACE_REG: LazyLock<Regex> =
    LazyLock::new(|| compile("split(/\\s+/)", rules::JS_WHITESPACE_CLASS));

/// `parseSrcAndTitle` (`utils.ts:200`) — split `(url "title")`'s inside into a
/// destination and a title.
///
/// Returns spans into `origin`, both sub-slices of `text`. `None` is muya's
/// `''`, and it covers both "no title was recognised" and "the title was
/// recognised and is empty" — muya's own `if (title)` collapses those two, so
/// `[a](u "")` has the src `u ""` and no title in both engines.
///
/// # `parts.length === 1` is a question about whitespace, not about parts
///
/// muya opens with `text.split(/\s+/)` and then only ever asks whether the
/// result has one element. A JavaScript split produces *matches + 1* elements,
/// so one element means zero matches, which means **`text` contains no `\s`
/// character at all**. That is the test, and it is what is written here — no
/// splitting, no allocation, and no chance of disagreeing with JavaScript
/// about what an empty leading or trailing part is.
///
/// It also removes the trap: `" url"` has *two* parts, because the split at
/// index 0 yields an empty first element. A single-token destination that
/// happens to start with a space therefore takes the *other* branch, strips no
/// prefix (`^[^ ]+` cannot match at a space), fails `TITLE_REG`, and lands on
/// `text.trim()` — the same answer by a different route, but only by luck, and
/// `a_leading_space_takes_the_two_part_branch` pins it.
pub(crate) fn parse_src_and_title(origin: &str, text: Span) -> (Span, Option<Span>) {
    let text_str = text.of(origin);

    if exec(&WHITESPACE_REG, text_str).is_none() {
        return (js_trim(origin, text), None);
    }

    // `const rawTitle = text.replace(/^[^ ]+ +/, '')` — a suffix of `text`,
    // and `text` itself when the pattern does not match.
    let raw_title = match exec(&LEFT_WORD_REG, text_str) {
        Some(caps) => Span::new(text.start + caps.get(0).expect("group 0").end(), text.end),
        None => text,
    };

    // `if (rawTitle && TITLE_REG.test(rawTitle))`, then `if (title)`. An empty
    // `rawTitle` cannot match `TITLE_REG` anyway — the pattern needs two
    // characters — so the truthiness guard is folded into the match.
    let title = exec(&TITLE_REG, raw_title.of(origin))
        .and_then(|caps| caps.get(2))
        .map(|m| Span::new(raw_title.start + m.start(), raw_title.start + m.end()))
        .filter(|title| !title.is_empty());

    match title {
        // `src = text.substring(0, text.length - rawTitle.length).trim()`.
        // `rawTitle` is a suffix, so that prefix ends where it begins.
        Some(title) => (
            js_trim(origin, Span::new(text.start, raw_title.start)),
            Some(title),
        ),
        None => (js_trim(origin, text), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{RuleId, rule};

    fn parse(text: &str) -> (&str, Option<&str>) {
        let (src, title) = parse_src_and_title(text, Span::new(0, text.len()));
        (src.of(text), title.map(|t| t.of(text)))
    }

    // -----------------------------------------------------------------------
    // findClosingBracket
    // -----------------------------------------------------------------------

    #[test]
    fn the_closing_bracket_is_the_first_one_nobody_opened() {
        let find = |s: &str| find_closing_bracket(s, ('(', ')'));
        assert_eq!(find("first.png) and also (parens"), Some(9));
        assert_eq!(find("path/to/(file).png"), None, "balanced, so no closer");
        assert_eq!(find("no parens at all"), None);
        assert_eq!(find(""), None);
        assert_eq!(find(")"), Some(0));
        assert_eq!(find("(())"), None, "still balanced");
        assert_eq!(find("(()))x"), Some(4));
    }

    /// The backslash skip, at a multi-byte character. JavaScript skips one
    /// code unit; this skips one `char`. The two agree because the character
    /// after a `\` is never one of the three the loop tests for.
    #[test]
    fn an_escaped_bracket_is_skipped_whatever_its_width() {
        let find = |s: &str| find_closing_bracket(s, ('(', ')'));
        assert_eq!(find(r"a\) b"), None, "the only `)` is escaped");
        assert_eq!(find(r"a\)) b"), Some(3));
        // A `\` before a four-byte character: the character is skipped whole
        // and the `)` after it is still found at its byte offset.
        assert_eq!(find("a\\\u{1f600})"), Some(6));
        // …and a `\` at the very end skips past the end without panicking.
        assert_eq!(find(r")\"), Some(0));
        assert_eq!(find(r"x\"), None);
    }

    /// The early return and the loop must agree, because the early return is
    /// justified only by their agreeing.
    #[test]
    fn the_includes_short_circuit_never_changes_the_answer() {
        for text in ["", "abc", r"a\(b", "(((", r"\\\\", "中文"] {
            assert_eq!(
                find_closing_bracket(text, ('(', ')')),
                None,
                "no `)` in {text:?}, so both paths must return None"
            );
        }
    }

    // -----------------------------------------------------------------------
    // correctUrl
    // -----------------------------------------------------------------------

    /// The proof in the module docs, executed: the greedy `(.*)` leaves group
    /// 5 empty for every input, so `isLengthEven(token[5])` is vacuous until
    /// `correctUrl` runs.
    #[test]
    fn the_regex_never_fills_group_five_itself() {
        for id in [RuleId::Image, RuleId::Link] {
            for src in [
                r"![a](b)",
                r"![a](b\)",
                r"![a](b\\)",
                r"![a](b\\\)",
                r"![a](b) (c)",
                r"![a](b\\) (c\\)",
                r"[a](b\\)",
                r"[a](b\\) and (c)",
            ] {
                let Some(caps) = exec(rule(id), src) else {
                    continue;
                };
                let five = caps.get(5).expect("group 5 always participates");
                assert_eq!(
                    five.as_str(),
                    "",
                    "`{}` filled group 5 on {src:?}; the correctUrl proof is wrong",
                    id.name()
                );
            }
        }
    }

    /// #1169 in one call: the destination stops at the matching `)` and the
    /// whole match is truncated with it, which is what makes the rest of the
    /// line re-enter the tokenizer loop.
    #[test]
    fn a_destination_is_truncated_at_the_paren_that_closes_it() {
        //          0123456789
        let origin = "![a](b) (c)";
        // What the regex produces: whole `![a](b) (c)`, group 4 `b) (c`.
        let corrected = correct_url(
            origin,
            Span::new(0, 11),
            Span::new(5, 10),
            Span::new(10, 10),
        );
        assert_eq!(corrected.whole.of(origin), "![a](b)");
        assert_eq!(corrected.url.of(origin), "b");
        assert_eq!(corrected.backlash.of(origin), "");
    }

    /// …and the `/(\\+)$/` re-split, which is the only thing that ever makes
    /// group 5 non-empty.
    #[test]
    fn a_trailing_backslash_run_is_split_off_the_truncated_destination() {
        //           0123456789012
        let origin = r"[a](b\\) c)";
        // whole = the lot, group 4 = `b\\) c`.
        let whole = Span::new(0, origin.len());
        let url = Span::new(4, origin.len() - 1);
        let corrected = correct_url(origin, whole, url, Span::empty_at(url.end));

        assert_eq!(corrected.whole.of(origin), r"[a](b\\)");
        assert_eq!(corrected.url.of(origin), "b");
        assert_eq!(corrected.backlash.of(origin), r"\\");
        assert!(
            crate::emphasis::is_length_even(Some(corrected.backlash)),
            "two backslashes: the link survives"
        );

        // An odd run is what the gate is for.
        let origin = r"[a](b\) c)";
        let whole = Span::new(0, origin.len());
        let url = Span::new(4, origin.len() - 1);
        let corrected = correct_url(origin, whole, url, Span::empty_at(url.end));
        assert_eq!(
            corrected.backlash.of(origin),
            "",
            "an escaped `)` is skipped, so this destination is not truncated at it"
        );
    }

    #[test]
    fn a_destination_with_no_unbalanced_paren_is_left_exactly_alone() {
        let origin = "![a](path/to/(file).png)";
        let whole = Span::new(0, origin.len());
        let url = Span::new(5, origin.len() - 1);
        let backlash = Span::empty_at(url.end);
        let corrected = correct_url(origin, whole, url, backlash);
        assert_eq!(
            corrected,
            Destination {
                whole,
                url,
                backlash
            }
        );
    }

    // -----------------------------------------------------------------------
    // encodeURI
    // -----------------------------------------------------------------------

    /// The premise the narrow `encodeURI` rests on, over the two groups that
    /// reach it: whatever the input, groups 3 and 5 of `image` are runs of
    /// backslashes.
    #[test]
    fn every_encode_uri_argument_is_a_run_of_backslashes() {
        for src in [
            r"![a](b)",
            r"![a\\](b)",
            r"![a\\\\](b\\)",
            r"![](x)",
            r"![a](b\\) (c)",
            "![中文](图片.png)",
        ] {
            let Some(caps) = exec(rule(RuleId::Image), src) else {
                continue;
            };
            for group in [3, 5] {
                let text = caps.get(group).expect("participates").as_str();
                assert!(
                    text.bytes().all(|b| b == b'\\'),
                    "group {group} of `image` on {src:?} is {text:?}"
                );
            }
        }
    }

    #[test]
    fn a_backslash_encodes_to_percent_five_c() {
        let origin = r"\\\";
        assert_eq!(encode_backslash_run(origin, Span::new(0, 0)), "");
        assert_eq!(encode_backslash_run(origin, Span::new(0, 1)), "%5C");
        assert_eq!(encode_backslash_run(origin, Span::new(0, 3)), "%5C%5C%5C");
    }

    // -----------------------------------------------------------------------
    // parseSrcAndTitle
    // -----------------------------------------------------------------------

    #[test]
    fn a_destination_with_no_whitespace_is_the_whole_text() {
        assert_eq!(parse("http://example.com"), ("http://example.com", None));
        assert_eq!(parse(""), ("", None));
        assert_eq!(parse("path/to/(file).png"), ("path/to/(file).png", None));
    }

    #[test]
    fn a_quoted_title_is_split_off_and_the_quotes_are_dropped() {
        assert_eq!(
            parse(r#"http://example.com "Example title""#),
            ("http://example.com", Some("Example title"))
        );
        assert_eq!(
            parse("http://example.com 'Example title'"),
            ("http://example.com", Some("Example title"))
        );
        // `\1` — the closing quote must be the one that opened.
        assert_eq!(
            parse("http://example.com 'mismatched\""),
            ("http://example.com 'mismatched\"", None)
        );
    }

    /// An empty title is falsy in JavaScript, so muya takes the *no title*
    /// branch and the src keeps the quotes. Surprising, faithful, and the
    /// reason `title` is an `Option` rather than a possibly-empty `Span`.
    #[test]
    fn an_empty_title_is_no_title_and_the_quotes_stay_in_the_src() {
        assert_eq!(parse(r#"u """#), (r#"u """#, None));
        assert_eq!(parse("u ''"), ("u ''", None));
    }

    /// The `parts.length === 1` trap: a leading space makes a single-token
    /// destination take the two-part branch.
    #[test]
    fn a_leading_space_takes_the_two_part_branch() {
        // `^[^ ]+ +` cannot match at a space, so nothing is stripped and
        // `rawTitle === text`; `TITLE_REG` then fails on the leading space.
        assert_eq!(parse(" url"), ("url", None));
        // …and with a title, the branch is the one that matters: the prefix
        // strip still fails, so the whole thing is trimmed as a src.
        assert_eq!(parse(r#" url "t""#), (r#"url "t""#, None));
    }

    /// The prefix strip uses a literal space; the branch test uses `\s`. A tab
    /// therefore chooses the two-part branch and then strips nothing.
    #[test]
    fn a_tab_separator_is_whitespace_to_the_split_and_not_to_the_strip() {
        assert_eq!(parse("u\t\"t\""), ("u\t\"t\"", None));
        assert_eq!(parse("u \"t\""), ("u", Some("t")));
    }

    /// M1.md §4 C2 at the trim: the two engines disagree about U+FEFF and
    /// U+0085 in opposite directions, and both are reachable through a link
    /// destination.
    #[test]
    fn the_trim_is_javascripts_and_not_rusts() {
        // U+FEFF is whitespace to JavaScript, so it is trimmed off the href.
        // `str::trim` would leave it, and the link would open the wrong URL.
        let src = "\u{feff}url\u{feff}";
        assert_eq!(parse(src), ("url", None), "U+FEFF must be trimmed");
        // U+0085 is not whitespace to JavaScript, so it stays. `str::trim`
        // would remove it. Note it is not `\s` either, so this input takes the
        // no-whitespace branch — `trim` is still the function under test.
        let src = "\u{85}url\u{85}";
        assert_eq!(parse(src), (src, None), "U+0085 must be kept");
    }

    /// The title is a capture group of a suffix of the input and the src is a
    /// trimmed prefix of it, so both are sub-slices. Asserted through the
    /// offsets rather than the text, because that is the property `token.rs`
    /// relies on when it declares them as `Span`.
    #[test]
    fn both_returned_spans_are_sub_slices_of_the_input() {
        let origin = r#"xx[a](http://e.com "T")yy"#;
        // The destination group of that link, by hand: `http://e.com "T"`.
        let text = Span::new(6, 22);
        let (src, title) = parse_src_and_title(origin, text);
        assert!(text.contains_span(src));
        assert!(text.contains_span(title.expect("a title")));
        assert_eq!(src.of(origin), "http://e.com");
        assert_eq!(title.expect("a title").of(origin), "T");
    }

    /// A title may not span a line terminator — JavaScript's `.`, not Rust's.
    #[test]
    fn a_title_may_not_contain_a_line_terminator() {
        assert_eq!(parse("u \"a\nb\""), ("u \"a\nb\"", None));
        assert_eq!(parse("u \"a\rb\""), ("u \"a\rb\"", None));
        assert_eq!(parse("u \"a\u{2028}b\""), ("u \"a\u{2028}b\"", None));
        assert_eq!(parse("u \"a b\""), ("u", Some("a b")));
    }

    /// `$` forces the lazy group to the last quote, so an interior quote of
    /// the same kind is part of the title.
    #[test]
    fn an_interior_quote_belongs_to_the_title() {
        assert_eq!(parse(r#"u "a"b""#), ("u", Some(r#"a"b"#)));
    }
}
