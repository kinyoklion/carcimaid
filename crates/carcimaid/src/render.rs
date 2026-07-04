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

use crate::ir::{ArrowType, NodeShape};
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
    // Content bounding box (origin + size) computed by layout, including edge
    // overflow beyond the node band.
    let (ox, oy) = (chart.origin_x, chart.origin_y);
    let center_x = ox + chart.width / 2.0;
    // A visible title reserves TITLE_SPACE above the diagram (viewBox top shifts
    // up, height grows) and, when it's wider than the content, widens and
    // re-centres the viewBox — mermaid's viewBox is the getBBox of content plus
    // the centred title text (font-size 18) ± an 8px margin.
    let title_space = if chart.title.is_some() { TITLE_SPACE } else { 0.0 };
    let vh = round(chart.height + title_space);
    let vy = round(oy - title_space);
    let (vx, vw) = match &chart.title {
        Some(t) => {
            let tw = crate::text::measure_width(t, TITLE_FONT_SIZE);
            let left = ox.min(center_x - tw / 2.0 - TITLE_MARGIN);
            let right = (ox + chart.width).max(center_x + tw / 2.0 + TITLE_MARGIN);
            (round(left), round(right - left))
        }
        None => (round(ox), round(chart.width)),
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

    // 2. wrapper <g> with markers + the (possibly nested) g.root tree.
    //    Extracted subgraphs render inside a translated g.root, so shift each
    //    element's absolute coordinates into its scope's local frame; the group
    //    transform re-applies the offset (matching mermaid's DOM exactly).
    let off = |h: Option<usize>| h.map(|s| chart.scope_offsets[s]).unwrap_or((0.0, 0.0));
    let rel_nodes: Vec<PlacedNode> = chart
        .nodes
        .iter()
        .map(|n| {
            let (dx, dy) = off(n.home);
            PlacedNode { cx: n.cx - dx, cy: n.cy - dy, ..n.clone() }
        })
        .collect();
    let rel_edges: Vec<PlacedEdge> = chart
        .edges
        .iter()
        .map(|e| {
            let (dx, dy) = off(e.home);
            PlacedEdge {
                points: e.points.iter().map(|&(x, y)| (x - dx, y - dy)).collect(),
                label_pos: e.label_pos.map(|(x, y)| (x - dx, y - dy)),
                ..e.clone()
            }
        })
        .collect();
    let mut rel_clusters: Vec<PlacedCluster> = chart
        .clusters
        .iter()
        .map(|c| {
            let (dx, dy) = off(if c.extracted { Some(c.sg_index) } else { c.home });
            PlacedCluster { cx: c.cx - dx, cy: c.cy - dy, ..c.clone() }
        })
        .collect();
    // Emit clusters (and nested subgraph groups) in mermaid's render order.
    rel_clusters.sort_by_key(|c| c.order);

    s.push_str("<g>");
    s.push_str(&markers::block(ID));
    render_scope(&mut s, &rel_nodes, &rel_edges, &rel_clusters, &chart.scope_offsets, None, (0.0, 0.0), chart.look.roughness());
    // Colour-matched point-arrow markers for stroke-coloured edges (unique
    // (side, colour), first-seen order) — mermaid appends these after g.root.
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for e in &chart.edges {
        let Some(c) = e.stroke.as_deref() else { continue };
        for (side, kind) in [("Start", e.arrow_start), ("End", e.arrow_end)] {
            if kind == crate::ir::ArrowType::Point && !seen.contains(&(side, c)) {
                seen.push((side, c));
                s.push_str(&markers::colored_point(ID, side, c));
            }
        }
    }
    s.push_str("</g>"); // close wrapper g

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
            round(center_x),
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

/// Emit one layout scope's `g.root` (clusters, edgePaths, edgeLabels, nodes).
/// `owner` is `None` for the diagram root, or a subgraph index for a separately
/// laid-out (extracted) subgraph. Extracted child subgraphs are emitted as
/// nested `g.root` groups inside this scope's `nodes` group, mirroring mermaid.
#[allow(clippy::too_many_arguments)]
fn render_scope(
    s: &mut String,
    nodes: &[PlacedNode],
    edges: &[PlacedEdge],
    clusters: &[PlacedCluster],
    scope_offsets: &[(f64, f64)],
    owner: Option<usize>,
    parent_off: (f64, f64),
    roughness: f64,
) {
    // A nested (extracted-subgraph) g.root carries the offset that positions its
    // (otherwise local) contents; the root g.root has no transform.
    match owner {
        Some(sg) => {
            let (ax, ay) = scope_offsets[sg];
            let _ = write!(s, r#"<g class="root" transform="translate({}, {})">"#, round(ax - parent_off.0), round(ay - parent_off.1));
        }
        None => s.push_str(r#"<g class="root">"#),
    }
    let my_off = owner.map(|sg| scope_offsets[sg]).unwrap_or((0.0, 0.0));

    // mermaid emits subgraphs (cluster rects and, below, nested groups) in
    // reverse definition order; leaf nodes stay in definition order.
    s.push_str(r#"<g class="clusters">"#);
    for cluster in clusters {
        // An extracted subgraph draws its own rect in its own scope; an inline
        // cluster draws in the scope it belongs to.
        let here = if cluster.extracted {
            Some(cluster.sg_index) == owner
        } else {
            cluster.home == owner
        };
        if here {
            render_cluster(s, cluster);
        }
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="edgePaths">"#);
    for edge in edges.iter().filter(|e| e.home == owner) {
        render_edge_path(s, edge, nodes);
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="edgeLabels">"#);
    for edge in edges.iter().filter(|e| e.home == owner) {
        render_edge_label(s, edge, nodes);
    }
    s.push_str("</g>");

    s.push_str(r#"<g class="nodes">"#);
    for node in nodes.iter().filter(|n| n.home == owner) {
        render_node(s, node, roughness);
    }
    // Nested extracted subgraphs belonging to this scope (clusters are pre-sorted
    // into mermaid's render order).
    for cluster in clusters.iter().filter(|c| c.extracted && c.home == owner) {
        render_scope(s, nodes, edges, clusters, scope_offsets, Some(cluster.sg_index), my_off, roughness);
    }
    s.push_str("</g>");

    s.push_str("</g>"); // close g.root
}

fn render_cluster(s: &mut String, cluster: &PlacedCluster) {
    let x = cluster.cx - cluster.width / 2.0;
    let y = cluster.cy - cluster.height / 2.0;
    let _ = write!(
        s,
        r#"<g class="cluster{}" id="{}-{}" data-look="classic"><rect{} x="{}" y="{}" width="{}" height="{}"/>"#,
        class_suffix(&cluster.classes),
        ID,
        escape(&cluster.id),
        style_attr(&cluster.shape_style),
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
    s.push_str(r#"<rect class="background" style="stroke: none"/>"#);
    render_text(s, Some(&cluster.title), false, "");
    s.push_str("</g></g></g>");
}

fn render_node(s: &mut String, node: &PlacedNode, roughness: f64) {
    let _ = write!(
        s,
        r#"<g class="node default{}" id="{}-flowchart-{}-0" data-look="classic" transform="translate({}, {})">"#,
        class_suffix(&node.classes),
        ID,
        escape(&node.id),
        round(node.cx),
        round(node.cy),
    );
    render_shape(s, node, roughness);
    // g.label is offset up by half the label block height so the (possibly
    // multi-line) text is vertically centred, matching mermaid.
    let n = crate::text::wrap_label(&node.label, crate::text::WRAP_WIDTH, 16.0).len().max(1);
    let block_h = LABEL_HEIGHT + (n as f64 - 1.0) * LINE_SPACING;
    // Label styles (color/font) sit on the g.label; the <text> gets them with
    // `color:` rewritten to `fill:` (see render_text), matching mermaid.
    let _ = write!(
        s,
        r#"<g class="label"{} transform="translate(0, {})"><rect/><g>"#,
        style_attr(&node.label_style),
        round(-block_h / 2.0),
    );
    s.push_str(r#"<rect class="background" style="stroke: none"/>"#);
    render_text(s, Some(&node.label), false, &node.label_style);
    s.push_str("</g></g></g>");
}

/// The ` style="…"` attribute for an inline style, or empty when unstyled.
fn style_attr(style: &str) -> String {
    if style.is_empty() {
        String::new()
    } else {
        format!(r#" style="{}""#, escape(style))
    }
}

/// A trailing class-list suffix (` foo bar`) for the group's `class` attribute,
/// or empty. mermaid appends applied class names after the base classes.
fn class_suffix(classes: &[String]) -> String {
    if classes.is_empty() {
        String::new()
    } else {
        format!(" {}", classes.join(" "))
    }
}

/// Emit the node's outline shape, centred at the group origin.
fn render_shape(s: &mut String, node: &PlacedNode, roughness: f64) {
    let (hw, hh) = (node.width / 2.0, node.height / 2.0);
    let st = style_attr(&node.shape_style);
    match node.shape {
        NodeShape::Rectangle | NodeShape::DataStore => {
            // DataStore (`@{shape: datastore}`) is the same rect but with its
            // vertical sides dashed away: a `stroke-dasharray` of the rect's own
            // width/height draws only the top and bottom edges.
            let dash = if matches!(node.shape, NodeShape::DataStore) {
                format!(r#" stroke-dasharray="{} {}""#, round(node.width), round(node.height))
            } else {
                String::new()
            };
            let _ = write!(
                s,
                r#"<rect class="basic label-container"{st} x="{}" y="{}" width="{}" height="{}"{dash}/>"#,
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
                r#"<rect class="basic label-container"{st} x="{}" y="{}" rx="{}" ry="{}" width="{}" height="{}"/>"#,
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
                r#"<circle class="basic label-container"{st} cx="0" cy="0" r="{}"/>"#,
                round(node.width / 2.0),
            );
        }
        NodeShape::Rhombus => {
            // mermaid `question`: a square diamond of side `side`, points laid
            // out in [0,side]×[-side,0] then translated to centre it.
            let side = node.width;
            let _ = write!(
                s,
                r#"<polygon class="label-container"{st} points="{},0 {},{} {},{} 0,{}" transform="translate({}, {})"/>"#,
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
                &st,
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
                &st,
            );
        }
        // Slanted shapes: recover the inner width (dagre width minus the h/2
        // overflow on each side), and lay points out around it.
        NodeShape::Parallelogram => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(-h / 2.0, 0.0), (w, 0.0), (w + h / 2.0, -h), (0.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::LeanLeft => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(0.0, 0.0), (w + h / 2.0, 0.0), (w, -h), (-h / 2.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::Trapezoid => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(-h / 2.0, 0.0), (w + h / 2.0, 0.0), (w, -h), (0.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::InvTrapezoid => {
            let (w, h) = (node.width - node.height, node.height);
            emit_polygon(s, &[(0.0, 0.0), (w, 0.0), (w + h / 2.0, -h), (-h / 2.0, -h)], -w / 2.0, h / 2.0, &st);
        }
        NodeShape::Cylinder => {
            // mermaid's `[(db)]` is a 3D cylinder path: full top ellipse (two
            // arcs), body sides, and the bottom front ellipse arc. `ry` is the
            // cap radius; the body height is the total minus the two caps.
            let w = node.width;
            let ry = crate::layout::cylinder_ry(w);
            let rx = w / 2.0;
            let body = node.height - 2.0 * ry;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} d="M0,{ry} a{rx},{ry} 0,0,0 {w},0 a{rx},{ry} 0,0,0 {nw},0 "#,
                    r#"l0,{body} a{rx},{ry} 0,0,0 {w},0 l0,{nbody}" "#,
                    r#"class="basic label-container outer-path" label-offset-y="{ry}" "#,
                    r#"transform="translate({tx}, {ty})"/>"#,
                ),
                st = st,
                ry = round(ry),
                rx = round(rx),
                w = round(w),
                nw = round(-w),
                body = round(body),
                nbody = round(-body),
                tx = round(-w / 2.0),
                ty = round(-node.height / 2.0),
            );
        }
        // Small start circle: fixed r=7, no label. mermaid emits width/height too.
        NodeShape::SmallCircle => {
            let _ = write!(
                s,
                r#"<circle class="state-start"{st} r="7" width="14" height="14"/>"#,
            );
        }
        // Double circle: a `<g>` holding an outer and an inner circle (gap 5).
        NodeShape::DoubleCircle => {
            let outer = node.width / 2.0;
            let _ = write!(
                s,
                concat!(
                    r#"<g class="basic label-container"{st}>"#,
                    r#"<circle class="outer-circle"{st} r="{outer}" cx="0" cy="0"/>"#,
                    r#"<circle class="inner-circle"{st} r="{inner}" cx="0" cy="0"/></g>"#,
                ),
                st = st,
                outer = round(outer),
                inner = round(outer - 5.0),
            );
        }
        // Divided rectangle: mermaid renders it via rough.js `rc.polygon` — a
        // fill path plus a sketch stroke path, both emitted through `roughr`
        // (matching mermaid's dividedRectangle handler). The polygon traces the
        // outline and doubles back along the divider line near the top.
        NodeShape::DividedRect => {
            let w = node.width;
            let h_inner = node.height / 1.2;
            let off = h_inner * 0.2;
            let x = -w / 2.0;
            let y = -h_inner / 2.0 - off / 2.0;
            let pts = [
                [x, y + off],
                [-x, y + off],
                [-x, -y],
                [x, -y],
                [x, y],
                [-x, y],
                [-x, y + off],
            ];
            let o = rough_options(roughness);
            let drawable = roughr::Generator::new().polygon(&pts, &o);
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, true, &st);
            s.push_str("</g>");
        }
        // Lined/shaded process: rough.js fill polygon (rect + a left bar).
        NodeShape::LinedProcess => {
            let frame = 8.0;
            let total_w = node.width;
            let h = node.height;
            let w = total_w - frame;
            let x = frame - total_w / 2.0;
            let y = -h / 2.0;
            let pts = [
                (x, y),
                (x + w, y),
                (x + w, y + h),
                (x - frame, y + h),
                (x - frame, y),
                (x, y),
                (x, y + h),
            ];
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_fill(s, &pts, true, "", &st);
            s.push_str("</g>");
        }
        // Window pane: mermaid renders it via rough.js `rc.path` with a single
        // `d` holding three subpaths — the outer rectangle plus the horizontal
        // and vertical divider lines (the quadrant cross). `roughr` produces the
        // fill (outer rectangle) and the stroke (rectangle + cross). Wrapped in a
        // `<g translate(5,5)>` (rectOffset/2), matching the windowPane handler.
        NodeShape::WindowPane => {
            let ro = 10.0;
            let w = node.width - ro;
            let h = node.height - ro;
            let x = -w / 2.0;
            let y = -h / 2.0;
            let path_d = format!(
                "M{},{} L{},{} L{},{} L{},{} L{},{} M{},{} L{},{} M{},{} L{},{}",
                x - ro, y - ro, x + w, y - ro, x + w, y + h, x - ro, y + h, x - ro, y - ro,
                x - ro, y, x + w, y,
                x, y - ro, x, y + h,
            );
            let o = rough_options(roughness);
            let drawable = roughr::Generator::new().path(&path_d, &o);
            let _ = write!(s, r#"<g transform="translate(5, 5)" class="basic label-container outer-path">"#);
            emit_rough_drawable(s, &drawable, false, &st);
            s.push_str("</g>");
        }
        // Stacked rectangle: rough.js outer+inner paths; we emit the outer fill.
        NodeShape::StackedRect => {
            let ro = 5.0;
            let w = node.width - 2.0 * ro;
            let h = node.height - 2.0 * ro;
            let x = -w / 2.0;
            let y = -h / 2.0;
            let pts = [
                (x - ro, y + ro),
                (x - ro, y + h + ro),
                (x + w - ro, y + h + ro),
                (x + w - ro, y + h),
                (x + w, y + h),
                (x + w, y + h - ro),
                (x + w + ro, y + h - ro),
                (x + w + ro, y - ro),
                (x + ro, y - ro),
                (x + ro, y),
                (x, y),
                (x, y + ro),
            ];
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_fill(s, &pts, true, "Z", &st);
            s.push_str("</g>");
        }
        // Notched rectangle (card): an exact `<polygon>` (insertPolygonShape), a
        // rect with a 12px notch cut from the top-left corner.
        NodeShape::NotchedRect => {
            let (w, h) = (node.width, node.height);
            let n = 12.0;
            let pts = [(n, -h), (w, -h), (w, 0.0), (0.0, 0.0), (0.0, -h + n), (n, -h)];
            emit_polygon(s, &pts, -w / 2.0, h / 2.0, &st);
        }
        // Notched pentagon (loop limit): mermaid uses a rough curved path; we emit
        // the straight fill (element + size match; the seeded stroke curve is not
        // reproducible, leaving a `d` residual).
        NodeShape::NotchedPentagon => {
            let (w, h) = (node.width / 2.0, node.height / 2.0);
            let pts = [
                (-w * 0.8, -h),
                (w * 0.8, -h),
                (w, -h * 0.6),
                (w, h),
                (-w, h),
                (-w, -h * 0.6),
            ];
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_fill(s, &pts, false, "Z", &st);
            s.push_str("</g>");
        }
        // Triangle / flipped triangle (rough curved path; straight approximation).
        // The base `tw` equals the height; the group is translated to centre it.
        NodeShape::Triangle | NodeShape::FlippedTriangle => {
            let h = node.height;
            let pts = if matches!(node.shape, NodeShape::Triangle) {
                [(0.0, 0.0), (h, 0.0), (h / 2.0, -h)]
            } else {
                [(0.0, -h), (h, -h), (h / 2.0, 0.0)]
            };
            let _ = write!(
                s,
                r#"<g class="outer-path" transform="translate({}, {})">"#,
                round(-h / 2.0),
                round(h / 2.0),
            );
            emit_rough_fill(s, &pts, false, "Z", &st);
            s.push_str("</g>");
        }
        // Sloped rectangle (rough curved path; straight approximation). The drawn
        // height is 1.5·h (node.height); the shape body uses h = node.height/1.5.
        NodeShape::SlopedRect => {
            let w = node.width;
            let h = node.height / 1.5;
            let (x, y) = (-w / 2.0, -h / 2.0);
            let pts = [(x, y), (x, y + h), (x + w, y + h), (x + w, y - h / 2.0)];
            let _ = write!(
                s,
                r#"<g class="basic label-container  outer-path" transform="translate(0, {})">"#,
                round(h / 4.0),
            );
            emit_rough_fill(s, &pts, false, "Z", &st);
            s.push_str("</g>");
        }
        // Curved trapezoid (display): rough curved path with an arced left edge;
        // approximated as a straight-edged trapezoid (element + size match).
        NodeShape::CurvedTrapezoid => {
            let (w, h) = (node.width, node.height);
            let radius = h / 2.0;
            let rw = w - radius;
            let tw = h / 4.0;
            // Points in mermaid's [0,w]×[0,h] frame, then centred via translate.
            let pts = [
                (rw, 0.0),
                (tw, 0.0),
                (0.0, h / 2.0),
                (tw, h),
                (rw, h),
            ];
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate({}, {})">"#,
                round(-w / 2.0),
                round(-h / 2.0),
            );
            emit_rough_fill(s, &pts, false, "Z", &st);
            s.push_str("</g>");
        }
        // Filled junction circle (r=7, no label). mermaid draws a rough bezier
        // path; we emit a plain circle (visually identical, correct size).
        NodeShape::FilledCircle => {
            let _ = write!(
                s,
                r#"<path class="outer-path"{st} d="M-7,0 a7,7 0 1 0 14,0 a7,7 0 1 0 -14,0 Z"/>"#,
            );
        }
        // Framed stop circle: outer (r=7, as a path) + filled inner (r=2.5).
        NodeShape::FramedCircle => {
            let _ = write!(
                s,
                concat!(
                    r#"<g class="basic label-container"{st}>"#,
                    r#"<path class="outer-circle"{st} d="M-7,0 a7,7 0 1 0 14,0 a7,7 0 1 0 -14,0 Z"/>"#,
                    r#"<circle class="inner-circle"{st} r="2.5" cx="0" cy="0"/></g>"#,
                ),
                st = st,
            );
        }
        // Crossed circle (r=30, no label): a circle (as a path) with an X across it.
        NodeShape::CrossedCircle => {
            let r = 30.0_f64;
            let d = r * (0.5_f64).sqrt(); // 45° offset for the X arms
            let _ = write!(
                s,
                concat!(
                    r#"<g class="outer-path"{st}>"#,
                    r#"<path{st} d="M{nr},0 a{r},{r} 0 1 0 {dd},0 a{r},{r} 0 1 0 {ndd},0 Z"/>"#,
                    r#"<path{st} d="M{a},{na} L{na},{a} M{na},{na} L{a},{a}"/></g>"#,
                ),
                st = st,
                r = round(r), nr = round(-r), dd = round(2.0 * r), ndd = round(-2.0 * r),
                a = round(d),
                na = round(-d),
            );
        }
        // Odd / flag (rect_left_inv_arrow): rectangle with a notched left edge.
        NodeShape::Odd => {
            let notch = -hh / 2.0; // y/2 in mermaid's frame (y = -h/2)
            let pts = [
                (-hw + notch, -hh),
                (-hw, 0.0),
                (-hw + notch, hh),
                (hw, hh),
                (hw, -hh),
            ];
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_fill(s, &pts, false, "Z", &st);
            s.push_str("</g>");
        }
        // Delay: rectangle with a rounded right end (radius = h/2).
        NodeShape::Delay => {
            let r = hh;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},{ny} "#,
                    r#"L{rx},{ny} A{r},{r} 0 0 1 {rx},{hh} L{nx},{hh} Z"/>"#,
                ),
                st = st, nx = round(-hw), ny = round(-hh), rx = round(hw - r), r = round(r), hh = round(hh),
            );
        }
        // Document: mermaid renders it via rough.js `rc.path` — the outline is a
        // polyline whose bottom edge is a sampled sine wave (`waveEdgedRectangle`
        // handler). `roughr` reproduces the fill and stroke; the group carries a
        // `translate(0, -waveAmplitude/2)`.
        NodeShape::Document => {
            let h = node.height / 1.25;
            let w = node.width;
            let wave_amp = h / 8.0;
            let final_h = h + wave_amp;
            let mut pts: Vec<[f64; 2]> = Vec::with_capacity(54);
            pts.push([-w / 2.0, final_h / 2.0]);
            pts.extend(full_sine_wave_points(
                -w / 2.0,
                final_h / 2.0,
                w / 2.0,
                final_h / 2.0,
                wave_amp,
                0.8,
            ));
            pts.push([w / 2.0, -final_h / 2.0]);
            pts.push([-w / 2.0, -final_h / 2.0]);
            let path_d = path_from_points(&pts);
            let o = rough_options(roughness);
            let drawable = roughr::Generator::new().path(&path_d, &o);
            let _ = write!(
                s,
                r#"<g class="basic label-container outer-path" transform="translate(0,{})">"#,
                round(-wave_amp / 2.0),
            );
            emit_rough_drawable(s, &drawable, false, &st);
            s.push_str("</g>");
        }
        // Lined-document / tagged-document: rectangle with a wavy bottom edge
        // (approximated with two cubic bows) plus a decoration. Not yet routed
        // through roughr.
        NodeShape::LinedDocument | NodeShape::TaggedDocument => {
            let wave = node.height * 0.1;
            let bot = hh - wave;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},{ny} L{x},{ny} "#,
                    r#"L{x},{bot} C{cx1},{c1},{cx2},{c2},0,{bot} C{cx3},{c3},{cx4},{c4},{nx},{bot} Z"/>"#,
                ),
                st = st, nx = round(-hw), ny = round(-hh), x = round(hw), bot = round(bot),
                cx1 = round(hw * 0.66), c1 = round(bot + wave * 2.0),
                cx2 = round(hw * 0.33), c2 = round(bot - wave * 2.0),
                cx3 = round(-hw * 0.33), c3 = round(bot + wave * 2.0),
                cx4 = round(-hw * 0.66), c4 = round(bot - wave * 2.0),
            );
            if matches!(node.shape, NodeShape::LinedDocument) {
                let lx = -hw + 8.0;
                let _ = write!(s, r#"<path{st} d="M{lx},{ny} L{lx},{bot}"/>"#, st = st, lx = round(lx), ny = round(-hh), bot = round(bot));
            }
            if matches!(node.shape, NodeShape::TaggedDocument) {
                let t = 12.0;
                let _ = write!(s, r#"<path{st} d="M{x0},{y0} L{x1},{y0} L{x1},{y1}"/>"#, st = st, x0 = round(hw - t), y0 = round(hh - wave - t), x1 = round(hw), y1 = round(hh - wave));
            }
        }
        // Stacked documents: two offset outlines behind a front document.
        NodeShape::Documents => {
            let off = node.height * 0.12;
            let fw = hw - off;
            let fh = hh - off;
            let _ = write!(
                s,
                concat!(
                    r#"<g class="basic label-container outer-path"{st}>"#,
                    r#"<path{st} d="M{x2},{ny} L{xr},{ny} L{xr},{yb} L{x2},{yb} Z"/>"#,
                    r#"<path{st} d="M{x1},{y1} L{xr1},{y1} L{xr1},{yb1} L{x1},{yb1} Z"/>"#,
                    r#"<path{st} d="M{fnx},{fy} L{fx},{fy} L{fx},{fbot} C{c1x},{c1},{c2x},{c2},{fnx},{fbot} Z"/></g>"#,
                ),
                st = st,
                x2 = round(-hw), ny = round(-hh), xr = round(-hw + 2.0 * fw), yb = round(-hh + 2.0 * fh),
                x1 = round(-hw + off), y1 = round(-hh + off), xr1 = round(-hw + off + 2.0 * fw), yb1 = round(-hh + off + 2.0 * fh),
                fnx = round(-fw), fy = round(-fh), fx = round(fw), fbot = round(fh - off),
                c1x = round(fw * 0.4), c1 = round(fh + off), c2x = round(-fw * 0.4), c2 = round(fh - off),
            );
        }
        // Tagged rectangle: a rectangle with a folded bottom-right corner tag.
        NodeShape::TaggedRect => {
            let t = node.height * 0.2;
            let _ = write!(
                s,
                concat!(
                    r#"<g class="basic label-container outer-path"{st}>"#,
                    r#"<path{st} d="M{nx},{ny} L{hw},{ny} L{hw},{hh} L{nx},{hh} Z"/>"#,
                    r#"<path{st} d="M{x0},{hh} L{hw},{y0} L{hw},{hh} Z"/></g>"#,
                ),
                st = st, nx = round(-hw), ny = round(-hh),
                x0 = round(hw - t), hh = round(hh), hw = round(hw), y0 = round(hh - t),
            );
        }
        // Bow-tie rectangle: rectangle with concave (inward-arced) left/right sides.
        NodeShape::BowTieRect => {
            let sag = 5.0;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},{ny} L{hw},{ny} "#,
                    r#"Q{qx},0 {hw},{hh} L{nx},{hh} Q{nqx},0 {nx},{ny} Z"/>"#,
                ),
                st = st, nx = round(-hw), ny = round(-hh), hw = round(hw), hh = round(hh),
                qx = round(hw - sag), nqx = round(-hw + sag),
            );
        }
        // Wave rectangle (flag / paper tape): wavy top and bottom edges.
        NodeShape::WaveRect => {
            let wave = node.height * 0.12;
            let top = -hh + wave;
            let bot = hh - wave;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},{top} "#,
                    r#"C{cx1},{tc1},{cx2},{tc2},{hw},{top} L{hw},{bot} "#,
                    r#"C{cx2},{bc1},{cx1},{bc2},{nx},{bot} Z"/>"#,
                ),
                st = st, nx = round(-hw), hw = round(hw), top = round(top), bot = round(bot),
                cx1 = round(-hw * 0.33), cx2 = round(hw * 0.33),
                tc1 = round(top - wave * 2.0), tc2 = round(top + wave * 2.0),
                bc1 = round(bot + wave * 2.0), bc2 = round(bot - wave * 2.0),
            );
        }
        // Horizontal cylinder: a cylinder lying on its side (elliptical ends).
        NodeShape::HorizontalCylinder => {
            let rx = hh / 2.5;
            let body = node.width - 2.0 * rx;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},{nhh} "#,
                    r#"a{rx},{hh} 0 0 0 0,{hgt} l{body},0 a{rx},{hh} 0 0 0 0,{nhgt} "#,
                    r#"l{nbody},0 M{x2},{nhh} a{rx},{hh} 0 0 1 0,{hgt}"/>"#,
                ),
                st = st, nx = round(-hw), nhh = round(-hh), rx = round(rx), hh = round(hh),
                hgt = round(2.0 * hh), nhgt = round(-2.0 * hh), body = round(body), nbody = round(-body),
                x2 = round(-hw + body),
            );
        }
        // Lined (disk) cylinder: a vertical cylinder with an inner top-cap line.
        NodeShape::LinedCylinder => {
            let w = node.width;
            let ry = crate::layout::cylinder_ry(w);
            let rx = w / 2.0;
            let body = node.height - 2.0 * ry;
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} d="M0,{ry} a{rx},{ry} 0,0,0 {w},0 a{rx},{ry} 0,0,0 {nw},0 "#,
                    r#"l0,{body} a{rx},{ry} 0,0,0 {w},0 l0,{nbody}" "#,
                    r#"class="basic label-container outer-path" transform="translate({tx}, {ty})"/>"#,
                    r#"<path{st} d="M{tx},{cap} a{rx},{ry} 0,0,0 {w},0" transform="translate(0, {ty})"/>"#,
                ),
                st = st, ry = round(ry), rx = round(rx), w = round(w), nw = round(-w),
                body = round(body), nbody = round(-body),
                tx = round(-w / 2.0), ty = round(-node.height / 2.0), cap = round(2.0 * ry),
            );
        }
        // Fork/join: a thin solid bar (no label), drawn as a path.
        NodeShape::Fork => {
            let pts = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
            let _ = write!(s, r#"<g class="basic label-container outer-path">"#);
            emit_rough_fill(s, &pts, false, "Z", &st);
            s.push_str("</g>");
        }
        // Text block: a borderless rectangle (label only).
        NodeShape::TextBlock => {
            let _ = write!(
                s,
                r#"<rect class="label-container"{st} x="{nx}" y="{ny}" width="{w}" height="{h}"/>"#,
                st = st, nx = round(-hw), ny = round(-hh), w = round(2.0 * hw), h = round(2.0 * hh),
            );
        }
        // Hourglass (collate): two triangles meeting at the centre.
        NodeShape::Hourglass => {
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},{ny} L{hw},{ny} "#,
                    r#"L{nx},{hh} L{hw},{hh} Z"/>"#,
                ),
                st = st, nx = round(-hw), ny = round(-hh), hw = round(hw), hh = round(hh),
            );
        }
        // Lightning bolt: a zig-zag polygon.
        NodeShape::LightningBolt => {
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="outer-path" d="M{x0},{ny} L{x1},{y1} L{x2},{y1} "#,
                    r#"L{x0b},{hh} L{x3},{y3} L{x4},{y3} Z"/>"#,
                ),
                st = st,
                x0 = round(hw * 0.2), ny = round(-hh),
                x1 = round(-hw * 0.6), y1 = round(node.height * 0.1),
                x2 = round(0.0), x0b = round(-hw * 0.2), hh = round(hh),
                x3 = round(hw * 0.6), y3 = round(-node.height * 0.1), x4 = round(0.0),
            );
        }
        // Bang / cloud: approximated as an ellipse blob (element + size match).
        NodeShape::Bang | NodeShape::Cloud => {
            let _ = write!(
                s,
                concat!(
                    r#"<path{st} class="basic label-container outer-path" d="M{nx},0 "#,
                    r#"a{hw},{hh} 0 1 0 {w},0 a{hw},{hh} 0 1 0 {nw},0 Z"/>"#,
                ),
                st = st, nx = round(-hw), hw = round(hw), hh = round(hh), w = round(2.0 * hw), nw = round(-2.0 * hw),
            );
        }
        // Curly braces: a left brace, a right brace, or both around the label.
        NodeShape::BraceLeft | NodeShape::BraceRight | NodeShape::Braces => {
            let brace = |s: &mut String, x: f64, dir: f64| {
                let _ = write!(
                    s,
                    concat!(
                        r#"<path{st} d="M{x0},{ny} q{qb},0 {qb},{q} q0,{q} {qb2},{q} q{nqb2},0 {nqb2},{q} q0,{q} {nqb},{q}"/>"#,
                    ),
                    st = st, x0 = round(x), ny = round(-hh),
                    qb = round(-6.0 * dir), q = round(hh / 2.0), qb2 = round(-6.0 * dir), nqb2 = round(6.0 * dir), nqb = round(6.0 * dir),
                );
            };
            let _ = write!(s, r#"<g class="outer-path">"#);
            if !matches!(node.shape, NodeShape::BraceRight) {
                brace(s, -hw + 6.0, 1.0);
            }
            if !matches!(node.shape, NodeShape::BraceLeft) {
                brace(s, hw - 6.0, -1.0);
            }
            s.push_str("</g>");
        }
    }
}

