# MarkText → Rust: Implementation Plan

**Status:** Draft · **Date:** 2026-08-01
**Scope:** Concrete engineering plan for reimplementing MarkText as a native Rust application.
**Relationship to `NATIVE-REWRITE-PLAN.md`:** that document argued the stack choice and surveyed alternatives. The stack question is settled — this is the build plan. Where the two disagree, this one wins.

---

## 0. The design premise (this changes the estimate)

Reading `packages/muya/src/inlineRenderer/types.ts` and `state/types.ts` reveals that muya is **not** a conventional rich-text engine, and that materially reduces the hard part described in the earlier plan.

Two facts drive the whole design:

**1. Leaf blocks store raw markdown source, not a parsed inline AST.**
`IParagraphState` is `{ name: 'paragraph', text: string }`. So is a heading, a table cell, a code block. Inline structure is *not* stored — it is re-derived by tokenizing `text` on every render pass.

**2. Every inline token preserves its source bytes and its range.**

```ts
export type StrongEmToken = IBaseToken & {
    type: 'strong' | 'em';
    marker: string;      // the literal "**" or "_"
    children: Token[];
    backlash: string;    // trailing backslashes, preserved verbatim
};
export interface IBaseToken {
    raw: string;         // exact source slice
    range: { start: number; end: number };
}
```

Together these mean MarkText's WYSIWYG is **"render the source text with decorations, and reveal the syntax markers when the caret enters their range."** It is not "hide the syntax and edit an opaque model."

The consequences for a Rust implementation are large and all favourable:

- The document is fundamentally **text**, so the buffer, undo, and edit ops are text ops — not tree surgery on a rich-text model.
- Inline layout is **styled runs over a single string plus a handful of replaced elements** (image, inline math, emoji). That is *exactly* `parley`'s data model. There is no arbitrary inline-widget nesting to invent.
- Round-trip fidelity is close to free: serializing a leaf block is emitting its `text`. The lossiness risk lives only in block-level structure.
- The state JSON is a clean discriminated union, so a Rust `enum` maps 1:1 — which unlocks **differential testing against the existing TS engine** (§11.2).

The earlier plan's §3 "hard parts" list still applies to *layout and input* (shaping, bidi, line breaking, IME, a11y, hit-testing). But hard-part 8, "inline-flow layout with arbitrary nested widgets", largely evaporates. **Revised estimate: 20–26 months rather than 34.**

### 0.1 Where the 73k lines actually are

Measured sizes of the semantically hard modules:

| Module | Lines |
|---|---:|
| `inlineRenderer/lexer.ts` | 898 |
| `state/stateToMarkdown.ts` | 529 |
| `editor/index.ts` (op apply, event routing) | 442 |
| `state/markdownToState.ts` | 422 |
| `history/index.ts` | 358 |
| `search/index.ts` | 179 |
| `clipboard/index.ts` | 168 |
| `selection/index.ts` | 166 |
| `inlineRenderer/rules.ts` | 112 |
| `state/htmlToMarkdown.ts` + `renderToStaticHTML.ts` | 141 |
| **Core semantics subtotal** | **~3,400** |
| `muya.ts` (public API + command surface) | 1,471 |

So the genuinely hard, must-be-correct logic is **~5,000 lines**, not 73,000. The remaining ~68k is ~30 block classes and ~20 floating UI components, dominated by DOM/vdom/CSS plumbing that a native design replaces with different — and smaller — scaffolding.

**Port the ~5,000 lines with obsessive care. Redesign the rest.**

---

## 1. Workspace

