// The reference half of the **highlight-span** differential harness
// (docs/M3.md §5 D14/D15, §6 S3).
//
//   {lang, code} ──┬──► node harness → Prism.tokenize → span JSON ──┐
//                  │                                                 ├──► compare
//                  └──► mt_highlight (xtask/src/highlight.rs) ────────┘
//
// This is the third of three dumpers under `tools/diff/`. `dump-ts-state.mjs`
// compares **block state**, `dump-ts-tokens.mjs` compares muya's **inline token
// stream**, and this one compares the **highlight spans of one fenced code
// block**. Like both siblings it has no opinion about whether the two engines
// agree — comparison is `xtask/src/highlight.rs`'s job, so the pass/fail logic
// lives in one place and this stays a dumb dumper.
//
// WHY THIS EXISTS, AND WHY IT IS WRITTEN BEFORE THE THING IT JUDGES
//
// M3 §7's uncomfortable finding is that for the first time in this project
// *"does it match MarkText?" cannot be answered by running MarkText*. S3 is the
// one exception §6 names: the reference for syntax highlighting is Prism's own
// tokenization, Prism is a Node module in the reference clone, and the harness
// shape that runs it already exists in two working instances. So the stage's
// cross-check can be **automated over every fence in the corpus** rather than
// performed by reading a stylesheet.
//
// §6's "S3 is open" record also fixes the order: **harness → interpreter →
// differential green**, in separate commits. *"An interpreter written first is
// an interpreter whose first check is the thing it was written to satisfy."*
// This file is the first of those commits, and at the moment it lands the Rust
// side ports **no** grammars — `cargo xtask highlight` reports coverage 0/297
// and that is the honest state, not a failure.
//
// WHERE PRISM COMES FROM
//
// **This repository does not depend on prismjs and must not start.** Its
// `node_modules` contains exactly one package (`tsx`). Prism lives in the
// reference clone, where it is `packages/muya/package.json:77`'s dependency at
// version **1.30.0**, and it is resolved the same way the two siblings resolve
// muya: `createRequire` against the clone's own `package.json`. A missing clone
// and an uninstalled clone are **different** failures and this script reports
// them differently — see `resolveMarktextDir` and `loadPrism`.
//
// prismjs is CommonJS **with global side effects**: requiring `prismjs` builds
// `Prism` and assigns `global.Prism` (prism.js:1218), and requiring a component
// file mutates `Prism.languages` rather than exporting anything. Neither
// sibling has that shape, so this one loads it with `createRequire(...)(...)`
// rather than `await import`, which keeps every grammar in one CJS module
// registry — the registry `components/index.js` expects to find on the global.
//
// THE LOAD SET IS PART OF THE CONTRACT (D14)
//
// `components.json` has **298** keys under `languages`; the 298th is `meta`,
// which is not a language. Of the **297** real ones, **292** register a grammar
// under their own id — the other five (`css-extras`, `js-extras`,
// `js-templates`, `php-extras`, `xml-doc`) are modifier packs that only mutate
// somebody else's grammar.
//
// Two things follow, and D14 paid a measurement for each:
//
//   1. **Load order is load-bearing.** Requiring the 297 in alphabetical order
//      throws for `jsdoc`, `plsql`, `racket` and `sparql`, which mutate a
//      grammar that has not been defined yet. `components.json`'s `require`
//      field is the dependency order and `components/index.js` is the loader
//      that honours it. This script calls that loader with the full id list and
//      does not roll its own.
//   2. **A grammar's content depends on the load set**, because thirteen files
//      mutate *other* grammars at load time. So the load set is a property of
//      the differential rather than of the language, both sides must agree on
//      it, and **the full 297 is the definition**. The set is recorded in the
//      output envelope so a run can never be reinterpreted later against a
//      different one.
//
// THE TWO PLACES MUYA'S PRISM IS NOT STOCK PRISM
//
// The question this harness answers is "does the port match **MarkText**", not
// "does it match Prism", and muya patches Prism in two places at load time.
// Both are reproduced here; neither is optional, and a run that skipped them
// would silently be measuring the wrong reference.
//
//   | Patch | Reference | Effect |
//   |---|---|---|
//   | `c++` and `h++` become aliases of `cpp` | `utils/prism/index.ts:16-24` | ```` ```c++ ```` resolves to the cpp grammar (upstream marktext#2910); stock Prism leaves it unhighlighted |
//   | `latex.comment` requires the `%` not to follow a backslash | `utils/prism/index.ts:74-78`, applied at `:81` | `\%` is an escaped literal percent in LaTeX; stock Prism's `/%.*/` swallows the rest of the line (upstream marktext#3037). `tex` and `context` alias the same grammar object, so the one override covers all three |
//
// The alias patch is applied to `components.languages` **before** any alias is
// resolved, because that object is the table `transformAliasToOrigin` reads.
//
// HOW A LANGUAGE NAME BECOMES A GRAMMAR
//
// MarkText does not hand the info-string word to Prism directly. It resolves it
// through its own table first — `transformAliasToOrigin`
// (`utils/prism/loadLanguage.ts:23-54`), called at
// `block/content/codeBlockContent/index.ts:170` before rendering and again at
// `:423` before `prism.tokenize`. The rule is: an id that is a key of
// `components.languages` is itself; otherwise the **first** language whose
// `alias` field contains it, in `components.json` key order; otherwise the word
// unchanged, which then finds no grammar.
//
// That function is **reproduced here rather than imported**, and the reason is
// that importing it would not be enough: the `c++` patch above lives in a
// different file (`utils/prism/index.ts`) which cannot be loaded outside a
// browser — it assigns `window.Prism` and dynamically imports a DOM plugin at
// module scope. Since the patch has to be reproduced either way, reproducing
// the twenty lines beside it is more honest than importing half of the pair.
// The reproduction is checked against the reference by construction: both read
// the same `components.languages` object out of the same prismjs install.
//
// HOW NESTED TOKENS FLATTEN — THE ONE THING D14 AND D15 DO NOT SETTLE
//
// Prism does not emit spans. `Prism.tokenize` returns an **array of strings and
// `Token`s**, and a `Token`'s `content` is itself either a string or another
// such array, arbitrarily deep. Something has to decide what the flat,
// non-overlapping, ascending run list D15 requires looks like.
//
// The reference clone has a flattener — `utils/prism/walkToken.ts:9-27` — and
// **this script deliberately does not use its rule.** `walkTokens` recurses
// into any token whose content is not a string and invokes its callback only on
// the leaves, so the outer token's type is dropped entirely. That is correct
// for what it is used for (`codeBlockContent/index.ts:430-441` walks tokens to
// delete one character and re-emit the text, where only the leaf text matters)
// and wrong for colour: the text inside `<a href="x">` that no inner token
// covers — the tag name, the spaces — renders in the **tag** colour, because
// what MarkText actually paints is `Prism.highlightElement`'s nested `<span>`s,
// where an outer class shows through wherever an inner one does not cover it.
//
// So the rule here is **innermost wins, and an outer token fills its own
// gaps**:
//
//   - a `Token` whose content is a string emits one span with its own class;
//   - a `Token` whose content is nested recurses, and any *string* inside it
//     that no deeper token covers emits a span with the **enclosing** token's
//     class;
//   - a string at the top level, enclosed by nothing, emits **no span at all** —
//     that is the block default, which D15 says must never be pushed as a run.
//
// which reproduces exactly what a browser draws. Adjacent spans that resolve to
// the same class are **not** merged. Merging is a normalisation, this is a dumb
// dumper, and a port that splits one keyword into two tokens has a real grammar
// bug that a merged wire form would hide behind an identical rendering.
//
// WHICH CLASS A TOKEN RESOLVES TO
//
// Prism writes `class="token <type> <alias>…"` (prism.js:864-875) and its
// stylesheets resolve by source order, so an alias wins over the type it
// aliases. `CodePalette::by_class` takes a single class, so D15 picks a
// precedence and records it: **alias over type, last alias wins**, because that
// is what a browser applying muya's own stylesheet does. `type` on the wire is
// that resolved single class. The differential compares boundaries and cannot
// see this choice, which is why D15 records it rather than leaving it to code.
//
// OFFSETS ARE UTF-8 BYTES, AND PRISM COUNTS IN UTF-16
//
// Prism emits **no offsets at all**. What it emits is `Token.length`, set to
// `(matchedStr || '').length | 0` at prism.js:817 — a **UTF-16 code-unit**
// count — and the walk above accumulates it, exactly as muya's own walk does
// (`codeBlockContent/index.ts:441`, `offset -= token.length`).
//
// Converting the accumulated position to bytes is S5's second handoff note,
// inherited verbatim from `dump-ts-tokens.mjs:260-307`: `offsetTable` walks
// **code points** and stores the byte offset of each UTF-16 index, because an
// astral character is one code point, two UTF-16 units and four bytes, so
// neither the index nor any fixed multiple of it is the answer. The interior of
// a surrogate pair is **-1** and `byteOffset` throws rather than returning a
// plausible number: a harness bug and a port bug look identical in the output
// unless the harness refuses to guess. Accumulating UTF-8 lengths directly
// while walking is the wrong fix — `Token.length` is not in bytes and the walk
// never sees the matched text of a nested token.
//
// One check comes free with the walk and is worth having: a nested token's
// `length` must equal the total its children consume. That held across all 292
// grammars when this file was written, including the templating languages whose
// `after-tokenize` hooks rebuild the token array, so a mismatch means the walk
// has lost its place and it throws rather than emitting shifted spans.
//
// USAGE
//   node tools/diff/dump-prism-tokens.mjs --inputs <FILE> [OPTIONS]
//
//   --inputs <FILE>     JSON: an array of `{lang, code}` objects, `label`
//                       optional and ignored (results are positional, and the
//                       Rust side already holds the labels). A file rather than
//                       argv because the corpus is thousands of fences and
//                       every platform has a command-line length limit.
//   --out <FILE>        Write the result here instead of to stdout.
//   --marktext <DIR>    Path to the marktext clone. Defaults to $MARKTEXT_DIR,
//                       then to a sibling `../marktext` of this repo.
//
//   No `--import tsx`, unlike the two siblings: nothing here is TypeScript.
//
// OUTPUT
//   { engine, marktextDir, prism: {version, languages, grammars, modifierPacks},
//     loaded: [id…], patches: [str…], results: [ … ] }
//
//   One result per input, in order, in exactly one of four shapes:
//
//   | Shape | Means |
//   |---|---|
//   | `{resolved, spans: [{start, end, type}…]}` | tokenized, and highlighted |
//   | `{resolved}` | tokenized, and **nothing** was highlighted |
//   | `{resolved, noGrammar: true}` | no grammar under that name — a coverage fact, not a tokenization one |
//   | `{error}` | Prism threw on this input |
//
//   `resolved` is the id **after** MarkText's alias resolution, so `ts` reports
//   `typescript`. It is `''` for a fence with no info string, which resolves to
//   nothing and is reported as `noGrammar`. `noGrammar` is decided against the
//   **292**, not against `Prism.languages`, which also carries four aliases of
//   one empty plain-text grammar that no manifest lists; see `main`. `spans` is
//   omitted when empty, for
//   the same absent-versus-empty reason `dump-ts-tokens.mjs` omits an empty
//   `highlights`: an empty list and no list are one state, and both sides must
//   spell it the same way.
//
// EXIT CODES
//   0  success
//   1  error (bad arguments, unreadable file, harness bug)
//   3  Prism could not be loaded — the marktext clone is missing or its
//      dependencies are not installed. Distinct from 1 so the comparison runner
//      can report SKIPPED rather than failing CI on a machine that simply does
//      not have the reference engine checked out. A throw from Prism on **one
//      input** is not this; it is recorded per input as `{error}`.

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const EXIT_OK = 0;
const EXIT_ERROR = 1;
const EXIT_ENGINE_UNAVAILABLE = 3;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

