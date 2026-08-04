//! `gitlabMath.spec.ts` — 16 cases.
//!
//! GitLab-flavoured Markdown lets a ```` ```math ```` fence render as block
//! math. The promotion is split across two seams and the split is the point:
//!
//! - **parse** — `utils/marked/walkTokens.ts` rewrites a `code`/`lang=math`
//!   token into `multiplemath` with `mathStyle: 'gitlab'`, but only when
//!   **both** `math` and `isGitlabCompatibilityEnabled` are true.
//! - **serialize** — `_serializeMathBlock` picks the fence purely from
//!   `meta.mathStyle`. It does **not** re-read the option, so a block keeps
//!   the style it was born with.
//!
//! The last describe block characterises where `@muyajs/core` agrees with the
//! legacy `muyajs` engine (whose `multiplemathGitlab` regex was backtick-only)
//! and where it does not.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "gitlab_math",

fn promotes_a_math_fence_to_a_gitlab_styled_math_block() {
    let doc = parse("```math\nx^2\n```\n", GITLAB_MATH);
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "math-block");
    assert_eq!(meta(&doc, block), BlockMeta::MathBlock { style: MathStyle::Gitlab });
    assert_eq!(text(&doc, block), "x^2");
}

fn leaves_a_math_fence_as_a_code_block_when_gitlab_compatibility_is_off() {
    let doc = parse(
        "```math\nx^2\n```\n",
        Options { gitlab_compatibility: false, ..GITLAB_MATH },
    );
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "code-block");
    assert_eq!(
        meta(&doc, block),
        BlockMeta::CodeBlock { kind: CodeKind::Fenced, info: "math".to_string(), fence_len: None }
    );
}

/// Both flags are required — `math: false` alone is enough to leave it a code
/// block.
fn leaves_a_math_fence_as_a_code_block_when_math_is_off() {
    let doc = parse("```math\nx^2\n```\n", Options { math: false, ..GITLAB_MATH });
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "code-block");
    assert_eq!(
        meta(&doc, block),
        BlockMeta::CodeBlock { kind: CodeKind::Fenced, info: "math".to_string(), fence_len: None }
    );
}

/// `MathStyle::Default` serializes to the empty string, not `"default"` —
/// M0's note on the enum.
fn always_parses_dollar_dollar_as_a_non_gitlab_math_block() {
    for gitlab in [true, false] {
        let doc = parse(
            "$$\nx^2\n$$\n",
            Options { gitlab_compatibility: gitlab, ..GITLAB_MATH },
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

fn serializes_a_gitlab_styled_math_block_back_to_a_math_fence() {
    let doc = parse("```math\nx^2\n```\n", GITLAB_MATH);
    assert_eq!(serialize(&doc, GITLAB_MATH), "```math\nx^2\n```\n");
}

fn serializes_a_dollar_dollar_math_block_back_to_dollar_dollar() {
    let doc = parse("$$\nx^2\n$$\n", GITLAB_MATH);
    assert_eq!(serialize(&doc, GITLAB_MATH), "$$\nx^2\n$$\n");
}

/// The fence choice rides on the stored style, never the runtime flag: a block
/// parsed from `$$` and then *given* the gitlab style serializes as
/// ```` ```math ````.
fn keys_the_fence_purely_on_meta_math_style_not_on_the_option() {
    let mut doc = parse("$$\nx^2\n$$\n", GITLAB_MATH);
    let block = top(&doc)[0];
    doc.apply(&[Edit::SetMeta {
        node: block,
        meta: BlockMeta::MathBlock { style: MathStyle::Gitlab },
    }]);
    assert_eq!(serialize(&doc, GITLAB_MATH), "```math\nx^2\n```\n");
}

fn preserves_indentation_when_a_gitlab_math_block_is_nested_in_a_list() {
    let md = "- item\n\n  ```math\n  x^2\n  ```\n";
    assert_eq!(round_trip(md, GITLAB_MATH), md);
}

fn round_trips_a_math_fence_unchanged_with_gitlab_compatibility_on() {
    let md = "```math\nx^2\n```\n";
    assert_eq!(round_trip(md, GITLAB_MATH), md);
}

fn round_trips_dollar_dollar_unchanged_regardless_of_the_flag() {
    let md = "$$\nx^2\n$$\n";
    assert_eq!(
        round_trip(md, Options { gitlab_compatibility: false, ..GITLAB_MATH }),
        md
    );
}

/// Agrees with legacy muyajs: ≤ 3 leading spaces still promotes.
fn a_three_space_indented_math_fence_is_still_promoted() {
    let doc = parse("   ```math\nx^2\n```\n", GITLAB_MATH);
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "math-block");
    assert_eq!(meta(&doc, block), BlockMeta::MathBlock { style: MathStyle::Gitlab });
}

/// Agrees with legacy muyajs: four spaces makes it an indented code block.
fn a_four_space_indented_fence_is_an_indented_code_block_not_math() {
    let doc = parse("    ```math\nx^2\n```\n", GITLAB_MATH);
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "code-block");
    assert!(
        matches!(meta(&doc, block), BlockMeta::CodeBlock { kind: CodeKind::Indented, .. }),
        "expected an indented code block, got {:?}",
        meta(&doc, block)
    );
}

/// Agrees with legacy muyajs: a longer fence still promotes.
fn a_four_backtick_math_fence_is_promoted() {
    let doc = parse("````math\nx^2\n````\n", GITLAB_MATH);
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "math-block");
    assert_eq!(meta(&doc, block), BlockMeta::MathBlock { style: MathStyle::Gitlab });
}

/// Agrees with legacy muyajs: the language must be exactly `math`.
fn an_info_string_after_math_is_not_promoted() {
    let doc = parse("```math foo\nx^2\n```\n", GITLAB_MATH);
    assert_eq!(name(&doc, top(&doc)[0]), "code-block");
}

/// Agrees with legacy muyajs, and note the info string is stored whole and
/// **un-lowercased** — M0's constraint 1.
fn the_math_language_tag_is_case_sensitive() {
    let doc = parse("```MATH\nx^2\n```\n", GITLAB_MATH);
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "code-block");
    assert_eq!(
        meta(&doc, block),
        BlockMeta::CodeBlock { kind: CodeKind::Fenced, info: "MATH".to_string(), fence_len: None }
    );
}

/// **Diverges from legacy muyajs.** `@muyajs/core` keys on marked's generic
/// fenced-code parser, which accepts `~~~` as well as ```` ``` ````, so the
/// math tag promotes either way and the block re-serializes with a backtick
/// fence. The legacy `multiplemathGitlab` regex was backtick-only, so the same
/// source stayed a plain code block there.
fn a_tilde_math_fence_is_promoted_by_muya_unlike_muyajs() {
    let doc = parse("~~~math\nx^2\n~~~\n", GITLAB_MATH);
    let block = top(&doc)[0];
    assert_eq!(name(&doc, block), "math-block");
    assert_eq!(meta(&doc, block), BlockMeta::MathBlock { style: MathStyle::Gitlab });
    assert_eq!(round_trip("~~~math\nx^2\n~~~\n", GITLAB_MATH), "```math\nx^2\n```\n");
}

}
