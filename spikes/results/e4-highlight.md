# E4 — `mt-highlight`'s engine (M3 S0, D3, C11)

## The answer

**D3 does not resolve to a single winner — the three axes point three
different ways, and the strongest one (differential-testability) is the axis
C11's own argument under-weighted.** Measured on this machine:

| Axis | `syntect` 5.3.0 (regex-onig) | `tree-sitter` 0.26.12 | Prism-grammar port |
|---|---|---|---|
| Time to first highlight (cold, `50-code-fences.md`'s fence 0) | **≈2.5 ms** (setup 0.77 ms + first fence 1.75 ms) | **15.3–80.9 ms**, depending on how many grammars must configure before the first fence highlights (lazy 1 grammar vs. eager 14) | Not spiked — a port has no crate to run. Est. **single-digit ms**: no C static-init, only regex compilation, and Prism's own eager languages (`latex`, `yaml`) are already a precedent for "compile a few regexes at startup" |
| Steady-state throughput, code-dense file (120,000 lines, 6,089,569 B, 2,000 fences) | **24,815–24,344 lines/s, 1.235–1.259 MB/s** (best-of-5) | **41,060–41,394 lines/s, 2.084–2.101 MB/s** (best-of-5) — tree-sitter is **~1.65× faster** here | Not spiked. `fancy-regex` is *"about half the speed"* of `regex` per C11's own sourced claim, so plausibly slower than both, but no number exists |
| Binary delta (cited from `spikes/results/e2-binary-size.md`, **not remeasured**) | **≈+1.00 MB** marginal linkage (1,098,240 B standalone − 98,304 B floor; see correction below — this is **not** the "+1.178 MB" figure a literal reading of E2's scenario table suggests) | **+1.180 MB for 1 grammar, +25.907 MB for 20** (≈1.301 MB marginal/grammar) — **the single largest lever in E2's whole binary-size budget** | Grammars are data: all 297 non-minified Prism grammar source files total **806,895 B (788 KiB)** on disk. The regex engine itself (`fancy-regex` + its `regex-automata`/`aho-corasick`/`regex-syntax` stack) is **already sunk** — `mt-inline` depends on it — so marginal cost is plausibly **well under 1.5 MB for all 297 languages**, not per-language |
| Language coverage of MarkText's baseline (see correction: **297**, not 298 — §below) | **75 bundled syntaxes → 44/297 Prism IDs (≈15%)**, measured by running the real crate. Notable misses: **TypeScript, TOML** (both absent from the default bundle, confirmed directly) | **20 grammar crates proven to build (E2)**; ecosystem coverage of the 297 is an **unverified estimate, 35–50%** — no crates.io access from this machine | **297/297 by construction** — a port doesn't cover a subset, it re-derives the reference |
| Contract fit (byte ranges) | **No adapter needed if you use the public `RangedHighlightIterator`** (`Item = (Style, &str, Range<usize>)`) — C11's claim only holds for the convenience `easy::HighlightLines` wrapper, and even there the adapter is 7 lines / ~11 ns per token | **Direct fit**, as C11 says: `HighlightEvent::{Source{start,end}, HighlightStart, HighlightEnd}` | Whatever the port's own tokenizer chooses — free to design byte ranges in from the start |
| Differential-testability against Prism (S3's `--require-ts` gate) | **Weak.** Different tokenization grammar (Sublime `.sublime-syntax` scope stacks) will disagree with Prism's regex/state-machine boundaries constantly, on inputs where both are "right" | **Weak**, same reason, different grammar formalism (context-free grammar node boundaries vs. Prism's regex scanner) | **Strong, close to trivial.** Same grammar rules translated, not reinvented — boundary agreement is close to the null hypothesis, and disagreement is diagnostic of a port bug, not engine disagreement |

The size axis (D3 dominates D1 per E2) and the throughput axis both lean
`tree-sitter`; the cold-start and differential-testability axes both lean hard
away from it. Coverage leans toward whichever engine's data governs a given
language — and a Prism port is the only option with no coverage question.
**No single number added up here decides D3.** See "What this does not
decide" below.

---

## Versions, environment, corpus

| | |
|---|---|
| toolchain | cargo 1.97.1 / rustc 1.97.1, Windows 11, `x86_64-pc-windows-msvc`, `--release` |
| `syntect` | `5.3.0`, `default-onig` feature (matches E2's binary-size build and the default a bare `cargo add syntect` gives) |
| `tree-sitter` | `0.26.12` (core) |
| `tree-sitter-highlight` | `0.26.12` — released alongside core, confirmed by `cargo add` resolving it cleanly against `tree-sitter = "0.26.12"` with no version conflict |
| Grammar crates (14, one per corpus language that has one) | `tree-sitter-{rust 0.24.2, javascript 0.25.0, typescript 0.23.2, python 0.25.0, c 0.24.2, cpp 0.23.4, java 0.23.5, go 0.25.0, bash 0.25.1, yaml 0.7.2, json 0.24.8, toml-ng 0.7.0, html 0.23.2, css 0.25.0}` — the E2 20-grammar set, trimmed to what `50-code-fences.md` and the code-dense file actually name, plus `toml-ng` added back in (the corpus has TOML fences, E2's build didn't need it) |
| Inputs | `bench/corpus/50-code-fences.md` (51 code blocks: 50 fences + 1 indented block, 1,230 bytes of code text, 16 distinct declared languages) and `spikes/.gen/code-dense.md` (**already present from E3** — `e3 gen-code-dense`, seed `0xE3C0DE`, 2,000 rust fences × 60 lines = 120,000 code lines, 6,089,569 bytes of code, 6,144,687-byte file. Not regenerated; E3's copy was reused directly) |
| Fence extraction | Real `mt_md::parse` + `Block::highlight_language()` (`spikes/e4-common`), **not** a hand-rolled fence regex — so a corpus quirk (the four-backtick fences containing a three-backtick run, `title="…" {.numberLines}` info strings) is parsed the same way `mt-highlight` will actually see it |

---

## Method

Two new spike crates, `spikes/e4-syntect` and `spikes/e4-treesitter`, both
path-depending on a new `spikes/e4-common` (which path-depends on `mt-md` +
`mt-doc`, the E1/E3 precedent for driving the real parser rather than a
stand-in). Each engine crate is a single binary with subcommands, one process
launch per scenario — the same discipline E3 used for peak-RSS numbers, here
because "how long does *this process's* first regex/query compile take" would
be contaminated by anything that ran before it in the same process.

```text
e4-syntect.exe    cold FILE | throughput FILE REPS | adapter [FILE] | coverage
e4-treesitter.exe cold FILE [--eager] | throughput FILE REPS | coverage
```

**What "cold" times, and what it deliberately excludes.** From `Instant::now()`
at the top of `main` (process/CRT startup before that point is not observable
from inside the binary — stated once here rather than repeated per engine) to
the first highlighted span/event of the first non-empty fence. File read and
`mt_md::parse` happen **before** the timer starts and are excluded: per
`mt-highlight`'s own contract (`crates/mt-highlight/src/lib.rs`), the crate
receives a code block's text and a language, nothing else — parsing the
enclosing markdown is `mt-md`'s cost, not the highlighter's.

**What "throughput" times.** A warm-up pass (untimed, discarded) then
best-of-*N* timed passes over every code block in the file, each highlighted
independently from a fresh parse/highlight state (matching real per-block
highlighting — no cross-fence continuation). *N* and the full per-rep spread
are printed, not just the best.

Hardware: same machine as E1–E3 (see `spikes/results/e2-binary-size.md`'s
environment table; this experiment did not re-collect CPU/RAM/disk specs
since nothing here is a peak-RSS or scroll-fps number sensitive to them
beyond what's already on record).

---

## `syntect` — detail

### Cold start

7 process launches, `e4-syntect.exe cold ../bench/corpus/50-code-fences.md`:

```
setup=762.9µs  first_span_at=1.7297ms  fence_total=1.7452ms  total=2.5081ms
setup=766.6µs  first_span_at=1.6621ms  fence_total=1.6778ms  total=2.4444ms
setup=775.2µs  first_span_at=1.7709ms  fence_total=1.7861ms  total=2.5613ms
setup=765.6µs  first_span_at=1.7505ms  fence_total=1.766ms   total=2.5316ms
setup=780.2µs  first_span_at=1.7671ms  fence_total=1.7828ms  total=2.563ms
setup=773µs    first_span_at=1.7415ms  fence_total=1.7573ms  total=2.5303ms
setup=763.7µs  first_span_at=1.6625ms  fence_total=1.6783ms  total=2.442ms
```

`setup` is `SyntaxSet::load_defaults_newlines()` + `ThemeSet::load_defaults()`
— **0.76–0.78 ms**, tightly clustered, confirming the upstream "~23 ms whole
bundle load" claim C11 cites is not just plausible but **conservative on this
hardware by roughly 30×** (0.77 ms measured vs. ~23 ms claimed) — worth
recording as a place the plan under-states syntect's number in syntect's
*favour*, the rare direction. `fence_total` (highlighting fence 0's one line,
`let value_0 = 5744;`, tagged `rust`) is **1.66–1.79 ms**, dominated by
Oniguruma's onig_sys lazy regex compilation for the Rust syntax's patterns on
first use — this is the "lazy regex compilation" upstream credits for the
fast bundle load; it does not eliminate compilation cost, it moves it to here.
Total time to first highlighted span: **2.44–2.56 ms**, i.e. **≈1% of the
250 ms cold-start budget**, for the highlighter alone.

### Steady-state throughput

Code-dense file, best-of-5, warm-up pass discarded:

```
[syntect throughput] fences=2000 total_lines=120000 total_bytes=6089569 lang_misses=0
reps=5 best=4.8358436s worst=5.0056032s lines_per_sec=24815 mb_per_sec=1.259
  rep0: 4.8463738s spans=1722179
  rep1: 4.8358436s spans=1722179
  rep2: 5.0056032s spans=1722179
  rep3: 4.879622s spans=1722179
  rep4: 4.914875s spans=1722179
```

**24,344–24,815 lines/s, 1.235–1.259 MB/s.** Upstream's two claims (C11):
9,200 lines of jQuery in 600 ms (**15,333 lines/s**) and "50,000 lines/s on
simple syntaxes." Measured throughput sits **above** the jQuery figure and
**below half** the "simple syntaxes" ceiling — plausible, not exactly either
claim, and the corpus explains why: every fence is `let value_N = NNNN;`
tagged `rust`, and Rust's syntax (generics, lifetimes, macros) is not
"simple" in Sublime-syntax terms even though the actual matched content is
trivial. **1,722,179 spans over 120,000 lines is ~14.4 spans/line** — high
span density per line is exactly what a scope-stack-based grammar spends the
most time on, so this number is believable as "harder than jQuery, easier
than the ceiling" rather than a contradiction of either upstream figure.

### The adapter C11 says is needed

```
[syntect adapter] easy_api: pieces_converted=37 adapter_time_total=400ns
  adapter_time_per_piece=10.8ns contiguous_and_covers_line=true
[syntect adapter] ranged_iterator (no adapter, public API): regions=37
  total_time=61.5µs — RangedHighlightIterator<Item = (Style, &str, Range<usize>)>
  is already the contract
```

**C11's claim is only true for one of syntect's two public entry points.**
`easy::HighlightLines::highlight_line` does return `Vec<(Style, &str)>`, and
recovering byte ranges from it costs exactly what was written and measured: a
7-line pointer-arithmetic function (`piece.as_ptr() as usize - line.as_ptr()
as usize`, valid because syntect's own doc comment guarantees the pieces are
contiguous, in-order subslices of the input line — checked at runtime here,
not just trusted), **10.8 ns per token**. But one layer below that
convenience wrapper, syntect's own public
`syntect::highlighting::RangedHighlightIterator` (`highlighter.rs:74`,
`Item = (Style, &'b str, Range<usize>)`) **already yields the byte range
directly** — no adapter, no pointer arithmetic, nothing to write. `mt-highlight`
would enter through this iterator, not through `easy::HighlightLines`, and the
"needs an adapter" line in C11/§5 D3 would not apply at all. This is corrected
below under "Where this contradicts the plan."

### Coverage — what the default bundle actually ships

```
[syntect coverage] syntaxes=75
```

Full list dumped and inspected (`spikes/.gen/syntect-coverage.txt`, generated
by `e4-syntect.exe coverage`, 75 syntax names + 229 distinct
extensions/file-name tokens). Of the 75, roughly 8 are non-language utility
syntaxes (Cargo Build Results, JavaDoc, LaTeX Log, Make Output, R Console,
Plain Text, camlp4, `commands-builtin-shell-bash`) and several are dialect
variants of one base language (5 HTML flavours, 3 "Regular Expression"
flavours, 3 Rails flavours, 3 OCaml-tooling entries) — distinct language
*families* are closer to 50–55. **Confirmed absent, by direct lookup against
the running `SyntaxSet` (not by list-reading): TypeScript, TOML.** Both are
declared languages in `50-code-fences.md` and both silently fall back to
plain text under this bundle. See the corpus intersection table below for
what that costs on this project's own fixture, not a hypothetical one.

---

## `tree-sitter` — detail

### Cold start — and why the fair number depends on the loading policy

```
lazy (1 grammar built — only the fence's own language), 7 launches:
grammars_configured=1  setup=15.5ms    first_event_at=152.7µs  total=15.6593ms
grammars_configured=1  setup=15.3141ms first_event_at=127.8µs  total=15.4489ms
grammars_configured=1  setup=15.2017ms first_event_at=125.8µs  total=15.3344ms
grammars_configured=1  setup=15.108ms  first_event_at=124.4µs  total=15.2393ms
grammars_configured=1  setup=15.3022ms first_event_at=127.6µs  total=15.437ms
grammars_configured=1  setup=15.3131ms first_event_at=127.8µs  total=15.4478ms
grammars_configured=1  setup=15.1733ms first_event_at=122.3µs  total=15.3023ms

eager (all 14 linked grammars built before the first fence), 3 launches:
grammars_configured=14 setup=80.7607ms first_event_at=132µs    total=80.9001ms
grammars_configured=14 setup=78.6884ms first_event_at=132.1µs  total=78.8275ms
grammars_configured=14 setup=80.2655ms first_event_at=136.6µs  total=80.409ms
```

Both arms measure `HighlightConfiguration::new(language, name,
highlights_query, injections_query, "")` + `.configure(&RECOGNIZED_CAPTURES)`
— parsing the grammar's `.scm` query text into a `tree_sitter::Query`,
resolving capture indices, and compiling any `#match?` predicate regexes —
which is the real, unavoidable per-language setup cost `tree-sitter-highlight`
imposes and which has no equivalent in syntect's single whole-bundle load
(every grammar here is a separately linked static library with its own query,
by construction — C10's static-linkage finding again, in setup time rather
than binary bytes). **One grammar (Rust) costs 15.1–15.6 ms.** Fourteen
grammars cost 78.7–80.8 ms — since 14× the single-grammar cost would be
~213 ms, the marginal cost per *additional* grammar after the first is lower,
≈(80 − 15) / 13 ≈ 5 ms, consistent with a one-time fixed cost (first
tree-sitter call in the process — a `LazyLock` static, thread-local buffer
setup, or similar) amortizing across the eager arm's 14 calls. **Either
reading is expensive against a 250 ms cold-start budget**: lazy-single is
**≈6%** of budget for one language; eager-all-corpus-languages is **≈32%** —
for highlighter setup alone, before layout, render, or window creation. The
event stream itself, once a config exists, is fast (`first_event_at` 122–213 µs,
`fence_total` well under 250 µs in every run) — **the cost is entirely in
`HighlightConfiguration::new`, not in highlighting**.

### Steady-state throughput

```
[tree-sitter throughput] fences=2000 total_lines=120000 total_bytes=6089569
  lang_misses=250 reps=5 best=2.9225462s worst=3.0391786s
  lines_per_sec=41060 mb_per_sec=2.084
  rep0: 2.9844491s events=2522427
  rep1: 2.9225462s events=2522427
  rep2: 2.9277283s events=2522427
  rep3: 3.0391786s events=2522427
  rep4: 3.0133773s events=2522427
```

**41,060–41,394 lines/s, 2.084–2.101 MB/s — ~1.65× syntect's throughput on
the same file.** `lang_misses=250` is expected and correct, not a bug: E3's
`gen-code-dense` generator produces 8 languages at 250 fences each (`c, go,
js, python, rust, sh, sql, yaml` — confirmed by grepping the file's own fence
markers), and this crate's 14 linked grammars do not include `sql`
(deliberately — no `tree-sitter-sql` was part of E2's proven-buildable set;
see the coverage discussion below). The 1,750 covered fences highlight at the
rate above; the 250 `sql` fences fall through to a miss, counted, not silently
dropped.

### Coverage — what this spike links, not what the ecosystem has

```
[tree-sitter coverage] linked_grammars=14
  rust js ts python c cpp java go sh yaml json toml html css
```

This is **not** a count of tree-sitter's ecosystem — it is the 14 grammars
this spike proved buildable (a subset of E2's 20, trimmed to what the corpus
actually names, `toml-ng` added back for the corpus's TOML fences). Whether
substantially more of Prism's 297 have a maintained tree-sitter crate on
crates.io **could not be verified from this machine** — no network access to
crates.io. See "Language coverage" below for what is and is not backed by a
number.

---

## Language coverage — as an intersection, not two counts

### First, a correction to the baseline number itself

**MarkText's baseline is 297 languages, not 298.**
`node_modules/prismjs/components.json`'s `languages` object has 298 keys, but
one of them — `meta` — is the file-path template for the whole block
(`{"path": "components/prism-{id}", "noCSS": true, ...}`), not a language.
Excluding it: **297**. Cross-checked against
`node_modules/prismjs/components/`: 298 non-minified `prism-*.js` files, of
which one is `prism-core.js` (the engine, not a language) — 297 again, two
independent counts agreeing. There are also **104 alias strings** (`js` for
`javascript`, `html`/`xml`/`svg`/`mathml`/`ssml`/`atom`/`rss` all for
`markup`, etc.), so the full space of names a user could type and have Prism
recognize is **401**, not 297 and not 298. `docs/M3.md` cites "298" in three
places (§4 C11, §5 D3, §6 S3's gate); the number this experiment measures
against below is corrected to 297, with the arithmetic shown so a re-reader
can check it rather than take it on faith.

### The corpus itself, measured directly — the cleanest illustration available

`50-code-fences.md` names 16 distinct languages, 3 fences each (48 fences) +
2 empty-info-string fences + 1 indented block with no language (51 blocks
total, matching both engines' `fences=51` output):

| Language | syntect (75-syntax default) | tree-sitter (14 linked grammars) |
|---|:---:|:---:|
| rust, js, python, go, c, cpp, java, sh, yaml, json, html, css (12 languages) | matched | matched |
| **ts** (TypeScript) | **miss** — absent from default bundle | matched |
| **toml** | **miss** — absent from default bundle | matched |
| **sql** | matched | **miss** — no grammar linked |
| **diff** | matched | **miss** — no grammar linked |
| (empty info string / indented, 3 blocks) | miss (correctly — no language to guess) | miss (correctly) |

**Both engines cover exactly 42/51 blocks (82.4%) on this corpus, and they
disagree on which 6.** `lang_misses=9` for both, confirmed in the raw
throughput output above, is not a coincidence of this run — it falls
directly out of this table. Twelve of sixteen languages (75%) are covered by
*both* engines; two are syntect-only, two are tree-sitter-only. **This is the
task brief's point made concrete on this project's own fixture**: "44/297"
and "an unverified 35–50% estimate" are not comparable numbers, and even
where both engines' raw counts were similar, the *specific* misses are what a
user hits.

### The wider 297, engine by engine

**syntect: measured directly, by running the real crate.**
`SyntaxSet::load_defaults_newlines()` bundles **75 syntaxes**
(`spikes/.gen/syntect-coverage.txt`, full list). Matching syntax names/
extensions against Prism's 297 IDs by hand: **52 of the 75 map to some Prism
ID, covering 44 of the 297 (≈15%)**. Confirmed misses beyond TypeScript/TOML:
Kotlin, Swift, Dart, Dockerfile, INI, GraphQL, JSX/Vue/Svelte, CoffeeScript,
SCSS/LESS, PowerShell, Elixir, Julia, Zig, Nim, HCL/Terraform, AsciiDoc,
Protocol Buffers, CMake, and essentially every templating-engine grammar
(Handlebars, Jinja2, Twig, Pug, EJS). **75 is a zero-effort default, not a
ceiling** — syntect can load arbitrary additional `.sublime-syntax`/
`.tmLanguage` files, but sourcing, vetting and maintaining a few hundred
third-party syntax definitions is real, open-ended work structurally similar
to what a Prism port would owe (see below), not a solved problem the crate
hands you.

**tree-sitter: partially verified, partially estimated — stated as such,
not blended together.** Verified from files on disk: E2's
`spikes/e2-treesitter-20/Cargo.toml` lists 20 grammar crates that build
cleanly on this machine (`spikes/Cargo.lock` resolves 23 `tree-sitter-*`
packages total — the 20 plus `tree-sitter`/`tree-sitter-highlight`/
`tree-sitter-language`); this experiment's own 14-grammar subset is a further
proof point in the same direction. **Not verified**: the true size of
tree-sitter's published-crate ecosystem, because this machine has no network
access to crates.io. Best-effort estimate carried over from research into
this report, clearly flagged as *estimate*: on the order of 100–180
actively-maintained grammar crates exist for mainstream languages, but
tree-sitter structurally under-covers Prism's long tail — display/DSL
pseudo-languages Prism ships that a context-free-grammar tool has little
reason to parse (`regex`, `diff`, `ignore`, `properties`, `http`, `uri`,
`json5`, `csv` all exist as real Prism grammar files) and most templating
engines. **Estimated 35–50% of 297 — a range, not a number, and the range
itself is not sourced from a file, unlike every other figure in this
section.**

**Prism port: 297/297 by construction**, with the caveat that "covers" here
means "the grammar's rules are re-derived," not "ships for free" — the cost
of getting there is §5's subject, below.

---

## The Prism-port feasibility question

Grounded in a 25-file representative read plus a population-wide grep across
all 298 non-minified `prism-*.js` files in
`C:\Dev\marktext\node_modules\prismjs\components\` (prismjs `1.30.0`).

### Grammar structure

Every Prism grammar is `Prism.languages.<id> = { <token-name>: { pattern:
/regex/, lookbehind: bool, greedy: bool, inside: {...}, alias: '...' }, ... }`
— a plain object tree, in every file checked. **`lookbehind: true` is Prism's
own convention, not a request for regex-engine lookbehind support**: the
pattern matches a *wider* span including one extra leading group, and Prism's
engine trims that group off the matched token after the fact
(`utils/prism/index.ts:77`'s comment gives LaTeX's comment-matcher,
`/(^|[^\\])%.*/` with `lookbehind: true`, as the canonical example — the
regex itself is plain, no `(?<=`). This distinction matters directly for the
port's regex-engine choice, below.

### Regex features, counted across all 298 files (not sampled)

| Construct | Files with ≥1 hit, raw grep | Files with ≥1 *functional* hit (outside comments) |
|---|---:|---:|
| Native lookbehind `(?<=`/`(?<!` | 3 | **0** |
| Native lookahead `(?=`/`(?!` | 220 / 207 | 218 / 206 |
| Backreference `\1`–`\9` | 102 | 101 |
| `lookbehind: true` flag (Prism's own convention, above) | 225 | 224 |
| `greedy: true` flag | 219 | 218 |

**The three "functional native lookbehind" hits are all inside `//` comments**
(`prism-regex.js` explaining regex syntax in prose; `prism-php.js` commenting
on "the complex lookbehind" of a pattern that doesn't itself use one) — real
functional native-lookbehind usage across all 298 files is **zero**. This
directly narrows C11's claim: *"lookbehind and backreferences need
fancy-regex"* names the wrong construct for the dominant case. **Native
lookahead — used in 218/298 files (≈73%) — is the one that actually forces
the issue**, because Rust's plain `regex` crate supports **none** of
lookahead, lookbehind, or backreferences (only `fancy-regex` does, of the two
crates this project already touches). Backreferences (101/298, ≈34%) add a
smaller number of files that need `fancy-regex` for a second, independent
reason. Taking the union (a file needing `fancy-regex` for *either* reason),
the true count is close to **218–250 of 298 (≈75–85%)** — nearly all of them,
not an exceptional minority. **Where this contradicts the plan** (below)
restates this as a correction, not just a refinement: a Prism port would
depend on `fancy-regex` for the large majority of its grammars, not for a
lookbehind-and-backreference edge case, and the plan's own framing undersells
how central that dependency would be. The saving grace is that
`fancy-regex 0.19` is a **sunk cost already**: `crates/mt-inline/Cargo.toml`
depends on it today, so this is not a new dependency D3 would introduce —
only a much larger fraction of the port's own regex traffic running through
it than "lookbehind and backreferences" implies.

### Pure-data vs. JavaScript-logic grammars

Across all 298 files: **156/298 (52%) carry JavaScript logic** beyond a plain
object literal — calling `Prism.languages.insertBefore(...)` to splice one
language's rules into another (`markup` embedding `css`/`javascript`; `php`
embedding `markup`), registering `Prism.hooks.add(...)` post-processing
hooks, or generating patterns via a function rather than a literal. The
remaining **142/298 (48%) are pure declarative data**. `insertBefore` calls
alone appear in 76/298 files (26%) — this is the mechanism most directly
analogous to what a Rust port would need to reproduce structurally (grammar
composition/embedding, e.g. Rust code fences inside a Markdown fence, or CSS
inside an HTML `<style>` block), not just per-file translation.

### The binary-cost estimate C11's own analysis owes

All 298 non-minified `prism-*.js` source files (297 grammars + `prism-core.js`)
total **806,895 bytes (788 KiB)** on disk; minified, **574,075 bytes
(561 KiB)**. This is a size-of-the-JS-source proxy, not a Rust binary
measurement — but it bounds the scale of the problem: even the *least*
compact representation of every single Prism grammar's rules is under 1 MB.
A Rust port's compiled pattern data (`Regex`/`fancy_regex::Regex` objects
built from translated pattern strings, plus small per-language tables for
`inside`/`alias`/token-name structure) is a different representation, but has
no structural reason to be larger than its JS source, and every syntect/
tree-sitter number in this report shows compiled artifacts landing at the
same order of magnitude as — or smaller than — their textual source. The
regex-engine machinery itself is **not** a marginal cost of this decision:
E2's `regex-fancy` build already measured `fancy_regex` + `regex_automata` +
`aho_corasick` + `regex_syntax` at **~520 KB of `.text`** combined, and that
dependency is sunk regardless of D3 (`mt-inline` already carries it). **Net
estimate: a full 297-language Prism port is plausibly well under 1.5 MB of
marginal binary cost, essentially flat regardless of whether 44, 75, or all
297 languages ship** — structurally unlike tree-sitter's ~1.3 MB **per
grammar**, because grammar rules are data walked by one shared interpreter,
not 297 separately compiled and statically linked C parse tables. This is an
estimate, explicitly not a measurement — no port exists to build and weigh.

### Honest cost the count does not soften

Owning 297 grammars "as they drift" (§5 D3's phrase) means: every time Prism
ships a new/updated grammar (this project observed real logic complexity —
`insertBefore`, hooks, generated patterns — in the majority of files, not a
tail of exceptions), the port's maintainer re-reads that file and re-derives
its Rust equivalent by hand. Upstream Prism absorbs this cost today across
its own maintainer base; a fork absorbs it alone, indefinitely, with no
automated sync possible once translated (regex syntax differences and the
`lookbehind: true` convention's semantics are not machine-translatable in
general, only pattern-by-pattern). This is the cost §5 D3 names and does not
price, and this experiment cannot price it either — it can only say, now with
counts instead of a vibe, that **more than half the corpus (156/298 files)
is more than "translate a regex," and the fraction needing `fancy-regex`
(≈75–85%) is much larger than "lookbehind and backreferences" suggests.**

---

## Differential-testability — the axis §7 already calls the strongest tool, tested against what actually exists

`xtask/src/diff.rs` and `xtask/src/tokens.rs` are the two existing
differentials, and both share one shape: a Node-side dumper
(`tools/diff/dump-ts-state.mjs`, `tools/diff/dump-ts-tokens.mjs`) that loads
the *actual* muya/Prism source from a sibling `marktext` checkout and prints a
canonical JSON wire form, compared by `first_difference`
(`xtask/src/diff.rs:88`) — a recursive walk that reports the exact JSON path
of the first disagreement, not just "differs." **Neither existing dumper
touches Prism or code-fence tokenization** — `dump-ts-state.mjs` is block
state, `dump-ts-tokens.mjs` is `mt-inline`'s inline-token stream. **A
`--require-ts` highlight comparator (`cargo xtask highlight`, S3's gate) does
not exist yet** (`grep`-confirmed: no `highlight` subcommand anywhere in
`xtask/src/main.rs`, expected — S3 hasn't started) and would need a new
`tools/diff/dump-ts-highlights.mjs` built to the same shape.

**That script is not hard to write, and MarkText's own source hands it the
one non-obvious piece.** Prism's `tokenize()` returns a token *tree* (strings
interleaved with `Token` objects, arbitrarily nested via `inside`), not
byte offsets — recovering byte ranges means walking that tree and summing
matched-text lengths as you go. MarkText already solves exactly this problem:
`packages/muya/src/utils/prism/walkToken.ts` (27 lines, confirmed present)
recursively flattens Prism's token tree back into offset-bearing spans for
rendering. A comparator script would mirror that logic — the same relationship
`dump-ts-tokens.mjs`'s `offsetTable`/`byteOffset` helpers already have to
muya's UTF-16 ranges (`tools/diff/dump-ts-tokens.mjs:284`) — so the
*mechanics* of building this harness are proven out twice already in this
codebase, once for state and once for tokens.

**What it would actually find is a different question per engine, and this
is the part §5 D3's text does not weigh:**

- **Prism port**: span boundaries agree with Prism *by construction*, modulo
  actual translation bugs — the same grammar rules, restated. A disagreement
  is close to unambiguously a port defect, exactly the "did I port the
  898-line lexer correctly?" → boolean transformation §7 credits `diff`/
  `tokens` with for block state and inline tokens. `first_difference` could
  be reused verbatim on a `Vec<{start, end, scope}>` array, no new comparison
  logic needed.
- **syntect**: Sublime `.sublime-syntax` grammars are authored independently
  of Prism's regex/state-machine grammars, by a different community, for a
  different engine's scope model. Even where both correctly recognize "this
  is a string literal," the exact byte at which the token boundary falls
  (does a string's closing quote belong to the string token or the next
  token; is a decorator/attribute one token or three) is a **grammar-authoring
  choice**, and the two grammar families were never designed to agree on it.
  An exact-boundary comparator would fail on a large fraction of real fences
  even when both engines are highlighting "correctly" by their own grammar's
  rules — noise, not signal.
- **tree-sitter**: same shape of problem, different cause — a context-free
  grammar's node boundaries (where does a `field_expression` node start) do
  not correspond one-to-one with a regex scanner's token boundaries, even for
  semantically-equivalent output.

**What this does to the S3 gate, per engine:** for a Prism port, `--require-ts`
is close to free and stays exact-equality, the same posture `diff`/`tokens`
already have. For syntect or tree-sitter, the gate as M3.md currently states
it ("compares span boundaries against Prism's own tokenization... exact
equality") **would fail constantly and tell M3 nothing** — it would have to
be redefined before S3 could use it at all: fuzzy/coarse comparison (token
*category* — string vs. keyword vs. comment — rather than exact byte
boundary), a threshold on boundary agreement, or dropped as a gate and kept
only as an eyeballed spot-check. **This is the axis M3.md's own §7 calls "the
one place M3 keeps M1/M2's strongest tool," and picking syntect or
tree-sitter is the one choice under D3 that gives that tool up entirely, not
just weakens it.** Nothing else in this experiment's numbers has that
property — a size or speed disadvantage is a number you can be worse at and
still ship; losing the one real differential is a category change to M3's
whole test posture, the same kind of finding §7 already flagged for the
milestone as a whole ("for the first time in this project, 'does it match
MarkText?' cannot be answered by running MarkText" — true for everything
*except* highlighting, if and only if highlighting is a Prism port).

---

## What this does not decide

This experiment measured five axes with numbers and estimated a sixth
(Prism-port cost) from counts. It does **not** pick D3 for the milestone
document, and should not be read as though the numbers forced a winner:

- **No axis here weighs the others.** Whether a 6 ms binary-size difference
  matters more than a 78 ms cold-start difference, or whether keeping S3's
  exact-equality gate is worth more than tree-sitter's 1.65× throughput
  advantage, is a judgement about M3's priorities that this experiment was
  not asked to make and does not have the standing to make.
- **The Prism-port cost estimate is a size/count argument, not a schedule
  one.** This experiment did not attempt to estimate engineering hours for
  translating 297 grammars (142 pure data, 156 with logic, ≈75–85% needing
  `fancy-regex`), only what the grammars themselves contain. "How long" and
  "who maintains it as Prism drifts" are organizational questions, not
  measurement ones.
- **tree-sitter's ecosystem coverage (35–50% of 297) is an unverified
  estimate**, explicitly flagged as such above — a follow-up with network
  access to crates.io could turn this into a real number in under an hour,
  and this experiment recommends that as the one clearly missing data point
  rather than guessing further.
- **syntect's 75-syntax default is a floor, not syntect's ceiling** — loading
  additional `.sublime-syntax` files is possible and unmeasured here, both
  for its coverage upside and for the maintenance cost it would add (plausibly
  comparable to the Prism-port cost this experiment did estimate, since both
  are "source and vet a large number of third-party grammar files").
- **Nothing here touches incremental re-highlighting.** C11's argument that
  tree-sitter's edit-locality benefit is M4's, not M3's, was the premise
  this experiment was commissioned to test the *cost* side of — it did not
  re-test the *benefit* side (M4's editing scenario), because M3 is
  read-only and that benefit genuinely does not apply yet. If D3 is decided
  for M3 alone and revisited at M4, that revisit should re-run this
  question with edits in scope, not assume M3's answer still holds.
- **The corpus's own code content is trivial** (`let value_N = NNNN;` in
  every fence, real Rust text only in the code-dense file). Steady-state
  throughput on genuinely idiomatic code in all 16 corpus languages — not
  just Rust — is unmeasured and could shift the throughput numbers in either
  direction depending on how span-dense each language's real syntax is.

---

## Where this contradicts the plan

1. **C11's "needs an adapter" claim is only true for one of syntect's two
   public APIs, and it is easy to avoid.** `easy::HighlightLines` returns
   `Vec<(Style, &str)>`, as C11 says. But `syntect::highlighting::
   RangedHighlightIterator` — public, one layer below, no additional
   dependency — returns `(Style, &str, Range<usize>)` directly. `mt-highlight`
   would enter through the second, and the "needs an adapter" line in
   §5 D3 would not apply. Priced anyway, for the case where a future
   implementer reaches for the convenience wrapper instead: 7 lines,
   10.8 ns/token, self-checked at runtime for the contiguity invariant it
   depends on.

2. **The upstream "~23 ms" `SyntaxSet` load figure C11 cites is real but
   conservative by roughly 30× on this hardware.** Measured: 0.76–0.78 ms.
   This is the rare direction — every other upstream number this experiment
   checked (throughput) landed inside or below the claimed range, not above
   it, but this one landed *faster* than claimed. Worth recording precisely
   because C11 uses "~23 ms" as part of an argument for syntect's cold-start
   advantage; the real advantage is larger than C11 states, not smaller.

3. **C11's "lookbehind and backreferences need `fancy-regex`" names the
   wrong dominant construct.** Population-wide grep across all 298 Prism
   grammar files: functional native lookbehind is used in **zero** files
   (Prism's `lookbehind: true` flag is a different, non-regex-engine
   mechanism — a wider match trimmed after the fact). Native lookahead,
   which C11 does not mention, is what actually forces the issue, in
   **218/298 files (≈73%)**. Backreferences add **101/298 (≈34%)**, mostly
   overlapping. The union — files needing `fancy-regex` for either real
   reason — is **≈218–250/298 (≈75–85%)**, not an edge case.

4. **"298 languages" is off by one.** `components.json`'s `languages` object
   has 298 keys because it includes `meta` (a path template, not a
   language). The real count is **297**, confirmed two independent ways
   (298 keys minus `meta`; 298 non-minified `prism-*.js` files minus
   `prism-core.js`). Every "coverage of 298" framing in `docs/M3.md` (§4 C11,
   §5 D3, §6 S3) should read 297. Immaterial to which engine wins, material
   to whether a later reader can reproduce the count.

5. **"Prism load/wiring, highlight-range HTML, line numbers | 397" is off
   by 8 lines and describes four files, not one.** The current TypeScript
   sum across `packages/muya/src/utils/prism/index.ts` (86),
   `loadLanguage.ts` (119), `utils/marked/getHighlightHtml.ts` (88), and
   `utils/codeBlockLineNumbers.ts` (112) is **405**, not 397 — and it is
   four files covering the row's three named concerns (wiring,
   highlight-to-HTML, line numbers), not one file, which the table's single
   row could be misread to imply. A ~2% line-count discrepancy decides
   nothing; recorded per this project's own established practice of
   correcting a number even when the correction doesn't move a decision
   (`spikes/results/e2-binary-size.md`'s "Rust std + runtime measures lower
   than claimed" entry is the precedent for recording a small honest miss).

6. **§12.2/E2's scenario table can be misread as pricing syntect at
   "+1.178 MB," and that figure is actually the fonts row, not syntect's
   own cost.** `spikes/e2-syntect-onig` has no dependency on `e2-common`
   (unlike the tree-sitter builds, which do), so E2 never built
   "baseline + syntect" as one linked binary — its scenario totals
   (`8.98 MB` GPU+syntect, `4.66 MB` CPU-only+syntect) are **sums of
   independently measured deltas**, and the constant `+1.178 MB` addend that
   appears on every scenario row, including both syntect rows, is the
   **bundled DejaVu Sans Mono fonts**, unrelated to the highlighter choice.
   Reconstructing syntect's own marginal linkage cost from E2's raw numbers
   — standalone `syntect`-onig binary (1,098,240 B) minus the floor build
   (98,304 B) — gives **999,936 B ≈ 1.000 MB**, the figure this report's
   comparison table cites. Confirmed independently: the CPU-only+syntect
   scenario total (4.66 MB) minus build 1 (2.481664 MB) minus the fonts
   addend (1.178 MB) is 1.000336 MB, matching to four significant figures.

7. **Contradicts nothing, but should be said plainly: no single engine wins
   on every axis, and the plan's decision text (§5 D3) frames the choice as
   resolvable by "measure all three" without naming that the axes could
   point different directions.** They do. See "What this does not decide."

---

## Reproducing

From `spikes/`, with `%USERPROFILE%\.cargo\bin` on `PATH`:

```sh
cargo build --release -p e4-syntect -p e4-treesitter

E_SYN=./target/release/e4-syntect.exe
E_TS=./target/release/e4-treesitter.exe

# cold start (run several times — separate process launches, not reps)
$E_SYN cold ../bench/corpus/50-code-fences.md
$E_TS  cold ../bench/corpus/50-code-fences.md            # lazy, 1 grammar
$E_TS  cold ../bench/corpus/50-code-fences.md --eager    # eager, all 14

# steady-state throughput (the code-dense file is E3's, reused as-is;
# regenerate with `e3-layout-main.exe gen-code-dense .gen/code-dense.md 2000 60`
# from spikes/ if .gen/ was ever cleaned)
$E_SYN throughput .gen/code-dense.md 5
$E_TS  throughput .gen/code-dense.md 5

# the adapter and coverage questions
$E_SYN adapter
$E_SYN coverage > .gen/syntect-coverage.txt
$E_TS  coverage
```

`spikes/e4-common` is the shared corpus-extraction crate every other E4
member path-depends on; `spikes/Cargo.toml` carries the updated member list.

## Verification

```
$ cargo xtask deps
```
reports **14** crates in `crates/` — unaffected, since nothing in this
experiment touched `crates/`, `xtask/`, `docs/`, `bench/`, or the root
`Cargo.toml`/`Cargo.lock`. `git status` at the end of this experiment shows
changes only under `spikes/` (three new crates — `e4-common`, `e4-syntect`,
`e4-treesitter` — the updated `spikes/Cargo.toml` member list, this file, and
`spikes/.gen/syntect-coverage.txt`).

---