```
marktext-rs/
├── Cargo.toml                 # workspace, shared profile
├── crates/
│   ├── mt-doc/                # model, buffer, edits, undo         [no I/O, no UI]
│   ├── mt-inline/             # inline tokenizer (port of lexer.ts) [pure]
│   ├── mt-md/                 # markdown ⇄ Document                 [pure]
│   ├── mt-layout/             # block flow + parley inline layout   [no GPU]
│   ├── mt-render/             # display list → pixels
│   ├── mt-highlight/          # tree-sitter code fences
│   ├── mt-math/               # TeX layout
│   ├── mt-diagram/            # native diagram subset
│   ├── mt-export/             # HTML + PDF
│   ├── mt-fs/                 # io, encodings, watch, atomic save
│   ├── mt-search/             # in-doc find/replace + project grep
│   ├── mt-ui/                 # widgets, floats, panes, a11y tree
│   ├── mt-app/                # winit shell, menus, prefs, session
│   └── mt-cli/                # argv, headless convert
├── spec/                      # CommonMark + GFM fixtures (copied verbatim)
├── bench/                     # corpus + criterion harnesses
└── xtask/                     # build/package/dist automation
```

Dependency direction is strictly downward; `mt-doc`, `mt-inline`, `mt-md`, and `mt-layout` never depend on `mt-ui` or `mt-app`. Those four compile and test **without a window**, which is what makes §11 runnable in CI.

```toml
# Cargo.toml
[workspace]
members = ["crates/*", "xtask"]
resolver = "3"

[workspace.package]
edition = "2024"
rust-version = "1.85"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"

[profile.dist]          # size-tuned; see §12
inherits = "release"
opt-level = "s"
```

---

## 2. `mt-doc` — document model

Direct translation of `state/types.ts`. Keeping the shape identical is deliberate: it preserves JSON compatibility for differential testing.

```rust
pub struct Document {
    arena:    Arena<Node>,
    root:     NodeId,
    revision: u64,
    dirty:    DirtySet,     // drives incremental layout
}

pub struct Node {
    parent:   Option<NodeId>,
    block:    Block,
}

pub enum Block {
    // ---- leaves: own text ----
    Paragraph      { text: Text },
    AtxHeading     { level: u8, text: Text },
    SetextHeading  { level: u8, underline: Underline, text: Text },
    ThematicBreak  { text: Text },
    CodeBlock      { kind: CodeKind, info: String, fence_len: Option<u8>, text: Text },
    HtmlBlock      { text: Text },
    MathBlock      { style: MathStyle, text: Text },
    Frontmatter    { lang: FrontmatterLang, style: FrontmatterStyle, text: Text },
    Diagram        { lang: DiagramLang, kind: DiagramKind, text: Text },
    TableCell      { align: Align, text: Text },

    // ---- containers: own children ----
    BlockQuote     { children: Vec<NodeId> },
    BulletList     { marker: BulletMarker, loose: bool, children: Vec<NodeId> },
    OrderList      { start: u32, delimiter: OrderDelim, loose: bool, children: Vec<NodeId> },
    ListItem       { children: Vec<NodeId> },
    TaskList       { marker: BulletMarker, loose: bool, children: Vec<NodeId> },
    TaskListItem   { checked: bool, children: Vec<NodeId> },
    Table          { children: Vec<NodeId> },
    TableRow       { children: Vec<NodeId> },
    Footnote       { identifier: String, children: Vec<NodeId> },
}
```

Notes carried over from the TS source that are easy to lose in translation:

- `CodeBlock::info` is the **full verbatim info string** (`js title="x"`, or a Pandoc `{…}` block). The highlight language is its first word. `packages/muya/src/state/types.ts:34` is explicit that you must never assume a single word.
- **Reference definitions are not a block type.** `[label]: url "title"` round-trips as a `Paragraph` whose text is the raw line. A pass over the tree collects them into a label map for the inline lexer. Reproduce this exactly — `ILinkReferenceDefinitionState` is a deprecated stub, do not implement it.
- `fence_len` must be preserved; there is an upstream fix specifically for code-fence length round-tripping.

### 2.1 Text storage

```rust
pub enum Text {
    Inline(String),   // < 8 KiB — the overwhelming majority
    Rope(Rope),       // ropey, promoted on threshold
}
```

Per-block text keeps edits O(block), not O(document) — a 5 MB file is thousands of small independent buffers. The rope variant exists only so a single enormous code fence doesn't degrade to O(n) inserts.

### 2.2 Edits and undo

