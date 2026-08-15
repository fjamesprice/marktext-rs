//! Syntax-highlight spans, resolved above the seam — M3 §5 **D15**.
//!
//! # Why this is a table the caller fills rather than a crate edge
//!
//! D15: *"`mt-layout` takes no dependency on `mt-highlight`. Spans arrive as a
//! caller-supplied table on [`LayoutOptions`](crate::LayoutOptions), beside
//! `images: ImageSizes`, and are resolved to [`StyleRun`](crate::StyleRun)s at
//! the one site that already shapes a fence."*
//!
//! **The check that looks like it decides this cannot.** `cargo xtask deps` is
//! a line scan over the fourteen manifests plus a substring grep over the
//! headless crates' `src/`. `mt-highlight` is on neither denylist, so
//! `mt-layout → mt-highlight` **passes today**; the manifest scan is *not*
//! transitive, so an `mt-highlight` that opened files would keep passing while
//! §1's actual invariant — the headless four compile and test with no window
//! and no I/O — was broken. The edge is therefore refused **by construction**
//! and not because a tool said no. Whether `deps.rs` should be made transitive
//! is S7's to decide.
//!
//! # This is [`ImageSizes`](crate::ImageSizes)' idiom, and deliberately so
//!
//! D12 has a declared table, a shell that decodes headers and inserts, and a
//! plain value that crosses the seam. D15 has the same three, with two
//! differences worth naming:
//!
//! | | `ImageSizes` | `CodeSpans` |
//! |---|---|---|
//! | key | the token's `src`, verbatim | the block's [`NodeId`] |
//! | a miss | a **completed failure** with its own CSS geometry | an **unhighlighted fence**, which is S3's coverage number and not a defect |
//!
//! The key is a `NodeId` rather than the code text because two fences with
//! identical bytes are still two blocks, and because the shell already has the
//! id: it walked the tree to find the fence in the first place. It follows that
//! a table is only meaningful against the `Document` it was built from — a
//! generational id from another parse resolves to nothing here, exactly as it
//! would in `mt_doc`'s own arena.
//!
//! # There is no offset conversion at this seam
//!
//! A code block does not lay out inline markdown, so its visible-text map is
//! [`VisibleTextMap::identity`](crate::inline::VisibleTextMap::identity) over
//! the whole text (`flow.rs`'s `build`). Highlight byte ranges are therefore
//! already ranges into the string `mt-layout` hands parley, and the third
//! offset space §4 C6 warns about does not open here. **The ranges are bytes
//! into the block's own text, not into the document.**
//!
//! # What a class is, and what an unknown one means
//!
//! [`HighlightSpan::class`] is one Prism token class — `keyword`, `string`,
//! `comment` — resolved **alias over type, last alias wins** (D15). A class
//! [`CodePalette::by_class`](crate::theme::CodePalette::by_class) does not
//! style produces **no style run at all**, which is what a browser does with a
//! class no rule matches: the text inherits. See `flow.rs`'s
//! `code_style_runs` for the one place that decides it, and for the two cases
//! where inheriting and the block default are measurably not the same thing.

use std::collections::BTreeMap;
use std::ops::Range;

use mt_doc::NodeId;

/// One highlight run: a byte range into **one block's** text, and the Prism
/// token class it paints in.
///
/// Deliberately not `mt_highlight::Span` — this crate names no type of that
/// crate's and takes no dependency on it (D15). The shape is the same because
/// the seam is a copy, not a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Start byte, inclusive.
    pub start: usize,
    /// End byte, exclusive.
    pub end: usize,
    /// The Prism token class, spelled as a stylesheet spells it —
    /// `attr-name`, not `attr_name`.
    pub class: String,
}

impl HighlightSpan {
    /// A span.
    pub fn new(range: Range<usize>, class: impl Into<String>) -> HighlightSpan {
        HighlightSpan {
            start: range.start,
            end: range.end,
            class: class.into(),
        }
    }

