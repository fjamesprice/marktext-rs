//! The font seam — M3 §5 D7 and the `FontId` half of D8.
//!
//! > **Decision: the third option — `mt-layout` depends on parley with
//! > `default-features = false`, never enumerates anything, and receives a
//! > font collection constructed by the shell.**
//!
//! # The division of labour, and it is absolute
//!
//! `mt-layout` **reads nothing**. [`FaceList`] describes *which files to open
//! and how to wire them up*; the shell opens them and hands the bytes to
//! [`Fonts::register_face`]. `cargo xtask deps` greps this crate's `src/` for
//! filesystem calls — and the reason that check is worth passing is not
//! tidiness: it is that D10's layout goldens must be reproducible on a machine
//! whose installed fonts nobody controls, and the only way to guarantee that
//! is for the layout engine to be incapable of consulting them.
//!
//! (The grep is for the literal module path, so this file cannot spell it out.
//! `xtask/src/deps.rs` names the three prefixes it scans for.)
//!
//! # An empty registration is a hard error. Always.
//!
//! D7's fact 1, and M3-R10 exists because of it: **skrifa rejects `.woff`
//! silently.** `Collection::register_fonts` returns an empty `Vec` — no error,
//! no panic, no log. MarkText's own bundled Open Sans is eight `.woff` files,
//! so a naive port of the obvious code loads zero faces and lays the whole
//! document out in whatever the last-resort font turns out to be, with no
//! diagnostic anywhere. [`Fonts::register_face`] therefore returns
//! [`FontError::Rejected`] naming the file, and there is no "warn and carry
//! on" path to fall into.
//!
//! # The three wiring calls, and why the face list has two script fields
//!
//! Registering the faces is not enough — S1's face-list probe measured that
//! the theme's body stack over a bare registered set leaves **249 of 291**
//! corpus codepoints as tofu. What closes the gap is the wiring
//! [`Fonts::wire`] performs, and `assets/fonts/faces.toml`'s header records
//! why each half is needed:
//!
//! - [`Collection::append_fallbacks`] per script, from
//!   [`Face::fallback_scripts`] — a *separate* field from [`Face::scripts`],
//!   because "what this face covers" and "what fontique should be told to
//!   reach for" are different questions.
//! - [`Collection::append_generic_families`] per CSS generic, from
//!   [`Face::generic_families`]. This one is a measurement, not a preference:
//!   **parley resolves emoji through the generic family, not through script
//!   fallback** (`parley/src/shape/mod.rs:329` appends
//!   `GenericFamily::Emoji` for any cluster where `CharCluster::is_emoji()`),
//!   so an emoji face registered only as a `Zsye` script fallback is never
//!   consulted. And with `system_fonts: false` nothing at all defines what the
//!   `sans-serif` and `monospace` that both theme stacks end in mean.
//!
//! [`Collection::append_fallbacks`]: parley::fontique::Collection::append_fallbacks
//! [`Collection::append_generic_families`]: parley::fontique::Collection::append_generic_families

use std::collections::BTreeMap;
use std::fmt;

use parley::FontContext;
use parley::fontique::{
    Blob, Collection, CollectionOptions, FamilyId, GenericFamily, Script, SourceCache,
};
use serde::Deserialize;

/// `assets/fonts/faces.toml`, embedded at compile time.
///
/// `include_str!` is a compile-time read, so this does not breach the crate's
/// "no I/O" constraint — the same mechanism `theme::MUYA_DEFAULT_TOML` uses.
/// The *fonts* still have to be opened by somebody else; this is only the
/// list.
pub const BUNDLED_FACES_TOML: &str = include_str!("../../../assets/fonts/faces.toml");

// ---------------------------------------------------------------------------
// FontId
// ---------------------------------------------------------------------------

