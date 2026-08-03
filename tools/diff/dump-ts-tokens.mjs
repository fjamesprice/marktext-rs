// The TypeScript half of the **token-stream** differential harness
// (docs/M1.md §5 D3, §6 S7).
//
//   input string ──┬──► node harness → muya tokenizer → token JSON ──┐
//                  │                                                  ├──► compare
//                  └──► mt_inline::tokenizer (xtask/src/tokens.rs) ───┘
//
// This is the sibling of `dump-ts-state.mjs`, one layer down. That script
// compares **block state** and is what RUST-REWRITE-PLAN.md §11.2 specifies;
// this one compares the **token stream** of a single leaf block, which is what
// `mt-inline` produces and what M1's divergence register (§5 D3) is written in
// terms of. Neither subsumes the other: block state does not carry inline
// tokens, and a token stream has no blocks.
//
// Like its sibling it has no opinion about whether the two engines agree —
// comparison is `xtask/src/tokens.rs`'s job, so the pass/fail logic lives in
// one place and this stays a dumb dumper.
//
// WHY THIS EXISTS
//
// `spec/divergences.json` records every place the port deliberately disagrees
// with muya. Register rule 3 says an entry with no failing differential case is
// stale — but from S0 to S6 there was no token-stream comparison to fail, so
// every entry reported SKIPPED and the rule could not fire. Five stages each
// built a throwaway comparator by hand (49,751 inputs at S5, 5586 at S6), and
// S5 established what that costs: a hand-run can return a **false negative and
// be written down as a clean result** — S4 probed one divergence class with two
// inputs, found both agreeing, recorded that the register needed no widening,
// and 96 of 228 swept inputs disagree. This script is the sixth comparator and
// the first permanent one.
//
// THE WIRE SHAPE, AND THE FIVE THINGS THAT MAKE IT COMPARABLE
//
// Handed down by S3, S5 and S6 (see xtask/src/divergences.rs, "What S5 leaves
// for whoever builds it"), and every one of them cost a run to learn:
//
//   1. Strings are compared as **hex of their UTF-8 bytes**. That removes every
//      encoding question from the diff in one move, and makes a disagreement in
//      an invisible character (a BOM, a NEL, a ZWJ) visible in the output.
//   2. muya's `range` is in **UTF-16 code units**; the port's is in UTF-8
//      bytes. The conversion walks code points — not `length`, not a division —
//      because an astral character is one code point, two UTF-16 units and four
//      bytes. `offsetTable` below builds the map once per input, and stores
//      **-1** for the interior of a surrogate pair so that a harness that ever
//      reads one throws instead of silently reporting a plausible number.
//   3. **The transport must not normalise.** S5 lost a run to `TextDecoder`'s
//      default BOM-stripping showing up as four tokenizer disagreements. Inputs
//      arrive here as a JSON array parsed with `{ignoreBOM: true}` and are
//      never re-encoded.
//   4. **`undefined` and `''` are different, per field, not per type.** muya
//      writes `to[3] || ''` at some sites and a bare `to[3]` at others, so
//      `inline_code`'s `backlash` really is `undefined` while `header`'s is
//      `''`. The rule here is uniform and mechanical: **a key whose value is
//      `undefined` is omitted**, and so is a key muya's object literal never
//      had (the comment branch of `tryHtmlTag` has no `closeTag`, `content` or
//      `children`). The Rust side omits exactly the same fields, so "absent"
//      compares against "absent" and `''` against `''`.
//   5. **`highlights` is the same question again.** muya creates the key on the
//      first intersection, so a token that intersects nothing has no key at
//      all; the port gives every token an empty `Vec` (M1.md §5 D7). An empty
//      list is therefore omitted here too — otherwise every token in every
//      document reports a disagreement.
//
// `parent` is dropped: it is a cyclic back-reference into the array the token
// lives in, it is not data (M1.md §3), and the port does not model it.
//
// getAttributes NEEDS A DOM
//
// `tryHtmlTag` calls `getAttributes`, which calls `DOMParser` — which is why
// every one of muya's seven inline spec files carries
// `// @vitest-environment happy-dom`. Without a `DOMParser` global the handler
// throws, so this script registers happy-dom's before loading anything. It is
// resolved with `createRequire` against the **marktext clone's** package.json,
// not by bare specifier: happy-dom is that repository's dev dependency, not
// this one's.
//
// USAGE
//   node --import tsx tools/diff/dump-ts-tokens.mjs --inputs <FILE> [OPTIONS]
//
//   --inputs <FILE>     JSON: an array whose entries are either a plain string
//                       (tokenized with muya's defaults) or an object
//                       `{src, labels, footnote}`. Required. A file rather than
//                       argv because a sweep is thousands of inputs and every
//                       platform has a command-line length limit.
//
//                       The object form exists because three of the 26 token
//                       types are unreachable under the defaults:
//                       `reference_link` and `reference_image` are gated on
//                       `state.labels.has(label)` and `footnote_identifier` on
//                       `options.footnote`. Only membership in the label map is
//                       read (lexer.ts:415, :466), so `labels` is a list of
//                       keys with no targets.
//   --out <FILE>        Write the result here instead of to stdout.
//   --marktext <DIR>    Path to the marktext clone. Defaults to $MARKTEXT_DIR,
//                       then to a sibling `../marktext` of this repo.
//
// OUTPUT
//   { engine, marktextDir, options, results: [ {tokens} | {error} ] }
//   one entry per input, in order.
//
// EXIT CODES
//   0  success
//   1  error (bad arguments, unreadable file, harness bug)
//   3  the muya engine could not be loaded — the marktext clone is missing or
//      its dependencies are not installed. Distinct from 1 so the comparison
//      runner can report SKIPPED rather than failing CI on a machine that
//      simply does not have the TypeScript engine checked out.

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const EXIT_OK = 0;
const EXIT_ERROR = 1;
const EXIT_ENGINE_UNAVAILABLE = 3;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