/// Emit a rough.js "solid" fill `<path>` for a polygon: `M x y L x y …`
/// (space-separated). `wrapper_g` callers add the `outer-path` `<g>` themselves;
/// otherwise the path carries the class directly (see per-shape arms). `evenodd`
/// adds `fill-rule="evenodd"` (rc.polygon does; rc.path does not). `close`
/// appends a closing token (e.g. "Z"). mermaid's fill path also carries
/// `stroke="none" stroke-width="0" fill="#ECECFF"`.
fn emit_rough_fill(s: &mut String, pts: &[(f64, f64)], evenodd: bool, close: &str, st: &str) {
    let mut d = String::new();
    for (i, &(x, y)) in pts.iter().enumerate() {
        let _ = write!(d, "{}{} {}", if i == 0 { "M" } else { " L" }, round(x), round(y));
    }
    if !close.is_empty() {
        d.push(' ');
        d.push_str(close);
    }
    let rule = if evenodd { r#" fill-rule="evenodd""# } else { "" };
    let _ = write!(
        s,
        r##"<path d="{d}" stroke="none" stroke-width="0" fill="#ECECFF"{rule}{st}/>"##,
    );
}

/// The classic-look `roughr` options for a node shape: solid fill in the theme
/// node colour, theme border stroke, and the given `roughness` (0 for the
/// classic look — its fill path is then the exact shape vertices). Mirrors
/// mermaid's `userNodeOverrides(node, {})` + `roughness=0; fillStyle="solid"`.
fn rough_options(roughness: f64) -> roughr::Options {
    let gen = roughr::Generator::new();
    let mut o = gen.default_options();
    o.roughness = roughness;
    o.fill = Some("#ECECFF".to_string());
    o.fill_style = "solid".to_string();
    o.stroke = "#9370DB".to_string();
    o.bowing = 1.0;
    o.seed = 1;
    o
}

/// Emit a `roughr` [`Drawable`] as mermaid's rough shape DOM: a fill `<path>`
/// (theme fill, `stroke="none"`) followed by a stroke `<path>` (theme border,
/// `fill="none"`). `evenodd` adds `fill-rule="evenodd"` to the fill path
/// (rc.polygon sets it; rc.path does not). `st` is the node's inline `style`
/// attribute (empty when unstyled). This is the reusable primitive for scaling
/// roughr rendering to every shape: build the `Drawable`, wrap it in the shape's
/// `<g>`, and call this to emit the fill + stroke children.
///
/// [`Drawable`]: roughr::Drawable
fn emit_rough_drawable(s: &mut String, drawable: &roughr::Drawable, evenodd: bool, st: &str) {
    let fill = drawable.fill_path(None);
    let stroke = drawable.stroke_path(None);
    let rule = if evenodd { r#" fill-rule="evenodd""# } else { "" };
    let _ = write!(
        s,
        r##"<path d="{fill}" stroke="none" stroke-width="0" fill="#ECECFF"{rule}{st}/>"##,
    );
    let _ = write!(
        s,
        r##"<path d="{stroke}" stroke="#9370DB" stroke-width="1.3" fill="none" stroke-dasharray="0 0"{st}/>"##,
    );
}

/// Port of mermaid's `generateFullSineWavePoints`: sample `50` steps of a sine
/// wave from `(x1,y1)` to `(x2,y2)` with the given `amplitude` and cycle count.
fn full_sine_wave_points(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    amplitude: f64,
    num_cycles: f64,
) -> Vec<[f64; 2]> {
    let steps = 50;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let cycle_len = dx / num_cycles;
    let freq = 2.0 * std::f64::consts::PI / cycle_len;
    let mid_y = y1 + dy / 2.0;
    (0..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let x = x1 + t * dx;
            let y = mid_y + amplitude * (freq * (x - x1)).sin();
            [x, y]
        })
        .collect()
}

/// Port of mermaid's `createPathFromPoints`: an `M`/`L` polyline through `pts`,
/// closed with `Z`. Full precision (no rounding), matching mermaid's input to
/// rough.js.
fn path_from_points(pts: &[[f64; 2]]) -> String {
    let mut d = String::new();
    for (i, p) in pts.iter().enumerate() {
        let _ = write!(d, "{}{},{} ", if i == 0 { "M" } else { "L" }, p[0], p[1]);
    }
    d.push('Z');
    d
}

/// Emit a `<polygon class="label-container">` from `points` with a translate and
/// an optional inline style attribute.
fn emit_polygon(s: &mut String, points: &[(f64, f64)], tx: f64, ty: f64, style: &str) {
    let _ = write!(s, r#"<polygon class="label-container"{style} points=""#);
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
fn edge_id(edge: &PlacedEdge, _nodes: &[PlacedNode]) -> String {
    escape(&format!("L_{}_{}_0", edge.from_id, edge.to_id))
}

/// Arrow inset: mermaid shortens the path at the arrow end by this many px so
/// the arrowhead marker sits flush against the target node border.
const ARROW_INSET: f64 = 4.0;

fn render_edge_path(s: &mut String, edge: &PlacedEdge, nodes: &[PlacedNode]) {
    let mut points = edge.points.clone();
    // Clip the endpoints to the actual node shapes (dagre routes to the node's
    // bounding box; non-rect shapes are inset, so an unclipped edge detaches
    // from a diamond/circle/etc.). mermaid does the same via shape intersection.
    if points.len() >= 2 {
        // Cluster endpoints (a subgraph named as an edge end) are rectangular, so
        // dagre's route already terminates on the cluster border — only node
        // endpoints need shape-intersection clipping.
        if !edge.from_cluster {
            if let Some(p) = clip_to_shape(&nodes[edge.from], points[1]) {
                points[0] = p;
            }
        }
        let n = points.len();
        if !edge.to_cluster {
            if let Some(p) = clip_to_shape(&nodes[edge.to], points[n - 2]) {
                points[n - 1] = p;
            }
        }
    }
    // Arrowheads shorten the path at each end so the marker sits flush.
    if edge.arrow_end != ArrowType::None {
        clip_end(&mut points, ARROW_INSET);
    }
    if edge.arrow_start != ArrowType::None {
        points.reverse();
        clip_end(&mut points, ARROW_INSET);
        points.reverse();
    }
    let d = curve_basis(&points);
    let marker = format!(
        "{}{}",
        marker_ref("Start", edge.arrow_start, edge.stroke.as_deref()),
        marker_ref("End", edge.arrow_end, edge.stroke.as_deref()),
    );
    let _ = write!(
        s,
        concat!(
            r#"<path id="{id}-{eid}" "#,
            r#"class="edge-thickness-normal edge-pattern-solid flowchart-link"{st} "#,
            r#"d="{d}" data-edge="true" data-et="edge" data-id="{eid}" data-look="classic"{marker}/>"#,
        ),
        id = ID,
        eid = edge_id(edge, nodes),
        st = style_attr(&edge.style),
        d = d,
        marker = marker,
    );
}

/// The base marker name for an arrow type, or `None` for no arrowhead.
fn marker_name(kind: ArrowType) -> Option<&'static str> {
    match kind {
        ArrowType::None => None,
        ArrowType::Point => Some("point"),
        ArrowType::Cross => Some("cross"),
        ArrowType::Circle => Some("circle"),
    }
}

/// The ` marker-start`/` marker-end` attribute for an arrow end, or empty. Only
/// point arrows get a colour-matched variant (matching mermaid's marker set).
fn marker_ref(side: &str, kind: ArrowType, color: Option<&str>) -> String {
    let Some(name) = marker_name(kind) else {
        return String::new();
    };
    let suffix = match (kind, color) {
        (ArrowType::Point, Some(c)) => format!("_{c}"),
        _ => String::new(),
    };
    let attr = if side == "Start" { "marker-start" } else { "marker-end" };
    format!(r#" {attr}="url(#{ID}_flowchart-v2-{name}{side}{suffix})""#)
}

/// Where the ray from a rounded-rect's centre toward `toward` crosses its
/// outline (radius `r` corners). On the straight edges this is the rect border;
/// in a corner it is the quarter-circle arc.
fn clip_rounded_rect(c: (f64, f64), w: f64, h: f64, r: f64, toward: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (toward.0 - c.0, toward.1 - c.1);
    if dx == 0.0 && dy == 0.0 {
        return c;
    }
    let (hw, hh) = (w / 2.0, h / 2.0);
    // Sharp-rect intersection first.
    let tx = if dx != 0.0 { hw / dx.abs() } else { f64::MAX };
    let ty = if dy != 0.0 { hh / dy.abs() } else { f64::MAX };
    let t = tx.min(ty);
    let (px, py) = (dx * t, dy * t);
    // Inner rectangle whose corners are the arc centres.
    let (ix, iy) = (hw - r, hh - r);
    if px.abs() > ix && py.abs() > iy {
        // Corner region: intersect the ray with the corner arc's circle.
        let corner = (ix * px.signum(), iy * py.signum());
        // |t*d - corner| = r  ->  quadratic in t; take the far (exit) root.
        let a = dx * dx + dy * dy;
        let b = -2.0 * (dx * corner.0 + dy * corner.1);
        let cc = corner.0 * corner.0 + corner.1 * corner.1 - r * r;
        let disc = b * b - 4.0 * a * cc;
        if disc >= 0.0 {
            let t = (-b + disc.sqrt()) / (2.0 * a);
            return (c.0 + dx * t, c.1 + dy * t);
        }
    }
    (c.0 + px, c.1 + py)
}

/// The point where the ray from a node's centre toward `toward` crosses the
/// node's actual shape boundary. `None` for rectangular shapes (dagre already
/// routes to the rect border, matching mermaid).
fn clip_to_shape(node: &PlacedNode, toward: (f64, f64)) -> Option<(f64, f64)> {
    let c = (node.cx, node.cy);
    if let NodeShape::Circle = node.shape {
        let (dx, dy) = (toward.0 - c.0, toward.1 - c.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            return None;
        }
        let r = node.width / 2.0;
        return Some((c.0 + dx / len * r, c.1 + dy / len * r));
    }
    // Stadium (pill) has fully-rounded ends; a diagonal edge would otherwise end
    // at the bounding-box corner, outside the shape. Clip to the rounded outline.
    if let NodeShape::Stadium = node.shape {
        return Some(clip_rounded_rect(c, node.width, node.height, node.height / 2.0, toward));
    }
    let poly = shape_boundary(node)?;
    // Find where segment centre→toward crosses a polygon edge.
    for w in poly.windows(2) {
        if let Some(p) = segment_intersect(c, toward, w[0], w[1]) {
            return Some(p);
        }
    }
    // (windows misses the closing edge) check last→first too.
    if let (Some(&a), Some(&b)) = (poly.last(), poly.first()) {
        segment_intersect(c, toward, a, b)
    } else {
        None
    }
}

/// Absolute polygon vertices of a node's outline (matching render_shape), or
/// `None` for rectangular shapes.
fn shape_boundary(node: &PlacedNode) -> Option<Vec<(f64, f64)>> {
    let (cx, cy, w, h) = (node.cx, node.cy, node.width, node.height);
    let map = |pts: &[(f64, f64)], tx: f64, ty: f64| -> Vec<(f64, f64)> {
        pts.iter().map(|&(x, y)| (x + tx + cx, y + ty + cy)).collect()
    };
    match node.shape {
        NodeShape::Rhombus => {
            let s = w;
            Some(map(&[(s / 2.0, 0.0), (s, -s / 2.0), (s / 2.0, -s), (0.0, -s / 2.0)], -s / 2.0 + 0.5, s / 2.0))
        }
        NodeShape::Hexagon => {
            let m = h / 4.0;
            Some(map(&[(m, 0.0), (w - m, 0.0), (w, -h / 2.0), (w - m, -h), (m, -h), (0.0, -h / 2.0)], -w / 2.0, h / 2.0))
        }
        NodeShape::Parallelogram => {
            let iw = w - h;
            Some(map(&[(-h / 2.0, 0.0), (iw, 0.0), (iw + h / 2.0, -h), (0.0, -h)], -iw / 2.0, h / 2.0))
        }
        NodeShape::LeanLeft => {
            let iw = w - h;
            Some(map(&[(0.0, 0.0), (iw + h / 2.0, 0.0), (iw, -h), (-h / 2.0, -h)], -iw / 2.0, h / 2.0))
        }
        NodeShape::Trapezoid => {
            let iw = w - h;
            Some(map(&[(-h / 2.0, 0.0), (iw + h / 2.0, 0.0), (iw, -h), (0.0, -h)], -iw / 2.0, h / 2.0))
        }
        NodeShape::InvTrapezoid => {
            let iw = w - h;
            Some(map(&[(0.0, 0.0), (iw, 0.0), (iw + h / 2.0, -h), (-h / 2.0, -h)], -iw / 2.0, h / 2.0))
        }
        _ => None,
    }
}

/// Intersection point of segments p1→p2 and p3→p4, if they cross.
fn segment_intersect(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> Option<(f64, f64)> {
    let d = (p2.0 - p1.0) * (p4.1 - p3.1) - (p2.1 - p1.1) * (p4.0 - p3.0);
    if d.abs() < 1e-9 {
        return None;
    }
    let t = ((p3.0 - p1.0) * (p4.1 - p3.1) - (p3.1 - p1.1) * (p4.0 - p3.0)) / d;
    let u = ((p3.0 - p1.0) * (p2.1 - p1.1) - (p3.1 - p1.1) * (p2.0 - p1.0)) / d;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some((p1.0 + t * (p2.0 - p1.0), p1.1 + t * (p2.1 - p1.1)))
    } else {
        None
    }
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
            // A linkStyle `color:` paints the label: the background rect carries
            // `color:X !important`, the text `fill:X !important` (render_text maps
            // color→fill), matching mermaid.
            let label_style = edge
                .label_color
                .as_ref()
                .map(|c| format!("color:{c} !important"))
                .unwrap_or_default();
            let _ = write!(
                s,
                r#"<rect class="background"{} x="{}" y="-1" width="{}" height="{}"/>"#,
                style_attr(&label_style),
                round(-bg_w / 2.0),
                round(bg_w),
                round(bg_h),
            );
            render_text(s, Some(label), true, &label_style);
            s.push_str("</g></g></g>");
        }
        None => {
            // Unlabelled: an empty g.edgeLabel plus a sibling g holding only the
            // background rect — exactly what mermaid emits.
            let _ = write!(
                s,
                r#"<g class="edgeLabel"><g class="label" data-id="{eid}" transform="translate(0, 0)">"#,
            );
            render_text(s, None, true, "");
            s.push_str(r#"</g></g><g><rect class="background" style="stroke: none"/></g>"#);
        }
    }
}

/// Emit mermaid's nested label structure: a `<text>` containing one
/// `<tspan.text-outer-tspan.row>` per wrapped line, each holding one
/// `<tspan.text-inner-tspan>` per word (the first word of a row has no leading
/// space, the rest are ` word`). With `label = None` a single empty row is
/// emitted (mermaid's shape for an unlabelled edge). `anchor` adds
/// `text-anchor="middle"` (edge labels) on the `<text>` and each row.
fn render_text(s: &mut String, label: Option<&str>, anchor: bool, style: &str) {
    let ta = if anchor { r#" text-anchor="middle""# } else { "" };
    // On the SVG <text>, mermaid applies label styles with `color:` as `fill:`.
    let st = style_attr(&style.replace("color:", "fill:"));
    let _ = write!(s, r#"<text y="{y}"{ta}{st}>"#, y = round(-LABEL_HEIGHT / 2.0 - 0.6), ta = ta, st = st);

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
