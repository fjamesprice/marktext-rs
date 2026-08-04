//! `gitlabMathToggle.spec.ts` — 4 cases, transcribed as their expressible half.
//!
//! # What the original asserts, and what is transcribable here
//!
//! Each case boots a `Muya` instance, calls
//! `muya.setOptions({ isGitlabCompatibilityEnabled }, true)`, and asserts that
//! the already-loaded document's first block changes kind. The reason it can
//! is the point of the spec: GitLab compatibility is a **parse-time** option
//! (`walkTokens` decides whether ```` ```math ```` becomes a math block), so a
//! render-only rebuild from the already-parsed state cannot reclassify an
//! existing fence — the toggle has to re-parse.
//!
//! The re-parse *trigger* is `editor.setOptions`, which is M4's. What is M2's
//! is the claim that makes the trigger necessary: **the same source under the
//! two option values yields two different block kinds, in both directions.**
//! That is `mt_md::parse` called twice, and it is what these transcribe.
//!
//! **Owed to M4:** that `setOptions(..., true)` re-parses rather than
//! re-rendering. Recorded in M2.md §10.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "gitlab_math_toggle",

fn starts_as_a_code_block_when_gitlab_compatibility_is_off() {
    let doc = parse(
        "```math\nx^2\n```\n",
        Options { math: true, gitlab_compatibility: false, ..NO_EXT },
    );
    assert_eq!(name(&doc, top(&doc)[0]), "code-block");
}

/// Off → on. The same source becomes a gitlab-styled math block.
fn promotes_a_math_fence_to_a_math_block_when_the_option_is_on() {
    let source = "```math\nx^2\n```\n";
    let before = parse(source, Options { math: true, gitlab_compatibility: false, ..NO_EXT });
    assert_eq!(name(&before, top(&before)[0]), "code-block");

    let after = parse(source, Options { math: true, gitlab_compatibility: true, ..NO_EXT });
    let block = top(&after)[0];
    assert_eq!(name(&after, block), "math-block");
    assert_eq!(meta(&after, block), BlockMeta::MathBlock { style: MathStyle::Gitlab });
}

/// On → off, which is the direction a render-only rebuild fails at most
/// obviously: the block has to become a `code-block` whose `meta.lang` is
/// `math` again.
fn demotes_a_math_fence_back_to_a_code_block_when_the_option_is_off() {
    let source = "```math\nx^2\n```\n";
    let before = parse(source, Options { math: true, gitlab_compatibility: true, ..NO_EXT });
    assert_eq!(name(&before, top(&before)[0]), "math-block");

    let after = parse(source, Options { math: true, gitlab_compatibility: false, ..NO_EXT });
    let block = top(&after)[0];
    assert_eq!(name(&after, block), "code-block");
    assert_eq!(
        meta(&after, block),
        BlockMeta::CodeBlock { kind: CodeKind::Fenced, info: "math".to_string(), fence_len: None }
    );
}

/// `$$` is not gitlab syntax, so the toggle must not touch it in either
/// direction — `MathStyle::Default` both times.
fn leaves_dollar_dollar_math_blocks_untouched_across_a_toggle() {
    for gitlab in [false, true] {
        let doc = parse(
            "$$\nx^2\n$$\n",
            Options { math: true, gitlab_compatibility: gitlab, ..NO_EXT },
        );
        let block = top(&doc)[0];
        assert_eq!(name(&doc, block), "math-block", "gitlab = {gitlab}");
        assert_eq!(
            meta(&doc, block),
            BlockMeta::MathBlock { style: MathStyle::Default },
            "gitlab = {gitlab}"
        );
    }
}

}
