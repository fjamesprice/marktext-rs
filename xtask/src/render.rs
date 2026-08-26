//! `cargo xtask render` — **D19's pixel goldens**.
//!
//! ```text
//! bench/corpus/*.md ──► mt_md::parse ──► layout_with ──► DisplayList
//!                                                            │
//!        assets/fonts/faces.toml ──► Fonts + D16's FontTable ─┤
//!                                                             ▼
//!                                    mt_render::VelloCpuRenderer ──► Pixels
//!                                                             │
//!                              Pixels::to_png ──► bench/render-goldens/*.png
//! ```
//!
//! # What an image contains — D19 part 1, as amended twice
//!
//! Two sets, discharging two different clauses, and the gate must say which is
//! which rather than let one number stand for both.
//!
//! **Per block kind × theme**, one image of *one block*, cropped to that
//! block's extent. D10 argues against pixel goldens on the grounds that *"a
//! text diff names the block that moved and a pixel diff does not"* — and a
//! per-kind crop gives that property back, because the file that differs is
//! named `atx-heading.dark.png` and nothing has to be inferred. The list is
//! [`BlockKind::ALL`], all **19**, and a kind whose block paints nothing still
//! gets an image: a container that *starts* painting is a change worth failing
//! on.
//!
//! **Per whole input**, the four corpus files whose point is something a
//! per-kind crop cannot show — `rtl.md`, `cjk.md`, `emoji.md` and
//! `50-code-fences.md`. Bidi order, CJK line breaking, ZWJ clusters and
//! highlighted fences are all properties *between* blocks or *inside* runs, and
//! all four render in full. None is truncated and there is no height cap: the
//! tallest is `50-code-fences.md` at 6,823 px, which a `u16` target holds with
//! room to spare.
//!
//! ## The crop is `paint_bounds`, and D19 said `bounds`
//!
//! **This is the second correction D19 has taken and the more serious one.**
//! The decision as written crops *"to the block's `bounds`"*. A block's items
//! are not inside its bounds — 3,511 of them are not, across the goldens that
//! carry per-item detail — so that crop deletes exactly what
//! `mt-render`'s conditional clip exists to preserve: a `list-item`'s marker
//! sits up to 30.50 px to its left, so the image would show **no bullet**, and
//! a `table.cell`'s collapsed right and bottom borders sit *on* `max_x`/`max_y`,
//! so all 1,044 cells would show **two borders of four**. Frozen as a reference
//! image, that reads as deliberate.
//!
//! [`BlockDisplay::paint_bounds`] is the union `mt-layout` computes for this,
//! and the crop reads it. A per-kind pad would have been a guess — the
//! excursion is not constant within a kind, ranging 13.60 to 30.50 px inside
//! `list-item` alone.
//!
//! ## The source of each crop is derived, not named
//!
//! D19's first amendment already found that the file it named as carrying all
//! 19 kinds carries 5. So no filename is written here at all: the walk visits
//! every corpus input in both themes and takes, for each kind, the **first**
//! block of that kind in input order — deterministic, and correct however the
//! corpus changes. `MANIFEST.txt` records which input and which block index
//! each image came from, so the provenance is reviewable in a text diff rather
//! than by opening a PNG.
//!
//! # What compares two of them — D19 part 2
//!
//! **Exact byte equality, and no threshold.** D10's reasoning applies more
//! sharply here than where it was written: *"loosening the precision is a
//! reviewable event, not a fix"*, and **a threshold written before the first
//! divergence is a threshold that will absorb it**. If three OSes agree byte
//! for byte, the milestone's weakest instrument has become one of its
//! strongest. If they do not, the escape hatch is a per-channel max-Δ plus a
//! cap on differing pixels — and the divergence that forced it gets written
//! into `docs/M3.md` rather than becoming a number nobody can account for.
//!
//! Two things make byte-identity a fair question rather than a hopeful one.
//! [`RENDER_LEVEL`] pins the SIMD level, so the rasterizer kernel is no longer
//! a property of whichever host picked up the job; and sub-pixel geometry does
//! reach these images — a link underline is 0.39 px tall — so a coverage
//! difference shows up immediately instead of being masked by integer-aligned
//! fills.
//!
//! **No comparison crate.** `nv-flip` needs a C++ toolchain on three runners,
//! and `dssim` and `image-compare` pull families this workspace does not carry.
//! PNG encode arrives at zero cost — `png` is a default feature of `vello_cpu`
//! 0.2.0 — and [`sha256_hex`] is already in this crate for exactly this move.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use mt_layout::{BlockDisplay, BlockKind, Brush, DisplayList, TextShaper, layout_with};
use mt_render::{Frame, Pixels, RENDER_LEVEL, Renderer, VelloCpuRenderer};

