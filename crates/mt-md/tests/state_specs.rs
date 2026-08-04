//! The 187 in-scope cases of `packages/muya/src/state/__tests__/`, transcribed.
//!
//! M2.md §4 C6 is the finding these exist because of: §14 step 5 names
//! `inlineRenderer/__tests__/` and M1 read that as the only transcribable
//! suite in muya. `state/__tests__/` holds **26 files and 221 cases**, and
//! they test exactly the two files this milestone ports.
//!
//! §14 step 5's method applies here — transcribe first, make them pass
//! afterwards — so S0 transcribes rather than S1 discovering.
//!
//! # The counts, and the correction to §6's S0 row
//!
//! §6's S0 row says 215 in-scope cases and §7's inventory says 221 total.
//! Counted against the source rather than against the row, the in-scope number
//! is **187**. The arithmetic, because M2.md §6 asks for it to be done against
//! `PENDING` rather than against the row:
//!
//! | Excluded | Cases | Why it has no `mt-doc`/`mt-md` surface |
//! |---|---:|---|
//! | `htmlToMarkdown` | 11 | §1's excluded row — M5's paste path |
//! | `flushPendingOps` | 4 | the editor's operation cache, `muya.ts` |
//! | `setContentClearsOpCache` | 2 | same |
//! | `parityExportHtml` | 6 | `MarkdownToHtml.generate()` — inlined CSS, heading slugs, the TOC stylesheet |
//! | `exportHtmlDir` | 3 | `MarkdownToHtml.generate({ dir })` — `<html dir="rtl">` |
//! | `footnoteExportHtml` | 2 | `MarkdownToHtml.renderHtml()` — the export path's footnote pass |
//! | `mermaidExportResilience` | 2 | `MarkdownToHtml.renderHtml()` with a mocked mermaid module |
//! | `blockSerialization`'s live-`CodeBlock` cases | 3 | `codeBlock.lang = 'js'`, a DOM class swap, and an OT-op flush |
//! | `diagramFlowchartSequence`'s `sequenceTheme` case | 1 | `MUYA_DEFAULT_OPTIONS.sequenceTheme` — a diagram *rendering* default |
//! | | **34** | 221 − 34 = **187** |
//!
//! The first three are §7's own 17. The next four are one class: `markdownToHtml.ts`
//! is the **standalone document** export — `<article class="markdown-body">`,
//! inlined stylesheets, async diagram rendering — and §2's port table does not
//! list it. That is `mt-export`, which §9 schedules at M6 and whose crate doc
//! already says "Emit HTML (from `mt-md`)". §4 C3 is explicit that
//! `renderToStaticHTML` is what M2 ports, and `renderToStaticHTML.spec.ts`
//! itself asserts the export-only behaviours are *absent* from it (case 9
//! here: "returns the bare body HTML, no `<article>` wrapper"; case 2:
//! "id-less `<hN>`"). Transcribing them against `render_to_static_html` would
//! assert the opposite of what muya asserts.
//!
//! **None of the 34 is debt.** Each is owed to a named later milestone, and
//! M2.md's S0 section repeats this table so it is findable from the plan.
//!
//! # One definitional correction to §7
//!
//! §7's table says *"DOM" means the file carries `// @vitest-environment
//! happy-dom`*. Ten files do; **five carry `jsdom` instead** —
//! `exportHtmlDir`, `htmlToMarkdown`, `mermaidExportResilience`,
//! `parityExportHtml` and `renderToStaticHTML`. The ● marks in §7's table are
//! right about which files need a DOM (102 cases across 15 files, leaving
//! §7's 119 with none); only the sentence defining the mark is.
//!
//! # The pending ratchet — the fourth instance
//!
//! `spec/README.md`: *"Three registers, one shape, on purpose."* This is the
//! fourth, and it is copied from `crates/mt-inline/tests/inline_renderer_specs.rs`
//! rather than reinvented.
//!
//! | State | Result |
//! |---|---|
//! | Listed, still failing | passes — it is on the list |
//! | Listed, now **passing** | **fails** — delist it |
//! | Unlisted, passing | passes |
//! | Unlisted, now failing | **fails**, with the original panic |
//!
//! Row two is what makes it a ratchet rather than `#[ignore]`: getting better
//! must also fail the build, so the list is forced down instead of
//! accumulating entries that later let a fixed case quietly regress.
//!
//! **The injected-list variant is carried over from M1 S5.** M1 emptied
//! `PENDING` and found that an empty list leaves row two with nothing to test
//! against — so emptying the list would silently switch the ratchet off. The
//! same guard is here from the start rather than added at the end: see
//! [`the_ratchet_fails_a_listed_case_that_starts_passing`].
//!
//! # What a green run claims, and how that changed at S3
//!
//! **At S0** every entry point returned `Err(Unimplemented)`, so all 184 listed
//! cases failed and were meant to. A green run claimed two things: the cases
//! compile against the real API, and each failed because the engine was
//! unimplemented rather than because a type was missing —
//! `every_pending_case_fails_because_the_engine_is_unimplemented` was that
//! claim, and this stage deleted it, as its own doc comment said to.
//!
//! **From S3** `mt_md::parse` answers, so a listed case fails for one of two
//! reasons: the entry point it needs is still closed (`render_to_static_html`
//! at S5) or the port does not agree with muya yet. The second kind is the
//! interesting one and it is enumerated rather than left in the pile — see
//! [`the_only_listed_cases_that_fail_for_their_own_reason_are_the_ten_m2_names`].
//!
//! Three of the 187 were **not** listed at S0, because they passed then: the
//! `strongCjkFlanking` cases that go through the inline tokenizer rather than
//! through `renderToStaticHTML`. M1 ported the CJK flanking widening
//! (`crates/mt-inline/src/emphasis.rs`), so those inputs already tokenize to
//! `strong`/`em`. Row two of the table above demands they be delisted, and
//! M1's S1 argument applies unchanged: leaving a passing case listed means the
//! list stops measuring what is implemented.
//!
//! **S3 delisted 63**, leaving 121 — the arithmetic and the three corrections
//! it forced on M2.md's §6 and §7 are in that document's "S3's verification".
//! **S4 delisted 79**, leaving 42: thirty-two of them are S5's
//! `render_to_static_html`, one is S5's footnote block extension, and the other
//! nine are the finding S4 turned up, enumerated in `KNOWN` below.
//!
//! **Caveat, inherited:** the ratchet uses `catch_unwind`, so
//! `cargo test --release` aborts rather than catching (the release profile
//! sets `panic = "abort"`). CI runs the dev profile, where this is fine.

