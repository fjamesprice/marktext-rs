//! S1 step 2 — the face list, measured rather than asserted.
//!
//! M3.md D7 decided that `mt-layout` never enumerates or opens a font: the
//! collection is built above it and passed in, and goldens run against a
//! **pinned bundled set registered from bytes**. That makes `assets/fonts/` a
//! committed artifact, and M3-R10 is the risk that the artifact does not
//! actually cover the corpus. D7 left three facts on the table, two measured at
//! S0 and one not:
//!
//!   1. MarkText's Open Sans is `.woff`, which skrifa rejects **silently** —
//!      `register_fonts` returns an empty `Vec`, not an error.
//!   2. DejaVu Sans Mono renders Arabic but not Hebrew, and `rtl.md` is half
//!      Hebrew.
//!   3. `cjk.md` and `emoji.md` are gate inputs too, and **their coverage was
//!      unanswered**. Answering it is what this binary is for.
//!
//! So this is E1's font probe (`spikes/e1-fonts-main`) pointed at the committed
//! set instead of at MarkText's directory, with the same instrument: register
//! from a `Blob`, lay text out, and **count `.notdef` glyphs**. A tofu count is
//! a measurement; a cmap listing is an argument, because a codepoint present in
//! `cmap` can still fail to reach a glyph through family selection.
//!
//! Three things here are *not* in E1 and are the point of the exercise:
//!
//!   * **`assets/fonts/faces.toml` is checked against the files.** Family name,
//!     weight, style, byte length and SHA-256 are all re-derived from the bytes
//!     on every run. A face list that has drifted from its fonts is worse than
//!     no face list, because D10's goldens would inherit the drift silently.
//!   * **Three fallback chains are compared, not one.** (A) the theme's stack
//!     as written, (B) the `faces.toml` order as an explicit family list, and
//!     (C) the theme's stack with the collection wired up from `faces.toml` —
//!     `append_fallbacks` per script and `append_generic_families` per CSS
//!     generic. Which of these covers the corpus decides what the shell must
//!     actually do at S1, and that is a fact about fontique, not a preference.
//!     The measured answer is that A leaves 249 of 291 codepoints as tofu,
//!     and B and C are identical at 146 — all of them CJK.
//!   * **Coverage is reported per corpus file**, over distinct non-ASCII
//!     codepoints, which is the unit M3's exit gate is written in.
//!
//! Usage:
//!
//! ```text
//! cargo run -p s1-faces --release                 # the committed set
//! cargo run -p s1-faces --release -- --extra DIR  # plus uncommitted candidates
//! ```
//!
//! `--extra` exists because the CJK question is a *pricing* question: the
//! candidates that would answer it are 4.5–16.4 MB each and are deliberately
//! not committed (see `assets/fonts/README.md`). Pointing this at a scratch
//! directory reproduces the numbers in that file's CJK section without putting
//! the bytes in git.
//!
//! Exit status is non-zero if a `faces.toml` entry disagrees with its file, or
//! if any registration comes back empty. Coverage gaps do **not** fail the run
//! — the CJK gap is known, priced and deliberate, and a probe that refused to
//! run until it was closed would just get deleted.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, FamilyId, Script, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, GenericFamily, Layout,
    LayoutContext, PositionedLayoutItem, StyleProperty,
};
use serde::Deserialize;

/// Recorded in the output so a reader can tell which arm produced the numbers.
/// Same string shape as `e1-fonts-main`'s, for the same reason.
const ARM: &str = "parley git main @ a0752c7bd, default-features = false, features = [\"std\"]";

/// MarkText's own bundled font directory, in the reference clone. Used for one
/// thing only: re-running the `.woff` trap on a real `.woff` file, so D7's
/// first fact is **reproduced** here rather than quoted from S0. Absent on a
/// machine without the clone, and the probe says so instead of failing — the
/// rest of the measurement does not depend on it.
const MARKTEXT_FONT_DIR: &str = r"C:\Dev\marktext\packages\muya\src\assets\styles\fonts";

/// Files above this size are counted for distinct codepoints but never laid
/// out. `5mb.md` has no non-ASCII content at all; laying it out would measure
/// parley's throughput, which is S6's job and not this probe's.
const LAYOUT_SIZE_LIMIT: u64 = 64 * 1024;

// ---------------------------------------------------------------------------
// assets/fonts/faces.toml
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FaceList {
    meta: Meta,
    face: Vec<Face>,
}

