//! # `mt-highlight` — syntax highlighting for fenced code
//!
//! ## Contract
//!
//! Given a code block's text and a language, produce highlight spans as byte
//! ranges into that text. Built on `tree-sitter` plus grammars; replaces
//! `prismjs` (§8). Chosen over `syntect` for its incremental behaviour: an
//! edit inside a fence re-parses the edited region, not the fence.
//!
//! Spans are byte ranges, never styled strings — `mt-layout` turns them into
//! parley style ranges and `mt-render` into colours. This crate has no opinion
//! about colour; the theme does.
//!
//! ## The language is the first word of the info string
//!
//! `mt_doc::Block::CodeBlock::info` holds the **full verbatim info string**
//! (`js title="x"`, or a Pandoc `{…}` block). Highlighting uses its first
//! word, via `Block::highlight_language()`. Never take the whole info string
//! as a language name, and never mutate `info` to make lookup easier — it
//! round-trips verbatim.
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU. No dependency on `mt-ui` or `mt-app`.**
//! - Grammar loading is the one legitimate I/O this crate may perform, and
//!   only if §12.2's "load tree-sitter grammars on demand" lever is taken —
//!   that lever is worth −3–4 MB of binary and is decided at M3.
//!
//! ## M0 status
//!
//! Stub. `mt-highlight` is M3 (§9).