class EngineUnavailable extends Error {}

function parseArgs(argv) {
    let inputsFile = null;
    let outFile = null;
    let marktextDir = null;

    for (let i = 0; i < argv.length; i++) {
        const arg = argv[i];
        if (arg === '--inputs') {
            inputsFile = argv[++i];
            if (inputsFile === undefined)
                throw new Error('--inputs requires a file argument');
        }
        else if (arg === '--out') {
            outFile = argv[++i];
            if (outFile === undefined)
                throw new Error('--out requires a file argument');
        }
        else if (arg === '--marktext') {
            marktextDir = argv[++i];
            if (marktextDir === undefined)
                throw new Error('--marktext requires a directory argument');
        }
        else if (arg === '--help' || arg === '-h') {
            return { help: true };
        }
        else {
            throw new Error(`unrecognised argument: ${arg}`);
        }
    }

    if (!inputsFile)
        throw new Error('--inputs is required; try --help');

    return { inputsFile, outFile, marktextDir, help: false };
}

/**
 * Locate the marktext clone.
 *
 * Identical policy to both siblings, deliberately: `--marktext` beats
 * `$MARKTEXT_DIR` beats a sibling `../marktext`, and an *explicitly named*
 * directory is authoritative — if it does not contain the engine that is an
 * error, not a cue to look elsewhere. Falling back would let a typo in
 * $MARKTEXT_DIR silently compare against a different engine than the one that
 * was asked for, which is worse than failing.
 *
 * **The sentinel differs from the siblings' in one considered way.** They probe
 * the file they go on to import. The file *this* script imports from the clone
 * is `node_modules/prismjs/prism.js`, and probing that would collapse two
 * failures that want different remediations into one message: "there is no
 * clone here" and "the clone is here but `pnpm install` has not run". So the
 * sentinel is the clone's own alias table — the source file this script
 * reproduces, present in any checkout — and the uninstalled case is reported by
 * `loadPrism` with the install command.
 */
