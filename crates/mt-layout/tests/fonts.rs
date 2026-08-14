//! The font seam against the real files — M3 §5 D7, D8, and risk M3-R10.
//!
//! # Why this is an integration test and not a unit test
//!
//! `cargo xtask deps` greps `crates/mt-layout/src/` for `std::fs::` and fails
//! the build if it finds it, including inside `#[cfg(test)]`. That check is
//! the executable form of §1's "No I/O", and its own failure message names the
//! way out: *"If a test genuinely needs a fixture, move it to an integration
//! test."* `tests/` is not scanned, and the split is the right one — the
//! **library** must be incapable of reading a font, and the **test** has to
//! read twelve of them to prove the library does the right thing with the
//! bytes.
//!
//! # What is asserted here that cannot be asserted anywhere else
//!
//! 1. Every file `faces.toml` names exists, at the declared length and the
//!    declared SHA-256. A face list that has drifted from its fonts is worse
//!    than no face list, because D10's goldens would inherit the drift
//!    silently.
//! 2. Registering all twelve succeeds and the family names skrifa reports
//!    match the declared ones.
//! 3. **The `.woff` trap fires.** MarkText's own bundled Open Sans is
//!    registered here on purpose, and the assertion is that it comes back a
//!    *hard error* — D7 fact 1, asserted rather than quoted.
//! 4. **D8's `FontId` round trip.** Text that forces three different faces is
//!    laid out, and each run's `FontId` is checked against the face that
//!    actually covers that script.

use std::path::{Path, PathBuf};

use mt_layout::display::DisplayItem;
use mt_layout::fonts::{Face, FaceList, FaceRole, FaceStyle, FontError, Fonts};
use mt_layout::text::{TextRequest, TextShaper};

/// MarkText's own bundled font directory in the reference clone. Read-only,
/// and used for exactly one thing: re-running the `.woff` trap on a real
/// `.woff`. Absent on a machine without the clone, in which case that one test
/// reports why it skipped rather than failing — the finding stands from S0 and
/// the equivalent hard-error path is covered by a unit test on garbage bytes.
const MARKTEXT_FONT_DIR: &str = r"C:\Dev\marktext\packages\muya\src\assets\styles\fonts";

fn repo_root() -> PathBuf {
    // `crates/mt-layout` -> `crates` -> the repo root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<crate> is two levels below the repo root")
        .to_path_buf()
}

fn fonts_dir() -> PathBuf {
    repo_root().join("assets/fonts")
}

