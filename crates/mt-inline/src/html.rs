//! `getAttributes` (`utils.ts:172`) without a DOM — **M1.md §5 D2, decided at
//! S4.**
//!
//! # What D2 asked, and what the evidence said
//!
//! D2 recommended *"a hand-written scanner over the open tag implementing the
//! HTML5 attribute-tokenization states"*, with `html5ever` as the escape hatch
//! if the scanner diverged on real corpus content. It says outright that the
//! recommendation predates knowing what the corpus contains. Both halves of it
//! turned out to be aimed at the wrong problem, and the measurements are
//! recorded here because the conclusion is not guessable from the source.
//!
//! **The reference is not an HTML5 tokenizer.** Every muya spec file carries
//! `// @vitest-environment happy-dom`, so `getAttributes`'s `DOMParser` is
//! happy-dom's. happy-dom parses a start tag's attributes with *one regular
//! expression* (`html-parser/HTMLParser.js`, `ATTRIBUTE_REGEXP`) — four
//! alternatives, applied globally to the text between the tag name and the
//! tag's end. It is not the spec's state machine and it does not agree with
//! it: `<div class=a/>` gives `class="a"` here and `class="a/"` in a
//! spec-conformant parser. So [`parse_attribute_string`] ports *that regex*,
//! alternative by alternative, rather than the HTML5 states.
//!
//! **`html5ever` would not have helped.** The escape hatch's whole value would
//! be agreeing with the reference, and it cannot: it is spec-conformant, and
//! happy-dom is not — measured, it keeps a `<colgroup>` and a `<frame>` that
//! the spec's "in body" insertion mode discards, and its attribute regex
//! differs as above. Swapping a small hand-written disagreement for a large
//! dependency's *different* disagreement is not a trade worth making, so
//! `html5ever` is rejected rather than deferred. `mt-inline` stays
//! dependency-free apart from `fancy-regex`.
//!
//! **The hard part is tree construction, not attribute tokenization.** Over
//! 14,035 cases — every distinct `html_tag` match in `bench/corpus/` and
//! `spec/fixtures/` (1199 of them) plus a synthetic sweep of every HTML
//! element name against 46 attribute shapes and 14 content shapes — the
//! attribute tokenizer disagreed with happy-dom **zero** times. Every
//! disagreement was the *element*: which one `body.firstElementChild` is.
//!
//! # `body.firstElementChild` without a tree
//!
//! `getAttributes` parses the **whole match** — content and close tag
//! included, not the open tag — and reads `body.firstElementChild`. That is
//! `null` for a class of tag names the `html_tag` regex happily matches, and
//! `null` makes `tryHtmlTag` fail, so `<td class="x">y</td>` is **not** an
//! `html_tag` in muya; it is text. A scanner that only reads the open tag gets
//! every one of those wrong.
//!
//! Three mechanisms produce it, all measured against happy-dom rather than
//! recalled from the HTML spec — which would have been wrong twice over, since
//! happy-dom is not conformant and the spec does *not* hoist `<meta>`,
//! `<link>` or `<base>` out of the body the way one might remember:
//!
//! 1. **[`DROPPED_IN_BODY`]** — eleven names whose element happy-dom refuses
//!    to append under `<body>`. The element is discarded and parsing
//!    continues, so a later element in the *content* takes the slot:
//!    `<td>b <code>|</code> az</td>` returns `{}` (the `code`'s whitelisted
//!    attributes, of which there are none) rather than `null`, and that is a
//!    real corpus input — CommonMark example 148. [`first_element`] therefore
//!    keeps scanning after a dropped element instead of giving up.
//! 2. **`<head>` stops the scan.** Alone among the eleven it is not discarded
//!    but *merged*, and it becomes the insertion point, so the content lands
//!    in the head and never reaches the body. `<head …>anything</head>` is
//!    `null` unconditionally.
//! 3. **An unterminated quoted value discards the element** — `<a href="hi'>`
//!    is `null`, CommonMark example 620. happy-dom then retries the same
//!    element at the next tag terminator, which is why `<a href="hi'>x"</a>`
//!    *does* produce one. [`first_element`] reproduces the retry.
//!
//! # Where the line is drawn, and what is registered
//!
//! The one mechanism this module does **not** model is happy-dom's
//! foster parenting: an element that is not a permitted descendant of
//! `<table>` or `<colgroup>` is *moved to before it*, so
//! `<table class="t"><img src="s"></table>` reports the **image's**
//! attributes for a `table` token. Reproducing it means porting happy-dom's
//! per-element `permittedDescendants` / `moveForbiddenDescendant`
//! configuration, which is a test-only DOM's non-conformant table handling —
//! and the shipped editor is Electron, i.e. Chromium, which does it
//! differently again. There the port reports the open tag's own attributes,
//! which is what the token is about.
//!
//! That is a deliberate difference, so it is registered:
//! `html-tag-attrs-from-a-foster-parented-element` in
//! `spec/divergences.json`. It is the only disagreement left — the eleven
//! names, the `<head>` rule and the unterminated-quote rule between them make
//! this module agree with happy-dom on **every one of the 1199** `html_tag`
//! matches in the corpus and the fixtures, in both directions, and the
//! registered class does not occur in either.
//!
//! # Values are `String`, not [`Span`](crate::token::Span)
//!
//! Names are ASCII-lowercased by the parser and values have their character
//! references resolved ([`decode_attribute_value`]), so neither is generally a
//! slice of the source. A duplicate attribute keeps the **first**.

use crate::emphasis::is_unicode_whitespace;
use crate::entities::NAMED_REFERENCES;

// ---------------------------------------------------------------------------
// The whitelist
// ---------------------------------------------------------------------------

