//! **The M2 S4 gate**: the round trip is a fixed point, and where it is the
//! identity is enumerated rather than summarised.
//!
//! ```text
//! parse(serialize(parse(s))) == parse(s)     over every input, no exceptions
//! serialize(parse(s))        == s            over every input but a named set
//! ```
//!
//! `mt-md`'s crate doc has stated the first of those as the crate's contract
//! since M0 and nothing has ever run it. This is that.
//!
//! # Three input sets with three different denominators
//!
//! | Set | Count | Options | Also read by |
//! |---|---:|---|---|
//! | `bench/corpus/` | 11 | `MUYA_DEFAULT` | `cargo xtask diff` (as 11 of its 22) |
//! | `spec/fixtures/marktext-round-trip/` | 11 | `MUYA_DEFAULT` | `cargo xtask diff` (the other 11) |
//! | `spec/fixtures/{commonmark,gfm}-spec-*.json` | 1324 | `SPEC` | `cargo xtask conformance`, `cargo xtask blocks` |
//! | | **1346** | | |
//!
//! That is **not** `cargo xtask blocks`' 1344 and the difference is worth
//! stating: `blocks` drives 20 documents, excluding `1mb.md` and `5mb.md`
//! because it hands every input to a Node process. This gate is in-process, so
//! it runs all 22 — and §6's S4 row asks for *"every corpus file"* in as many
//! words.
//!
//! # Why an integration test and not `cargo xtask roundtrip`
//!
//! Both were possible and the choice is not about taste. **`mt-md` may not do
//! I/O** — §1, machine-checked by `cargo xtask deps`, which scans `src/` for
//! `std::fs` and which fired on S3 for exactly this reason — so the property
//! cannot live beside the code it is about. That leaves an integration test
//! here, an `xtask` runner beside `blocks`, or both.
//!
//! It is this file, and it is here rather than in `xtask` for one reason:
//! **`xtask`'s runners exist to drive the TypeScript engine.** `diff`,
//! `blocks`, `divergences` and `conformance` are all differential or ratcheted
//! against something outside the workspace, which is why they need a Node
//! process, a clone of the reference engine, and a `--require-ts`. The
//! round-trip property is a claim about the Rust engine **alone**: it needs no
//! second engine and no subprocess, and putting it in `xtask` would have meant
//! a runner that shells out to nothing.
//!
//! What CI runs is therefore `cargo test --workspace`, which already runs on
//! three platforms and which this file joins with no wiring. `cargo xtask ci`
//! is unchanged and still does not know about it, which is correct — it is the
//! *differential* chain.
//!
//! **Generated inputs are S7's, not this file's.** §11.3 puts proptest and
//! `cargo-fuzz` on the parse/serialize path in S7 and M2.md's S7 row says so;
//! the property over *fixed corpora* is S4's, and it is the one that can be
//! stated as a ratchet because its inputs do not change between runs.
//!
//! # The exception list is a ratchet, not a report
//!
//! [`IDENTITY_EXCEPTIONS`] names every input for which `serialize(parse(s))`
//! is not `s`, and it behaves exactly like `state_specs.rs`'s `PENDING` and
//! `spec/expected-failures.json` — the third instance of the shape
//! `spec/README.md` asks to be kept:
//!
//! | State | Result |
//! |---|---|
//! | Listed, still differing | passes |
//! | Listed, now **identical** | **fails** — delist it |
//! | Unlisted, identical | passes |
//! | Unlisted, now differing | **fails**, with the diff |
//!
//! Row two is what makes it a floor that can only shrink. A list that can grow
//! silently is what §8's risk table is about, and *"the round trip is mostly
//! the identity"* is the sentence this file exists to make unwriteable.
//!
//! # CRLF, and why this file calls [`mt_md::normalize_source`]
//!
//! The eleven `marktext-round-trip` fixtures are stored `text=auto eol=lf` and
//! check out **CRLF** on Windows. `serialize(parse(s)) == s` against CRLF bytes
//! can never hold — the serializer emits `\n` — so a file layer that did not
//! normalise would put all eleven in the exception list for a reason that has
//! nothing to do with the serializer. That is S3's eleven-of-twenty-two failure
//! in a new costume, and the fix is not to add a fourth spelling of the rule:
//! `mt_md::normalize_source` **is** `mt-cli::read_markdown`, which **is**
//! `dump-ts-state.mjs`'s `readMarkdown`, and S4 collapsed the first two into
//! one function so that this file cannot drift from them.

use std::path::{Path, PathBuf};

use mt_md::{Options, normalize_source, parse, serialize, state::to_state};

/// The repository root, from this crate's manifest directory rather than the
/// working directory, so `cargo test` behaves the same however it is invoked.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// One input: a name for the failure message, its source, and the options both
/// engines would be driven with.
struct Input {
    name: String,
    src: String,
    options: Options,
}