use crate::layout::{
    Provenance, build_collection, build_font_table, inputs, layout_options, parse_options,
    sha256_hex, themes,
};

/// Where the images live.
const GOLDEN_DIR: &str = "bench/render-goldens";

/// The provenance sidecar. It is the half of this golden set a human can
/// actually review, and it is compared on exact equality like the images.
const MANIFEST: &str = "MANIFEST.txt";

/// Bumped when the manifest's shape changes, for `layout-golden`'s reason: a
/// reader must be able to tell a format change from a pixel change.
const FORMAT_VERSION: &str = "render-golden v1";

/// The inputs that get a whole-document image, and what each is here to show.
///
/// These are not "big files" — they are the four whose subject matter lives
/// between blocks or inside a run, where a per-kind crop shows nothing.
const WHOLE_INPUTS: [(&str, &str); 4] = [
    ("rtl.md", "bidi visual order"),
    ("cjk.md", "CJK line breaking"),
    ("emoji.md", "ZWJ and skin-tone clusters"),
    ("50-code-fences.md", "highlighted fences, S3's spans"),
];

struct Opts {
    update: bool,
    only: Option<String>,
    verbose: bool,
}

/// One image, and everything about it a reviewer needs without opening it.
struct Image {
    name: String,
    input: String,
    theme: String,
    block: Option<usize>,
    crop: mt_layout::Rect,
    png: Vec<u8>,
}

