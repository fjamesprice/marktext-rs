//! `cargo xtask layout` — the textual layout goldens of docs/M3.md §5 D10.
//!
//! ```text
//! bench/corpus/*.md ──► mt_md::parse ──► mt_layout::layout ──► DisplayList
//!                                                                  │
//!         assets/fonts/faces.toml ──► one Fonts collection ────────┘
//!                                                                  │
//!                          serialize ──► bench/layout-goldens/*.txt
//! ```
//!
//! # What this is
//!
//! D10: *"one line-oriented text file per (corpus file × theme) under
//! `bench/layout-goldens/`, written and compared only by `cargo xtask layout`,
//! exact equality, `--update` the sole writer."* §11.3 asks for pixel goldens
//! with a perceptual threshold; that is S4's instrument on `mt-render`.
//! `mt-layout` is headless and deserves a **textual** golden, because a text
//! diff names the block that moved and a pixel diff does not.
//!
//! The format, the header fields and the review ritual are documented for a
//! reader in `bench/layout-goldens/README.md`. What follows here is why the
//! *code* is shaped the way it is.
//!
//! # Three hard gates run before a single golden is written or compared
//!
//! Each of the three is a **silent** failure that produces a perfectly
//! well-formed golden, which is why none of them is a warning:
//!
//! 1. **The face files are verified against `faces.toml`'s SHA-256s.** A face
//!    that has drifted from its declared digest shapes different advances, and
//!    the golden it produces looks exactly like an ordinary layout change.
//! 2. **An empty registration is an error** — D7's rule, and M3-R10's whole
//!    point. `Collection::register_fonts` returns `Vec<(FamilyId, …)>`, not a
//!    `Result`, and MarkText's own `.woff` files come back empty with no error
//!    and no panic. [`Fonts::register_face`] turns that into
//!    `FontError::Rejected`; this module only has to not swallow it.
//! 3. **[`assert_corpus_fully_covered`]** resolves every distinct codepoint of
//!    every input this run is about to lay out, and hard-fails listing the
//!    offenders if any of them reaches glyph id 0. `assets/fonts/README.md` §6
//!    states the obligation: the committed CJK face is a `pyftsubset` of Noto
//!    Sans CJK **derived from the corpus itself**, so it stops covering the
//!    corpus the moment the corpus changes — and *a tofu advance is a perfectly
//!    valid-looking golden*. It has a width and a position. Without this check
//!    a corpus edit quietly rewrites the goldens full of `.notdef` boxes and
//!    the diff reads as geometry.
//!
//! # One collection, threaded through the whole command
//!
//! `Blob::id()` is a **process-unique counter**, so two `FaceList` +
//! `register_face` loops register the same twelve files under twelve
//! *different* [`FontId`](mt_layout::FontId)s. A tree built against one
//! `Fonts` and emitted against another resolves nothing and every run comes
//! back `FontError::UnresolvedFont`. There is therefore exactly one
//! [`Fonts`] in this command and it is borrowed, never rebuilt.
//!
//! # What it costs to run
//!
//! The alias is `cargo run --package xtask`, i.e. the **dev profile**, so
//! parley shapes `5mb.md` unoptimised — see `bench/layout-goldens/README.md`
//! for the measured numbers and for why the three large inputs get a digest
//! golden rather than a full one. This is the slowest step in `cargo xtask ci`
//! by a wide margin and it is placed accordingly.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mt_layout::{
    BlockKind, Brush, DisplayItem, DisplayList, Fonts, GlyphRun, LayoutOptions, TextRequest,
    TextShaper, Theme, layout_with,
};

// ---------------------------------------------------------------------------
// What is an input, and what is not
// ---------------------------------------------------------------------------

/// Where the goldens live.
const GOLDEN_DIR: &str = "bench/layout-goldens";

/// The one `bench/corpus/*.md` file that is **not** a layout input.
///
/// `bench/corpus/` is a *generated* corpus — `cargo xtask corpus --check`
/// verifies that every committed file still matches the generator in
/// `xtask/src/corpus.rs` — and `README.md` is documentation *about* that
/// corpus, hand-written and outside the generator. Laying it out would couple
/// prose edits to golden churn: fixing a typo in the provenance table would
/// rewrite two goldens and put a layout diff in a documentation commit, which
/// erodes exactly the review ritual M3-R6 depends on. **This is a decision,
/// not an oversight.**
const NOT_AN_INPUT: &str = "README.md";

/// The inputs whose golden is a digest rather than a full serialization.
///
/// Measured rather than guessed — `cargo xtask layout --measure` prints the
/// would-be full size of every input and `bench/layout-goldens/README.md`
/// records the numbers this list was cut from. The rule is that a golden a
/// human cannot open is M3-R6's failure mode with extra steps: these three are
/// prose-generated files of 250 KB, 1 MB and 5 MB whose full serializations run
/// to megabytes, and nobody reviews a 40 MB diff.
///
/// The tool needs no other per-file knowledge. Everything else — the header,
/// the census, the float precision, the comparison — is identical for a digest
/// golden and a full one; only the block section is replaced by a hash of it.
const DIGEST_INPUTS: [&str; 3] = ["250kb.md", "1mb.md", "5mb.md"];

/// Format version, in the first line of every golden.
///
/// Bumped when the serialization changes shape. It is in the file rather than
/// only in this source so that a golden generated by an older tool is
/// identifiable from the artifact.
///
/// **v2** added the `parse` header field. v1 recorded `LayoutOptions` and left
/// the parse options implicit, which was safe only while every input was parsed
/// identically — see [`PARSE_OVERRIDES`].
///
/// **v3** added `fill=` to the glyph-run line. Until S2 every leaf was one
/// style, so every glyph run in a block carried the block's own colour and the
/// field said nothing; with inline styling it carries a link's blue, a
/// `<strong>`'s override, inline code's `--editor-color` and an unresolved
/// shortcode's `--delete-color`, and a serialization that omits it is not
/// *"a stable serialization of the display list"* (D10) but of most of one. A
/// `rect` has printed its `fill=` and a `line` its `stroke=` since v1; this is
/// the third painted item catching up.
const FORMAT_VERSION: &str = "layout-golden v3";

// ---------------------------------------------------------------------------
// Parse options — per input, and visible in the header
// ---------------------------------------------------------------------------

/// A named deviation from `Options::MUYA_DEFAULT`, applied to one input.
///
/// # Why this exists at all
///
/// `Block::Footnote` **cannot occur in a default document**:
/// `Options::MUYA_DEFAULT` sets `footnote: false` because
/// `packages/muya/src/config` does, so `[^id]: …` parses as a paragraph — which
/// is exactly what `bench/corpus/README.md` already says happens to reference
/// definitions. So the choice was between never exercising the `footnote`
/// block kind in a golden, and turning the extension on.
///
/// Turning it on **corpus-wide** was rejected twice over: it would rewrite every
/// existing golden, and it would make the whole golden set describe a
/// configuration MarkText does not ship. Turning it on for one input keeps both
/// properties — twenty-two goldens describe the default, and one describes a
/// documented deviation.
///
/// The deviation is in the artifact rather than only here, on D10 property 3's
/// argument: a golden that does not say how it was parsed can be read as the
/// default configuration by someone who has never opened this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseOverride {
    /// `Options::footnote = true`.
    Footnote,
}

impl ParseOverride {
    fn apply(self, options: &mut mt_md::Options) {
        match self {
            ParseOverride::Footnote => options.footnote = true,
        }
    }

    /// The suffix this override contributes to the header's `parse` label.
    fn label(self) -> &'static str {
        match self {
            ParseOverride::Footnote => "+footnote",
        }
    }
}