/// `bench/corpus/`, all eleven files, at `MUYA_DEFAULT`.
fn corpus_inputs() -> Vec<Input> {
    let dir = repo_root().join("bench").join("corpus");
    let mut inputs: Vec<Input> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter(|path| path.file_name().is_some_and(|name| name != "README.md"))
        .map(|path| {
            let name = format!(
                "bench/corpus/{}",
                path.file_name().expect("a file").to_string_lossy()
            );
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            Input {
                name,
                src: normalize_source(&text),
                options: Options::MUYA_DEFAULT,
            }
        })
        .collect();
    inputs.sort_by(|a, b| a.name.cmp(&b.name));
    inputs
}

/// `spec/fixtures/marktext-round-trip/`, all eleven, at `MUYA_DEFAULT`.
///
/// These are whole documents chosen to be dense in round-trip hazards, which
/// §7 notes is worth more than their count suggests for exactly this gate.
fn fixture_inputs() -> Vec<Input> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<Input>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let name = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                out.push(Input {
                    name,
                    src: normalize_source(&text),
                    options: Options::MUYA_DEFAULT,
                });
            }
        }
    }

    let root = repo_root().join("spec").join("fixtures");
    let mut inputs = Vec::new();
    walk(&root.join("marktext-round-trip"), &root, &mut inputs);
    inputs.sort_by(|a, b| a.name.cmp(&b.name));
    inputs
}

/// The `markdown` field of every example in a CommonMark/GFM spec fixture, at
/// `SPEC` — the same two files and the same shape `xtask/src/conformance.rs`
/// and `xtask/src/blocks.rs` read.
fn spec_inputs(file: &str) -> Vec<Input> {
    let path = repo_root().join("spec").join("fixtures").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let examples: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    examples
        .as_array()
        .unwrap_or_else(|| panic!("{}: not a JSON array", path.display()))
        .iter()
        .map(|example| {
            let number = example["number"].as_u64().unwrap_or_default();
            let markdown = example["markdown"]
                .as_str()
                .unwrap_or_else(|| panic!("{file}: example {number} has no `markdown`"))
                .to_string();
            Input {
                name: format!("{file}#{number}"),
                src: markdown,
                options: Options::SPEC,
            }
        })
        .collect()
}

/// All 1347, in the order the gate names them.
fn every_input() -> Vec<Input> {
    let mut inputs = corpus_inputs();
    inputs.extend(fixture_inputs());
    inputs.extend(spec_inputs("commonmark-spec-0.31.json"));
    inputs.extend(spec_inputs("gfm-spec-0.29-gfm.json"));
    inputs
}

// ---------------------------------------------------------------------------
// The fixed point
// ---------------------------------------------------------------------------

/// **The gate's first clause, over all three sets.**
///
/// `parse(serialize(parse(s))) == parse(s)`, compared as the state tree —
/// which is the same comparison `cargo xtask diff` and `cargo xtask blocks`
/// make against the TypeScript engine, so a failure here is readable in the
/// same vocabulary as a failure there.
///
/// It is not the same claim as byte identity: a serializer is allowed to
/// canonicalise — a `~~~` fence becomes ```` ``` ````, a table's columns are
/// padded — and this is what says the canonicalisation does not *lose*
/// anything.
///
/// **It holds over all 23 whole documents**, which is §9's exit gate in its own
/// words (*"round-trip is a fixed point on corpus + fixtures"*), and fails on
/// **eleven single spec examples**, each appearing twice because CommonMark and
/// GFM ship the same example. Those are [`FIXED_POINT_EXCEPTIONS`], and the
/// reason they are a list rather than eleven bugs is measured rather than
/// argued: for every one of them the port's output is **byte-identical to
/// muya's**, and muya's own round trip is not a fixed point on them either.
#[test]
fn the_round_trip_is_a_fixed_point_over_every_input() {
    let inputs = every_input();
    assert_eq!(
        inputs.len(),
        1347,
        "12 corpus files + 11 round-trip fixtures + 1324 spec examples. The twelfth corpus          file is `block-kinds.md`, added at M3 S1 for D10's layout goldens; it is a fixed          point on the first run, like the other eleven."
    );

    let mut regressions = Vec::new();
    let mut delistable = Vec::new();
    for input in &inputs {
        let once = parse(&input.src, input.options);
        let markdown = serialize(&once.document, input.options);
        let twice = parse(&markdown, input.options);
        let fixed = to_state(&once.document) == to_state(&twice.document);
        let listed = FIXED_POINT_EXCEPTIONS.contains(&input.name.as_str());
        match (listed, fixed) {
            (true, false) | (false, true) => {}
            (true, true) => delistable.push(input.name.clone()),
            (false, false) => regressions.push(format!(
                "  {}\n    source:     {:?}\n    serialized: {:?}",
                input.name,
                truncate(&input.src),
                truncate(&markdown)
            )),
        }
    }

    assert!(
        regressions.is_empty(),
        "parse(serialize(parse(s))) != parse(s) for {} input(s) that are NOT in \
         FIXED_POINT_EXCEPTIONS.\nThis is the clause §10 marks non-negotiable: a document that \
         does not survive save-and-reopen unchanged has lost something. Fix it, or argue the \
         addition individually in the same commit.\n{}",
        regressions.len(),
        regressions.join("\n")
    );
    assert!(
        delistable.is_empty(),
        "{} input(s) are listed in FIXED_POINT_EXCEPTIONS but are now fixed points.\n\
         Delete them from the list:\n  {}",
        delistable.len(),
        delistable.join("\n  ")
    );

    // Not silent about what it covered: a bounded run that does not say what it
    // bounded reads as full coverage, which is `deps.rs`'s own caution.
    println!(
        "fixed point: {} of {} inputs (11 corpus files, 11 round-trip fixtures, \
         1324 spec examples); {} enumerated exceptions, all of them single spec examples",
        inputs.len() - FIXED_POINT_EXCEPTIONS.len(),
        inputs.len(),
        FIXED_POINT_EXCEPTIONS.len()
    );
}

