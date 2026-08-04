//! `referenceLink.spec.ts` — 8 cases.
//!
//! The whole `markdown → state → label collection → inline tokenize` pipeline,
//! which is what makes this file the one place the two halves of the port meet
//! before M3.
//!
//! M2.md §5 D3 is the decision these cases pin: **`mt_md::block` scans for
//! definition lines itself and does not read `pulldown-cmark`'s `RefDefs`.**
//!
//! ~~[`a_duplicate_label_keeps_the_first_definition`] is the reproducer D3
//! names — `RefDefs` is keyed by Unicode-case-folded label, so `[dup]: /a` and
//! `[dup]: /b` collapse to one entry and the second line is unrecoverable.
//! muya emits two paragraphs and keeps the first definition as the target.~~
//! **Corrected at S3.** muya emits **one** paragraph and drops the second line
//! — D3's own correction says so, and this file was written before it. The
//! duplicate collapse is a rule the port reproduces rather than a reason to
//! avoid `RefDefs`; the reasons that survive are about *position*, and D3 lists
//! three. See that case for what the assertion used to claim.
//!
//! The label map is built the way muya builds it —
//! `InlineRenderer.collectReferenceDefinitions`, a regex over paragraph text —
//! rather than from a parser API, so that the map and the round-tripped
//! paragraphs cannot disagree. From S3 that pass ships as
//! [`mt_md::labels::collect`] and [`mt_md::parse`] returns its result; the
//! local mirror below is kept because the TypeScript spec has one too, and
//! because a spec that called the implementation would stop being a check of
//! it. The two differ in one respect, named on
//! [`a_duplicate_label_keeps_the_first_definition`]'s sibling in
//! `mt_md::labels::tests`: this mirror is first-write-wins, like
//! `referenceLink.spec.ts`'s, and the engine is last-write-wins.

#![allow(unused_imports)]
use crate::*;

use mt_inline::{Label, Labels, Token, TokenKind, TokenizerOptions};

/// muya's `collectReferenceDefinitions`, over a parsed document.
///
/// Walks every `paragraph` in tree position (D3: a definition inside a block
/// quote belongs to the block quote, `cm#218`), runs
/// `beginRules.reference_definition` over its text, and keeps the **first**
/// definition for each lowercased label.
///
/// The regex is not re-implemented here: `mt_inline` already has it as a begin
/// rule, so tokenizing the paragraph with `has_begin_rules: true` and reading
/// the leading `ReferenceDefinition` token *is* running muya's regex. That is
/// the point of D3's second bullet — one scanner, so the label map and the
/// round-tripped paragraph cannot disagree.
fn collect_labels(doc: &Document) -> Labels {
    let mut labels = Labels::new();
    for paragraph in paragraph_texts(doc) {
        let tokens = mt_inline::tokenizer(
            &paragraph,
            &TokenizerOptions {
                has_begin_rules: true,
                ..TokenizerOptions::muya_default()
            },
        );
        let Some(TokenKind::ReferenceDefinition(definition)) = tokens.first().map(|t| &t.kind)
        else {
            continue;
        };
        let key = definition.label.of(&paragraph).to_lowercase();
        labels.entry(key).or_insert_with(|| Label {
            href: definition.href.of(&paragraph).to_string(),
            title: definition
                .title
                .map(|span| span.of(&paragraph).to_string())
                .unwrap_or_default(),
        });
    }
    labels
}

/// Every `paragraph`'s text, in document order, at any depth.
fn paragraph_texts(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = top(doc);
    stack.reverse();
    while let Some(id) = stack.pop() {
        if name(doc, id) == "paragraph" {
            out.push(text(doc, id));
        } else {
            let mut children = kids(doc, id);
            children.reverse();
            stack.extend(children);
        }
    }
    out
}

fn tokenize_with(text: &str, labels: Labels) -> Vec<Token> {
    mt_inline::tokenizer(
        text,
        &TokenizerOptions {
            has_begin_rules: false,
            labels,
            ..TokenizerOptions::muya_default()
        },
    )
}

fn reference_links(tokens: &[Token]) -> Vec<&mt_inline::ReferenceLink> {
    tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::ReferenceLink(link) => Some(link),
            _ => None,
        })
        .collect()
}

