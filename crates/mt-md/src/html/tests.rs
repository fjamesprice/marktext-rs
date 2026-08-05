//! Every expected value here was **measured** from the running engine through
//! `renderToStaticHTML(src, { …, sanitize: false })`, not derived from reading
//! `Renderer.ts` — which is the method S2 recorded and the reason
//! `marked-highlight`'s replacement of the `code` renderer was not missed.
//!
//! Where the port deliberately differs from muya, the test says so in its name
//! and its doc comment, so a green run is a claim about a decision rather than
//! a claim about a coincidence.

use super::*;
use crate::{ListIndentation, Options};

/// `renderToStaticHTML`'s own defaults, which is what the transcribed spec
/// file drives: `superSubScript` on, everything else off.
const RENDER: Options = Options {
    footnote: false,
    math: false,
    super_sub_script: true,
    gitlab_compatibility: false,
    front_matter: false,
    trim_unnecessary_code_block_empty_lines: false,
    list_indentation: ListIndentation::Spaces(1),
};

fn render(src: &str) -> String {
    crate::render_to_static_html(src, RENDER, false).expect("S5 opened this door")
}

fn render_with(src: &str, options: Options) -> String {
    crate::render_to_static_html(src, options, false).expect("S5 opened this door")
}

// ---------------------------------------------------------------------------
// Block shapes, measured
// ---------------------------------------------------------------------------

#[test]
fn the_block_shapes_are_markeds_byte_for_byte() {
    let cases: &[(&str, &str)] = &[
        ("# h\n", "<h1>h</h1>\n"),
        ("Setext\n===\n", "<h1>Setext</h1>\n"),
        ("p\n", "<p>p</p>\n"),
        ("---\n", "<hr>\n"),
        ("> q\n> r\n", "<blockquote>\n<p>q\nr</p>\n</blockquote>\n"),
        ("- a\n- b\n", "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n"),
        ("1. one\n", "<ol>\n<li>one</li>\n</ol>\n"),
        ("5. five\n", "<ol start=\"5\">\n<li>five</li>\n</ol>\n"),
        // Loose lists wrap each item's paragraph; tight ones do not.
        (
            "- a\n\n- b\n",
            "<ul>\n<li><p>a</p>\n</li>\n<li><p>b</p>\n</li>\n</ul>\n",
        ),
        // A nested list follows the item's inline content with no separator.
        (
            "- a\n  - b\n",
            "<ul>\n<li>a<ul>\n<li>b</li>\n</ul>\n</li>\n</ul>\n",
        ),
        (
            "- [ ] a\n- [x] b\n",
            "<ul>\n<li><input disabled=\"\" type=\"checkbox\"> a</li>\n\
             <li><input checked=\"\" disabled=\"\" type=\"checkbox\"> b</li>\n</ul>\n",
        ),
        (
            "| a | b |\n|:--|--:|\n| 1 | 2 |\n",
            "<table>\n<thead>\n<tr>\n<th align=\"left\">a</th>\n\
             <th align=\"right\">b</th>\n</tr>\n</thead>\n\
             <tbody><tr>\n<td align=\"left\">1</td>\n\
             <td align=\"right\">2</td>\n</tr>\n</tbody></table>\n",
        ),
        // A reference definition renders as nothing — `Renderer.def`.
        (
            "[foo]: /url \"t\"\n\n[foo]\n",
            "<p><a href=\"/url\" title=\"t\">foo</a></p>\n",
        ),
        ("<div>\nraw\n</div>\n", "<div>\nraw\n</div>\n"),
    ];
    for (src, expected) in cases {
        assert_eq!(&render(src), expected, "input was {src:?}");
    }
}

