//! carcimaid — a pure-Rust renderer that turns [mermaid] diagrams into SVG.
//!
//! The pipeline is staged so each diagram type plugs into shared infrastructure:
//!
//! ```text
//!   text ──▶ parse ──▶ IR (diagram model) ──▶ layout ──▶ render ──▶ SVG
//! ```
//!
//! - [`parser`] turns mermaid source text into a typed diagram model.
//! - [`ir`] is that typed model: layout-independent description of a diagram.
//! - [`layout`] assigns geometry (positions, sizes, edge routes).
//! - [`render`] emits SVG from a laid-out diagram.
//!
//! [mermaid]: https://github.com/mermaid-js/mermaid
//!
//! ## Attribution
//!
//! carcimaid is developed for behavioural compliance with mermaid. See
//! `ATTRIBUTION.md` at the repository root for the grammar, layout algorithm,
//! and example corpus it derives from, all used under their original licenses.

pub mod error;
pub mod ir;
pub mod layout;
pub mod parser;
pub mod render;
pub mod style;
pub mod text;

pub use error::{Error, Result};
pub use ir::Theme;

/// The background painted behind a rendered diagram.
///
/// This controls only the SVG root element's `background-color`; it has no
/// effect on the diagram's own fills, strokes, or geometry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Background {
    /// mermaid's default — an opaque white background.
    #[default]
    Default,
    /// No background fill; the SVG root is left transparent (`rgba(0,0,0,0)`),
    /// so whatever sits behind the SVG shows through.
    Transparent,
    /// A caller-supplied CSS colour (e.g. `"black"`, `"#1e1e1e"`, `"rgb(...)"`).
    Color(String),
}

/// Caller-supplied overrides for a render.
///
/// Defaults reproduce [`render_to_svg`] exactly (opaque white background, theme
/// taken from the diagram's own frontmatter). Set only the fields you want to
/// change and leave the rest with `..Default::default()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// The SVG root background. See [`Background`].
    pub background: Background,
    /// Colour theme to use *in place of the built-in default*. `None` keeps the
    /// current behaviour: the diagram renders in whatever theme its own
    /// frontmatter (`config.theme`) selects, falling back to
    /// [`Theme::Default`] (light).
    ///
    /// When `Some`, the given theme becomes the default for diagrams that do
    /// not pick one themselves; a diagram's explicit frontmatter `config.theme`
    /// still wins (matching mermaid's precedence of frontmatter over the global
    /// `initialize()` theme).
    ///
    /// Honoured for flowcharts and sequence diagrams.
    pub theme: Option<Theme>,
}

/// Render mermaid source text to an SVG string.
///
/// This is the top-level convenience entry point that runs the full pipeline.
/// It is intentionally thin so callers that need intermediate stages (e.g. the
/// compliance harness) can drive [`parser`], [`layout`], and [`render`] directly.
///
/// Equivalent to [`render_to_svg_with`] with [`RenderOptions::default`], and its
/// output is byte-for-byte the historical default.
pub fn render_to_svg(source: &str) -> Result<String> {
    render_to_svg_with(source, &RenderOptions::default())
}

/// Render mermaid source text to an SVG string with explicit [`RenderOptions`].
///
/// Only the overrides in `options` differ from [`render_to_svg`].
pub fn render_to_svg_with(source: &str, options: &RenderOptions) -> Result<String> {
    let mut diagram = parser::parse(source)?;
    // A consumer-supplied theme is a *default*: it applies only when the source
    // didn't select a theme itself, so an explicit frontmatter `config.theme`
    // still wins (mermaid resolves frontmatter over the global initialize theme).
    if let Some(theme) = options.theme {
        if parser::frontmatter_theme(source).is_none() {
            diagram.set_default_theme(theme);
        }
    }
    let laid_out = layout::layout(&diagram)?;
    Ok(render::to_svg(&laid_out, &options.background))
}
