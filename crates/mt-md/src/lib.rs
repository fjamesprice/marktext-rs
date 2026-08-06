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
//! `cargo xtask blocks` is the finer gate and it compares text by default from
//! S2 on; `--no-text` is what asks for less.
//!
//! ## M2 S4 status — `serialize` answers, and the round trip is a fixed point
//!
//! [`serialize`] is `stateToMarkdown.ts` transcribed ([`serialize`], 626 lines
//! of TypeScript) on top of muya's own [`string_width`], and it is **infallible
//! from this stage**: `stateToMarkdown.spec.ts` asks what happens to a document
//! `MarkdownToState` cannot produce, twice, and both times the answer is
//! degraded output rather than an error.
//!
//! `crates/mt-md/tests/round_trip.rs` is the gate that wakes with it, and it is
//! this crate's contract above run for the first time:
//! `parse(serialize(parse(s))) == parse(s)` over 1346 inputs — 11 corpus files,
//! 11 round-trip fixtures and all 1324 spec examples — plus the stronger
//! `serialize(parse(s)) == s` with the set on which it does not hold
//! enumerated rather than summarised.
//!
//! One measurement is worth carrying here rather than only in the plan:
//! **the port's serialization of all 1346 is byte-identical to
//! `ExportMarkdown.generate`'s.** That is what makes both exception lists
//! readable as facts about the *reference* serializer rather than as this
//! crate's losses.
//!
//! ## M2 S5 status — the last entry point, and the one-way door
//!
//! [`render_to_static_html`] answers, so **all three of §5 D6's ratchets are
//! awake** and `cargo xtask conformance` enforces where it used to skip 1,324
//! examples. §4 C3 decides this renders through [`Document`]; [`html`]'s
//! module docs carry the half of that decision S5 had to take, which is that
//! the leaves' inline layer is rendered from [`mt_inline::tokenizer`]'s tokens
//! rather than from a second inline engine — *the exporter must agree with the
//! editor*, which is C3's own argument one layer down.
//!
//! **What that costs is a number and it is frozen: 71.0 % CommonMark and
//! 70.8 % GFM, against muya's 87.7 % and 86.3 %.** The two measure different
//! engines; `spec/conformance.md` says which, and docs/M2.md §6 argues all 231
//! entries the ratchet's list gained. The block half is 83.7 % and the inline
//! half is 60.3 %, so `spec/expected-failures.json` is now a to-do list for
//! `mt-inline` — which is the most useful thing a ratchet can be.
//!
//! Three more things landed with it: [`Options::footnote`] finally gates
//! something ([`block`]'s port of muya's `marked` footnote extension, with
//! [`footnotes::transform_footnotes`] for the GFM/pandoc shape),
//! [`sanitize::clean`] is the `sanitize: true` arm, and `mt-cli` gained
//! `--to-html`.
//!
//! ## M2 S6 status — the reparse, and the map says which ranges may be sliced
//!
//! [`reparse::Incremental`] is §4.1 and §5 D7: a parse that survives edits.
//! `edit` splices the source and brings the tree, the label map and the source
//! ranges back into agreement with it — by re-deriving **one leaf** where the
//! edit cannot have moved a block boundary, and by re-parsing the minimal
//! enclosing region and diffing the result against the existing subtree where
//! it can. [`reparse::Reparse::blocks_reparsed`] is the gate's counter and it is
//! public rather than test-only.
//!
//! The diff matches on **full structural equality of a subtree**, so a node that
//! keeps its [`mt_doc::NodeId`] is a node whose content is identical — which is
//! what makes "`mt-layout` never gets a cached layout for a different block" a
//! property of the algorithm rather than of the inputs. Every mutation goes
//! through [`mt_doc::Document::apply`] (D9) and the inverse batch comes back
//! with the report.
//!
//! Two things landed with it. `block::split_refused_list_starts` pays §10's
//! "Owed by S4" — `marked` lexes a list item with `state.top = false` and the
//! rule about which list starts may interrupt a paragraph lives in the
//! top-level `paragraph` regex alone — which takes the transcribed spec suite's
//! `PENDING` from 11 to **2**, and both survivors are M6's. And [`SourceMap`]
//! gained [`SourceMap::is_exact`], because the map stopped being injective at S5
//! and S6 is the first code that would have been wrong about it.
//!
//! The direction table above is M0's transcription of §4 and **§4 C2 corrects
//! its first row**: leaf text is not "captured as raw source slices" — it is a
//! per-kind reconstruction, and S2 is the stage that wrote the eight rules.
//! The second half of that sentence — never as parsed inline events — does
//! hold, and [`block`] never reads one.

