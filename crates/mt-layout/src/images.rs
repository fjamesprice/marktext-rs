//! Inline image sizes, resolved above the seam — M3 §5 **D12**.
//!
//! # Why this is a table the caller fills rather than a function this crate has
//!
//! An `InlineBox` needs a width and a height before parley can place it, and
//! the size of a PNG is in the PNG. `mt_inline::token::Image` carries no
//! dimension of any kind — markdown has nowhere to put one — and `mt-layout`
//! opens nothing and could not resolve `./x.png` if it did, because the
//! document's directory is not an input to layout and §3 makes `Document` the
//! whole input.
//!
//! So this is D7's idiom one layer up, with a smaller payload. D7 has a
//! declared manifest ([`FaceList`](crate::FaceList)), a shell that reads bytes
//! and registers them, and an opaque handle ([`FontId`](crate::FontId)) that
//! crosses the seam. D12 has a declared table ([`ImageSizes`]), a shell that
//! decodes headers and inserts, and a plain [`ImageSize`] that crosses it.
//!
//! # A miss is not a number this crate invents
//!
//! A `src` with no entry does **not** get a guess and does not error. It takes
//! the reference's own no-bitmap geometry, which is specified in CSS:
//! `.mu-image-fail` and `.mu-empty-image` are both `width: 100%; height: 50px`
//! (`inlineSyntax.css:434-440`) on a `--code-block-bg-color` ground with a
//! **2px** radius (`:346-352`, deliberately not inline code's 3). Which of the
//! two applies is decided by whether `src` is empty, not by taste: muya's
//! `image.ts:170-193` never starts a load for an empty `src`.
//!
//! muya's third state, `.mu-image-loading` at 400 × 250 (`:442-448`), is
//! **unreachable at M3** and is deliberately not modelled: this table is M3's
//! whole notion of "loaded", so a miss is a completed failure and never a
//! pending one. S5's previewer is the first thing in the project that could
//! report "pending".

use std::collections::BTreeMap;

/// One image's decoded size, in CSS px.
///
/// The **intrinsic** size — what the bitmap's header says — not the laid-out
/// size. `max-width: 100%` is applied twice by the reference
/// (`inlineSyntax.css:374-390`) and belongs to whoever knows the containing
/// block's width, which is [`crate::flow`] and not the shell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSize {
    /// Intrinsic width in px.
    pub width_px: f32,
    /// Intrinsic height in px.
    pub height_px: f32,
}

impl ImageSize {
    /// An intrinsic size.
    pub const fn new(width_px: f32, height_px: f32) -> ImageSize {
        ImageSize {
            width_px,
            height_px,
        }
    }
}

/// The shell's answer to *"how big is the image at this `src`?"* — **D12**.
///
/// Keyed by the token's own `src` **exactly as it appears in the source**: this
/// crate does no path resolution, no percent-decoding and no normalisation, so
/// a shell that resolved `./x.png` against a directory has to insert it under
/// `./x.png` and not under the absolute path it opened.
///
/// An empty table is the correct value for a caller with no image loader — the
/// goldens use one — and it means every image takes the no-bitmap geometry
/// above. It is not a degenerate case to be defended against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageSizes {
    by_src: BTreeMap<String, ImageSize>,
}

impl ImageSizes {
    /// An empty table: every image takes D12's no-bitmap geometry.
    pub fn new() -> ImageSizes {
        ImageSizes::default()
    }

    /// Record one image's intrinsic size. Replaces any previous entry.
    pub fn insert(&mut self, src: impl Into<String>, size: ImageSize) -> Option<ImageSize> {
        self.by_src.insert(src.into(), size)
    }

    /// The size recorded for `src`, or `None` — which is a **completed
    /// failure**, not a missing answer, and has its own geometry.
    pub fn get(&self, src: &str) -> Option<ImageSize> {
        self.by_src.get(src).copied()
    }

    /// How many images the shell resolved.
    pub fn len(&self) -> usize {
        self.by_src.len()
    }

    /// Whether the shell resolved none.
    pub fn is_empty(&self) -> bool {
        self.by_src.is_empty()
    }
}

impl<S: Into<String>> FromIterator<(S, ImageSize)> for ImageSizes {
    fn from_iter<T: IntoIterator<Item = (S, ImageSize)>>(iter: T) -> ImageSizes {
        ImageSizes {
            by_src: iter.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_miss_is_none_rather_than_a_guessed_size() {
        let mut sizes = ImageSizes::new();
        sizes.insert("./x.png", ImageSize::new(320.0, 240.0));
        assert_eq!(sizes.get("./x.png"), Some(ImageSize::new(320.0, 240.0)));
        // No normalisation of any kind: the key is the source spelling.
        assert_eq!(sizes.get("x.png"), None);
        assert_eq!(sizes.get(""), None);
        assert_eq!(sizes.len(), 1);
    }

    #[test]
    fn an_empty_table_is_a_value_and_not_an_absence() {
        let sizes = ImageSizes::new();
        assert!(sizes.is_empty());
        assert_eq!(sizes.get("./anything.png"), None);
    }
}