pub fn main(repo_root: &Path, args: &[String]) -> Result<i32, String> {
    let mut opts = Opts {
        update: false,
        only: None,
        verbose: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--update" => opts.update = true,
            "--verbose" => opts.verbose = true,
            "--only" => {
                opts.only = Some(
                    it.next()
                        .ok_or_else(|| "--only needs a substring".to_string())?
                        .clone(),
                );
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }

    let provenance = Provenance::read(repo_root)?;
    let mut fonts = build_collection(repo_root, &provenance)?;
    let table = build_font_table(repo_root, &provenance, &fonts)?;
    let mut renderer = VelloCpuRenderer::new(table);
    let mut shaper = TextShaper::new();

    let mut images: Vec<Image> = Vec::new();
    let mut seen_kinds: BTreeMap<(String, String), ()> = BTreeMap::new();
    /// `(input, block index, the block, list width, list height)`.
    type Pick = (String, usize, BlockDisplay, f32, f32);
    let mut chosen: BTreeMap<(String, String), Pick> = BTreeMap::new();
    let mut fallback: BTreeMap<(String, String), Pick> = BTreeMap::new();

    // **Smallest input first, and that is a decision rather than an
    // optimization.** The rule is *first block of the kind in walk order*, so
    // walk order chooses every per-kind golden's source. Ordering by size means
    // a kind is taken from the smallest corpus file that contains one — the
    // most readable source available for an image a human is meant to read —
    // instead of from whichever filename sorts first, which would put several
    // kinds in `1mb.md`. It also means the giant inputs are usually skipped
    // outright by the check below rather than laid out for nothing.
    let mut ordered = inputs(repo_root)?;
    ordered.sort_by_key(|p| {
        (
            std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX),
            p.clone(),
        )
    });

    // Which inputs the skip below passed over, so the `blank` line can name
    // them. A blank golden and an unexamined input are different claims.
    let mut skipped: Vec<String> = Vec::new();

    for theme in themes() {
        let background = Brush::resolve(theme.colors.editor_bg, Brush::default());
        for path in &ordered {
            let name = crate::layout::file_name(path);
            let is_whole = WHOLE_INPUTS.iter().any(|(f, _)| *f == name);

            // Every kind has been *seen* in this theme and this input owes no
            // whole-document one, so there is nothing here to find. Skipping
            // the layout is what keeps `5mb.md` out of a command that needs one
            // block from it, and it earns its place: the three large inputs are
            // the whole cost of this command.
            //
            // **The condition is `seen_kinds`, and `seen_kinds` counts blank
            // fallbacks too** — see the selection rule below, which says what
            // that costs.
            if !is_whole
                && BlockKind::ALL.iter().all(|k| {
                    seen_kinds.contains_key(&(k.name().to_string(), theme.name.to_string()))
                })
            {
                skipped.push(format!("{name} ({})", theme.name));
                if opts.verbose {
                    println!(
                        "skip    {name} ({}) — every kind already covered",
                        theme.name
                    );
                }
                continue;
            }

            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?
                .replace("\r\n", "\n");
            let (parse_opts, _) = parse_options(&name);
            let parsed = mt_md::parse(&text, parse_opts);
            let options = layout_options(&parse_opts, parsed.labels.clone(), &parsed.document);
            let list = layout_with(
                &parsed.document,
                &theme,
                f32::INFINITY,
                &mut fonts,
                &mut shaper,
                &options,
            )
            .map_err(|e| format!("{name} at {}: {e}", theme.name))?;

            // -- the whole-input set ---------------------------------------
            if is_whole {
                let stem = name.trim_end_matches(".md");
                let crop = mt_layout::Rect::new(0.0, 0.0, list.width, list.height);
                let png = render_crop(&mut renderer, &list, crop, background)?;
                images.push(Image {
                    name: format!("{stem}.{}.png", theme.name),
                    input: name.clone(),
                    theme: theme.name.to_string(),
                    block: None,
                    crop,
                    png,
                });
            }

            // -- the per-kind set ------------------------------------------
            //
            // **A kind is claimed by a block that paints, not by the first
            // block that carries the name.** The first draft of this took the
            // first block of each kind in walk order and gave `paragraph` the
            // one in `empty.md` — a paragraph with no items, whose golden is
            // 200 bytes of background and would have frozen "a paragraph looks
            // like nothing" as the reference image.
            //
            // Within the claiming input the richest instance wins, ties going
            // to the lowest index, so the image shows as much of the kind as
            // that input has. Containers legitimately paint nothing —
            // `table`, `table.row` and the three list kinds carry `items=0` by
            // design — so a kind that never finds a painting block falls back
            // to its first instance, and the run says out loud which kinds
            // ended up there rather than letting a blank image pass as
            // evidence.
            //
            // **The rule this actually implements is narrower than the
            // paragraph above, and the difference is worth stating.**
            // `seen_kinds` is inserted for the fallback branch as well as the
            // painting one, and the skip above fires once every kind has been
            // *seen*. So the true rule is *"a kind that never finds a painting
            // block **in the inputs walked before every kind was seen** falls
            // back to its first instance"* — the walk can stop while a richer
            // instance is still ahead of it.
            //
            // **Inert over today's corpus, and the number is the reason:** all
            // 406 blocks of the five container kinds across the 24 layout
            // goldens carry `items=0`, so there is no painting container
            // anywhere for the skip to hide. It stays a hazard rather than a
            // defect because the day a container *does* start painting — which
            // is exactly the change the ten blank goldens exist to catch — the
            // instrument could report the absence of the thing it was built to
            // detect. The `blank` line therefore names the inputs that were
            // never walked, so the claim is checkable rather than assumed.
            for kind in BlockKind::ALL {
                let key = (kind.name().to_string(), theme.name.to_string());
                if chosen.contains_key(&key) {
                    continue;
                }
                let best = list
                    .blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.kind == kind)
                    .max_by_key(|(i, b)| (b.items.len(), std::cmp::Reverse(*i)));
                let Some((i, block)) = best else {
                    continue;
                };
                let pick = (name.clone(), i, block.clone(), list.width, list.height);
                if block.items.is_empty() {
                    fallback.entry(key).or_insert(pick);
                } else {
                    chosen.insert(key, pick);
                }
                seen_kinds.insert((kind.name().to_string(), theme.name.to_string()), ());
            }
        }
    }

    // Kinds no painting block was ever found for take their first instance.
    let mut blank: Vec<String> = Vec::new();
    for (key, pick) in fallback {
        if let std::collections::btree_map::Entry::Vacant(e) = chosen.entry(key.clone()) {
            blank.push(format!("{}.{}", key.0, key.1));
            e.insert(pick);
        }
    }
    blank.sort();
    if !blank.is_empty() {
        println!(
            "blank   {} kind×theme image(s) come from a block with no items: {}",
            blank.len(),
            blank.join(", ")
        );
        println!("        Expected for containers. A leaf kind in this list is a finding.");
        if !skipped.is_empty() {
            skipped.sort();
            skipped.dedup();
            println!(
                "        Not walked (every kind already seen): {}",
                skipped.join(", ")
            );
        }
    }

    // Rendering is deferred to here so that a block discarded by a later,
    // richer instance is never rasterized at all.
    for theme in themes() {
        let background = Brush::resolve(theme.colors.editor_bg, Brush::default());
        for ((kind, theme_name), (input, i, block, w, h)) in &chosen {
            if theme_name.as_str() != theme.name {
                continue;
            }
            let crop = block.paint_bounds;
            let one = one_block(*w, *h, block);
            let png = render_crop(&mut renderer, &one, crop, background)?;
            images.push(Image {
                name: format!("{kind}.{theme_name}.png"),
                input: input.clone(),
                theme: theme_name.clone(),
                block: Some(*i),
                crop,
                png,
            });
        }
    }

    // Every kind must have an image in both themes, or the set silently covers
    // less than the gate will claim. `BlockKind::ALL` is the list, not a count
    // in prose — S4's own instruction to itself.
    let mut missing = Vec::new();
    for kind in BlockKind::ALL {
        for theme in themes() {
            if !seen_kinds.contains_key(&(kind.name().to_string(), theme.name.to_string())) {
                missing.push(format!("{}.{}", kind.name(), theme.name));
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "no corpus block produced an image for {} kind×theme pair(s): {}\n\n\
             Every one of BlockKind::ALL must be exercised. A gate that reports \
             \"19 kinds\" over a set covering fewer is the failure mode D19 was \
             written to avoid.",
            missing.len(),
            missing.join(", ")
        ));
    }

    images.sort_by(|a, b| a.name.cmp(&b.name));

    // Built from the **unfiltered** set, deliberately before `--only` narrows
    // it. `manifest_text` is a function of the whole list — it emits `images
    // {n}` and one row per image — so building it after the filter has two
    // consequences and both are bugs: `--only <x> --update` overwrites
    // `MANIFEST.txt` with a manifest describing only the images that matched,
    // silently discarding the provenance of the other 42; and `--only` in check
    // mode can never pass, because the filtered manifest never equals the
    // committed one. `layout.rs:633` guards its whole-set reporting behind
    // `only.is_none()` for exactly this reason. Nothing is skipped by the
    // filter anyway — every image is already rasterized above — so the full
    // manifest is always available to build here.
    let manifest = manifest_text(&provenance, &images);

    if let Some(sub) = &opts.only {
        images.retain(|i| i.name.contains(sub.as_str()));
        println!("only    {} image(s) matching {sub:?}", images.len());
    }

    let dir = repo_root.join(GOLDEN_DIR);
    if opts.update {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }

    let mut drifted: Vec<String> = Vec::new();
    let mut written = 0usize;

    for image in &images {
        let path = dir.join(&image.name);
        let existing = match std::fs::read(&path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        if opts.update {
            if existing.as_deref() == Some(image.png.as_slice()) {
                if opts.verbose {
                    println!(
                        "ok      {} ({} bytes, unchanged)",
                        image.name,
                        image.png.len()
                    );
                }
            } else {
                std::fs::write(&path, &image.png)
                    .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
                println!("wrote   {} ({} bytes)", image.name, image.png.len());
                written += 1;
            }
        } else {
            match existing {
                None => {
                    println!("MISSING {}", image.name);
                    drifted.push(image.name.clone());
                }
                Some(bytes) if bytes != image.png => {
                    println!(
                        "DIFFERS {} (golden {} bytes, measured {})",
                        image.name,
                        bytes.len(),
                        image.png.len()
                    );
                    for line in describe_divergence(&bytes, &image.png) {
                        println!("        {line}");
                    }
                    drifted.push(image.name.clone());
                }
                Some(_) => {
                    if opts.verbose {
                        println!("ok      {} ({} bytes)", image.name, image.png.len());
                    }
                }
            }
        }
    }

    // The manifest is compared like an image, and it is the one a review can
    // actually read: a crop rect that moved shows up here as a text diff even
    // when the pixels happen to hash the same.
    let manifest_path = dir.join(MANIFEST);
    let existing_manifest = std::fs::read_to_string(&manifest_path)
        .ok()
        .map(|s| s.replace("\r\n", "\n"));
    if opts.update {
        if existing_manifest.as_deref() != Some(manifest.as_str()) {
            std::fs::write(&manifest_path, &manifest)
                .map_err(|e| format!("cannot write {}: {e}", manifest_path.display()))?;
            println!("wrote   {MANIFEST} ({} bytes)", manifest.len());
            written += 1;
        }
    } else if existing_manifest.as_deref() != Some(manifest.as_str()) {
        println!("DIFFERS {MANIFEST}");
        drifted.push(MANIFEST.to_string());
    }

    println!();
    println!(
        "images  {} across {} kind(s) and {} whole input(s)",
        images.len(),
        BlockKind::ALL.len(),
        WHOLE_INPUTS.len()
    );
    println!(
        "level   {RENDER_LEVEL:?}, threads {}",
        mt_render::NUM_THREADS
    );

    if opts.update {
        println!();
        println!(
            "{written} image(s) rewritten, {} checked.",
            images.len() + 1
        );
        println!(
            "A golden update is a REVIEWABLE EVENT. These are pixels: read the \
             manifest diff first, and open any image whose crop rect did not move."
        );
        return Ok(0);
    }
    if drifted.is_empty() {
        println!();
        println!("{} image(s) and the manifest match exactly.", images.len());
        return Ok(0);
    }
    Err(format!(
        "{} golden(s) differ: {}\n\n\
         Comparison is EXACT and there is no threshold — D19 part 2. If this is \
         a platform divergence rather than a change in this repository, that is \
         the finding the decision asks for: record it in docs/M3.md before \
         loosening anything, because a threshold written to absorb a divergence \
         absorbs the next one too.",
        drifted.len(),
        drifted.join(", ")
    ))
}

