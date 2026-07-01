//! SVG emission from a laid-out diagram.
//!
//! The output is aligned to the DOM that mermaid's flowchart renderer produces
//! when run with `htmlLabels: false` (SVG `<text>` labels rather than HTML
//! `foreignObject`s), because the project's compliance target is a *structural*
//! diff against the mermaid CLI. The hierarchy mirrors mermaid:
//!
//! ```text
//! svg.flowchart
//!   style
//!   g                         (wrapper)
//!     marker × 12             (arrowheads)
//!     g.root
//!       g.clusters
//!       g.edgePaths > path.flowchart-link
//!       g.edgeLabels > g.edgeLabel > g.label > … > text
//!       g.nodes > g.node > shape.label-container + g.label > … > text
//!   defs (drop-shadow filter)
//!   defs (drop-shadow-small filter)
//! ```
//!
//! Geometry is still produced by the placeholder layout, so numeric
//! coordinates do not yet match mermaid — but the *structure* does, which is
//! what the structural comparator keys on first. See `ATTRIBUTION.md`.

mod markers;

use crate::ir::NodeShape;
use crate::layout::{LaidOut, LaidOutFlowchart, PlacedCluster, PlacedEdge, PlacedNode};
use std::fmt::Write;

/// Diagram id prefix. mermaid generates a per-render id (`my-svg`, etc.); the
/// exact value is irrelevant to structural comparison (ids are ignored) but it
/// must be internally consistent between markers and the edges that reference
/// them. We use `my-svg` to match the id the mermaid CLI assigns by default, so
/// `marker-end="url(#...)"` references compare equal.
const ID: &str = "my-svg";

/// Approximate single-line label height in px (mermaid ~19 at 16px font).
const LABEL_HEIGHT: f64 = 19.0;

/// Render a laid-out diagram to an SVG document string.
pub fn to_svg(diagram: &LaidOut) -> String {
    match diagram {
        LaidOut::Flowchart(f) => flowchart_svg(f),
    }
}