/// `WHITELIST_ATTRIBUTES` (`utils.ts:11`), verbatim and in source order.
///
/// **Twenty-six, not twenty-five.** M1.md §5 D2 says 25; the array has 26.
/// The extra one is `data-align`, which is marktext's own addition rather than
/// part of the MDN list the comment above the array cites — it carries its own
/// `// Used in img` comment, and `utils/__tests__/dompurifyXss.spec.ts`
/// explains why it survives sanitisation. Counted against the running module,
/// not the source: `WHITELIST_ATTRIBUTES.length` is 26.
///
/// Order does not matter here — this is only ever a membership test — but it
/// is kept because the file is the spec.
pub(crate) const WHITELIST_ATTRIBUTES: [&str; 26] = [
    "align",
    "alt",
    "checked",
    "class",
    "color",
    "dir",
    "disabled",
    "for",
    "height",
    "hidden",
    "href",
    "id",
    "lang",
    "lazyload",
    "rel",
    "spellcheck",
    "src",
    "srcset",
    "start",
    "style",
    "target",
    "title",
    "type",
    "value",
    "width",
    // Used in img
    "data-align",
];

/// Tag names happy-dom will not append under `<body>`.
///
/// Measured, name by name, over every element name in the HTML standard: these
/// eleven give `body.firstElementChild === null` and the other 143 do not.
/// Reading the spec instead would have produced a different and wrong list —
/// happy-dom keeps `<colgroup>` and `<frame>`, which the "in body" insertion
/// mode discards, and it does not move `<meta>`, `<link>` or `<base>` anywhere.
///
/// `<html>`, `<head>` and `<body>` are here for a different reason from the
/// other eight: they are not discarded but resolved to the document's existing
/// nodes, so nothing new is appended. Only `<head>` also swallows the content
/// — see [`first_element`].
///
/// ASCII-lowercased before the lookup, as happy-dom's `asciiLowerCase` does.
const DROPPED_IN_BODY: [&str; 11] = [
    "body", "caption", "col", "head", "html", "tbody", "td", "tfoot", "th", "thead", "tr",
];

/// happy-dom's `rawText` content model — the two elements whose content is not
/// markup and which are appended only once their end tag is seen.
///
/// Both are on GFM §6.11's disallowed list, so `tryHtmlTag` rejects them
/// before [`get_attributes`] is reached. They are still handled here because
/// the *content* scan can meet one: `<td><style>x</style></td>` reaches this
/// module with `td` as the open tag.
const RAW_TEXT: [&str; 2] = ["script", "style"];

// ---------------------------------------------------------------------------
// getAttributes
// ---------------------------------------------------------------------------

/// `getAttributes(html)` (`utils.ts:172`).
///
/// `None` is muya's `null` and **fails** [`crate::lexer::try_html_tag`];
/// `Some(vec![])` is muya's `{}`, which does not, because an empty object is
/// truthy in JavaScript. Getting that distinction right is the first thing D2
/// asks for.
///
/// # The `IMG` seeding is order-observable
///
/// `{title: '', src: '', alt: ''}` is assigned **before** the loop, so those
/// three keys exist, in that order, even for `<img src=x>` — and the loop
/// overwrites values *without moving keys*, which is what a JavaScript object
/// does and what makes this a `Vec` rather than a map. `tagName === 'IMG'` is
/// the parsed name uppercased, so `<IMG>` seeds too.
pub(crate) fn get_attributes(html: &str) -> Option<Vec<(String, String)>> {
    let element = first_element(html)?;

    let mut attrs: Vec<(String, String)> = Vec::new();
    if element.name == "img" {
        for key in ["title", "src", "alt"] {
            attrs.push((key.to_string(), String::new()));
        }
    }

    for (name, value) in element.attributes {
        if !WHITELIST_ATTRIBUTES.contains(&name.as_str()) {
            continue;
        }
        // muya: `if (/width|height/.test(attr) && attribute)`. The `&&` is
        // load-bearing and is not a null check — `getAttribute` cannot return
        // null here (see `first_element`) — it is an *emptiness* check, so an
        // empty value skips the coercion entirely and stores `''`, which is
        // the same answer the coercion would give.
        let value = if is_width_or_height(&name) && !value.is_empty() {
            valid_width_and_height(&value)
        } else {
            value
        };
        // A JavaScript object assignment: an existing key keeps its position
        // and takes the new value. Only reachable via the `IMG` seeding, since
        // the attribute list is already first-wins.
        match attrs.iter_mut().find(|(key, _)| *key == name) {
            Some(slot) => slot.1 = value,
            None => attrs.push((name, value)),
        }
    }

    Some(attrs)
}

/// muya's `/width|height/.test(attr)`, which is **unanchored**.
///
/// That is the same shape as the bug D3 site 2 fixes — an unanchored test
/// where an exact one is meant — and it is deliberately *not* fixed and not
/// registered, because it is **not reachable**: `attr` has already been
/// filtered through [`WHITELIST_ATTRIBUTES`], and no whitelisted name contains
/// `width` or `height` as a proper substring. Over those 26 names this
/// predicate is exactly `name == "width" || name == "height"`, which
/// `the_unanchored_width_test_cannot_matter` asserts, so a future reader need
/// neither "fix" it nor register it.
fn is_width_or_height(name: &str) -> bool {
    name.contains("width") || name.contains("height")
}

/// `validWidthAndHeight` (`utils.ts:132`).
///
/// ```js
/// if (!/^\d+$/.test(value)) return '';
/// const num = Number.parseInt(value);
/// return num >= 0 ? num.toString() : '';
/// ```
///
/// Three things that are not what they look like:
///
/// - **`\d` is ASCII.** M1.md §4 C2's row, live here: `width="１２３"` and
///   `width="١٢٣"` are `''`, not `123`.
/// - **`num >= 0` is dead.** `^\d+$` admits no sign, so the parse is
///   non-negative by construction. Reproduced, not simplified away, because a
///   reader who deletes it should first know it is dead.
/// - **`parseInt(...).toString()` is not the identity.** It strips leading
///   zeros — `"0100"` becomes `"100"`, and 25 zeros become `"0"` — and, since
///   the round trip goes through an `f64`, a long enough digit string comes
///   back in exponential form: 23 nines become `"1e+23"`, and enough digits to
///   overflow the exponent become `"Infinity"`. All four are measured against
///   the engine, not assumed. [`js_number_to_string`] is that formatting.
fn valid_width_and_height(value: &str) -> String {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return String::new();
    }
    js_number_to_string(js_parse_int(value))
}

