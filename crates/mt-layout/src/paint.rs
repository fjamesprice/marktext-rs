//! The non-glyph half of a theme, turned into display primitives.
//!
//! D8's module doc puts it plainly: *"Of D4's 148 required theme fields, the
//! great majority describe rectangles and rules … `mt-render` computes **no**
//! geometry, so every one of those has to arrive here already positioned."*
//! This module is where that translation happens, kept apart from
//! [`crate::flow`] so that the numbers in §2's geometry table can be asserted
//! against small pure functions instead of against a whole laid-out document.
//!
//! Everything here is `pub(crate)`: these are `mt-layout`'s internals, and D8's
//! public surface is the display list, not the recipe for building it.
//!
//! # Three places where CSS asks for something the display list cannot express
//!
//! 1. **Strokes.** [`FilledRect`] fills; there is no stroked rectangle. A CSS
//!    border is therefore four fills ([`push_border`]) and a bordered circle is
//!    a filled disc with a smaller disc of the background colour on top
//!    ([`push_ring`]). The second is why [`push_ring`] needs to be told the
//!    background: an unchecked checkbox's interior is `--editor-bg-color`, and
//!    without it the ring would be a blob.
//! 2. **Rotation about a point that is not the rectangle's centre.**
//!    [`FilledRect::rotation_deg`] rotates about the rectangle's own centre,
//!    but muya's checkmark is `transform: rotate(-45deg)` with
//!    `transform-origin: bottom` applied to a *box* whose two borders are the
//!    thing being drawn. [`rotated_about`] resolves that exactly rather than
//!    approximately: a rotation about an arbitrary pivot is a rotation about
//!    the centre plus a translation, so the same angle with a moved centre is
//!    the identical transform, not a near one.
//! 3. **UA list markers.** See [`BULLET_SIZE_EM`] — the one place in this
//!    module where a number is an estimate rather than a transcription, and it
//!    says so.

use crate::display::{Brush, DisplayItem, FilledRect, Rect, StrokedLine};
use crate::theme::{Checkbox, DashMarker, ListMarker, Theme};

// ---------------------------------------------------------------------------
// List-marker geometry — the estimated numbers, all three of them, together
// ---------------------------------------------------------------------------

/// A `disc`, `circle` or `square` bullet's diameter, as a multiple of the list
/// item's font size.
///
/// **This is an estimate, not a transcription.** CSS does not specify list
/// marker geometry and muya sets none: `list-style: disc outside none` hands
/// the whole question to the UA, and Chromium answers it with internal
/// constants in `LayoutListMarker` that no stylesheet can read. Every other
/// number in this module comes from a declaration in
/// `packages/muya/src/assets/styles/blockSyntax.css`; these three do not, and a
/// later stage that wants MarkText's exact bullets will have to measure them in
/// a browser and turn them into theme fields.
///
/// Grouped here rather than spread across the call sites precisely so that the
/// estimated part of the geometry is a short, findable list.
pub(crate) const BULLET_SIZE_EM: f32 = 0.35;

/// The stroke width of a hollow `circle` bullet. Estimated — see
/// [`BULLET_SIZE_EM`].
pub(crate) const BULLET_STROKE_PX: f32 = 1.0;

