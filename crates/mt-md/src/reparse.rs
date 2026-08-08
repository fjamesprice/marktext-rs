//! Incremental reparse — RUST-REWRITE-PLAN.md §4.1, M2.md §5 D7, M2 S6.
//!
//! §4.1 reads as an optimisation and D7 settled at S0 that it is a **shape**
//! decision. Its three concrete outputs landed then — [`mt_doc::Dirt`]'s two
//! kinds, [`mt_doc::Edit`]'s six variants, and "`mt_md` exposes a reparse entry
//! point that takes a region rather than only a document". This module is the
//! stage that writes the region reparse and the subtree diff on top of them.
//!
//! ```text
//! 1. An edit to a leaf's text usually changes nothing structurally: re-derive
//!    **only the edited block** and mark it layout-dirty.
//! 2. If the edit introduces a block-boundary trigger, re-run block parsing
//!    over the minimal enclosing region and diff the resulting subtree against
//!    the existing one.
//! 3. Full document reparse only on load and on external file change.
//! ```
//!
//! # Step 3 is not "throw the document away"
//!
//! The widest region this module ever chooses is `0..src.len()`, and it goes
//! through the **same diff**. That matters for the gate's third clause: a
//! `Document` rebuilt by [`crate::parse`] has entirely fresh [`NodeId`]s, so a
//! fallback that re-parsed from scratch would lose every id on the very inputs
//! the guards below are conservative about. Here the fallback is a region
//! reparse whose region happens to be the document, and an untouched block
//! keeps its id either way.
//!
//! # The matching key, and why "a wrong match" is not a risk category
//!
//! §8's risk row is that a live `NodeId` matched to the wrong block hands
//! `mt-layout` a cached layout for something else — silently, and in a crate
//! that does not exist yet to notice. Generational ids do **not** protect
//! against that; they turn a *stale* handle into `None` and have nothing to say
//! about a live one.
//!
//! So the key is **full structural equality of the subtree** — name, `meta`,
//! text, and children recursively — matched by a longest common subsequence
//! over each sibling list. Two subtrees that compare equal *are* the same block
//! to every consumer, so reusing the id is correct by construction rather than
//! by heuristic. Between those anchors the leftovers are paired positionally
//! and reconciled only when they are the **same kind**, and reconciling emits
//! the edits that make the node hold exactly the new block. There is no path on
//! which a live id ends up addressing content it was not given.
//!
//! What the strong key costs is therefore not correctness but reuse: a
//! mis-paired leftover keeps a node alive that a human would have called a
//! different block, and `mt-layout` re-lays it out because its content changed.
//! That is the whole downside, and it is why the key is equality rather than
//! position or `(name, meta)`.
//!
//! # Where the entry point lives
//!
//! Here, in `mt-md`, which is what D7 said. The diff has to emit [`Edit`]s
//! against a live [`Document`] and D9 forbids any write path that bypasses
//! [`Document::apply`] — but `mt-doc` cannot parse, and `block::parse_blocks`
//! already goes through `apply` for exactly this reason (see `block::insert`).
//! `mt-md` is the only crate that can both run the block parser and see the
//! tree.
//!
//! # What the region has to satisfy, and the three guards that are not free
//!
//! A region is sound when no block of a **full** parse crosses either of its
//! ends. That is not decidable from a span, so it is established from the tree
//! and then re-checked against the reparse's own output:
//!
//! - **Chosen** from the top-level siblings the edit touches, widened outwards
//!   while the boundary between two siblings is not *hard* — a hard boundary is
//!   a blank line between blocks that cannot merge across one.
//! - **Re-checked** afterwards: if the reparse's first or last block would now
//!   merge with the neighbour outside the region, or if the last block ran to
//!   the region's end and swallowed the gap (an unterminated fence, an
//!   unterminated `<pre>`), the region widens and the parse runs again. The
//!   loop terminates at the whole document.
//!
//! Three things S5's experience said to check rather than assume, all three of
//! which are real:
//!
//! 1. **`seen_labels` is document-global** (`marked`'s `tokens.links`): a
//!    definition whose label was already defined *anywhere earlier* emits no
//!    block at all. A region that does not thread it can resurrect a definition
//!    the document already consumed, and one that defines a label can suppress
//!    a later duplicate outside itself. So a definition-shaped line at or before
//!    the region's end widens the region to the whole document. The precise
//!    version — per-block label contributions carried on the side — is owed
//!    forward rather than guessed at here.
//! 2. **`build` splits on footnote definitions before regions are chosen**, and
//!    a definition's extent runs forward through blank lines, so "minimal
//!    enclosing region" and "footnote segment boundary" can disagree. With
//!    [`Options::footnote`] on, the region is the whole document.
//! 3. **Front matter is only front matter at offset 0**, and its trailing rule
//!    is `\n{2,}` *or* one-or-two newlines at end of input — so asking about a
//!    truncated string lets the truncation create it. [`block::build_span`]
//!    asks about the whole `src`; the region is widened past the front matter's
//!    end so that the answer fits inside it.
//!
//! # The counter
//!
//! [`Reparse::blocks_reparsed`] is public, on the return value, and is not
//! behind `#[cfg(test)]` — the gate asks for a counter rather than a timer, and
//! a counter a test can only read through a test-only door is a counter the
//! shipped code can stop maintaining.

use std::collections::HashSet;
use std::ops::Range;

use mt_doc::{Block, Document, Edit, NodeId};
use mt_inline::Labels;

use crate::block::{self, Built};
use crate::{Options, Parsed, SourceMap};

/// A parse that survives edits.
///
/// Holds the four things one parse knows — the source, the tree, the label map
/// and the source ranges — and keeps all four true across
/// [`Incremental::edit`]. [`crate::parse`] is what it is built from and
/// [`Incremental::into_parsed`] is how it is handed back.
#[derive(Debug)]
pub struct Incremental {
    source: String,
    options: Options,
    document: Document,
    labels: Labels,
    source_map: SourceMap,
}

