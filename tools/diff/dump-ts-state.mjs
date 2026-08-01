// The TypeScript half of the differential-test harness (RUST-REWRITE-PLAN.md §11.2).
//
//   corpus file ──┬──► node harness → @muyajs/core → state JSON ──┐
//                 │                                                ├──► assert equal
//                 └──► mt-cli --dump-state ────────────────────────┘
//
// This script is the top branch. It loads `MarkdownToState` from the muya
// TypeScript source, runs it over one or more markdown files, and prints the
// resulting state JSON. It has no opinion about whether the Rust side agrees —
// comparison is `cargo xtask diff`'s job (xtask/src/diff.rs), so that the
// pass/fail logic lives in one place and this stays a dumb dumper.
//
// Why this exists at M0, before there is anything to compare against: it
// converts "did I port the lexer correctly?" from a judgement call into a
// boolean, across the whole corpus, on every commit — from the first commit
// of M1 rather than the end of it. Building it after `mt-inline` would be
// building it after the moment it was most needed.
//
// USAGE
//   node --import tsx tools/diff/dump-ts-state.mjs [OPTIONS] <FILE...>
//
//   --bare              Print the bare state array for a single file, the
//                       same shape `mt-cli --dump-state` prints. Useful by
//                       hand; the comparison runner uses the envelope.
//   --spec-options      Parse with every muya extension off, matching
//                       commonmark.spec.ts / gfm.spec.ts. Default is muya's
//                       own DEFAULT_OPTIONS (math and front matter on).
//   --marktext <DIR>    Path to the marktext clone. Defaults to $MARKTEXT_DIR,
//                       then to a sibling `../marktext` of this repo.
//
// EXIT CODES
//   0  success
//   1  error (bad arguments, unreadable file, engine threw)
//   3  the muya engine could not be loaded — the marktext clone is missing or
//      its dependencies are not installed. Distinct from 1 so the comparison
//      runner can report SKIPPED rather than failing CI on a machine that
//      simply does not have the TypeScript engine checked out.

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const EXIT_OK = 0;
const EXIT_ERROR = 1;
const EXIT_ENGINE_UNAVAILABLE = 3;

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

// muya's DEFAULT_OPTIONS for MarkdownToState, from
// packages/muya/src/state/markdownToState.ts. Kept in lockstep with
// `mt_md::Options::MUYA_DEFAULT` — if the two engines are driven with
// different options, a disagreement is uninterpretable.
const MUYA_DEFAULT_OPTIONS = {
    footnote: false,
    math: true,
    isGitlabCompatibilityEnabled: true,
    trimUnnecessaryCodeBlockEmptyLines: false,
    frontMatter: true,
};

// The flags the CommonMark / GFM spec runners use: every muya extension off.
// Mirrors `mt_md::Options::SPEC`.
const SPEC_OPTIONS = {
    footnote: false,
    math: false,
    isGitlabCompatibilityEnabled: false,
    trimUnnecessaryCodeBlockEmptyLines: false,
    frontMatter: false,
};

function parseArgs(argv) {
    const files = [];
    let bare = false;
    let specOptions = false;
    let marktextDir = null;

    for (let i = 0; i < argv.length; i++) {
        const arg = argv[i];
        if (arg === '--bare') {
            bare = true;
        } else if (arg === '--spec-options') {
            specOptions = true;
        } else if (arg === '--marktext') {
            marktextDir = argv[++i];
            if (marktextDir === undefined)
                throw new Error('--marktext requires a directory argument');
        } else if (arg === '--help' || arg === '-h') {
            return { help: true };
        } else if (arg.startsWith('-')) {
            throw new Error(`unrecognised argument: ${arg}`);
        } else {
            files.push(arg);
        }
    }

    if (files.length === 0)
        throw new Error('no input files; try --help');
    if (bare && files.length !== 1)
        throw new Error('--bare takes exactly one file');

    return { files, bare, specOptions, marktextDir, help: false };
}

/**
 * Locate the marktext clone.
 *
 * `--marktext` beats `$MARKTEXT_DIR` beats a sibling `../marktext`. The
 * sibling default is what makes `cargo xtask diff` work with no configuration
 * on a machine that has both repositories checked out next to each other.
 *
 * An *explicitly named* directory is authoritative: if it does not contain the
 * engine, that is an error, not a cue to go looking elsewhere. Falling back
 * would mean a typo in $MARKTEXT_DIR — or a CI checkout that landed in the
 * wrong place — silently compares against a different engine than the one
 * that was asked for, which is worse than failing.
 */