/// The gap between a marker's trailing edge and the list item's content edge,
/// as a multiple of the item's font size. Estimated — see [`BULLET_SIZE_EM`].
///
/// One rule for bullets and for ordered numbers, so that a `1.` and a `•` in
/// the same document line up their right edges. CSS `outside` markers do the
/// same thing, by a different mechanism.
pub(crate) const MARKER_GAP_EM: f32 = 0.5;

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// A rectangle rotated `deg` degrees clockwise about an arbitrary pivot.
///
/// [`FilledRect::rotation_deg`] rotates about the rectangle's own centre, which
/// is not what CSS `transform-origin` generally asks for. A rotation about a
/// pivot `P` equals the same rotation about the rectangle's centre `C` composed
/// with the translation that carries `C` to `R(C − P) + P`, so moving the
/// centre and keeping the angle reproduces the CSS transform **exactly**.
///
/// Clockwise with y down, matching both CSS's `rotate()` and
/// [`FilledRect::rotation_deg`]'s own documented convention.
pub(crate) fn rotated_about(
    rect: Rect,
    pivot_x: f32,
    pivot_y: f32,
    deg: f32,
    corner_radius: f32,
    brush: Brush,
) -> FilledRect {
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (dx, dy) = (cx - pivot_x, cy - pivot_y);
    let (sin, cos) = deg.to_radians().sin_cos();
    let nx = dx * cos - dy * sin + pivot_x;
    let ny = dx * sin + dy * cos + pivot_y;
    FilledRect {
        rect: Rect::new(
            nx - rect.width / 2.0,
            ny - rect.height / 2.0,
            rect.width,
            rect.height,
        ),
        corner_radius,
        rotation_deg: deg,
        brush,
    }
}

/// Push a CSS border as four fills around the inside of `border_box`.
///
/// Nothing is pushed when the width is zero or the brush is invisible — which
/// is the normal case rather than an edge one: correction C1 found that **30 of
/// the 32 shipped themes** set `border: none !important` on `pre.mu-code-block`,
/// so `dark` reaches here with `border_width_px = 0` for every code block in
/// the document.
///
/// The corners are drawn by the top and bottom runs spanning the full width and
/// the sides spanning what is left, rather than mitred. At 1px and 2px — the
/// only widths any shipped theme uses — a mitre and a butt joint are the same
/// pixels.
pub(crate) fn push_border(out: &mut Vec<DisplayItem>, border_box: Rect, width: f32, brush: Brush) {
    if width <= 0.0 || !brush.is_visible() || border_box.width <= 0.0 || border_box.height <= 0.0 {
        return;
    }
    let w = width
        .min(border_box.width / 2.0)
        .min(border_box.height / 2.0);
    let Rect { x, y, .. } = border_box;
    let (bw, bh) = (border_box.width, border_box.height);
    let side_h = (bh - 2.0 * w).max(0.0);
    for rect in [
        Rect::new(x, y, bw, w),
        Rect::new(x, y + bh - w, bw, w),
        Rect::new(x, y + w, w, side_h),
        Rect::new(x + bw - w, y + w, w, side_h),
    ] {
        out.push(DisplayItem::Rect(FilledRect::new(rect, brush)));
    }
}

/// Push a filled shape with a border of a different colour, as two fills.
///
/// `background` shows through the middle because there is no stroke primitive;
/// see the module doc. `rotation_deg` is applied to both fills about the centre
/// of `outer`, so the pair rotates as one element the way a CSS `transform` on
/// the parent does.
///
/// Nine parameters, and grouping them into a struct would only move the same
/// nine names one line up: every one is a separate theme field with a separate
/// CSS source, and the two callers pass different ones.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_ring(
    out: &mut Vec<DisplayItem>,
    outer: Rect,
    border: f32,
    corner_radius: f32,
    rotation_deg: f32,
    pivot_x: f32,
    pivot_y: f32,
    border_brush: Brush,
    background: Brush,
) {
    if outer.width <= 0.0 || outer.height <= 0.0 {
        return;
    }
    if border_brush.is_visible() {
        out.push(DisplayItem::Rect(rotated_about(
            outer,
            pivot_x,
            pivot_y,
            rotation_deg,
            corner_radius,
            border_brush,
        )));
    }
    let b = border
        .max(0.0)
        .min(outer.width / 2.0)
        .min(outer.height / 2.0);
    if b <= 0.0 || !background.is_visible() {
        return;
    }
    let inner = Rect::new(
        outer.x + b,
        outer.y + b,
        outer.width - 2.0 * b,
        outer.height - 2.0 * b,
    );
    out.push(DisplayItem::Rect(rotated_about(
        inner,
        pivot_x,
        pivot_y,
        rotation_deg,
        (corner_radius - b).max(0.0),
        background,
    )));
}

