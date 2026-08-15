//! The shape of a snapshotted Prism grammar, and the lazy regex cache.
//!
//! `generated.rs` is one long list of these three types and nothing else. The
//! flat-array-plus-`(first, count)` encoding is deliberate: a grammar written as
//! nested slice literals would compile to the same thing, but it would nest a
//! `&[&[…]]` fourteen levels deep in a file that a human is expected to review a
//! diff of, and it would give the regex cache below no index to key on.
//!
//! # What a Prism grammar is, in the two sentences that matter
//!
//! An **ordered** map from a token name to one or more patterns
//! (prism-core.js:916 iterates it with `for (var token in grammar)`, so the
//! order rules are tried in is the object's own key order, and it is what
//! decides which of two overlapping patterns wins). A pattern may carry an
//! `inside` grammar, which is tokenized recursively over the matched text — and
//! the `inside` graph has **cycles**, because `markup`'s `<script>` contains
//! `javascript` whose template strings contain `markup`, so `inside` is an index
//! rather than a nested value.

use std::sync::{LazyLock, OnceLock};

use fancy_regex::Regex;

use crate::generated::PATTERNS;

/// One `GrammarToken` — prism-core.js:1218-1232's four documented fields, plus
/// the class the span will carry.
pub(crate) struct Pattern {
    /// The pattern, already translated to `fancy-regex` syntax and already
    /// carrying its flags as a `(?im:…)` wrapper. `xtask/src/grammars.rs`'s
    /// `translate` is the only thing that writes this, and it compiles every
    /// one of them before emitting.
    pub(crate) source: &'static str,
    /// Prism's `lookbehind`, which is **not** a regex feature: when set, the
    /// text captured by group 1 is trimmed off the front of the match after the
    /// fact (prism-core.js:890-899). Prism uses it wherever a rule needs to see
    /// a preceding character without consuming it, which is most of the places
    /// another engine would use a real lookbehind.
    pub(crate) lookbehind: bool,
    /// Prism's `greedy`: match against the **whole remaining text** from the
    /// current position rather than against one string node, and be allowed to
    /// swallow tokens other rules have already produced.
    pub(crate) greedy: bool,
    /// The single class a matched span paints in.
    ///
    /// D15 fixes the precedence — *alias over type, last alias wins* — because
    /// Prism emits `class="token <type> <alias>…"` and muya's stylesheet
    /// resolves by source order. Resolved at generation time so that the
    /// interpreter has no opinion, and so that the one place the rule is
    /// written down is the same place `tools/diff/dump-prism-tokens.mjs` writes
    /// it.
    pub(crate) class: &'static str,
    /// Index into [`crate::generated::GRAMMARS`], tokenized over the matched
    /// text.
    pub(crate) inside: Option<u16>,
}

/// One named entry of a grammar: `patterns` is `PATTERNS[first..first + count]`.
///
/// `name` is **not** the span class — `class` on each pattern is. It is kept
/// because `matchGrammar`'s re-match guard is keyed on the rule, and because a
/// generated file whose rules were anonymous would be unreadable.
pub(crate) struct Rule {
    /// **Read by nobody, and kept anyway.** Prism keys its re-match guard on
    /// this name (`token + ',' + j`, prism-core.js:926); the interpreter uses
    /// the rule's *index*, which is the same key without the allocation because
    /// names are unique within a grammar. What the field buys is the one thing
    /// D14 says a generated file must have — a diff a human can read. A
    /// `RULES` table of anonymous `(first, count)` pairs would make every
    /// regeneration unreviewable, which is the failure mode
    /// `bench/layout-goldens/README.md` exists to prevent.
    #[allow(dead_code)]
    pub(crate) name: &'static str,
    pub(crate) first: usize,
    pub(crate) count: usize,
}

/// One grammar: `rules` is `RULES[first..first + count]`, **in Prism's key
/// order**, which is the order they are tried in.
pub(crate) struct Grammar {
    pub(crate) first: usize,
    pub(crate) count: usize,
}

/// One compiled regex per pattern, built on first use and never rebuilt.
///
/// **Lazy per pattern rather than per language, and that is a measured
/// choice.** §5's `mt-highlight` header carries D3's fallback trigger — *"if the
/// ported engine highlights a visible fence in more than 16 ms … `syntect`
/// takes the top-40 languages"* — and compiling a whole grammar eagerly is the
/// shape most likely to trip it: `java`'s closure alone is 311 patterns.
/// `matchGrammar` does end up trying every pattern of the grammars it reaches,
/// so the saving is not in a single fence; it is that a document with one Rust
/// fence never compiles TypeScript's 204.
///
/// `OnceLock` rather than a `Mutex<HashMap>`: the cache is write-once per slot
/// and read from every fence, so a lock on the read path would be pure cost, and
/// D9's parallel layout is a stated M3 direction.
static COMPILED: LazyLock<Box<[OnceLock<Regex>]>> =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());

/// The compiled form of `PATTERNS[index]`.
///
/// Panics if the pattern does not compile. That is not a runtime failure mode:
/// `cargo xtask grammars` compiles every pattern with this same engine before
/// writing the file, and `every_generated_pattern_compiles` compiles all of them
/// again in `cargo test`. A panic here means the committed file was hand-edited,
/// which its header forbids.
pub(crate) fn compiled(index: usize) -> &'static Regex {
    COMPILED[index].get_or_init(|| {
        Regex::new(PATTERNS[index].source).unwrap_or_else(|e| {
            panic!(
                "generated pattern {index} does not compile: {e}\n  {}",
                PATTERNS[index].source
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::{GRAMMARS, RULES};

    /// Every generated pattern compiles.
    ///
    /// The generator checks this before it writes, so this test is the guard on
    /// the *committed* file rather than on the translation — it is what turns a
    /// hand-edit of `generated.rs` into a red test instead of a panic in front
    /// of a user.
    #[test]
    fn every_generated_pattern_compiles() {
        for index in 0..PATTERNS.len() {
            let _ = compiled(index);
        }
    }

    /// The `(first, count)` slices are in range and tile their arrays exactly.
    ///
    /// A generator that emitted an off-by-one here would produce a grammar that
    /// silently tried the wrong rules — which would look like a Prism
    /// disagreement rather than like the encoding bug it is.
    #[test]
    fn the_flat_encoding_tiles_without_gaps_or_overlaps() {
        let mut rule_cursor = 0;
        for grammar in GRAMMARS {
            assert_eq!(grammar.first, rule_cursor, "grammars must tile RULES");
            rule_cursor += grammar.count;
        }
        assert_eq!(rule_cursor, RULES.len(), "grammars must cover all of RULES");

        let mut pattern_cursor = 0;
        for rule in RULES {
            assert_eq!(rule.first, pattern_cursor, "rules must tile PATTERNS");
            assert!(rule.count > 0, "a rule with no patterns cannot match");
            pattern_cursor += rule.count;
        }
        assert_eq!(
            pattern_cursor,
            PATTERNS.len(),
            "rules must cover all of PATTERNS"
        );

        for pattern in PATTERNS {
            if let Some(inside) = pattern.inside {
                assert!(
                    (inside as usize) < GRAMMARS.len(),
                    "`inside` {inside} is out of range"
                );
            }
            assert!(!pattern.class.is_empty(), "a span class is never empty");
        }
    }
}