fn read_face(face: &Face) -> Vec<u8> {
    let path = fonts_dir().join(&face.file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The committed set, registered and wired exactly as a shell must do it.
fn bundled_fonts() -> (FaceList, Fonts) {
    let list = FaceList::bundled();
    let mut fonts = Fonts::new();
    for face in &list.faces {
        fonts
            .register_face(face, read_face(face))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    fonts.wire(&list).expect("wiring the committed set");
    (list, fonts)
}

// ---------------------------------------------------------------------------
// 1. The list agrees with the files
// ---------------------------------------------------------------------------

#[test]
fn every_file_the_face_list_names_exists_at_the_declared_length_and_hash() {
    let list = FaceList::bundled();
    let mut total = 0u64;
    for face in &list.faces {
        let path = fonts_dir().join(&face.file);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(
            bytes.len() as u64,
            face.bytes,
            "{}: {} bytes on disk, faces.toml declares {}",
            face.file,
            bytes.len(),
            face.bytes
        );
        assert_eq!(
            sha256_hex(&bytes),
            face.sha256,
            "{}: the file's SHA-256 disagrees with faces.toml",
            face.file
        );
        total += bytes.len() as u64;
    }
    assert_eq!(total, list.meta.total_font_bytes);
}

/// The one derived entry is a **test fixture, not a shipped asset**, and the
/// three `derived_*` keys are what make committing it defensible. Asserted
/// here because a future face added without them would otherwise only be
/// caught by someone reading the file.
#[test]
fn exactly_one_face_is_derived_and_it_is_re_derivable() {
    let list = FaceList::bundled();
    let derived: Vec<&Face> = list.faces.iter().filter(|f| f.derived).collect();
    assert_eq!(derived.len(), 1, "the CJK subset is the only derived entry");
    let cjk = derived[0];
    assert_eq!(cjk.family, "Noto Sans CJK SC");
    assert!(cjk.derived_from.contains("sha256"));
    assert!(cjk.derived_command.contains("fontTools.subset"));
}

// ---------------------------------------------------------------------------
// 2. Registration
// ---------------------------------------------------------------------------

#[test]
fn registering_the_committed_set_succeeds_and_the_families_match() {
    let (list, fonts) = bundled_fonts();
    assert_eq!(fonts.len(), list.faces.len());
    for (i, face) in list.faces.iter().enumerate() {
        let id = mt_layout::FontId::from_index(i as u32);
        assert_eq!(
            fonts.family_name(id),
            Some(face.family.as_str()),
            "{}: registered family does not match faces.toml",
            face.file
        );
        assert_eq!(fonts.file_name(id), Some(face.file.as_str()));
    }
    // Twelve files, five families — the four DejaVu faces are one family and
    // the four Open Sans faces are another. Recorded because the fallback
    // chain resolves on family names, so the *family* count is what the chain
    // is long.
    let mut families: Vec<&str> = fonts.ids().filter_map(|id| fonts.family_name(id)).collect();
    families.dedup();
    assert_eq!(
        families,
        [
            "Open Sans",
            "Noto Sans Arabic",
            "Noto Emoji",
            "Noto Sans CJK SC",
            "DejaVu Sans Mono",
        ]
    );
}

/// The declared weight and style are what a shell asks for, so a face list
/// that lies about them selects the wrong file. The unit tests cannot check
/// this — it needs the font's own tables.
#[test]
fn the_declared_weights_and_styles_select_distinct_files() {
    let list = FaceList::bundled();
    let mut seen: Vec<(&str, u16, FaceStyle)> = Vec::new();
    for face in &list.faces {
        let key = (face.family.as_str(), face.weight, face.style);
        assert!(
            !seen.contains(&key),
            "{}: {:?} duplicates another entry's family/weight/style, so one of them is \
             unreachable",
            face.file,
            key
        );
        seen.push(key);
    }
}

/// D7 splits the bundled set's two jobs, and `role` is where the split is
/// written down. Both theme stacks must name a family some face claims,
/// or the stack's first entry resolves to nothing.
#[test]
fn every_theme_stack_family_is_provided_by_some_registered_face() {
    let (list, _fonts) = bundled_fonts();
    let theme = mt_layout::Theme::muya_default();
    for (stack, role) in [
        (&theme.fonts.body, FaceRole::Body),
        (&theme.fonts.code, FaceRole::Code),
        (&theme.fonts.inline_code, FaceRole::Code),
    ] {
        let first = stack.first().expect("a non-empty stack");
        let face = list
            .faces
            .iter()
            .find(|f| &f.family == first)
            .unwrap_or_else(|| panic!("no committed face provides {first:?}"));
        assert_eq!(
            face.role, role,
            "{first:?} is named by a {role:?} stack but faces.toml calls it {:?}",
            face.role
        );
    }
}

// ---------------------------------------------------------------------------
// 3. M3-R10's trap
// ---------------------------------------------------------------------------

/// **D7 fact 1, asserted rather than quoted.**
///
/// skrifa rejects WOFF1 *silently*: `register_fonts` returns an empty `Vec`,
/// no error and no panic. All eight of MarkText's bundled Open Sans files are
/// WOFF1, so a port that treated an empty result as "nothing to do" would lay
/// the whole document out in the last-resort font with no diagnostic anywhere.
/// This test is the guarantee that `mt-layout` cannot do that.
#[test]
fn marktexts_own_woff_open_sans_is_a_hard_error() {
    let dir = Path::new(MARKTEXT_FONT_DIR);
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!(
            "SKIPPED: {MARKTEXT_FONT_DIR} is not present on this machine. The equivalent \
             hard-error path is covered by `garbage_bytes_are_a_hard_error_naming_the_file` in \
             src/fonts.rs."
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
        eprintln!("SKIPPED: no .woff files under {MARKTEXT_FONT_DIR}.");
        return;
    };

    let bytes = std::fs::read(path).expect("read the woff");
    assert_eq!(
        &bytes[..4],
        b"wOFF",
        "expected a WOFF1 container; the trap is about that magic"
    );

    let file = path.file_name().unwrap_or_default().to_string_lossy();
    let face = Face {
        file: file.to_string(),
        family: "Open Sans".into(),
        subfamily: "Regular".into(),
        weight: 400,
        style: FaceStyle::Normal,
        variable: false,
        axes: vec![],
        scripts: vec!["Latn".into()],
        fallback_scripts: vec!["Latn".into()],
        generic_families: vec![],
        role: FaceRole::Body,
        derived: false,
        derived_from: String::new(),
        derived_command: String::new(),
        bytes: bytes.len() as u64,
        sha256: String::new(),
    };

    let mut fonts = Fonts::new();
    let err = fonts
        .register_face(&face, bytes)
        .expect_err("registering a WOFF1 file must be a hard error, not an empty success");
    assert_eq!(
        err,
        FontError::Rejected {
            file: file.to_string()
        }
    );
    assert!(err.to_string().contains(file.as_ref()));
    assert!(fonts.is_empty(), "a rejected face must not be counted");
}

// ---------------------------------------------------------------------------
// 4. D8's FontId round trip
// ---------------------------------------------------------------------------

/// **The uncertainty §9 records as open, closed.**
///
/// > *"D8's `FontId` mapping is unbuilt. Resolving parley's font handle back to
/// > a face registered under D7 is the one part of the seam no spike has
/// > exercised. S1 either builds it or falls back to carrying the handle
/// > across the seam, and says which."*
///
/// It is built. This test lays out one string that no single committed face
/// covers — Latin, then Arabic, then an emoji, then a CJK ideograph — and
/// asserts that every run resolves to the face that actually covers its
/// script. Three separate mechanisms have to work for it to pass: named family
/// selection (Latin → Open Sans), script fallback (Arabic → Noto Sans Arabic,
/// CJK → the Noto Sans CJK subset) and generic-family selection (emoji → Noto
/// Emoji, which script fallback alone never reaches).
///
/// It is deliberately not a pointer-identity test. See `Fonts::by_blob`: the
/// key is `Blob::id()`, a `u64` from an atomic counter that `Blob::clone`
/// copies rather than regenerates, so the mapping holds across the clones
/// fontique performs on the way from `register_fonts` to a shaped run.
#[test]
fn every_run_resolves_to_the_face_that_covers_it() {
    let (_list, mut fonts) = bundled_fonts();
    let mut shaper = TextShaper::new();

    // The theme's own body stack, verbatim, so the test exercises the chain a
    // real document gets rather than a hand-built one.
    let theme = mt_layout::Theme::muya_default();
    let text = "Hello \u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629} \u{1F600} \u{4E2D}\u{6587}";
    let request = TextRequest::new(
        text,
        &theme.fonts.body,
        theme.metrics.font_size_px,
        theme.metrics.line_height,
    );
    let shaped = shaper.shape(&mut fonts, &request);

    let mut items = Vec::new();
    shaped
        .emit(&fonts, 0.0, 0.0, &mut items)
        .expect("every run must resolve to a registered face");

    let mut seen: Vec<(String, &str)> = Vec::new();
    let mut tofu = 0usize;
    for item in &items {
        let DisplayItem::Glyphs(run) = item else {
            continue;
        };
        let family = fonts
            .family_name(run.font)
            .expect("the id must name a registered face");
        seen.push((text[run.text_range.clone()].to_string(), family));
        tofu += run.glyphs.iter().filter(|g| g.id == 0).count();
    }

    assert!(
        seen.len() >= 4,
        "expected at least four runs (Latin, Arabic, emoji, CJK), got {seen:?}"
    );
    assert_eq!(
        tofu, 0,
        "the committed set must cover this string: {seen:?}"
    );

    let family_for = |needle: &str| -> &str {
        seen.iter()
            .find(|(t, _)| t.contains(needle))
            .map(|(_, f)| *f)
            .unwrap_or_else(|| panic!("no run covers {needle:?}; runs were {seen:?}"))
    };
    assert_eq!(family_for("Hello"), "Open Sans", "Latin → the body family");
    assert_eq!(
        family_for("\u{627}"),
        "Noto Sans Arabic",
        "Arabic → script fallback, and NOT the monospace code face"
    );
    assert_eq!(
        family_for("\u{1F600}"),
        "Noto Emoji",
        "emoji → GenericFamily::Emoji, which script fallback alone never reaches"
    );
    assert_eq!(
        family_for("\u{4E2D}"),
        "Noto Sans CJK SC",
        "CJK → script fallback to the corpus subset"
    );
}

/// The same mapping, from the other direction: a face that is *not* used must
/// not be resolved to. Latin-only text may only produce Open Sans runs.
#[test]
fn a_run_never_resolves_to_a_face_that_did_not_shape_it() {
    let (_list, mut fonts) = bundled_fonts();
    let mut shaper = TextShaper::new();
    let theme = mt_layout::Theme::muya_default();
    let request = TextRequest::new(
        "plain latin text",
        &theme.fonts.body,
        theme.metrics.font_size_px,
        theme.metrics.line_height,
    );
    let shaped = shaper.shape(&mut fonts, &request);
    let mut items = Vec::new();
    shaped.emit(&fonts, 0.0, 0.0, &mut items).unwrap();
    let families: Vec<&str> = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(r) => fonts.family_name(r.font),
            _ => None,
        })
        .collect();
    assert!(!families.is_empty());
    assert!(
        families.iter().all(|f| *f == "Open Sans"),
        "Latin text reached {families:?}"
    );
}

/// The code stack has to reach DejaVu Sans Mono rather than the body face, or
/// every code block in every golden is proportional.
#[test]
fn the_code_stack_reaches_the_monospace_face() {
    let (_list, mut fonts) = bundled_fonts();
    let mut shaper = TextShaper::new();
    let theme = mt_layout::Theme::muya_default();
    let request = TextRequest::new(
        "fn main() {}",
        &theme.fonts.code,
        theme.metrics.font_size_px * theme.code_block.font_size_pct,
        theme.code_block.line_height,
    );
    let shaped = shaper.shape(&mut fonts, &request);
    let mut items = Vec::new();
    shaped.emit(&fonts, 0.0, 0.0, &mut items).unwrap();
    for item in &items {
        if let DisplayItem::Glyphs(run) = item {
            assert_eq!(fonts.family_name(run.font), Some("DejaVu Sans Mono"));
        }
    }
}

/// The walk's geometry, checked against the one invariant that does not depend
/// on the pin: translating the origin translates every coordinate by the same
/// amount, and nothing else changes.
#[test]
fn the_walk_translates_and_changes_nothing_else() {
    let (_list, mut fonts) = bundled_fonts();
    let mut shaper = TextShaper::new();
    let theme = mt_layout::Theme::muya_default();
    let mut request = TextRequest::new(
        "the quick brown fox jumps over the lazy dog, twice, so that it wraps at this width",
        &theme.fonts.body,
        theme.metrics.font_size_px,
        theme.metrics.line_height,
    );
    request.max_width = Some(200.0);
    let shaped = shaper.shape(&mut fonts, &request);
    assert!(shaped.line_count() > 1, "the text should have wrapped");
    assert!(!shaped.break_offsets().is_empty());

    let mut at_origin = Vec::new();
    shaped.emit(&fonts, 0.0, 0.0, &mut at_origin).unwrap();
    let mut moved = Vec::new();
    shaped.emit(&fonts, 100.0, 50.0, &mut moved).unwrap();
    assert_eq!(at_origin.len(), moved.len());

    for (a, b) in at_origin.iter().zip(&moved) {
        let (DisplayItem::Glyphs(a), DisplayItem::Glyphs(b)) = (a, b) else {
            continue;
        };
        assert_eq!(a.font, b.font);
        assert_eq!(a.font_size, b.font_size);
        assert_eq!(a.is_rtl, b.is_rtl);
        assert_eq!(a.text_range, b.text_range);
        assert_eq!(a.advance, b.advance);
        assert_eq!(a.offset + 100.0, b.offset);
        assert_eq!(a.baseline + 50.0, b.baseline);
        assert_eq!(a.glyphs.len(), b.glyphs.len());
        for (ga, gb) in a.glyphs.iter().zip(&b.glyphs) {
            assert_eq!(ga.id, gb.id);
            assert_eq!(ga.x + 100.0, gb.x);
            assert_eq!(ga.y + 50.0, gb.y);
        }
    }
}

/// RTL is a gate clause, and `is_rtl` is the only thing on the neutral run
/// that reports it. Hebrew as well as Arabic, because D7 fact 2 was that the
/// original bundled set rendered Arabic and not Hebrew.
#[test]
fn rtl_runs_are_flagged_and_hebrew_is_covered() {
    let (_list, mut fonts) = bundled_fonts();
    let mut shaper = TextShaper::new();
    let theme = mt_layout::Theme::muya_default();
    // "shalom" in Hebrew, then "salam" in Arabic.
    let text = "\u{5E9}\u{5DC}\u{5D5}\u{5DD} \u{633}\u{644}\u{627}\u{645}";
    let request = TextRequest::new(
        text,
        &theme.fonts.body,
        theme.metrics.font_size_px,
        theme.metrics.line_height,
    );
    let shaped = shaper.shape(&mut fonts, &request);
    let mut items = Vec::new();
    shaped.emit(&fonts, 0.0, 0.0, &mut items).unwrap();

    let runs: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            DisplayItem::Glyphs(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|r| r.is_rtl), "every run should be RTL");
    let tofu: usize = runs
        .iter()
        .map(|r| r.glyphs.iter().filter(|g| g.id == 0).count())
        .sum();
    assert_eq!(tofu, 0, "Hebrew and Arabic must both be covered");
    // Hebrew comes from Open Sans (re-sourcing upstream discharged the Hebrew
    // half of M3-R10 for free); Arabic from the face added for it.
    let families: Vec<&str> = runs
        .iter()
        .filter_map(|r| fonts.family_name(r.font))
        .collect();
    assert!(families.contains(&"Open Sans"), "{families:?}");
    assert!(families.contains(&"Noto Sans Arabic"), "{families:?}");
}

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), hand-rolled
// ---------------------------------------------------------------------------
//
// A dependency was the obvious alternative and was declined for the house rule
// at `xtask/src/deps.rs:14-17`: hand-rolled over dependency for something this
// small. It is self-validating — a wrong implementation mismatches all twelve
// committed digests at once rather than none — and it is the same routine
// `spikes/s1-faces` uses, so the two agree by construction.

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
