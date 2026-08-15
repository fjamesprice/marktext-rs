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
//! **D3's central claim was false as written, and D14 is what makes it true.**
//! *"Prism grammars are data"* is not true of the grammar **source**: 91 of the
//! 297 files build regexes at load time and 130 call `extend`/`insertBefore`, so
//! a third of the corpus is a program. What rescues the decision is that all of
//! that runs at load time and none of it survives — after requiring all 297 the
//! reachable registry holds **no** function-valued leaves. So the port copies
//! the *output* of Prism's load. See `xtask/src/grammars.rs`.
//!
//! **The credit D3 took, now measured.** Prism's steady-state throughput in Rust
//! was unmeasured because no port existed. §8 M3-R9's fallback trigger is
//! *"if the ported engine highlights a visible fence in more than 16 ms — one
//! frame at 60 fps — `syntect` takes the top-40 languages"*, and on a release
//! build:
//!
//! | | |
//! |---|---:|
//! | all 2,504 corpus fences, warm | **80 ms** total |
//! | median fence | **0.029 ms** |
//! | slowest fence | **3.28 ms** (and that one pays a first-use compile) |
//! | first highlight in a language, incl. compiling its patterns | 0.5 – **19.5 ms** (`sql`, `css`) |
//! | whole source files, warm | **1.4 MB/s**, median 2.3 ms |
//!
//! **The trigger has not fired**, and the number that comes closest is not a
//! fence at all: `sql` and `css` cost ~19 ms the *first* time they are used,
//! because each is one enormous keyword alternation. That is why the regex cache
//! is lazy per pattern rather than per language.
//!
//! Two things remain on credit. `fancy-regex` backtracks, so throughput has no
//! upper bound: the worst input in a 307-file sweep was 176 ms for 14.5 KB of
//! machine-generated JavaScript — 20× below the average rate. And a pattern that
//! blows up entirely degrades to *"that rule did not match"* rather than to a
//! hang (`interpreter::match_from`), which is containment, not a bound.
//!
//! ## Scope: a ratchet, not a big bang
//!
//! MarkText lazily loads all 297 grammars and eagerly loads **two**. D3: *"port
//! languages in usage order, and let `cargo xtask highlight` count coverage as a
//! number that only goes up."* The list of ported languages lives in exactly one
//! place — `xtask/src/highlight.rs`'s `PORTED` — and it drives **both** the
//! differential and `cargo xtask grammars`, so a language is in this crate's
//! data if and only if a differential over every corpus fence in it agreed with
//! Prism span for span. Everything reachable from those languages through
//! `inside` comes with them, because `markup` cannot be highlighted without
//! `css` and `javascript`.
//!
//! It is **297** languages, not the 298 keys in Prism's manifest: the 298th is
//! `meta`, which is not a language. [`MANIFEST`] carries all 297 regardless of
//! what is ported, because [`resolve_language`] has to answer for the whole set.
//!
//! ## Where this port is known to disagree with Prism
//!
//! S3's gate is **exact span equality against Prism, per fence**, and it is
//! green over all 2,507 corpus fences. A wider sweep — 307 real source files,
//! 407,114 Prism spans — found **two** disagreements, and they are one cause,
//! recorded here rather than left for a user to find:
//!
//! **A non-`u` ECMAScript regex matches UTF-16 code units; `regex` matches
//! Unicode scalar values.** `/[^']/` consumes half of `😀` in a browser and all
//! of it here. It shows only where a pattern needs *exactly one* code unit —
//! under `*` or `+` the engines agree, so an emoji inside a string literal
//! tokenizes identically — and in practice that is `rust`'s `char` rule: `'😀'`
//! and `'𝄞'` are `char` tokens here and plain text in MarkText. Rust's `str`
//! cannot hold half a character, so there is no translation that closes it; the
//! near-miss fixes are worse and `xtask/src/grammars.rs`'s `translate` says why.
//!
//! Two smaller ones with no measured effect, kept on the record because *"no
//! measured effect"* is not *"no effect"*: `(?i)` here is Unicode case folding,
//! so U+212A (KELVIN SIGN) matches `k` and ECMAScript's `Canonicalize` would not;
//! and `(?m)` treats only `\n` as a line boundary, where ECMAScript also counts
//! CR, U+2028 and U+2029.
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
//!   rather than by engineering. Grammars are compiled in as data, and D15
//!   gives the second reason this is not merely a preference: `mt-layout` is
//!   headless, it is the consumer, and a crate that reached the filesystem on
//!   its behalf would make that guarantee depend on which code path ran.
//!
//! ## Status
//!
//! **M3 S3, landed.** The interpreter is a transcription of Prism's
//! `tokenize`/`matchGrammar`; the grammars are generated by `cargo xtask
//! grammars`; `cargo xtask highlight --require-ts` is the gate. D15's seam into
//! `mt-layout` is a **later** commit of the same stage and nothing here reaches
//! for it: this crate still produces spans and nothing else.

