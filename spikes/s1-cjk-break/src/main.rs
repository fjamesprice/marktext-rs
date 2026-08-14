//! S1 step 3 — is parley's `complex-scripts` feature required for M3's CJK
//! gate clause? Measured, not inherited.
//!
//! C9 says the feature (dictionary line- and word-breaking for CJK, Thai,
//! Khmer, Lao and Myanmar) is *"almost certainly required"* because M3's exit
//! gate says "CJK renders correctly" and `bench/corpus/cjk.md` carries a long
//! unspaced line. D7 then priced it: **+3.6 MiB, 0 extra crates** — the
//! second-largest line item in the binary after the GPU backend D1 declined to
//! ship. Nobody checked the claim the price is being paid for.
//!
//! The doubt is specific. UAX #14 already permits a break **between two
//! ideographs** with no dictionary at all (classes ID/ID), and parley's own
//! feature description says the fallback is "a lightweight segmenter … that
//! falls back to character-level breaks for those scripts". Character-level
//! breaks are, for Han, very nearly the right answer: what a dictionary buys is
//! the *prohibitions* (don't split a word, don't strand a closing bracket),
//! not the permission. The corpus has no Thai, Khmer, Lao or Myanmar, where a
//! dictionary is the only thing that finds a break at all.
//!
//! So: lay `cjk.md`'s "Line breaking" line out at **both** theme widths
//! (muya-default 800px, dark 750px) and print every break position. Run twice:
//!
//! ```text
//! cargo run -p s1-cjk-break --release
//! cargo run -p s1-cjk-break --release --features complex-scripts
//! ```
//!
//! `-p` rather than `--workspace`, for the reason `spikes/Cargo.toml` records:
//! the feature resolver unifies across everything it builds at once, and the
//! `e1-parley-*` members would hand this crate a different `parley`.
//!
//! Diff the two outputs. Identical break positions mean the 3.6 MiB buys
//! nothing the corpus can see; different positions mean it does, and the
//! difference is the evidence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, FamilyId, Script, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, GenericFamily, Layout,
    LayoutContext, PositionedLayoutItem, StyleProperty,
};
use serde::Deserialize;

/// Both theme widths, both theme **content** widths, and one deliberately
/// narrow column.
///
/// 800 and 750 are `[metrics] content_width_px` in `muya-default.toml` and
/// `dark.toml`. 700 and 650 are those minus `2 × container_padding_x_px`
/// (50px), which is the width text is actually wrapped at — and the
/// distinction turns out to matter, because the corpus line is 681.49px wide:
/// it fits 800, 750 and 700 and wraps only at 650. 320 is not a theme width;
/// it is here so the comparison has several breaks to disagree about rather
/// than one, since "the two arms agree" is only evidence if there was
/// something to agree about.
const WIDTHS: [f32; 5] = [800.0, 750.0, 700.0, 650.0, 320.0];

/// A control, and the probe is worthless without it.
///
/// "No difference on Chinese" is indistinguishable from "the feature flag is
/// not reaching the code" unless something in the same run *does* differ. Thai
/// is the case the dictionary exists for: it has no inter-word spaces **and**
/// no per-character break opportunity in UAX #14, so the lightweight segmenter
/// has nowhere legal to break and the dictionary does. The corpus contains no
/// Thai — that is the point; this string is a probe instrument, not a gate
/// input, and no committed face covers it (the glyphs come back `.notdef`,
/// which does not affect where the breaks land).
///
/// "ภาษาไทยไม่มีช่องว่างระหว่างคำจึงต้องใช้พจนานุกรมในการตัดบรรทัด" — "Thai has
/// no spaces between words, so a dictionary is needed to break lines."
const THAI_CONTROL: &str = "ภาษาไทยไม่มีช่องว่างระหว่างคำจึงต้องใช้พจนานุกรมในการตัดบรรทัด";

/// `[metrics] font_size_px` in both shipped themes.
const FONT_SIZE: f32 = 16.0;

#[derive(Deserialize)]
struct FaceList {
    face: Vec<Face>,
}

#[derive(Deserialize)]
struct Face {
    file: String,
    family: String,
    fallback_scripts: Vec<String>,
    generic_families: Vec<String>,
}

