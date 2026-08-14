//! The theme model — M3 §5 D4.
//!
//! > **Decision: a typed `Theme` struct whose fields are all required,
//! > deserialized from TOML. No CSS is parsed at runtime; the 32 shipped
//! > stylesheets are a *source* to be transcribed, not a format to be read.
//! > S1 delivers two themes, not 32.**
//!
//! # Why every field is required
//!
//! The variable surface MarkText actually has is a **colour** system. Of the 46
//! custom properties muya's own stylesheets consume, four are geometry
//! (`--editor-area-width`, `--mu-font-size`, `--mu-line-height`,
//! `--mu-code-font-size`) and two are font families; the rest are paint. Every
//! geometric thing a theme changes escapes the variable system and lands as a
//! literal rule on a pseudo-element. **So this schema is sized to what themes
//! override, not to what the variable system exposes** — which is why fields
//! neither shipped theme uses ([`Checkbox::shape`], [`ListMarker::Dash`],
//! [`ThematicBreak::width_fraction`], [`Headings::align`]) are here anyway.
//!
//! Required fields exist because the alternative has a live failure mode. The
//! default `light` theme has no `.theme.css` file: it is muya's base stylesheet
//! plus a two-line patch bridging **2 of the 41** kebab-case names muya reads
//! (`util/theme.ts:71-74`). Nothing anywhere reports that the other 39 were not
//! bridged. A struct whose fields are all required, with
//! `#[serde(deny_unknown_fields)]` so a typo is an error rather than an ignored
//! key, cannot have that failure mode.
//!
//! # What "muya-default" and "dark" are
//!
//! [`Theme::muya_default`] is muya's own `:root` block
//! (`packages/muya/src/assets/styles/index.css:1-55`) with every
//! `var(--x, fallback)` in `blockSyntax.css` / `inlineSyntax.css` resolved. Its
//! content column is **800px**, because the kebab-case alias that carries the
//! desktop's 750px exists only inside the 32 named theme files — an install
//! that never picks a theme gets muya's own default.
//!
//! [`Theme::dark`] is `dark.theme.css`, which is not one of the three geometry
//! outliers. Its content column is **750px**, and — see [`CodeBlock`] — its
//! code-block border is **0px**, not muya's 1px.
//!
//! # Units, and what each number multiplies
//!
//! CSS resolves lengths against three different bases and the field names say
//! which:
//!
//! - `*_px` is an absolute device-independent pixel.
//! - `*_em` multiplies the **element's own** computed font size, which for a
//!   heading is [`Metrics::font_size_px`] × [`Headings::scale_em`] and for a
//!   code block is [`Metrics::font_size_px`] × [`CodeBlock::font_size_pct`].
//! - `*_rem` multiplies [`Metrics::root_font_size_px`], the `html` font size,
//!   which `--mu-font-size` does **not** change: it is set on `.mu-editor`, so
//!   a reader who enlarges the editor font keeps 16px heading margins.
//! - `*_pct` is a percentage of the **parent's** font size.
//!
//! # Colour representation
//!
//! [`Color`] is 8-bit sRGB with 8-bit alpha, plus the CSS keyword `inherit`,
//! which three of muya's own defaults use (`--strong-color`, `--em-color`,
//! `--list-marker-color`). Fractional alphas are rounded to the nearest 1/255
//! on transcription: `rgba(255, 255, 255, 0.7)` becomes `#ffffffb3`.

use serde::Deserialize;
use std::fmt;

/// muya's own base stylesheet as a theme. Content column 800px.
///
/// Embedded at compile time. `mt-layout` performs no I/O — see the crate doc's
/// dependency constraints — so the shipped themes are `include_str!`, and a
/// theme read from a user's config directory is `mt-fs`'s job to fetch and
/// [`Theme::from_toml_str`]'s job to parse.
pub const MUYA_DEFAULT_TOML: &str = include_str!("../themes/muya-default.toml");

/// MarkText's shipped `dark` theme. Content column 750px, no code-block border.
pub const DARK_TOML: &str = include_str!("../themes/dark.toml");

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/// Everything layout needs to turn a `Document` into positioned blocks, plus
/// everything rendering needs to paint them.
///
/// Grouped by the construct each number belongs to rather than by the CSS file
/// it came from, because a later reader is asking "how tall is a blockquote?",
/// not "what is on line 149 of `blockSyntax.css`?".
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    /// The theme's identifier, matching its file stem.
    pub name: String,
    /// Page-level geometry: column width, padding, root type scale.
    pub metrics: Metrics,
    /// The three font stacks, in fallback order.
    pub fonts: Fonts,
    /// h1–h6.
    pub headings: Headings,
    /// Block quote box and its left bar.
    pub blockquote: Blockquote,
    /// Fenced and indented code, and — per D11 — frontmatter, HTML, math and
    /// diagram blocks, which share the same box.
    pub code_block: CodeBlock,
    /// `` `code` `` inside a paragraph.
    pub inline_code: InlineCode,
    /// Bullet, ordered and task lists.
    pub lists: Lists,
    /// The task-list checkbox, the single construct the three geometry
    /// outliers reshape most.
    pub checkbox: Checkbox,
    /// Table figure, inner table, and cells.
    pub table: Table,
    /// `---`.
    pub thematic_break: ThematicBreak,
    /// Footnote definition block.
    pub footnote: Footnote,
    /// The colours muya's document stylesheets consume.
    pub colors: Colors,
    /// The colours muya's *chrome* stylesheets consume. Carried so that M5's
    /// importer round-trips a theme file, and never read by `mt-layout`.
    pub chrome: ChromeColors,
    /// Syntax highlighting, keyed by Prism token class.
    pub code_palette: CodePalette,
}

impl Theme {
    /// Parse a theme from TOML.
    ///
    /// Fails if any field is missing or any key is unrecognised. Both are
    /// deliberate: D4's argument for a typed schema is that the CSS model's
    /// failure mode is silence, and an optional field would reintroduce it.
    pub fn from_toml_str(src: &str) -> Result<Theme, ThemeError> {
        toml::from_str(src).map_err(|e| ThemeError {
            message: e.to_string(),
        })
    }

    /// muya's own base stylesheet as a theme. Content column 800px.
    ///
    /// # Panics
    ///
    /// Never in a build that passes its own tests: the source is embedded at
    /// compile time and [`tests::both_shipped_themes_parse`] is the guard.
    pub fn muya_default() -> Theme {
        Theme::from_toml_str(MUYA_DEFAULT_TOML).expect("the embedded muya-default theme must parse")
    }

    /// MarkText's shipped `dark` theme. Content column 750px.
    ///
    /// # Panics
    ///
    /// See [`Theme::muya_default`].
    pub fn dark() -> Theme {
        Theme::from_toml_str(DARK_TOML).expect("the embedded dark theme must parse")
    }

    /// The width available to a block's content: the column minus the
    /// container's horizontal padding on both sides.
    ///
    /// This is the number the whole milestone's 800/750 golden comparison turns
    /// on, so it is computed in one place rather than at each call site.
    pub fn content_width_px(&self) -> f32 {
        self.metrics.content_width_px - 2.0 * self.metrics.container_padding_x_px
    }
}

/// A theme file that could not be parsed.
///
/// Carries the message rather than the underlying `toml` error so that the
/// dependency does not appear in `mt-layout`'s public API, which would make a
/// `toml` version bump a breaking change for every consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeError {
    message: String,
}

impl ThemeError {
    /// The parser's message, including the line and column when it has one.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid theme: {}", self.message)
    }
}

impl std::error::Error for ThemeError {}

// ---------------------------------------------------------------------------
// Page metrics
// ---------------------------------------------------------------------------

