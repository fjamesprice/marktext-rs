//! Generate `bench/corpus/` (RUST-REWRITE-PLAN.md §14 step 4).
//!
//! The corpus serves three masters at once, which is why it is worth
//! generating rather than collecting:
//!
//! 1. **The performance regression suite** (§12.1). "Open 5 MB document
//!    ≤ 800 ms" needs a 5 MB document that exists on every machine.
//! 2. **The differential harness corpus** (§11.2). Every file here goes
//!    through both engines on every commit.
//! 3. **The tiling-invariant corpus** (§3, M1 exit gate). "Concatenating every
//!    token's `raw` reproduces the source byte-for-byte" is checked over this.
//!
//! Everything is generated deterministically from a fixed seed, so
//! regeneration is byte-identical and `--check` is meaningful. The PRNG is a
//! four-line xorshift written here rather than pulled in, for the same reason
//! nothing else has dependencies at M0.
//!
//! Provenance for each file is recorded in `bench/corpus/README.md`.

use std::fmt::Write as _;
use std::path::Path;

/// xorshift64*, seeded. Deterministic across platforms and Rust versions —
/// `rand`'s generators are explicitly not, which is why this is hand-rolled.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }

    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() % (hi - lo) as u64) as usize
    }
}

const WORDS: &[&str] = &[
    "markdown",
    "editor",
    "document",
    "paragraph",
    "heading",
    "fence",
    "inline",
    "token",
    "range",
    "offset",
    "cluster",
    "glyph",
    "shaping",
    "layout",
    "render",
    "caret",
    "selection",
    "buffer",
    "rope",
    "arena",
    "revision",
    "dirty",
    "viewport",
    "scroll",
    "reflow",
    "baseline",
    "ligature",
    "bidi",
    "grapheme",
    "codepoint",
    "encoding",
    "fixture",
    "invariant",
    "ratchet",
    "conformance",
    "serializer",
    "round",
    "trip",
    "lossless",
    "verbatim",
    "marker",
    "reveal",
    "decoration",
    "widget",
    "atlas",
    "raster",
    "incremental",
    "latency",
    "throughput",
    "budget",
];

fn sentence(rng: &mut Rng) -> String {
    let n = rng.range(6, 18);
    let mut words: Vec<String> = (0..n).map(|_| (*rng.pick(WORDS)).to_string()).collect();
    // Sprinkle inline syntax so the tokenizer has real work to do rather than
    // measuring plain-text throughput.
    match rng.next() % 6 {
        0 => words[n / 2] = format!("**{}**", words[n / 2]),
        1 => words[n / 2] = format!("_{}_", words[n / 2]),
        2 => words[n / 2] = format!("`{}`", words[n / 2]),
        3 => words[n / 2] = format!("[{}](https://example.com/{})", words[n / 2], words[n / 3]),
        4 => words[n / 2] = format!("~~{}~~", words[n / 2]),
        _ => {}
    }
    let mut s = words.join(" ");
    if let Some(first) = s.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    s.push('.');
    s
}

fn paragraph(rng: &mut Rng) -> String {
    let n = rng.range(2, 6);
    (0..n).map(|_| sentence(rng)).collect::<Vec<_>>().join(" ")
}

/// Prose-like markdown of at least `target` bytes, cut at a block boundary so
/// the file never ends mid-construct.
fn prose(seed: u64, target: usize) -> String {
    let mut rng = Rng::new(seed);
    let mut out = String::with_capacity(target + 4096);
    let mut section = 0usize;

    while out.len() < target {
        section += 1;
        let _ = writeln!(out, "## Section {section}\n");
        let _ = writeln!(out, "{}\n", paragraph(&mut rng));

        match rng.next() % 5 {
            0 => {
                let _ = writeln!(
                    out,
                    "- {}\n- {}\n- {}\n",
                    sentence(&mut rng),
                    sentence(&mut rng),
                    sentence(&mut rng)
                );
            }
            1 => {
                let _ = writeln!(
                    out,
                    "```rust\nfn {}() -> usize {{\n    {}\n}}\n```\n",
                    rng.pick(WORDS),
                    rng.range(0, 1000)
                );
            }
            2 => {
                let _ = writeln!(out, "> {}\n", sentence(&mut rng));
            }
            3 => {
                let _ = writeln!(
                    out,
                    "| {} | {} |\n| --- | ---: |\n| {} | {} |\n",
                    rng.pick(WORDS),
                    rng.pick(WORDS),
                    rng.range(0, 99),
                    rng.pick(WORDS)
                );
            }
            _ => {
                let _ = writeln!(out, "{}\n", paragraph(&mut rng));
            }
        }
    }
    out
}

