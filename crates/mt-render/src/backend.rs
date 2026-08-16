//! D20's trait, and the two neutral types on either side of it.
//!
//! > **Decision: the trait's methods are the intersection of what `vello_cpu`
//! > 0.2.0 and `vello`/`wgpu` both already do in this repository's own spikes,
//! > and nothing else. Its unit of work is a `DisplayList` plus a viewport, and
//! > its output is a buffer of pixels — not a scene, not a command list, not a
//! > device.**
//!
//! # Why the output is not a `Pixmap`
//!
//! `vello_cpu::Pixmap` is the obvious return type and it is the exact mistake
//! D20 was written to prevent: a trait typed on its one implementor's buffer is
//! shaped like that implementor, and the swap it was insurance for turns out to
//! need a different shape. The GPU spike's frame does not arrive as a `Pixmap`
//! — it arrives as bytes read back out of a `wgpu` buffer with a 256-byte row
//! stride (`spikes/e2-vello-gpu/src/main.rs:135`). [`Pixels`] is what both can
//! produce: width, height, and premultiplied sRGB RGBA8 with no padding.
//!
//! # Why the caller owns the buffer
//!
//! D17 measures **N separate process launches** of a full-viewport repaint from
//! a warm display list. A `render` that allocated its own target would put a
//! multi-megabyte allocation inside the thing being timed, which is a
//! measurement of the allocator. The caller keeps one [`Pixels`] and hands it
//! back every frame.

use mt_layout::{DisplayList, FontId, Rect};

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

/// What a caller asks for when it asks for one frame.
///
/// Two fields, and both are in the intersection D20 names. The viewport is
/// D18's — *"`mt-render` takes a viewport `Rect` and skips blocks whose
/// `bounds` do not intersect it"* — and it is the only place a scroll offset
/// enters this crate. The ground colour is `vello`'s
/// `RenderParams::base_color` and `vello_cpu`'s pixmap clear, which is to say
/// it is a parameter of a frame in both backends rather than an invention here.
///
/// # The ground is not on the display list, and that is deliberate
///
/// `DisplayList` carries no page background: the blocks start at the top-left
/// of the content column and the editor's own `--editor-bg-color` is chrome
/// around them, not a block. A renderer that defaulted it to transparent would
/// make `dark`'s `#ffffffb3` prose invisible in every viewer, and one that
/// defaulted it to white would silently disagree with the theme. So it is
/// required, and the caller reads it from `theme.colors.editor_bg`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The document-space rectangle this frame shows, y down.
    ///
    /// Its origin is the scroll offset. Its size should be the target's size in
    /// pixels; nothing here derives one from the other, and a mismatch simply
    /// means the raster is cut off or padded, which is `render_with`'s own
    /// documented behaviour.
    pub viewport: Rect,
    /// The ground every item is painted onto.
    pub background: mt_layout::Brush,
}

// ---------------------------------------------------------------------------
// Pixels
// ---------------------------------------------------------------------------

/// A frame's pixels: premultiplied sRGB RGBA8, row-major, four bytes per pixel,
/// no row padding.
///
/// **Premultiplied**, because that is what both backends hand back and
/// converting on the way out would cost a pass over the buffer per frame for a
/// consumer (a window surface) that wants premultiplied anyway. `to_png` is
/// where the un-premultiply happens, once, off the frame path.
///
/// Dimensions are `u16` rather than `u32` because `vello_cpu::Pixmap`'s are,
/// and a type that admits sizes the rasterizer cannot represent would push the
/// failure to a `try_into` somewhere less useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    width: u16,
    height: u16,
    data: Vec<u8>,
}

impl Pixels {
    /// A transparent buffer of the given size.
    pub fn new(width: u16, height: u16) -> Pixels {
        Pixels {
            width,
            height,
            data: vec![0; Self::byte_len(width, height)],
        }
    }

