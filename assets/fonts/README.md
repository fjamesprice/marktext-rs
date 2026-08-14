# `assets/fonts/` — the pinned face set

The font set M3 §8 **D7** says the milestone owes, and the mitigation
**M3-R10** names. D7 decided that `mt-layout` never enumerates or opens a font:
a collection is built above it by the shell and passed in, and *"goldens run
against a pinned bundled set registered from bytes"*. **D10** is why that has to
be pinned rather than discovered — layout goldens are exact-equality text files,
and a golden that depends on the developer's installed fonts is not a golden.

So this directory is a committed artifact. [`faces.toml`](faces.toml) is the
list, in fallback order, and that order is load-bearing.

Everything below is measured by `spikes/s1-faces`, on parley
`a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e` (D2's pin) with
`default-features = false, features = ["std"]`:

```sh
cd spikes && cargo run -p s1-faces --release
```

It re-derives every family name, weight, style, byte count and SHA-256 from the
files and **exits non-zero** if `faces.toml` has drifted from them.

> Not `assets/fonts/` inside `crates/` on purpose: `cargo xtask deps` asserts
> that `crates/` holds exactly the 14 crates RUST-REWRITE-PLAN.md §1 names, and
> `mt-layout` may not open a file in any case.

---

## 1. What is here

| # | File | Bytes | Family (skrifa) | Weight | Style | Covers |
|--:|---|---:|---|---:|---|---|
| 1 | `OpenSans-Regular.ttf` | 147,528 | Open Sans | 400 | normal | Latn Grek Cyrl **Hebr** |
| 2 | `OpenSans-Italic.ttf` | 153,256 | Open Sans | 400 | italic | Latn Grek Cyrl Hebr |
| 3 | `OpenSans-Bold.ttf` | 147,264 | Open Sans | 700 | normal | Latn Grek Cyrl Hebr |
| 4 | `OpenSans-BoldItalic.ttf` | 153,308 | Open Sans | 700 | italic | Latn Grek Cyrl Hebr |
| 5 | `NotoSansArabic-Regular.ttf` | 271,652 | Noto Sans Arabic | 400 | normal | **Arab** |
| 6 | `NotoSansArabic-Bold.ttf` | 298,372 | Noto Sans Arabic | 700 | normal | Arab |
| 7 | `NotoEmoji[wght].ttf` | 1,982,596 | Noto Emoji | 400 | normal | **Zsye** (variable) |
| 8 | `NotoSansCJKsc-Regular-corpus-subset.otf` | 56,268 | Noto Sans CJK SC | 400 | normal | **Hani Hira Kana Hang** (**derived fixture**) |
| 9 | `DejaVuSansMono.ttf` | 340,712 | DejaVu Sans Mono | 400 | normal | Latn Grek Cyrl Arab Zyyy |
| 10 | `DejaVuSansMono-Oblique.ttf` | 251,932 | DejaVu Sans Mono | 400 | italic | Latn Grek Cyrl Zyyy |
| 11 | `DejaVuSansMono-Bold.ttf` | 331,992 | DejaVu Sans Mono | 700 | normal | Latn Grek Cyrl Arab Zyyy |
| 12 | `DejaVuSansMono-BoldOblique.ttf` | 253,580 | DejaVu Sans Mono | 700 | italic | Latn Grek Cyrl Zyyy |
| | **12 files, 5 families** | **4,388,460** | | | | |

Plus five licence files (26,224 B) — **4,414,684 B total**, against a ~12 MB
budget.

**Entry 8 is a corpus-derived subset and a test fixture, never a shipped
asset.** It is 0.34 % of the upstream font it came from, it exists only so the
goldens have CJK to be reproducible about, and it stops covering the corpus the
moment the corpus changes. §6 is the whole story: the reasoning, the
regeneration command, the fragility contract and the check that makes the
failure loud.

With it, the set covers **291 of 291** distinct non-ASCII codepoints in
`bench/corpus/*.md` — every clause of M3's exit gate (bidi, CJK, emoji,
ligatures) has a face behind it.