fn readme_like() -> String {
    let mut out = String::new();
    out.push_str("# marktext-rs corpus — 10 KB README\n\n");
    out.push_str(
        "A README-shaped document: the single most common thing a markdown editor is\n\
         pointed at. Deliberately dense in *variety* rather than volume — headings at\n\
         several levels, a table, nested lists, a fenced block, links, images,\n\
         reference definitions, and a footnote.\n\n",
    );
    out.push_str("## Install\n\n```sh\ncargo install marktext-rs\n```\n\n");
    out.push_str("## Why\n\n");
    let mut rng = Rng::new(0x10_5B);
    for _ in 0..6 {
        out.push_str(&paragraph(&mut rng));
        out.push_str("\n\n");
    }
    out.push_str("### Features\n\n");
    for i in 1..=12 {
        let _ = writeln!(out, "{i}. {}", sentence(&mut rng));
    }
    out.push('\n');
    out.push_str("- [x] CommonMark\n- [x] GFM\n- [ ] Collaborative editing\n\n");
    out.push_str("| Metric | Target | Stretch |\n| --- | ---: | ---: |\n");
    out.push_str("| Cold start | 250 ms | 100 ms |\n| RSS, empty | 60 MB | 35 MB |\n");
    out.push_str("| Open 5 MB | 800 ms | 300 ms |\n\n");
    out.push_str("See [the plan][plan] and ![a diagram](./diagram.png \"Architecture\").\n\n");
    out.push_str("Reference definitions round-trip as paragraphs[^why].\n\n");
    out.push_str("[plan]: https://example.com/plan \"The build plan\"\n");
    out.push_str("[^why]: Because muya stores them that way, and the state shape is 1:1.\n\n");
    out.push_str("---\n\n");
    while out.len() < 10 * 1024 {
        out.push_str(&paragraph(&mut rng));
        out.push_str("\n\n");
    }
    out
}

fn code_fences(n: usize) -> String {
    let mut out = String::new();
    out.push_str("# 50 code fences\n\n");
    out.push_str(
        "Exercises the two code-block constraints from §2 that are easiest to lose in\n\
         translation: `info` is the **full verbatim info string** (the highlight language\n\
         is only its first word), and `fence_len` must be preserved.\n\n",
    );
    let langs = [
        "rust", "js", "ts", "python", "go", "c", "cpp", "java", "sh", "yaml", "json", "toml",
        "html", "css", "sql", "diff", "",
    ];
    let mut rng = Rng::new(0xFE_CE);
    for i in 0..n {
        let lang = langs[i % langs.len()];
        // Every fourth fence carries a full info string, and every seventh
        // uses a longer fence — both are round-trip hazards.
        let info = match i % 4 {
            0 if !lang.is_empty() => format!("{lang} title=\"example-{i}.txt\" {{.numberLines}}"),
            _ => lang.to_string(),
        };
        let ticks = if i % 7 == 0 { "````" } else { "```" };
        let _ = writeln!(out, "## Fence {i} — `{info}`\n");
        let _ = writeln!(out, "{ticks}{info}");
        let _ = writeln!(out, "let value_{i} = {};", rng.range(0, 10_000));
        if i % 7 == 0 {
            // A three-backtick run inside a four-backtick fence: normalising
            // fence_len to 3 corrupts this file.
            let _ = writeln!(out, "``` not a fence close ```");
        }
        let _ = writeln!(out, "{ticks}\n");
    }
    out.push_str("An indented code block, which has no fence and therefore no fence length:\n\n");
    out.push_str("    indented = true\n    also_indented = true\n\n");
    out
}

fn inline_math(n: usize) -> String {
    let mut out = String::new();
    out.push_str("# 100 inline math spans\n\n");
    out.push_str("Inline math is a *replaced element* in inline layout (§5) — a parley\n`InlineBox` with a baseline, not a bitmap. This file is the density test.\n\n");
    let mut rng = Rng::new(0x4A7);
    for i in 0..n {
        if i % 10 == 0 {
            let _ = writeln!(
                out,
                "\n$$\n\\sum_{{k=0}}^{{{}}} \\frac{{1}}{{k^2}} = \\frac{{\\pi^2}}{{6}}\n$$\n",
                rng.range(2, 99)
            );
        }
        let _ = writeln!(
            out,
            "Line {i}: the value $x_{{{}}} = \\alpha^{} + \\beta$ appears inline, and \\$5 is not math.",
            rng.range(0, 99),
            rng.range(2, 9)
        );
    }
    out.push_str("\nEscaped delimiters: \\$not math\\$ and a literal `$` in code.\n");
    out
}