// ---------------------------------------------------------------------------
// Theme constructs
// ---------------------------------------------------------------------------

/// The `height` of the thematic break's `::before` box, before its border —
/// `blockSyntax.css:179`.
///
/// A literal in muya's own sheet with no theme variable in front of it, and
/// `ulysses` — the only theme that reshapes the rule at all
/// (`ulysses.theme.css:246-252`) — overrides `width`, `left`, `border-top` and
/// `transform` but **not** `height`. So it is the same 2px in every shipped
/// theme, which is why it is a constant here rather than a `ThematicBreak`
/// field. See [`thematic_break_rule`] for what it costs to get wrong.
pub(crate) const THEMATIC_BREAK_BOX_HEIGHT_PX: f32 = 2.0;

/// The thematic break's rule, given the block's **content** box.
///
/// `blockSyntax.css:172-186`. The `::before` is `top: 50%` with
/// `translateY(-50%)`, so its **border box** is centred on half the line box's
/// height — and the painted band is not that border box.
///
/// # The border box is 4px tall and only its top 2px are painted
///
/// The rule declares `height: 2px` **and** `border-top: 2px dashed`, with no
/// `box-sizing` on that selector: the six `box-sizing` declarations under
/// `assets/styles/` are `blockSyntax.css:17, 532, 552, 626, 709, 839`, none of
/// them this one, and the desktop renderer ships no global reset. So the
/// default `content-box` applies and the border box is
/// `2px content + 2px border = 4px`, which is what `translateY(-50%)` resolves
/// against.
///
/// Border box → `[0.5H − 2, 0.5H + 2]`. `background: none`, so the content area
/// paints nothing and the only painted band is the border: `[0.5H − 2, 0.5H]`,
/// whose centre is **`0.5H − 1`**. Emitting `0.5H` puts the rule one pixel low
/// on a 2px rule — half its own width.
///
/// Written out as `center − (box + thickness)/2 + thickness/2` rather than as
/// `center − 1` so that a theme which changed the border width would still land
/// on the top edge of a 4px-plus box, which is what the CSS would do.
///
/// `left_fraction` and `width_fraction` are already normalised past `ulysses`'s
/// `left: 50%; translateX(-50%)` by D4, so they multiply the content width
/// directly.
///
/// A [`StrokedLine`] rather than a thin [`FilledRect`] because
/// `thematic_break.style` is `Dashed` in muya's own default, and a dash pattern
/// is not expressible as a fill.
pub(crate) fn thematic_break_rule(theme: &Theme, content: Rect, brush: Brush) -> StrokedLine {
    let tb = &theme.thematic_break;
    let x0 = content.x + tb.left_fraction * content.width;
    let x1 = x0 + tb.width_fraction * content.width;
    let border_box_top = content.y + tb.center_fraction * content.height
        - (THEMATIC_BREAK_BOX_HEIGHT_PX + tb.thickness_px) / 2.0;
    let y = border_box_top + tb.thickness_px / 2.0;
    StrokedLine {
        x0,
        y0: y,
        x1,
        y1: y,
        width: tb.thickness_px,
        style: tb.style,
        brush,
    }
}

/// The blockquote's left bar, given the quote's **border** box.
///
/// `blockSyntax.css:150-158`: `position: absolute; top: 0; left: 15px; width:
/// 2px; height: 100%`. The `100%` is why this is emitted after the children are
/// placed — the bar's length is the quote's laid-out height and nothing knows
/// it earlier.
pub(crate) fn blockquote_bar(theme: &Theme, border_box: Rect, brush: Brush) -> FilledRect {
    FilledRect::new(
        Rect::new(
            border_box.x + theme.blockquote.bar_inset_px,
            border_box.y,
            theme.blockquote.bar_width_px,
            border_box.height,
        ),
        brush,
    )
}