function resolveMarktextDir(explicit) {
    const named = explicit ?? process.env.MARKTEXT_DIR;
    const source = explicit ? '--marktext' : '$MARKTEXT_DIR';
    const candidates = named ? [named] : [path.resolve(repoRoot, '..', 'marktext')];

    for (const candidate of candidates) {
        const aliasTable = path.join(
            candidate, 'packages', 'muya', 'src', 'utils', 'prism', 'loadLanguage.ts',
        );
        if (fs.existsSync(aliasTable))
            return path.resolve(candidate);
    }

    throw new EngineUnavailable(
        named
            ? `${source} points at ${named}, which does not contain `
              + 'packages/muya/src/utils/prism/loadLanguage.ts.'
            : `could not find the marktext clone at ${candidates[0]}.\n`
              + 'Set MARKTEXT_DIR or pass --marktext <DIR>.',
    );
}

/**
 * Load Prism and every grammar, from the clone's `node_modules`.
 *
 * Returns `{Prism, components, ids, version, grammars, modifierPacks}`.
 *
 * `createRequire` against the clone's `package.json` rather than a bare
 * specifier: prismjs is that repository's dependency, not this one's, and this
 * one must not acquire it. `require` rather than `await import` because
 * prismjs is CommonJS whose whole interface is a side effect on a shared
 * object — see the header.
 */
