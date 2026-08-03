//! The token-stream differential harness (docs/M1.md §5 D3, §6 S7).
//!
//! ```text
//! input string ──┬──► node → muya tokenizer → token JSON ──┐
//!                │                                          ├──► assert equal
//!                └──► mt_inline::tokenizer ─────────────────┘
//! ```
//!
//! # What this is, and what it is not
//!
//! [`diff.rs`](crate::diff) is RUST-REWRITE-PLAN.md §11.2's harness and it
//! compares **block state**. This one compares the **token stream of one leaf
//! block**, which is what `mt-inline` produces and what M1's divergence
//! register is written in terms of. Neither subsumes the other: block state
//! does not carry inline tokens, and a token stream has no blocks. They share
//! [`crate::diff::first_difference`], the Node-invocation shape and the
//! engine-unavailable convention, and nothing else.
//!
//! # Why it exists — the gap it closes
//!
//! `spec/divergences.json` records every place the port deliberately disagrees
//! with muya, and rule 3 says an entry with no failing differential case is
//! **stale**. From S0 to S6 that rule could not fire, because
//! `divergences::disagrees` returned `None` for every input: there was no
//! TypeScript token stream to compare a Rust one against. Every entry reported
//! `SKIPPED`, and reverting a fix would still have exited 0.
//!
//! Five stages closed the gap by hand instead, each with a throwaway script —
//! 207 inputs at S2, 319 at S3, 4172 at S4, 49,751 at S5, 5586 at S6. **S5 is
//! what makes that untenable rather than merely repetitive.** S4 probed one
//! divergence class at a new container with two inputs, found both agreeing,
//! and wrote into M1.md that the register needed no widening; 96 of 228 swept
//! inputs disagree, and one of S4's two inputs agreed for a reason that does
//! not generalise. A hand-run does not just find a class once instead of every
//! time — it can find **half** of one and read as a clean result.
//!
//! # The five handoff notes, and where each one is implemented
//!
//! Everything below was learned by a stage losing a run to it. They are
//! restated here beside the code that satisfies them, because a note in a
//! document nobody opens while writing the comparator is a note that gets
//! rediscovered.
//!
//! | Note | From | Here |
//! |---|---|---|
//! | Compare hex of UTF-8 bytes, not strings | S5 | [`hex`] |
//! | Convert muya's UTF-16 offsets by walking code points | S5 | the JS side's `offsetTable` |
//! | Do not let the transport normalise (`{ignoreBOM: true}`) | S5 | the JS side reads JSON with `readFileSync`, which does not strip a BOM |
//! | Normalise `undefined` vs `''` **per field**, not per type | S3 | [`wire_token`] — see below |
//! | Sweep, do not sample | S3/S4/S5 | [`sweep_inputs`] |
//! | Run a negative control every time | S6 | `divergences::main` runs the register's own inputs |
//! | Compare fields, not rendered text | S6 | this module compares the token object |
//! | `highlights` is a per-field absent/empty normalisation | S6 | [`wire_token`] |
//!
//! ## `undefined` versus `''`, which is the one that would sink this
//!
//! muya writes `to[3] || ''` at some sites and a bare `to[3]` at others, so the
//! answer is **per field**. `token.rs`'s header lists the four that serialize
//! as `undefined` — [`CodeEmojiMath::backlash`], `ReferenceDefinition::
//! left_title_space`, `HtmlTag::content` and `HtmlTag::close_tag` — and the
//! comment branch of `tryHtmlTag` omits three keys outright.
//!
//! Rather than encode that as a table, both sides follow one mechanical rule:
//! **a field whose muya value is `undefined` is omitted from the object
//! entirely.** The JS side omits any own key whose value is `undefined` (and
//! naturally has no key muya never wrote); this side omits the corresponding
//! `Option::None`. So "absent" compares against "absent" and `''` against
//! `''`, and [`crate::diff::first_difference`] reports a key that only one side
//! has as `present`/`missing` rather than as a value mismatch — which is the
//! diagnostic you want when this goes wrong.
//!
//! `highlights` is the same question wearing different clothes: muya creates
//! the key on the first intersection, so a token that intersects nothing has no
//! key, while the port gives every token a (possibly empty) `Vec` (M1.md
//! §5 D7). An empty list is omitted on both sides. Get this wrong and **every
//! token in every document** reports a disagreement.
//!
//! # Where the UTF-16 boundary is
//!
//! `mt-inline`'s crate docs say the conversion to UTF-16 belongs in exactly one
//! place, "the boundary where a token is serialized for comparison against the
//! TypeScript engine". This module is one half of that boundary and it does the
//! conversion by **not doing it**: the Rust side is already in bytes, and the
//! JS side converts *its* offsets to bytes before they are compared. Bytes are
//! the wire unit, so nothing in `mt-inline` ever learns what a UTF-16 code unit
//! is.

use std::path::{Path, PathBuf};
use std::process::Command;

use mt_inline::{
    AutoLinkExtension, BacklashPair, BeginRule, CodeEmojiMath, Emphasis, HtmlTagName, Image, Link,
    ReferenceDefinition, ReferenceImage, ReferenceLink, Span, Token, TokenKind, TokenizerOptions,
};
use serde_json::{Map, Value, json};

/// Exit code the dumper uses for "the reference engine is not available".
/// Same convention as `dump-ts-state.mjs`; see [`crate::diff`].
const EXIT_UNIMPLEMENTED: i32 = 3;

// ---------------------------------------------------------------------------
// The wire form
// ---------------------------------------------------------------------------