mod generated;
mod grammar;
mod interpreter;

pub use generated::MANIFEST;

/// One highlight run: a byte range into the code, and the class it paints in.
///
/// **Byte ranges into the block's text, not into anything else.** D15 notes that
/// a code block's visible-text map is `VisibleTextMap::identity(text.len())`, so
/// these are already offsets into the text `mt-layout` shapes and no conversion
/// happens at that seam — the third offset space §4 C6 warns about does not open
/// here.
///
/// `class` is a Prism token class (`keyword`, `string`, `comment`, …), resolved
/// **alias over type, last alias wins** per D15 — see `grammar::Pattern::class`.
/// It is `&'static str` because it always names a class the generator emitted,
/// which is what lets `mt_layout::CodePalette::by_class` take it without an
/// allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub class: &'static str,
}

/// A fence's language word → the Prism grammar id it names.
///
/// Reproduces MarkText's `transformAliasToOrigin`
/// (`packages/muya/src/utils/prism/loadLanguage.ts:23-54`), which muya calls
/// before rendering (`codeBlockContent/index.ts:170`) and again before
/// tokenizing (`:423`). Three cases, and **the order is the contract**:
///
/// 1. a word that is a `components.json` id is itself — even if some *other*
///    language claims it as an alias;
/// 2. otherwise the **first** language whose `alias` list contains it, in
///    `components.json` key order;
/// 3. otherwise nothing, which in MarkText is the `noexist` path and here is
///    `None`.
///
/// `ts` answers `typescript`; `c++` answers `cpp`, which is muya's own patch
/// (`utils/prism/index.ts:16-24`) and not stock Prism. A resolved id is **not**
/// a promise that the grammar is ported — [`highlight`] is where that is
/// decided, and the two questions are kept apart because the coverage ratchet
/// counts grammars while the alias table covers all 297.
pub fn resolve_language(name: &str) -> Option<&'static str> {
    if let Ok(index) = MANIFEST.binary_search(&name) {
        return Some(MANIFEST[index]);
    }
    generated::ALIASES
        .binary_search_by_key(&name, |(alias, _)| alias)
        .ok()
        .map(|index| generated::ALIASES[index].1)
}

/// Every language this build can highlight, sorted.
///
/// The coverage ratchet's numerator, as data. `cargo xtask highlight` counts
/// `PORTED`, and `cargo xtask grammars` generates from the same list, so this
/// and that list are the same set by construction — asserted by
/// `xtask::highlight`'s `ported_languages_have_an_interpreter`.
pub fn languages() -> impl ExactSizeIterator<Item = &'static str> {
    generated::LANGUAGES.iter().map(|(name, _)| *name)
}

