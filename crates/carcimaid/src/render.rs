//! SVG emission from a laid-out diagram.
//!
//! The element hierarchy here is a first approximation. Because the project's
//! compliance target is **structural** SVG comparison against the mermaid CLI,
//! this module will be aligned to mermaid's actual DOM (group classes like
//! `.nodes`, `.edgePaths`, `.edgeLabels`, `.clusters`, per-node `g.node`
//! wrappers, arrowhead `<marker>` defs) once the structure is documented. For
//! now it emits a valid, self-describing SVG so the pipeline runs end to end.

use crate::ir::NodeShape;
use crate::layout::{LaidOut, LaidOutFlowchart, PlacedNode};
use std::fmt::Write;

/// Render a laid-out diagram to an SVG document string.
pub fn to_svg(diagram: &LaidOut) -> String {
    match diagram {
        LaidOut::Flowchart(f) => flowchart_svg(f),
    }
}

fn flowchart_svg(chart: &LaidOutFlowchart) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = round(chart.width),
        h = round(chart.height),
    );
    s.push_str(r#"<g class="root">"#);

    // Edges first so nodes paint over their endpoints.
    s.push_str(r#"<g class="edgePaths">"#);
    for edge in &chart.edges {
        let d = path_d(&edge.points);
        let _ = write!(s, r#"<path class="edge" d="{d}" fill="none"/>"#);
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="nodes">"#);
    for node in &chart.nodes {
        render_node(&mut s, node);
    }
    s.push_str("</g>");

    s.push_str("</g></svg>");
    s
}

fn render_node(s: &mut String, node: &PlacedNode) {
    let x = node.cx - node.width / 2.0;
    let y = node.cy - node.height / 2.0;
    let _ = write!(s, r#"<g class="node" id="{}">"#, escape(&node.id));
    match node.shape {
        NodeShape::Rectangle => {
            let _ = write!(
                s,
                r#"<rect class="basic" x="{}" y="{}" width="{}" height="{}"/>"#,
                round(x),
                round(y),
                round(node.width),
                round(node.height)
            );
        }
        NodeShape::RoundedRectangle | NodeShape::Stadium => {
            let rx = if matches!(node.shape, NodeShape::Stadium) {
                node.height / 2.0
            } else {
                5.0
            };
            let _ = write!(
                s,
                r#"<rect class="basic" x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}"/>"#,
                round(x),
                round(y),
                round(node.width),
                round(node.height),
                round(rx),
                round(rx)
            );
        }
        NodeShape::Circle => {
            let r = node.width.max(node.height) / 2.0;
            let _ = write!(
                s,
                r#"<circle class="basic" cx="{}" cy="{}" r="{}"/>"#,
                round(node.cx),
                round(node.cy),
                round(r)
            );
        }
        NodeShape::Rhombus => {
            let (cx, cy) = (node.cx, node.cy);
            let (hw, hh) = (node.width / 2.0, node.height / 2.0);
            let _ = write!(
                s,
                r#"<polygon class="basic" points="{},{} {},{} {},{} {},{}"/>"#,
                round(cx),
                round(cy - hh),
                round(cx + hw),
                round(cy),
                round(cx),
                round(cy + hh),
                round(cx - hw),
                round(cy)
            );
        }
    }
    let _ = write!(
        s,
        r#"<text class="label" x="{}" y="{}" text-anchor="middle" dominant-baseline="central">{}</text>"#,
        round(node.cx),
        round(node.cy),
        escape(&node.label)
    );
    s.push_str("</g>");
}

fn path_d(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        let _ = write!(d, "{cmd}{},{} ", round(*x), round(*y));
    }
    d.trim_end().to_string()
}

/// Round to a stable precision so output is deterministic and diff-friendly.
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use crate::render_to_svg;

    #[test]
    fn renders_valid_svg_root() {
        let svg = render_to_svg("flowchart TD\n A[Start] --> B[End]").unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("class=\"nodes\""));
        assert!(svg.contains("class=\"edgePaths\""));
        assert!(svg.trim_end().ends_with("</svg>"));
    }
}
