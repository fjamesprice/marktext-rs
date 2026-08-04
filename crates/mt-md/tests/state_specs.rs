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
//! # What a green run at S0 does and does not claim
//!
//! `mt_md::parse`, `serialize`, `dump_state` and `render_to_static_html` all
//! return `Err(Unimplemented)`, so **every one of the 184 listed cases fails,
//! and is meant to.** A green `cargo test --workspace` at S0 claims exactly
//! two things about this file: the cases exist and compile against the real
//! API, and each one fails because the engine is unimplemented rather than
//! because a type is missing. [`every_pending_case_fails_because_the_engine_is_unimplemented`]
//! is that claim, machine-checked.
//!
//! Three of the 187 are **not** listed, because they pass today: the
//! `strongCjkFlanking` cases that go through the inline tokenizer rather than
//! through `renderToStaticHTML`. M1 ported the CJK flanking widening
//! (`crates/mt-inline/src/emphasis.rs`), so those inputs already tokenize to
//! `strong`/`em`. Row two of the table above demands they be delisted, and
//! M1's S1 argument applies unchanged: leaving a passing case listed means the
//! list stops measuring what is implemented.
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
// Every one of these panics with `Unimplemented` at S0, which is what the
// listed cases fail on and what
// `every_pending_case_fails_because_the_engine_is_unimplemented` checks for.
// They are deliberately not `Result`-returning: a transcribed spec should read
// like its TypeScript original, and its original does not handle an error the
// engine cannot produce once it is implemented.

fn parse(markdown: &str, options: Options) -> Document {
    mt_md::parse(markdown, options).unwrap_or_else(|e| panic!("mt_md::parse: {e:?} — {e}"))
}