use std::panic::{self, AssertUnwindSafe};

use mt_doc::{
    Align, Block, BlockMeta, BulletMarker, CodeKind, DiagramKind, DiagramLang, Document, Edit,
    MathStyle, NodeId, OrderDelim, Text, Underline,
};
use mt_md::{ListIndentation, Options};

// ---------------------------------------------------------------------------
// Option presets — one per `generate()` helper in the TypeScript suite
// ---------------------------------------------------------------------------

/// `{ footnote: false, math: false, isGitlabCompatibilityEnabled: false,
/// trimUnnecessaryCodeBlockEmptyLines: false, frontMatter: false }` — the
/// helper `markdownToState.spec.ts`, `listSerialization.spec.ts`,
/// `referenceLink.spec.ts`, `diagramFlowchartSequence.spec.ts`,
/// `codeFenceLength.spec.ts` and `stateToMarkdown.spec.ts` all share.
const NO_EXT: Options = Options {
    footnote: false,
    math: false,
    super_sub_script: false,
    gitlab_compatibility: false,
    front_matter: false,
    trim_unnecessary_code_block_empty_lines: false,
    list_indentation: ListIndentation::Spaces(1),
};

/// `new MarkdownToState()` with no argument — `DEFAULT_OPTIONS` at
/// `state/markdownToState.ts:27`, which is `Options::MUYA_DEFAULT` field for
/// field. Used by `codeFenceInfoString`, `infoStringModel` and
/// `tableEscapedPipe`.
const MUYA_DEFAULT: Options = Options::MUYA_DEFAULT;

/// `gitlabMath.spec.ts`'s `parse()`: math and gitlab compatibility on,
/// everything else off.
const GITLAB_MATH: Options = Options {
    math: true,
    gitlab_compatibility: true,
    ..NO_EXT
};

/// `blockSerialization.spec.ts`: math and front matter on, gitlab off.
const BLOCKS: Options = Options {
    math: true,
    front_matter: true,
    ..NO_EXT
};

/// `nestedMixedLists.spec.ts`: math, gitlab and front matter all on.
const NESTED_LISTS: Options = Options {
    math: true,
    gitlab_compatibility: true,
    front_matter: true,
    ..NO_EXT
};

/// `mathTrailingSpace.spec.ts`: math only.
const MATH_ONLY: Options = Options {
    math: true,
    ..NO_EXT
};

/// `renderToStaticHTML`'s own defaults, per its option-surface tests:
/// `superSubScript` on, everything else off.
const RENDER_DEFAULT: Options = Options {
    super_sub_script: true,
    ..NO_EXT
};

// ---------------------------------------------------------------------------
// Engine entry points, wrapped so a failure names the reason
// ---------------------------------------------------------------------------
//
// At S0 every one of these panicked with `Unimplemented`, which is what the
// listed cases failed on. **From S3, `parse` is infallible** — it returns
// `mt_md::Parsed` and this helper keeps only the tree, which is what the
// TypeScript specs assert against; the label map and the source ranges have
// their own tests in `mt_md`. `serialize` and `html` still panic, and D6's
// order is what makes that the interesting fact about the remaining list.
//
// They are deliberately not `Result`-returning: a transcribed spec should read
// like its TypeScript original, and its original does not handle an error the
// engine cannot produce once it is implemented.

