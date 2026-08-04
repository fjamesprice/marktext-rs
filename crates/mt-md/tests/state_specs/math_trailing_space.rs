//! `mathTrailingSpace.spec.ts` — 3 cases.
//!
//! #1931. A display-math block whose closing `$$` carried trailing whitespace
//! was not recognised as math: the block regex required the closing marker to
//! be followed immediately by a newline or end-of-input, so any trailing space
//! made it fall through to plain text. Fenced code blocks already tolerated
//! trailing whitespace; math should too.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "math_trailing_space",

fn parses_a_math_block_whose_closing_fence_has_a_trailing_space() {
    let doc = parse("$$\nx = 1\n$$ \n\nbar\n", MATH_ONLY);
    assert!(
        top(&doc).iter().any(|id| name(&doc, *id) == "math-block"),
        "got {:?}",
        names(&doc, &top(&doc))
    );
}

fn parses_a_math_block_whose_closing_fence_has_a_trailing_tab() {
    let doc = parse("$$\nx = 1\n$$\t\n\nbar\n", MATH_ONLY);
    assert!(
        top(&doc).iter().any(|id| name(&doc, *id) == "math-block"),
        "got {:?}",
        names(&doc, &top(&doc))
    );
}

/// The positive control: the fix must not break the case that already worked.
fn still_parses_a_math_block_with_no_trailing_space() {
    let doc = parse("$$\nx = 1\n$$\n\nbar\n", MATH_ONLY);
    assert!(
        top(&doc).iter().any(|id| name(&doc, *id) == "math-block"),
        "got {:?}",
        names(&doc, &top(&doc))
    );
}

}
