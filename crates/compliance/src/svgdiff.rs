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
    /// A child present in the reference has no counterpart in the candidate.
    ElementMissing { key: String },
    /// A child present in the candidate has no counterpart in the reference.
    ElementExtra { key: String },
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
            // id/class are non-deterministic or cosmetic; data-points is a base64
            // blob duplicating the path geometry already checked via `d`. `style`
            // is compared, but as a normalized property set (see diff_attrs).
            ignore_attrs: vec![
                "id".into(),
                "class".into(),
                "data-points".into(),
            ],
            max_differences: 50,
        }
    }
}

/// Compare two parsed SVG trees.
pub fn compare(reference: &El, candidate: &El, opts: &Options) -> Report {
    let mut diffs = Vec::new();
    diff_el(reference, candidate, "svg", opts, &mut diffs, false);
    Report {
        tag_similarity: histogram_similarity(reference, candidate),
        reference_size: reference.size(),
        candidate_size: candidate.size(),
        differences: diffs,
    }
}

fn diff_el(r: &El, c: &El, path: &str, opts: &Options, out: &mut Vec<Difference>, rough: bool) {
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
    diff_attrs(r, c, path, opts, out, rough);
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
    diff_children(r, c, path, opts, out);
}

/// A matching key for aligning children across the two trees: tag + first class
/// token + id. Deliberately excludes text so that a leaf whose text differs is
/// still *matched* (and reported as a `TextMismatch`) rather than shown as a
/// missing/extra pair.
fn child_key(el: &El) -> String {
    let class = el.attrs.get("class").and_then(|c| c.split_whitespace().next()).unwrap_or("");
    let id = el.attrs.get("id").map(|i| strip_id_counter(i)).unwrap_or("");
    format!("{}|{}|{}", el.tag, class, id)
}

/// Strip a trailing `-N`/`_N` insertion counter from an id. mermaid suffixes
/// node ids with a per-node counter (`…-flowchart-C-2`) that we don't reproduce
/// exactly, so it must not defeat key matching.
fn strip_id_counter(id: &str) -> &str {
    if let Some(pos) = id.rfind(['-', '_']) {
        if pos + 1 < id.len() && id[pos + 1..].bytes().all(|b| b.is_ascii_digit()) {
            return &id[..pos];
        }
    }
    id
}

/// Align children by key using a longest-common-subsequence, so an inserted or
/// omitted child is reported precisely instead of shifting every later child
/// out of alignment (which would mask real per-element differences).
fn diff_children(r: &El, c: &El, path: &str, opts: &Options, out: &mut Vec<Difference>) {
    // Children of a rough shape group (`<g class="…outer-path">`) are rough
    // paths whose bezier control points are non-deterministic — mark them so
    // their `d` is compared by anchor points. (`rc.polygon` fills are pure M/L,
    // so anchor comparison is exact for them anyway.)
    let child_rough = is_rough_group(r) || is_rough_group(c);
    let rk: Vec<String> = r.children.iter().map(child_key).collect();
    let ck: Vec<String> = c.children.iter().map(child_key).collect();
    let (n, m) = (rk.len(), ck.len());

    // LCS length table over the key sequences.
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if rk[i] == ck[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let missing = |out: &mut Vec<Difference>, k: &str| {
        out.push(Difference { path: path.to_string(), kind: DiffKind::ElementMissing { key: k.to_string() } });
    };
    let extra = |out: &mut Vec<Difference>, k: &str| {
        out.push(Difference { path: path.to_string(), kind: DiffKind::ElementExtra { key: k.to_string() } });
    };

    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if out.len() >= opts.max_differences {
            return;
        }
        if rk[i] == ck[j] {
            let child_path = format!("{path}/{}[{i}]", r.children[i].tag);
            diff_el(&r.children[i], &c.children[j], &child_path, opts, out, child_rough);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            missing(out, &rk[i]);
            i += 1;
        } else {
            extra(out, &ck[j]);
            j += 1;
        }
    }
    while i < n && out.len() < opts.max_differences {
        missing(out, &rk[i]);
        i += 1;
    }
    while j < m && out.len() < opts.max_differences {
        extra(out, &ck[j]);
        j += 1;
    }
}

