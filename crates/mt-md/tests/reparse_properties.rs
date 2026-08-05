//! S6's gate, wide: **the incremental tree equals a full reparse of the whole
//! document, over generated edit sequences** — and so do the source ranges, and
//! so do the ids of the blocks the edit did not touch.
//!
//! # Why the generator generates *edits* and not markdown
//!
//! §6's S4 entry records that *"the generators are S7's and the property over
//! fixed corpora is S4's"*, and §6's S6 row asks for a property *"over
//! generated edit sequences"*. Those only look like a conflict. S4's sentence
//! is about generating **markdown**, which is what a round-trip property would
//! need and what §11.3 schedules for S7. This generates **edits**, over
//! documents that are fixed — the spec fixtures, the round-trip fixtures, and a
//! table of shapes chosen for the block boundaries they contain.
//!
//! That is the same shape `mt-doc`'s S0 gate already used `proptest` for
//! (`tests/edit_inverse.rs`: a generated batch's inverse restores the document,
//! over ≥ 10⁴ batches), so `proptest` becomes a `[dev-dependencies]` entry of
//! `mt-md` here rather than the gate being met over a fixed set and called
//! generated. It adds nothing to the lockfile.
//!
//! # What "equal" means
//!
//! `mt_md::dump_state`'s JSON — names, `meta` and leaf text, which is exactly
//! what `cargo xtask diff` and `cargo xtask blocks` compare — plus the source
//! ranges in tree order. The ranges are the half a tree comparison would miss,
//! and `SourceMap`'s own docs promise them: *"S6's region reparse produces new
//! ones for the region rather than patching these"*.

use std::fmt::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};

use mt_doc::{Document, NodeId};
use mt_md::reparse::{Incremental, ReparsePath};
use mt_md::{Options, SourceMap};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

fn tree_ids(doc: &Document) -> Vec<NodeId> {
    fn walk(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
        for child in doc.children(id) {
            out.push(*child);
            walk(doc, *child, out);
        }
    }
    let mut out = Vec::new();
    walk(doc, doc.root(), &mut out);
    out
}

/// Every leaf's range, in tree order, for the leaves that came from somewhere in
/// the source — as the **content** it names rather than as two numbers.
///
/// Three narrowings, all measured, and all of them the same phenomenon that S4
/// named as "the trap nobody named": **`pulldown-cmark`'s ranges are not
/// consistent about the newline and the whitespace that end a block.** A region
/// has an end and a document does not, so the two parses see the same block
/// with a different amount of trailing nothing after it.
///
/// - **Containers are excluded.** The blank line before the next block is
///   inside a list mid-document and not at end of input, so a list's range can
///   end two bytes earlier. Its children's do not.
/// - **Zero-width leaves are excluded** — the synthetic empty paragraph an empty
///   block quote or list item gets (mechanism 2, muya #1735), whose range is
///   `container.end..container.end` and therefore inherits the same bytes.
/// - **The end is trimmed.** A leaf's range can end one byte later after a
///   leaf-text edit, on the trailing newline, because the fast path moves the
///   range by the edit's delta and `pulldown-cmark` re-decides whether that
///   newline is inside the block.
///
/// What is compared is therefore the leaf's start and the content its range
/// names with trailing whitespace off — which is the whole of what a caret
/// mapping or §10's per-leaf reverse map can read out of it, since the text is
/// already asserted identical by the tree comparison.
fn leaf_ranges<'a>(doc: &Document, map: &SourceMap, src: &'a str) -> Vec<(usize, &'a str)> {
    tree_ids(doc)
        .into_iter()
        .filter(|id| doc.block(*id).is_some_and(|b| b.is_leaf()))
        .filter_map(|id| map.get(id))
        .filter(|range| range.start < range.end)
        .map(|range| (range.start, src[range].trim_end()))
        .collect()
}

/// A child's range is inside its parent's and siblings are in document order —
/// `tests/source_ranges.rs`'s claim, asked of an incrementally-maintained map.
///
/// **Compared against the full parse rather than asserted outright**, and the
/// reason is a finding rather than a convenience: `parse` itself does not
/// guarantee it on every string. `-\t- [a]: /x[xter\n` — a tab inside a list
/// item's marker padding — gives the innermost paragraph the range `1..13`
/// inside an item at `2..17`, because `strip_lines`' "take the leading
/// whitespace back" clip (S2's, and 30 of its 47 first-run disagreements) is
/// measured from the line rather than from the item's content column. The
/// corpus contains no such document, which is why `source_ranges.rs` can assert
/// nesting outright and this cannot. M2.md §10 carries it.
fn ranges_nest(doc: &Document, map: &SourceMap, id: NodeId, parent: Option<&Range<usize>>) -> bool {
    let mut previous: Option<Range<usize>> = None;
    for child in doc.children(id) {
        let Some(range) = map.get(*child) else {
            return false;
        };
        if range.start > range.end {
            return false;
        }
        if parent.is_some_and(|p| range.start < p.start || range.end > p.end) {
            return false;
        }
        if previous.as_ref().is_some_and(|p| range.start < p.start) {
            return false;
        }
        previous = Some(range.clone());
        if !ranges_nest(doc, map, *child, Some(&range)) {
            return false;
        }
    }
    true
}