/// `Number.parseInt` over a string of ASCII digits.
///
/// Digits only — no sign, no radix prefix, no trailing garbage — so this is
/// the mathematical value of the digits rounded to the nearest `f64`, and
/// infinity past ~1.8e308, which is what `parseInt` produces and what
/// `str::parse` gives for free.
///
/// **Not `n * 10 + d` in a loop.** That was the first version and the
/// differential caught it: repeated multiply-and-add rounds at every step, so
/// twenty-five nines came back as `1.0000000000000003e+25` where JavaScript
/// says `1e+25`. ECMA-262 rounds the *whole* mathematical value once, and so
/// does `str::parse`.
fn js_parse_int(digits: &str) -> f64 {
    digits.parse::<f64>().unwrap_or(f64::INFINITY)
}

/// ECMA-262 `Number::toString`, for the non-negative integral values
/// [`js_parse_int`] can produce.
///
/// Rust's `Display` for `f64` is already the shortest round-tripping decimal,
/// which is ECMA-262's `s` and `k`; what differs is the *presentation* rule.
/// JavaScript switches to exponential notation once the decimal point sits
/// past digit 21, and Rust never does. So the digits come from Rust and the
/// layout is applied here.
fn js_number_to_string(value: f64) -> String {
    if value.is_infinite() {
        return "Infinity".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }

    // `value` is a non-negative integer, so `Display` gives plain digits.
    let plain = format!("{value}");
    let n = plain.len(); // the decimal point sits after all of them
    let significant = plain.trim_end_matches('0');
    let k = significant.len();

    if n <= 21 {
        return plain;
    }
    // ECMA-262 step: `s[0] . s[1..k] e+ (n-1)`, with the fraction omitted when
    // there is only one significant digit.
    let exponent = n - 1;
    if k == 1 {
        format!("{significant}e+{exponent}")
    } else {
        format!("{}.{}e+{exponent}", &significant[..1], &significant[1..])
    }
}

// ---------------------------------------------------------------------------
// Finding the element
// ---------------------------------------------------------------------------

/// The element `body.firstElementChild` would be.
struct Element {
    /// ASCII-lowercased, as happy-dom's `getStartTagElement` lowercases it.
    ///
    /// Not the same as `html_tag`'s capture 3. happy-dom's own start-tag
    /// pattern is `<([^\s/!>?]+)`, which runs past characters the rule's
    /// `[a-z][a-z\d-]*` stops at, so `<div.foo class=x>` is an element named
    /// `div.foo` here and a tag named `div` in the token. Only this name
    /// decides the `IMG` seeding and the [`DROPPED_IN_BODY`] gate.
    name: String,
    /// Lowercased names with decoded values, first occurrence winning.
    attributes: Vec<(String, String)>,
}

/// happy-dom's parse loop, reduced to the question `getAttributes` asks.
///
/// The parser is a two-state machine over `MARKUP_REGEXP`: in `any` it looks
/// for a start tag, in `startTag` it looks for the tag's terminator. What is
/// modelled here is exactly the part that can change which element ends up
/// first under `<body>`:
///
/// - start tags, and their terminator — the first `>`, or the `/` of a
///   trailing `/>`, or the `>` of an end tag that interrupts an unclosed start
///   tag (`<div\n</ul>`);
/// - comments, doctypes and processing instructions, skipped so that a `<div>`
///   inside one is not mistaken for markup;
/// - [`DROPPED_IN_BODY`], with `<head>` ending the scan and the other ten
///   letting it continue into the content;
/// - [`RAW_TEXT`], which is appended only once its end tag is seen;
/// - the unterminated-quote retry.
///
/// What is *not* modelled is foster parenting — see the module docs and
/// `spec/divergences.json`.
fn first_element(html: &str) -> Option<Element> {
    // The start tag being assembled: its lowercased name and where its
    // attribute string begins. It survives a failed terminator, because
    // happy-dom keeps the element and retries at the next one.
    let mut pending: Option<(String, usize)> = None;
    let mut i = 0;

    while i < html.len() {
        let Some((name, attributes_from)) = pending.take() else {
            // `any`: look for the next start tag. No more `<` means no more
            // elements, so nothing can be appended.
            let lt = i + html[i..].find('<')?;
            if let Some(resume) = skip_non_markup(html, lt) {
                i = resume;
                continue;
            }
            match start_tag_name(&html[lt..]) {
                Some(name) => {
                    let after = lt + 1 + name.len();
                    pending = Some((name.to_ascii_lowercase(), after));
                    i = after;
                }
                // `<` followed by whitespace, `/`, `>` or `?`: not a start tag
                // under `MARKUP_REGEXP`'s first alternative.
                None => i = lt + 1,
            }
            continue;
        };

        // `startTag`: look for the tag's terminator.
        let Some((attributes_to, resume)) = tag_terminator(html, i) else {
            // No terminator anywhere: the element is never appended.
            return None;
        };
        i = resume;

        match parse_attribute_string(&html[attributes_from..attributes_to]) {
            // The element is discarded and the *same* one is retried at the
            // next terminator. Re-parsing from `attributes_from` rather than
            // from after the last accepted attribute — which is where
            // happy-dom resumes — reaches the same set: the re-read prefix
            // yields the same names in the same order, and `push_first`
            // discards the repeats.
            Attributes::UnterminatedQuote => pending = Some((name, attributes_from)),
            Attributes::Parsed(attributes) => {
                // `<head>` is merged into the document's existing head and
                // *becomes the insertion point*, so everything after it lands
                // in the head. Nothing can reach the body.
                if name == "head" {
                    return None;
                }
                // The other ten are discarded without becoming the insertion
                // point, so the scan continues and a later element takes the
                // slot.
                if DROPPED_IN_BODY.contains(&name.as_str()) {
                    continue;
                }
                if RAW_TEXT.contains(&name.as_str()) {
                    // Appended only once its own end tag arrives; until then
                    // the parser is in `rawTextElement` and appends nothing.
                    return has_end_tag(html, resume, &name)
                        .then_some(Element { name, attributes });
                }
                return Some(Element { name, attributes });
            }
        }
    }

    None
}