/// Which inputs are parsed with something other than `Options::MUYA_DEFAULT`.
///
/// Exactly one today. Everything else in `bench/corpus/` is read at muya's own
/// defaults, like every other harness in `xtask` reads it.
const PARSE_OVERRIDES: [(&str, ParseOverride); 1] = [("block-kinds.md", ParseOverride::Footnote)];

/// The parse options for one input, and the label the header records.
///
/// `pub(crate)` since M3 S3, for the same reason [`inputs`] is: `crate::highlight`
/// parses the same corpus and must parse it the same way.
pub(crate) fn parse_options(input: &str) -> (mt_md::Options, String) {
    let mut options = mt_md::Options::MUYA_DEFAULT;
    let mut label = String::from("muya-default");
    for (name, over) in PARSE_OVERRIDES {
        if name == input {
            over.apply(&mut options);
            label.push_str(over.label());
        }
    }
    (options, label)
}

/// The layout options for one input.
///
/// `mt-layout` may not depend on `mt-md`, so the two `mt_md::Options` flags
/// that change what a leaf's text *tokenizes to* — footnotes and sup/sub —
/// have to be handed across explicitly. This is the one place in the harness
/// that knows both sides, and it is the same override that decided the parse.
///
/// The header does not grow a field for them: the `parse` line already records
/// `footnote=` and `super-sub=`, and recording the same bit twice invites the
/// two copies to disagree.
///
/// # The labels come from the same parse, and that is the whole trick
///
/// `TokenizerOptions::labels` is typed `mt_inline::Labels`, which `mt_md::labels`
/// merely populates — so handing `Parsed::labels` down needs no new dependency
/// edge and no `mt-md` reference inside `mt-layout`. Without it,
/// `[the plan][plan]` in `10kb.md` lays out as four literal brackets, and a
/// golden written over that reading would freeze it.
///
/// # The image table is deliberately empty
///
/// D12 has the shell resolve image sizes, and this harness resolves none: it
/// opens no PNG, so every inline image takes the reference's own no-bitmap
/// geometry. That is the point — the goldens then exercise `.mu-image-fail`
/// deterministically, with no file on disk and no machine dependence.
fn layout_options(parse: &mt_md::Options, labels: mt_inline::Labels) -> LayoutOptions {
    LayoutOptions {
        inline_syntax: mt_layout::InlineSyntax {
            footnote: parse.footnote,
            super_sub_script: parse.super_sub_script,
            labels,
        },
        ..LayoutOptions::default()
    }
}

/// The full parse-option set, spelled out for the header.
///
/// Every field of `mt_md::Options`, not just the overridden one: the whole
/// struct determines the tree, so a change to any of muya's defaults should
/// appear as a one-line diff in every golden rather than silently shifting the
/// geometry underneath them. Same argument as the `parley` and `faces` lines.
fn parse_fields(options: &mt_md::Options) -> String {
    format!(
        "footnote={} math={} super-sub={} gitlab={} front-matter={} trim-code-blanks={} \
         list-indent={:?}",
        options.footnote,
        options.math,
        options.super_sub_script,
        options.gitlab_compatibility,
        options.front_matter,
        options.trim_unnecessary_code_block_empty_lines,
        options.list_indentation,
    )
}

/// Every corpus file that is a layout input, sorted by name.
///
/// `pub(crate)` since M3 S3: `crate::highlight` collects the corpus's fences and
/// must read the same file set. Two harnesses with two definitions of which
/// files count is a difference that would eventually be mistaken for a finding.
pub(crate) fn inputs(repo_root: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = repo_root.join("bench").join("corpus");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some(NOT_AN_INPUT))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no corpus inputs under {}", dir.display()));
    }
    Ok(paths)
}

/// The two shipped themes, in the order goldens are generated.
fn themes() -> [Theme; 2] {
    [Theme::muya_default(), Theme::dark()]
}

/// `bench/layout-goldens/<input stem>.<theme>.txt`.
fn golden_name(input: &str, theme: &str) -> String {
    let stem = input.strip_suffix(".md").unwrap_or(input);
    format!("{stem}.{theme}.txt")
}

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

/// Parsed command line.
struct Opts {
    /// Rewrite the goldens from the measured output. The **sole** writer, and
    /// never implied by anything else.
    update: bool,
    /// Only inputs whose file name contains this.
    only: Option<String>,
    /// Print every differing line rather than the first twenty, and print the
    /// per-block-kind census for each input as it is generated.
    verbose: bool,
    /// Dump the serialization with one line per glyph to stdout. Compares
    /// nothing and writes nothing — a debugging view, not a golden.
    glyphs: bool,
    /// Print the would-be **full** serialization size of every selected input
    /// at both themes. Compares nothing and writes nothing; this is the
    /// measurement `DIGEST_INPUTS` was cut from.
    measure: bool,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut opts = Opts {
            update: false,
            only: None,
            verbose: false,
            glyphs: false,
            measure: false,
        };
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "--update" => opts.update = true,
                "--verbose" => opts.verbose = true,
                "--glyphs" => opts.glyphs = true,
                "--measure" => opts.measure = true,
                "--only" => {
                    opts.only = Some(
                        rest.next()
                            .ok_or_else(|| "--only needs a substring".to_string())?
                            .clone(),
                    );
                }
                other => return Err(format!("unrecognised argument: {other}")),
            }
        }
        // Three mutually exclusive modes, and the exclusion is load-bearing
        // rather than tidiness: `--glyphs` produces a *different serialization*
        // and `--measure` produces none at all, so either one silently paired
        // with `--update` would overwrite every golden with something that is
        // not the golden format.
        if opts.update && opts.glyphs {
            return Err("--glyphs is a debugging dump and can never write a golden".into());
        }
        if opts.update && opts.measure {
            return Err("--measure writes nothing; drop one of the two".into());
        }
        if opts.glyphs && opts.only.is_none() {
            return Err(
                "--glyphs needs --only <SUBSTR>: a per-glyph dump of the whole corpus is \
                 gigabytes. Name the input you are debugging."
                    .into(),
            );
        }
        Ok(opts)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// `cargo xtask layout [--update] [--only SUBSTR] [--verbose] [--glyphs] [--measure]`.
pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let opts = Opts::parse(args)?;

    let all = inputs(repo_root)?;
    let selected: Vec<PathBuf> = all
        .into_iter()
        .filter(|p| match &opts.only {
            Some(substr) => file_name(p).contains(substr.as_str()),
            None => true,
        })
        .collect();
    if selected.is_empty() {
        return Err(format!(
            "--only {:?} matched no corpus input",
            opts.only.unwrap_or_default()
        ));
    }

    // Gate 1 and 2: the digests, and the empty-registration rule. One
    // collection for the whole command — see the module doc on `Blob::id()`.
    let provenance = Provenance::read(repo_root)?;
    let mut fonts = build_collection(repo_root, &provenance)?;
    println!(
        "fonts   {} faces registered from {} ({} B), sha256-verified",
        fonts.len(),
        provenance.face_list.faces.len(),
        provenance.face_list.total_declared_bytes()
    );

    // Read the inputs once. CRLF is normalised for the same reason
    // `corpus.rs --check` normalises it: a checkout that rewrote line endings
    // is not drift, and a golden that depended on the checkout would not be a
    // golden.
    let mut documents: Vec<(String, String)> = Vec::new();
    for path in &selected {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
            .replace("\r\n", "\n");
        documents.push((file_name(path), text));
    }

    // Gate 3, and it runs before anything is written or compared.
    let mut shaper = TextShaper::new();
    assert_corpus_fully_covered(&mut fonts, &mut shaper, &documents)?;

    if opts.measure {
        return measure(&mut fonts, &mut shaper, &provenance, &documents);
    }

    let themes = themes();
    let mut written = 0usize;
    let mut drifted: Vec<String> = Vec::new();
    // Keyed by (input, theme) so the both-themes-differ check below reads the
    // same bytes the comparison did rather than re-deriving them.
    let mut produced: BTreeMap<(String, String), String> = BTreeMap::new();

    let dir = repo_root.join(GOLDEN_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    for (name, text) in &documents {
        let (parse_opts, parse_label) = parse_options(name);
        let parsed = mt_md::parse(text, parse_opts);
        let options = layout_options(&parse_opts, parsed.labels.clone());
        for theme in &themes {
            let list = layout_with(
                &parsed.document,
                theme,
                f32::INFINITY,
                &mut fonts,
                &mut shaper,
                &options,
            )
            .map_err(|e| format!("{name} at {}: {e}", theme.name))?;

            // A cheap invariant the golden itself deliberately does not carry:
            // `BlockDisplay::node` is an arena slot, which is an artifact of
            // `mt-md`'s allocation order rather than geometry, so putting it in
            // a golden would make an unrelated parser change rewrite every
            // file. What must hold is that the mapping is injective, and that
            // is checked here instead.
            let distinct: BTreeSet<_> = list.blocks.iter().map(|b| b.node).collect();
            if distinct.len() != list.blocks.len() {
                return Err(format!(
                    "{name} at {}: {} blocks share {} distinct NodeIds",
                    theme.name,
                    list.blocks.len(),
                    distinct.len()
                ));
            }

            if opts.glyphs {
                print!(
                    "{}",
                    serialize(
                        &Golden {
                            provenance: &provenance,
                            theme,
                            input: name,
                            input_bytes: text.len(),
                            parse_options: parse_opts,
                            parse_label: &parse_label,
                        },
                        &list,
                        Detail::PerGlyph,
                        &fonts,
                    )
                );
                continue;
            }

            let digest = DIGEST_INPUTS.contains(&name.as_str());
            let detail = if digest {
                Detail::Digest
            } else {
                Detail::PerItem
            };
            let body = serialize(
                &Golden {
                    provenance: &provenance,
                    theme,
                    input: name,
                    input_bytes: text.len(),
                    parse_options: parse_opts,
                    parse_label: &parse_label,
                },
                &list,
                detail,
                &fonts,
            );

            if opts.verbose {
                println!(
                    "        {name} {} — {} blocks, height {}",
                    theme.name,
                    list.blocks.len(),
                    f2(list.height)
                );
                for (kind, count) in census(&list) {
                    if count > 0 {
                        println!("          {:<16} {count}", kind.name());
                    }
                }
            }

            let file = golden_name(name, &theme.name);
            let path = dir.join(&file);
            let existing = match std::fs::read_to_string(&path) {
                Ok(s) => Some(s.replace("\r\n", "\n")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
            };

            if opts.update {
                if existing.as_deref() == Some(body.as_str()) {
                    println!("ok      {file} ({} bytes, unchanged)", body.len());
                } else {
                    std::fs::write(&path, &body)
                        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
                    println!("wrote   {file} ({} bytes)", body.len());
                    written += 1;
                }
            } else {
                match existing {
                    None => {
                        println!("MISSING {file}");
                        drifted.push(file.clone());
                    }
                    Some(old) if old == body => {
                        println!("ok      {file} ({} bytes)", body.len());
                    }
                    Some(old) => {
                        println!("DRIFT   {file}");
                        report_drift(&old, &body, opts.verbose);
                        drifted.push(file.clone());
                    }
                }
            }
            produced.insert((name.clone(), theme.name.clone()), body);
        }
    }

    if opts.glyphs {
        return Ok(0);
    }

    // M3-R6's first-review clause is *"the first golden for each block kind is
    // read by eye once"*. Which kinds those are is a property of the corpus,
    // not of the layout, and it is printed on every run rather than recorded
    // once — a reviewer who cannot see the gap will not go looking for it. It
    // is a notice and not a failure because a corpus with no front matter in it
    // is not a defect in this tool. `UNEXERCISED_KINDS` is where it ratchets.
    let uncovered = kinds_no_golden_exercises(&produced);
    if opts.only.is_none() {
        if uncovered.is_empty() {
            println!(
                "\nkinds   all {} block kinds are exercised by at least one golden",
                BlockKind::ALL.len()
            );
        } else {
            println!(
                "\nnote    {} of {} block kinds appear in no golden: {}",
                uncovered.len(),
                BlockKind::ALL.len(),
                uncovered.join(", ")
            );
            println!(
                "        bench/corpus/ contains none of them, so M3-R6's by-eye review has \
                 nothing to read for these."
            );
        }
    }

    // S1's third gate clause, asserted rather than assumed: both theme widths
    // must produce *different* goldens from the same document, and differ in
    // the geometry rather than only in the header.
    let mismatches = themes_must_differ(&produced);
    if !mismatches.is_empty() {
        for m in &mismatches {
            println!("SAME    {m}");
        }
        return Err(format!(
            "{} input(s) produced identical geometry at 800px and 750px. The two themes \
             differ on `content_width_px` alone (700 vs 650 after padding), so a document \
             whose layout does not move is either empty of content or laid out at a width \
             that is not the theme's.",
            mismatches.len()
        ));
    }

    if opts.update {
        println!(
            "\n{written} golden(s) rewritten, {} checked.",
            produced.len()
        );
        if written > 0 {
            println!(
                "A golden update is a REVIEWABLE EVENT on the same footing as a parley pin \
                 move.\nRead the diff, and commit it together with whatever changed the \
                 output. See {GOLDEN_DIR}/README.md."
            );
        }
        return Ok(0);
    }

    if drifted.is_empty() {
        println!("\n{} golden(s) match exactly.", produced.len());
        return Ok(0);
    }
    println!(
        "\n{} of {} goldens do not match {GOLDEN_DIR}/.",
        drifted.len(),
        produced.len()
    );
    println!("If the change is intended, run `cargo xtask layout --update` and review the diff.");
    Ok(1)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into()
}

// ---------------------------------------------------------------------------
// Provenance — what the header line pins
// ---------------------------------------------------------------------------

/// Everything the header records about *how* a golden was produced.
///
/// D10 property 3 asks for theme, width and the pinned parley revision, so that
/// a pin move is a one-line diff at the top of every golden rather than an
/// unexplained shift in every number below it.
///
/// **The `faces.toml` content hash is an addition to D10** and it is here for
/// the identical argument: the font set determines the geometry exactly as much
/// as the shaper does — reorder the fallback chain and every fallback run in
/// the corpus moves — so a face swap must be as visible in a diff as a pin
/// move. D10's own text says the order of `[[face]]` entries "is a
/// golden-changing event on the same footing as moving the parley pin"; without
/// this field that sentence has no artifact behind it.
struct Provenance {
    /// The exact commit `Cargo.lock` resolved parley to.
    parley_rev: String,
    /// SHA-256 of `assets/fonts/faces.toml`, CRLF-normalised.
    faces_sha256: String,
    /// `[meta] revision` from the face list.
    faces_revision: u32,
    /// The parsed face list, so the collection builder does not re-read it.
    face_list: mt_layout::FaceList,
}

impl Provenance {
    fn read(repo_root: &Path) -> Result<Provenance, String> {
        let faces_path = repo_root.join("assets").join("fonts").join("faces.toml");
        let faces_src = std::fs::read_to_string(&faces_path)
            .map_err(|e| format!("cannot read {}: {e}", faces_path.display()))?
            .replace("\r\n", "\n");
        let face_list = mt_layout::FaceList::from_toml_str(&faces_src)
            .map_err(|e| format!("{}: {e}", faces_path.display()))?;

        let lock = repo_root.join("Cargo.lock");
        let parley_rev = parley_rev_from_lock(
            &std::fs::read_to_string(&lock)
                .map_err(|e| format!("cannot read {}: {e}", lock.display()))?,
        )
        .ok_or_else(|| {
            format!(
                "no `parley` git source in {} — D2's pin is what every golden header cites",
                lock.display()
            )
        })?;

        // The face list was *measured* against a parley revision (p2's probe
        // re-runs on every `cargo run -p s1-faces`), so a lockfile that has
        // moved out from under it means the coverage numbers in
        // `assets/fonts/README.md` describe a shaper this run is not using.
        // Cheap to check, and the alternative is discovering it in a golden.
        if face_list.meta.parley_rev != parley_rev {
            return Err(format!(
                "Cargo.lock pins parley {parley_rev} but assets/fonts/faces.toml was measured \
                 against {}. Move both together, or neither.",
                face_list.meta.parley_rev
            ));
        }

        Ok(Provenance {
            faces_sha256: sha256_hex(faces_src.as_bytes()),
            faces_revision: face_list.meta.revision,
            parley_rev,
            face_list,
        })
    }
}

/// Pull the resolved parley commit out of `Cargo.lock`.
///
/// The lockfile rather than the manifest, because the lockfile records what was
/// actually built: a `rev = ` in `Cargo.toml` is a request and `#<sha>` in the
/// lock's `source` is the answer. A crude line scan for the same reason
/// `deps.rs:14-17` gives — the alternative is a TOML dependency in the root
/// workspace for eight lines of parsing.
fn parley_rev_from_lock(lock: &str) -> Option<String> {
    let mut in_parley = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_parley = false;
        } else if line == "name = \"parley\"" {
            in_parley = true;
        } else if in_parley && line.starts_with("source = ") {
            let (_, fragment) = line.rsplit_once('#')?;
            return Some(fragment.trim_end_matches('"').to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The collection — D7's shell, and the two hard gates it owes
// ---------------------------------------------------------------------------

/// Read every face `faces.toml` names, verify it, and register it.
///
/// D7: *"`mt-layout` never enumerates or opens a font."* This function is the
/// shell that does the I/O, and it is the whole of it — the loop below is the
/// one `crates/mt-layout/tests/fonts.rs` documents, plus the two gates.
fn build_collection(repo_root: &Path, provenance: &Provenance) -> Result<Fonts, String> {
    let dir = repo_root.join("assets").join("fonts");
    let mut fonts = Fonts::new();
    let mut declared_total = 0u64;

    for face in &provenance.face_list.faces {
        let path = dir.join(&face.file);
        let bytes =
            std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;

        // Gate 1. Length first, because it is the cheap half of the same
        // question and it makes a truncated file say so plainly.
        if bytes.len() as u64 != face.bytes {
            return Err(format!(
                "{}: {} bytes on disk, faces.toml declares {}",
                face.file,
                bytes.len(),
                face.bytes
            ));
        }
        let digest = sha256_hex(&bytes);
        if digest != face.sha256 {
            return Err(format!(
                "{}: sha256 {digest}\n  faces.toml declares {}\n  \
                 A face that has drifted from its digest shapes different advances, and the \
                 golden it produces is indistinguishable from an ordinary layout change.",
                face.file, face.sha256
            ));
        }
        declared_total += face.bytes;

        // Gate 2 is inside `register_face`: an empty `register_fonts` comes
        // back as `FontError::Rejected` naming the file, rather than as a
        // silently absent family. D7's rule; M3-R10's reason.
        fonts
            .register_face(face, bytes)
            .map_err(|e| format!("{}: {e}", face.file))?;
    }

    if declared_total != provenance.face_list.meta.total_font_bytes {
        return Err(format!(
            "faces.toml [meta] total_font_bytes {} != the sum of its entries {declared_total}",
            provenance.face_list.meta.total_font_bytes
        ));
    }
    if fonts.is_empty() {
        return Err("no faces registered — every golden would be tofu".into());
    }

    // The three wiring calls p2 measured as necessary: registering the faces is
    // not enough. Without `append_generic_families` both theme stacks end in a
    // CSS generic that resolves to nothing (`system_fonts: false`), and emoji
    // never resolve at all — parley reaches for `GenericFamily::Emoji`
    // directly rather than through script fallback.
    fonts
        .wire(&provenance.face_list)
        .map_err(|e| format!("wiring the fallback chain: {e}"))?;
    Ok(fonts)
}

// ---------------------------------------------------------------------------
// Gate 3 — assert_corpus_fully_covered
// ---------------------------------------------------------------------------

/// Hard-fail if any codepoint of any input resolves to `.notdef`.
///
/// The contract is written down in `assets/fonts/README.md` §6 and the reason
/// it fails rather than warns is written down there too: **a tofu advance is a
/// perfectly valid-looking golden**, so the check has to be upstream of the
/// artifact rather than a note beside it.
///
/// Three outcomes per codepoint, and the third is the one a `cmap` check gets
/// wrong: a default-ignorable codepoint (ZWJ U+200D, VS16 U+FE0F) is in no
/// font's `cmap` and produces **no glyph at all**, which is correct rather than
/// a coverage failure.
///
/// Both theme stacks are probed. The body stack is what prose resolves through;
/// the code stack is a different literal list (`inlineSyntax.css:66`) and is
/// what every fenced block, math block, front matter block, HTML block and
/// diagram resolves through, so checking only the first would leave the five
/// D11 kinds unmeasured.
fn assert_corpus_fully_covered(
    fonts: &mut Fonts,
    shaper: &mut TextShaper,
    documents: &[(String, String)],
) -> Result<(), String> {
    // The union first, so a codepoint shared by four files is shaped once, and
    // the files it came from are still nameable in the failure message.
    let mut union: BTreeMap<char, Vec<&str>> = BTreeMap::new();
    for (name, text) in documents {
        let mut seen: BTreeSet<char> = BTreeSet::new();
        for ch in text.chars() {
            // Control characters are structure, not glyphs: `\n` and `\t` never
            // reach a face and a run that contained one would say nothing.
            if ch.is_control() || !seen.insert(ch) {
                continue;
            }
            union.entry(ch).or_default().push(name.as_str());
        }
    }

    let theme = Theme::muya_default();
    let stacks: [(&str, &[String]); 2] = [("body", &theme.fonts.body), ("code", &theme.fonts.code)];

    let mut offenders: Vec<String> = Vec::new();
    for (ch, files) in &union {
        let text = ch.to_string();
        for (label, families) in stacks {
            let request = TextRequest::new(&text, families, theme.metrics.font_size_px, 1.0);
            let shaped = shaper.shape(fonts, &request);
            let mut items = Vec::new();
            shaped
                .emit(fonts, 0.0, 0.0, &mut items)
                .map_err(|e| format!("probing U+{:04X}: {e}", *ch as u32))?;
            let mut glyphs = 0usize;
            let mut notdef = 0usize;
            for item in &items {
                if let DisplayItem::Glyphs(run) = item {
                    glyphs += run.glyphs.len();
                    notdef += run.glyphs.iter().filter(|g| g.id == 0).count();
                }
            }
            // `glyphs == 0` is the default-ignorable case and is fine.
            if glyphs > 0 && notdef > 0 {
                offenders.push(format!(
                    "  U+{:04X} {:?}  {label} stack  in {}",
                    *ch as u32,
                    ch,
                    files.join(", ")
                ));
            }
        }
    }

    if offenders.is_empty() {
        println!(
            "cover   {} distinct codepoints across {} input(s), 0 tofu",
            union.len(),
            documents.len()
        );
        return Ok(());
    }
    Err(format!(
        "{} codepoint(s) resolve to glyph id 0 (.notdef) against the committed face set:\n{}\n\n\
         A tofu advance makes a perfectly valid-looking golden — it has a width and a \
         position — which is why this fails rather than warns.\n\
         `assets/fonts/NotoSansCJKsc-Regular-corpus-subset.otf` is a pyftsubset of Noto Sans \
         CJK **derived from bench/corpus/ itself**, so it stops covering the corpus the moment \
         the corpus changes. Regenerate it with the commands in assets/fonts/README.md §6 and \
         update `bytes`, `sha256` and `[meta] total_font_bytes` in faces.toml.",
        offenders.len(),
        offenders.join("\n")
    ))
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/// How much of the display list a serialization carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// One line per block and one per display item. **The golden format.**
    ///
    /// A per-glyph dump is roughly twenty times larger and makes the diff name
    /// a glyph rather than a block, which defeats D10 property 2 — a golden's
    /// whole job is to make `git diff` name the block that moved. What a
    /// per-item line loses is caught instead by a short hash of the run's
    /// `(id, x, y)` sequence, so a shaping change that preserves the glyph
    /// count and the total advance still fails.
    PerItem,
    /// [`PerItem`](Self::PerItem) plus one line per glyph. `--glyphs`, for
    /// debugging. Never written to a file.
    PerGlyph,
    /// Header and census only, with the full [`PerItem`] serialization reduced
    /// to a SHA-256. For the three inputs whose full golden nobody would read.
    Digest,
}

/// Everything about one golden except the display list itself.
///
/// A struct rather than six parameters because [`serialize`] would otherwise
/// take eight, which is both a clippy lint and a real hazard: `input` and
/// `parse_label` are adjacent, same-typed and swappable, and a swap would
/// produce a plausible-looking golden rather than a compile error.
struct Golden<'a> {
    provenance: &'a Provenance,
    theme: &'a Theme,
    input: &'a str,
    input_bytes: usize,
    parse_options: mt_md::Options,
    parse_label: &'a str,
}

/// The whole artifact, as a string.
///
/// Layout of the file — the same shape for every input and both forms:
///
/// ```text
/// <provenance header, one field per line>
/// <blank>
/// list …            geometry summary
/// [full-sha256 …]   digest form only
/// count <kind> <n>  ×19, in BlockKind::ALL order
/// <blank>
/// block … / items   full form only
/// ```
///
/// The blank line after the header is the seam the both-themes-differ check
/// cuts at: everything above it is provenance, everything below it is geometry.
fn serialize(golden: &Golden<'_>, list: &DisplayList, detail: Detail, fonts: &Fonts) -> String {
    let Golden {
        provenance,
        theme,
        input,
        input_bytes,
        parse_options,
        parse_label,
    } = golden;
    let mut out = String::with_capacity(4096);
    let digest = detail == Detail::Digest;

    // ---- provenance -------------------------------------------------------
    let _ = writeln!(out, "{FORMAT_VERSION}");
    let _ = writeln!(out, "theme          {}", theme.name);
    let _ = writeln!(out, "theme-width    {}", f2(theme.metrics.content_width_px));
    let _ = writeln!(out, "content-width  {}", f2(theme.content_width_px()));
    let _ = writeln!(out, "parley         {}", provenance.parley_rev);
    let _ = writeln!(
        out,
        "faces          {} rev {}",
        provenance.faces_sha256, provenance.faces_revision
    );
    // Both muya defaults. A golden generated with either flag on is a different
    // artifact, so the flags are in the file rather than assumed.
    let options = LayoutOptions::default();
    let _ = writeln!(
        out,
        "options        wrap-code-blocks={} line-numbers={}",
        options.wrap_code_blocks, options.code_block_line_numbers
    );
    // `mt_md::Options`, spelled out, with the preset label in front so a
    // deviation from muya's default is legible at a glance and greppable across
    // the directory. `block-kinds.md` is the one input that is not
    // `muya-default`; see `PARSE_OVERRIDES`.
    let _ = writeln!(
        out,
        "parse          {parse_label}  {}",
        parse_fields(parse_options)
    );
    let _ = writeln!(out, "input          {input}");
    let _ = writeln!(out, "input-bytes    {input_bytes}");
    // `full+glyphs` can never be a committed golden — `--update` refuses to
    // pair with `--glyphs` — and it is named in the header anyway so that a
    // dump someone saved to a file is self-identifying.
    let _ = writeln!(
        out,
        "form           {}",
        match detail {
            Detail::PerItem => "full",
            Detail::PerGlyph => "full+glyphs",
            Detail::Digest => "digest",
        }
    );
    let _ = writeln!(out);

    // ---- geometry summary -------------------------------------------------
    let _ = writeln!(
        out,
        "list           width={} height={} blocks={}",
        f2(list.width),
        f2(list.height),
        list.blocks.len()
    );
    if digest {
        let full = blocks_section(list, Detail::PerItem, fonts);
        let _ = writeln!(out, "full-sha256    {}", sha256_hex(full.as_bytes()));
        let _ = writeln!(out, "full-bytes     {}", full.len());
    }
    // Every kind, always, including the zeroes: the census is then the same
    // nineteen lines in every golden, so "this input gained its first table"
    // reads as a value change rather than as a new line appearing in a diff.
    for (kind, count) in census(list) {
        let _ = writeln!(out, "count {:<15}{count}", kind.name());
    }

    if !digest {
        let _ = writeln!(out);
        out.push_str(&blocks_section(list, detail, fonts));
    }
    out
}

/// The per-block half of the format. Split out because the digest form hashes
/// exactly these bytes.
fn blocks_section(list: &DisplayList, detail: Detail, fonts: &Fonts) -> String {
    let mut out = String::new();
    for (i, block) in list.blocks.iter().enumerate() {
        let _ = write!(
            out,
            "block {i} {} bounds=[{} {} {} {}]",
            block.kind.name(),
            f2(block.bounds.x),
            f2(block.bounds.y),
            f2(block.bounds.width),
            f2(block.bounds.height)
        );
        // Present only when they say something. `lang=` is D11's evidence that
        // the mapping landed; `overflow=` is the tell that a block scrolls
        // rather than wraps, and both are rare enough that carrying them on
        // every line would bury the geometry.
        if let Some(lang) = &block.language {
            let _ = write!(out, " lang={lang}");
        }
        if block.overflow_x != 0.0 {
            let _ = write!(out, " overflow={}", f2(block.overflow_x));
        }
        let _ = writeln!(out, " items={}", block.items.len());

        for item in &block.items {
            match item {
                DisplayItem::Glyphs(run) => {
                    let _ = writeln!(out, "  {}", glyph_run_line(run, fonts));
                    if detail == Detail::PerGlyph {
                        for g in &run.glyphs {
                            let _ = writeln!(out, "    g {} [{} {}]", g.id, f2(g.x), f2(g.y));
                        }
                    }
                }
                DisplayItem::InlineBox(b) => {
                    let _ = writeln!(
                        out,
                        "  inline-box id={} [{} {} {} {}] baseline={} flow={}",
                        b.id,
                        f2(b.x),
                        f2(b.y),
                        f2(b.width),
                        f2(b.height),
                        match b.baseline {
                            Some(v) => f2(v),
                            None => "none".to_string(),
                        },
                        match b.flow {
                            mt_layout::InlineBoxFlow::InFlow => "in-flow",
                            mt_layout::InlineBoxFlow::OutOfFlow => "out-of-flow",
                            mt_layout::InlineBoxFlow::CustomOutOfFlow => "custom-out-of-flow",
                        }
                    );
                }
                DisplayItem::Rect(r) => {
                    let _ = write!(
                        out,
                        "  rect [{} {} {} {}] fill={}",
                        f2(r.rect.x),
                        f2(r.rect.y),
                        f2(r.rect.width),
                        f2(r.rect.height),
                        hex(r.brush)
                    );
                    if r.corner_radius != 0.0 {
                        let _ = write!(out, " radius={}", f2(r.corner_radius));
                    }
                    if r.rotation_deg != 0.0 {
                        let _ = write!(out, " rot={}", f2(r.rotation_deg));
                    }
                    let _ = writeln!(out);
                }
                DisplayItem::Line(l) => {
                    let _ = writeln!(
                        out,
                        "  line [{} {}]-[{} {}] width={} style={} stroke={}",
                        f2(l.x0),
                        f2(l.y0),
                        f2(l.x1),
                        f2(l.y1),
                        f2(l.width),
                        match l.style {
                            mt_layout::theme::LineStyle::Solid => "solid",
                            mt_layout::theme::LineStyle::Dashed => "dashed",
                            mt_layout::theme::LineStyle::Dotted => "dotted",
                            mt_layout::theme::LineStyle::None => "none",
                        },
                        hex(l.brush)
                    );
                }
            }
        }
    }
    out
}

/// One glyph run, minus its glyphs.
///
/// `origin` and `advance` are the run's own `offset` / `advance`, which is the
/// deliberate half of a decision the block-flow phase flagged: parley's
/// `Layout::width()` **excludes** trailing whitespace where a run's
/// `offset + advance` **includes** it, so the two disagree on any line that
/// ends in spaces. The golden reports the run's numbers, because they are what
/// a renderer draws from and what M4 will hit-test against;
/// `Layout::width()` reaches a golden only indirectly, through
/// `BlockDisplay::overflow_x`, which is the one place `mt-layout` deliberately
/// asks the *ink* question rather than the advance one.
///
/// The face is named by its **file**, not by its [`FontId`](mt_layout::FontId).
/// A `FontId` is an index into whatever order the shell happened to register
/// in, so `font7` would be both unreadable and unstable under a `faces.toml`
/// reorder that changed nothing about the glyphs. The file name is the first
/// consumer `Fonts::file_name` has had, and it is what D8 said the accessor
/// was for: getting from an id in the display list back to the bytes the shell
/// registered.
///
/// The fields are in two groups: `font size fill` are the run's resolved
/// **style**, and `rtl text origin advance n seq` are where it landed and what
/// is in it. `fill` joined the first group at v3 — see [`FORMAT_VERSION`] for
/// why a golden without it was not a serialization of the display list.
fn glyph_run_line(run: &GlyphRun, fonts: &Fonts) -> String {
    format!(
        "glyphs font={} size={} fill={} rtl={} text={}..{} origin=[{} {}] advance={} n={} seq={}",
        face_name(run.font, fonts),
        f2(run.font_size),
        hex(run.brush),
        u8::from(run.is_rtl),
        run.text_range.start,
        run.text_range.end,
        f2(run.offset),
        f2(run.baseline),
        f2(run.advance),
        run.glyphs.len(),
        glyph_seq_hash(run)
    )
}

/// A face's file name without its extension, or `<unregistered:N>`.
///
/// The fallback branch is unreachable through this command — the same `Fonts`
/// shapes and emits — and is spelled out rather than `unwrap`ped because it is
/// the exact symptom of the `Blob::id()` trap in the module doc, and a reader
/// who ever sees it in a golden should be told which mistake produced it.
fn face_name(id: mt_layout::FontId, fonts: &Fonts) -> String {
    match fonts.file_name(id) {
        Some(file) => file
            .rsplit_once('.')
            .map_or(file, |(stem, _)| stem)
            .to_string(),
        None => format!("<unregistered:{}>", id.index()),
    }
}

/// A short hash of the run's `(id, x, y)` sequence.
///
/// Without it a per-item line is blind to the change that matters most: a
/// shaping regression that keeps the glyph count and the total advance but
/// reorders, re-substitutes or re-positions the glyphs inside the run — which
/// is exactly what a ligature or `GSUB` change looks like.
///
/// **It hashes the formatted two-decimal text, not the `f32` bits.** D10
/// property 4 makes two decimals *the* precision of the artifact, and a hash
/// over raw bits would make the golden strictly more sensitive than the numbers
/// printed beside it — a platform disagreeing in the third decimal would fail
/// with no visible diff to read, which is the opposite of what a golden is for.
fn glyph_seq_hash(run: &GlyphRun) -> String {
    let mut buf = String::with_capacity(run.glyphs.len() * 24);
    for g in &run.glyphs {
        let _ = writeln!(buf, "{} {} {}", g.id, f2(g.x), f2(g.y));
    }
    sha256_hex(buf.as_bytes())[..8].to_string()
}

/// One entry per [`BlockKind`], in `BlockKind::ALL` order.
fn census(list: &DisplayList) -> Vec<(BlockKind, usize)> {
    BlockKind::ALL
        .iter()
        .map(|kind| {
            (
                *kind,
                list.blocks.iter().filter(|b| b.kind == *kind).count(),
            )
        })
        .collect()
}

/// Two decimals, fixed — D10 property 4 — with `-0.00` normalised to `0.00`.
///
/// The normalisation is not cosmetic. IEEE-754 has two zeroes and a sign bit
/// survives arithmetic that produces one (`0.0 * -1.0`, `-0.004` rounded),
/// so without this a golden could differ between platforms on a value that is
/// numerically identical — a false positive that would then be "fixed" by
/// loosening the precision, which is exactly the move property 4 exists to
/// forbid.
fn f2(v: f32) -> String {
    let s = format!("{v:.2}");
    if s == "-0.00" { "0.00".to_string() } else { s }
}

/// `#rrggbbaa`, always eight digits, so a brush is one fixed-width token.
fn hex(b: Brush) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", b.r, b.g, b.b, b.a)
}

// ---------------------------------------------------------------------------
// The both-themes-differ gate
// ---------------------------------------------------------------------------

/// The block kinds whose census is zero in every golden.
///
/// Read out of the serialized census rather than out of the display lists, so
/// that it says what a *reviewer opening the files* would find rather than what
/// the layout engine happens to know.
fn kinds_no_golden_exercises(produced: &BTreeMap<(String, String), String>) -> Vec<&'static str> {
    BlockKind::ALL
        .iter()
        .map(|kind| kind.name())
        .filter(|name| {
            let zero = format!("count {name:<15}0\n");
            produced.values().all(|golden| golden.contains(&zero))
        })
        .collect()
}