/// Every name in [`FIXED_POINT_EXCEPTIONS`] names a real input, and none of
/// them is one of the 22 whole documents.
///
/// The second half is the interesting one: §9's exit gate is stated over
/// *corpus + fixtures*, so an entry from either set would be that gate failing
/// rather than a canonicalisation to enumerate, and it must not be possible to
/// add one without noticing.
#[test]
fn no_fixed_point_exception_is_one_of_the_twenty_two_documents() {
    let names: Vec<String> = every_input().into_iter().map(|i| i.name).collect();
    for entry in FIXED_POINT_EXCEPTIONS {
        assert!(
            names.iter().any(|n| n == entry),
            "FIXED_POINT_EXCEPTIONS names an input that does not exist: {entry}"
        );
        assert!(
            !entry.starts_with("bench/") && !entry.starts_with("marktext-round-trip/"),
            "{entry} is one of the 22 documents §9's exit gate is stated over. \
             The round trip losing a whole document is not an exception to enumerate."
        );
    }
}

/// The fixed point is not the identity, and this is the difference stated as a
/// test rather than as a sentence.
///
/// It drives one input from [`IDENTITY_EXCEPTIONS`]'s own class — a `~~~`
/// fence, which the serializer canonicalises to backticks — and asserts both
/// halves: the bytes change and the tree does not. Without it, "the round trip
/// is a fixed point" and "the round trip is the identity" would be two
/// sentences with one test between them.
#[test]
fn a_fixed_point_that_is_not_the_identity_is_still_a_fixed_point() {
    let src = "~~~js\nconst a = 1\n~~~\n";
    let once = parse(src, Options::SPEC);
    let markdown = serialize(&once.document, Options::SPEC);
    assert_ne!(markdown, src, "the fence is canonicalised to backticks");
    assert_eq!(markdown, "```js\nconst a = 1\n```\n");
    let twice = parse(&markdown, Options::SPEC);
    assert_eq!(to_state(&once.document), to_state(&twice.document));
}

// ---------------------------------------------------------------------------
// The identity, and the set on which it does not hold
// ---------------------------------------------------------------------------

/// **The gate's second clause**: `serialize(parse(s)) == s`, byte for byte,
/// with the set on which it does not hold enumerated rather than summarised.
///
/// The four-row decision table is in this file's header. The short form is
/// that this list is a **floor that can only shrink**: a listed input that
/// starts round-tripping byte-for-byte fails the build, exactly as a listed
/// spec case that starts passing does.
#[test]
fn serializing_a_parse_is_the_identity_outside_the_enumerated_set() {
    let inputs = every_input();

    let mut regressions = Vec::new();
    let mut delistable = Vec::new();
    for input in &inputs {
        let markdown = serialize(&parse(&input.src, input.options).document, input.options);
        let identical = markdown == input.src;
        let listed = IDENTITY_EXCEPTIONS.contains(&input.name.as_str());
        match (listed, identical) {
            (true, false) | (false, true) => {}
            (true, true) => delistable.push(input.name.clone()),
            (false, false) => regressions.push(format!(
                "  {}\n    source: {:?}\n    output: {:?}",
                input.name,
                truncate(&input.src),
                truncate(&markdown)
            )),
        }
    }

    assert!(
        regressions.is_empty(),
        "serialize(parse(s)) != s for {} input(s) that are NOT in IDENTITY_EXCEPTIONS.\n\
         Either fix the serializer or add the name with a reason in the same commit — \
         a list that grows silently is what the ratchet exists to prevent.\n{}",
        regressions.len(),
        regressions.join("\n")
    );
    assert!(
        delistable.is_empty(),
        "{} input(s) are listed in IDENTITY_EXCEPTIONS but now round-trip byte-for-byte.\n\
         Delete them from the list — this is the ratchet, and a floor that does not fall \
         when the code improves has stopped being one:\n  {}",
        delistable.len(),
        delistable.join("\n  ")
    );

    println!(
        "identity: {} of {} inputs round-trip byte-for-byte",
        inputs.len() - IDENTITY_EXCEPTIONS.len(),
        inputs.len()
    );
}