pub mod block;
pub mod footnotes;
pub mod html;
pub mod labels;
pub mod reparse;
pub mod sanitize;
pub mod serialize;
pub mod state;
pub mod string_width;

use std::collections::BTreeMap;
use std::ops::Range;

use mt_doc::{Document, NodeId};
use mt_inline::Labels;

/// The deepest a node may sit in a [`Document`] this crate produces, and the
/// deepest any walk in this crate descends.
///
/// # Why there is a limit at all (§11.3)
///
/// *"Malformed input must never panic — `panic = "abort"` makes a panic a
/// crash."* A **stack overflow is not a panic**: it is an abort with no
/// `catch_unwind`, no `Result` and no recovery, so the only way to honour that
/// clause is to make the deep walk impossible rather than to catch it. S7's
/// property tests and fuzz targets found that `"> "` repeated 2,000 times —
/// **4,001 bytes** — aborted `mt-cli --dump-state` on a plain release build.
///
/// The recursion is **this crate's**, not `pulldown-cmark`'s: its
/// `OffsetIter` is a position loop, and `Parser::new_ext(&"> ".repeat(200_000),
/// …).into_offset_iter().collect()` completes on a 1 MiB stack — 200,000 levels,
/// 156× the depth at which this crate's own walks die there.
/// [`block::tests::pulldown_cmark_is_iterative_over_container_depth`] is that
/// measurement as a test, so a future upstream rewrite that made it recursive
/// would say so here rather than in a crash report.
///
/// # Why 128, measured three ways
///
/// 1. **The deepest input this repository owns is 11.** Over all 1,347 —
///    `cargo xtask blocks`' 1,324 spec examples, the 11 round-trip fixtures and
///    the 12 corpus documents — the depth histogram is
///    `{1: 1102, 2: 60, 3: 130, 4: 23, 5: 25, 7: 4, 9: 2, 11: 1}`, and the 11
///    is `spec/fixtures/marktext-round-trip/common/Lists.md`. So 128 is 11.6×
///    the deepest markdown anyone in this project has written, and **no
///    existing test can reach the limit** — which is what makes this change
///    provably behaviour-preserving over the whole measured corpus.
///    `round_trip_properties::the_deepest_input_this_repository_owns_is_an_order_of_magnitude_below_the_limit`
///    re-measures it rather than trusting this paragraph.
/// 2. **The reference engine has no limit to copy.** muya dies too — see the
///    at-the-limit note on [`parse`] — so the number is this port's own choice
///    and the third measurement is what fixes it.
/// 3. **The stack it has to fit in.** `mt-cli`'s main thread on Windows is the
///    PE default of 1 MiB, and the cost per nesting level, measured by binary
///    search on a thread of exactly 1 MiB, is worst for
///    [`render_to_static_html`]: **≈ 1.0 KiB/level in release and ≈ 2.8 KiB/level
///    in `dev`** (995 and 369 levels respectively before the abort). 128 levels
///    is therefore ≈ 131 KiB release and ≈ 355 KiB debug — **a 2.9× margin in
///    the worst configuration**, which is the one a `cargo test` run uses. 256
///    would leave 1.4×, which is not a margin.
///
/// # What is *not* claimed
///
/// This bounds recursion over **container nesting**, which is the axis a
/// document's shape controls. It is not a bound on `mt_inline`'s tokenizer
/// (measured non-recursive over nesting to depth 1,000 at S7 and unchanged
/// here), nor on the one input class where `pulldown-cmark` aborts before this
/// crate runs at all (`docs/upstream-issues.md`, and [`parse`]'s docs).
pub const MAX_NESTING_DEPTH: usize = 128;