    /// Resize in place, keeping the allocation when it is already big enough.
    ///
    /// The contents are **not** preserved: every frame under D17 is a full
    /// repaint, so preserving them would be dead work with a plausible-looking
    /// name.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.data.clear();
        self.data.resize(Self::byte_len(width, height), 0);
    }

    /// Width in pixels.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The whole buffer.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The whole buffer, mutably. A backend's only way to write one.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// One pixel as `[r, g, b, a]`, premultiplied.
    ///
    /// For tests and for hit-free inspection; not a drawing primitive. Returns
    /// transparent for a coordinate outside the buffer rather than panicking,
    /// because the callers that want it are assertions and a panic there says
    /// less than a value does.
    pub fn pixel(&self, x: u16, y: u16) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = (usize::from(y) * usize::from(self.width) + usize::from(x)) * 4;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }

    /// This frame as PNG bytes.
    ///
    /// **D19 leans on this costing nothing**: *"PNG encode and decode arrive at
    /// zero cost — `png` is a default feature of `vello_cpu` 0.2.0, so
    /// `Pixmap::into_png` and `Pixmap::from_png` exist the moment the backend
    /// is added."* This is that sentence cashed, and it is the reason the pixel
    /// goldens need no comparison crate: `nv-flip` wants a C++ toolchain on
    /// three runners and `dssim`/`image-compare` pull families this workspace
    /// does not carry.
    ///
    /// The error is a `String` because it is `png::EncodingError`, and `png` is
    /// a crate this one does not name — it arrives through `vello_cpu`. A
    /// variant on [`RenderError`] would put a third party's failure mode on a
    /// type whose whole point is that the display list has only one.
    ///
    /// Off the frame path on purpose: this is where the un-premultiply happens,
    /// once, rather than on every repaint D17 times.
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        let data: Vec<vello_cpu::peniko::color::PremulRgba8> = self
            .data
            .chunks_exact(4)
            .map(|p| vello_cpu::peniko::color::PremulRgba8 {
                r: p[0],
                g: p[1],
                b: p[2],
                a: p[3],
            })
            .collect();
        vello_cpu::Pixmap::from_parts(data, self.width, self.height)
            .into_png()
            .map_err(|e| e.to_string())
    }

    /// Whether every pixel is fully transparent.
    ///
    /// The predicate three of this crate's tests are written around: *nothing
    /// was drawn* is the assertion `LineStyle::None` and an invisible brush
    /// both have to satisfy, and it is stronger than *"the pixel I looked at is
    /// empty"*.
    pub fn is_blank(&self) -> bool {
        self.data.iter().all(|&b| b == 0)
    }

    /// How many pixels are not fully transparent.
    pub fn painted_pixels(&self) -> usize {
        self.data.chunks_exact(4).filter(|p| p[3] != 0).count()
    }

    fn byte_len(width: u16, height: u16) -> usize {
        usize::from(width) * usize::from(height) * 4
    }
}

// ---------------------------------------------------------------------------
// FrameStats
// ---------------------------------------------------------------------------

/// What one frame actually drew.
///
/// D17 requires the frame time to be reported beside *"how many of `5mb.md`'s
/// **54,896 blocks** intersect the viewport at each [scroll offset]"*, on the
/// argument that a frame time alone is unfalsifiable. Returning it from the
/// render call rather than recomputing it is the difference between one number
/// and two numbers that can disagree.
///
/// It is also what makes D18's culling testable without reading pixels: *a
/// block outside the viewport is excluded and one straddling the edge is
/// included* is an assertion about these counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameStats {
    /// Blocks on the list, whether drawn or not.
    pub blocks_total: usize,
    /// Blocks whose `bounds` intersected the viewport.
    pub blocks_drawn: usize,
    /// Blocks clipped to their own `bounds` — see [`crate::cpu`]'s note on
    /// which those are.
    pub blocks_clipped: usize,
    /// Items handed to the rasterizer.
    pub items_drawn: usize,
    /// Items skipped without being drawn: an invisible brush, a
    /// [`LineStyle::None`](mt_layout::theme::LineStyle::None) rule, an empty
    /// glyph run, or an inline box (which M3 draws nothing in).
    pub items_skipped: usize,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong turning a display list into pixels.