/// A string as lowercase hex of its UTF-8 bytes.
///
/// S5's first handoff note. Comparing hex rather than strings removes every
/// encoding question from the diff in one move, and it makes a disagreement in
/// an invisible character — a BOM, a NEL, a ZWJ, a lone combining mark —
/// visible in the failure output instead of rendering as two identical-looking
/// strings.
fn hex(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for byte in s.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// A span's text, hex-encoded.
///
/// [`Span::of`] rather than [`Span::get`] deliberately: every span the
/// tokenizer produces is in bounds and on `char` boundaries, so a panic here is
/// a genuine tokenizer bug and this harness should not paper over it with a
/// sentinel that shows up as an ordinary disagreement.
fn text(span: Span, src: &str) -> String {
    hex(span.of(src))
}

/// An optional span whose muya form is `''` when the capture group is absent —
/// the `to[3] || ''` sites.
fn text_or_empty(span: Option<Span>, src: &str) -> String {
    span.map(|s| text(s, src)).unwrap_or_default()
}

fn range(span: Span) -> Value {
    json!({ "start": span.start, "end": span.end })
}

/// Insert `key` only when the capture group participated.
///
/// The other half of the `undefined`/`''` rule: these are the sites where muya
/// writes a **bare** `to[n]`, so an absent group leaves `undefined` on the
/// token and the JS side drops the key.
fn put_optional(object: &mut Map<String, Value>, key: &str, span: Option<Span>, src: &str) {
    if let Some(span) = span {
        object.insert(key.to_string(), Value::String(text(span, src)));
    }
}

fn put(object: &mut Map<String, Value>, key: &str, value: impl Into<Value>) {
    object.insert(key.to_string(), value.into());
}

/// `{first, second}` as links and images carry it.
///
/// `image` and `link` read `imageTo[5]` / `linkTo[5]` bare, and those groups sit
/// outside any optional group so they always participate. `reference_link` and
/// `reference_image` read `rLinkTo[4] || ''`, so an absent group is `''` there.
/// `reference` picks between the two.
fn backlash_pair(pair: BacklashPair, src: &str, reference: bool) -> Value {
    let mut object = Map::new();
    put(&mut object, "first", text(pair.first, src));
    if reference {
        put(&mut object, "second", text_or_empty(pair.second, src));
    } else {
        put_optional(&mut object, "second", pair.second, src);
    }
    Value::Object(object)
}

fn begin_rule(object: &mut Map<String, Value>, rule: &BeginRule, src: &str) {
    put(object, "marker", text(rule.marker, src));
    put(object, "content", text_or_empty(rule.content, src));
    put(object, "backlash", text_or_empty(rule.backlash, src));
}

fn emphasis(object: &mut Map<String, Value>, e: &Emphasis, src: &str) {
    put(object, "marker", text(e.marker, src));
    put(object, "children", wire_tokens(&e.children, src));
    // `to[3]` is read bare, but `(\\*)` sits outside any optional group in all
    // three rules, so it always participates and is `''` at worst.
    put(object, "backlash", text(e.backlash, src));
}

fn chunk(object: &mut Map<String, Value>, c: &CodeEmojiMath, src: &str) {
    put(object, "marker", text(c.marker, src));
    put(object, "content", text(c.content, src));
    // `lexer.ts:227` writes a bare `backlash: to[3]`, and only `inline_math`
    // has a third group — so `inline_code` and `emoji` really do carry
    // `undefined`. S3 found this by diffing token streams, where a dumper
    // assuming the general `|| ''` rule reported ten false disagreements.
    put_optional(object, "backlash", c.backlash, src);
}

fn reference_definition(object: &mut Map<String, Value>, d: &ReferenceDefinition, src: &str) {
    put(object, "leftBracket", text(d.left_bracket, src));
    put(object, "label", text(d.label, src));
    put(object, "backlash", text(d.backlash, src));
    put(object, "rightBracket", text(d.right_bracket, src));
    put(object, "leftHrefMarker", text(d.left_href_marker, src));
    put(object, "href", text(d.href, src));
    put(object, "rightHrefMarker", text(d.right_href_marker, src));
    // `lexer.ts:102` reads `def[8]` without a fallback — the second of the four
    // `undefined` fields, and the one `types.ts` is wrong about.
    put_optional(object, "leftTitleSpace", d.left_title_space, src);
    put(object, "titleMarker", text_or_empty(d.title_marker, src));
    put(object, "title", text_or_empty(d.title, src));
    put(object, "rightTitleSpace", text(d.right_title_space, src));
}

fn image(object: &mut Map<String, Value>, i: &Image, src: &str) {
    put(object, "marker", text(i.marker, src));
    put(object, "srcAndTitle", text(i.src_and_title, src));
    // Insertion order is observable — the JS side reads `Object.keys` — and
    // `lexer.ts:326` writes src, title, alt in that order.
    put(
        object,
        "attrs",
        json!([
            [hex("src"), hex(&i.attrs.src)],
            [hex("title"), hex(&i.attrs.title)],
            [hex("alt"), hex(&i.attrs.alt)],
        ]),
    );
    put(object, "src", text(i.src, src));
    put(object, "title", text_or_empty(i.title, src));
    put(object, "alt", text(i.alt, src));
    put(object, "backlash", backlash_pair(i.backlash, src, false));
}

fn link(object: &mut Map<String, Value>, l: &Link, src: &str) {
    put(object, "marker", text(l.marker, src));
    put(object, "hrefAndTitle", text(l.href_and_title, src));
    put(object, "href", text(l.href, src));
    put(object, "title", text_or_empty(l.title, src));
    put(object, "anchor", text(l.anchor, src));
    put(object, "children", wire_tokens(&l.children, src));
    put(object, "backlash", backlash_pair(l.backlash, src, false));
}

fn reference_link(object: &mut Map<String, Value>, l: &ReferenceLink, src: &str) {
    put(object, "isFullLink", l.is_full_link);
    put(object, "anchor", text(l.anchor, src));
    put(object, "backlash", backlash_pair(l.backlash, src, true));
    put(object, "label", text(l.label, src));
    put(object, "children", wire_tokens(&l.children, src));
}

fn reference_image(object: &mut Map<String, Value>, i: &ReferenceImage, src: &str) {
    put(object, "isFullLink", i.is_full_link);
    put(object, "alt", text(i.alt, src));
    put(object, "backlash", backlash_pair(i.backlash, src, true));
    put(object, "label", text(i.label, src));
}

fn auto_link_extension(object: &mut Map<String, Value>, a: &AutoLinkExtension, src: &str) {
    // Exactly one of the three participates; the other two are `undefined` on
    // the token, so the JS side has no key for them and neither does this.
    put_optional(object, "www", a.www, src);
    put_optional(object, "url", a.url, src);
    put_optional(object, "email", a.email, src);
    put(object, "linkType", a.link_type.as_str());
}

/// One token in muya's wire shape, with byte offsets and hex-encoded strings.
///
/// The field names are muya's, including the load-bearing `backlash` typo (see
/// `token.rs`), because the comparison is by key.
fn wire_token(token: &Token, src: &str) -> Value {
    let mut object = Map::new();
    put(&mut object, "type", token.type_str());
    put(&mut object, "raw", text(token.raw, src));
    object.insert("range".to_string(), range(token.range));

    match &token.kind {
        TokenKind::Header(r)
        | TokenKind::Hr(r)
        | TokenKind::CodeFence(r)
        | TokenKind::MultipleMath(r) => begin_rule(&mut object, r, src),
        TokenKind::ReferenceDefinition(d) => reference_definition(&mut object, d, src),
        TokenKind::Text { content } => put(&mut object, "content", text(*content, src)),
        TokenKind::Backlash { marker } => {
            put(&mut object, "marker", text(*marker, src));
            // `lexer.ts:130` writes the literal `''`: the escaped character
            // goes into `pending` and surfaces in the *next* text token.
            put(&mut object, "content", "");
        }
        TokenKind::Strong(e) | TokenKind::Em(e) | TokenKind::Del(e) => {
            emphasis(&mut object, e, src);
        }
        TokenKind::InlineCode(c) | TokenKind::Emoji(c) | TokenKind::InlineMath(c) => {
            chunk(&mut object, c, src);
        }
        TokenKind::SuperSubScript { marker, content }
        | TokenKind::FootnoteIdentifier { marker, content } => {
            put(&mut object, "marker", text(*marker, src));
            put(&mut object, "content", text(*content, src));
        }
        TokenKind::Image(i) => image(&mut object, i, src),
        TokenKind::Link(l) => link(&mut object, l, src),
        TokenKind::ReferenceLink(l) => reference_link(&mut object, l, src),
        TokenKind::ReferenceImage(i) => reference_image(&mut object, i, src),
        TokenKind::HtmlEscape { escape_character } => {
            put(&mut object, "escapeCharacter", text(*escape_character, src));
        }
        TokenKind::AutoLinkExtension(a) => auto_link_extension(&mut object, a, src),
        TokenKind::AutoLink(a) => {
            put_optional(&mut object, "href", a.href, src);
            put_optional(&mut object, "email", a.email, src);
            put(&mut object, "isLink", a.is_link);
            // muya stores the literal `'<'`, which is always the byte at
            // `raw.start`. Not modelled on the Rust token; emitted here.
            put(&mut object, "marker", hex("<"));
        }
        TokenKind::HtmlTag(t) => {
            put(
                &mut object,
                "tag",
                match t.tag {
                    // The comment branch sets the literal `'<!---->'`, which is
                    // not a slice of the source.
                    HtmlTagName::Comment => hex("<!---->"),
                    HtmlTagName::Name(span) => text(span, src),
                },
            );
            put(&mut object, "openTag", text(t.open_tag, src));
            // The comment branch never writes these three; the element branch
            // writes `closeTag`/`content` bare, so an element with no close tag
            // carries `undefined` for both. Same rule, two reasons.
            put_optional(&mut object, "closeTag", t.close_tag, src);
            put_optional(&mut object, "content", t.content, src);
            put(
                &mut object,
                "attrs",
                Value::Array(
                    t.attrs
                        .iter()
                        .map(|(name, value)| json!([hex(name), hex(value)]))
                        .collect(),
                ),
            );
            if let Some(children) = &t.children {
                put(&mut object, "children", wire_tokens(children, src));
            }
        }
        TokenKind::SoftLineBreak {
            line_break,
            is_at_end,
        } => {
            put(&mut object, "lineBreak", text(*line_break, src));
            put(&mut object, "isAtEnd", *is_at_end);
        }
        TokenKind::HardLineBreak {
            spaces,
            line_break,
            is_at_end,
        } => {
            put(&mut object, "spaces", text(*spaces, src));
            put(&mut object, "lineBreak", text(*line_break, src));
            put(&mut object, "isAtEnd", *is_at_end);
        }
        TokenKind::TailHeader { marker } => put(&mut object, "marker", text(*marker, src)),
    }

    // M1.md §5 D7: muya creates the key on the first intersection, so a token
    // that intersects nothing has none. Omitted when empty on both sides.
    if !token.highlights.is_empty() {
        put(
            &mut object,
            "highlights",
            Value::Array(
                token
                    .highlights
                    .iter()
                    .map(|h| {
                        json!({
                            "start": h.span.start,
                            "end": h.span.end,
                            "active": h.active,
                        })
                    })
                    .collect(),
            ),
        );
    }

    Value::Object(object)
}

fn wire_tokens(tokens: &[Token], src: &str) -> Value {
    Value::Array(tokens.iter().map(|t| wire_token(t, src)).collect())
}

/// One input, with the options it needs.
///
/// Almost every input is tokenized with muya's defaults, which is what a plain
/// `String` gives. Three of the 26 token types cannot be reached that way and
/// would otherwise never have their wire mapping compared against the engine at
/// all:
///
/// - `reference_link` and `reference_image` are gated on
///   `state.labels.has(label)`, and the default label map is empty;
/// - `footnote_identifier` is gated on `options.footnote`, which defaults to
///   **false**.
///
/// Only *membership* in the label map is read by either handler
/// (`lexer.ts:415`, `:466`), so [`Input::labels`] carries keys and no targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Input {
    pub src: String,
    /// Reference-definition labels, already lowercased.
    pub labels: Vec<String>,
    /// `options.footnote`.
    pub footnote: bool,
}