// muya's `tokenizer` destructuring defaults (lexer.ts:855-861), which is what
// `mt_inline::TokenizerOptions::muya_default()` mirrors. If the two engines are
// driven with different options a disagreement is uninterpretable.
const MUYA_DEFAULT_SYNTAX = {
    superSubScript: true,
    footnote: false,
};

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
 * Identical policy to `dump-ts-state.mjs`, deliberately: `--marktext` beats
 * `$MARKTEXT_DIR` beats a sibling `../marktext`, and an *explicitly named*
 * directory is authoritative — if it does not contain the engine that is an
 * error, not a cue to look elsewhere. Falling back would let a typo in
 * $MARKTEXT_DIR silently compare against a different engine than the one that
 * was asked for, which is worse than failing.
 */
function resolveMarktextDir(explicit) {
    const named = explicit ?? process.env.MARKTEXT_DIR;
    const source = explicit ? '--marktext' : '$MARKTEXT_DIR';
    const candidates = named ? [named] : [path.resolve(repoRoot, '..', 'marktext')];

    for (const candidate of candidates) {
        const lexer = path.join(candidate, 'packages', 'muya', 'src', 'inlineRenderer', 'lexer.ts');
        if (fs.existsSync(lexer))
            return path.resolve(candidate);
    }

    throw new EngineUnavailable(
        named
            ? `${source} points at ${named}, which does not contain `
              + 'packages/muya/src/inlineRenderer/lexer.ts.'
            : `could not find the marktext clone at ${candidates[0]}.\n`
              + 'Set MARKTEXT_DIR or pass --marktext <DIR>.',
    );
}

/**
 * Register a `DOMParser` global from the marktext clone's happy-dom.
 *
 * `getAttributes` (utils.ts:172) constructs one unconditionally, so `html_tag`
 * throws without this and every `<tag …>` in the sweep would report an engine
 * error rather than a token stream. muya's own spec files get it from
 * `// @vitest-environment happy-dom`; this is that line, by hand.
 */