/// Every name in [`IDENTITY_EXCEPTIONS`] names a real input.
///
/// A stale entry silently weakens the ratchet by one input — the same failure
/// `spec/README.md` records for `expected-failures.json`, where a listed number
/// that names no example stops guarding anything.
#[test]
fn every_identity_exception_names_a_real_input() {
    let names: Vec<String> = every_input().into_iter().map(|i| i.name).collect();
    let unknown: Vec<&&str> = IDENTITY_EXCEPTIONS
        .iter()
        .filter(|e| !names.iter().any(|n| n == *e))
        .collect();
    assert!(
        unknown.is_empty(),
        "IDENTITY_EXCEPTIONS names inputs that do not exist: {unknown:?}"
    );

    let mut sorted: Vec<&&str> = IDENTITY_EXCEPTIONS.iter().collect();
    sorted.sort_unstable();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "duplicate entry in IDENTITY_EXCEPTIONS"
    );
}

fn truncate(s: &str) -> String {
    const LIMIT: usize = 300;
    if s.len() <= LIMIT {
        return s.to_string();
    }
    let mut end = LIMIT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes)", &s[..end], s.len())
}

/// **The eleven inputs on which `parse(serialize(parse(s))) == parse(s)` does
/// not hold**, each listed twice because CommonMark and GFM ship the same
/// example under different numbers.
///
/// # Why this is a list and not eleven bugs
///
/// Measured, not argued: **the port's serialization of all 1346 inputs is
/// byte-identical to `ExportMarkdown.generate`'s**, these eleven included, and
/// muya's own round trip is not a fixed point on any of them either — asked
/// directly, all eleven come back changed, and five of them are still moving on
/// the third pass. So every entry below is a property of the serializer this
/// milestone is porting. "Fix it" would mean deciding to diverge from the
/// reference engine, which is a register entry (§5 D4) — and register rule 3
/// will not accept one without a failing *differential* case, while no harness
/// in this repository compares serialized markdown. That gap is itself a
/// finding and M2.md §10 carries it.
///
/// # The six mechanisms, each with the example that shows it
///
/// 1. **A grown fence is stored as the grown length.** `_codeFenceLength`
///    lengthens the opening fence past any all-backtick line in the body, so
///    `~~~\naaa\n```\n~~~` emits a four-backtick fence — and reparsing records
///    `meta.fenceLength: 4` where the original had none. The third pass is
///    stable; it is the first that moves. (`commonmark#123`, `#137`.)
/// 2. **A backtick fence cannot carry an info string containing backticks.**
///    The serializer always emits backticks whatever the source used, and
///    CommonMark forbids a backtick inside a backtick fence's info string. So
///    `~~~ aa ``` ~~~` comes back as a fence whose info string closes it, and
///    the block becomes three. This is the only entry here that loses content
///    rather than moving it, and the only one still unstable on the third pass.
///    (`commonmark#146`.)
/// 3. **A lazy continuation is re-emitted as an explicit container line.**
///    `> foo\n    - bar` is one block quote holding one paragraph whose text is
///    `foo\n- bar`; re-emitted with the quote prefix on every line it becomes
///    `> foo\n> - bar`, whose second line is a list. (`commonmark#238`.)
/// 4. **The marker's own padding is normalised, moving the content column.**
///    `-    foo` becomes `- foo`, so a following block indented between two and
///    five columns changes which container it belongs to.
///    (`commonmark#257`, `#276`.)
/// 5. **Varying item indents collapse to the marker width.** `- a\n - b\n  - c`
///    is one list of three items written at three indents; all three re-emit at
///    column 0, and an item that *was* nested stops being.
///    (`commonmark#312`, `#313`.)
/// 6. **A blank line before a non-paragraph block inside a tight item makes the
///    list loose.** `_insertLineBreak` fires before a block quote or a fence
///    wherever it is, including inside a tight list item — and a blank line
///    between an item's blocks is exactly what `loose` is read back from.
///    (`commonmark#300`, `#320`, `#321`.)
///
/// Mechanisms 1 and 6 are the two a later stage could plausibly change without
/// diverging from muya on any real *document*, because both only move a blank
/// line and a stored number. The other four are muya deciding what canonical
/// markdown looks like.
const FIXED_POINT_EXCEPTIONS: &[&str] = &[
    // 1 — a grown fence is stored as the grown length
    "commonmark-spec-0.31.json#123",
    "gfm-spec-0.29-gfm.json#93",
    "commonmark-spec-0.31.json#137",
    "gfm-spec-0.29-gfm.json#107",
    // 2 — a backtick fence cannot carry backticks in its info string
    "commonmark-spec-0.31.json#146",
    "gfm-spec-0.29-gfm.json#116",
    // 3 — a lazy continuation becomes an explicit container line
    "commonmark-spec-0.31.json#238",
    "gfm-spec-0.29-gfm.json#216",
    // 4 — the marker's padding is normalised, moving the content column
    "commonmark-spec-0.31.json#257",
    "gfm-spec-0.29-gfm.json#235",
    "commonmark-spec-0.31.json#276",
    "gfm-spec-0.29-gfm.json#254",
    // 5 — varying item indents collapse to the marker width
    "commonmark-spec-0.31.json#312",
    "gfm-spec-0.29-gfm.json#292",
    "commonmark-spec-0.31.json#313",
    "gfm-spec-0.29-gfm.json#293",
    // 6 — a blank line before a block inside a tight item makes the list loose
    "commonmark-spec-0.31.json#300",
    "gfm-spec-0.29-gfm.json#278",
    "commonmark-spec-0.31.json#320",
    "gfm-spec-0.29-gfm.json#300",
    "commonmark-spec-0.31.json#321",
    "gfm-spec-0.29-gfm.json#301",
];