```rust
pub enum Edit {
    SpliceText   { node: NodeId, at: usize, remove: usize, insert: String },
    SetMeta      { node: NodeId, meta: BlockMeta },
    InsertNode   { parent: NodeId, index: usize, block: Block },
    RemoveNode   { node: NodeId },
    MoveNode     { node: NodeId, new_parent: NodeId, index: usize },
    ReplaceBlock { node: NodeId, block: Block },
}

impl Document {
    pub fn apply(&mut self, edits: &[Edit]) -> Vec<Edit>;  // returns the inverse
}
```

Every `Edit` is invertible, so undo is a stack of inverse batches — no snapshotting. Batches are grouped into user-visible undo steps by a coalescing policy (time gap + edit-kind change), matching `history/index.ts`.

**On collaborative editing:** muya's `ot-json1` + `ot-text-unicode` layer is *not* carried forward. The `Edit` enum above is deliberately shaped as discrete invertible ops so a CRDT (`loro`) can be slid underneath later without redesigning the model. This is the one deferred-architecture bet in the plan; make it consciously.

---

## 3. `mt-inline` — the tokenizer

A faithful port of `lexer.ts` (898 lines) + `rules.ts` (112 lines). **This is the single highest-fidelity-risk component in the project** and it is small enough to port line by line rather than reinterpret.

```rust
pub struct Token {
    pub kind:       TokenKind,
    pub range:      Range<usize>,   // byte offsets into the block's text
    pub raw:        Range<usize>,   // exact source slice
    pub highlights: SmallVec<[Highlight; 2]>,
}

pub enum TokenKind {
    Text, Backslash { marker: Range<usize> },
    Strong { marker: Marker, children: Vec<Token> },
    Em     { marker: Marker, children: Vec<Token> },
    Del    { marker: Marker, children: Vec<Token> },
    InlineCode { marker: Marker, content: Range<usize> },
    InlineMath { marker: Marker, content: Range<usize> },
    Emoji      { marker: Marker, content: Range<usize> },
    SuperSubScript { marker: Marker, content: Range<usize> },
    FootnoteIdentifier { marker: Marker, content: Range<usize> },
    Image  { attrs: ImageAttrs, backslash: (Range<usize>, Range<usize>) },
    Link   { href: Range<usize>, title: Range<usize>, children: Vec<Token> },
    ReferenceLink  { label: Range<usize>, full: bool, children: Vec<Token> },
    ReferenceImage { label: Range<usize>, full: bool, alt: Range<usize> },
    ReferenceDefinition { /* label, href, title, all markers */ },
    AutoLink { href: Range<usize>, is_email: bool },
    AutoLinkExtension { kind: AutoLinkKind },
    HtmlTag { tag: Range<usize>, attrs: Vec<(Range<usize>, Option<Range<usize>>)>,
              children: Option<Vec<Token>> },
    HtmlEscape { ch: Range<usize> },
    SoftLineBreak { at_end: bool },
    HardLineBreak { spaces: Range<usize>, at_end: bool },
    BeginRule { kind: BeginRuleKind },   // header | hr | code_fence | multiple_math
    TailHeader { marker: Range<usize> },
}
```

Three rules for this port, all non-negotiable:

1. **Ranges are byte offsets, and they must tile the input exactly.** Assert in debug builds that concatenating every token's `raw` reproduces the source byte-for-byte. This single invariant catches most porting errors immediately.
2. **`Marker` and `backslash` spans are retained**, not normalised away. They are what the renderer reveals when the caret is inside a token, and what the serializer re-emits.
3. **Behaviour is defined by the TS tests, not by the spec.** `inlineRenderer/__tests__/` covers autolink trailing punctuation, emoji word boundaries, inline-math escaping, CJK flanking for `strong`, reference-link/image anchors, and link-followed-by-autolink. Port every one of those specs first, then make them pass.

`rules.ts` is regex-driven; use `regex` with pre-compiled `LazyLock<Regex>`. Where a regex is a hot path, replace it with a hand-written scanner **only after** the regex version passes the full suite, and only with the suite as the guard.

### 3.1 Marker reveal — the WYSIWYG behaviour