///
/// One variant, and that is a finding rather than an oversight: the display
/// list is plain scalars, so nothing on it can fail to be understood. The one
/// thing a renderer cannot resolve by itself is the font handle behind a
/// [`FontId`], which is exactly what D16 exists to hand across — so the only
/// failure is the hand-off being wrong.
///
/// It is an error and not a panic, and not a silent skip, for D7's reason one
/// layer down: **a missing face draws a perfectly well-formed frame** with a
/// hole in it, and a renderer that swallowed it would produce an artifact
/// indistinguishable from correct output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// A glyph run named a face the [`FontTable`](crate::FontTable) does not
    /// hold.
    UnknownFont {
        /// The id on the run.
        font: FontId,
        /// How many faces the table holds, because *"font11 in a table of 4"*
        /// names the likely cause — two orderings that drifted — and *"font11
        /// is unknown"* does not.
        table_len: usize,
    },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::UnknownFont { font, table_len } => write!(
                f,
                "{font} is not in the font table ({table_len} face(s)). D16: the shell builds \
                 the table in the same order it built the `Fonts` collection, and a `FontId` is \
                 the registration index — so a short or reordered table is the usual cause."
            ),
        }
    }
}

impl std::error::Error for RenderError {}

// ---------------------------------------------------------------------------
// The trait
// ---------------------------------------------------------------------------

/// A backend: a display list and a frame in, pixels out.
///
/// See this module's doc for what shaped it, and the crate doc for the table of
/// what the two spikes have in common. It has exactly two methods and neither
/// invents a capability — [`name`](Renderer::name) is what both spikes already
/// print beside their output, and D17 forbids a frame rate quoted without the
/// backend and thread count that produced it.
pub trait Renderer {
    /// The backend's name, for the report D17 owes.
    fn name(&self) -> &'static str;

    /// Draw `list` as seen through `frame` into `target`.
    ///
    /// `target`'s dimensions are the raster size; `frame.viewport`'s origin is
    /// the scroll offset and its size is what culling tests against. The two
    /// are the caller's to keep consistent — deriving one from the other is
    /// precisely the geometry D18 forbids.
    fn render(
        &mut self,
        list: &DisplayList,
        frame: &Frame,
        target: &mut Pixels,
    ) -> Result<FrameStats, RenderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_buffer_is_transparent_and_the_right_size() {
        let p = Pixels::new(3, 2);
        assert_eq!(p.width(), 3);
        assert_eq!(p.height(), 2);
        assert_eq!(p.data().len(), 3 * 2 * 4);
        assert!(p.is_blank());
        assert_eq!(p.painted_pixels(), 0);
    }

    #[test]
    fn resize_reshapes_the_buffer_and_does_not_carry_the_old_frame_over() {
        let mut p = Pixels::new(2, 2);
        p.data_mut().fill(0xff);
        p.resize(4, 1);
        assert_eq!((p.width(), p.height()), (4, 1));
        assert_eq!(p.data().len(), 4 * 4);
        assert!(
            p.is_blank(),
            "a resized buffer that kept its pixels would make the next frame's diff a lie"
        );
    }

    #[test]
    fn a_pixel_outside_the_buffer_reads_transparent_rather_than_panicking() {
        let mut p = Pixels::new(1, 1);
        p.data_mut().copy_from_slice(&[1, 2, 3, 4]);
        assert_eq!(p.pixel(0, 0), [1, 2, 3, 4]);
        assert_eq!(p.pixel(1, 0), [0, 0, 0, 0]);
        assert_eq!(p.pixel(0, 1), [0, 0, 0, 0]);
    }

    /// The message has to name the length, because the failure D16 predicts is
    /// *two orderings that drifted* and a bare id cannot show that.
    #[test]
    fn the_unknown_font_error_names_the_table_length() {
        let e = RenderError::UnknownFont {
            font: FontId::from_index(11),
            table_len: 4,
        };
        let text = e.to_string();
        assert!(text.contains("font11"), "{text}");
        assert!(text.contains("4 face(s)"), "{text}");
    }
}