/// What one [`Incremental::edit`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Reparse {
    /// **How many blocks the parser rebuilt.** One for a leaf-text edit; the
    /// region's own block count otherwise. This is the gate's counter.
    pub blocks_reparsed: usize,
    /// How many blocks the document holds afterwards, so that a caller can read
    /// the counter as a fraction without walking the tree.
    pub blocks_total: usize,
    /// Which of §4.1's two paths ran.
    pub path: ReparsePath,
    /// The span of the **new** source that was reparsed. Empty on the leaf-text
    /// path, which parses nothing.
    pub region: Range<usize>,
    /// The inverse batch, in the order that undoes this edit. Push it onto an
    /// undo stack, or drop it and call [`Document::prune_detached`].
    pub inverse: Vec<Edit>,
}

/// Which of §4.1's two paths an edit took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReparsePath {
    /// §4.1 step 1 — the edit changed one leaf's text and nothing else. The
    /// node is the leaf, and it keeps its id.
    LeafText { node: NodeId },
    /// §4.1 step 2 — block parsing re-ran over [`Reparse::region`].
    Region,
}

impl Incremental {
    /// Parse `source` and keep everything an edit will need.
    #[must_use]
    pub fn new(source: &str, options: Options) -> Self {
        let parsed = crate::parse(source, options);
        Incremental {
            source: source.to_string(),
            options,
            document: parsed.document,
            labels: parsed.labels,
            source_map: parsed.source_map,
        }
    }

    /// The markdown this document is a parse of.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The document, mutably — for `mt-layout`'s
    /// [`Document::clear_dirty`](mt_doc::Document::clear_dirty).
    ///
    /// Editing *through* this handle desynchronises the source from the tree,
    /// which is the one thing this type exists to prevent; the reason it is
    /// still offered is that clearing the dirty set is a `&mut` operation that
    /// has nothing to do with the text.
    ///
    /// It is also the one way to put a node past [`crate::MAX_NESTING_DEPTH`]
    /// into a document this type holds — every tree it builds itself comes from
    /// `block::build_span`, which clamps — and the recursive walks below
    /// (`graft`, `subtree_eq`, `record_ranges`, `shift_subtree`) are bounded by
    /// that clamp rather than by a guard of their own. Both consequences have
    /// the same cause and the same answer: do not edit through this.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    #[must_use]
    pub fn labels(&self) -> &Labels {
        &self.labels
    }

    #[must_use]
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    /// Give up the incremental state and keep the parse.
    #[must_use]
    pub fn into_parsed(self) -> Parsed {
        Parsed {
            document: self.document,
            labels: self.labels,
            source_map: self.source_map,
        }
    }

    /// Splice `insert` over `at..at + remove` in the source and bring the tree,
    /// the label map and the source ranges back into agreement with it.
    ///
    /// # Panics
    ///
    /// If `at..at + remove` is not a character-boundary range of
    /// [`Incremental::source`].
    pub fn edit(&mut self, at: usize, remove: usize, insert: &str) -> Reparse {
        let end = at.checked_add(remove).expect("edit range overflows");
        assert!(
            end <= self.source.len()
                && self.source.is_char_boundary(at)
                && self.source.is_char_boundary(end),
            "{at}..{end} is not a character-boundary range of the source"
        );

        if let Some(report) = self.leaf_text_edit(at, remove, insert) {
            return report;
        }
        self.region_edit(at, remove, insert)
    }
}

// ---------------------------------------------------------------------------
// §4.1 step 1 — the leaf-text path
// ---------------------------------------------------------------------------

/// The characters that can turn one line into a different block, given that the
/// edit is mid-line and inserts no newline.
///
/// Everything else a block start is made of — `#`, `>`, a bullet, a fence, an
/// underline — has to appear at the line's **first non-space** position, and
/// [`Incremental::leaf_text_edit`]'s fourth guard is that that position is an
/// ordinary character and lies before the edit. What is left is the two things
/// that act from the middle of a line: a `|` changes a header row's cell count
/// and so decides whether the *next* line is a table delimiter row, and a `$`
/// can complete the `$$` a block-math paragraph opens with.
const MID_LINE_TRIGGERS: &[char] = &['\n', '\r', '\t', '|', '$'];

/// Characters that can start a block, and therefore may not be the first
/// non-space character of the edited line.
const BLOCK_STARTERS: &[char] = &[
    '#', '>', '*', '+', '-', '_', '=', '~', '`', '<', '[', '|', ':', '$', '\\',
];

