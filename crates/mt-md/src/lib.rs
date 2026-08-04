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
//! ## M2 S1 status — the block tree exists, the entry points still do not
//!
//! [`block::parse_blocks`] maps `pulldown-cmark`'s event stream onto
//! `mt_doc::Block` and agrees with `MarkdownToState` on names and `meta` over
//! all 1344 of §4 C1's inputs; [`state::to_state`] emits muya's `TState[]`
//! from a [`Document`]. **[`parse`], [`serialize`], [`dump_state`] and
//! [`render_to_static_html`] all still return [`Unimplemented`], deliberately**
//! — docs/M2.md §5 D6 stages the three ratchets by entry point, and the first
//! `Ok` from any of them wakes one. S3 is where `parse` becomes
//! `parse_blocks` plus leaf text plus the label-map pass.
//!
//! Every leaf [`block::parse_blocks`] builds carries the **empty string**:
//! §4 C2 measured that a leaf's text is a per-kind reconstruction rather than
//! a source slice, and that reconstruction is S2's. `cargo xtask blocks` is
//! the gate, and it says which fields it compares rather than quietly
//! excluding one.
//!
//! The direction table above is M0's transcription of §4 and **§4 C2 corrects
//! its first row**: leaf text is not "captured as raw source slices". The
//! second half of that sentence — never as parsed inline events — does hold,
//! and [`block`] never reads one.

pub mod block;
pub mod state;

use mt_doc::Document;

/// Returned by every entry point in this crate at M0.
///
/// Callers in the test harnesses treat this as "skip", not "fail", so the
/// harnesses can run green in CI before there is a parser. When these
/// functions start returning `Ok`, the harnesses begin enforcing — no harness
/// change required. That is the M2 flip described in `spec/README.md`.
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

/// Markdown → [`Document`]. Port of `markdownToState.ts`.
pub fn parse(_markdown: &str, _options: Options) -> Result<Document, Unimplemented> {
    Err(Unimplemented)
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
pub fn dump_state(_markdown: &str, _options: Options) -> Result<String, Unimplemented> {
    Err(Unimplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The M0 contract: every entry point reports "unimplemented" rather than
    /// panicking, so the harnesses can distinguish *not built yet* from
    /// *broken*. When these start returning `Ok`, this test is the reminder
    /// to delete it and let the harnesses enforce.
    #[test]
    fn entry_points_report_unimplemented_at_m0() {
        assert_eq!(
            render_to_static_html("x", Options::SPEC, false),
            Err(Unimplemented)
        );
        assert_eq!(dump_state("x", Options::MUYA_DEFAULT), Err(Unimplemented));
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
