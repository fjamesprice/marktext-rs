//! `codeFenceLength.spec.ts` — 3 cases.
//!
//! #1841, and M0's constraint 3. A fence longer than three backticks — needed
//! when the block's own content contains a ```` ``` ```` line — was always
//! re-serialized with exactly three, so the first inner ```` ``` ````
//! prematurely closed the block and the saved markdown was corrupt.
//!
//! `meta.fenceLength` is **omitted, not null**, when the fence is three
//! backticks, which is why `CodeBlock::fence_len` is `Option<u8>` and why the
//! third case here is a distinct assertion rather than a special case of the
//! first two.

#![allow(unused_imports)]
use crate::*;

/// The opening fence of a serialized document.
fn opening_fence(md: &str) -> &str {
    md.split('\n').next().unwrap_or("")
}

spec_cases! { "code_fence_length",

fn keeps_a_fence_long_enough_to_wrap_content_containing_a_triple_backtick() {
    let input = "````\n```\nfoo\n```\n````\n";
    let md = round_trip(input, NO_EXT);

    let fence = super::opening_fence(&md);
    assert!(
        fence.len() >= 4 && fence.chars().all(|c| c == '`'),
        "the opening fence must be at least four backticks, got {fence:?}"
    );

    // Re-parsing must still yield one code block whose body keeps the inner
    // fence — i.e. the round trip did not corrupt the document.
    let reparsed = parse(&md, NO_EXT);
    let blocks: Vec<_> = top(&reparsed)
        .into_iter()
        .filter(|id| name(&reparsed, *id) == "code-block")
        .collect();
    assert_eq!(blocks.len(), 1);
    assert!(text(&reparsed, blocks[0]).contains("```"), "the inner fence survived");
}

fn round_trips_a_long_fenced_block_byte_stably() {
    let input = "````js\n```\nconst a = 1\n```\n````\n";
    let once = round_trip(input, NO_EXT);
    let twice = round_trip(&once, NO_EXT);
    assert_eq!(twice, once);
}

/// The other direction: an ordinary block must not grow a longer fence, and
/// `fence_len` stays `None` rather than becoming `Some(3)`.
fn still_uses_a_plain_three_backtick_fence_for_ordinary_blocks() {
    let md = round_trip("```js\nconst a = 1\n```\n", NO_EXT);
    assert_eq!(super::opening_fence(&md), "```js");

    let doc = parse("```js\nconst a = 1\n```\n", NO_EXT);
    assert_eq!(
        meta(&doc, top(&doc)[0]),
        BlockMeta::CodeBlock { kind: CodeKind::Fenced, info: "js".to_string(), fence_len: None }
    );
}

}