/// A one-block list, so a per-kind image shows that kind and not its neighbours.
///
/// Rendering the whole document and cropping would let an adjacent block bleed
/// into the frame — for a table cell, several of them — and the property D19
/// buys with a per-kind crop is precisely that *the file that differs names the
/// thing that changed*. A neighbour's glyph inside `atx-heading.dark.png`
/// spends that property for nothing.
fn one_block(width: f32, height: f32, block: &BlockDisplay) -> DisplayList {
    DisplayList {
        width,
        height,
        blocks: vec![block.clone()],
    }
}

/// Render `crop` of `list` into a target exactly that size.
///
/// The viewport *is* the crop, which is why this needs no cropping code: D18
/// already has `mt-render` translating by the scroll offset and culling to a
/// caller-supplied rect, so asking for the block's own extent as the viewport
/// produces the cropped image directly — and exercises the cull path while
/// doing it.
fn render_crop(
    renderer: &mut VelloCpuRenderer,
    list: &DisplayList,
    crop: mt_layout::Rect,
    background: Brush,
) -> Result<Vec<u8>, String> {
    // The **lower** bound gets the same treatment as the upper one below, and
    // for the same reason the upper one gives. `.max(1.0)` would turn a
    // zero-area, negative or NaN `paint_bounds` into a 1x1 PNG that compares
    // equal to itself forever — a golden that has quietly stopped testing
    // anything. The asymmetry was the finding: one line refused a silent cap
    // and the line above it took one.
    //
    // Unreachable over today's corpus — all 46 committed crops have positive
    // area, and the narrowest display list in the corpus (`empty.md`, one
    // paragraph at 650.00 x 25.60) is not close. The first block that trips
    // this is a finding about `paint_bounds`, not a size to round up.
    if crop.width.is_nan() || crop.height.is_nan() || crop.width <= 0.0 || crop.height <= 0.0 {
        return Err(format!(
            "crop {}x{} has no positive area; a sub-pixel, negative or NaN              paint_bounds would raster as a 1x1 image that compares equal to              itself forever, and that is a finding about the display list              rather than a number to round up",
            crop.width, crop.height
        ));
    }
    let w = crop.width.ceil();
    let h = crop.height.ceil();
    if w > f32::from(u16::MAX) || h > f32::from(u16::MAX) {
        return Err(format!(
            "crop {w}×{h} exceeds a u16 target; no corpus block does this today, \
             and the first one that does needs a decision rather than a silent cap"
        ));
    }
    let mut target = Pixels::new(w as u16, h as u16);
    let frame = Frame {
        viewport: crop,
        background,
    };
    renderer
        .render(list, &frame, &mut target)
        .map_err(|e| format!("render: {e}"))?;
    target.to_png()
}

