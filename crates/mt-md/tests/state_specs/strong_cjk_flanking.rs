//! `strongCjkFlanking.spec.ts` — 6 cases.
//!
//! marktext #4307. CommonMark's emphasis flanking rule classifies every
//! character as whitespace, punctuation or "other"; CJK ideographs and Hangul
//! are "other". In `例子例子**"加粗"**例子例子` the character after the opening
//! `**` is `"` (punctuation) and the one before it is `子` ("other"), so
//! clause 2b makes the run not left-flanking and the `**` stays literal.
//!
//! Legacy marktext treated CJK as punctuation in its flanking helpers. muya
//! restores that **additively** in both of its inline paths, so nothing
//! CommonMark accepts as non-emphasis becomes emphasis and the conformance
//! suites are unaffected.
//!
//! # Three of these pass today, and are therefore not listed
//!
//! The `live editor path` cases go through the inline tokenizer, which M1
//! ported — `crates/mt-inline/src/emphasis.rs` carries `CJK_REG` and the
//! `canOpenEmphasis` / `canCloseEmphasis` widening, transcribed from muya's
//! `inlineRenderer/utils.ts`. So they pass at S0 and row two of the ratchet
//! requires them to be delisted.
//!
//! That is not a free pass: they are live guards with normal teeth from here
//! on. A stage that narrows the CJK class fails them, and the three
//! `static / export path` cases beside them stay listed until S5 implements
//! `render_to_static_html`, so this file is not delisted as a unit.

#![allow(unused_imports)]
use crate::*;

use mt_inline::{Token, TokenKind, TokenizerOptions};

/// The four #4307 examples: CJK ideographs, Kana, Hangul, and a non-BMP CJK
/// Ext-B case whose boundary check must read the whole code point.
const CJK_CASES: [&str; 4] = [
    "例子例子**\"加粗\"**例子例子",
    "日本語**(強調)**日本語",
    "한국어**[강조]**한국어",
    "𠀀𠀁**\"加粗\"**𠀀𠀁",
];

/// Already-working cases, kept so the widening cannot regress them.
const SANITY_CASES: [&str; 3] = [
    "before **\"normal\"** after",
    "before**normal**after",
    "中文**加粗**中文",
];

/// Must **not** emphasise — the widening is additive and these stay as
/// CommonMark rejects them.
const NEGATIVE_CASES: [&str; 3] = [
    "a * foo bar*", // space after the opening `*` — not left-flanking
    "a_foo bar_",   // intraword `_` emphasis is disallowed
    "*(*foo)",      // the inner `(` makes the run both-flanking, so it cannot open
];

/// The static / export path's options: everything off, `sanitize: false`.
const STATIC_OPTIONS: Options = NO_EXT;

fn renders_strong(src: &str) -> bool {
    html(src, STATIC_OPTIONS, false).contains("<strong>")
}

fn renders_em(src: &str) -> bool {
    html(src, STATIC_OPTIONS, false).contains("<em>")
}

fn collect_types(tokens: &[Token], out: &mut Vec<&'static str>) {
    for token in tokens {
        out.push(match &token.kind {
            TokenKind::Strong(_) => "strong",
            TokenKind::Em(_) => "em",
            _ => "other",
        });
        if let Some(children) = token.children() {
            collect_types(children, out);
        }
    }
}

/// The spec's `tokenizesEmphasis`: `tokenizer(src, { hasBeginRules: false })`,
/// then any `strong` or `em` anywhere in the tree.
fn tokenizes_emphasis(src: &str) -> bool {
    let tokens = mt_inline::tokenizer(
        src,
        &TokenizerOptions {
            has_begin_rules: false,
            ..TokenizerOptions::muya_default()
        },
    );
    let mut types = Vec::new();
    collect_types(&tokens, &mut types);
    types.iter().any(|t| *t == "strong" || *t == "em")
}

spec_cases! { "strong_cjk_flanking",

fn static_path_recognises_strong_in_the_sanity_cases() {
    for src in super::SANITY_CASES {
        assert!(super::renders_strong(src), "{src}");
    }
}

fn static_path_recognises_strong_in_cjk_context() {
    for src in super::CJK_CASES {
        assert!(super::renders_strong(src), "{src}");
    }
}

/// Both tags, not just `<strong>`: widening the flanking logic could surface
/// as an unexpected `<em>` while a strong-only check still passed.
fn static_path_does_not_bold_or_italicise_the_negative_cases() {
    for src in super::NEGATIVE_CASES {
        assert!(!super::renders_strong(src), "{src}");
        assert!(!super::renders_em(src), "{src}");
    }
}

fn editor_path_tokenizes_strong_or_em_in_the_sanity_cases() {
    for src in super::SANITY_CASES {
        assert!(super::tokenizes_emphasis(src), "{src}");
    }
}

fn editor_path_tokenizes_strong_or_em_in_cjk_context() {
    for src in super::CJK_CASES {
        assert!(super::tokenizes_emphasis(src), "{src}");
    }
}

fn editor_path_does_not_tokenize_strong_or_em_in_the_negative_cases() {
    for src in super::NEGATIVE_CASES {
        assert!(!super::tokenizes_emphasis(src), "{src}");
    }
}

}