/// A bullet marker.
///
/// `marker_right_x` is the marker's trailing edge and `marker_center_y` the
/// first line box's vertical midpoint; both are computed by the caller from
/// [`MARKER_GAP_EM`] and the theme's line height, so that bullets and ordered
/// numbers share one placement rule.
///
/// [`ListMarker::Dash`] ignores both: `ulysses` gives its marker literal
/// `left`/`top` offsets against the list item (`ulysses.theme.css:158-171`),
/// which is D4's own evidence that theme geometry escapes the variable system,
/// and honouring the literals is the only way to reproduce it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_bullet(
    out: &mut Vec<DisplayItem>,
    marker: &ListMarker,
    item_content_x: f32,
    item_content_y: f32,
    marker_right_x: f32,
    marker_center_y: f32,
    em_px: f32,
    brush: Brush,
    background: Brush,
) {
    if let ListMarker::Dash(DashMarker {
        width_px,
        height_px,
        left_px,
        top_px,
    }) = marker
    {
        out.push(DisplayItem::Rect(FilledRect::new(
            Rect::new(
                item_content_x + left_px,
                item_content_y + top_px,
                *width_px,
                *height_px,
            ),
            brush,
        )));
        return;
    }

    let size = BULLET_SIZE_EM * em_px;
    let rect = Rect::new(
        marker_right_x - size,
        marker_center_y - size / 2.0,
        size,
        size,
    );
    match marker {
        ListMarker::Disc => out.push(DisplayItem::Rect(FilledRect {
            rect,
            corner_radius: size / 2.0,
            rotation_deg: 0.0,
            brush,
        })),
        ListMarker::Circle => push_ring(
            out,
            rect,
            BULLET_STROKE_PX,
            size / 2.0,
            0.0,
            rect.x + size / 2.0,
            rect.y + size / 2.0,
            brush,
            background,
        ),
        ListMarker::Square => out.push(DisplayItem::Rect(FilledRect::new(rect, brush))),
        ListMarker::Dash(_) => unreachable!("handled above"),
    }
}