/// A face, as an index into the collection the shell registered under D7.
///
/// **This is the seam's one named exception** (D8): every other field crossing
/// the `mt-layout` boundary is a plain `f32`, `u32`, `bool` or `Range<usize>`,
/// and this one is a handle. It is an index rather than a parley type
/// precisely so that `mt-export` at M6 and `mt-render` at S4 can hold it
/// without depending on parley at all — PDF needs the *bytes* behind it and
/// the screen needs a *font handle*, and the shell that registered the face is
/// the one that can produce either.
///
/// The value is the order in which faces were registered, which for a shell
/// driving [`FaceList::faces`] is the order of `[[face]]` entries in
/// `assets/fonts/faces.toml`. **That order is load-bearing** — it is the
/// fallback chain, it is baked into every golden, and reordering it is a
/// golden-changing event on the same footing as moving the parley pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(u32);

impl FontId {
    /// The index this id wraps.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// An id from a registration index.
    ///
    /// Public so that a shell which persists ids (a golden, a PDF's font
    /// table) can reconstruct them. Nothing validates the index here; ask
    /// [`Fonts::family_name`] whether it names a registered face.
    pub const fn from_index(index: u32) -> Self {
        Self(index)
    }
}

impl fmt::Display for FontId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "font{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong at the font seam.
///
/// Every variant names the file, because the diagnostic these exist to replace
/// is the absence of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// The face list did not parse.
    Parse {
        /// The deserializer's message.
        message: String,
    },
    /// **The `.woff` trap.** `register_fonts` returned no families at all,
    /// which skrifa does silently for any container it does not understand.
    Rejected {
        /// The `file` key of the entry that failed.
        file: String,
    },
    /// The face loaded but calls itself something else.
    ///
    /// A hard error rather than a note, because the fallback chain resolves on
    /// family *names*: a face whose real name differs from its declared one is
    /// registered but unreachable, which is the silent-tofu failure again in a
    /// different costume.
    FamilyMismatch {
        /// The `file` key of the entry.
        file: String,
        /// What `faces.toml` says.
        declared: String,
        /// What the font's own name table says.
        observed: String,
    },
    /// Wiring named a family no registered face provides.
    UnknownFamily {
        /// The `file` key of the entry that named it.
        file: String,
        /// The family name.
        family: String,
    },
    /// A `fallback_scripts` entry is not a four-letter ISO 15924 code.
    InvalidScript {
        /// The `file` key of the entry that named it.
        file: String,
        /// The offending value.
        script: String,
    },
    /// A `generic_families` entry is not a CSS generic family name.
    InvalidGenericFamily {
        /// The `file` key of the entry that named it.
        file: String,
        /// The offending value.
        generic: String,
    },
    /// A laid-out run's face is not one this [`Fonts`] registered.
    ///
    /// Unreachable for a `system_fonts: false` collection unless the text was
    /// shaped against a *different* [`Fonts`]. It is an error rather than a
    /// dropped run because a dropped run is invisible text, which is the same
    /// silent failure [`Rejected`](Self::Rejected) exists to prevent.
    UnresolvedFont {
        /// The run's byte range in the block's text, so the caller can say
        /// which text vanished.
        text_range: std::ops::Range<usize>,
    },
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Parse { message } => write!(f, "face list does not parse: {message}"),
            FontError::Rejected { file } => write!(
                f,
                "{file}: the font engine registered 0 families from this file and reported no \
                 error. skrifa does this for containers it cannot read — `.woff` above all — and \
                 a face that fails to load silently renders tofu everywhere with no diagnostic \
                 (M3.md §5 D7 fact 1, risk M3-R10). Re-source the file as `.ttf` or `.otf`."
            ),
            FontError::FamilyMismatch {
                file,
                declared,
                observed,
            } => write!(
                f,
                "{file}: the font's own name table says {observed:?} but faces.toml declares \
                 {declared:?}. The fallback chain resolves on the family name, so the face would \
                 be registered and unreachable."
            ),
            FontError::UnknownFamily { file, family } => write!(
                f,
                "{file}: wiring names family {family:?}, which no registered face provides. \
                 Register every face in the list before wiring it."
            ),
            FontError::InvalidScript { file, script } => write!(
                f,
                "{file}: {script:?} is not a four-letter ISO 15924 script code"
            ),
            FontError::InvalidGenericFamily { file, generic } => {
                write!(f, "{file}: {generic:?} is not a CSS generic family name")
            }
            FontError::UnresolvedFont { text_range } => write!(
                f,
                "the run covering bytes {text_range:?} was shaped with a face this collection did \
                 not register — the text was laid out against a different `Fonts`"
            ),
        }
    }
}

