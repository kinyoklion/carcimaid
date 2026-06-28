//! Structural comparison of two SVG documents.
//!
//! The user-selected compliance metric is a *structural* diff: compare the
//! element trees node-by-node — tag names, attributes, and text content
//! strictly, but numeric geometry (coordinates, sizes, transforms, path data)
//! only within a configurable tolerance. This lets us measure convergence on
//! mermaid's output without requiring byte-identical float coordinates from a
//! not-yet-identical layout engine.
//!
//! Two views are produced:
//! - A coarse **tag histogram** similarity, robust even when the trees diverge
//!   heavily (the expected early state).
//! - A recursive **tree diff** that pinpoints the first structural divergences
//!   once the outputs are close.

use std::collections::BTreeMap;

/// A normalized SVG element: tag, attributes, text, children. Namespaces and
/// insignificant whitespace are dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct El {
    pub tag: String,
    pub attrs: BTreeMap<String, String>,
    pub text: String,
    pub children: Vec<El>,
}

/// Parse an SVG string into a normalized element tree rooted at `<svg>`.
pub fn parse(svg: &str) -> Result<El, roxmltree::Error> {
    let doc = roxmltree::Document::parse(svg)?;
    Ok(convert(doc.root_element()))
}

fn convert(node: roxmltree::Node) -> El {
    let mut attrs = BTreeMap::new();
    for a in node.attributes() {
        attrs.insert(a.name().to_string(), a.value().to_string());
    }
    let mut text = String::new();
    let mut children = Vec::new();
    for child in node.children() {
        if child.is_element() {
            children.push(convert(child));
        } else if child.is_text() {
            if let Some(t) = child.text() {
                let t = t.trim();
                if !t.is_empty() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(t);
                }
            }
        }
    }
    El {
        tag: node.tag_name().name().to_string(),
        attrs,
        text,
        children,
    }
}

impl El {
    /// Count of this element plus all descendants.
    pub fn size(&self) -> usize {
        1 + self.children.iter().map(El::size).sum::<usize>()
    }

    /// Histogram of tag names over the whole subtree.
    pub fn tag_histogram(&self) -> BTreeMap<String, usize> {
        let mut h = BTreeMap::new();
        self.accumulate_tags(&mut h);
        h
    }

    fn accumulate_tags(&self, h: &mut BTreeMap<String, usize>) {
        *h.entry(self.tag.clone()).or_insert(0) += 1;
        for c in &self.children {
            c.accumulate_tags(h);
        }
    }
}

/// A single structural difference, addressed by a path like `svg/g[0]/rect[1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub path: String,
    pub kind: DiffKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiffKind {
    TagMismatch { reference: String, candidate: String },
    ChildCountMismatch { reference: usize, candidate: usize },
    TextMismatch { reference: String, candidate: String },
    AttrMissing { name: String, reference: String },
    AttrExtra { name: String, candidate: String },
    AttrValueMismatch { name: String, reference: String, candidate: String },
}

/// The result of comparing two SVG trees.
#[derive(Debug, Clone)]
pub struct Report {
    /// Cosine-like overlap of tag histograms, in [0, 1].
    pub tag_similarity: f64,
    pub reference_size: usize,
    pub candidate_size: usize,
    pub differences: Vec<Difference>,
}

impl Report {
    /// True when the trees match structurally within tolerance.
    pub fn is_match(&self) -> bool {
        self.differences.is_empty()
    }
}

/// Comparison options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Absolute tolerance for numeric attribute values.
    pub numeric_tolerance: f64,
    /// Attribute names to ignore entirely (e.g. volatile ids/styles), compared
    /// case-sensitively.
    pub ignore_attrs: Vec<String>,
    /// Cap on reported differences so output stays readable.
    pub max_differences: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            numeric_tolerance: 1.0,
            // id/class/style are non-deterministic or cosmetic; data-points is a
            // base64 blob duplicating the path geometry already checked via `d`.
            ignore_attrs: vec![
                "id".into(),
                "class".into(),
                "style".into(),
                "data-points".into(),
            ],
            max_differences: 50,
        }
    }
}

/// Compare two parsed SVG trees.
pub fn compare(reference: &El, candidate: &El, opts: &Options) -> Report {
    let mut diffs = Vec::new();
    diff_el(reference, candidate, "svg", opts, &mut diffs);
    Report {
        tag_similarity: histogram_similarity(reference, candidate),
        reference_size: reference.size(),
        candidate_size: candidate.size(),
        differences: diffs,
    }
}

fn diff_el(r: &El, c: &El, path: &str, opts: &Options, out: &mut Vec<Difference>) {
    if out.len() >= opts.max_differences {
        return;
    }
    if r.tag != c.tag {
        out.push(Difference {
            path: path.to_string(),
            kind: DiffKind::TagMismatch {
                reference: r.tag.clone(),
                candidate: c.tag.clone(),
            },
        });
        return;
    }
    diff_attrs(r, c, path, opts, out);
    // <style> holds CSS, not diagram structure; mermaid emits a large theme
    // block we don't reproduce. Compare its presence/position but not its text.
    if r.tag != "style" && r.text != c.text {
        out.push(Difference {
            path: path.to_string(),
            kind: DiffKind::TextMismatch {
                reference: r.text.clone(),
                candidate: c.text.clone(),
            },
        });
    }
    if r.children.len() != c.children.len() {
        out.push(Difference {
            path: path.to_string(),
            kind: DiffKind::ChildCountMismatch {
                reference: r.children.len(),
                candidate: c.children.len(),
            },
        });
    }
    for (i, (rc, cc)) in r.children.iter().zip(c.children.iter()).enumerate() {
        let child_path = format!("{path}/{}[{i}]", rc.tag);
        diff_el(rc, cc, &child_path, opts, out);
    }
}