/// Where a start tag's attribute string ends, and where scanning resumes.
///
/// The terminator is `MARKUP_REGEXP`'s `/>`, `>` or end-tag alternative,
/// whichever comes first, and in every case the attribute string stops at the
/// `>` — or at the `/` of a `/>`, which is why `<div class=a/>` gives `a` and
/// `<div class=a/ id=b>` gives `a/`.
///
/// `-->` and `--!>` are skipped rather than treated as terminators: they are
/// their own alternative, so the regex consumes the `>` inside them and
/// happy-dom's `startTag` branch ignores it. A `<!--` is *not* skipped, for
/// the same reason in reverse — the parser does not enter comment state from
/// `startTag`, so a `>` inside a comment inside a start tag does terminate it.
fn tag_terminator(html: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = html.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'-' {
            if html[i..].starts_with("-->") {
                i += 3;
                continue;
            }
            if html[i..].starts_with("--!>") {
                i += 4;
                continue;
            }
        }
        if bytes[i] == b'>' {
            let attributes_to = if i > from && bytes[i - 1] == b'/' {
                i - 1
            } else {
                i
            };
            return Some((attributes_to, i + 1));
        }
        i += 1;
    }
    None
}

/// Whether `</name>` — `MARKUP_REGEXP`'s `<\/([^\s/!>?]+)\s*>`, with the name
/// compared ASCII-case-insensitively — occurs at or after `from`.
///
/// A *different* end tag does not end a raw-text element: happy-dom's
/// `parseRawTextElementContent` returns without leaving the state, so it keeps
/// looking.
fn has_end_tag(html: &str, from: usize, name: &str) -> bool {
    let mut i = from;
    while let Some(offset) = html[i..].find("</") {
        // `start_tag_name` reads the run after a leading delimiter, which is
        // the `/` here — the same `[^\s/!>?]+` class the end-tag alternative
        // uses.
        let slash = i + offset + 1;
        if let Some(found) = start_tag_name(&html[slash..]) {
            let after = skip_whitespace(html, slash + 1 + found.len());
            if html.as_bytes().get(after) == Some(&b'>') && found.eq_ignore_ascii_case(name) {
                return true;
            }
        }
        i = slash + 1;
    }
    false
}

/// Whether `html[at..]` opens a comment, doctype or processing instruction,
/// and where to resume if so.
///
/// `MARKUP_REGEXP`'s start-tag alternative is `<([^\s/!>?]+)`, so a `<`
/// followed by `!` or `?` cannot begin one. A comment runs to `-->` or `--!>`;
/// the other two run to the next `>`. Unterminated, they swallow the rest of
/// the input, which is why this returns the end of the string rather than
/// giving up.
fn skip_non_markup(html: &str, at: usize) -> Option<usize> {
    let rest = &html[at..];
    if let Some(body) = rest.strip_prefix("<!--") {
        let end = body
            .find("-->")
            .map(|p| p + 3)
            .or_else(|| body.find("--!>").map(|p| p + 4))
            .unwrap_or(body.len());
        return Some(at + 4 + end);
    }
    if rest.starts_with("<!") || rest.starts_with("<?") {
        let end = rest[2..].find('>').map(|p| p + 3).unwrap_or(rest.len());
        return Some(at + end);
    }
    None
}

/// `MARKUP_REGEXP`'s group 1 — `<` then one or more characters that are not
/// whitespace, `/`, `!`, `>` or `?`.
///
/// Returns `None` when the run would be empty, which is the regex failing to
/// match rather than matching nothing: `< a`, `</a>`, `<>` and `<?x` all fall
/// through to another alternative.
fn start_tag_name(rest: &str) -> Option<&str> {
    let after = &rest[1..];
    let end = after
        .find(|c: char| is_unicode_whitespace(Some(c)) || matches!(c, '/' | '!' | '>' | '?'))
        .unwrap_or(after.len());
    (end > 0).then(|| &after[..end])
}

// ---------------------------------------------------------------------------
// The attribute string
// ---------------------------------------------------------------------------

/// What [`parse_attribute_string`] concluded about one start tag.
enum Attributes {
    /// The attributes, lowercased and decoded, first occurrence winning.
    Parsed(Vec<(String, String)>),
    /// A quoted value ran to the end without its closing quote. happy-dom
    /// abandons `parseEndOfStartTag` here without appending the element, and
    /// retries the *same* element at the next terminator.
    UnterminatedQuote,
}