function loadPrism(marktextDir) {
    const require = createRequire(path.join(marktextDir, 'package.json'));
    let entry;
    try {
        entry = require.resolve('prismjs');
    }
    catch (cause) {
        throw new EngineUnavailable(
            `prismjs is not installed in ${marktextDir}: ${cause.message}\n`
            + 'Run:  pnpm install --filter @muyajs/core --ignore-scripts\n'
            + `in ${marktextDir}`,
        );
    }

    let Prism;
    let components;
    let loadLanguages;
    let version;
    try {
        Prism = require('prismjs');
        components = require('prismjs/components.js');
        loadLanguages = require('prismjs/components/index.js');
        version = require('prismjs/package.json').version;
    }
    catch (cause) {
        throw new EngineUnavailable(
            `failed to load prismjs from ${entry}: ${cause.message}\n`
            + 'Run:  pnpm install --filter @muyajs/core --ignore-scripts\n'
            + `in ${marktextDir}`,
        );
    }

    // Before the load, because a component file may read the alias table, and
    // before any resolution, because the table is what resolution reads.
    const patches = applyMuyaAliasPatch(components);

    // `meta` is the 298th key and is not a language: it is the path template
    // (`components/prism-{id}`) every other entry is expanded against.
    const ids = Object.keys(components.languages).filter(id => id !== 'meta');

    // The full set, in dependency order, by Prism's own loader. See the
    // header: alphabetical order throws, and a partial load produces different
    // grammars than a full one.
    loadLanguages.silent = true;
    try {
        loadLanguages(ids);
    }
    catch (cause) {
        throw new EngineUnavailable(
            `prismjs failed to load its ${ids.length} grammars: ${cause.message}\n`
            + `The install under ${marktextDir} may be incomplete.`,
        );
    }

    patches.push(...applyMuyaLatexPatch(Prism));

    // The five modifier packs register nothing under their own id. Computed
    // rather than hard-coded: the day the reference clone moves to a prismjs
    // that changes the set, the envelope says so instead of a comment lying.
    const modifierPacks = ids.filter(
        id => !Object.prototype.hasOwnProperty.call(Prism.languages, id),
    );

    return {
        Prism,
        components,
        ids,
        version,
        grammars: ids.length - modifierPacks.length,
        modifierPacks,
        patches,
    };
}