impl Incremental {
    fn leaf_text_edit(&mut self, at: usize, remove: usize, insert: &str) -> Option<Reparse> {
        if insert.contains(MID_LINE_TRIGGERS)
            || self.source[at..at + remove].contains(MID_LINE_TRIGGERS)
        {
            return None;
        }
        // **Strictly inside a line's content**, which is two questions and not
        // one.
        //
        // Not at a line start, because a line start is where every block-start
        // pattern is anchored and where four spaces become indented code. Asked
        // through the grid rather than of one byte, because a lone `\r` starts a
        // line too — see the note above `block::line_start_of`.
        //
        // And not **inside a line terminator**: an edit between a `\r` and its
        // `\n` splits one line ending into two, a lone `\r` and a `\n`, and the
        // line grid moves under the whole document from a single space. Not one
        // character of that edit is in `MID_LINE_TRIGGERS` — the `\r` it changes
        // the meaning of is not in the edit at all, it is beside it. Found by
        // `reparse_properties::the_generator_can_reach_the_lone_carriage_return_class`
        // on its first run, which is the run S6's gate could not make.
        // `at - 1` is safe under the first disjunct, and it is the whole test:
        // a `\r` **not** followed by `\n` already makes `at` a line start.
        if at == 0
            || block::line_start_of(&self.source, at) == at
            || self.source.as_bytes()[at - 1] == b'\r'
        {
            return None;
        }
        // **The guard `span_for` already has, and this path did not.** Every
        // other check here asks whether the edited *line* becomes a different
        // block; none of them asks whether the enclosing block still ends where
        // it did. A link reference definition whose destination sits on the
        // following line makes the paragraph's extent depend on that line's
        // content — `"[a]:\naa\na"` is one paragraph, and inserting a space into
        // `aa` invalidates the destination, so the construct stops terminating
        // there and the block runs on and swallows the line below. Not one
        // character of that edit is a `MID_LINE_TRIGGERS` character and the
        // `[a]:` is on a *different* line, so nothing above can see it.
        //
        // The question is asked of the old source up to the end of the edited
        // line: `defines_a_label` is `marked`'s `def` rule head loosened to a
        // scan, and a definition *after* the edited line cannot have its extent
        // changed by an edit above it. The new source needs no separate ask —
        // an edit that *creates* a definition has to put a `[` first on its
        // line, and `BLOCK_STARTERS` already declines that. Declining sends the
        // edit to the region path, where the `defines_a_label` check at
        // [`Incremental::span_for`] widens it to the whole document.
        if defines_a_label(&self.source[..block::line_end_of(&self.source, at)]) {
            return None;
        }

        // The deepest node whose range contains the whole edit, with its
        // ancestors' blocks and ranges as we descend — which is what the prefix
        // chain is rebuilt from.
        let mut ancestors: Vec<(&Block, Range<usize>)> = Vec::new();
        let mut node = self.document.root();
        'descend: loop {
            for child in self.document.children(node) {
                let Some(range) = self.source_map.get(*child) else {
                    continue;
                };
                // A coarse range locates a block; it does not delimit one, and
                // this path is about to re-derive text from it. See
                // [`SourceMap::is_exact`].
                if !self.source_map.is_exact(*child) {
                    return None;
                }
                if range.start <= at && at + remove <= range.end {
                    if let Some(block) = self.document.block(*child) {
                        ancestors.push((block, range));
                    }
                    node = *child;
                    continue 'descend;
                }
            }
            break;
        }
        let (block, range) = ancestors.pop()?;
        if !matches!(block, Block::Paragraph { .. }) {
            return None;
        }
        // **Strictly inside**, both ends. The path's whole assumption is that
        // the block's range moves by the edit's delta and by nothing else, and
        // an edit that touches either end breaks it in a way no guard about
        // characters can see: `pulldown-cmark`'s paragraph range ends at the
        // last content byte at top level and at the last *non-space* byte
        // inside a list item — measured, and the reason `a  \nb  ` keeps its
        // trailing spaces where `+ ody- ` does not. Appending at a paragraph's
        // very end therefore takes the region path, which is one block for a
        // top-level paragraph and the enclosing block otherwise.
        if at <= range.start || at + remove >= range.end {
            return None;
        }
        let old_text = block.text()?.to_str().into_owned();
        // **A pipe anywhere in the paragraph, not merely in the edit.** A GFM
        // table needs a header row and a delimiter row with the *same cell
        // count*, so `. b |\n| - | - |` is a paragraph and `. b |<div>\n| - | -
        // |` is a table — inserting five characters that contain no `|` at all
        // changed how many cells the line before the delimiter row has. Only
        // the whole paragraph can answer that, so it is the whole paragraph
        // that is asked.
        if old_text.contains('|') {
            return None;
        }
        let prefixes = block::prefix_chain(&self.source, &ancestors);

        // **Check the derivation against an answer it already has before
        // trusting it with a new one.** Re-deriving this leaf from the range it
        // was built with must reproduce the text it was built with; if it does
        // not, the range is approximate and the whole path is unsound.
        //
        // Two places produce approximate ranges and both are real: every node
        // under a `footnote` carries the *definition's* range (S5, §10), and
        // every node the `state.top = false` re-lex produces carries a range
        // mapped line-by-line out of a de-indented copy (S6,
        // `block::retarget_through`). A `SourceMap` that is not injective is
        // the trap the brief named, and this is the one place that would have
        // walked into it: a paragraph whose range starts at the list marker
        // rather than after it, spliced as though `20.` were its own text.
        if block::paragraph_text(&self.source, &range, &prefixes) != old_text {
            return None;
        }

        // The edited line, with its container prefix off: its first non-space
        // character decides what block the line is, and that character has to
        // be an ordinary one *before* the edit.
        let lines = block::stripped_lines(&self.source, &range, &prefixes);
        let (text, line_at) = lines
            .iter()
            .rev()
            .find(|(_, source_start)| *source_start <= at)?;
        // **The line's text has to be a slice of the source at that offset.**
        // `StrippedLine::source_start` is exact except where a tab inside a list
        // item was expanded to spaces (`expand_tabs`, the one case S2 records
        // where a line's text is not a slice), and there it names the line
        // rather than the content — so an offset comparison against it is off by
        // the item's indent and the next two guards silently pass. Asking the
        // source directly costs one string compare and cannot be off.
        if self.source.get(*line_at..line_at + text.len()) != Some(text.as_str()) {
            return None;
        }
        // Inside that line's **content**, not merely at or after its start: an
        // offset between the line's start and its stripped content start is
        // inside the container's own prefix, where `> ` becomes `># ` and the
        // block below changes without a single trigger character being typed.
        if at + remove > line_at + text.len() {
            return None;
        }
        // **Non-whitespace**, not merely non-space. A line that is nothing but
        // spaces and tabs is a *blank* line, which ends a paragraph — so the
        // edit has to start strictly after the line's first real character, and
        // that character then survives whatever the edit does. `\te` → `\t ` is
        // the shape: one keystroke, no trigger character, and the paragraph
        // stops one line earlier.
        let first = text.find(|c: char| !c.is_whitespace())?;
        if text[first..].starts_with(BLOCK_STARTERS)
            || text[first..].starts_with(|c: char| c.is_ascii_digit())
            || line_at + first >= at
        {
            return None;
        }

        // Re-derive that one block's text from the edited source. The node's
        // own range grows by the edit; every ancestor contains the edit, so
        // theirs grow too and their starts do not move — which is why the chain
        // above could be built from the ranges as they were.
        let mut source = self.source.clone();
        source.replace_range(at..at + remove, insert);
        if self.front_matter_moved(&source, at, remove, insert.len()) {
            return None;
        }
        let widened = range.start..shift(range.end, at, remove, insert.len());
        let new_text = block::paragraph_text(&source, &widened, &prefixes);