fn diff_attrs(r: &El, c: &El, path: &str, opts: &Options, out: &mut Vec<Difference>) {
    let ignored = |name: &str| opts.ignore_attrs.iter().any(|a| a == name);
    for (name, rv) in &r.attrs {
        if ignored(name) {
            continue;
        }
        match c.attrs.get(name) {
            None => out.push(Difference {
                path: path.to_string(),
                kind: DiffKind::AttrMissing {
                    name: name.clone(),
                    reference: rv.clone(),
                },
            }),
            Some(cv) if !attr_eq(rv, cv, opts.numeric_tolerance) => out.push(Difference {
                path: path.to_string(),
                kind: DiffKind::AttrValueMismatch {
                    name: name.clone(),
                    reference: rv.clone(),
                    candidate: cv.clone(),
                },
            }),
            Some(_) => {}
        }
    }
    for (name, cv) in &c.attrs {
        if !ignored(name) && !r.attrs.contains_key(name) {
            out.push(Difference {
                path: path.to_string(),
                kind: DiffKind::AttrExtra {
                    name: name.clone(),
                    candidate: cv.clone(),
                },
            });
        }
    }
}

/// Attribute equality with numeric tolerance: if both values parse as the same
/// sequence of numbers (with the same interleaved non-numeric tokens), compare
/// the numbers within `tol`; otherwise compare as strings. This handles
/// coordinates, sizes, transforms, and path `d` data uniformly.
fn attr_eq(a: &str, b: &str, tol: f64) -> bool {
    if a == b {
        return true;
    }
    let (at, an) = tokenize(a);
    let (bt, bn) = tokenize(b);
    if at != bt || an.len() != bn.len() {
        return false;
    }
    an.iter().zip(bn.iter()).all(|(x, y)| (x - y).abs() <= tol)
}

/// Split a string into its non-numeric "skeleton" tokens and its numbers.
fn tokenize(s: &str) -> (Vec<String>, Vec<f64>) {
    let mut skeleton = Vec::new();
    let mut numbers = Vec::new();
    let mut cur = String::new();
    let mut num = String::new();

    let flush_num = |num: &mut String, numbers: &mut Vec<f64>, skeleton: &mut Vec<String>| {
        if !num.is_empty() {
            if let Ok(v) = num.parse::<f64>() {
                numbers.push(v);
                skeleton.push("#".to_string());
            } else {
                skeleton.push(std::mem::take(num));
            }
            num.clear();
        }
    };

    for ch in s.chars() {
        let is_num_char = ch.is_ascii_digit()
            || ch == '.'
            || ch == '-'
            || ch == '+'
            || ch == 'e'
            || ch == 'E';
        if is_num_char && (ch.is_ascii_digit() || !num.is_empty() || ch == '-' || ch == '.') {
            if !cur.is_empty() {
                skeleton.push(std::mem::take(&mut cur));
            }
            num.push(ch);
        } else {
            flush_num(&mut num, &mut numbers, &mut skeleton);
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        skeleton.push(cur);
    }
    flush_num(&mut num, &mut numbers, &mut skeleton);
    (skeleton, numbers)
}

/// Cosine similarity of two tag-frequency histograms, in [0, 1].
fn histogram_similarity(a: &El, b: &El) -> f64 {
    let ha = a.tag_histogram();
    let hb = b.tag_histogram();
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for v in ha.values() {
        na += (*v as f64).powi(2);
    }
    for v in hb.values() {
        nb += (*v as f64).powi(2);
    }
    for (tag, va) in &ha {
        if let Some(vb) = hb.get(tag) {
            dot += (*va as f64) * (*vb as f64);
        }
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_trees_match() {
        let a = parse(r#"<svg><g><rect x="1" y="2"/></g></svg>"#).unwrap();
        let b = parse(r#"<svg><g><rect x="1" y="2"/></g></svg>"#).unwrap();
        let report = compare(&a, &b, &Options::default());
        assert!(report.is_match(), "{:?}", report.differences);
        assert!((report.tag_similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn numeric_tolerance_applies() {
        let a = parse(r#"<svg><rect x="10.0" y="20.0"/></svg>"#).unwrap();
        let b = parse(r#"<svg><rect x="10.4" y="20.3"/></svg>"#).unwrap();
        let opts = Options { numeric_tolerance: 0.5, ..Options::default() };
        assert!(compare(&a, &b, &opts).is_match());
        let strict = Options { numeric_tolerance: 0.1, ..Options::default() };
        assert!(!compare(&a, &b, &strict).is_match());
    }

    #[test]
    fn detects_tag_and_text_mismatch() {
        let a = parse(r#"<svg><text>Hello</text></svg>"#).unwrap();
        let b = parse(r#"<svg><text>World</text></svg>"#).unwrap();
        let report = compare(&a, &b, &Options::default());
        assert!(matches!(report.differences[0].kind, DiffKind::TextMismatch { .. }));
    }

    #[test]
    fn path_data_compared_numerically() {
        let a = parse(r#"<svg><path d="M0,0 L10,10"/></svg>"#).unwrap();
        let b = parse(r#"<svg><path d="M0,0 L10.4,9.7"/></svg>"#).unwrap();
        let opts = Options { numeric_tolerance: 0.5, ..Options::default() };
        assert!(compare(&a, &b, &opts).is_match());
    }
}