/**
 * muya's `c++` / `h++` aliases (`utils/prism/index.ts:16-24`).
 *
 * prismjs ships C++ without them, so ```` ```c++ ```` never resolves and stays
 * unhighlighted. Reproduced with the same guard the reference carries — the
 * alias array is a shared singleton and pushing a duplicate makes Prism's
 * dependency loader throw *"c++ cannot be alias for both cpp and cpp"*.
 */
function applyMuyaAliasPatch(components) {
    const cpp = components.languages.cpp;
    if (!cpp)
        return [];
    const existing = cpp.alias;
    const alias = Array.isArray(existing) ? [...existing] : existing ? [existing] : [];
    const added = [];
    for (const name of ['c++', 'h++']) {
        if (!alias.includes(name)) {
            alias.push(name);
            added.push(name);
        }
    }
    cpp.alias = alias;
    return added.length ? [`cpp: +${added.join(' +')} (marktext utils/prism/index.ts:16-24)`] : [];
}

/**
 * muya's LaTeX escaped-percent fix (`utils/prism/index.ts:74-78`).
 *
 * Applied to the loaded grammar, which is what `patchLatexEscapedPercent` does
 * at `:81` once `loadLanguage('latex')` resolves. `tex` and `context` are
 * aliases of the same grammar **object**, so this one override covers all
 * three — which is a property of Prism's alias mechanism, not a coincidence,
 * and the port has to reproduce the sharing as well as the pattern.
 */
function applyMuyaLatexPatch(Prism) {
    const latex = Prism.languages.latex;
    if (!latex?.comment)
        return [];
    latex.comment = { pattern: /(^|[^\\])%.*/, lookbehind: true };
    return ['latex.comment: /(^|[^\\\\])%.*/ lookbehind (marktext utils/prism/index.ts:74-78)'];
}

/**
 * MarkText's info-string word → grammar id, reproduced from
 * `utils/prism/loadLanguage.ts:23-54`.
 *
 * Reproduced rather than imported; the header says why. Three cases in the
 * reference's own order, and the order is the contract: a name that is itself a
 * language wins even if some *other* language claims it as an alias, and among
 * aliases the first match in `components.json` key order wins.
 *
 * An id that resolves to nothing comes back unchanged — the reference pushes
 * `lang` and lets `initLoadLanguage` report `noexist`. Here that becomes
 * `noGrammar` on the wire.
 */
function transformAliasToOrigin(languages, lang) {
    if (languages[lang])
        return lang;

    const found = Object.keys(languages).find((name) => {
        const alias = languages[name].alias;
        if (!alias)
            return false;
        return alias === lang || (Array.isArray(alias) && alias.includes(lang));
    });

    return found ?? lang;
}

// ---------------------------------------------------------------------------
// The wire form
// ---------------------------------------------------------------------------

function utf8Length(codePoint) {
    if (codePoint < 0x80)
        return 1;
    if (codePoint < 0x800)
        return 2;
    if (codePoint < 0x10000)
        return 3;
    return 4;
}