/// How far apart two encoded PNGs are, in pixels and per channel.
///
/// `DIFFERS` on its own reports *"not equal"*, and that is precisely the number
/// D19 refuses to let a threshold be built on: *"a threshold written before the
/// first divergence is a threshold that will absorb it."* A divergence nobody
/// has measured is a divergence nobody can argue about, and the decision
/// requires the first one to be **recorded in `docs/M3.md`** before anything
/// loosens. This is what makes that record a measurement instead of an
/// adjective.
///
/// **It loosens nothing.** The comparison above stays byte-exact and still
/// fails; this runs only after it has. The reporting shape is the one
/// `report_drift` established (`layout.rs:1438-1464`): the count, the worst
/// case, and the first offenders by position.
fn describe_divergence(golden: &[u8], measured: &[u8]) -> Vec<String> {
    /// Enough offenders to see whether a divergence is localized or spread,
    /// and few enough that a red CI log stays readable.
    const FIRST_N: usize = 4;

    let (a, b) = match (Pixels::from_png(golden), Pixels::from_png(measured)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return vec!["could not decode both PNGs, so no delta is reported".to_string()],
    };
    if a.width() != b.width() || a.height() != b.height() {
        return vec![format!(
            "dimensions differ: golden {}x{}, measured {}x{}              — that is a geometry change, not a raster one",
            a.width(),
            a.height(),
            b.width(),
            b.height()
        )];
    }

    let (ga, gb) = (a.data(), b.data());
    let total = ga.len() / 4;
    let mut differing = 0usize;
    let mut max_delta = 0u8;
    let mut worst = (0u16, 0u16);
    let mut first: Vec<String> = Vec::new();

    for i in 0..total {
        let (pa, pb) = (&ga[i * 4..i * 4 + 4], &gb[i * 4..i * 4 + 4]);
        if pa == pb {
            continue;
        }
        differing += 1;
        let delta = (0..4).map(|c| pa[c].abs_diff(pb[c])).max().unwrap_or(0);
        let x = (i % usize::from(a.width())) as u16;
        let y = (i / usize::from(a.width())) as u16;
        if delta > max_delta {
            max_delta = delta;
            worst = (x, y);
        }
        if first.len() < FIRST_N {
            first.push(format!(
                "({x},{y}) {:02x}{:02x}{:02x}{:02x} -> {:02x}{:02x}{:02x}{:02x}",
                pa[0], pa[1], pa[2], pa[3], pb[0], pb[1], pb[2], pb[3]
            ));
        }
    }

    if differing == 0 {
        // Same pixels, different bytes: the encoder changed, not the raster.
        return vec![
            "every pixel is identical — the PNG encoding differs, not the image".to_string(),
        ];
    }

    let pct = 100.0 * differing as f64 / total as f64;
    vec![
        format!(
            "{differing} of {total} px differ ({pct:.4}%), max per-channel delta {max_delta} at {:?}",
            worst
        ),
        format!("first {}: {}", first.len(), first.join("  ")),
    ]
}