#[derive(Deserialize)]
struct Meta {
    revision: u32,
    parley_rev: String,
    total_font_bytes: u64,
}

/// Every entry carries every key, so a loader needs no per-file special casing
/// — that is a requirement on the format, not an accident of this struct.
#[derive(Deserialize)]
struct Face {
    file: String,
    family: String,
    subfamily: String,
    weight: u16,
    style: String,
    variable: bool,
    axes: Vec<Axis>,
    scripts: Vec<String>,
    fallback_scripts: Vec<String>,
    generic_families: Vec<String>,
    role: String,
    /// True for a file generated from an upstream font rather than committed
    /// as upstream shipped it. Printed in the registration table because a
    /// derived face is a **test fixture** and the run that produces the
    /// goldens should say so out loud every time.
    derived: bool,
    derived_from: String,
    derived_command: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct Axis {
    tag: String,
    min: f32,
    default: f32,
    max: f32,
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let root = repo_root();
    let fonts_dir = root.join("assets/fonts");
    let extra_dir = parse_extra_arg();

    println!("=== S1 face-list probe — arm: {ARM} ===");
    println!("repo root:   {}", root.display());
    println!("face list:   {}", fonts_dir.join("faces.toml").display());
    println!(
        "complex-scripts feature enabled in this build: {}",
        cfg!(feature = "complex-scripts")
    );
    println!();

    let list: FaceList = toml::from_str(
        &std::fs::read_to_string(fonts_dir.join("faces.toml")).expect("read faces.toml"),
    )
    .expect("faces.toml must parse");

    let mut problems: Vec<String> = Vec::new();

    // (1) Register the committed set and check the list against the bytes.
    println!("(1) registration — D7's in-memory path, and the list checked against the files");
    println!(
        "    Collection::new(CollectionOptions {{ shared: false, system_fonts: false }}), then"
    );
    println!("    register_fonts(Blob<u8>, None) -> Vec<(FamilyId, Vec<FontInfo>)>");
    println!();
    let mut cx = new_context();
    println!(
        "    families visible before registering anything: {}",
        cx.collection.family_names().count()
    );
    println!();
    println!(
        "    {:<32} {:>9} {:<14} {:<8} {:<18} declared vs observed",
        "file", "bytes", "magic", "sha256", "family (from skrifa)"
    );
    let mut family_ids: Vec<(String, FamilyId)> = Vec::new();
    let mut blob_names: Vec<(u64, String)> = Vec::new();
    for face in &list.face {
        let path = fonts_dir.join(&face.file);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let observed = register_one(&mut cx, &bytes, &mut blob_names, &face.file);
        report_face(face, &bytes, &observed, &mut problems);
        for (fid, name) in &observed.families {
            if !family_ids.iter().any(|(n, _)| n == name) {
                family_ids.push((name.clone(), *fid));
            }
        }
    }
    println!();
    println!(
        "    total families registered: {} — {:?}",
        cx.collection.family_names().count(),
        family_ids.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    let actual_total: u64 = list.face.iter().map(|f| f.bytes).sum();
    println!(
        "    faces.toml [meta]: revision {}, parley_rev {}, total_font_bytes {} (sum of entries: {actual_total})",
        list.meta.revision, list.meta.parley_rev, list.meta.total_font_bytes
    );
    if actual_total != list.meta.total_font_bytes {
        problems.push(format!(
            "[meta] total_font_bytes {} != sum of entries {actual_total}",
            list.meta.total_font_bytes
        ));
    }

    // (2) The `.woff` trap, reproduced rather than quoted.
    println!();
    woff_trap(&mut problems);

    // (3) Coverage, three chains.
    println!();
    let corpus = read_corpus(&root);
    let theme_body = theme_stack(&root, "body");
    let theme_code = theme_stack(&root, "code");
    println!("(3) coverage per corpus file — distinct non-ASCII codepoints, tofu-counted");
    println!("    theme [fonts] body: {theme_body:?}");
    println!("    theme [fonts] code: {theme_code:?}");
    println!();

    let chain_b: Vec<FontFamilyName<'static>> = family_ids
        .iter()
        .map(|(n, _)| FontFamilyName::Named(n.clone().into()))
        .collect();
    let chain_a: Vec<FontFamilyName<'static>> = theme_body.iter().map(|s| family_of(s)).collect();

    println!("    chain A = the theme's body stack, verbatim, over the registered set");
    coverage_table(&mut cx, &corpus, &chain_a);
    println!();
    println!("    chain B = faces.toml order as an explicit family list — THE PROPOSED CHAIN");
    coverage_table(&mut cx, &corpus, &chain_b);

    // Chain C needs its own collection: `append_fallbacks` mutates the
    // collection, so measuring A without it and C with it cannot share one.
    println!();
    println!(
        "    chain C = the theme's body stack + the three wiring calls a shell must make,\n              all driven from faces.toml: append_generic_families() from\n              `generic_families`, append_fallbacks() from `fallback_scripts`"
    );
    let mut cx_fb = new_context();
    let mut fb_ids: Vec<(String, FamilyId)> = Vec::new();
    let mut sink = Vec::new();
    for face in &list.face {
        let bytes = std::fs::read(fonts_dir.join(&face.file)).expect("read face");
        let observed = register_one(&mut cx_fb, &bytes, &mut sink, &face.file);
        for (fid, name) in &observed.families {
            if !fb_ids.iter().any(|(n, _)| n == name) {
                fb_ids.push((name.clone(), *fid));
            }
        }
    }
    let mut scripts: Vec<String> = list
        .face
        .iter()
        .flat_map(|f| f.fallback_scripts.iter().cloned())
        .collect();
    scripts.sort();
    scripts.dedup();
    for script in &scripts {
        // faces.toml order, deduplicated: four DejaVu files are one family, and
        // handing the same `FamilyId` to `append_fallbacks` four times would
        // say nothing extra while making the printed list unreadable.
        let mut ids: Vec<FamilyId> = Vec::new();
        for face in list
            .face
            .iter()
            .filter(|f| f.fallback_scripts.contains(script))
        {
            if let Some((_, id)) = fb_ids.iter().find(|(n, _)| *n == face.family)
                && !ids.contains(id)
            {
                ids.push(*id);
            }
        }
        let tag = script.as_bytes();
        if tag.len() != 4 {
            continue;
        }
        let key = Script::from_bytes([tag[0], tag[1], tag[2], tag[3]]);
        let n = ids.len();
        let accepted = cx_fb.collection.append_fallbacks(key, ids.into_iter());
        println!("      append_fallbacks({script}, {n} families) accepted: {accepted}");
    }
    // The generic families. `sans-serif` and `monospace` because both theme
    // stacks end in one and nothing else defines them when `system_fonts` is
    // off; `emoji` because parley reaches for it directly — see the note on
    // `generic_families` in faces.toml.
    let mut generics: Vec<String> = list
        .face
        .iter()
        .flat_map(|f| f.generic_families.iter().cloned())
        .collect();
    generics.sort();
    generics.dedup();
    for generic in &generics {
        let Some(g) = GenericFamily::parse(generic) else {
            problems.push(format!(
                "faces.toml: {generic:?} is not a CSS generic family"
            ));
            continue;
        };
        let mut ids: Vec<FamilyId> = Vec::new();
        for face in list
            .face
            .iter()
            .filter(|f| f.generic_families.contains(generic))
        {
            if let Some((_, id)) = fb_ids.iter().find(|(n, _)| *n == face.family)
                && !ids.contains(id)
            {
                ids.push(*id);
            }
        }
        println!(
            "      append_generic_families({generic}, {} families)",
            ids.len()
        );
        cx_fb.collection.append_generic_families(g, ids.into_iter());
    }
    coverage_table(&mut cx_fb, &corpus, &chain_a);

    // (4) Whole-file layout for the small files, plus the emoji cluster probe.
    println!();
    println!("(4) whole-file layout, chain B, for corpus files under {LAYOUT_SIZE_LIMIT} B");
    whole_file_layout(&mut cx, &corpus, &chain_b);
    println!();
    emoji_clusters(&mut cx, &chain_b);

    // (5) What the committed set still cannot render.
    println!();
    println!("(5) codepoints the committed set cannot render, under chain B");
    let mut uncovered_all: BTreeSet<u32> = BTreeSet::new();
    for file in &corpus {
        let missing = tofu_codepoints(&mut cx, &file.codepoints, &chain_b);
        if !missing.is_empty() {
            println!(
                "    {:<20} {:>4} of {:>4}   {}",
                file.name,
                missing.len(),
                file.codepoints.len(),
                fmt_codepoints(&missing, 16)
            );
            uncovered_all.extend(missing);
        }
    }
    if uncovered_all.is_empty() {
        println!("    none.");
    } else {
        println!(
            "    union: {} codepoints. See assets/fonts/README.md — this is the CJK gap, priced.",
            uncovered_all.len()
        );
    }

    // (6) Uncommitted candidates, for the pricing question only.
    if let Some(dir) = extra_dir {
        println!();
        println!(
            "(6) --extra candidates from {} (NOT committed)",
            dir.display()
        );
        extra_candidates(&dir, &corpus, &chain_b, &family_ids);
    }

    println!();
    if problems.is_empty() {
        println!("=== faces.toml agrees with every file it lists, and no registration was empty.");
    } else {
        println!("=== {} PROBLEM(S):", problems.len());
        for p in &problems {
            println!("  - {p}");
        }
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// registration
// ---------------------------------------------------------------------------

struct Observed {
    families: Vec<(FamilyId, String)>,
    weights: Vec<f32>,
    styles: Vec<String>,
}

fn new_context() -> FontContext {
    FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    }
}

fn register_one(
    cx: &mut FontContext,
    bytes: &[u8],
    blob_names: &mut Vec<(u64, String)>,
    label: &str,
) -> Observed {
    let blob = Blob::new(Arc::new(bytes.to_vec()) as Arc<dyn AsRef<[u8]> + Send + Sync>);
    blob_names.push((blob.id(), label.to_string()));
    let result = cx.collection.register_fonts(blob, None);
    let mut families = Vec::new();
    let mut weights = Vec::new();
    let mut styles = Vec::new();
    for (fid, infos) in &result {
        let name = cx
            .collection
            .family_name(*fid)
            .unwrap_or("<unnamed>")
            .to_string();
        families.push((*fid, name));
        for info in infos {
            weights.push(info.weight().value());
            styles.push(format!("{:?}", info.style()).to_lowercase());
        }
    }
    Observed {
        families,
        weights,
        styles,
    }
}

fn report_face(face: &Face, bytes: &[u8], observed: &Observed, problems: &mut Vec<String>) {
    // Both forms, because the ASCII form is what makes `wOFF` legible and the
    // hex form is what makes `00010000` (a plain TrueType) and `OTTO` (CFF)
    // distinguishable from it at a glance. This column is the trap's tell.
    let magic = magic_of(bytes);
    let digest = sha256_hex(bytes);
    let mut notes: Vec<String> = Vec::new();

    // The whole reason the empty case is checked first: `register_fonts`
    // returning `Vec::new()` is D7's silent failure, and a silent failure that
    // the probe reports as "ok" would defeat the probe.
    if observed.families.is_empty() {
        notes.push("REJECTED — 0 families".into());
        problems.push(format!("{}: register_fonts returned 0 families", face.file));
    }
    for (_, name) in &observed.families {
        if *name != face.family {
            notes.push(format!("family {name:?} != declared {:?}", face.family));
            problems.push(format!(
                "{}: family {name:?} != declared {:?}",
                face.file, face.family
            ));
        }
    }
    if bytes.len() as u64 != face.bytes {
        notes.push(format!("bytes {} != declared {}", bytes.len(), face.bytes));
        problems.push(format!(
            "{}: {} bytes on disk != declared {}",
            face.file,
            bytes.len(),
            face.bytes
        ));
    }
    if digest != face.sha256 {
        notes.push(format!("sha256 {digest} != declared"));
        problems.push(format!("{}: sha256 {digest} != declared", face.file));
    }
    if !observed
        .weights
        .iter()
        .any(|w| (*w - f32::from(face.weight)).abs() < 0.5)
    {
        notes.push(format!(
            "weight {:?} != declared {}",
            observed.weights, face.weight
        ));
        problems.push(format!(
            "{}: weight {:?} != declared {}",
            face.file, observed.weights, face.weight
        ));
    }
    if !observed.styles.iter().any(|s| s.starts_with(&face.style)) {
        notes.push(format!(
            "style {:?} != declared {:?}",
            observed.styles, face.style
        ));
        problems.push(format!(
            "{}: style {:?} != declared {:?}",
            face.file, observed.styles, face.style
        ));
    }
    let axes = if face.variable {
        let a = face
            .axes
            .iter()
            .map(|a| format!("{}={}..{}..{}", a.tag, a.min, a.default, a.max))
            .collect::<Vec<_>>()
            .join(",");
        format!(" VARIABLE[{a}]")
    } else {
        String::new()
    };
    // A derived face is a fixture, not an asset, and both its provenance keys
    // must be present or it is not re-derivable — which is the only thing that
    // makes committing a derived artifact defensible in the first place.
    if face.derived && (face.derived_from.is_empty() || face.derived_command.is_empty()) {
        problems.push(format!(
            "{}: derived = true but derived_from/derived_command is empty — not re-derivable",
            face.file
        ));
    }
    if !face.derived && !(face.derived_from.is_empty() && face.derived_command.is_empty()) {
        problems.push(format!(
            "{}: derived = false but carries derived_from/derived_command",
            face.file
        ));
    }
    let derived = if face.derived { " DERIVED-FIXTURE" } else { "" };
    let verdict = if notes.is_empty() {
        format!(
            "ok  w{} {} {} {:?} [{}]{axes}{derived}",
            face.weight,
            face.style,
            face.role,
            face.subfamily,
            face.scripts.join(",")
        )
    } else {
        notes.join("; ")
    };
    println!(
        "    {:<32} {:>9} {:<14} {:<8} {:<18} {verdict}",
        face.file,
        bytes.len(),
        magic,
        &digest[..8],
        observed
            .families
            .first()
            .map(|(_, n)| n.as_str())
            .unwrap_or("-"),
    );
}

/// The first four bytes as ASCII and hex — the container tag, which is what
/// E1's question 3(c) turned on and what `.woff` fails on.
fn magic_of(bytes: &[u8]) -> String {
    let ascii: String = bytes
        .iter()
        .take(4)
        .map(|b| {
            let c = *b as char;
            if c.is_ascii_graphic() { c } else { '.' }
        })
        .collect();
    let hex: String = bytes.iter().take(4).map(|b| format!("{b:02X}")).collect();
    format!("{ascii} {hex}")
}

/// D7's fact 1, re-run here so the finding is reproduced on this machine rather
/// than cited from `spikes/results/e1-bidi-inline-box.md`.
fn woff_trap(problems: &mut Vec<String>) {
    println!("(2) the .woff trap — D7 fact 1, reproduced rather than quoted");
    let dir = Path::new(MARKTEXT_FONT_DIR);
    let Ok(entries) = std::fs::read_dir(dir) else {
        println!("    SKIPPED: {MARKTEXT_FONT_DIR} is not present on this machine.");
        println!("    The finding stands from S0: all eight of MarkText's Open Sans files are");
        println!(
            "    WOFF1 (`wOFF` / 77 4F 46 46) and register_fonts returns 0 families for each."
        );
        return;
    };
    let mut woffs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("woff"))
        .collect();
    woffs.sort();
    let Some(path) = woffs.first() else {
        println!("    SKIPPED: no .woff files found under {MARKTEXT_FONT_DIR}.");
        return;
    };
    let bytes = std::fs::read(path).expect("read woff");
    let magic = magic_of(&bytes);
    let mut cx = new_context();
    let blob = Blob::new(Arc::new(bytes.clone()) as Arc<dyn AsRef<[u8]> + Send + Sync>);
    let result = cx.collection.register_fonts(blob, None);
    println!(
        "    {}  {} B  magic {magic}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        bytes.len()
    );
    println!(
        "    register_fonts -> {} families, and **no error and no panic**: the Vec is simply empty.",
        result.len()
    );
    println!(
        "    families in the collection afterwards: {}",
        cx.collection.family_names().count()
    );
    if result.is_empty() {
        println!(
            "    CONFIRMED. This is why D7 requires an empty result to be a HARD ERROR: a face"
        );
        println!(
            "    that fails to load without saying so renders tofu everywhere with no diagnostic."
        );
    } else {
        // If a future skrifa learns WOFF1 this branch fires, and that is a
        // finding worth failing the run over rather than quietly absorbing.
        println!("    UNEXPECTED: skrifa accepted a WOFF1 file. D7 fact 1 no longer holds.");
        problems.push("skrifa accepted a WOFF1 file — D7 fact 1 has changed".into());
    }
}

// ---------------------------------------------------------------------------
// corpus + coverage
// ---------------------------------------------------------------------------

struct CorpusFile {
    name: String,
    bytes: u64,
    text: Option<String>,
    codepoints: Vec<u32>,
}

fn read_corpus(root: &Path) -> Vec<CorpusFile> {
    let dir = root.join("bench/corpus");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read bench/corpus")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).expect("read corpus file");
            let bytes = text.len() as u64;
            let mut set: BTreeSet<u32> = BTreeSet::new();
            for c in text.chars() {
                if !c.is_ascii() {
                    set.insert(c as u32);
                }
            }
            CorpusFile {
                name: p.file_name().unwrap_or_default().to_string_lossy().into(),
                bytes,
                // The three large files are read for their codepoint set and
                // then dropped: laying them out would measure throughput, not
                // coverage, and the brief for this step says a set union is
                // enough for them.
                text: (bytes <= LAYOUT_SIZE_LIMIT).then_some(text),
                codepoints: set.into_iter().collect(),
            }
        })
        .collect()
}