    /// The range, as a range.
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

/// The shell's answer to *"how is this fence highlighted?"* — **D15**.
///
/// An empty table is the correct value for a caller with no highlighter, and
/// every `mt-layout` test that does not name this field uses one: a fence then
/// shapes with zero style runs and one flat brush, which is exactly what every
/// golden held before S3. **It is not a degenerate case to be defended
/// against.**
///
/// # The contract on a span list, which this crate does not police at runtime
///
/// Spans must be **ascending, non-overlapping and non-empty**, and every offset
/// must be a char boundary of the block's text. `mt_highlight::highlight`
/// guarantees all four and asserts them in its own tests over every ported
/// language. `mt-layout` re-checks them in `debug_assert!`s at the resolver and
/// skips a span that violates them in release, because [`LayoutTree::build`]
/// has no error channel and a panic in a viewer is worse than an uncoloured
/// token.
///
/// [`LayoutTree::build`]: crate::LayoutTree::build
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeSpans {
    by_node: BTreeMap<NodeId, Vec<HighlightSpan>>,
}

impl CodeSpans {
    /// An empty table: every fence lays out unhighlighted.
    pub fn new() -> CodeSpans {
        CodeSpans::default()
    }

    /// Record one block's spans. Replaces any previous entry.
    pub fn insert(
        &mut self,
        node: NodeId,
        spans: Vec<HighlightSpan>,
    ) -> Option<Vec<HighlightSpan>> {
        self.by_node.insert(node, spans)
    }

    /// The spans recorded for `node`, or `None` — which means *"the shell has
    /// no grammar for this fence"* and is S3's coverage number rather than a
    /// defect.
    ///
    /// An empty slice is a **different** answer: the language is ported and
    /// nothing in this fence matched a rule. Both lay out identically here; the
    /// distinction is the caller's and `mt_highlight::highlight`'s.
    pub fn get(&self, node: NodeId) -> Option<&[HighlightSpan]> {
        self.by_node.get(&node).map(Vec::as_slice)
    }

    /// How many blocks the shell highlighted.
    pub fn len(&self) -> usize {
        self.by_node.len()
    }

    /// Whether the shell highlighted none.
    pub fn is_empty(&self) -> bool {
        self.by_node.is_empty()
    }

    /// Every span in the table, for a shell that wants to report what it
    /// handed down. `cargo xtask layout` prints this in the golden header, so
    /// that *"the seam ran"* is visible in the artifact rather than inferred
    /// from it.
    pub fn total_spans(&self) -> usize {
        self.by_node.values().map(Vec::len).sum()
    }
}

impl FromIterator<(NodeId, Vec<HighlightSpan>)> for CodeSpans {
    fn from_iter<T: IntoIterator<Item = (NodeId, Vec<HighlightSpan>)>>(iter: T) -> CodeSpans {
        CodeSpans {
            by_node: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mt_doc::Document;

    /// A live `NodeId`: `Document::new` is a root holding one empty paragraph.
    fn one_node() -> (Document, NodeId) {
        let doc = Document::new();
        let node = doc.children(doc.root())[0];
        (doc, node)
    }

    #[test]
    fn a_miss_is_an_unhighlighted_fence_and_not_an_empty_span_list() {
        let (doc, node) = one_node();
        let mut spans = CodeSpans::new();
        assert_eq!(spans.get(node), None);
        spans.insert(node, Vec::new());
        // Now it is present and empty: "ported, nothing matched".
        assert_eq!(spans.get(node), Some(&[][..]));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans.total_spans(), 0);
        let _ = doc;
    }

    #[test]
    fn an_empty_table_is_a_value_and_not_an_absence() {
        let (doc, node) = one_node();
        let spans = CodeSpans::new();
        assert!(spans.is_empty());
        assert_eq!(spans.get(node), None);
        let _ = doc;
    }

    #[test]
    fn a_span_carries_a_range_and_a_class_spelled_as_css_spells_it() {
        let span = HighlightSpan::new(3..7, "attr-name");
        assert_eq!(span.range(), 3..7);
        assert_eq!(span.class, "attr-name");
    }
}