/// `marked-highlight` replaces `Renderer.code`, and the three details reading
/// `Renderer.ts` would get wrong are all here: **no** trailing newline after
/// `</pre>`, **no** `class` attribute for an empty info string, and the body's
/// one trailing newline stripped and re-added.
#[test]
fn a_code_block_is_marked_highlights_shape_and_not_renderer_codes() {
    assert_eq!(
        render("```js\nx\n```\n"),
        "<pre><code class=\"language-js\">x\n</code></pre>"
    );
    assert_eq!(
        render("    indented code\n"),
        "<pre><code>indented code\n</code></pre>"
    );
    // `getLang` is `/\S*/`, so only the first word becomes the class.
    assert_eq!(
        render("```js title=\"x\"\nx\n```\n"),
        "<pre><code class=\"language-js\">x\n</code></pre>"
    );
}

/// Prism is not ported, and this is the one place where that shows up as a
/// **divergence in the port's favour**: muya rewrites a `js` fence's body into
/// `<span class="token …">` markup that no spec example expects.
#[test]
fn a_known_language_is_not_syntax_highlighted_the_way_prism_would() {
    let out = render("```js\nlet x = 1;\n```\n");
    assert!(!out.contains("class=\"token"), "{out}");
    assert_eq!(
        out,
        "<pre><code class=\"language-js\">let x = 1;\n</code></pre>"
    );
}

/// A diagram fence is inert on this path: `renderToStaticHTML` is the
/// *synchronous* renderer and never runs the diagram pass.
#[test]
fn a_diagram_fence_is_an_inert_placeholder() {
    let out = render("```mermaid\ngraph LR; A-->B\n```\n");
    assert_eq!(
        out,
        "<pre><code class=\"language-mermaid\">graph LR; A--&gt;B\n</code></pre>"
    );
}

#[test]
fn front_matter_is_front_matter_renders_shape() {
    let out = render_with(
        "---\ntitle: Hello\n---\n\n# Body\n",
        Options {
            front_matter: true,
            ..RENDER
        },
    );
    assert!(
        out.starts_with(
            "<pre class=\"front-matter\" data-style=\"-\" data-lang=\"yaml\">\ntitle: Hello</pre>\n"
        ),
        "{out}"
    );
    assert!(out.contains("<h1>Body</h1>"), "{out}");
}

// ---------------------------------------------------------------------------
// The inline layer
// ---------------------------------------------------------------------------

#[test]
fn the_inline_shapes_are_markeds() {
    let cases: &[(&str, &str)] = &[
        (
            "*em* **strong** ~~del~~\n",
            "<p><em>em</em> <strong>strong</strong> <del>del</del></p>\n",
        ),
        ("`co<de`\n", "<p><code>co&lt;de</code></p>\n"),
        (
            "a & b < c \"d\" 'e'\n",
            "<p>a &amp; b &lt; c &quot;d&quot; &#39;e&#39;</p>\n",
        ),
        // A recognised character reference survives; a bare `&` does not.
        (
            "&amp; &#35; &nbsp; &\n",
            "<p>&amp; &#35; &nbsp; &amp;</p>\n",
        ),
        ("\\*not em\\*\n", "<p>*not em*</p>\n"),
        (
            "![alt](/u \"t\")\n",
            "<p><img src=\"/u\" alt=\"alt\" title=\"t\"></p>\n",
        ),
        (
            "<https://x.com>\n",
            "<p><a href=\"https://x.com\">https://x.com</a></p>\n",
        ),
        (
            "<a@b.com>\n",
            "<p><a href=\"mailto:a@b.com\">a@b.com</a></p>\n",
        ),
        (
            "H~2~O and 2^n^\n",
            "<p>H<sub>2</sub>O and 2<sup>n</sup></p>\n",
        ),
        // #3676: a soft break stays a newline, a hard break is a `<br>`.
        ("a\nb\n", "<p>a\nb</p>\n"),
        ("a  \nb\n", "<p>a<br>b</p>\n"),
    ];
    for (src, expected) in cases {
        assert_eq!(&render(src), expected, "input was {src:?}");
    }
}

