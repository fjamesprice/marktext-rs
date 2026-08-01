// Materialise the CommonMark spec fixtures into `spec/fixtures/`.
//
// WHY THIS EXISTS — a gap between the plan and the repository
//
// RUST-REWRITE-PLAN.md §11.1 and §14 step 2 say to copy
// `packages/muya/test/spec/` into `spec/` unchanged and run the ratchet
// against it. That has been done, and it is enough for GFM: the GFM 0.29
// fixtures are a committed JSON file (`fixtures/gfm-spec-0.29-gfm.json`,
// 672 examples).
//
// The CommonMark fixtures are not. `commonmark.spec.ts` gets them from the
// `commonmark-spec` npm package at test time:
//
//     import cms from 'commonmark-spec';
//     const examples = cms.tests;              // 652 examples
//
// So a verbatim copy of `spec/` yields a ratchet that can check GFM and not
// CommonMark — which would silently halve the conformance gate, including the
// 87.7 % CommonMark floor the M2 exit gate is stated in.
//
// This script closes that gap by extracting the same 652 examples from the
// npm package into `spec/fixtures/commonmark-spec-<version>.json`, in exactly
// the shape the GFM file already uses: `{ markdown, html, section, number }`.
// The output is committed, so CI needs neither the marktext clone nor a Node
// install to run the conformance ratchet.
//
// Re-run this only to move to a newer CommonMark spec — and treat that as the
// deliberate act it is. Changing the fixture set changes what the ratchet
// measures, so `spec/expected-failures.json` and `spec/conformance.md` must be
// re-baselined in the same commit.
//
// USAGE
//   node tools/vendor-commonmark-spec.mjs [--marktext <DIR>] [--check]
//
//   --check   Verify the committed fixture matches the installed package
//             without writing. Exits 1 on drift.

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// An explicitly named directory is authoritative — see the same function in
// tools/diff/dump-ts-state.mjs for why falling back would be worse than
// failing.
function resolveMarktextDir(explicit) {
    const named = explicit ?? process.env.MARKTEXT_DIR;
    const candidate = named ?? path.resolve(repoRoot, '..', 'marktext');

    if (fs.existsSync(path.join(candidate, 'packages', 'muya', 'package.json')))
        return path.resolve(candidate);

    throw new Error(
        named
            ? `${explicit ? '--marktext' : '$MARKTEXT_DIR'} points at ${named}, `
              + 'which does not contain packages/muya/package.json.'
            : `could not find the marktext clone at ${candidate}.\n`
              + 'Set MARKTEXT_DIR or pass --marktext <DIR>.',
    );
}

function main(argv) {
    let explicit = null;
    let check = false;
    for (let i = 0; i < argv.length; i++) {
        if (argv[i] === '--marktext') explicit = argv[++i];
        else if (argv[i] === '--check') check = true;
        else throw new Error(`unrecognised argument: ${argv[i]}`);
    }

    const marktextDir = resolveMarktextDir(explicit);
    const muyaDir = path.join(marktextDir, 'packages', 'muya');
    const require = createRequire(path.join(muyaDir, 'package.json'));

    const pkg = require('commonmark-spec/package.json');
    const cms = require('commonmark-spec');

    if (!Array.isArray(cms.tests))
        throw new TypeError('commonmark-spec did not export a `tests` array');

    // Reduce to exactly the four keys the GFM fixture file uses, so both
    // suites deserialize into the same Rust struct.
    const examples = cms.tests.map(({ markdown, html, section, number }) => ({
        markdown,
        html,
        section,
        number,
    }));

    const minor = pkg.version.split('.').slice(0, 2).join('.');
    const outPath = path.join(repoRoot, 'spec', 'fixtures', `commonmark-spec-${minor}.json`);
    const json = `${JSON.stringify(examples, null, 2)}\n`;

    if (check) {
        if (!fs.existsSync(outPath)) {
            process.stderr.write(`missing: ${outPath}\nRun: node tools/vendor-commonmark-spec.mjs\n`);
            return 1;
        }
        if (fs.readFileSync(outPath, 'utf8') !== json) {
            process.stderr.write(
                `${path.relative(repoRoot, outPath)} does not match commonmark-spec@${pkg.version}.\n`
                + 'Re-vendoring changes what the conformance ratchet measures — re-baseline\n'
                + 'spec/expected-failures.json and spec/conformance.md in the same commit.\n',
            );
            return 1;
        }
        process.stdout.write(`ok: ${examples.length} CommonMark examples match commonmark-spec@${pkg.version}\n`);
        return 0;
    }

    fs.writeFileSync(outPath, json);
    process.stdout.write(
        `wrote ${path.relative(repoRoot, outPath)}: `
        + `${examples.length} examples from commonmark-spec@${pkg.version}\n`,
    );
    return 0;
}

try {
    process.exitCode = main(process.argv.slice(2));
} catch (e) {
    process.stderr.write(`vendor-commonmark-spec: ${e.message}\n`);
    process.exitCode = 1;
}