impl Input {
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            labels: Vec::new(),
            footnote: false,
        }
    }

    pub fn with_labels(mut self, labels: &[&str]) -> Self {
        self.labels = labels.iter().map(|l| l.to_lowercase()).collect();
        self
    }

    pub fn with_footnote(mut self) -> Self {
        self.footnote = true;
        self
    }

    fn options(&self) -> TokenizerOptions {
        let mut options = TokenizerOptions::muya_default();
        options.syntax.footnote = self.footnote;
        options.labels = self
            .labels
            .iter()
            .map(|label| (label.clone(), mt_inline::Label::default()))
            .collect();
        options
    }

    fn to_json(&self) -> Value {
        if self.labels.is_empty() && !self.footnote {
            return Value::String(self.src.clone());
        }
        json!({
            "src": self.src,
            "labels": self.labels,
            "footnote": self.footnote,
        })
    }
}

impl From<String> for Input {
    fn from(src: String) -> Self {
        Input::new(src)
    }
}

impl From<&str> for Input {
    fn from(src: &str) -> Self {
        Input::new(src)
    }
}

/// One input's token stream, in the shape the TypeScript side emits.
pub fn wire_with(input: &Input) -> Value {
    let tokens = mt_inline::tokenizer(&input.src, &input.options());
    wire_tokens(&tokens, &input.src)
}