## 2. Provenance

Every file below was fetched from a **commit-pinned** URL, so the source is
reproducible and not "whatever upstream's `main` said that day".

### Open Sans 3.003 — OFL 1.1

* Upstream: `https://github.com/googlefonts/opensans` @ `bd7e37632246368c60fdcbd374dbf9bad11969b6`, path `fonts/ttf/`
* Version string (name ID 5): `Version 3.003; ttfautohint (v1.8.4)`
* Licence: [`LICENSE-OpenSans.txt`](LICENSE-OpenSans.txt), OFL 1.1, `Copyright 2020 The Open Sans Project Authors`. **No Reserved Font Name** — the phrase appears only in the licence's own definitions section, never in the copyright notice, so no RFN notice is owed.
* Static instances, not the variable font. google/fonts ships only
  `OpenSans[wdth,wght].ttf` (532,636 B) and `OpenSans-Italic[wdth,wght].ttf`
  (583,992 B); the upstream project ships the statics, and D10 prefers a static
  because there is then no default-instance question to get wrong.

```
OpenSans-Regular.ttf     147528  c53aceea2dcf5b4098099c0c4d0a061d17e178a049317b42a422b1a9f7f8eb59
OpenSans-Italic.ttf      153256  93bc1bb6abf4e6b7c75d7131714061d5b57cc478abcabe4cb3519bb38fb917aa
OpenSans-Bold.ttf        147264  27da758f4dcac9a65abe914c13b463b42982b9909bc65713424099f4810bd1e6
OpenSans-BoldItalic.ttf  153308  d672a770037104b6af45e1336b3d3c1729c8aea940f81e010f5a8a7319c29a21
```

### Noto Sans Arabic 2.013 — OFL 1.1

* Upstream: `https://github.com/notofonts/notofonts.github.io` @ `445abfe2d405cb658a9d825ab056e2004fb60627`, path `fonts/NotoSansArabic/full/ttf/`
* Version string: `Version 2.013; ttfautohint (v1.8.4.16-eb6c)`
* Licence: [`LICENSE-NotoSansArabic.txt`](LICENSE-NotoSansArabic.txt), OFL 1.1, no Reserved Font Name.

```
NotoSansArabic-Regular.ttf  271652  7ed3fe069312aceac454f17cf613a30f95271d6ed7ce58005ed4d016bd3823d7
NotoSansArabic-Bold.ttf     298372  5ccd1a8914f7c7e8aa8050f2c7c37b10fc5e855f06583c2a2248a436aad3fc0f
```

### Noto Emoji 3.002 — OFL 1.1 — **variable**

* Upstream: `https://github.com/google/fonts` @ `73fc2ff52147e34a74804b500cf89ca219eac55d`, path `ofl/notoemoji/NotoEmoji[wght].ttf`
* Version string: `Version 3.002`
* Licence: [`LICENSE-NotoEmoji.txt`](LICENSE-NotoEmoji.txt), OFL 1.1, `Copyright 2013 Google LLC`, no Reserved Font Name.
* **This is the one variable face, and there is no static instance to prefer.**
  `ofl/notoemoji` has no `static/` directory and googlefonts/noto-emoji
  publishes only the colour builds. So the honest statement, as D7's write-up
  asks for: `fvar` carries one axis, `wght`, range **300 – 700, default 400**.
  Text at weight 400 resolves the default instance and applies no deltas; a
  bold heading resolves `wght = 700` and gets different advances — fontique
  synthesises the axis value whenever the request differs from the axis default
  (`fontique/src/font.rs`, `FontInfo::synthesis`). Both outcomes are
  deterministic and both come out of this file, so goldens are reproducible;
  they are just not weight-invariant.
* **Monochrome, not colour, and that is a size decision with a date on it.**
  S1 needs correct advances and cluster counts; whether the face carries colour
  is S4's problem. The prices, for whoever revisits it:
  `NotoEmoji[wght].ttf` **1,982,596 B**, `Noto-COLRv1.ttf` **4,991,984 B**,
  `NotoColorEmoji.ttf` (CBDT) **10,673,480 B**. Swapping this entry changes
  advances and therefore every emoji golden.