The rule that makes MarkText feel like MarkText:

```rust
pub enum MarkerState { Hidden, Revealed }

pub fn marker_state(token: &Token, caret: Option<usize>, sel: Option<Range<usize>>)
    -> MarkerState
```

A token's markers reveal when the caret is inside `token.range` (inclusive of edges) or the selection intersects it. Everything else renders decorated. Ancestor tokens reveal when a descendant reveals. Get this exactly right early — it is load-bearing for the product's identity, and it is cheap to test headlessly since it's a pure function of `(tokens, caret)`.

---

## 4. `mt-md` — markdown ⇄ Document

| Direction | Approach |
|---|---|
| Markdown → `Document` | `pulldown-cmark` for block structure only; leaf text is captured as **raw source slices**, never as parsed inline events. Port `markdownToState.ts` (422 lines) semantics on top. |
| `Document` → Markdown | Port `stateToMarkdown.ts` (529 lines). Leaf blocks emit `text` verbatim; containers emit their markers. |
| `Document` → HTML | Port `renderToStaticHTML.ts`, sanitize with `ammonia`. |
| HTML → Markdown (paste) | `html5ever` + a port of turndown's rules; `htmd` evaluated first. |

Using `pulldown-cmark` for blocks only — and never for inlines — is the key integration decision. It gives spec-correct block parsing (`pulldown-cmark` is >99 % CommonMark) while leaving inline handling to `mt-inline`, which must match muya's marker-preserving behaviour rather than the spec's normalised output.

**Contract, property-tested:** `parse(serialize(parse(s))) == parse(s)` for every corpus file and every spec fixture.

### 4.1 Incremental reparse

An edit to a leaf's text usually changes nothing structurally. The reparse path is:

1. Re-tokenize **only the edited block** (`mt-inline`), mark it layout-dirty. This is the common case and must be sub-millisecond.
2. If the edit introduces a block-boundary trigger (blank line, list marker, `#`, fence delimiter, `>`, table pipe, `---`), re-run block parsing over the **minimal enclosing region** — from the previous blank line to the next one — and diff the resulting subtree against the existing one.
3. Full document reparse only on load and on external file change.

---

## 5. `mt-layout` — the real work

Consumes `Document` + theme, produces a positioned display list. No GPU, no windowing — testable headlessly against golden data.

```rust
pub struct LayoutTree {
    blocks:      Vec<BlockLayout>,   // document order, y-sorted
    total_height: f32,
}

pub struct BlockLayout {
    node:     NodeId,
    rect:     Rect,
    content:  BlockContent,
}

pub enum BlockContent {
    Text  { layout: parley::Layout<Brush>, markers: Vec<MarkerSpan>,
            widgets: Vec<InlineWidget> },
    Code  { lines: Vec<parley::Layout<Brush>>, highlights: Vec<HighlightSpan> },
    Table { columns: Vec<f32>, cells: Vec<BlockLayout> },
    Rule, Image { .. }, Math { .. }, Diagram { .. },
}
```

**Inline layout** is `parley`: build a `RangedBuilder` over the block's text, push style ranges derived from the token tree (bold/italic/strike/code/link colour), push `InlineBox` placeholders for images, inline math, and emoji. Parley returns lines, runs, glyph positions, and cluster boundaries — which is simultaneously the shaping, bidi, font-fallback, and line-breaking answer, and the hit-testing and cursor-motion answer. This is the payoff of §0: because inline content is styled runs over a string, parley is a direct fit rather than a component to build around.

**Block flow** is a straightforward vertical stack with per-block-type margins, indentation for quotes and lists, and marker gutters. Tables need two passes: measure natural column widths, then distribute available width (min-content / max-content, capped).

**Incrementality** is the performance contract:

```rust
impl LayoutTree {
    pub fn relayout(&mut self, doc: &Document, dirty: &DirtySet, width: f32);
}
```

A dirty block re-lays out; blocks after it get their `y` shifted by the height delta; nothing else is touched. Re-wrapping the whole document happens only on width change — and even then only for visible blocks plus a viewport margin, with the rest lazily laid out on scroll. **Never lay out the whole document synchronously.** This is precisely what Electron cannot do and is where the 5 MB-file target is won.