/// Page-level geometry — `.mu-editor` and `.mu-container`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    /// `.mu-container { max-width }` — `blockSyntax.css:18`. 800px in muya's
    /// own sheet, 750px in all 32 shipped theme files.
    pub content_width_px: f32,
    /// `.mu-container { padding }` top — `blockSyntax.css:21`, `0 50px 100px`.
    pub container_padding_top_px: f32,
    /// The `50px` of `0 50px 100px`, applied on both sides.
    pub container_padding_x_px: f32,
    /// The `100px` of `0 50px 100px`, which exists so the last block can be
    /// scrolled clear of the window bottom.
    pub container_padding_bottom_px: f32,
    /// `.mu-editor { font-size: var(--mu-font-size, 16px) }` —
    /// `blockSyntax.css:2`. The base every `em` in a body block resolves
    /// against.
    pub font_size_px: f32,
    /// `.mu-editor { line-height: var(--mu-line-height, 1.6) }` —
    /// `blockSyntax.css:4`. A multiplier on the element's own font size.
    pub line_height: f32,
    /// `html, body { font-size: 16px }` — `index.css:206`. What `rem`
    /// resolves against, and **not** the same knob as
    /// [`Metrics::font_size_px`]: `--mu-font-size` is set on `.mu-editor`, so
    /// changing the editor font leaves every `rem` where it was.
    pub root_font_size_px: f32,
    /// `margin: 0.5em 0` on `p`, `blockquote`, `ul`, `ol`, `dl`, `table` and
    /// non-table `figure` — `blockSyntax.css:26-35`.
    pub block_margin_em: f32,
}

/// The font stacks, each in fallback order.
///
/// **No theme file sets `font-family`** — zero declarations across all 32 — so
/// these are muya's own stacks and their shipped value is identical everywhere.
/// They are theme fields anyway because D7's face list is a property of them,
/// and because a family that resolves to nothing must be a hard error rather
/// than a silently substituted default.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fonts {
    /// `blockSyntax.css:3`, and identically `index.css:207`.
    pub body: Vec<String>,
    /// `blockSyntax.css:220-227`, the `--mu-code-font-family` fallback. Reaches
    /// `.mu-code-block` only.
    pub code: Vec<String>,
    /// `inlineSyntax.css:66`. A **literal** stack, not the variable: inline
    /// code does not follow the code-block font setting, which
    /// `blockSyntax.css:217`'s own comment says out loud. The shipped value
    /// happens to equal [`Fonts::code`].
    pub inline_code: Vec<String>,
}

// ---------------------------------------------------------------------------
// Headings
// ---------------------------------------------------------------------------

/// h1–h6 — `blockSyntax.css:37-83`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Headings {
    /// h1 through h6, as multiples of the container's font size.
    ///
    /// muya's scale is **not** the browser's: h1 is `1.875em` where the UA
    /// default is `2em`, and h3 is `1.375em` where the UA default is `1.17em`.
    /// There is no algorithm here to port — every value is a decision someone
    /// made in CSS, which is why it is transcribed and asserted rather than
    /// computed.
    pub scale_em: [f32; 6],
    /// `margin: 1rem 0` — `blockSyntax.css:45`. **`rem`, not `em`**: the same
    /// 16px above an h1 and an h6.
    pub margin_rem: f32,
    /// `line-height: 1.4` — `blockSyntax.css:50`. Overrides the editor's 1.6.
    pub line_height: f32,
    /// `font-weight: bold` — `blockSyntax.css:49`.
    pub bold: bool,
    /// muya sets no `text-align`, so headings start-align. `ulysses` centres
    /// every one of them (`ulysses.theme.css:147-155`), which is the whole
    /// reason this field exists.
    pub align: TextAlign,
}

/// CSS `text-align`, in its logical spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign {
    /// The inline start edge — left in LTR.
    Start,
    /// Centred.
    Center,
    /// The inline end edge — right in LTR.
    End,
}

// ---------------------------------------------------------------------------
// Block quote
// ---------------------------------------------------------------------------

/// Block quote — `blockSyntax.css:137-165`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blockquote {
    /// From the shared `margin: 0.5em 0` rule; `margin-right` and
    /// `margin-left` are then explicitly zeroed at `blockSyntax.css:140-141`.
    pub margin_em: f32,
    /// `padding: 0 30px` — `blockSyntax.css:142`.
    pub padding_x_px: f32,
    /// `font-size: 1em` — `blockSyntax.css:146`. Explicit, so a nested quote
    /// does not compound.
    pub font_size_em: f32,
    /// The `::before` bar's width — `blockSyntax.css:155`.
    pub bar_width_px: f32,
    /// The bar's `left` — `blockSyntax.css:152`. It spans the quote's full
    /// height (`height: 100%`).
    pub bar_inset_px: f32,
    /// `blockquote blockquote { padding-right: 0 }` —
    /// `blockSyntax.css:163-165`. A quote inside a quote loses its trailing
    /// padding but keeps its leading padding.
    pub nested_padding_end_px: f32,
}

// ---------------------------------------------------------------------------
// Code
// ---------------------------------------------------------------------------

/// Code block — `blockSyntax.css:197-228` and `:322-367`.
///
/// Also the box for frontmatter, HTML, math and diagram blocks, which share the
/// same rule (`blockSyntax.css:197-201`) and which D11 lays out as their own
/// source text.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeBlock {
    /// `margin-top: 1.5em` — `blockSyntax.css:204`.
    pub margin_top_em: f32,
    /// **Not in muya's CSS.** `.mu-code-block` is a `<pre>` and muya sets only
    /// `margin-top`, so the bottom margin is Chromium's UA `pre { margin: 1em
    /// 0 }`. A Rust layout engine has no UA stylesheet, so the number has to be
    /// stated here or the gap silently vanishes.
    pub margin_bottom_em: f32,
    /// `padding: 1em` — `blockSyntax.css:205`.
    pub padding_em: f32,
    /// `font-size: var(--mu-code-font-size, 90%)` — `blockSyntax.css:219`. A
    /// percentage of the **parent's** size, so 14.4px inside a 16px container.
    pub font_size_pct: f32,
    /// `line-height: 1.6` — `blockSyntax.css:210`. Multiplies the code block's
    /// own 14.4px, not the editor's 16px.
    pub line_height: f32,
    /// `border: 1px solid` — `blockSyntax.css:213`.
    ///
    /// **30 of the 32 shipped themes set `border: none !important` on
    /// `pre.mu-code-block`** (`dark.theme.css:200-206` and its 29 siblings),
    /// so this is 0 for almost every named theme and 1 only for muya's own
    /// default and for `graphite` and `ulysses`. That makes it the second
    /// geometry axis on which `dark` differs from `muya-default`.
    pub border_width_px: f32,
    /// `border-radius: 3px` — `blockSyntax.css:214`.
    pub corner_radius_px: f32,
    /// The line-number gutter's width, and the extra `padding-left` the block
    /// takes when the gutter is on — `blockSyntax.css:323-334`.
    pub gutter_width_em: f32,
    /// `.mu-line-numbers-rows { top: 1em }` — `blockSyntax.css:329`, matching
    /// the block's own padding.
    pub gutter_top_em: f32,
    /// `padding-right: 0.8em` on each number — `blockSyntax.css:359`.
    pub gutter_padding_end_em: f32,
    /// `transform: scale(0.8)` on each number — `blockSyntax.css:364`.
    pub gutter_scale: f32,
    /// `letter-spacing: -1px` — `blockSyntax.css:336`.
    pub gutter_letter_spacing_px: f32,
}

/// Inline code — `inlineSyntax.css:60-70`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InlineCode {
    /// The `0.4em` of `padding: 0.2em 0.4em`.
    pub padding_x_em: f32,
    /// The `0.2em` of `padding: 0.2em 0.4em`.
    pub padding_y_em: f32,
    /// `font-size: 0.8em` — relative to the **surrounding** text, so inline
    /// code in an h1 is 24px and in a paragraph is 12.8px.
    pub font_size_em: f32,
    /// `border-radius: 3px`.
    pub corner_radius_px: f32,
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