/// Reads a font stack straight out of the theme rather than transcribing it,
/// so this probe cannot drift from D4's theme model.
fn theme_stack(root: &Path, key: &str) -> Vec<String> {
    let src = std::fs::read_to_string(root.join("crates/mt-layout/themes/muya-default.toml"))
        .expect("read muya-default.toml");
    let value: toml::Value = toml::from_str(&src).expect("theme parses");
    value["fonts"][key]
        .as_array()
        .expect("font stack is an array")
        .iter()
        .map(|v| v.as_str().expect("font family is a string").to_string())
        .collect()
}

fn family_of(name: &str) -> FontFamilyName<'static> {
    match GenericFamily::parse(name) {
        Some(g) => FontFamilyName::Generic(g),
        None => FontFamilyName::Named(name.to_string().into()),
    }
}

/// One codepoint, laid out on its own, classified by what came back.
///
/// Three outcomes, and the third is the one a naive cmap check gets wrong:
/// a default-ignorable codepoint (ZWJ U+200D, VS16 U+FE0F) is in no font's
/// cmap and produces **no glyph at all**, which is correct behaviour and not
/// a coverage failure.
enum Verdict {
    Rendered,
    Tofu,
    NoGlyph,
}

fn probe(cx: &mut FontContext, cp: u32, chain: &[FontFamilyName<'static>]) -> Verdict {
    let Some(ch) = char::from_u32(cp) else {
        return Verdict::NoGlyph;
    };
    let text = ch.to_string();
    let mut lcx: LayoutContext<()> = LayoutContext::new();
    let mut builder = lcx.ranged_builder(cx, &text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(16.0));
    builder.push_default(StyleProperty::FontFamily(FontFamily::List(
        chain.to_vec().into(),
    )));
    let mut layout: Layout<()> = builder.build(&text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());
    let (glyphs, notdef) = count_glyphs(&layout);
    if glyphs == 0 {
        Verdict::NoGlyph
    } else if notdef > 0 {
        Verdict::Tofu
    } else {
        Verdict::Rendered
    }
}

fn count_glyphs(layout: &Layout<()>) -> (usize, usize) {
    let mut glyphs = 0usize;
    let mut notdef = 0usize;
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(gr) = item {
                for g in gr.glyphs() {
                    glyphs += 1;
                    if g.id == 0 {
                        notdef += 1;
                    }
                }
            }
        }
    }
    (glyphs, notdef)
}