fn diff_attrs(r: &El, c: &El, path: &str, opts: &Options, out: &mut Vec<Difference>, rough: bool) {
    let ignored = |name: &str| opts.ignore_attrs.iter().any(|a| a == name);
    // A rough shape path (a fill or stroke path inside a `<g class="…outer-path">`
    // emitted by rough.js) has NON-DETERMINISTIC bezier control points: mermaid
    // seeds rough.js with 0, which falls back to Math.random(), so its `d`
    // differs on every render and can never be byte-matched. Its endpoints (the
    // actual shape vertices) ARE stable, so we compare such a path by its anchor
    // points within tolerance — verifying the outline while forgiving only the
    // random control points. A bare stroke path (`fill="none"`) is treated the
    // same. Edges are unaffected (their fill:none lives in `style`, not a `fill`
    // attr, and they are not inside an outer-path group).
    let d_by_anchor = (rough || is_stroke_path(r)) && r.tag == "path";
    for (name, rv) in &r.attrs {
        if ignored(name) {
            continue;
        }
        if name == "d" && d_by_anchor {
            let cv = c.attrs.get("d").map(String::as_str).unwrap_or("");
            if !stroke_path_eq(rv, cv, opts.numeric_tolerance) {
                out.push(Difference {
                    path: path.to_string(),
                    kind: DiffKind::AttrValueMismatch {
                        name: name.clone(),
                        reference: rv.clone(),
                        candidate: cv.to_string(),
                    },
                });
            }
            continue;
        }
        // `style` is compared as a normalized property set, so an empty style and
        // an absent one are equal and property order / duplicates don't matter.
        if name == "style" {
            let cv = c.attrs.get(name).map(String::as_str).unwrap_or("");
            if !style_eq(rv, cv) {
                out.push(Difference {
                    path: path.to_string(),
                    kind: DiffKind::AttrValueMismatch {
                        name: name.clone(),
                        reference: rv.clone(),
                        candidate: cv.to_string(),
                    },
                });
            }
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
        if ignored(name) || r.attrs.contains_key(name) {
            continue;
        }
        // A candidate-only `style` matters only if it carries real properties.
        if name == "style" {
            if !style_eq("", cv) {
                out.push(Difference {
                    path: path.to_string(),
                    kind: DiffKind::AttrExtra { name: name.clone(), candidate: cv.clone() },
                });
            }
            continue;
        }
        out.push(Difference {
            path: path.to_string(),
            kind: DiffKind::AttrExtra { name: name.clone(), candidate: cv.clone() },
        });
    }
}

/// Compare two `style` attribute values as property sets: split into `k:v`
/// declarations, drop `!important`/whitespace/duplicates, and compare
/// order-independently. Handles mermaid's duplicated (`;;;`) declarations and
/// our `!important` formatting uniformly.
fn style_eq(a: &str, b: &str) -> bool {
    normalize_style(a) == normalize_style(b)
}

fn normalize_style(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split(';')
        .map(|d| d.replace("!important", ""))
        .map(|d| match d.split_once(':') {
            Some((k, val)) => format!("{}:{}", k.trim(), val.trim()),
            None => d.trim().to_string(),
        })
        .filter(|d| !d.is_empty() && d != ":")
        .collect();
    v.sort();
    v.dedup();
    v
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

/// Whether `el` is a rough shape's stroke `<path>`: `fill="none"` as an
/// attribute. Rough shapes emit the outline stroke this way; our own edges keep
/// `fill:none` inside `style`, so they are (correctly) excluded.
fn is_stroke_path(el: &El) -> bool {
    el.tag == "path" && el.attrs.get("fill").map(|f| f == "none").unwrap_or(false)
}

/// Whether `el` is a rough shape wrapper — the `<g class="…outer-path">` that
/// rough.js emits, holding fill/stroke/inner-detail paths with non-deterministic
/// control points. Its child paths are compared by anchor points.
fn is_rough_group(el: &El) -> bool {
    el.tag == "g"
        && el
            .attrs
            .get("class")
            .map(|c| c.split_whitespace().any(|t| t == "outer-path"))
            .unwrap_or(false)
}

/// Compare two rough stroke paths by their anchor points (each path command's
/// endpoint), ignoring bezier control points, within `tol`. Equal anchor counts
/// are required — a differing outline (wrong/missing segments) still fails.
fn stroke_path_eq(a: &str, b: &str, tol: f64) -> bool {
    let (pa, pb) = (path_anchor_points(a), path_anchor_points(b));
    pa.len() == pb.len()
        && pa
            .iter()
            .zip(pb.iter())
            .all(|((ax, ay), (bx, by))| (ax - bx).abs() <= tol && (ay - by).abs() <= tol)
}

/// The anchor points of an SVG path `d`: the endpoint of each drawing command
/// (the pen position after it), dropping control points. Handles the commands
/// rough.js emits (`M`/`L`/`C`) plus `H`/`V`/`S`/`Q`/`T`/`A`/`Z`. Absolute
/// coordinates (what rough.js produces); relative commands are not expected.
fn path_anchor_points(d: &str) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let (mut sx, mut sy) = (0.0f64, 0.0f64); // current subpath start (for Z)
    let mut i = 0;
    let bytes = d.as_bytes();
    // Read the run of numbers following a command letter at position `i`.
    let read_nums = |start: usize| -> (Vec<f64>, usize) {
        let mut nums = Vec::new();
        let mut j = start;
        let mut cur = String::new();
        while j < bytes.len() {
            let ch = bytes[j] as char;
            if ch.is_ascii_alphabetic() {
                break;
            }
            let is_num = ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E';
            // A '-'/'+' starts a new number unless it's an exponent sign.
            let is_sign = (ch == '-' || ch == '+')
                && !(cur.ends_with('e') || cur.ends_with('E'));
            if is_sign && !cur.is_empty() {
                if let Ok(v) = cur.parse() {
                    nums.push(v);
                }
                cur.clear();
            }
            if is_num || ch == '-' || ch == '+' {
                cur.push(ch);
            } else if !cur.is_empty() {
                if let Ok(v) = cur.parse() {
                    nums.push(v);
                }
                cur.clear();
            }
            j += 1;
        }
        if !cur.is_empty() {
            if let Ok(v) = cur.parse() {
                nums.push(v);
            }
        }
        (nums, j)
    };
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if !ch.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let (nums, next) = read_nums(i + 1);
        match ch {
            'M' => {
                if nums.len() >= 2 {
                    x = nums[0];
                    y = nums[1];
                    sx = x;
                    sy = y;
                    pts.push((x, y));
                }
            }
            'L' | 'T' => {
                if nums.len() >= 2 {
                    x = nums[nums.len() - 2];
                    y = nums[nums.len() - 1];
                    pts.push((x, y));
                }
            }
            'C' | 'S' | 'Q' | 'A' => {
                if nums.len() >= 2 {
                    x = nums[nums.len() - 2];
                    y = nums[nums.len() - 1];
                    pts.push((x, y));
                }
            }
            'H' => {
                if let Some(&v) = nums.last() {
                    x = v;
                    pts.push((x, y));
                }
            }
            'V' => {
                if let Some(&v) = nums.last() {
                    y = v;
                    pts.push((x, y));
                }
            }
            'Z' | 'z' => {
                x = sx;
                y = sy;
                pts.push((x, y));
            }
            _ => {}
        }
        i = next;
    }
    pts
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
    fn inserted_child_does_not_cascade() {
        // Reference has an extra <title> before the shared <g>; the <g> and its
        // contents must still align (no cascade of tag mismatches).
        let a = parse(r#"<svg><title>t</title><g><rect x="1"/></g></svg>"#).unwrap();
        let b = parse(r#"<svg><g><rect x="1"/></g></svg>"#).unwrap();
        let report = compare(&a, &b, &Options::default());
        let kinds: Vec<_> = report.differences.iter().map(|d| &d.kind).collect();
        // Exactly one missing element (the title); the <g>/<rect> match cleanly.
        assert_eq!(report.differences.len(), 1, "{:?}", report.differences);
        assert!(matches!(kinds[0], DiffKind::ElementMissing { .. }));
    }

    #[test]
    fn rough_stroke_paths_match_by_anchor_points() {
        // Two rough strokes: same anchor endpoints, different (random) control
        // points. They must match; a stroke with a moved endpoint must not.
        let a = parse(
            r##"<svg><path fill="none" stroke="#000" d="M0 0 C10 0, 20 0, 30 0 M30 0 C30 10, 30 20, 30 30"/></svg>"##,
        )
        .unwrap();
        let b = parse(
            r##"<svg><path fill="none" stroke="#000" d="M0 0 C5 1, 25 -1, 30 0 M30 0 C31 15, 29 25, 30 30"/></svg>"##,
        )
        .unwrap();
        assert!(compare(&a, &b, &Options::default()).is_match());

        let moved = parse(
            r##"<svg><path fill="none" stroke="#000" d="M0 0 C5 1, 25 -1, 40 0 M40 0 C31 15, 29 25, 30 30"/></svg>"##,
        )
        .unwrap();
        assert!(!compare(&a, &moved, &Options::default()).is_match());
    }

    #[test]
    fn fill_paths_still_compared_exactly() {
        // A filled path (not fill:none) is NOT given anchor-only tolerance: a
        // moved control point is a real difference.
        let a = parse(r##"<svg><path fill="#eee" d="M0 0 C10 0, 20 0, 30 0"/></svg>"##).unwrap();
        let b = parse(r##"<svg><path fill="#eee" d="M0 0 C5 5, 25 5, 30 0"/></svg>"##).unwrap();
        assert!(!compare(&a, &b, &Options::default()).is_match());
    }

    #[test]
    fn path_data_compared_numerically() {
        let a = parse(r#"<svg><path d="M0,0 L10,10"/></svg>"#).unwrap();
        let b = parse(r#"<svg><path d="M0,0 L10.4,9.7"/></svg>"#).unwrap();
        let opts = Options { numeric_tolerance: 0.5, ..Options::default() };
        assert!(compare(&a, &b, &opts).is_match());
    }
}
