//! `packages/muya/src/utils/stringWidth.ts`, 60 lines, transcribed.
//!
//! # Why this is not `unicode-width`
//!
//! Because it is not the same function. `unicode-width` implements UAX #11:
//! it consults the East_Asian_Width property for every assigned code point.
//! muya's file inlines **fifteen hard-coded ranges**, with a comment saying
//! why — *"JavaScript's Unicode property escapes do not expose
//! East_Asian_Width"* — and a fifteen-entry list is not that property. The
//! difference is visible by reading the list rather than by trusting this
//! sentence: the emoji block starts at `0x1F300`, so every wide code point
//! below it is one column here. `U+1F004` (🀄, MAHJONG TILE RED DRAGON) is
//! `W` in UAX #11 and **1** in muya; so is every regional-indicator flag
//! letter, `U+1F1E6..=U+1F1FF`. In the other direction the ranges are solid
//! blocks where the property is not: everything in `0x1F300..=0x1F64F` is two
//! columns here regardless of what UAX #11 says about it.
//!
//! Column width is what `_serializeTable`'s padding arithmetic is made of — a
//! `+ 2` on the way in and a `- 1` on the way out — so a one-column
//! disagreement is a byte difference in a round trip that is supposed to be
//! the identity. **The file is ported, not substituted.**
//!
//! # The one thing that is not inlined
//!
//! `/\p{Mn}|\p{Me}/u`. That is a Unicode general-category test over ~2,000 code
//! points in a few hundred ranges, and hand-inlining it would be a table that
//! drifts from the Unicode version silently — the failure mode this milestone
//! keeps recording. `fancy-regex` is already in the workspace graph
//! (`mt-inline` depends on it and `mt-md` depends on `mt-inline`), so the class
//! is transcribed as the pattern it is. `cargo xtask deps` is unchanged.
//!
//! Both engines therefore answer from *their own* Unicode tables, which is the
//! same exposure JavaScript's `\p{Mn}` already has against Node's version. It
//! is a narrower exposure than a frozen hand-written list.

use std::sync::LazyLock;

use fancy_regex::Regex;

/// East-Asian Wide (W) and Fullwidth (F) code-point ranges, verbatim from
/// `stringWidth.ts`. Each pair is an inclusive `[start, end]` range whose code
/// points occupy two monospace columns.
///
/// Deliberately in source order rather than sorted-and-merged: this is a
/// transcription, and a diff against the TypeScript should be readable.
const WIDE_RANGES: &[(u32, u32)] = &[
    (0x1100, 0x115F),   // Hangul Jamo
    (0x2E80, 0x303E),   // CJK Radicals .. Kangxi Radicals .. CJK symbols
    (0x3041, 0x33FF),   // Hiragana, Katakana, CJK symbols and punctuation
    (0x3400, 0x4DBF),   // CJK Unified Ideographs Extension A
    (0x4E00, 0x9FFF),   // CJK Unified Ideographs
    (0xA000, 0xA4CF),   // Yi Syllables / Radicals
    (0xAC00, 0xD7A3),   // Hangul Syllables
    (0xF900, 0xFAFF),   // CJK Compatibility Ideographs
    (0xFE10, 0xFE19),   // Vertical forms
    (0xFE30, 0xFE6F),   // CJK Compatibility Forms / Small Form Variants
    (0xFF00, 0xFF60),   // Fullwidth Forms
    (0xFFE0, 0xFFE6),   // Fullwidth signs
    (0x1F300, 0x1F64F), // Emoticons / Misc symbols and pictographs
    (0x1F900, 0x1F9FF), // Supplemental symbols and pictographs
    (0x20000, 0x3FFFD), // CJK Unified Ideographs Extension B and beyond
];

/// `const COMBINING_MARK = /\p{Mn}|\p{Me}/u`.
///
/// Compiled once. The pattern is applied to a single code point at a time, as
/// the TypeScript applies it — `COMBINING_MARK.test(char)` inside a `for…of` —
/// so it is a category test rather than a search, and the unanchored form is
/// what muya wrote.
static COMBINING_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{Mn}|\p{Me}").expect("a literal category class"));