/// The task-list checkbox: ring, and — when checked — the tick.
///
/// `(box_x, box_y)` is the top-left of the 12×12 hit target, which itself
/// **paints nothing**: `appearance: none` on the `<input>` removes the native
/// control, and everything visible is its `::before` (the ring) and `::after`
/// (the tick). The box survives as the coordinate space both are positioned in
/// and as `transform-origin: center` for the unchecked rotation.
///
/// `blockSyntax.css:490-577`, and the three outliers replace all of it —
/// which is why every number below is a theme field.
pub(crate) fn push_checkbox(
    out: &mut Vec<DisplayItem>,
    theme: &Theme,
    box_x: f32,
    box_y: f32,
    checked: bool,
) {
    let cb: &Checkbox = &theme.checkbox;
    let colors = &theme.colors;

    // `transform-origin: center` on the box; the ring rides along with it.
    let pivot_x = box_x + cb.box_size_px / 2.0;
    let pivot_y = box_y + cb.box_size_px / 2.0;
    // The rotation animates to 0 on check (`blockSyntax.css:511`), so a checked
    // box is upright whatever the unchecked angle was.
    let rotation = if checked {
        0.0
    } else {
        cb.unchecked_rotation_deg
    };

    // Checked flips both the ring's fill and its border to the accent
    // (`blockSyntax.css:573-577`); unchecked is `--editor-bg-color` inside a
    // `--editor-color-50` border (`:531-537`).
    let (border_brush, fill_brush) = if checked {
        (
            Brush::resolve(colors.theme, Brush::default()),
            Brush::resolve(colors.theme, Brush::default()),
        )
    } else {
        (
            Brush::resolve(colors.editor_50, Brush::default()),
            Brush::resolve(colors.editor_bg, Brush::default()),
        )
    };

    push_ring(
        out,
        Rect::new(
            box_x + cb.ring_offset_px,
            box_y + cb.ring_offset_px,
            cb.ring_size_px,
            cb.ring_size_px,
        ),
        cb.ring_border_px,
        cb.ring_corner_radius_px,
        rotation,
        pivot_x,
        pivot_y,
        border_brush,
        fill_brush,
    );

    if !checked {
        // `transform: rotate(…) scale(0)` — the tick exists in the DOM at zero
        // size, which is an animation detail and not a shape.
        return;
    }

    // The tick is `box-sizing: content-box` with `border-left` and
    // `border-bottom` only, so its painted extent is the 8×4 content box grown
    // by one border on the left and one on the bottom.
    let bw = cb.check_border_px;
    let (bx, by) = (box_x + cb.check_left_px, box_y + cb.check_top_px);
    let (bwidth, bheight) = (cb.check_width_px + bw, cb.check_height_px + bw);
    // `transform-origin: bottom` — the horizontal centre of the bottom edge.
    let (tpx, tpy) = (bx + bwidth / 2.0, by + bheight);
    let tick_brush = Brush::resolve(colors.editor_bg, Brush::default());
    for rect in [
        Rect::new(bx, by, bw, bheight),
        Rect::new(bx, by + bheight - bw, bwidth, bw),
    ] {
        out.push(DisplayItem::Rect(rotated_about(
            rect,
            tpx,
            tpy,
            cb.check_rotation_deg,
            0.0,
            tick_brush,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    fn rects(items: &[DisplayItem]) -> Vec<FilledRect> {
        items
            .iter()
            .filter_map(|i| match i {
                DisplayItem::Rect(r) => Some(*r),
                _ => None,
            })
            .collect()
    }

    // ---- §2: blockquote bar 2px wide at 15px inset, full height ------------

    #[test]
    fn the_blockquote_bar_is_two_pixels_at_fifteen_and_spans_the_quote() {
        let theme = Theme::muya_default();
        let bar = blockquote_bar(
            &theme,
            Rect::new(100.0, 40.0, 700.0, 250.0),
            Brush::rgb(1, 2, 3),
        );
        assert_eq!(bar.rect, Rect::new(115.0, 40.0, 2.0, 250.0));
        assert_eq!(bar.brush, Brush::rgb(1, 2, 3));
    }

    // ---- §2: thematic break 2px dashed, centred at top: 50% ----------------

    #[test]
    fn the_thematic_break_is_centred_in_its_line_box() {
        let theme = Theme::muya_default();
        let line = thematic_break_rule(
            &theme,
            Rect::new(0.0, 100.0, 700.0, 25.6),
            Brush::rgb(9, 9, 9),
        );
        assert_eq!((line.x0, line.x1), (0.0, 700.0));
        // 0.5 × 25.6 = 12.8 is where the ::before's **border box** is centred,
        // not where the rule is painted. The box is `height: 2px` plus a 2px
        // `border-top` at the default `content-box`, so it spans [10.8, 14.8]
        // and only its top 2px paint: [10.8, 12.8], centre 11.8.
        //
        // This number moved at the post-review fix: it used to assert 12.8 and
        // was defending a rule painted one pixel — half its own width — low.
        assert!((line.y0 - 111.8).abs() < 1e-4, "got {}", line.y0);
        assert_eq!(line.y0, line.y1);
        assert_eq!(line.width, 2.0);
        assert_eq!(line.style, crate::theme::LineStyle::Dashed);
    }

    /// `ulysses` is a half-width rule centred by `translateX(-50%)`, which D4
    /// normalised to `left_fraction = 0.25`. If the fractions were applied to
    /// anything but the content width this would land off-centre.
    #[test]
    fn a_half_width_rule_is_centred_by_its_fractions_alone() {
        let mut theme = Theme::muya_default();
        theme.thematic_break.width_fraction = 0.5;
        theme.thematic_break.left_fraction = 0.25;
        let line = thematic_break_rule(&theme, Rect::new(0.0, 0.0, 700.0, 26.0), Brush::default());
        assert_eq!((line.x0, line.x1), (175.0, 525.0));
    }

    // ---- borders -----------------------------------------------------------

    #[test]
    fn a_one_pixel_border_is_four_fills_inside_the_border_box() {
        let mut out = Vec::new();
        push_border(
            &mut out,
            Rect::new(10.0, 20.0, 100.0, 50.0),
            1.0,
            Brush::default(),
        );
        let r = rects(&out);
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].rect, Rect::new(10.0, 20.0, 100.0, 1.0));
        assert_eq!(r[1].rect, Rect::new(10.0, 69.0, 100.0, 1.0));
        assert_eq!(r[2].rect, Rect::new(10.0, 21.0, 1.0, 48.0));
        assert_eq!(r[3].rect, Rect::new(109.0, 21.0, 1.0, 48.0));
    }

    /// Correction C1: `dark` and 29 other themes zero the code-block border.
    /// A zero-width border must push nothing at all, not a zero-area rect that
    /// every golden then carries.
    #[test]
    fn a_zero_width_or_invisible_border_pushes_nothing() {
        let mut out = Vec::new();
        push_border(
            &mut out,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            0.0,
            Brush::default(),
        );
        push_border(
            &mut out,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            1.0,
            Brush::TRANSPARENT,
        );
        assert!(out.is_empty());
    }

    // ---- rotation ----------------------------------------------------------

    /// A rotation about a pivot is a rotation about the centre plus a
    /// translation. Ninety degrees clockwise about the origin takes (1, 0) to
    /// (0, 1) with y down.
    #[test]
    fn rotating_about_a_pivot_moves_the_centre_and_keeps_the_angle() {
        let r = Rect::new(0.0, -1.0, 2.0, 2.0); // centre (1, 0)
        let out = rotated_about(r, 0.0, 0.0, 90.0, 0.0, Brush::default());
        assert!((out.rect.x - -1.0).abs() < 1e-5);
        assert!((out.rect.y - 0.0).abs() < 1e-5);
        assert_eq!(out.rotation_deg, 90.0);
        assert_eq!((out.rect.width, out.rect.height), (2.0, 2.0));
    }

    #[test]
    fn rotating_about_a_rectangles_own_centre_leaves_it_where_it_was() {
        let r = Rect::new(4.0, 6.0, 8.0, 2.0);
        let out = rotated_about(r, 8.0, 7.0, -45.0, 0.0, Brush::default());
        assert!((out.rect.x - 4.0).abs() < 1e-5);
        assert!((out.rect.y - 6.0).abs() < 1e-5);
    }

    // ---- §2: task checkbox, 12×12 target inside an 18×18 ring --------------

    #[test]
    fn the_checkbox_ring_is_eighteen_pixels_offset_minus_two_from_the_target() {
        let theme = Theme::muya_default();
        let mut out = Vec::new();
        push_checkbox(&mut out, &theme, 100.0, 200.0, false);
        let r = rects(&out);
        assert_eq!(r.len(), 2, "an unchecked box is a ring and nothing else");
        assert_eq!(r[0].rect, Rect::new(98.0, 198.0, 18.0, 18.0));
        assert_eq!(r[0].corner_radius, 9.0, "border-radius: 50% of 18px");
        assert_eq!(r[1].rect, Rect::new(100.0, 200.0, 14.0, 14.0));
        assert_eq!(r[1].corner_radius, 7.0);
        assert_eq!(
            r[1].brush,
            Brush::resolve(theme.colors.editor_bg, Brush::default())
        );
    }

    #[test]
    fn checking_the_box_paints_the_ring_with_the_accent_and_adds_a_tick() {
        let theme = Theme::muya_default();
        let mut out = Vec::new();
        push_checkbox(&mut out, &theme, 0.0, 0.0, true);
        let r = rects(&out);
        assert_eq!(
            r.len(),
            4,
            "ring, ring interior, and the tick's two borders"
        );
        let accent = Brush::resolve(theme.colors.theme, Brush::default());
        assert_eq!(r[0].brush, accent);
        assert_eq!(r[1].brush, accent);
        for tick in &r[2..] {
            assert_eq!(tick.rotation_deg, -45.0);
            assert_eq!(
                tick.brush,
                Brush::resolve(theme.colors.editor_bg, Brush::default())
            );
        }
    }

    /// The outlier set: a 16×16 square ring flush with a 16×16 target, rotated
    /// −90° while unchecked. If the rotation were applied about each fill's own
    /// centre instead of the box's, the two fills would separate — the whole
    /// reason [`rotated_about`] exists.
    #[test]
    fn the_outlier_checkbox_rotates_as_one_element() {
        let mut theme = Theme::muya_default();
        theme.checkbox.shape = crate::theme::CheckboxShape::Square;
        theme.checkbox.box_size_px = 16.0;
        theme.checkbox.ring_size_px = 16.0;
        theme.checkbox.ring_offset_px = 0.0;
        theme.checkbox.ring_corner_radius_px = 2.0;
        theme.checkbox.unchecked_rotation_deg = -90.0;
        let mut out = Vec::new();
        push_checkbox(&mut out, &theme, 50.0, 50.0, false);
        let r = rects(&out);
        assert_eq!(r.len(), 2);
        // Both fills are square and concentric with the 16×16 box, so a
        // rotation about that centre must leave both origins untouched.
        assert_eq!(r[0].rect, Rect::new(50.0, 50.0, 16.0, 16.0));
        assert_eq!(r[1].rect, Rect::new(52.0, 52.0, 12.0, 12.0));
        assert_eq!(r[0].rotation_deg, -90.0);
        assert_eq!(r[1].rotation_deg, -90.0);
    }

    // ---- bullets -----------------------------------------------------------

    #[test]
    fn a_disc_is_a_circle_and_a_square_is_not() {
        let mut out = Vec::new();
        push_bullet(
            &mut out,
            &ListMarker::Disc,
            0.0,
            0.0,
            100.0,
            50.0,
            16.0,
            Brush::default(),
            Brush::rgb(255, 255, 255),
        );
        let r = rects(&out);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].rect, Rect::new(94.4, 47.2, 5.6, 5.6));
        assert_eq!(r[0].corner_radius, 2.8);

        let mut out = Vec::new();
        push_bullet(
            &mut out,
            &ListMarker::Square,
            0.0,
            0.0,
            100.0,
            50.0,
            16.0,
            Brush::default(),
            Brush::rgb(255, 255, 255),
        );
        assert_eq!(rects(&out)[0].corner_radius, 0.0);

        let mut out = Vec::new();
        push_bullet(
            &mut out,
            &ListMarker::Circle,
            0.0,
            0.0,
            100.0,
            50.0,
            16.0,
            Brush::default(),
            Brush::rgb(255, 255, 255),
        );
        let r = rects(&out);
        assert_eq!(r.len(), 2, "hollow, so a fill plus a background fill");
        assert_eq!(r[1].brush, Brush::rgb(255, 255, 255));
    }

    /// `ulysses`'s marker is literal `px` against the list item and ignores the
    /// gap rule entirely.
    #[test]
    fn the_ulysses_dash_marker_uses_its_own_literal_offsets() {
        let mut out = Vec::new();
        push_bullet(
            &mut out,
            &ListMarker::Dash(DashMarker {
                width_px: 5.0,
                height_px: 2.0,
                left_px: -18.0,
                top_px: 15.0,
            }),
            200.0,
            300.0,
            999.0,
            999.0,
            16.0,
            Brush::default(),
            Brush::default(),
        );
        assert_eq!(rects(&out)[0].rect, Rect::new(182.0, 315.0, 5.0, 2.0));
    }

    #[test]
    fn inherit_on_the_list_marker_takes_the_surrounding_colour() {
        // muya's own `--list-marker-color` is the keyword `inherit`, so this is
        // the default path rather than an edge case (C4).
        let theme = Theme::muya_default();
        assert_eq!(theme.colors.list_marker, Color::Inherit);
        assert_eq!(
            Brush::resolve(theme.colors.list_marker, Brush::rgb(7, 7, 7)),
            Brush::rgb(7, 7, 7)
        );
    }
}
