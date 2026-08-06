//! [`mt_doc::Document`] → static HTML. `packages/muya/src/state/renderToStaticHTML.ts`
//! and the `marked` renderer it reaches through, transcribed.
//!
//! # What this is a port of, and the correction it is built on
//!
//! §4's table says *"`Document` → HTML: port `renderToStaticHTML.ts`"*, and
//! M2.md §4 C3 corrects the direction: `renderToStaticHTML` takes a **markdown
//! string**, hands it to `getHighlightHtml`, and that calls `marked.parse`.
//! muya never builds a `TState[]` on this path and never touches the inline
//! tokenizer M1 ported.
//!
//! **C3 decides the port renders through [`Document`] anyway**, and says in as
//! many words that this *guarantees* the conformance number differs from
//! muya's. This module is where that decision is paid for, and S5 had to take
//! the half of it C3 left open — see below.
//!
//! # The half C3 left open: what renders the inline layer
//!
//! A `Document`'s leaves carry **unparsed inline source** (§4 C2), so
//! `Document → HTML` is two renderers and M2 had neither. The choice was
//! between an inline→HTML pass over [`mt_inline::tokenizer`]'s tokens and a
//! second inline path shaped like `marked`'s `Renderer`.
//!
//! **Decided in favour of `mt_inline`**, and the argument is the same one C3
//! gave for blocks rather than a new one: *the exporter must agree with the
//! editor.* muya reaches that agreement by patching `marked` — `cjkEmStrong`
//! exists only so that `例子**"加粗"**例子` bolds in the export the way it bolds
//! on screen, which is marktext #4307 — and `softBreakExportHtml` (#3676) is
//! the same bug in the other direction. Both are "the two inline engines
//! disagreed". A port that ships a second inline renderer inherits that class
//! of bug permanently and has no consumer for the second engine inside the
//! application; a port that renders from the token stream the editor already
//! holds cannot have it at all.
//!
//! What it costs is stated rather than hidden, because M2.md §5 D5 freezes the
//! number this produces: **`mt_inline` is muya's WYSIWYG tokenizer, not a
//! CommonMark inline parser**, and where the two disagree the port now
//! disagrees with the spec. §6's "S5's re-baseline" enumerates the classes and
//! `spec/conformance.md` carries the per-section rates.
//!
//! # Block shapes are `marked`'s, measured rather than assumed
//!
//! Every block shape below was taken from the running engine through
//! `renderToStaticHTML`, not from reading `Renderer.ts` — the two disagree in
//! one place that matters and would have been missed: **`marked-highlight`
//! replaces the `code` renderer**, so a fenced block ends `</code></pre>` with
//! no trailing newline and an empty info string produces no `class` attribute
//! at all. `Renderer.code` says otherwise and is not what runs.
//!
//! Prism is *not* ported here, and that is a divergence in the port's favour:
//! `getHighlightHtml`'s `highlight()` runs `Prism.highlight` for any language
//! `prismjs` has a grammar for, which rewrites the code block's body into
//! `<span class="token …">` markup that no spec example expects. Those
//! examples fail for muya and pass here. Syntax highlighting belongs to
//! `mt-highlight` (tree-sitter, §9's M3) and to the *editor*, not to a
//! conformance renderer.
//!
//! # What is deliberately not here
//!
//! - **KaTeX.** `getHighlightHtml` builds its math extension with
//!   `useKatexRender: true`; there is no TeX renderer in the workspace until
//!   `mt-math` (§9, M6). Math renders as its source, and M2.md §10 owes the
//!   two `renderToStaticHTML` cases that assert KaTeX markup to that crate.
//!   `Options::SPEC` has `math: false`, so no conformance example reaches it.
//! - **The emoji table.** `emojiExtension` is unconditional in
//!   `getHighlightHtml` and resolves `:smile:` against `config/emojis.ts`'s
//!   1,812 entries. Measured over both fixture files, **zero** examples reach
//!   a valid alias, so the table is owed forward rather than transcribed here;
//!   an unresolved alias renders as its own source in both engines, which is
//!   what makes the omission invisible to the gate and worth writing down.
//! - **A document wrapper, and heading slug ids.** `renderToStaticHTML.spec.ts`
//!   asserts both are *absent*; they are `MarkdownToHtml`'s, which is
//!   `mt-export`'s at M6. M2.md §10 carries the thirteen cases.
//!
//! # One byte C3 costs, and it is worth naming because it is the whole shape
//!
//! §4 C3 says going through `Document` means *"anything `MarkdownToState`
//! normalises away is gone before the renderer sees it"*. The smallest live
//! instance: a `Frontmatter` block's `text` has one trailing whitespace
//! character removed (`block::front_matter_text`), and muya's
//! `frontMatterRender` renders the **token's** text, which still has it. So
//! muya's `<pre class="front-matter">` ends `title: Hello`, newline,
//! `</pre>` and this one ends `title: Hello</pre>`.
//!
//! It is **not** reconstructed, and the reason is the one this crate keeps
//! arriving at: the newline cannot be recovered from the state, because the
//! front-matter regex's closing delimiter is not anchored to a line start —
//! the body of `---⏎foo---⏎⏎` is `"foo"` with no trailing newline at all, so
//! appending one unconditionally would be right in the common case and wrong
//! in the same shape §4 C1's front-matter mechanism exists to reproduce.
//! Rendering the model rather than guessing at the source is the rule
//! everywhere else in this module, and this is where it costs something.
//! `Options::SPEC` has `front_matter: false`, so it costs the conformance
//! number nothing; `tests::front_matter_is_front_matter_renders_shape` pins
//! what the port actually emits.

