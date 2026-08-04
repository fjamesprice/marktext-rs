//! `diagramFlowchartSequence.spec.ts` — 5 of its 6 cases.
//!
//! Parity restoration for flowchart + sequence diagrams. The legacy `muyajs`
//! engine rendered ```` ```flowchart ```` (flowchart.js) and
//! ```` ```sequence ```` (js-sequence-diagrams) as diagram blocks; the TS
//! rewrite dropped them. These pin the parse and round-trip so both stay
//! first-class beside mermaid / plantuml / vega-lite.
//!
//! **The sixth case is out of scope.** It asserts
//! `MUYA_DEFAULT_OPTIONS.sequenceTheme === 'hand'` — a rendering theme for
//! js-sequence-diagrams, carried in `config.ts` and consumed by the diagram
//! renderer. `mt_md::Options` mirrors `MarkdownToState`'s and
//! `renderToStaticHTML`'s option surfaces and has no counterpart for it;
//! diagram theming is `mt-diagram`'s, at M3.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "diagram_flowchart_sequence",

/// `DiagramKind::Flowchart`. `meta.lang` stays `yaml` because only vega-lite
/// uses `json`.
fn parses_a_flowchart_fence_as_a_diagram_block() {
    let doc = parse("```flowchart\nst=>start: Start\ne=>end: End\nst->e\n```\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "diagram");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::Diagram { lang: DiagramLang::Yaml, kind: DiagramKind::Flowchart }
    );
    assert!(text(&doc, states[0]).contains("st=>start: Start"));
}

fn parses_a_sequence_fence_as_a_diagram_block() {
    let doc = parse("```sequence\nAlice->Bob: Hello Bob\nBob-->Alice: Hi Alice\n```\n", NO_EXT);
    let states = top(&doc);
    assert_eq!(states.len(), 1);
    assert_eq!(name(&doc, states[0]), "diagram");
    assert_eq!(
        meta(&doc, states[0]),
        BlockMeta::Diagram { lang: DiagramLang::Yaml, kind: DiagramKind::Sequence }
    );
    assert!(text(&doc, states[0]).contains("Alice->Bob: Hello Bob"));
}

fn round_trips_a_flowchart_diagram_block() {
    let out = round_trip("```flowchart\nst=>start: Start\ne=>end: End\nst->e\n```\n", NO_EXT);
    assert!(out.contains("```flowchart"), "{out:?}");
    assert!(out.contains("st=>start: Start"), "{out:?}");
    assert!(out.contains("st->e"), "{out:?}");
}

fn round_trips_a_sequence_diagram_block() {
    let out = round_trip("```sequence\nAlice->Bob: Hello Bob\nBob-->Alice: Hi Alice\n```\n", NO_EXT);
    assert!(out.contains("```sequence"), "{out:?}");
    assert!(out.contains("Alice->Bob: Hello Bob"), "{out:?}");
    assert!(out.contains("Bob-->Alice: Hi Alice"), "{out:?}");
}

/// No regression in the three that already worked. `VegaLite` is retained even
/// though §10 drops Vega *rendering* in v1 — the block must still round-trip
/// losslessly.
fn still_parses_mermaid_plantuml_and_vega_lite() {
    for (info, kind) in [
        ("mermaid", DiagramKind::Mermaid),
        ("plantuml", DiagramKind::PlantUml),
        ("vega-lite", DiagramKind::VegaLite),
    ] {
        let doc = parse(&format!("```{info}\nfoo\n```\n"), NO_EXT);
        let block = top(&doc)[0];
        assert_eq!(name(&doc, block), "diagram", "{info}");
        assert!(
            matches!(meta(&doc, block), BlockMeta::Diagram { kind: k, .. } if k == kind),
            "{info}: got {:?}",
            meta(&doc, block)
        );
    }
}

}