/// [`wire_with`] under muya's default options. Used by this module's tests,
/// which are where the wire shape is pinned without a marktext clone.
#[cfg(test)]
fn wire(src: &str) -> Value {
    wire_with(&Input::new(src))
}

// ---------------------------------------------------------------------------
// Running both engines
// ---------------------------------------------------------------------------

/// What happened on one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputVerdict {
    /// Both engines produced the same token stream, field for field.
    Agree,
    /// They differ. Carries the JSON path of the first disagreement, so a
    /// failure names the token and the field rather than dumping two trees.
    Disagree { at: String, ts: String, rs: String },
    /// muya threw on this input. A finding, not a harness bug — recorded per
    /// input so one bad input does not blind the rest of a sweep.
    EngineThrew(String),
}

/// Where the harness writes the two sides of one run.
///
/// Under `target/` rather than a system temp directory so that a failed run
/// leaves the exact inputs and the exact TypeScript output on disk beside the
/// build, which is what makes a disagreement reproducible by hand.
fn scratch(repo_root: &Path) -> Result<PathBuf, String> {
    let dir = repo_root.join("target").join("xtask");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Ask muya for every input's token stream in one process.
///
/// The outer `Result` is "did the harness itself work"; `Ok(Err(reason))` is
/// "the reference engine was not available", which is a skip rather than a
/// failure. Same split, and the same reason for it, as [`crate::diff`].
fn dump_ts_tokens(
    repo_root: &Path,
    inputs: &[Input],
) -> Result<Result<Vec<Value>, String>, String> {
    let dir = scratch(repo_root)?;
    let inputs_path = dir.join("token-inputs.json");
    let out_path = dir.join("token-dump-ts.json");

    std::fs::write(
        &inputs_path,
        serde_json::to_vec(&Value::Array(inputs.iter().map(Input::to_json).collect()))
            .map_err(|e| format!("cannot serialize inputs: {e}"))?,
    )
    .map_err(|e| format!("cannot write {}: {e}", inputs_path.display()))?;

    let script = repo_root
        .join("tools")
        .join("diff")
        .join("dump-ts-tokens.mjs");
    let output = match Command::new("node")
        .current_dir(repo_root)
        .arg("--import")
        .arg("tsx")
        .arg(&script)
        .arg("--inputs")
        .arg(&inputs_path)
        .arg("--out")
        .arg(&out_path)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return Ok(Err(format!(
                "cannot run `node`: {e}. Install Node 20.19+ and run `pnpm install` in this repo."
            )));
        }
    };

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if code == EXIT_UNIMPLEMENTED {
        return Ok(Err(stderr));
    }
    if code != 0 {
        return Err(format!("dump-ts-tokens.mjs exited {code}:\n{stderr}"));
    }

    let text = std::fs::read_to_string(&out_path)
        .map_err(|e| format!("cannot read {}: {e}", out_path.display()))?;
    let envelope: Value = serde_json::from_str(&text)
        .map_err(|e| format!("dump-ts-tokens.mjs produced invalid JSON: {e}"))?;
    let results = envelope
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| "dump-ts-tokens.mjs envelope has no `results` array".to_string())?;
    if results.len() != inputs.len() {
        return Err(format!(
            "dump-ts-tokens.mjs returned {} results for {} inputs",
            results.len(),
            inputs.len()
        ));
    }
    Ok(Ok(results.clone()))
}