use std::fmt::Write as _;

use mt_doc::{
    Align, Block, DiagramKind, Document, FrontmatterLang, FrontmatterStyle, MathStyle, NodeId,
};
use mt_inline::{AutoLinkKind, Labels, SyntaxOptions, Token, TokenKind, TokenizerOptions};

use crate::Options;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Render a parsed document to `marked`-shaped HTML.
///
/// `labels` is the document's reference-definition map — [`crate::labels`]'s
/// output — because a reference link is not resolvable from a leaf's text
/// alone.
#[must_use]
pub fn to_html(doc: &Document, labels: &Labels, options: Options) -> String {
    let mut renderer = Renderer {
        doc,
        inline: TokenizerOptions {
            highlights: Vec::new(),
            has_begin_rules: false,
            labels: labels.clone(),
            syntax: SyntaxOptions {
                super_sub_script: options.super_sub_script,
                footnote: options.footnote,
            },
        },
        out: String::new(),
        depth: 0,
    };
    renderer.blocks(doc.children(doc.root()), Looseness::Block);
    renderer.out
}

/// How a [`Block::Paragraph`] child renders.
///
/// `marked` decides this at the *list* level, not per item: a tight list's
/// items hold `text` tokens, which `Parser.parse` renders through
/// `parseInline` with no `<p>`, and a loose list's hold `paragraph` tokens.
/// `Block::BulletList::loose` and its siblings are the same bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Looseness {
    /// Top level, a block quote, a loose item: paragraphs get `<p>…</p>\n`.
    Block,
    /// A tight list item: a paragraph is its inline content and nothing else.
    Tight,
}

struct Renderer<'a> {
    doc: &'a Document,
    /// The tokenizer options for every leaf, built once. `has_begin_rules` is
    /// the one field that varies per call and it is set at the call site.
    inline: TokenizerOptions,
    out: String,
    /// How many [`Renderer::blocks`] frames are open — the depth of the blocks
    /// the innermost one is rendering, in [`crate::MAX_NESTING_DEPTH`]'s unit.
    ///
    /// This is the **deepest-framed** of the crate's three walks: S7 measured
    /// 995 levels on a 1 MiB thread in release and 369 in `dev`, against
    /// `parse`'s 1,229, so it is the walk that fixes the limit.
    depth: usize,
}