/// Bullet, ordered and task lists — `blockSyntax.css:400-482`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lists {
    /// `padding-inline-start: 30px` — `blockSyntax.css:410`. Logical, so it
    /// mirrors under `dir="rtl"`.
    pub indent_px: f32,
    /// `ul.mu-task-list { padding-inline-start: 30px }` —
    /// `blockSyntax.css:481`. Its own declaration, so a theme could diverge.
    pub task_indent_px: f32,
    /// From the shared `margin: 0.5em 0` rule.
    pub margin_em: f32,
    /// `li > ol, li > ul { margin: 0 }` — `blockSyntax.css:422-425`. A nested
    /// list carries no vertical margin.
    pub nested_margin_em: f32,
    /// The bullet glyph per nesting depth; the last entry repeats for deeper
    /// levels.
    ///
    /// muya follows the browser cascade — disc, then circle, then square
    /// (`blockSyntax.css:417-456`). `ulysses` replaces the lot with a drawn
    /// rectangle; see [`ListMarker::Dash`].
    pub bullet_markers: Vec<ListMarker>,
    /// `ol.mu-order-list { list-style: decimal outside none }` —
    /// `blockSyntax.css:413-415`.
    pub ordered_marker: OrderedMarker,
}

/// A bullet-list marker.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListMarker {
    /// A filled circle — CSS `disc`, the first level.
    Disc,
    /// A hollow circle — CSS `circle`, the second level.
    Circle,
    /// A filled square — CSS `square`, the third level and deeper.
    Square,
    /// **`ulysses` alone.** `list-style: none` plus a `content: ''` overlay
    /// drawn in the gutter (`ulysses.theme.css:158-171`) — an element muya's
    /// own stylesheet does not define at all, and therefore the clearest
    /// evidence for D4's rule that the schema is sized to what themes
    /// override.
    Dash(DashMarker),
}

/// The rectangle `ulysses` draws in place of a bullet.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashMarker {
    /// `width: 5px` — `ulysses.theme.css:166`.
    pub width_px: f32,
    /// `height: 2px` — `ulysses.theme.css:167`.
    pub height_px: f32,
    /// `left: -18px` — `ulysses.theme.css:168`, relative to the list item.
    pub left_px: f32,
    /// `top: 15px` — `ulysses.theme.css:169`. An absolute offset, so it does
    /// not track the font size.
    pub top_px: f32,
}

/// An ordered-list marker — CSS `list-style-type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrderedMarker {
    /// `1.` `2.` `3.` — muya's, and the only one any shipped theme selects.
    Decimal,
    /// `a.` `b.` `c.`
    LowerAlpha,
    /// `A.` `B.` `C.`
    UpperAlpha,
    /// `i.` `ii.` `iii.`
    LowerRoman,
    /// `I.` `II.` `III.`
    UpperRoman,
}

// ---------------------------------------------------------------------------
// Task checkbox
// ---------------------------------------------------------------------------

/// The task-list checkbox — `blockSyntax.css:490-577`.
///
/// The construct the three geometry outliers reshape: `graphite`, `one-dark`
/// and `ulysses` replace muya's 12×12 circular target at `-23px` with a 16×16
/// **square** at `-24px` carrying a 9×5 checkmark, rotated −90° while unchecked.
/// Not one of those rules is `!important` — they win on source order, the theme
/// sheet being appended after muya's — and they *do* use CSS variables for
/// their colours. What escapes the variable system is the geometry alone.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkbox {
    /// Round or square. muya draws `border-radius: 50%`
    /// (`blockSyntax.css:538`); the three outliers draw `border-radius: 2px`
    /// (`graphite.theme.css:211`). A different **shape**, not a different size,
    /// which is why this is a discriminator and not a number.
    pub shape: CheckboxShape,
    /// `width` / `height` of the hit target — `blockSyntax.css:504-505`.
    /// 12 for muya, 16 for the outliers.
    pub box_size_px: f32,
    /// `inset-inline-start` — `blockSyntax.css:517`. −23 for muya, −24 for the
    /// outliers, which spell it as the physical `left`.
    pub inset_inline_start_px: f32,
    /// How the box's `top` is computed. The two families use different
    /// formulas, not different constants.
    pub top: CheckboxTop,
    /// `transform: rotate(…)` while unchecked, animating to 0 on check. muya
    /// does not rotate; the outliers start at −90°
    /// (`graphite.theme.css:194`).
    pub unchecked_rotation_deg: f32,
    /// The `::before` ring's `width` / `height` — `blockSyntax.css:533-534`.
    /// 18 for muya, 16 for the outliers, which make the ring the same size as
    /// the box.
    pub ring_size_px: f32,
    /// The ring's `top` and `left` relative to the box —
    /// `blockSyntax.css:528-529`. −2 for muya, 0 for the outliers.
    pub ring_offset_px: f32,
    /// `border: 2px solid` on the ring — `blockSyntax.css:537`.
    pub ring_border_px: f32,
    /// The ring's corner radius. muya's `border-radius: 50%` on an 18px box is
    /// 9px; the outliers use a literal 2px.
    pub ring_corner_radius_px: f32,
    /// The checkmark's `width` — `blockSyntax.css:553`. 8 for muya, 9 for the
    /// outliers.
    pub check_width_px: f32,
    /// The checkmark's `height` — `blockSyntax.css:554`. 4 for muya, 5 for the
    /// outliers.
    pub check_height_px: f32,
    /// The checkmark's stroke, drawn as a two-sided border —
    /// `blockSyntax.css:556`.
    pub check_border_px: f32,
    /// The checkmark's `left` — `blockSyntax.css:549`. 4 for muya, 5 for the
    /// outliers.
    pub check_left_px: f32,
    /// The checkmark's `top` — `blockSyntax.css:548`.
    pub check_top_px: f32,
    /// `transform: rotate(-45deg)` — `blockSyntax.css:559`. The tick is a
    /// rotated corner, not a glyph.
    pub check_rotation_deg: f32,
}

/// Round or square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckboxShape {
    /// muya's `border-radius: 50%`.
    Circle,
    /// The outliers' `border-radius: 2px`.
    Square,
}

/// How a checkbox's vertical position is derived.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckboxTop {
    /// muya: `calc(--mu-line-height * 0.5 * --mu-font-size - <offset>px)`,
    /// `blockSyntax.css:502` — the marker tracks the first text line's
    /// midpoint at any font size, which is why the CSS reads the variables
    /// rather than using `em`.
    LineCentered(LineCenteredTop),
    /// The three outliers: a flat `top: <em>em`, `graphite.theme.css:193`.
    Em(EmTop),
}

/// The offset subtracted from the first line's midpoint.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineCenteredTop {
    /// Half the ring's height minus the ring's own offset: 9 − 2 = 7.
    pub offset_px: f32,
}

/// A flat `em` offset from the list item's top.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmTop {
    /// Multiplies the list item's font size.
    pub em: f32,
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

/// Table — `blockSyntax.css:580-683` and `:972-974`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// `.mu-table { margin: 0.5em 0 0 }` — `blockSyntax.css:581`.
    /// **Asymmetric**: the table figure is excluded from the shared
    /// `figure:not(.mu-table)` rule and gets a top margin only.
    pub figure_margin_top_em: f32,
    /// The zero of `margin: 0.5em 0 0`.
    pub figure_margin_bottom_em: f32,
    /// `.mu-table { padding: 0.5em 0 }` — `blockSyntax.css:582`.
    pub figure_padding_y_em: f32,
    /// The inner `<table>` still matches `.mu-container table`'s
    /// `margin: 0.5em 0` (`blockSyntax.css:31-33`), which `.mu-table-inner`
    /// does not reset. Total clearance above a table is therefore
    /// 0.5 + 0.5 + 0.5 = 1.5em and below it 0.5 + 0.5 + 0 = 1em.
    pub inner_margin_em: f32,
    /// The `13px` of `padding: 6px 13px` — `blockSyntax.css:602`.
    pub cell_padding_x_px: f32,
    /// The `6px` of `padding: 6px 13px`.
    pub cell_padding_y_px: f32,
    /// `border: 1px solid` — `blockSyntax.css:630`, drawn on an
    /// absolutely-positioned `::before` overlay one pixel larger than the cell
    /// so neighbouring cells' borders coincide.
    pub cell_border_px: f32,
    /// `.mu-table-cell-content { min-width: 10em }` —
    /// `blockSyntax.css:972-974`. A floor on column width that §2's table
    /// omits and that column sizing cannot ignore.
    pub cell_min_width_em: f32,
    /// `border-collapse: collapse` — `blockSyntax.css:590`.
    pub collapse_borders: bool,
    /// `th { font-weight: bold }` — `blockSyntax.css:604`.
    pub header_bold: bool,
}