* Upstream's file name contains brackets. Kept verbatim, because provenance is
  the point and nothing globs for it — loaders read `file` out of `faces.toml`.

```
NotoEmoji[wght].ttf  1982596  de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551
```

### Noto Sans CJK SC 2.004 — OFL 1.1 — **corpus-derived subset, test fixture**

* Input: `https://github.com/notofonts/noto-cjk` @ `f8d157532fbfaeda587e826d4cd5b21a49186f7c`, path `Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf`
* Input size and hash: **16,437,364 B**, `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`
* Version string: `Version 2.004;hotconv 1.0.118;makeotfexe 2.5.65603` — carried through to the subset unchanged
* Licence: [`LICENSE-NotoSansCJK.txt`](LICENSE-NotoSansCJK.txt), OFL 1.1.
* **No Reserved Font Name, so the family name is kept** — which matters
  mechanically, because the fallback chain resolves on it. The evidence, since
  this is the one entry where the question has teeth: the licence file mentions
  "Reserved Font Name" only inside the OFL's own definitions section (line 32)
  and declares none; the font's `name` table ID 0 is
  `© 2014-2021 Adobe (http://www.adobe.com/).` with no RFN clause attached.
  ID 7 does carry `Noto is a trademark of Google Inc.` — that is trademark law
  rather than OFL §3, and this fixture is neither shipped nor used to promote
  anything, so it does not bar keeping the name. Recorded so the judgement is
  auditable rather than assumed.