impl Renderer<'_> {
    /// The depth clamp lives here rather than in [`Renderer::block`] because
    /// this is the single funnel every container's children pass through —
    /// `blockquote`, `list_item`, `table` and `to_html` itself. Beyond
    /// [`crate::MAX_NESTING_DEPTH`] a container renders its own tags and no
    /// children, which is what [`crate::serialize`] and [`crate::state`] do
    /// with the same document; [`crate::parse`] cannot build one that deep, so
    /// this is reachable only through a hand-built [`Document`].
    fn blocks(&mut self, ids: &[NodeId], looseness: Looseness) {
        // The blocks about to be rendered sit at `self.depth + 1`; see the
        // same arithmetic in `serialize::ExportMarkdown::convert`.
        if self.depth + 1 >= crate::MAX_NESTING_DEPTH {
            // The leaves, at this level — `crate::leaves_below`'s docs carry
            // the argument, and a leaf cannot recurse back into here.
            for id in crate::leaves_below(self.doc, ids) {
                self.block(id, looseness);
            }
            return;
        }
        self.depth += 1;
        for id in ids {
            self.block(*id, looseness);
        }
        self.depth -= 1;
    }

    fn block(&mut self, id: NodeId, looseness: Looseness) {
        let Some(block) = self.doc.block(id) else {
            return;
        };
        match block {
            Block::Paragraph { text } => {
                let text = text.to_str();
                // `Renderer.def` returns `''`. A reference definition is a
                // `Paragraph` in this model (§2's constraint 2), so the
                // renderer has to recognise one — and it asks the same parser
                // the same question rather than re-deriving `marked`'s rule:
                // a text `pulldown-cmark` consumes entirely as link reference
                // definitions produces no events at all.
                if is_reference_definition(&text) {
                    return;
                }
                // **An empty paragraph is not a `marked` token at all.** This
                // model has one wherever the editor needs an editable line
                // that the source does not contain — an empty block quote
                // (`>`), an empty list item (`*` on its own), the placeholder
                // `Document::new` seeds — and `MarkdownToState` puts it there
                // on purpose. `marked` has no `paragraph` token for any of
                // them, so it emits nothing, and this reproduces that rather
                // than emitting `<p></p>`.
                //
                // It is a rule about the *model*, not about whitespace: a
                // paragraph whose text is a run of spaces still renders, because
                // `MarkdownToState` only ever produces the empty string here.
                if text.is_empty() {
                    return;
                }
                match looseness {
                    Looseness::Block => {
                        self.out.push_str("<p>");
                        self.inline(&text, false);
                        self.out.push_str("</p>\n");
                    }
                    Looseness::Tight => self.inline(&text, false),
                }
            }

            // The one leaf whose `text` still carries its marker: muya's state
            // for `# h` is `text: "# h"`, measured. `has_begin_rules` is what
            // strips it, which is exactly what `beginRules.header` is for.
            Block::AtxHeading { level, text } => {
                let (level, text) = (*level, text.to_str());
                let _ = write!(self.out, "<h{level}>");
                self.inline(&text, true);
                let _ = writeln!(self.out, "</h{level}>");
            }
            Block::SetextHeading { level, text, .. } => {
                let (level, text) = (*level, text.to_str());
                let _ = write!(self.out, "<h{level}>");
                self.inline(&text, false);
                let _ = writeln!(self.out, "</h{level}>");
            }

            Block::ThematicBreak { .. } => self.out.push_str("<hr>\n"),

            Block::CodeBlock { info, text, .. } => {
                self.code_block(first_word(info), &text.to_str());
            }
            // A diagram fence is an inert `<pre><code class="language-…">` on
            // this path: `renderToStaticHTML` is the *synchronous* renderer and
            // never invokes the async diagram pass, so `highlight()` returns
            // the source unchanged for every `DIAGRAM_TYPE`.
            Block::Diagram { kind, text, .. } => {
                self.code_block(diagram_lang(*kind), &text.to_str());
            }

            // `Renderer.html` is `text` verbatim; sanitization is the caller's.
            // The state's text is trimmed (§4 C2), and `marked`'s html token
            // ends with the newline that closed the block.
            Block::HtmlBlock { text } => {
                self.out.push_str(&text.to_str());
                self.out.push('\n');
            }

            // muya's own non-KaTeX math renderer, which is what `lexBlock`
            // uses. `getHighlightHtml` uses the KaTeX one; see the module docs.
            Block::MathBlock { style, text } => {
                let _ = writeln!(
                    self.out,
                    "<pre class=\"multiple-math\" data-math-style=\"{}\">{}</pre>",
                    math_style(*style),
                    escape_html(&text.to_str(), true)
                );
            }

            // `frontMatterRender`, verbatim — including the newline after the
            // open tag and the absence of one before `</pre>`.
            Block::Frontmatter { lang, style, text } => {
                let _ = writeln!(
                    self.out,
                    "<pre class=\"front-matter\" data-style=\"{}\" data-lang=\"{}\">\n{}</pre>",
                    front_matter_style(*style),
                    front_matter_lang(*lang),
                    escape_html(&text.to_str(), true)
                );
            }

            Block::BlockQuote { children } => {
                let children = children.clone();
                self.out.push_str("<blockquote>\n");
                self.blocks(&children, Looseness::Block);
                self.out.push_str("</blockquote>\n");
            }

            Block::BulletList {
                loose, children, ..
            }
            | Block::TaskList {
                loose, children, ..
            } => {
                let (loose, children) = (*loose, children.clone());
                self.out.push_str("<ul>\n");
                self.items(&children, loose);
                self.out.push_str("</ul>\n");
            }

            Block::OrderList {
                start,
                loose,
                children,
                ..
            } => {
                let (start, loose, children) = (*start, *loose, children.clone());
                if start == 1 {
                    self.out.push_str("<ol>\n");
                } else {
                    let _ = writeln!(self.out, "<ol start=\"{start}\">");
                }
                self.items(&children, loose);
                self.out.push_str("</ol>\n");
            }

            Block::ListItem { children } => {
                // Reached only when an item is not under a list, which `parse`
                // cannot build; `serialize` makes the same choice for the same
                // shape. Render it as a tight item rather than dropping it.
                let children = children.clone();
                self.list_item(&children, Looseness::Tight, None);
            }
            Block::TaskListItem { checked, children } => {
                let (checked, children) = (*checked, children.clone());
                self.list_item(&children, Looseness::Tight, Some(checked));
            }

            Block::Table { children } => {
                let rows = children.clone();
                self.table(&rows);
            }
            // A row or a cell outside a table: `stateToMarkdown` warns and
            // emits nothing for the same shape, and so does this.
            Block::TableRow { .. } | Block::TableCell { .. } => {}

            Block::Footnote {
                identifier,
                children,
            } => {
                let (identifier, children) = (identifier.clone(), children.clone());
                let _ = write!(
                    self.out,
                    "<div class=\"footnote-block\" data-identifier=\"{}\">",
                    escape_attr(&identifier)
                );
                self.blocks(&children, Looseness::Block);
                self.out.push_str("</div>\n");
            }
        }
    }

    fn items(&mut self, ids: &[NodeId], loose: bool) {
        let looseness = if loose {
            Looseness::Block
        } else {
            Looseness::Tight
        };
        for id in ids {
            let checked = match self.doc.block(*id) {
                Some(Block::TaskListItem { checked, .. }) => Some(*checked),
                _ => None,
            };
            let children = self.doc.children(*id).to_vec();
            self.list_item(&children, looseness, checked);
        }
    }

    /// `Renderer.listitem` plus `Renderer.checkbox`, which `Parser.parse`
    /// emits from the item's own token list rather than from the renderer.
    fn list_item(&mut self, children: &[NodeId], looseness: Looseness, checked: Option<bool>) {
        self.out.push_str("<li>");
        if let Some(checked) = checked {
            self.out.push_str("<input ");
            if checked {
                self.out.push_str("checked=\"\" ");
            }
            self.out.push_str("disabled=\"\" type=\"checkbox\"> ");
        }
        self.blocks(children, looseness);
        self.out.push_str("</li>\n");
    }

    /// `Renderer.table` + `tablerow` + `tablecell`. The first row is the
    /// header, which is how `Block::Table` stores it.
    fn table(&mut self, rows: &[NodeId]) {
        let Some((header, body)) = rows.split_first() else {
            return;
        };
        self.out.push_str("<table>\n<thead>\n");
        self.table_row(*header, true);
        self.out.push_str("</thead>\n");
        if !body.is_empty() {
            self.out.push_str("<tbody>");
            for row in body {
                self.table_row(*row, false);
            }
            self.out.push_str("</tbody>");
        }
        self.out.push_str("</table>\n");
    }

    fn table_row(&mut self, row: NodeId, header: bool) {
        let cells = self.doc.children(row).to_vec();
        self.out.push_str("<tr>\n");
        for cell in cells {
            let Some(Block::TableCell { align, text }) = self.doc.block(cell) else {
                continue;
            };
            let (align, text) = (*align, text.to_str());
            let tag = if header { "th" } else { "td" };
            match align_attr(align) {
                Some(value) => {
                    let _ = write!(self.out, "<{tag} align=\"{value}\">");
                }
                None => {
                    let _ = write!(self.out, "<{tag}>");
                }
            }
            self.inline(&text, false);
            let _ = writeln!(self.out, "</{tag}>");
        }
        self.out.push_str("</tr>\n");
    }

    /// `marked-highlight`'s `code` renderer, which replaces `Renderer.code`.
    ///
    /// Three details that reading `Renderer.ts` would get wrong, all measured:
    /// an empty info string yields **no `class` attribute** rather than
    /// `<pre><code>`-with-a-class; the body's single trailing newline is
    /// stripped and one is added back; and there is **no newline after
    /// `</pre>`**.
    fn code_block(&mut self, lang: &str, text: &str) {
        if lang.is_empty() {
            self.out.push_str("<pre><code>");
        } else {
            let _ = write!(
                self.out,
                "<pre><code class=\"language-{}\">",
                escape_html(lang, false)
            );
        }
        let body = text.strip_suffix('\n').unwrap_or(text);
        self.out.push_str(&escape_html(body, true));
        self.out.push_str("\n</code></pre>");
    }

    // -----------------------------------------------------------------------
    // The inline layer
    // -----------------------------------------------------------------------

    /// Tokenize one leaf's text with [`mt_inline`] and render the tokens.
    ///
    /// `has_begin_rules` is on for exactly one caller — an atx heading, whose
    /// `text` still carries its `#` marker — and off everywhere else, because
    /// every other leaf's text has had its markers removed by the block layer
    /// and a paragraph beginning `#` would otherwise lose it.
    fn inline(&mut self, text: &str, has_begin_rules: bool) {
        self.inline.has_begin_rules = has_begin_rules;
        let tokens = mt_inline::tokenizer(text, &self.inline);
        self.tokens(&tokens, text);
    }

    fn tokens(&mut self, tokens: &[Token], src: &str) {
        for token in tokens {
            self.token(token, src);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per wire type; splitting it would hide the 1:1 mapping"
    )]
    fn token(&mut self, token: &Token, src: &str) {
        let raw = || token.raw.of(src);
        match &token.kind {
            // `beginRules.header` matches only the `#`s and the space after
            // them, so the heading's content is the tokens that follow.
            TokenKind::Header(_) | TokenKind::TailHeader { .. } => {}
            // The other three begin rules and a reference definition cannot
            // reach this renderer — no leaf whose text starts with one is
            // rendered inline — but a token type with no arm is a silent hole,
            // so they emit their source.
            TokenKind::Hr(_)
            | TokenKind::CodeFence(_)
            | TokenKind::MultipleMath(_)
            | TokenKind::ReferenceDefinition(_) => {
                self.out.push_str(&escape_html(raw(), false));
            }

            TokenKind::Text { .. } => self.out.push_str(&escape_html(raw(), false)),

            // muya's `backlash` token is the `\` alone: the escaped character
            // goes into `pending` and surfaces in the next `text` token, which
            // escapes it. So this emits nothing and `\*` renders as `*`.
            TokenKind::Backlash { .. } => {}

            TokenKind::Strong(emphasis) => {
                self.out.push_str("<strong>");
                self.tokens(&emphasis.children, src);
                self.out.push_str("</strong>");
            }
            TokenKind::Em(emphasis) => {
                self.out.push_str("<em>");
                self.tokens(&emphasis.children, src);
                self.out.push_str("</em>");
            }
            TokenKind::Del(emphasis) => {
                self.out.push_str("<del>");
                self.tokens(&emphasis.children, src);
                self.out.push_str("</del>");
            }

            TokenKind::InlineCode(code) => {
                let _ = write!(
                    self.out,
                    "<code>{}</code>",
                    escape_html(&code_span_text(code.content.of(src)), true)
                );
            }

            // `emojiExtension`'s renderer falls back to `:alias:` whenever
            // `validEmoji` misses, and this port has no table yet — see the
            // module docs for the measurement that makes that acceptable.
            TokenKind::Emoji(_) => self.out.push_str(&escape_html(raw(), false)),

            // `mathExtension` is KaTeX; `mt-math` is M6's. Source, escaped.
            TokenKind::InlineMath(_) => self.out.push_str(&escape_html(raw(), false)),

            TokenKind::SuperSubScript { marker, content } => {
                let tag = if marker.of(src) == "^" { "sup" } else { "sub" };
                let _ = write!(
                    self.out,
                    "<{tag}>{}</{tag}>",
                    escape_html(content.of(src).trim(), false)
                );
            }

            // Left as source so `transform_footnotes` can rewrite it: that
            // pass reads `[^id]` out of the rendered HTML, exactly as muya's
            // does, because the numbering is by *inline* order and only the
            // finished body knows it.
            TokenKind::FootnoteIdentifier { .. } => {
                self.out.push_str(&escape_html(raw(), false));
            }

            TokenKind::Image(image) => {
                let alt = plain_text_of(image.alt.of(src), &self.inline);
                self.image(image.src.of(src), image.title.map(|t| t.of(src)), &alt);
            }
            TokenKind::Link(link) => {
                let href = link.href.of(src);
                let title = link.title.map(|t| t.of(src));
                self.open_link(href, title);
                self.tokens(&link.children, src);
                self.close_link(href);
            }

            // `tryReferenceLink` only produces a token when the label resolves,
            // so there is no unresolved arm to write: an orphan `[a][b]` never
            // becomes one of these.
            TokenKind::ReferenceLink(reference) => {
                let label = self.inline.labels.get(&lookup_key(reference.label.of(src)));
                let (href, title) = match label {
                    Some(label) => (label.href.clone(), label.title.clone()),
                    None => (String::new(), String::new()),
                };
                let title = (!title.is_empty()).then_some(title);
                self.open_link(&href, title.as_deref());
                self.tokens(&reference.children, src);
                self.close_link(&href);
            }
            TokenKind::ReferenceImage(reference) => {
                let label = self.inline.labels.get(&lookup_key(reference.label.of(src)));
                let (href, title) = match label {
                    Some(label) => (label.href.clone(), label.title.clone()),
                    None => (String::new(), String::new()),
                };
                let alt = plain_text_of(reference.alt.of(src), &self.inline);
                let title = (!title.is_empty()).then_some(title);
                self.image(&href, title.as_deref(), &alt);
            }

            // A recognised character reference passes through untouched, which
            // is what `escapeReplaceNoEncode`'s lookahead does for `marked`.
            TokenKind::HtmlEscape { .. } => self.out.push_str(raw()),

            TokenKind::AutoLink(auto) => {
                let (target, href) = if auto.is_link {
                    let target = auto.href.map_or("", |s| s.of(src));
                    (target, target.to_string())
                } else {
                    let target = auto.email.map_or("", |s| s.of(src));
                    (target, format!("mailto:{target}"))
                };
                self.open_link(&href, None);
                self.out.push_str(&escape_html(target, false));
                self.close_link(&href);
            }
            TokenKind::AutoLinkExtension(auto) => {
                let target = auto.target().map_or("", |s| s.of(src));
                let href = match auto.link_type {
                    AutoLinkKind::Www => format!("http://{target}"),
                    AutoLinkKind::Url => target.to_string(),
                    AutoLinkKind::Email => format!("mailto:{target}"),
                };
                self.open_link(&href, None);
                self.out.push_str(&escape_html(target, false));
                self.close_link(&href);
            }

            // `Renderer.html` again, one level down: raw, and the sanitize
            // pass is what makes it safe.
            //
            // A **comment**'s `open_tag` is the whole match and its `content`
            // and `close_tag` are both `None` — the branch is discriminated by
            // the absence of capture 3, not by capture 1 — so the three pushes
            // below emit it exactly once. That is worth a sentence because the
            // first version of this arm emitted it twice and CommonMark #625
            // and #626 were the only things that said so.
            TokenKind::HtmlTag(tag) => {
                self.out.push_str(tag.open_tag.of(src));
                if let Some(children) = token.children() {
                    self.tokens(children, src);
                } else if let Some(content) = tag.content {
                    self.out.push_str(content.of(src));
                }
                if let Some(close) = tag.close_tag {
                    self.out.push_str(close.of(src));
                }
            }

            // #3676: a soft break stays a newline. CommonMark reserves `<br>`
            // for a hard break, and `.mu-content` is `pre-wrap`, so the
            // conformant form is also the one that renders correctly.
            TokenKind::SoftLineBreak { .. } => self.out.push('\n'),
            TokenKind::HardLineBreak { .. } => self.out.push_str("<br>"),
        }
    }

    fn open_link(&mut self, href: &str, title: Option<&str>) {
        let Some(href) = clean_url(href) else {
            return;
        };
        let _ = write!(self.out, "<a href=\"{href}\"");
        if let Some(title) = title.filter(|t| !t.is_empty()) {
            let _ = write!(self.out, " title=\"{}\"", escape_html(title, false));
        }
        self.out.push('>');
    }

    fn close_link(&mut self, href: &str) {
        if clean_url(href).is_some() {
            self.out.push_str("</a>");
        }
    }

    fn image(&mut self, src: &str, title: Option<&str>, alt: &str) {
        let Some(href) = clean_url(src) else {
            self.out.push_str(&escape_html(alt, false));
            return;
        };
        let _ = write!(
            self.out,
            "<img src=\"{href}\" alt=\"{}\"",
            escape_html(alt, false)
        );
        if let Some(title) = title.filter(|t| !t.is_empty()) {
            let _ = write!(self.out, " title=\"{}\"", escape_html(title, false));
        }
        self.out.push('>');
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `Renderer.image`'s `parseInline(tokens, textRenderer)` — the alt text with
/// its inline markup removed.
fn plain_text_of(text: &str, options: &TokenizerOptions) -> String {
    let mut options = options.clone();
    options.has_begin_rules = false;
    let tokens = mt_inline::tokenizer(text, &options);
    mt_inline::tokens_to_plain_text(text, &tokens)
}

/// A paragraph whose whole text `pulldown-cmark` consumes as link reference
/// definitions, which `Renderer.def` renders as the empty string.
///
/// Asking the parser rather than re-deriving `marked`'s `def` rule is what
/// keeps this in step with [`crate::block::parse_blocks`]: the paragraph exists
/// *because* the same parser consumed the same bytes and emitted no events.
fn is_reference_definition(text: &str) -> bool {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('[') {
        return false;
    }
    pulldown_cmark::Parser::new_ext(text, crate::block::cmark_options())
        .next()
        .is_none()
}

/// `getLang` in `marked-highlight`: `(lang || '').match(/\S*/)[0]`.
fn first_word(info: &str) -> &str {
    let end = info.find(char::is_whitespace).unwrap_or(info.len());
    &info[..end]
}

/// CommonMark §6.1's code-span normalisation, which `Tokenizer.codespan` does
/// and `commonMarkRules.inline_code` does not.
///
/// Line endings become spaces, and one leading **and** trailing space is
/// stripped when the content is not all spaces. Both halves are `marked`'s.
fn code_span_text(content: &str) -> String {
    let spaced: String = content
        .chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect();
    let has_non_space = spaced.chars().any(|c| c != ' ');
    if has_non_space && spaced.starts_with(' ') && spaced.ends_with(' ') && spaced.len() >= 2 {
        spaced[1..spaced.len() - 1].to_string()
    } else {
        spaced
    }
}

/// The lookup key for [`mt_inline::Labels`], which is keyed lowercase
/// (CommonMark §6.5 matches link labels case-insensitively).
fn lookup_key(label: &str) -> String {
    label.to_lowercase()
}

/// `escapeHtmlEntities` from `marked`'s `helpers.ts`.
///
/// With `encode`, every `&` is escaped. Without it, an `&` that already opens a
/// recognised character reference is left alone — `escapeTestNoEncode`'s
/// `&(?!(#\d{1,7}|#[Xx][a-fA-F0-9]{1,6}|\w+);)`. That lookahead is why
/// `&amp;` survives a paragraph and `&` does not.
fn escape_html(text: &str, encode: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => out.push_str("&lt;"),
            b'>' => out.push_str("&gt;"),
            b'"' => out.push_str("&quot;"),
            b'\'' => out.push_str("&#39;"),
            b'&' if encode || !opens_character_reference(&text[i..]) => out.push_str("&amp;"),
            _ => {
                let ch = text[i..].chars().next().expect("char boundary");
                out.push(ch);
                i += ch.len_utf8();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The lookahead of `escapeTestNoEncode`, applied to a string starting at `&`.
fn opens_character_reference(rest: &str) -> bool {
    let body = &rest[1..];
    let (digits, radix, max) =
        if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
            (hex, 16, 6)
        } else if let Some(decimal) = body.strip_prefix('#') {
            (decimal, 10, 7)
        } else {
            // `\w+;` — ASCII word characters, and `\w` includes digits and `_`.
            let name: &str = &body[..body
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(body.len())];
            return !name.is_empty() && body[name.len()..].starts_with(';');
        };
    let run: &str = &digits[..digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len())];
    !run.is_empty() && run.len() <= max && digits[run.len()..].starts_with(';')
}

/// The footnote extension's own `escapeAttr`, which is `escape_html` minus the
/// apostrophe.
fn escape_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// `cleanUrl` from `marked`'s `helpers.ts`: `encodeURI(href)` with `%25`
/// folded back to `%`, and `null` when `encodeURI` throws.
///
/// `encodeURI` throws only on a lone surrogate, which a `&str` cannot hold, so
/// the `None` arm is unreachable from a parse — it is kept because
/// `Renderer.link` has a visible behaviour for it (emit the anchor text with no
/// `<a>` at all) and a caller could hand this module a hand-built document.
fn clean_url(href: &str) -> Option<String> {
    /// `encodeURI`'s unreserved set: `A-Za-z0-9` plus these.
    const KEEP: &[u8] = b";,/?:@&=+$-_.!~*'()#";
    let mut out = String::with_capacity(href.len());
    for byte in href.bytes() {
        if byte.is_ascii_alphanumeric() || KEEP.contains(&byte) {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    Some(out.replace("%25", "%"))
}

fn align_attr(align: Align) -> Option<&'static str> {
    match align {
        Align::None => None,
        Align::Left => Some("left"),
        Align::Center => Some("center"),
        Align::Right => Some("right"),
    }
}

fn front_matter_style(style: FrontmatterStyle) -> &'static str {
    match style {
        FrontmatterStyle::Dash => "-",
        FrontmatterStyle::Plus => "+",
        FrontmatterStyle::Semicolon => ";",
        FrontmatterStyle::Brace => "{",
    }
}

fn front_matter_lang(lang: FrontmatterLang) -> &'static str {
    match lang {
        FrontmatterLang::Yaml => "yaml",
        FrontmatterLang::Toml => "toml",
        FrontmatterLang::Json => "json",
    }
}

/// `IMathBlockMeta.mathStyle`, which is `""` for the `$$` form and `"gitlab"`
/// for a ```` ```math ```` fence.
fn math_style(style: MathStyle) -> &'static str {
    match style {
        MathStyle::Default => "",
        MathStyle::Gitlab => "gitlab",
    }
}

/// The info string a diagram fence was opened with, which is the class the
/// inert placeholder carries.
fn diagram_lang(kind: DiagramKind) -> &'static str {
    match kind {
        DiagramKind::Mermaid => "mermaid",
        DiagramKind::PlantUml => "plantuml",
        DiagramKind::VegaLite => "vega-lite",
        DiagramKind::Flowchart => "flowchart",
        DiagramKind::Sequence => "sequence",
    }
}

#[cfg(test)]
#[path = "html/tests.rs"]
mod tests;