/// **The set on which `serialize(parse(s)) == s` does not hold** — 348 of
/// 1346, enumerated because §6's S4 row asks for exactly that and because
/// *"the round trip is mostly the identity"* is a sentence that cannot be
/// checked.
///
/// # What these 348 are, measured
///
/// **They are muya's canonicalisations, not the port's losses.** The port's
/// output for all 1346 inputs is byte-identical to `ExportMarkdown.generate`'s
/// — every corpus file, every fixture and every spec example, `5mb.md`,
/// `cjk.md` and `rtl.md` included — so this list measures what the *reference*
/// serializer does to markdown that is not already in its own canonical form.
/// Shrinking it means diverging from the engine this milestone is porting.
///
/// The four mechanisms that account for nearly all of them:
///
/// - **Table columns are padded to a common visual width** (`| a   | b   |`),
///   so any table not already padded that way differs. `20-tables.md` is 6,510
///   bytes in and 11,352 out.
/// - **Blocks are separated by exactly one blank line**, so a document with two
///   or with none is normalised.
/// - **Fences, markers and indentation are canonical**: backticks rather than
///   tildes, one space after a bullet, the stored fence length or three.
/// - **An empty document is one empty paragraph**, so `empty.md` — zero bytes —
///   serializes as `"\n"`. That is `markdownToState`'s
///   `states.length ? states : [{ name: 'paragraph', text: '' }]`, and it is
///   the whole of that file's entry.
///
/// # The two numbers worth reading
///
/// **Nine of the eleven `marktext-round-trip` fixtures round-trip byte for
/// byte**, and the two that do not are `Links.md` and `Lists.md`. Those files
/// were produced by marktext's own serializer, so they are the closest thing
/// this repository has to *markdown a user of this editor would have on disk* —
/// and §10's non-negotiable is about saving such a file, not about
/// canonicalising someone else's.
///
/// **All eleven corpus files differ**, and none is a counter-example to
/// anything: `bench/corpus/` is generated by `cargo xtask corpus` to stress the
/// layout and tokenizer paths and has never been canonical markdown.
///
/// The list is grouped by source set with the four counts in the comments, and
/// [`every_identity_exception_names_a_real_input`] pins the whole against the
/// inputs that exist.
#[rustfmt::skip]
const IDENTITY_EXCEPTIONS: &[&str] = &[
    // bench/corpus/ — 11 of 11. Generated by `cargo xtask corpus` to stress the
    // layout and tokenizer paths, with unpadded tables and deliberately awkward
    // spacing; it has never been canonical markdown.
    "bench/corpus/100-inline-math.md", "bench/corpus/10kb.md",
    "bench/corpus/1mb.md", "bench/corpus/20-tables.md",
    "bench/corpus/250kb.md", "bench/corpus/50-code-fences.md",
    "bench/corpus/5mb.md", "bench/corpus/cjk.md",
    "bench/corpus/emoji.md", "bench/corpus/empty.md",
    "bench/corpus/rtl.md",

    // spec/fixtures/marktext-round-trip/ — 2 of 11. The other nine are the
    // number that matters: these files were produced by marktext's own
    // serializer, so they are the closest thing here to markdown a user of this
    // editor would have on disk.
    "marktext-round-trip/common/Links.md", "marktext-round-trip/common/Lists.md",

    // commonmark-spec-0.31.json — 165 of 652.
    "commonmark-spec-0.31.json#4", "commonmark-spec-0.31.json#6", "commonmark-spec-0.31.json#8", "commonmark-spec-0.31.json#9",
    "commonmark-spec-0.31.json#19", "commonmark-spec-0.31.json#24", "commonmark-spec-0.31.json#34", "commonmark-spec-0.31.json#43",
    "commonmark-spec-0.31.json#47", "commonmark-spec-0.31.json#57", "commonmark-spec-0.31.json#58", "commonmark-spec-0.31.json#59",
    "commonmark-spec-0.31.json#60", "commonmark-spec-0.31.json#62", "commonmark-spec-0.31.json#67", "commonmark-spec-0.31.json#68",
    "commonmark-spec-0.31.json#71", "commonmark-spec-0.31.json#72", "commonmark-spec-0.31.json#73", "commonmark-spec-0.31.json#76",
    "commonmark-spec-0.31.json#77", "commonmark-spec-0.31.json#78", "commonmark-spec-0.31.json#79", "commonmark-spec-0.31.json#82",
    "commonmark-spec-0.31.json#84", "commonmark-spec-0.31.json#85", "commonmark-spec-0.31.json#86", "commonmark-spec-0.31.json#88",
    "commonmark-spec-0.31.json#89", "commonmark-spec-0.31.json#91", "commonmark-spec-0.31.json#92", "commonmark-spec-0.31.json#93",
    "commonmark-spec-0.31.json#94", "commonmark-spec-0.31.json#96", "commonmark-spec-0.31.json#97", "commonmark-spec-0.31.json#98",
    "commonmark-spec-0.31.json#99", "commonmark-spec-0.31.json#100", "commonmark-spec-0.31.json#101", "commonmark-spec-0.31.json#103",
    "commonmark-spec-0.31.json#105", "commonmark-spec-0.31.json#108", "commonmark-spec-0.31.json#109", "commonmark-spec-0.31.json#110",
    "commonmark-spec-0.31.json#111", "commonmark-spec-0.31.json#113", "commonmark-spec-0.31.json#114", "commonmark-spec-0.31.json#115",
    "commonmark-spec-0.31.json#117", "commonmark-spec-0.31.json#120", "commonmark-spec-0.31.json#123", "commonmark-spec-0.31.json#124",
    "commonmark-spec-0.31.json#125", "commonmark-spec-0.31.json#126", "commonmark-spec-0.31.json#127", "commonmark-spec-0.31.json#128",
    "commonmark-spec-0.31.json#130", "commonmark-spec-0.31.json#131", "commonmark-spec-0.31.json#132", "commonmark-spec-0.31.json#133",
    "commonmark-spec-0.31.json#135", "commonmark-spec-0.31.json#136", "commonmark-spec-0.31.json#137", "commonmark-spec-0.31.json#139",
    "commonmark-spec-0.31.json#140", "commonmark-spec-0.31.json#141", "commonmark-spec-0.31.json#143", "commonmark-spec-0.31.json#144",
    "commonmark-spec-0.31.json#146", "commonmark-spec-0.31.json#148", "commonmark-spec-0.31.json#150", "commonmark-spec-0.31.json#169",
    "commonmark-spec-0.31.json#170", "commonmark-spec-0.31.json#172", "commonmark-spec-0.31.json#176", "commonmark-spec-0.31.json#177",
    "commonmark-spec-0.31.json#179", "commonmark-spec-0.31.json#180", "commonmark-spec-0.31.json#182", "commonmark-spec-0.31.json#183",
    "commonmark-spec-0.31.json#184", "commonmark-spec-0.31.json#185", "commonmark-spec-0.31.json#191", "commonmark-spec-0.31.json#204",
    "commonmark-spec-0.31.json#208", "commonmark-spec-0.31.json#210", "commonmark-spec-0.31.json#214", "commonmark-spec-0.31.json#215",
    "commonmark-spec-0.31.json#216", "commonmark-spec-0.31.json#217", "commonmark-spec-0.31.json#221", "commonmark-spec-0.31.json#225",
    "commonmark-spec-0.31.json#227", "commonmark-spec-0.31.json#228", "commonmark-spec-0.31.json#229", "commonmark-spec-0.31.json#230",
    "commonmark-spec-0.31.json#232", "commonmark-spec-0.31.json#233", "commonmark-spec-0.31.json#234", "commonmark-spec-0.31.json#235",
    "commonmark-spec-0.31.json#236", "commonmark-spec-0.31.json#237", "commonmark-spec-0.31.json#238", "commonmark-spec-0.31.json#239",
    "commonmark-spec-0.31.json#240", "commonmark-spec-0.31.json#241", "commonmark-spec-0.31.json#245", "commonmark-spec-0.31.json#246",
    "commonmark-spec-0.31.json#247", "commonmark-spec-0.31.json#249", "commonmark-spec-0.31.json#250", "commonmark-spec-0.31.json#251",
    "commonmark-spec-0.31.json#254", "commonmark-spec-0.31.json#257", "commonmark-spec-0.31.json#258", "commonmark-spec-0.31.json#259",
    "commonmark-spec-0.31.json#260", "commonmark-spec-0.31.json#262", "commonmark-spec-0.31.json#263", "commonmark-spec-0.31.json#264",
    "commonmark-spec-0.31.json#268", "commonmark-spec-0.31.json#271", "commonmark-spec-0.31.json#276", "commonmark-spec-0.31.json#277",
    "commonmark-spec-0.31.json#278", "commonmark-spec-0.31.json#279", "commonmark-spec-0.31.json#280", "commonmark-spec-0.31.json#281",
    "commonmark-spec-0.31.json#282", "commonmark-spec-0.31.json#283", "commonmark-spec-0.31.json#284", "commonmark-spec-0.31.json#286",
    "commonmark-spec-0.31.json#287", "commonmark-spec-0.31.json#288", "commonmark-spec-0.31.json#289", "commonmark-spec-0.31.json#290",
    "commonmark-spec-0.31.json#291", "commonmark-spec-0.31.json#292", "commonmark-spec-0.31.json#293", "commonmark-spec-0.31.json#295",
    "commonmark-spec-0.31.json#297", "commonmark-spec-0.31.json#300", "commonmark-spec-0.31.json#303", "commonmark-spec-0.31.json#305",
    "commonmark-spec-0.31.json#306", "commonmark-spec-0.31.json#307", "commonmark-spec-0.31.json#309", "commonmark-spec-0.31.json#310",
    "commonmark-spec-0.31.json#311", "commonmark-spec-0.31.json#312", "commonmark-spec-0.31.json#313", "commonmark-spec-0.31.json#314",
    "commonmark-spec-0.31.json#315", "commonmark-spec-0.31.json#316", "commonmark-spec-0.31.json#317", "commonmark-spec-0.31.json#318",
    "commonmark-spec-0.31.json#320", "commonmark-spec-0.31.json#321", "commonmark-spec-0.31.json#325", "commonmark-spec-0.31.json#326",
    "commonmark-spec-0.31.json#544", "commonmark-spec-0.31.json#565", "commonmark-spec-0.31.json#570", "commonmark-spec-0.31.json#571",
    "commonmark-spec-0.31.json#647",

    // gfm-spec-0.29-gfm.json — 170 of 672.
    "gfm-spec-0.29-gfm.json#4", "gfm-spec-0.29-gfm.json#6", "gfm-spec-0.29-gfm.json#8", "gfm-spec-0.29-gfm.json#9",
    "gfm-spec-0.29-gfm.json#13", "gfm-spec-0.29-gfm.json#17", "gfm-spec-0.29-gfm.json#27", "gfm-spec-0.29-gfm.json#28",
    "gfm-spec-0.29-gfm.json#29", "gfm-spec-0.29-gfm.json#30", "gfm-spec-0.29-gfm.json#32", "gfm-spec-0.29-gfm.json#37",
    "gfm-spec-0.29-gfm.json#38", "gfm-spec-0.29-gfm.json#41", "gfm-spec-0.29-gfm.json#42", "gfm-spec-0.29-gfm.json#43",
    "gfm-spec-0.29-gfm.json#46", "gfm-spec-0.29-gfm.json#47", "gfm-spec-0.29-gfm.json#48", "gfm-spec-0.29-gfm.json#49",
    "gfm-spec-0.29-gfm.json#52", "gfm-spec-0.29-gfm.json#54", "gfm-spec-0.29-gfm.json#55", "gfm-spec-0.29-gfm.json#56",
    "gfm-spec-0.29-gfm.json#58", "gfm-spec-0.29-gfm.json#59", "gfm-spec-0.29-gfm.json#61", "gfm-spec-0.29-gfm.json#62",
    "gfm-spec-0.29-gfm.json#63", "gfm-spec-0.29-gfm.json#64", "gfm-spec-0.29-gfm.json#66", "gfm-spec-0.29-gfm.json#67",
    "gfm-spec-0.29-gfm.json#68", "gfm-spec-0.29-gfm.json#69", "gfm-spec-0.29-gfm.json#70", "gfm-spec-0.29-gfm.json#71",
    "gfm-spec-0.29-gfm.json#73", "gfm-spec-0.29-gfm.json#75", "gfm-spec-0.29-gfm.json#78", "gfm-spec-0.29-gfm.json#79",
    "gfm-spec-0.29-gfm.json#80", "gfm-spec-0.29-gfm.json#81", "gfm-spec-0.29-gfm.json#83", "gfm-spec-0.29-gfm.json#84",
    "gfm-spec-0.29-gfm.json#85", "gfm-spec-0.29-gfm.json#87", "gfm-spec-0.29-gfm.json#90", "gfm-spec-0.29-gfm.json#93",
    "gfm-spec-0.29-gfm.json#94", "gfm-spec-0.29-gfm.json#95", "gfm-spec-0.29-gfm.json#96", "gfm-spec-0.29-gfm.json#97",
    "gfm-spec-0.29-gfm.json#98", "gfm-spec-0.29-gfm.json#100", "gfm-spec-0.29-gfm.json#101", "gfm-spec-0.29-gfm.json#102",
    "gfm-spec-0.29-gfm.json#103", "gfm-spec-0.29-gfm.json#105", "gfm-spec-0.29-gfm.json#106", "gfm-spec-0.29-gfm.json#107",
    "gfm-spec-0.29-gfm.json#109", "gfm-spec-0.29-gfm.json#110", "gfm-spec-0.29-gfm.json#111", "gfm-spec-0.29-gfm.json#113",
    "gfm-spec-0.29-gfm.json#114", "gfm-spec-0.29-gfm.json#116", "gfm-spec-0.29-gfm.json#118", "gfm-spec-0.29-gfm.json#120",
    "gfm-spec-0.29-gfm.json#139", "gfm-spec-0.29-gfm.json#140", "gfm-spec-0.29-gfm.json#141", "gfm-spec-0.29-gfm.json#145",
    "gfm-spec-0.29-gfm.json#146", "gfm-spec-0.29-gfm.json#148", "gfm-spec-0.29-gfm.json#149", "gfm-spec-0.29-gfm.json#151",
    "gfm-spec-0.29-gfm.json#152", "gfm-spec-0.29-gfm.json#153", "gfm-spec-0.29-gfm.json#154", "gfm-spec-0.29-gfm.json#160",
    "gfm-spec-0.29-gfm.json#173", "gfm-spec-0.29-gfm.json#177", "gfm-spec-0.29-gfm.json#179", "gfm-spec-0.29-gfm.json#183",
    "gfm-spec-0.29-gfm.json#184", "gfm-spec-0.29-gfm.json#185", "gfm-spec-0.29-gfm.json#186", "gfm-spec-0.29-gfm.json#191",
    "gfm-spec-0.29-gfm.json#195", "gfm-spec-0.29-gfm.json#197", "gfm-spec-0.29-gfm.json#199", "gfm-spec-0.29-gfm.json#200",
    "gfm-spec-0.29-gfm.json#201", "gfm-spec-0.29-gfm.json#202", "gfm-spec-0.29-gfm.json#204", "gfm-spec-0.29-gfm.json#206",
    "gfm-spec-0.29-gfm.json#207", "gfm-spec-0.29-gfm.json#208", "gfm-spec-0.29-gfm.json#210", "gfm-spec-0.29-gfm.json#211",
    "gfm-spec-0.29-gfm.json#212", "gfm-spec-0.29-gfm.json#213", "gfm-spec-0.29-gfm.json#214", "gfm-spec-0.29-gfm.json#215",
    "gfm-spec-0.29-gfm.json#216", "gfm-spec-0.29-gfm.json#217", "gfm-spec-0.29-gfm.json#218", "gfm-spec-0.29-gfm.json#219",
    "gfm-spec-0.29-gfm.json#223", "gfm-spec-0.29-gfm.json#224", "gfm-spec-0.29-gfm.json#225", "gfm-spec-0.29-gfm.json#227",
    "gfm-spec-0.29-gfm.json#228", "gfm-spec-0.29-gfm.json#229", "gfm-spec-0.29-gfm.json#232", "gfm-spec-0.29-gfm.json#235",
    "gfm-spec-0.29-gfm.json#236", "gfm-spec-0.29-gfm.json#237", "gfm-spec-0.29-gfm.json#238", "gfm-spec-0.29-gfm.json#240",
    "gfm-spec-0.29-gfm.json#241", "gfm-spec-0.29-gfm.json#242", "gfm-spec-0.29-gfm.json#246", "gfm-spec-0.29-gfm.json#249",
    "gfm-spec-0.29-gfm.json#254", "gfm-spec-0.29-gfm.json#255", "gfm-spec-0.29-gfm.json#256", "gfm-spec-0.29-gfm.json#257",
    "gfm-spec-0.29-gfm.json#258", "gfm-spec-0.29-gfm.json#259", "gfm-spec-0.29-gfm.json#260", "gfm-spec-0.29-gfm.json#261",
    "gfm-spec-0.29-gfm.json#262", "gfm-spec-0.29-gfm.json#264", "gfm-spec-0.29-gfm.json#265", "gfm-spec-0.29-gfm.json#266",
    "gfm-spec-0.29-gfm.json#267", "gfm-spec-0.29-gfm.json#268", "gfm-spec-0.29-gfm.json#269", "gfm-spec-0.29-gfm.json#270",
    "gfm-spec-0.29-gfm.json#271", "gfm-spec-0.29-gfm.json#273", "gfm-spec-0.29-gfm.json#275", "gfm-spec-0.29-gfm.json#278",
    "gfm-spec-0.29-gfm.json#283", "gfm-spec-0.29-gfm.json#285", "gfm-spec-0.29-gfm.json#286", "gfm-spec-0.29-gfm.json#287",
    "gfm-spec-0.29-gfm.json#289", "gfm-spec-0.29-gfm.json#290", "gfm-spec-0.29-gfm.json#291", "gfm-spec-0.29-gfm.json#292",
    "gfm-spec-0.29-gfm.json#293", "gfm-spec-0.29-gfm.json#294", "gfm-spec-0.29-gfm.json#295", "gfm-spec-0.29-gfm.json#296",
    "gfm-spec-0.29-gfm.json#297", "gfm-spec-0.29-gfm.json#298", "gfm-spec-0.29-gfm.json#300", "gfm-spec-0.29-gfm.json#301",
    "gfm-spec-0.29-gfm.json#305", "gfm-spec-0.29-gfm.json#306", "gfm-spec-0.29-gfm.json#315", "gfm-spec-0.29-gfm.json#320",
    "gfm-spec-0.29-gfm.json#330", "gfm-spec-0.29-gfm.json#552", "gfm-spec-0.29-gfm.json#573", "gfm-spec-0.29-gfm.json#578",
    "gfm-spec-0.29-gfm.json#579", "gfm-spec-0.29-gfm.json#667",
];