fn tables(n: usize) -> String {
    let mut out = String::new();
    out.push_str("# 20 tables\n\n");
    out.push_str("Tables need two layout passes (§5): measure natural column widths, then\ndistribute available width. This is the input that makes that expensive.\n\n");
    let mut rng = Rng::new(0x7AB1E);
    let aligns = [
        (":---", ":---:", "---:"),
        ("---", ":---", "---:"),
        (":---:", ":---:", ":---:"),
    ];
    for t in 0..n {
        let (a, b, c) = aligns[t % aligns.len()];
        let _ = writeln!(out, "## Table {t}\n");
        let _ = writeln!(out, "| Left | Centre | Right |");
        let _ = writeln!(out, "| {a} | {b} | {c} |");
        for _ in 0..rng.range(3, 9) {
            let _ = writeln!(
                out,
                "| {} | **{}** | {} |",
                rng.pick(WORDS),
                rng.pick(WORDS),
                rng.range(0, 100_000)
            );
        }
        // A cell containing an escaped pipe, which is the classic GFM table
        // round-trip hazard.
        let _ = writeln!(
            out,
            "| a \\| b | `x \\| y` | [link](https://example.com) |\n"
        );
    }
    out
}

/// The five block kinds no other corpus file contains.
///
/// **Added at M3 S1, and the reason is D11.** `cargo xtask layout`'s goldens
/// are M3-R6's instrument — *"the first golden for each block kind is read by
/// eye once"* — and until this file existed, five of `mt_doc::Block`'s nineteen
/// variants appeared in no golden at all: `setext-heading`, `html-block`,
/// `frontmatter`, `diagram` and `footnote`. Three of those five are D11's own
/// arms, so S1 would have shipped the decision implemented and never once
/// exercised by the artifact that is supposed to verify it.
///
/// Three constraints shaped the content, and all three are load-bearing:
///
/// 1. **ASCII only.** `assets/fonts/NotoSansCJKsc-Regular-corpus-subset.otf` is
///    a `pyftsubset` derived from this directory, so a codepoint outside the
///    subset silently stops being covered. `assert_corpus_fully_covered` in
///    `cargo xtask layout` fails loudly if that ever happens; the answer is to
///    fix the corpus file, not to re-subset the font.
/// 2. **Front matter must be the first bytes of the file** — `mt_md::block`
///    only looks for it at offset 0 — so this file opens with it rather than
///    with a heading.
/// 3. **Small enough to read.** ~1.6 KB. The golden it produces is the one a
///    human opens to review five block kinds at once, so a wall of generated
///    prose would defeat its purpose.
///
/// A `Footnote` block additionally needs `Options::footnote`, which
/// `Options::MUYA_DEFAULT` sets to `false` because muya does. `xtask layout`
/// therefore parses **this file and only this file** with the extension on, and
/// says so in the golden's `parse` header line. Every other harness reads this
/// file at `MUYA_DEFAULT` like all the others, so the `[^why]:` line is a
/// paragraph to them — exactly what this README says reference definitions are.
fn block_kinds() -> String {
    // Deliberately a literal rather than a generated document: the point of
    // this input is that a reviewer can read it beside the golden it produces,
    // and the PRNG above would make that harder rather than easier.
    String::from(
        r#"---
title: Block kinds
lang: en
tags: [layout, goldens]
---

Setext heading, level one
=========================

Every block kind below appears in no other corpus file. This file exists so the
layout goldens carry one readable instance of every `mt_doc::Block` variant, and
so the four kinds that share the code-block box are exercised by an artifact
rather than only by a unit test.

Setext heading, level two
-------------------------

## Raw HTML

<div class="callout">
  <strong>Raw HTML</strong> lays out as its own source in the code-block style,
  because there is no HTML engine here and the honest presentation of a
  permanent failure state is the source with its language named.
</div>

A paragraph after the block, so the golden shows the HTML in flow rather than in
isolation.

## Diagrams

All five of the diagram kinds, so that both diagram languages occur: `vega-lite`
carries JSON and the other four carry YAML.

```mermaid
graph TD
  A[parse] --> B[layout]
  B --> C[render]
```

```plantuml
@startuml
Parser -> Layout: Document
@enduml
```

```vega-lite
{"mark": "bar", "data": {"values": [{"a": 1}, {"a": 3}, {"a": 2}]}}
```

```flowchart
st=>start: Open file
e=>end: Draw
st->e
```

```sequence
Alice->Bob: a display list
Bob-->Alice: pixels
```

## Footnotes

Footnotes are off in muya's own default options[^why], so this is the only
corpus file the layout goldens parse with the extension on, and the golden's
`parse` header line says which options produced it.

[^why]: `Options::MUYA_DEFAULT` sets `footnote: false`, matching muya's config.
"#,
    )
}