        let mut inverse = Vec::new();
        if let Some((offset, removed, inserted)) = minimal_splice(&old_text, &new_text) {
            inverse = self.document.apply(&[Edit::SpliceText {
                node,
                at: offset,
                remove: removed,
                insert: inserted,
            }]);
        }

        self.source = source;
        shift_map(&mut self.source_map, at, remove, insert.len());
        self.labels = crate::labels::collect(&self.document);
        Some(Reparse {
            blocks_reparsed: 1,
            blocks_total: count_blocks(&self.document, self.document.root()),
            path: ReparsePath::LeafText { node },
            region: at..at,
            inverse,
        })
    }
}

impl Incremental {
    /// Whether the edit moved the front matter's extent — which no edit in the
    /// body should be able to do, and which one at the end of the document can.
    ///
    /// muya's front-matter regex closes on **the earliest `---` that satisfies
    /// its trailing rule**, and that delimiter is *not anchored to a line
    /// start*: `---\na\n\nb\n\nmore---\n` is one front-matter block whose text
    /// is everything up to the last three characters. So typing `---` at the
    /// end of the last paragraph of a document that merely *begins* with `---`
    /// turns the whole document into front matter, from a keystroke that is
    /// otherwise indistinguishable from typing a word.
    ///
    /// Asking the regex both ways is a linear scan of a string that is already
    /// in hand, and it is the only test that catches this without also
    /// declining every edit in every document that has front matter.
    fn front_matter_moved(&self, source: &str, at: usize, remove: usize, inserted: usize) -> bool {
        if !self.options.front_matter {
            return false;
        }
        let before = block::front_matter_end(&self.source, self.options);
        let after = block::front_matter_end(source, self.options);
        after != shift(before, at, remove, inserted)
    }
}

/// Where `offset` lands after `at..at + remove` becomes `inserted` bytes.
fn shift(offset: usize, at: usize, remove: usize, inserted: usize) -> usize {
    if offset >= at + remove {
        offset + inserted - remove
    } else if offset > at {
        at + inserted
    } else {
        offset
    }
}

fn shift_map(map: &mut SourceMap, at: usize, remove: usize, inserted: usize) {
    let moved: Vec<(NodeId, Range<usize>)> = map
        .iter()
        .map(|(id, range)| {
            let start = shift(range.start, at, remove, inserted);
            (id, start..shift(range.end, at, remove, inserted).max(start))
        })
        .collect();
    for (id, range) in moved {
        map.insert(id, range);
    }
}

/// The one `SpliceText` that turns `old` into `new`, or `None` if they agree.
///
/// Common prefix and suffix are trimmed so that a keystroke is a one-byte
/// splice rather than a whole-text replacement — which is what makes
/// `Edit::SpliceText`'s inverse cheap and what an undo stack coalesces.
fn minimal_splice(old: &str, new: &str) -> Option<(usize, usize, String)> {
    if old == new {
        return None;
    }
    let (o, n) = (old.as_bytes(), new.as_bytes());
    let limit = o.len().min(n.len());
    let mut head = 0;
    while head < limit && o[head] == n[head] {
        head += 1;
    }
    while head > 0 && !old.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = 0;
    while tail < limit - head && o[o.len() - 1 - tail] == n[n.len() - 1 - tail] {
        tail += 1;
    }
    while tail > 0
        && !(old.is_char_boundary(old.len() - tail) && new.is_char_boundary(n.len() - tail))
    {
        tail -= 1;
    }
    Some((
        head,
        old.len() - head - tail,
        new[head..new.len() - tail].to_string(),
    ))
}

// ---------------------------------------------------------------------------
// §4.1 step 2 — the region path
// ---------------------------------------------------------------------------

impl Incremental {
    fn region_edit(&mut self, at: usize, remove: usize, insert: &str) -> Reparse {
        let mut source = self.source.clone();
        source.replace_range(at..at + remove, insert);

        let top: Vec<NodeId> = self.document.children(self.document.root()).to_vec();
        let bounds: Vec<Range<usize>> = top
            .iter()
            .map(|id| self.source_map.get(*id).unwrap_or(0..0))
            .collect();
        let blocks: Vec<&Block> = top
            .iter()
            .filter_map(|id| self.document.block(*id))
            .collect();
        let line_starts: Vec<usize> = bounds
            .iter()
            .map(|range| block::line_start_of(&self.source, range.start))
            .collect();
        // Whether each top-level block **ends in a construct whose terminator is
        // not a blank line**, at any depth. Such a block can grow into the gap
        // after it without being edited at all — measured on
        // `- ``` * \n\n\nafter`, where inserting a blank line *between* the list
        // and the paragraph lengthened the fenced code block inside the list,
        // which no region starting after the list can see.
        let trailing_open: Vec<bool> = top
            .iter()
            .map(|id| ends_open(&self.document, *id))
            .collect();

        let last = top.len().saturating_sub(1);
        let (mut lo, mut hi) = touched(&line_starts, &bounds, self.source.len(), at, at + remove);
        let mut built;
        let mut region;
        loop {
            // Widen while the boundary with the neighbour is not hard.
            while lo > 0 && !hard_boundary(&self.source, &bounds, &blocks, &trailing_open, lo - 1) {
                lo -= 1;
            }
            while hi < last && !hard_boundary(&self.source, &bounds, &blocks, &trailing_open, hi) {
                hi += 1;
            }
            region = match self.span_for(
                lo,
                hi,
                &bounds,
                &line_starts,
                &source,
                at,
                remove,
                insert.len(),
            ) {
                Some(span) => span,
                None => 0..source.len(),
            };
            // The region and the sibling run it replaces are one decision: a
            // guard that widens the span has to widen the run with it, or the
            // patch grafts the whole document over three paragraphs.
            if region == (0..source.len()) {
                lo = 0;
                hi = last;
            }

            let mut seen = HashSet::new();
            built = block::build_span(&source, self.options, region.clone(), &mut seen);

            let next_indent = bounds
                .get(hi + 1)
                .map_or(0, |next| next.start - line_starts[hi + 1]);
            if region == (0..source.len())
                || self.region_is_self_contained(
                    &built,
                    &region,
                    &source,
                    &blocks,
                    &trailing_open,
                    lo,
                    hi,
                    top.len(),
                    next_indent,
                )
            {
                break;
            }
            // Step out one sibling on each side. `lo` falls and `hi` rises, so
            // the loop terminates at the whole document.
            if lo == 0 && hi >= last {
                region = 0..source.len();
                let mut seen = HashSet::new();
                built = block::build_span(&source, self.options, region.clone(), &mut seen);
                break;
            }
            lo = lo.saturating_sub(1);
            hi = (hi + 1).min(last);
        }

        let blocks_reparsed = built.iter().map(count_built).sum();
        let root = self.document.root();
        let after: Vec<NodeId> = top[(hi + 1).min(top.len())..].to_vec();

        // Everything after the region moves by the edit's delta and nothing
        // else; everything before it is untouched. The region's own ranges come
        // from the parse that just ran, which is what `SourceMap`'s docs promise
        // — "S6's region reparse produces new ones for the region rather than
        // patching these".
        for id in &after {
            shift_subtree(
                &self.document,
                &mut self.source_map,
                *id,
                at,
                remove,
                insert.len(),
            );
        }

        let mut inverse = Vec::new();
        patch(
            &mut self.document,
            &mut self.source_map,
            root,
            lo,
            hi + 1 - lo,
            built,
            &mut inverse,
        );

        // `to_document`'s fallback, one edit later: a document with no blocks is
        // an empty paragraph, because that is `markdownToState`'s own
        // `states.length ? states : [{ name: 'paragraph', text: '' }]`.
        if self.document.children(root).is_empty() {
            let mut back = self.document.apply(&[Edit::InsertNode {
                parent: root,
                index: 0,
                block: Block::Paragraph {
                    text: mt_doc::Text::new(),
                },
            }]);
            if let [Edit::RemoveNode { node }] = back.as_slice() {
                self.source_map.insert(*node, 0..0);
            }
            back.append(&mut inverse);
            inverse = back;
        }

        inverse.reverse();
        self.source = source;
        self.labels = crate::labels::collect(&self.document);
        Reparse {
            blocks_reparsed,
            blocks_total: count_blocks(&self.document, root),
            path: ReparsePath::Region,
            region,
            inverse,
        }
    }