/// Every leaf at or below `ids`, in document order — what the three walks that
/// take a [`Document`] emit when they reach [`MAX_NESTING_DEPTH`].
///
/// # Why they emit anything at all
///
/// [`parse`] cannot build a document this deep, so the only way to reach the
/// limit here is to hand one of them a tree built with `mt_doc::Edit::InsertNode`
/// — and the reference engine's answer to that is a `RangeError`, which is no
/// answer. Returning the leaves keeps the content; returning nothing would make
/// `serialize` silently empty a document, because muya's block-quote serializer
/// carries the `>` markers in the *indent* it passes down and realises them only
/// at a leaf, so a clamp that stopped descending would drop the markers too.
/// `block::clamp_depth` reparents leaves for the same reason and this is the
/// same rule one layer up.
///
/// Iterative, like everything else on this path: the tree it is asked about is
/// by definition the one that is too deep to recurse over.
pub(crate) fn leaves_below(doc: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    let mut leaves = Vec::new();
    let mut work: Vec<NodeId> = ids.iter().rev().copied().collect();
    while let Some(id) = work.pop() {
        let children = doc.children(id);
        if children.is_empty() {
            leaves.push(id);
        } else {
            work.extend(children.iter().rev().copied());
        }
    }
    leaves
}

/// Returned by the entry points this milestone has not reached yet.
///
/// Callers in the test harnesses treat this as "skip", not "fail", so the
/// harnesses can run green in CI before there is a parser. When one of these
/// functions starts returning `Ok`, the harnesses begin enforcing — no harness
/// change required. That is the M2 flip described in `spec/README.md`, and
/// §5 D6 stages it one entry point at a time.
///
/// **[`parse`] and [`dump_state`] stopped returning this at S3, and
/// [`serialize`] at S4.** Their signatures say so: they are infallible. A
/// `Result` whose `Err` arm no input can reach is the shape this milestone
/// keeps learning to distrust, so the arm is removed in the stage that opens
/// the entry point rather than left as an unreachable branch.
/// [`render_to_static_html`] keeps it because for it that is still the truth,
/// and `mt-cli`'s exit 3 is still how a caller in another process hears it.
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
/// **S6 kept that commitment and it is worth being precise about what it
/// means.** [`reparse::Incremental`] hands back a map whose region entries came
/// from the parse that just ran, whose entries before the edit are untouched,
/// and whose entries after it are shifted by the edit's delta — a shift being
/// exact rather than a patch. `tests/reparse_properties.rs` checks the whole of
/// it against a full parse's map, leaf by leaf, after every generated edit.
///
/// The half that is **not** free from this map is the per-kind half: an atx
/// heading's text is rebuilt from scratch, a code block loses columns *after*
/// stripping, a table cell is trimmed and unescaped. Each of those needs its
/// own inverse and each inverse belongs beside its rule — there is no stage at
/// which that half falls out of anything.
/// # Not injective, and [`SourceMap::is_exact`] is how a caller sees it
///
/// Two mechanisms re-lex a **de-indented copy** of the source rather than a
/// slice of it — muya's footnote extension (S5) and the `state.top = false`
/// list-item re-lex (S6) — and an offset into a copy does not map back to a
/// document offset by addition. §4 C2's fact, one level up. Every node those
/// two produce therefore carries a range that is *true but not tight*: coarse
/// enough to nest and to order correctly, not tight enough to slice with.
///
/// So two nodes can share a range, and `is_exact` is the difference between a
/// range a caller may re-derive text from and one it may only locate a block
/// with. S6's leaf-text fast path consults it before re-running the stripper,
/// which is the check that stopped `20.` from being spliced as though it were
/// a paragraph's own text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    ranges: BTreeMap<NodeId, Range<usize>>,
    /// The nodes whose range is a coarse stand-in. Empty for most documents,
    /// which is why it is a set of exceptions rather than a flag per entry.
    coarse: std::collections::BTreeSet<NodeId>,
}