fn coverage_table(cx: &mut FontContext, corpus: &[CorpusFile], chain: &[FontFamilyName<'static>]) {
    println!(
        "      {:<20} {:>9} {:>8} {:>9} {:>6} {:>10}",
        "corpus file", "bytes", "non-ASCII", "rendered", "tofu", "no-glyph"
    );
    let mut t_cp = 0usize;
    let mut t_ok = 0usize;
    let mut t_tofu = 0usize;
    let mut t_none = 0usize;
    for file in corpus {
        let (mut ok, mut tofu, mut none) = (0usize, 0usize, 0usize);
        for cp in &file.codepoints {
            match probe(cx, *cp, chain) {
                Verdict::Rendered => ok += 1,
                Verdict::Tofu => tofu += 1,
                Verdict::NoGlyph => none += 1,
            }
        }
        println!(
            "      {:<20} {:>9} {:>8} {:>9} {:>6} {:>10}",
            file.name,
            file.bytes,
            file.codepoints.len(),
            ok,
            tofu,
            none
        );
        t_cp += file.codepoints.len();
        t_ok += ok;
        t_tofu += tofu;
        t_none += none;
    }
    println!(
        "      {:<20} {:>9} {:>8} {:>9} {:>6} {:>10}",
        "(sum, files)", "", t_cp, t_ok, t_tofu, t_none
    );
}

fn tofu_codepoints(
    cx: &mut FontContext,
    cps: &[u32],
    chain: &[FontFamilyName<'static>],
) -> Vec<u32> {
    cps.iter()
        .copied()
        .filter(|cp| matches!(probe(cx, *cp, chain), Verdict::Tofu))
        .collect()
}

fn fmt_codepoints(cps: &[u32], limit: usize) -> String {
    let mut s = cps
        .iter()
        .take(limit)
        .map(|c| format!("U+{c:04X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if cps.len() > limit {
        s.push_str(&format!(" … (+{})", cps.len() - limit));
    }
    s
}

fn whole_file_layout(
    cx: &mut FontContext,
    corpus: &[CorpusFile],
    chain: &[FontFamilyName<'static>],
) {
    println!(
        "      {:<20} {:>9} {:>8} {:>7} {:>8}",
        "corpus file", "glyphs", "notdef", "lines", "width"
    );
    for file in corpus {
        let Some(text) = &file.text else {
            println!(
                "      {:<20} {:>9} — over {LAYOUT_SIZE_LIMIT} B, codepoint set only",
                file.name, "skipped"
            );
            continue;
        };
        if text.is_empty() {
            println!("      {:<20} {:>9}", file.name, "0 (empty)");
            continue;
        }
        let mut lcx: LayoutContext<()> = LayoutContext::new();
        let mut builder = lcx.ranged_builder(cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(16.0));
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(
            chain.to_vec().into(),
        )));
        let mut layout: Layout<()> = builder.build(text);
        // 800 px is muya's `--editor-area-width` (D4's theme `content_width_px`),
        // so the line count below is at least the right order of magnitude for
        // what D10's goldens will hold.
        layout.break_all_lines(Some(800.0));
        layout.align(Alignment::Start, AlignmentOptions::default());
        let (glyphs, notdef) = count_glyphs(&layout);
        println!(
            "      {:<20} {:>9} {:>8} {:>7} {:>8.2}",
            file.name,
            glyphs,
            notdef,
            layout.lines().count(),
            layout.width()
        );
    }
}

/// The emoji face is in the set for advances and cluster counts, not colour
/// (that is S4's call). This is the cheapest check that the face actually
/// forms the sequences: each of these is **one grapheme** made of several
/// codepoints, and a face without the GSUB rules renders one glyph per
/// codepoint instead of one glyph per sequence.
fn emoji_clusters(cx: &mut FontContext, chain: &[FontFamilyName<'static>]) {
    const SEQUENCES: &[(&str, &str)] = &[
        (
            "family ZWJ",
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
        ),
        ("skin tone", "\u{1F44B}\u{1F3FD}"),
        ("flag GB", "\u{1F1EC}\u{1F1E7}"),
        ("keycap", "1\u{FE0F}\u{20E3}"),
        ("plain", "\u{1F600}"),
    ];
    println!("    emoji sequence shaping (one grapheme each) — chain B");
    println!(
        "      {:<12} {:>10} {:>7} {:>7} {:>9}",
        "sequence", "codepoints", "glyphs", "notdef", "advance"
    );
    for (name, text) in SEQUENCES {
        let mut lcx: LayoutContext<()> = LayoutContext::new();
        let mut builder = lcx.ranged_builder(cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(16.0));
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(
            chain.to_vec().into(),
        )));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());
        let (glyphs, notdef) = count_glyphs(&layout);
        println!(
            "      {:<12} {:>10} {:>7} {:>7} {:>9.2}",
            name,
            text.chars().count(),
            glyphs,
            notdef,
            layout.width()
        );
    }
}

