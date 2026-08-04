//! `renderToStaticHTML.spec.ts` — 20 cases.
//!
//! `renderToStaticHTML` is the synchronous markdown → HTML API the CommonMark
//! and GFM spec runners call, and §4 C3 is the correction that matters here:
//! **it takes a markdown string, not a `Document`.** §4's table says
//! `Document → HTML`; the function never constructs a `TState[]` and never
//! touches `MarkdownToState`.
//!
//! §4 C3 decides the port renders **through `Document`** anyway, which
//! guarantees the conformance number differs from muya's — see §5 D5, the
//! one-way door S5 opens. These cases are the behavioural contract that has to
//! survive that choice.
//!
//! `sanitize` is a separate argument rather than an option because the spec
//! runners pass `false` deliberately: they measure the *parser*, and
//! CommonMark §6.9 explicitly tests that an unknown tag like `<bab>` survives,
//! which a sanitizer would strip. §6 S5 puts `ammonia` on the `true` path.

#![allow(unused_imports)]
use crate::*;

spec_cases! { "render_to_static_html",

fn renders_a_simple_paragraph() {
    let out = html("Hello, world!", RENDER_DEFAULT, true);
    assert!(out.contains("<p>Hello, world!</p>"), "{out:?}");
}

/// **Id-less** headings. The slug-bearing `<hN id="…">` form is
/// `MarkdownToHtml`'s, which is `mt-export`'s at M6 — see this suite's module
/// docs on why `parityExportHtml` is out of scope.
fn renders_headings_with_id_less_h_tags() {
    let out = html("# Heading 1\n\n## Heading 2", RENDER_DEFAULT, true);
    assert!(out.contains("<h1>Heading 1</h1>"), "{out:?}");
    assert!(out.contains("<h2>Heading 2</h2>"), "{out:?}");
}

fn renders_bullet_and_ordered_lists() {
    let bullet = html("- a\n- b", RENDER_DEFAULT, true);
    assert!(bullet.contains("<ul>"), "{bullet:?}");
    assert!(bullet.contains("<li>a</li>"), "{bullet:?}");
    assert!(bullet.contains("<li>b</li>"), "{bullet:?}");

    let ordered = html("1. one\n2. two", RENDER_DEFAULT, true);
    assert!(ordered.contains("<ol>"), "{ordered:?}");
    assert!(ordered.contains("<li>one</li>"), "{ordered:?}");
    assert!(ordered.contains("<li>two</li>"), "{ordered:?}");
}

fn renders_fenced_code_blocks_with_a_language_class() {
    let out = html("```js\nconsole.log(1);\n```", RENDER_DEFAULT, true);
    assert!(out.contains("<pre>"), "{out:?}");
    assert!(out.contains("language-js"), "{out:?}");
    assert!(out.contains("console"), "{out:?}");
}

/// The synchronous renderer must never invoke the async mermaid rewriter: the
/// output stays a static `<pre><code class="language-mermaid">` with the
/// source escaped as plain text.
fn renders_mermaid_code_blocks_as_inert_placeholders() {
    let out = html("```mermaid\ngraph LR; A-->B\n```", RENDER_DEFAULT, true);
    assert!(out.contains("language-mermaid"), "{out:?}");
    assert!(out.contains("graph LR"), "{out:?}");
    assert!(!out.contains("<graph"), "the angle brackets are escaped: {out:?}");
    assert!(
        !out.contains("aria-roledescription=\"flowchart"),
        "no mermaid SVG — the renderer was not invoked: {out:?}"
    );
}

fn renders_vega_lite_and_plantuml_code_blocks_as_inert_placeholders() {
    let vega = html("```vega-lite\n{\"mark\":\"bar\"}\n```", RENDER_DEFAULT, true);
    assert!(vega.contains("language-vega-lite"), "{vega:?}");

    let puml = html(
        "```plantuml\n@startuml\nA -> B\n@enduml\n```",
        RENDER_DEFAULT,
        true,
    );
    assert!(puml.contains("language-plantuml"), "{puml:?}");
}

/// The sanitize path. muya uses DOMPurify; §6 S5 uses `ammonia`.
fn strips_inline_event_handler_attributes() {
    let out = html("<a href=\"x\" onclick=\"alert(1)\">x</a>", RENDER_DEFAULT, true);
    assert!(!out.contains("onclick"), "{out:?}");
    assert!(!out.contains("alert(1)"), "{out:?}");
}

fn strips_script_tags() {
    let out = html("before\n\n<script>alert(1)</script>\n\nafter", RENDER_DEFAULT, true);
    assert!(!out.to_lowercase().contains("<script"), "{out:?}");
    assert!(out.contains("before"), "{out:?}");
    assert!(out.contains("after"), "{out:?}");
}

/// `MarkdownToHtml.renderHtml` wraps its output in
/// `<article class="markdown-body">`; this must not, because the spec runner
/// compares raw block-level HTML. It is also the sharpest statement of why the
/// `MarkdownToHtml` specs are out of M2's scope.
fn returns_the_bare_body_html_with_no_article_wrapper() {
    let out = html("paragraph", RENDER_DEFAULT, true);
    assert!(!out.contains("<article"), "{out:?}");
    assert!(!out.contains("markdown-body"), "{out:?}");
}

/// `superSubScript` is **on** by default in this entry point, unlike
/// `MarkdownToState`'s options.
fn honours_super_sub_script() {
    let on = html("H~2~O and 2^n^", RENDER_DEFAULT, true);
    assert!(on.contains("<sub>2</sub>"), "{on:?}");
    assert!(on.contains("<sup>n</sup>"), "{on:?}");

    let off = html(
        "H~2~O and 2^n^",
        Options { super_sub_script: false, ..RENDER_DEFAULT },
        true,
    );
    assert!(!off.contains("<sub>"), "{off:?}");
    assert!(!off.contains("<sup>"), "{off:?}");
}

/// marktext `b8e2cd82` "Fix inline html renderer", defensively. muya's
/// `superSubscript.ts` wires the renderer directly, so inline rendering and
/// HTML export share one emitter and there is no separate text renderer to
/// keep in sync.
fn emits_sup_and_sub_wrappers_mixed_with_surrounding_text() {
    let out = html("water H~2~O and exp 2^n^ done", RENDER_DEFAULT, true);
    assert!(out.contains("<sub>2</sub>"), "{out:?}");
    assert!(out.contains("<sup>n</sup>"), "{out:?}");
    assert!(
        out.contains("H<sub>2</sub>O") && out.contains("2<sup>n</sup>"),
        "both wrappers must coexist in the same paragraph: {out:?}"
    );
}

fn emits_sup_and_sub_wrappers_inside_list_items_and_headings() {
    let out = html("# title H~2~O text\n\n- exp 2^n^ items", RENDER_DEFAULT, true);
    assert!(out.contains("<h1>"), "{out:?}");
    assert!(out.contains("H<sub>2</sub>O"), "{out:?}");
    assert!(out.contains("<li>"), "{out:?}");
    assert!(out.contains("2<sup>n</sup>"), "{out:?}");
}

/// `walkTokens` rewrites the code token to `multiplemath` only when **both**
/// `math` and `isGitlabCompatibilityEnabled` are true; without the second the
/// block stays a plain fence with language `math` and no KaTeX.
fn honours_gitlab_compatibility_for_math_fences() {
    let source = "```math\nx^2\n```";
    let gitlab = html(
        source,
        Options { math: true, gitlab_compatibility: true, ..RENDER_DEFAULT },
        true,
    );
    let lowered = gitlab.to_lowercase();
    assert!(
        lowered.contains("katex") || lowered.contains("<math"),
        "{gitlab:?}"
    );

    let strict = html(
        source,
        Options { math: true, gitlab_compatibility: false, ..RENDER_DEFAULT },
        true,
    );
    let lowered = strict.to_lowercase();
    assert!(
        !lowered.contains("katex") && !lowered.contains("<math"),
        "{strict:?}"
    );
    assert!(strict.contains("x^2"), "{strict:?}");
}

/// With `frontMatter` off the `---` fence is an hr / setext underline to
/// marked; with it on the YAML block is consumed by the front-matter renderer
/// and removed from the body, and the body after it still renders.
fn honours_the_front_matter_option() {
    let source = "---\ntitle: Hello\n---\n\n# Body";
    let off = html(source, RENDER_DEFAULT, true).to_lowercase();
    assert!(!off.contains("front-matter") && !off.contains("frontmatter"), "{off:?}");

    let on = html(source, Options { front_matter: true, ..RENDER_DEFAULT }, true);
    let lowered = on.to_lowercase();
    assert!(lowered.contains("front-matter") || lowered.contains("frontmatter"), "{on:?}");
    assert!(on.contains("<h1>Body</h1>"), "{on:?}");
}

/// Off by default: the definition parses as a paragraph / link reference and
/// the inline `[^1]` stays literal, with no backref section. On, PR-8c
/// post-processes the marked extension's output into the GFM/pandoc shape and
/// the intermediate `<div class="footnote-block">` wrapper is lifted away.
fn honours_the_footnote_option() {
    let source = "See [^1].\n\n[^1]: footnote body";
    let off = html(source, RENDER_DEFAULT, true);
    assert!(!off.contains("<section class=\"footnotes\">"), "{off:?}");
    assert!(!off.contains("class=\"footnote-ref\""), "{off:?}");

    let on = html(source, Options { footnote: true, ..RENDER_DEFAULT }, true);
    assert!(
        on.contains("<sup class=\"footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup>"),
        "{on:?}"
    );
    assert!(on.contains("<section class=\"footnotes\">"), "{on:?}");
    assert!(on.contains("</section>"), "{on:?}");
    assert!(on.contains("<li id=\"fn-1\">"), "{on:?}");
    assert!(on.contains("<a href=\"#fnref-1\" class=\"footnote-backref\">"), "{on:?}");
    assert!(on.contains("footnote body"), "{on:?}");
    assert!(!on.contains("<div class=\"footnote-block\""), "{on:?}");
}

fn honours_the_math_option() {
    let out = html("$$\nx^2\n$$", Options { math: true, ..RENDER_DEFAULT }, true);
    let lowered = out.to_lowercase();
    assert!(lowered.contains("katex") || lowered.contains("<math"), "{out:?}");
}

fn returns_a_string_for_empty_input() {
    assert_eq!(html("", RENDER_DEFAULT, true), "");
}

/// `sanitize: false` exists so the spec runners can compare against the
/// parser's raw output. CommonMark §6.9 expects the unknown `<bab>` preserved
/// verbatim, and a sanitizer would strip it.
fn preserves_arbitrary_raw_html_tags_when_sanitize_is_false() {
    let out = html("<a><bab><c2c>", RENDER_DEFAULT, false);
    assert!(out.contains("<bab>"), "{out:?}");
    assert!(out.contains("<c2c>"), "{out:?}");
}

/// The danger is exactly the point of the mode, hence the warning on the
/// function. `sanitize: false` must never see user-supplied markdown.
fn does_not_strip_script_when_sanitize_is_false() {
    let out = html("<script>alert(1)</script>", RENDER_DEFAULT, false);
    assert!(out.contains("<script>"), "{out:?}");
}

fn still_strips_script_when_sanitize_is_true() {
    let out = html("<script>alert(1)</script>", RENDER_DEFAULT, true);
    assert!(!out.to_lowercase().contains("<script"), "{out:?}");
}

}