    /// The region's span in the **new** source, or `None` for "the whole
    /// document" — the three guards the module docs name.
    #[allow(clippy::too_many_arguments)]
    fn span_for(
        &self,
        lo: usize,
        hi: usize,
        bounds: &[Range<usize>],
        line_starts: &[usize],
        source: &str,
        at: usize,
        remove: usize,
        inserted: usize,
    ) -> Option<Range<usize>> {
        if bounds.is_empty() || self.options.footnote {
            return None;
        }
        let start = if lo == 0 { 0 } else { bounds[lo - 1].end };
        let was_end = if hi + 1 >= bounds.len() {
            self.source.len()
        } else {
            line_starts[hi + 1]
        };
        let end = shift(was_end, at, remove, inserted).min(source.len());
        let start = start.min(end);
        // The edit has to be inside the region for the shift above to be the
        // whole of the difference between the two sources.
        if start > at || end < at + inserted {
            return None;
        }
        // A label defined at or before the region's end is a label the region
        // cannot see and cannot know it must suppress. One *after* it is
        // harmless — but only if there is none before it in **either** source:
        // an edit that destroys the first of two definitions of the same label
        // un-suppresses the second, which lives outside the region and would
        // otherwise never be revisited. Asking the new source alone misses that
        // by construction, because the definition it has to notice is the one
        // the edit removed.
        if defines_a_label(&source[..end]) || defines_a_label(&self.source[..was_end]) {
            return None;
        }
        // **A lone `\r` in the region makes it unreasonable in isolation too**,
        // and for a sharper reason than a label's. This crate has two engines
        // behind it: the block structure is `pulldown-cmark`'s, which ends a
        // line at a bare `\r`, and the text is re-derived `marked`-style, where
        // a `\r` is an ordinary character (see the grid note above
        // `block::line_start_of`). Unifying the grid stops the port
        // *crashing* on the disagreement; it does not make the two engines agree
        // about what the document **is**, and a region is exactly a bet that
        // parsing a slice gives what parsing the whole would. That bet loses
        // here: `parse("    indented\n    code\n\n\r#")` drops the trailing
        // heading, while the same region parsed on its own keeps it, because in
        // isolation there is no indented code block in front of it.
        //
        // Asked of **both** sources for the same reason the labels are: the `\r`
        // that matters may be the one the edit brought in, which the old source
        // cannot show, or the one it took out, which the new one cannot. It
        // costs a whole-document reparse on a document containing a lone `\r`
        // and nothing at all on one that does not — and the file layer
        // ([`crate::normalize_source`]) means no document opened from disk can
        // contain one. Found by `reparse_properties`' generated sequences at
        // 6,000 cases, twice, once from each side.
        if block::holds_a_lone_carriage_return(source, start..end)
            || block::holds_a_lone_carriage_return(&self.source, start..was_end)
        {
            return None;
        }
        // The front matter is a whole-document construct with an unanchored
        // closing delimiter (see [`Incremental::front_matter_moved`]), so a
        // region either contains all of it or begins after all of it. Three
        // things follow, and the middle one is the trap: **`Block::Frontmatter`'s
        // range is its *text*, not its extent** — the delimiters are not in it —
        // so `bounds[0].end` sits before the closing `---`, and a region that
        // starts there hands `build_region` a `---` that becomes a thematic
        // break the document does not contain.
        if self.front_matter_moved(source, at, remove, inserted) {
            return None;
        }
        let front_matter = block::front_matter_end(source, self.options);
        if front_matter > 0 {
            if lo == 0 {
                // The run to be replaced contains the front-matter block itself.
                return None;
            }
            let start = start.max(front_matter);
            return (start <= at).then_some(start..end);
        }
        if start == 0 && front_matter > end {
            return None;
        }
        Some(start..end)
    }

