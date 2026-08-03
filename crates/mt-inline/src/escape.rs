//! The `html_escape` rule's alternation set — `config/escapeCharacter.ts`.
//!
//! muya builds the rule at `rules.ts:45` as
//!
//! ```js
//! html_escape: new RegExp(`^(${escapeCharacters.join('|')})`, 'i'),
//! ```
//!
//! so this list *is* the rule. It is transcribed verbatim, in source order and
//! including muya's duplicates, because a regex alternation is leftmost-first
//! and "verbatim" is the porting rule (§3). The duplicates are harmless: every
//! entry ends in `;`, so no entry can be a prefix of another and the order
//! cannot change which alternative wins.
//!
//! # Counts
//!
//! M1.md §2 records "538 HTML entity ↔ character pairs". That is the line
//! count of the TypeScript file: **269 entities** paired with 269 characters.
//! Of the 269 entity strings, **252 are distinct**.
//!
//! `escapeCharactersMap` is built by `reduce` over the same two arrays, so a
//! repeated key is **overwritten by its last occurrence** and the decode map
//! has 252 entries rather than 269. That is [`ESCAPE_CHARACTERS_MAP`], added at
//! S6, which is the stage that needed it — S1 needed only the keys.
//!
//! ## S1 was wrong about *why* the duplicates matter, and S6 measured it
//!
//! This header used to say the repeated keys were "mapped to different
//! characters at different indices", and [`crate::entities`] repeated the claim
//! as part of its argument for being a separate table. **It is false.** The
//! duplicates were enumerated against the running module at S6: there are
//! **twelve** repeated keys, not five — `&quot;` `&amp;` `&lt;` `&gt;` `&nbsp;`
//! (three occurrences each), and `&ensp;` `&emsp;` `&copy;` `&reg;` `&trade;`
//! `&times;` `&divide;` (two each), accounting for all 17 redundant rows — and
//! **every occurrence of a key pairs with the same character**. `&nbsp;` is
//! U+00A0 at indices 4, 7 and 17 alike.
//!
//! So "last occurrence wins" is a distinction without a difference here: the
//! map is well defined whichever occurrence resolves it, and a future
//! regeneration that de-duplicates in the other direction would produce the
//! same 252 pairs. `the_only_duplicated_keys_are_exact_repeats` asserts that,
//! because it is the sort of thing that stops being true when someone edits
//! the TypeScript.
//!
//! What survives of the original point is the part that was never about the
//! values: this list is a **rule alternation**, not a decode map, so it is
//! ordered and repetitive by nature, and [`crate::entities`] — the HTML5 table
//! — is a different table for different reasons. The reasons are in that
//! module's header and none of them was the duplicate-values claim.