fn rtl() -> String {
    let mut out = String::new();
    out.push_str("# RTL and mixed bidirectional text\n\n");
    out.push_str(
        "MarkText has an upstream RTL fix in its history (`43bd8b77`), so the\n\
         Unicode Bidirectional Algorithm is a live requirement, not a theoretical one\n\
         (NATIVE-REWRITE-PLAN.md §3 item 3). Every line below mixes scripts, because\n\
         pure-RTL text hides exactly the bugs that matter.\n\n",
    );
    out.push_str("## العربية\n\n");
    out.push_str("هذا نص عربي يحتوي على **نص عريض** وكلمة إنجليزية Rust في المنتصف.\n\n");
    out.push_str("- عنصر أول مع رقم 12345\n- عنصر ثانٍ مع `code span`\n- [رابط](https://example.com/عربي)\n\n");
    out.push_str("## עברית\n\n");
    out.push_str("שורה בעברית עם *הדגשה* ומילה באנגלית parley באמצע המשפט.\n\n");
    out.push_str("> ציטוט בעברית שמכיל 42 ומספרים נוספים 3.14159.\n\n");
    out.push_str("## Mixed in one paragraph\n\n");
    out.push_str(
        "An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,\n\
         then a number 2026 and a URL https://example.com/mixed — all in one logical\n\
         line, which is where caret motion and hit-testing go wrong.\n\n",
    );
    out.push_str("| المفتاح | القيمة |\n| --- | ---: |\n| الأول | 1 |\n| الثاني | 2 |\n\n");
    out
}

fn cjk() -> String {
    let mut out = String::new();
    out.push_str("# CJK\n\n");
    out.push_str(
        "`zh-CN`, `zh-TW`, `ja` and `ko` are shipped locales, so CJK correctness is a\n\
         requirement rather than a nice-to-have (§7, §10). This file also carries the\n\
         CJK flanking cases for `strong`, which `inlineRenderer/__tests__` covers and\n\
         which the lexer port must reproduce exactly (§3 rule 3).\n\n",
    );
    out.push_str(
        "## 简体中文\n\n这是一段中文文本，包含**粗体**和*斜体*，以及一个 `代码片段`。\n\n",
    );
    out.push_str("中文**粗体**紧邻中文字符，没有空格——这是 CJK flanking 的关键用例。\n\n");
    out.push_str("## 繁體中文\n\n這是一段繁體中文，包含[連結](https://example.com/中文)與圖片 ![替代文字](./x.png)。\n\n");
    out.push_str("## 日本語\n\n日本語の文章です。**太字**と*斜体*、そして`コード`を含みます。\n\n");
    out.push_str(
        "平仮名、片仮名（カタカナ）、漢字が混在する行。IME の変換途中でも壊れないこと。\n\n",
    );
    out.push_str(
        "## 한국어\n\n한국어 문장입니다. **굵게**와 *기울임*, 그리고 `코드`를 포함합니다.\n\n",
    );
    out.push_str("- 첫 번째 항목\n- 두 번째 항목\n- 세 번째 항목\n\n");
    out.push_str("## Line breaking\n\n");
    out.push_str(
        "中文没有空格所以断行规则完全依赖UAX14的CJK规则而不是空格这一行故意很长用来触发换行。\n\n",
    );
    out.push_str("| 列一 | 列二 | 列三 |\n| --- | :---: | ---: |\n| 中文 | 日本語 | 한국어 |\n| 值 | 値 | 값 |\n\n");
    out
}