impl SourceMap {
    /// The range `id`'s block was built from, or `None` for the root and for a
    /// node this map did not build.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<Range<usize>> {
        self.ranges.get(&id).cloned()
    }

    /// Whether `id`'s range is its own extent rather than a coarse stand-in —
    /// see the type's docs. `true` for a node this map has never heard of,
    /// because there is nothing coarse about it either.
    #[must_use]
    pub fn is_exact(&self, id: NodeId) -> bool {
        !self.coarse.contains(&id)
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
        self.insert_with(id, range, true);
    }

    pub(crate) fn insert_with(&mut self, id: NodeId, range: Range<usize>, exact: bool) {
        self.ranges.insert(id, range);
        if exact {
            self.coarse.remove(&id);
        } else {
            self.coarse.insert(id);
        }
    }

    /// Drop a node's range. S6's subtree diff detaches nodes that no longer
    /// exist in the tree, and their entries go with them — the map's invariant
    /// is one entry per **live** node.
    pub(crate) fn remove(&mut self, id: NodeId) {
        self.ranges.remove(&id);
        self.coarse.remove(&id);
    }
}

/// The **file layer's** normalisation: strip a UTF-8 BOM, then fold `\r\n` and
/// a lone `\r` to `\n`.
///
/// # This is not the parser's job, and S3 measured that the obvious reading is
/// backwards
///
/// `marked`'s `Lexer.lex` opens with `src.replace(/\r\n|\r/g, '\n')`, which
/// reads as though the parser normalises. **muya does not call `lex`** — its
/// `lexBlock` calls `new Lexer(defaults).blockTokens(src)` directly, skipping
/// the preprocessing step — so muya's parser sees carriage returns raw, and
/// asked directly it proves it: `"a\r\nb\r\n"` is one paragraph whose text is
/// `"a\r\nb\r"`. Putting this inside [`parse`] would make the port disagree
/// with the reference engine on any string a caller hands it directly.
///
/// So this is the transcription of `tools/diff/dump-ts-state.mjs`'s
/// `readMarkdown`, whose M0 doc comment already said *"The Rust side must do
/// the same, and `mt-fs` owns that in the real application"*. `mt-cli` is what
/// calls it today, `mt-fs` is where it moves when there is a file layer, and
/// `crates/mt-md/tests/round_trip.rs` calls it because it reads the same files.
///
/// It is a pure `&str → String`, so it does not put I/O in this crate: §1's
/// constraint is about `src/` reaching the filesystem, and this function
/// reaches nothing.
///
/// **One consequence for M4**, recorded on [`SourceMap`] as well: the ranges a
/// parse returns index the string `parse` was given, so a range from a file
/// read through this function is an offset into the *normalised* text and not
/// into the bytes on disk.
#[must_use]
pub fn normalize_source(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

/// Markdown → [`Parsed`]. Port of `markdownToState.ts`.
///
/// [`block::parse_blocks`] plus [`labels::collect`], which is what S1 recorded
/// this function would be. **Infallible:** every string is a document, exactly
/// as `MarkdownToState.generate()` is total — with the one exception below,
/// which is `pulldown-cmark`'s and not this crate's.
///
/// # The one exception, and it is not this crate's
///
/// `docs/M2.md` §10, *"Owed by S6"*, item 1: **`pulldown-cmark` 0.13.4 panics**
/// on a document holding a list item, a link reference definition inside it,
/// and a following line that is whitespace-only and not all spaces —
/// `"> - [a]: /x\n\t"` is the minimal one. `Option::unwrap()` on `None` inside
/// `OffsetIter::next` (`parse.rs:2199`), reached before any of this crate's
/// code runs, so the totality above is false for that class alone. There is no
/// published version to upgrade to and the release profile is `panic = "abort"`
/// (§12), so catching it is not open either; §10 owes the decision to M4.
/// `docs/upstream-issues.md` records it and
/// [`block::tests::pulldown_cmark_panics_on_a_definition_in_a_quoted_list_item_before_a_tab_line`]
/// is `#[should_panic]`, so the day upstream fixes it the test says so.
///
/// **This crate's own violation of §11.3's *"malformed input must never
/// panic"* was repaired at S7** and is not a second exception: the dedent in
/// [`block`]'s container-prefix stripper cut a whitespace-only continuation
/// line at a byte that need not be a `char` boundary. See
/// [`block::tests::a_wide_whitespace_continuation_line_dedents_to_nothing`].
///
/// # Nesting is clamped, and the reference engine is not where the number came
/// from
///
/// No tree this returns is deeper than [`MAX_NESTING_DEPTH`]. At the limit the
/// parse **stops opening containers and the rest of that container's source
/// becomes one paragraph's text** — so `"> "` × 2000 is 127 block quotes whose
/// paragraph reads `"> " × 1873 + "x"`, and serializing it reproduces the input
/// byte for byte. [`block::Builder::would_exceed_depth`] argues that choice
/// against the two alternatives.
///
/// **muya has no limit to copy: it dies too, and where it dies is an artifact.**
/// Measured against `@muyajs/core` at `MARKTEXT_REF` on Node 24.14, one process
/// per depth, by binary search on `"> " × n + "x"`:
///
/// | muya entry point | deepest `n` that returns | first `n` that throws |
/// |---|---:|---:|
/// | `MarkdownToState.generate` | 1953 | 1992 |
/// | `+ ExportMarkdown.generate` | 1757 | 1796 |
/// | `+ JSON.stringify` | 1953 | 1992 |
/// | `renderToStaticHTML({sanitize: false})` | 1953 | 1992 |
/// | `renderToStaticHTML({sanitize: true})` | 1875 | 1914 |
///
/// Every one of those is `RangeError: Maximum call stack size exceeded`, and
/// the number is V8's stack rather than a decision: the same measurement at
/// `--stack-size=500` gives 938 and at `--stack-size=4000` gives 9550, a
/// straight line through the interpreter's stack size. There is no muya
/// behaviour to reproduce here, so the limit is **this port's stated choice**,
/// argued on [`MAX_NESTING_DEPTH`] from the three things that *are* measurable.
/// What the port keeps is the one thing the reference engine's answer implies:
/// a `RangeError` returns nothing at all, so any output is closer to muya's
/// intent than a crash, and content that survives the clamp is a bonus rather
/// than a divergence.
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
///
/// **Infallible from M2 S4**, and the argument is in [`serialize`]'s module
/// docs rather than here because it is longer than a signature: a `Document`
/// can be hand-built with a shape `MarkdownToState` cannot express, so "can it
/// fail" was the wrong question and "what does muya do" is the right one —
/// `stateToMarkdown.spec.ts` asks it twice, of a table row with too many cells
/// and of one with too few, and both times the answer is degraded output rather
/// than an error.
///
/// The only option it reads is [`Options::list_indentation`], which is
/// `ExportMarkdown`'s only constructor argument.
#[must_use]
pub fn serialize(doc: &Document, options: Options) -> String {
    serialize::to_markdown(doc, options)
}

/// Markdown → static HTML. Port of `renderToStaticHTML.ts`.
///
/// This is what the conformance ratchet (§11.1) calls. The spec runners in
/// muya call it with `sanitize: false` deliberately — they measure the
/// *parser's* compliance, not the sanitiser, and CommonMark §6.9 "Raw HTML"
/// explicitly tests that unknown tags like `<bab>` survive. **That distinction
/// survives here**: sanitization is an export-time and paste-time concern, not
/// a parse-time one, and it is [`sanitize::clean`] rather than anything the
/// renderer does.
///
/// # The four steps, in muya's order
///
/// 1. Empty input fast-paths to an empty string, so callers do not special-case
///    it — muya's `if (!markdown) return ''`.
/// 2. [`parse`], then [`html::to_html`]. §4 C3 decides this goes through
///    [`Document`]; `html`'s module docs carry the half of that decision S5 had
///    to take, which is what renders the inline layer.
/// 3. [`footnotes::transform_footnotes`], **before** sanitization, because the
///    `data-identifier` marker it reads is a `data-*` attribute and the export
///    sanitizer config drops those.
/// 4. [`sanitize::clean`], unless the caller asked for the raw output.
///
/// **Infallible from M2 S5** in everything but its signature, which keeps
/// [`Unimplemented`] because `mt-cli`'s exit 3 is a contract with
/// `cargo xtask diff` and `--to-html` is the command that inherits it. The
/// three entry points that opened before this one each dropped the `Result` in
/// the stage that opened them; this one is the last and it does not, because
/// M6's `mt-export` will wrap it and a `Result` is the shape an export path
/// wants. Recorded rather than assumed — see the crate docs on
/// [`Unimplemented`].
pub fn render_to_static_html(
    markdown: &str,
    options: Options,
    sanitize: bool,
) -> Result<String, Unimplemented> {
    if markdown.is_empty() {
        return Ok(String::new());
    }
    let parsed = parse(markdown, options);
    let mut rendered = html::to_html(&parsed.document, &parsed.labels, options);
    if options.footnote {
        rendered = footnotes::transform_footnotes(&rendered);
    }
    Ok(if sanitize {
        sanitize::clean(&rendered)
    } else {
        rendered
    })
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

    // `entry_points_report_unimplemented_at_m0` lived here from M0 until S5.
    // It shrank by one clause per stage by design — `dump_state`'s went at S3,
    // `serialize`'s at S4 — and its own doc comment named this stage as the one
    // it would disappear in, because `render_to_static_html` was the last
    // clause and D6 stages it last. It is gone rather than inverted: the claim
    // "the door is open" is what `render_to_static_html_answers_from_s5_on`
    // below makes, and `spec/README.md`'s ratchet is what makes it mean
    // something.

    /// D6's ratchets, at their narrowest: the entry points S3 and S4 opened
    /// return an answer at all.
    ///
    /// The *content* of those answers is `cargo xtask diff`'s claim over 22
    /// whole documents, `cargo xtask blocks`'s over 1344 inputs, and
    /// `tests/round_trip.rs`'s over 1346 — the first two of which do not run
    /// under `cargo test`, which is why this asserts the thing they cannot:
    /// that the doors are open.
    #[test]
    fn parse_and_dump_state_answer_from_s3_on() {
        let parsed = parse("# hi\n\n[a]: /u\n", Options::MUYA_DEFAULT);
        assert_eq!(parsed.document.children(parsed.document.root()).len(), 2);
        assert_eq!(parsed.labels.len(), 1);
        assert_eq!(parsed.source_map.len(), 2);

        let json = dump_state("# hi\n", Options::MUYA_DEFAULT);
        assert!(json.contains("\"atx-heading\""), "{json}");
    }

    /// D6's third and last ratchet. The two clauses that cannot be seen from
    /// `cargo xtask conformance`, which drives this function with
    /// `sanitize: false` only: that the sanitize arm runs at all, and that the
    /// empty-input fast path returns `Ok("")` rather than `<p></p>`.
    #[test]
    fn render_to_static_html_answers_from_s5_on() {
        let raw = render_to_static_html(
            "# hi

<script>x</script>",
            Options::SPEC,
            false,
        )
        .expect("S5 opened this door");
        assert!(raw.contains("<h1>hi</h1>"), "{raw}");
        assert!(raw.contains("<script>"), "sanitize: false is raw: {raw}");

        let clean = render_to_static_html(
            "# hi

<script>x</script>",
            Options::SPEC,
            true,
        )
        .expect("S5 opened this door");
        assert!(clean.contains("<h1>hi</h1>"), "{clean}");
        assert!(!clean.contains("<script"), "{clean}");

        assert_eq!(
            render_to_static_html("", Options::SPEC, true),
            Ok(String::new()),
            "muya's `if (!markdown) return ''`"
        );
    }

    /// D6's second ratchet, the same way.
    #[test]
    fn serialize_answers_from_s4_on() {
        let source = "# hi\n\n- a\n- b\n";
        let parsed = parse(source, Options::MUYA_DEFAULT);
        assert_eq!(serialize(&parsed.document, Options::MUYA_DEFAULT), source);
    }

    /// The file layer's rule, and the assertion that `parse` does **not** apply
    /// it — muya's `lexBlock` calls `blockTokens` directly and never sees
    /// `Lexer.lex`'s preprocessing, so its parser keeps carriage returns and so
    /// does this one.
    #[test]
    fn normalize_source_is_the_file_layers_rule_and_not_the_parsers() {
        assert_eq!(normalize_source("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(normalize_source("a\rb\r"), "a\nb\n");
        assert_eq!(normalize_source("\u{feff}# h\n"), "# h\n");
        // A BOM that is not at position 0 is content, not a BOM.
        assert_eq!(normalize_source("a\u{feff}b\n"), "a\u{feff}b\n");

        let parsed = parse("a\r\nb\r\n", Options::MUYA_DEFAULT);
        let text = parsed
            .document
            .block(parsed.document.children(parsed.document.root())[0])
            .and_then(mt_doc::Block::text)
            .expect("a paragraph");
        assert_eq!(text.to_str(), "a\r\nb\r", "measured from muya, not chosen");
    }

    /// **The three entry points that take a [`Document`] must be safe on one
    /// this crate did not build**, which a limit in [`parse`] cannot give them:
    /// `mt_doc::Edit::InsertNode` reaches any depth without going near
    /// `mt-md`, and `crates/mt-doc/tests/edit_inverse.rs` generates trees that
    /// way already.
    ///
    /// 5,000 levels is ≈ 14 MB of frames at the `dev` build's measured cost
    /// (see [`MAX_NESTING_DEPTH`]), so every one of these aborted before S7 —
    /// on a 1 MiB main thread and on a `cargo test` thread alike.
    ///
    /// [`render_to_static_html`] is deliberately absent: it takes `&str` and
    /// parses, so there is no way to hand it a foreign document and
    /// [`block::parse_blocks`]'s limit is the whole of its answer.
    /// [`html::to_html`] is the part of it that does take one, and it is here.
    #[test]
    fn every_entry_point_survives_a_document_no_parse_could_have_built() {
        use mt_doc::{Block, Edit};

        const DEPTH: usize = 5_000;
        let mut doc = Document::new();
        let mut parent = doc.root();
        for _ in 0..DEPTH {
            // `InsertNode`'s inverse is `RemoveNode` carrying the minted id —
            // the same way `block::insert` reads one, and the reason M2.md §5
            // D9's "no edit path may bypass `Document::apply`" costs nothing.
            let inverse = doc.apply(&[Edit::InsertNode {
                parent,
                index: doc.children(parent).len(),
                block: Block::BlockQuote {
                    children: Vec::new(),
                },
            }]);
            let [Edit::RemoveNode { node }] = inverse.as_slice() else {
                unreachable!("InsertNode's inverse is RemoveNode")
            };
            parent = *node;
        }
        doc.apply(&[Edit::InsertNode {
            parent,
            index: 0,
            block: Block::Paragraph {
                text: mt_doc::Text::from("[a]: /u".to_string()),
            },
        }]);

        // Each of these used to overflow the stack on this document. The
        // assertions are deliberately weak — that they *return* is the claim.
        let labels = labels::collect(&doc);
        assert_eq!(labels.len(), 1, "the walk reached the deepest paragraph");

        // The blocks below the limit are flattened onto the deepest one that
        // is inside it, so all three keep the paragraph rather than emptying
        // the document — `leaves_below`'s docs argue for that over the
        // alternative, which for `serialize` is losing the `>` markers too.
        let quotes = "> ".repeat(MAX_NESTING_DEPTH - 1);
        let markdown = serialize(&doc, Options::MUYA_DEFAULT);
        assert!(
            markdown.contains(&format!("{quotes}[a]: /u")),
            "{markdown:?}"
        );

        let json = state::to_state_json(&doc);
        assert_eq!(
            json.matches("\"block-quote\"").count(),
            MAX_NESTING_DEPTH - 1,
            "one per level inside the limit and none below it"
        );

        let rendered = html::to_html(&doc, &labels, Options::MUYA_DEFAULT);
        assert_eq!(
            rendered.matches("<blockquote>").count(),
            MAX_NESTING_DEPTH - 1
        );
    }

    /// The constant is the number the docs quote, in one place, so that a
    /// change to it is a change to the paragraph that argues for it.
    #[test]
    fn the_nesting_limit_is_the_number_its_own_documentation_states() {
        assert_eq!(MAX_NESTING_DEPTH, 128);
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