fn parse(markdown: &str, options: Options) -> Document {
    mt_md::parse(markdown, options).document
}

fn serialize(doc: &Document, options: Options) -> String {
    mt_md::serialize(doc, options)
}

/// `markdown → state → markdown`, the shape most of these specs assert on.
fn round_trip(markdown: &str, options: Options) -> String {
    serialize(&parse(markdown, options), options)
}

fn html(markdown: &str, options: Options, sanitize: bool) -> String {
    mt_md::render_to_static_html(markdown, options, sanitize)
        .unwrap_or_else(|e| panic!("mt_md::render_to_static_html: {e:?} — {e}"))
}

// ---------------------------------------------------------------------------
// Tree navigation
// ---------------------------------------------------------------------------

/// The document's top-level blocks — what `MarkdownToState.generate()`
/// returns, and what `dump_state` serializes. The root itself is a synthetic
/// container with no `TState` counterpart (`mt_doc::Node`).
fn top(doc: &Document) -> Vec<NodeId> {
    doc.children(doc.root()).to_vec()
}

fn name(doc: &Document, id: NodeId) -> &'static str {
    doc.block(id).expect("a block").name()
}

fn names(doc: &Document, ids: &[NodeId]) -> Vec<&'static str> {
    ids.iter().map(|id| name(doc, *id)).collect()
}

fn kids(doc: &Document, id: NodeId) -> Vec<NodeId> {
    doc.children(id).to_vec()
}

fn text(doc: &Document, id: NodeId) -> String {
    doc.block(id)
        .and_then(Block::text)
        .unwrap_or_else(|| panic!("{} is not a leaf", name(doc, id)))
        .to_str()
        .into_owned()
}

fn meta(doc: &Document, id: NodeId) -> BlockMeta {
    doc.block(id)
        .and_then(Block::meta)
        .unwrap_or_else(|| panic!("{} has no meta", name(doc, id)))
}

/// The first descendant-or-self at this level whose name matches — the
/// `children.find(c => c.name === '…')` the TypeScript specs use.
fn child_named(doc: &Document, id: NodeId, wanted: &str) -> Option<NodeId> {
    kids(doc, id).into_iter().find(|c| name(doc, *c) == wanted)
}

fn top_named(doc: &Document, wanted: &str) -> Option<NodeId> {
    top(doc).into_iter().find(|c| name(doc, *c) == wanted)
}

/// The text of a list item's leading paragraph — `firstText` in
/// `nestedMixedLists.spec.ts`.
fn first_text(doc: &Document, id: NodeId) -> Option<String> {
    kids(doc, id)
        .first()
        .filter(|c| doc.block(**c).and_then(Block::text).is_some())
        .map(|c| text(doc, *c))
}

// ---------------------------------------------------------------------------
// Building a document from literal states
// ---------------------------------------------------------------------------

/// A literal state tree, for the specs that construct `TState[]` by hand and
/// hand it straight to `ExportMarkdown` — `stateToMarkdown.spec.ts`'s tables,
/// and `listSerialization.spec.ts`'s nested-empty-item cases.
#[derive(Debug, Clone)]
enum S {
    Leaf(Block),
    Node(Block, Vec<S>),
}

fn para(text: &str) -> S {
    S::Leaf(Block::Paragraph {
        text: Text::from(text),
    })
}

fn cell(text: &str, align: Align) -> S {
    S::Leaf(Block::TableCell {
        align,
        text: Text::from(text),
    })
}

fn row(cells: Vec<S>) -> S {
    S::Node(
        Block::TableRow {
            children: Vec::new(),
        },
        cells,
    )
}

fn table(rows: Vec<S>) -> S {
    S::Node(
        Block::Table {
            children: Vec::new(),
        },
        rows,
    )
}

fn list_item(children: Vec<S>) -> S {
    S::Node(
        Block::ListItem {
            children: Vec::new(),
        },
        children,
    )
}

fn task_item(checked: bool, children: Vec<S>) -> S {
    S::Node(
        Block::TaskListItem {
            checked,
            children: Vec::new(),
        },
        children,
    )
}

fn empty_item() -> S {
    list_item(vec![para("")])
}

fn bullet_list(marker: BulletMarker, loose: bool, children: Vec<S>) -> S {
    S::Node(
        Block::BulletList {
            marker,
            loose,
            children: Vec::new(),
        },
        children,
    )
}

fn order_list(start: u32, delimiter: OrderDelim, loose: bool, children: Vec<S>) -> S {
    S::Node(
        Block::OrderList {
            start,
            delimiter,
            loose,
            children: Vec::new(),
        },
        children,
    )
}

fn task_list(marker: BulletMarker, loose: bool, children: Vec<S>) -> S {
    S::Node(
        Block::TaskList {
            marker,
            loose,
            children: Vec::new(),
        },
        children,
    )
}

