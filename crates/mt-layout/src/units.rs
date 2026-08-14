//! CSS length resolution, in one place — `em`, `rem`, `%` and `px`.
//!
//! # Why this is a module and not four multiplications at the call sites
//!
//! D4 kept every theme number in the unit its stylesheet used, and named the
//! unit in the field's suffix, because *"the resolution base differs per
//! element and a pre-resolved number would be wrong the moment
//! `--mu-font-size` changed"*. That decision moves the resolution here, and
//! resolution is where silent errors live: a heading's margin is `1rem` but its
//! size is `1.875em`, a code block's size is `90%` of its parent but its
//! padding is `1em` of **itself**, a blockquote's bar inset is plain `px`. Four
//! bases, and nothing in the type system distinguishes them once they are all
//! `f32`.
//!
//! So [`Units`] carries the three bases together and exposes one method per CSS
//! rule rather than per number. A call site reads `units.em(cb.padding_em)` or
//! `units.rem(h.margin_rem)`, and choosing the wrong one is a visible mistake
//! in the diff instead of an invisible one in the output.
//!
//! # The three bases, and the rule each one comes from
//!
//! | Method | CSS | Base |
//! |---|---|---|
//! | [`Units::em`] | `<n>em` on any property **except** `font-size` | the element's **own** computed `font-size` |
//! | [`Units::font_size_em`] | `font-size: <n>em` | the **parent's** computed `font-size` |
//! | [`Units::font_size_pct`] | `font-size: <n>%` | the **parent's** computed `font-size` |
//! | [`Units::rem`] | `<n>rem` | the **root** `font-size`, `html, body { font-size: 16px }` |
//!
//! The first two rows are the pair that catches people: `em` resolves against
//! the element's own size everywhere *except* in `font-size` itself, where the
//! element's own size is what is being computed and the parent's is used
//! instead. Both appear on the same element in muya's own sheet — a code block
//! is `font-size: 90%` **and** `padding: 1em`, so its padding is `1em` of
//! 14.4px and not of the 16px it inherited.
//!
//! `rem` is a genuinely different knob from `em`, not a synonym that happens to
//! agree: `--mu-font-size` is set on `.mu-editor` and `html, body { font-size:
//! 16px }` is set on the root, so enlarging the editor font moves every `em` in
//! the document and leaves every `rem` — that is, every heading margin —
//! exactly where it was.
//!
//! # `%` on properties other than `font-size` is deliberately absent
//!
//! A CSS percentage on a *length* resolves against the containing block's
//! width, not against a font size, and the two theme fields shaped that way
//! (`thematic_break.width_fraction`, `thematic_break.left_fraction`) are stored
//! as fractions and multiplied by the content width at the one call site that
//! has it. Giving [`Units`] a `pct` method would invite them through the wrong
//! door.

/// The three font-size bases a CSS length can resolve against, for one element.
///
/// Constructed once per document by [`Units::root`] and then threaded down the
/// tree: [`Units::child`] enters a child that inherits, and
/// [`Units::with_font_size`] applies that child's own computed `font-size`.
/// The pair is deliberately two calls rather than one, because between them is
/// exactly where `font-size: 0.8em` has to be resolved against the *parent*.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Units {
    own_px: f32,
    parent_px: f32,
    root_px: f32,
}

impl Units {
    /// The document root's units.
    ///
    /// `editor_font_size_px` is `[metrics] font_size_px`, muya's
    /// `--mu-font-size` on `.mu-editor`; `root_font_size_px` is
    /// `[metrics] root_font_size_px`, the `html, body` rule that `rem` reads.
    /// They are 16 and 16 in both shipped themes and are still two parameters,
    /// because D4's whole argument is that a theme model which conflates two
    /// knobs that happen to agree cannot express the case where they do not.
    pub fn root(editor_font_size_px: f32, root_font_size_px: f32) -> Units {
        Units {
            own_px: editor_font_size_px,
            parent_px: editor_font_size_px,
            root_px: root_font_size_px,
        }
    }

    /// Enter a child element that inherits this element's `font-size`.
    ///
    /// The child's `em` base is its parent's size until
    /// [`Units::with_font_size`] says otherwise, which is exactly CSS's
    /// `font-size: inherit` default.
    pub fn child(self) -> Units {
        Units {
            own_px: self.own_px,
            parent_px: self.own_px,
            root_px: self.root_px,
        }
    }

    /// Apply a computed `font-size` to a child produced by [`Units::child`].
    ///
    /// The parent base is left alone, so `font_size_em` and `font_size_pct`
    /// keep answering against the parent even after the child's own size is
    /// known — which matters because nothing forbids reading them twice.
    pub fn with_font_size(self, px: f32) -> Units {
        Units { own_px: px, ..self }
    }

    /// The element's own computed `font-size` in px.
    pub fn own_px(self) -> f32 {
        self.own_px
    }

    /// The parent's computed `font-size` in px.
    pub fn parent_px(self) -> f32 {
        self.parent_px
    }