fn manifest_text(provenance: &Provenance, images: &[Image]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{FORMAT_VERSION}");
    let _ = writeln!(out, "parley         {}", provenance.parley_rev);
    let _ = writeln!(
        out,
        "faces          {} rev {}",
        provenance.faces_sha256, provenance.faces_revision
    );
    let _ = writeln!(out, "level          {RENDER_LEVEL:?}");
    let _ = writeln!(out, "threads        {}", mt_render::NUM_THREADS);
    let _ = writeln!(out, "images         {}", images.len());
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "# Every image is one block of one kind, cropped to that block's \
         `paint_bounds` —"
    );
    let _ = writeln!(
        out,
        "# NOT to `bounds`, which would cut a list marker and half a table \
         cell's borders."
    );
    let _ = writeln!(
        out,
        "# The four whole-input images carry what a per-kind crop cannot show."
    );
    let _ = writeln!(out);
    for image in images {
        let block = match image.block {
            Some(i) => format!("block {i}"),
            None => "whole".to_string(),
        };
        let _ = writeln!(
            out,
            "{:<34} {:<20} {:<14} {:<10} crop=[{} {} {} {}] bytes={} sha256={}",
            image.name,
            image.input,
            image.theme,
            block,
            f2(image.crop.x),
            f2(image.crop.y),
            f2(image.crop.width),
            f2(image.crop.height),
            image.png.len(),
            sha256_hex(&image.png),
        );
    }
    out
}