impl std::error::Error for FontError {}

// ---------------------------------------------------------------------------
// The face list
// ---------------------------------------------------------------------------

/// `assets/fonts/faces.toml` — the pinned bundled set, as data.
///
/// A committed artifact of the repository rather than a property of the
/// developer's machine, which is what D10's exact-equality goldens need in
/// order to mean anything. Read that file's header comment for what each key
/// is for; this struct is its schema, and like [`Theme`](crate::theme::Theme)
/// every field is required and unknown keys are an error.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceList {
    /// Set-level provenance.
    pub meta: FaceListMeta,
    /// The faces, **in fallback order**.
    #[serde(rename = "face")]
    pub faces: Vec<Face>,
}

impl FaceList {
    /// Parse a face list from TOML.
    ///
    /// Fails if any field is missing or any key is unrecognised, for the
    /// reason [`Theme::from_toml_str`](crate::theme::Theme::from_toml_str)
    /// does: the failure mode this schema exists to prevent is silence.
    pub fn from_toml_str(src: &str) -> Result<FaceList, FontError> {
        toml::from_str(src).map_err(|e| FontError::Parse {
            message: e.to_string(),
        })
    }

    /// The committed set in `assets/fonts/`.
    ///
    /// # Panics
    ///
    /// Never in a build that passes its own tests: the source is embedded at
    /// compile time and a test parses it.
    pub fn bundled() -> FaceList {
        FaceList::from_toml_str(BUNDLED_FACES_TOML)
            .expect("the embedded assets/fonts/faces.toml must parse")
    }

    /// The total size of every file named, in bytes.
    ///
    /// A shell can compare this against [`FaceListMeta::total_font_bytes`]
    /// before reading anything, which catches an entry added without the
    /// header being updated.
    pub fn total_declared_bytes(&self) -> u64 {
        self.faces.iter().map(|f| f.bytes).sum()
    }
}

/// `[meta]` — provenance for the set as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceListMeta {
    /// Bumped when the *set* changes — a face added, removed or reordered.
    /// A golden header cites it.
    pub revision: u32,
    /// The parley revision these faces were measured against (D2's pin). A
    /// face list is only reproducible together with the shaper that reads it.
    pub parley_rev: String,
    /// Sum of every entry's `bytes`.
    pub total_font_bytes: u64,
}