/// `function isZeroWidth(codePoint)`: format characters that occupy no columns.
fn is_zero_width(code_point: u32) -> bool {
    code_point == 0x200B                            // zero width space
        || (0x200C..=0x200F).contains(&code_point)  // ZWNJ/ZWJ/marks
        || code_point == 0xFEFF // zero width no-break space (BOM)
}

/// `function isWide(codePoint)`: a linear scan of [`WIDE_RANGES`], as muya's
/// `.some()` is.
fn is_wide(code_point: u32) -> bool {
    WIDE_RANGES
        .iter()
        .any(|(start, end)| code_point >= *start && code_point <= *end)
}

/// The number of monospace columns `s` occupies.
///
/// Combining marks and zero-width formatting characters contribute 0;
/// East-Asian wide / fullwidth code points contribute 2; everything else
/// contributes 1.
///
/// muya iterates with `for…of`, which walks the string **by code point**, so
/// an astral character is measured once rather than per UTF-16 code unit.
/// Rust's `chars()` is the same iteration, which is the one place the port is
/// simpler than the original rather than merely equal to it.
#[must_use]
pub fn string_width(s: &str) -> usize {
    let mut width = 0;
    let mut buf = [0u8; 4];
    for ch in s.chars() {
        let code_point = ch as u32;
        if is_zero_width(code_point)
            || COMBINING_MARK
                .is_match(ch.encode_utf8(&mut buf))
                .unwrap_or(false)
        {
            continue;
        }
        width += usize::from(is_wide(code_point)) + 1;
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `stateToMarkdown.spec.ts`'s combining-marks case, measured: `aʊ̯x` is
    /// four code points and **three** columns, because U+032F is `Mn`.
    #[test]
    fn a_combining_mark_costs_no_columns() {
        assert_eq!(string_width("a\u{28a}\u{32f}x"), 3);
        assert_eq!("a\u{28a}\u{32f}x".chars().count(), 4);
        // The sibling in the same spec table, which has no mark at all.
        assert_eq!(string_width("n\u{254}x"), 3);
    }

    /// The wide case from the same table.
    #[test]
    fn an_east_asian_character_costs_two_columns() {
        assert_eq!(string_width("中文"), 4);
        assert_eq!(string_width("id"), 2);
        // Hangul syllables and fullwidth forms are separate ranges.
        assert_eq!(string_width("한"), 2);
        assert_eq!(string_width("Ａ"), 2);
    }

    /// The zero-width family is a hand-written list in the TypeScript and
    /// therefore here: a code point outside it that is *also* invisible still
    /// costs a column, because muya charges it one.
    #[test]
    fn the_zero_width_list_is_the_list_and_not_a_property() {
        assert_eq!(string_width("a\u{200b}b"), 2);
        assert_eq!(string_width("a\u{200d}b"), 2);
        assert_eq!(string_width("a\u{feff}b"), 2);
        // U+2060 WORD JOINER is invisible and is *not* in muya's list.
        assert_eq!(string_width("a\u{2060}b"), 3);
    }

    /// An astral code point is one iteration, not two — the `for…of` comment in
    /// the original. An emoji in muya's `1F300..1F64F` range is two columns.
    #[test]
    fn an_astral_code_point_is_measured_once() {
        assert_eq!(string_width("😀"), 2);
        assert_eq!("😀".encode_utf16().count(), 2);
        // Outside every range: two columns' worth of UTF-16, one column here.
        assert_eq!(string_width("\u{10400}"), 1);
    }

    #[test]
    fn the_empty_string_is_zero_columns() {
        assert_eq!(string_width(""), 0);
    }

    /// The range list is transcribed in source order, and its boundaries are
    /// the boundaries — one past each end is narrow.
    #[test]
    fn the_range_boundaries_are_inclusive_and_the_gaps_are_real() {
        // 0x2E80..=0x303E is wide; 0x303F is the gap before 0x3041.
        assert_eq!(string_width("\u{303E}"), 2);
        assert_eq!(string_width("\u{303F}"), 1);
        assert_eq!(string_width("\u{3040}"), 1);
        assert_eq!(string_width("\u{3041}"), 2);
        // 0x4DBF ends Extension A; 0x4DC0 (Yijing hexagrams) is the gap.
        assert_eq!(string_width("\u{4DBF}"), 2);
        assert_eq!(string_width("\u{4DC0}"), 1);
    }
}