* Regeneration: see [§6](#6-cjk--a-corpus-derived-subset-decided-and-priced).
  fontTools **4.55.3**. Byte-reproducible: two runs produced identical
  SHA-256s. (`pyftsubset` does not recalculate `head.modified` unless
  `--recalc-timestamp` is passed, which is why the output is stable.)

```
NotoSansCJKsc-Regular-corpus-subset.otf  56268  05cc026ccff2e5c4954fed438be311a0fe3747caf0826a7117c649a252c0137d
```

### DejaVu Sans Mono 2.37 — Bitstream Vera + Arev licence

* **Copied from the reference clone**, `C:\Dev\marktext\packages\muya\src\assets\styles\fonts\`, not re-downloaded — these are MarkText's own four files and the half of its bundled set that D7 found loads (1,178,216 B). The hashes below are the evidence they were copied byte-for-byte.
* Version string: `Version 2.37`
* Licence: [`LICENSE-DejaVuSansMono.txt`](LICENSE-DejaVuSansMono.txt), the clone's `DejaVuSansMono_LICENSE` verbatim (Bitstream Vera Fonts Copyright + Arev Fonts Copyright).
* Note the subfamily of the upright face is **`Book`**, not `Regular`.

```
DejaVuSansMono.ttf              340712  b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582
DejaVuSansMono-Oblique.ttf      251932  742097840c541870e8d6dc5c9b37bb1ceeea6c0dedd1d475faf903ef9df734b0
DejaVuSansMono-Bold.ttf         331992  bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769
DejaVuSansMono-BoldOblique.ttf  253580  91713a71d550bba22c2a6b2bb2a9ad8f9a159e12e4e9f0a5b2677998ba21213e
```

---

## 3. The `.woff` trap, reproduced

D7's first fact, re-run by `s1-faces` on this machine against MarkText's real
file rather than cited from S0:

```
open-sans-v27-latin-ext_latin-300.woff  27512 B  magic wOFF 774F4646
register_fonts -> 0 families, and **no error and no panic**: the Vec is simply empty.
families in the collection afterwards: 0
```

`Collection::register_fonts` returns `Vec<(FamilyId, Vec<FontInfo>)>`, not a
`Result`. A WOFF1 container is per-table zlib-compressed OpenType and
skrifa/read-fonts parse OpenType only, so all eight of MarkText's Open Sans
files register as **zero families, silently**. The registration table for the
committed set prints the same `magic` column, where every entry reads
`.... 00010000` — a plain TrueType.

**An empty `register_fonts` result must be a hard error, never a warning.**
That is D7's rule and M3-R10's mitigation, and it is **owed by a later phase**:
the code that enforces it does not exist yet. Concretely, it belongs in two
places, and both are outside this directory:

1. **The shell's collection builder** — whatever function turns `faces.toml`
   plus the bytes into a `FontContext`. Every `register_fonts` call must be
   checked for an empty return and must **panic or return `Err`** naming the
   file. A silently-empty face produces a subtly wrong layout everywhere with
   no diagnostic, which is strictly worse than a crash.
2. **`cargo xtask layout`** (D10) — before generating or comparing goldens, it
   must verify each file's SHA-256 against `faces.toml` and hard-fail on a
   mismatch. Goldens compared against a set that silently differs from the
   pinned one are the same failure in a slower form.

`spikes/s1-faces/src/main.rs` already implements both checks, deliberately, so
the later phase has a working reference to lift rather than a description —
including a hand-rolled SHA-256 that avoids adding a dependency to the **root**
workspace, which is where `xtask` lives.

---

## 4. Coverage, measured

Distinct non-ASCII codepoints per corpus file. **"tofu" is counted by laying
each codepoint out and counting glyph id 0**, not by reading `cmap` — a
codepoint present in a `cmap` can still fail to reach a glyph through family
selection, and §5 is exactly that case. `250kb.md`, `1mb.md` and `5mb.md` are
counted for their codepoint set only and never laid out; all three turn out to
be pure ASCII.

Chain B is the proposed chain: `faces.toml` order as an explicit family list.

| Corpus file | Bytes | Distinct non-ASCII | Rendered | Tofu |
|---|---:|---:|---:|---:|
| `100-inline-math.md` | 9,171 | 2 | 2 | 0 |
| `10kb.md` | 10,521 | 1 | 1 | 0 |
| `1mb.md` | 1,048,796 | 0 | 0 | 0 |
| `20-tables.md` | 6,510 | 1 | 1 | 0 |
| `250kb.md` | 256,224 | 0 | 0 | 0 |
| `50-code-fences.md` | 4,277 | 2 | 2 | 0 |
| `5mb.md` | 5,243,009 | 0 | 0 | 0 |
| `README.md` | 5,679 | 14 | 14 | 0 |
| `cjk.md` | 1,266 | 140 | **140** | **0** |
| `emoji.md` | 1,443 | 82 | **82** | **0** |
| `empty.md` | 0 | 0 | 0 | 0 |
| `rtl.md` | 1,103 | 49 | **49** | **0** |
| **sum** | | **291** | **291** | **0** |

**Zero tofu, everywhere.** Before the CJK subset of §6 was added this read
145/291 with all 146 misses CJK: 138 in `cjk.md`, the two Han characters in
`emoji.md` (`世界`, U+4E16 U+754C) and six in the corpus's own `README.md`
prose (`中体文粗紧邻` — that file is the corpus provenance document rather than
a corpus document, and is included because it matches `bench/corpus/*.md`).

Whole-file layout of the small files, same chain, 800 px wrap (muya's
`content_width_px`):

| Corpus file | Glyphs | `.notdef` | Lines |
|---|---:|---:|---:|
| `100-inline-math.md` | 9,010 | 0 | 158 |
| `10kb.md` | 10,307 | 0 | 177 |
| `20-tables.md` | 6,247 | 0 | 247 |
| `50-code-fences.md` | 3,850 | 0 | 320 |
| `rtl.md` | 914 | **0** | 33 |
| `emoji.md` | 947 | **0** | 41 |
| `cjk.md` | 726 | **0** | 41 |
| `README.md` | 5,493 | **0** | 91 |

`cjk.md`'s laid-out width moves 549.13 → 681.49 px with the CJK face in the
chain, which is the tell that the subset is really shaping: `.notdef` boxes are
narrower than the full-width glyphs that replaced them.

### Per-face `cmap` census

Codepoints present in each file's character map, by block. This is the
supporting detail behind `faces.toml`'s `scripts` field; the tofu counts above
are the authority.

| File | Latn | Grek | Cyrl | Hebr | Arab | ArabPF | Punct | Sym | Emoji | RegInd | total |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `OpenSans-Regular.ttf` | 307 | 75 | 255 | **45** | 0 | 1 | 34 | 14 | 0 | 0 | 1,010 |
| `OpenSans-Italic.ttf` | 307 | 75 | 255 | 45 | 0 | 1 | 34 | 14 | 0 | 0 | 1,010 |
| `OpenSans-Bold.ttf` | 307 | 75 | 255 | 45 | 0 | 1 | 34 | 14 | 0 | 0 | 1,010 |
| `OpenSans-BoldItalic.ttf` | 307 | 75 | 255 | 45 | 0 | 1 | 34 | 14 | 0 | 0 | 1,010 |
| `NotoSansArabic-Regular.ttf` | 240 | 0 | 0 | 0 | **256** | 141 | 21 | 3 | 0 | 0 | 1,551 |
| `NotoSansArabic-Bold.ttf` | 240 | 0 | 0 | 0 | 256 | 141 | 21 | 3 | 0 | 0 | 1,551 |
| `NotoEmoji[wght].ttf` | 2 | 0 | 0 | 0 | 0 | 0 | 3 | 160 | **1,184** | **26** | 1,489 |
| `NotoSansCJKsc-…-corpus-subset.otf` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **143** |
| `DejaVuSansMono.ttf` | 466 | 116 | 180 | **0** | **99** | 141 | 54 | 1,019 | 0 | 0 | 3,322 |
| `DejaVuSansMono-Oblique.ttf` | 463 | 116 | 180 | 0 | **0** | **0** | 54 | 756 | 0 | 0 | 2,683 |
| `DejaVuSansMono-Bold.ttf` | 466 | 116 | 180 | 0 | 99 | 141 | 54 | 1,020 | 0 | 0 | 3,261 |
| `DejaVuSansMono-BoldOblique.ttf` | 464 | 116 | 180 | 0 | **0** | **0** | 54 | 756 | 0 | 0 | 2,683 |

Ranges: Latn U+0041–024F, Grek U+0370–03FF, Cyrl U+0400–04FF, Hebr U+0590–05FF,
Arab U+0600–06FF, ArabPF U+FE70–FEFF, Punct U+2000–206F, Sym U+2190–2BFF,
Emoji U+1F300–1FAFF, RegInd U+1F1E6–1F1FF. The CJK subset's 143 cmap entries
fall outside every one of those columns, which is the point of it: it is
exactly the corpus's CJK and nothing else.

### Emoji sequence shaping

Each row is **one grapheme** made of several codepoints. A face without the
GSUB rules produces one glyph per codepoint; correct advances and cluster
counts are what S1 needs from an emoji face, and this is the check.

| Sequence | Codepoints | Glyphs | `.notdef` | Advance |
|---|---:|---:|---:|---:|
| family ZWJ `👨‍👩‍👧‍👦` | 7 | **1** | 0 | 20.31 |
| skin tone `👋🏽` | 2 | **1** | 0 | 20.31 |
| flag GB `🇬🇧` | 2 | **1** | 0 | 20.31 |
| keycap `1️⃣` | 3 | 2 | 0 | 20.31 |
| plain `😀` | 1 | 1 | 0 | 20.31 |

ZWJ sequences, skin-tone modifiers and regional-indicator flags all ligate to a
single glyph. The keycap is two glyphs because U+20E3 is a zero-advance
combining enclosure, which is correct. Noto Emoji is monospaced, so every
advance is the same 20.31 at 16 px.

---

## 5. Registering the set: the shell owes three calls, not one

The set only covers the corpus if the collection is wired up correctly, and
"correctly" turned out to be a measurement rather than a guess. Three chains,
same faces, same corpus, 291 distinct non-ASCII codepoints:

| Chain | Rendered | Tofu |
|---|---:|---:|
| **A** — the theme's `[fonts] body` stack verbatim, faces merely registered | 42 | **249** |
| **B** — `faces.toml` order as an explicit `FontFamily::List` | **145** | 146 |
| **C** — the theme's stack + `append_fallbacks` + `append_generic_families`, both driven from `faces.toml` | **145** | 146 |

**Chain A is the naive thing and it fails badly**: `rtl.md` drops to 22/49 (all
Arabic tofu) and `emoji.md` to 8/82. Registering a face does not make parley
reach for it — the theme stack names `Open Sans`, and Open Sans has no Arabic
and no emoji.

**C matches B exactly**, which is the useful result: a shell can keep the
theme's own stacks and wire the collection instead of rewriting every stack.
The three calls, all driven from fields `faces.toml` already carries:

```rust
collection.append_generic_families(GenericFamily::SansSerif, [open_sans]);   // generic_families
collection.append_generic_families(GenericFamily::Monospace, [dejavu]);      // generic_families
collection.append_generic_families(GenericFamily::Emoji,     [noto_emoji]);  // generic_families
collection.append_fallbacks(Script::from_bytes(*b"Arab"), [noto_arabic, dejavu]);  // fallback_scripts
// …one append_fallbacks per distinct script in the list
```

Two of those are not obvious and both were found by measurement:

* **Emoji does not go through script fallback.** `parley/src/shape/mod.rs`
  appends `QueryFamily::Generic(GenericFamily::Emoji)` to the query for any
  cluster where `CharCluster::is_emoji()`. Noto Emoji registered only as a
  `Zsye` script fallback is never consulted: that arrangement left **67 of
  `emoji.md`'s 82 codepoints as tofu**. `Zyyy` (Common) is the script property
  emoji pictographs actually carry, and even registering under `Zyyy` only got
  to 15/82. The generic family is the route.
* **`sans-serif` and `monospace` mean nothing by default.** Both theme stacks
  end in a CSS generic, and with `system_fonts: false` there is no system to
  define them. They resolve to nothing unless the shell says what they are.

This is why `faces.toml` carries `fallback_scripts` and `generic_families`
alongside `scripts`: `scripts` says what a face covers, the other two say what
to tell fontique. Merging them would force one of the two to be wrong.

---

## 6. CJK — a corpus-derived subset, decided and priced

**Decided: subset `NotoSansCJKsc-Regular.otf` to the corpus and commit the
56,268 B result as a test fixture.** The reasoning is D7's own, which already
splits the two roles:

> *"Goldens run against a pinned bundled set registered from bytes; the shipped
> previewer adds system fonts on top."*

The bundled set's job is **golden reproducibility**, not user-facing coverage. A
corpus-derived subset serves that role exactly and honestly. 16 MB of real font
in git history serves it **292× more expensively and no better**, because the
previewer's actual users get their CJK from system fonts at S5 either way.

**This is a test fixture. It must never be described, shipped or reused as a
user-facing asset.** `faces.toml` marks it `derived = true`, and
`cargo run -p s1-faces --release` prints `DERIVED-FIXTURE` beside it on every
run so the distinction cannot quietly erode.

### What it cost, against what it would have cost

No option that renders `cjk.md` in full fits the ~12 MB budget, and it is not
close: the file needs 140 distinct non-ASCII codepoints across four shipped
locales — 79 Han, 31 Hangul syllables, 17 hiragana, 6 katakana, plus CJK and
fullwidth punctuation — so a Simplified-Chinese face alone does not do it. Every
row was measured through parley by `s1-faces --extra`, not inferred from cmaps:

| Option | Added bytes | Total | `cjk.md` | |
|---|---:|---:|---:|---|
| ship nothing | 0 | 4,332,192 | 2/140 | Gate clause unanswered |
| `NotoSansKR-Regular.otf` | 4,644,748 | 8,976,940 | 123/140 | Fits, but Korean-form Han for Chinese text and 17 Simplified chars still tofu |
| `NotoSansJP` + `NotoSansKR` | 9,177,776 | 13,509,968 | 128/140 | Over budget **and** incomplete |
| `NotoSansSC` + `NotoSansKR` | 12,976,084 | 17,308,276 | 140/140 | Cheapest complete multi-file. 44 % over |
| `NotoSansCJKsc-Regular.otf` | 16,437,364 | 20,769,556 | 140/140 | Cheapest complete single file |
| **the subset — chosen** | **56,268** | **4,388,460** | **140/140** | 0.34 % of the input |

Singles for reference: `NotoSansSC-Regular.otf` 8,331,336 B → 109/140;
`NotoSansTC-Regular.otf` 5,683,368 B → 93/140; `NotoSansJP-Regular.otf`
4,533,028 B → 97/140. All OFL 1.1 from
`https://github.com/notofonts/noto-cjk` @ `f8d157532fbfaeda587e826d4cd5b21a49186f7c`.
The google/fonts variable TTFs are worse on every axis — `NotoSansSC[wght].ttf`
alone is 17,772,300 B.

Two zero-byte alternatives, rejected: fetching the CJK face at build time
against a pinned SHA-256 trades committed bytes for a network dependency, and
offline reproducibility is precisely what D7 bought; Git LFS trades bytes for a
checkout prerequisite on every contributor and every CI runner.

### Regenerating it

From the repository root, with `fontTools == 4.55.3`:

```sh
# 1. Fetch the input. 16,437,364 B, sha256
#    2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b
curl -fL -o NotoSansCJKsc-Regular.otf \
  https://raw.githubusercontent.com/notofonts/noto-cjk/f8d157532fbfaeda587e826d4cd5b21a49186f7c/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf

# 2. Every distinct non-ASCII codepoint in the corpus, one U+XXXX per line.
#    `encoding='utf-8'` is not decoration: it is what makes step 3 produce the
#    same bytes on a machine whose default encoding is not UTF-8.
python -c "import glob;print('\n'.join(sorted({'U+%04X'%ord(c) for f in glob.glob('bench/corpus/*.md') for c in open(f,encoding='utf-8').read() if ord(c)>127})))" > corpus-nonascii.txt
#    -> 259 codepoints

# 3. Subset. Keeps every layout feature, because subsetting can drop the tables
#    shaping needs and a font that maps codepoints but cannot shape them is a
#    subtler failure than tofu.
python -m fontTools.subset NotoSansCJKsc-Regular.otf \
    --unicodes-file=corpus-nonascii.txt \
    --layout-features='*' \
    --output-file=NotoSansCJKsc-Regular-corpus-subset.otf
#    -> 56,268 B, sha256
#       05cc026ccff2e5c4954fed438be311a0fe3747caf0826a7117c649a252c0137d
```

Byte-reproducible: two runs produced identical SHA-256s. `pyftsubset` leaves
`head.modified` alone unless `--recalc-timestamp` is passed, which is what makes
that true — do not add that flag.

The subset keeps 143 of the 259 requested codepoints, being the intersection
with what the font has; the other 116 are Hebrew, Arabic, emoji and Latin
punctuation, which the faces above it in the chain cover.

**Verified through parley with the real chain, not through a cmap check**,
because subsetting can drop tables that shaping needs: `cjk.md` goes to
**140/140** and the corpus total from **145/291 to 291/291**, with `cjk.md`'s
whole-file layout at 726 glyphs and **0 `.notdef`**.

### The fragility, as a written contract

**A corpus-derived subset stops covering the corpus the moment the corpus
changes.** Add one Han character to `bench/corpus/cjk.md`, or a new corpus file
in any script, and this font silently stops covering it. That is the price of
the 292× saving, it is accepted deliberately, and it is written here rather than
discovered later.

Two things follow, and both are obligations rather than suggestions:

1. **Any change to `bench/corpus/*.md` requires regenerating this file** with
   the commands above, and updating `bytes`, `sha256` and `[meta]
   total_font_bytes` in `faces.toml`. `cargo run -p s1-faces --release` fails
   loudly if you forget the hash; it reports, but does not fail on, new
   uncovered codepoints.
2. **`cargo xtask layout` (D10) owes a check that does fail on them.** Name:
   **`assert_corpus_fully_covered`**, run before any golden is written or
   compared. For every distinct codepoint in every corpus file it is about to
   lay out, it resolves the codepoint against the registered collection and
   **hard-fails, listing the offending codepoints and their corpus files**, if
   any resolves to glyph id 0 — or to no glyph at all where the codepoint is
   not default-ignorable.

   The reason it has to be a hard failure rather than a warning is that **a
   tofu advance is a perfectly valid-looking golden.** It has a width and a
   position; the file it produces is well-formed. Without this check a corpus
   change quietly rewrites goldens full of `.notdef` boxes and the diff reads
   as an ordinary layout change. This is the same shape of hazard as §3's
   silently-empty `register_fonts`, one level further down the pipeline, and it
   deserves the same answer.

## 7. What the measurements changed

Three things S0 recorded turned out to be imprecise once the set was actually
assembled. Each is a **correction**, not a preference:

1. **The Hebrew half of M3-R10 cost nothing to fix.** M3-R10 says *"DejaVu Sans
   Mono has no Hebrew while `rtl.md` is half Hebrew"*, and D7 concludes a
   Hebrew-capable face is *"a deliverable, not a nicety"*. Both true — but the
   deliverable turned out to be *already inside* the Open Sans that D7 required
   re-sourcing anyway. MarkText shipped the `latin-ext_latin` **web subset**;
   upstream Open Sans 3.003 carries **45 codepoints of U+0590–05FF**, which is
   every Hebrew codepoint in `rtl.md`. `rtl.md` is 49/49 with no face added for
   it. No separate Hebrew font was vendored.
   *Available if wanted*: `NotoSansHebrew-Regular.ttf` (66,156 B) +
   `-Bold.ttf` (66,608 B) = **132,764 B**, adding 43 codepoints of niqqud and
   cantillation (U+0591–05AF, U+05C0–05C6, U+05EF–05F4) that Open Sans lacks
   and that **no corpus file uses**. Left out on D7's own logic: the pinned set
   is sized to the corpus, and the shipped previewer adds system fonts on top.

2. **S0 measured only the upright DejaVu faces, and the obliques are
   different.** D7 records *"DejaVu Sans Mono renders Arabic (0/11 tofu)"*.
   `DejaVuSansMono.ttf` and `-Bold.ttf` carry 99 codepoints in U+0600–06FF;
   `DejaVuSansMono-Oblique.ttf` and `-BoldOblique.ttf` carry **0**, and 0
   Arabic presentation forms. Emphasised Arabic had no covering face at all
   until Noto Sans Arabic was added. This is why entries 5–6 exist and why they
   sit **before** DejaVu Sans Mono in the chain.

3. **`cjk.md` and `emoji.md` are now answered, and they answer differently.**
   D7 left both open. Emoji is solved outright by an upstream face, for
   1,982,596 B, with ZWJ sequences, skin tones and flags all ligating
   correctly. **CJK is not solved at any price under the budget by an upstream
   face** — the cheapest complete one is 16,437,364 B — and is instead solved
   by a corpus-derived 56,268 B fixture, which is a different kind of answer
   with a different kind of obligation attached. §6.

And one thing that is a **judgement call**, labelled as such: Noto Sans Arabic
is not strictly required for `rtl.md`, because the upright DejaVu Sans Mono
covers every Arabic codepoint in it and the chain would reach it. It is in the
set because DejaVu Sans Mono is *monospace* — letting the code font catch
Arabic in a body paragraph renders Arabic prose monospaced, and a golden would
freeze that. That is an argument from typographic correctness, not a
measurement, and 570,024 B is its price.
