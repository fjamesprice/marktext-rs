# MarkText Native Rewrite — Plan

> **Superseded on the stack question.** Rust was chosen; see `RUST-REWRITE-PLAN.md` for the build plan.
> This document is retained as the rationale record — the alternatives considered (§4), why Tauri was
> rejected (§4.4), and the incremental fallback (Appendix A). Where the two disagree, the Rust plan wins.
> Note that §2 and §9 here **overestimate the work**; see §0 of the Rust plan for why.

**Status:** Superseded in part · **Author:** planning doc · **Date:** 2026-08-01
**Goal:** Replace the Electron/Vue/TypeScript implementation with a natively compiled application that is dramatically smaller on disk, faster to start, and cheaper in RAM — without giving up the WYSIWYG markdown editing model that is MarkText's reason to exist.

This document is a decision + execution plan. Sections 1–5 are the argument and the stack choice; sections 6–13 are the work.

---

## 1. Success criteria

The rewrite is only worth doing if it clears a wide margin, not a few percent. Proposed acceptance targets for v1 on Windows x64, measured on the same machine as the Electron baseline:

| Metric | Electron today (estimate — **measure first**) | Native v1 target | Stretch |
|---|---|---|---|
| Installer size | 90–130 MB | ≤ 20 MB | ≤ 10 MB |
| Installed on-disk | 250–400 MB | ≤ 45 MB | ≤ 25 MB |
| Cold start → editable caret | 900–2000 ms | ≤ 250 ms | ≤ 100 ms |
| Warm start | 400–800 ms | ≤ 120 ms | ≤ 50 ms |
| RSS, empty document | 180–300 MB | ≤ 60 MB | ≤ 35 MB |
| RSS, 1 MB markdown doc | 350–600 MB | ≤ 120 MB | ≤ 70 MB |
| Keystroke → glyph on screen (p99) | 16–50 ms | ≤ 8 ms | ≤ 4 ms |
| Open 5 MB markdown file | 3–15 s (often unusable) | ≤ 800 ms | ≤ 300 ms |
| Idle CPU, window focused | 0.5–3 % | ~0 % | 0 % |

### 1.1 Phase 0 obligation: capture the real baseline

Every number in the "today" column above is an estimate from typical Electron behaviour, **not** a measurement of this build. Before any code is written, produce `bench/BASELINE.md` with real figures:

```bash
pnpm install
pnpm run build:win        # produces dist/ installer
pnpm run start            # PERF_TESTING=true is set automatically
```

Measure with a fixed corpus committed to `bench/corpus/` — at minimum: empty doc, 10 KB README, 250 KB spec-like doc, 1 MB doc, 5 MB doc, a doc with 50 code fences, a doc with 100 inline math spans, a doc with 20 tables. The same corpus becomes the native build's regression suite. Without this file the rewrite has no scoreboard and the targets above are unfalsifiable.

---

## 2. What is actually being rewritten

Measured from the working tree at `e52106fd` (source lines of `.ts/.js/.vue/.css/.html`, excluding `node_modules`):

| Area | Files | Lines | Rewrite disposition |
|---|---:|---:|---|
| `packages/muya/src` — TS editor engine (`@muyajs/core`) | 451 | 72,868 | **Rewrite.** The core problem. |
| `packages/desktop/src/renderer` — Vue 3 UI | 202 | 32,326 | **Rewrite.** Shrinks substantially in native UI. |
| `packages/muyajs/lib` — legacy JS engine | 170 | 25,698 | **Drop.** Already being retired upstream. |
| `packages/desktop/test` | 112 | 11,911 | Port selectively; spec fixtures reused verbatim. |
| `packages/desktop/src/main` — Electron main | 76 | 9,627 | **Rewrite**, largely 1:1 in scope. |
| `packages/desktop/src/{common,shared,preload}` | 18 | 1,604 | Mostly evaporates (no IPC boundary). |
| `packages/website` | 48 | 5,165 | **Out of scope.** Stays as-is. |
| **Total in scope** | ~860 | **~128,000** | |