fn emoji() -> String {
    let mut out = String::new();
    out.push_str("# Emoji-heavy\n\n");
    out.push_str(
        "Emoji are the cheapest way to break grapheme-cluster handling: ZWJ sequences,\n\
         skin-tone modifiers, and regional-indicator flags are all multi-codepoint\n\
         single graphemes. Arrow keys must not split them (§5, via parley's cluster\n\
         API), and `mt-inline` must not tokenise inside them.\n\n",
    );
    out.push_str("## Simple\n\n😀 😃 😄 😁 😆 😅 🤣 😂 🙂 🙃 😉 😊 😇 🥰 😍 🤩 😘 😗 ☺️ 😚\n\n");
    out.push_str("## ZWJ sequences (one grapheme each)\n\n");
    out.push_str("👨‍👩‍👧‍👦 👩‍👩‍👦 👨‍👨‍👧‍👧 👩‍💻 👨‍🚀 🧑‍🔬 🏳️‍🌈 🏴‍☠️ 👁️‍🗨️\n\n");
    out.push_str("## Skin-tone modifiers\n\n");
    out.push_str("👋🏻 👋🏼 👋🏽 👋🏾 👋🏿 🤝🏽 👨🏿‍🦱 👩🏻‍🦰\n\n");
    out.push_str("## Regional-indicator flags\n\n🇬🇧 🇺🇸 🇯🇵 🇰🇷 🇨🇳 🇹🇼 🇩🇪 🇫🇷 🇪🇸 🇧🇷 🇮🇳 🇸🇦 🇮🇱\n\n");
    out.push_str("## Emoji adjacent to inline syntax\n\n");
    out.push_str(
        "**🎉bold🎉** and *🚀italic🚀* and `🐛code🐛` and [🔗link🔗](https://example.com).\n\n",
    );
    out.push_str("Shortcode form: :smile: :rocket: :+1: — muya lexes these as emoji tokens,\nand the word-boundary rule around them is covered by `emojiWordBoundary.spec.ts`.\n\n");
    out.push_str("## Mixed with other scripts\n\n");
    out.push_str("Hello 👋 世界 🌏 مرحبا 🕌 שלום 🕎 — one line, four scripts, five emoji.\n\n");
    out.push_str("| Emoji | Name | Codepoints |\n| :---: | --- | --- |\n");
    out.push_str("| 👨‍👩‍👧‍👦 | family | 7 |\n| 🏳️‍🌈 | rainbow flag | 4 |\n| 👋🏿 | wave, dark | 2 |\n\n");
    out
}

/// Every corpus file: `(filename, contents)`.
pub fn files() -> Vec<(&'static str, String)> {
    vec![
        ("empty.md", String::new()),
        ("10kb.md", readme_like()),
        ("250kb.md", prose(0x250, 250 * 1024)),
        ("1mb.md", prose(0x1_000, 1024 * 1024)),
        ("5mb.md", prose(0x5_000, 5 * 1024 * 1024)),
        ("50-code-fences.md", code_fences(50)),
        ("100-inline-math.md", inline_math(100)),
        ("20-tables.md", tables(20)),
        ("block-kinds.md", block_kinds()),
        ("rtl.md", rtl()),
        ("cjk.md", cjk()),
        ("emoji.md", emoji()),
    ]
}