/// Compare both engines over `inputs`.
///
/// `Ok(None)` means the TypeScript engine was not available — the caller
/// decides whether that is a skip or a failure, exactly as `--require-ts` does
/// for [`crate::diff`].
pub fn compare(repo_root: &Path, inputs: &[Input]) -> Result<Option<Vec<InputVerdict>>, String> {
    if inputs.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let ts = match dump_ts_tokens(repo_root, inputs)? {
        Ok(results) => results,
        Err(_reason) => return Ok(None),
    };

    let mut verdicts = Vec::with_capacity(inputs.len());
    for (input, result) in inputs.iter().zip(&ts) {
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            verdicts.push(InputVerdict::EngineThrew(error.to_string()));
            continue;
        }
        let ts_tokens = result
            .get("tokens")
            .ok_or_else(|| "a result has neither `tokens` nor `error`".to_string())?;
        let rs_tokens = wire_with(input);
        verdicts.push(match crate::diff::first_difference(ts_tokens, &rs_tokens) {
            None => InputVerdict::Agree,
            Some((at, ts, rs)) => InputVerdict::Disagree { at, ts, rs },
        });
    }
    Ok(Some(verdicts))
}

/// Why the TypeScript engine could not be consulted, for a caller that wants to
/// report it rather than just skip.
pub fn unavailable_reason(repo_root: &Path) -> Option<String> {
    match dump_ts_tokens(repo_root, &[Input::new("x")]) {
        Ok(Err(reason)) => Some(reason),
        Ok(Ok(_)) => None,
        Err(e) => Some(e),
    }
}

// ---------------------------------------------------------------------------
// The sweep input set
// ---------------------------------------------------------------------------

/// Above this, a corpus file is swept line by line rather than whole.
///
/// Not a performance dodge — `mt-inline` tokenizes a **leaf block's** text, and
/// a 5 MB generated document is not one. Feeding it whole exercises one
/// enormous level, which is a real input shape and worth having (the round-trip
/// gate does exactly that), but it is not the shape the tokenizer meets in the
/// application. Lines are the closer analogue of a leaf block, and they are
/// what make the count large enough to find things.
const WHOLE_FILE_LIMIT: usize = 256 * 1024;

/// How wide a sweep to run.
///
/// The two differ in **one** thing: whether `1mb.md` and `5mb.md` contribute
/// their lines. Everything else is in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breadth {
    /// Every commit. 4,264 inputs, ~22 s of Node.
    Default,
    /// The nightly soak. 41,009 inputs, ~120 s of Node.
    Full,
}

/// The inputs a sweep runs, beyond the register's own.
///
/// **Sweep, do not sample** — S3's, S4's and S5's findings all came from a
/// combination nobody would have written by hand, and S5's came from an
/// exhaustive enumeration rather than a chosen set. What is available to sweep
/// over here is the repository's own corpora, which between them are the widest
/// real markdown it has:
///
/// - every `bench/corpus/` file, whole when it is under [`WHOLE_FILE_LIMIT`];
/// - every **line** of every `bench/corpus/` file;
/// - every CommonMark 0.31 and GFM 0.29 example (1324 of them);
/// - every `spec/fixtures/marktext-round-trip/` file, whole and by line.
///
/// This is register **rule 1's other half**: a disagreement on a registered
/// input is expected, and a disagreement on anything else is a failure. Without
/// a set to be silent about, the register only ever proves that its own inputs
/// still disagree — which is necessary and nothing like sufficient.
///
/// # The one cap, stated rather than applied quietly
///
/// [`Breadth::Default`] leaves out the lines of `1mb.md` and `5mb.md`, and
/// nothing else. Measured at S7: the full set is **41,009** inputs (40,982
/// swept plus the register's 27) and **~120 s** of Node; without those two
/// files it is **4,264** and **~22 s**.
///
/// Read those two timings with the same caution the benchmark's A/B earned:
/// the *first* full-sweep run of a session measured **277 s** for a slightly
/// smaller set, and a later one measured 119 s for a larger one. Node tiers up
/// over a long run and the OS caches the corpus, so the first measurement of
/// anything is not the number. The ratio between the two breadths is what the
/// cap is about, and it is roughly 5×.
///
/// Three platforms × every push is not the place
/// for the first number, so `cargo xtask ci` runs the second and
/// `.github/workflows/soak.yml` runs the first nightly, beside the 24-hour fuzz
/// soak.
///
/// The cap is defensible rather than merely convenient, and the reason is in
/// `xtask/src/corpus.rs`: `250kb.md`, `1mb.md` and `5mb.md` are the **same
/// generator** with three seeds, over the same 60-word table and the same
/// sentence shapes. What the two larger ones add is repetition, not diversity —
/// deduplication alone removes about 18,000 of the 58,000 raw lines because
/// they are *literally identical* to a line already in the set. So the bounded
/// run is not a thinner version of the same coverage; it is the same coverage
/// with the repetition removed, and `250kb.md`'s 2,284 lines still carry the
/// long-prose shape. The 2,968 corpus lines it does sweep are, not by accident,
/// exactly the set S6's hand-run used.
///
/// What it is **not** is a claim that the big files are unchecked:
/// `round_trip.rs`'s release gate tiles both of them whole, and `cargo xtask
/// diff` runs the block engine over both on every commit. This is about which
/// inputs the *token-stream* comparison sees.
/// The characters M1.md §4 C2 says JavaScript and Rust classify differently.
///
/// Not a list of *expected* results — that is
/// `crates/mt-inline/tests/character_classes.rs`, which holds hand-transcribed
/// goldens and so runs on a machine with no marktext clone. This is the same
/// area of risk probed the other way: the engine is asked live, so a
/// **mis-transcribed** golden is caught too, which is the failure mode S5
/// established is real.
///
/// Both entries in each pair matter. U+0085 NEL is in Rust's `\p{White_Space}`
/// and not in JavaScript's `\s`; U+FEFF BOM is the reverse, and a BOM is the
/// first character of a great many real files.
const C2_CHARACTERS: &[char] = &[
    // --- `\s`: the two that disagree, then the rest of both definitions -----
    '\u{85}',   // NEL — Rust yes, JavaScript no
    '\u{feff}', // BOM — JavaScript yes, Rust no
    '\u{2028}',
    '\u{2029}',
    '\u{a0}',
    '\u{3000}',
    '\u{b}',
    '\u{c}',
    '\u{9}',
    ' ',
    '\u{1680}',
    '\u{2000}',
    '\u{200a}',
    '\u{202f}',
    '\u{205f}',
    // Adjacent non-whitespace that a sloppy class would swallow.
    '\u{200b}', // ZWSP — whitespace in neither
    '\u{180e}', // MONGOLIAN VOWEL SEPARATOR — whitespace in neither since 6.3
    // --- `\w`: ASCII-only in JavaScript, Unicode-aware in Rust -------------
    'a',
    'Z',
    '_',
    'я',
    '中',
    'α',
    'ﬁ',
    '\u{17f}',  // ſ — folds to `s` under full Unicode folding, not under ASCII
    '\u{212a}', // K — folds to `k` the same way
    // --- `\d`: `[0-9]` in JavaScript, `\p{Nd}` in Rust ---------------------
    '9',
    '\u{966}',   // ० … ९ Devanagari
    '\u{669}',   // ٩ Arabic-Indic
    '\u{1d7f9}', // 𝟹 MATHEMATICAL — astral, and `Nd`
    '\u{b2}',    // ² — `No`, not `Nd`, so neither engine admits it
    // --- `.`: JavaScript excludes four characters, Rust excludes one -------
    '\n',
    '\r',
    // --- grapheme traps, because the corpus has them -----------------------
    '\u{200d}', // ZWJ
    '\u{fe0f}', // VARIATION SELECTOR-16
    '\u{301}',  // COMBINING ACUTE ACCENT
];

