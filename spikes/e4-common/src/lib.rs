//! Shared corpus extraction for E4 (M3 S0, D3 — `mt-highlight`'s engine).
//!
//! `mt-highlight`'s contract (`crates/mt-highlight/src/lib.rs`) is: given a
//! code block's text and a language, produce spans. The language is
//! **`Block::highlight_language()`** — the first word of the info string,
//! never the whole string. This module drives the real parser
//! (`mt_md::parse`) and the real accessor so every fence handed to a
//! highlighter in this experiment is extracted the way `mt-highlight` will
//! actually receive it, not by a hand-rolled fence regex that could disagree
//! with the corpus's own syntax (a four-backtick fence containing a
//! three-backtick run, a title-bearing info string, etc.).

use mt_doc::Block;

/// One fenced or indented code block, as `mt-highlight` would see it.
pub struct CodeFence {
    /// `Block::highlight_language()` — `None` for an indented block or a
    /// fence with an empty info string.
    pub lang: Option<String>,
    pub text: String,
}

/// Parse `markdown` and collect every code block's (language, text) pair, in
/// document order.
///
/// Uses `mt_md::parse` + an iterative pre-order walk, the same shape E3's
/// `collect_texts` used (`spikes/e3-layout-main/src/main.rs`) — iterative
/// because `MAX_NESTING_DEPTH` is 128 and a generated corpus can hit it.
pub fn code_fences(markdown: &str) -> Vec<CodeFence> {
    let parsed = mt_md::parse(markdown, mt_md::Options::MUYA_DEFAULT);
    let doc = &parsed.document;
    let mut out = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        if let Some(block) = doc.block(id)
            && let Block::CodeBlock { text, .. } = block
        {
            out.push(CodeFence {
                lang: block.highlight_language().map(str::to_owned),
                text: text.to_str().into_owned(),
            });
        }
        for &child in doc.children(id).iter().rev() {
            stack.push(child);
        }
    }
    out
}

/// Read a file to a `String`, panicking with the path on failure — every E4
/// binary needs this and it is not worth a crate for.
pub fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}