/// `cargo xtask corpus [--check]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let check = args.iter().any(|a| a == "--check");
    for arg in args {
        if arg != "--check" {
            return Err(format!("unrecognised argument: {arg}"));
        }
    }

    let dir = repo_root.join("bench").join("corpus");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let mut drift = false;
    for (name, contents) in files() {
        let path = dir.join(name);
        if check {
            let existing = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            // Compare with CRLF normalised: a git checkout with
            // core.autocrlf=true rewrites line endings, and that is not drift.
            if existing.replace("\r\n", "\n") != contents {
                println!("DRIFT  {name}");
                drift = true;
            } else {
                println!("ok     {name} ({} bytes)", contents.len());
            }
        } else {
            std::fs::write(&path, &contents)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            println!("wrote  {name} ({} bytes)", contents.len());
        }
    }

    if drift {
        println!("\nbench/corpus/ does not match the generator in xtask/src/corpus.rs.");
        println!("Run `cargo xtask corpus` and commit, or revert the hand edit.");
        return Ok(1);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = files();
        let b = files();
        for ((n1, c1), (n2, c2)) in a.iter().zip(&b) {
            assert_eq!(n1, n2);
            assert_eq!(c1, c2, "{n1} is not deterministic");
        }
    }

    #[test]
    fn every_file_from_section_14_step_4_is_present() {
        let names: Vec<&str> = files().iter().map(|(n, _)| *n).collect();
        for expected in [
            "empty.md",
            "10kb.md",
            "250kb.md",
            "1mb.md",
            "5mb.md",
            "50-code-fences.md",
            "100-inline-math.md",
            "20-tables.md",
            "rtl.md",
            "cjk.md",
            "emoji.md",
        ] {
            assert!(names.contains(&expected), "missing corpus file {expected}");
        }
        // §14 step 4's eleven, plus `block-kinds.md` — added at M3 S1 and not
        // in the plan's list, because the plan's list predates the layout
        // goldens that need it. See `block_kinds`.
        assert!(names.contains(&"block-kinds.md"));
        assert_eq!(names.len(), 12);
    }

    /// The one file whose whole purpose is *which constructs it contains*.
    ///
    /// A generated corpus is verified by `--check` against its generator, which
    /// proves the bytes did not drift but says nothing about whether they still
    /// carry the five block kinds `block_kinds` exists to supply. If someone
    /// edits the literal and drops the front matter, `--check` stays green and
    /// only the layout golden notices — one harness later than it should.
    #[test]
    fn block_kinds_carries_the_five_constructs_no_other_corpus_file_has() {
        let by_name: std::collections::HashMap<&str, String> = files().into_iter().collect();
        let src = &by_name["block-kinds.md"];
        assert!(src.starts_with("---\n"), "front matter must be at offset 0");
        assert!(src.contains("\n=========================\n"), "setext h1");
        assert!(src.contains("\n-------------------------\n"), "setext h2");
        assert!(src.contains("\n<div class=\"callout\">\n"), "html block");
        assert!(src.contains("\n[^why]: "), "footnote definition");
        for kind in ["mermaid", "plantuml", "vega-lite", "flowchart", "sequence"] {
            assert!(src.contains(&format!("\n```{kind}\n")), "{kind} diagram");
        }
        // The CJK face is a pyftsubset of this directory; a non-ASCII byte here
        // is a coverage regression waiting to happen. See `block_kinds`.
        assert!(src.is_ascii(), "block-kinds.md must be pure ASCII");
    }

    #[test]
    fn size_targets_are_met() {
        let by_name: std::collections::HashMap<&str, String> = files().into_iter().collect();
        assert_eq!(by_name["empty.md"].len(), 0);
        for (name, min) in [
            ("10kb.md", 10 * 1024),
            ("250kb.md", 250 * 1024),
            ("1mb.md", 1024 * 1024),
            ("5mb.md", 5 * 1024 * 1024),
        ] {
            let len = by_name[name].len();
            assert!(len >= min, "{name} is {len} bytes, want at least {min}");
            // Generation stops at the first block boundary past the target, so
            // overshoot should be small.
            assert!(
                len < min + 8192,
                "{name} overshot: {len} bytes for a {min}-byte target"
            );
        }
    }

    #[test]
    fn counted_files_contain_what_their_names_claim() {
        let by_name: std::collections::HashMap<&str, String> = files().into_iter().collect();
        assert_eq!(
            by_name["50-code-fences.md"].matches("\n## Fence ").count(),
            50
        );
        assert_eq!(
            by_name["100-inline-math.md"].matches("\nLine ").count(),
            100
        );
        assert_eq!(by_name["20-tables.md"].matches("\n## Table ").count(), 20);
    }

    /// The fence file must actually exercise `fence_len`: if every fence is
    /// three backticks, it is not testing the constraint it claims to.
    #[test]
    fn the_fence_corpus_exercises_fence_length_and_full_info_strings() {
        let by_name: std::collections::HashMap<&str, String> = files().into_iter().collect();
        let fences = &by_name["50-code-fences.md"];
        assert!(fences.contains("````"), "no four-backtick fence");
        assert!(fences.contains("title=\""), "no multi-word info string");
        assert!(
            fences.contains("    indented = true"),
            "no indented code block"
        );
    }

    #[test]
    fn every_file_is_valid_utf8_and_uses_lf() {
        for (name, contents) in files() {
            assert!(!contents.contains('\r'), "{name} contains CR");
        }
    }
}