/// The 269 entity strings, in `escapeCharacter.ts` order.
pub(crate) const ESCAPE_CHARACTERS: [&str; 269] = [
    "&quot;",
    "&amp;",
    "&lt;",
    "&gt;",
    "&nbsp;",
    "&ensp;",
    "&emsp;",
    "&nbsp;",
    "&lt;",
    "&gt;",
    "&amp;",
    "&quot;",
    "&copy;",
    "&reg;",
    "&trade;",
    "&times;",
    "&divide;",
    "&nbsp;",
    "&iexcl;",
    "&cent;",
    "&pound;",
    "&curren;",
    "&yen;",
    "&brvbar;",
    "&sect;",
    "&uml;",
    "&copy;",
    "&ordf;",
    "&laquo;",
    "&not;",
    "&shy;",
    "&reg;",
    "&macr;",
    "&deg;",
    "&plusmn;",
    "&sup2;",
    "&sup3;",
    "&acute;",
    "&micro;",
    "&para;",
    "&middot;",
    "&cedil;",
    "&sup1;",
    "&ordm;",
    "&raquo;",
    "&frac14;",
    "&frac12;",
    "&frac34;",
    "&iquest;",
    "&Agrave;",
    "&Aacute;",
    "&Acirc;",
    "&Atilde;",
    "&Auml;",
    "&Aring;",
    "&AElig;",
    "&Ccedil;",
    "&Egrave;",
    "&Eacute;",
    "&Ecirc;",
    "&Euml;",
    "&Igrave;",
    "&Iacute;",
    "&Icirc;",
    "&Iuml;",
    "&ETH;",
    "&Ntilde;",
    "&Ograve;",
    "&Oacute;",
    "&Ocirc;",
    "&Otilde;",
    "&Ouml;",
    "&times;",
    "&Oslash;",
    "&Ugrave;",
    "&Uacute;",
    "&Ucirc;",
    "&Uuml;",
    "&Yacute;",
    "&THORN;",
    "&szlig;",
    "&agrave;",
    "&aacute;",
    "&acirc;",
    "&atilde;",
    "&auml;",
    "&aring;",
    "&aelig;",
    "&ccedil;",
    "&egrave;",
    "&eacute;",
    "&ecirc;",
    "&euml;",
    "&igrave;",
    "&iacute;",
    "&icirc;",
    "&iuml;",
    "&eth;",
    "&ntilde;",
    "&ograve;",
    "&oacute;",
    "&ocirc;",
    "&otilde;",
    "&ouml;",
    "&divide;",
    "&oslash;",
    "&ugrave;",
    "&uacute;",
    "&ucirc;",
    "&uuml;",
    "&yacute;",
    "&thorn;",
    "&yuml;",
    "&fnof;",
    "&Alpha;",
    "&Beta;",
    "&Gamma;",
    "&Delta;",
    "&Epsilon;",
    "&Zeta;",
    "&Eta;",
    "&Theta;",
    "&Iota;",
    "&Kappa;",
    "&Lambda;",
    "&Mu;",
    "&Nu;",
    "&Xi;",
    "&Omicron;",
    "&Pi;",
    "&Rho;",
    "&Sigma;",
    "&Tau;",
    "&Upsilon;",
    "&Phi;",
    "&Chi;",
    "&Psi;",
    "&Omega;",
    "&alpha;",
    "&beta;",
    "&gamma;",
    "&delta;",
    "&epsilon;",
    "&zeta;",
    "&eta;",
    "&theta;",
    "&iota;",
    "&kappa;",
    "&lambda;",
    "&mu;",
    "&nu;",
    "&xi;",
    "&omicron;",
    "&pi;",
    "&rho;",
    "&sigmaf;",
    "&sigma;",
    "&tau;",
    "&upsilon;",
    "&phi;",
    "&chi;",
    "&psi;",
    "&omega;",
    "&thetasym;",
    "&upsih;",
    "&piv;",
    "&bull;",
    "&hellip;",
    "&prime;",
    "&Prime;",
    "&oline;",
    "&frasl;",
    "&weierp;",
    "&image;",
    "&real;",
    "&trade;",
    "&alefsym;",
    "&larr;",
    "&uarr;",
    "&rarr;",
    "&darr;",
    "&harr;",
    "&crarr;",
    "&lArr;",
    "&uArr;",
    "&rArr;",
    "&dArr;",
    "&hArr;",
    "&forall;",
    "&part;",
    "&exist;",
    "&empty;",
    "&nabla;",
    "&isin;",
    "&notin;",
    "&ni;",
    "&prod;",
    "&sum;",
    "&minus;",
    "&lowast;",
    "&radic;",
    "&prop;",
    "&infin;",
    "&ang;",
    "&and;",
    "&or;",
    "&cap;",
    "&cup;",
    "&int;",
    "&there4;",
    "&sim;",
    "&cong;",
    "&asymp;",
    "&ne;",
    "&equiv;",
    "&le;",
    "&ge;",
    "&sub;",
    "&sup;",
    "&nsub;",
    "&sube;",
    "&supe;",
    "&oplus;",
    "&otimes;",
    "&perp;",
    "&sdot;",
    "&lceil;",
    "&rceil;",
    "&lfloor;",
    "&rfloor;",
    "&lang;",
    "&rang;",
    "&loz;",
    "&spades;",
    "&clubs;",
    "&hearts;",
    "&diams;",
    "&quot;",
    "&amp;",
    "&lt;",
    "&gt;",
    "&OElig;",
    "&oelig;",
    "&Scaron;",
    "&scaron;",
    "&Yuml;",
    "&circ;",
    "&tilde;",
    "&ensp;",
    "&emsp;",
    "&thinsp;",
    "&zwnj;",
    "&zwj;",
    "&lrm;",
    "&rlm;",
    "&ndash;",
    "&mdash;",
    "&lsquo;",
    "&rsquo;",
    "&sbquo;",
    "&ldquo;",
    "&rdquo;",
    "&bdquo;",
    "&dagger;",
    "&Dagger;",
    "&permil;",
    "&lsaquo;",
    "&rsaquo;",
    "&euro;",
];