/// The whole of the claim, for one document after one edit.
fn agrees_with_a_full_parse(incremental: &Incremental, options: Options, what: &str) {
    let full = mt_md::parse(incremental.source(), options);

    let left = mt_md::state::to_state_json(incremental.document());
    let right = mt_md::state::to_state_json(&full.document);
    assert_eq!(left, right, "{what}: the trees disagree");

    assert_eq!(
        leaf_ranges(
            incremental.document(),
            incremental.source_map(),
            incremental.source()
        ),
        leaf_ranges(&full.document, &full.source_map, incremental.source()),
        "{what}: the leaf source ranges disagree"
    );
    assert_eq!(
        ranges_nest(
            incremental.document(),
            incremental.source_map(),
            incremental.document().root(),
            None
        ),
        ranges_nest(&full.document, &full.source_map, full.document.root(), None),
        "{what}: the incremental ranges nest where the full parse's do not, or the reverse"
    );
    assert_eq!(
        incremental.source_map().len(),
        tree_ids(incremental.document()).len(),
        "{what}: the map holds an entry that is not a live node, or misses one"
    );
    assert_eq!(
        incremental.labels().len(),
        full.labels.len(),
        "{what}: the label maps disagree"
    );
}

// ---------------------------------------------------------------------------
// The seed documents
// ---------------------------------------------------------------------------

/// Shapes chosen for the block boundaries in them rather than for coverage —
/// every one of §4.1's triggers, plus the containers whose reparse is not a
/// slice of the source.
const SHAPES: &[&str] = &[
    "alpha\n\nbravo\n\ncharlie\n",
    "# head\n\ntext\n\n## sub\n\nmore\n",
    "- a\n- b\n- c\n",
    "- a\n\n- b\n\n- c\n",
    "1. a\n2. b\n   1. c\n",
    "> quoted\n> lines\n\nafter\n",
    "> - a\n> - b\n\nafter\n",
    "```js\ncode\n```\n\nafter\n",
    "    indented\n    code\n\nafter\n",
    "| a | b |\n| - | - |\n| 1 | 2 |\n\nafter\n",
    "text\n---\nmore\n",
    "***\n\ntext\n\n___\n",
    "<div>\nhtml\n</div>\n\nafter\n",
    "- [ ] todo\n- [x] done\n\nafter\n",
    "para\nwith\nlines\n\n- list\n- items\n",
    "a\n\n    code\n\nb\n\n    code\n",
    "$$\nx^2\n$$\n\nafter\n",
    "- a\n  * \n  * \n\nafter\n",
    "3. foo\n   20. foo\n       141. foo\n",
    "---\ntitle: x\n---\n\nbody\n\nmore\n",
    "",
    "\n",
    "x\n",
];

/// The round-trip fixtures — real documents, which is what the shapes above are
/// not. Two-and-a-bit kilobytes each, so an edit sequence over them is still
/// fast.
fn fixtures() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "md")
                && let Ok(source) = std::fs::read_to_string(&path)
            {
                out.push((path.display().to_string(), mt_md::normalize_source(&source)));
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mt-md is two levels below the repo root")
        .to_path_buf();
    let mut out = Vec::new();
    walk(
        &PathBuf::from(&root)
            .join("spec")
            .join("fixtures")
            .join("marktext-round-trip"),
        &mut out,
    );
    out.sort();
    assert!(out.len() >= 11, "the round-trip fixtures have moved");
    out
}

// ---------------------------------------------------------------------------
// The generated edits
// ---------------------------------------------------------------------------

/// The alphabet an edit inserts from: §4.1's own trigger list, the characters
/// that decide a block's kind, and ordinary text so that the leaf-text path is
/// exercised too.
const INSERTS: &[&str] = &[
    "", "x", "xy", " ", "  ", "\n", "\n\n", "#", "# ", "## ", ">", "> ", "- ", "* ", "+ ", "1. ",
    "20. ", "```", "~~~", "---", "===", "|", "[", "]", ":", "$$", "<div>", "</div>", "\t", "*",
    "_", ".", ")", "[a]: /x", "é", "日本",
];

