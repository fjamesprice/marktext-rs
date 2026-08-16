//! D18's culling, in one function, so that a review has one place to look.
//!
//! > **Decision: `mt-render` takes a viewport `Rect` and skips blocks whose
//! > `bounds` do not intersect it. It invents no coordinate, and the scan is
//! > linear.**
//!
//! # Why this is not the geometry §6 forbids
//!
//! A comparison between two rectangles the caller supplied produces no new
//! position. The failure the constraint exists to prevent is `mt-render`
//! deciding *where* something goes and M6's PDF writer deciding differently;
//! culling changes only *whether* an item is drawn, and both consumers would
//! cull the same list the same way.
//!
//! # Why linear rather than a binary search, and it is a finding
//!
//! `DisplayList::blocks` is in **document order, not paint order**
//! (`display.rs:203-207`), and blocks **nest** — a `BlockQuote`'s bounds contain
//! its `Paragraph`'s. So document order is not sorted in `y`, and a binary
//! search over it is *incorrect*, not merely approximate. An index would be a
//! structure `mt-layout` owns, and building one here would be geometry crossing
//! the seam in the other direction.
//!
//! So the linear scan is the honest first implementation and **D17's
//! measurement is the instrument that prices it**: 54,896 bounds tests per
//! frame on `5mb.md` is either invisible against rasterization or it is the
//! whole frame, and S4 gets the number rather than the intuition.
//!
//! # The comparison is inclusive at the edge, deliberately
//!
//! A half-open test (`a.x < b.max_x()`) drops a zero-width or zero-height
//! rectangle entirely, and the display list has both: a container block with no
//! painted height still carries items, and a rule is a `StrokedLine` whose two
//! endpoints share a `y`. The cost of the inclusive form is drawing a block
//! that touches the viewport edge and contributes no pixel; the cost of the
//! half-open form is losing one that does. **A false positive costs a frame a
//! few microseconds and a false negative costs it a visible block**, so the
//! asymmetry decides it.

use mt_layout::{BlockDisplay, Rect};

/// Whether two rectangles overlap, edges included.
///
/// This is the only coordinate comparison in the crate, and every argument to
/// it comes from either a [`BlockDisplay::bounds`] or the caller's viewport.
pub fn intersects(a: Rect, b: Rect) -> bool {
    a.x <= b.max_x() && b.x <= a.max_x() && a.y <= b.max_y() && b.y <= a.max_y()
}

/// Whether a block has to be drawn for this viewport — D18's whole content.
///
/// # A note the caller does not have to act on, but a reviewer should read
///
/// This tests `bounds`, and `display.rs:250-256` says a block's *items* can
/// reach past `bounds` horizontally. That is not a hole: everything past
/// `bounds.max_x()` is what [`crate::cpu`] clips away, so an item outside the
/// bounds of a block outside the viewport is doubly invisible. The direction it
/// would matter in — an item reaching *into* the viewport from a block whose
/// bounds do not — is the same case, and it is clipped for the same reason.
pub fn is_visible_in(block: &BlockDisplay, viewport: Rect) -> bool {
    intersects(block.bounds, viewport)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn a_block_wholly_inside_the_viewport_intersects_it() {
        assert!(intersects(
            r(10.0, 10.0, 5.0, 5.0),
            r(0.0, 0.0, 100.0, 100.0)
        ));
    }

    #[test]
    fn a_block_wholly_above_or_below_the_viewport_does_not() {
        let viewport = r(0.0, 100.0, 800.0, 600.0);
        assert!(!intersects(r(0.0, 0.0, 800.0, 50.0), viewport));
        assert!(!intersects(r(0.0, 900.0, 800.0, 50.0), viewport));
    }

    /// The case that decides whether a scrolled frame has a seam at the top of
    /// the window: a block half above the viewport is still drawn.
    #[test]
    fn a_block_straddling_the_viewport_edge_is_included() {
        let viewport = r(0.0, 100.0, 800.0, 600.0);
        assert!(intersects(r(0.0, 60.0, 800.0, 50.0), viewport), "top edge");
        assert!(
            intersects(r(0.0, 680.0, 800.0, 50.0), viewport),
            "bottom edge"
        );
    }

    /// Zero-height blocks exist on the list, and the half-open test would drop
    /// them — see this module's doc for why the trade goes this way.
    #[test]
    fn a_zero_height_block_on_the_viewport_edge_survives_the_test() {
        let viewport = r(0.0, 0.0, 800.0, 600.0);
        assert!(intersects(r(0.0, 0.0, 800.0, 0.0), viewport));
        assert!(intersects(r(0.0, 600.0, 800.0, 0.0), viewport));
        assert!(!intersects(r(0.0, 600.1, 800.0, 0.0), viewport));
    }

    #[test]
    fn horizontal_culling_works_the_same_way() {
        let viewport = r(200.0, 0.0, 400.0, 600.0);
        assert!(!intersects(r(0.0, 0.0, 100.0, 10.0), viewport));
        assert!(!intersects(r(700.0, 0.0, 100.0, 10.0), viewport));
        assert!(intersects(r(150.0, 0.0, 100.0, 10.0), viewport));
    }
}