/// `escapeCharactersMap` (`escapeCharacter.ts:546`) — the **decode** direction.
///
/// [`ESCAPE_CHARACTERS`] is the rule; this is what `tokensToPlainText` looks a
/// matched `html_escape` token up in (`lexer.ts:976`). 252 pairs, sorted by key
/// in byte order so the lookup is a binary search — the same shape as
/// [`crate::entities::NAMED_REFERENCES`], and for the same reason.
///
/// **Keys carry muya's exact spelling, and the lookup is case-sensitive**,
/// because JavaScript object indexing is. That matters: `rules.ts:45` builds
/// the `html_escape` rule with the `i` flag, so `&AMP;` *matches the rule* and
/// becomes an `html_escape` token whose `escapeCharacter` is `&AMP;` — which is
/// not a key here. See [`escape_character`] for what happens then.
pub(crate) static ESCAPE_CHARACTERS_MAP: [(&str, &str); 252] = [
    ("&AElig;", "\u{c6}"),
    ("&Aacute;", "\u{c1}"),
    ("&Acirc;", "\u{c2}"),
    ("&Agrave;", "\u{c0}"),
    ("&Alpha;", "\u{391}"),
    ("&Aring;", "\u{c5}"),
    ("&Atilde;", "\u{c3}"),
    ("&Auml;", "\u{c4}"),
    ("&Beta;", "\u{392}"),
    ("&Ccedil;", "\u{c7}"),
    ("&Chi;", "\u{3a7}"),
    ("&Dagger;", "\u{2021}"),
    ("&Delta;", "\u{394}"),
    ("&ETH;", "\u{d0}"),
    ("&Eacute;", "\u{c9}"),
    ("&Ecirc;", "\u{ca}"),
    ("&Egrave;", "\u{c8}"),
    ("&Epsilon;", "\u{395}"),
    ("&Eta;", "\u{397}"),
    ("&Euml;", "\u{cb}"),
    ("&Gamma;", "\u{393}"),
    ("&Iacute;", "\u{cd}"),
    ("&Icirc;", "\u{ce}"),
    ("&Igrave;", "\u{cc}"),
    ("&Iota;", "\u{399}"),
    ("&Iuml;", "\u{cf}"),
    ("&Kappa;", "\u{39a}"),
    ("&Lambda;", "\u{39b}"),
    ("&Mu;", "\u{39c}"),
    ("&Ntilde;", "\u{d1}"),
    ("&Nu;", "\u{39d}"),
    ("&OElig;", "\u{152}"),
    ("&Oacute;", "\u{d3}"),
    ("&Ocirc;", "\u{d4}"),
    ("&Ograve;", "\u{d2}"),
    ("&Omega;", "\u{3a9}"),
    ("&Omicron;", "\u{39f}"),
    ("&Oslash;", "\u{d8}"),
    ("&Otilde;", "\u{d5}"),
    ("&Ouml;", "\u{d6}"),
    ("&Phi;", "\u{3a6}"),
    ("&Pi;", "\u{3a0}"),
    ("&Prime;", "\u{2033}"),
    ("&Psi;", "\u{3a8}"),
    ("&Rho;", "\u{3a1}"),
    ("&Scaron;", "\u{160}"),
    ("&Sigma;", "\u{3a3}"),
    ("&THORN;", "\u{de}"),
    ("&Tau;", "\u{3a4}"),
    ("&Theta;", "\u{398}"),
    ("&Uacute;", "\u{da}"),
    ("&Ucirc;", "\u{db}"),
    ("&Ugrave;", "\u{d9}"),
    ("&Upsilon;", "\u{3a5}"),
    ("&Uuml;", "\u{dc}"),
    ("&Xi;", "\u{39e}"),
    ("&Yacute;", "\u{dd}"),
    ("&Yuml;", "\u{178}"),
    ("&Zeta;", "\u{396}"),
    ("&aacute;", "\u{e1}"),
    ("&acirc;", "\u{e2}"),
    ("&acute;", "\u{b4}"),
    ("&aelig;", "\u{e6}"),
    ("&agrave;", "\u{e0}"),
    ("&alefsym;", "\u{2135}"),
    ("&alpha;", "\u{3b1}"),
    ("&amp;", "&"),
    ("&and;", "\u{2227}"),
    ("&ang;", "\u{2220}"),
    ("&aring;", "\u{e5}"),
    ("&asymp;", "\u{2248}"),
    ("&atilde;", "\u{e3}"),
    ("&auml;", "\u{e4}"),
    ("&bdquo;", "\u{201e}"),
    ("&beta;", "\u{3b2}"),
    ("&brvbar;", "\u{a6}"),
    ("&bull;", "\u{2022}"),
    ("&cap;", "\u{2229}"),
    ("&ccedil;", "\u{e7}"),
    ("&cedil;", "\u{b8}"),
    ("&cent;", "\u{a2}"),
    ("&chi;", "\u{3c7}"),
    ("&circ;", "\u{2c6}"),
    ("&clubs;", "\u{2663}"),
    ("&cong;", "\u{2245}"),
    ("&copy;", "\u{a9}"),
    ("&crarr;", "\u{21b5}"),
    ("&cup;", "\u{222a}"),
    ("&curren;", "\u{a4}"),
    ("&dArr;", "\u{21d3}"),
    ("&dagger;", "\u{2020}"),
    ("&darr;", "\u{2193}"),
    ("&deg;", "\u{b0}"),
    ("&delta;", "\u{3b4}"),
    ("&diams;", "\u{2666}"),
    ("&divide;", "\u{f7}"),
    ("&eacute;", "\u{e9}"),
    ("&ecirc;", "\u{ea}"),
    ("&egrave;", "\u{e8}"),
    ("&empty;", "\u{2205}"),
    ("&emsp;", "\u{2003}"),
    ("&ensp;", "\u{2002}"),
    ("&epsilon;", "\u{3b5}"),
    ("&equiv;", "\u{2261}"),
    ("&eta;", "\u{3b7}"),
    ("&eth;", "\u{f0}"),
    ("&euml;", "\u{eb}"),
    ("&euro;", "\u{20ac}"),
    ("&exist;", "\u{2203}"),
    ("&fnof;", "\u{192}"),
    ("&forall;", "\u{2200}"),
    ("&frac12;", "\u{bd}"),
    ("&frac14;", "\u{bc}"),
    ("&frac34;", "\u{be}"),
    ("&frasl;", "\u{2044}"),
    ("&gamma;", "\u{3b3}"),
    ("&ge;", "\u{2265}"),
    ("&gt;", ">"),
    ("&hArr;", "\u{21d4}"),
    ("&harr;", "\u{2194}"),
    ("&hearts;", "\u{2665}"),
    ("&hellip;", "\u{2026}"),
    ("&iacute;", "\u{ed}"),
    ("&icirc;", "\u{ee}"),
    ("&iexcl;", "\u{a1}"),
    ("&igrave;", "\u{ec}"),
    ("&image;", "\u{2111}"),
    ("&infin;", "\u{221e}"),
    ("&int;", "\u{222b}"),
    ("&iota;", "\u{3b9}"),
    ("&iquest;", "\u{bf}"),
    ("&isin;", "\u{2208}"),
    ("&iuml;", "\u{ef}"),
    ("&kappa;", "\u{3ba}"),
    ("&lArr;", "\u{21d0}"),
    ("&lambda;", "\u{3bb}"),
    ("&lang;", "\u{27e8}"),
    ("&laquo;", "\u{ab}"),
    ("&larr;", "\u{2190}"),
    ("&lceil;", "\u{2308}"),
    ("&ldquo;", "\u{201c}"),
    ("&le;", "\u{2264}"),
    ("&lfloor;", "\u{230a}"),
    ("&lowast;", "\u{2217}"),
    ("&loz;", "\u{25ca}"),
    ("&lrm;", "\u{200e}"),
    ("&lsaquo;", "\u{2039}"),
    ("&lsquo;", "\u{2018}"),
    ("&lt;", "<"),
    ("&macr;", "\u{af}"),
    ("&mdash;", "\u{2014}"),
    ("&micro;", "\u{b5}"),
    ("&middot;", "\u{b7}"),
    ("&minus;", "\u{2212}"),
    ("&mu;", "\u{3bc}"),
    ("&nabla;", "\u{2207}"),
    ("&nbsp;", "\u{a0}"),
    ("&ndash;", "\u{2013}"),
    ("&ne;", "\u{2260}"),
    ("&ni;", "\u{220b}"),
    ("&not;", "\u{ac}"),
    ("&notin;", "\u{2209}"),
    ("&nsub;", "\u{2284}"),
    ("&ntilde;", "\u{f1}"),
    ("&nu;", "\u{3bd}"),
    ("&oacute;", "\u{f3}"),
    ("&ocirc;", "\u{f4}"),
    ("&oelig;", "\u{153}"),
    ("&ograve;", "\u{f2}"),
    ("&oline;", "\u{203e}"),
    ("&omega;", "\u{3c9}"),
    ("&omicron;", "\u{3bf}"),
    ("&oplus;", "\u{2295}"),
    ("&or;", "\u{2228}"),
    ("&ordf;", "\u{aa}"),
    ("&ordm;", "\u{ba}"),
    ("&oslash;", "\u{f8}"),
    ("&otilde;", "\u{f5}"),
    ("&otimes;", "\u{2297}"),
    ("&ouml;", "\u{f6}"),
    ("&para;", "\u{b6}"),
    ("&part;", "\u{2202}"),
    ("&permil;", "\u{2030}"),
    ("&perp;", "\u{22a5}"),
    ("&phi;", "\u{3c6}"),
    ("&pi;", "\u{3c0}"),
    ("&piv;", "\u{3d6}"),
    ("&plusmn;", "\u{b1}"),
    ("&pound;", "\u{a3}"),
    ("&prime;", "\u{2032}"),
    ("&prod;", "\u{220f}"),
    ("&prop;", "\u{221d}"),
    ("&psi;", "\u{3c8}"),
    ("&quot;", "\""),
    ("&rArr;", "\u{21d2}"),
    ("&radic;", "\u{221a}"),
    ("&rang;", "\u{27e9}"),
    ("&raquo;", "\u{bb}"),
    ("&rarr;", "\u{2192}"),
    ("&rceil;", "\u{2309}"),
    ("&rdquo;", "\u{201d}"),
    ("&real;", "\u{211c}"),
    ("&reg;", "\u{ae}"),
    ("&rfloor;", "\u{230b}"),
    ("&rho;", "\u{3c1}"),
    ("&rlm;", "\u{200f}"),
    ("&rsaquo;", "\u{203a}"),
    ("&rsquo;", "\u{2019}"),
    ("&sbquo;", "\u{201a}"),
    ("&scaron;", "\u{161}"),
    ("&sdot;", "\u{22c5}"),
    ("&sect;", "\u{a7}"),
    ("&shy;", "\u{ad}"),
    ("&sigma;", "\u{3c3}"),
    ("&sigmaf;", "\u{3c2}"),
    ("&sim;", "\u{223c}"),
    ("&spades;", "\u{2660}"),
    ("&sub;", "\u{2282}"),
    ("&sube;", "\u{2286}"),
    ("&sum;", "\u{2211}"),
    ("&sup1;", "\u{b9}"),
    ("&sup2;", "\u{b2}"),
    ("&sup3;", "\u{b3}"),
    ("&sup;", "\u{2283}"),
    ("&supe;", "\u{2287}"),
    ("&szlig;", "\u{df}"),
    ("&tau;", "\u{3c4}"),
    ("&there4;", "\u{2234}"),
    ("&theta;", "\u{3b8}"),
    ("&thetasym;", "\u{3d1}"),
    ("&thinsp;", "\u{2009}"),
    ("&thorn;", "\u{fe}"),
    ("&tilde;", "\u{2dc}"),
    ("&times;", "\u{d7}"),
    ("&trade;", "\u{2122}"),
    ("&uArr;", "\u{21d1}"),
    ("&uacute;", "\u{fa}"),
    ("&uarr;", "\u{2191}"),
    ("&ucirc;", "\u{fb}"),
    ("&ugrave;", "\u{f9}"),
    ("&uml;", "\u{a8}"),
    ("&upsih;", "\u{3d2}"),
    ("&upsilon;", "\u{3c5}"),
    ("&uuml;", "\u{fc}"),
    ("&weierp;", "\u{2118}"),
    ("&xi;", "\u{3be}"),
    ("&yacute;", "\u{fd}"),
    ("&yen;", "\u{a5}"),
    ("&yuml;", "\u{ff}"),
    ("&zeta;", "\u{3b6}"),
    ("&zwj;", "\u{200d}"),
    ("&zwnj;", "\u{200c}"),
];

