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
    let upem = face.units_per_em() as f64;
    let scale = font_size / upem;
    let mut total: f64 = 0.0;
    for ch in text.chars() {
        let advance = match face.glyph_index(ch).and_then(|g| face.glyph_hor_advance(g)) {
            Some(a) => a as f64,
            // Glyphs DejaVu lacks (notably CJK) render full-width (1em) in the
            // fonts the mermaid CLI falls back to (Noto Sans CJK), so measure
            // them as one em rather than a space.
            None => upem,
        };
        total += advance;
    }
    total * scale
}

/// mermaid's default flowchart label wrapping width (config `wrappingWidth`).
pub const WRAP_WIDTH: f64 = 200.0;

/// Wrap `label` into lines of words. Explicit newlines are honoured as forced
/// line breaks; within each segment, words are greedily wrapped so each line's
/// measured width stays within `max_width` (matching mermaid). A single word
/// wider than `max_width` occupies its own line. Returns lines, each a list of
/// words (without inter-word spaces); blank segments are dropped.
pub fn wrap_label(label: &str, max_width: f64, font_size: f64) -> Vec<Vec<String>> {
    let mut lines: Vec<Vec<String>> = Vec::new();
    let label = replace_br(label);
    for segment in label.split('\n') {
        let mut cur: Vec<String> = Vec::new();
        for word in segment.split_whitespace() {
            let candidate = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", cur.join(" "), word)
            };
            if measure_width(&candidate, font_size) <= max_width {
                cur.push(word.to_string());
                continue;
            }
            // The word doesn't fit on the current line: flush it first.
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            // A word longer than the whole line is broken at character level
            // (matching mermaid); the last chunk stays open for following words.
            if measure_width(word, font_size) > max_width {
                let mut chunks = break_word(word, max_width, font_size);
                let last = chunks.pop().unwrap_or_default();
                for chunk in chunks {
                    lines.push(vec![chunk]);
                }
                cur.push(last);
            } else {
                cur.push(word.to_string());
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
    }
    lines
}

/// Break a single over-long word into consecutive character chunks that each fit
/// within `max_width` (mermaid's break-word behaviour for unbreakable tokens).
fn break_word(word: &str, max_width: f64, font_size: f64) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for ch in word.chars() {
        if !cur.is_empty() && measure_width(&format!("{cur}{ch}"), font_size) > max_width {
            chunks.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Replace HTML line breaks (`<br>`, `<br/>`, `<br />`, any case) with `\n` so
/// they act as forced line breaks, matching mermaid's label handling.
fn replace_br(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if lower[i..].starts_with("<br") {
            // Must be a real <br…> tag: after "br" comes `>`, `/`, or whitespace.
            let after = &lower[i + 3..];
            let is_tag = after.starts_with('>') || after.starts_with('/') || after.starts_with(char::is_whitespace);
            if is_tag {
                if let Some(gt) = s[i..].find('>') {
                    out.push('\n');
                    i += gt + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Measured width of a wrapped line (its words joined by single spaces).
pub fn line_width(words: &[String], font_size: f64) -> f64 {
    measure_width(&words.join(" "), font_size)
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
    fn wraps_to_width_and_splits_words() {
        // A short label stays on one line, one word per token.
        let one = wrap_label("Is it ready?", WRAP_WIDTH, 16.0);
        assert_eq!(one, vec![vec!["Is", "it", "ready?"]]);
        // A long label wraps into multiple lines, each within the width.
        let many = wrap_label("This is a fairly long label that should wrap", WRAP_WIDTH, 16.0);
        assert!(many.len() >= 2, "expected wrapping, got {many:?}");
        for line in &many {
            assert!(line_width(line, 16.0) <= WRAP_WIDTH || line.len() == 1);
        }
    }

    #[test]
    fn widths_are_positive_and_ordered() {
        let a = measure_width("i", 16.0);
        let b = measure_width("W", 16.0);
        assert!(a > 0.0 && b > a, "expected W wider than i: {a} vs {b}");
        // Empty string measures to zero.
        assert_eq!(measure_width("", 16.0), 0.0);
    }
}