/// Contexts that reach a rule using one of the C2 classes.
///
/// `{}` is where the character goes. Chosen so that every row of C2's table is
/// reached **through a rule that produces a token** — which is the standard
/// `character_classes.rs` sets for itself, and the reason its own table had
/// rows waiting on a stage until S5.
const C2_CONTEXTS: &[&str] = &[
    "#{}x",                   // header's `(\s+|$)`
    "x{}###",                 // tail_header's `^(\s+#+)(\s*)$`
    "&{}amp;",                // html_escape, and its `i` flag
    "&am{}p;",                //   …inside the name rather than before it
    "a{}:smile:",             // the emoji word boundary, lexer.ts:206 — `\w`
    ":sm{}ile:",              // emoji's content class `[a-z_\d+-]`
    ":{}:",                   //   …as the whole shortcode
    "`a{}b`",                 // inline_code's `.{2,}`
    "$a{}b$",                 // inline_math's `\\.`
    "**a{}b**",               // flanking, inside
    "{}**a**",                // flanking, before — UNICODE_WHITESPACE_REG
    "**a**{}",                //   …and after
    "*a{}b*",                 // em, whose base is 1 (the S3 direction of D3 site 1)
    "~~a{}b~~",               // del
    "^a{}b^",                 // super_sub_script
    "http{}://x.y",           // auto_link's scheme — `\d` and the removed `(?i)`
    "<https://x{}.y>",        //   …and the angle-bracket form's host
    "a{}b@c.de",              // auto_link_extension's email local part — `\w`
    "www.x{}.y",              //   …and the www alternative
    "<b{}>x</b>",             // html_tag's tag-name class, wrapped in `(?-i:…)`
    "<b class=\"a{}\">x</b>", // …and an attribute value, which the DOM decodes
    "[a](b{}c)",              // link destination
    "![a{}](b)",              // image alt
    "[a{}]: /u \"t\"",        // reference_definition's label
    "x{}\ny",                 // soft_line_break, and `.`'s four exclusions
    "x  {}\ny",               // hard_line_break's `(\s+)`
    "a\\{}b",                 // backlash — what the escaped character may be
];