// ---------------------------------------------------------------------------
// Thematic break
// ---------------------------------------------------------------------------

/// `---` — `blockSyntax.css:168-194`.
///
/// The element is a paragraph carrying its own hidden source text, so it
/// occupies a **full text line**; the rule is a `::before` drawn across it. Two
/// themes redraw that pseudo-element outright and `ulysses` halves its width,
/// which is what [`ThematicBreak::width_fraction`] exists for.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThematicBreak {
    /// `width: 100%` — `blockSyntax.css:178`. `ulysses` sets `50%`
    /// (`ulysses.theme.css:248`).
    pub width_fraction: f32,
    /// The rule's left edge as a fraction of the content width, **after** any
    /// `translateX`. muya's `left: 0` with no horizontal translate is 0;
    /// `ulysses`'s `left: 50%` with `translateX(-50%)` on a half-width rule is
    /// 0.25. Normalising here keeps the transform out of the display list.
    pub left_fraction: f32,
    /// `top: 50%` with `translateY(-50%)` — `blockSyntax.css:174, 183`. The
    /// rule's vertical centre as a fraction of the line box.
    pub center_fraction: f32,
    /// `border-top: 2px dashed` — `blockSyntax.css:182`.
    pub thickness_px: f32,
    /// `dashed`.
    pub style: LineStyle,
}

// ---------------------------------------------------------------------------
// Footnote
// ---------------------------------------------------------------------------

/// Footnote definition block — `blockSyntax.css:987-999`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footnote {
    /// `margin: 1.4em 0`.
    pub margin_y_em: f32,
    /// The `1.2em` of `padding: 1.2em 2em 0.05em 1em`, which leaves room for
    /// the absolutely-positioned `[^id]:` label.
    pub padding_top_em: f32,
    /// The `2em`.
    pub padding_end_em: f32,
    /// The `0.05em`.
    pub padding_bottom_em: f32,
    /// The `1em`.
    pub padding_start_em: f32,
    /// `font-size: 0.8em`. Nested `pre` and `code` shrink by 0.8 again
    /// (`blockSyntax.css:1001-1009`).
    pub font_size_em: f32,
}

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

/// The colours muya's **document** stylesheets consume.
///
/// 24 fields covering 30 of the 46 custom properties: 29 pure colours plus the
/// compound [`Colors::float_shadow`]. Every one is required, so a theme that
/// forgets `--hr-color` is a parse error rather than a horizontal rule that
/// silently inherits.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    /// `--theme-color`. The accent: checked checkboxes, selected table cells,
    /// hovered footnote backlinks.
    pub theme: Color,
    /// `--highlight-color`. Search-hit background.
    pub highlight: Color,
    /// `--selection-color`. Text selection background.
    pub selection: Color,
    /// `--editor-color`. Body text.
    pub editor: Color,
    /// `--editor-color-80`. Headings, and the search-hit foreground.
    pub editor_80: Color,
    /// `--editor-color-50`. Code-block text, de-emphasised markers.
    pub editor_50: Color,
    /// `--editor-color-30`. Syntax markers, line numbers, placeholders.
    pub editor_30: Color,
    /// `--editor-color-10`. Hairlines, including the code-block border.
    pub editor_10: Color,
    /// `--editor-color-04`. Faint block tints — footnotes, hovered math.
    pub editor_04: Color,
    /// `--editor-bg-color`. The page.
    pub editor_bg: Color,
    /// `--delete-color`. Error text.
    pub delete: Color,
    /// `--icon-color`. Inline icons.
    pub icon: Color,
    /// `--code-block-bg-color`. Also the inline-code and image-placeholder
    /// background.
    pub code_block_bg: Color,
    /// `--table-border-color`.
    pub table_border: Color,
    /// `--float-bg-color`. Math and diagram preview popovers.
    pub float_bg: Color,
    /// `--float-border-color`.
    pub float_border: Color,
    /// `--float-shadow`, a three-layer `box-shadow`. One of the five compound
    /// values in the consumed set.
    pub float_shadow: Vec<Shadow>,
    /// `--link-color`.
    pub link: Color,
    /// `--h1-color` … `--h6-color`, in order.
    pub heading: [Color; 6],
    /// `--blockquote-text-color`.
    pub blockquote_text: Color,
    /// `--blockquote-border-color`. The left bar.
    pub blockquote_border: Color,
    /// `--strong-color`. muya's own default is the keyword `inherit`.
    pub strong: Color,
    /// `--em-color`. muya's own default is `inherit`.
    pub em: Color,
    /// `--list-marker-color`. muya's own default is `inherit`; the desktop's
    /// light palette says `--editorColor50`, and because the alias is not
    /// bridged the two disagree.
    pub list_marker: Color,
    /// `--hr-color`.
    pub hr: Color,
}

/// The colours muya's **chrome** stylesheets consume — `ui/*/index.css`.
///
/// Ten of the 46: six paint, four compound borders. `mt-layout` never reads
/// them. They are here so that a `Theme` is a complete, round-trippable
/// transcription of a theme file, which is what makes M5's importer able to
/// fail loudly on a theme it cannot express instead of dropping fields.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChromeColors {
    /// `--button-font-color`.
    pub button_font: Color,
    /// `--button-bg-color`.
    pub button_bg: Color,
    /// `--button-bg-color-hover`. **Not a colour** in muya's own default or in
    /// three of the shipped themes: it is a `linear-gradient`, which is why
    /// this field is a [`Fill`].
    pub button_bg_hover: Fill,
    /// `--button-bg-color-active`.
    pub button_bg_active: Color,
    /// `--button-border`, a `1px solid <color>` shorthand.
    pub button_border: Border,
    /// `--button-border-hover`.
    pub button_border_hover: Border,
    /// `--button-border-active`.
    pub button_border_active: Border,
    /// `--button-border-focus`.
    pub button_border_focus: Border,
    /// `--float-hover-color`.
    pub float_hover: Color,
    /// `--input-bg-color`.
    pub input_bg: Color,
}

/// 8-bit sRGB with 8-bit alpha, or the CSS keyword `inherit`.
///
/// Parsed from `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `transparent` or
/// `inherit`. Anything else is a parse error — a theme with a colour typo does
/// not load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// An explicit colour.
    Rgba {
        /// Red.
        r: u8,
        /// Green.
        g: u8,
        /// Blue.
        b: u8,
        /// Alpha; 255 is opaque.
        a: u8,
    },
    /// CSS `inherit` — take the surrounding text colour. Three of muya's own
    /// defaults are this, so it is a value the model has to hold rather than a
    /// gap it can refuse.
    Inherit,
}

impl Color {
    /// An opaque colour from its three channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgba { r, g, b, a: 255 }
    }

    /// A colour with an explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color::Rgba { r, g, b, a }
    }
}