/// Build a document whose top-level blocks are `states`.
fn doc_of(states: Vec<S>) -> Document {
    let mut doc = Document::new();
    // `Document::new` seeds one empty paragraph; drop it, so the tree is
    // exactly the `TState[]` the spec wrote.
    let seed = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: seed }]);
    doc.prune_detached();
    let root = doc.root();
    for (index, state) in states.iter().enumerate() {
        attach(&mut doc, root, index, state);
    }
    doc.clear_dirty();
    doc
}

fn attach(doc: &mut Document, parent: NodeId, index: usize, state: &S) {
    let block = match state {
        S::Leaf(block) | S::Node(block, _) => block.clone(),
    };
    doc.apply(&[Edit::InsertNode {
        parent,
        index,
        block,
    }]);
    let id = doc.children(parent)[index];
    if let S::Node(_, children) = state {
        for (i, child) in children.iter().enumerate() {
            attach(doc, id, i, child);
        }
    }
}

// ---------------------------------------------------------------------------
// The ratchet
// ---------------------------------------------------------------------------

/// Run one transcribed case under the ratchet's decision table.
fn spec_case(name: &str, case: fn()) {
    let listed = PENDING.contains(&name);
    let outcome = panic::catch_unwind(AssertUnwindSafe(case));

    match (listed, outcome) {
        // Listed, still failing — expected while the engine is unimplemented.
        (true, Err(_)) => {}
        // Listed, now passing — the ratchet's whole point.
        (true, Ok(())) => panic!(
            "{name} is listed in PENDING but PASSES.\n\
             Delete it from the list — a passing case that stays listed stops the list \
             measuring what is implemented, and lets the case regress unnoticed later."
        ),
        // Unlisted, passing — a normal green test.
        (false, Ok(())) => {}
        // Unlisted, now failing — a regression, with the original panic.
        (false, Err(payload)) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            panic!(
                "{name} is NOT listed in PENDING and FAILED — this is a regression.\n\
                 Original panic: {message}"
            );
        }
    }
}

