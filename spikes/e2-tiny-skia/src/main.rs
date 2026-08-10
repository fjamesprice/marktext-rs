//! E2 build 3 — build 1 (parley + `vello_cpu`) plus `tiny-skia`.
//!
//! `tiny-skia` cannot rasterize text (its own README lists that as out of
//! scope), so its branch below draws paths and rects instead — real vector
//! work, not a no-op call made just to keep the linker honest.
//!
//! ## The DCE guard
//!
//! With `lto = "fat"` and `opt-level = "s"` (the `dist` profile), the linker
//! will delete any renderer that is linked but never *reachably* called. Both
//! branches here are gated on `std::env::args()`, which the optimizer cannot
//! evaluate at compile time, so neither the `vello_cpu` path nor the
//! `tiny-skia` path can be proven dead — and each one's result is hashed and
//! printed, so the work inside can't be proven unobservable either.
fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    if arg == "skia" {
        run_tiny_skia();
    } else {
        e2_common::run_and_print_baseline("tiny_skia+vello_cpu");
    }
}

/// Real `tiny-skia` work: a filled rect, an overlapping semi-transparent
/// rect, and a stroked circle — the same three-shape shape as `vello_cpu`'s
/// own `examples/basic.rs`, so the two renderers are exercised comparably.
fn run_tiny_skia() {
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

    let mut pixmap = Pixmap::new(100, 100).expect("100x100 pixmap");

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(30, 90, 200, 255));
    let rect = tiny_skia::Rect::from_ltrb(25.0, 25.0, 75.0, 75.0).unwrap();
    let path = PathBuilder::from_rect(rect);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    let mut paint2 = Paint::default();
    paint2.set_color(Color::from_rgba8(200, 40, 40, 128));
    let rect2 = tiny_skia::Rect::from_ltrb(50.0, 50.0, 85.0, 85.0).unwrap();
    let path2 = PathBuilder::from_rect(rect2);
    pixmap.fill_path(
        &path2,
        &paint2,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    let mut paint3 = Paint::default();
    paint3.set_color(Color::from_rgba8(40, 180, 90, 255));
    paint3.anti_alias = true;
    let mut pb = PathBuilder::new();
    pb.push_circle(50.0, 50.0, 30.0);
    let circle = pb.finish().unwrap();
    pixmap.stroke_path(
        &circle,
        &paint3,
        &Stroke::default(),
        Transform::identity(),
        None,
    );

    let bytes = pixmap.data();
    let hash = e2_common::fnv1a(bytes);
    println!(
        "[tiny-skia] pixmap={}x{} bytes={} hash={hash:016x}",
        pixmap.width(),
        pixmap.height(),
        bytes.len()
    );
}