/// Candidates that are **not** committed, measured so the CJK decision can be
/// priced rather than argued. Each is appended to the proposed chain in turn.
fn extra_candidates(
    dir: &Path,
    corpus: &[CorpusFile],
    chain: &[FontFamilyName<'static>],
    committed: &[(String, FamilyId)],
) {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read --extra dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("ttf" | "otf" | "ttc" | "otc")
            )
        })
        .collect();
    paths.sort();
    println!(
        "      {:<34} {:>10} {:<22} {:>13} {:>13}",
        "candidate", "bytes", "family", "cjk.md", "emoji.md"
    );
    let fonts_dir = repo_root().join("assets/fonts");
    for path in &paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let bytes = std::fs::read(path).expect("read candidate");
        // A fresh collection per candidate: the committed set plus exactly one
        // candidate, so the number attributed to the candidate is the number
        // the candidate is responsible for.
        let mut cx = new_context();
        let mut sink = Vec::new();
        let mut full_chain = chain.to_vec();
        for (family, _) in committed {
            let b = std::fs::read(fonts_dir.join(committed_file_for(family)))
                .expect("read committed face");
            register_one(&mut cx, &b, &mut sink, family);
        }
        let observed = register_one(&mut cx, &bytes, &mut sink, &name);
        let fam = observed
            .families
            .first()
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "<REJECTED>".into());
        for (_, n) in &observed.families {
            full_chain.push(FontFamilyName::Named(n.clone().into()));
        }
        let mut cells = Vec::new();
        for want in ["cjk.md", "emoji.md"] {
            let file = corpus.iter().find(|f| f.name == want).expect("corpus file");
            let ok = file
                .codepoints
                .iter()
                .filter(|cp| matches!(probe(&mut cx, **cp, &full_chain), Verdict::Rendered))
                .count();
            cells.push(format!("{ok}/{}", file.codepoints.len()));
        }
        println!(
            "      {:<34} {:>10} {:<22} {:>13} {:>13}",
            name,
            bytes.len(),
            fam,
            cells[0],
            cells[1]
        );
    }

    // And all of them at once. The CJK options worth pricing are combinations
    // (no single sub-16 MB face covers `cjk.md`), so the union has to be a
    // measurement too rather than an arithmetic claim about cmaps.
    if paths.len() > 1 {
        let mut cx = new_context();
        let mut sink = Vec::new();
        let mut full_chain = chain.to_vec();
        let mut total = 0usize;
        for (family, _) in committed {
            let b = std::fs::read(fonts_dir.join(committed_file_for(family)))
                .expect("read committed face");
            register_one(&mut cx, &b, &mut sink, family);
        }
        for path in &paths {
            let bytes = std::fs::read(path).expect("read candidate");
            total += bytes.len();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let observed = register_one(&mut cx, &bytes, &mut sink, &name);
            for (_, n) in &observed.families {
                if !full_chain
                    .iter()
                    .any(|f| matches!(f, FontFamilyName::Named(x) if x == n))
                {
                    full_chain.push(FontFamilyName::Named(n.clone().into()));
                }
            }
        }
        let mut cells = Vec::new();
        for want in ["cjk.md", "emoji.md"] {
            let file = corpus.iter().find(|f| f.name == want).expect("corpus file");
            let ok = file
                .codepoints
                .iter()
                .filter(|cp| matches!(probe(&mut cx, **cp, &full_chain), Verdict::Rendered))
                .count();
            cells.push(format!("{ok}/{}", file.codepoints.len()));
        }
        println!(
            "      {:<34} {:>10} {:<22} {:>13} {:>13}",
            format!("(all {} together)", paths.len()),
            total,
            "",
            cells[0],
            cells[1]
        );
    }
}

