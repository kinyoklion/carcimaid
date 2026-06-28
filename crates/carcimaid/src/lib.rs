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

pub use error::{Error, Result};

/// Render mermaid source text to an SVG string.
///
/// This is the top-level convenience entry point that runs the full pipeline.
/// It is intentionally thin so callers that need intermediate stages (e.g. the
/// compliance harness) can drive [`parser`], [`layout`], and [`render`] directly.
pub fn render_to_svg(source: &str) -> Result<String> {
    let diagram = parser::parse(source)?;
    let laid_out = layout::layout(&diagram)?;
    Ok(render::to_svg(&laid_out))
}