**Cursor motion and hit-testing** go through parley's cluster API, so grapheme clusters, bidi runs, and ligatures behave correctly without bespoke Unicode code.

---

## 6. `mt-render`

Display list → pixels. Two backends behind one trait:

| Backend | Crate | Role |
|---|---|---|
| GPU | `vello` on `wgpu` | Default |
| CPU | `tiny-skia` | Fallback for VMs, RDP, old drivers — **a Phase 3 deliverable, not an afterthought** |

Glyph rasterization via `swash`, cached in an atlas keyed by `(font, size, subpixel offset, glyph)`.

**Redraw policy: on event only.** No animation loop, no timers, no idle polling. Caret blink is the sole timer and it stops when the window loses focus. This is the "0 % idle CPU" target, and it is trivially lost by accident.

Dirty-rect rendering: redraw only changed blocks plus the caret rect. Scrolling blits and renders the newly exposed band.

> **Note (§12):** `wgpu` + `vello` is ~4–6 MB of the binary. For a markdown editor, `tiny-skia` may be both smaller and fast enough. Benchmark both in Phase 3 and let the data pick the default.

---

## 7. Input, IME, accessibility

Windowing is `winit`. Three areas that must be built in from the start, never retrofitted:

**IME.** `winit`'s `Ime::{Enabled, Preedit, Commit, Disabled}` events. The preedit string renders as a transient styled run at the caret **without entering the document or the undo stack**; only `Commit` produces an `Edit`. The candidate window is positioned via `set_ime_cursor_area` using the caret rect from `mt-layout`. Verified against Japanese, Korean, and Chinese IMEs by native speakers — `zh-CN`, `zh-TW`, `ja`, and `ko` are shipped locales, so this is a correctness requirement, not a nice-to-have.

**Accessibility.** `accesskit` (UIA on Windows, AT-SPI on Linux, NSAccessibility on macOS). The document maps to a tree of text nodes with character ranges, so screen readers get real text navigation rather than a canvas. Build the tree in the same phase as the caret, and snapshot it in CI from the first commit that has one.

**Keybindings.** The three platform tables in `packages/desktop/src/main/keyboard/keybindings{Darwin,Linux,Windows}.ts` port as **data**, not code — a TOML table keyed by command id, with user overrides layered on top.

---

## 8. `mt-ui`, `mt-app`, and the rest

`mt-ui` is a small retained-mode widget layer — enough for tabs, sidebar file tree, command palette, find/replace bar, preference panes, and the ~20 floating surfaces muya has (inline format toolbar, table tools, image tools, emoji picker, quick-insert menu, language selector, footnote tool). Popup positioning replaces `@floating-ui/dom` in roughly 300 lines. Every widget contributes to the AccessKit tree.

Supporting crates, each a thin wrapper over a mature dependency:

| Crate | Built on | Replaces |
|---|---|---|
| `mt-highlight` | `tree-sitter` + grammars | `prismjs` |
| `mt-math` | `rex`, or MicroTeX via FFI | `katex` |
| `mt-diagram` | `layout-rs` + native flowchart/sequence | `mermaid`, `flowchart.js` |
| `mt-export` | `mt-layout` → `pdf-writer`; HTML from `mt-md` | Chromium `printToPDF` |
| `mt-fs` | `notify`, `encoding_rs`, `chardetng`, `tempfile` | `chokidar`, `ced`, `write-file-atomic` |
| `mt-search` | `ignore` + `grep-searcher` (**linked, not spawned**) | `@vscode/ripgrep` |
| `mt-app` | `winit`, `muda` (menus), `rfd` (dialogs), `keyring`, `serde` | Electron main (9,627 lines) |

PDF export routing through `mt-layout` rather than a separate renderer means export matches the screen **by construction** — a fidelity improvement over the Chromium path, at the cost of implementing the PDF emitter.

---

## 9. Milestones

Sequential critical path is M2 → M3 → M4. Durations assume 3–4 engineers from M2 onward.

