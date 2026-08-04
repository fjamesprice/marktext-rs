//! # `mt-md` — markdown ⇄ `Document`
//!
//! ## Contract
//!
//! **Round-trip fidelity.** Specifically, the property that is tested (§4):
//!
//! ```text
//! parse(serialize(parse(s))) == parse(s)
//! ```
//!
//! for every corpus file and every spec fixture. Lossless round-trip is one of
//! the three things §10 marks non-negotiable regardless of schedule, because
//! it cannot credibly be added after launch — people trust this editor with
//! their notes.
//!
//! | Direction | Approach |
//! |---|---|
//! | Markdown → `Document` | `pulldown-cmark` for **block structure only**; leaf text is captured as raw source slices, never as parsed inline events. Port `markdownToState.ts` (422 lines) semantics on top. |
//! | `Document` → Markdown | Port `stateToMarkdown.ts` (529 lines). Leaf blocks emit `text` verbatim; containers emit their markers. |
//! | `Document` → HTML | Port `renderToStaticHTML.ts`, sanitize with `ammonia`. |
//! | HTML → Markdown (paste) | `html5ever` + a port of turndown's rules; `htmd` evaluated first. |
//!
//! Using `pulldown-cmark` for blocks and **never** for inlines is the key
//! integration decision (§4). It buys spec-correct block parsing (>99 %
//! CommonMark) while leaving inline handling to `mt-inline`, which must match
//! muya's marker-preserving behaviour rather than the spec's normalised
//! output. Feeding inline events from `pulldown-cmark` into the document would
//! destroy the marker spans and silently break the WYSIWYG reveal behaviour.
//!
//! ## Dependency constraints
//!
//! Per §1:
//!
//! - **No windowing. No GPU. No I/O.** `mt-md` takes a `&str` and returns a
//!   `Document`; opening the file is `mt-fs`'s job.
//! - **No dependency on `mt-ui` or `mt-app`,** ever.
//! - Depends downward only, on `mt-doc` (and on `mt-inline` once inline
//!   re-derivation lands).
//!
//! This is what makes the conformance ratchet (§11.1) and the differential
//! harness (§11.2) runnable in CI on all three platforms without a display.
//!
//! ## Incremental reparse (§4.1)
//!
//! 1. An edit to a leaf's text usually changes nothing structurally:
//!    re-tokenize **only the edited block** and mark it layout-dirty. This is
//!    the common case and must be sub-millisecond.
//! 2. If the edit introduces a block-boundary trigger (blank line, list
//!    marker, `#`, fence delimiter, `>`, table pipe, `---`), re-run block
//!    parsing over the **minimal enclosing region** — previous blank line to
//!    next blank line — and diff the resulting subtree against the existing
//!    one.
//! 3. Full document reparse happens only on load and on external file change.
//!
//! ## M0 status
//!
//! Stub. The two entry points below exist so that the conformance ratchet and
//! the differential harness can be wired up and run in CI from day one; both
//! return [`Unimplemented`], which those harnesses report as *skipped* rather
//! than *failed*. See `spec/README.md` for how the ratchet flips on at M2.
//!
//! ## M2 S3 status — `parse` and `dump_state` answer; the other two do not
//!
//! [`block::parse_blocks`] maps `pulldown-cmark`'s event stream onto
//! `mt_doc::Block` and agrees with `MarkdownToState` on the **full `TState`
//! JSON** — names, `meta` and leaf text — over all 1344 of §4 C1's inputs and
//! all 4,985 leaves in them; [`state::to_state`] emits that JSON from a
//! [`Document`]. S3 wired both into [`parse`] and [`dump_state`], added the
//! label-map pass ([`labels`]) and kept the parse's per-node source ranges
//! ([`SourceMap`]). **The first of D6's three ratchets is awake:**
//! `cargo xtask diff` compares 22 whole documents where it used to skip them.
//!
//! **[`serialize`] and [`render_to_static_html`] still return
//! [`Unimplemented`], deliberately** — docs/M2.md §5 D6 stages the three
//! ratchets by entry point so that one harness at a time starts complaining,
//! and those two are S4's and S5's.
//!
//! `cargo xtask blocks` is the finer gate and it compares text by default from
//! S2 on; `--no-text` is what asks for less.
//!
//! The direction table above is M0's transcription of §4 and **§4 C2 corrects
//! its first row**: leaf text is not "captured as raw source slices" — it is a
//! per-kind reconstruction, and S2 is the stage that wrote the eight rules.
//! The second half of that sentence — never as parsed inline events — does
//! hold, and [`block`] never reads one.