/**
 * `table[i]` is the UTF-8 byte offset of UTF-16 code-unit index `i`.
 *
 * Verbatim from `dump-ts-tokens.mjs:284-296`, and deliberately not factored
 * into a shared module: these three scripts share a convention, not a library,
 * and each one is meant to be readable on its own by whoever is debugging a
 * disagreement at 2am. Built by walking code points, because that is the only
 * way that is right for astral characters: `'𝄞'.length` is 2 and its UTF-8
 * length is 4, so neither the index nor any fixed multiple of it is the answer.
 *
 * The **interior of a surrogate pair is -1**, not the pair's start.
 */
function offsetTable(src) {
    const table = new Int32Array(src.length + 1).fill(-1);
    let byte = 0;
    let i = 0;
    while (i < src.length) {
        const codePoint = src.codePointAt(i);
        table[i] = byte;
        byte += utf8Length(codePoint);
        i += codePoint > 0xFFFF ? 2 : 1;
    }
    table[src.length] = byte;
    return table;
}

function byteOffset(table, utf16) {
    const at = table[utf16];
    if (at === undefined || at < 0) {
        throw new Error(
            `UTF-16 offset ${utf16} is out of range or inside a surrogate pair; `
            + 'a Prism pattern split an astral character, and this harness will not '
            + 'guess which side of it a span ends on',
        );
    }
    return at;
}

/**
 * The single class a token resolves to: **alias over type, last alias wins**.
 *
 * D15, and the header. Prism's class list is `token <type> <alias>…`
 * (prism.js:864-875); `CodePalette::by_class` takes one class, so a precedence
 * has to be chosen and written down. `alias` is a string or an array of them.
 */
function resolveClass(token) {
    const alias = token.alias;
    if (Array.isArray(alias))
        return alias.length ? alias[alias.length - 1] : token.type;
    if (typeof alias === 'string' && alias !== '')
        return alias;
    return token.type;
}

/**
 * Walk one level of Prism's token array, emitting spans; returns the UTF-16
 * position after it.
 *
 * `enclosing` is the class of the token this array is the content of, or `null`
 * at the top level. That parameter *is* the flattening rule described in the
 * header: a bare string paints in its enclosing token's colour, and at the top
 * level there is no enclosing token, so it paints in the block default and
 * emits nothing.
 */
function flatten(items, at, enclosing, table, spans) {
    for (const item of items) {
        if (typeof item === 'string') {
            if (enclosing !== null && item.length > 0) {
                spans.push({
                    start: byteOffset(table, at),
                    end: byteOffset(table, at + item.length),
                    type: enclosing,
                });
            }
            at += item.length;
            continue;
        }

        const type = resolveClass(item);
        if (typeof item.content === 'string') {
            if (item.content.length !== item.length) {
                throw new Error(
                    `token ${item.type} has length ${item.length} but ${item.content.length} `
                    + 'code units of content; the walk has lost its place',
                );
            }
            if (item.length > 0) {
                spans.push({
                    start: byteOffset(table, at),
                    end: byteOffset(table, at + item.length),
                    type,
                });
            }
            at += item.length;
            continue;
        }

        // `content` is an array, or — for a token built by a hook — a single
        // nested token. muya's own walk normalises the second case the same
        // way (`utils/prism/walkToken.ts:22`).
        const nested = Array.isArray(item.content) ? item.content : [item.content];
        const after = flatten(nested, at, type, table, spans);
        if (after - at !== item.length) {
            throw new Error(
                `token ${item.type} claims length ${item.length} but its content spans `
                + `${after - at}; the walk has lost its place`,
            );
        }
        at = after;
    }
    return at;
}

/** Every span of one input, in ascending order, non-overlapping. */
function spansOf(Prism, grammar, code) {
    const table = offsetTable(code);
    const spans = [];
    const end = flatten(Prism.tokenize(code, grammar), 0, null, table, spans);
    if (end !== code.length) {
        throw new Error(
            `tokenization covered ${end} of ${code.length} code units; `
            + 'the walk has lost its place',
        );
    }
    return spans;
}

