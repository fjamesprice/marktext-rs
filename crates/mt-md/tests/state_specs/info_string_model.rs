//! `infoStringModel.spec.ts` — 3 cases.
//!
//! M0's constraint 1 stated directly against the model rather than through a
//! round trip: `meta.lang` holds the **whole** info string.
//! `packages/muya/src/state/types.ts:34` is explicit that you must never
//! assume a single word.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "info_string_model",

fn keeps_a_language_plus_attributes_verbatim() {
    let doc = parse("```js title=\"app.js\"\nx\n```\n", MUYA_DEFAULT);
    let block = top_named(&doc, "code-block").expect("a code block");
    assert_eq!(
        meta(&doc, block),
        BlockMeta::CodeBlock {
            kind: CodeKind::Fenced,
            info: "js title=\"app.js\"".to_string(),
            fence_len: None,
        }
    );
}

fn keeps_a_pandoc_attribute_block_verbatim() {
    let doc = parse("```{example, listing1-name}\nx\n```\n", MUYA_DEFAULT);
    let block = top_named(&doc, "code-block").expect("a code block");
    assert_eq!(
        meta(&doc, block),
        BlockMeta::CodeBlock {
            kind: CodeKind::Fenced,
            info: "{example, listing1-name}".to_string(),
            fence_len: None,
        }
    );
}

fn stores_a_plain_language_as_is() {
    let doc = parse("```js\nx\n```\n", MUYA_DEFAULT);
    let block = top_named(&doc, "code-block").expect("a code block");
    assert_eq!(
        meta(&doc, block),
        BlockMeta::CodeBlock {
            kind: CodeKind::Fenced,
            info: "js".to_string(),
            fence_len: None,
        }
    );
}

}