/// Everything below the provenance header — the geometry half of a golden.
fn geometry_of(golden: &str) -> &str {
    match golden.split_once("\n\n") {
        Some((_, rest)) => rest,
        None => golden,
    }
}

/// Inputs whose two goldens are geometrically identical.
///
/// S1's gate says *"both theme widths (800/750) produce different, correct
/// goldens from the same document"*. Different **goldens** is trivially true —
/// the header names the theme — so what is checked is that they differ below
/// the header.
fn themes_must_differ(produced: &BTreeMap<(String, String), String>) -> Vec<String> {
    let mut by_input: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for ((input, _), body) in produced {
        by_input.entry(input.as_str()).or_default().push(body);
    }
    by_input
        .into_iter()
        .filter(|(_, bodies)| bodies.len() == 2 && geometry_of(bodies[0]) == geometry_of(bodies[1]))
        .map(|(input, _)| input.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// --measure
// ---------------------------------------------------------------------------

/// Print the would-be **full** serialization size of every selected input at
/// both themes, and write nothing.
///
/// This is the measurement `DIGEST_INPUTS` was cut from, kept as a subcommand
/// flag rather than as a number in a comment so that the cut can be re-checked
/// when the corpus or the format changes.
fn measure(
    fonts: &mut Fonts,
    shaper: &mut TextShaper,
    provenance: &Provenance,
    documents: &[(String, String)],
) -> Result<i32, String> {
    let themes = themes();
    println!(
        "\n{:<20} {:>10} {:>9} {:>9} {:>13} {:>13}",
        "input", "bytes", "blocks", "items", "full (muya)", "full (dark)"
    );
    for (name, text) in documents {
        let (parse_opts, parse_label) = parse_options(name);
        let parsed = mt_md::parse(text, parse_opts);
        let options = layout_options(&parse_opts, parsed.labels.clone());
        let mut sizes = Vec::new();
        let mut blocks = 0usize;
        let mut items = 0usize;
        for theme in &themes {
            let list = layout_with(
                &parsed.document,
                theme,
                f32::INFINITY,
                fonts,
                shaper,
                &options,
            )
            .map_err(|e| format!("{name} at {}: {e}", theme.name))?;
            blocks = list.blocks.len();
            items = list.blocks.iter().map(|b| b.items.len()).sum();
            sizes.push(
                serialize(
                    &Golden {
                        provenance,
                        theme,
                        input: name,
                        input_bytes: text.len(),
                        parse_options: parse_opts,
                        parse_label: &parse_label,
                    },
                    &list,
                    Detail::PerItem,
                    fonts,
                )
                .len(),
            );
        }
        println!(
            "{:<20} {:>10} {:>9} {:>9} {:>13} {:>13}",
            name,
            text.len(),
            blocks,
            items,
            sizes[0],
            sizes[1]
        );
    }
    println!(
        "\nDigest inputs today: {}. `full (…)` is what a full golden would weigh.",
        DIGEST_INPUTS.join(", ")
    );
    Ok(0)
}

// ---------------------------------------------------------------------------
// Drift reporting
// ---------------------------------------------------------------------------

/// Print the differing lines, capped unless `--verbose`.
fn report_drift(expected: &str, actual: &str, verbose: bool) {
    const CAP: usize = 20;
    let old: Vec<&str> = expected.lines().collect();
    let new: Vec<&str> = actual.lines().collect();
    let mut shown = 0usize;
    let mut total = 0usize;
    for i in 0..old.len().max(new.len()) {
        let a = old.get(i).copied().unwrap_or("<end of file>");
        let b = new.get(i).copied().unwrap_or("<end of file>");
        if a == b {
            continue;
        }
        total += 1;
        if verbose || shown < CAP {
            println!("        line {}:", i + 1);
            println!("          golden {a}");
            println!("          actual {b}");
            shown += 1;
        }
    }
    if total > shown {
        println!(
            "        … and {} more differing line(s); --verbose for all",
            total - shown
        );
    }
}

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), hand-rolled
// ---------------------------------------------------------------------------
//
// Lifted from `spikes/s1-faces/src/main.rs`, which wrote it for exactly this
// move: `xtask` lives in the **root** workspace, where a new dependency is a
// deliberate act, and D10 needs digests in three places (the face files, the
// `faces.toml` content hash in every header, and the digest goldens). Sixty
// lines that are self-validating — a wrong implementation mismatches all
// twelve committed face digests at once rather than none — are cheaper to move
// than a dependency is to add. The house rule is `deps.rs:14-17`.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data`, lower-case hex.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let b = &chunk[i * 4..i * 4 + 4];
            *word = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (i, kv) in K.iter().enumerate() {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*kv)
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        crate::repo_root()
    }

    /// The three FIPS 180-4 vectors, so a transcription error fails here rather
    /// than as twelve mysterious face digest mismatches.
    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    /// It agrees with the committed face list, which is the check that matters:
    /// the digests in `faces.toml` were produced by a different program.
    #[test]
    fn sha256_agrees_with_every_committed_face_digest() {
        let dir = root().join("assets").join("fonts");
        let list = mt_layout::FaceList::from_toml_str(
            &std::fs::read_to_string(dir.join("faces.toml"))
                .expect("faces.toml")
                .replace("\r\n", "\n"),
        )
        .expect("faces.toml parses");
        assert_eq!(list.faces.len(), 12, "the committed face set is twelve");
        for face in &list.faces {
            let bytes = std::fs::read(dir.join(&face.file)).expect("face file");
            assert_eq!(
                sha256_hex(&bytes),
                face.sha256,
                "{} disagrees with faces.toml",
                face.file
            );
        }
    }

    #[test]
    fn minus_zero_is_normalised_and_precision_is_two_decimals() {
        assert_eq!(f2(0.0), "0.00");
        assert_eq!(f2(-0.0), "0.00");
        assert_eq!(f2(-0.004), "0.00");
        assert_eq!(f2(-0.006), "-0.01");
        assert_eq!(f2(681.4912), "681.49");
        assert_eq!(f2(25.6), "25.60");
    }

    #[test]
    fn the_parley_rev_comes_from_the_lockfile_and_the_face_list_agrees() {
        let lock = std::fs::read_to_string(root().join("Cargo.lock")).expect("Cargo.lock");
        let rev = parley_rev_from_lock(&lock).expect("a parley git source in Cargo.lock");
        assert_eq!(rev.len(), 40, "a git rev is 40 hex characters: {rev}");
        assert!(rev.chars().all(|c| c.is_ascii_hexdigit()));
        let list = mt_layout::FaceList::bundled();
        assert_eq!(
            list.meta.parley_rev, rev,
            "assets/fonts/faces.toml was measured against a different parley than Cargo.lock pins"
        );
    }

    /// The list of digest inputs may only name files that exist, or the rule
    /// silently stops applying to a renamed corpus file and a 40 MB golden
    /// lands in a review.
    #[test]
    fn every_digest_input_is_a_real_corpus_input() {
        let names: Vec<String> = inputs(&root())
            .expect("corpus inputs")
            .iter()
            .map(|p| file_name(p))
            .collect();
        for name in DIGEST_INPUTS {
            assert!(
                names.contains(&name.to_string()),
                "DIGEST_INPUTS names {name}, which is not a corpus input"
            );
        }
        assert!(
            !names.contains(&NOT_AN_INPUT.to_string()),
            "bench/corpus/README.md must not be a layout input"
        );
    }

    /// Every input has exactly two goldens, and no golden is orphaned.
    #[test]
    fn the_committed_goldens_are_exactly_one_per_input_per_theme() {
        let root = root();
        let mut expected: Vec<String> = Vec::new();
        for path in inputs(&root).expect("corpus inputs") {
            for theme in themes() {
                expected.push(golden_name(&file_name(&path), &theme.name));
            }
        }
        expected.sort();

        let dir = root.join(GOLDEN_DIR);
        let mut found: Vec<String> = std::fs::read_dir(&dir)
            .expect("bench/layout-goldens exists")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".txt"))
            .collect();
        found.sort();
        assert_eq!(found, expected);
    }

    /// **S1's third gate clause, against the committed artifacts.**
    ///
    /// Not "the two files differ" — the header names the theme, so that is free
    /// — but that they differ *below* the header, in the geometry. `800` and
    /// `750` are the only axis on which the two shipped themes disagree
    /// geometrically (plus `dark`'s zeroed code-block border), so a document
    /// that lays out identically at both would mean the width is not reaching
    /// the flow.
    #[test]
    fn both_theme_widths_produce_different_geometry_for_every_input() {
        let root = root();
        let dir = root.join(GOLDEN_DIR);
        for path in inputs(&root).expect("corpus inputs") {
            let name = file_name(&path);
            let read = |theme: &str| {
                std::fs::read_to_string(dir.join(golden_name(&name, theme)))
                    .unwrap_or_else(|e| panic!("{name} {theme}: {e}"))
                    .replace("\r\n", "\n")
            };
            let muya = read("muya-default");
            let dark = read("dark");
            assert_ne!(muya, dark, "{name}: the two goldens are byte-identical");
            assert_ne!(
                geometry_of(&muya),
                geometry_of(&dark),
                "{name}: the two goldens differ only in their header, so nothing about the \
                 800px/750px difference reached the layout"
            );
            // And the difference is a *width* difference, not an accident of
            // the header seam: the content column is the whole of it.
            assert!(
                muya.contains("content-width  700.00"),
                "{name}: muya-default's content column is 800 - 2*50"
            );
            assert!(
                dark.contains("content-width  650.00"),
                "{name}: dark's content column is 750 - 2*50"
            );
        }
    }

    /// Every committed golden is in the format this tool writes: the version
    /// line, the ten provenance fields, one blank line, then geometry.
    #[test]
    fn every_committed_golden_has_the_declared_header() {
        let root = root();
        let dir = root.join(GOLDEN_DIR);
        let faces_sha = sha256_hex(
            std::fs::read_to_string(root.join("assets").join("fonts").join("faces.toml"))
                .expect("faces.toml")
                .replace("\r\n", "\n")
                .as_bytes(),
        );
        let rev = parley_rev_from_lock(
            &std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock"),
        )
        .expect("parley rev");

        for entry in std::fs::read_dir(&dir).expect("bench/layout-goldens") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .expect("golden")
                .replace("\r\n", "\n");
            let name = file_name(&path);
            let (header, geometry) = text
                .split_once("\n\n")
                .unwrap_or_else(|| panic!("{name}: no blank line after the header"));
            let lines: Vec<&str> = header.lines().collect();
            assert_eq!(
                lines[0], FORMAT_VERSION,
                "{name}: version is {:?}, want {FORMAT_VERSION}",
                lines[0]
            );
            assert_eq!(
                lines.len(),
                11,
                "{name}: header is the version + ten fields"
            );
            for (i, key) in [
                "theme",
                "theme-width",
                "content-width",
                "parley",
                "faces",
                "options",
                "parse",
                "input",
                "input-bytes",
                "form",
            ]
            .iter()
            .enumerate()
            {
                assert!(
                    lines[i + 1].starts_with(key),
                    "{name}: header field {} is {:?}, want {key}",
                    i + 1,
                    lines[i + 1]
                );
            }
            assert!(
                header.contains(&format!("parley         {rev}")),
                "{name}: header does not cite the pinned parley revision"
            );
            assert!(
                header.contains(&format!("faces          {faces_sha}")),
                "{name}: header does not cite the current faces.toml hash"
            );
            assert!(
                geometry.starts_with("list           width="),
                "{name}: the geometry half must open with the list summary"
            );
            // Nineteen census lines, one per BlockKind, always.
            assert_eq!(
                geometry.lines().filter(|l| l.starts_with("count ")).count(),
                BlockKind::ALL.len(),
                "{name}: the census is one line per BlockKind"
            );
        }
    }

    /// Only the three named inputs get a digest golden, and they all do.
    #[test]
    fn the_digest_rule_is_applied_to_exactly_three_inputs() {
        let root = root();
        let dir = root.join(GOLDEN_DIR);
        for path in inputs(&root).expect("corpus inputs") {
            let name = file_name(&path);
            let want = if DIGEST_INPUTS.contains(&name.as_str()) {
                "form           digest"
            } else {
                "form           full"
            };
            for theme in themes() {
                let golden = std::fs::read_to_string(dir.join(golden_name(&name, &theme.name)))
                    .expect("golden")
                    .replace("\r\n", "\n");
                assert!(golden.contains(want), "{name} {}: want {want}", theme.name);
            }
        }
    }

    /// **M3-R6's first-review clause, ratcheted — and now at 19 of 19.**
    ///
    /// The risk register says the first golden for each block kind is read by
    /// eye once and the review is recorded. Until `bench/corpus/block-kinds.md`
    /// existed, five of the nineteen kinds had no first golden to read at all:
    /// `setext-heading`, `html-block`, `frontmatter`, `diagram` and `footnote`.
    /// Three of those five are D11's own arms and a fourth cost six new theme
    /// fields and a drawn label at S1, so the gap was not academic — the
    /// decision was implemented and never once exercised by the artifact meant
    /// to verify it.
    ///
    /// **The list is now empty and it must stay empty.** The assertion is
    /// two-way, which is `spec/expected-failures.json`'s shape: a kind that
    /// stops being exercised is a corpus regression, and a kind that starts
    /// being exercised is a golden nobody has read yet. Either way, look at the
    /// golden, then edit the list in the same commit.
    #[test]
    fn every_block_kind_is_exercised_by_at_least_one_golden() {
        const UNEXERCISED_KINDS: [&str; 0] = [];
        let dir = root().join(GOLDEN_DIR);
        let mut produced = BTreeMap::new();
        for entry in std::fs::read_dir(&dir).expect("bench/layout-goldens") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            produced.insert(
                (file_name(&path), String::new()),
                std::fs::read_to_string(&path)
                    .expect("golden")
                    .replace("\r\n", "\n"),
            );
        }
        assert_eq!(
            kinds_no_golden_exercises(&produced),
            UNEXERCISED_KINDS.to_vec(),
            "the set of block kinds no golden exercises has changed. If a kind has GAINED a \
             golden, read it by eye once (M3-R6), record the review, and delist it here."
        );
    }

    #[test]
    fn update_is_never_implied_and_never_pairs_with_a_dump() {
        assert!(!Opts::parse(&[]).expect("no args").update);
        assert!(
            !Opts::parse(&["--verbose".into(), "--measure".into()])
                .expect("flags")
                .update
        );
        assert!(Opts::parse(&["--update".into()]).expect("update").update);
        assert!(Opts::parse(&["--update".into(), "--glyphs".into()]).is_err());
        assert!(Opts::parse(&["--update".into(), "--measure".into()]).is_err());
        assert!(Opts::parse(&["--glyphs".into()]).is_err());
        assert!(Opts::parse(&["--nonsense".into()]).is_err());
        assert!(Opts::parse(&["--only".into()]).is_err());
    }

    #[test]
    fn the_geometry_seam_is_the_first_blank_line() {
        let golden = "layout-golden v1\ntheme          x\n\nlist           width=1.00\n";
        assert_eq!(geometry_of(golden), "list           width=1.00\n");
    }

    #[test]
    fn identical_geometry_at_two_themes_is_reported() {
        let mut produced = BTreeMap::new();
        produced.insert(
            ("a.md".to_string(), "muya-default".to_string()),
            "header a\n\nbody\n".to_string(),
        );
        produced.insert(
            ("a.md".to_string(), "dark".to_string()),
            "header b\n\nbody\n".to_string(),
        );
        assert_eq!(themes_must_differ(&produced), vec!["a.md".to_string()]);
        produced.insert(
            ("a.md".to_string(), "dark".to_string()),
            "header b\n\nbody 2\n".to_string(),
        );
        assert!(themes_must_differ(&produced).is_empty());
    }
}
