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
/// Extra height per wrapped line (mermaid's 1.1em row step at 16px).
const LINE_SPACING: f64 = 17.6;
/// Vertical space reserved above the diagram for a visible title.
const TITLE_SPACE: f64 = 50.0;
/// Font size of the visible title (mermaid `.flowchartTitleText`).
const TITLE_FONT_SIZE: f64 = 18.0;
/// Diagram margin used when a title widens the viewBox.
const TITLE_MARGIN: f64 = 8.0;

/// Render a laid-out diagram to an SVG document string.
pub fn to_svg(diagram: &LaidOut) -> String {
    match diagram {
        LaidOut::Flowchart(f) => flowchart_svg(f),
    }
}

fn flowchart_svg(chart: &LaidOutFlowchart) -> String {
    let mut s = String::new();
    let w = round(chart.width);
    // A visible title reserves TITLE_SPACE above the diagram (viewBox top shifts
    // up, height grows) and, when it's wider than the content, widens and
    // re-centres the viewBox — mermaid's viewBox is the getBBox of content plus
    // the centred title text (font-size 18) ± an 8px margin.
    let title_space = if chart.title.is_some() { TITLE_SPACE } else { 0.0 };
    let vh = round(chart.height + title_space);
    let vy = round(-title_space);
    let (vx, vw) = match &chart.title {
        Some(t) => {
            let tw = crate::text::measure_width(t, TITLE_FONT_SIZE);
            let left = (chart.width / 2.0 - tw / 2.0).min(TITLE_MARGIN);
            let right = (chart.width / 2.0 + tw / 2.0).max(chart.width - TITLE_MARGIN);
            (round(left - TITLE_MARGIN), round(right - left + 2.0 * TITLE_MARGIN))
        }
        None => (0.0, w),
    };
    // Accessibility metadata references (only present when acc* were given).
    let mut aria = String::new();
    if chart.acc_title.is_some() {
        let _ = write!(aria, r#" aria-labelledby="chart-title-{ID}""#);
    }
    if chart.acc_descr.is_some() {
        let _ = write!(aria, r#" aria-describedby="chart-desc-{ID}""#);
    }
    let _ = write!(
        s,
        concat!(
            r#"<svg id="{id}" width="{vw}" xmlns="http://www.w3.org/2000/svg" "#,
            r#"xmlns:xlink="http://www.w3.org/1999/xlink" class="flowchart" "#,
            r#"height="{vh}" viewBox="{vx} {vy} {vw} {vh}" role="graphics-document document" "#,
            r#"aria-roledescription="flowchart-v2"{aria} style="background-color: white;">"#,
        ),
        id = ID,
        vw = vw,
        vx = vx,
        vh = vh,
        vy = vy,
        aria = aria,
    );

    // <title>/<desc> from accTitle/accDescr, before <style> (mermaid's order).
    if let Some(t) = &chart.acc_title {
        let _ = write!(s, r#"<title id="chart-title-{ID}">{}</title>"#, escape(t));
    }
    if let Some(d) = &chart.acc_descr {
        let _ = write!(s, r#"<desc id="chart-desc-{ID}">{}</desc>"#, escape(d));
    }

    // 1. <style> — a focused theme mirroring mermaid's defaults. Crucially it
    //    centres node labels (`text-anchor:middle`) and gives edge labels a
    //    background; without it our labels are left-anchored (off-centre). The
    //    comparator ignores <style> text, so this adds no structural diff.
    s.push_str(&style_block());

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

    // 4. Visible title (last svg child), horizontally centred above the diagram.
    if let Some(t) = &chart.title {
        let _ = write!(
            s,
            r#"<text text-anchor="middle" x="{}" y="-25" class="flowchartTitleText">{}</text>"#,
            round(w / 2.0),
            escape(t),
        );
    }

    s.push_str("</svg>");
    s
}

/// A focused stylesheet mirroring mermaid's default theme for the elements we
/// emit. Scoped to the diagram id so it doesn't leak. Mirrors mermaid's own
/// approach of embedding node/edge/label styling (which is why label centering
/// lives here, not in a `text-anchor` attribute).
fn style_block() -> String {
    const CSS: &str = concat!(
        "SVGID{font-family:\"trebuchet ms\",verdana,arial,sans-serif;font-size:16px;fill:#333;}",
        "SVGID .label{font-family:\"trebuchet ms\",verdana,arial,sans-serif;color:#333;}",
        "SVGID .label text{fill:#333;}",
        "SVGID .node .label text{text-anchor:middle;}",
        "SVGID .label-container{fill:#ECECFF;stroke:#9370DB;stroke-width:1px;}",
        "SVGID .cluster rect{fill:#ffffde;stroke:#aaaa33;stroke-width:1px;}",
        "SVGID .flowchart-link{stroke:#333;fill:none;}",
        "SVGID .edgeLabel{background-color:rgba(232,232,232,0.8);}",
        "SVGID .edgeLabel rect{opacity:0.5;fill:rgba(232,232,232,0.8);}",
        "SVGID .marker{fill:#333;stroke:#333;}",
        "SVGID .arrowMarkerPath{fill:#333;stroke:#333;}",
    );
    format!("<style>{}</style>", CSS.replace("SVGID", &format!("#{ID}")))
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
    // g.label is offset up by half the label block height so the (possibly
    // multi-line) text is vertically centred, matching mermaid.
    let n = crate::text::wrap_label(&node.label, crate::text::WRAP_WIDTH, 16.0).len().max(1);
    let block_h = LABEL_HEIGHT + (n as f64 - 1.0) * LINE_SPACING;
    let _ = write!(
        s,
        r#"<g class="label" transform="translate(0, {})"><rect/><g>"#,
        round(-block_h / 2.0),
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
            // Centred at the node group's origin (the group is already
            // translated to the node centre), matching mermaid's `cx=0 cy=0`.
            let _ = write!(
                s,
                r#"<circle class="basic label-container" cx="0" cy="0" r="{}"/>"#,
                round(node.width / 2.0),
            );
        }
        NodeShape::Rhombus => {
            // mermaid `question`: a square diamond of side `side`, points laid
            // out in [0,side]×[-side,0] then translated to centre it.
            let side = node.width;
            let _ = write!(
                s,
                r#"<polygon class="label-container" points="{},0 {},{} {},{} 0,{}" transform="translate({}, {})"/>"#,
                round(side / 2.0),
                round(side),
                round(-side / 2.0),
                round(side / 2.0),
                round(-side),
                round(-side / 2.0),
                round(-side / 2.0 + 0.5),
                round(side / 2.0),
            );
        }
        NodeShape::Hexagon => {
            let (w, h) = (node.width, node.height);
            let m = h / 4.0;
            emit_polygon(
                s,
                &[(m, 0.0), (w - m, 0.0), (w, -h / 2.0), (w - m, -h), (m, -h), (0.0, -h / 2.0)],
                -w / 2.0,
                h / 2.0,
            );
        }
        NodeShape::Subroutine => {
            let (w, h) = (node.width - 16.0, node.height); // inner width
            emit_polygon(
                s,
                &[
                    (0.0, 0.0), (w, 0.0), (w, -h), (0.0, -h), (0.0, 0.0),
                    (-8.0, 0.0), (w + 8.0, 0.0), (w + 8.0, -h), (-8.0, -h), (-8.0, 0.0),
                ],
                -w / 2.0,
                h / 2.0,
            );
        }
        // Slanted shapes: recover the inner width (dagre width minus the h/2
        // overflow on each side), and lay points out around it.
        NodeShape::Parallelogram => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(-h / 2.0, 0.0), (w, 0.0), (w + h / 2.0, -h), (0.0, -h)], -w / 2.0, h / 2.0);
        }
        NodeShape::LeanLeft => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(0.0, 0.0), (w + h / 2.0, 0.0), (w, -h), (-h / 2.0, -h)], -w / 2.0, h / 2.0);
        }
        NodeShape::Trapezoid => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(-h / 2.0, 0.0), (w + h / 2.0, 0.0), (w, -h), (0.0, -h)], -w / 2.0, h / 2.0);
        }
        NodeShape::InvTrapezoid => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(0.0, 0.0), (w, 0.0), (w + h / 2.0, -h), (-h / 2.0, -h)], -w / 2.0, h / 2.0);
        }
        NodeShape::Cylinder => {
            // mermaid's `datastore` is a rect whose stroke-dasharray "{w} {h}"
            // draws only the top and bottom edges, leaving the sides open.
            let (hw, hh) = (node.width / 2.0, node.height / 2.0);
            let _ = write!(
                s,
                r#"<rect class="basic label-container" x="{}" y="{}" width="{}" height="{}" stroke-dasharray="{} {}"/>"#,
                round(-hw), round(-hh), round(node.width), round(node.height),
                round(node.width), round(node.height),
            );
        }
    }
}

