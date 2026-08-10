//! E1 secondary questions 3 and 4 — the font-strategy probe that feeds D7.
//!
//! `mt-layout` is one of the crates RUST-REWRITE-PLAN.md §1 marks "No I/O", and
//! parley's default feature set is `["system"]`, which walks the filesystem to
//! enumerate installed faces. This binary asks three things that D7 needs
//! answered before it can choose between "bundle fonts" and "use the system":
//!
//!   (a) does parley build and lay out at all with `default-features = false`?
//!       The Cargo.toml is the experiment; reaching `main` is the result.
//!   (b) can a `FontContext` be assembled from in-memory bytes with the system
//!       backend switched off, so that no path is ever opened?
//!   (c) what does MarkText's own bundled font directory actually give us —
//!       Open Sans ships as `.woff`, which is a *compressed container*, not a
//!       font file skrifa can parse.
//!
//! and then question 4: with only those bundled faces registered, can Arabic
//! and Hebrew be rendered at all, or does an Arabic-capable face have to come
//! from the system? The answer is counted in `.notdef` glyphs, not asserted.
//!
//! This is the **parley 0.11.0** arm.

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

const ARM: &str = "parley 0.11.0 (crates.io), default-features = false, features = [\"std\"]";

/// MarkText's own bundled faces, from the Electron app this project is
/// rewriting. The extensions are the point of the probe.
const FONT_DIR: &str = r"C:\Dev\marktext\packages\muya\src\assets\styles\fonts";

const SAMPLES: &[(&str, &str)] = &[
    ("latin", "The quick brown fox 12345"),
    ("arabic", "هذا نص عربي"),
    ("hebrew", "שלום עולם"),
];

fn main() {
    println!("=== E1 font-strategy probe — arm: {ARM} ===");
    println!();

    // (a) We are running, therefore parley compiled and linked without the
    //     `system` feature. Say so, then prove it lays text out below.
    println!("(a) parley with default-features = false, features = [\"std\"]: BUILDS and RUNS.");
    println!("    (this binary is that build; see this crate's Cargo.toml)");
    println!();

    // (b) A collection with the system backend explicitly disabled. Nothing
    //     below this line opens a font path except our own explicit reads of
    //     MarkText's bundled files, which are the application's own assets and
    //     in the real crate would be `include_bytes!`.
    println!("(b) FontContext assembled with CollectionOptions {{ system_fonts: false, .. }}:");
    let mut font_cx = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: SourceCache::default(),
    };
    println!("    Collection::new(CollectionOptions {{ shared: false, system_fonts: false }})");
    println!(
        "    families visible before registering anything: {}",
        family_count(&mut font_cx)
    );

    // (c) Register MarkText's bundled faces, one file at a time, and report
    //     what each container type yields.
    println!();
    println!("(c) registering MarkText's bundled faces from {FONT_DIR}");
    println!(
        "    API: Collection::register_fonts(Blob<u8>, Option<FontInfoOverride>) -> Vec<(FamilyId, Vec<FontInfo>)>"
    );
    let mut registered: Vec<String> = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(FONT_DIR)
        .unwrap_or_else(|e| panic!("cannot read {FONT_DIR}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("ttf" | "otf" | "woff" | "woff2")
            )
        })
        .collect();
    files.sort();

    println!(
        "    {:<48} {:>8}  {:<14} register_fonts result",
        "file", "bytes", "magic (ascii+hex)"
    );
    for path in &files {
        // In `mt-layout` this would be `include_bytes!`, not a read; the read
        // is here because a spike must not vendor 2 MB of fonts into the repo.
        let bytes = std::fs::read(path).expect("read font file");
        // The first four bytes are the container tag, and they are the whole
        // point of question 3(c): `wOFF` is a *compressed archive* of an
        // OpenType font, not an OpenType font.
        let magic: String = {
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
        };
        let blob = Blob::new(std::sync::Arc::new(bytes.clone()) as _);
        let result = font_cx.collection.register_fonts(blob, None);
        let summary = if result.is_empty() {
            "REJECTED — 0 families".to_string()
        } else {
            let mut s = String::new();
            for (fid, infos) in &result {
                let name = font_cx
                    .collection
                    .family_name(*fid)
                    .unwrap_or("<unnamed>")
                    .to_string();
                s.push_str(&format!("{name:?} x{} ", infos.len()));
                registered.push(name);
            }
            format!("OK — {s}")
        };
        println!(
            "    {:<48} {:>8}  {:<14} {}",
            path.file_name().unwrap().to_string_lossy(),
            bytes.len(),
            magic,
            summary
        );
    }

    registered.sort();
    registered.dedup();
    println!();
    println!("    families now registered (no system enumeration): {registered:?}");
    println!(
        "    total families in collection: {}",
        family_count(&mut font_cx)
    );

    // (4) Can Arabic and Hebrew render from those faces alone?
    println!();
    println!("(4) laying out with ONLY the bundled faces available (no system fonts):");
    let mut layout_cx: LayoutContext<()> = LayoutContext::new();
    let family = registered
        .first()
        .cloned()
        .unwrap_or_else(|| "DejaVu Sans Mono".to_string());
    println!("    font stack forced to {family:?}");
    println!(
        "    {:<8} {:<28} {:>7} {:>7} {:>8}  verdict",
        "sample", "text", "glyphs", "notdef", "width"
    );
    for (name, text) in SAMPLES {
        let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(16.0));
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(&family)));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());

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
        let verdict = if glyphs == 0 {
            "NO GLYPHS AT ALL"
        } else if notdef == 0 {
            "renders"
        } else if notdef == glyphs {
            "ALL TOFU"
        } else {
            "partial tofu"
        };
        println!(
            "    {:<8} {:<28} {:>7} {:>7} {:>8.2}  {}",
            name,
            text,
            glyphs,
            notdef,
            layout.width(),
            verdict
        );
    }

    println!();
    println!("=== complex-scripts ===");
    println!(
        "  this crate re-exports parley's `complex-scripts` as its own feature; enabled here: {}",
        cfg!(feature = "complex-scripts")
    );
    println!("  the dependency cost is a `cargo tree` question, not a runtime one —");
    println!("  the results file records the diff with and without.");
}

fn family_count(cx: &mut FontContext) -> usize {
    cx.collection.family_names().count()
}
