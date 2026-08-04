//! `codeFenceInfoString.spec.ts` — 4 cases.
//!
//! #4770. MarkText used only the **first word** of a fence's info string as
//! the language and serialized just that word back, so
//! ```` ```{example, listing1-name} ```` was rewritten to ```` ```{example, ````
//! on save — everything after the first space dropped.
//!
//! This is M0's constraint 1 in its round-trip form: `CodeBlock::info` is the
//! whole info string, and `Block::highlight_language` is the derived first
//! word. `mt_doc`'s `block::tests::highlight_language_is_the_first_word_of_the_info_string`
//! is the other half.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "code_fence_info_string",

fn preserves_a_pandoc_style_attribute_info_string() {
    let out = round_trip(
        "```{example, listing1-name}\nlabel for code listing 1\n```\n",
        MUYA_DEFAULT,
    );
    assert!(out.contains("```{example, listing1-name}"), "{out:?}");
}

fn preserves_a_language_followed_by_attributes() {
    let out = round_trip("```js title=\"app.js\"\nconst a = 1\n```\n", MUYA_DEFAULT);
    assert!(out.contains("```js title=\"app.js\""), "{out:?}");
}

fn leaves_a_plain_single_word_language_unchanged() {
    let out = round_trip("```js\nconst a = 1\n```\n", MUYA_DEFAULT);
    assert!(out.contains("```js\n"), "{out:?}");
}

/// The `undefined` guard: an absent info string must serialize as nothing, not
/// as the string `"undefined"`.
fn leaves_a_language_less_fence_unchanged() {
    let out = round_trip("```\nplain\n```\n", MUYA_DEFAULT);
    assert!(out.contains("```\n"), "{out:?}");
    assert!(!out.contains("```undefined"), "{out:?}");
}

}