/// One `[[face]]`.
///
/// Every entry carries every key — a requirement on the format, so that a
/// loader needs no per-file special casing. The one derived entry (the CJK
/// corpus subset) is flagged by [`derived`](Self::derived) rather than by
/// being shaped differently.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Face {
    /// File name within `assets/fonts/`. **Never globbed for** — a loader
    /// reads this key, which is why the one file with brackets in its name
    /// costs nothing.
    pub file: String,
    /// The family name **skrifa reports**, not the one we would like.
    pub family: String,
    /// The style name in the font's own name table (ID 2). Recorded because it
    /// is not always `"Regular"` — DejaVu Sans Mono says `"Book"`.
    pub subfamily: String,
    /// CSS weight to register and request this face as.
    pub weight: u16,
    /// The style fontique reports. Note that both DejaVu obliques report
    /// `italic`: the name table says "Oblique" but OS/2 `fsSelection` sets the
    /// italic bit and there is no `slnt` axis.
    pub style: FaceStyle,
    /// True if the file has an `fvar` table.
    pub variable: bool,
    /// Variation axes; empty for every static face.
    pub axes: Vec<Axis>,
    /// ISO 15924 codes this face is in the list to cover. **Measured** from
    /// the font's own cmap, not claimed from the family name.
    pub scripts: Vec<String>,
    /// ISO 15924 keys to register this face under with `append_fallbacks`.
    /// See the module doc for why this is not [`scripts`](Self::scripts).
    pub fallback_scripts: Vec<String>,
    /// CSS generic names to register this face under with
    /// `append_generic_families`. See the module doc: this field exists
    /// because of a measurement.
    pub generic_families: Vec<String>,
    /// Which theme stack names this family, or `fallback` if none does.
    pub role: FaceRole,
    /// True if this file was **generated from** an upstream font rather than
    /// committed as upstream shipped it. Exactly one entry is `true`: the CJK
    /// corpus subset, which is a test fixture and not a shipped asset.
    pub derived: bool,
    /// The upstream file, version and SHA-256. Empty when `derived = false`.
    pub derived_from: String,
    /// The exact command that regenerates the file. Empty when
    /// `derived = false`. This is the whole reason a derived artifact is
    /// acceptable: it is re-derivable, byte for byte, by anyone.
    pub derived_command: String,
    /// File size, so a truncated checkout is caught before the hash is.
    pub bytes: u64,
    /// SHA-256 of the file as committed.
    pub sha256: String,
}

/// A variation axis.
///
/// Only one committed face has any: `NotoEmoji[wght].ttf`. Its `default` is
/// what parley resolves when nothing asks otherwise, and therefore what a
/// golden bakes in — a bold heading resolves `wght = 700` and gets different
/// advances, deterministically.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Axis {
    /// Four-character OpenType axis tag, e.g. `wght`. A `String` because that
    /// is what TOML gives and nothing here needs it as four bytes.
    pub tag: String,
    /// Minimum value.
    pub min: f32,
    /// The default instance.
    pub default: f32,
    /// Maximum value.
    pub max: f32,
}

/// `style` — the value fontique reports, not the one the name table spells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FaceStyle {
    /// Upright.
    Normal,
    /// Italic, including the two DejaVu obliques.
    Italic,
    /// A true oblique with a `slnt` axis. No committed face is one today.
    Oblique,
}

/// `role` — which theme stack names this family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FaceRole {
    /// `[fonts] body`.
    Body,
    /// `[fonts] code` and `[fonts] inline_code`.
    Code,
    /// Named by no stack; exists only to catch codepoints the named families
    /// lack.
    Fallback,
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

/// One registered face, as `Fonts` remembers it.
#[derive(Debug, Clone)]
struct Registered {
    file: String,
    family: String,
    family_id: FamilyId,
}

/// The font collection, built from bytes the shell supplies.
///
/// Wraps parley's `FontContext` (a `Collection` plus a `SourceCache`) and
/// keeps it private, because D8 says no public signature here may name a
/// parley type. What crosses the seam instead is [`FontId`].
///
/// Constructed with `CollectionOptions { shared: false, system_fonts: false }`
/// — the second half being the one that matters. With `system_fonts` on,
/// fontique enumerates the machine's font directories, and a golden that
/// depends on the developer's installed fonts is not a golden.
pub struct Fonts {
    cx: FontContext,
    /// Indexed by [`FontId`].
    registered: Vec<Registered>,
    /// `(blob id, collection index)` → [`FontId`].
    ///
    /// **This is D8's unbuilt mapping, built.** `Blob::id()` is a
    /// process-unique `u64` handed out by an atomic counter when the blob is
    /// created, and it is copied — not regenerated — by `Blob::clone`. The
    /// collection stores the blob we registered as `SourceKind::Memory`, and
    /// every path from there back out to a laid-out run
    /// (`FontInfo::load` → `SourceCache::get` → `QueryFont::blob` →
    /// `FontData::data`) is a `clone`. So the id observed on a shaped run is
    /// the id of the blob this crate created, by construction rather than by
    /// pointer coincidence.
    by_blob: BTreeMap<(u64, u32), FontId>,
}