async function main() {
    let args;
    try {
        args = parseArgs(process.argv.slice(2));
    }
    catch (e) {
        process.stderr.write(`dump-prism-tokens: ${e.message}\n`);
        return EXIT_ERROR;
    }

    if (args.help) {
        process.stdout.write(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8')
            .split('\n')
            .filter(l => l.startsWith('//'))
            .map(l => l.replace(/^\/\/ ?/, ''))
            .join('\n') + '\n');
        return EXIT_OK;
    }

    let inputs;
    try {
        // `readFileSync(..., 'utf8')` does not strip a BOM, unlike
        // `TextDecoder`'s default — S5's third handoff note. A code fence whose
        // first character is U+FEFF is a real input and the transport must not
        // quietly repair it.
        const parsed = JSON.parse(fs.readFileSync(args.inputsFile, 'utf8'));
        const entries = Array.isArray(parsed) ? parsed : parsed.inputs;
        if (!Array.isArray(entries))
            throw new Error('expected an array of entries, or an object with an `inputs` array');
        inputs = entries.map((entry) => {
            if (entry === null || typeof entry !== 'object' || typeof entry.code !== 'string')
                throw new TypeError(`entry ${JSON.stringify(entry)} is not {lang, code}`);
            return { lang: entry.lang ?? '', code: entry.code };
        });
    }
    catch (e) {
        process.stderr.write(`dump-prism-tokens: cannot read ${args.inputsFile}: ${e.message}\n`);
        return EXIT_ERROR;
    }

    let engine;
    let marktextDir;
    try {
        marktextDir = resolveMarktextDir(args.marktextDir);
        engine = loadPrism(marktextDir);
    }
    catch (e) {
        // One place, one marker class. Anything else is a harness bug and must
        // not be laundered into a skip.
        if (e instanceof EngineUnavailable) {
            process.stderr.write(`dump-prism-tokens: engine unavailable: ${e.message}\n`);
            return EXIT_ENGINE_UNAVAILABLE;
        }
        throw e;
    }

    const { Prism, components } = engine;
    // Membership in `components.languages`, not merely in `Prism.languages`.
    // The two differ by more than aliases: prism-core registers `plain`,
    // `plaintext`, `text` and `txt` as one shared **empty** grammar
    // (prism.js:308-311) which is in no manifest and highlights nothing.
    // MarkText's own condition is `loadedLanguages.has(fullLengthLang)`
    // (`codeBlockContent/index.ts:181` and `:424`) and that set only ever contains
    // manifest ids, so a ```` ```txt ```` fence is unhighlighted there. Keeping
    // the same boundary here makes `noGrammar` mean exactly *"not one of the
    // 292"*, which is the fact the coverage ratchet counts — otherwise four
    // pseudo-languages would report as "tokenized to nothing" and quietly
    // inflate the tokenized count.
    const known = new Set(engine.ids);
    const results = [];
    for (const input of inputs) {
        const resolved = transformAliasToOrigin(components.languages, input.lang);
        const grammar = known.has(resolved) ? Prism.languages[resolved] : undefined;
        if (!grammar || typeof grammar !== 'object') {
            results.push({ resolved, noGrammar: true });
            continue;
        }
        try {
            const spans = spansOf(Prism, grammar, input.code);
            // Absent and empty are one state and are spelled the same way on
            // both sides — the rule `dump-ts-tokens.mjs` applies to
            // `highlights`. A result with a `resolved` and no `spans` is "this
            // tokenized to nothing", which is a *tokenization* fact and is not
            // the same claim as `noGrammar`.
            results.push(spans.length ? { resolved, spans } : { resolved });
        }
        catch (e) {
            // A throw from the engine is a finding, not a harness bug: record
            // it per input so one bad fence does not blind the rest of a run.
            results.push({ error: `${e.name}: ${e.message}` });
        }
    }

    const envelope = {
        engine: `prismjs ${engine.version} (components/index.js, full load)`,
        marktextDir,
        prism: {
            version: engine.version,
            languages: engine.ids.length,
            grammars: engine.grammars,
            modifierPacks: engine.modifierPacks,
        },
        // D14 consequence 2: a grammar's content depends on the load set, so a
        // run that does not record its load set cannot be reinterpreted later.
        loaded: engine.ids,
        patches: engine.patches,
        results,
    };
    const text = `${JSON.stringify(envelope)}\n`;
    if (args.outFile)
        fs.writeFileSync(args.outFile, text);
    else
        process.stdout.write(text);

    return EXIT_OK;
}

process.exitCode = await main();