/// One input per wire shape, so that every branch of [`wire_token`] is compared
/// against the engine at least once rather than only the ones the corpus
/// happens to contain.
///
/// Three of the 26 types need non-default options and are the reason [`Input`]
/// carries any: `reference_link` and `reference_image` are gated on the label
/// map, `footnote_identifier` on `options.footnote`. Without them the serializer
/// for those three — `isFullLink`, the `|| ''` on `backlash.second`, the
/// `rLinkTo[3] || rLinkTo[1]` label fallback — would ship unchecked.
fn token_type_probes() -> Vec<Input> {
    let plain = [
        "# heading",               // header
        "***",                     // hr
        "``` rust title=\"x\"",    // code_fence
        "$$",                      // multiple_math
        "[label]: /url \"title\"", // reference_definition
        "plain",                   // text
        "a\\*b",                   // backlash
        "**bold**",                // strong
        "*em*",                    // em
        "~~del~~",                 // del
        "`code`",                  // inline_code
        ":smile:",                 // emoji
        "$a+b$",                   // inline_math
        "^sup^",                   // super_sub_script
        "~sub~",                   //   …the other marker
        "![alt](/src \"t\")",      // image
        "[anchor](/href \"t\")",   // link
        "&amp;",                   // html_escape
        "https://example.com/a",   // auto_link_extension — url
        "www.example.com",         //   …www
        "user@example.com",        //   …email
        "<https://example.com>",   // auto_link — link
        "<user@example.com>",      //   …email
        "<span id=\"i\">x</span>", // html_tag — element
        "<br>",                    //   …void, no close tag: content undefined
        "<!-- comment -->",        //   …comment: three keys absent
        "a\nb",                    // soft_line_break
        "a  \nb",                  // hard_line_break
        "# h #",                   // tail_header
        // The two `attrs` shapes, which no other probe reaches.
        "![a](/s)", // image attrs: src/title/alt, encoded
        "<a href=\"&ouml;.html\" data-align=\"c\">x</a>", // whitelist + entities
    ];

    let mut probes: Vec<Input> = plain.iter().map(|s| Input::new(*s)).collect();
    probes.extend([
        Input::new("[anchor][label]").with_labels(&["label"]),
        Input::new("[anchor][]").with_labels(&["anchor"]),
        Input::new("[label]").with_labels(&["label"]),
        Input::new("![alt][label]").with_labels(&["label"]),
        Input::new("![alt][]").with_labels(&["alt"]),
        Input::new("![label]").with_labels(&["label"]),
        Input::new("x [^note]").with_footnote(),
        Input::new("x [^1] y").with_footnote(),
    ]);
    probes
}