impl fmt::Debug for Fonts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fonts")
            .field("faces", &self.registered.len())
            .finish_non_exhaustive()
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    /// An empty collection that will never touch the filesystem.
    pub fn new() -> Fonts {
        Fonts {
            cx: FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                source_cache: SourceCache::default(),
            },
            registered: Vec::new(),
            by_blob: BTreeMap::new(),
        }
    }

    /// Register one face from bytes the caller read.
    ///
    /// `bytes` is taken by value: fontique keeps it alive behind a `Blob`, and
    /// borrowing would put a lifetime on [`Fonts`] that every consumer would
    /// then carry.
    ///
    /// # Errors
    ///
    /// [`FontError::Rejected`] if the font engine produced no families — the
    /// silent failure D7 fact 1 names, made loud. [`FontError::FamilyMismatch`]
    /// if the file's own name table disagrees with `face.family`.
    pub fn register_face(&mut self, face: &Face, bytes: Vec<u8>) -> Result<FontId, FontError> {
        let blob = Blob::from(bytes);
        let blob_id = blob.id();
        let registered = self.cx.collection.register_fonts(blob, None);
        if registered.is_empty() {
            return Err(FontError::Rejected {
                file: face.file.clone(),
            });
        }

        // A single file may in principle carry several families (a `.ttc`).
        // None of the committed twelve does, so the first family is the face's
        // and any others are a surprise worth reporting through the same
        // mismatch error rather than being absorbed.
        let mut id = None;
        for (family_id, infos) in &registered {
            let observed = self
                .cx
                .collection
                .family_name(*family_id)
                .unwrap_or_default()
                .to_string();
            if observed != face.family {
                return Err(FontError::FamilyMismatch {
                    file: face.file.clone(),
                    declared: face.family.clone(),
                    observed,
                });
            }
            if id.is_none() {
                let next = FontId(self.registered.len() as u32);
                self.registered.push(Registered {
                    file: face.file.clone(),
                    family: observed,
                    family_id: *family_id,
                });
                id = Some(next);
            }
            // One key per font *within* the file, so a `.ttc` resolves each of
            // its members back to this entry rather than only its first.
            let this = id.expect("set on the first iteration");
            for info in infos {
                self.by_blob.insert((blob_id, info.index()), this);
            }
        }
        Ok(id.expect("registered is non-empty"))
    }

    /// Both wiring calls, driven from the list's own data.
    ///
    /// Call after every face in `list` has been registered. See the module doc
    /// for what each half buys and why neither is optional.
    pub fn wire(&mut self, list: &FaceList) -> Result<(), FontError> {
        self.append_fallbacks(list)?;
        self.append_generic_families(list)?;
        Ok(())
    }

    /// `append_fallbacks` for every script any face claims.
    ///
    /// Families are offered in face-list order and deduplicated: four DejaVu
    /// files are one family, and handing fontique the same `FamilyId` four
    /// times says nothing extra.
    pub fn append_fallbacks(&mut self, list: &FaceList) -> Result<(), FontError> {
        for script in ordered_unique(list.faces.iter().flat_map(|f| f.fallback_scripts.iter())) {
            let bytes = script.as_bytes();
            let [a, b, c, d] = *bytes else {
                let file = list
                    .faces
                    .iter()
                    .find(|f| f.fallback_scripts.contains(script))
                    .map(|f| f.file.clone())
                    .unwrap_or_default();
                return Err(FontError::InvalidScript {
                    file,
                    script: script.clone(),
                });
            };
            let key = Script::from_bytes([a, b, c, d]);
            let families =
                self.families_for(list, |face| face.fallback_scripts.contains(script))?;
            self.cx
                .collection
                .append_fallbacks(key, families.into_iter());
        }
        Ok(())
    }

    /// `append_generic_families` for every CSS generic any face claims.
    pub fn append_generic_families(&mut self, list: &FaceList) -> Result<(), FontError> {
        for generic in ordered_unique(list.faces.iter().flat_map(|f| f.generic_families.iter())) {
            let Some(parsed) = GenericFamily::parse(generic) else {
                let file = list
                    .faces
                    .iter()
                    .find(|f| f.generic_families.contains(generic))
                    .map(|f| f.file.clone())
                    .unwrap_or_default();
                return Err(FontError::InvalidGenericFamily {
                    file,
                    generic: generic.clone(),
                });
            };
            let families =
                self.families_for(list, |face| face.generic_families.contains(generic))?;
            self.cx
                .collection
                .append_generic_families(parsed, families.into_iter());
        }
        Ok(())
    }

    /// How many faces are registered.
    pub fn len(&self) -> usize {
        self.registered.len()
    }

    /// Whether nothing is registered. A collection in this state lays every
    /// document out as tofu, so it is worth asking before laying anything out.
    pub fn is_empty(&self) -> bool {
        self.registered.is_empty()
    }

    /// Every registered id, in registration order.
    pub fn ids(&self) -> impl Iterator<Item = FontId> + '_ {
        (0..self.registered.len() as u32).map(FontId)
    }

    /// The family name of a registered face, or `None` if the id is unknown.
    pub fn family_name(&self, id: FontId) -> Option<&str> {
        self.registered.get(id.index()).map(|r| r.family.as_str())
    }

    /// The `file` key of a registered face, or `None` if the id is unknown.
    ///
    /// The seam's answer to "give me the bytes": a shell that registered from
    /// `assets/fonts/` can reopen this name, and M6's PDF writer can embed it.
    pub fn file_name(&self, id: FontId) -> Option<&str> {
        self.registered.get(id.index()).map(|r| r.file.as_str())
    }

    // -- internals; these are the only places parley types are named --------

    /// The `FamilyId`s a predicate selects, in face-list order, deduplicated.
    fn families_for(
        &self,
        list: &FaceList,
        mut wanted: impl FnMut(&Face) -> bool,
    ) -> Result<Vec<FamilyId>, FontError> {
        let mut out: Vec<FamilyId> = Vec::new();
        for face in list.faces.iter().filter(|f| wanted(f)) {
            let Some(entry) = self.registered.iter().find(|r| r.family == face.family) else {
                return Err(FontError::UnknownFamily {
                    file: face.file.clone(),
                    family: face.family.clone(),
                });
            };
            if !out.contains(&entry.family_id) {
                out.push(entry.family_id);
            }
        }
        Ok(out)
    }

    pub(crate) fn context_mut(&mut self) -> &mut FontContext {
        &mut self.cx
    }

    /// Resolve a shaped run's font handle back to the face that was
    /// registered for it. **D8's named uncertainty, closed** — see
    /// [`Fonts::by_blob`].
    ///
    /// Returns `None` only if the handle came from a collection this `Fonts`
    /// did not build, which for a `system_fonts: false` collection means never.
    pub(crate) fn resolve(&self, font: &parley::FontData) -> Option<FontId> {
        self.by_blob.get(&(font.data.id(), font.index)).copied()
    }
}