/// `superSubScript` is a *tokenizer* option, so turning it off leaves the
/// source alone rather than producing a different tag. That is where the port
/// and muya part company: `marked` with the extension off reads `~2~` as GFM
/// strikethrough and emits `<del>2</del>`.
#[test]
fn super_sub_script_off_leaves_the_source_and_muya_emits_del() {
    let out = render_with(
        "H~2~O and 2^n^\n",
        Options {
            super_sub_script: false,
            ..RENDER
        },
    );
    assert_eq!(out, "<p>H~2~O and 2^n^</p>\n");
    assert!(!out.contains("<sub>") && !out.contains("<sup>"), "{out}");
}

/// The CJK flanking widening is `mt_inline`'s, so it reaches the export for
/// free — which is the whole argument for rendering from the token stream.
/// muya needs the `cjkEmStrong` marked extension to get the same answer here.
#[test]
fn cjk_flanking_reaches_the_export_because_the_editor_and_the_export_share_a_tokenizer() {
    assert!(render("例子例子**\"加粗\"**例子例子\n").contains("<strong>"));
    assert!(render("한국어**[강조]**한국어\n").contains("<strong>"));
    // Additive: what CommonMark rejects is still rejected.
    assert!(!render("a * foo bar*\n").contains("<em>"));
    assert!(!render("a_foo bar_\n").contains("<em>"));
}

/// Math renders as its source until `mt-math` lands — recorded here rather
/// than only in the plan, because it is the reason two `renderToStaticHTML`
/// cases are still in `PENDING`.
#[test]
fn math_renders_as_source_because_there_is_no_tex_renderer_yet() {
    let out = render_with(
        "$$\nx^2\n$$\n",
        Options {
            math: true,
            ..RENDER
        },
    );
    assert_eq!(
        out,
        "<pre class=\"multiple-math\" data-math-style=\"\">x^2</pre>\n"
    );
    assert!(!out.to_lowercase().contains("katex"), "{out}");
}

/// The emoji table is not transcribed, and an unresolved alias renders as its
/// own source in **both** engines — which is what makes the omission invisible
/// to the conformance gate and worth pinning here.
#[test]
fn an_emoji_alias_renders_as_its_own_source() {
    assert_eq!(render(":smile:\n"), "<p>:smile:</p>\n");
    assert_eq!(render(":notanemoji:\n"), "<p>:notanemoji:</p>\n");
}

// ---------------------------------------------------------------------------
// Footnotes, end to end
// ---------------------------------------------------------------------------

#[test]
fn the_footnote_option_renders_the_gfm_pandoc_shape() {
    let out = render_with(
        "See [^1].\n\n[^1]: body\n",
        Options {
            footnote: true,
            ..RENDER
        },
    );
    assert!(
        out.contains("<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup>"),
        "{out}"
    );
    assert!(out.contains("<section class=\"footnotes\">"), "{out}");
    assert!(out.contains("<li id=\"fn-1\">"), "{out}");
    assert!(
        !out.contains("footnote-block"),
        "the wrapper is lifted: {out}"
    );

    // Off, the same source is a definition-shaped paragraph and a shortcut
    // reference link to it — which is §10's "Owed by S1" item 1, measured at
    // S3 and unchanged here.
    let off = render("See [^1].\n\n[^1]: body\n");
    assert!(!off.contains("footnote-ref"), "{off}");
    assert!(off.contains("<a href=\"body\">"), "{off}");
}

// ---------------------------------------------------------------------------
// The helpers, where they carry a rule of their own
// ---------------------------------------------------------------------------

/// `escapeHtmlEntities`'s two modes. The lookahead in the non-encoding one is
/// why `&amp;` survives a paragraph and `&` does not, and it is the difference
/// between a text token and a code span.
#[test]
fn escape_html_reproduces_both_of_markeds_modes() {
    assert_eq!(escape_html("a & b", false), "a &amp; b");
    assert_eq!(escape_html("&amp;", false), "&amp;");
    assert_eq!(escape_html("&amp;", true), "&amp;amp;");
    assert_eq!(
        escape_html("&#35; &#Xa1; &nbsp;", false),
        "&#35; &#Xa1; &nbsp;"
    );
    // Eight digits is past `#\d{1,7}`, so this one is not a reference.
    assert_eq!(escape_html("&#12345678;", false), "&amp;#12345678;");
    assert_eq!(escape_html("&copy", false), "&amp;copy");
    assert_eq!(
        escape_html("<a>\"x\"</a>", false),
        "&lt;a&gt;&quot;x&quot;&lt;/a&gt;"
    );
}

