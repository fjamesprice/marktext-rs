//! D16's table: the one thing on the display list a renderer cannot resolve by
//! itself.
//!
//! > **Decision: the shell builds a `FontId → vello_cpu::peniko::FontData`
//! > table in the same order it built the `Fonts` collection, and `mt-render`
//! > receives it. `mt-layout` grows no accessor and `mt-render` does not depend
//! > on parley.**
//!
//! # Why a `Vec` and not a map
//!
//! `FontId` **is** the registration index (`fonts.rs:509`), which for a shell
//! driving `FaceList::faces` is the order of `[[face]]` entries in
//! `assets/fonts/faces.toml` — an order frozen by a test over twelve faces
//! (`fonts.rs:751-771`). A hash map keyed on an index is a slower `Vec` with an
//! extra way to be wrong, and it would also hide the failure this table is most
//! likely to have: a *short* table looks fine to a map lookup right up to the
//! id that is missing.
//!
//! # What the shell owes, and why this type cannot check it
//!
//! Two orderings that drift is exactly the failure `fonts.rs:751-771` prevents
//! on one side and nothing prevents on the other. This type holds no `Fonts`
//! and cannot compare itself against one — that is the point, since holding one
//! would put parley on `mt-render`'s surface. So the obligation is the shell's,
//! it is stated in [`FontTable`]'s own doc, and the harness that builds one
//! discharges it by asserting equal length and equal `Fonts::file_name` per
//! index **before** it renders anything.
//!
//! It matters because a wrong face produces a perfectly well-formed frame:
//! every glyph id resolves to *some* outline, nothing errors, and the artifact
//! is indistinguishable from a correct one. That is
//! `assert_corpus_fully_covered`'s argument (`layout.rs:865-867`) pointed at a
//! second artifact.
//!
//! # Blob identity does not have to hold
//!
//! §9's hazard is that `Blob::id()` is a process-unique counter, so a second
//! registration of the same twelve files yields twelve *unequal* handles. It
//! does not bite here. Glyph ids are per-face and are **already resolved** on
//! the display list, so two `FontData` values over the same file and index draw
//! identically whether or not they compare equal. **The table has to agree on
//! file and index, not on identity** — a weaker obligation than the one D8
//! closed at S1.

use mt_layout::FontId;
use vello_cpu::peniko::FontData;

/// `FontId` → the bytes to shape with, in registration order.
///
/// # The order is the contract
///
/// Entry *i* must be the face `Fonts` registered at index *i*. Build this from
/// the same `FaceList` in the same loop, or from `Fonts::ids()`, and assert the
/// two agree before rendering:
///
/// ```no_run
/// # use mt_layout::Fonts;
/// # use mt_render::{FontData, FontTable};
/// # fn face_bytes(_: &str) -> Vec<u8> { Vec::new() }
/// # let fonts = Fonts::new();
/// let mut table = FontTable::new();
/// for id in fonts.ids() {
///     let file = fonts.file_name(id).expect("a registered id names its file");
///     table.push(FontData::new(face_bytes(file).into(), 0));
/// }
/// assert_eq!(table.len(), fonts.len());
/// ```
///
/// The `0` is the font's index *within* its file. Every one of the twelve
/// bundled faces is a single-font file; a `.ttc` would need the index its
/// family was registered under, which is the same number `Fonts` keyed on.
#[derive(Debug, Clone, Default)]
pub struct FontTable {
    faces: Vec<FontData>,
}

impl FontTable {
    /// An empty table.
    pub fn new() -> FontTable {
        FontTable { faces: Vec::new() }
    }

    /// Append the next registration index's face.
    ///
    /// Returns the [`FontId`] it now answers for, so that a shell building the
    /// table beside the collection can compare the two ids rather than trust
    /// that its two loops stayed in step.
    pub fn push(&mut self, font: FontData) -> FontId {
        let id = FontId::from_index(self.faces.len() as u32);
        self.faces.push(font);
        id
    }

    /// The face for an id, or `None` if the table is shorter than the list
    /// expects.
    pub fn get(&self, id: FontId) -> Option<&FontData> {
        self.faces.get(id.index())
    }

    /// How many faces the table holds.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Whether the table is empty.
    ///
    /// Worth asking before rendering for the same reason `Fonts::is_empty` is
    /// worth asking before laying out: a collection in this state draws a page
    /// of nothing, and nothing about the resulting frame says why.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Every id this table answers for, in registration order.
    pub fn ids(&self) -> impl Iterator<Item = FontId> + '_ {
        (0..self.faces.len() as u32).map(FontId::from_index)
    }
}

impl FromIterator<FontData> for FontTable {
    fn from_iter<T: IntoIterator<Item = FontData>>(iter: T) -> FontTable {
        FontTable {
            faces: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four bytes that are not a font. Nothing in this module parses one — the
    /// table is a hand-off, and a face's validity is `Fonts::register_face`'s
    /// question one layer down.
    fn stub(tag: u8) -> FontData {
        FontData::new(vec![tag; 4].into(), 0)
    }

    #[test]
    fn push_returns_the_registration_index_it_just_claimed() {
        let mut table = FontTable::new();
        assert!(table.is_empty());
        assert_eq!(table.push(stub(1)), FontId::from_index(0));
        assert_eq!(table.push(stub(2)), FontId::from_index(1));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn a_face_comes_back_under_the_id_it_was_pushed_at() {
        let mut table = FontTable::new();
        let a = table.push(stub(0xaa));
        let b = table.push(stub(0xbb));
        assert_eq!(table.get(a).expect("a").data.as_ref(), &[0xaa; 4]);
        assert_eq!(table.get(b).expect("b").data.as_ref(), &[0xbb; 4]);
    }

    /// The failure D16 predicts: a table one entry short of the collection.
    /// It has to be a `None` a renderer can name, not a panic and not a
    /// silently-substituted face.
    #[test]
    fn an_id_past_the_end_is_none_rather_than_a_panic() {
        let mut table = FontTable::new();
        table.push(stub(1));
        assert!(table.get(FontId::from_index(1)).is_none());
        assert!(table.get(FontId::from_index(11)).is_none());
    }

    #[test]
    fn ids_enumerates_registration_order() {
        let table: FontTable = (0..3).map(stub).collect();
        let ids: Vec<FontId> = table.ids().collect();
        assert_eq!(
            ids,
            [
                FontId::from_index(0),
                FontId::from_index(1),
                FontId::from_index(2)
            ]
        );
    }
}
