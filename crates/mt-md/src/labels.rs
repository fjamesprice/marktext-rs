//! The label-map pass — `InlineRenderer.collectReferenceDefinitions`,
//! transcribed.
//!
//! Forty lines of `inlineRenderer/index.ts` (77–115) and the whole of what
//! makes `[text][ref]` a reference link rather than plain text:
//! [`mt_inline::TokenizerOptions::labels`] is the only input that decides it,
//! and until this module existed nothing had ever populated it.
//!
//! ```text
//! Document ──► every `paragraph`, depth-first ──► mt_inline::label_info ──► Labels
//! ```
//!
//! # There are two reference-definition scanners, and there must stay two
//!
//! This is the second one, and it disagrees with the first on purpose.
//!
//! | | [`block::scan_definitions`](crate::block) | this module |
//! |---|---|---|
//! | Decides | **where a paragraph is** | **what a label points at** |
//! | Reads | the source a gap in the event stream covers | a paragraph's reconstructed text |
//! | Rule it reproduces | `marked`'s `tokens.links` | `beginRules.reference_definition` |
//! | Normalises a label with | `toLowerCase()` **and** `/\s+/g → ' '` | `toLowerCase()` only |
//! | On a repeated label | the second emits **no block at all** | the last write **wins** |
//!
//! muya has both and they disagree, so `"[foo bar]: /a"` and `"[foo  bar]: /a"`
//! are one label to the block layer and two to this one, and two definitions of
//! the same label resolve oppositely in each. Unifying them is the obvious
//! cleanup and it is the wrong one: the block layer decides which bytes survive
//! a round trip and this one decides which link a `[ref]` resolves to, and
//! those are different questions that muya answers with different code.
//!
//! S2 deleted `content_start` for what looks like the opposite reason and is
//! not: that helper and `strip_lines` were two implementations of **one**
//! question — where a line's content starts — and the two answers had to be the
//! same or a definition's paragraph would have had a range that did not match
//! its own text. Here there are two questions.
//!
//! # Last write wins, and why the transcribed spec says first
//!
//! `_collectReferenceDefinitions` is a `Map.set` in document order, so a later
//! definition **overwrites** an earlier one. `referenceLink.spec.ts`'s own
//! mirror of it guards with `if (!labels.has(label))` — first wins — and the
//! two cannot be told apart by any input the spec uses, because a repeated
//! label never produces a second paragraph for the pass to see (the block
//! layer already dropped it, `marked`'s `def` branch).
//!
//! They *are* distinguishable, by a paragraph that is not a definition to
//! `marked` but is one to this regex: `[a\]: /first` has an escaped `]`, so
//! neither `marked` nor `pulldown-cmark` makes a definition of it and two of
//! them are two ordinary paragraphs — which this pass then reads as two
//! definitions of the label `a\`. **The running engine is what is transcribed**,
//! so the last one wins; [`tests::the_last_definition_of_a_label_wins`] carries
//! the input.

use mt_doc::{Block, Document, NodeId};
use mt_inline::Labels;

/// Every `paragraph` in the document, depth-first, in document order.
///
/// `travel` visits `st.name === 'paragraph'` and otherwise recurses into
/// `st.children` if there are any — so a container recurses and **every other
/// leaf kind is skipped**. A `table-cell` holding `[a]: /u` defines nothing; so
/// does a `code-block`, an `atx-heading` and an `html-block`. That is not an
/// oversight of muya's worth repairing: the pass runs on every block render,
/// and a leaf kind it skips is one it never has to re-read.
///
/// A `footnote` is a container, so a definition inside one *is* collected.
pub fn collect(doc: &Document) -> Labels {
    let mut labels = Labels::new();
    visit(doc, doc.root(), &mut labels);
    labels
}