    /// Whether the reparse's own output agrees that the region was a region.
    ///
    /// The choice was made from the tree *before* the edit; this asks the same
    /// question of the tree the edit produced, which is the only way to see an
    /// edit that **created** a merge — a paragraph that became a list beside a
    /// list, a fence that opened and never closed.
    #[allow(clippy::too_many_arguments)]
    fn region_is_self_contained(
        &self,
        built: &[Built],
        region: &Range<usize>,
        source: &str,
        blocks: &[&Block],
        trailing_open: &[bool],
        lo: usize,
        hi: usize,
        siblings: usize,
        next_indent: usize,
    ) -> bool {
        let (Some(first), Some(last)) = (built.first(), built.last()) else {
            // The region emptied. Whether the neighbours then merge with each
            // other is not answerable from here.
            return lo == 0 && hi + 1 >= siblings;
        };
        if lo > 0
            && (!block::preceded_by_blank_line(source, first.range.start)
                || trailing_open[lo - 1]
                || mergeable(
                    blocks[lo - 1],
                    &first.block,
                    indent_of(source, first.range.start),
                ))
        {
            return false;
        }
        if hi + 1 < siblings {
            // The region ends where the next block's line begins, so a block
            // that runs to the region's end has consumed the gap and in a full
            // parse would keep going.
            if last.range.end >= region.end
                || !block::preceded_by_blank_line(source, region.end)
                || mergeable(&last.block, blocks[hi + 1], next_indent)
            {
                return false;
            }
            // **The two constructs whose terminator is not a blank line**, at
            // any depth. A fenced code block ends at its closing fence and an
            // html block of `marked`'s type 1 at its closing tag, so either can
            // reach past the gap — and a region that ends *inside* one is
            // closed by end-of-input instead, which is a different block. The
            // list item's fence in `- ```*\n\n\nfter` is where this was
            // measured: the region gave it the empty string and the document
            // gives it a newline.
            if matches!(
                deepest_last(built).map(|b| &b.block),
                Some(Block::CodeBlock {
                    kind: mt_doc::CodeKind::Fenced,
                    ..
                }) | Some(Block::HtmlBlock { .. })
            ) {
                return false;
            }
        }
        true
    }
}

/// The top-level siblings the edit touches, as an inclusive index range.
///
/// Two things about it that the obvious version gets wrong, both measured:
///
/// - **A block owns the gap after it.** Deleting the blank line between a
///   paragraph and an indented code block changes the paragraph without
///   touching a byte of it. Asking `bounds[i].end >= start` would miss that,
///   and cannot be repaired, because `pulldown-cmark` sometimes puts the
///   newline that ends a block inside its range and sometimes does not — the
///   *end* of a block is not a number this can be built on.
/// - **A block owns the indent its range does not cover.** An indented code
///   block's range starts at its first non-whitespace byte (`strip_lines`'s
///   clip is where S2 records that), so a tab typed into `    code`'s indent is
///   *before* the block by that measure and inside it by any other. Every
///   boundary here is therefore a **line start**.
fn touched(
    line_starts: &[usize],
    bounds: &[Range<usize>],
    src_len: usize,
    start: usize,
    end: usize,
) -> (usize, usize) {
    let n = bounds.len();
    if n == 0 {
        return (0, 0);
    }
    let gap_end = |i: usize| {
        if i + 1 < n {
            line_starts[i + 1]
        } else {
            src_len
        }
    };
    // `>` rather than `>=`: an insertion at exactly the next block's line start
    // is that block's content, not this block's gap.
    let lo = (0..n).find(|i| gap_end(*i) > start).unwrap_or(n - 1);
    let hi = (0..n).rev().find(|i| line_starts[*i] <= end).unwrap_or(0);
    if lo > hi { (hi, lo) } else { (lo, hi) }
}

/// Whether siblings `i` and `i + 1` are separated by a boundary a full parse
/// could not cross.
///
/// A blank line in the gap between them, and two blocks that cannot merge
/// across one. The question is asked of the **gap** rather than of the second
/// block's line, because `pulldown-cmark`'s ranges are not consistent about
/// whether the trailing newline is inside the block (S4's "the trap nobody
/// named") — and a gap that reads one newline short answers `false`, which
/// widens the region and is the safe direction.
fn hard_boundary(
    src: &str,
    bounds: &[Range<usize>],
    blocks: &[&Block],
    trailing_open: &[bool],
    i: usize,
) -> bool {
    let Some(next) = bounds.get(i + 1) else {
        return true;
    };
    if !block::preceded_by_blank_line(src, next.start) || trailing_open.get(i) == Some(&true) {
        return false;
    }
    match (blocks.get(i), blocks.get(i + 1)) {
        (Some(a), Some(b)) => !mergeable(a, b, indent_of(src, next.start)),
        _ => false,
    }
}

/// Whether a node's subtree **ends** in a fenced code block or an html block —
/// the two constructs `marked` and CommonMark both close with something other
/// than a blank line, and therefore the two that can grow into the gap after
/// them when that gap changes.
fn ends_open(doc: &Document, id: NodeId) -> bool {
    match doc.children(id).last() {
        Some(child) => ends_open(doc, *child),
        None => matches!(
            doc.block(id),
            Some(Block::CodeBlock {
                kind: mt_doc::CodeKind::Fenced,
                ..
            }) | Some(Block::HtmlBlock { .. })
        ),
    }
}

/// Whether `a` could **absorb** `b` across the blank line between them.
///
/// Directional, because absorption is: a blank line ends a paragraph, a
/// heading, a block quote and a table for good, and the three kinds below reach
/// across one.
///
/// - **A list.** Its items have a content column, so anything indented after a
///   blank line joins the last item and makes the list loose — and another list
///   is simply more of it. `b_indent` is what keeps this from widening every
///   region that ends beside a list: a paragraph at column 0 after a list is
///   its own block in every parse.
/// - **An indented code block**, which is exactly "indented lines, blank lines
///   allowed".
/// - **An html block**, whose terminator is its own closing tag; an edit can
///   delete it and the block runs on.
///
/// Conservative in the only direction that costs anything: `true` widens a
/// region, `false` where they can merge is a wrong tree.
fn mergeable(a: &Block, b: &Block, b_indent: usize) -> bool {
    let list = |block: &Block| {
        matches!(
            block,
            Block::BulletList { .. } | Block::OrderList { .. } | Block::TaskList { .. }
        )
    };
    match a {
        _ if list(a) => b_indent > 0 || list(b),
        Block::CodeBlock {
            kind: mt_doc::CodeKind::Indented,
            ..
        } => b_indent > 0,
        Block::HtmlBlock { .. } => true,
        _ => false,
    }
}