function resolveMarktextDir(explicit) {
    const named = explicit ?? process.env.MARKTEXT_DIR;
    const source = explicit ? '--marktext' : '$MARKTEXT_DIR';
    const candidates = named
        ? [named]
        : [path.resolve(repoRoot, '..', 'marktext')];

    for (const candidate of candidates) {
        const muyaSrc = path.join(candidate, 'packages', 'muya', 'src', 'state', 'markdownToState.ts');
        if (fs.existsSync(muyaSrc))
            return path.resolve(candidate);
    }

    throw new EngineUnavailable(
        named
            ? `${source} points at ${named}, which does not contain `
              + 'packages/muya/src/state/markdownToState.ts.'
            : `could not find the marktext clone at ${candidates[0]}.\n`
              + 'Set MARKTEXT_DIR or pass --marktext <DIR>.',
    );
}

class EngineUnavailable extends Error {}

async function loadEngine(marktextDir) {
    const entry = path.join(marktextDir, 'packages', 'muya', 'src', 'state', 'markdownToState.ts');
    // Import by file:// URL, not by path: on Windows an absolute path like
    // `C:\...` is read by the ESM loader as a URL with scheme `c:`.
    const url = pathToFileURL(entry).href;
    try {
        const mod = await import(url);
        if (typeof mod.MarkdownToState !== 'function')
            throw new Error('markdownToState.ts did not export MarkdownToState');
        return mod.MarkdownToState;
    } catch (cause) {
        // A resolution failure here almost always means muya's own
        // dependencies are not installed. Report it as "engine unavailable"
        // (exit 3) rather than as a harness bug (exit 1).
        throw new EngineUnavailable(
            `failed to load @muyajs/core from ${marktextDir}: ${cause.message}\n`
            + 'Run:  pnpm install --filter @muyajs/core --ignore-scripts\n'
            + `in ${marktextDir}`,
        );
    }
}

/**
 * Read a markdown file the way muya would see it.
 *
 * Two normalisations, both deliberate:
 *   - Strip a UTF-8 BOM. muya's file layer strips it before the parser sees
 *     it, so leaving it in would make every BOM'd corpus file disagree for a
 *     reason that has nothing to do with the parser.
 *   - Normalise CRLF to LF. muya normalises to LF internally (see the note in
 *     test/spec/roundTrip.spec.ts). The Rust side must do the same, and
 *     `mt-fs` owns that in the real application.
 */
function readMarkdown(file) {
    let text = fs.readFileSync(file, 'utf8');
    if (text.charCodeAt(0) === 0xFEFF)
        text = text.slice(1);
    return text.replace(/\r\n?/g, '\n');
}

/**
 * Sort object keys recursively so that the two engines' output can be compared
 * as text and diffed readably. Arrays keep their order — block order is
 * semantic, not incidental.
 *
 * `mt_md::dump_state` must produce the same canonical form. serde_json's
 * default `Map` is a `BTreeMap`, so it sorts keys for free; this is the
 * JavaScript side paying the same cost explicitly.
 */
function canonicalize(value) {
    if (Array.isArray(value))
        return value.map(canonicalize);
    if (value !== null && typeof value === 'object') {
        const out = {};
        for (const key of Object.keys(value).sort())
            out[key] = canonicalize(value[key]);
        return out;
    }
    return value;
}

async function main() {
    let args;
    try {
        args = parseArgs(process.argv.slice(2));
    } catch (e) {
        process.stderr.write(`dump-ts-state: ${e.message}\n`);
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

    let MarkdownToState;
    let marktextDir;
    try {
        marktextDir = resolveMarktextDir(args.marktextDir);
        MarkdownToState = await loadEngine(marktextDir);
    } catch (e) {
        if (e instanceof EngineUnavailable) {
            process.stderr.write(`dump-ts-state: engine unavailable: ${e.message}\n`);
            return EXIT_ENGINE_UNAVAILABLE;
        }
        throw e;
    }

    const options = args.specOptions ? SPEC_OPTIONS : MUYA_DEFAULT_OPTIONS;
    const converter = new MarkdownToState(options);

    const results = [];
    for (const file of args.files) {
        let markdown;
        try {
            markdown = readMarkdown(file);
        } catch (e) {
            process.stderr.write(`dump-ts-state: cannot read ${file}: ${e.message}\n`);
            return EXIT_ERROR;
        }

        let state;
        try {
            state = canonicalize(converter.generate(markdown));
        } catch (e) {
            // A throw from the engine is a real finding, not a harness bug:
            // record it per-file so one bad corpus entry does not blind the
            // rest of the run.
            results.push({ path: file, error: `${e.name}: ${e.message}` });
            continue;
        }
        results.push({ path: file, state });
    }

    if (args.bare) {
        const only = results[0];
        if (only.error) {
            process.stderr.write(`dump-ts-state: ${only.path}: ${only.error}\n`);
            return EXIT_ERROR;
        }
        process.stdout.write(`${JSON.stringify(only.state, null, 2)}\n`);
        return EXIT_OK;
    }

    const envelope = {
        engine: '@muyajs/core',
        marktextDir,
        options,
        files: results,
    };
    process.stdout.write(`${JSON.stringify(envelope, null, 2)}\n`);
    return results.some(r => r.error) ? EXIT_ERROR : EXIT_OK;
}

process.exitCode = await main();
