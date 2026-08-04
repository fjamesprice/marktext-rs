# Upstream issue drafts — docs/M2.md §5 D11

**Status: drafted, not filed.** Register rule 4 (`spec/README.md`) says
*"`upstream` holds the marktext issue URL once filed. **File them.**"* — and
all three entries in `spec/divergences.json` still read `upstream: null`, so
`cargo xtask divergences` still prints `NOT FILED` beside each of them.

**That rule is therefore still open at the end of M2 S0, deliberately.** D11
decided *fork and rename, and file cases 1, 2 and 4 upstream regardless*; what
S0 was asked to settle first is that filing means opening three public issues
on someone else's repository. The decision taken was to write the bodies here
for review rather than post them. Recorded rather than left implicit, because
an owed action with no trace is the failure mode M1 catalogued four times.

**What closes it:** post these three, then set each register entry's
`upstream` field to its issue URL and delete that entry's section here. The
runner's `NOT FILED` line is what will keep asking.

---

## Scope, and why it is three and not four

D11's table has four cases. Only three are drafted below.

| # | Case | In the register? | Filed? |
|---:|---|---|---|
| 1 | `tryChunks`'s emoji word boundary reads the child string at an absolute offset | yes — `emoji-nested-boundary` | **draft below** |
| 2 | `disallowedHtmlTag` matches unanchored | yes — `disallowed-html-tag-substring-match` | **draft below** |
| 3 | `getAttributes` reads a foster-parented element's attributes | yes — `html-tag-attrs-from-a-foster-parented-element` | **no — see below** |
| 4 | `tryImage` / `tryReferenceImage` never call `lowerPriority` | **no** — the port reproduces it | **draft below**, as a question |

### Case 3 is not filed, and the register entry says why