pub mod block;
pub mod labels;
pub mod state;

use std::collections::BTreeMap;
use std::ops::Range;

use mt_doc::{Document, NodeId};
use mt_inline::Labels;

/// Returned by the entry points this milestone has not reached yet.
///
/// Callers in the test harnesses treat this as "skip", not "fail", so the
/// harnesses can run green in CI before there is a parser. When one of these
/// functions starts returning `Ok`, the harnesses begin enforcing — no harness
/// change required. That is the M2 flip described in `spec/README.md`, and
/// §5 D6 stages it one entry point at a time.
///
/// **[`parse`] and [`dump_state`] no longer return this at all**, and their
/// signatures say so: they are infallible. A `Result` whose `Err` arm no input
/// can reach is the shape this milestone keeps learning to distrust, so the arm
/// was removed rather than left as an unreachable branch.
/// [`serialize`] and [`render_to_static_html`] keep it because for them it is
/// still the truth, and `mt-cli`'s exit 3 is still how a caller in another
/// process hears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unimplemented;

impl std::fmt::Display for Unimplemented {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not implemented until M2 (RUST-REWRITE-PLAN.md §9)")
    }
}

impl std::error::Error for Unimplemented {}

/// Options mirroring muya's parse/render flags.
///
/// The names and defaults are taken from `MarkdownToState`'s
/// `IMarkdownToStateOptions` and from `renderToStaticHTML`'s options so that
/// the differential harness can drive both engines with identical settings.
/// Divergent options would make any disagreement uninterpretable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub footnote: bool,
    pub math: bool,
    pub super_sub_script: bool,
    pub gitlab_compatibility: bool,
    pub front_matter: bool,

    /// muya #1265: strip leading and trailing blank lines from a code block's
    /// text.
    ///
    /// **`false` in both option sets the differential harness drives**, so no
    /// fixture in §4 C1's 1344 can reach it and `cargo xtask blocks` cannot
    /// say whether it works. It is implemented rather than skipped at S2, and
    /// `block::tests::trim_unnecessary_code_block_empty_lines_is_off_in_both_option_sets_and_works`
    /// carries values measured from the running engine with the flag flipped —
    /// because "the gate cannot see it" is a reason to write the test, not a
    /// reason to leave the rule out.
    ///
    /// One detail that is not the obvious reading: the guard is
    /// `endsWith('\n') || startsWith('\n')` and the body strips **both** ends,
    /// so a block with a leading blank line and no trailing one loses nothing
    /// at the end and everything at the start, and a block with neither is
    /// untouched.
    pub trim_unnecessary_code_block_empty_lines: bool,

    /// How far a nested list is indented when [`serialize`] emits it.
    ///
    /// **Added at M2 S0.** The doc comment above says these names come from
    /// `MarkdownToState`'s options and `renderToStaticHTML`'s. There is a
    /// third source and M0 did not have it: `ExportMarkdown`'s constructor
    /// takes `{ listIndentation }` (`state/stateToMarkdown.ts:64`), which is
    /// the only option the serializer reads and which five of
    /// `listSerialization.spec.ts`'s cases vary. Carrying it here rather than
    /// widening [`serialize`]'s signature keeps one options type for the whole
    /// crate, which is what makes the differential harness able to drive both
    /// engines from one struct.
    pub list_indentation: ListIndentation,
}