impl std::str::FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Color, String> {
        match s {
            "inherit" => return Ok(Color::Inherit),
            "transparent" => return Ok(Color::rgba(0, 0, 0, 0)),
            _ => {}
        }
        let Some(hex) = s.strip_prefix('#') else {
            return Err(format!(
                "`{s}` is not a colour: expected `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, \
                 `transparent` or `inherit`"
            ));
        };
        let nibble = |c: u8| -> Result<u8, String> {
            (c as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or_else(|| format!("`{s}` is not a colour: `{}` is not a hex digit", c as char))
        };
        let bytes = hex.as_bytes();
        let pair =
            |i: usize| -> Result<u8, String> { Ok(nibble(bytes[i])? << 4 | nibble(bytes[i + 1])?) };
        let short = |i: usize| -> Result<u8, String> {
            let n = nibble(bytes[i])?;
            Ok(n << 4 | n)
        };
        match bytes.len() {
            3 => Ok(Color::rgb(short(0)?, short(1)?, short(2)?)),
            4 => Ok(Color::rgba(short(0)?, short(1)?, short(2)?, short(3)?)),
            6 => Ok(Color::rgb(pair(0)?, pair(2)?, pair(4)?)),
            8 => Ok(Color::rgba(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
            n => Err(format!(
                "`{s}` is not a colour: {n} hex digits, expected 3, 4, 6 or 8"
            )),
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Color::Inherit => f.write_str("inherit"),
            Color::Rgba { r, g, b, a } => write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}"),
        }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Color, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A `box-shadow` layer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    /// Horizontal offset.
    pub x_px: f32,
    /// Vertical offset.
    pub y_px: f32,
    /// Blur radius.
    pub blur_px: f32,
    /// Spread radius.
    pub spread_px: f32,
    /// The shadow's colour.
    pub color: Color,
}

/// A `<width> <style> <color>` border shorthand.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Border {
    /// Border width.
    pub width_px: f32,
    /// Border style.
    pub style: LineStyle,
    /// Border colour.
    pub color: Color,
}

/// CSS `border-style`, restricted to the values the shipped stylesheets use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineStyle {
    /// A continuous line.
    Solid,
    /// The thematic break's `2px dashed`.
    Dashed,
    /// Dotted.
    Dotted,
    /// No line drawn at all, whatever the width says.
    None,
}

/// A background: a colour, or a gradient.
///
/// Exists because `--button-bg-color-hover` is
/// `linear-gradient(#f9f9f9, #f2f2f2)` in muya's own default
/// (`index.css:24`) and in `graphite`, `ulysses` and `one-dark`. A field typed
/// as a colour could not hold it, and a field typed as a string could hold
/// anything.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fill {
    /// A flat colour.
    Solid(Color),
    /// A top-to-bottom gradient over two or more stops.
    LinearGradient(Vec<Color>),
}

// ---------------------------------------------------------------------------
// Code palette
// ---------------------------------------------------------------------------

/// Syntax highlighting, keyed by Prism token class.
///
/// A second typed struct rather than a convention, because MarkText's pairing
/// of theme to Prism stylesheet is a hand-written switch
/// (`renderer/src/util/themeColor.ts:39-69`), `material-dark` silently reuses
/// `dark`'s, and nothing checks that any pairing is right. None of the 31
/// shipped Prism files defines a single custom property — they are 100 %
/// literal `.token.*` selectors — so there is nothing to inherit and everything
/// to transcribe.
///
/// The 32 classes below are exactly the set the paired stylesheets define.
/// Prism itself emits more (`tag`'s inner `attr-name`, language-scoped
/// variants such as `.language-css .token.string`); those fall back to their
/// base class here, which is what an unstyled class does in the browser too.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodePalette {
    /// Code text carrying no token class.
    pub plain: TokenStyle,
    /// `.token.comment`
    pub comment: TokenStyle,
    /// `.token.prolog`
    pub prolog: TokenStyle,
    /// `.token.doctype`
    pub doctype: TokenStyle,
    /// `.token.cdata`
    pub cdata: TokenStyle,
    /// `.token.punctuation`
    pub punctuation: TokenStyle,
    /// `.namespace` — the one class styled by opacity rather than colour, and
    /// the one written without a `.token` prefix.
    pub namespace: TokenStyle,
    /// `.token.property`
    pub property: TokenStyle,
    /// `.token.tag`
    pub tag: TokenStyle,
    /// `.token.boolean`
    pub boolean: TokenStyle,
    /// `.token.number`
    pub number: TokenStyle,
    /// `.token.constant`
    pub constant: TokenStyle,
    /// `.token.symbol`
    pub symbol: TokenStyle,
    /// `.token.selector`
    pub selector: TokenStyle,
    /// `.token.attr-name`
    pub attr_name: TokenStyle,
    /// `.token.string`
    pub string: TokenStyle,
    /// `.token.char`
    pub char: TokenStyle,
    /// `.token.builtin`
    pub builtin: TokenStyle,
    /// `.token.inserted` — one of the two classes with a background.
    pub inserted: TokenStyle,
    /// `.token.deleted`
    pub deleted: TokenStyle,
    /// `.token.operator`
    pub operator: TokenStyle,
    /// `.token.entity`
    pub entity: TokenStyle,
    /// `.token.url`
    pub url: TokenStyle,
    /// `.token.atrule`
    pub atrule: TokenStyle,
    /// `.token.attr-value`
    pub attr_value: TokenStyle,
    /// `.token.function`
    pub function: TokenStyle,
    /// `.token.class-name`
    pub class_name: TokenStyle,
    /// `.token.keyword`
    pub keyword: TokenStyle,
    /// `.token.regex`
    pub regex: TokenStyle,
    /// `.token.important`
    pub important: TokenStyle,
    /// `.token.variable`
    pub variable: TokenStyle,
    /// `.token.bold` — weight only, no colour.
    pub bold: TokenStyle,
    /// `.token.italic` — style only, no colour.
    pub italic: TokenStyle,
}

impl CodePalette {
    /// Every Prism token class this palette styles, spelled as the CSS spells
    /// it.
    ///
    /// `plain` is deliberately absent: it is not a class, it is the absence of
    /// one.
    pub const TOKEN_CLASSES: [&'static str; 32] = [
        "comment",
        "prolog",
        "doctype",
        "cdata",
        "punctuation",
        "namespace",
        "property",
        "tag",
        "boolean",
        "number",
        "constant",
        "symbol",
        "selector",
        "attr-name",
        "string",
        "char",
        "builtin",
        "inserted",
        "deleted",
        "operator",
        "entity",
        "url",
        "atrule",
        "attr-value",
        "function",
        "class-name",
        "keyword",
        "regex",
        "important",
        "variable",
        "bold",
        "italic",
    ];

    /// The style for a Prism token class, or `None` if this palette does not
    /// style it.
    ///
    /// `None` means "paint it as [`CodePalette::plain`]", which is what the
    /// browser does with a class no rule matches.
    pub fn by_class(&self, class: &str) -> Option<&TokenStyle> {
        Some(match class {
            "comment" => &self.comment,
            "prolog" => &self.prolog,
            "doctype" => &self.doctype,
            "cdata" => &self.cdata,
            "punctuation" => &self.punctuation,
            "namespace" => &self.namespace,
            "property" => &self.property,
            "tag" => &self.tag,
            "boolean" => &self.boolean,
            "number" => &self.number,
            "constant" => &self.constant,
            "symbol" => &self.symbol,
            "selector" => &self.selector,
            "attr-name" => &self.attr_name,
            "string" => &self.string,
            "char" => &self.char,
            "builtin" => &self.builtin,
            "inserted" => &self.inserted,
            "deleted" => &self.deleted,
            "operator" => &self.operator,
            "entity" => &self.entity,
            "url" => &self.url,
            "atrule" => &self.atrule,
            "attr-value" => &self.attr_value,
            "function" => &self.function,
            "class-name" => &self.class_name,
            "keyword" => &self.keyword,
            "regex" => &self.regex,
            "important" => &self.important,
            "variable" => &self.variable,
            "bold" => &self.bold,
            "italic" => &self.italic,
            _ => return None,
        })
    }
}