    /// The root `font-size` in px — what `rem` multiplies.
    pub fn root_px(self) -> f32 {
        self.root_px
    }

    /// `<n>em` on any property other than `font-size`.
    pub fn em(self, n: f32) -> f32 {
        n * self.own_px
    }

    /// `font-size: <n>em` — resolved against the **parent**.
    pub fn font_size_em(self, n: f32) -> f32 {
        n * self.parent_px
    }

    /// `font-size: <n>%` — resolved against the **parent**.
    ///
    /// Kept distinct from [`Units::font_size_em`] even though `90%` and
    /// `0.9em` compute the same number, because D4 stored
    /// `code_block.font_size_pct` as a percentage on purpose: the two units
    /// coincide only while nothing intervenes, and the theme records what the
    /// stylesheet wrote.
    pub fn font_size_pct(self, n: f32) -> f32 {
        n / 100.0 * self.parent_px
    }

    /// `<n>rem` — resolved against the root, never against this element.
    ///
    /// Named for the CSS unit, which is the whole subject of this module, and
    /// therefore not renamed to appease `clippy::should_implement_trait`:
    /// implementing `std::ops::Rem` on a font-size context would be nonsense,
    /// and a `rem_length` would read as a length rather than as the unit.
    /// `mt_doc::Rope::from_str` carries the same waiver for the same reason.
    #[allow(clippy::should_implement_trait)]
    pub fn rem(self, n: f32) -> f32 {
        n * self.root_px
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn muya_root() -> Units {
        Units::root(16.0, 16.0)
    }

    #[test]
    fn a_paragraph_inherits_the_editor_size() {
        let p = muya_root().child();
        assert_eq!(p.own_px(), 16.0);
        // `margin: 0.5em 0`
        assert_eq!(p.em(0.5), 8.0);
    }

    /// muya's h1 is `font-size: 1.875em; margin: 1rem 0`. The two resolve
    /// against different bases, and getting that wrong is a 30px margin where
    /// a 16px one belongs.
    #[test]
    fn a_heading_sizes_in_em_and_margins_in_rem() {
        let h1 = muya_root().child();
        let size = h1.font_size_em(1.875);
        let h1 = h1.with_font_size(size);
        assert_eq!(size, 30.0);
        assert_eq!(h1.rem(1.0), 16.0, "1rem is the root's 16px, not the h1's");
        assert_eq!(h1.em(1.0), 30.0, "1em on an h1 would be 30px");
    }

    /// The pair that bites: `font-size: 90%` of the parent, `padding: 1em` of
    /// the result. 14.4px of padding, not 16.
    #[test]
    fn a_code_block_pads_in_its_own_shrunken_em() {
        let cb = muya_root().child();
        let size = cb.font_size_pct(90.0);
        let cb = cb.with_font_size(size);
        assert!((size - 14.4).abs() < 1e-4);
        assert!((cb.em(1.0) - 14.4).abs() < 1e-4, "padding: 1em");
        assert!((cb.em(1.5) - 21.6).abs() < 1e-4, "margin-top: 1.5em");
    }

    /// The case the brief names: a code block inside a blockquote inside a list
    /// item. The blockquote's `font-size: 1em` is explicit so it does not
    /// compound, the list item inherits, and the code block's 90% therefore
    /// still lands on 14.4 — but only because every step is resolved against
    /// the right base.
    #[test]
    fn nesting_compounds_only_where_the_css_says_it_does() {
        let list = muya_root().child();
        let item = list.child();
        let quote = item.child();
        let quote = quote.with_font_size(quote.font_size_em(1.0));
        assert_eq!(quote.own_px(), 16.0);
        let code = quote.child();
        let code = code.with_font_size(code.font_size_pct(90.0));
        assert!((code.own_px() - 14.4).abs() < 1e-4);
        assert!((code.em(1.0) - 14.4).abs() < 1e-4);
        assert_eq!(code.rem(1.0), 16.0, "rem does not care how deep we are");
    }

    /// A footnote *does* compound: `font-size: 0.8em`, so a paragraph inside it
    /// is 12.8px and its `0.5em` margin is 6.4px rather than 8.
    #[test]
    fn a_footnote_shrinks_everything_inside_it() {
        let fnote = muya_root().child();
        let fnote = fnote.with_font_size(fnote.font_size_em(0.8));
        assert!((fnote.own_px() - 12.8).abs() < 1e-4);
        assert!((fnote.em(1.4) - 17.92).abs() < 1e-3, "margin: 1.4em 0");
        let para = fnote.child();
        assert!((para.em(0.5) - 6.4).abs() < 1e-4);
    }

    /// The two bases are separate knobs, and this is the case that proves it:
    /// enlarge the editor font and every `em` moves while every `rem` does not.
    #[test]
    fn enlarging_the_editor_font_leaves_rem_alone() {
        let big = Units::root(24.0, 16.0).child();
        assert_eq!(big.em(1.0), 24.0);
        assert_eq!(big.rem(1.0), 16.0);
    }
}