/// Values in first-seen order, deduplicated.
///
/// Order matters and `sort` would destroy it: the face list's order **is** the
/// fallback chain, and fontique appends fallbacks in the order it is given
/// them.
fn ordered_unique<'a>(values: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut out: Vec<&'a String> = Vec::new();
    for v in values {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The real list is embedded, so these are not fixtures — they are the
    // artifact. What they cannot check is that the *files* exist and hash
    // correctly, which needs the filesystem and therefore lives in
    // `tests/fonts.rs`.

    #[test]
    fn the_embedded_face_list_parses() {
        let list = FaceList::bundled();
        assert_eq!(list.faces.len(), 12, "twelve committed faces");
        assert_eq!(list.meta.revision, 1);
        assert_eq!(
            list.meta.parley_rev, "a0752c7bdc3ad88dac19fc194d2b8e57e59bea2e",
            "the face list is only reproducible together with the parley D2 pinned"
        );
    }

    #[test]
    fn meta_total_agrees_with_the_entries() {
        let list = FaceList::bundled();
        assert_eq!(list.total_declared_bytes(), list.meta.total_font_bytes);
    }

    /// The format's own rule: every entry carries every key, so a loader needs
    /// no per-file special casing. Serde enforces presence; this enforces that
    /// the values are usable without a lookup table.
    #[test]
    fn every_entry_is_shaped_the_same_way() {
        let list = FaceList::bundled();
        for face in &list.faces {
            assert!(!face.file.is_empty(), "{}: empty file", face.file);
            assert!(!face.family.is_empty(), "{}: empty family", face.file);
            assert!(face.bytes > 0, "{}: zero bytes", face.file);
            assert_eq!(face.sha256.len(), 64, "{}: sha256 is not 64 hex", face.file);
            assert!(
                face.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: sha256 is not hex",
                face.file
            );
            for script in face.scripts.iter().chain(&face.fallback_scripts) {
                assert_eq!(
                    script.len(),
                    4,
                    "{}: {script:?} is not an ISO 15924 code",
                    face.file
                );
            }
            for generic in &face.generic_families {
                assert!(
                    GenericFamily::parse(generic).is_some(),
                    "{}: {generic:?} is not a CSS generic family",
                    face.file
                );
            }
            assert_eq!(
                face.variable,
                !face.axes.is_empty(),
                "{}: `variable` disagrees with `axes`",
                face.file
            );
            // A derived artifact is defensible only if it is re-derivable.
            assert_eq!(
                face.derived,
                !face.derived_from.is_empty() && !face.derived_command.is_empty(),
                "{}: `derived` disagrees with its provenance keys",
                face.file
            );
        }
    }

    /// `assets/fonts/faces.toml`'s header says the order is the fallback chain
    /// and that reordering it is a golden-changing event. Freezing it here
    /// makes an accidental reorder a test failure rather than a golden diff
    /// somebody rubber-stamps.
    #[test]
    fn the_fallback_order_is_frozen() {
        let list = FaceList::bundled();
        let files: Vec<&str> = list.faces.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(
            files,
            [
                "OpenSans-Regular.ttf",
                "OpenSans-Italic.ttf",
                "OpenSans-Bold.ttf",
                "OpenSans-BoldItalic.ttf",
                "NotoSansArabic-Regular.ttf",
                "NotoSansArabic-Bold.ttf",
                "NotoEmoji[wght].ttf",
                "NotoSansCJKsc-Regular-corpus-subset.otf",
                "DejaVuSansMono.ttf",
                "DejaVuSansMono-Oblique.ttf",
                "DejaVuSansMono-Bold.ttf",
                "DejaVuSansMono-BoldOblique.ttf",
            ]
        );
    }

    /// Emoji reaches its face through the generic family and not through
    /// script fallback — the measurement `faces.toml`'s header records, and the
    /// reason `generic_families` exists as a key at all.
    #[test]
    fn the_emoji_face_is_wired_through_the_generic_family() {
        let list = FaceList::bundled();
        let emoji = list
            .faces
            .iter()
            .find(|f| f.family == "Noto Emoji")
            .expect("the set has an emoji face");
        assert!(
            emoji
                .generic_families
                .iter()
                .any(|g| GenericFamily::parse(g) == Some(GenericFamily::Emoji)),
            "without this key 67 of emoji.md's 82 codepoints stay tofu"
        );
    }

    /// Both theme stacks end in a CSS generic and `system_fonts: false` means
    /// nothing else defines one.
    #[test]
    fn the_theme_stacks_trailing_generics_are_defined_by_some_face() {
        let list = FaceList::bundled();
        let theme = crate::theme::Theme::muya_default();
        for stack in [
            &theme.fonts.body,
            &theme.fonts.code,
            &theme.fonts.inline_code,
        ] {
            let last = stack.last().expect("a non-empty stack");
            let generic = GenericFamily::parse(last)
                .unwrap_or_else(|| panic!("{last:?} should be a CSS generic"));
            assert!(
                list.faces.iter().any(|f| f
                    .generic_families
                    .iter()
                    .any(|g| GenericFamily::parse(g) == Some(generic))),
                "no face defines {last:?}, so the stack's last resort resolves to nothing"
            );
        }
    }

    #[test]
    fn unknown_keys_and_missing_fields_are_errors() {
        let err = FaceList::from_toml_str(
            "[meta]\nrevision = 1\nparley_rev = \"x\"\ntotal_font_bytes = 0\nwat = true\n",
        )
        .unwrap_err();
        assert!(matches!(err, FontError::Parse { .. }));
        let err = FaceList::from_toml_str("[meta]\nrevision = 1\n").unwrap_err();
        assert!(matches!(err, FontError::Parse { .. }));
    }

    #[test]
    fn an_empty_collection_says_so() {
        let fonts = Fonts::new();
        assert!(fonts.is_empty());
        assert_eq!(fonts.len(), 0);
        assert_eq!(fonts.family_name(FontId::from_index(0)), None);
        assert_eq!(fonts.ids().count(), 0);
    }

    /// The trap, without a `.woff` to hand: any bytes skrifa cannot read
    /// produce the same empty `Vec`, and the same hard error. The real
    /// `.woff` from the reference clone is asserted in `tests/fonts.rs`.
    #[test]
    fn garbage_bytes_are_a_hard_error_naming_the_file() {
        let mut fonts = Fonts::new();
        let face = Face {
            file: "not-a-font.ttf".into(),
            family: "Nothing".into(),
            subfamily: "Regular".into(),
            weight: 400,
            style: FaceStyle::Normal,
            variable: false,
            axes: vec![],
            scripts: vec![],
            fallback_scripts: vec![],
            generic_families: vec![],
            role: FaceRole::Fallback,
            derived: false,
            derived_from: String::new(),
            derived_command: String::new(),
            bytes: 4,
            sha256: String::new(),
        };
        let err = fonts
            .register_face(&face, b"wOFFnope".to_vec())
            .unwrap_err();
        assert_eq!(
            err,
            FontError::Rejected {
                file: "not-a-font.ttf".into()
            }
        );
        assert!(err.to_string().contains("not-a-font.ttf"));
        assert!(fonts.is_empty(), "a rejected face must not be counted");
    }

    #[test]
    fn wiring_before_registering_names_the_offending_file() {
        let mut fonts = Fonts::new();
        let err = fonts.wire(&FaceList::bundled()).unwrap_err();
        match err {
            FontError::UnknownFamily { family, .. } => assert_eq!(family, "Open Sans"),
            other => panic!("expected UnknownFamily, got {other:?}"),
        }
    }

    #[test]
    fn font_ids_are_registration_indices() {
        assert_eq!(FontId::from_index(3).index(), 3);
        assert_eq!(FontId::from_index(3).to_string(), "font3");
        assert!(FontId::from_index(1) < FontId::from_index(2));
    }

    #[test]
    fn ordered_unique_keeps_first_seen_order() {
        let v: Vec<String> = ["b", "a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let got: Vec<&str> = ordered_unique(v.iter())
            .into_iter()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(got, ["b", "a", "c"]);
    }
}