/// How one Prism token class is painted.
///
/// Five required fields rather than four optional ones: a class that sets only
/// `font-weight` (`.token.bold`) still has to say what colour it is, and the
/// answer — [`Color::Inherit`] — is a value, not a gap.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenStyle {
    /// Foreground.
    pub color: Color,
    /// Background; `transparent` for all but `inserted` and `deleted`.
    pub background: Color,
    /// `font-weight: bold`.
    pub bold: bool,
    /// `font-style: italic`.
    pub italic: bool,
    /// `opacity`. 1.0 everywhere except `.namespace`, which is 0.7.
    pub opacity: f32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // §2's geometry table, as constants.
    //
    // M3 §6's S1 gate: "The §2 geometry table is asserted as constants by a
    // test that fails if a number drifts." §7 calls this "the closest thing to
    // a spec M3 has" — a stylesheet is a fixed reference even though it is not
    // runnable, and it is the only external check the geometry work has.
    //
    // Every literal below is transcribed from the CSS cited beside it, not
    // read back out of the TOML.
    // -----------------------------------------------------------------------

    /// `blockSyntax.css:18` — muya's own default, not the 750px every named
    /// theme sets.
    const CONTENT_WIDTH_PX: f32 = 800.0;
    /// `blockSyntax.css:21` — `padding: 0 50px 100px`.
    const CONTAINER_PADDING: (f32, f32, f32) = (0.0, 50.0, 100.0);
    /// `blockSyntax.css:2, 4`.
    const ROOT_FONT_SIZE_PX: f32 = 16.0;
    const ROOT_LINE_HEIGHT: f32 = 1.6;
    /// `blockSyntax.css:57, 62, 67, 72, 77, 82`.
    const HEADING_SCALE_EM: [f32; 6] = [1.875, 1.5, 1.375, 1.25, 1.125, 1.0];
    /// `blockSyntax.css:45, 50, 49`.
    const HEADING_MARGIN_REM: f32 = 1.0;
    const HEADING_LINE_HEIGHT: f32 = 1.4;
    /// `blockSyntax.css:33` — `p`, `blockquote`, `ul`, `ol`, `dl`, `table`.
    const BLOCK_MARGIN_EM: f32 = 0.5;
    /// `blockSyntax.css:142, 155, 152`.
    const BLOCKQUOTE_PADDING_X_PX: f32 = 30.0;
    const BLOCKQUOTE_BAR_WIDTH_PX: f32 = 2.0;
    const BLOCKQUOTE_BAR_INSET_PX: f32 = 15.0;
    /// `blockSyntax.css:204, 205, 219, 210, 213, 214, 325`.
    const CODE_MARGIN_TOP_EM: f32 = 1.5;
    const CODE_PADDING_EM: f32 = 1.0;
    const CODE_FONT_SIZE_PCT: f32 = 90.0;
    const CODE_LINE_HEIGHT: f32 = 1.6;
    const CODE_BORDER_PX: f32 = 1.0;
    const CODE_RADIUS_PX: f32 = 3.0;
    const CODE_GUTTER_EM: f32 = 2.5;
    /// `inlineSyntax.css:62, 65, 69`.
    const INLINE_CODE_PADDING_EM: (f32, f32) = (0.2, 0.4);
    const INLINE_CODE_FONT_SIZE_EM: f32 = 0.8;
    const INLINE_CODE_RADIUS_PX: f32 = 3.0;
    /// `blockSyntax.css:410`, and the disc → circle → square cascade at
    /// `:417-456`.
    const LIST_INDENT_PX: f32 = 30.0;
    /// `blockSyntax.css:504-505, 533-534, 517`.
    const CHECKBOX_BOX_PX: f32 = 12.0;
    const CHECKBOX_RING_PX: f32 = 18.0;
    const CHECKBOX_INSET_PX: f32 = -23.0;
    /// `blockSyntax.css:602, 630, 590`.
    const TABLE_CELL_PADDING_PX: (f32, f32) = (6.0, 13.0);
    const TABLE_CELL_BORDER_PX: f32 = 1.0;
    /// `blockSyntax.css:182, 174`.
    const HR_THICKNESS_PX: f32 = 2.0;
    const HR_CENTER_FRACTION: f32 = 0.5;
    /// `blockSyntax.css:3`, identically `index.css:207`.
    const BODY_FONT: [&str; 6] = [
        "Open Sans",
        "Clear Sans",
        "Helvetica Neue",
        "Helvetica",
        "Arial",
        "sans-serif",
    ];
    /// `blockSyntax.css:220-227`.
    const CODE_FONT: [&str; 5] = [
        "DejaVu Sans Mono",
        "Source Code Pro",
        "Droid Sans Mono",
        "Consolas",
        "monospace",
    ];

    /// Every geometry row of M3 §2's table, against the theme that is supposed
    /// to be muya's own stylesheet.
    ///
    /// If this fails, either a transcription drifted or the reference
    /// stylesheet changed. Both are worth stopping for; neither is worth
    /// discovering from a golden diff three stages later.
    #[test]
    fn the_section_2_geometry_table_holds_as_constants() {
        let t = Theme::muya_default();

        // Content column and container padding.
        assert_eq!(t.metrics.content_width_px, CONTENT_WIDTH_PX);
        assert_eq!(t.metrics.container_padding_top_px, CONTAINER_PADDING.0);
        assert_eq!(t.metrics.container_padding_x_px, CONTAINER_PADDING.1);
        assert_eq!(t.metrics.container_padding_bottom_px, CONTAINER_PADDING.2);

        // Root font size and line height.
        assert_eq!(t.metrics.font_size_px, ROOT_FONT_SIZE_PX);
        assert_eq!(t.metrics.line_height, ROOT_LINE_HEIGHT);
        assert_eq!(t.metrics.root_font_size_px, ROOT_FONT_SIZE_PX);

        // Heading scale. muya's h1 is 1.875em, not the UA's 2em, and h3 is
        // 1.375em, not 1.17em — the table's own headline point.
        assert_eq!(t.headings.scale_em, HEADING_SCALE_EM);
        assert_eq!(t.headings.margin_rem, HEADING_MARGIN_REM);
        assert_eq!(t.headings.line_height, HEADING_LINE_HEIGHT);
        assert!(t.headings.bold);

        // Block spacing.
        assert_eq!(t.metrics.block_margin_em, BLOCK_MARGIN_EM);
        assert_eq!(t.blockquote.margin_em, BLOCK_MARGIN_EM);
        assert_eq!(t.lists.margin_em, BLOCK_MARGIN_EM);

        // Block quote.
        assert_eq!(t.blockquote.padding_x_px, BLOCKQUOTE_PADDING_X_PX);
        assert_eq!(t.blockquote.bar_width_px, BLOCKQUOTE_BAR_WIDTH_PX);
        assert_eq!(t.blockquote.bar_inset_px, BLOCKQUOTE_BAR_INSET_PX);

        // Code block.
        assert_eq!(t.code_block.margin_top_em, CODE_MARGIN_TOP_EM);
        assert_eq!(t.code_block.padding_em, CODE_PADDING_EM);
        assert_eq!(t.code_block.font_size_pct, CODE_FONT_SIZE_PCT);
        assert_eq!(t.code_block.line_height, CODE_LINE_HEIGHT);
        assert_eq!(t.code_block.border_width_px, CODE_BORDER_PX);
        assert_eq!(t.code_block.corner_radius_px, CODE_RADIUS_PX);
        assert_eq!(t.code_block.gutter_width_em, CODE_GUTTER_EM);

        // Inline code.
        assert_eq!(t.inline_code.padding_y_em, INLINE_CODE_PADDING_EM.0);
        assert_eq!(t.inline_code.padding_x_em, INLINE_CODE_PADDING_EM.1);
        assert_eq!(t.inline_code.font_size_em, INLINE_CODE_FONT_SIZE_EM);
        assert_eq!(t.inline_code.corner_radius_px, INLINE_CODE_RADIUS_PX);

        // Lists: 30px indent, disc → circle → square.
        assert_eq!(t.lists.indent_px, LIST_INDENT_PX);
        assert_eq!(t.lists.task_indent_px, LIST_INDENT_PX);
        assert_eq!(
            t.lists.bullet_markers,
            vec![ListMarker::Disc, ListMarker::Circle, ListMarker::Square]
        );

        // Task checkbox: a 12×12 target inside an 18×18 ring at -23px.
        assert_eq!(t.checkbox.box_size_px, CHECKBOX_BOX_PX);
        assert_eq!(t.checkbox.ring_size_px, CHECKBOX_RING_PX);
        assert_eq!(t.checkbox.inset_inline_start_px, CHECKBOX_INSET_PX);
        assert_eq!(t.checkbox.shape, CheckboxShape::Circle);

        // Table cell.
        assert_eq!(t.table.cell_padding_y_px, TABLE_CELL_PADDING_PX.0);
        assert_eq!(t.table.cell_padding_x_px, TABLE_CELL_PADDING_PX.1);
        assert_eq!(t.table.cell_border_px, TABLE_CELL_BORDER_PX);
        assert!(t.table.collapse_borders);

        // Thematic break.
        assert_eq!(t.thematic_break.thickness_px, HR_THICKNESS_PX);
        assert_eq!(t.thematic_break.style, LineStyle::Dashed);
        assert_eq!(t.thematic_break.center_fraction, HR_CENTER_FRACTION);
        assert_eq!(t.thematic_break.width_fraction, 1.0);

        // Font stacks.
        assert_eq!(t.fonts.body, BODY_FONT);
        assert_eq!(t.fonts.code, CODE_FONT);
        assert_eq!(
            t.fonts.inline_code, CODE_FONT,
            "inline code's stack is a literal in inlineSyntax.css:66, not the \
             --mu-code-font-family variable, but its shipped value is the same"
        );
    }

    /// Both shipped themes are valid, which is what makes
    /// [`Theme::muya_default`] and [`Theme::dark`] non-panicking.
    #[test]
    fn both_shipped_themes_parse() {
        let default = Theme::from_toml_str(MUYA_DEFAULT_TOML).expect("muya-default parses");
        let dark = Theme::from_toml_str(DARK_TOML).expect("dark parses");
        assert_eq!(default.name, "muya-default");
        assert_eq!(dark.name, "dark");
    }

    /// **D4's central argument, asserted rather than claimed.**
    ///
    /// The failure mode a typed schema exists to remove is the `light` theme's:
    /// 39 of 41 variables never bridged, nothing reporting it. Here, deleting
    /// *any* single field from a shipped theme is a load error. The test
    /// deletes them one at a time and requires every deletion to fail.
    ///
    /// Continuation lines inside multi-line arrays are indented and therefore
    /// skipped: removing one shadow layer leaves a shorter but valid list,
    /// which is not the property under test.
    #[test]
    fn removing_any_single_field_makes_a_theme_fail_to_load() {
        let lines: Vec<&str> = MUYA_DEFAULT_TOML.lines().collect();
        let mut removed = 0;

        for (i, line) in lines.iter().enumerate() {
            let is_top_level_assignment = !line.starts_with(char::is_whitespace)
                && !line.starts_with('#')
                && line.contains(" = ");
            if !is_top_level_assignment {
                continue;
            }
            removed += 1;

            let mutilated: String = lines
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, l)| *l)
                .collect::<Vec<_>>()
                .join("\n");

            assert!(
                Theme::from_toml_str(&mutilated).is_err(),
                "removing line {} — `{}` — still parsed. Every field is required; \
                 a field that can go missing is the silent-fallthrough failure \
                 mode D4 exists to remove.",
                i + 1,
                line.trim()
            );
        }

        assert!(
            removed >= 100,
            "only {removed} fields were exercised; the scan stopped matching the \
             file's shape and the test has gone vacuous"
        );
    }

    /// `deny_unknown_fields`, at the top level and nested.
    ///
    /// A misspelled key must be an error, not a default silently applied — the
    /// same reasoning as the required fields, from the other direction.
    #[test]
    fn an_unknown_key_makes_a_theme_fail_to_load() {
        let top_level = format!("bogus_key = 1.0\n{MUYA_DEFAULT_TOML}");
        assert!(Theme::from_toml_str(&top_level).is_err());

        let nested = MUYA_DEFAULT_TOML.replace("[metrics]", "[metrics]\ncontent_wdith_px = 800.0");
        assert!(
            Theme::from_toml_str(&nested).is_err(),
            "a typo'd key inside a sub-table must be rejected too"
        );
    }

    /// The 800/750 split, which is the reason S1 ships two themes.
    ///
    /// The default case is the **800px** one: the kebab-case
    /// `--editor-area-width` alias that carries the desktop's 750px exists only
    /// inside the 32 named theme files, so an install that never opens
    /// preferences gets muya's own width.
    #[test]
    fn the_two_themes_differ_on_content_width() {
        let default = Theme::muya_default();
        let dark = Theme::dark();

        assert_eq!(default.metrics.content_width_px, 800.0);
        assert_eq!(dark.metrics.content_width_px, 750.0);

        // The number layout actually uses: the column minus 50px of padding on
        // each side.
        assert_eq!(default.content_width_px(), 700.0);
        assert_eq!(dark.content_width_px(), 650.0);
    }

    /// The **second** geometry axis on which `dark` differs, found by reading
    /// the stylesheet rather than the plan.
    ///
    /// `dark.theme.css:200-206` sets `border: none !important` on
    /// `pre.mu-code-block`, and 29 of the other 31 themes carry the identical
    /// rule. A code block in `dark` is therefore 2px narrower in content and
    /// 2px shorter than the same block in `muya-default`, which a golden
    /// comparison will show and which nothing in §2's table predicts.
    #[test]
    fn dark_removes_the_code_block_border_and_muya_default_keeps_it() {
        assert_eq!(Theme::muya_default().code_block.border_width_px, 1.0);
        assert_eq!(Theme::dark().code_block.border_width_px, 0.0);
    }

    /// Everything else about the two themes' geometry is identical, so a
    /// golden diff between them isolates the two axes above.
    #[test]
    fn the_two_themes_agree_on_every_other_geometry_field() {
        let mut default = Theme::muya_default();
        let dark = Theme::dark();

        default.name = dark.name.clone();
        default.metrics.content_width_px = dark.metrics.content_width_px;
        default.code_block.border_width_px = dark.code_block.border_width_px;
        default.colors = dark.colors.clone();
        default.chrome = dark.chrome.clone();
        default.code_palette = dark.code_palette.clone();

        assert_eq!(
            default, dark,
            "dark is colour-only apart from the content width and the code-block \
             border; a third difference means one of the two files drifted"
        );
    }

    /// The palette is keyed by Prism token class, and every class it claims to
    /// know resolves in both themes.
    #[test]
    fn every_prism_token_class_resolves_in_both_themes() {
        assert_eq!(CodePalette::TOKEN_CLASSES.len(), 32);
        for theme in [Theme::muya_default(), Theme::dark()] {
            for class in CodePalette::TOKEN_CLASSES {
                assert!(
                    theme.code_palette.by_class(class).is_some(),
                    "{}: no style for `.token.{class}`",
                    theme.name
                );
            }
            assert!(theme.code_palette.by_class("not-a-prism-class").is_none());
        }
    }

    /// The two palettes group the classes differently, which is what makes
    /// them a real second sample rather than a recoloured copy.
    #[test]
    fn the_two_palettes_group_tokens_differently() {
        let light = Theme::muya_default().code_palette;
        let dark = Theme::dark().code_palette;

        // muya's paired sheet gives boolean and number the same colour as
        // property; dark's splits them.
        assert_eq!(light.boolean.color, light.property.color);
        assert_ne!(dark.boolean.color, dark.property.color);

        // muya's pairs keyword with atrule; dark's pairs function with atrule
        // and gives keyword its own colour.
        assert_eq!(light.keyword.color, light.atrule.color);
        assert_ne!(dark.keyword.color, dark.atrule.color);
        assert_eq!(dark.function.color, dark.atrule.color);

        // `.namespace` is the one class styled by opacity, in both.
        assert_eq!(light.namespace.opacity, 0.7);
        assert_eq!(dark.namespace.opacity, 0.7);
        assert_eq!(light.comment.opacity, 1.0);
    }

    /// The three classes that carry no colour of their own are `inherit`, not
    /// an invented literal.
    #[test]
    fn weight_and_style_only_classes_inherit_their_colour() {
        for theme in [Theme::muya_default(), Theme::dark()] {
            let p = &theme.code_palette;
            assert_eq!(p.bold.color, Color::Inherit);
            assert!(p.bold.bold);
            assert_eq!(p.italic.color, Color::Inherit);
            assert!(p.italic.italic);
            assert_eq!(p.namespace.color, Color::Inherit);
            assert!(p.important.bold, "`.token.important` is coloured and bold");
        }
    }

    // -----------------------------------------------------------------------
    // Colour parsing
    // -----------------------------------------------------------------------

    #[test]
    fn colours_parse_in_every_css_form_the_themes_use() {
        let parse = |s: &str| s.parse::<Color>();
        assert_eq!(parse("#333").unwrap(), Color::rgb(0x33, 0x33, 0x33));
        assert_eq!(parse("#4d4d4d").unwrap(), Color::rgb(0x4d, 0x4d, 0x4d));
        assert_eq!(
            parse("#ffffffb3").unwrap(),
            Color::rgba(0xff, 0xff, 0xff, 0xb3)
        );
        assert_eq!(parse("#f003").unwrap(), Color::rgba(0xff, 0, 0, 0x33));
        assert_eq!(parse("transparent").unwrap(), Color::rgba(0, 0, 0, 0));
        assert_eq!(parse("inherit").unwrap(), Color::Inherit);
    }

    #[test]
    fn a_malformed_colour_is_an_error_not_a_default() {
        for bad in [
            "",
            "#",
            "#12",
            "#12345",
            "rgb(1,2,3)",
            "slategray",
            "#zzzzzz",
        ] {
            assert!(
                bad.parse::<Color>().is_err(),
                "`{bad}` must not parse as a colour"
            );
        }
    }

    #[test]
    fn a_colour_round_trips_through_its_display_form() {
        for theme in [Theme::muya_default(), Theme::dark()] {
            for c in [
                theme.colors.editor,
                theme.colors.editor_bg,
                theme.colors.theme,
                theme.colors.strong,
                theme.colors.heading[0],
            ] {
                assert_eq!(c.to_string().parse::<Color>().unwrap(), c);
            }
        }
    }

    #[test]
    fn a_theme_with_a_bad_colour_fails_to_load_and_says_where() {
        let broken = MUYA_DEFAULT_TOML.replace("editor = \"#4d4d4dff\"", "editor = \"greenish\"");
        let err = Theme::from_toml_str(&broken).expect_err("`greenish` is not a colour");
        assert!(
            err.message().contains("greenish"),
            "the error must name the offending value, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // The outlier fields, which neither shipped theme uses.
    //
    // D4's rule is that the schema is sized to what themes override, not to
    // what the variable system exposes. These assert that the shape the three
    // outliers need is expressible — the M5 import path's precondition.
    // -----------------------------------------------------------------------

    /// `ulysses`'s 16×16 square checkbox at −24px with a 9×5 tick, its
    /// half-width thematic break, its 5×2px bullet overlay and its centred
    /// headings all round-trip through the schema, on a theme built by
    /// patching `muya-default`'s TOML the way `ulysses.theme.css` patches
    /// muya's stylesheet.
    #[test]
    fn the_three_geometry_outliers_are_expressible() {
        let patched = MUYA_DEFAULT_TOML
            .replace("align = \"start\"", "align = \"center\"")
            .replace("shape = \"circle\"", "shape = \"square\"")
            .replace("box_size_px = 12.0", "box_size_px = 16.0")
            .replace("ring_size_px = 18.0", "ring_size_px = 16.0")
            .replace("ring_offset_px = -2.0", "ring_offset_px = 0.0")
            .replace("ring_corner_radius_px = 9.0", "ring_corner_radius_px = 2.0")
            .replace(
                "inset_inline_start_px = -23.0",
                "inset_inline_start_px = -24.0",
            )
            .replace("check_width_px = 8.0", "check_width_px = 9.0")
            .replace("check_height_px = 4.0", "check_height_px = 5.0")
            .replace("check_left_px = 4.0", "check_left_px = 5.0")
            .replace(
                "unchecked_rotation_deg = 0.0",
                "unchecked_rotation_deg = -90.0",
            )
            .replace(
                "[checkbox.top.line-centered]\noffset_px = 7.0",
                "[checkbox.top.em]\nem = 0.1",
            )
            .replace(
                "bullet_markers = [\"disc\", \"circle\", \"square\"]",
                "bullet_markers = [{ dash = { width_px = 5.0, height_px = 2.0, \
                 left_px = -18.0, top_px = 15.0 } }]",
            )
            .replace("width_fraction = 1.0", "width_fraction = 0.5")
            .replace("left_fraction = 0.0", "left_fraction = 0.25");

        let t = Theme::from_toml_str(&patched).expect("the ulysses shape must be expressible");

        assert_eq!(t.headings.align, TextAlign::Center);
        assert_eq!(t.checkbox.shape, CheckboxShape::Square);
        assert_eq!(t.checkbox.box_size_px, 16.0);
        assert_eq!(t.checkbox.ring_size_px, 16.0);
        assert_eq!(t.checkbox.ring_corner_radius_px, 2.0);
        assert_eq!(t.checkbox.inset_inline_start_px, -24.0);
        assert_eq!(t.checkbox.check_width_px, 9.0);
        assert_eq!(t.checkbox.check_height_px, 5.0);
        assert_eq!(t.checkbox.unchecked_rotation_deg, -90.0);
        assert_eq!(t.checkbox.top, CheckboxTop::Em(EmTop { em: 0.1 }));
        assert_eq!(
            t.lists.bullet_markers,
            vec![ListMarker::Dash(DashMarker {
                width_px: 5.0,
                height_px: 2.0,
                left_px: -18.0,
                top_px: 15.0,
            })]
        );
        assert_eq!(t.thematic_break.width_fraction, 0.5);
        assert_eq!(t.thematic_break.left_fraction, 0.25);
    }

    /// muya's own checkbox position is a formula over the editor's font size
    /// and line height, not a constant, so the model has to be an enum.
    #[test]
    fn muya_centres_its_checkbox_on_the_first_line_rather_than_offsetting_it() {
        let t = Theme::muya_default();
        let CheckboxTop::LineCentered(top) = &t.checkbox.top else {
            panic!("muya's checkbox top is calc(line-height * 0.5 * font-size - 7px)");
        };
        assert_eq!(top.offset_px, 7.0);
        // The resolved position at the shipped 16px / 1.6.
        let resolved = t.metrics.line_height * 0.5 * t.metrics.font_size_px - top.offset_px;
        assert_eq!(resolved, 5.8);
    }

    /// `--button-bg-color-hover` is a gradient in muya's own default, which is
    /// why it is a [`Fill`] rather than a [`Color`]. It is the one value in the
    /// consumed set that is neither a colour nor a border/shadow shorthand.
    #[test]
    fn the_hover_background_is_a_gradient_in_muya_and_a_colour_in_dark() {
        match Theme::muya_default().chrome.button_bg_hover {
            Fill::LinearGradient(stops) => assert_eq!(stops.len(), 2),
            Fill::Solid(c) => panic!("muya's is linear-gradient(#f9f9f9, #f2f2f2), got {c}"),
        }
        assert!(matches!(
            Theme::dark().chrome.button_bg_hover,
            Fill::Solid(_)
        ));
    }

    /// The compound values are structured, not strings: three shadow layers,
    /// four border shorthands.
    #[test]
    fn the_compound_values_are_parsed_into_their_parts() {
        for theme in [Theme::muya_default(), Theme::dark()] {
            assert_eq!(
                theme.colors.float_shadow.len(),
                3,
                "{}: --float-shadow is a three-layer box-shadow",
                theme.name
            );
            assert_eq!(theme.colors.float_shadow[0].spread_px, 1.0);
            for border in [
                &theme.chrome.button_border,
                &theme.chrome.button_border_hover,
                &theme.chrome.button_border_active,
                &theme.chrome.button_border_focus,
            ] {
                assert_eq!(border.width_px, 1.0);
                assert_eq!(border.style, LineStyle::Solid);
            }
        }
    }

    /// muya's own defaults for `--strong-color`, `--em-color` and
    /// `--list-marker-color` are the keyword `inherit`; `dark` replaces all
    /// three with literals. A model that could only hold colours would have to
    /// invent a value for the first case.
    #[test]
    fn inherit_is_a_value_the_default_theme_actually_uses() {
        let d = Theme::muya_default().colors;
        assert_eq!(d.strong, Color::Inherit);
        assert_eq!(d.em, Color::Inherit);
        assert_eq!(d.list_marker, Color::Inherit);

        let k = Theme::dark().colors;
        assert_ne!(k.strong, Color::Inherit);
        assert_ne!(k.em, Color::Inherit);
        assert_ne!(k.list_marker, Color::Inherit);
    }
}