fn visit(doc: &Document, id: NodeId, labels: &mut Labels) {
    for child in doc.children(id) {
        match doc.block(*child) {
            Some(Block::Paragraph { text }) => {
                if let Some((label, info)) = mt_inline::label_info(&text.to_str()) {
                    // `labels.set` — the last definition of a label wins. See
                    // the module docs for why the spec's mirror says otherwise
                    // and why nothing the spec runs can tell.
                    labels.insert(label, info);
                }
            }
            // A leaf that is not a paragraph: `st.children` is undefined and
            // `travel` falls off the end of the `else if`.
            Some(block) if block.text().is_some() => {}
            // A container, or the root.
            _ => visit(doc, *child, labels),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, block::parse_blocks};
    use mt_inline::Label;

    fn labels_of(markdown: &str) -> Labels {
        collect(&parse_blocks(markdown, Options::MUYA_DEFAULT))
    }

    fn label(href: &str, title: &str) -> Label {
        Label {
            href: href.into(),
            title: title.into(),
        }
    }

    #[test]
    fn a_definition_paragraph_becomes_a_label() {
        assert_eq!(
            labels_of("foo [bar][1]\n\n[1]: https://example.com \"title\"\n"),
            Labels::from([("1".into(), label("https://example.com", "title"))])
        );
    }

    /// CommonMark §6.5 matches labels case-insensitively and
    /// `collectReferenceDefinitions` lowercases on the way in.
    #[test]
    fn the_key_is_lowercased() {
        assert_eq!(labels_of("[REF]: /a\n").keys().collect::<Vec<_>>(), ["ref"]);
    }

    /// The **first** definition is the one that survives, and it survives at
    /// the block layer rather than here: `marked`'s `def` branch pushes a token
    /// only for a label it has not seen, so there is no second paragraph for
    /// this pass to overwrite from. `referenceLink.spec.ts` case 7.
    #[test]
    fn a_duplicate_label_keeps_the_first_definition_because_the_block_layer_dropped_the_second() {
        let markdown =
            "foo [bar][dup]\n\n[dup]: https://first.example\n[dup]: https://second.example\n";
        assert_eq!(
            labels_of(markdown),
            Labels::from([("dup".into(), label("https://first.example", ""))])
        );
        // And the reason: one paragraph reached the pass, not two.
        let doc = parse_blocks(markdown, Options::MUYA_DEFAULT);
        assert_eq!(doc.children(doc.root()).len(), 2);
    }

    /// The distinguishing input the module docs name. `[a\]: /…` has an escaped
    /// `]`, so neither block parser makes a definition of it — two ordinary
    /// paragraphs reach this pass, both match `beginRules.reference_definition`
    /// with the label `a\`, and **the last one wins**.
    ///
    /// This is the one place `collectReferenceDefinitions` and the mirror in
    /// `referenceLink.spec.ts` disagree. The engine is what is transcribed.
    #[test]
    fn the_last_definition_of_a_label_wins() {
        let doc = parse_blocks("[a\\]: /first\n\n[a\\]: /second\n", Options::MUYA_DEFAULT);
        assert_eq!(
            doc.children(doc.root()).len(),
            2,
            "both lines are ordinary paragraphs — the escaped `]` is why"
        );
        assert_eq!(
            collect(&doc),
            Labels::from([("a\\".into(), label("/second", ""))])
        );
    }

    /// Tree position: a definition inside a block quote belongs to the block
    /// quote (`cm#218`), and the walk recurses into containers, so it is still
    /// collected.
    #[test]
    fn a_definition_inside_a_container_is_collected() {
        assert_eq!(labels_of("> [q]: /q\n").keys().collect::<Vec<_>>(), ["q"]);
        assert_eq!(
            labels_of("- [item]: /i\n").keys().collect::<Vec<_>>(),
            ["item"]
        );
    }

    /// Every leaf kind that is not a `paragraph` is skipped, however
    /// definition-shaped its text is. `travel` asks for `st.children` and a
    /// leaf has none.
    #[test]
    fn a_non_paragraph_leaf_defines_nothing() {
        // An indented code block whose text is exactly a definition line.
        assert!(labels_of("    [c]: /c\n").is_empty());
        // A table cell.
        assert!(labels_of("| a |\n| --- |\n| [t]: /t |\n").is_empty());
        // An atx heading — whose text is reconstructed as `# [h]: /h`, so it
        // would not match anyway; the kind is what stops it first.
        assert!(labels_of("# [h]: /h\n").is_empty());
    }

    /// **The `Options::footnote` characterization M2.md §10 owes S3.**
    ///
    /// Both option sets the harnesses drive have `footnote: false`, under which
    /// `[^a]: note` is a plain `paragraph` on both sides — that is S1's half,
    /// and `block::tests` carries it. The half that only becomes a question
    /// here: the paragraph is definition-shaped, so **the label pass registers
    /// `^a`**, and a later `[^a]` in prose is then a shortcut *reference link*
    /// to `note` rather than plain text.
    ///
    /// muya does the same thing for the same reason, and it is not
    /// `Options::footnote`'s doing in either engine —
    /// `beginRules.reference_definition` has no opinion about `^`. So the
    /// narrowing is unchanged and now fully stated: `mt_md::Options::footnote`
    /// gates `lexBlock`'s footnote extension (S5) and, through it,
    /// `mt_inline::SyntaxOptions::footnote`; it has never gated this pass and
    /// does not need to.
    #[test]
    fn a_footnote_definition_registers_a_reference_label_with_footnotes_off() {
        let markdown = "[^a]: note\n";
        let doc = parse_blocks(markdown, Options::MUYA_DEFAULT);
        assert_eq!(
            doc.block(doc.children(doc.root())[0]).map(Block::name),
            Some("paragraph")
        );
        assert_eq!(
            collect(&doc),
            Labels::from([("^a".into(), label("note", ""))])
        );
    }

    /// The two scanners' normalisations, side by side, on the input that
    /// separates them. `marked` collapses runs of whitespace in a label and
    /// this pass does not — so the block layer sees **one** label and drops the
    /// second definition, and the label this pass then registers is the *first*
    /// line's un-collapsed spelling.
    #[test]
    fn the_two_scanners_normalise_a_label_differently() {
        let doc = parse_blocks("[foo bar]: /a\n[foo  bar]: /b\n", Options::MUYA_DEFAULT);
        assert_eq!(
            doc.children(doc.root()).len(),
            1,
            "`marked` collapses `/\\s+/g` in a label, so the second is a repeat"
        );
        assert_eq!(
            collect(&doc),
            Labels::from([("foo bar".into(), label("/a", ""))])
        );
    }

    /// A paragraph that holds a definition **and** something else defines
    /// nothing: the rule's `^` and `$` are not multi-line. `marked` folds a
    /// definition that follows a paragraph into it (M2.md §5 D3's second
    /// reason), so this is the shape that arrives.
    #[test]
    fn a_definition_absorbed_into_a_paragraph_defines_nothing() {
        let doc = parse_blocks("text\n[a]: /u\n", Options::MUYA_DEFAULT);
        assert_eq!(doc.children(doc.root()).len(), 1);
        assert!(collect(&doc).is_empty());
    }
}