`html-tag-attrs-from-a-foster-parented-element` is a divergence from
[`happy-dom`](https://github.com/capricorn86/happy-dom), which is a **test**
dependency, at a site where the shipped Electron/Chromium build behaves
differently again. `getAttributes` reads `body.firstElementChild` of the parsed
match; foster parenting moves an element that is not a permitted descendant of
`<table>` to *before* it, so
`<table class="t"><img src="s"></table>` reports the image's attributes for a
token whose `tag` is `table`.

There is no upstream change that makes that go away: the behaviour is the DOM
implementation's, it differs between the two DOMs muya runs against, and muya
does not choose either of them at the site in question. A bug report would be
asking marktext to fix a third party's non-conformant table handling. The port
declines to model it, the register records that it declines, and there is
nothing to file.

---

## Case 1 — `tryChunks` reads the emoji word-boundary character at an absolute offset

**Repository:** `marktext/marktext`
**Labels to suggest:** bug, muya
**Title:** `muya: emoji word-boundary check indexes the child string with an absolute offset (lexer.ts:206)`

### Summary

`tryChunks`'s emoji guard reads `state.originSrc[state.pos - 1]`. Inside a
nested `tokenizerFac` call `originSrc` is the **child** substring while `pos`
is an **absolute** offset into the top-level text, so the check tests the wrong
character — or reads off the end.

The error runs in both directions, which is why it is easy to miss:

- Where the child level's base offset is larger than the text before the
  shortcode, the read lands past the end and #1677's guard treats `undefined`
  as a boundary, so an emoji that should be suppressed is kept.
- Where the base is smaller, it lands on a real character that is not the one
  before the shortcode, so an emoji that should be kept is lost.
- At any rule whose base is 1 — `link`, `reference_link`, `em` — the character
  read is the `:` itself, which is never a word character, so #1677's guard
  **never fires inside those rules at all**.

### Reproducer

```js
import { tokenizer } from '@muyajs/core/dist/inlineRenderer/lexer';

// Loses an emoji it should keep:
tokenizer('**a :smile:**', { hasBeginRules: false });   // no `emoji` token
tokenizer('**:smile:**',   { hasBeginRules: false });   // no `emoji` token
tokenizer('<div>:smile:</div>', { hasBeginRules: false });

// Keeps one it should suppress — #1677's guard never fires here:
tokenizer('[a:smile:](u)',      { hasBeginRules: false });  // emits `emoji`
tokenizer('[12:00-14:00](u)',   { hasBeginRules: false });  // emits `emoji`
tokenizer('*a:smile:*',         { hasBeginRules: false });  // emits `emoji`
```

`*x :smile:*` happens to behave correctly, by luck rather than by the guard.

### Expected

The preceding character should be read **relative to the current tokenizer
level**: `origin[pos - base - 1]`, where `base` is the level's absolute base
offset. When `pos == base` there is no preceding character at this level, which
counts as a boundary and allows the emoji — correct at every call site, because
a child level is only ever entered after `*`, `_`, `~`, `[` or a `>`-terminated
open tag, none of which are word characters. It is also what the top level
already does at `pos == 0`.

### Scale

Sweeping open-tag length against shortcode position over 228 generated inputs,
**96 disagree with the corrected reading, and both directions occur.**

### Why we are reporting rather than sending a patch

We are porting muya to Rust
([`fjamesprice/marktext-rs`](https://github.com/fjamesprice/marktext-rs)) and
have fixed this in the port; the fix falls out of the data layout there rather
than being a special case, so the diff would not transfer. Happy to write the
JavaScript patch if that is useful — say the word.

---

## Case 2 — `disallowedHtmlTag` matches unanchored, so `<noscript>` is rejected

**Repository:** `marktext/marktext`
**Labels to suggest:** bug, muya
**Title:** `muya: disallowedHtmlTag matches unanchored, rejecting <noscript>, <subscript> and <iframe-x> (lexer.ts:21)`

### Summary

`lexer.ts:21` tests

```js
/title|textarea|style|xmp|iframe|noembed|noframes|script|plaintext/i
```

against the captured tag name **without anchors**, so any tag name *containing*
one of the nine is rejected. GFM §6.11 disallows exactly those nine names, not
names that contain them.

### Reproducer

```js
import { tokenizer } from '@muyajs/core/dist/inlineRenderer/lexer';

tokenizer('<noscript>',              { hasBeginRules: false }); // rejected — contains "script"
tokenizer('<subscript>x</subscript>',{ hasBeginRules: false }); // rejected — contains "script"
tokenizer('<subtitle>',              { hasBeginRules: false }); // rejected — contains "title"
tokenizer('<SubTitle>x</SubTitle>',  { hasBeginRules: false }); // rejected — the containing name is matched case-insensitively too
tokenizer('<iframe-x>x</iframe-x>',  { hasBeginRules: false }); // rejected — a hyphenated custom element GFM's tag-name production allows
tokenizer('<xstyle>x</xstyle>',      { hasBeginRules: false }); // rejected — contains "style"
```

`<noscript>` is the one that shows this is not a contrived class: it is a
standard HTML element, it is not on GFM's list, and it is rejected for
containing `script`. `<subscript>` and `<superscript>` are used in the wild.

### Scale

Of 4,172 differential inputs, **58 are this class** — every one of the nine
names with a prefix, a suffix, a hyphenated suffix and an inserted prefix.

### Expected

An exact, case-insensitive match against the nine names GFM §6.11 lists:
`title`, `textarea`, `style`, `xmp`, `iframe`, `noembed`, `noframes`, `script`,
`plaintext`. Anchoring the existing regex — `/^(?:title|textarea|…)$/i` —
would do it.

---

## Case 4 — `tryImage` and `tryReferenceImage` never call `lowerPriority`

**Repository:** `marktext/marktext`
**Labels to suggest:** question, muya
**Title:** `muya: is the image/link asymmetry around lowerPriority deliberate? (lexer.ts, tryImage / tryReferenceImage)`

### This is a question, not a bug report

We are not asking for a behaviour change. muya's current behaviour is what our
Rust port **reproduces**, deliberately, and changing it would break documents
that people have already written. We would like to know whether the asymmetry
is a decision or an oversight, because that determines whether the port should
keep tracking it.

### The asymmetry

`tryLink` and `tryReferenceLink` call `lowerPriority` before committing, so a
candidate whose bracket span is straddled by a higher-priority construct is
refused. `tryImage` and `tryReferenceImage` do not.

The consequence is that the same source shape is an image where the link form
is refused:

```md
![foo`](/uri)`     →  emits an `image` token
 [foo`](/uri)`     →  refused; the inline code wins
```

CommonMark §6.6 says an image's link label is parsed by the same rules as a
link's, which reads as though the two should agree.

### The two readings

1. **Deliberate.** Images are visually atomic in a WYSIWYG editor, so refusing
   one because a backtick straddles its label would leave a half-revealed
   marker on screen with nothing to show for it.
2. **Oversight.** `lowerPriority` was added to the link handlers and the image
   handlers were not updated alongside them.

If it is (1) we will note it in our divergence register as intended behaviour
and stop asking. If it is (2) we are happy to file a proper bug and, if
wanted, write the patch.

### Context

`fjamesprice/marktext-rs` is a Rust rewrite of muya's editing core. We keep a
machine-checked register of every place we deliberately differ from muya, and
this is one we deliberately *do not* differ on — which is why the question is
worth asking before it silently becomes a divergence in one direction or the
other.