fn flowchart_svg(chart: &LaidOutFlowchart) -> String {
    let mut s = String::new();
    let (w, h) = (round(chart.width), round(chart.height));
    let _ = write!(
        s,
        concat!(
            r#"<svg id="{id}" width="{w}" xmlns="http://www.w3.org/2000/svg" "#,
            r#"xmlns:xlink="http://www.w3.org/1999/xlink" class="flowchart" "#,
            r#"height="{h}" viewBox="0 0 {w} {h}" role="graphics-document document" "#,
            r#"aria-roledescription="flowchart-v2" style="background-color: white;">"#,
        ),
        id = ID,
        w = w,
        h = h,
    );

    // 1. <style> — mermaid emits a large CSS block here; we emit an empty one
    //    to keep the child structure aligned. The comparator ignores <style>
    //    text content.
    s.push_str("<style></style>");

    // 2. wrapper <g> with markers + g.root
    s.push_str("<g>");
    s.push_str(&markers::block(ID));
    s.push_str(r#"<g class="root">"#);

    s.push_str(r#"<g class="clusters">"#);
    for cluster in &chart.clusters {
        render_cluster(&mut s, cluster);
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="edgePaths">"#);
    for edge in &chart.edges {
        render_edge_path(&mut s, edge, &chart.nodes);
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="edgeLabels">"#);
    for edge in &chart.edges {
        render_edge_label(&mut s, edge, &chart.nodes);
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="nodes">"#);
    for node in &chart.nodes {
        render_node(&mut s, node);
    }
    s.push_str("</g>");

    s.push_str("</g></g>"); // close g.root and wrapper g

    // 3. drop-shadow filter defs (verbatim mermaid).
    let _ = write!(
        s,
        concat!(
            r#"<defs><filter id="{id}-drop-shadow" height="130%" width="130%">"#,
            r##"<feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"/>"##,
            r#"</filter></defs>"#,
            r#"<defs><filter id="{id}-drop-shadow-small" height="150%" width="150%">"#,
            r##"<feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"/>"##,
            r#"</filter></defs>"#,
        ),
        id = ID,
    );

    s.push_str("</svg>");
    s
}

fn render_cluster(s: &mut String, cluster: &PlacedCluster) {
    let x = cluster.cx - cluster.width / 2.0;
    let y = cluster.cy - cluster.height / 2.0;
    let _ = write!(
        s,
        r#"<g class="cluster" id="{}-{}" data-look="classic"><rect x="{}" y="{}" width="{}" height="{}"/>"#,
        ID,
        escape(&cluster.id),
        round(x),
        round(y),
        round(cluster.width),
        round(cluster.height),
    );
    // The label sits centred at the top of the cluster box.
    let label_x = cluster.cx - crate::text::measure_width(&cluster.title, 16.0) / 2.0;
    let _ = write!(
        s,
        r#"<g class="cluster-label" transform="translate({}, {})"><g>"#,
        round(label_x),
        round(y),
    );
    s.push_str(r#"<rect class="background"/>"#);
    render_text(s, Some(&cluster.title), false);
    s.push_str("</g></g></g>");
}

fn render_node(s: &mut String, node: &PlacedNode) {
    let _ = write!(
        s,
        r#"<g class="node default" id="{}-flowchart-{}-0" data-look="classic" transform="translate({}, {})">"#,
        ID,
        escape(&node.id),
        round(node.cx),
        round(node.cy),
    );
    render_shape(s, node);
    // g.label is offset up by half the label height, matching mermaid.
    let _ = write!(
        s,
        r#"<g class="label" transform="translate(0, {})"><rect/><g>"#,
        round(-LABEL_HEIGHT / 2.0),
    );
    s.push_str(r#"<rect class="background"/>"#);
    render_text(s, Some(&node.label), false);
    s.push_str("</g></g></g>");
}

/// Emit the node's outline shape, centred at the group origin.
fn render_shape(s: &mut String, node: &PlacedNode) {
    let (hw, hh) = (node.width / 2.0, node.height / 2.0);
    match node.shape {
        NodeShape::Rectangle => {
            let _ = write!(
                s,
                r#"<rect class="basic label-container" x="{}" y="{}" width="{}" height="{}"/>"#,
                round(-hw),
                round(-hh),
                round(node.width),
                round(node.height),
            );
        }
        NodeShape::RoundedRectangle | NodeShape::Stadium => {
            let rx = if matches!(node.shape, NodeShape::Stadium) { hh } else { 5.0 };
            let _ = write!(
                s,
                r#"<rect class="basic label-container" x="{}" y="{}" rx="{}" ry="{}" width="{}" height="{}"/>"#,
                round(-hw),
                round(-hh),
                round(rx),
                round(rx),
                round(node.width),
                round(node.height),
            );
        }
        NodeShape::Circle => {
            let r = node.width.max(node.height) / 2.0;
            let _ = write!(s, r#"<circle class="basic label-container" r="{}"/>"#, round(r));
        }
        NodeShape::Rhombus => {
            // Diamond around the origin.
            let _ = write!(
                s,
                r#"<polygon class="label-container" points="{},0 {},{} 0,{} {},{}"/>"#,
                round(hw),
                round(node.width),
                round(-hh),
                round(-node.height),
                round(-hw),
                round(-hh),
            );
        }
    }
}

/// `L_<fromId>_<toId>_0`, mermaid's stable edge id (uses node ids, not indices).
/// Escaped so it is always a valid XML attribute value.
fn edge_id(edge: &PlacedEdge, nodes: &[PlacedNode]) -> String {
    escape(&format!("L_{}_{}_0", nodes[edge.from].id, nodes[edge.to].id))
}

/// Arrow inset: mermaid shortens the path at the arrow end by this many px so
/// the arrowhead marker sits flush against the target node border.
const ARROW_INSET: f64 = 4.0;

fn render_edge_path(s: &mut String, edge: &PlacedEdge, nodes: &[PlacedNode]) {
    let mut points = edge.points.clone();
    if edge.arrow {
        clip_end(&mut points, ARROW_INSET);
    }
    let d = curve_basis(&points);
    let marker = if edge.arrow {
        format!(r#" marker-end="url(#{ID}_flowchart-v2-pointEnd)""#)
    } else {
        String::new()
    };
    let _ = write!(
        s,
        concat!(
            r#"<path id="{id}-{eid}" "#,
            r#"class="edge-thickness-normal edge-pattern-solid flowchart-link" "#,
            r#"d="{d}" data-edge="true" data-et="edge" data-id="{eid}" data-look="classic"{marker}/>"#,
        ),
        id = ID,
        eid = edge_id(edge, nodes),
        d = d,
        marker = marker,
    );
}

/// Move the last waypoint toward the previous one by `inset` px, so the
/// arrowhead fits — matching mermaid's path shortening at the arrow end.
fn clip_end(points: &mut [(f64, f64)], inset: f64) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let (x1, y1) = points[n - 1];
    let (x0, y0) = points[n - 2];
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len = (dx * dx + dy * dy).sqrt();
    if len > inset {
        let t = (len - inset) / len;
        points[n - 1] = (x0 + dx * t, y0 + dy * t);
    }
}

/// Emit the edge label. mermaid's structure differs for labelled vs unlabelled
/// edges (see the comments below), so we mirror both exactly.
fn render_edge_label(s: &mut String, edge: &PlacedEdge, nodes: &[PlacedNode]) {
    let eid = edge_id(edge, nodes);
    match &edge.label {
        Some(label) => {
            // Labelled: g.edgeLabel[transform] > g.label[data-id] > g >
            //           (rect.background + text).
            let _ = write!(
                s,
                r#"<g class="edgeLabel"><g class="label" data-id="{eid}" transform="translate(0, -10.5)"><g>"#,
            );
            s.push_str(r#"<rect class="background"/>"#);
            render_text(s, Some(label), true);
            s.push_str("</g></g></g>");
        }
        None => {
            // Unlabelled: an empty g.edgeLabel plus a sibling g holding only the
            // background rect — exactly what mermaid emits.
            let _ = write!(
                s,
                r#"<g class="edgeLabel"><g class="label" data-id="{eid}" transform="translate(0, 0)">"#,
            );
            render_text(s, None, true);
            s.push_str(r#"</g></g><g><rect class="background"/></g>"#);
        }
    }
}

/// Emit mermaid's nested `<text><tspan.outer><tspan.inner>` label structure.
/// With `label = None` the inner tspan is omitted (mermaid self-closes the
/// outer tspan for empty labels). `anchor` adds `text-anchor="middle"`, which
/// mermaid sets on edge labels but not node labels.
fn render_text(s: &mut String, label: Option<&str>, anchor: bool) {
    let ta = if anchor { r#" text-anchor="middle""# } else { "" };
    let _ = write!(
        s,
        r#"<text y="{y}"{ta}><tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em"{ta}>"#,
        y = round(-LABEL_HEIGHT / 2.0 - 0.6),
        ta = ta,
    );
    if let Some(label) = label {
        let _ = write!(
            s,
            r#"<tspan font-style="normal" class="text-inner-tspan" font-weight="normal">{}</tspan>"#,
            escape(label),
        );
    }
    s.push_str("</tspan></text>");
}

/// Render a path through `points` as a B-spline, matching d3's `curveBasis`
/// (mermaid's default flowchart edge curve). Reproduces d3's open-basis curve
/// lifecycle exactly so the emitted `M…L…C…C…L…` matches mermaid's `d`.
fn curve_basis(points: &[(f64, f64)]) -> String {
    let n = points.len();
    if n == 0 {
        return String::new();
    }
    if n == 1 {
        return format!("M{},{}", num(points[0].0), num(points[0].1));
    }
    if n == 2 {
        return format!(
            "M{},{}L{},{}",
            num(points[0].0),
            num(points[0].1),
            num(points[1].0),
            num(points[1].1)
        );
    }

    let mut d = String::new();
    let (mut x0, mut y0) = (f64::NAN, f64::NAN);
    let (mut x1, mut y1) = (f64::NAN, f64::NAN);
    let bezier = |d: &mut String, x0: f64, y0: f64, x1: f64, y1: f64, x: f64, y: f64| {
        let _ = write!(
            d,
            "C{},{},{},{},{},{}",
            num((2.0 * x0 + x1) / 3.0),
            num((2.0 * y0 + y1) / 3.0),
            num((x0 + 2.0 * x1) / 3.0),
            num((y0 + 2.0 * y1) / 3.0),
            num((x0 + 4.0 * x1 + x) / 6.0),
            num((y0 + 4.0 * y1 + y) / 6.0),
        );
    };

    for (i, &(x, y)) in points.iter().enumerate() {
        match i {
            0 => {
                let _ = write!(d, "M{},{}", num(x), num(y));
            }
            1 => {}
            2 => {
                let _ = write!(d, "L{},{}", num((5.0 * x0 + x1) / 6.0), num((5.0 * y0 + y1) / 6.0));
                bezier(&mut d, x0, y0, x1, y1, x, y);
            }
            _ => bezier(&mut d, x0, y0, x1, y1, x, y),
        }
        x0 = x1;
        x1 = x;
        y0 = y1;
        y1 = y;
    }
    // Curve end: emit the final bezier and line segment (d3 basis lineEnd).
    bezier(&mut d, x0, y0, x1, y1, x1, y1);
    let _ = write!(d, "L{},{}", num(x1), num(y1));
    d
}

/// Format a coordinate like d3-path/mermaid: round to 3 decimals, trim trailing
/// zeros and a trailing dot.
fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let mut s = format!("{r:.3}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// Round to a stable precision so node/shape output is deterministic.
fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
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
    fn renders_mermaid_aligned_structure() {
        let svg = render_to_svg("flowchart TD\n A[Start] --> B[End]").unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r#"class="flowchart""#));
        assert!(svg.contains(r#"aria-roledescription="flowchart-v2""#));
        assert!(svg.contains(r#"<g class="nodes">"#));
        assert!(svg.contains(r#"<g class="edgePaths">"#));
        assert!(svg.contains("flowchart-link"));
        assert!(svg.contains("flowchart-v2-pointEnd"));
        assert!(svg.contains("text-inner-tspan"));
        assert!(!svg.contains("foreignObject"));
        assert!(svg.ends_with("</svg>"));
    }
}