fn main() {
    let root = repo_root();
    let fonts_dir = root.join("assets/fonts");

    println!("=== S1 complex-scripts probe ===");
    println!(
        "parley git main @ a0752c7bd, default-features = false, features = [\"std\"]{}",
        if cfg!(feature = "complex-scripts") {
            " + complex-scripts"
        } else {
            ""
        }
    );
    println!(
        "complex-scripts enabled in this build: {}",
        cfg!(feature = "complex-scripts")
    );
    println!();

    let list: FaceList = toml::from_str(
        &std::fs::read_to_string(fonts_dir.join("faces.toml")).expect("read faces.toml"),
    )
    .expect("faces.toml parses");

    // The committed set, registered and wired exactly as `s1-faces` chain C
    // does it: `append_fallbacks` from `fallback_scripts`, then
    // `append_generic_families` from `generic_families`.
    let mut cx = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    let mut ids: BTreeMap<String, FamilyId> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for face in &list.face {
        let bytes = std::fs::read(fonts_dir.join(&face.file)).expect("read face");
        let blob = Blob::new(Arc::new(bytes) as Arc<dyn AsRef<[u8]> + Send + Sync>);
        let registered = cx.collection.register_fonts(blob, None);
        assert!(
            !registered.is_empty(),
            "{}: register_fonts returned 0 families",
            face.file
        );
        for (fid, _) in &registered {
            let name = cx.collection.family_name(*fid).unwrap_or("").to_string();
            if ids.insert(name.clone(), *fid).is_none() {
                order.push(name);
            }
        }
    }
    let mut scripts: Vec<&String> = list.face.iter().flat_map(|f| &f.fallback_scripts).collect();
    scripts.sort();
    scripts.dedup();
    for script in scripts {
        let tag = script.as_bytes();
        if tag.len() != 4 {
            continue;
        }
        let key = Script::from_bytes([tag[0], tag[1], tag[2], tag[3]]);
        let mut fams: Vec<FamilyId> = Vec::new();
        for face in list
            .face
            .iter()
            .filter(|f| f.fallback_scripts.contains(script))
        {
            if let Some(id) = ids.get(&face.family)
                && !fams.contains(id)
            {
                fams.push(*id);
            }
        }
        cx.collection.append_fallbacks(key, fams.into_iter());
    }
    let mut generics: Vec<&String> = list.face.iter().flat_map(|f| &f.generic_families).collect();
    generics.sort();
    generics.dedup();
    for generic in generics {
        let Some(g) = GenericFamily::parse(generic) else {
            continue;
        };
        let mut fams: Vec<FamilyId> = Vec::new();
        for face in list
            .face
            .iter()
            .filter(|f| f.generic_families.contains(generic))
        {
            if let Some(id) = ids.get(&face.family)
                && !fams.contains(id)
            {
                fams.push(*id);
            }
        }
        cx.collection.append_generic_families(g, fams.into_iter());
    }
    let chain: Vec<FontFamilyName<'static>> = order
        .iter()
        .map(|n| FontFamilyName::Named(n.clone().into()))
        .collect();
    println!("registered, in faces.toml order: {order:?}");
    println!();

    let line = cjk_line(&root);
    measure(
        &mut cx,
        &chain,
        "bench/corpus/cjk.md, the \"Line breaking\" paragraph — THE GATE INPUT",
        &line,
    );
    measure(
        &mut cx,
        &chain,
        "Thai control — NOT a corpus input, present only to prove the flag reaches the code",
        THAI_CONTROL,
    );
}

fn measure(cx: &mut FontContext, chain: &[FontFamilyName<'static>], label: &str, text: &str) {
    println!("--- {label}");
    println!("  {text}");
    println!(
        "  {} chars, {} bytes, no ASCII space anywhere: {}",
        text.chars().count(),
        text.len(),
        !text.contains(' ')
    );
    println!();

    for width in WIDTHS {
        let mut lcx: LayoutContext<()> = LayoutContext::new();
        let mut builder = lcx.ranged_builder(cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(FONT_SIZE));
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(
            chain.to_vec().into(),
        )));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(Some(width));
        layout.align(Alignment::Start, AlignmentOptions::default());

        println!(
            "  width {width:.0}px — {} lines, layout width {:.2}, height {:.2}",
            layout.lines().count(),
            layout.width(),
            layout.height()
        );
        for (i, l) in layout.lines().enumerate() {
            let range = l.text_range();
            let mut notdef = 0usize;
            for item in l.items() {
                if let PositionedLayoutItem::GlyphRun(gr) = item {
                    notdef += gr.glyphs().filter(|g| g.id == 0).count();
                }
            }
            println!(
                "    line {i}: bytes {:>4}..{:<4} chars {:>3}  advance {:>7.2}  notdef {notdef}  {}",
                range.start,
                range.end,
                text[range.clone()].chars().count(),
                l.metrics().advance,
                &text[range.clone()],
            );
        }
        println!("    break byte offsets: {:?}", break_offsets(&layout));
    }
    println!();
}

/// Every line start after the first — the thing the two runs are compared on.
fn break_offsets(layout: &Layout<()>) -> Vec<usize> {
    layout
        .lines()
        .skip(1)
        .map(|l| l.text_range().start)
        .collect()
}

/// The long unspaced Chinese line under `## Line breaking`, read from the
/// corpus rather than transcribed, so the probe cannot drift from the gate
/// input it is about.
fn cjk_line(root: &Path) -> String {
    let src = std::fs::read_to_string(root.join("bench/corpus/cjk.md")).expect("read cjk.md");
    let mut lines = src
        .lines()
        .skip_while(|l| !l.starts_with("## Line breaking"));
    lines.next();
    lines
        .find(|l| !l.trim().is_empty())
        .expect("cjk.md has a paragraph under `## Line breaking`")
        .to_string()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("spikes/<crate> is two levels below the repo root")
        .to_path_buf()
}