Plus the non-code surface that must be preserved: **10 locales**, **72 preference keys** (`packages/desktop/static/preference.json`), 9 preference panes, three per-platform keybinding tables, and the theme set.

### 2.1 The engine, concretely

`packages/muya` is not a thin wrapper around a markdown parser. Per `packages/muya/CLAUDE.md`, it is:

- A **block tree** (`TreeNode → Parent → Content|Format`) with ~30 concrete block types across `commonMark/`, `gfm/`, `extra/`, `content/`, each registered in `registerBlocks()`.
- A **JSON document state** with `ot-json1` operational-transform ops, and inline edits encoded as `ot-text-unicode` ops nested inside them — i.e. the architecture is pre-wired for collaborative editing even though no transport exists.
- A **custom inline lexer** + snabbdom virtual DOM renderer, with reference-link definitions collected on every render pass.
- **~20 floating UI surfaces** in `src/ui/` (inline format toolbar, table tools, image tools, emoji picker, quick-insert menu, language selector, footnote tool, …) positioned with `@floating-ui/dom`.
- Markdown round-trip through `marked@18` in and a hand-written serializer out, plus HTML bridges via `turndown`.
- A conformance baseline locked in `test/spec/expected-failures.json`: **CommonMark 0.31 at 87.7 %, GFM 0.29 at 86.3 %**.

That last bullet is the single most valuable asset for the rewrite — see §10.

---

## 3. The hard part (read this before estimating)

The instinct is that "rewrite in C++" means porting 128k lines. It does not. The real cost is that **the Electron version gets an entire rich-text layout engine for free from Chromium, and a native version does not.**

`contenteditable` on a styled DOM is doing, invisibly:

1. **Text shaping** — HarfBuzz, per-script, with ligatures and kerning.
2. **Font fallback** — resolving a glyph the chosen font lacks, across the system font set, per run.
3. **Bidirectional text** — the UBA (Unicode Bidirectional Algorithm) for RTL/mixed content. MarkText has an upstream RTL fix in its history (`43bd8b77`), so this is a live requirement, not theoretical.
4. **Line breaking** — UAX #14, plus CJK rules, plus word-wrap inside `pre`.
5. **Grapheme/word/line cursor motion** — UAX #29 segmentation, so arrow keys don't split emoji or Devanagari clusters.
6. **IME composition** — pre-edit strings, candidate window positioning, commit semantics. Non-negotiable: MarkText ships `zh-CN`, `zh-TW`, `ja`, and `ko` locales.
7. **Accessibility tree** — screen readers on three platforms.
8. **Inline-flow layout** — images, math, and widgets flowing inside paragraphs; tables with content-driven column widths; nested lists and block quotes.
9. **Hit-testing and selection** — pixel → text position, across wrapped, bidi, mixed-font lines.

Items 1–5 and 8–9 are the work. Items 6–7 are the work that gets skipped and then sinks the release. **Any plan that does not name a concrete owner for each of these nine is not a plan.** The stack choice in §5 is essentially a choice about how many of them you buy versus build.

---

## 4. Stack evaluation

### 4.1 Criteria

| # | Criterion | Weight |
|---|---|---|
| C1 | Delivers §1 size and speed targets | Must |
| C2 | Supplies or enables shaping + bidi + line breaking (hard-part 1–4, 8–9) | Must |
| C3 | Correct IME on Windows/macOS/Linux | Must |
| C4 | Screen-reader accessibility on all three platforms | Must |
| C5 | License compatible with MIT redistribution | Must |
| C6 | Ecosystem covers the non-UI needs (markdown, PDF, search, spellcheck, watch) | High |
| C7 | Team can hire/retain for it | High |
| C8 | Time to a shippable v1 | High |

### 4.2 Candidates

