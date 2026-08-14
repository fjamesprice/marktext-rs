//! # `mt-highlight` — syntax highlighting for fenced code
//!
//! ## Contract
//!
//! Given a code block's text and a language, produce highlight spans as byte
//! ranges into that text.
//!
//! Spans are byte ranges, never styled strings — `mt-layout` turns them into
//! parley style ranges and `mt-render` into colours. This crate has no opinion
//! about colour; the theme does, through the code palette `mt_layout::Theme`
//! names by value.
//!
//! ## The engine is a Prism grammar port. **The M0 decision is overturned**
//!
//! This crate's stub said tree-sitter, *"chosen over `syntect` for its
//! incremental behaviour: an edit inside a fence re-parses the edited region,
//! not the fence."* **M3 §5 D3 overturns that at S0, with numbers**, and the
//! rationale is retired rather than merely bypassed: M3 is a read-only
//! previewer, so there are no edits inside a fence. **The benefit is M4's and
//! every cost is M3's.**
//!
//! **Decision: port Prism's grammars as data. `tree-sitter` is dropped from M3
//! and does not enter the dependency graph. `syntect` is retained as a named
//! fallback with a measured trigger.**
//!
//! Three axes decided it and none is close:
//!
//! - **Coverage.** `syntect`'s default bundle has no TypeScript and no TOML —
//!   a defect users hit on day one — and covers a measured 44 of Prism's 297.
//!   tree-sitter reaches parity only by linking grammars at **1.301 MB each**;
//!   twenty cost **+25.9 MB** measured, so Prism parity would be roughly
//!   386 MB against a plan whose entire budget was 17–25 MB. Prism grammars
//!   are **data**, so all 297 are reachable and lazy loading is free rather
//!   than being the dynamic-library subsystem §4 C10 identifies.
//! - **Cold start.** Time to first highlight: `syntect` ≈ 2.5 ms, tree-sitter
//!   15.3 ms for one lazily-built grammar and **80.9 ms for fourteen eager
//!   ones** — a third of the 250 ms cold-start budget spent before a glyph is
//!   drawn. Query compilation is the whole cost.
//! - **Differential testability, which decides it.** §7's finding is that for
//!   the first time in this project *"does it match MarkText?" cannot be
//!   answered by running MarkText* — and this crate is the **one place** M1
//!   and M2's strongest instrument survives. A port restates Prism's own
//!   rules, so span boundaries compare for **exact equality** through the
//!   existing Node harness. `syntect` and tree-sitter tokenize genuinely
//!   differently and would disagree with Prism constantly while both are
//!   "right", which does not weaken S3's gate so much as delete it.
//!
//! **What this buys on credit, stated plainly:** Prism's steady-state
//! throughput in Rust is **unmeasured**, because no port exists to measure.
//! ~73 % of the grammars need lookahead and therefore `fancy-regex` (already a
//! dependency of `mt-inline`), whose backend `syntect`'s own documentation
//! puts at *"about half the speed"* of the alternative. **The fallback trigger
//! is written down before the work starts so it cannot be argued about later:
//! if the ported engine highlights a visible fence in more than 16 ms — one
//! frame at 60 fps — `syntect` takes the top-40 languages and the port
//! continues behind it** (§8 M3-R9).
//!
//! **Scope, because 297 grammars sounds like a milestone and is not.**
//! MarkText lazily loads all 297 and eagerly loads **two**. The port is a
//! ratchet, not a big bang: implement the grammar interpreter once, port
//! languages in usage order, and let `cargo xtask highlight` count coverage as
//! a number that only goes up — structurally the same instrument as M2's
//! `PENDING` ratchet. **S3's gate is the interpreter plus a counted,
//! differentially-verified subset.**
//!
//! It is **297** languages, not the 298 keys in Prism's manifest: the 298th is
//! `meta`, which is not a language.
//!
//! ## The language is the first word of the info string
//!
//! `mt_doc::Block::CodeBlock::info` holds the **full verbatim info string**
//! (`js title="x"`, or a Pandoc `{…}` block). Highlighting uses its first
//! word, via `Block::highlight_language()`. Never take the whole info string
//! as a language name, and never mutate `info` to make lookup easier — it
//! round-trips verbatim.
//!
//! `highlight_language()` returns `None` for **every** non-`CodeBlock`
//! variant, enforced by a test, and that is correct: a math block has no info
//! string. M3 §5 D11 puts the block → language mapping in `mt-layout`
//! instead, where a `Diagram` resolves through `mt_doc::DiagramKind` and a
//! `MathBlock` takes `latex`. **A language with no ported grammar degrades to
//! unhighlighted source**, which is S3's coverage number rather than a defect.
//!
//! ## Dependency constraints
//!
//! - **No windowing. No GPU. No dependency on `mt-ui` or `mt-app`.**
//! - **No grammar I/O.** The stub reserved filesystem access for §12.2's
//!   *"load tree-sitter grammars on demand"* lever, billed at −3–4 MB. That
//!   lever is gone twice over: S0 measured the real cost of bundling twenty
//!   grammars at **25.9 MB**, so the figure was understated by 6–8×, and D3's
//!   grammars-as-data answer obtains the same saving **by construction**
//!   rather than by engineering. Grammars are compiled in as data.
//!
//! ## Status
//!
//! Stub. This crate is **M3 S3**; S1 landed `mt-layout` around it.