/// A block's own indent: how far its content sits from the start of its line.
fn indent_of(src: &str, start: usize) -> usize {
    start.saturating_sub(block::line_start_of(src, start))
}

/// Whether `text` holds a line that could be a link reference definition.
///
/// ` {0,3}\[` at a line start with a `]:` after it — `marked`'s `def` rule head,
/// loosened to a scan because the question is only *may a label be defined
/// here*. `seen_labels` is document-global, so a `true` anywhere at or before
/// the region's end means the region cannot be reasoned about in isolation.
fn defines_a_label(text: &str) -> bool {
    text.lines().any(|line| {
        let indent = line.bytes().take(4).take_while(|b| *b == b' ').count();
        indent <= 3 && line[indent..].starts_with('[') && line[indent..].contains("]:")
    })
}

/// The last block of a built forest in document order, at **any** depth.
fn deepest_last(built: &[Built]) -> Option<&Built> {
    let last = built.last()?;
    Some(deepest_last(&last.children).unwrap_or(last))
}

fn count_built(built: &Built) -> usize {
    1 + built.children.iter().map(count_built).sum::<usize>()
}

fn count_blocks(doc: &Document, id: NodeId) -> usize {
    doc.children(id)
        .iter()
        .map(|child| 1 + count_blocks(doc, *child))
        .sum()
}

fn shift_subtree(
    doc: &Document,
    map: &mut SourceMap,
    id: NodeId,
    at: usize,
    remove: usize,
    inserted: usize,
) {
    if let Some(range) = map.get(id) {
        let start = shift(range.start, at, remove, inserted);
        map.insert(id, start..shift(range.end, at, remove, inserted).max(start));
    }
    for child in doc.children(id) {
        shift_subtree(doc, map, *child, at, remove, inserted);
    }
}

// ---------------------------------------------------------------------------
// The subtree diff
// ---------------------------------------------------------------------------

/// One position in the alignment between an old sibling run and a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// The subtrees are equal. The node keeps its id and emits no edit at all,
    /// which is what makes "the `NodeId`s of untouched blocks survive" a
    /// property of the algorithm rather than of the inputs.
    Keep(usize, usize),
    /// Same kind, different content. The node keeps its id and is edited into
    /// shape.
    Update(usize, usize),
    Insert(usize),
    Remove(usize),
}

/// Replace `old_count` of `parent`'s children starting at `first` with `new`,
/// keeping the id of every node that survives.
fn patch(
    doc: &mut Document,
    map: &mut SourceMap,
    parent: NodeId,
    first: usize,
    old_count: usize,
    new: Vec<Built>,
    inverse: &mut Vec<Edit>,
) {
    let olds: Vec<NodeId> = doc.children(parent)[first..first + old_count].to_vec();
    let plan = align(doc, &olds, &new);

    let mut cursor = first;
    for step in plan {
        match step {
            Step::Keep(_, n) => {
                record_ranges(doc, map, doc.children(parent)[cursor], &new[n]);
                cursor += 1;
            }
            Step::Update(o, n) => {
                update(doc, map, olds[o], &new[n], inverse);
                cursor += 1;
            }
            Step::Remove(o) => {
                forget_subtree(doc, map, olds[o]);
                inverse.extend(doc.apply(&[Edit::RemoveNode { node: olds[o] }]));
            }
            Step::Insert(n) => {
                graft(doc, map, parent, cursor, &new[n], inverse);
                cursor += 1;
            }
        }
    }
}