/// Highlight `code` as `language`.
///
/// `None` means *"no ported grammar for that name"* — an unknown word, or a
/// Prism language this build does not carry — and the caller shows the fence
/// unhighlighted. An empty `Vec` is a different answer: the language is ported
/// and nothing in this fence matched a rule. **The two must not be conflated**;
/// `xtask/src/highlight.rs` counts the first against coverage and compares the
/// second against Prism.
///
/// Spans are ascending, non-overlapping and never zero-width. Adjacent spans of
/// the *same* class are **not** merged — see `interpreter::flatten` for why that
/// is a deliberate refusal rather than an omission.
pub fn highlight(language: &str, code: &str) -> Option<Vec<Span>> {
    let resolved = resolve_language(language)?;
    let index = generated::LANGUAGES
        .binary_search_by_key(&resolved, |(name, _)| name)
        .ok()?;
    let grammar = generated::LANGUAGES[index].1 as usize;
    let items = interpreter::tokenize(code, 0, grammar);
    let mut spans = Vec::new();
    interpreter::flatten(&items, None, &mut spans);
    Some(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three cases of `transformAliasToOrigin`, in its own order.
    #[test]
    fn a_language_word_resolves_the_way_marktext_resolves_it() {
        assert_eq!(resolve_language("typescript"), Some("typescript"));
        assert_eq!(resolve_language("ts"), Some("typescript"));
        assert_eq!(resolve_language("js"), Some("javascript"));
        assert_eq!(resolve_language("html"), Some("markup"));
        // muya's patch, not stock Prism.
        assert_eq!(resolve_language("c++"), Some("cpp"));
        assert_eq!(resolve_language(""), None);
        assert_eq!(resolve_language("not-a-language"), None);
        // `plaintext` is registered by prism-core as a shared empty grammar and
        // is in no manifest; MarkText's own condition is manifest membership, so
        // a ```txt fence is unhighlighted there and unresolved here.
        assert_eq!(resolve_language("txt"), None);
    }

    /// The two tables `resolve_language` binary-searches must be sorted.
    #[test]
    fn the_generated_tables_are_sorted() {
        assert!(MANIFEST.windows(2).all(|w| w[0] < w[1]));
        assert!(generated::ALIASES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(generated::LANGUAGES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(MANIFEST.len(), 297, "docs/M3.md §5 D14's denominator");
    }

    /// A ported language highlights; an unported one is `None`, not `Some([])`.
    #[test]
    fn not_ported_and_nothing_matched_are_different_answers() {
        let spans = highlight("rust", "let x = 1;").expect("rust is ported");
        assert!(!spans.is_empty());
        assert!(highlight("brainfuck", "+++").is_none());
        // Ported, and genuinely nothing to highlight.
        assert_eq!(highlight("rust", "").map(|s| s.len()), Some(0));
    }

    /// D15's two contracts on the run list, over every ported language.
    #[test]
    fn spans_are_ascending_non_overlapping_and_never_empty() {
        let code = "fn main() { let s = \"a\\\"b\"; // c\n }\n<a href='x'>&amp;</a>\n";
        for language in languages() {
            let spans = highlight(language, code).expect("a listed language is ported");
            let mut last = 0;
            for span in &spans {
                assert!(span.start < span.end, "{language}: empty span {span:?}");
                assert!(span.start >= last, "{language}: out of order {span:?}");
                assert!(code.is_char_boundary(span.start), "{language}: {span:?}");
                assert!(code.is_char_boundary(span.end), "{language}: {span:?}");
                last = span.end;
            }
        }
    }

    /// Prism's `lookbehind` is a post-hoc trim of group 1, not a regex feature.
    ///
    /// The clearest case in the ported set is `javascript.class-name`, whose
    /// pattern is `` /(\b(?:class|extends|…|new)\s+)[\w.\\]+/ `` with
    /// `lookbehind: true`. Group 1 captures `class ` and Prism then moves the
    /// match start past it (prism-core.js:893-898), so the emitted token is
    /// **`Foo` and not `class Foo`**. A port that read `lookbehind` as `(?<=…)`,
    /// or that ignored it, would produce a span six bytes too wide here and the
    /// differential would say so — but a unit test says *why*.
    #[test]
    fn a_lookbehind_prefix_is_not_part_of_the_span() {
        let code = "class Foo {}";
        let spans = highlight("javascript", code).expect("javascript is ported");
        let class_name = spans
            .iter()
            .find(|s| s.class == "class-name")
            .expect("`class Foo` produces a class-name token");
        assert_eq!(&code[class_name.start..class_name.end], "Foo");
    }
}