/// happy-dom's `ATTRIBUTE_REGEXP` loop (`HTMLParser.js`), ported alternative
/// by alternative.
///
/// ```js
/// /\s*(NAME)\s*=\s*([^"'=<>\\`\s]+)      // 1: unquoted
/// |\s*(NAME)\s*=\s*"([^"]*)("{0,1})      // 2: double-quoted
/// |\s*(NAME)\s*=\s*'([^']*)('{0,1})      // 3: single-quoted
/// |\s*(NAME)/gm                          // 4: bare name
/// ```
///
/// with `NAME` = `[a-zA-Z0-9-_:.$@?\\<\[\]]+`. JavaScript alternation is
/// leftmost-first, so the four are tried in order at each position, and `/g`
/// means a position where none matches is skipped rather than fatal — which is
/// why `<div クラス="a" class="b">` yields only `class`, and why `<div =foo>`
/// yields nothing.
///
/// The accept/reject logic is muya's `if` transcribed: alternative 1 and 4
/// always accept, 2 and 3 accept only when the closing quote was found, and a
/// **failed** 2 or 3 is [`Attributes::UnterminatedQuote`].
fn parse_attribute_string(source: &str) -> Attributes {
    let bytes = source.as_bytes();
    let mut attributes: Vec<(String, String)> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        i = skip_whitespace(source, i);
        let Some(name_end) = attribute_name_end(source, i) else {
            // No name here. The regex's `/g` scan moves on by one character.
            i = next_char_boundary(source, i);
            continue;
        };
        let name = source[i..name_end].to_ascii_lowercase();
        let after_name = skip_whitespace(source, name_end);

        if bytes.get(after_name) != Some(&b'=') {
            // Alternative 4: a bare name. Note this does *not* consume the
            // whitespace after the name — the next iteration re-skips it.
            push_first(&mut attributes, name, String::new());
            i = name_end;
            continue;
        }

        let value_start = skip_whitespace(source, after_name + 1);
        match bytes.get(value_start) {
            // Alternatives 2 and 3.
            Some(&quote @ (b'"' | b'\'')) => {
                let body_start = value_start + 1;
                match source[body_start..].find(quote as char) {
                    Some(len) => {
                        push_first(
                            &mut attributes,
                            name,
                            decode_attribute_value(&source[body_start..body_start + len]),
                        );
                        i = body_start + len + 1;
                    }
                    None => return Attributes::UnterminatedQuote,
                }
            }
            // Alternative 1, whose value class is `[^"'=<>\\`\s]+`. An empty
            // run means alternative 1 fails and alternative 4 takes the name
            // with no value — `<div class=>` and `<div class==a>` are both
            // `class=""`.
            _ => {
                let end = unquoted_value_end(source, value_start);
                if end == value_start {
                    push_first(&mut attributes, name, String::new());
                    i = name_end;
                } else {
                    push_first(
                        &mut attributes,
                        name,
                        decode_attribute_value(&source[value_start..end]),
                    );
                    i = end;
                }
            }
        }
    }

    Attributes::Parsed(attributes)
}

/// A duplicate attribute keeps the **first**: happy-dom guards the insert with
/// `if (!attributes.getNamedItem(name))`, and the name it looks up is already
/// lowercased, so `<span CLASS="a" class="b">` keeps `a`.
fn push_first(attributes: &mut Vec<(String, String)>, name: String, value: String) {
    if !attributes.iter().any(|(key, _)| *key == name) {
        attributes.push((name, value));
    }
}

fn skip_whitespace(source: &str, from: usize) -> usize {
    let mut i = from;
    while let Some(c) = source[i..].chars().next() {
        if !is_unicode_whitespace(Some(c)) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

fn next_char_boundary(source: &str, from: usize) -> usize {
    source[from..]
        .chars()
        .next()
        .map_or(source.len(), |c| from + c.len_utf8())
}

/// The end of `[a-zA-Z0-9-_:.$@?\\<\[\]]+` at `from`, or `None` if it is empty.
///
/// The class is happy-dom's, not the HTML spec's. It admits `<`, `[` and `]`
/// and rejects everything non-ASCII, which is why an attribute named `中` is
/// dropped rather than kept.
fn attribute_name_end(source: &str, from: usize) -> Option<usize> {
    let end = from
        + source[from..]
            .find(|c: char| {
                !(c.is_ascii_alphanumeric()
                    || matches!(
                        c,
                        '-' | '_' | ':' | '.' | '$' | '@' | '?' | '\\' | '<' | '[' | ']'
                    ))
            })
            .unwrap_or(source.len() - from);
    (end > from).then_some(end)
}

/// The end of `[^"'=<>\\`\s]+` at `from`.
fn unquoted_value_end(source: &str, from: usize) -> usize {
    from + source[from..]
        .find(|c: char| {
            matches!(c, '"' | '\'' | '=' | '<' | '>' | '\\' | '`') || is_unicode_whitespace(Some(c))
        })
        .unwrap_or(source.len() - from)
}

// ---------------------------------------------------------------------------
// Character references
// ---------------------------------------------------------------------------

/// `XMLEncodeUtility.decodeHTMLAttributeValue`, which is `entities`'
/// `decodeHTMLAttribute` — the HTML5 character-reference algorithm in
/// **attribute mode**.
///
/// Attribute mode differs from text mode in exactly one rule, and it is
/// observable: a named reference matched **without** its semicolon is left
/// literal when the next character is `=` or an ASCII alphanumeric. That is
/// what keeps query strings intact (`href="?a=1&id=2"` does not grow an `≡`)
/// and it is why `&notit;` stays `&notit;` while `&not;` becomes `¬`.
///
/// Numeric references do *not* require the semicolon in this mode — `&#65` is
/// `A` — and they go through the HTML5 replacement rules: zero, a surrogate
/// and anything past U+10FFFF all become U+FFFD, and the C1 range U+0080–U+009F
/// maps through the Windows-1252 table.
///
/// The named table is [`NAMED_REFERENCES`]; see that module for its provenance
/// and for why it is not [`crate::escape`]'s.
pub(crate) fn decode_attribute_value(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }

    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'&' {
            let c = raw[i..].chars().next().expect("in bounds");
            out.push(c);
            i += c.len_utf8();
            continue;
        }

        let consumed = if bytes.get(i + 1) == Some(&b'#') {
            decode_numeric_reference(raw, i, &mut out)
        } else {
            decode_named_reference(raw, i, &mut out)
        };
        match consumed {
            Some(len) => i += len,
            None => {
                out.push('&');
                i += 1;
            }
        }
    }

    out
}