/// `IExportMarkdownOptions.listIndentation` — a space count, or Daring
/// Fireball's fixed four.
///
/// muya's constructor clamps a number to `1..=4` and treats **any** non-number
/// that is not `"dfm"` as 1. That includes the desktop preferences UI's
/// `'tab'` value, which was never implemented — `listSerialization.spec.ts`
/// has a characterization case pinning the degraded behaviour, and it is
/// transcribed as [`ListIndentation::Spaces`]`(1)` with the same note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListIndentation {
    /// Clamped to `1..=4` on construction, exactly as muya's constructor does.
    Spaces(u8),
    /// Daring Fireball Markdown: a hard four spaces regardless of marker width.
    Dfm,
}

impl ListIndentation {
    /// muya's clamp: `Math.min(Math.max(listIndentation, 1), 4)`.
    pub const fn spaces(n: u8) -> Self {
        ListIndentation::Spaces(if n < 1 {
            1
        } else if n > 4 {
            4
        } else {
            n
        })
    }
}

impl Default for ListIndentation {
    /// `new ExportMarkdown()` with no argument defaults to `{ listIndentation: 1 }`.
    fn default() -> Self {
        ListIndentation::Spaces(1)
    }
}

impl Options {
    /// muya's `DEFAULT_OPTIONS` for `MarkdownToState`, used by the round-trip
    /// fixtures and by the §11.2 differential harness.
    pub const MUYA_DEFAULT: Self = Self {
        footnote: false,
        math: true,
        super_sub_script: false,
        gitlab_compatibility: true,
        front_matter: true,
        trim_unnecessary_code_block_empty_lines: false,
        list_indentation: ListIndentation::Spaces(1),
    };

    /// The flags the CommonMark and GFM spec runners use: every muya extension
    /// off, so the suites measure CommonMark/GFM compliance rather than
    /// muya's supersets. Matches `commonmark.spec.ts` and `gfm.spec.ts`.
    pub const SPEC: Self = Self {
        footnote: false,
        math: false,
        super_sub_script: false,
        gitlab_compatibility: false,
        front_matter: false,
        trim_unnecessary_code_block_empty_lines: false,
        list_indentation: ListIndentation::Spaces(1),
    };

    /// The same options with a different list indentation — the shape
    /// `listSerialization.spec.ts`'s five indentation cases need.
    #[must_use]
    pub const fn with_list_indentation(mut self, indentation: ListIndentation) -> Self {
        self.list_indentation = indentation;
        self
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::MUYA_DEFAULT
    }
}

/// Everything one parse knows: the tree, the label map, and where each node
/// came from.
///
/// [`parse`] returns this rather than a bare [`Document`] because two of the
/// three are facts about *the parse* and not about the document, and there is
/// nowhere on a `Document` to put them that stays true:
///
/// - **`document`** is the model the editor holds and edits.
/// - **`labels`** is what makes `[text][ref]` a reference link. muya rebuilds
///   it inside `patch()` — a full depth-first walk of the document on **every
///   block render**, which is a keystroke. That is a shape to notice rather
///   than to copy: it is rebuilt here once per parse, and [`labels::collect`]
///   is public so that an editor which has just changed a definition can redo
///   it deliberately rather than on every frame.
/// - **`source_map`** is [`SourceMap`], below.
///
/// The alternative shapes were considered and rejected in the same breath: a
/// side table hung off `Document` would be a field whose invariant no `Edit`
/// maintains, and a second entry point returning "parse, but with the extras"
/// would fork the only parse path in the crate so that the interesting half
/// could be forgotten.
#[derive(Debug)]
pub struct Parsed {
    /// The block tree — `MarkdownToState.generate()`'s `TState[]`, rooted.
    pub document: Document,
    /// The reference definitions this document defines, keyed lowercase.
    pub labels: Labels,
    /// Where every node's block came from in the markdown that was parsed.
    pub source_map: SourceMap,
}