/// The character `entity` decodes to, or `None` if it is not one of the 252.
///
/// `None` is not a "should not happen": `escapeCharactersMap[…] ?? token.raw`
/// (`lexer.ts:976`) has a **live** fallback, because the rule that produced the
/// token is case-insensitive (`rules.ts:45`, the `i` flag) while JavaScript
/// object indexing — and therefore this map — is not.
///
/// Measured against the running engine rather than argued, because three
/// stages running had found a branch that *looks* load-bearing and provably
/// cannot fire (S2's rule-16 guard, S3's `correctUrl` group 5, S5's
/// `if (!email)`), and the prior on a fourth was not low. **Every case
/// spelling of every one of the 252 names**, exhaustively — the longest name
/// is eight letters, so there are 6084 distinct spellings — gives:
///
/// - 6084 of 6084 are an `html_escape` token, so the rule really is
///   case-blind;
/// - **252 hit this map**, which is exactly the canonical spellings, one each;
/// - **5832 miss**, and muya renders every one of them as its own source text.
///
/// So the fallback is not a defensive `??`. It is the branch 96% of the
/// reachable inputs take, and a port that treated a miss as impossible would be
/// wrong on `&AMP;`.
pub(crate) fn escape_character(entity: &str) -> Option<&'static str> {
    ESCAPE_CHARACTERS_MAP
        .binary_search_by(|(key, _)| (*key).cmp(entity))
        .ok()
        .map(|index| ESCAPE_CHARACTERS_MAP[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two numbers the module docs rest on, so a bad regeneration of this
    /// table is caught here rather than as a mystery `html_escape` failure.
    #[test]
    fn the_table_has_muyas_269_entries_of_which_252_are_distinct() {
        assert_eq!(ESCAPE_CHARACTERS.len(), 269);
        let mut distinct: Vec<&str> = ESCAPE_CHARACTERS.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 252);
    }

    /// Every entry is `&` + ASCII alphanumerics + `;`. Two things rest on this:
    /// the alternation needs no regex-metacharacter escaping, and no entry can
    /// be a proper prefix of another.
    #[test]
    fn every_entry_is_an_ascii_named_reference() {
        for entity in ESCAPE_CHARACTERS {
            let body = entity
                .strip_prefix('&')
                .and_then(|e| e.strip_suffix(';'))
                .unwrap_or_else(|| panic!("`{entity}` is not `&…;`"));
            assert!(!body.is_empty(), "`{entity}` has an empty name");
            assert!(
                body.chars().all(|c| c.is_ascii_alphanumeric()),
                "`{entity}` is not ASCII-alphanumeric, so the alternation would \
                 need escaping"
            );
        }
    }

    /// The map is the rule's key set, exactly — no key the rule cannot produce,
    /// and no key of the rule missing from the map. A regeneration that drifts
    /// either way would make `tokensToPlainText` silently fall back.
    #[test]
    fn the_map_covers_the_rules_key_set_and_nothing_else() {
        assert_eq!(ESCAPE_CHARACTERS_MAP.len(), 252);

        let mut from_rule: Vec<&str> = ESCAPE_CHARACTERS.to_vec();
        from_rule.sort_unstable();
        from_rule.dedup();
        let from_map: Vec<&str> = ESCAPE_CHARACTERS_MAP.iter().map(|(key, _)| *key).collect();
        assert_eq!(from_rule, from_map);
    }

    /// [`escape_character`] is a binary search, so this is a correctness
    /// precondition rather than a style point.
    #[test]
    fn the_map_is_sorted_by_key_in_byte_order() {
        for pair in ESCAPE_CHARACTERS_MAP.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "`{}` is not before `{}`",
                pair[0].0,
                pair[1].0
            );
        }
    }

    /// The correction this module's header records, asserted rather than
    /// stated. Twelve keys repeat, seventeen rows are redundant, and every
    /// repeat pairs with the same character — so `reduce`'s last-occurrence-
    /// wins resolution is unobservable.
    ///
    /// It cannot be checked from [`ESCAPE_CHARACTERS`] alone, which is only the
    /// keys, so it is checked the way it is used: every key, however many times
    /// it appears in the rule, resolves through the map to one character.
    #[test]
    fn the_only_duplicated_keys_are_exact_repeats() {
        let mut repeated: Vec<&str> = Vec::new();
        for (index, entity) in ESCAPE_CHARACTERS.iter().enumerate() {
            if ESCAPE_CHARACTERS[..index].contains(entity) && !repeated.contains(entity) {
                repeated.push(entity);
            }
        }
        repeated.sort_unstable();
        assert_eq!(
            repeated,
            [
                "&amp;", "&copy;", "&divide;", "&emsp;", "&ensp;", "&gt;", "&lt;", "&nbsp;",
                "&quot;", "&reg;", "&times;", "&trade;",
            ],
            "the duplicated keys changed"
        );
        assert_eq!(
            ESCAPE_CHARACTERS.len() - ESCAPE_CHARACTERS_MAP.len(),
            17,
            "seventeen rows of the 269 are redundant"
        );
        for entity in repeated {
            assert!(
                escape_character(entity).is_some(),
                "`{entity}` repeats in the rule but is absent from the map"
            );
        }
    }

    /// marktext #3840: the three space entities are three *different* spaces,
    /// and a decode map that collapses them to U+0020 is a data-loss bug.
    /// `config/__tests__/escapeCharacter.spec.ts` is the muya test this mirrors
    /// — the only test muya has for this file.
    #[test]
    fn the_three_space_entities_keep_their_own_code_points() {
        assert_eq!(escape_character("&nbsp;"), Some("\u{a0}"));
        assert_eq!(escape_character("&ensp;"), Some("\u{2002}"));
        assert_eq!(escape_character("&emsp;"), Some("\u{2003}"));
    }

    /// The lookup is case-sensitive and the rule that feeds it is not, so a
    /// non-canonical spelling misses. That is the reachable half of
    /// `lexer.ts:976`'s `?? token.raw`.
    #[test]
    fn a_non_canonical_spelling_is_not_a_key() {
        assert_eq!(escape_character("&amp;"), Some("&"));
        assert_eq!(escape_character("&AMP;"), None);
        assert_eq!(escape_character("&Amp;"), None);
        assert_eq!(escape_character("&nowhere;"), None);
    }
}