fn serialize(doc: &Document, options: Options) -> String {
    mt_md::serialize(doc, options).unwrap_or_else(|e| panic!("mt_md::serialize: {e:?} — {e}"))
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
/// # 184 of 187, and the three that are not here
///
/// At S0 every entry point of `mt_md` returns `Err(Unimplemented)`, so every
/// case that touches one fails. The three exceptions go through
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
    // block_serialization — 19
    "block_serialization::round_trips_an_atx_heading_at_every_level",
    "block_serialization::round_trips_a_setext_h1_with_equals_underline",
    "block_serialization::round_trips_a_setext_h2_with_dashes_underline",
    "block_serialization::round_trips_a_thematic_break",
    "block_serialization::round_trips_a_fenced_code_block_with_a_language_tag",
    "block_serialization::round_trips_a_fenced_code_block_without_a_language_tag",
    "block_serialization::round_trips_a_fenced_code_block_containing_blank_lines",
    "block_serialization::round_trips_a_single_line_blockquote",
    "block_serialization::round_trips_a_multi_line_blockquote",
    "block_serialization::round_trips_a_nested_blockquote",
    "block_serialization::round_trips_a_dollar_dollar_math_block",
    "block_serialization::round_trips_a_simple_2x2_table_with_default_alignment",
    "block_serialization::round_trips_a_table_with_explicit_alignment",
    "block_serialization::round_trips_a_cell_containing_an_escaped_pipe",
    "block_serialization::serialises_an_empty_trailing_cell_as_a_blank_cell",
    "block_serialization::parses_a_four_space_indented_block_as_an_indented_code_block",
    "block_serialization::round_trips_an_indented_code_block",
    "block_serialization::round_trips_a_multi_line_indented_code_block",
    "block_serialization::round_trips_a_yaml_frontmatter_block",
    // code_fence_info_string — 4
    "code_fence_info_string::preserves_a_pandoc_style_attribute_info_string",
    "code_fence_info_string::preserves_a_language_followed_by_attributes",
    "code_fence_info_string::leaves_a_plain_single_word_language_unchanged",
    "code_fence_info_string::leaves_a_language_less_fence_unchanged",
    // code_fence_length — 3
    "code_fence_length::keeps_a_fence_long_enough_to_wrap_content_containing_a_triple_backtick",
    "code_fence_length::round_trips_a_long_fenced_block_byte_stably",
    "code_fence_length::still_uses_a_plain_three_backtick_fence_for_ordinary_blocks",
    // diagram_flowchart_sequence — 5
    "diagram_flowchart_sequence::parses_a_flowchart_fence_as_a_diagram_block",
    "diagram_flowchart_sequence::parses_a_sequence_fence_as_a_diagram_block",
    "diagram_flowchart_sequence::round_trips_a_flowchart_diagram_block",
    "diagram_flowchart_sequence::round_trips_a_sequence_diagram_block",
    "diagram_flowchart_sequence::still_parses_mermaid_plantuml_and_vega_lite",
    // footnote_html — 6
    "footnote_html::emits_a_footnotes_section_with_an_li_for_a_single_ref_and_def_pair",
    "footnote_html::numbers_inline_references_in_source_order",
    "footnote_html::leaves_an_orphan_inline_reference_as_plain_text",
    "footnote_html::points_every_repeated_inline_reference_to_the_same_target",
    "footnote_html::preserves_a_footnote_definition_containing_a_nested_bullet_list",
    "footnote_html::does_not_transform_a_literal_reference_inside_a_fenced_code_block",
    // gitlab_math — 16
    "gitlab_math::promotes_a_math_fence_to_a_gitlab_styled_math_block",
    "gitlab_math::leaves_a_math_fence_as_a_code_block_when_gitlab_compatibility_is_off",
    "gitlab_math::leaves_a_math_fence_as_a_code_block_when_math_is_off",
    "gitlab_math::always_parses_dollar_dollar_as_a_non_gitlab_math_block",
    "gitlab_math::serializes_a_gitlab_styled_math_block_back_to_a_math_fence",
    "gitlab_math::serializes_a_dollar_dollar_math_block_back_to_dollar_dollar",
    "gitlab_math::keys_the_fence_purely_on_meta_math_style_not_on_the_option",
    "gitlab_math::preserves_indentation_when_a_gitlab_math_block_is_nested_in_a_list",
    "gitlab_math::round_trips_a_math_fence_unchanged_with_gitlab_compatibility_on",
    "gitlab_math::round_trips_dollar_dollar_unchanged_regardless_of_the_flag",
    "gitlab_math::a_three_space_indented_math_fence_is_still_promoted",
    "gitlab_math::a_four_space_indented_fence_is_an_indented_code_block_not_math",
    "gitlab_math::a_four_backtick_math_fence_is_promoted",
    "gitlab_math::an_info_string_after_math_is_not_promoted",
    "gitlab_math::the_math_language_tag_is_case_sensitive",
    "gitlab_math::a_tilde_math_fence_is_promoted_by_muya_unlike_muyajs",
    // gitlab_math_toggle — 4
    "gitlab_math_toggle::starts_as_a_code_block_when_gitlab_compatibility_is_off",
    "gitlab_math_toggle::promotes_a_math_fence_to_a_math_block_when_the_option_is_on",
    "gitlab_math_toggle::demotes_a_math_fence_back_to_a_code_block_when_the_option_is_off",
    "gitlab_math_toggle::leaves_dollar_dollar_math_blocks_untouched_across_a_toggle",
    // info_string_model — 3
    "info_string_model::keeps_a_language_plus_attributes_verbatim",
    "info_string_model::keeps_a_pandoc_attribute_block_verbatim",
    "info_string_model::stores_a_plain_language_as_is",
    // list_marker_option — 6
    "list_marker_option::default_bullet_list_uses_a_dash_marker",
    "list_marker_option::a_star_bullet_list_marker_emits_star_markers",
    "list_marker_option::a_plus_bullet_list_marker_emits_plus_markers",
    "list_marker_option::default_ordered_list_uses_the_period_delimiter",
    "list_marker_option::a_paren_order_list_delimiter_emits_paren",
    "list_marker_option::the_paren_delimiter_carries_to_the_ol_bullet_command_label_too",
    // list_serialization — 30
    "list_serialization::keeps_consecutive_empty_task_items_on_separate_lines",
    "list_serialization::keeps_consecutive_empty_bullet_items_on_separate_lines",
    "list_serialization::keeps_adjacent_empty_bullet_items_before_a_following_paragraph",
    "list_serialization::keeps_an_empty_bullet_item_between_populated_sibling_items",
    "list_serialization::uses_an_alternate_nested_marker_instead_of_making_a_tight_list_loose",
    "list_serialization::uses_the_same_safe_marker_under_ordered_and_task_list_parents",
    "list_serialization::handles_a_first_empty_nested_item_followed_by_a_non_empty_item",
    "list_serialization::does_not_rewrite_nested_dash_lists_whose_first_item_is_not_empty",
    "list_serialization::keeps_already_loose_parent_lists_on_their_original_dash_marker",
    "list_serialization::serializes_parser_created_empty_list_items_as_separate_lines",
    "list_serialization::indent_by_1_space_round_trips_the_marktext_fixture",
    "list_serialization::indent_by_2_spaces_round_trips_the_marktext_fixture",
    "list_serialization::indent_by_3_spaces_round_trips_the_marktext_fixture",
    "list_serialization::round_trips_an_ordered_list_nested_inside_a_blockquote",
    "list_serialization::round_trips_a_bullet_list_nested_inside_a_blockquote",
    "list_serialization::round_trips_a_blockquote_nested_inside_a_list_item",
    "list_serialization::round_trips_a_loose_list_with_a_subsequent_paragraph",
    "list_serialization::round_trips_a_loose_list_containing_a_fenced_code_block",
    "list_serialization::does_not_emit_trailing_whitespace_on_blank_lines_inside_a_list_item",
    "list_serialization::round_trips_an_ordered_list_with_two_digit_item_numbers",
    "list_serialization::indent_by_4_spaces_round_trips_the_marktext_fixture",
    "list_serialization::indent_using_daring_fireball_round_trips_the_marktext_fixture",
    "list_serialization::treats_the_unimplemented_tab_option_as_a_1_space_indent",
    "list_serialization::inserts_blank_lines_between_items_when_loose_is_true",
    "list_serialization::keeps_items_adjacent_when_loose_is_false",
    "list_serialization::list_meta_loose_carries_the_prefer_loose_list_item_flag_verbatim",
    "list_serialization::keeps_a_non_1_start_number_through_the_round_trip",
    "list_serialization::parses_the_start_number_into_order_list_meta_start",
    "list_serialization::emits_the_configured_delimiter_for_an_ordered_list",
    "list_serialization::combines_a_non_1_start_with_the_paren_delimiter",
    // markdown_to_state — 32
    "markdown_to_state::keeps_an_empty_unchecked_task_item_after_a_populated_task_item",
    "markdown_to_state::parses_a_single_empty_unchecked_task_item_as_a_task_list_item",
    "markdown_to_state::parses_a_single_empty_checked_task_item_as_a_checked_task_list_item",
    "markdown_to_state::parses_an_empty_task_marker_with_lazy_continuation_text_as_a_task_item",
    "markdown_to_state::keeps_lazy_continuation_text_on_the_final_empty_task_marker",
    "markdown_to_state::does_not_treat_dash_empty_brackets_as_an_empty_task_item",
    "markdown_to_state::does_not_treat_dash_bracket_space_text_as_a_task_item",
    "markdown_to_state::keeps_three_levels_of_task_list_nesting",
    "markdown_to_state::parses_setext_h1_as_setext_heading_level_1",
    "markdown_to_state::parses_setext_h2_as_setext_heading_level_2",
    "markdown_to_state::parses_hash_text_as_atx_heading_not_setext",
    "markdown_to_state::starts_a_new_list_when_the_bullet_marker_changes",
    "markdown_to_state::starts_a_new_list_when_the_ordered_delimiter_changes",
    "markdown_to_state::does_not_parse_dash_foo_without_a_space_as_a_list_item",
    "markdown_to_state::still_parses_dash_space_foo_as_a_list_item",
    "markdown_to_state::splits_a_mixed_task_and_bullet_sequence_into_two_lists",
    "markdown_to_state::converts_block_level_footnote_tokens_into_footnote_states",
    "markdown_to_state::round_trips_a_single_paragraph_footnote_through_state",
    "markdown_to_state::keeps_tight_nested_task_lists_nested",
    "markdown_to_state::retains_surrounding_blank_lines_when_trim_is_false",
    "markdown_to_state::strips_leading_and_trailing_blank_lines_when_trim_is_true",
    "markdown_to_state::keeps_interior_blank_lines_while_trimming_the_surrounding_ones",
    "markdown_to_state::honours_the_trim_option_through_a_state_to_markdown_round_trip",
    "markdown_to_state::parses_text_then_dashes_as_a_single_level_2_setext_heading",
    "markdown_to_state::parses_text_then_equals_as_a_single_level_1_setext_heading",
    "markdown_to_state::parses_a_bare_dashes_line_as_a_single_thematic_break",
    "markdown_to_state::parses_three_dashes_as_a_thematic_break",
    "markdown_to_state::parses_three_stars_as_a_thematic_break",
    "markdown_to_state::parses_three_underscores_as_a_thematic_break",
    "markdown_to_state::does_not_parse_mixed_markers_as_a_thematic_break",
    "markdown_to_state::lowers_a_lone_img_tag_to_a_paragraph",
    "markdown_to_state::keeps_other_html_as_an_html_block_state",
    // math_trailing_space — 3
    "math_trailing_space::parses_a_math_block_whose_closing_fence_has_a_trailing_space",
    "math_trailing_space::parses_a_math_block_whose_closing_fence_has_a_trailing_tab",
    "math_trailing_space::still_parses_a_math_block_with_no_trailing_space",
    // nested_mixed_lists — 4
    "nested_mixed_lists::preserves_a_bullet_list_nested_inside_an_ordered_list_item_round_trip",
    "nested_mixed_lists::preserves_an_ordered_list_nested_inside_a_bullet_list_item_round_trip",
    "nested_mixed_lists::produces_a_bullet_list_state_nested_inside_the_second_order_list_item",
    "nested_mixed_lists::produces_an_order_list_state_nested_inside_a_bullet_list_item",
    // reference_link — 8
    "reference_link::loading_markdown_with_a_definition_keeps_it_as_a_paragraph_state",
    "reference_link::round_trip_output_contains_the_reference_definition_line",
    "reference_link::reference_link_resolves_to_a_token_whose_label_maps_to_href",
    "reference_link::full_collapsed_and_shortcut_forms_all_produce_reference_link_tokens",
    "reference_link::a_definitions_title_propagates_through_label_lookup",
    "reference_link::label_matching_is_case_insensitive",
    "reference_link::a_duplicate_label_keeps_the_first_definition",
    "reference_link::an_orphan_reference_link_stays_plain_text",
    // render_to_static_html — 20
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
    // soft_break_export_html — 3
    "soft_break_export_html::keeps_a_paragraph_soft_break_as_a_newline_never_a_br",
    "soft_break_export_html::keeps_a_soft_break_inside_a_tight_list_item_never_a_br",
    "soft_break_export_html::leaves_a_real_hard_break_as_a_br",
    // state_to_markdown — 11
    "state_to_markdown::does_not_crash_when_a_body_row_has_more_cells_than_the_header",
    "state_to_markdown::does_not_crash_when_a_body_row_has_fewer_cells_than_the_header",
    "state_to_markdown::serialises_a_well_formed_table_normally",
    "state_to_markdown::aligns_a_column_whose_cells_contain_combining_marks",
    "state_to_markdown::widens_a_column_to_fit_east_asian_wide_characters",
    "state_to_markdown::renders_the_delimiter_row_from_per_column_align",
    "state_to_markdown::the_header_row_drives_the_delimiter_not_body_rows",
    "state_to_markdown::round_trips_a_left_center_right_table_to_a_byte_stable_delimiter_row",
    "state_to_markdown::escapes_a_pipe_at_the_very_start_of_a_cell",
    "state_to_markdown::escapes_both_of_two_consecutive_pipes_in_a_cell",
    "state_to_markdown::round_trips_a_cell_starting_with_a_pipe_byte_stably",
    // strong_cjk_flanking — 3 of 6; the other 3 pass today (see the module docs)
    "strong_cjk_flanking::static_path_recognises_strong_in_the_sanity_cases",
    "strong_cjk_flanking::static_path_recognises_strong_in_cjk_context",
    "strong_cjk_flanking::static_path_does_not_bold_or_italicise_the_negative_cases",
    // table_escaped_pipe — 4
    "table_escaped_pipe::an_escaped_pipe_inside_code_is_stored_as_a_bare_pipe",
    "table_escaped_pipe::keeps_the_table_structure_two_columns_three_rows",
    "table_escaped_pipe::round_trips_the_escaped_pipes",
    "table_escaped_pipe::round_trips_an_escaped_pipe_in_plain_cell_text",
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
    assert_eq!(PENDING.len(), 184, "187 transcribed, 3 already passing");
}

