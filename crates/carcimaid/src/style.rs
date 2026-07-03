//! Resolving mermaid `style` / `classDef` / `class` declarations into the inline
//! `style` attributes mermaid emits on shapes and labels.
//!
//! Mirrors mermaid's `styles2String`: class-compiled styles then direct styles
//! are merged into an ordered map (last value wins, first-seen key order), then
//! each declaration is split — `color`/`text-*`/`font-*` (see [`is_label_style`])
//! go to the label, everything else to the shape — and emitted as `k:v !important`.

use std::collections::HashMap;

/// Whether a CSS property applies to the label text rather than the shape
/// (mermaid's `isLabelStyle`).
fn is_label_style(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

/// The resolved inline styles for one element.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolved {
    /// `style` attribute for the shape (fill/stroke/…), or empty.
    pub shape: String,
    /// `style` attribute for the label text (color/font/…), or empty.
    pub label: String,
}

/// Resolve an element's styles from its classes and direct declarations.
/// `include_default` prepends the `default` classDef (mermaid applies it to all
/// nodes).
pub fn resolve(
    class_defs: &HashMap<String, Vec<String>>,
    classes: &[String],
    direct: &[String],
    include_default: bool,
) -> Resolved {
    // Ordered declarations: implicit base classes, then explicit classes, then
    // direct styles. mermaid applies `classDef default` and `classDef node` to
    // every node (both match a node's base element classes).
    let mut decls: Vec<&str> = Vec::new();
    if include_default {
        for base in ["default", "node"] {
            if let Some(d) = class_defs.get(base) {
                decls.extend(d.iter().map(String::as_str));
            }
        }
    }
    for c in classes {
        if let Some(d) = class_defs.get(c) {
            decls.extend(d.iter().map(String::as_str));
        }
    }
    decls.extend(direct.iter().map(String::as_str));

    // Ordered map: first-seen key order, last value wins.
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, String> = HashMap::new();
    for decl in decls {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let (k, v) = (k.trim().to_string(), v.trim().to_string());
        if !map.contains_key(&k) {
            order.push(k.clone());
        }
        map.insert(k, v);
    }

    let (mut shape, mut label): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for k in order {
        let decl = format!("{k}:{} !important", map[&k]);
        if is_label_style(&k) {
            label.push(decl);
        } else {
            shape.push(decl);
        }
    }
    Resolved { shape: shape.join(";"), label: label.join(";") }
}