| # | Milestone | Duration | Exit gate |
|---|---|---|---|
| **M0** | Workspace, CI, `spec/` imported, `bench/` corpus, differential-test harness (§11.2) skeleton | 3 wks | `cargo test` green in CI on all three platforms |
| **M1** | `mt-inline` — full tokenizer port | 2 mo | Every `inlineRenderer/__tests__` spec passes. Tiling invariant holds on the whole corpus. Fuzzer runs 24 h without a panic. |
| **M2** | `mt-doc` + `mt-md` — model, edits, undo, both directions | 3 mo | **≥ 87.7 % CommonMark / ≥ 86.3 % GFM** (§11.1). Round-trip is a fixed point on corpus + fixtures. Differential test vs. TS engine agrees on 100 % of the corpus. |
| **M3** | `mt-layout` + `mt-render` + `mt-highlight` — read-only viewer | 5 mo | 5 MB doc opens ≤ 800 ms. 60 fps scroll. Bidi, CJK, emoji, ligatures render correctly. CPU fallback works in a VM. **Ships publicly as a standalone fast previewer.** |
| **M4** | Editing — caret, selection, all edit commands, undo, clipboard, IME, AccessKit | 5 mo | p99 keystroke → glyph ≤ 8 ms. NVDA + VoiceOver + Orca pass a scripted read-and-navigate script. CJK IME signed off by native speakers. Marker-reveal behaviour matches muya on a side-by-side script. |
| **M5** | `mt-ui` + `mt-app` + `mt-fs` + `mt-search` — the application | 4 mo | §10 parity checklist green. Preferences, keybindings, and session import from an existing install. |
| **M6** | `mt-export`, `mt-math`, `mt-diagram`, spellcheck | 3 mo | PDF matches screen on the corpus. Math renders the KaTeX test set. Mermaid blocks never render blank. |
| **M7** | Harden, package, beta | 3 mo | All §12 budgets met. Zero data-loss reports in a ≥ 500-user beta. |

**Total ≈ 25 months.** Ship **Windows first** — it removes two-thirds of platform-integration risk from the critical path, and M3's public previewer gets real users ~14 months before v1.

---

## 10. v1 scope

**Ship:** CommonMark + GFM WYSIWYG editing · source-code mode · focus + typewriter modes · tabs · sidebar file tree · find/replace in-document and across a folder · the full paragraph/format command set · tables with row/column tools · images (paste, drag-drop, resize, local paths) · footnotes · front matter · math · TOC · HTML + PDF export · all 72 preference keys · 10 locales · 3 keybinding tables with user overrides · spellcheck · auto-save · file watching · session restore · CLI · theming.

**Defer to v1.1:** mermaid beyond the native subset · PlantUML rendering (the encoder ships in v1 so links resolve) · auto-updater · custom user themes.

**Drop:** Vega / Vega-Lite chart blocks — retained as passthrough code fences so documents round-trip losslessly and the block renders as source rather than vanishing.

**Non-negotiable regardless of schedule:** CJK IME correctness · screen-reader support on all three platforms · lossless markdown round-trip. None of these can be added credibly after launch.

---

## 11. Testing

### 11.1 Conformance ratchet

Copy `packages/muya/test/spec/` into `spec/` unchanged — the CommonMark 0.31 and GFM 0.29 fixtures are language-agnostic JSON. Carry over the ratchet rule from `expected-failures.json`: a listed example that starts passing must be delisted; an unlisted example that starts failing fails CI. **Compliance only goes up.** Floor is the current 87.7 % / 86.3 %; because `pulldown-cmark` handles blocks at >99 %, the realistic target is meaningfully higher and most residual failures will be in block-tree mapping, not parsing.

### 11.2 Differential testing against the TS engine

The highest-value test asset available, and it exists only because §0 keeps the state shape identical.

```
corpus file ──┬──► node harness → @muyajs/core → state JSON ──┐
              │                                                ├──► assert equal
              └──► mt-md (via mt-cli --dump-state) ────────────┘
```