/// `&#123;` / `&#x7b;`, with the semicolon optional.
fn decode_numeric_reference(raw: &str, at: usize, out: &mut String) -> Option<usize> {
    let bytes = raw.as_bytes();
    let (radix, digits_start) = match bytes.get(at + 2) {
        Some(&(b'x' | b'X')) => (16u32, at + 3),
        _ => (10u32, at + 2),
    };

    let mut end = digits_start;
    let mut code: u32 = 0;
    let mut overflow = false;
    while let Some(digit) = bytes.get(end).and_then(|b| (*b as char).to_digit(radix)) {
        // Saturate rather than wrap: everything past U+10FFFF is U+FFFD
        // anyway, and a 400-digit reference must not alias a valid scalar.
        code = code.saturating_mul(radix).saturating_add(digit);
        if code > 0x10_FFFF {
            overflow = true;
        }
        end += 1;
    }
    if end == digits_start {
        return None;
    }
    if bytes.get(end) == Some(&b';') {
        end += 1;
    }

    out.push(if overflow {
        '\u{fffd}'
    } else {
        replace_code_point(code)
    });
    Some(end - at)
}

/// The HTML5 "numeric character reference end state" replacements.
fn replace_code_point(code: u32) -> char {
    /// U+0080–U+009F map through Windows-1252; `0` means "unchanged".
    const C1: [u32; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    let code = match code {
        0 => return '\u{fffd}',
        0x80..=0x9F => {
            let mapped = C1[(code - 0x80) as usize];
            if mapped == 0 { code } else { mapped }
        }
        _ => code,
    };
    char::from_u32(code).unwrap_or('\u{fffd}')
}

/// `&amp;`, `&amp`, and the 2123 others.
///
/// Longest match over the names, where a name is available with its semicolon
/// always and without it only for the 106 legacy forms — then the attribute
/// rule on the semicolon-less case.
fn decode_named_reference(raw: &str, at: usize, out: &mut String) -> Option<usize> {
    let bytes = raw.as_bytes();
    let run_start = at + 1;
    let mut run_end = run_start;
    while bytes.get(run_end).is_some_and(u8::is_ascii_alphanumeric) {
        run_end += 1;
    }
    // The longest name is 31 bytes; nothing beyond that can match.
    let limit = run_end.min(run_start + 31);

    // Longest wins, so keep walking and remember the last usable candidate.
    let mut best: Option<(usize, &'static str, bool)> = None;
    for end in (run_start + 1)..=limit {
        let Ok(index) = NAMED_REFERENCES.binary_search_by_key(&&raw[run_start..end], |(n, _, _)| n)
        else {
            continue;
        };
        let (_, value, has_legacy_form) = NAMED_REFERENCES[index];
        if bytes.get(end) == Some(&b';') {
            best = Some((end + 1 - at, value, false));
        } else if has_legacy_form {
            best = Some((end - at, value, true));
        }
    }

    let (consumed, value, semicolonless) = best?;
    if semicolonless {
        // The attribute-mode rule. `entities` abandons the whole reference
        // rather than falling back to a shorter name, so `&ampa` is literal.
        let next = bytes.get(at + consumed);
        if next.is_some_and(|b| *b == b'=' || b.is_ascii_alphanumeric()) {
            return None;
        }
    }
    out.push_str(value);
    Some(consumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expectation below was **measured** against happy-dom's
    /// `getAttributes` at S4 and reproduced by the port, not derived from the
    /// HTML specification. Where the two disagree, happy-dom is what
    /// `// @vitest-environment happy-dom` makes authoritative for muya's own
    /// suite, and §3 rule 3 makes muya's behaviour the definition.
    fn attrs(html: &str) -> Option<Vec<(String, String)>> {
        get_attributes(html)
    }

    fn pairs(html: &str) -> Vec<(String, String)> {
        attrs(html).expect("an element")
    }

    fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// M1.md §5 D2 said 25. The array has 26 and the running module agrees:
    /// `WHITELIST_ATTRIBUTES.length === 26`. The doc was corrected at S4.
    #[test]
    fn the_whitelist_has_twenty_six_names_and_no_duplicates() {
        assert_eq!(WHITELIST_ATTRIBUTES.len(), 26);
        let mut sorted = WHITELIST_ATTRIBUTES;
        sorted.sort_unstable();
        let mut deduped = sorted.to_vec();
        deduped.dedup();
        assert_eq!(deduped.len(), 26, "duplicate whitelisted name");
        assert!(
            WHITELIST_ATTRIBUTES.contains(&"data-align"),
            "marktext's own"
        );
    }

    /// `/width|height/.test(attr)` is unanchored, which is the same shape as
    /// the bug D3 site 2 fixes — and unreachable, because the name has already
    /// been filtered through the whitelist. Asserted so nobody "fixes" it and
    /// nobody registers it.
    #[test]
    fn the_unanchored_width_test_cannot_matter() {
        let matched: Vec<&str> = WHITELIST_ATTRIBUTES
            .iter()
            .copied()
            .filter(|name| is_width_or_height(name))
            .collect();
        assert_eq!(matched, ["height", "width"]);
    }

    #[test]
    fn names_are_lowercased_and_a_duplicate_keeps_the_first() {
        assert_eq!(
            pairs("<div CLASS=\"a\" ID=\"b\">"),
            kv(&[("class", "a"), ("id", "b")])
        );
        assert_eq!(
            pairs("<div class=\"a\" class=\"b\">"),
            kv(&[("class", "a")])
        );
        assert_eq!(
            pairs("<div CLASS=\"a\" class=\"b\">"),
            kv(&[("class", "a")])
        );
        // First-wins applies to the valueless form too, in both orders.
        assert_eq!(pairs("<div class=\"a\" class>"), kv(&[("class", "a")]));
        assert_eq!(pairs("<div class class=\"a\">"), kv(&[("class", "")]));
    }

    /// A valueless attribute is `""`, never absent — the finding that removed
    /// the inner `Option` from [`crate::token::HtmlTag::attrs`].
    #[test]
    fn a_valueless_attribute_has_the_empty_string_as_its_value() {
        for src in [
            "<input disabled>",
            "<input disabled=\"\">",
            "<input DISABLED>",
        ] {
            assert_eq!(pairs(src), kv(&[("disabled", "")]), "{src:?}");
        }
    }

    /// happy-dom's attribute regex, not the HTML5 tokenizer states. The two
    /// differ, and where they do this follows happy-dom: `<div class=a/>` is
    /// `a` here and `a/` under the spec, because `/>` is its own alternative
    /// and the attribute string stops before the `/`.
    #[test]
    fn the_attribute_scanner_is_happy_doms_regex() {
        for (src, expected) in [
            ("<div class=c>", &[("class", "c")][..]),
            ("<div class='c'>", &[("class", "c")][..]),
            (
                "<div class=\"c\"id=\"i\">",
                &[("class", "c"), ("id", "i")][..],
            ),
            ("<div   class=\"c\"   >", &[("class", "c")][..]),
            ("<div\tclass = \"c\">", &[("class", "c")][..]),
            // `/` before `>` is the self-closing marker; `/` elsewhere is part
            // of an unquoted value.
            ("<div class=c/>", &[("class", "c")][..]),
            ("<div class=c/ id=i>", &[("class", "c/"), ("id", "i")][..]),
            ("<div /class=\"c\">", &[("class", "c")][..]),
            // An unquoted value stops at a quote, `=`, `<`, `>`, backslash,
            // backtick or whitespace.
            ("<div class=a\"b>", &[("class", "a")][..]),
            ("<div class=a'b>", &[("class", "a")][..]),
            ("<div class==c>", &[("class", "")][..]),
            ("<div class=>", &[("class", "")][..]),
            ("<div class>", &[("class", "")][..]),
            ("<div class=\"=c\">", &[("class", "=c")][..]),
            // A name outside `[a-zA-Z0-9-_:.$@?\\<\[\]]+` is skipped whole.
            ("<div 中=\"a\" class=\"c\">", &[("class", "c")][..]),
            ("<div =foo class=c>", &[("class", "c")][..]),
            // Values are not restricted to ASCII.
            ("<div class=\"中\">", &[("class", "中")][..]),
            ("<div class=\"🎉\">", &[("class", "🎉")][..]),
        ] {
            assert_eq!(pairs(src), kv(expected), "{src:?}");
        }
    }

    /// An unterminated quoted value discards the element outright — muya's
    /// `null`, so `tryHtmlTag` fails. happy-dom then retries the same element
    /// at the next terminator, which is why the second case *does* parse.
    #[test]
    fn an_unterminated_quoted_value_discards_the_element() {
        assert_eq!(attrs("<a href=\"hi'>"), None);
        assert_eq!(attrs("<a href='hi\">"), None);
        assert_eq!(
            pairs("<a href=\"hi'>x\" y>"),
            kv(&[("href", "hi'>x")]),
            "the retry at the next terminator finds a closing quote"
        );
    }

    /// The eleven names happy-dom will not append under `<body>`, and the one
    /// of them that also swallows its content.
    #[test]
    fn the_dropped_names_give_null_unless_the_content_supplies_an_element() {
        for name in DROPPED_IN_BODY {
            let bare = format!("<{name} class=\"c\">");
            assert_eq!(attrs(&bare), None, "{bare:?}");
        }
        // A discarded element is not the insertion point, so a later element
        // takes the slot — CommonMark example 148 is exactly this shape.
        assert_eq!(
            pairs("<td>b <code class=\"k\">|</code> az</td>"),
            kv(&[("class", "k")])
        );
        assert_eq!(
            pairs("<tr><td><span id=\"i\">y</span></td></tr>"),
            kv(&[("id", "i")])
        );
        // …except `<head>`, which *becomes* the insertion point.
        assert_eq!(
            attrs("<head class=\"c\"><span id=\"i\">y</span></head>"),
            None
        );
        // Names outside the eleven are ordinary elements, including the ones a
        // reading of the HTML spec would expect to be hoisted into the head.
        for src in [
            "<meta charset=\"utf-8\" id=\"i\">",
            "<link rel=\"stylesheet\" href=\"a.css\">",
            "<base href=\"/\">",
            "<colgroup class=\"c\">",
            "<frame class=\"c\">",
        ] {
            assert!(attrs(src).is_some(), "{src:?}");
        }
    }

    /// `{title:'', src:'', alt:''}` is assigned before the loop, so the three
    /// keys exist in that order even when the tag sets none of them, and the
    /// loop overwrites values without moving keys.
    #[test]
    fn img_seeds_three_keys_in_order_and_only_for_img() {
        assert_eq!(
            pairs("<img>"),
            kv(&[("title", ""), ("src", ""), ("alt", "")])
        );
        assert_eq!(
            pairs("<img src=x>"),
            kv(&[("title", ""), ("src", "x"), ("alt", "")])
        );
        // `tagName === 'IMG'` is the parsed name uppercased, so case is
        // irrelevant — and the seeded keys keep their positions.
        assert_eq!(
            pairs("<IMG alt=\"a\" src=\"b\" title=\"c\">"),
            kv(&[("title", "c"), ("src", "b"), ("alt", "a")])
        );
        assert_eq!(
            pairs("<img data-align=\"left\" src=\"x\">"),
            kv(&[
                ("title", ""),
                ("src", "x"),
                ("alt", ""),
                ("data-align", "left")
            ])
        );
        // Not seeded for anything else, including a name that merely starts
        // with `img`.
        assert_eq!(pairs("<image src=x>"), kv(&[("src", "x")]));
        assert_eq!(pairs("<div src=x>"), kv(&[("src", "x")]));
    }

    /// `validWidthAndHeight`, including the three things it is not.
    #[test]
    fn width_and_height_are_coerced_through_a_digits_only_check() {
        for (src, expected) in [
            ("<div width=\"100\">", "100"),
            // parseInt(...).toString() strips leading zeros…
            ("<div width=\"0100\">", "100"),
            ("<div width=\"0000000000000000000000000\">", "0"),
            // …and is not the identity at the top of the f64 range.
            ("<div width=\"99999999999999999999999\">", "1e+23"),
            ("<div width=\"9999999999999999999999999\">", "1e+25"),
            // `^\d+$` admits no sign, no unit and no exponent, and `\d` is
            // ASCII — M1.md §4 C2's row.
            ("<div width=\"abc\">", ""),
            ("<div width=\"10px\">", ""),
            ("<div width=\"-5\">", ""),
            ("<div width=\"+1\">", ""),
            ("<div height=\"1e3\">", ""),
            ("<div width=\"１２３\">", ""),
            ("<div width=\"١٢٣\">", ""),
            // An empty value skips the coercion entirely via muya's `&&`.
            ("<div width=\"\">", ""),
            ("<div width>", ""),
            // …and `height` goes through the same path.
            ("<div height=\"0\">", "0"),
        ] {
            let name = if src.contains("height") {
                "height"
            } else {
                "width"
            };
            assert_eq!(pairs(src), kv(&[(name, expected)]), "{src:?}");
        }
    }

    /// A digit string long enough to overflow an `f64` reaches
    /// `Infinity.toString()`, and `num >= 0` does not stop it because
    /// `Infinity >= 0`.
    #[test]
    fn an_overflowing_width_becomes_the_string_infinity() {
        let src = format!("<div width=\"{}\">", "9".repeat(400));
        assert_eq!(pairs(&src), kv(&[("width", "Infinity")]));
    }

    /// The HTML5 character-reference algorithm in **attribute mode**.
    #[test]
    fn attribute_values_have_their_character_references_resolved() {
        for (src, expected) in [
            ("<div class=\"a&amp;b\">", "a&b"),
            ("<div class=\"&quot;\">", "\""),
            ("<div class=\"&ouml;&ouml;\">", "öö"),
            ("<div class=\"&#65;&#x42;\">", "AB"),
            // The semicolon is optional for a numeric reference…
            ("<div class=\"&#65\">", "A"),
            // …and for the 106 legacy named forms.
            ("<div class=\"&amp\">", "&"),
            ("<div class=\"&AMP\">", "&"),
            ("<div class=\"&lt\">", "<"),
            // The attribute rule: a semicolon-less match followed by `=` or an
            // alphanumeric is left literal. This is what keeps query strings
            // intact.
            ("<div class=\"&ampa\">", "&ampa"),
            ("<div class=\"&amp=\">", "&amp="),
            ("<div class=\"&notit;\">", "&notit;"),
            ("<div class=\"&not;\">", "¬"),
            // Not a reference at all.
            ("<div class=\"a&b\">", "a&b"),
            ("<div class=\"&\">", "&"),
            ("<div class=\"&#;\">", "&#;"),
            // Numeric fixups: zero, surrogates and out-of-range are U+FFFD;
            // the C1 range maps through Windows-1252.
            ("<div class=\"&#0;\">", "\u{fffd}"),
            ("<div class=\"&#xD800;\">", "\u{fffd}"),
            ("<div class=\"&#x110000;\">", "\u{fffd}"),
            ("<div class=\"&#x80;\">", "€"),
            ("<div class=\"&#13;\">", "\r"),
            // Decoding happens once, so an encoded `&` does not cascade.
            ("<div class=\"&#38;amp;\">", "&amp;"),
        ] {
            assert_eq!(pairs(src), kv(&[("class", expected)]), "{src:?}");
        }
    }

    /// `&ouml;` is not in [`crate::escape`]'s list, which is why the HTML5
    /// table is a separate module rather than a reuse of it. Its ordering is
    /// what makes the lookup a binary search.
    #[test]
    fn the_named_reference_table_is_sorted_and_ascii() {
        let names: Vec<&str> = NAMED_REFERENCES.iter().map(|(n, _, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "the table must be sorted for binary_search");
        assert!(
            names
                .iter()
                .all(|n| n.bytes().all(|b| b.is_ascii_alphanumeric()))
        );
        assert_eq!(names.iter().map(|n| n.len()).max(), Some(31));
        assert_eq!(
            NAMED_REFERENCES
                .iter()
                .filter(|(_, _, legacy)| *legacy)
                .count(),
            106,
            "the semicolon-less legacy forms"
        );
    }

    /// A non-whitelisted attribute is dropped, whatever it is.
    #[test]
    fn attributes_outside_the_whitelist_are_dropped() {
        assert_eq!(
            pairs("<div onclick=\"alert(1)\" class=\"c\">"),
            kv(&[("class", "c")])
        );
        assert_eq!(pairs("<div data-width=\"5\">"), Vec::new());
        // …and an element with no whitelisted attribute is `{}`, which is
        // truthy in JavaScript and so does NOT fail `tryHtmlTag`.
        assert_eq!(attrs("<span>x</span>"), Some(Vec::new()));
    }

    /// `getAttributes` is called on the whole match, so the content is parsed
    /// too — but the attributes come from the first element, not from a
    /// descendant.
    #[test]
    fn the_whole_match_is_parsed_but_the_first_element_answers() {
        assert_eq!(
            pairs("<div class=\"a\"><span id=\"s\">t</span></div>"),
            kv(&[("class", "a")])
        );
        assert_eq!(pairs("<div class=\"a\">text</div>"), kv(&[("class", "a")]));
    }
}