pub fn sweep_inputs(repo_root: &Path, breadth: Breadth) -> Result<Vec<Input>, String> {
    /// The two files whose per-line contribution [`Breadth::Default`] omits.
    const LARGE_PROSE: [&str; 2] = ["1mb.md", "5mb.md"];

    let mut inputs: Vec<String> = Vec::new();

    let push_lines = |text: &str, inputs: &mut Vec<String>| {
        for line in text.lines() {
            if !line.trim().is_empty() {
                inputs.push(line.to_string());
            }
        }
    };

    // bench/corpus/
    let corpus = repo_root.join("bench").join("corpus");
    let mut corpus_files: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .map_err(|e| format!("cannot read {}: {e}", corpus.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .filter(|p| {
            !p.file_stem()
                .is_some_and(|s| s.eq_ignore_ascii_case("README"))
        })
        .collect();
    corpus_files.sort();
    for path in corpus_files {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if !text.is_empty() && text.len() <= WHOLE_FILE_LIMIT {
            inputs.push(text.clone());
        }
        let is_large_prose = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| LARGE_PROSE.contains(&n));
        if breadth == Breadth::Full || !is_large_prose {
            push_lines(&text, &mut inputs);
        }
    }

    // spec/fixtures/ — the CommonMark and GFM examples.
    let fixtures = repo_root.join("spec").join("fixtures");
    for file in ["commonmark-spec-0.31.json", "gfm-spec-0.29-gfm.json"] {
        let path = fixtures.join(file);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let examples: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        for example in examples
            .as_array()
            .ok_or_else(|| format!("{}: not a JSON array", path.display()))?
        {
            if let Some(markdown) = example.get("markdown").and_then(Value::as_str)
                && !markdown.is_empty()
            {
                inputs.push(markdown.to_string());
            }
        }
    }

    // spec/fixtures/marktext-round-trip/ — whole documents.
    let round_trip = fixtures.join("marktext-round-trip");
    let mut round_trip_files = Vec::new();
    collect_markdown(&round_trip, &mut round_trip_files)?;
    round_trip_files.sort();
    for path in round_trip_files {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if !text.is_empty() {
            inputs.push(text.clone());
        }
        push_lines(&text, &mut inputs);
    }

    // M1.md §4 C2, swept rather than sampled: every trap character in every
    // context that reaches a rule using its class.
    for context in C2_CONTEXTS {
        for character in C2_CHARACTERS {
            inputs.push(context.replace("{}", &character.to_string()));
        }
    }

    // The per-type probes go **first**, and that ordering is load-bearing
    // rather than stylistic. Deduplication below keeps the first occurrence, so
    // a probe whose text also appears as a corpus line survives and the line is
    // dropped. Put them last instead and `Breadth::Default` stops being a
    // subset of `Breadth::Full`: a probe deduped against a `5mb.md` line would
    // be present in one breadth and absent from the other, which is the kind of
    // difference that makes a nightly failure impossible to reproduce from the
    // per-commit run. `the_full_sweep_is_a_superset_of_the_default_one` is what
    // notices.
    let mut out: Vec<Input> = token_type_probes();
    out.extend(inputs.into_iter().map(Input::new));

    // Deduplicate: the corpora overlap (a GFM example is often one line of a
    // round-trip fixture), and running an input twice inflates the headline
    // number without widening the coverage. By whole `Input`, not by `src`, so
    // that `[anchor][label]` with a label map and the same text without one
    // stay the two different tests they are.
    let mut seen = std::collections::HashSet::new();
    out.retain(|input| seen.insert(input.clone()));
    Ok(out)
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?
    {
        let path = entry
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_markdown(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one field-mapping rule the whole comparison rests on, asserted from
    /// the Rust side alone so that a regression here fails `cargo test` rather
    /// than waiting for a machine with a marktext clone.
    ///
    /// `inline_code` has no third capture group, so muya's `backlash: to[3]`
    /// leaves `undefined` and the key must be **absent**. `inline_math` has
    /// one, and it is `''`. Same token payload struct, opposite wire forms.
    #[test]
    fn an_absent_capture_group_read_bare_has_no_key_at_all() {
        let code = wire("`x`");
        let token = &code[0];
        assert_eq!(token["type"], "inline_code");
        assert!(
            token.get("backlash").is_none(),
            "inline_code has no third group, so muya carries `undefined`: {token}"
        );

        let math = wire("$x$");
        let token = &math[0];
        assert_eq!(token["type"], "inline_math");
        assert_eq!(
            token["backlash"], "",
            "inline_math's third group participates and matches empty"
        );
    }

    /// The `|| ''` sites, which are the other half of the same rule.
    #[test]
    fn an_absent_capture_group_read_with_a_fallback_is_the_empty_string() {
        let header = wire("# x");
        assert_eq!(header[0]["type"], "header");
        assert_eq!(header[0]["backlash"], "");
        // `hr` has no content group at all, and muya writes `to[2] || ''`.
        let hr = wire("***");
        assert_eq!(hr[0]["type"], "hr");
        assert_eq!(hr[0]["content"], "");
    }

    /// `tryHtmlTag`'s comment branch writes no `closeTag`, `content` or
    /// `children` key; its element branch writes all three.
    #[test]
    fn an_html_comment_has_no_close_tag_content_or_children_key() {
        let comment = wire("<!-- c -->");
        let token = &comment[0];
        assert_eq!(token["type"], "html_tag");
        assert_eq!(token["tag"], hex("<!---->"));
        for absent in ["closeTag", "content", "children"] {
            assert!(token.get(absent).is_none(), "{absent} should be absent");
        }

        let element = wire("<b>x</b>");
        let token = &element[0];
        assert_eq!(token["tag"], hex("b"));
        assert_eq!(token["content"], hex("x"));
        assert!(token["children"].is_array());
    }

    /// Ranges cross the wire as **byte** offsets, so `mt-inline` never learns
    /// what a UTF-16 code unit is. The JS side converts; this side does not
    /// have to.
    #[test]
    fn ranges_are_byte_offsets() {
        let tokens = wire("中**x**");
        // "中" is three UTF-8 bytes and one UTF-16 code unit. muya would say
        // the strong token starts at 1; here it starts at 3.
        assert_eq!(tokens[1]["type"], "strong");
        assert_eq!(tokens[1]["range"]["start"], 3);
    }

    /// M1.md §5 D7: an empty highlight list is the same state as muya's absent
    /// key. Get this wrong and every token in every document disagrees.
    #[test]
    fn an_empty_highlight_list_has_no_key() {
        for token in wire("a **b** c").as_array().expect("array") {
            assert!(token.get("highlights").is_none(), "{token}");
        }
    }

    /// Strings cross as hex of their UTF-8 bytes — S5's first handoff note.
    #[test]
    fn strings_are_hex_of_their_utf8_bytes() {
        assert_eq!(hex(""), "");
        assert_eq!(hex("a"), "61");
        assert_eq!(hex("中"), "e4b8ad");
        assert_eq!(wire("ab")[0]["raw"], "6162");
    }

    /// The sweep has to be big enough to find something. S3's, S4's and S5's
    /// findings all came from a combination nobody would have written by hand.
    #[test]
    fn the_sweep_is_wide() {
        let inputs = sweep_inputs(&crate::repo_root(), Breadth::Default).expect("collect");
        assert!(
            inputs.len() > 3000,
            "the sweep collapsed to {} inputs",
            inputs.len()
        );
        // Deduplicated, or the headline number flatters the coverage.
        let unique: std::collections::HashSet<&Input> = inputs.iter().collect();
        assert_eq!(unique.len(), inputs.len());
    }

    /// Every one of the 26 wire shapes is compared against the engine at least
    /// once, so no branch of [`wire_token`] ships unchecked. Three of them are
    /// why [`Input`] carries options at all.
    #[test]
    fn every_token_type_has_a_probe() {
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for probe in token_type_probes() {
            fn walk(tokens: &Value, seen: &mut std::collections::BTreeSet<String>) {
                for token in tokens.as_array().expect("array") {
                    seen.insert(token["type"].as_str().expect("a type").to_string());
                    if let Some(children) = token.get("children") {
                        walk(children, seen);
                    }
                }
            }
            walk(&wire_with(&probe), &mut seen);
        }
        let expected = [
            "auto_link",
            "auto_link_extension",
            "backlash",
            "code_fence",
            "del",
            "em",
            "emoji",
            "footnote_identifier",
            "hard_line_break",
            "header",
            "hr",
            "html_escape",
            "html_tag",
            "image",
            "inline_code",
            "inline_math",
            "link",
            "multiple_math",
            "reference_definition",
            "reference_image",
            "reference_link",
            "soft_line_break",
            "strong",
            "super_sub_script",
            "tail_header",
            "text",
        ];
        assert_eq!(expected.len(), 26, "types.ts declares 26 `type` strings");
        for wanted in expected {
            assert!(
                seen.contains(wanted),
                "no probe produces a `{wanted}` token"
            );
        }
    }

    /// The one cap the default breadth applies, pinned so that widening or
    /// narrowing it is a deliberate edit rather than a side effect of touching
    /// the corpus. `--full-sweep` is strictly larger and includes everything the
    /// default has.
    #[test]
    fn the_full_sweep_is_a_superset_of_the_default_one() {
        let root = crate::repo_root();
        let default: Vec<Input> = sweep_inputs(&root, Breadth::Default).expect("collect");
        let full = sweep_inputs(&root, Breadth::Full).expect("collect");
        let full_set: std::collections::HashSet<&Input> = full.iter().collect();
        for input in &default {
            assert!(full_set.contains(input), "the full sweep dropped an input");
        }
        assert!(
            full.len() > default.len() * 5,
            "the two breadths should differ by the two large prose files: {} vs {}",
            full.len(),
            default.len()
        );
    }
}
