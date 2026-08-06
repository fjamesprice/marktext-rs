# Upstream reporting — decided against, M2 S1

**Status: not filing. Register rule 4 is closed.**

M1's §5 D3 wrote rule 4 as *"`upstream` holds the marktext issue URL once
filed. **File them.**"*, and M2.md §5 D11 read that as *"fork and rename, and
file cases 1, 2 and 4 upstream regardless"*. S0 drafted three issue bodies for
review rather than posting them, because filing means opening issues on someone
else's public repository.

**The review answered no.** All three `upstream` fields stay `null`,
`cargo xtask divergences` no longer asks, and this file is the record rather
than a queue.

## Why not

`marktext/marktext` is the **reference**, not an upstream this project merges
from or ships to. D10 froze parity at `e52106fd` and D11 decided fork-and-
rename; the port has already replaced the regex engine, sixteen character
classes, `DOMParser` and — at M2 S1 — the entire block parser. There is no
patch flowing in either direction and no release waiting on an answer.

So filing would have been a courtesy: *we found these while porting your code*.
A good courtesy, and one this project can pay later at any time — the evidence
does not expire. What it is not is a dependency of anything here.

The one case that *was* a real question rather than a report — case 4 below,
whether the image/link asymmetry is deliberate — is the only one whose answer
would have changed the port's behaviour, and only by deciding whether to keep
reproducing muya or to register a divergence. Unanswered, the port keeps
reproducing it, which is the status quo and is what §10's lossless-round-trip
constraint wants anyway.

## What the three cases were, and where their evidence lives now

Nothing is lost by not filing: every claim below is already a register entry
with named inputs, and `cargo xtask divergences` re-proves all of them against
the running engine on every commit — 27 of 27 registered inputs disagreeing,
4,237 swept inputs agreeing.

| # | Case | Register entry | Verified |
|---:|---|---|---|
| 1 | `tryChunks` reads the emoji word-boundary character at an absolute offset (`lexer.ts:206`) | `emoji-nested-boundary`, 16 inputs | yes |
| 2 | `disallowedHtmlTag` matches unanchored, rejecting `<noscript>` (`lexer.ts:21`) | `disallowed-html-tag-substring-match`, 7 inputs | yes |
| 3 | `getAttributes` reads a foster-parented element's attributes (`utils.ts:172`) | `html-tag-attrs-from-a-foster-parented-element`, 4 inputs | n/a — never drafted |
| 4 | `tryImage`/`tryReferenceImage` never call `lowerPriority` | **none** — the port reproduces muya | yes |

Case 3 was never drafted even when filing was still the plan, and the reason
stands on its own: it is a divergence from `happy-dom`, a **test** dependency,
at a site where the shipped Electron/Chromium build behaves differently again.
There is no upstream change that makes it go away.

## The drafts were wrong, and finding that out was the useful part

S0's three bodies each carried a reproducer of the shape

```js
import { tokenizer } from '@muyajs/core/dist/inlineRenderer/lexer';
tokenizer('**a :smile:**', { hasBeginRules: false });
```

**None of them would have run.** The tokenizer lives at
`packages/muya/src/inlineRenderer/lexer.ts`, not under a `dist/` build; its
options object is `{ highlights, hasBeginRules, labels, options }` and three of
those four were missing; and `tryHtmlTag` calls `getAttributes`, which calls
`DOMParser`, so case 2's inputs throw outright without a DOM global — which is
why every one of muya's inline spec files carries
`// @vitest-environment happy-dom`.

That is the same failure this milestone has now catalogued six times: prose
that reads plausibly and was never executed. It was caught by running the
snippets before posting them rather than after, and it is recorded here because
**the drafts were reviewed and found broken, which is a better reason to have
drafted them than the filing that did not happen.**

The corrected harness is below. It is not needed for anything the project runs
— `tools/diff/dump-ts-tokens.mjs` is the maintained one and drives 4,264 inputs
per commit — but it is the smallest thing that reproduces a register entry by
hand, which is what a reader chasing one of these will want.

```js
// node --import tsx repro.mjs '<input>' …   — from the marktext checkout root
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import path from 'node:path';

const require = createRequire(path.join(process.cwd(), 'package.json'));
const { Window } = await import(pathToFileURL(require.resolve('happy-dom')).href);
const window = new Window();
globalThis.DOMParser = window.DOMParser;
globalThis.document = window.document;

const { tokenizer } = await import(
  pathToFileURL(path.join(process.cwd(), 'packages/muya/src/inlineRenderer/lexer.ts')).href
);

const types = (toks, acc = []) => {
  for (const t of toks ?? []) { acc.push(t.type); types(t.children, acc); }
  return acc;
};
const run = src => types(tokenizer(src, {
  highlights: [], hasBeginRules: true, labels: new Map(),
  options: { superSubScript: true, footnote: false },
}));

for (const src of process.argv.slice(2)) {
  console.log(JSON.stringify(src).padEnd(28), '->', run(src).join(', ') || '(none)');
}
```

Run against `e52106fd`, which is what the three cases assert:

```
"**a :smile:**"              -> strong, text          case 1: an emoji it should keep, lost
"*x :smile:*"                -> em, text, emoji       case 1: correct, by luck rather than by the guard
"[a:smile:](u)"              -> link, text, emoji     case 1: one it should suppress, kept
"[12:00-14:00](u)"           -> link, text, emoji, text   case 1: a clock emoji inside a time range
"*a:smile:*"                 -> em, text, emoji       case 1: same, at `em`
"<noscript>"                 -> text                  case 2: rejected for containing "script"
"<subscript>x</subscript>"   -> text                  case 2: rejected, and used in the wild
"<div>x</div>"               -> html_tag, text        case 2: the control — ordinary tags are fine
"![foo`](/uri)`"             -> image, text           case 4: the image is accepted…
" [foo`](/uri)`"             -> text, inline_code     case 4: …where the link form is refused
```