/// Emit a `<polygon class="label-container">` from `points` with a translate.
fn emit_polygon(s: &mut String, points: &[(f64, f64)], tx: f64, ty: f64) {
    s.push_str(r#"<polygon class="label-container" points=""#);
    for (i, (x, y)) in points.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{},{}", round(*x), round(*y));
    }
    // No space after the comma: matches mermaid's insertPolygonShape output.
    let _ = write!(s, r#"" transform="translate({},{})"/>"#, round(tx), round(ty));
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
            // Labelled: g.edgeLabel positioned at the dagre-computed label
            // centre > g.label[data-id] (offset up half a line) > g >
            //           (sized rect.background + text).
            let (lx, ly) = edge.label_pos.unwrap_or((0.0, 0.0));
            let lines = crate::text::wrap_label(label, crate::text::WRAP_WIDTH, 16.0);
            let bg_w = lines
                .iter()
                .map(|l| crate::text::line_width(l, 16.0))
                .fold(0.0_f64, f64::max)
                + 4.0;
            let bg_h = 23.0 + (lines.len().max(1) as f64 - 1.0) * LINE_SPACING;
            let _ = write!(
                s,
                r#"<g class="edgeLabel" transform="translate({}, {})"><g class="label" data-id="{eid}" transform="translate(0, {})"><g>"#,
                round(lx),
                round(ly),
                round(-bg_h / 2.0 + 1.0), // -10.5 for a single 23px line
            );
            let _ = write!(
                s,
                r#"<rect class="background" x="{}" y="-1" width="{}" height="{}"/>"#,
                round(-bg_w / 2.0),
                round(bg_w),
                round(bg_h),
            );
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

/// Emit mermaid's nested label structure: a `<text>` containing one
/// `<tspan.text-outer-tspan.row>` per wrapped line, each holding one
/// `<tspan.text-inner-tspan>` per word (the first word of a row has no leading
/// space, the rest are ` word`). With `label = None` a single empty row is
/// emitted (mermaid's shape for an unlabelled edge). `anchor` adds
/// `text-anchor="middle"` (edge labels) on the `<text>` and each row.
fn render_text(s: &mut String, label: Option<&str>, anchor: bool) {
    let ta = if anchor { r#" text-anchor="middle""# } else { "" };
    let _ = write!(s, r#"<text y="{y}"{ta}>"#, y = round(-LABEL_HEIGHT / 2.0 - 0.6), ta = ta);

    let lines = label
        .map(|l| crate::text::wrap_label(l, crate::text::WRAP_WIDTH, 16.0))
        .unwrap_or_default();

    if lines.is_empty() {
        // Empty label: a single self-closed outer row (matches mermaid).
        let _ = write!(
            s,
            r#"<tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em"{ta}></tspan>"#,
        );
    } else {
        for (i, words) in lines.iter().enumerate() {
            let y = num(-0.1 + i as f64 * 1.1);
            let _ = write!(
                s,
                r#"<tspan class="text-outer-tspan row" x="0" y="{y}em" dy="1.1em"{ta}>"#,
            );
            for (j, word) in words.iter().enumerate() {
                let text = if j == 0 { word.clone() } else { format!(" {word}") };
                let _ = write!(
                    s,
                    r#"<tspan font-style="normal" class="text-inner-tspan" font-weight="normal">{}</tspan>"#,
                    escape(&text),
                );
            }
            s.push_str("</tspan>");
        }
    }
    s.push_str("</text>");
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