/// Two decimals, the same form the layout goldens print, so a crop rect here
/// can be compared to a `bounds=` there by eye.
fn f2(v: f32) -> String {
    format!("{v:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        crate::repo_root()
    }

    /// Every kind and every whole input has exactly two images, and no image is
    /// orphaned.
    ///
    /// **The direction that was missing.** The `missing` check above
    /// (`BlockKind::ALL` x `themes()`) is exhaustive in the direction of *too
    /// few produced* — it errors if the walk fails to claim a kind. Nothing was
    /// exhaustive in the other direction: the compare loop iterates the images
    /// the run *measured* and opens each expected path, so a PNG sitting in
    /// `bench/render-goldens/` that the run no longer produces is never opened,
    /// never named and never failed on. Rename a `BlockKind`, or drop one, and
    /// the stale image stays in the tree while CI reports green over a smaller
    /// set — which is the same failure the `missing` check's own comment calls
    /// *"the failure mode D19 was written to avoid"*, arriving from the side it
    /// did not guard.
    ///
    /// `MANIFEST.txt` narrows the blast radius (it carries `images 46` and is
    /// compared exactly), but it is regenerated from the same vector, so it
    /// inherits the same blind spot for a file that exists only on disk.
    ///
    /// This is `layout.rs`'s `the_committed_goldens_are_exactly_one_per_input_per_theme`
    /// ported to the image set, and it is the first test this module has had.
    #[test]
    fn the_committed_images_are_exactly_one_per_kind_and_whole_input_per_theme() {
        let mut expected: Vec<String> = Vec::new();
        for theme in themes() {
            for kind in BlockKind::ALL {
                expected.push(format!("{}.{}.png", kind.name(), theme.name));
            }
            for (input, _why) in WHOLE_INPUTS {
                expected.push(format!(
                    "{}.{}.png",
                    input.trim_end_matches(".md"),
                    theme.name
                ));
            }
        }
        expected.sort();

        let dir = root().join(GOLDEN_DIR);
        let mut found: Vec<String> = std::fs::read_dir(&dir)
            .expect("bench/render-goldens exists")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".png"))
            .collect();
        found.sort();
        assert_eq!(found, expected);
    }

    /// The count the gate reports is the count the set actually has.
    ///
    /// 19 kinds x 2 themes + 4 whole inputs x 2 themes = 46. Written as the
    /// arithmetic rather than as `46` so that adding a `BlockKind` moves it.
    #[test]
    fn the_image_count_is_the_arithmetic_and_not_a_number_in_prose() {
        let expected = (BlockKind::ALL.len() + WHOLE_INPUTS.len()) * themes().len();
        let dir = root().join(GOLDEN_DIR);
        let found = std::fs::read_dir(&dir)
            .expect("bench/render-goldens exists")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".png"))
            .count();
        assert_eq!(
            found,
            expected,
            "{} kinds, {} whole inputs, {} themes",
            BlockKind::ALL.len(),
            WHOLE_INPUTS.len(),
            themes().len()
        );
    }
}
