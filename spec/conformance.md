# CommonMark / GFM spec conformance

**Re-baselined at M2 S5 (2026-08-04).** Everything below describes the **Rust**
engine. The file it replaces was copied verbatim from muya at PR-6a
(2026-05-20) and described `getHighlightHtml` — `marked` plus Prism plus
DOMPurify — which is a different pipeline, not an older version of this one.

Re-run via:

```sh
cargo xtask conformance            # the ratchet
cargo xtask conformance --sections # the tables below
```

The runner reads `expected-failures.json` to lock the baseline: any listed
example that starts passing fails the suite (you must delist it), and any
unlisted example must continue to pass. Net result: **compliance can only go
up.** `spec/README.md` has the full decision table.

Both suites call `render_to_static_html(…, sanitize = false)` — they measure
the *parser*, not the sanitiser, and CommonMark §6.9 explicitly tests that an
unknown tag like `<bab>` survives.

## Headline

| Suite | Passed | Total | Pass rate | Floor |
|---|---:|---:|---:|---:|
| CommonMark 0.31 | 463 | 652 | 71.0 % | 71.0 % |
| GFM 0.29-gfm | 476 | 672 | 70.8 % | 70.8 % |

muya scores 87.7 % and 86.3 % on the same fixtures. **The gap is one decision
and it is written down**: docs/M2.md §4 C3 renders HTML through `Document`, and
`crates/mt-md/src/html.rs` renders each leaf's inline layer with
`mt_inline::tokenizer` — muya's own **WYSIWYG** tokenizer, which is what the
editor uses and what M1 verified against the reference engine over 4,264
inputs. `renderToStaticHTML` does not use it; it uses `marked`'s inline parser,
which the editor never sees. So the two numbers measure different engines, and
this one measures the engine the application ships.

Read `expected-failures.json` as a **to-do list for `mt-inline`**, because that
is what it now is: 62 % on CommonMark §6.2 *Emphasis and strong emphasis* is a
statement about what the editor renders, not only about what the exporter
exports.

## Where the residue is

Blocks are `pulldown-cmark` through the mapping layer and agree with
`MarkdownToState` on all 1,344 of docs/M2.md §4 C1's inputs; inlines are the
hand-written regex lexer. CommonMark, split at that seam:

| Layer | Passed | Total | Rate |
|---|---:|---:|---:|
| Blocks — §1 Tabs, §4 leaf blocks, §5 container blocks | 247 | 295 | **83.7 %** |
| Inlines — §2.4/§2.5 escapes and entities, §6 in full | 214 | 355 | **60.3 %** |
| §3 Precedence and §6.0 Inlines | 2 | 2 | 100.0 % |

`Tabs` is 1 / 11 in both suites and ten of those ten are a **fixture** defect
shared with muya: the `commonmark-spec` npm package ships `→` (U+2192) where
the spec means a tab, on *both* sides of the example, so example 1 asks for
`<pre><code>` from a line that is a paragraph in any conforming engine. All ten
were on muya's inherited list too.

## CommonMark 0.31 — pass rate by section

| Section | Passed | Total | Pass rate |
|---|---:|---:|---:|
| Tabs | 1 | 11 | 9.1 % |
| Backslash escapes | 8 | 13 | 61.5 % |
| Entity and numeric character references | 4 | 17 | 23.5 % |
| Precedence | 1 | 1 | 100.0 % |
| Thematic breaks | 18 | 19 | 94.7 % |
| ATX headings | 17 | 18 | 94.4 % |
| Setext headings | 24 | 27 | 88.9 % |
| Indented code blocks | 11 | 12 | 91.7 % |
| Fenced code blocks | 28 | 29 | 96.6 % |
| HTML blocks | 40 | 44 | 90.9 % |
| Link reference definitions | 18 | 27 | 66.7 % |
| Paragraphs | 4 | 8 | 50.0 % |
| Blank lines | 1 | 1 | 100.0 % |
| Block quotes | 23 | 25 | 92.0 % |
| List items | 42 | 48 | 87.5 % |
| Lists | 20 | 26 | 76.9 % |
| Inlines | 1 | 1 | 100.0 % |
| Code spans | 16 | 22 | 72.7 % |
| Emphasis and strong emphasis | 82 | 132 | 62.1 % |
| Links | 56 | 90 | 62.2 % |
| Images | 17 | 22 | 77.3 % |
| Autolinks | 11 | 19 | 57.9 % |
| Raw HTML | 8 | 20 | 40.0 % |
| Hard line breaks | 9 | 15 | 60.0 % |
| Soft line breaks | 1 | 2 | 50.0 % |
| Textual content | 2 | 3 | 66.7 % |