async function installDom(marktextDir) {
    const require = createRequire(path.join(marktextDir, 'package.json'));
    let entry;
    try {
        entry = require.resolve('happy-dom');
    }
    catch (cause) {
        throw new EngineUnavailable(
            `happy-dom is not installed in ${marktextDir}: ${cause.message}\n`
            + 'Run:  pnpm install --filter @muyajs/core --ignore-scripts\n'
            + `in ${marktextDir}`,
        );
    }
    const { Window } = await import(pathToFileURL(entry).href);
    const window = new Window();
    globalThis.DOMParser = window.DOMParser;
    globalThis.document = window.document;
}

async function loadTokenizer(marktextDir) {
    const entry = path.join(marktextDir, 'packages', 'muya', 'src', 'inlineRenderer', 'lexer.ts');
    // Import by file:// URL, not by path: on Windows an absolute path like
    // `C:\...` is read by the ESM loader as a URL with scheme `c:`.
    const url = pathToFileURL(entry).href;
    try {
        const mod = await import(url);
        if (typeof mod.tokenizer !== 'function')
            throw new Error('lexer.ts did not export tokenizer');
        return mod.tokenizer;
    }
    catch (cause) {
        throw new EngineUnavailable(
            `failed to load muya's tokenizer from ${marktextDir}: ${cause.message}\n`
            + 'Run:  pnpm install --filter @muyajs/core --ignore-scripts\n'
            + `in ${marktextDir}`,
        );
    }
}

// ---------------------------------------------------------------------------
// The wire form
// ---------------------------------------------------------------------------

const encoder = new TextEncoder();

/** A string as lowercase hex of its UTF-8 bytes. See point 1 in the header. */
function hex(s) {
    if (s === '')
        return '';
    let out = '';
    for (const byte of encoder.encode(s))
        out += byte.toString(16).padStart(2, '0');
    return out;
}

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
 * Point 2 in the header. Built by walking code points, because that is the only
 * way that is right for astral characters: `'𝄞'.length` is 2 and its UTF-8
 * length is 4, so neither the index nor any fixed multiple of it is the answer.
 *
 * The **interior of a surrogate pair is -1**, not the pair's start. muya never
 * produces an offset there — every range endpoint is a regex match boundary and
 * JavaScript regexes do not split surrogate pairs at these sites — so a lookup
 * that hits one means this harness is wrong, and it should say so rather than
 * return a plausible number. S5's lesson: a harness bug and a port bug look
 * identical in the output unless the harness refuses to guess.
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
            + 'this is a harness bug, not a tokenizer disagreement',
        );
    }
    return at;
}

/**
 * One token, in the shape `xtask/src/tokens.rs` produces for the Rust side.
 *
 * Every own key except `parent` is carried through; a key whose value is
 * `undefined` is **omitted**, which is point 4 in the header and is what makes
 * `tryHtmlTag`'s two branches — one of which never writes `closeTag`,
 * `content` or `children` at all — compare against the port's `Option::None`
 * without a per-type field list on either side.
 */