/// The alignment: common prefix and suffix first, a longest common subsequence
/// over what is left, and a positional pairing inside each gap.
///
/// Trimming before the LCS is what keeps this linear on the shape that actually
/// happens — an edit changes one block and leaves every other sibling equal, so
/// the quadratic middle is one or two entries wide.
fn align(doc: &Document, olds: &[NodeId], news: &[Built]) -> Vec<Step> {
    let mut head = 0;
    while head < olds.len() && head < news.len() && subtree_eq(doc, olds[head], &news[head]) {
        head += 1;
    }
    let mut tail = 0;
    while tail < olds.len() - head
        && tail < news.len() - head
        && subtree_eq(
            doc,
            olds[olds.len() - 1 - tail],
            &news[news.len() - 1 - tail],
        )
    {
        tail += 1;
    }

    let mut plan: Vec<Step> = (0..head).map(|i| Step::Keep(i, i)).collect();
    let (o_mid, n_mid) = (
        &olds[head..olds.len() - tail],
        &news[head..news.len() - tail],
    );

    // Longest common subsequence over the middle, on the same equality.
    let (rows, cols) = (o_mid.len(), n_mid.len());
    let mut table = vec![0usize; (rows + 1) * (cols + 1)];
    for i in (0..rows).rev() {
        for j in (0..cols).rev() {
            table[i * (cols + 1) + j] = if subtree_eq(doc, o_mid[i], &n_mid[j]) {
                table[(i + 1) * (cols + 1) + j + 1] + 1
            } else {
                table[(i + 1) * (cols + 1) + j].max(table[i * (cols + 1) + j + 1])
            };
        }
    }

    let (mut i, mut j) = (0, 0);
    let (mut gap_o, mut gap_n) = (Vec::new(), Vec::new());
    let flush = |gap_o: &mut Vec<usize>, gap_n: &mut Vec<usize>, plan: &mut Vec<Step>| {
        let paired = gap_o.len().min(gap_n.len());
        for k in 0..paired {
            let (o, n) = (gap_o[k], gap_n[k]);
            if reconcilable(doc, olds[o], &news[n]) {
                plan.push(Step::Update(o, n));
            } else {
                plan.push(Step::Remove(o));
                plan.push(Step::Insert(n));
            }
        }
        for o in gap_o.drain(paired..) {
            plan.push(Step::Remove(o));
        }
        for n in gap_n.drain(paired..) {
            plan.push(Step::Insert(n));
        }
        gap_o.clear();
        gap_n.clear();
    };

    while i < rows && j < cols {
        if subtree_eq(doc, o_mid[i], &n_mid[j]) {
            flush(&mut gap_o, &mut gap_n, &mut plan);
            plan.push(Step::Keep(head + i, head + j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * (cols + 1) + j] >= table[i * (cols + 1) + j + 1] {
            gap_o.push(head + i);
            i += 1;
        } else {
            gap_n.push(head + j);
            j += 1;
        }
    }
    while i < rows {
        gap_o.push(head + i);
        i += 1;
    }
    while j < cols {
        gap_n.push(head + j);
        j += 1;
    }
    flush(&mut gap_o, &mut gap_n, &mut plan);

    for k in 0..tail {
        plan.push(Step::Keep(olds.len() - tail + k, news.len() - tail + k));
    }
    plan
}

/// The matching key: name, `meta`, text and children, recursively.
fn subtree_eq(doc: &Document, id: NodeId, built: &Built) -> bool {
    let Some(block) = doc.block(id) else {
        return false;
    };
    if !block_eq(block, &built.block) {
        return false;
    }
    let children = doc.children(id);
    children.len() == built.children.len()
        && children
            .iter()
            .zip(&built.children)
            .all(|(child, built)| subtree_eq(doc, *child, built))
}

/// A block without its children: everything [`mt_doc::state`]-shaped about it.
fn block_eq(a: &Block, b: &Block) -> bool {
    a.name() == b.name()
        && a.meta() == b.meta()
        && a.text().map(mt_doc::Text::to_str) == b.text().map(mt_doc::Text::to_str)
}

/// Whether a leftover old node can be *edited* into a leftover new one instead
/// of being replaced by it.
///
/// Two cases, and the second is the one [`Edit::ReplaceBlock`] was written for
/// at S0 — *"the paragraph-to-heading kind of transformation"*:
///
/// - **The same kind.** Text, `meta` and children are reconciled in place.
/// - **Two leaves of different kinds.** One `ReplaceBlock`, and the node keeps
///   its id through a change that would otherwise cost `mt-layout` its cache
///   entry every time a `#` is typed at the start of a line.
///
/// A container that changes kind is **not** reconcilable, and that is a
/// deliberate limit rather than an oversight: `ReplaceBlock` adopts the new
/// block's children and detaches the old block's, and a `Built` container
/// carries no ids, so replacing a `bullet-list` with an `order-list` would
/// detach every descendant and re-graft it with fresh ids anyway. Remove and
/// insert says so honestly instead of pretending the id survived.
fn reconcilable(doc: &Document, id: NodeId, built: &Built) -> bool {
    let Some(old) = doc.block(id) else {
        return false;
    };
    old.name() == built.block.name() || (old.is_leaf() && built.block.is_leaf())
}

/// Edit a surviving node into the shape of `built`.
fn update(
    doc: &mut Document,
    map: &mut SourceMap,
    id: NodeId,
    built: &Built,
    inverse: &mut Vec<Edit>,
) {
    map.insert_with(id, built.range.clone(), built.exact);

    if doc.block(id).map(Block::name) != Some(built.block.name()) {
        inverse.extend(doc.apply(&[Edit::ReplaceBlock {
            node: id,
            block: built.block.clone(),
        }]));
        return;
    }

    let old_text = doc
        .block(id)
        .and_then(Block::text)
        .map(|text| text.to_str().into_owned());
    if let (Some(old), Some(new)) = (old_text, built.block.text())
        && let Some((at, remove, insert)) = minimal_splice(&old, &new.to_str())
    {
        inverse.extend(doc.apply(&[Edit::SpliceText {
            node: id,
            at,
            remove,
            insert,
        }]));
    }
    if let (Some(old), Some(new)) = (doc.block(id).and_then(Block::meta), built.block.meta())
        && old != new
    {
        inverse.extend(doc.apply(&[Edit::SetMeta {
            node: id,
            meta: new,
        }]));
    }
    if built.block.children().is_some() {
        let count = doc.children(id).len();
        patch(doc, map, id, 0, count, built.children.clone(), inverse);
    }
}

/// Insert a whole new subtree, reading each minted id out of the inverse of the
/// `InsertNode` that minted it — `block::insert`'s trick, for D9's reason.
fn graft(
    doc: &mut Document,
    map: &mut SourceMap,
    parent: NodeId,
    index: usize,
    built: &Built,
    inverse: &mut Vec<Edit>,
) {
    let back = doc.apply(&[Edit::InsertNode {
        parent,
        index,
        block: built.block.clone(),
    }]);
    let id = match back.as_slice() {
        [Edit::RemoveNode { node }] => *node,
        other => unreachable!("InsertNode's inverse is RemoveNode; got {other:?}"),
    };
    inverse.extend(back);
    map.insert_with(id, built.range.clone(), built.exact);
    for (at, child) in built.children.iter().enumerate() {
        graft(doc, map, id, at, child, inverse);
    }
}

/// Give a surviving subtree the ranges of the parse that just produced it.
fn record_ranges(doc: &Document, map: &mut SourceMap, id: NodeId, built: &Built) {
    map.insert_with(id, built.range.clone(), built.exact);
    for (child, built) in doc.children(id).iter().zip(&built.children) {
        record_ranges(doc, map, *child, built);
    }
}

/// Drop a detached subtree's ranges. The nodes stay in the arena for the
/// inverse batch to name; the map is about the **live** tree, and
/// `tests/source_ranges.rs`'s "one entry per live node" is what says so.
fn forget_subtree(doc: &Document, map: &mut SourceMap, id: NodeId) {
    for child in doc.children(id) {
        forget_subtree(doc, map, *child);
    }
    map.remove(id);
}

#[cfg(test)]
mod tests;