**189 examples fail**, listed in `expected-failures.json`.

## GFM 0.29-gfm — pass rate by section

| Section | Passed | Total | Pass rate |
|---|---:|---:|---:|
| Tabs | 1 | 11 | 9.1 % |
| Precedence | 1 | 1 | 100.0 % |
| Thematic breaks | 18 | 19 | 94.7 % |
| ATX headings | 17 | 18 | 94.4 % |
| Setext headings | 24 | 27 | 88.9 % |
| Indented code blocks | 11 | 12 | 91.7 % |
| Fenced code blocks | 28 | 29 | 96.6 % |
| HTML blocks | 39 | 43 | 90.7 % |
| Link reference definitions | 19 | 28 | 67.9 % |
| Paragraphs | 4 | 8 | 50.0 % |
| Blank lines | 1 | 1 | 100.0 % |
| **Tables (extension)** | 8 | 8 | **100.0 %** |
| Block quotes | 23 | 25 | 92.0 % |
| List items | 42 | 48 | 87.5 % |
| Task list items (extension) | 1 | 2 | 50.0 % |
| Lists | 20 | 26 | 76.9 % |
| Inlines | 1 | 1 | 100.0 % |
| Backslash escapes | 8 | 13 | 61.5 % |
| Entity and numeric character references | 4 | 17 | 23.5 % |
| Code spans | 16 | 22 | 72.7 % |
| Emphasis and strong emphasis | 79 | 131 | 60.3 % |
| **Strikethrough (extension)** | 2 | 2 | **100.0 %** |
| Links | 55 | 87 | 63.2 % |
| Images | 17 | 22 | 77.3 % |
| Autolinks | 11 | 19 | 57.9 % |
| Autolinks (extension) | 6 | 11 | 54.5 % |
| Raw HTML | 8 | 20 | 40.0 % |
| Disallowed Raw HTML (extension) | 0 | 1 | 0.0 % |
| Hard line breaks | 9 | 15 | 60.0 % |
| Soft line breaks | 1 | 2 | 50.0 % |
| Textual content | 2 | 3 | 66.7 % |

**196 examples fail**, listed in `expected-failures.json`.

The two GFM **block** extensions this milestone maps — tables and
strikethrough — are at 100 %, which is the half of the number that comes from
`pulldown-cmark` and the mapping layer rather than from the inline lexer.

## The diff against the inherited list, in both directions

docs/M2.md §5 D5 requires this to be read rather than accepted, and requires
both directions rather than a length.

| | CommonMark | GFM |
|---|---:|---:|
| Inherited (muya's) | 78 | 90 |
| Regenerated | 189 | 196 |
| Kept — both engines fail | 71 | 83 |
| **Removed — muya fails, the port passes** | **7** | **7** |
| Added — muya passes, the port fails | 118 | 113 |

The seven removals are one class in each suite and they are the interesting
direction, because a *length* cannot show them:

| CommonMark | GFM | What |
|---:|---:|---|
| 155, 174 | 125, 143 | an HTML block ended by a blank line, inside and outside a block quote |
| 512 | 520 | `[link [foo [bar]]](/uri)` — nested brackets in link text |
| 524, 536 | 532, 544 | `[foo <bar attr="](baz)">` — a `]` inside a raw HTML attribute value |
| 526, 538 | 534, 546 | an autolink whose URL contains `](` |

docs/M2.md §6's "S5's re-baseline" carries the 231 additions with a named,
measured mechanism for each — which is what D5 means by *argued individually*.

## A known inconsistency, inherited — **fixed here**

The file this replaces said in prose that 80 CommonMark and 92 GFM examples
failed while `expected-failures.json` listed 78 and 90, and named CommonMark 84
and 89 and GFM 54 and 59 as failing where the JSON did not. `spec/README.md`
recorded the discrepancy and said it was worth fixing when this file was
rewritten at M2. It is: every number above is printed by
`cargo xtask conformance --sections`, and the two totals are
`expected-failures.json`'s own lengths.