/// Every node's source range, from the parse that built it.
///
/// # What it is for
///
/// A leaf's `text` is a *reconstruction* (§4 C2), so a byte offset into it does
/// not map back to a document offset by addition. Three later stages need the
/// reverse direction and all three need the same thing: S6's region reparse
/// (§4.1, D7), M4's caret placement from a click, and the per-leaf reverse
/// offset map M2.md §10 owes M3/M4.
///
/// §10's correction at S2 is why this is a map of **ranges** rather than of
/// offsets: `strip_lines` needs the source lines and the ancestor containers'
/// prefixes, the ancestors are the tree, and their prefixes are computable from
/// their ranges — so `(src, tree, ranges)` is enough to re-run the
/// container-prefix stripper for one block, lazily, at any later stage.
/// [`block::tests::the_stripper_is_re_runnable_from_src_tree_and_ranges`] is
/// that claim as a test rather than as a sentence.
///
/// # What it does not promise
///
/// **It is a fact about one parse, not an invariant of the document.** Nothing
/// updates it when an [`mt_doc::Edit`] is applied, and it is deliberately not
/// reachable from a [`Document`] so that it cannot be mistaken for something
/// that is. After an edit, the ranges of every node at or after the edit are
/// stale; the fix is another parse (or, at S6, a region reparse that produces
/// new ranges for the region), not a maintained side table.
///
/// The half that is **not** free from this map is the per-kind half: an atx
/// heading's text is rebuilt from scratch, a code block loses columns *after*
/// stripping, a table cell is trimmed and unescaped. Each of those needs its
/// own inverse and each inverse belongs beside its rule — there is no stage at
/// which that half falls out of anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    ranges: BTreeMap<NodeId, Range<usize>>,
}

impl SourceMap {
    /// The range `id`'s block was built from, or `None` for the root and for a
    /// node this map did not build.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<Range<usize>> {
        self.ranges.get(&id).cloned()
    }

    /// How many nodes carry a range. Every live node except the root does.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Every `(node, range)` pair, in `NodeId` order — which for a fresh parse
    /// is allocation order, which is document order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, Range<usize>)> + '_ {
        self.ranges.iter().map(|(id, range)| (*id, range.clone()))
    }

    pub(crate) fn insert(&mut self, id: NodeId, range: Range<usize>) {
        self.ranges.insert(id, range);
    }
}

/// Markdown → [`Parsed`]. Port of `markdownToState.ts`.
///
/// [`block::parse_blocks`] plus [`labels::collect`], which is what S1 recorded
/// this function would be. **Infallible:** every string is a document, exactly
/// as `MarkdownToState.generate()` is total.
///
/// Callers that want only the tree write `parse(md, options).document`.
#[must_use]
pub fn parse(markdown: &str, options: Options) -> Parsed {
    let (document, source_map) = block::parse_blocks_with_ranges(markdown, options);
    let labels = labels::collect(&document);
    Parsed {
        document,
        labels,
        source_map,
    }
}

/// [`Document`] → markdown. Port of `stateToMarkdown.ts`.
pub fn serialize(_doc: &Document, _options: Options) -> Result<String, Unimplemented> {
    Err(Unimplemented)
}

/// Markdown → static HTML. Port of `renderToStaticHTML.ts`.
///
/// This is what the conformance ratchet (§11.1) calls. The spec runners in
/// muya call it with `sanitize: false` deliberately — they measure the
/// *parser's* compliance, not the sanitiser, and CommonMark §6.9 "Raw HTML"
/// explicitly tests that unknown tags like `<bab>` survive. Keep that
/// distinction when this is implemented: sanitization is an export-time and
/// paste-time concern, not a parse-time one.
pub fn render_to_static_html(
    _markdown: &str,
    _options: Options,
    _sanitize: bool,
) -> Result<String, Unimplemented> {
    Err(Unimplemented)
}