/// Emit a module's cases: a body `fn` per case, a `#[test]` wrapper that runs
/// it through [`spec_case`], and a `CASES` table so the whole suite can be
/// driven by [`every_pending_case_fails_because_the_engine_is_unimplemented`]
/// as well as by `cargo test`.
macro_rules! spec_cases {
    ($module:literal, $( $(#[$meta:meta])* fn $case:ident() $body:block )*) => {
        mod case {
            #![allow(unused_imports)]
            // The crate root's helpers, then the enclosing module's — a spec
            // file that needs a helper of its own (a fixture constant, a
            // shared predicate) defines it beside its cases rather than in the
            // root, where nineteen modules would have to read past it.
            use super::super::*;
            use super::*;
            $( $(#[$meta])* pub fn $case() $body )*
        }

        pub(super) const CASES: &[(&str, fn())] = &[
            $( (concat!($module, "::", stringify!($case)), case::$case as fn()), )*
        ];

        $(
            #[test]
            fn $case() {
                crate::spec_case(concat!($module, "::", stringify!($case)), case::$case);
            }
        )*
    };
}

// ---------------------------------------------------------------------------
// The suite
// ---------------------------------------------------------------------------

// One module per TypeScript spec file, in `state_specs/`.
//
// `#[path]` on every one of them because this file *is* the crate root of its
// test binary, so a bare `mod x;` would resolve to `tests/x.rs` — which cargo
// would then also compile as a test binary of its own. The
// `tests/common/mod.rs` convention avoids that by naming the directory after
// the module; here the directory is named after the suite, so the path is
// spelled out instead.
#[path = "state_specs/block_serialization.rs"]
mod block_serialization;
#[path = "state_specs/code_fence_info_string.rs"]
mod code_fence_info_string;
#[path = "state_specs/code_fence_length.rs"]
mod code_fence_length;
#[path = "state_specs/diagram_flowchart_sequence.rs"]
mod diagram_flowchart_sequence;
#[path = "state_specs/footnote_html.rs"]
mod footnote_html;
#[path = "state_specs/gitlab_math.rs"]
mod gitlab_math;
#[path = "state_specs/gitlab_math_toggle.rs"]
mod gitlab_math_toggle;
#[path = "state_specs/info_string_model.rs"]
mod info_string_model;
#[path = "state_specs/list_marker_option.rs"]
mod list_marker_option;
#[path = "state_specs/list_serialization.rs"]
mod list_serialization;
#[path = "state_specs/markdown_to_state.rs"]
mod markdown_to_state;
#[path = "state_specs/math_trailing_space.rs"]
mod math_trailing_space;
#[path = "state_specs/nested_mixed_lists.rs"]
mod nested_mixed_lists;
#[path = "state_specs/reference_link.rs"]
mod reference_link;
#[path = "state_specs/render_to_static_html.rs"]
mod render_to_static_html;
#[path = "state_specs/soft_break_export_html.rs"]
mod soft_break_export_html;
#[path = "state_specs/state_to_markdown.rs"]
mod state_to_markdown;
#[path = "state_specs/strong_cjk_flanking.rs"]
mod strong_cjk_flanking;
#[path = "state_specs/table_escaped_pipe.rs"]
mod table_escaped_pipe;

/// Every transcribed case, with the function that runs it.
///
/// Built at runtime rather than as a `const` because the per-module tables are
/// separate `const`s and const slices do not concatenate. Nothing reads it on
/// the hot path — it exists so the whole suite can be driven as data by the
/// three tests at the bottom of this file, not only by `cargo test`.
fn all_cases() -> Vec<(&'static str, fn())> {
    let mut cases = Vec::new();
    for module in [
        block_serialization::CASES,
        code_fence_info_string::CASES,
        code_fence_length::CASES,
        diagram_flowchart_sequence::CASES,
        footnote_html::CASES,
        gitlab_math::CASES,
        gitlab_math_toggle::CASES,
        info_string_model::CASES,
        list_marker_option::CASES,
        list_serialization::CASES,
        markdown_to_state::CASES,
        math_trailing_space::CASES,
        nested_mixed_lists::CASES,
        reference_link::CASES,
        render_to_static_html::CASES,
        soft_break_export_html::CASES,
        state_to_markdown::CASES,
        strong_cjk_flanking::CASES,
        table_escaped_pipe::CASES,
    ] {
        cases.extend_from_slice(module);
    }
    cases
}

/// The 187 in-scope cases, and where each one lives.
///
/// Kept beside [`PENDING`] because the two numbers only mean anything
/// together: `TRANSCRIBED` is what §7's inventory promised and `PENDING` is
/// what is not yet delivered, and
/// [`the_transcribed_counts_match_the_typescript_suite`] fails if either
/// drifts from the file it describes.
const TRANSCRIBED: &[(&str, usize)] = &[
    ("blockSerialization", 19),
    ("codeFenceInfoString", 4),
    ("codeFenceLength", 3),
    ("diagramFlowchartSequence", 5),
    ("footnoteHtml", 6),
    ("gitlabMath", 16),
    ("gitlabMathToggle", 4),
    ("infoStringModel", 3),
    ("listMarkerOption", 6),
    ("listSerialization", 30),
    ("markdownToState", 32),
    ("mathTrailingSpace", 3),
    ("nestedMixedLists", 4),
    ("referenceLink", 8),
    ("renderToStaticHTML", 20),
    ("softBreakExportHtml", 3),
    ("stateToMarkdown", 11),
    ("strongCjkFlanking", 6),
    ("tableEscapedPipe", 4),
];

/// Cases that do not pass yet.
///
/// **This list only shrinks.** Delete an entry the moment its case passes —
/// [`spec_case`] fails the build if you do not. Grouped by source spec file and
/// listed in file order, so a stage that fixes one file's worth of cases is one
/// contiguous deletion.
///
/// Empty is the M2 exit gate (§6's closing table, clause 1).
///
/// # 121 of 187, from 184 at S0
///
/// **S3 delisted 63 in one commit**, which is D6's staging working as designed:
/// `parse` is one entry point and every case that needed only `parse` stopped
/// failing on the same day. §6's S3 row predicted 54 and a second reading of §7
/// predicted 86; neither is the answer, and M2.md's "S3's verification" has the
/// arithmetic and the three shapes that produced it. What is left needs
/// `serialize` (S4), `render_to_static_html` (S5), or one of the three
/// disagreements M2.md §10's "Owed by S3" names.
///
/// At S0 every entry point of `mt_md` returned `Err(Unimplemented)`, so every
/// case that touched one failed. The three exceptions go through
/// `mt_inline::tokenizer` instead —
/// `strong_cjk_flanking::editor_path_*` — and M1 already ported the CJK
/// flanking widening they assert, so they pass. Row two of the ratchet table
/// requires a passing case to be delisted, and M1 S1's argument for doing that
/// even when the pass looks like a freebie applies unchanged: a listed case
/// that passes means the list has stopped measuring what is implemented, and
/// the moment a later stage breaks it the harness would report "still pending"
/// rather than "regression".
///
/// # What each stage deletes
///
/// §7's per-stage column is a prediction written before the code, and M1's
/// experience is that it will be wrong — five of its eight stage rows needed a
/// correction, always in the same direction, because a spec file spans stages.
/// **Do the arithmetic against this list, not against the row**, and correct
/// M2.md's row when it is wrong rather than reading it loosely.
///
/// The prediction, for the record, so that the correction has something to
/// correct: S1 the block structure (`gitlabMath`'s parse half,
/// `diagramFlowchartSequence`, `codeFence*`, `infoStringModel`,
/// `nestedMixedLists`, most of `markdownToState`), S2 leaf text
/// (`tableEscapedPipe`, `mathTrailingSpace`), S3 `parse` + `dump_state`
/// (`referenceLink`), S4 `serialize` (`listSerialization`, `stateToMarkdown`,
/// `blockSerialization`, `listMarkerOption`), S5 `render_to_static_html`
/// (`renderToStaticHTML`, `footnoteHtml`, `softBreakExportHtml`,
/// `strongCjkFlanking`'s static half).
///
/// Kept as a Rust `const` rather than a JSON file in `spec/` for the reason
/// `mt-inline`'s does: it has exactly one consumer — this binary — and a
/// `const` needs no parser, no dependency and no path resolution. Move it to
/// `spec/` if `xtask` ever needs to report on it.
const PENDING: &[&str] = &[
    // list_serialization — 9 of 30, and they are S4's finding rather than S4's
    // debt. Every one fails at a **reparse**: the serializer emits what muya
    // emits, and the port then reads it back differently, because CommonMark
    // will not let an empty bullet (`  * `) or an ordered marker that is not 1
    // (`   20. `) interrupt a paragraph inside a list item and `marked` will.
    // `marked` re-lexes an item's dedented content line by line with
    // `state.top = false`, so the interruption rules never apply there at all.
    // That is S1's layer and a wider mechanism than the one §10 owed S4 —
    // M2.md §10's "Owed by S4" carries it with its reproducers, and
    // `block::tests::the_three_shapes_the_task_marker_fix_does_not_reach` pins
    // the neighbouring shapes.
    "list_serialization::uses_an_alternate_nested_marker_instead_of_making_a_tight_list_loose",
    "list_serialization::uses_the_same_safe_marker_under_ordered_and_task_list_parents",
    "list_serialization::handles_a_first_empty_nested_item_followed_by_a_non_empty_item",
    "list_serialization::indent_by_1_space_round_trips_the_marktext_fixture",
    "list_serialization::indent_by_2_spaces_round_trips_the_marktext_fixture",
    "list_serialization::indent_by_3_spaces_round_trips_the_marktext_fixture",
    "list_serialization::indent_by_4_spaces_round_trips_the_marktext_fixture",
    "list_serialization::indent_using_daring_fireball_round_trips_the_marktext_fixture",
    "list_serialization::treats_the_unimplemented_tab_option_as_a_1_space_indent",
    // markdown_to_state — 1 of 32. `Options::footnote` has no block extension;
    // §10's "Owed by S3" item 3 puts it at S5, with `footnoteHtml`'s six.
    "markdown_to_state::converts_block_level_footnote_tokens_into_footnote_states",
    // render_to_static_html — 20. S5's, and the largest single block of what is
    // left.
    "render_to_static_html::renders_a_simple_paragraph",
    "render_to_static_html::renders_headings_with_id_less_h_tags",
    "render_to_static_html::renders_bullet_and_ordered_lists",
    "render_to_static_html::renders_fenced_code_blocks_with_a_language_class",
    "render_to_static_html::renders_mermaid_code_blocks_as_inert_placeholders",
    "render_to_static_html::renders_vega_lite_and_plantuml_code_blocks_as_inert_placeholders",
    "render_to_static_html::strips_inline_event_handler_attributes",
    "render_to_static_html::strips_script_tags",
    "render_to_static_html::returns_the_bare_body_html_with_no_article_wrapper",
    "render_to_static_html::honours_super_sub_script",
    "render_to_static_html::emits_sup_and_sub_wrappers_mixed_with_surrounding_text",
    "render_to_static_html::emits_sup_and_sub_wrappers_inside_list_items_and_headings",
    "render_to_static_html::honours_gitlab_compatibility_for_math_fences",
    "render_to_static_html::honours_the_front_matter_option",
    "render_to_static_html::honours_the_footnote_option",
    "render_to_static_html::honours_the_math_option",
    "render_to_static_html::returns_a_string_for_empty_input",
    "render_to_static_html::preserves_arbitrary_raw_html_tags_when_sanitize_is_false",
    "render_to_static_html::does_not_strip_script_when_sanitize_is_false",
    "render_to_static_html::still_strips_script_when_sanitize_is_true",
    // footnote_html — 6. S5's, and the stage that lands the footnote extension.
    "footnote_html::emits_a_footnotes_section_with_an_li_for_a_single_ref_and_def_pair",
    "footnote_html::numbers_inline_references_in_source_order",
    "footnote_html::leaves_an_orphan_inline_reference_as_plain_text",
    "footnote_html::points_every_repeated_inline_reference_to_the_same_target",
    "footnote_html::preserves_a_footnote_definition_containing_a_nested_bullet_list",
    "footnote_html::does_not_transform_a_literal_reference_inside_a_fenced_code_block",
    // soft_break_export_html — 3. S5's.
    "soft_break_export_html::keeps_a_paragraph_soft_break_as_a_newline_never_a_br",
    "soft_break_export_html::keeps_a_soft_break_inside_a_tight_list_item_never_a_br",
    "soft_break_export_html::leaves_a_real_hard_break_as_a_br",
    // strong_cjk_flanking — 3 of 6, the static/export half. The other three go
    // through the inline tokenizer and passed at S0.
    "strong_cjk_flanking::static_path_recognises_strong_in_the_sanity_cases",
    "strong_cjk_flanking::static_path_recognises_strong_in_cjk_context",
    "strong_cjk_flanking::static_path_does_not_bold_or_italicise_the_negative_cases",
];

// ---------------------------------------------------------------------------
// The ratchet's own tests
// ---------------------------------------------------------------------------

/// The whole suite is transcribed, and the counts are the ones §7's inventory
/// promised minus S0's correction.
///
/// This is the arithmetic M2.md §6 asks to be done against `PENDING` rather
/// than against the row: it fails if a module gains or loses a case without
/// this table being updated, which is what stops the milestone's progress
/// meter drifting from the code.
#[test]
fn the_transcribed_counts_match_the_typescript_suite() {
    let cases = all_cases();
    let total: usize = TRANSCRIBED.iter().map(|(_, n)| n).sum();
    assert_eq!(
        cases.len(),
        total,
        "the per-module table and the case tables disagree"
    );
    assert_eq!(
        total, 187,
        "221 TypeScript it() call sites minus S0's 34 exclusions"
    );

    // Every name is unique, or `PENDING` could not address a case
    // unambiguously.
    let mut names: Vec<&str> = cases.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "duplicate case name");
}

/// Every name in [`PENDING`] names a real case.
///
/// A stale entry silently weakens the ratchet by one case — the same failure
/// `spec/README.md` records for `expected-failures.json`, where a listed
/// number that names no example stops guarding anything.
#[test]
fn every_pending_entry_names_a_transcribed_case() {
    let cases = all_cases();
    let known: Vec<&str> = cases.iter().map(|(name, _)| *name).collect();
    let unknown: Vec<&&str> = PENDING.iter().filter(|p| !known.contains(p)).collect();
    assert!(
        unknown.is_empty(),
        "PENDING names cases that do not exist: {unknown:?}"
    );
    assert_eq!(
        PENDING.len(),
        42,
        "187 transcribed; 3 passed at S0, S3 delisted 63 and S4 delisted 79"
    );
}

// `every_pending_case_fails_because_the_engine_is_unimplemented` lived here
// until M2 S3, and its own doc comment named this stage as the one to delete it
// in. It drove all 184 listed cases and asserted each panic named
// `Unimplemented` — which was the S0 gate's third clause, machine-checked: at
// S0 there was exactly one legitimate reason to fail, so "listed and failing
// for the *right* reason" was checkable and a transcription error was not
// hiding behind the list.
//
// From S3 that is no longer true and the test would fail for the right reason
// at the wrong time: `parse` answers, so a listed case now fails either because
// `serialize`/`render_to_static_html` are still closed **or** because the port
// does not agree with muya yet — and the second is the whole point of the list.
// The ratchet's four rows are what guard it from here.

/// **The listed cases that fail for a reason of their own**, and the claim that
/// they are the only ones.
///
/// From S3 a listed case fails for one of two reasons: the entry point it needs
/// is still closed (`render_to_static_html` at S5), or the port does not agree
/// with muya yet. The first is bookkeeping and the second is a finding — and a
/// list of 42 hides the difference, which is exactly how the S0 test this
/// replaces stopped being able to tell them apart.
///
/// So the second kind is enumerated. Each name here is a disagreement M2.md
/// records, with its measured reproducer and the stage that owes it; the
/// assertion is that **no further one is hiding in the list**. A new name
/// appearing means a stage introduced a disagreement, and a name that stops
/// failing means one was fixed without this being updated.
///
/// # It went from three to ten at S4, and that is two events rather than one
///
/// **Two came off**: `- [ ] \ntext` folds into the task item now
/// (`Builder::fold_empty_task_marker_continuations`), which is the fix §10
/// owed S4.
///
/// **Nine went on**, and they were invisible until `serialize` opened: every
/// one is a `listSerialization` round trip that reaches the *reparse* it could
/// not reach before. They are one mechanism — `marked` re-lexes a list item's
/// dedented content line by line, so a list start inside an item never has to
/// interrupt a paragraph, and CommonMark's rules about which list starts may do
/// so never apply. Nine cases, two shapes: an **empty** bullet (`  * `) and an
/// ordered marker that is **not 1** (`   20. `).
///
/// That is the shape §6 predicted would appear at every stage and the reason it
/// asks for the arithmetic to be run rather than read: a file marked S4 whose
/// cases need S1's layer.
#[test]
fn the_only_listed_cases_that_fail_for_their_own_reason_are_the_ten_m2_names() {
    /// M2.md §10. Nine are "Owed by S4" — the port's parse of markdown its own
    /// serializer produced, where `marked` starts a list inside a list item and
    /// CommonMark keeps the paragraph open. One is "Owed by S3" item 3,
    /// `Options::footnote`'s missing block extension, which is S5's.
    const KNOWN: &[&str] = &[
        "list_serialization::uses_an_alternate_nested_marker_instead_of_making_a_tight_list_loose",
        "list_serialization::uses_the_same_safe_marker_under_ordered_and_task_list_parents",
        "list_serialization::handles_a_first_empty_nested_item_followed_by_a_non_empty_item",
        "list_serialization::indent_by_1_space_round_trips_the_marktext_fixture",
        "list_serialization::indent_by_2_spaces_round_trips_the_marktext_fixture",
        "list_serialization::indent_by_3_spaces_round_trips_the_marktext_fixture",
        "list_serialization::indent_by_4_spaces_round_trips_the_marktext_fixture",
        "list_serialization::indent_using_daring_fireball_round_trips_the_marktext_fixture",
        "list_serialization::treats_the_unimplemented_tab_option_as_a_1_space_indent",
        "markdown_to_state::converts_block_level_footnote_tokens_into_footnote_states",
    ];

    let mut found = Vec::new();
    for (name, case) in all_cases() {
        if !PENDING.contains(&name) {
            continue;
        }
        let Err(payload) = panic::catch_unwind(AssertUnwindSafe(case)) else {
            // Row two's business; the per-case test reports it properly.
            continue;
        };
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        // The one helper that still panics names itself in the panic.
        // `serialize`'s clause came out at S4, when it stopped being able to.
        if !message.contains("mt_md::render_to_static_html") {
            found.push((name, message.lines().next().unwrap_or("").to_string()));
        }
    }

    let names: Vec<&str> = found.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        KNOWN,
        "the set of listed cases failing for their own reason changed.\n\
         Found:\n{}\n\
         If a name is new, M2.md's owed list needs it before this test does. \
         If one is gone, delete it from KNOWN in the commit that fixed it.",
        found
            .iter()
            .map(|(n, m)| format!("  {n}: {m}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// **Row two, and the injected-list variant M1 S5 had to invent.**
///
/// The ratchet's interesting claim is that a *listed* case which starts
/// passing fails the build. Testing that needs a listed case that passes, and
/// by construction there is never one in `PENDING` — so the decision table is
/// driven here over an injected list instead, exactly as
/// `crates/mt-inline/tests/inline_renderer_specs.rs` does.
///
/// M1 added this at S5, when emptying `PENDING` left row two with nothing to
/// test against and emptying the list would therefore have switched the
/// ratchet off silently. It is here from the start rather than at the end,
/// because the state it guards against is the state this suite is aiming for.
#[test]
fn the_ratchet_fails_a_listed_case_that_starts_passing() {
    fn decide(listed: bool, passes: bool) -> bool {
        // The same four-way decision `spec_case` makes, over an injected list.
        match (listed, passes) {
            (true, false) => true,   // listed, still failing — green
            (true, true) => false,   // listed, now passing — FAILS
            (false, true) => true,   // unlisted, passing — green
            (false, false) => false, // unlisted, now failing — FAILS
        }
    }

    assert!(decide(true, false), "listed and failing must pass");
    assert!(
        !decide(true, true),
        "listed and PASSING must fail — this is the ratchet"
    );
    assert!(decide(false, true), "unlisted and passing must pass");
    assert!(!decide(false, false), "unlisted and failing must fail");
}

/// And the decision table above is the one `spec_case` actually runs.
///
/// The test above asserts a table; this asserts that [`spec_case`] implements
/// it, by driving the real function over synthetic cases and a real entry in
/// `PENDING`. Without this the two could drift and the ratchet would be a
/// documented intention rather than a mechanism.
#[test]
fn spec_case_implements_that_decision_table() {
    fn passing() {}
    fn failing() {
        panic!("Unimplemented");
    }

    let listed = PENDING[0];
    let unlisted = "not_a_module::not_a_case";
    assert!(!PENDING.contains(&unlisted));

    // Listed, still failing — no panic.
    spec_case(listed, failing);
    // Unlisted, passing — no panic.
    spec_case(unlisted, passing);

    // Listed, now passing — must panic, and say to delist it.
    let err = panic::catch_unwind(|| spec_case(listed, passing))
        .expect_err("a listed case that passes must fail the build");
    let message = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("Delete it from the list"), "{message}");

    // Unlisted, now failing — must panic, carrying the original.
    let err = panic::catch_unwind(|| spec_case(unlisted, failing))
        .expect_err("an unlisted case that fails must fail the build");
    let message = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("regression"), "{message}");
    assert!(
        message.contains("Unimplemented"),
        "the original panic is carried: {message}"
    );
}