function wireToken(token, table) {
    const out = {};

    for (const key of Object.keys(token)) {
        if (key === 'parent')
            continue;

        const value = token[key];
        if (value === undefined)
            continue;

        switch (key) {
            case 'type':
            case 'linkType':
                out[key] = value;
                break;

            case 'range':
                out.range = {
                    start: byteOffset(table, value.start),
                    end: byteOffset(table, value.end),
                };
                break;

            case 'children':
                out.children = value.map(child => wireToken(child, table));
                break;

            case 'highlights':
                // Point 5: an empty list is the same state as no key at all,
                // and the port always has a (possibly empty) Vec.
                if (value.length) {
                    out.highlights = value.map(h => ({
                        start: byteOffset(table, h.start),
                        end: byteOffset(table, h.end),
                        active: h.active === undefined ? null : h.active,
                    }));
                }
                break;

            case 'attrs':
                // `image` gives `{src, title, alt}`; `html_tag` gives whatever
                // `getAttributes` whitelisted. Both are plain objects whose
                // insertion order is observable, so this is a list of pairs
                // rather than an object — the Rust side models it as a `Vec`
                // for exactly that reason (token.rs, `HtmlTag::attrs`).
                out.attrs = Object.keys(value).map(name => [hex(name), hex(value[name])]);
                break;

            case 'backlash':
                // A string on most tokens; `{first, second}` on links and
                // images.
                out.backlash = typeof value === 'string'
                    ? hex(value)
                    : {
                            first: hex(value.first),
                            ...(value.second === undefined ? {} : { second: hex(value.second) }),
                        };
                break;

            case 'isAtEnd':
            case 'isLink':
            case 'isFullLink':
                out[key] = value;
                break;

            default:
                if (typeof value !== 'string') {
                    throw new Error(
                        `token.${key} is a ${typeof value}; this dumper only knows how to `
                        + 'encode strings, and a new non-string field needs a case above',
                    );
                }
                out[key] = hex(value);
                break;
        }
    }

    return out;
}

async function main() {
    let args;
    try {
        args = parseArgs(process.argv.slice(2));
    }
    catch (e) {
        process.stderr.write(`dump-ts-tokens: ${e.message}\n`);
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
        // Point 3: `readFileSync(..., 'utf8')` does not strip a BOM, unlike
        // `TextDecoder`'s default. The inputs are JSON string literals anyway,
        // so a leading U+FEFF inside one survives into the tokenizer.
        const parsed = JSON.parse(fs.readFileSync(args.inputsFile, 'utf8'));
        const entries = Array.isArray(parsed) ? parsed : parsed.inputs;
        if (!Array.isArray(entries))
            throw new Error('expected an array of entries, or an object with an `inputs` array');
        inputs = entries.map((entry) => {
            if (typeof entry === 'string')
                return { src: entry, labels: [], footnote: false };
            if (entry === null || typeof entry !== 'object' || typeof entry.src !== 'string')
                throw new TypeError(`entry ${JSON.stringify(entry)} is neither a string nor {src, …}`);
            return {
                src: entry.src,
                labels: entry.labels ?? [],
                footnote: entry.footnote ?? false,
            };
        });
    }
    catch (e) {
        process.stderr.write(`dump-ts-tokens: cannot read ${args.inputsFile}: ${e.message}\n`);
        return EXIT_ERROR;
    }

    let tokenizer;
    let marktextDir;
    try {
        marktextDir = resolveMarktextDir(args.marktextDir);
        await installDom(marktextDir);
        tokenizer = await loadTokenizer(marktextDir);
    }
    catch (e) {
        if (e instanceof EngineUnavailable) {
            process.stderr.write(`dump-ts-tokens: engine unavailable: ${e.message}\n`);
            return EXIT_ENGINE_UNAVAILABLE;
        }
        throw e;
    }

    const results = [];
    for (const input of inputs) {
        try {
            // `tryReferenceLink` / `tryReferenceImage` only call `labels.has`,
            // so the values are never read; an empty object keeps the map's
            // shape without inventing a target.
            const labels = new Map(input.labels.map(label => [label, {}]));
            const tokens = tokenizer(input.src, {
                highlights: [],
                hasBeginRules: true,
                labels,
                options: { ...MUYA_DEFAULT_SYNTAX, footnote: input.footnote },
            });
            const table = offsetTable(input.src);
            results.push({ tokens: tokens.map(token => wireToken(token, table)) });
        }
        catch (e) {
            // A throw from the engine is a finding, not a harness bug: record
            // it per-input so one bad input does not blind the rest of a sweep.
            results.push({ error: `${e.name}: ${e.message}` });
        }
    }

    const envelope = {
        engine: 'muya inlineRenderer/lexer.ts',
        marktextDir,
        options: MUYA_DEFAULT_SYNTAX,
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
