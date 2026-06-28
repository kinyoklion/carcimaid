//! Text measurement.
//!
//! mermaid measures label text in a browser using the font stack
//! `"trebuchet ms", verdana, arial, sans-serif` at 16px. In the headless
//! Chromium used by the mermaid CLI, none of those proprietary families are
//! installed, so the whole stack resolves to **DejaVu Sans**. We therefore
//! measure with the very same font (vendored under `resources/`) via
//! [`ttf_parser`], summing glyph advance widths, so our node/label sizes match
//! mermaid's. See `ATTRIBUTION.md` for the font license.

use std::sync::OnceLock;
use ttf_parser::Face;

/// The font mermaid's headless Chromium resolves the default stack to.
const FONT: &[u8] = include_bytes!("../resources/DejaVuSans.ttf");

fn face() -> &'static Face<'static> {
    static FACE: OnceLock<Face<'static>> = OnceLock::new();
    FACE.get_or_init(|| Face::parse(FONT, 0).expect("vendored DejaVuSans.ttf is valid"))
}

/// Advance width of `text` rendered at `font_size` px, in px.
///
/// This sums per-glyph horizontal advances (no kerning); DejaVu Sans applies
/// kerning via GPOS which Chromium honours, but for the short Latin labels in
/// flowcharts the difference is sub-pixel and within the comparison tolerance.
pub fn measure_width(text: &str, font_size: f64) -> f64 {
    let face = face();
    let scale = font_size / face.units_per_em() as f64;
    let space = face
        .glyph_index(' ')
        .and_then(|g| face.glyph_hor_advance(g))
        .unwrap_or(0);
    let mut total: f64 = 0.0;
    for ch in text.chars() {
        let advance = match face.glyph_index(ch) {
            Some(g) => face.glyph_hor_advance(g).unwrap_or(space),
            None => space, // unknown glyph: approximate with a space's width
        };
        total += advance as f64;
    }
    total * scale
}

/// Line height in px at `font_size`, from the font's ascent/descent/line-gap.
pub fn line_height(font_size: f64) -> f64 {
    let face = face();
    let scale = font_size / face.units_per_em() as f64;
    let h = face.ascender() as f64 - face.descender() as f64 + face.line_gap() as f64;
    h * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_are_positive_and_ordered() {
        let a = measure_width("i", 16.0);
        let b = measure_width("W", 16.0);
        assert!(a > 0.0 && b > a, "expected W wider than i: {a} vs {b}");
        // Empty string measures to zero.
        assert_eq!(measure_width("", 16.0), 0.0);
    }
}