/// Markdown → muya-compatible state JSON.
///
/// The Rust half of the §11.2 differential harness. The output must be
/// byte-comparable (after key-order-insensitive JSON comparison) with what
/// `@muyajs/core`'s `MarkdownToState.generate()` produces for the same input
/// and options — which is possible only because `mt_doc::Block` maps 1:1 onto
/// the TypeScript `TState` union.
///
/// Exposed on the command line as `mt-cli --dump-state`.
///
/// [`parse`] plus [`state::to_state_json`], and infallible for the same reason
/// `parse` is. `mt-cli` therefore no longer exits 3 for this command — the
/// exit code stays defined because S5's `--to-html` will need it again, and
/// `cargo xtask diff` still maps it to `SKIP` for the same reason.
#[must_use]
pub fn dump_state(markdown: &str, options: Options) -> String {
    state::to_state_json(&parse(markdown, options).document)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The M0 contract, minus the half S3 discharged: an entry point this
    /// milestone has not reached reports "unimplemented" rather than panicking,
    /// so the harnesses can distinguish *not built yet* from *broken*.
    ///
    /// **The `dump_state` assertion was deleted at S3**, in the commit where it
    /// first returned an answer, and `serialize`'s is the reminder for S4. D6's
    /// order is `parse`+`dump_state`, then `serialize`, then
    /// `render_to_static_html`; this test shrinks by one line per stage and
    /// disappears at S5.
    #[test]
    fn entry_points_report_unimplemented_at_m0() {
        assert_eq!(
            render_to_static_html("x", Options::SPEC, false),
            Err(Unimplemented)
        );
        assert_eq!(
            serialize(&parse("x", Options::MUYA_DEFAULT).document, Options::SPEC),
            Err(Unimplemented)
        );
    }

    /// D6's first ratchet, at its narrowest: the two entry points S3 opened
    /// return an answer at all.
    ///
    /// The *content* of that answer is `cargo xtask diff`'s claim over 22 whole
    /// documents and `cargo xtask blocks`'s over 1344 inputs — neither of which
    /// runs under `cargo test`, which is why this asserts the thing they
    /// cannot: that the door is open.
    #[test]
    fn parse_and_dump_state_answer_from_s3_on() {
        let parsed = parse("# hi\n\n[a]: /u\n", Options::MUYA_DEFAULT);
        assert_eq!(parsed.document.children(parsed.document.root()).len(), 2);
        assert_eq!(parsed.labels.len(), 1);
        assert_eq!(parsed.source_map.len(), 2);

        let json = dump_state("# hi\n", Options::MUYA_DEFAULT);
        assert!(json.contains("\"atx-heading\""), "{json}");
    }

    #[test]
    fn spec_options_disable_every_muya_extension() {
        // Matches commonmark.spec.ts / gfm.spec.ts. If these drift, the
        // conformance numbers stop being comparable with the 87.7 % / 86.3 %
        // baseline in spec/conformance.md.
        let o = Options::SPEC;
        assert!(!o.footnote);
        assert!(!o.math);
        assert!(!o.super_sub_script);
        assert!(!o.gitlab_compatibility);
        assert!(!o.front_matter);
    }

    /// muya's `ExportMarkdown` constructor clamps to `1..=4` and defaults to 1
    /// — including for the `'tab'` value the desktop preferences UI offers and
    /// nothing implements.
    #[test]
    fn list_indentation_clamps_the_way_export_markdown_does() {
        assert_eq!(ListIndentation::spaces(0), ListIndentation::Spaces(1));
        assert_eq!(ListIndentation::spaces(1), ListIndentation::Spaces(1));
        assert_eq!(ListIndentation::spaces(4), ListIndentation::Spaces(4));
        assert_eq!(ListIndentation::spaces(9), ListIndentation::Spaces(4));
        assert_eq!(ListIndentation::default(), ListIndentation::Spaces(1));
        assert_eq!(
            Options::MUYA_DEFAULT.list_indentation,
            ListIndentation::Spaces(1)
        );
    }
}