/// **The S0 gate's third clause**, machine-checked: the listed cases fail
/// because the engine is unimplemented, not because a type is missing or a
/// transcription is wrong.
///
/// A transcribed case that panics for some other reason — a bad index into a
/// tree that was never built, a `.expect()` on the wrong `Option` — would
/// still be "listed and failing", so the ratchet alone cannot tell the two
/// apart. At S0 it can be checked directly, because there is exactly one
/// legitimate reason to fail.
///
/// **Delete this at S3**, in the commit where `parse` first returns `Ok`.
/// From then on a listed case fails because the port does not agree with muya
/// yet, which is the whole point of the list, and this test would fail for the
/// right reason at the wrong time. `mt_md`'s own
/// `entry_points_report_unimplemented_at_m0` is the paired reminder.
#[test]
fn every_pending_case_fails_because_the_engine_is_unimplemented() {
    let mut wrong_reason = Vec::new();

    for (name, case) in all_cases() {
        if !PENDING.contains(&name) {
            continue;
        }
        let Err(payload) = panic::catch_unwind(AssertUnwindSafe(case)) else {
            // A listed case that passes is row two's business; the per-case
            // test reports it with the right message.
            continue;
        };
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        if !message.contains("Unimplemented") {
            wrong_reason.push(format!("{name}: {message}"));
        }
    }

    assert!(
        wrong_reason.is_empty(),
        "{} listed case(s) failed for a reason other than the engine being \
         unimplemented, which means the transcription is wrong rather than the \
         engine missing:\n{}",
        wrong_reason.len(),
        wrong_reason.join("\n")
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