Build this in M0 and run it in CI for the entire project. It converts "did I port the lexer correctly?" from a judgement call into a boolean, across the whole corpus, on every commit. Extend it to serialized markdown and exported HTML as those land.

### 11.3 Remaining layers

| Layer | Approach |
|---|---|
| Round-trip | Property test: `parse ∘ serialize ∘ parse` is a fixed point. |
| Fuzzing | `cargo-fuzz` on `mt-inline` and the HTML-paste path. Malformed input must never panic — `panic = "abort"` makes a panic a crash. |
| Layout | Golden images per block type per platform, perceptual-diff threshold. |
| Latency | `criterion` in CI on fixed hardware; a §12 regression fails the build. |
| IME | Scripted composition sequences per platform + native-speaker sign-off each release. |
| Accessibility | AccessKit tree snapshots in CI + live NVDA/VoiceOver/Orca script each release. |
| Data safety | Kill the process mid-save, repeatedly. No corruption, no truncation, ever. |

---

## 12. Budgets

### 12.1 Performance (Windows x64, `dist` profile)

| Metric | Target | Stretch |
|---|---|---|
| Cold start → editable caret | ≤ 250 ms | ≤ 100 ms |
| Warm start | ≤ 120 ms | ≤ 50 ms |
| RSS, empty document | ≤ 60 MB | ≤ 35 MB |
| RSS, 1 MB document | ≤ 120 MB | ≤ 70 MB |
| Open 5 MB document | ≤ 800 ms | ≤ 300 ms |
| Keystroke → glyph, p99 | ≤ 8 ms | ≤ 4 ms |
| Idle CPU, focused | ~0 % | 0 % |

### 12.2 Binary size

**Measured at M3 S0, 2026-08-10**, replacing the estimates this section carried until then. `dist` profile as specified in §1 — `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`, `opt-level = "s"` — on `x86_64-pc-windows-msvc`, rustc 1.97.1. Sizes are the stripped artifact. Evidence and per-crate `cargo bloat` output: `spikes/results/e2-binary-size.md`.

Each row is a whole-binary delta against the row above it or against the floor, because that is the only attribution that survives — see the warning at the end of this section.

| Component | Size | Provenance |
|---|---:|---|
| Rust std + runtime (empty `fn main`) | **0.098 MB** | measured |
| `parley` + `vello_cpu` + skrifa + harfrust + ICU | **2.383 MB** | measured |
| `vello` + `wgpu` + `naga`, on top of the above | **4.318 MB** | measured |
| `tiny-skia`, on top of the above | 0.390 MB | measured — **and it renders no text** |
| tree-sitter core + **1** grammar | **1.180 MB** | measured |
| tree-sitter, each additional grammar | **~1.301 MB** | measured (20-grammar build) |
| `syntect` 5.3.0 | **1.000 MB** | measured |
| parley `complex-scripts` feature (CJK/Thai/Khmer/Lao/Myanmar line breaking) | **3.6 MB** | measured — **not previously budgeted at all** |
| `ignore` + `grep-searcher` | **1.289 MB** | measured |
| `accesskit` core, no platform adapter | 0.016 MB | measured — floor only, adapters unpriced |
| Bundled fonts | **1.178 MB** for 4 mono faces | measured — see the caveat below |
| `rex` (math), `notify`, `pdf-writer`, `spellbook`, icons, themes, locales | — | **still unmeasured**; the crates belong to M5/M6 |

**M3's own configuration, as decided at S0** (CPU-only renderer, Prism-port highlighter, no tree-sitter): **≈ 2.5 MB before fonts and `complex-scripts`, ≈ 7.3 MB with both.** That is the whole shipped previewer.

### Both levers were wrong, in opposite directions