| Stack | C1 size/speed | C2 text engine | C3 IME | C4 a11y | C5 license | C8 time | Verdict |
|---|---|---|---|---|---|---|---|
| **Rust + parley/cosmic-text + wgpu/vello + AccessKit** | Excellent | Build on solid primitives | Via winit; needs care | AccessKit (UIA/AT-SPI/NSAccessibility) | MIT/Apache | 24–36 mo | **Recommended** |
| **C++20 + Qt 6 Widgets** | Good | QTextDocument/QTextLayout — mature but constraining | Built in, mature | Built in, mature | LGPLv3 ⚠ | 15–24 mo | **Recommended fallback** |
| C++ + platform-native per OS (DirectWrite / CoreText / Pango) | Best possible | Best per-platform | Best | Best | Permissive | 36–48 mo | 3× UI cost; only if one platform ships first |
| Rust + Tauri (system WebView) | Small binary, **webview RAM stays** | Free (it's a browser) | Free | Free | MIT | 6–10 mo | **Rejected** — fails "fastest"; see §4.4 |
| Rust + Slint / iced | Good | Basic editing only; build rich text anyway | Partial | Partial | MIT / GPL-or-commercial ⚠ | 24–36 mo | No advantage over parley directly |
| Dear ImGui / egui (C++/Rust) | Excellent | Immediate-mode; no real text engine | Poor | Poor/none | MIT | — | **Rejected** — disqualified by C3/C4 |
| Flutter (Dart) | ~20 MB + runtime; mediocre desktop text | Skia + own engine | Good | Good | BSD | 18–24 mo | Runtime bloat; weak desktop text editing |
| Go + Fyne / Gio | Small | Weak rich-text | Weak | Weak | Permissive | — | **Rejected** — C2/C3 |
| Sciter / Ultralight (embedded HTML) | Small | Free | Free | Partial | Proprietary ⚠ | 8–14 mo | **Rejected** — C5 |

### 4.3 Recommendation

**Primary: Rust.**

Rust is the better answer than C++ here for four specific reasons, not as a general preference:

1. **The exact hard parts have first-class Rust crates.** `parley` (layout) over `swash`/`fontique` and `rustybuzz` (a HarfBuzz port) covers shaping, fallback, bidi, and line breaking. `accesskit` is the only cross-platform accessibility abstraction of its kind in any language. There is no equivalently coherent C++ set outside of a full toolkit like Qt.
2. **The tools MarkText already depends on are Rust.** `@vscode/ripgrep` is a subprocess wrapper around ripgrep; in Rust that becomes a direct link against the `ignore` + `grep-searcher` crates — faster, no process spawn, no shipped binary.
3. **The input is untrusted.** A markdown editor parses arbitrary files, pasted HTML, and image data. This codebase's threat surface is exactly the class where C++ memory bugs become CVEs, and `unsafe`-free parsing is worth real money in maintenance.
4. **Proof of viability.** Zed demonstrates a GPU-composited, sub-frame-latency native text editor on essentially this stack.

The cost is honest: **no Rust crate gives you a rich-text WYSIWYG document layout engine.** `parley` lays out a styled paragraph; block-level flow, tables, inline widgets, and hit-testing across them are yours to write. That is the bulk of the 24–36 months.

**Fallback: C++20 + Qt 6 Widgets**, if 24–36 months is not fundable. Qt's `QTextDocument`/`QTextLayout` already solves hard-parts 1–5 and 8–9 at a usable (not excellent) fidelity, and Qt ships mature IME, accessibility, printing-to-PDF, file watching, and settings. You reach v1 perhaps a year sooner and land at roughly 40 MB / 150 ms / 80 MB RSS instead of 15 MB / 80 ms / 45 MB — still an order of magnitude better than Electron on every axis.

Two caveats on Qt, both must be resolved before choosing it:
- **Licensing (C5).** Qt is LGPLv3 or commercial. LGPLv3 permits shipping an MIT app that *dynamically* links Qt, but the static linking that gets you to the small binary sizes requires either distributing relinkable objects or buying a commercial license. Get this in writing from counsel before committing.
- **Fidelity ceiling.** `QTextDocument` is a constrained rich-text model. Prototype MarkText's three hardest constructs in it — a GFM table with resize handles, an inline image with drag-resize, and a math span inline in a paragraph — before betting the project on it.

### 4.4 Why Tauri is rejected despite being the cheapest path

Tauri would let you keep the 73k-line muya engine nearly unchanged behind a Rust shell, shipping in well under a year with a 5–15 MB installer. It is genuinely the best *effort-adjusted* option and it should be named explicitly so the decision is made consciously rather than by omission.

It is rejected because it does not satisfy the stated goal. The webview still renders the document, so RAM stays in the 150–250 MB range, startup stays webview-bound, and typing latency remains at Chromium's mercy. It also *adds* a platform-variance problem: WebKitGTK on Linux behaves materially differently from WebView2 on Windows, and MarkText's rendering is currently guaranteed identical everywhere because it ships its own Chromium.

If the real constraint turns out to be budget rather than performance, Tauri is the correct answer and this document should be re-scoped — see Appendix A.

---

## 5. Language decision gate

Do not skip this. Run a **6-week spike, timeboxed, both stacks in parallel** (one engineer each), building the identical vertical slice:

> Open a 1 MB markdown file. Render paragraphs, ATX headings, bullet lists, one GFM table, and one fenced code block with syntax highlighting. Place a caret by mouse click. Type — including via a Japanese IME. Select across a wrapped line. Arrow-key through an emoji sequence and a Hindi cluster. Read the caret line aloud with NVDA (Windows) and VoiceOver (macOS). Save.

Score both against §1 metrics and hard-parts 1–9. Commit to one stack at week 6. Everything after §6 assumes the Rust track; substitute Qt equivalents if the gate goes the other way.

---

## 6. Target architecture

```
marktext-native/
  crates/
    mt-doc/          Document model: block tree, rope buffer, undo/redo,
                     text edit ops. No I/O, no UI, no platform code.
    mt-md/           Markdown ⇄ document model. Parse (pulldown-cmark or
                     comrak), incremental reparse, serialize. Round-trip
                     fidelity is this crate's contract.
    mt-layout/       Block flow + line layout over parley. Tables, lists,
                     block quotes, inline widgets (image/math/diagram).
                     Produces a positioned display list. Incremental:
                     an edit relayouts a paragraph, not a document.
    mt-render/       Display list → pixels via vello/wgpu, with a
                     tiny-skia CPU fallback for VM/RDP/old-GPU.
    mt-highlight/    tree-sitter grammars for fenced code.
    mt-math/         TeX layout for $…$ and $$…$$.
    mt-diagram/      Native subset of mermaid-style diagrams (§7).
    mt-export/       HTML and PDF emitters. PDF reuses mt-layout, so
                     export matches the screen by construction.
    mt-fs/           Files, encodings, atomic writes, watching, trash.
    mt-search/       In-doc find/replace; project search via ignore +
                     grep-searcher (linked, not spawned).
    mt-ui/           Widget layer: tabs, sidebar, command palette,
                     floating toolbars, preference panes. AccessKit tree.
    mt-app/          Windowing (winit), menus, keybindings, preferences,
                     session restore, updater, CLI.
  bench/             Corpus + harness; gates CI against §1 targets.
  spec/              CommonMark + GFM fixtures, imported verbatim (§10).
```

Design rules that carry the performance targets:

- **No process boundary, no serialization.** The renderer/main split and the entire `shared/types/ipc.ts` contract disappear. Deleting IPC is a large chunk of the latency win and of the 1,604 lines in `common/shared/preload`.
- **Incremental everywhere.** Parse, layout, and highlight all reparse the smallest dirty region. A keystroke must not touch the whole document — this is what makes the 5 MB file target reachable when Electron cannot.
- **Layout is the single source of truth for geometry**, shared by screen rendering, PDF export, hit-testing, and print. One engine, three consumers.
- **Zero idle work.** No timers, no polling, no animation loop when nothing changed. Redraw on event only.
- **`mt-doc`, `mt-md`, and `mt-layout` are headless and unit-testable** without a window. This is what makes the conformance suite in §10 runnable in CI.

### 6.1 Decision required: collaborative editing

muya's state layer is built on `ot-json1` + `ot-text-unicode`, which is a deliberate bet on future collaborative editing. The rewrite must decide:

- **(a) Carry it forward** — adopt a CRDT (`loro`, `diamond-types`, or `yrs`) in `mt-doc` from day one. Materially constrains the document model and costs ~3 months.
- **(b) Drop it** — a plain rope + undo tree, much simpler and faster, with collaboration deferred to a future major version at the cost of a later rewrite of `mt-doc`.

**Recommendation: (b), with the `mt-doc` public API written so edits are already expressed as discrete, invertible operations.** That preserves the option without paying for it now. Decide before Phase 2 starts; it is expensive to reverse afterwards.

---

## 7. Dependency replacement map

Every runtime JS dependency needs a native answer or an explicit drop.

| Current | Purpose | Native replacement | Confidence |
|---|---|---|---|
| `marked@18` | Markdown → AST | `pulldown-cmark` or `comrak` | High — both faster and more spec-compliant |
| `turndown` + `joplin-turndown-plugin-gfm` | HTML → Markdown (paste) | `htmd`, or hand-written over `html5ever` | Medium |
| `dompurify` | Sanitize embedded HTML | `ammonia` | High |
| `prismjs` | Code fence highlighting | `tree-sitter` (preferred) or `syntect` | High — better incremental behaviour |
| `codemirror@5` | Source-code mode | Same native editor, plain-text mode | High — deletes a whole dependency |
| `katex` | Math rendering | `rex` (Rust TeX) or MicroTeX via FFI | **Low — see §12 R2** |
| `mermaid` | Diagrams | Native subset + optional `mmdc` passthrough | **Low — see §12 R3** |
| `vega` / `vega-lite` / `vega-embed` | Chart blocks | **Drop** in v1; optional CLI passthrough | Decided |
| `flowchart.js`, `snapsvg-cjs` | Legacy diagrams | Folded into `mt-diagram` | Medium |
| `plantuml-encoder` + `pako` | PlantUML server URLs | `flate2` + base64; it's just an encoded URL | High |
| `snabbdom`, `snabbdom-to-html` | Virtual DOM | **N/A** — no DOM | High |
| `ot-json1`, `ot-text-unicode` | OT ops | See §6.1 | Decision pending |
| `rxjs` | Event stream merging | Plain event dispatch | High |
| `@floating-ui/dom` | Popup positioning | ~300 lines in `mt-ui` | High |
| `vue`, `pinia`, `vue-router`, `element-plus` | UI framework | `mt-ui` | High |
| `chokidar` | File watching | `notify` | High |
| `@vscode/ripgrep` | Project search | `ignore` + `grep-searcher`, linked | High — strict improvement |
| `ced` | Encoding detection | `chardetng` + `encoding_rs` | High |
| `keytar` | Credential storage | `keyring` | High |
| `electron-store` | Preferences | `serde` + JSON, same schema (§11) | High |
| `electron-updater` | Auto-update | `cargo-dist` updater, or in-house | Medium |
| `electron-log` | Logging | `tracing` + `tracing-appender` | High |
| `electron-window-state` | Window geometry | `mt-app` session state | High |
| `native-keymap` | Physical key layout | `winit` key codes + platform APIs | Medium |
| `font-list` | Font enumeration | `fontique` | High |
| Chromium `printToPDF` | PDF export | `mt-layout` → `pdf-writer` | Medium — **better fidelity control**, more work |
| `webfontloader`, `github-markdown-css` | Web fonts / CSS theme | Bundled fonts + native theme format | High |
| Chromium spellcheck | Spellcheck | `spellbook` (Hunspell-compatible, pure Rust) or platform APIs | Medium |
| `vue-i18n` + 10 locale JSONs | Localization | `fluent-rs`, or keep the JSON format | High |

---

## 8. Feature scope for v1

**Ship (parity required):** CommonMark + GFM WYSIWYG editing; source-code mode; focus and typewriter modes; tabs and sidebar file tree; find/replace in document and across a folder; the full paragraph/format command set; tables with row/column tools; images with paste, drag-drop, resize, and local-path handling; footnotes; front matter; math; TOC; export to HTML and PDF; 72 preferences; 10 locales; three platform keybinding sets with user overrides; spellcheck; auto-save and file watching; session restore; CLI arguments; theming.

**Defer to v1.1:** Mermaid beyond the native subset; PlantUML rendering (the encoder ships in v1, so links work); auto-updater (v1 ships via installers and package managers); custom user themes beyond the bundled set.

**Drop, with a note in the release announcement:** Vega / Vega-Lite chart blocks (retained as passthrough code fences so documents round-trip losslessly — the block renders as source, never silently disappears).

**Non-negotiable at v1 regardless of schedule pressure:** IME correctness for CJK; screen-reader support on all three platforms; lossless markdown round-trip. These are the three that cannot be added credibly after launch.

---

## 9. Phased plan

Each phase ends in a **gate** with an explicit kill/continue decision. Durations assume 3–4 engineers after Phase 1.

| Phase | Duration | Deliverable | Gate |
|---|---|---|---|
| **0 — Baseline** | 3 wks | `bench/BASELINE.md`, corpus, harness. Feature inventory audited against the running app. | Real numbers exist for every §1 row. |
| **1 — Stack spike** | 6 wks | Both vertical slices from §5, scored. | **Stack committed.** If neither slice clears §1 targets, stop and reconsider Appendix A. |
| **2 — Headless core** | 4 mo | `mt-doc`, `mt-md`. Round-trips the CommonMark + GFM suites. No UI. | **Beat 87.7 % / 86.3 % conformance** (§10). Round-trip is lossless on the corpus. |
| **3 — Layout + render** | 6 mo | `mt-layout`, `mt-render`, `mt-highlight`. Read-only viewer for the full corpus. | 5 MB doc opens ≤ 800 ms; scrolling holds 60 fps; hard-parts 1–4, 8–9 demonstrably handled. |
| **4 — Editing** | 6 mo | Caret, selection, all edit ops, undo/redo, clipboard, IME, AccessKit. | p99 keystroke ≤ 8 ms. **NVDA + VoiceOver + Orca pass a scripted screen-reader script.** CJK IME accepted by a native speaker. |
| **5 — Application shell** | 5 mo | `mt-ui`, `mt-app`, `mt-fs`, `mt-search`: tabs, sidebar, palette, preferences, menus, keybindings, i18n, watching, session. | Feature parity checklist from §8 "Ship" is green. |
| **6 — Export + extras** | 3 mo | `mt-export`, `mt-math`, `mt-diagram`, spellcheck. | PDF matches screen rendering on the corpus; math renders the KaTeX test set. |
| **7 — Harden + ship** | 4 mo | Three-platform packaging, migration (§11), fuzzing, beta. | All §1 targets met. Zero data-loss bugs in a ≥ 500-user beta. |

**Total: ~34 months** to a v1 with the Rust track, ~22 with Qt. Phases 3–6 partially overlap with a team of 4+; the critical path is 2 → 3 → 4.

Ship the **Windows build first**. It is the user's platform, it removes two-thirds of the platform-integration risk from the critical path, and it gets real feedback ~6 months earlier. macOS and Linux follow in Phase 7+.

---

## 10. Testing and conformance

The strongest asset the rewrite inherits: **the CommonMark 0.31 and GFM 0.29 conformance suites are language-agnostic JSON fixtures.** Copy `packages/muya/test/spec/` into `spec/` and run the native parser against it unchanged. `expected-failures.json` gives a precise, non-negotiable bar — the same ratchet rule applies:

- Any listed example that starts passing must be removed from the list.
- Any unlisted example that starts failing fails CI.
- **Compliance can only go up.** The native engine must ship at ≥ 87.7 % CommonMark and ≥ 86.3 % GFM, and the goal is materially higher — `pulldown-cmark` and `comrak` both score above 99 % on CommonMark out of the box, so most of the gap is in how the block tree maps back to markdown, not in parsing.

Additional layers:

| Layer | Approach |
|---|---|
| Round-trip | Property test: `parse → serialize → parse` is a fixed point for every corpus file and every spec fixture. |
| Fuzzing | `cargo-fuzz` on `mt-md` parse and on the HTML-paste path. Malformed markdown must never panic. |
| Layout | Golden-image tests per block type, per platform, with a perceptual-diff threshold. |
| Latency | `bench/` runs in CI on fixed hardware; a §1 regression fails the build. |
| IME | Scripted composition sequences per platform, plus manual native-speaker sign-off each release. |
| Accessibility | AccessKit tree snapshots in CI; live NVDA/VoiceOver/Orca script each release. |
| Data safety | Kill-the-process-mid-save tests. No corruption, no truncation, ever. |
| Cross-check | Differential test: render the corpus in both the Electron build and the native build; diff the exported HTML. |

---

## 11. Compatibility and migration

Existing users must not lose anything on upgrade. The native app reads the Electron app's data in place:

| Artifact | Requirement |
|---|---|
| `preference.json` (72 keys) | Read the existing file and schema verbatim. Keys with no native equivalent are preserved on write, never dropped. |
| Keybindings | Import the user's overrides; the three platform default tables port as data, not code. |
| Themes | Bundled themes reproduced natively. Custom user CSS themes cannot carry over — detect them, warn on first run, document the new theme format. |
| Session / window state | Import `electron-window-state` and open-tab state on first launch. |
| Recently-opened, pinned folders | Import. |
| Locales | Keep the existing 10 JSON files as the source of truth; translators' workflow is unchanged. |
| Documents | Markdown files are the format. Nothing to migrate — but §10's round-trip property test is what actually guarantees this. |

Also required: a **user-visible compatibility note** listing exactly what changed (dropped Vega blocks, deferred mermaid, custom-theme format). Silent feature removal in an editor people trust with their notes is how a rewrite loses its userbase.

---

## 12. Risk register

| ID | Risk | Impact | Mitigation |
|---|---|---|---|
| **R1** | Rich-text layout engine is underestimated; Phase 3 doubles | Schedule collapse | This is the single most likely failure. Phase 1 spike must build real wrapped/bidi/mixed-font layout, not a mockup. Phase 3 gate is hard. |
| **R2** | Native math rendering can't match KaTeX quality | Visible regression for a core audience | Evaluate `rex` and MicroTeX **during Phase 1**, against KaTeX's own test set. Fallback: bundle MicroTeX via FFI and accept the C++ dependency. |
| **R3** | Mermaid has no native equivalent and users depend on it | Feature regression | Scoped as deferred in §8. Native subset covers flowchart + sequence; everything else is an optional `mmdc` passthrough with a clear "requires Node" message. Never render a mermaid block as blank. |
| **R4** | IME defects found late | Unshippable in CJK markets | IME is a Phase 4 gate criterion, not a polish item. Recruit native-speaker testers at Phase 4 start. |
| **R5** | Accessibility bolted on late | Legal/ethical exposure; unusable for some users | AccessKit tree built in Phase 4 alongside the caret, never after. CI snapshots from day one. |
| **R6** | Qt LGPL static-linking constraint (if Qt track) | Blocks the size target or forces a commercial license | Resolve with counsel **before** the §5 gate, not after. |
| **R7** | 34 months with no user-visible progress; project loses momentum | Abandonment | Ship the read-only viewer (Phase 3) as a standalone fast previewer. Real users, real feedback, real bug reports, 18 months early. |
| **R8** | Upstream MarkText keeps evolving; the rewrite is always behind | Perpetual catch-up | Freeze the parity target at a specific upstream tag. Track new upstream features as post-v1 backlog, not as scope creep. |
| **R9** | GPU rendering breaks on VMs, RDP, older drivers | Support burden | `tiny-skia` CPU fallback is a Phase 3 deliverable, not an afterthought. Test in a VM in CI. |
| **R10** | Hiring for the stack | Staffing | Rust GUI + text-layout specialists are scarce. Budget for it, or the Qt track's larger talent pool becomes the deciding factor at the §5 gate. |

---

## 13. Effort and staffing

| Role | Count | Phases |
|---|---|---|
| Text layout / rendering engineer | 1–2 | 1, 3, 4 (critical path) |
| Core / parser engineer | 1 | 2, 6 |
| Application / UI engineer | 1–2 | 5, 6, 7 |
| Platform integration (Win/mac/Linux) | 1 | 4, 5, 7 |
| QA / accessibility / i18n | 1 (from Phase 4) | 4–7 |

Roughly **8–11 engineer-years** for the Rust track, **5–7** for Qt. Nothing about the ordering changes if the team is smaller — the critical path 2 → 3 → 4 does not parallelize well, so a 2-person team takes proportionally longer rather than the same time.

---

## Appendix A — If the full rewrite isn't fundable

The performance goal can be pursued incrementally, at roughly a tenth of the cost. This is not the plan the request asked for, and it does not reach the §1 stretch targets — but it is the highest expected-value option if §13's budget is unavailable, and it should be on the table when the §5 gate is decided:

1. **Move muya's hot paths to WebAssembly.** Rewrite `mt-md`-equivalent parsing and the block-tree diff in Rust, compile to WASM, keep the DOM rendering. Weeks, not years. Directly attacks large-file open time and typing latency.
2. **Replace Electron with Tauri.** Installer 130 MB → ~12 MB, and the `main`-process rewrite (9.6k lines) is the same work Phase 5 needs anyway. RAM improves modestly; startup improves a lot on Windows where WebView2 is already resident.
3. **Cut renderer weight.** Element Plus and CodeMirror 5 are large; the renderer's 32k lines carry real fat.
4. **Make rendering virtualized.** Only lay out visible blocks. This alone can fix the 5 MB-file case.

Realistic combined outcome: **~15 MB installer, ~400 ms start, ~140 MB RSS** — perhaps 60–70 % of the benefit for 10 % of the cost. Steps 1 and 4 are worth doing regardless, since they benefit the current app immediately and the work is not wasted if the full rewrite later proceeds.

---

## Appendix B — Open decisions

| # | Decision | Owner | Needed by |
|---|---|---|---|
| D1 | Rust vs Qt/C++ | Eng lead | End of Phase 1 |
| D2 | Collaborative editing: carry OT/CRDT forward or drop (§6.1) | Product + eng | Start of Phase 2 |
| D3 | Qt commercial license, if Qt track (R6) | Legal | Before Phase 1 gate |
| D4 | Mermaid: native subset scope vs external CLI vs drop | Product | Start of Phase 6 |
| D5 | Windows-first vs simultaneous three-platform launch | Product | Start of Phase 5 |
| D6 | Upstream parity freeze tag (R8) | Eng lead | Start of Phase 2 |
| D7 | Fork-and-rename vs propose upstream to marktext/marktext | Product | Before Phase 2 |