## What would reopen this

Someone deciding the courtesy is worth paying. The bodies are reconstructible
from the register entries plus the table above in about an hour, and the
evidence will still be true — `MARKTEXT_REF` is pinned at `e52106fd` (D10) and
bumping it is a deliberate commit that re-runs the whole sweep.

---

# A second upstream, and this one is a dependency — `pulldown-cmark`, M2 S6, widened at S7

**Status: not filed, and unlike the three above this one has a cost.**

Everything above is about `marktext/marktext`, which is a *reference* rather
than a dependency. `pulldown-cmark` is a dependency, it is in the shipped
binary, and M2 S6's generated edit sequences found that it **panics**.

## The reproducer

```rust
pulldown_cmark::Parser::new_ext("> - [a]: /x\n\t", options)
    .into_offset_iter()
    .count();
// panicked at pulldown-cmark-0.13.4/src/parse.rs:2199:
// called `Option::unwrap()` on a `None` value
```

`options` is `ENABLE_TABLES | ENABLE_STRIKETHROUGH | ENABLE_TASKLISTS`
(`mt_md::block::cmark_options`). Four ingredients, and all four are needed:

| Input | Result |
|---|---|
| `"> - [a]: /x\n\t"` | **panic** |
| `"> - [a]: /x\n \t"` | **panic** — the whitespace-only line may be indented |
| `"> [a]: /x\n\t"` | ok — no list item |
| `"- [a]: /x\n\t"` | ok — no block quote |
| `"> - x\n\t"` | ok — no reference definition |
| `"> - [a]: /x\n\tb"` | ok — the last line has content |

So: a block quote, containing a list item, containing a link reference
definition, followed by a line that is whitespace-only and ends in a tab.

`crates/mt-md/src/block/tests.rs` carries both tables as tests. The reproducing
one is `#[should_panic]`, which makes it a ratchet in the useful direction: the
day the upstream fix lands, `cargo test` fails and points here.

## The class is wider than that — corrected at M2 S7

S7's generated markdown reached the same `unwrap()` from inputs the table above
says should be fine. **The block quote is not required, and the tab is not
either**: any whitespace character CommonMark does not count as blank will do.

| Input | Result |
|---|---|
| `"- [a]:x\n\u{b}"` | **panic** — no block quote, no tab, twelve bytes |
| `"> - [a]: /x\n\t"` | **panic** — the shape recorded at S6, still true |

Every row of the S6 table stays correct and the `#[should_panic]` test stays
green; this **widens** the class rather than contradicting it. Two ingredients
are load-bearing — a list item containing a link reference definition, and a
following line that is whitespace in `str::trim`'s sense but not a blank line in
CommonMark's — and the block quote that looked like a third is an accident of the
input the generator happened to find first.

**Why the widening matters more than the extra reproducer.** Option 3 below is
*"pre-scan for the shape in `mt_md::block` and route around it"*, and a pre-scan
written against the four-ingredient reading would have let most of the class
through while reporting that it was handled. `is_the_known_upstream_panic` in
`crates/mt-md/tests/round_trip_properties.rs` is the widened predicate — it skips
**89 of 1,600** generated documents, narrow enough that
`the_only_panics_the_generators_reach_are_the_ones_the_guards_name` still fails
on anything else that panics — and
`the_upstream_panic_guard_covers_the_recorded_shape_and_the_wider_class` pins
both readings so neither can be quietly narrowed again.

S7 also found, and **repaired**, a panic on the neighbouring shape
`"- a\n\u{2028}"` that had been read as a second face of this one. It was not:
it was `mt_md::block`'s own `apply_prefix` cutting a line mid-`char`.
`docs/M2.md` §6's "S7's verification" has the measurement that chose the fix —
26,035 of 47,264 enumerated rows, against `marked`'s UTF-16 `slice`. The
distinction is the point: one of the two was upstream's and one was this port's,
and only running them separately said which.

## Why it matters more than the three above

**`mt_md::parse`'s doc comment says it is total** — *"every string is a
document, exactly as `MarkdownToState.generate()` is total"* — and for this one
input class it is not. muya parses the same string without complaint, so this is
not a divergence to register (nothing disagrees; one side aborts).

It is not reachable from anything the harnesses drive: not from
`cargo xtask blocks`' 1344, not from `cargo xtask diff`'s 22, not from the 1,324
spec fixtures. Five stages did not see it. What found it is
`crates/mt-md/tests/reparse_properties.rs` — generated **edit sequences** over
fixed documents, which is a different denominator from any of those, and the
first thing in this milestone able to construct a string nobody wrote.

## What is owed, and to whom

The **decision**, to M4 — the first milestone with a user who can type it.
Three options, none of them S6's to take:

1. **File it upstream.** Unlike the marktext three, there is a live dependency
   relationship and a maintained project on the other end; the reproducer above
   is a complete bug report.
2. **Pin a patched fork**, if the fix is small and upstream is slow.
3. **Pre-scan for the shape** in `mt_md::block` and route around it. The
   cheapest and the ugliest, and the one that has to be measured against the
   1344 before it is taken.

**Catching the panic is not one of the options**: §12's release profile is
`panic = "abort"`, so `catch_unwind` is not available where it would matter.

0.13.4 is the latest published version as of M2 S6, so there is no upgrade to
take today. `docs/M2.md` §10's "Owed by S6" carries the same item.