/// `cleanUrl` is `encodeURI` with `%25` folded back to `%`, which is why a URL
/// that already contains a percent-escape is not double-encoded.
#[test]
fn clean_url_is_encode_uri_with_percent_folded_back() {
    assert_eq!(clean_url("/a b").as_deref(), Some("/a%20b"));
    assert_eq!(clean_url("/a%20b").as_deref(), Some("/a%20b"));
    assert_eq!(
        clean_url("https://x/?a=1&b=2#f").as_deref(),
        Some("https://x/?a=1&b=2#f")
    );
    assert_eq!(clean_url("/\u{e9}").as_deref(), Some("/%C3%A9"));
}

/// CommonMark §6.1's code-span normalisation, which `Tokenizer.codespan` does
/// and `commonMarkRules.inline_code` does not — so the renderer is where it
/// has to happen.
#[test]
fn a_code_span_collapses_newlines_and_strips_one_space_each_side() {
    assert_eq!(code_span_text("foo"), "foo");
    assert_eq!(code_span_text(" foo "), "foo");
    assert_eq!(code_span_text("  foo  "), " foo ");
    assert_eq!(code_span_text(" "), " ");
    assert_eq!(code_span_text("  "), "  ");
    assert_eq!(code_span_text("a\nb"), "a b");
}

/// The predicate that makes `Renderer.def` reachable from a model in which a
/// definition is a `Paragraph`. It asks `pulldown-cmark` rather than
/// re-deriving `marked`'s rule, so it cannot drift from the parse that created
/// the paragraph.
#[test]
fn a_reference_definition_paragraph_is_recognised_and_a_lookalike_is_not() {
    assert!(is_reference_definition("[foo]: /url"));
    assert!(is_reference_definition("[foo]: /url \"title\""));
    assert!(is_reference_definition("[a]: /x\n[b]: /y"));
    // Not a definition: no destination, so `pulldown-cmark` keeps a paragraph.
    assert!(!is_reference_definition("[foo]:"));
    assert!(!is_reference_definition("[foo] bar"));
    assert!(!is_reference_definition("text"));
    assert!(!is_reference_definition(""));
}

/// A `<hN>`'s text still carries its `#` marker in this model, so the heading
/// is the one leaf rendered with begin rules on — and a paragraph is not,
/// which is what stops `\#` losing its escape.
#[test]
fn only_a_heading_is_tokenized_with_begin_rules() {
    assert_eq!(render("# a *b*\n"), "<h1>a <em>b</em></h1>\n");
    assert_eq!(render("###### a\n"), "<h6>a</h6>\n");
    // The trailing `#`s of a closed atx heading are not content.
    assert_eq!(render("# a #\n"), "<h1>a</h1>\n");
    // A paragraph that merely starts with a `#` keeps it.
    assert_eq!(render("\\# a\n"), "<p># a</p>\n");
}

/// A `table.row` or `table.cell` outside a table emits nothing, which is the
/// choice `serialize` already made for the same shape and for the same reason:
/// `parse` cannot build one, and a crash is not a behaviour worth porting.
#[test]
fn a_stray_row_or_cell_emits_nothing() {
    use mt_doc::{Align, Edit, Text};
    let mut doc = Document::new();
    let seed = doc.children(doc.root())[0];
    doc.apply(&[Edit::RemoveNode { node: seed }]);
    doc.prune_detached();
    let root = doc.root();
    doc.apply(&[Edit::InsertNode {
        parent: root,
        index: 0,
        block: Block::TableRow {
            children: Vec::new(),
        },
    }]);
    doc.apply(&[Edit::InsertNode {
        parent: root,
        index: 1,
        block: Block::TableCell {
            align: Align::None,
            text: Text::from("x"),
        },
    }]);
    assert_eq!(to_html(&doc, &Labels::new(), RENDER), "");
}
