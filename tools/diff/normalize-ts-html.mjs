// The TypeScript half of the `normalizeHtml` differential (docs/M2.md §10,
// "Discharged in M2 — `normalizeHtml`"; M0 decision 12 and M1 §10 both owe it
// here).
//
//   rendered HTML ──┬──► spec/runner.ts normalizeHtml ──┐
//                   │                                    ├──► assert equal
//                   └──► xtask/src/html.rs normalize_html┘
//
// This script is the top branch. It reads a JSON array of HTML strings and
// prints the array of normalised strings. It has no opinion about whether the
// Rust side agrees — comparison is `cargo xtask normalize`'s job
// (xtask/src/normalize.rs), for the same reason `dump-ts-state.mjs` leaves it
// to `diff.rs`: the pass/fail logic lives in one place.
//
// WHY THIS EXISTS AT ALL
//
// `xtask/src/html.rs` is a hand port of four regex passes, and its own header
// has said since M0 that "nothing here can be checked end-to-end until
// `mt_md::render_to_static_html` produces output. When it does, the cheapest
// high-confidence check is differential." M2 S5 is when that became possible.
//
// WHAT IS BEING CHECKED, AND WHAT IS NOT
//
// The claim is **agreement**, not correctness. `normalizeHtml` has two known
// limitations — an attribute value containing `>` breaks tag scanning, and the
// attribute pass does not lower-case tag names — and the Rust port reproduces
// both **deliberately**. Fixing either here would make the two runners
// disagree, which is the one thing this check exists to prevent. See
// xtask/src/html.rs's header and docs/M2.md §10.
//
// USAGE
//   node --import tsx tools/diff/normalize-ts-html.mjs --inputs <FILE> [--out <FILE>]
//
// The input file is a JSON array of strings, not argv: a run is thousands of
// HTML fragments and every platform has a command-line length limit. Same
// shape, and the same reason, as `dump-ts-tokens.mjs --inputs`.
//
// EXIT CODES
//   0  success
//   1  error (bad arguments, unreadable file)
//
// There is no exit 3 here and that is the point: `spec/runner.ts` lives in
// *this* repository, so unlike the other two harnesses this one needs no
// marktext clone. It needs Node and `tsx`, which `--require-ts` is about.

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const EXIT_OK = 0;
const EXIT_ERROR = 1;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

function parseArgs(argv) {
    let inputsPath = null;
    let outPath = null;
    for (let i = 0; i < argv.length; i++) {
        const arg = argv[i];
        if (arg === '--inputs') {
            inputsPath = argv[++i];
            if (inputsPath === undefined)
                throw new Error('--inputs requires a file argument');
        }
        else if (arg === '--out') {
            outPath = argv[++i];
            if (outPath === undefined)
                throw new Error('--out requires a file argument');
        }
        else if (arg === '--help' || arg === '-h') {
            return { help: true };
        }
        else {
            throw new Error(`unrecognised argument: ${arg}`);
        }
    }
    if (inputsPath === null)
        throw new Error('--inputs is required; try --help');
    return { inputsPath, outPath, help: false };
}

async function main() {
    let args;
    try {
        args = parseArgs(process.argv.slice(2));
    }
    catch (e) {
        process.stderr.write(`normalize-ts-html: ${e.message}\n`);
        return EXIT_ERROR;
    }

    if (args.help) {
        process.stdout.write(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8')
            .split('\n')
            .filter(line => line.startsWith('//'))
            .map(line => line.replace(/^\/\/ ?/, ''))
            .join('\n'));
        return EXIT_OK;
    }

    const inputs = JSON.parse(fs.readFileSync(args.inputsPath, 'utf8'));
    if (!Array.isArray(inputs) || inputs.some(x => typeof x !== 'string')) {
        process.stderr.write('normalize-ts-html: --inputs must be a JSON array of strings\n');
        return EXIT_ERROR;
    }

    // By file:// URL, not by path: on Windows an absolute path like `C:\...`
    // is read by the ESM loader as a URL with scheme `c:`.
    const runner = pathToFileURL(path.join(repoRoot, 'spec', 'runner.ts')).href;
    const { normalizeHtml } = await import(runner);

    const output = JSON.stringify(inputs.map(normalizeHtml));
    if (args.outPath)
        fs.writeFileSync(args.outPath, output);
    else
        process.stdout.write(output);
    return EXIT_OK;
}

process.exit(await main());