/// Maps a family name back to one representative committed file. Only used by
/// `--extra`, which needs the committed set present alongside each candidate.
fn committed_file_for(family: &str) -> &'static str {
    match family {
        "Open Sans" => "OpenSans-Regular.ttf",
        "Noto Sans Arabic" => "NotoSansArabic-Regular.ttf",
        "Noto Emoji" => "NotoEmoji[wght].ttf",
        _ => "DejaVuSansMono.ttf",
    }
}

// ---------------------------------------------------------------------------
// plumbing
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    // `spikes/s1-faces` -> `spikes` -> repo root. `CARGO_MANIFEST_DIR` rather
    // than the current directory so `cargo run -p s1-faces` works from
    // anywhere in either workspace.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("spikes/<crate> is two levels below the repo root")
        .to_path_buf()
}

fn parse_extra_arg() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--extra" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), hand-rolled
// ---------------------------------------------------------------------------
//
// A dependency was the obvious alternative and was declined for two reasons.
// The first is the house rule at `xtask/src/deps.rs:14-17` — hand-rolled over
// dependency for something this small. The second is specific to this probe:
// a later phase has to verify these same hashes from `cargo xtask layout`,
// which lives in the **root** workspace, where a new dependency is a
// deliberate act. Sixty lines that are already checked against
// `faces.toml`'s committed digests on every run are cheaper to move than a
// dependency is to add — and they are self-validating, because a wrong
// implementation mismatches all eleven digests at once rather than none.

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

fn sha256_hex(data: &[u8]) -> String {
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