#[derive(Debug, Clone)]
struct Edit {
    /// A fraction of the source's length, resolved to a character boundary.
    at: usize,
    remove: usize,
    insert: String,
}

fn an_edit() -> impl Strategy<Value = Edit> {
    (0usize..4096, 0usize..12, prop::sample::select(INSERTS)).prop_map(|(at, remove, insert)| {
        Edit {
            at,
            remove,
            insert: insert.to_string(),
        }
    })
}

/// Snap a generated `(at, remove)` onto character boundaries of `source`.
fn resolve(source: &str, edit: &Edit) -> (usize, usize) {
    if source.is_empty() {
        return (0, 0);
    }
    let mut at = edit.at % (source.len() + 1);
    while at < source.len() && !source.is_char_boundary(at) {
        at += 1;
    }
    let mut end = (at + edit.remove).min(source.len());
    while end < source.len() && !source.is_char_boundary(end) {
        end += 1;
    }
    (at, end - at)
}

/// Run something that may panic, without printing a backtrace for it.
///
/// The hook is swapped because a *deliberate* probe over 200,000 cases would
/// otherwise bury the run in output that says nothing.
fn quietly<T>(f: impl FnOnce() -> T) -> std::thread::Result<T> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(previous);
    out
}

fn run_sequence(source: &str, options: Options, edits: &[Edit], what: &str) {
    let mut doc = Incremental::new(source, options);
    let mut history = String::new();
    for (step, edit) in edits.iter().enumerate() {
        let before_ids = tree_ids(doc.document());
        let (at, remove) = resolve(doc.source(), edit);
        let _ = write!(
            history,
            "\n  [{step}] {at}..{} → {:?}",
            at + remove,
            edit.insert
        );

        // **The incremental path may panic only where a full parse does.**
        // `pulldown-cmark` 0.13.4 panics on `"> - [a]: /x\n\t"` — see
        // `block::tests::pulldown_cmark_panics_on_a_definition_in_a_quoted_list_item_before_a_tab_line`,
        // which S6's generated edits are what found — so a sequence can reach
        // an input on which "equals a full reparse" has nothing to compare.
        // Skipping it silently would be a hole; asserting that the *full* parse
        // panics too makes the skip a claim.
        let mut edited = doc.source().to_string();
        edited.replace_range(at..at + remove, &edit.insert);
        let Ok(report) = quietly(|| doc.edit(at, remove, &edit.insert)) else {
            assert!(
                quietly(|| mt_md::parse(&edited, options)).is_err(),
                "{what}{history}: the incremental path panicked where a full parse does not"
            );
            return;
        };
        agrees_with_a_full_parse(&doc, options, &format!("{what}{history}"));

        if let ReparsePath::LeafText { node } = report.path {
            assert_eq!(
                report.blocks_reparsed, 1,
                "{what}{history}: the leaf-text path reparsed more than one block"
            );
            let after_ids = tree_ids(doc.document());
            assert_eq!(
                before_ids, after_ids,
                "{what}{history}: a leaf-text edit renumbered the tree"
            );
            assert!(
                doc.document().get(node).is_some(),
                "{what}{history}: the edited leaf did not survive its own edit"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The properties
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 192, ..ProptestConfig::default() })]

    /// **The gate's second clause**, over the hand-chosen shapes: an edit
    /// sequence leaves a tree, a range map and a label map identical to a full
    /// parse of the same source.
    #[test]
    fn an_edit_sequence_over_a_shape_agrees_with_a_full_parse(
        shape in prop::sample::select(SHAPES),
        edits in prop::collection::vec(an_edit(), 1..6),
    ) {
        run_sequence(shape, Options::SPEC, &edits, &format!("{shape:?} at SPEC"));
    }

    /// The same at muya's own defaults, where front matter and block math are
    /// on — the two options the region choice has a guard for.
    #[test]
    fn an_edit_sequence_at_muya_defaults_agrees_with_a_full_parse(
        shape in prop::sample::select(SHAPES),
        edits in prop::collection::vec(an_edit(), 1..6),
    ) {
        run_sequence(shape, Options::MUYA_DEFAULT, &edits, &format!("{shape:?} at MUYA_DEFAULT"));
    }

    /// And with footnotes on, which is the option no harness in the repository
    /// drives and the one whose segmentation forces the whole document.
    #[test]
    fn an_edit_sequence_with_footnotes_on_agrees_with_a_full_parse(
        shape in prop::sample::select(SHAPES),
        edits in prop::collection::vec(an_edit(), 1..4),
    ) {
        let options = Options { footnote: true, ..Options::MUYA_DEFAULT };
        run_sequence(shape, options, &edits, &format!("{shape:?} with footnotes"));
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    /// The same claim over real documents rather than over shapes. Fewer cases
    /// because each one parses a whole fixture per edit **twice** — once
    /// incrementally and once to check it.
    #[test]
    fn an_edit_sequence_over_a_round_trip_fixture_agrees_with_a_full_parse(
        which in 0usize..64,
        edits in prop::collection::vec(an_edit(), 1..4),
    ) {
        let files = fixtures();
        let (name, source) = &files[which % files.len()];
        run_sequence(source, Options::MUYA_DEFAULT, &edits, name);
    }
}

// ---------------------------------------------------------------------------
// The deterministic sweep — breadth the generator does not promise
// ---------------------------------------------------------------------------

/// Every §4.1 trigger inserted at **every line start** of every shape, and the
/// same deleted backwards from every line start.
///
/// A `proptest` run samples; this covers. It is the shape of check that caught
/// the region's `hard_boundary` disagreeing with the reparse's own output,
/// which no random position happened to hit in the first hundred cases.
#[test]
fn every_trigger_at_every_line_start_agrees_with_a_full_parse() {
    let triggers = [
        "# ",
        "> ",
        "- ",
        "1. ",
        "```\n",
        "---\n",
        "|",
        "\n",
        "    ",
        "[a]: /x\n",
    ];
    let mut checked = 0usize;
    for shape in SHAPES {
        for options in [Options::SPEC, Options::MUYA_DEFAULT] {
            let starts: Vec<usize> = std::iter::once(0)
                .chain(
                    shape
                        .char_indices()
                        .filter_map(|(i, c)| (c == '\n' && i < shape.len()).then_some(i + 1)),
                )
                .collect();
            for at in starts {
                for trigger in triggers {
                    let mut doc = Incremental::new(shape, options);
                    doc.edit(at, 0, trigger);
                    agrees_with_a_full_parse(
                        &doc,
                        options,
                        &format!("{shape:?}: insert {trigger:?} at {at}"),
                    );
                    checked += 1;

                    // And the deletion that undoes a line, which is the other
                    // direction a boundary moves in.
                    let mut doc = Incremental::new(shape, options);
                    let end = shape[at..].find('\n').map_or(shape.len(), |i| at + i + 1);
                    doc.edit(at, end - at, "");
                    agrees_with_a_full_parse(
                        &doc,
                        options,
                        &format!("{shape:?}: delete the line at {at}"),
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 1_000, "only {checked} edits were checked");
}

/// **The gate's first clause, over the corpus rather than over one input.**
///
/// Typing an ordinary character into every paragraph of every round-trip
/// fixture reparses exactly one block, and renumbers nothing. This is the
/// counter the row asks for, read at the scale where a timer would be the
/// tempting alternative.
#[test]
fn typing_into_every_paragraph_of_every_fixture_reparses_one_block() {
    let mut typed = 0usize;
    let mut leaf_path = 0usize;
    for (name, source) in fixtures() {
        let seed = mt_md::parse(&source, Options::MUYA_DEFAULT);
        let paragraphs: Vec<Range<usize>> = tree_ids(&seed.document)
            .into_iter()
            .filter(|id| {
                seed.document
                    .block(*id)
                    .is_some_and(|b| b.name() == "paragraph")
            })
            .filter_map(|id| seed.source_map.get(id))
            .filter(|range| range.end > range.start + 1)
            .collect();

        for range in paragraphs {
            let mut at = range.end - 1;
            while at > range.start && !source.is_char_boundary(at) {
                at -= 1;
            }
            let mut doc = Incremental::new(&source, Options::MUYA_DEFAULT);
            let before = tree_ids(doc.document());

            let report = doc.edit(at, 0, "z");
            typed += 1;
            if let ReparsePath::LeafText { .. } = report.path {
                leaf_path += 1;
                assert_eq!(report.blocks_reparsed, 1, "{name} at {at}");
                assert_eq!(tree_ids(doc.document()), before, "{name} at {at}");
            }
            agrees_with_a_full_parse(&doc, Options::MUYA_DEFAULT, &format!("{name} at {at}"));
        }
    }

    assert!(typed > 100, "only {typed} paragraphs were typed into");
    // Not every paragraph is reachable by the fast path — a definition-shaped
    // line, a heading, a table cell and the last byte of a block all decline
    // it. The number is asserted as a floor so that a change which quietly
    // switches the fast path off shows up here rather than in a profile.
    assert!(
        leaf_path * 2 >= typed,
        "the leaf-text path took {leaf_path} of {typed} — it used to take more than half"
    );
}