- **"Drop `wgpu`/`vello`, ship `tiny-skia` only: −4–6 MB."** The magnitude is right — **measured −4.318 MB** — but the mechanism is impossible. `tiny-skia`'s README lists text rendering as out of scope, so a `tiny-skia`-only build cannot draw a character of a text editor's content. The real lever is *ship `vello_cpu` only*, which did not exist when this section was written (`vello_cpu` 0.2.0, 2026-08-07). **M3 takes it.**
- **"Load tree-sitter grammars on demand: −3–4 MB."** Understated by **6–8×**. Twenty grammars measure **25.907 MB**, ~1.301 MB each — more than this section's entire *default total*. And it is not a flag: grammar crates compile checked-in generated C via `build.rs` (`tree-sitter-rust` ships a 6.5 MB `parser.c`) and link statically, so "on demand" means shipping dynamic libraries or embedding a wasm engine. M3 S0 dropped tree-sitter instead; see M3.md D3.

**A methodological warning.** `cargo bloat --crates` **under-reports tree-sitter by an order of magnitude**, because parse tables live in `.rdata` and `cargo bloat` attributes `.text`. Grammar cost is only recoverable from whole-binary deltas. Any future re-measurement that trusts `cargo bloat` alone will conclude grammars are nearly free.

**A caveat on the font row.** The 1.178 MB is MarkText's four bundled DejaVu Sans Mono `.ttf` files. Its eight Open Sans faces ship as `.woff`, which skrifa **rejects silently**, and the bundled set covers no Hebrew, CJK or emoji. A font row that satisfies M3's own exit gate is therefore larger than measured and not yet sized — see M3.md D7.

Treat this table as the scoreboard it now is: rows marked measured are facts about this repository at a stated commit; the remaining rows are still estimates and are marked as such rather than totalled into a headline number.

---

## 13. Rust-track risks

| ID | Risk | Mitigation |
|---|---|---|
| **R1** | Lexer port diverges subtly; markdown renders *almost* right | §11.2 differential testing from M0. The tiling invariant. Port the TS test files before the implementation. |
| **R2** | `parley` lacks something the design needs (inline widget baseline alignment, nested bidi + inline box interaction) | Exercise the awkward cases in M3 week 1, not month 5. Linebender is responsive; budget for upstream contributions. |
| **R3** | Native math (`rex`) falls short of KaTeX | Evaluate against KaTeX's own test set during M1, in parallel with the lexer. Fallback: MicroTeX via FFI, accepting a C++ dependency. |
| **R4** | Mermaid has no native equivalent | Scoped as deferred (§10). Native flowchart + sequence subset; everything else is an optional `mmdc` passthrough with a clear message. **Never render a mermaid block blank.** |
| **R5** | IME defects found late | M4 gate criterion, not polish. Recruit native-speaker testers at M4 start. |
| **R6** | Accessibility retrofitted | AccessKit tree built alongside the caret in M4. CI snapshots from day one. |
| **R7** | GPU rendering breaks on VMs / RDP / old drivers | `tiny-skia` fallback is an M3 deliverable. VM test in CI. |
| **R8** | 25 months with nothing shipped; momentum dies | M3 ships publicly as a standalone previewer. Real users, real bugs, ~14 months early. |
| **R9** | Upstream MarkText keeps moving | Freeze parity at a specific upstream tag at M2 start. New upstream features are post-v1 backlog, not scope creep. |
| **R10** | Hiring Rust text-layout engineers | Genuinely scarce. The M1/M2 work (lexer, model, serializer) is ordinary Rust and can absorb generalists; only M3–M4 need the specialism. Sequence hiring accordingly. |

---

## 14. Immediate next steps

1. Scaffold the workspace in §1; CI on Windows, macOS, Linux from commit one.
2. Copy `packages/muya/test/spec/` → `spec/`; wire the ratchet.
3. Build the §11.2 differential harness against the existing TS engine **before** writing any of `mt-inline`.
4. Assemble `bench/corpus/`: empty, 10 KB, 250 KB, 1 MB, 5 MB, 50-code-fence, 100-inline-math, 20-table, RTL, CJK, emoji-heavy.
5. Port `inlineRenderer/__tests__/` to Rust as failing tests. Then start M1.

Open decisions: CRDT deferral (§2.2) confirm at M2 start · GPU vs CPU default (§12.2) decide at M3 · upstream parity tag (R9) at M2 start · fork-and-rename vs. propose upstream.
