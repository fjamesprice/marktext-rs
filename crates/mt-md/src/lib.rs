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
    };
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
}