spec_cases! { "reference_link",

/// Case 1. §2's constraint 2: a reference definition is **not** a block type —
/// it round-trips as a `paragraph` whose text is the raw line.
fn loading_markdown_with_a_definition_keeps_it_as_a_paragraph_state() {
    let doc = parse("foo [bar][1]\n\n[1]: https://example.com \"title\"\n", NO_EXT);
    let definition = paragraph_texts(&doc)
        .into_iter()
        .find(|t| t.trim_start().starts_with("[1]:"))
        .expect("the reference definition should be preserved as a paragraph");
    assert!(definition.contains("[1]: https://example.com"), "{definition:?}");
    assert!(definition.contains("\"title\""), "{definition:?}");
}

/// Case 2. And therefore it survives serialization for free.
fn round_trip_output_contains_the_reference_definition_line() {
    let out = round_trip("foo [bar][1]\n\n[1]: https://example.com \"title\"\n", NO_EXT);
    assert!(out.contains("[1]:"), "{out:?}");
    assert!(out.contains("https://example.com"), "{out:?}");
    assert!(out.contains("\"title\""), "{out:?}");
}

/// Case 3. The label map reaches the tokenizer, and the token resolves.
fn reference_link_resolves_to_a_token_whose_label_maps_to_href() {
    let doc = parse("foo [bar][1]\n\n[1]: https://example.com\n", NO_EXT);
    let labels = collect_labels(&doc);
    assert_eq!(
        labels.get("1").map(|l| l.href.as_str()),
        Some("https://example.com")
    );

    let paragraph = paragraph_texts(&doc)
        .into_iter()
        .find(|t| t.starts_with("foo"))
        .expect("the prose paragraph");
    let tokens = tokenize_with(&paragraph, labels);
    let links = reference_links(&tokens);
    assert_eq!(links.len(), 1, "a reference_link should be emitted once labels are known");
    assert_eq!(links[0].label.of(&paragraph), "1");
}

/// Case 4. Full, collapsed and shortcut forms all produce `reference_link`
/// tokens, and only the full form sets `is_full_link`.
fn full_collapsed_and_shortcut_forms_all_produce_reference_link_tokens() {
    let md = "A [full][1] and [collapsed][] and [shortcut] here.\n\n\
              [1]: https://a.example\n[collapsed]: https://b.example\n[shortcut]: https://c.example\n";
    let doc = parse(md, NO_EXT);
    let labels = collect_labels(&doc);
    let paragraph = paragraph_texts(&doc)
        .into_iter()
        .find(|t| t.starts_with("A "))
        .expect("the prose paragraph");

    let tokens = tokenize_with(&paragraph, labels);
    let links = reference_links(&tokens);
    assert_eq!(links.len(), 3);
    assert!(links[0].is_full_link, "[full][1] is the full form");
    assert!(!links[1].is_full_link, "[collapsed][] is not");
    assert!(!links[2].is_full_link, "[shortcut] is not");
}

/// Case 5.
fn a_definitions_title_propagates_through_label_lookup() {
    let doc = parse("foo [bar][ref]\n\n[ref]: https://example.com \"Ref Title\"\n", NO_EXT);
    let labels = collect_labels(&doc);
    assert_eq!(
        labels.get("ref"),
        Some(&Label {
            href: "https://example.com".to_string(),
            title: "Ref Title".to_string(),
        })
    );
}

/// Case 6. CommonMark §6.5 matches labels case-insensitively, and
/// `collectReferenceDefinitions` lowercases on the way in.
fn label_matching_is_case_insensitive() {
    let doc = parse("foo [bar][REF]\n\n[ref]: https://example.com\n", NO_EXT);
    let labels = collect_labels(&doc);
    assert_eq!(
        labels.get("ref").map(|l| l.href.as_str()),
        Some("https://example.com")
    );

    let paragraph = paragraph_texts(&doc)
        .into_iter()
        .find(|t| t.starts_with("foo"))
        .expect("the prose paragraph");
    let tokens = tokenize_with(&paragraph, labels);
    assert_eq!(
        reference_links(&tokens).len(),
        1,
        "a reference_link must match its label irrespective of case"
    );
}

/// Case 7 — the first definition wins.
///
/// **Corrected at S3, and the correction is S0 inheriting a premise M2.md §5 D3
/// had already withdrawn.** This case used to assert a second half that the
/// TypeScript original does not have: that *both* lines survive a round trip,
/// "which is the half `RefDefs` cannot deliver at all". Asked of the running
/// engine, muya emits **one** paragraph for these two lines and drops the
/// second — `marked`'s `def` branch pushes a token only for a label it has not
/// already seen — so the assertion asserted the opposite of the reference.
///
/// D3's correction records exactly this and the port reproduces it
/// (`block::scan_definitions`); what S0 transcribed was D3's *original* text.
/// The lesson is the milestone's recurring one and this is its eighth
/// instance: an assertion nothing had run yet is not evidence, and the
/// transcription is the place it hid.
///
/// What survives is the case the original makes, and D3's actual reason for
/// scanning rather than reading `RefDefs`: the *first* definition is the live
/// one, which `RefDefs` also happens to say — its real failure is that a
/// definition's tree position and its document order are unrecoverable from a
/// map keyed by case-folded label.
fn a_duplicate_label_keeps_the_first_definition() {
    let doc = parse(
        "foo [bar][dup]\n\n[dup]: https://first.example\n[dup]: https://second.example\n",
        NO_EXT,
    );
    let labels = collect_labels(&doc);
    assert_eq!(
        labels.get("dup").map(|l| l.href.as_str()),
        Some("https://first.example")
    );

    // And the reason it is the first: the second line never became a block, so
    // there was no second paragraph for the label pass to overwrite from.
    assert_eq!(paragraph_texts(&doc).len(), 2);
}

/// Case 8. No definition, no token — the brackets stay literal text.
fn an_orphan_reference_link_stays_plain_text() {
    let doc = parse("Look at [missing][nope] please.\n", NO_EXT);
    let labels = collect_labels(&doc);
    assert!(labels.is_empty());

    let paragraph = paragraph_texts(&doc)
        .into_iter()
        .next()
        .expect("the prose paragraph");
    let tokens = tokenize_with(&paragraph, labels);
    assert!(
        reference_links(&tokens).is_empty(),
        "no reference_link should fire without a matching definition"
    );
    let raw: String = tokens.iter().map(|t| t.raw.of(&paragraph)).collect();
    assert!(raw.contains("[missing][nope]"), "{raw:?}");
}

}
